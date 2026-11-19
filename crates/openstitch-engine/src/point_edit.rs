//! The Select Point tool's menu as commands (#1258): `object.setPointKind` (To Straight,
//! To Curve, To Smooth, To Symmetry, To Cusp) and `object.splitAtPoint`. Deleting points is
//! `object.deletePoint` in `commands.rs`.

use crate::commands::{CONTOUR, CommandSpec, Effect, contour_mut, opt, req, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_design::{Contour, ObjectKind, PointKind};
use serde_json::{Value, json};

pub(crate) const INDEXES: crate::commands::ParamDoc = opt(
    "indexes",
    "[int]",
    "Several vertices of the contour at once, in place of 'index' (one undo step)",
);

/// The vertices named by `index` or `indexes`, each once, ascending.
pub(crate) fn point_indexes(p: &Params, c: &Contour) -> Result<Vec<usize>, CommandError> {
    let last = c.points.len() as i64 - 1;
    let mut out: Vec<usize> = if p.has("indexes") {
        if p.has("index") {
            return Err(p.invalid("indexes", "cannot be combined with 'index'"));
        }
        let list = p.array("indexes")?;
        if list.is_empty() {
            return Err(p.invalid("indexes", "must not be empty"));
        }
        list.iter()
            .map(|v| {
                v.as_i64()
                    .filter(|i| (0..=last).contains(i))
                    .map(|i| i as usize)
                    .ok_or_else(|| p.invalid("indexes", format!("must be integers in 0...{last}")))
            })
            .collect::<Result<_, _>>()?
    } else {
        vec![p.int("index", None, 0, last)? as usize]
    };
    out.sort_unstable();
    out.dedup();
    Ok(out)
}

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "object.setPointKind",
            "Change what kind of point a vertex is, as the Select Point tool's right-click menu does (To Straight, To Curve, To Smooth, To Symmetry, To Cusp). The kind belongs to the point, as in PE-DESIGN: a straight point has no handles, a curve point has two and is smooth, symmetric or a cusp; a side is a curve when either of its ends is a curve point. Answers 'changed', the number of points changed. The stitches change only where the outline does. One outline of one object a call: for points on several outlines or objects send one call per outline (the editor's Point tool does, with all of them in one undo step)",
            Effect::Edit,
            vec![
                req("id", "int", "Shape object"),
                CONTOUR,
                opt("index", "int", "Vertex index (or 'indexes')"),
                INDEXES,
                req(
                    "kind",
                    "straight | curve | smooth | symmetric | cusp",
                    "straight: the point loses its handles and its kind; a side to another straight point becomes a straight line, a side to a curve point stays a curve that runs straight into this point. curve (a straight point only; a curve point is left as it is): the point is stored as smooth with its two handles a quarter of the way from the previous vertex to the next one on either side of it (point -/+ (next - previous) / 4; at the end of an open line the missing neighbour is the point itself), as PE-DESIGN 11.50 places them, so both sides at it become curves and the outline bends at once. smooth: stored as smooth and the two handles are turned onto one line through the point, each keeping its length. symmetric: stored as symmetric, the handles on one line at their mean length. cusp: stored as a cusp, handles not moved. smooth, symmetric and cusp are refused on a straight point (make it a curve first); with one handle only (the end of an open line) the kind is stored and nothing moves. Afterwards object.setSegment moves the opposite handle of a smooth or symmetric point with the one it is given. PES 0110 files keep the kind (0x11 smooth, 0x15 symmetric, 0x19 cusp)",
                ),
            ],
            set_point_kind,
        ),
        spec(
            "object.splitAtPoint",
            "Split an outline at a vertex (Select Point's Split at Point). A closed outline is opened there: it becomes one open line that starts and ends at the point, and a region on an object left with no closed outline is removed, as PE-DESIGN removes it; the line sewing stays. An object sewn as a region only gets line sewing in its place, a running stitch in the region's thread, as object.regionToLine gives it (the answer's regionToLine says so; by extension: PE-DESIGN's own shapes always have a line). An open line is cut at an inner vertex into two objects that both keep the point: the first keeps the id, the second gets a new id right after it in the sewing order with the same attributes. Refused at the first and last point of an open line. Hole sewing is cancelled on the object's pair, whether it is the outer or the inner shape (holeSewingOff in the answer; no piece keeps it), and text fitted to its outline is taken off it and stays where it was laid out (textOff, the number of texts), as PE-DESIGN's manual states a split does. One undo step",
            Effect::Edit,
            vec![
                req("id", "int", "Shape object"),
                CONTOUR,
                req("index", "int", "Vertex index"),
            ],
            split_at_point,
        ),
        spec(
            "object.movePoints",
            "Move several vertices of one outline by the same offset, as dragging one of several selected points or pressing an arrow key with the Point tool does (PE-DESIGN's arrow keys move the selected points 0.5 mm). Each point's own curve handles move with it; a straight point beside a curve point has none. On an object of a hole-sewing pair (the outer shape or the inner one) a move that would take an outline across the other object's outline is refused and nothing moves: cancel hole sewing first (object.setHoleSewing {ids: [id], enabled: false}), as PE-DESIGN asks. One undo step. One outline of one object a call: for points on several outlines or objects send one call per outline (the editor's Point tool does, with all of them in one undo step)",
            Effect::Edit,
            vec![
                req("id", "int", "Shape object"),
                CONTOUR,
                opt("index", "int", "Vertex index (or 'indexes')"),
                INDEXES,
                req(
                    "dx",
                    "mm",
                    "Offset to the right; the stored positions are the old ones plus this",
                ),
                req("dy", "mm", "Offset downwards"),
            ],
            move_points,
        ),
        spec(
            "object.openOutline",
            "Open a closed outline (the Point tool's object menu, Open): its closing side, from the last vertex to the first, is removed and it becomes an open line through the same points. A region on an object left with no closed outline is removed, as PE-DESIGN removes it (the answer's regionOff says so); the line sewing stays. An object sewn as a region only gets line sewing in its place, a running stitch in the region's thread, as object.regionToLine gives it (regionToLine in the answer). Hole sewing is cancelled on the object's pair, whether it is the outer or the inner shape (holeSewingOff), and text fitted to its outline is taken off it (textOff, the number of texts), as with object.splitAtPoint. Refused on an open outline. One undo step",
            Effect::Edit,
            vec![req("id", "int", "Shape object"), CONTOUR],
            open_outline,
        ),
        spec(
            "object.closeOutline",
            "Close an open line (the Point tool's object menu, Close): a straight closing side joins its last vertex to its first. An object without region sewing gets one, as PE-DESIGN gives it: Fill Stitch at the default direction of 45 degrees in the line's thread (the answer's regionAdded says so); 'region: false' leaves it a closed line only. Ends that lie on each other (within 0.05 mm) become one point. Refused on a closed outline. One undo step",
            Effect::Edit,
            vec![
                req("id", "int", "Shape object"),
                CONTOUR,
                opt(
                    "region",
                    "bool",
                    "false: do not add region sewing (default true: added as Fill Stitch at 45 degrees when the object has none)",
                ),
            ],
            close_outline,
        ),
    ]
}

