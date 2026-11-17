//! Join Lines and Break Apart (#744, beyond PE-DESIGN). `shape.joinLines` makes open lines,
//! satin skeletons or Manual Punch columns whose ends meet one object, sewn as one path with
//! one tie-in and one tie-off. `object.breakApart` does the reverse for compound objects:
//! lettering into one text object per letter, a region with several islands into one object
//! per piece (holes stay with the piece around them), a line object with several outlines into
//! one object per outline, and grouped sets (copies along a line, a monogram) into separate
//! objects. Each is one undo step.

use crate::commands::{CommandSpec, Effect, IDS, MM, indexes, opt, req, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_design::boolean;
use openstitch_design::extend::{self, End};
use openstitch_design::skeleton::{self, Skeleton};
use openstitch_design::text::{self, TextSpec};
use openstitch_design::{
    Bezier, Contour, Design, DesignObject, ObjectKind, Rect, Transform, Vec2, column,
};
use serde_json::json;

/// Default and largest gap between ends that still join, mm.
const DEFAULT_TOLERANCE: f64 = 1.0;
const MAX_TOLERANCE: f64 = 10.0;
/// Ends this close (0.1 mm) are one point: the second is dropped instead of bridged.
const SAME_POINT: f64 = 0.5;
/// A letter broken out of a text must land within this (0.1 mm) of where it was.
const LETTER_MATCH: f64 = 0.5;
/// Outlines this close (0.1 mm) count as derived from their skeleton or direction lines.
const SAME: f64 = 0.05;
/// Most pieces one Break Apart makes.
const MAX_PIECES: usize = 2_000;

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "shape.joinLines",
            "Join open lines (or satin skeletons, or Manual Punch columns) whose ends meet into one object, sewn as one path with no trim between them. Sewing attributes and thread: the first id's (its line or region sewing, colour included); the others' are dropped. Sewing order: the joined object sews where the first id did; the others are removed from the order. The others are chained on at their nearest ends",
            Effect::Edit,
            vec![
                req(
                    "ids",
                    "[int]",
                    "2 or more objects of one kind: open lines (one open outline each), satin skeletons or Manual Punch columns; the first one's attributes are kept",
                ),
                opt(
                    "tolerance",
                    "mm 0-10",
                    "Largest gap between two ends that are joined (default 1); a gap is bridged by a straight segment",
                ),
            ],
            join_lines,
        ),
        spec(
            "object.breakApart",
            "Break compound objects into pieces: text into one text object per letter (same place; text on a path, in a Text Transform, arched or in a Name Drop box into one shape per letter), a region with several islands into one object per piece (holes stay with their piece), a line object with several outlines into one per outline, and groups (copies along a line, monograms) into separate objects. Sewing attributes and thread: every piece keeps the whole's (same colour, so no colour change between them). Sewing order: the pieces sew one after another where the whole did, the first keeping its id",
            Effect::Edit,
            vec![IDS],
            break_apart,
        ),
    ]
}

// MARK: join

/// What one joinable object is.
#[derive(Clone)]
enum Part {
    Line(Contour),
    Skeleton(Skeleton),
    Column(Vec<[Vec2; 2]>, bool),
}

