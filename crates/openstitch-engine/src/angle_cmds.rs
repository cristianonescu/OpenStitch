//! Stitch angle lines (#534, beyond PE-DESIGN): `object.setAngleLines` sets the lines across a
//! fill that its rows turn between (see `openstitch_design::angle_lines`).

use crate::commands::{CommandSpec, Effect, MM, req, spec};
use crate::params::{CommandError, Params};
use openstitch_design::{RegionType, Vec2, v2};
use serde_json::{Value, json};

/// Most angle lines on one fill.
pub const MAX_LINES: usize = 64;

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
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
            let n = lines.len();
            if let Some(r) = s.design.objects[i].region.as_mut() {
                r.angle_lines = lines;
            }
            Ok(json!({ "id": id, "lines": n }))
        },
    )]
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