fn set_point_kind(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let kind = p.string("kind", None)?;
    let c = contour_mut(s, p)?;
    let at = point_indexes(p, c)?;
    let stored = match kind.as_str() {
        "straight" | "curve" => None,
        "smooth" => Some(PointKind::Smooth),
        "symmetric" => Some(PointKind::Symmetric),
        "cusp" => Some(PointKind::Cusp),
        _ => {
            return Err(p.invalid("kind", "must be straight, curve, smooth, symmetric or cusp"));
        }
    };
    if stored.is_some()
        && let Some(&i) = at.iter().find(|&&i| !c.is_curve_point(i))
    {
        return Err(p.failed(format!(
            "point {i} is a straight point; make it a curve first (kind: curve)"
        )));
    }
    let mut changed = 0;
    for &i in &at {
        let before = (c.curves.clone(), c.kind(i));
        match stored {
            Some(k) => c.apply_kind(i, k),
            None if kind == "straight" => c.make_straight(i),
            None => {
                c.make_curve(i);
            }
        }
        if before != (c.curves.clone(), c.kind(i)) {
            changed += 1;
        }
    }
    Ok(json!({ "changed": changed }))
}

fn move_points(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let dx = p.req("dx", -5000.0, 5000.0)? * crate::commands::MM;
    let dy = p.req("dy", -5000.0, 5000.0)? * crate::commands::MM;
    let before = hole_pair_before(s, p);
    let c = contour_mut(s, p)?;
    let at = point_indexes(p, c)?;
    for &i in &at {
        let to = c.points[i] + openstitch_design::v2(dx, dy);
        c.move_point(i, to);
    }
    refuse_hole_crossing(s, p, before.as_ref())?;
    Ok(json!({ "moved": at.len() }))
}

