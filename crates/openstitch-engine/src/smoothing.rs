//! Smooth Shape and Smooth Stitches (#478): `shape.simplify` takes nodes off outlines within a
//! tolerance, `stitches.smooth` evens out a run of needle points. Geometry lives in
//! `openstitch_design::{simplify, stitch_edit}`.

use crate::commands::{CommandSpec, Effect, IDS, MM, indexes, opt, req, spec};
use openstitch_design::simplify::same_outline;
use openstitch_design::skeleton::{self, Skeleton};
use openstitch_design::{Contour, DesignObject, ObjectKind, RegionSewing, Vec2, column};
use openstitch_design::{simplify, stitch_edit};
use serde_json::json;

/// Outlines this close (0.1 mm) count as derived from their direction lines or skeleton.
const SAME: f64 = 0.05;

/// The Manual Punch column `o` is, if its outline is the one its direction lines make.
fn punch_column(o: &DesignObject) -> Option<&RegionSewing> {
    let r = o.region.as_ref()?;
    if r.direction_lines.len() < 2 || o.contours.len() != 1 {
        return None;
    }
    let own = if r.curved_lines {
        column::curved_outline(&r.direction_lines)
    } else {
        column::outline(&r.direction_lines)
    };
    same_outline(&o.contours[0], &own, SAME).then_some(r)
}

/// The skeleton satin `o` is, if its outline is the one its skeleton makes.
fn skeleton_satin(o: &DesignObject) -> Option<&Skeleton> {
    let sk = o.region.as_ref()?.skeleton.as_ref()?;
    let own = skeleton::outline(sk)?;
    (o.contours.len() == 1 && same_outline(&o.contours[0], &own, SAME)).then_some(sk)
}

fn column_outline(lines: &[[Vec2; 2]], curved: bool) -> Contour {
    if curved {
        column::curved_outline(lines)
    } else {
        column::outline(lines)
    }
}

/// Nodes of `o` that Smooth Shape counts: direction lines, skeleton points or vertices.
fn nodes(o: &DesignObject) -> usize {
    if let Some(r) = punch_column(o) {
        return r.direction_lines.len();
    }
    if let Some(sk) = skeleton_satin(o) {
        return sk.points.len();
    }
    o.contours.iter().map(|c| c.points.len()).sum()
}

/// One pass of Smooth Shape on `o`.
fn simplify_object(o: &mut DesignObject, tolerance: f64) {
    if let Some(r) = punch_column(o) {
        let curved = r.curved_lines;
        if let Some(lines) = simplify::column(&r.direction_lines, curved, tolerance) {
            o.contours = vec![column_outline(&lines, curved)];
            o.region.as_mut().expect("column").direction_lines = lines;
        }
        return;
    }
    if let Some(sk) = skeleton_satin(o) {
        if let Some(out) = simplify::skeleton(sk, tolerance)
            && let Some(c) = skeleton::outline(&out)
        {
            o.contours = vec![c];
            o.region.as_mut().expect("skeleton").skeleton = Some(out);
        }
        return;
    }
    for c in &mut o.contours {
        *c = simplify::contour(c, tolerance);
    }
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "shape.simplify",
            "Smooth Shape: remove nodes that deviate less than a tolerance (curves refitted, corners and open ends kept); a Manual Punch column loses at most one direction line per pass, a satin skeleton its redundant centre-line points",
            Effect::Edit,
            vec![
                IDS,
                opt(
                    "tolerance",
                    "mm",
                    "How far the outline may move (0.01-10, default 0.2)",
                ),
            ],
            |s, p| {
                let tolerance =
                    p.opt("tolerance", simplify::DEFAULT_TOLERANCE_MM, 0.01, 10.0)? * MM;
                let idx = indexes(p, &s.design)?;
                for &i in &idx {
                    let o = &s.design.objects[i];
                    match o.kind {
                        ObjectKind::Shape => {}
                        ObjectKind::Text => {
                            return Err(p.failed(format!(
                                "object {} is text; convert it to shapes first (text.toShapes)",
                                o.id
                            )));
                        }
                        ObjectKind::Stitches => {
                            return Err(p.failed(format!(
                                "object {} is a stitch pattern; use stitches.smooth",
                                o.id
                            )));
                        }
                    }
                }
                let mut report = Vec::new();
                for i in idx {
                    let o = &mut s.design.objects[i];
                    let before = nodes(o);
                    simplify_object(o, tolerance);
                    report.push(json!({ "id": o.id, "before": before, "after": nodes(o) }));
                }
                Ok(json!({ "objects": report }))
            },
        ),
        spec(
            "stitches.smooth",
            "Smooth Stitches: move each needle point of a stitch pattern's range towards the midpoint of its neighbours (jumps, trims, colour changes and run ends stay; the number of points does not change)",
            Effect::Edit,
            vec![
                req("id", "int", "Stitch pattern object"),
                opt("from", "int", "First entry of the range (default 0)"),
                opt("to", "int", "Last entry of the range (default the last)"),
                opt("iterations", "int 1-100", "Passes (default 1)"),
                opt(
                    "strength",
                    "factor 0.01-1",
                    "How far each pass moves a point towards the midpoint (default 0.5)",
                ),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let o = &mut s.design.objects[i];
                let Some(b) = o
                    .stitch_block
                    .as_mut()
                    .filter(|_| o.kind == ObjectKind::Stitches)
                else {
                    return Err(p.invalid("id", format!("object {id} is not a stitch pattern")));
                };
                let last = b.stitches.len().saturating_sub(1) as i64;
                let from = p.int("from", Some(0), 0, last)? as usize;
                let to = p.int("to", Some(last), from as i64, last)? as usize;
                let iterations = p.int("iterations", Some(1), 1, 100)? as u32;
                let strength = p.opt("strength", 0.5, 0.01, 1.0)?;
                let moved = stitch_edit::smooth(b, from, to, iterations, strength)
                    .map_err(|e| p.failed(e.0))?;
                Ok(json!({ "moved": moved }))
            },
        ),
    ]
}