impl Part {
    fn family(&self) -> &'static str {
        match self {
            Part::Line(_) => "an open line",
            Part::Skeleton(_) => "a satin skeleton",
            Part::Column(..) => "a Manual Punch column",
        }
    }

    /// Its two ends (start, end).
    fn ends(&self) -> (Vec2, Vec2) {
        let mid = |l: &[Vec2; 2]| (l[0] + l[1]) * 0.5;
        match self {
            Part::Line(c) => (c.points[0], *c.points.last().expect("open line")),
            Part::Skeleton(s) => (s.points[0], *s.points.last().expect("skeleton")),
            Part::Column(lines, _) => (mid(&lines[0]), mid(lines.last().expect("column"))),
        }
    }

    fn reversed(&self) -> Part {
        match self {
            Part::Line(c) => Part::Line(reverse_contour(c)),
            Part::Skeleton(s) => Part::Skeleton(Skeleton {
                points: s.points.iter().rev().copied().collect(),
                widths: s.widths.iter().rev().copied().collect(),
            }),
            Part::Column(lines, curved) => {
                Part::Column(lines.iter().rev().copied().collect(), *curved)
            }
        }
    }

    /// Length along the part, 0.1 mm (lines only; the others report 0).
    fn length(&self) -> f64 {
        match self {
            Part::Line(c) => c.path().windows(2).map(|w| w[0].distance(w[1])).sum(),
            _ => 0.0,
        }
    }
}

/// The open contour `c` drawn the other way (curve handles swapped).
fn reverse_contour(c: &Contour) -> Contour {
    let n = c.segment_count();
    let mut out = Contour::new(c.points.iter().rev().copied().collect(), false);
    if c.has_curves() {
        out.curves = (0..n)
            .rev()
            .map(|i| c.curve(i).map(|b| Bezier { c1: b.c2, c2: b.c1 }))
            .collect();
    }
    out
}

/// The joinable part object `i` is.
fn part(d: &Design, p: &Params, i: usize) -> Result<Part, CommandError> {
    let o = &d.objects[i];
    match o.kind {
        ObjectKind::Shape => {}
        ObjectKind::Text => {
            return Err(p.failed(format!(
                "object {} is text; only open lines, satin skeletons and Manual Punch columns join",
                o.id
            )));
        }
        ObjectKind::Stitches => {
            return Err(p.failed(format!("object {} is a stitch pattern", o.id)));
        }
    }
    if let Some(r) = &o.region
        && let [c] = o.contours.as_slice()
    {
        if let Some(sk) = &r.skeleton
            && skeleton::is_outline_of(c, sk, SAME)
        {
            return Ok(Part::Skeleton(sk.clone()));
        }
        if r.direction_lines.len() >= 2 {
            let own = if r.curved_lines {
                column::curved_outline(&r.direction_lines)
            } else {
                column::outline(&r.direction_lines)
            };
            if extend::same_outline(c, &own, SAME) {
                return Ok(Part::Column(r.direction_lines.clone(), r.curved_lines));
            }
        }
    }
    match o.contours.as_slice() {
        [c] if !c.closed && c.points.len() >= 2 => Ok(Part::Line(c.clone())),
        [c] if c.closed => Err(p.failed(format!(
            "object {} is closed; only open lines join (shape.merge combines closed shapes)",
            o.id
        ))),
        _ => Err(p.failed(format!(
            "object {} has {} outlines; only an object with a single open line joins (break it apart first)",
            o.id,
            o.contours.len()
        ))),
    }
}

