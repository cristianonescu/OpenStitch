//! The background image on the canvas (#518, beyond PE-DESIGN): moving and scaling it with
//! handles as Modify Image does numerically (one `background.modify`, one undo step), and
//! where its layer splits the drawing when it sits above an object.

use super::request::CommandRequest;
use openstitch_core::{Command, StitchPattern};
use openstitch_design::{BackgroundImage, Rect, Vec2, v2};
use serde_json::json;
use std::ops::Range;

/// What a press on the image grabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grab {
    Move,
    /// A corner, clockwise from the top left: the opposite corner stays put.
    Corner(usize),
}

/// The image's rectangle, model units.
pub fn rect(b: &BackgroundImage) -> Rect {
    Rect {
        min_x: b.x,
        min_y: b.y,
        max_x: b.x + b.width,
        max_y: b.y + b.height,
    }
}

fn corners(r: Rect) -> [Vec2; 4] {
    [
        v2(r.min_x, r.min_y),
        v2(r.max_x, r.min_y),
        v2(r.max_x, r.max_y),
        v2(r.min_x, r.max_y),
    ]
}

/// What model point `p` grabs: a corner within `radius` (model units), else the inside.
pub fn grab(b: &BackgroundImage, p: Vec2, radius: f64) -> Option<Grab> {
    let r = rect(b);
    if let Some(i) = corners(r).iter().position(|c| c.distance(p) <= radius) {
        return Some(Grab::Corner(i));
    }
    (p.x >= r.min_x && p.x <= r.max_x && p.y >= r.min_y && p.y <= r.max_y).then_some(Grab::Move)
}

/// The image's x, y, width and height (model units) after dragging `g` by `d`. Scaling keeps
/// the image's proportions, as Modify Image's width does, and the opposite corner in place; the
/// image stays at least 1 mm wide.
pub fn dragged(b: &BackgroundImage, g: Grab, d: Vec2) -> (f64, f64, f64, f64) {
    match g {
        Grab::Move => (b.x + d.x, b.y + d.y, b.width, b.height),
        Grab::Corner(i) => {
            let c = corners(rect(b));
            let fixed = c[(i + 2) % 4];
            let moved = c[i] + d;
            let ratio = b.height / b.width.max(1e-9);
            let w = (moved.x - fixed.x).abs().max(10.0);
            let h = w * ratio;
            let x = if moved.x < fixed.x {
                fixed.x - w
            } else {
                fixed.x
            };
            let y = if i == 0 || i == 1 {
                fixed.y - h
            } else {
                fixed.y
            };
            (x, y, w, h)
        }
    }
}

/// The `background.modify` that applies a drag (mm), or `None` when nothing moved.
pub fn modify_request(b: &BackgroundImage, g: Grab, d: Vec2) -> Option<CommandRequest> {
    let (x, y, w, _) = dragged(b, g, d);
    if (x, y, w) == (b.x, b.y, b.width) {
        return None;
    }
    let mut params = json!({ "x": x / 10.0, "y": y / 10.0 });
    if w != b.width {
        params["width"] = json!(w / 10.0);
    }
    Some(CommandRequest::new("background.modify", params))
}

/// Stitches sewn up to the end of object `after` (from the compiler's per-object record
/// ranges): the background image above that object is drawn after them.
pub fn split_stitch(
    pattern: &StitchPattern,
    spans: &[(u64, Range<usize>)],
    after: u64,
) -> Option<usize> {
    let (_, r) = spans.iter().find(|(id, _)| *id == after)?;
    Some(
        pattern.stitches[..r.end.min(pattern.stitches.len())]
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .count(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::render::RenderModel;
    use openstitch_design::Compiler;
    use openstitch_engine::Session;

    fn image() -> BackgroundImage {
        BackgroundImage {
            path: "x.png".into(),
            pixel_width: 200,
            pixel_height: 100,
            x: 100.0,
            y: 100.0,
            width: 400.0,
            height: 200.0,
            opacity: 0.5,
            visible: true,
            adjust: Default::default(),
            layer: Default::default(),
        }
    }

    #[test]
    fn handles_move_and_scale_keeping_the_proportions() {
        let b = image();
        assert_eq!(grab(&b, v2(101.0, 101.0), 5.0), Some(Grab::Corner(0)));
        assert_eq!(grab(&b, v2(300.0, 200.0), 5.0), Some(Grab::Move));
        assert_eq!(grab(&b, v2(0.0, 0.0), 5.0), None);
        assert_eq!(
            dragged(&b, Grab::Move, v2(10.0, -20.0)),
            (110.0, 80.0, 400.0, 200.0)
        );
        // Bottom-right corner 200 right: twice as wide, top-left stays.
        assert_eq!(
            dragged(&b, Grab::Corner(2), v2(400.0, 0.0)),
            (100.0, 100.0, 800.0, 400.0)
        );
        // Top-left corner 200 right: half as wide, bottom-right stays.
        assert_eq!(
            dragged(&b, Grab::Corner(0), v2(200.0, 0.0)),
            (300.0, 200.0, 200.0, 100.0)
        );
        assert!(modify_request(&b, Grab::Move, v2(0.0, 0.0)).is_none());
    }

    #[test]
    fn a_handle_drag_equals_the_same_change_in_modify_image() {
        let dir = openstitch_testutil::TempDir::new("bg-handles");
        let png = dir.path().join("bg.png");
        let bytes = crate::png::encode_rgba(4, 2, &[200; 4 * 2 * 4]).expect("encode");
        std::fs::write(&png, bytes).expect("png");
        let mut s = Session::new(dir.path().to_path_buf());
        s.execute("background.set", Some(&json!({ "path": png, "width": 40 })))
            .expect("set");
        let b = s.design().background.clone().expect("image");
        let r = modify_request(&b, Grab::Corner(2), v2(200.0, 0.0)).expect("changed");
        s.execute(r.command, Some(&r.params)).expect("modify");
        let by_handle = s.design().background.clone().expect("image");
        s.execute("history.undo", None).expect("one undo step");
        assert_eq!(s.design().background, Some(b.clone()));
        s.execute("background.modify", Some(&json!({ "width": 60 })))
            .expect("modify image");
        assert_eq!(s.design().background, Some(by_handle));
    }

    #[test]
    fn the_layer_splits_the_drawing_after_its_object() {
        let mut s = Session::new(std::env::temp_dir());
        for x in [10, 50] {
            s.execute(
                "shape.rectangle",
                Some(&json!({ "x": x, "y": 10, "width": 20, "height": 20, "fill": "Red" })),
            )
            .expect("rectangle");
        }
        let d = s.design();
        let (pattern, spans) = Compiler::for_design(d).compile_traced(d);
        let k = split_stitch(&pattern, &spans, d.objects[0].id).expect("split");
        let all = RenderModel::build(&pattern, usize::MAX);
        assert!(k > 0 && k < all.shown);
        let lower = RenderModel::build(&pattern, k);
        let upper = RenderModel::build_range(&pattern, k, usize::MAX);
        let points = |m: &RenderModel| -> usize {
            m.layers.iter().flat_map(|l| &l.strokes).map(Vec::len).sum()
        };
        // Every stitch is drawn once: the halves add up to the whole (each half's first stroke
        // also carries the point it starts from).
        assert!(points(&lower) + points(&upper) >= points(&all));
        assert_eq!(upper.shown, all.shown);
        assert!(split_stitch(&pattern, &spans, 999).is_none());
    }
}