/// The design before a point move of object `id`, kept only when the object is in a
/// hole-sewing pair (there is nothing to check otherwise).
pub(crate) fn hole_pair_before(s: &EngineState, p: &Params) -> Option<openstitch_design::Design> {
    let id = p.int("id", None, 0, i64::MAX).ok()? as u64;
    openstitch_design::holes::partner(&s.design, id).map(|_| s.design.clone())
}

/// Refuses a point move that took object `id`'s outline across the outline of its
/// hole-sewing partner (#1271): `before` is the design before the move
/// ([`hole_pair_before`]).
pub(crate) fn refuse_hole_crossing(
    s: &EngineState,
    p: &Params,
    before: Option<&openstitch_design::Design>,
) -> Result<(), CommandError> {
    let Some(before) = before else {
        return Ok(());
    };
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let Some(i) = s.design.index_of(id) else {
        return Ok(());
    };
    if openstitch_design::holes::moves_across_partner(before, id, &s.design.objects[i].contours) {
        let other = openstitch_design::holes::partner(before, id).map_or(0, |o| o.id);
        return Err(p.failed(format!(
            "object {id} has hole sewing with object {other}: a point cannot be moved across the other outline. Cancel hole sewing first (object.setHoleSewing {{ids: [{id}], enabled: false}})"
        )));
    }
    Ok(())
}

/// After object `i`'s outline was split or opened (#1271, PE-DESIGN's manual): hole sewing is
/// cancelled on its pair (the flags of the outer object, whether `i` is the outer or the
/// inner one), and text fitted to its outline comes off it, as the Knife does. Returns
/// (hole sewing cancelled, texts taken off).
fn cancel_hole_and_text(s: &mut EngineState, i: usize) -> (bool, usize) {
    let id = s.design.objects[i].id;
    let mut hole_off = false;
    let outer: Vec<usize> = s
        .design
        .objects
        .iter()
        .enumerate()
        .filter(|(_, o)| {
            o.hole_sewing
                && (o.id == id
                    || openstitch_design::holes::paired_inner(o, &s.design)
                        .is_some_and(|inner| inner.id == id))
        })
        .map(|(k, _)| k)
        .collect();
    for k in outer {
        s.design.objects[k].hole_sewing = false;
        s.design.objects[k].hole_inner = None;
        hole_off = true;
    }
    let mut texts = 0;
    for t in 0..s.design.objects.len() {
        if s.design.objects[t].text.as_ref().and_then(|x| x.path) == Some(id) {
            s.design.detach_text(t);
            texts += 1;
        }
    }
    (hole_off, texts)
}

/// After an outline of object `i` was opened: with no closed outline left its region is
/// removed, and an object that was sewn as a region only is sewn as a line instead, a running
/// stitch in the region's thread (what `object.regionToLine` does). Returns (region removed,
/// turned into line sewing).
fn region_after_opening(s: &mut EngineState, i: usize) -> (bool, bool) {
    let o = &mut s.design.objects[i];
    if o.contours.iter().any(|c| c.closed) {
        return (false, false);
    }
    let Some(region) = o.region.take() else {
        return (false, false);
    };
    let to_line = o.line.is_none();
    if to_line {
        o.line = Some(openstitch_design::LineSewing::new(
            openstitch_design::LineType::Running,
            region.thread,
        ));
    }
    (true, to_line)
}