/// `a` (ending at the joint) continued by `b` (starting there).
fn chain(p: &Params, a: Part, b: Part) -> Result<Part, CommandError> {
    match (a, b) {
        (Part::Line(a), Part::Line(b)) => {
            let skip =
                usize::from(a.points.last().expect("open").distance(b.points[0]) <= SAME_POINT);
            let pts = &b.points[skip..];
            // Segment k of the continuation runs from the previous point to pts[k]: with the
            // first point dropped it is b's own segment k; else segment 0 is the bridge.
            let curves: Vec<Option<Bezier>> = (0..pts.len())
                .map(|k| {
                    if skip == 1 {
                        b.curve(k)
                    } else {
                        k.checked_sub(1).and_then(|j| b.curve(j))
                    }
                })
                .collect();
            if pts.is_empty() {
                return Ok(Part::Line(a));
            }
            extend::path(&a, End::End, pts, &curves)
                .map(Part::Line)
                .ok_or_else(|| p.failed("cannot join these lines"))
        }
        (Part::Skeleton(a), Part::Skeleton(b)) => {
            let skip =
                usize::from(a.points.last().expect("skeleton").distance(b.points[0]) <= SAME_POINT);
            Ok(Part::Skeleton(extend::skeleton(
                &a,
                End::End,
                &b.points[skip..],
                &b.widths[skip..],
            )))
        }
        (Part::Column(a, curved), Part::Column(b, _)) => {
            let last = *a.last().expect("column");
            // The continuation's first ends go on the same side as the column's.
            let straight = last[0].distance(b[0][0]) + last[1].distance(b[0][1]);
            let swapped = last[0].distance(b[0][1]) + last[1].distance(b[0][0]);
            let mut lines: Vec<[Vec2; 2]> = if swapped < straight {
                b.iter().map(|l| [l[1], l[0]]).collect()
            } else {
                b.clone()
            };
            if last[0].distance(lines[0][0]) <= SAME_POINT
                && last[1].distance(lines[0][1]) <= SAME_POINT
            {
                lines.remove(0);
            }
            if lines.is_empty() {
                return Ok(Part::Column(a, curved));
            }
            extend::column(&a, End::End, &lines, curved)
                .map(|l| Part::Column(l, curved))
                .map_err(|e| crate::punch::rung_error(p, e))
        }
        (a, b) => Err(p.failed(format!(
            "{} and {} cannot be joined",
            a.family(),
            b.family()
        ))),
    }
}

fn join_lines(s: &mut EngineState, p: &Params) -> crate::params::CommandResult {
    let idx = indexes(p, &s.design)?;
    let mut seen = idx.clone();
    seen.sort_unstable();
    seen.dedup();
    if seen.len() != idx.len() || idx.len() < 2 {
        return Err(p.invalid("ids", "needs at least 2 different objects"));
    }
    let tolerance = p.opt("tolerance", DEFAULT_TOLERANCE, 0.0, MAX_TOLERANCE)? * MM;
    let parts: Vec<Part> = idx
        .iter()
        .map(|&i| part(&s.design, p, i))
        .collect::<Result<_, _>>()?;
    let first_family = parts[0].family();
    if let Some((k, other)) = parts
        .iter()
        .enumerate()
        .find(|(_, q)| q.family() != first_family)
    {
        return Err(p.failed(format!(
            "object {} is {} and object {} {}; only objects of one kind join",
            s.design.objects[idx[0]].id,
            first_family,
            s.design.objects[idx[k]].id,
            other.family()
        )));
    }
    // The chain keeps the first object's direction; others are added at either end.
    let first_len = parts[0].length();
    let mut joined = parts[0].clone();
    // Length added before the first object's start (for width keys), 0.1 mm.
    let mut before = 0.0;
    let mut rest: Vec<(usize, Part)> = parts.into_iter().enumerate().skip(1).collect();
    while !rest.is_empty() {
        let (start, end) = joined.ends();
        // The remaining part with the end nearest either end of the chain.
        let mut best: Option<(f64, usize, bool, bool)> = None;
        for (k, (_, q)) in rest.iter().enumerate() {
            let (qs, qe) = q.ends();
            for (dist, at_end, flip) in [
                (end.distance(qs), true, false),
                (end.distance(qe), true, true),
                (start.distance(qe), false, false),
                (start.distance(qs), false, true),
            ] {
                if best.is_none_or(|b| dist < b.0) {
                    best = Some((dist, k, at_end, flip));
                }
            }
        }
        let (dist, k, at_end, flip) = best.expect("parts remain");
        let (pos, q) = rest.remove(k);
        if dist > tolerance {
            return Err(p.failed(format!(
                "object {} is {:.2} mm from the nearest end of the others; the tolerance is {:.2} mm",
                s.design.objects[idx[pos]].id,
                dist / MM,
                tolerance / MM
            )));
        }
        let q = if flip { q.reversed() } else { q };
        if at_end {
            joined = chain(p, joined, q)?;
        } else {
            // Prepend: chain the reversed pieces, then turn the result back.
            let added = q.length() + dist;
            joined = chain(p, joined.reversed(), q.reversed())?.reversed();
            before += added;
        }
    }
    let keep = idx[0];
    let id = s.design.objects[keep].id;
    let total = joined.length();
    let o = &mut s.design.objects[keep];
    match joined {
        Part::Line(c) => {
            o.contours = vec![c];
            if let Some(l) = &mut o.line {
                // Width keys and a gap are placed along the first line: they move with it.
                if total > 0.0 && first_len > 0.0 {
                    let scale = |at: f64| (before + at * first_len) / total;
                    for key in &mut l.width_keys {
                        key.at = scale(key.at).clamp(0.0, 1.0);
                    }
                }
                if let Some(g) = &mut l.gap {
                    g.offset += before / MM;
                }
            }
        }
        Part::Skeleton(sk) => {
            let outline =
                skeleton::outline(&sk).ok_or_else(|| p.failed("the joined satin is too short"))?;
            o.contours = vec![outline];
            o.region.as_mut().expect("skeleton").skeleton = Some(sk);
        }
        Part::Column(lines, curved) => {
            o.contours = vec![if curved {
                column::curved_outline(&lines)
            } else {
                column::outline(&lines)
            }];
            o.region.as_mut().expect("column").direction_lines = lines;
        }
    }
    crate::shape_ops::keep_points_on_outline(o);
    let gone: Vec<u64> = idx[1..].iter().map(|&i| s.design.objects[i].id).collect();
    for t in 0..s.design.objects.len() {
        if s.design.objects[t]
            .text
            .as_ref()
            .and_then(|x| x.path)
            .is_some_and(|p| gone.contains(&p))
        {
            s.design.detach_text(t);
        }
    }
    s.design.objects.retain(|o| !gone.contains(&o.id));
    Ok(json!({ "id": id, "removed": gone }))
}

