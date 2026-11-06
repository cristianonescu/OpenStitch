use crate::finish::{FinishMode, LetterBreak, LetterTrim};
use crate::fmath::Det;
use crate::generate::{self, Run};
use crate::geometry::{Contour, Rect, Transform, Vec2, v2};
use crate::lines;
use crate::model::{
    ChainShape, Design, DesignObject, LineGap, LineSewing, LineType, ObjectKind, RegionType,
    StartEnd, StitchBlock,
};
use crate::patterns::{self, Pattern, PatternKind};
use crate::region;
use crate::regions;
use crate::route;
use crate::stamp;
use openstitch_core::{Command, Stitch, StitchPattern, Thread, round_coordinate};
use std::ops::Range;

/// Turns an editable [`Design`] into a machine [`StitchPattern`].
///
/// Order: objects in list order; within a shape, underlay, region, then outline (outline first
/// when the object has `line_first`, #502). A thread change
/// between elements is a colour change (after a trim); travel longer than `trim_distance` is a
/// trim + jump, shorter travel a jump, or a straight stitch inside one element below
/// `connect_distance`. All distances 0.1 mm.
///
/// `trim_distance` is 0 by default: every frame move inside the design is trimmed, as in
/// PE-DESIGN 11.50's files. In 177 of its PES files (compat/reference and the owner's round-2
/// folders, 2026-10-27) every interior move carries the PEC trim flag and a plain jump appears only
/// as the first record and right after a colour change. A move between objects no longer than
/// `sew_across_distance` is not a jump at all but a stitch across the gap: PE-DESIGN sews a
/// 2.24 mm needle-to-needle move across and trims a 2.33 mm one (compat machine-trim-fine,
/// machine-trim-bound, machine-jumps, 2026-10-28), so the bound is 2.3 mm. Multi-needle
/// designs use PE-DESIGN's "minimum jump stitch length for trimming" setting for the trims
/// instead ([`Compiler::for_design`]).
#[derive(Clone, Copy, Debug)]
pub struct Compiler {
    pub trim_distance: f64,
    pub connect_distance: f64,
    pub max_stitch: f64,
    /// A move between objects up to this long (0.1 mm) is sewn as a stitch.
    pub sew_across_distance: f64,
}

impl Default for Compiler {
    fn default() -> Self {
        Compiler {
            trim_distance: 0.0,
            connect_distance: 40.0,
            max_stitch: 120.0,
            sew_across_distance: 23.0,
        }
    }
}

struct Element {
    thread: Thread,
    runs: Vec<Run>,
    /// Stitches longer than `max_stitch` stay whole: satin, whose crossings PE-DESIGN 11
    /// writes as one stitch however wide the column (30 mm in region-travel-u-satin).
    whole: bool,
    /// Every run the writer separates with a trim ties in and locks, as PE-DESIGN 11.50 sews
    /// its objects (all but one of the 2,590 trimmed runs in its reference files end with a
    /// lock). Off for fixed needle points (opened stitch files) and cutwork blades.
    lock: bool,
    /// The runs are separate pieces of a letter (an i's dot and stem, the pieces of a small N):
    /// each run after the first starts after a trim, as PE-DESIGN 11.50 trims between a
    /// letter's separate runs (stroke-satin-lower-i, text-size-hg's x and g), never a straight
    /// stitch across the gap.
    pieces: bool,
    /// The longest move into this element sewn across as a stitch instead of a trim (0.1 mm);
    /// `None` for the compiler's `sew_across_distance`. A letter of a text takes
    /// [`LETTER_ACROSS`].
    across: Option<f64>,
    /// How the move into this element is trimmed: PE-DESIGN's rule unless a text's
    /// [`crate::finish::Finish::between`] says otherwise (#548).
    rule: Rule,
}

/// How the move into an element is trimmed (#548).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
enum Rule {
    /// PE-DESIGN's rule: sewn across up to the element's across distance, trimmed beyond the
    /// compiler's trim distance.
    #[default]
    Pe,
    /// As [`Rule::Pe`] with this trim distance (0.1 mm).
    Far(f64),
    /// Trimmed however short the move.
    Always,
    /// Never trimmed: sewn across up to the across distance, a jump beyond.
    Never,
}

/// The longest move from one letter of a text to the next sewn across as a stitch (0.1 mm):
/// longer ones are trimmed (#465). Every move between letters in PE-DESIGN 11.50's text
/// captures is trimmed, and all of them are longer than 2.3 mm, so the compiler's own
/// sew-across distance is kept: at 2.0 mm, six Text Transform captures lost a needle point
/// each against PE-DESIGN's. No satin capture below 8 mm exists to tell where PE-DESIGN's
/// bound lies under 2.3 mm.
const LETTER_ACROSS: f64 = 23.0;

struct Writer {
    cfg: Compiler,
    out: StitchPattern,
    thread: Option<Thread>,
    position: Option<Vec2>,
    cut: bool,
    /// Long travel is trimmed (off for objects with jump trimming off on multi-needle machines).
    trim_jumps: bool,
    /// The stitches since the last trim belong to an element that locks ([`Element::lock`]).
    lock_open: bool,
    /// How the move into the next object is trimmed: the finish of the object sewn last
    /// (#548).
    leaving: Rule,
    /// The object being sewn has not travelled to its first run yet.
    entering: bool,
    /// The object's own ties (#548); `None` = PE-DESIGN's.
    ties: Option<Ties>,
    /// Tie in the next run even when no trim or jump comes before it (tie: always).
    force_tie: bool,
    /// The ties of the object whose run is open: its lock may be written after the next
    /// object has started (with the move into it).
    open_ties: Option<Ties>,
    /// A stop before every trimmed move between parts (`stopAtJumps`, #512).
    stop_at_jumps: bool,
    /// An object asked for a stop after it (#512): written before the next move unless a
    /// colour change (itself a stop) comes first.
    stop_pending: bool,
    /// The basting run (#509) waiting to be sewn in the first element's thread.
    basting: Vec<Run>,
}

/// An object's own ties (#548).
#[derive(Clone, Copy, Debug)]
struct Ties {
    off: bool,
    always: bool,
    steps: usize,
    /// Tie stitch length, 0.1 mm.
    len: f64,
}

/// Needle points closer than this (0.1 mm) are one penetration, wherever they lie on the
/// export grid (#567): generators give the same point twice up to 0.004 mm apart, and merging
/// only those that round to the same 0.1 mm cell made a moved shape gain or lose stitches.
const SAME_POINT: f64 = 0.5;

impl Writer {
    fn push(&mut self, p: Vec2, c: Command) {
        // A stitch on the needle point just sewn would be a zero-length record: a second
        // penetration in the same hole. PE-DESIGN 11.50 writes none (no PEC record of length 0 in
        // any of its 177 files measured on 2026-10-27), so generators that end one run where the
        // next begins, or produce two points within 0.05 mm ([`SAME_POINT`]), sew one needle point. A zero-length
        // stitch right after a jump is kept: the jump's landing is not a penetration, so that
        // stitch is the first needle point of the block (PE-DESIGN ties in there instead).
        if c == Command::Stitch
            && let Some(last) = self.out.stitches.last()
            && last.command == Command::Stitch
            && (v2(last.x, last.y).distance(p) < SAME_POINT
                || (round_coordinate(last.x) == round_coordinate(p.x)
                    && round_coordinate(last.y) == round_coordinate(p.y)))
        {
            return;
        }
        self.out.stitches.push(Stitch::new(p.x, p.y, c));
    }

    /// Before a trim: PE-DESIGN's end lock on the run just sewn, unless it already ends with one.
    fn end_lock(&mut self) {
        if !std::mem::take(&mut self.lock_open) {
            return;
        }
        let tail: Vec<Vec2> = self
            .out
            .stitches
            .iter()
            .rev()
            .take_while(|s| s.command == Command::Stitch)
            .take(5)
            .map(|s| v2(s.x, s.y))
            .collect();
        // The object's own ties lock every run (its generators' locks were left out, #548).
        if tail.len() < 2 || (self.open_ties.is_none() && crate::region::ends_locked(&tail)) {
            return;
        }
        let lock = match self.open_ties {
            Some(t) => crate::finish::lock_points(tail[1], tail[0], t.steps, t.len),
            None => crate::region::lock_after(&tail),
        };
        for p in lock {
            self.push(p, Command::Stitch);
        }
    }

    fn stitch_line(&mut self, p: Vec2) {
        self.stitch_split(p, self.cfg.max_stitch);
    }

    /// A stitch to `p`, split into equal stitches no longer than `max`.
    fn stitch_split(&mut self, p: Vec2, max: f64) {
        let from = self.position.unwrap_or(p);
        let n = ((from.distance(p) / max).ceil() as usize).max(1);
        for k in 1..=n {
            self.push(from.lerp(p, k as f64 / n as f64), Command::Stitch);
        }
        self.position = Some(p);
        self.cut = false;
    }

    fn travel(&mut self, p: Vec2, within_element: bool, across: f64, rule: Rule) {
        let Some(from) = self.position else {
            self.push(p, Command::Jump);
            self.position = Some(p);
            return;
        };
        let d = from.distance(p);
        if d == 0.0 {
            return;
        }
        let sew_across = rule != Rule::Always && !self.cut;
        if within_element && d <= self.cfg.connect_distance && sew_across {
            self.stitch_line(p);
            return;
        }
        // PE-DESIGN sews a short move between objects across as a stitch rather than jumping.
        if !within_element && d <= across && sew_across {
            self.stitch_line(p);
            return;
        }
        let trim = match rule {
            Rule::Pe => d > self.cfg.trim_distance && self.trim_jumps,
            Rule::Far(t) => d > t,
            Rule::Always => true,
            Rule::Never => false,
        };
        if trim && !self.cut {
            self.end_lock();
            self.push(from, Command::Trim);
            self.cut = true;
            // No cutter: the machine stops so the user can snip the thread before the jump.
            if self.stop_at_jumps && d > self.cfg.trim_distance.max(self.cfg.sew_across_distance) {
                self.push(from, Command::Stop);
            }
        }
        self.push(p, Command::Jump);
        self.position = Some(p);
    }

    /// Feed (Manual Punch): the frame moves to `p` with a jump stitch; the thread is not cut.
    fn feed(&mut self, p: Vec2) {
        if self.position != Some(p) {
            self.push(p, Command::Jump);
            self.position = Some(p);
        }
    }

