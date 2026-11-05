use crate::fmath::Det;
use crate::generate::{self, Run};
use crate::geometry::{Contour, Rect, Transform, Vec2, v2};
use crate::lines;
use crate::model::{
    ChainShape, Design, DesignObject, LineSewing, LineType, ObjectKind, RegionType, StartEnd,
    StitchBlock,
};
use crate::patterns::{self, Pattern, PatternKind};
use crate::region;
use crate::regions;
use crate::route;
use crate::stamp;
use openstitch_core::{Command, Stitch, StitchPattern, Thread, round_coordinate};

/// Turns an editable [`Design`] into a machine [`StitchPattern`].
///
/// Order: objects in list order; within a shape, underlay, region, then outline. A thread change
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
}

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
}

impl Writer {
    fn push(&mut self, p: Vec2, c: Command) {
        // A stitch on the needle point just sewn would be a zero-length record: a second
        // penetration in the same hole. PE-DESIGN 11.50 writes none (no PEC record of length 0 in
        // any of its 177 files measured on 2026-10-27), so generators that end one run where the
        // next begins, or produce two points within 0.05 mm, sew one needle point. A zero-length
        // stitch right after a jump is kept: the jump's landing is not a penetration, so that
        // stitch is the first needle point of the block (PE-DESIGN ties in there instead).
        if c == Command::Stitch
            && let Some(last) = self.out.stitches.last()
            && last.command == Command::Stitch
            && round_coordinate(last.x) == round_coordinate(p.x)
            && round_coordinate(last.y) == round_coordinate(p.y)
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
        if tail.len() < 2 || crate::region::ends_locked(&tail) {
            return;
        }
        for p in crate::region::lock_points(tail[1], tail[0]) {
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

    fn travel(&mut self, p: Vec2, within_element: bool) {
        let Some(from) = self.position else {
            self.push(p, Command::Jump);
            self.position = Some(p);
            return;
        };
        let d = from.distance(p);
        if d == 0.0 {
            return;
        }
        if within_element && d <= self.cfg.connect_distance && !self.cut {
            self.stitch_line(p);
            return;
        }
        // PE-DESIGN sews a short move between objects across as a stitch rather than jumping.
        if !within_element && d <= self.cfg.sew_across_distance && !self.cut {
            self.stitch_line(p);
            return;
        }
        if d > self.cfg.trim_distance && !self.cut && self.trim_jumps {
            self.end_lock();
            self.push(from, Command::Trim);
            self.cut = true;
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
        match &self.thread {
            Some(t) if *t != element.thread => {
                let p = self.position.unwrap_or_default();
                if !self.cut && self.position.is_some() {
                    self.end_lock();
                    self.push(p, Command::Trim);
                    self.cut = true;
                }
                self.push(p, Command::ColorChange);
                self.out.threads.push(element.thread.clone());
            }
            None => self.out.threads.push(element.thread.clone()),
            _ => {}
        }
        self.thread = Some(element.thread);
        for (i, run) in runs.iter().enumerate() {
            self.travel(run[0], i > 0);
            // A run the writer starts after a trim ties in (unless its generator already did).
            let fresh = self.position.is_none()
                || self.cut
                || self
                    .out
                    .stitches
                    .last()
                    .is_some_and(|s| s.command == Command::Jump);
            let tied;
            let run = if element.lock && fresh && !crate::region::starts_tied(run) {
                let mut r = run.clone();
                crate::region::tie_in(&mut r);
                tied = r;
                &tied
            } else {
                run
            };
            self.lock_open = element.lock;
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
        let mut w = self.writer();
        w.out.set_name(design.name.clone());
        for (key, value) in design.properties.fields() {
            if !value.is_empty() {
                w.out.metadata.insert(key.into(), value.into());
            }
        }
        for object in design.objects.iter().filter(|o| o.visible) {
            sew_object(&mut w, object, design);
        }
        w.finish()
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

    fn writer(&self) -> Writer {
        Writer {
            cfg: *self,
            out: StitchPattern::new(),
            thread: None,
            position: None,
            cut: true,
            trim_jumps: true,
            lock_open: false,
        }
    }
}

impl Writer {
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
    match object.kind {
        ObjectKind::Shape | ObjectKind::Text => {
            let holes = holes_above(object, design);
            let turns = design
                .objects
                .iter()
                .position(|o| std::ptr::eq(o, object))
                .map(|i| design.text_turns(i))
                .unwrap_or_default();
            for e in elements(object, &holes, &design.patterns, &turns) {
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
) -> Vec<Element> {
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
            // stroke cut of warped letters is not measured yet and drifts 0.1-0.3 mm past
            // seven transform references' bounds (#428).
            let warped = object.text.as_ref().is_some_and(|t| t.envelope.is_some());
            let by_stroke = (text
                && (region.kind == RegionType::Satin
                    || (region.kind == RegionType::Fill && !warped))
                && region.stamps.is_empty())
            .then(|| stroke_columns(&closed, region, turns.get(index).copied().flatten()))
            .flatten();
            if let (Some(runs), Some(top)) = (by_stroke, layers.last_mut()) {
                *top = runs;
            } else if let Some(top) = layers.last_mut() {
                let runs = stamp::apply(std::mem::take(top), &region.stamps);
                // PE-DESIGN 11.50 sews Concentric Circle and Spiral in the order the Start
                // Point button gives, jumping in and ending where the pattern ends (references
                // concentric-*, spiral-*): no travel from R1 or to R2, no reversal.
                *top = if matches!(
                    region.kind,
                    RegionType::ConcentricCircle | RegionType::Spiral | RegionType::Contour
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
            if matches!(
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
            for runs in layers {
                out.push(Element {
                    thread: region.thread.clone(),
                    runs,
                    whole: region.kind == RegionType::Satin,
                    lock: true,
                });
            }
        }
    }
    if let Some(line) = &object.line {
        out.extend(line_elements(object, line, user));
    }
    out
}

/// Rungs of a letter's blocks this far apart (0.1 mm): the direction lines the satin follows.
const STROKE_RUNGS: f64 = 30.0;
/// Rungs of a fill letter's blocks this far apart (0.1 mm), the step the strokes' sides are
/// sampled at: the outline its rows end on.
const STROKE_FILL_RUNGS: f64 = 2.0;
/// Travel between a letter's blocks: running stitches along its outline at most 2 mm long.
const STROKE_TRAVEL: f64 = 20.0;

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
fn stroke_columns(
    closed: &[Contour],
    region: &crate::model::RegionSewing,
    turn: Option<f64>,
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
    let g = crate::strokes::strokes(&upright)?;
    let turned = |mut blocks: Vec<crate::strokes::Block>| {
        if let Some(t) = &back {
            for p in blocks.iter_mut().flat_map(|b| b.rungs.iter_mut().flatten()) {
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
    let blocks = g.blocks_paired(&upright, STROKE_RUNGS, pairing);
    if !blocks_cover(&blocks, &upright) {
        return None;
    }
    let blocks = turned(blocks);
    // Rows and crossings take their ends from rungs this close together, so they stay on the
    // outline where it curves: fill rows interpolated between the 3 mm rungs ended up to
    // 0.2 mm inside O's counter, and satin crossings cut the chords of a small letter's bowls
    // (the 4 mm o's crossings stopped short of its outer edge, #444). The same strokes give
    // the same blocks in the same order at any rung spacing.
    let fine = Some(turned(g.blocks_paired(
        &upright,
        STROKE_FILL_RUNGS,
        pairing,
    )))
    .filter(|f| f.len() == blocks.len());
    let spacing = 10.0 / region.density.max(0.1);
    let pull = region.pull_compensation * 10.0;
    let fill = region.kind == RegionType::Fill;
    let stitch = (region.stitch_length * 10.0).max(1.0);
    let mut runs: Vec<Run> = Vec::new();
    let mut run: Run = Vec::new();
    for (i, b) in blocks.iter().enumerate() {
        if b.rungs.len() < 2 {
            continue;
        }
        if b.new_run && run.len() >= 2 {
            runs.push(std::mem::take(&mut run));
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
            // rungs (T's, short and skewed, would read as bending).
            let rungs = match fine.as_ref() {
                Some(f)
                    if !b.junction
                        && (b.rungs.len() <= 3
                            || block_bends(&f[i].rungs)
                            || turns_corner(&b.rungs)) =>
                {
                    satin_rungs(&f[i].rungs, turns_corner(&b.rungs), 2.0 * spacing)
                }
                _ => b.rungs.clone(),
            };
            crate::column::satin(&rungs, spacing, pull)
        };
        // PE-DESIGN walks the letter's outline between its blocks (stroke-blocks-H: running
        // pieces between them).
        if let (Some(&from), Some(&to)) = (run.last(), column.first()) {
            run.extend(
                route::travel(closed, from, to, STROKE_TRAVEL)
                    .into_iter()
                    .skip(1),
            );
            run.pop();
        }
        run.extend(column);
    }
    if run.len() >= 2 {
        runs.push(run);
    }
    (!runs.is_empty()).then_some(runs)
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

fn line_elements(object: &DesignObject, line: &LineSewing, user: &[Pattern]) -> Vec<Element> {
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
    let contours: Vec<Contour> = object
        .contours
        .iter()
        .map(|c| match line.entry {
            Some(p) if !in_place(c) => c.starting_at(p),
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
                lines::zigzag_underlay(&c.path(), width / 4.0, pitch)
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
                if let Some((top, lead)) = lines::start_loop(&path, l, exit, run_from) {
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
                LineType::Zigzag if !c.has_curves() => {
                    lines::zigzag_rails(&c.path(), width, 10.0 / line.density.max(0.1))
                }
                LineType::Zigzag if line.half_stitch => {
                    lines::zigzag_half(&c.path(), width, 10.0 / line.density.max(0.1))
                }
                LineType::Zigzag => generate::zigzag(c, width, 10.0 / line.density.max(0.1)),
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
                    width,
                    spacing,
                    // Counter-clockwise from the line on screen, as stored.
                    line.angle.unwrap_or(45.0),
                    line.single,
                ),
                // PE-DESIGN's Medium knot is a star of 24 points (density 12).
                LineType::Candlewick => lines::candlewick(
                    &c.path(),
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
    if line.kind != LineType::Cutting {
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
                stroke_columns(closed, &text_satin(), turn)
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
            let Some(runs) = stroke_columns(closed, &text_satin(), None) else {
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
            // top), by at most 0.35 mm.
            let off = inside.iter().filter(|&&d| d > 1.0).count();
            let worst = inside.iter().copied().fold(0.0, f64::max);
            assert!(
                10 * off <= inside.len(),
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
}