/// The object and contour indexes `id` / `contour` name (after [`contour_mut`] checked them).
fn place(s: &mut EngineState, p: &Params) -> Result<(usize, usize), CommandError> {
    contour_mut(s, p)?;
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    Ok((i, p.int("contour", Some(0), 0, i64::MAX)? as usize))
}

fn open_outline(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let (i, ci) = place(s, p)?;
    let o = &s.design.objects[i];
    let id = o.id;
    if !o.contours[ci].closed {
        return Err(p.failed(format!("object {id}'s outline is open already")));
    }
    s.design.objects[i].contours[ci].open_up();
    let (hole_off, texts) = cancel_hole_and_text(s, i);
    let (region_off, to_line) = region_after_opening(s, i);
    Ok(
        json!({ "id": id, "regionOff": region_off, "regionToLine": to_line, "holeSewingOff": hole_off, "textOff": texts }),
    )
}

fn close_outline(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let with_region = p.bool("region", Some(true))?;
    let (i, ci) = place(s, p)?;
    let o = &mut s.design.objects[i];
    let id = o.id;
    let c = &mut o.contours[ci];
    if c.closed {
        return Err(p.failed(format!("object {id}'s outline is closed already")));
    }
    // Ends dropped on each other are one point (0.5 units = 0.05 mm).
    let n = c.points.len();
    if n > 2 && c.points[0].distance(c.points[n - 1]) <= 0.5 {
        c.delete_point(n - 1);
    }
    if !c.close_up() {
        return Err(p.failed("an outline needs two points to be closed"));
    }
    let mut region_added = false;
    if with_region && o.region.is_none() {
        let thread = o
            .line
            .as_ref()
            .map_or_else(crate::commands::default_fill, |l| l.thread.clone());
        o.region = Some(openstitch_design::RegionSewing::new(
            openstitch_design::RegionType::Fill,
            thread,
        ));
        region_added = true;
    }
    Ok(json!({ "id": id, "regionAdded": region_added }))
}

fn split_at_point(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let (ci, index, pieces) = {
        let c = contour_mut(s, p)?;
        let index = p.int("index", None, 0, c.points.len() as i64 - 1)? as usize;
        let pieces = c
            .split_at(index)
            .ok_or_else(|| p.failed("an open line cannot be split at its first or last point"))?;
        let n = p.int("contour", Some(0), 0, i64::MAX)? as usize;
        (n, index, pieces)
    };
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    let mut pieces = pieces.into_iter();
    let first = pieces.next().expect("a split makes at least one piece");
    match pieces.next() {
        // A closed outline opened at the point.
        None => {
            s.design.objects[i].contours[ci] = first;
            let (hole_off, texts) = cancel_hole_and_text(s, i);
            let (region_off, to_line) = region_after_opening(s, i);
            let _ = index;
            Ok(
                json!({ "ids": [id], "opened": true, "regionOff": region_off, "regionToLine": to_line, "holeSewingOff": hole_off, "textOff": texts }),
            )
        }
        // An open line cut in two.
        Some(second) => {
            debug_assert_eq!(s.design.objects[i].kind, ObjectKind::Shape);
            // Before the second piece is copied, so that neither piece keeps the flags.
            let (hole_off, texts) = cancel_hole_and_text(s, i);
            let mut tail = s.design.objects[i].clone();
            tail.contours = vec![second];
            tail.id = s.design.next_object_id;
            s.design.next_object_id += 1;
            let new_id = tail.id;
            s.design.objects[i].contours[ci] = first;
            s.design.objects.insert(i + 1, tail);
            Ok(
                json!({ "ids": [id, new_id], "opened": false, "regionOff": false, "regionToLine": false, "holeSewingOff": hole_off, "textOff": texts }),
            )
        }
    }
}
