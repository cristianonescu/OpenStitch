//! Shape builder and line gaps (#497, beyond PE-DESIGN, part 2). `shape.build` cuts the
//! overlapping shapes into the pieces of their arrangement and merges (or removes) the pieces a
//! drag crosses; `object.setLineGap` leaves part of a closed line unsewn, the turning gap of
//! in-the-hoop pieces. Geometry in `openstitch_design::{builder, model::LineGap}`.

use crate::commands::{CommandSpec, Effect, IDS, indexes, opt, req, spec};
use crate::session::EngineState;
use openstitch_design::builder::{self, Build};
use openstitch_design::{Contour, DesignObject, LineGap};
use serde_json::{Value, json};

/// Most points a drag may have.
const MAX_POINTS: usize = 10_000;

fn piece(o: &DesignObject, r: Vec<Contour>) -> DesignObject {
    let mut p = o.clone();
    p.contours = r;
    p.name = None;
    if let Some(reg) = &mut p.region {
        reg.direction_lines.clear();
        reg.skeleton = None;
    }
    crate::shape_ops::keep_points_on_outline(&mut p);
    p
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "shape.build",
            "Shape builder (beyond PE-DESIGN): the shapes are cut into the pieces of their overlaps; the pieces a drag path crosses are merged into one (sewn like the topmost of them) or, with mode remove, deleted",
            Effect::Edit,
            vec![
                req("ids", "[int]", "Closed shapes (2 or more)"),
                req("path", "[[x, y]] mm", "The drag, 1 or more points"),
                opt(
                    "mode",
                    "merge|remove",
                    "What happens to the crossed pieces (default merge)",
                ),
            ],
            |s: &mut EngineState, p| {
                let mode = match p.string("mode", Some("merge"))?.as_str() {
                    "merge" => Build::Merge,
                    "remove" => Build::Remove,
                    _ => return Err(p.invalid("mode", "must be merge or remove")),
                };
                let path = p.points("path", 1)?;
                if path.len() > MAX_POINTS {
                    return Err(p.invalid("path", format!("at most {MAX_POINTS} points")));
                }
                let mut idx = indexes(p, &s.design)?;
                idx.sort_unstable();
                idx.dedup();
                if idx.len() < 2 {
                    return Err(p.invalid("ids", "needs at least 2 different objects"));
                }
                for &i in &idx {
                    crate::shape_ops::closed_shape(&s.design, p, i)?;
                    if s.design.objects[i].hole_sewing {
                        return Err(p.failed("shapes with hole sewing cannot be built from"));
                    }
                }
                let shapes: Vec<&[Contour]> = idx
                    .iter()
                    .map(|&i| &s.design.objects[i].contours[..])
                    .collect();
                let pieces = builder::build(&shapes, &path, mode)
                    .ok_or_else(|| p.failed("the drag crosses no piece of the shapes"))?;
                let mut out: Vec<(usize, DesignObject)> = pieces
                    .into_iter()
                    .map(|(owner, r)| (owner, piece(&s.design.objects[idx[owner]], r)))
                    .collect();
                // Pieces in their owners' sewing order, in the first shape's place.
                out.sort_by_key(|(owner, _)| *owner);
                let at = idx[0];
                crate::shape_ops::remove(&mut s.design, &idx);
                let mut ids = Vec::new();
                for (k, (_, mut o)) in out.into_iter().enumerate() {
                    o.id = s.design.next_object_id;
                    s.design.next_object_id += 1;
                    ids.push(o.id);
                    s.design
                        .objects
                        .insert((at + k).min(s.design.objects.len()), o);
                }
                Ok(json!({ "ids": ids }))
            },
        ),
        spec(
            "object.setLineGap",
            "Leave part of a closed line unsewn (beyond PE-DESIGN): the turning gap of in-the-hoop pieces, tied in and locked at both ends. Running, triple, zigzag, E and V lines; the first closed outline",
            Effect::Edit,
            vec![
                IDS,
                opt(
                    "offset",
                    "mm",
                    "Where the gap starts, along the outline from its first point (default 0)",
                ),
                req("length", "mm | null", "Gap length; null removes the gap"),
            ],
            |s, p| {
                let gap = if p.is_null("length") {
                    None
                } else {
                    Some(LineGap {
                        offset: p.opt("offset", 0.0, 0.0, 100_000.0)?,
                        length: p.req("length", 0.1, 10_000.0)?,
                    })
                };
                let idx = indexes(p, &s.design)?;
                for &i in &idx {
                    let o = &s.design.objects[i];
                    let Some(l) = &o.line else {
                        return Err(p.failed(format!("object {} has no line sewing", o.id)));
                    };
                    if let Some(g) = gap {
                        if !LineGap::applies_to(l.kind) {
                            return Err(p.failed(format!(
                                "object {}: a gap works with running, triple, zigzag, E and V lines",
                                o.id
                            )));
                        }
                        let Some(c) = o.contours.iter().find(|c| c.closed) else {
                            return Err(p.failed(format!("object {} has no closed outline", o.id)));
                        };
                        if g.open(c).is_none() {
                            return Err(p.invalid(
                                "length",
                                format!("object {}: the gap must be at least 1 mm shorter than the outline", o.id),
                            ));
                        }
                    }
                }
                for i in idx {
                    if let Some(l) = &mut s.design.objects[i].line {
                        l.gap = gap;
                    }
                }
                Ok(Value::Null)
            },
        ),
    ]
}