    fn sew(&mut self, element: Element) {
        let max = if element.whole {
            f64::INFINITY
        } else {
            self.cfg.max_stitch
        };
        let runs: Vec<Run> = element.runs.into_iter().filter(|r| !r.is_empty()).collect();
        if runs.is_empty() {
            return;
        }
        if self.thread.is_none() && !self.basting.is_empty() {
            // Basting in the first thread: no colour change of its own (#509).
            let basting = std::mem::take(&mut self.basting);
            self.sew(basting_element(basting, element.thread.clone()));
        }
        match &self.thread {
            Some(t) if *t != element.thread => {
                let p = self.position.unwrap_or_default();
                if !self.cut && self.position.is_some() {
                    self.end_lock();
                    self.push(p, Command::Trim);
                    self.cut = true;
                }
                // The colour change stops the machine: no separate stop (#512).
                self.stop_pending = false;
                self.push(p, Command::ColorChange);
                self.out.threads.push(element.thread.clone());
            }
            None => self.out.threads.push(element.thread.clone()),
            _ => {}
        }
        self.thread = Some(element.thread);
        if std::mem::take(&mut self.stop_pending)
            && let Some(p) = self.position
        {
            self.push(p, Command::Stop);
        }
        let across = element.across.unwrap_or(self.cfg.sew_across_distance);
        for (i, run) in runs.iter().enumerate() {
            // A letter's separate pieces are trimmed apart (pieces close enough to join inside
            // the letter are one run already: `stroke_columns`).
            let across = if i > 0 { 0.0 } else { across };
            // The move into an object follows the finish of the one before it (#548).
            let rule = match i {
                0 if std::mem::take(&mut self.entering) => self.leaving,
                0 => element.rule,
                _ => Rule::Pe,
            };
            self.travel(run[0], i > 0 && !element.pieces, across, rule);
            // A run the writer starts after a trim ties in (unless its generator already did).
            let fresh = self.position.is_none()
                || self.cut
                || self
                    .out
                    .stitches
                    .last()
                    .is_some_and(|s| s.command == Command::Jump);
            // Only a run that locks takes the object's forced tie-in (not a basting run).
            let force = element.lock && std::mem::take(&mut self.force_tie);
            let tied;
            let run = match self.ties {
                // The object's own ties: its generators' ties were left out (#548).
                Some(t) if !t.off && element.lock && (fresh || force) => {
                    let mut r = run.clone();
                    crate::finish::tie_in(&mut r, t.steps, t.len);
                    tied = r;
                    &tied
                }
                None if element.lock && fresh && !crate::region::starts_tied(run) => {
                    let mut r = run.clone();
                    crate::region::tie_in(&mut r);
                    tied = r;
                    &tied
                }
                _ => run,
            };
            self.lock_open = element.lock && !self.ties.is_some_and(|t| t.off);
            self.open_ties = self.ties;
            // A jump already ends on the run's first needle point: PE-DESIGN 11 never writes a
            // zero-length stitch there and drops one when it opens a file (compat pes-default-v6).
            let landed = self
                .out
                .stitches
                .last()
                .is_some_and(|s| s.command == Command::Jump && v2(s.x, s.y) == run[0]);
            if landed {
                self.cut = false;
            } else if self.position.is_none() || self.cut {
                self.push(run[0], Command::Stitch);
                self.position = Some(run[0]);
                self.cut = false;
            }
            for &p in &run[1..] {
                self.stitch_split(p, max);
            }
        }
    }
}

impl Compiler {
    /// The default compiler with the design's jump-stitch trimming setting applied.
    pub fn for_design(design: &Design) -> Self {
        let d = Compiler::default();
        Compiler {
            trim_distance: design.machine.trim_distance(d.trim_distance),
            ..d
        }
    }

    pub fn compile(&self, design: &Design) -> StitchPattern {
        self.compile_traced(design).0
    }

    /// [`Self::compile`] plus, per visible object in sewing order, its id and the range of
    /// the pattern's stitches it wrote, travel into it included (the quality checks name the
    /// objects behind a problem from this).
    pub fn compile_traced(&self, design: &Design) -> (StitchPattern, Vec<(u64, Range<usize>)>) {
        let mut w = self.writer();
        w.out.set_name(design.name.clone());
        for (key, value) in design.properties.fields() {
            if !value.is_empty() {
                w.out.metadata.insert(key.into(), value.into());
            }
        }
        let production = &design.production;
        w.stop_at_jumps = production.stop_at_jumps;
        if let Some(b) = &production.basting {
            let runs = crate::production::basting_runs(design, b);
            match &b.thread {
                Some(t) => w.sew(basting_element(runs, t.clone())),
                None => w.basting = runs,
            }
        }
        let mut spans = Vec::new();
        for object in design.objects.iter().filter(|o| o.visible) {
            let start = w.out.stitches.len();
            let started = match (production.auto_start, w.position) {
                (crate::production::AutoStart::Nearest, Some(from)) => {
                    crate::production::nearest_start(object, from).filter(|c| {
                        // Only where it helps: the start rule must not add a trim inside the
                        // object (an echo or stippling route can split up from another start)
                        // and must shorten the travel, the move into the object included.
                        let (t0, d0) = self.object_cost(object, design, from);
                        let (t1, d1) = self.object_cost(c, design, from);
                        t1 <= t0 && d1 < d0
                    })
                }
                _ => None,
            };
            sew_object(&mut w, started.as_ref().unwrap_or(object), design);
            if object.stop_after && w.position.is_some() {
                w.stop_after();
            }
            spans.push((object.id, start..w.out.stitches.len()));
        }
        (w.finish(), spans)
    }

    /// The stitches `design.objects[index]` alone generates, as it would be sewn inside the
    /// design (hole sewing still skips its paired inner shape), visible or not. Used to
    /// convert a shape or text into a stitch pattern.
    pub fn compile_object(&self, design: &Design, index: usize) -> StitchPattern {
        let mut w = self.writer();
        if let Some(object) = design.objects.get(index) {
            sew_object(&mut w, object, design);
        }
        w.finish()
    }

    /// Trims and total travel (0.1 mm) of `object` sewn alone from needle position `from`.
    fn object_cost(&self, object: &DesignObject, design: &Design, from: Vec2) -> (usize, f64) {
        let mut w = self.writer();
        w.position = Some(from);
        w.cut = false;
        sew_object(&mut w, object, design);
        let mut last = from;
        let mut travel = 0.0;
        for s in &w.out.stitches {
            let p = v2(s.x, s.y);
            if s.command == Command::Jump {
                travel += last.distance(p);
            }
            if matches!(s.command, Command::Stitch | Command::Jump) {
                last = p;
            }
        }
        (w.out.count(Command::Trim), travel)
    }

    fn writer(&self) -> Writer {
        Writer {
            cfg: *self,
            out: StitchPattern::new(),
            thread: None,
            position: None,
            cut: true,
            trim_jumps: true,
            lock_open: false,
            leaving: Rule::Pe,
            entering: false,
            ties: None,
            force_tie: false,
            open_ties: None,
            stop_at_jumps: false,
            stop_pending: false,
            basting: Vec::new(),
        }
    }
}

/// The basting run as an element: loose running stitches, no locks (it is pulled out after
/// sewing), separate loops trimmed apart.
fn basting_element(runs: Vec<Run>, thread: Thread) -> Element {
    Element {
        thread,
        runs,
        whole: false,
        lock: false,
        pieces: true,
        across: Some(0.0),
        rule: Rule::Pe,
    }
}

impl Writer {
    /// "Stop after this object" (#512): the run is locked and trimmed here, and the stop is
    /// written before the next move.
    fn stop_after(&mut self) {
        if let Some(p) = self.position
            && !self.cut
        {
            self.end_lock();
            self.push(p, Command::Trim);
            self.cut = true;
        }
        self.stop_pending = true;
    }

    fn finish(mut self) -> StitchPattern {
        if let Some(p) = self.position {
            if !self.cut {
                self.end_lock();
                self.push(p, Command::Trim);
            }
            self.push(p, Command::End);
        }
        self.out
    }
}

fn sew_object(w: &mut Writer, object: &DesignObject, design: &Design) {
    let multi = design.machine.machine == crate::machine::MachineType::MultiNeedle;
    w.trim_jumps = !(multi && object.trim_jumps == Some(false));
    let finish = object.finish.as_ref().filter(|f| !f.is_default());
    w.entering = true;
    w.ties = finish.filter(|f| f.custom_ties()).map(|f| {
        let (steps, len) = f.tie_steps();
        Ties {
            off: f.tie == FinishMode::Off,
            always: f.tie == FinishMode::Always,
            steps,
            len,
        }
    });
    w.force_tie = w.ties.is_some_and(|t| t.always);
    match object.kind {
        ObjectKind::Shape | ObjectKind::Text => {
            let holes = holes_above(object, design);
            let index = design.objects.iter().position(|o| std::ptr::eq(o, object));
            let turns = index.map(|i| design.text_turns(i)).unwrap_or_default();
            let between = finish.map_or(LetterTrim::Auto, |f| f.between);
            let breaks = match (between, index) {
                (LetterTrim::Auto, _) | (_, None) => Vec::new(),
                (_, Some(i)) => design.text_breaks(i),
            };
            let sewing = Sewing {
                custom_ties: w.ties.is_some(),
                between,
                breaks: &breaks,
            };
            // Generated where the object's bounds start, then moved back (#567): row and needle
            // grids follow the object, so moving it moves its stitches and nothing else.
            let anchor = object
                .bounds()
                .map_or(v2(0.0, 0.0), |b| v2(b.min_x, b.min_y));
            let (local, holes) = if anchor == v2(0.0, 0.0) {
                (std::borrow::Cow::Borrowed(object), holes)
            } else {
                let to = Transform::translation(-anchor.x, -anchor.y);
                let mut o = object.clone();
                o.apply(&to);
                let h = holes.iter().map(|c| c.transformed(&to)).collect();
                (std::borrow::Cow::Owned(o), h)
            };
            for mut e in elements(&local, &holes, &design.patterns, &turns, &sewing) {
                if anchor != v2(0.0, 0.0) {
                    for p in e.runs.iter_mut().flatten() {
                        *p = *p + anchor;
                    }
                }
                w.sew(e);
            }
            // Stitch glyphs of user-mapped text sew after the outlined glyphs.
            if let Some(block) = &object.stitch_block
                && object.kind == ObjectKind::Text
            {
                sew_block(w, block);
            }
        }
        ObjectKind::Stitches => {
            if let Some(block) = &object.stitch_block {
                sew_block(w, block);
            }
        }
    }
    // Tie: always locks the object's end even when no trim follows it.
    if w.ties.is_some_and(|t| t.always) && !w.cut {
        w.lock_open = true;
        w.end_lock();
    }
    w.entering = false;
    w.force_tie = false;
    w.ties = None;
    w.leaving = match finish {
        None => Rule::Pe,
        Some(f) => match f.trim {
            FinishMode::Always => Rule::Always,
            FinishMode::Off => Rule::Never,
            FinishMode::Auto => f.trim_distance.map_or(Rule::Pe, |d| Rule::Far(d * 10.0)),
        },
    };
}

/// What an object's finish asks of its elements (#548).
struct Sewing<'a> {
    /// The writer ties the object's runs: its generators' PE-DESIGN ties are left out.
    custom_ties: bool,
    /// Where a text trims between letters.
    between: LetterTrim,
    /// What lies before each letter after the first ([`Design::text_breaks`]).
    breaks: &'a [LetterBreak],
}

impl Sewing<'_> {
    /// The rule for the move into letter `index` (> 0) of a text.
    fn letter_rule(&self, index: usize) -> Rule {
        let at = index
            .checked_sub(1)
            .and_then(|i| self.breaks.get(i))
            .copied()
            .unwrap_or(LetterBreak::Letter);
        match self.between {
            LetterTrim::Auto => Rule::Pe,
            LetterTrim::Never => Rule::Never,
            LetterTrim::Letter => Rule::Always,
            LetterTrim::Word if at >= LetterBreak::Word => Rule::Always,
            LetterTrim::Line if at == LetterBreak::Line => Rule::Always,
            LetterTrim::Word | LetterTrim::Line => Rule::Never,
        }
    }
}

