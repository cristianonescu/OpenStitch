//! `object.setRegion {flowField}` (#538, beyond PE-DESIGN): a fill's rows follow a direction
//! field (see `openstitch_design::flow`). The field is given, never taken from a picture.

use crate::commands::MM;
use crate::params::{CommandError, Params};
use openstitch_design::flow::FlowField;
use openstitch_design::{RegionSewing, RegionType, v2};
use serde_json::Value;

/// The `flowField` parameter's documentation.
pub(crate) const DOC: &str = "Fill: flow fill (beyond PE-DESIGN), rows that follow a direction field instead of one angle: radial or swirl (round the object's own centre), {kind: radial, center: [x, y] mm} (rows radiate from the centre), {kind: swirl, center: [x, y] mm} (rows circle it), or {kind: grid, origin: [x, y] mm, cell: mm, cols: n, angles: [degrees, row by row]} (the direction blends between cells), or grid (keeps a grid field already set); null or false: straight rows again. Not mapped in PES 0110";

/// Reads `flowField` into the region, if given; `centre` is the object's own centre (0.1 mm),
/// for `"radial"` and `"swirl"` given without one.
pub(crate) fn set_region(
    p: &Params,
    r: &mut RegionSewing,
    centre: Option<openstitch_design::geometry::Vec2>,
) -> Result<(), CommandError> {
    let Some(v) = p.get("flowField") else {
        return Ok(());
    };
    if v.is_null() || v == &Value::Bool(false) || v.as_str() == Some("straight") {
        r.flow_field = None;
        return Ok(());
    }
    if r.kind != RegionType::Fill {
        return Err(p.invalid("flowField", "is for fill regions"));
    }
    // `"grid"` keeps the region's own grid field: the inspector shows Field grid only then, and
    // choosing it again changes nothing.
    if v.as_str() == Some("grid") {
        return if matches!(r.flow_field, Some(FlowField::Grid { .. })) {
            Ok(())
        } else {
            Err(p.invalid(
                "flowField",
                "grid keeps a grid field already set; give {kind: grid, origin, cell, cols, angles}",
            ))
        };
    }
    if let Some(kind) = v.as_str() {
        let center = centre.ok_or_else(|| p.invalid("flowField", "the object has no outline"))?;
        r.flow_field = Some(match kind {
            "radial" => FlowField::Radial { center },
            "swirl" => FlowField::Swirl { center },
            _ => return Err(p.invalid("flowField", "must be radial, swirl, straight or a field")),
        });
        return Ok(());
    }
    let bad = |why: &str| p.invalid("flowField", why.to_string());
    let point = |k: &str| -> Result<openstitch_design::geometry::Vec2, CommandError> {
        match v.get(k).and_then(Value::as_array).map(Vec::as_slice) {
            Some([x, y]) => match (x.as_f64(), y.as_f64()) {
                (Some(x), Some(y)) if x.is_finite() && y.is_finite() => Ok(v2(x * MM, y * MM)),
                _ => Err(bad(&format!("{k} must be [x, y] in mm"))),
            },
            _ => Err(bad(&format!("{k} must be [x, y] in mm"))),
        }
    };
    let field = match v.get("kind").and_then(Value::as_str) {
        Some("radial") => FlowField::Radial {
            center: point("center")?,
        },
        Some("swirl") => FlowField::Swirl {
            center: point("center")?,
        },
        Some("grid") => {
            let cell = v
                .get("cell")
                .and_then(Value::as_f64)
                .filter(|c| (0.1..=100.0).contains(c))
                .ok_or_else(|| bad("cell must be 0.1-100 mm"))?;
            let cols = v
                .get("cols")
                .and_then(Value::as_u64)
                .filter(|c| (1..=1000).contains(c))
                .ok_or_else(|| bad("cols must be 1-1000"))?;
            let angles: Vec<f64> = v
                .get("angles")
                .and_then(Value::as_array)
                .ok_or_else(|| bad("angles must be an array of degrees"))?
                .iter()
                .map(|a| {
                    a.as_f64()
                        .ok_or_else(|| bad("every angle must be a number"))
                })
                .collect::<Result<_, _>>()?;
            FlowField::Grid {
                origin: point("origin")?,
                cell: cell * MM,
                cols: cols as usize,
                angles,
            }
        }
        _ => return Err(bad("kind must be radial, swirl or grid")),
    };
    field.check().map_err(|e| bad(&e))?;
    r.flow_field = Some(field);
    Ok(())
}
