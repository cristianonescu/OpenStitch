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
            "Change what kind of point a vertex is, as the Select Point tool's right-click menu does (To Straight, To Curve, To Smooth, To Symmetry, To Cusp). The kind belongs to the point, as in PE-DESIGN: a straight point has no handles, a curve point has two and is smooth, symmetric or a cusp; a side is a curve when either of its ends is a curve point. Answers 'changed', the number of points changed. The stitches change only where the outline does",
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
            "Split an outline at a vertex (Select Point's Split at Point). A closed outline is opened there: it becomes one open line that starts and ends at the point, and a region on an object left with no closed outline is switched off (its line sewing stays; refused when the object has no line sewing). An open line is cut at an inner vertex into two objects that both keep the point: the first keeps the id, the second gets a new id right after it in the sewing order with the same attributes. Refused at the first and last point of an open line. One undo step",
            Effect::Edit,
            vec![
                req("id", "int", "Shape object"),
                CONTOUR,
                req("index", "int", "Vertex index"),
            ],
            split_at_point,
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
            let o = &s.design.objects[i];
            let still_closed = o
                .contours
                .iter()
                .enumerate()
                .any(|(k, c)| k != ci && c.closed);
            if !still_closed && o.region.is_some() && o.line.is_none() {
                return Err(p.failed(format!(
                    "object {id} is sewn as a region only; opening its outline at point {index} would leave nothing to sew (give it line sewing first)"
                )));
            }
            let o = &mut s.design.objects[i];
            o.contours[ci] = first;
            let mut region_off = false;
            if !still_closed && o.region.take().is_some() {
                region_off = true;
            }
            Ok(json!({ "ids": [id], "opened": true, "regionOff": region_off }))
        }
        // An open line cut in two.
        Some(second) => {
            debug_assert_eq!(s.design.objects[i].kind, ObjectKind::Shape);
            let mut tail = s.design.objects[i].clone();
            tail.contours = vec![second];
            tail.id = s.design.next_object_id;
            s.design.next_object_id += 1;
            let new_id = tail.id;
            s.design.objects[i].contours[ci] = first;
            s.design.objects.insert(i + 1, tail);
            Ok(json!({ "ids": [id, new_id], "opened": false, "regionOff": false }))
        }
    }
}