/// Sews fixed needle points: runs split at jumps and trims, one element per colour block. The
/// points are the machine's, so a stitch longer than `max_stitch` stays one penetration, as
/// PE-DESIGN 11.50 exports an opened stitch file (compat machine-formats: its 15 mm satin
/// stitches are one jump and one stitch in every format, never two stitches).
fn sew_block(w: &mut Writer, block: &StitchBlock) {
    // A feed pattern is jumps only: sewn as the jumps themselves.
    if !block.stitches.is_empty() && block.stitches.iter().all(|s| s.command == Command::Jump) {
        for s in &block.stitches {
            w.feed(v2(s.x, s.y));
        }
        return;
    }
    let mut block_index = 0;
    let mut pending: Vec<Run> = vec![Vec::new()];
    let thread_for = |i: usize| {
        block
            .threads
            .get(i)
            .or(block.threads.last())
            .cloned()
            .unwrap_or(Thread::rgb(0))
    };
    // Where the last jump landed: the first needle point of the run after it (compiled
    // patterns, and PE-DESIGN's PEC in most files, have no zero-length stitch there). A block that does carry a stitch
    // on the landing keeps it: `stitches.import` restores the one PE-DESIGN's own designs have
    // there (its machine exports write it; `pec::restore_jump_landings`, compat
    // machine-sample-basic), so the run starts with the landing twice and the writer sews the
    // second as the zero-length stitch after the jump.
    let mut landing: Option<Vec2> = None;
    // The move into the next element sewn: trimmed after a trim in the block.
    let mut across: Option<f64> = None;
    for s in &block.stitches {
        match s.command {
            Command::Stitch => {
                let run = pending.last_mut().expect("non-empty");
                let p = v2(s.x, s.y);
                if let Some(l) = landing.take()
                    && run.is_empty()
                {
                    run.push(l);
                    if l == p {
                        run.push(p);
                        continue;
                    }
                }
                run.push(p);
            }
            Command::Trim if pending.iter().any(|r| !r.is_empty()) => {
                // A trim stays a trim: the runs after it are sewn as a new element that the
                // writer trims to however short the move (a letter's dot and stem, #465), not
                // joined by a stitch within the connect distance.
                w.sew(Element {
                    thread: thread_for(block_index),
                    runs: std::mem::replace(&mut pending, vec![Vec::new()]),
                    whole: true,
                    lock: false,
                    pieces: false,
                    across: across.take(),
                    rule: Rule::Pe,
                });
                across = Some(0.0);
            }
            Command::Jump | Command::Trim => {
                if pending.last().is_some_and(|r| !r.is_empty()) {
                    pending.push(Vec::new());
                }
                if s.command == Command::Jump {
                    landing = Some(v2(s.x, s.y));
                }
            }
            Command::ColorChange => {
                landing = None;
                w.sew(Element {
                    thread: thread_for(block_index),
                    runs: std::mem::replace(&mut pending, vec![Vec::new()]),
                    whole: true,
                    lock: false,
                    pieces: false,
                    across: across.take(),
                    rule: Rule::Pe,
                });
                block_index += 1;
            }
            Command::Stop | Command::End => {}
        }
    }
    w.sew(Element {
        thread: thread_for(block_index),
        runs: pending,
        whole: true,
        lock: false,
        pieces: false,
        across,
        rule: Rule::Pe,
    });
}

/// Closed outlines of the visible inner shape `object` is sewn around, when it has hole
/// sewing on. The inner shape stays a hole when its own region is Not Sewn, as in PE-DESIGN
/// 11.50 (references `concentric-ring-hole15`, `spiral-ring-hole15`: a 40 mm circle with a
/// 15 mm hole whose inner circle sews nothing).
fn holes_above(object: &DesignObject, design: &Design) -> Vec<Contour> {
    crate::holes::paired_inner(object, design)
        .filter(|o| o.visible && o.is_outline())
        .map(|o| o.contours.iter().filter(|c| c.closed).cloned().collect())
        .unwrap_or_default()
}

/// `turns`: how far each letter of a text is turned ([`Design::text_turns`]).
fn elements(
    object: &DesignObject,
    holes: &[Contour],
    user: &[Pattern],
    turns: &[Option<f64>],
    sewing: &Sewing,
) -> Vec<Element> {
    if let Some(mut letters) = letter_line_elements(object, user) {
        for (index, e) in letters.iter_mut().enumerate().skip(1) {
            e.rule = sewing.letter_rule(index);
        }
        return letters;
    }
    let mut out = Vec::new();
    if let (Some(stored), true) = (&object.region, object.has_closed_contour()) {
        // The guide's exclusions: no gradation or feathered edge on text, no feathered edge
        // with hole sewing. Values a file holds anyway are kept, not sewn.
        let text = object.kind == crate::model::ObjectKind::Text;
        let mut sewn = stored.clone();
        if text {
            sewn.gradation = None;
        }
        if text || !holes.is_empty() {
            sewn.feather = None;
        }
        let region = &sewn;
        // A shape is one region; text is one region per glyph, sewn letter by letter.
        for (index, group) in object.glyph_groups().into_iter().enumerate() {
            let closed: Vec<Contour> = group.iter().filter(|c| c.closed).cloned().collect();
            if closed.is_empty() {
                continue;
            }
            let mut layers = region::layers_with(&closed, region, holes, user);
            // PE-DESIGN 11.50 sews a TrueType letter in Satin and in Fill stroke by stroke, each
            // stroke a column, in its Convert to Blocks order (docs/research/stroke-lettering.md).
            // Fill text under a Text Transform keeps its region rows for now: PE-DESIGN sews it
            // by stroke as well (text-transform-9: level rows on the slanted stems), but the
            // stroke cut of warped letters drifts past the transform references' bounds and
            // their stitch counts: measured 2026-11-06, five references 0.6 mm past their
            // 0.5 mm bounds and 167 of 182 transform captures further from PE-DESIGN's count
            // (11 % off on average by region, 28 % by stroke), though nearer its needle
            // points (0.24 -> 0.18 mm) (#428).
            let warped = object.text.as_ref().is_some_and(|t| t.envelope.is_some());
            let by_stroke = (text
                && (region.kind == RegionType::Satin
                    || (region.kind == RegionType::Fill && !warped))
                && region.stamps.is_empty())
            .then(|| {
                stroke_columns(
                    &closed,
                    region,
                    turns.get(index).copied().flatten(),
                    object.text.as_ref().is_some_and(|t| t.size < SMALL_TEXT),
                )
            })
            .flatten();
            // A hairline stretch of a region's satin sews as a running line (#368).
            if region.kind == RegionType::Satin
                && by_stroke.is_none()
                && let Some(top) = layers.last_mut()
            {
                top.iter_mut().for_each(region::hairline_runs);
            }
            let stroked = by_stroke.is_some() && !layers.is_empty();
            if let (Some(runs), Some(top)) = (by_stroke, layers.last_mut()) {
                *top = runs;
            } else if let Some(top) = layers.last_mut() {
                let runs = stamp::apply(std::mem::take(top), &region.stamps);
                // PE-DESIGN 11.50 sews Concentric Circle and Spiral in the order the Start
                // Point button gives, jumping in and ending where the pattern ends (references
                // concentric-*, spiral-*): no travel from R1 or to R2, no reversal. So does
                // the flexible spiral, which starts at R1 and ends in the middle: travel to R2
                // ran straight back out across its turns (#461).
                *top = if matches!(
                    region.kind,
                    RegionType::ConcentricCircle
                        | RegionType::Spiral
                        | RegionType::Contour
                        | RegionType::FlexibleSpiral
                ) {
                    runs
                } else {
                    // Travel from the start point and to the end point: PE-DESIGN's motif and
                    // decorative fills walk it in steps of at most 2 mm (motif-fill-2pt,
                    // deco-fill-1line-100mm).
                    let pitch = if matches!(
                        region.kind,
                        RegionType::MotifFill | RegionType::DecorativeFill
                    ) {
                        regions::MOTIF_FILL_TRAVEL_PITCH
                    } else {
                        25.0
                    };
                    route::route_region(runs, &closed, region.entry, region.exit, pitch)
                };
            }
            // PE-DESIGN's fill under sewing runs straight on into the top stitching.
            let parts = region.underlay_parts();
            if region.underlay
                && region.direction_lines.len() < 2
                && !region.legacy_underlay()
                && (parts.lattice
                    || (matches!(region.kind, RegionType::Fill | RegionType::ProgrammableFill)
                        && parts.any()))
                && let Some(path) = region.travel_path()
            {
                layers = vec![region::join_layers(layers, &closed, holes, path)];
            }
            // Satin, fill, motif and decorative fill tie in and lock as PE-DESIGN 11 sews them
            // (motif-fill-2pt, deco-fill-1line-100mm: one 0.3 mm step out and back at the
            // start and at the end).
            if sewing.custom_ties {
                // The writer ties the runs with the object's own ties (#548).
            } else if matches!(
                region.kind,
                RegionType::Satin
                    | RegionType::Fill
                    | RegionType::MotifFill
                    | RegionType::DecorativeFill
            ) {
                region::tie_and_lock(&mut layers);
            } else if matches!(
                region.kind,
                RegionType::ConcentricCircle | RegionType::Spiral
            ) {
                // PE-DESIGN 11.50 ties in and locks every run it separates with a trim: the
                // whole circles and the cut ones of Concentric Circle, a spiral and its closing
                // run (references concentric-*, spiral-*).
                for run in layers.iter_mut().flatten() {
                    let mut one = vec![vec![std::mem::take(run)]];
                    region::tie_and_lock(&mut one);
                    *run = one.pop().and_then(|mut l| l.pop()).unwrap_or_default();
                }
            }
            let n = layers.len();
            for (k, runs) in layers.into_iter().enumerate() {
                out.push(Element {
                    thread: region.thread.clone(),
                    runs,
                    whole: region.kind == RegionType::Satin,
                    lock: true,
                    // A letter sewn by stroke: its top layer's runs are its separate pieces.
                    pieces: stroked && k + 1 == n,
                    // The move into a letter from the one before (#465).
                    across: (text && k == 0).then_some(LETTER_ACROSS),
                    rule: if text && k == 0 && index > 0 {
                        sewing.letter_rule(index)
                    } else {
                        Rule::Pe
                    },
                });
            }
        }
    }
    if let Some(line) = &object.line {
        let lines = line_elements(object, line, user, sewing.custom_ties);
        if object.line_first {
            // Sew outline first (#502): the outline's elements go before the region's.
            out.splice(0..0, lines);
        } else {
            out.extend(lines);
        }
    }
    out
}

/// The longest move between two letters of pixel text sewn across as a stitch (0.1 mm);
/// farther letters are trimmed (#536). Single-line letters on a line are joined by running
/// stitch up to 1.2 times the text size, at least this far (#530).
const LINE_LETTER_ACROSS: f64 = 20.0;