// MARK: break apart

fn break_apart(s: &mut EngineState, p: &Params) -> crate::params::CommandResult {
    let mut idx = indexes(p, &s.design)?;
    idx.sort_unstable();
    idx.dedup();
    let ids: Vec<u64> = idx.iter().map(|&i| s.design.objects[i].id).collect();
    // Groups first: every member of a selected object's group goes free.
    let mut ungrouped = Vec::new();
    for gid in ids.iter().filter_map(|&id| {
        s.design
            .index_of(id)
            .and_then(|i| s.design.objects[i].group)
    }) {
        if !ungrouped.contains(&gid) {
            ungrouped.push(gid);
        }
    }
    for o in &mut s.design.objects {
        if o.group.is_some_and(|g| ungrouped.contains(&g)) {
            o.group = None;
            o.monogram = None;
        }
    }
    let mut pieces = Vec::new();
    let mut letters_as = Vec::new();
    for &id in &ids {
        let i = s.design.index_of(id).expect("selected");
        let made = match s.design.objects[i].kind {
            ObjectKind::Text => {
                let (made, how) = break_text(s, p, i)?;
                if !made.is_empty() {
                    letters_as.push(json!({ "id": id, "as": how }));
                }
                made
            }
            ObjectKind::Shape => break_shape(s, i),
            ObjectKind::Stitches => Vec::new(),
        };
        if pieces.len() + made.len() > MAX_PIECES {
            return Err(p.failed(format!("would make more than {MAX_PIECES} pieces")));
        }
        pieces.extend(made);
    }
    if pieces.is_empty() && ungrouped.is_empty() {
        return Err(p.failed(
            "nothing to break apart: select text, a region with several islands, a line object with several outlines, or a group",
        ));
    }
    Ok(json!({ "ids": pieces, "ungrouped": ungrouped, "text": letters_as }))
}

/// Points of `contours`, flattened, for comparing placements.
fn outline_points(contours: &[Contour]) -> Vec<Vec2> {
    contours.iter().flat_map(Contour::outline_points).collect()
}

