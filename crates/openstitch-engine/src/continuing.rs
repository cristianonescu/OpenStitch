//! Continuing objects (#479): `object.extendPath` / `object.extendColumn` add points or
//! direction lines to either end of an open line, a satin skeleton or a Manual Punch column,
//! keeping it one object. Geometry lives in `openstitch_design::extend`.

use crate::commands::{CommandSpec, Effect, opt, req, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_design::extend::{self, End, same_outline};
use openstitch_design::skeleton::{self, Skeleton};
use openstitch_design::{Bezier, Contour, DesignObject, ObjectKind, RegionSewing, Vec2, column};
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

/// The open-line, skeleton or column object `id`.
fn shape_index(s: &EngineState, p: &Params) -> Result<usize, CommandError> {
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    match s.design.objects[i].kind {
        ObjectKind::Shape => Ok(i),
        ObjectKind::Text => Err(p.failed(format!(
            "object {id} is text; convert it to shapes first (text.toShapes)"
        ))),
        ObjectKind::Stitches => Err(p.failed(format!("object {id} is a stitch pattern"))),
    }
}

fn end_param(p: &Params) -> Result<End, CommandError> {
    End::from_id(&p.string("end", None)?).ok_or_else(|| p.invalid("end", "must be start or end"))
}

/// `lines`: one or more `[[x1, y1], [x2, y2]]` in mm.
fn lines_param(p: &Params) -> Result<Vec<[Vec2; 2]>, CommandError> {
    let arr = p.array("lines")?;
    if arr.is_empty() || arr.len() > crate::punch::MAX_LINES {
        return Err(p.invalid(
            "lines",
            format!("needs 1...{} lines", crate::punch::MAX_LINES),
        ));
    }
    arr.iter()
        .map(|v| match v.as_array() {
            Some(a) if a.len() == 2 => Ok([
                p.point_value("lines", &a[0])?,
                p.point_value("lines", &a[1])?,
            ]),
            _ => Err(p.invalid("lines", "each line must be [[x1, y1], [x2, y2]] in mm")),
        })
        .collect()
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "object.extendPath",
            "Continue an open line, path or satin skeleton from its start or end: the points become part of the same object (points added at the start are given outward from it)",
            Effect::Edit,
            vec![
                req(
                    "id",
                    "int",
                    "Open line or path (one open contour), or a satin skeleton",
                ),
                req("end", "start|end", "Which end to continue from"),
                req(
                    "points",
                    "[[x, y]] mm",
                    "New points in drawing order (1-10000)",
                ),
                opt(
                    "curves",
                    "[null | [[c1x, c1y], [c2x, c2y]]] mm",
                    "One entry per new segment (segment k runs from the previous point to point k); null = straight",
                ),
                opt(
                    "widths",
                    "[mm]",
                    "Satin skeleton: column width at each new point (default the end's width)",
                ),
            ],
            |s, p| {
                let i = shape_index(s, p)?;
                let end = end_param(p)?;
                let points = p.points("points", 1)?;
                if points.len() > skeleton::MAX_POINTS {
                    return Err(
                        p.invalid("points", format!("at most {} points", skeleton::MAX_POINTS))
                    );
                }
                let mut curves = Vec::new();
                if p.has("curves") {
                    let arr = p.array("curves")?;
                    if arr.len() != points.len() {
                        return Err(p.invalid(
                            "curves",
                            format!("needs one entry per new segment ({})", points.len()),
                        ));
                    }
                    for v in arr {
                        curves.push(
                            p.bezier_value("curves", v)?
                                .map(|(c1, c2)| Bezier { c1, c2 }),
                        );
                    }
                }
                let o = &mut s.design.objects[i];
                let id = o.id;
                if let Some(sk) = skeleton_satin(o) {
                    if !curves.is_empty() {
                        return Err(p.invalid(
                            "curves",
                            "a satin skeleton's centre line is straight between points",
                        ));
                    }
                    let at = match end {
                        End::Start => 0,
                        End::End => sk.widths.len() - 1,
                    };
                    let widths = if p.has("widths") {
                        let w = p.array("widths")?;
                        if w.len() != points.len() {
                            return Err(p.invalid("widths", "needs one width per new point"));
                        }
                        w.iter()
                            .map(|v| {
                                v.as_f64()
                                    .ok_or_else(|| p.invalid("widths", "must be numbers (mm)"))
                            })
                            .collect::<Result<Vec<_>, _>>()?
                    } else {
                        vec![sk.widths[at]; points.len()]
                    };
                    let grown = extend::skeleton(sk, end, &points, &widths);
                    if grown.validate().is_err() {
                        return Err(p.invalid(
                            "widths",
                            format!(
                                "widths must be 0...{} mm and the points finite",
                                skeleton::MAX_WIDTH
                            ),
                        ));
                    }
                    let outline = skeleton::outline(&grown)
                        .ok_or_else(|| p.invalid("points", "the centre line is too short"))?;
                    o.contours = vec![outline];
                    o.region.as_mut().expect("skeleton").skeleton = Some(grown);
                    return Ok(
                        json!({ "id": id, "points": o.region.as_ref().and_then(|r| r.skeleton.as_ref()).map_or(0, |k| k.points.len()) }),
                    );
                }
                if p.has("widths") {
                    return Err(p.invalid("widths", "only a satin skeleton has widths"));
                }
                if o.contours.len() != 1 {
                    return Err(p.failed(format!(
                        "object {id} has {} contours; only a single open line can be continued",
                        o.contours.len()
                    )));
                }
                let old = o.contours[0].clone();
                if old.closed {
                    return Err(p.failed(format!(
                        "object {id} is closed; only open lines can be continued"
                    )));
                }
                let grown = extend::path(&old, end, &points, &curves)
                    .ok_or_else(|| p.invalid("points", "cannot continue this line"))?;
                // The sewing keeps starting (or ending) at the end that moved.
                let (was, now) = match end {
                    End::Start => (old.points[0], grown.points[0]),
                    End::End => (
                        *old.points.last().expect("open"),
                        *grown.points.last().expect("open"),
                    ),
                };
                if let Some(l) = &mut o.line {
                    for slot in [&mut l.entry, &mut l.exit] {
                        if slot.is_some_and(|q| q.distance(was) <= 5.0) {
                            *slot = Some(now);
                        }
                    }
                }
                let n = grown.points.len();
                o.contours[0] = grown;
                Ok(json!({ "id": id, "points": n }))
            },
        ),
        spec(
            "object.extendColumn",
            "Continue a Manual Punch column from its first or last direction line (lines added at the start are given outward from it; validated as shape.column does)",
            Effect::Edit,
            vec![
                req("id", "int", "Manual Punch column (shape.column)"),
                req("end", "start|end", "Which end to continue from"),
                req(
                    "lines",
                    "[[[x1, y1], [x2, y2]]] mm",
                    "New direction lines in drawing order; first ends on the same side as the column's first ends",
                ),
            ],
            |s, p| {
                let i = shape_index(s, p)?;
                let end = end_param(p)?;
                let lines = lines_param(p)?;
                let o = &mut s.design.objects[i];
                let id = o.id;
                let Some(r) = punch_column(o) else {
                    return Err(p.failed(format!(
                        "object {id} is not a Manual Punch column (shape.column)"
                    )));
                };
                let curved = r.curved_lines;
                if r.direction_lines.len() + lines.len() > crate::punch::MAX_LINES {
                    return Err(p.invalid(
                        "lines",
                        format!("a column has at most {} lines", crate::punch::MAX_LINES),
                    ));
                }
                let grown =
                    extend::column(&r.direction_lines, end, &lines, curved).map_err(|e| {
                        if curved
                            && matches!(e, column::RungError::Crossing(_))
                            && extend::column(&r.direction_lines, end, &lines, false).is_ok()
                        {
                            p.invalid("lines", "the curved sides cross")
                        } else {
                            crate::punch::rung_error(p, e)
                        }
                    })?;
                o.contours = vec![column_outline(&grown, curved)];
                let n = grown.len();
                o.region.as_mut().expect("column").direction_lines = grown;
                Ok(json!({ "id": id, "lines": n }))
            },
        ),
    ]
}
