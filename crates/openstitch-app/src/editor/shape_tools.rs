//! Requests of the shape tools beyond PE-DESIGN (#477, #478, #479, #497): Smooth Shape and
//! Smooth Stitches. Each is one engine command, so one Undo.

use super::request::CommandRequest;
use openstitch_design::{Design, ObjectKind};
use serde_json::json;
use std::collections::BTreeSet;

/// Smooth Shape's default tolerance, mm.
pub const SMOOTH_TOLERANCE: f64 = openstitch_design::simplify::DEFAULT_TOLERANCE_MM;

/// The selected shape objects (Smooth Shape works on shapes only).
pub fn shapes(design: &Design, selection: &BTreeSet<u64>) -> BTreeSet<u64> {
    selection
        .iter()
        .copied()
        .filter(|&id| {
            design
                .index_of(id)
                .is_some_and(|i| design.objects[i].kind == ObjectKind::Shape)
        })
        .collect()
}

/// `shape.simplify` of `ids` at `tolerance` mm.
pub fn simplify(ids: &BTreeSet<u64>, tolerance: f64) -> CommandRequest {
    CommandRequest::new(
        "shape.simplify",
        json!({ "ids": CommandRequest::ids(ids), "tolerance": (tolerance.clamp(0.01, 10.0) * 100.0).round() / 100.0 }),
    )
}

/// `stitches.smooth` of stitch pattern `id`: the span of the selected needle points when two
/// or more are selected, else the whole pattern.
pub fn smooth_stitches(id: u64, needles: &[usize]) -> CommandRequest {
    let mut params = json!({ "id": id });
    if let (Some(&from), Some(&to)) = (needles.iter().min(), needles.iter().max())
        && to > from
    {
        params["from"] = json!(from);
        params["to"] = json!(to);
    }
    CommandRequest::new("stitches.smooth", params)
}