/// A text's letters as text objects at the same places, or (when a letter cannot be set on its
/// own there) as shapes. Returns the new ids in sewing order and "text" or "shapes".
fn break_text(
    s: &mut EngineState,
    p: &Params,
    i: usize,
) -> Result<(Vec<u64>, &'static str), CommandError> {
    let o = &s.design.objects[i];
    let spec = o.text.clone().expect("text object");
    if o.stitch_block.is_some() {
        return Err(p.failed(format!(
            "object {} has stitch glyphs; convert it with text.toShapes",
            o.id
        )));
    }
    let groups: Vec<Vec<Contour>> = o.glyph_groups().iter().map(|g| g.to_vec()).collect();
    if groups.len() < 2 {
        return Ok((Vec::new(), "text"));
    }
    let path = spec.path.and_then(|id| s.design.path_contour(id));
    let layout = text::layout_in(&spec, path.as_ref(), &s.design.fonts)
        .map_err(|e| p.failed(e.to_string()))?;
    let glyphs: Vec<(char, usize)> = layout.outlined().map(|g| (g.ch, g.index)).collect();
    let base = o.name.clone().unwrap_or_else(|| format!("Text {}", o.id));
    let plain = spec.path.is_none()
        && spec.envelope.is_none()
        && spec.custom_envelope.is_none()
        && spec.arc.is_none()
        && spec.name_drop.is_none()
        && glyphs.len() == groups.len();
    let letters = if plain {
        letter_texts(s, o, &spec, &glyphs, &groups)
    } else {
        None
    };
    let how = if letters.is_some() { "text" } else { "shapes" };
    let made: Vec<DesignObject> = match letters {
        Some(l) => l,
        None => groups
            .iter()
            .enumerate()
            .map(|(k, g)| {
                let mut shape = piece_of(o);
                shape.kind = ObjectKind::Shape;
                shape.text = None;
                shape.stitch_block = None;
                shape.contours = g.clone();
                shape.name = Some(letter_name(&base, &glyphs, groups.len(), k));
                shape
            })
            .collect(),
    };
    Ok((replace_with(&mut s.design, i, made), how))
}

fn letter_name(base: &str, glyphs: &[(char, usize)], groups: usize, k: usize) -> String {
    match glyphs.get(k) {
        Some((c, _)) if glyphs.len() == groups => format!("{base} '{c}'"),
        _ => format!("{base} {}", k + 1),
    }
}

/// One text object per letter, each laid out on its own and moved onto the letter it was;
/// `None` when any letter does not land where it was.
fn letter_texts(
    s: &EngineState,
    o: &DesignObject,
    spec: &TextSpec,
    glyphs: &[(char, usize)],
    groups: &[Vec<Contour>],
) -> Option<Vec<DesignObject>> {
    let base = o.name.clone().unwrap_or_else(|| format!("Text {}", o.id));
    let mut out = Vec::new();
    for (k, (&(ch, index), group)) in glyphs.iter().zip(groups).enumerate() {
        let mut one = TextSpec {
            text: ch.to_string(),
            characters: spec
                .characters
                .iter()
                .filter(|e| e.index == index)
                .map(|e| text::CharEdit {
                    index: 0,
                    ..e.clone()
                })
                .collect(),
            ..spec.clone()
        };
        let first = text::layout_in(&one, None, &s.design.fonts).ok()?;
        let here = outline_points(&first.contours());
        let want = outline_points(group);
        let (&a, &b) = (here.first()?, want.first()?);
        one.transform = spec
            .transform
            .then(&Transform::translation(b.x - a.x, b.y - a.y));
        let placed = text::layout_in(&one, None, &s.design.fonts).ok()?;
        let got = outline_points(&placed.contours());
        if got.len() != want.len()
            || got
                .iter()
                .zip(&want)
                .any(|(g, w)| g.distance(*w) > LETTER_MATCH)
        {
            return None;
        }
        let mut letter = piece_of(o);
        letter.contours = placed.contours();
        one.glyphs = placed.glyph_counts();
        letter.text = Some(one);
        letter.name = Some(letter_name(&base, glyphs, groups.len(), k));
        out.push(letter);
    }
    Some(out)
}

