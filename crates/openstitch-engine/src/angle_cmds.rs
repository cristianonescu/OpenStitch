//! Stitch angle lines (#534, beyond PE-DESIGN): `object.setAngleLines` sets the lines across a
//! fill that its rows turn between (see `openstitch_design::angle_lines`).

use crate::commands::{CommandSpec, Effect, MM, req, spec};
use crate::params::{CommandError, Params};
use openstitch_design::{RegionType, Vec2, v2};
use serde_json::{Value, json};

/// Most angle lines on one fill.
pub const MAX_LINES: usize = 64;

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "object.setAngleLines",
            "Stitch angle lines (beyond PE-DESIGN): lines across a fill (2 or more, in order along the shape) whose directions its rows follow, turning smoothly from one line to the next, so a U, Z or ring sews in one object with no seam. An empty list removes them. Lines that cross inside the shape, or whose middle is outside it, are refused",
            Effect::Edit,
            vec![
                req("id", "int", "A shape whose region is a fill"),
                req(
                    "lines",
                    "[[[x, y], [x, y]], …] mm",
                    "The lines, each from one side of the shape to the other (they are extended to the outline), in order along it; [] removes them",
                ),
                crate::commands::opt(
                    "rule",
                    "sides | fan",
                    "How the rows turn between two lines, stored with the lines. sides (OpenStitch's, the default on a shape without lines): a row joins the points at the same fraction along the two outline sides. fan (PE-DESIGN's direction lines): the rows are a fan through the point where the two lines meet when extended, a row spacing apart at their far ends; parallel lines give parallel rows. Not given: the shape's rule is kept. A rule other than the one the shape's lines have is refused (lines are not converted from one rule to the other: remove them first with lines [])",
                ),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let lines = lines(p)?;
                let o = &s.design.objects[i];
                let Some(r) = o.region.as_ref().filter(|r| r.kind == RegionType::Fill) else {
                    return Err(p.failed(format!("object {id} is not a fill")));
                };
                if r.gradient.is_some() && !lines.is_empty() {
                    return Err(p.failed(format!(
                        "object {id} is part of a gradient; angle lines are for plain fills"
                    )));
                }
                if !lines.is_empty() {
                    let closed: Vec<_> = o.contours.iter().filter(|c| c.closed).cloned().collect();
                    openstitch_design::angle_lines::check(&lines, &closed, &[])
                        .map_err(|why| p.invalid("lines", why))?;
                }
                // One shape has one rule: lines of the other rule are not converted.
                if let Some(asked) = p.get("rule").and_then(Value::as_str)
                    && r.angle_lines.len() >= 2
                    && !lines.is_empty()
                    && asked != r.angle_rule.key()
                    && matches!(asked, "sides" | "fan")
                {
                    return Err(p.invalid(
                    "rule",
                    format!(
                        "object {id} has lines with the rule {}; they are not converted: remove them first (lines [])",
                        r.angle_rule.key()
                    ),
                ));
                }
                // The rule the lines are sewn by: kept when not given, and OpenStitch's on a
                // shape without lines.
                let rule = match p.get("rule").and_then(Value::as_str) {
                    None if r.angle_lines.is_empty() => openstitch_design::AngleRule::Sides,
                    None => r.angle_rule,
                    Some("sides") => openstitch_design::AngleRule::Sides,
                    Some("fan") => openstitch_design::AngleRule::Fan,
                    Some(_) => return Err(p.invalid("rule", "must be sides or fan")),
                };
                let n = lines.len();
                if let Some(r) = s.design.objects[i].region.as_mut() {
                    r.angle_rule = if lines.is_empty() {
                        openstitch_design::AngleRule::Sides
                    } else {
                        rule
                    };
                    r.angle_lines = lines;
                }
                Ok(json!({ "id": id, "lines": n, "rule": rule.key() }))
            },
        ),
        spec(
            "object.addDirectionLine",
            "Add Direction Line, as PE-DESIGN's Select Point tool adds one with two clicks: a straight line across a filled shape, from a point outside it to a point outside it on another side. A shape's fill angle is its first direction line; with two or more lines the fill's rows are parallel to the first line before it and to the last line after it, and between two lines they are a fan through the point where the two lines meet when extended, a row spacing apart at their far ends (PE-DESIGN's rule; parallel lines give parallel rows). The lines are stored as the shape's angle lines with the rule fan (object.setAngleLines shows and sets them), in order across the shape: the new line is put where it lies among the others. Refused, changing nothing: when an end lies inside the shape; when the line does not cross the shape; when it would cross another direction line inside the shape; on a shape with hole sewing; on a shape whose angle lines have the rule sides (OpenStitch's own, from the Angle Lines tool: remove them first with object.setAngleLines lines [], they are not converted); on a shape with a curved direction or Manual Punch direction lines; on a region that is not a fill. PES 0110 files keep the lines. One undo step",
            Effect::Edit,
            vec![
                req("id", "int", "A shape whose region is a fill"),
                req(
                    "line",
                    "[[x, y], [x, y]] mm",
                    "The line's two ends, both outside the shape: the first is its start (drawn with a square), the second its end (the arrow head). Stored along the same line from 5 mm before its first crossing of the outline to 5 mm after its last one, wherever the two ends were put",
                ),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let new = one_line(p)?;
                let o = &s.design.objects[i];
                let fan = fan_lines_of(p, &s.design, i)?;
                let closed: Vec<_> = o.contours.iter().filter(|c| c.closed).cloned().collect();
                for (end, what) in [(new[0], "first"), (new[1], "second")] {
                    if openstitch_design::geometry::contains(end, &closed) {
                        return Err(p.invalid(
                        "line",
                        format!("its {what} end lies inside the shape; both ends of a direction line are outside it"),
                    ));
                    }
                }
                // Stored across the shape, 5 mm beyond the outline at both ends, wherever
                // the two ends were put.
                let new = openstitch_design::angle_lines::across(new, &closed)
                    .ok_or_else(|| p.invalid("line", "the line does not cross the shape"))?;
                let mut lines = fan;
                if lines.len() >= MAX_LINES {
                    return Err(p.failed(format!("at most {MAX_LINES} direction lines")));
                }
                let at = openstitch_design::angle_lines::place_for(&lines, new);
                lines.insert(at, new);
                openstitch_design::angle_lines::check(&lines, &closed, &[]).map_err(|why| {
                    p.invalid("line", why.replace("angle line", "direction line"))
                })?;
                let n = lines.len();
                if let Some(r) = s.design.objects[i].region.as_mut() {
                    r.angle_lines = lines;
                    r.angle_rule = openstitch_design::AngleRule::Fan;
                    r.auto_direction = false;
                }
                Ok(json!({ "id": id, "lines": n, "index": at }))
            },
        ),
        spec(
            "object.deleteDirectionLine",
            "Delete one direction line of a filled shape (PE-DESIGN's Delete Selected Line, and the Delete key with a line selected), or with 'keep' every line but that one (Delete Lines Not Selected). The lines are the shape's angle lines with the rule fan, numbered from 0 in order across the shape (object.addDirectionLine). When one line is left it becomes the shape's fill angle again (the direction it points in) and no line is stored. A shape's last line, its fill angle, cannot be deleted: refused. One undo step",
            Effect::Edit,
            vec![
                req("id", "int", "A shape with two or more direction lines"),
                crate::commands::opt("index", "int", "The line to delete (from 0)"),
                crate::commands::opt(
                    "keep",
                    "int",
                    "Instead of 'index': the one line to keep; all the others are deleted",
                ),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let o = &s.design.objects[i];
                let fan = o.region.as_ref().is_some_and(|r| {
                    r.angle_rule == openstitch_design::AngleRule::Fan && r.angle_lines.len() >= 2
                });
                if !fan {
                    return Err(p.failed(format!(
                    "object {id} has one direction line, its fill angle: the last line cannot be deleted"
                )));
                }
                let mut lines = o
                    .region
                    .as_ref()
                    .map(|r| r.angle_lines.clone())
                    .unwrap_or_default();
                let last = lines.len() as i64 - 1;
                if p.has("keep") {
                    let k = p.int("keep", None, 0, last)? as usize;
                    lines = vec![lines[k]];
                } else {
                    let k = p.int("index", None, 0, last)? as usize;
                    lines.remove(k);
                }
                let n = lines.len();
                if let Some(r) = s.design.objects[i].region.as_mut() {
                    if let [only] = lines[..] {
                        // One line left: it is the fill angle again.
                        r.angle = openstitch_design::angle_lines::line_angle(only);
                        r.angle_lines.clear();
                        r.angle_rule = openstitch_design::AngleRule::Sides;
                    } else {
                        r.angle_lines = lines;
                    }
                }
                Ok(json!({ "id": id, "lines": n }))
            },
        ),
    ]
}