/// Text in a pixel font (#536), or text whose letters are all open centre lines (OpenStitch
/// Line, #530, or an imported single-line font), sewn letter by letter: pixels as
/// [`crate::letter_lines::pixel_runs`] walks them, centre lines with the text's line sewing
/// along [`crate::letter_lines::route_strokes`]. `None` for other objects, and for centre-line
/// text whose line type is not running, triple or zigzag (sewn as plain lines then).
fn letter_line_elements(object: &DesignObject, _user: &[Pattern]) -> Option<Vec<Element>> {
    use crate::letter_lines::{Piece, pixel_runs, resample, route_strokes};
    let spec = object
        .text
        .as_ref()
        .filter(|_| object.kind == ObjectKind::Text)?;
    let pixel = crate::text::bundled_kind(&spec.font) == Some(crate::text::FontKind::Pixel);
    let open = !object.contours.is_empty() && object.contours.iter().all(|c| !c.closed);
    if !pixel && !open {
        return None;
    }
    let mut out = Vec::new();
    let mut from: Option<Vec2> = None;
    if pixel {
        let thread = object
            .region
            .as_ref()
            .map(|r| r.thread.clone())
            .or_else(|| object.line.as_ref().map(|l| l.thread.clone()))?;
        for group in object.glyph_groups() {
            let runs = pixel_runs(group, spec.pixel_style, from);
            from = runs.last().and_then(|r| r.last().copied()).or(from);
            out.push(Element {
                thread: thread.clone(),
                runs,
                whole: false,
                lock: true,
                pieces: true,
                across: Some(LINE_LETTER_ACROSS),
                rule: Rule::Pe,
            });
        }
        return Some(out);
    }
    let line = object.line.as_ref()?;
    if !matches!(
        line.kind,
        LineType::Running | LineType::Triple | LineType::Zigzag
    ) {
        return None;
    }
    let pitch = (line.pitch * 10.0).max(3.0);
    let passes = match line.kind {
        LineType::Triple => 3,
        _ => line.run_times.clamp(1, 5) as usize,
    };
    let width = line.width * 10.0;
    let spacing = 10.0 / line.density.max(0.1);
    for group in object.glyph_groups() {
        let strokes: Vec<Vec<Vec2>> = group.iter().map(Contour::path).collect();
        let mut runs: Vec<Run> = Vec::new();
        for pieces in route_strokes(&strokes, from) {
            let mut run: Run = Vec::new();
            for piece in pieces {
                let (pts, stroke) = match &piece {
                    Piece::Travel(p) => (p, false),
                    Piece::Stroke(p) => (p, true),
                };
                if stroke && line.kind == LineType::Zigzag {
                    if run.is_empty() {
                        run.push(pts[0]);
                    }
                    run.extend(lines::zigzag_rails(pts, width, spacing));
                    // The column ends on its centre line, where the next travel starts.
                    run.push(*pts.last().expect("stroke"));
                    continue;
                }
                let r = resample(pts, pitch);
                if run.is_empty() {
                    run.push(r[0]);
                }
                for w in r.windows(2) {
                    // Bean stitch: each stitch sewn forward, back and forward again.
                    let n = if stroke { passes } else { 1 };
                    for k in 0..n {
                        run.push(if k % 2 == 0 { w[1] } else { w[0] });
                    }
                }
            }
            run.dedup_by(|a, b| a.distance(*b) < 1e-9);
            runs.push(run);
        }
        // Letters on a line are joined by running stitch, as PE-DESIGN's Small Text sews its
        // characters without trims between them; farther letters are trimmed.
        let join = (spec.size * 12.0).max(LINE_LETTER_ACROSS);
        if let (Some(f), Some(first)) = (from, runs.first_mut())
            && let Some(&start) = first.first()
            && f.distance(start) <= join
            && f.distance(start) > 1e-9
        {
            let mut connector = resample(&[f, start], pitch);
            connector.pop();
            connector.append(first);
            *first = connector;
        }
        from = runs.last().and_then(|r| r.last().copied()).or(from);
        out.push(Element {
            thread: line.thread.clone(),
            runs,
            whole: line.kind == LineType::Zigzag,
            lock: true,
            pieces: true,
            across: Some(LINE_LETTER_ACROSS),
            rule: Rule::Pe,
        });
    }
    Some(out)
}

/// Rungs of a letter's blocks this far apart (0.1 mm): the direction lines the satin follows.
const STROKE_RUNGS: f64 = 30.0;
/// Rungs of a fill letter's blocks this far apart (0.1 mm), the step the strokes' sides are
/// sampled at: the outline its rows end on.
const STROKE_FILL_RUNGS: f64 = 2.0;
/// Travel between a letter's blocks: running stitches along its outline at most 2 mm long.
const STROKE_TRAVEL: f64 = 20.0;
/// A letter's next piece this close (0.1 mm), straight through the letter, is travelled to
/// instead of trimmed to: the compiler's connect distance, which joined them before #465.
const PIECE_JOIN: f64 = 40.0;

/// A letter sewn stroke by stroke: each block a column along its rungs, joined by travel along
/// the outline, a new run (a trim) where a piece is not joined to the one before. Satin text
/// sews each block as a column satin; Fill text as the rows of a Manual Punch fill block, across
/// the stroke and turning with it (PE-DESIGN 11.50's fill H: horizontal rows on the stems,
/// vertical rows on the crossbar, the junction square a group of its own; its B's rows turn
/// round each bowl). A letter turned by `turn` degrees (text fitted to an outline, an arch,
/// a turned character) is cut into strokes upright and its blocks turned back: PE-DESIGN
/// 11.50 sews a fitted letter as the upright one turned (text-fit-satin-ahhhh: its H's at -18
/// to +32 degrees match the upright H within 0.1 mm), and the stroke cut of a turned outline
/// is not stable (Lato's I lost its stroke at 35 degrees, P and E fell apart at -40: #444).
/// `None` where the letter has no strokes.
/// Text below this size (mm) takes [`letter_travel`]'s straight cuts between a letter's
/// blocks; larger text walks the outline. PE-DESIGN's captures (6 mm and up) are matched by
/// the walk in stitch count (text-defaults-h, stroke-satin-a, text-fit-satin-ahhhh,
/// text-size-hg, text-fit-size-6: the cut lowered each 5-45 needle points away from
/// PE-DESIGN's), and no capture exists below 6 mm, where the walk drew boxes round t's bar
/// (#465).
const SMALL_TEXT: f64 = 6.0;