/// A copy of `o` to become one of its pieces: same sewing attributes, no hand edits (they were
/// made on the whole), no monogram.
fn piece_of(o: &DesignObject) -> DesignObject {
    let mut c = o.clone();
    c.hand_edits = None;
    c.monogram = None;
    c.knockdown = None;
    c
}

/// A shape's pieces: one object per island of a region (holes kept with their island), or one
/// per outline of a line-only object. Empty when it is already one piece.
fn break_shape(s: &mut EngineState, i: usize) -> Vec<u64> {
    let o = &s.design.objects[i];
    // Satin skeletons and Manual Punch columns are one piece by construction.
    if o.region
        .as_ref()
        .is_some_and(|r| r.skeleton.is_some() || r.direction_lines.len() >= 2)
    {
        return Vec::new();
    }
    let all_closed = !o.contours.is_empty() && o.contours.iter().all(|c| c.closed);
    let mut sets: Vec<Vec<Contour>> = if o.region.is_some() && all_closed {
        boolean::simplify(&o.contours)
    } else {
        o.contours.iter().map(|c| vec![c.clone()]).collect()
    };
    if sets.len() < 2 {
        return Vec::new();
    }
    // Top to bottom, then left to right: a stable, readable order.
    let corner = |set: &Vec<Contour>| {
        Rect::enclosing(outline_points(set)).map_or((0.0, 0.0), |b| (b.min_y, b.min_x))
    };
    sets.sort_by(|a, b| {
        let (ya, xa) = corner(a);
        let (yb, xb) = corner(b);
        ya.total_cmp(&yb).then(xa.total_cmp(&xb))
    });
    // Hole sewing stays with the piece around the inner shape.
    let inner_at = o.hole_inner.and_then(|h| {
        let d = &s.design;
        d.index_of(h)
            .and_then(|j| d.objects[j].bounds())
            .map(|b| b.center())
    });
    let id = o.id;
    let made: Vec<DesignObject> = sets
        .iter()
        .map(|set| {
            let mut piece = piece_of(o);
            crate::shape_ops::set_region(&mut piece, std::slice::from_ref(set));
            if piece.hole_sewing {
                let holds = inner_at.is_some_and(|c| {
                    set.first().is_some_and(|outer| {
                        openstitch_design::geometry::contains(c, std::slice::from_ref(outer))
                    })
                });
                if !holds {
                    piece.hole_sewing = false;
                    piece.hole_inner = None;
                }
            }
            piece
        })
        .collect();
    // Text fitted to this outline no longer has one outline to follow.
    for t in 0..s.design.objects.len() {
        if s.design.objects[t].text.as_ref().and_then(|x| x.path) == Some(id) {
            s.design.detach_text(t);
        }
    }
    replace_with(&mut s.design, i, made)
}

/// Puts `pieces` where object `i` was: the first keeps its id (and its place in the sewing
/// order), the others get new ids right after it, in order. Returns every piece's id.
fn replace_with(d: &mut Design, i: usize, pieces: Vec<DesignObject>) -> Vec<u64> {
    if pieces.is_empty() {
        return Vec::new();
    }
    let keep = d.objects[i].id;
    let mut ids = Vec::new();
    for (k, mut piece) in pieces.into_iter().enumerate() {
        if k == 0 {
            piece.id = keep;
            d.objects[i] = piece;
        } else {
            piece.id = d.next_object_id;
            d.next_object_id += 1;
            d.objects.insert(i + k, piece);
        }
        ids.push(d.objects[i + k].id);
    }
    ids
}