/// One line, `[[x, y], [x, y]]` mm, in 0.1 mm.
fn one_line(p: &Params) -> Result<[Vec2; 2], CommandError> {
    let bad = || p.invalid("line", "must be [[x, y], [x, y]] in mm");
    let ends = p
        .get("line")
        .and_then(Value::as_array)
        .filter(|e| e.len() == 2)
        .ok_or_else(bad)?;
    let point = |v: &Value| -> Option<Vec2> {
        let a = v.as_array().filter(|a| a.len() == 2)?;
        let (x, y) = (a[0].as_f64()?, a[1].as_f64()?);
        (x.is_finite() && y.is_finite() && x.abs() <= 5000.0 && y.abs() <= 5000.0)
            .then(|| v2(x * MM, y * MM))
    };
    let line = [
        point(&ends[0]).ok_or_else(bad)?,
        point(&ends[1]).ok_or_else(bad)?,
    ];
    if line[0].distance(line[1]) < 1.0 {
        return Err(p.invalid("line", "its two ends are the same point"));
    }
    Ok(line)
}

/// The direction lines object `i` has for Add Direction Line to add to: its stored fan lines,
/// or the one line its fill angle is. An error says why the shape cannot take a line.
fn fan_lines_of(
    p: &Params,
    d: &openstitch_design::Design,
    i: usize,
) -> Result<Vec<[Vec2; 2]>, CommandError> {
    let o = &d.objects[i];
    let id = o.id;
    let Some(r) = o.region.as_ref().filter(|r| r.kind == RegionType::Fill) else {
        return Err(p.failed(format!(
            "object {id} is not a fill: direction lines are added to a fill"
        )));
    };
    if o.hole_sewing || openstitch_design::holes::partner(d, id).is_some() {
        return Err(p.failed(format!(
            "object {id} has hole sewing: a direction line cannot be added; cancel hole sewing first (object.setHoleSewing {{ids: [{id}], enabled: false}})"
        )));
    }
    if r.gradient.is_some() {
        return Err(p.failed(format!(
            "object {id} is part of a gradient; direction lines are for plain fills"
        )));
    }
    if r.direction_curve.is_some() || r.direction_lines.len() >= 2 {
        return Err(p.failed(format!(
            "object {id} has a curved direction or Manual Punch direction lines; remove them first (object.setDirectionCurve points null, object.setDirectionLines lines null)"
        )));
    }
    if r.angle_lines.len() >= 2 {
        if r.angle_rule != openstitch_design::AngleRule::Fan {
            return Err(p.failed(format!(
                "object {id} has OpenStitch angle lines (rule sides); they are not converted: remove them first (object.setAngleLines with lines [])"
            )));
        }
        return Ok(r.angle_lines.clone());
    }
    let closed: Vec<_> = o.contours.iter().filter(|c| c.closed).cloned().collect();
    let angle = if r.auto_direction { 45.0 } else { r.angle };
    openstitch_design::angle_lines::angle_line(&closed, angle)
        .map(|l| vec![l])
        .ok_or_else(|| p.failed(format!("object {id} has no closed outline")))
}

/// The `lines` list, in 0.1 mm.
fn lines(p: &Params) -> Result<Vec<[Vec2; 2]>, CommandError> {
    let bad = || p.invalid("lines", "must be a list of [[x, y], [x, y]] in mm");
    let list = p.get("lines").and_then(Value::as_array).ok_or_else(bad)?;
    if list.len() > MAX_LINES {
        return Err(p.invalid("lines", format!("at most {MAX_LINES} lines")));
    }
    let point = |v: &Value| -> Option<Vec2> {
        let a = v.as_array().filter(|a| a.len() == 2)?;
        let (x, y) = (a[0].as_f64()?, a[1].as_f64()?);
        (x.is_finite() && y.is_finite() && x.abs() <= 5000.0 && y.abs() <= 5000.0)
            .then(|| v2(x * MM, y * MM))
    };
    list.iter()
        .map(|l| {
            let ends = l.as_array().filter(|e| e.len() == 2).ok_or_else(bad)?;
            Ok([
                point(&ends[0]).ok_or_else(bad)?,
                point(&ends[1]).ok_or_else(bad)?,
            ])
        })
        .collect()
}