fn stroke_columns(
    closed: &[Contour],
    region: &crate::model::RegionSewing,
    turn: Option<f64>,
    small: bool,
) -> Option<Vec<Run>> {
    let (upright, back) = match turn.filter(|t| t.abs() > UPRIGHT_TURN) {
        Some(t) => {
            let b = Rect::enclosing(closed.iter().flat_map(|c| c.points.iter().copied()))?;
            let centre = v2((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0);
            let to = Transform::rotation(-t, centre);
            (
                closed.iter().map(|c| c.transformed(&to)).collect(),
                Some(Transform::rotation(t, centre)),
            )
        }
        None => (closed.to_vec(), None),
    };
    let turned = |mut blocks: Vec<crate::strokes::Block>| {
        if let Some(t) = &back {
            for p in blocks.iter_mut().flat_map(|b| b.rungs.iter_mut().flatten()) {
                *p = t.apply(*p);
            }
            for p in blocks.iter_mut().filter_map(|b| b.lead.as_mut()) {
                *p = t.apply(*p);
            }
        }
        blocks
    };
    // Checked upright: a turned letter's box corners are not its strokes' ends.
    // Satin crossings stay across a bending stroke, as PE-DESIGN's S; fill rows keep the
    // pairing their spacing was measured with (the fill captures' 8 %: paired by progress, e
    // fell from 429 to 342 needle points against PE-DESIGN's 412).
    let pairing = if region.kind == RegionType::Fill {
        crate::strokes::Pairing::Length
    } else {
        crate::strokes::Pairing::Progress
    };
    // Each separate piece of the letter (an i's dot and its stem) is cut into strokes on its
    // own, top piece first: cut together, the stem's free end reached across the gap into the
    // dot's outline, so the stem tapered into it and a small dot was lost in the stem's
    // column (#465). PE-DESIGN sews the dot, then the stem, with trims between
    // (stroke-satin-lower-i).
    // Rows and crossings take their ends from rungs this close together, so they stay on the
    // outline where it curves: fill rows interpolated between the 3 mm rungs ended up to
    // 0.2 mm inside O's counter, and satin crossings cut the chords of a small letter's bowls
    // (the 4 mm o's crossings stopped short of its outer edge, #444). The same strokes give
    // the same blocks in the same order at any rung spacing.
    // `alone`: blocks that are a whole piece of the letter on their own (an i's dot, its
    // stem).
    let cut = |pieces: &[Vec<Contour>]| {
        let mut blocks = Vec::new();
        let mut fine_blocks = Vec::new();
        let mut fine_match = true;
        let mut alone = Vec::new();
        for (k, piece) in pieces.iter().enumerate() {
            let g = crate::strokes::strokes(piece)?;
            let mut b = g.blocks_paired(piece, STROKE_RUNGS, pairing);
            let f = g.blocks_paired(piece, STROKE_FILL_RUNGS, pairing);
            fine_match &= f.len() == b.len();
            if k > 0
                && let Some(first) = b.first_mut()
            {
                first.new_run = true;
            }
            alone.extend(std::iter::repeat_n(
                b.len() == 1 && pieces.len() > 1,
                b.len(),
            ));
            blocks.extend(b);
            fine_blocks.extend(f);
        }
        blocks_cover(&blocks, &upright).then_some((blocks, fine_blocks, fine_match, alone))
    };
    // A letter whose pieces do not all cut into strokes on their own (a small script i, its
    // dot too small) is cut whole, as before #465.
    let pieces = letter_pieces(&upright);
    let (blocks, fine_blocks, fine_match, alone) = cut(&pieces).or_else(|| {
        (pieces.len() > 1)
            .then(|| cut(std::slice::from_ref(&upright)))
            .flatten()
    })?;
    let blocks = turned(blocks);
    let fine = fine_match.then(|| turned(fine_blocks));
    let spacing = 10.0 / region.density.max(0.1);
    let pull = region.pull_compensation * 10.0;
    let fill = region.kind == RegionType::Fill;
    let stitch = (region.stitch_length * 10.0).max(1.0);
    // The quads between each block's rungs, block by block: where travel may cut across
    // the letter under a block still to be sewn.
    let mut quads: Vec<Contour> = Vec::new();
    let mut quads_from = Vec::with_capacity(blocks.len());
    for b in &blocks {
        quads_from.push(quads.len());
        quads.extend(
            b.rungs
                .windows(2)
                .map(|w| Contour::new(vec![w[0][0], w[0][1], w[1][1], w[1][0]], true)),
        );
    }
    let mut runs: Vec<Run> = Vec::new();
    let mut run: Run = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        if b.rungs.len() < 2 {
            continue;
        }
        let column = if fill {
            stroke_fill(
                &crate::column::sawtooth(fill_rungs(
                    &b.rungs,
                    fine.as_ref().map_or(&b.rungs, |f| &f[i].rungs),
                    2.0 * spacing,
                    pull,
                )),
                stitch,
                region.fill_stagger(),
            )
        } else {
            // A long straight block keeps its 3 mm rungs: their chords are its sides, and the
            // fine rungs would fan where a junction cuts one side short (A's legs at the bar).
            // A block of one or two 3 mm sections (small letters) takes the fine rungs: its
            // chords would cut its ends (a 4 mm i's slanted top). A junction square keeps its
            // rungs (T's, short and skewed, would read as bending). A piece sewn as one block
            // whose chords cut its outline takes the fine rungs too: an i's dot, cut on its
            // own, is a short column across a round outline, and its chords sewed it as a
            // hexagon 3.9 mm wide where the dot is 4.5 mm (stroke-satin-lower-i, #465).
            let rungs = match fine.as_ref() {
                Some(f)
                    if !b.junction
                        && (b.rungs.len() <= 3
                            || block_bends(&f[i].rungs)
                            || turns_corner(&b.rungs)
                            || (alone[i] && chords_cut(&b.rungs, &f[i].rungs))) =>
                {
                    satin_rungs(&f[i].rungs, turns_corner(&b.rungs), 2.0 * spacing)
                }
                _ => b.rungs.clone(),
            };
            crate::column::satin(&rungs, spacing, pull)
        };
        // A piece not joined to the one before starts a new run, which the writer trims to,
        // unless the needle can reach it without leaving the letter (V's arms at its foot,
        // within the writer's connect distance): one stitch there, as the writer sewed it
        // before #465.
        let mut walk = true;
        if b.new_run
            && run.len() >= 2
            && let (Some(&from), Some(&to)) = (run.last(), column.first())
        {
            walk = false;
            if from.distance(to) > PIECE_JOIN || !inside_letter(from, to, closed) {
                runs.push(std::mem::take(&mut run));
            }
        }
        // A block with a lead is reached straight from it, under its satin (X's arms run out
        // along the arm first, stroke-satin-X, #446; a new run starts at the lead).
        if let (Some(lead), Some(&to)) = (b.lead, column.first()) {
            let mut path: Vec<Vec2> = run.last().copied().into_iter().chain([lead, to]).collect();
            path.dedup_by(|a, b| a.distance(*b) < 1.0);
            run.extend(
                generate::subdivide(&path, STROKE_TRAVEL)
                    .into_iter()
                    .skip(usize::from(!run.is_empty())),
            );
            run.pop();
        } else if walk && let (Some(&from), Some(&to)) = (run.last(), column.first()) {
            // PE-DESIGN walks the letter's outline between its blocks (stroke-blocks-H:
            // running pieces between them).
            let travel = if small {
                letter_travel(closed, from, to, &quads[quads_from[i]..])
            } else {
                route::travel(closed, from, to, STROKE_TRAVEL)
            };
            run.extend(travel.into_iter().skip(1));
            run.pop();
        }
        run.extend(column);
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    (!runs.is_empty()).then_some(runs)
}

/// How far (0.1 mm) a block's rung ends may lie off the chords between its 3 mm rungs
/// before the chords are said to cut its outline ([`chords_cut`]).
const CHORD_CUT: f64 = 3.0;

/// Whether the chords between a block's `coarse` rungs, on either side, pass more than
/// [`CHORD_CUT`] from the ends of its `fine` rungs (the outline).
fn chords_cut(coarse: &[[Vec2; 2]], fine: &[[Vec2; 2]]) -> bool {
    (0..2).any(|k| {
        fine.iter().any(|r| {
            coarse
                .windows(2)
                .map(|w| crate::skeleton::segment_distance(r[k], w[0][k], w[1][k]))
                .fold(f64::INFINITY, f64::min)
                > CHORD_CUT
        })
    })
}

/// The separate pieces of a letter's region (even-odd): each outer contour with the holes
/// inside it, top piece first (then left). The whole letter is one piece when its contours
/// overlap (some script fonts draw a letter as crossing strokes).
fn letter_pieces(closed: &[Contour]) -> Vec<Vec<Contour>> {
    let n = closed.len();
    if n < 2 {
        return vec![closed.to_vec()];
    }
    let pts: Vec<Vec<Vec2>> = closed.iter().map(Contour::outline_points).collect();
    // inside[a][b]: contour a lies inside contour b.
    let mut inside = vec![vec![false; n]; n];
    for a in 0..n {
        for b in 0..n {
            if a == b || pts[a].is_empty() {
                continue;
            }
            let one = std::slice::from_ref(&closed[b]);
            let count = pts[a]
                .iter()
                .filter(|&&p| crate::geometry::contains(p, one))
                .count();
            if count == pts[a].len() {
                inside[a][b] = true;
            } else if count > 0 {
                return vec![closed.to_vec()];
            }
        }
    }
    let depth: Vec<usize> = (0..n)
        .map(|a| (0..n).filter(|&b| inside[a][b]).count())
        .collect();
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    let mut owner = vec![usize::MAX; n];
    for a in (0..n).filter(|&a| depth[a].is_multiple_of(2)) {
        owner[a] = pieces.len();
        pieces.push(vec![a]);
    }
    for a in (0..n).filter(|&a| depth[a] % 2 == 1) {
        // A hole belongs to the outer contour it lies directly inside.
        match (0..n).find(|&b| inside[a][b] && depth[b] + 1 == depth[a]) {
            Some(b) if owner[b] != usize::MAX => pieces[owner[b]].push(a),
            _ => return vec![closed.to_vec()],
        }
    }
    let mut out: Vec<Vec<Contour>> = pieces
        .into_iter()
        .map(|p| p.into_iter().map(|i| closed[i].clone()).collect())
        .collect();
    let key = |p: &Vec<Contour>| {
        Rect::enclosing(p.iter().flat_map(|c| c.points.iter().copied()))
            .map_or((0.0, 0.0), |r| (r.min_y, r.min_x))
    };
    out.sort_by(|a, b| {
        let (ka, kb) = (key(a), key(b));
        ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
    });
    out
}

/// Whether the stitch from `a` to `b` stays inside the letter `closed` between its ends
/// (sampled every 0.1 mm; on the outline counts as inside, within [`ON_OUTLINE`]).
fn inside_letter(a: Vec2, b: Vec2, closed: &[Contour]) -> bool {
    let n = (a.distance(b).ceil() as usize).max(2);
    (1..n).all(|i| {
        let p = a.lerp(b, i as f64 / n as f64);
        crate::geometry::contains(p, closed)
            || closed
                .iter()
                .flat_map(Contour::edges)
                .any(|(e, f)| crate::skeleton::segment_distance(p, e, f) <= ON_OUTLINE)
    })
}

/// Whether the stitch from `a` to `b` lies under the quads `later` (within [`ON_OUTLINE`])
/// between its ends, sampled every 0.1 mm.
fn under(a: Vec2, b: Vec2, later: &[Contour]) -> bool {
    let n = (a.distance(b).ceil() as usize).max(2);
    (1..n).all(|i| {
        let p = a.lerp(b, i as f64 / n as f64);
        later.iter().any(|q| {
            crate::geometry::contains(p, std::slice::from_ref(q))
                || q.edges()
                    .into_iter()
                    .any(|(e, f)| crate::skeleton::segment_distance(p, e, f) <= ON_OUTLINE)
        })
    })
}

/// A point this close to a letter's outline (0.1 mm) is on it.
const ON_OUTLINE: f64 = 0.5;

/// Travel between two blocks of a letter: along its outline, as PE-DESIGN walks it, cut
/// short straight through the letter wherever the outline goes round a stroke the straight
/// way crosses (the walk more than twice as long) and the straight way lies under blocks
/// still to be sewn (`later`: quads between their rungs): the t's travel from its stem above
/// the bar to the stem below went round the bar's outline, a box drawn round the bar at
/// 5-10 mm (#465); it now runs under the bar's satin. The shortest such cut from where the
/// needle is is taken, then the walk goes on: H's travel from its left stem's top part to its foot walked
/// up round the stem's top (18 stitches); it now crosses the stem under the junction square
/// and runs down its edge, as PE-DESIGN's 8 travel stitches (text-defaults-h). A walk along
/// one straight edge is the straight way already and stays as it is.
fn letter_travel(closed: &[Contour], from: Vec2, to: Vec2, later: &[Contour]) -> Run {
    let walk = route::travel(closed, from, to, STROKE_TRAVEL);
    if walk.len() < 3 {
        return walk;
    }
    let mut along = vec![0.0];
    for w in walk.windows(2) {
        along.push(along[along.len() - 1] + w[0].distance(w[1]));
    }
    let mut out = vec![walk[0]];
    let mut i = 0;
    while i + 1 < walk.len() {
        // Of the cuts from here, the shortest (the farthest of equal ones): straight across
        // the stroke, not along it.
        let j = (i + 2..walk.len())
            .filter(|&j| {
                along[j] - along[i] > 2.0 * walk[i].distance(walk[j])
                    && inside_letter(walk[i], walk[j], closed)
                    && under(walk[i], walk[j], later)
            })
            .min_by(|&a, &b| {
                let (da, db) = (walk[i].distance(walk[a]), walk[i].distance(walk[b]));
                da.total_cmp(&db).then(b.cmp(&a))
            })
            .unwrap_or(i + 1);
        let n = ((walk[i].distance(walk[j]) / STROKE_TRAVEL).ceil() as usize).max(1);
        out.extend((1..=n).map(|k| walk[i].lerp(walk[j], k as f64 / n as f64)));
        i = j;
    }
    out
}

/// A block bends when its middle line turns by more than this (degrees) at its median step of
/// [`BLOCK_BEND_STEP`]: O, S, e and the bowls of small letters turn at nearly every step; a
/// straight stem, leg or bar never, and X's chevrons only where their arms meet.
const BLOCK_BEND: f64 = 4.0;
/// See [`BLOCK_BEND`] (0.1 mm).
const BLOCK_BEND_STEP: f64 = 10.0;

/// Whether a block, by its rungs close together `fine`, bends (see [`BLOCK_BEND`]).
fn block_bends(fine: &[[Vec2; 2]]) -> bool {
    let mids: Vec<Vec2> = fine.iter().map(|[l, r]| l.lerp(*r, 0.5)).collect();
    let total: f64 = mids.windows(2).map(|w| w[0].distance(w[1])).sum();
    let steps = (total / BLOCK_BEND_STEP).floor() as usize;
    if steps < 2 {
        return false;
    }
    let at = |s: f64| {
        let mut run = 0.0;
        for w in mids.windows(2) {
            let d = w[0].distance(w[1]);
            if run + d >= s {
                return w[0].lerp(w[1], ((s - run) / d.max(1e-9)).clamp(0.0, 1.0));
            }
            run += d;
        }
        mids[mids.len() - 1]
    };
    let pts: Vec<Vec2> = (0..=steps)
        .map(|k| at(total * k as f64 / steps as f64))
        .collect();
    let mut turns: Vec<f64> = pts
        .windows(3)
        .map(|w| {
            let (u, v) = (w[1] - w[0], w[2] - w[1]);
            let cos = (u.x * v.x + u.y * v.y) / (u.length() * v.length()).max(1e-12);
            cos.clamp(-1.0, 1.0).dacos().to_degrees()
        })
        .collect();
    turns.sort_by(f64::total_cmp);
    turns.get(turns.len() / 2).is_some_and(|&t| t > BLOCK_BEND)
}

/// Letters turned by less than this (degrees) are cut into strokes as they stand.
const UPRIGHT_TURN: f64 = 0.01;

/// A satin block's rungs `pitch` apart (two crossings) along its longer side, as PE-DESIGN
/// 11.50 spaces a letter's crossings by its outer edge (stroke-satin-O: 0.41 mm per pair
/// outside, 0.26-0.28 mm inside), or along its inner side where the block turns a `corner`
/// (see [`turns_corner`]: X's chevrons, L), as fill text fans its rows round one. Taken from
/// `fine`, the block's rungs close together: a whole number of equal steps (rounded down),
/// first and last rungs kept. Needle points at 20 mm against PE-DESIGN's stroke-satin-*
/// (PE-DESIGN / longer side everywhere / this / the 3 mm rungs before #444): X 277 / 364 /
/// 302 / 302, O 358 / 338 / 338 / 318, S 278 / 281 / 281 / 253, e 267 / 269 / 269 / 235.
fn satin_rungs(fine: &[[Vec2; 2]], corner: bool, pitch: f64) -> Vec<[Vec2; 2]> {
    if fine.len() < 2 {
        return fine.to_vec();
    }
    let side_length =
        |k: usize| -> f64 { fine.windows(2).map(|w| w[0][k].distance(w[1][k])).sum() };
    let (a, b) = (side_length(0), side_length(1));
    let k = usize::from(if corner { b < a } else { b > a });
    let lengths: Vec<f64> = fine.windows(2).map(|w| w[0][k].distance(w[1][k])).collect();
    let total: f64 = lengths.iter().sum();
    let n = ((total / pitch.max(0.1) + 1e-9).floor() as usize).clamp(1, 100_000);
    let step = total / n as f64;
    let mut out = vec![fine[0]];
    let (mut i, mut start) = (0, 0.0);
    for k in 1..=n {
        let at = if k == n { total } else { k as f64 * step };
        while i + 1 < lengths.len() && start + lengths[i] < at {
            start += lengths[i];
            i += 1;
        }
        let t = ((at - start) / lengths[i].max(1e-9)).clamp(0.0, 1.0);
        let ([a, b], [c, d]) = (fine[i], fine[i + 1]);
        out.push([a.lerp(c, t), b.lerp(d, t)]);
    }
    out
}

/// How far (0.1 mm) a letter's blocks may stop short of its outline's bounding box on any side
/// before the letter is sewn by region instead: a stroke cut that misses part of a letter (a
/// strongly warped Text Transform letter) would leave it unsewn.
const STROKE_COVER: f64 = 5.0;

/// Whether the blocks reach the letter's bounding box on every side within [`STROKE_COVER`].
fn blocks_cover(blocks: &[crate::strokes::Block], closed: &[Contour]) -> bool {
    let bounds = |pts: &mut dyn Iterator<Item = Vec2>| {
        pts.fold(None, |b: Option<(Vec2, Vec2)>, p| {
            Some(b.map_or((p, p), |(lo, hi)| {
                (
                    v2(lo.x.min(p.x), lo.y.min(p.y)),
                    v2(hi.x.max(p.x), hi.y.max(p.y)),
                )
            }))
        })
    };
    let outline = bounds(&mut closed.iter().flat_map(|c| c.points.iter().copied()));
    let sewn = bounds(
        &mut blocks
            .iter()
            .flat_map(|b| b.rungs.iter().flatten().copied()),
    );
    match (outline, sewn) {
        (Some((lo, hi)), Some((slo, shi))) => {
            slo.x - lo.x <= STROKE_COVER
                && slo.y - lo.y <= STROKE_COVER
                && hi.x - shi.x <= STROKE_COVER
                && hi.y - shi.y <= STROKE_COVER
        }
        _ => false,
    }
}

/// A fill block turns a corner when its sides run straight (their median bend between two 3 mm
/// sections under [`CORNER_STRAIGHT`] degrees) and bend once by more than [`CORNER_TURN`]: L's
/// sides run at 0° and bend by 78-175° at its corner, while the bowls of e, S and 8 bend by
/// 15-30° almost everywhere (and by up to 82° where their sampling is uneven).
const CORNER_TURN: f64 = 60.0;
/// See [`CORNER_TURN`].
const CORNER_STRAIGHT: f64 = 5.0;

/// Whether a letter's block turns a corner (see [`CORNER_TURN`]), from its 3 mm rungs.
fn turns_corner(coarse: &[[Vec2; 2]]) -> bool {
    let turns: Vec<Vec<f64>> = (0..2)
        .map(|side| {
            coarse
                .windows(3)
                .map(|w| {
                    let (u, v) = (w[1][side] - w[0][side], w[2][side] - w[1][side]);
                    let cos = (u.x * v.x + u.y * v.y) / (u.length() * v.length()).max(1e-12);
                    cos.clamp(-1.0, 1.0).dacos().to_degrees()
                })
                .collect()
        })
        .collect();
    let median = |t: &[f64]| {
        let mut t = t.to_vec();
        t.sort_by(f64::total_cmp);
        t.get(t.len() / 2).copied().unwrap_or(0.0)
    };
    turns.iter().all(|t| median(t) < CORNER_STRAIGHT)
        && turns.iter().flatten().any(|&t| t > CORNER_TURN)
}

/// A fill block's rows `pitch` apart: a whole number of equal steps along the block (rounded
/// down), their ends on `fine`, the block's rungs close together, each lengthened by `pull` at
/// both ends. A block is measured along its middle, the mean of its two sides' lengths; one
/// that turns a corner (see [`CORNER_TURN`], from its 3 mm rungs `coarse`: L's stem into its
/// foot) along its inner, shorter side, as PE-DESIGN fans the rows round the corner. Against
/// the round-5 fill captures (needle points, PE-DESIGN / middle / longer side): L 276 / 341 /
/// 361, by the inner side 274; S 461 / 548 / 603; e 412 / 429 / 574; O 613 / 526 / 667;
/// 8 598 / 459 / 592. The middle keeps every letter within 25 %, 8 % on average; PE-DESIGN
/// packs O's and 8's bowls closer than that (#376).
fn fill_rungs(coarse: &[[Vec2; 2]], fine: &[[Vec2; 2]], pitch: f64, pull: f64) -> Vec<[Vec2; 2]> {
    let corner = turns_corner(coarse);
    let section = |[a, b]: [Vec2; 2], [c, d]: [Vec2; 2]| {
        if corner {
            a.distance(c).min(b.distance(d))
        } else {
            0.5 * (a.distance(c) + b.distance(d))
        }
    };
    let lengths: Vec<f64> = fine.windows(2).map(|w| section(w[0], w[1])).collect();
    let total: f64 = lengths.iter().sum();
    let mut out: Vec<[Vec2; 2]> = fine.first().copied().into_iter().collect();
    if fine.len() >= 2 {
        let n = ((total / pitch.max(0.1) + 1e-9).floor() as usize).clamp(1, 100_000);
        let step = total / n as f64;
        let (mut i, mut start) = (0, 0.0);
        for k in 1..=n {
            let at = if k == n { total } else { k as f64 * step };
            while i + 1 < lengths.len() && start + lengths[i] < at {
                start += lengths[i];
                i += 1;
            }
            let t = ((at - start) / lengths[i].max(1e-9)).clamp(0.0, 1.0);
            let ([a, b], [c, d]) = (fine[i], fine[i + 1]);
            out.push([a.lerp(c, t), b.lerp(d, t)]);
        }
    }
    for [l, r] in &mut out {
        let len = l.distance(*r).max(1e-9);
        let d = (*r - *l) * (pull / len);
        (*l, *r) = (*l - d, *r + d);
    }
    out
}

/// Needle points of a fill stroke closer than this (0.1 mm) to either end of its crossing are
/// left out: PE-DESIGN 11.50's fill H drops a grid point 0.2 mm from the row's end and keeps one
/// 0.4 mm from its start (`text-defaults-fill`).
const STROKE_FILL_MARGIN: f64 = 3.0;

/// Fill rows of a letter's stroke as PE-DESIGN 11.50 sews them (`text-defaults-fill`, the round-5
/// `stroke-fill-*` captures): the satin sawtooth of the block, a row across the stroke and a
/// slanting row back, two crossings per two row spacings with no step along the edge, every
/// crossing split on a grid of `stitch` (the Step pitch). The grid is measured from the rungs'
/// first side and moves back by the Frequency from one crossing to the next: on the H's stems
/// (3.8 mm, 4 mm steps, 30 %) the needle points of consecutive crossings lie 2.8, 1.6, 0.4, 3.2,
/// 2.0 mm from the stem's right edge. A grid point within [`STROKE_FILL_MARGIN`] of a crossing's
/// end is left out.
fn stroke_fill(sawtooth: &[Vec2], stitch: f64, stagger: generate::Stagger) -> Run {
    let mut run: Run = sawtooth.first().copied().into_iter().collect();
    for (k, w) in sawtooth.windows(2).enumerate() {
        let (from, to) = (w[0], w[1]);
        let len = from.distance(to);
        // Even crossings run from the first side, odd ones back to it.
        let (side, other) = if k % 2 == 0 { (from, to) } else { (to, from) };
        let shift = stagger.anchored(k).unwrap_or_else(|| stagger.phase(k));
        let mut s = (1.0 - shift).fract() * stitch;
        let mut points = Vec::new();
        while s < len - STROKE_FILL_MARGIN {
            if s > STROKE_FILL_MARGIN {
                points.push(side.lerp(other, s / len.max(1e-9)));
            }
            s += stitch;
        }
        if k % 2 == 1 {
            points.reverse();
        }
        run.extend(points);
        run.push(to);
    }
    run
}

/// `custom_ties`: the writer ties the line with its object's own ties (#548), so the line's
/// PE-DESIGN tie is left out.
fn line_elements(
    object: &DesignObject,
    line: &LineSewing,
    user: &[Pattern],
    custom_ties: bool,
) -> Vec<Element> {
    let pitch = line.pitch * 10.0;
    // Closed stem, candlewick and triangle chain lines keep their elements in place and start
    // at the entry point as PE-DESIGN's Start/End Type says (see `start_loop` below).
    let in_place = |c: &Contour| {
        c.closed
            && match line.kind {
                LineType::Stem | LineType::Candlewick => true,
                LineType::Chain => line.chain_shape == ChainShape::Triangle,
                _ => false,
            }
    };
    // The line entry point (L1): closed outlines start there, open ones from the nearer end.
    // A gap (#497) opens the first closed outline: it is sewn from the gap's far end round to
    // its near end, tied in and locked at both.
    let gapped = line
        .gap
        .filter(|_| LineGap::applies_to(line.kind))
        .and_then(|g| {
            let first = object.contours.iter().position(|c| c.closed)?;
            Some((first, g.open(&object.contours[first])?))
        });
    let contours: Vec<Contour> = object
        .contours
        .iter()
        .enumerate()
        .map(|(i, c)| match (&gapped, line.entry) {
            (Some((first, open)), _) if *first == i => open.clone(),
            (_, Some(p)) if !in_place(c) => c.starting_at(p),
            _ => c.clone(),
        })
        .collect();
    // Whether the first line starts with running stitch to the pattern (a shorter tie-in).
    let mut lead_in = false;
    if line.kind == LineType::Cutting {
        let mut needles: [Vec<Run>; 4] = Default::default();
        for c in &contours {
            for (n, runs) in lines::cutting(&c.path(), c.closed, line.leave_uncut)
                .into_iter()
                .enumerate()
            {
                needles[n].extend(runs);
            }
        }
        return needles
            .into_iter()
            .enumerate()
            .map(|(n, runs)| Element {
                thread: cutwork_needle(&line.thread, n),
                runs,
                whole: false,
                lock: false,
                pieces: false,
                across: None,
                rule: Rule::Pe,
            })
            .collect();
    }
    let mut runs_so_far = 0;
    let mut runs: Vec<Run> = contours
        .iter()
        .map(|c| {
            let width = line.width * 10.0;
            let spacing = line.spacing_mm() * 10.0;
            let mut run = if line.kind == LineType::Zigzag && line.underlay {
                lines::line_underlay(
                    &c.path(),
                    width,
                    pitch,
                    line.underlay_style,
                    &line.underlay_auto,
                )
            } else {
                Vec::new()
            };
            let loop_line = match line.kind {
                LineType::Stem => Some(lines::LoopLine::Stem {
                    width,
                    spacing,
                    angle: line.angle.unwrap_or(45.0),
                    single: line.single,
                }),
                LineType::Candlewick => Some(lines::LoopLine::Candlewick {
                    size: width,
                    points: (line.density * 2.0).round().clamp(4.0, 48.0) as usize,
                    spacing,
                    pitch,
                }),
                LineType::Chain => Some(lines::LoopLine::Chain {
                    size: width,
                    times: line.run_times,
                    arrange: line.arrange,
                }),
                _ => None,
            };
            if let (Some(entry), Some(l)) = (line.entry, loop_line)
                && in_place(c)
            {
                let path = c.path();
                // The pattern starts and ends at the exit point (L2); Run leads in from L1.
                let exit = lines::arc_of(&path, line.exit.unwrap_or(entry));
                let run_from = match line.start_end {
                    StartEnd::Feed => None,
                    StartEnd::Run => Some(lines::arc_of(&path, entry)),
                };
                if let Some((top, lead)) =
                    lines::start_loop(&path, &lines::nodes_of(c), l, exit, run_from)
                {
                    lead_in |= lead && runs_so_far == 0;
                    run.extend(top);
                    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
                    runs_so_far += 1;
                    return run;
                }
            }
            runs_so_far += 1;
            let top = match line.kind {
                LineType::Running => generate::running(c, pitch, line.run_times),
                LineType::Triple => generate::triple(c, pitch),
                // PE-DESIGN's zigzag (rails matched along each leg) on straight-line paths;
                // curves keep the evenly spaced zigzag, with half stitch where asked.
                // OpenStitch's satin shaping (#520-#522) replaces both when set.
                LineType::Zigzag if !line.satin_shaping.is_plain() => lines::zigzag_shaped(
                    &c.path(),
                    width,
                    10.0 / line.density.max(0.1),
                    &line.satin_shaping,
                    c.has_curves().then_some(line.half_stitch),
                ),
                LineType::Zigzag if !c.has_curves() => {
                    lines::zigzag_rails(&c.path(), width, 10.0 / line.density.max(0.1))
                }
                LineType::Zigzag => {
                    let path = c.path();
                    let spacing = 10.0 / line.density.max(0.1);
                    let mut run = if line.half_stitch {
                        lines::zigzag_half(&path, width, spacing)
                    } else {
                        generate::zigzag(c, width, spacing)
                    };
                    // Points folded over the line at a cusp or a tight curve (#466).
                    lines::unfold_zigzag(&path, &mut run, width);
                    run
                }
                LineType::Motif => {
                    let pattern =
                        patterns::resolve(user, PatternKind::Motif, line.motif.as_deref());
                    lines::motif(
                        &c.path(),
                        c.closed,
                        &lines::MotifLine {
                            pattern,
                            width,
                            height: line.height.unwrap_or(3.0) * 10.0,
                            gap: spacing,
                            pitch,
                            arrange: line.arrange,
                        },
                    )
                }
                LineType::Stem => lines::stem(
                    &c.path(),
                    &lines::nodes_of(c),
                    width,
                    spacing,
                    // Counter-clockwise from the line on screen, as stored.
                    line.angle.unwrap_or(45.0),
                    line.single,
                ),
                // PE-DESIGN's Medium knot is a star of 24 points (density 12).
                LineType::Candlewick => lines::candlewick(
                    &c.path(),
                    &lines::nodes_of(c),
                    width,
                    (line.density * 2.0).round().clamp(4.0, 48.0) as usize,
                    spacing,
                    pitch,
                ),
                LineType::EStitch | LineType::VStitch => {
                    let e = lines::StrokeLine {
                        interval: spacing,
                        stroke: width,
                        pitch,
                        run_times: line.run_times,
                        stroke_times: line.stroke_times.unwrap_or(1),
                        arrange: line.arrange,
                    };
                    if line.kind == LineType::EStitch {
                        lines::e_stitch(&c.path(), &e)
                    } else {
                        lines::v_stitch(&c.path(), &e)
                    }
                }
                LineType::Chain => lines::chain(
                    &c.path(),
                    &lines::nodes_of(c),
                    width,
                    line.chain_shape,
                    line.run_times,
                    line.arrange,
                ),
                LineType::Cutting => Vec::new(),
            };
            run.extend(top);
            run.dedup_by(|a, b| a.distance(*b) < 1e-9);
            run
        })
        .collect();
    // PE-DESIGN ties in and locks these lines like a satin (line-*-default.pes; motif lines:
    // line-motif-zigzag-3pt.pes, three steps out after its 2.8 mm first stitch); V, stem,
    // candlewicking and chain tie in with three steps out instead of one, but not where a
    // moved entry point starts a stem or candlewick with running stitch
    // (line-*-startend-*.pes; a chain's lead keeps the long one).
    if line.kind != LineType::Cutting && !custom_ties {
        region::tie_and_lock(std::slice::from_mut(&mut runs));
        if matches!(
            line.kind,
            LineType::VStitch | LineType::Stem | LineType::Candlewick | LineType::Chain
        ) && !(lead_in && line.kind != LineType::Chain)
            && let Some(run) = runs.iter_mut().find(|r| r.len() >= 4)
        {
            long_tie_in(run);
        }
    }
    vec![Element {
        thread: line.thread.clone(),
        runs,
        whole: false,
        lock: true,
        pieces: false,
        across: None,
        rule: Rule::Pe,
    }]
}

/// Turns the tie-in that [`region::tie_and_lock`] put at the start of `run` (start, 0.3 mm
/// out, start) into PE-DESIGN's longer one for V, stem, candlewicking and chain: 0.3, 0.6 and
/// 0.9 mm out along the first stitch (at most its length), back to 0.5 mm, then the start.
fn long_tie_in(run: &mut Run) {
    let (a, b) = (run[0], run[3]);
    let len = a.distance(b);
    if len <= 1e-9 || run[2].distance(a) > 1e-9 {
        return;
    }
    let at = |d: f64| a + (b - a) * (d.min(len) / len);
    run.splice(1..3, [at(3.0), at(6.0), at(9.0), at(5.0), a]);
}

/// Thread of cutwork needle `n` (0–3, blade at n × 45°): the line's colour, named after the
/// needle so each needle is its own colour block.
pub fn cutwork_needle(thread: &Thread, n: usize) -> Thread {
    let mut t = thread.clone();
    t.name = Some(format!("Cutwork needle {} ({}°)", n + 1, n * 45).into());
    t.catalog_number = None;
    t.brand = None;
    t
}

#[cfg(test)]
mod stroke_fill_tests {
    use super::*;
    use crate::model::RegionSewing;
    use crate::text::{DEFAULT_FONT, TextSpec};

    /// PE-DESIGN's Fill for text: density 5.0, Step 4.0 mm, Frequency 30 %, no under sewing.
    fn text_fill() -> RegionSewing {
        let mut r = RegionSewing::new(RegionType::Fill, Thread::rgb(0xFF0000));
        r.density = 5.0;
        r.stitch_length = 4.0;
        r.frequency = Some(30.0);
        r.underlay = false;
        r
    }

    fn compiled(text: &str, size: f64, envelope: Option<u8>) -> StitchPattern {
        let mut spec = TextSpec::new(text, DEFAULT_FONT, size);
        spec.envelope = envelope;
        let (obj, _) = DesignObject::text(spec, Some(text_fill()), None).expect("text");
        let mut d = Design::default();
        d.add(obj);
        Compiler::default().compile(&d)
    }

    #[test]
    fn a_fill_stroke_is_split_on_pe_designs_moving_grid() {
        // The left stem of PE-DESIGN's fill H (text-defaults-fill): a row across the stem and a
        // slanting row back, 0.4 mm per pair, every crossing split on a 4 mm grid that moves
        // back 30 % per crossing: needle points 2.8, 1.6, 0.4, 3.2, 2.0 mm from the first side;
        // a grid point 0.2 mm from the far end is left out.
        let sawtooth = [
            v2(0.0, 0.0),
            v2(38.0, 0.0),
            v2(0.0, 4.0),
            v2(38.0, 4.0),
            v2(0.0, 8.0),
            v2(38.0, 8.0),
            v2(0.0, 12.0),
            v2(38.0, 12.0),
        ];
        let run = stroke_fill(&sawtooth, 40.0, generate::Stagger::Regular(0.3));
        let inner: Vec<f64> = run
            .iter()
            .filter(|p| p.x > 0.5 && p.x < 37.5)
            .map(|p| p.x.round())
            .collect();
        // Crossing k's point lies (1 - 0.3 (k + 1)) mod 1 of 4 mm from the first side (x = 0),
        // along the crossing (the slanting ones a little left of that x).
        assert_eq!(inner, vec![28.0, 16.0, 4.0, 32.0, 20.0, 8.0], "{run:?}");
        // No step along the edge: every stitch crosses the stem.
        for w in run.windows(2) {
            assert!(w[0].distance(w[1]) > 3.0, "{w:?}");
        }
    }

    #[test]
    fn fill_text_is_sewn_stroke_by_stroke_as_pe_design_does() {
        // text-defaults-fill: PE-DESIGN 11.50 sews the 20 mm fill H in 483 needle points;
        // OpenStitch sewed 595 by region before.
        let h = compiled("H", 20.0, None).count(Command::Stitch);
        assert!((460..=510).contains(&h), "{h}");
        // stroke-fill-L: spaced by its middle, 341 needle points; by its inner side round its
        // corner, within the text references' 25 % of PE-DESIGN's 276.
        let l = compiled("L", 20.0, None).count(Command::Stitch);
        assert!(l <= 345, "{l}");
    }

    #[test]
    fn fill_rungs_follow_the_inner_side_round_a_corner_and_the_middle_elsewhere() {
        // An L-shaped column, 4 mm wide, straight down then straight on to the right after a
        // 90° corner: its outer side is 20 mm long, its inner one 12 mm.
        let corner = [
            [v2(0.0, -100.0), v2(40.0, -100.0)],
            [v2(0.0, -50.0), v2(40.0, -70.0)],
            [v2(0.0, 0.0), v2(40.0, -40.0)],
            [v2(50.0, 0.0), v2(70.0, -40.0)],
            [v2(100.0, 0.0), v2(100.0, -40.0)],
        ];
        // Inner side: 120 → 30 steps of 4.
        assert_eq!(fill_rungs(&corner, &corner, 4.0, 0.0).len(), 31);
        // A bowl bends all along: measured along its middle.
        let bowl: Vec<[Vec2; 2]> = (0..=8)
            .map(|k| {
                let a = (k as f64 * 22.5).to_radians();
                let (s, c) = (a.dsin(), a.dcos());
                [v2(80.0 * c, 80.0 * s), v2(40.0 * c, 40.0 * s)]
            })
            .collect();
        let middle: f64 = bowl
            .windows(2)
            .map(|w| 0.5 * (w[0][0].distance(w[1][0]) + w[0][1].distance(w[1][1])))
            .sum();
        assert_eq!(
            fill_rungs(&bowl, &bowl, 4.0, 0.0).len(),
            (middle / 4.0).floor() as usize + 1
        );
        // A straight column has no corner: measured along its middle, (100 + 80) / 2 → 22.
        let straight = [
            [v2(0.0, 0.0), v2(40.0, 0.0)],
            [v2(0.0, 100.0), v2(40.0, 80.0)],
        ];
        assert_eq!(fill_rungs(&straight, &straight, 4.0, 0.0).len(), 23);
    }

    /// Satin as PE-DESIGN's text default: density 5.0, no under sewing.
    fn text_satin() -> RegionSewing {
        let mut r = RegionSewing::new(RegionType::Satin, Thread::rgb(0xFF0000));
        r.density = 5.0;
        r.underlay = false;
        r
    }

    /// The README's arched title (examples/lettering.json): 8 mm OPENSTITCH fitted to an arc.
    fn arched(text: &str) -> Design {
        let mut d = Design::default();
        let mut arc = Contour::new(vec![v2(140.0, 520.0), v2(860.0, 520.0)], false);
        arc.curves = vec![Some(crate::geometry::Bezier {
            c1: v2(220.0, 80.0),
            c2: v2(780.0, 80.0),
        })];
        let path = d.add(DesignObject::shape(vec![arc], None, None));
        let mut spec = TextSpec::new(text, DEFAULT_FONT, 8.0);
        spec.path = Some(path);
        let (obj, _) = DesignObject::text(spec, Some(text_satin()), None).expect("text");
        d.add(obj);
        d.relayout_text();
        d.objects[0].visible = false;
        d
    }

    #[test]
    fn a_fitted_letter_is_sewn_as_the_upright_letter_turned() {
        // #444: the stroke cut of a turned outline is not stable (Lato's P and E fell apart on
        // the README's arc, its N was sewn by region). PE-DESIGN sews a fitted letter as the
        // upright one turned (text-fit-satin-ahhhh), so every letter of the arched word has
        // the upright letter's needle points, turned.
        let d = arched("OPENSTITCH");
        let turns = d.text_turns(1);
        assert_eq!(turns.len(), 10);
        assert!(turns.iter().flatten().any(|t| t.abs() > 30.0), "{turns:?}");
        let (upright, _) =
            DesignObject::text(TextSpec::new("OPENSTITCH", DEFAULT_FONT, 8.0), None, None)
                .expect("text");
        let fitted = &d.objects[1];
        for ((up, fit), turn) in upright
            .glyph_groups()
            .iter()
            .zip(fitted.glyph_groups())
            .zip(&turns)
        {
            let sewn = |closed: &[Contour], turn: Option<f64>| {
                stroke_columns(closed, &text_satin(), turn, false)
                    .map(|runs| runs.iter().map(Vec::len).collect::<Vec<_>>())
            };
            let want = sewn(up, None).expect("upright strokes");
            let got = sewn(fit, *turn).expect("fitted strokes");
            assert_eq!(got.len(), want.len(), "pieces at {turn:?}");
            for (g, w) in got.iter().zip(&want) {
                assert!(g.abs_diff(*w) <= 2, "{got:?} vs {want:?} at {turn:?}");
            }
        }
    }

    #[test]
    fn small_satin_letters_reach_their_outline() {
        // #444: "made with open fonts" at 4 mm. Crossings interpolated between 3 mm rungs cut
        // the chords of the bowls (the o's needle points stopped 0.3 mm short of its outer
        // edge); taken from 0.2 mm rungs every needle point lies on the outline.
        let mut spec = TextSpec::new("made with open fonts", DEFAULT_FONT, 4.0);
        spec.letter_spacing = 0.3;
        let (obj, _) = DesignObject::text(spec, Some(text_satin()), None).expect("text");
        for (k, closed) in obj.glyph_groups().into_iter().enumerate() {
            let Some(runs) = stroke_columns(closed, &text_satin(), None, true) else {
                panic!("letter {k} is not sewn by stroke");
            };
            let edges: Vec<(Vec2, Vec2)> = closed.iter().flat_map(Contour::edges).collect();
            let inside: Vec<f64> = runs
                .iter()
                .flatten()
                .map(|&p| {
                    edges
                        .iter()
                        .map(|&(a, b)| crate::skeleton::segment_distance(p, a, b))
                        .fold(f64::INFINITY, f64::min)
                })
                .collect();
            // Off the outline: only crossings fanning into a pointed end (w's feet, i's slanted
            // top), by at most 0.35 mm, and travel cut straight through the letter under a
            // block still to be sewn (#465: one letter here has 6 such of its 58 points).
            let off = inside.iter().filter(|&&d| d > 1.0).count();
            let worst = inside.iter().copied().fold(0.0, f64::max);
            assert!(
                8 * off <= inside.len(),
                "letter {k}: {off} of {} needle points inside",
                inside.len()
            );
            assert!(
                worst < 3.5,
                "letter {k}: a needle point {worst} inside its outline"
            );
        }
    }

    #[test]
    fn warped_fill_text_keeps_its_region_rows() {
        // Under a Text Transform fill text is sewn by region for now (#428): the region rows
        // step along the letter's edge between rows, which a stroke's sawtooth never does.
        let p = compiled("H", 10.0, Some(9));
        let steps = p
            .stitches
            .windows(2)
            .filter(|w| {
                w[1].command == Command::Stitch && {
                    let d = ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
                    d > 0.5 && d < 3.0
                }
            })
            .count();
        assert!(steps > 20, "{steps}");
        assert!(p.count(Command::Stitch) > 100);
    }

    /// `text` in Lato Bold at `size` mm, Satin (PE-DESIGN's text default), compiled.
    fn satin_text(text: &str, size: f64) -> (DesignObject, StitchPattern) {
        let (obj, _) = DesignObject::text(
            TextSpec::new(text, DEFAULT_FONT, size),
            Some(text_satin()),
            None,
        )
        .expect("text");
        let mut d = Design::default();
        d.add(obj.clone());
        (obj, Compiler::default().compile(&d))
    }

    /// How far `p` lies outside the region `closed` (0 inside), 0.1 mm.
    fn outside_by(p: Vec2, closed: &[Contour]) -> f64 {
        if crate::geometry::contains(p, closed) {
            return 0.0;
        }
        closed
            .iter()
            .flat_map(Contour::edges)
            .map(|(a, b)| crate::skeleton::segment_distance(p, a, b))
            .fold(f64::INFINITY, f64::min)
    }

    /// The stitches (needle to needle, no trim or jump between) whose middle lies more than
    /// 0.3 mm outside the text: exposed travel, connectors across gaps, spikes. `(length, from,
    /// to)`, 0.1 mm.
    fn exposed(obj: &DesignObject, p: &StitchPattern) -> Vec<(f64, Vec2, Vec2)> {
        let closed: Vec<Contour> = obj.contours.iter().filter(|c| c.closed).cloned().collect();
        p.stitches
            .windows(2)
            .filter(|w| w[0].command == Command::Stitch && w[1].command == Command::Stitch)
            .map(|w| (v2(w[0].x, w[0].y), v2(w[1].x, w[1].y)))
            .filter(|&(a, b)| outside_by(a.lerp(b, 0.5), &closed) > 3.0)
            .map(|(a, b)| (a.distance(b), a, b))
            .collect()
    }

    #[test]
    fn an_is_dot_is_sewn_as_its_own_piece() {
        // #465: cut together with its stem, the i's dot was lost at 5-10 mm (the stem's column
        // reached up through the gap into it) and the stem tapered into the dot at 20 mm.
        // PE-DESIGN 11.50 sews the dot, trims, then sews the stem (stroke-satin-lower-i).
        for size in [4.0, 5.0, 10.0, 20.0] {
            let (obj, p) = satin_text("iji", size);
            let groups = obj.glyph_groups();
            for k in [0, 2] {
                let closed: Vec<Contour> = groups[k].iter().filter(|c| c.closed).cloned().collect();
                let pieces = letter_pieces(&closed);
                assert_eq!(pieces.len(), 2, "{size} mm");
                let dot = Rect::enclosing(pieces[0].iter().flat_map(Contour::outline_points))
                    .expect("dot");
                let stem = Rect::enclosing(pieces[1].iter().flat_map(Contour::outline_points))
                    .expect("stem");
                assert!(
                    dot.max_y < stem.min_y,
                    "{size} mm: the dot is the top piece"
                );
                let runs = stroke_columns(&closed, &text_satin(), None, size < SMALL_TEXT)
                    .expect("by stroke");
                assert_eq!(runs.len(), 2, "{size} mm: the dot and the stem apart");
                // The dot's satin spans the dot (within 0.3 mm of each side) and stays in it.
                let sewn = Rect::enclosing(runs[0].iter().copied()).expect("dot run");
                for (got, want) in [
                    (sewn.min_x, dot.min_x),
                    (sewn.max_x, dot.max_x),
                    (sewn.min_y, dot.min_y),
                    (sewn.max_y, dot.max_y),
                ] {
                    assert!(
                        (got - want).abs() <= 3.0,
                        "{size} mm: dot {sewn:?} vs {dot:?}"
                    );
                }
                // No needle point of the stem in the gap above it.
                let top = runs[1].iter().map(|q| q.y).fold(f64::INFINITY, f64::min);
                assert!(
                    top >= stem.min_y - 1.0,
                    "{size} mm: stem from {top} ({stem:?})"
                );
            }
            // Nothing sewn across the gaps: every move from a dot to its stem is a trim.
            let gaps = exposed(&obj, &p);
            assert!(gaps.is_empty(), "{size} mm: {gaps:?}");
        }
    }

    #[test]
    fn small_lettering_shows_no_travel_outside_its_letters() {
        // #465: at 5 mm the N's separate pieces were joined by straight stitches along its top
        // and bottom (a crossed box), e->n ("Open") and o->n ("fonts") by connectors across
        // the gap between the letters. PE-DESIGN 11.50 trims between letters and between a
        // letter's separate runs; a move of up to 2 mm between letters may still be sewn
        // across (LETTER_ACROSS).
        for text in [
            "Open",
            "fonts",
            "ROUND AND ROUND",
            "NNN",
            "tttt",
            "made with open fonts",
        ] {
            for size in [4.0, 5.0, 6.0, 8.0, 10.0] {
                let (obj, p) = satin_text(text, size);
                let long: Vec<_> = exposed(&obj, &p)
                    .into_iter()
                    .filter(|e| e.0 > LETTER_ACROSS)
                    .collect();
                assert!(long.is_empty(), "{text} at {size} mm: {long:?}");
            }
        }
    }

    #[test]
    fn travel_cuts_straight_under_a_stroke_it_would_walk_round() {
        // #465: a t's travel from its stem above the bar to the stem below walked round the
        // bar's outline, a box drawn round the bar at 5-10 mm. A stem 2 mm wide from y 0 to
        // 100 with a bar 3 mm long out of its right side at y 40-52: the travel down the
        // stem's right edge from above the bar to below it crosses the bar's root under the
        // bar's satin, still to be sewn, instead of walking round the bar's end.
        let outline = vec![Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(20.0, 0.0),
                v2(20.0, 40.0),
                v2(50.0, 40.0),
                v2(50.0, 52.0),
                v2(20.0, 52.0),
                v2(20.0, 100.0),
                v2(0.0, 100.0),
            ],
            true,
        )];
        let bar = Contour::new(
            vec![
                v2(20.0, 40.0),
                v2(20.0, 52.0),
                v2(50.0, 52.0),
                v2(50.0, 40.0),
            ],
            true,
        );
        let (from, to) = (v2(20.0, 30.0), v2(20.0, 70.0));
        let length = |r: &Run| r.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
        let cut = letter_travel(&outline, from, to, std::slice::from_ref(&bar));
        assert!((length(&cut) - 40.0).abs() < 1e-6, "{cut:?}");
        assert!(cut.iter().all(|p| (p.x - 20.0).abs() < 1e-6), "{cut:?}");
        assert!(
            cut.windows(2)
                .all(|w| w[0].distance(w[1]) <= STROKE_TRAVEL + 1e-9)
        );
        // With nothing left to sew over the bar's root it walks round the bar, as before.
        let walk = letter_travel(&outline, from, to, &[]);
        assert!((length(&walk) - 100.0).abs() < 1e-6, "{walk:?}");
        // Along one straight edge the walk is the straight way and stays.
        let edge = letter_travel(
            &outline,
            v2(0.0, 10.0),
            v2(0.0, 90.0),
            std::slice::from_ref(&bar),
        );
        assert!((length(&edge) - 80.0).abs() < 1e-6, "{edge:?}");
    }
}
