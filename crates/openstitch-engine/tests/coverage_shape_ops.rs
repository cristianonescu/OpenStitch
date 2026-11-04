//! Coverage for shape-operation claims no other test pins (#351): offset lines' curves within
//! 0.05 mm of the true offset, `roundedCorners` rounding the corners, and a split curved shape
//! keeping Bézier pieces.

use openstitch_design::{Vec2, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

/// Every flattened outline point of object `id`.
fn outline(s: &Session, id: u64) -> Vec<Vec2> {
    let d = s.design();
    d.objects[d.index_of(id).expect("object")]
        .contours
        .iter()
        .flat_map(|c| c.outline_points())
        .collect()
}

#[test]
fn offset_lines_of_a_circle_stay_within_0_05_mm_of_the_offset_circle() {
    let mut s = Session::new(std::env::temp_dir());
    let circle = id(&run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 10, "ry": 10}),
    ));
    let centre = v2(500.0, 500.0);
    for (direction, radius) in [("outward", 120.0), ("inward", 80.0)] {
        let ring = id(&run(
            &mut s,
            "shape.offsetLines",
            json!({"id": circle, "direction": direction, "spacing": 2}),
        ));
        let d = s.design();
        let o = &d.objects[d.index_of(ring).expect("ring")];
        assert!(
            o.contours.iter().any(|c| c.has_curves()),
            "{direction}: the offset keeps Bézier pieces"
        );
        let points = outline(&s, ring);
        assert!(points.len() > 16);
        let worst = points
            .iter()
            .map(|p| ((*p - centre).length() - radius).abs())
            .fold(0.0_f64, f64::max);
        assert!(
            worst <= 0.5,
            "{direction}: {worst:.3} units (0.1 mm) from the {radius} unit circle"
        );
    }
}

#[test]
fn rounded_corners_round_the_offset_of_a_square() {
    let mut s = Session::new(std::env::temp_dir());
    let square = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 20, "y": 20, "width": 20, "height": 20}),
    ));
    // The farthest an outward offset of 3 mm reaches from the square's corner (20, 20) mm.
    let reach = |s: &mut Session, rounded: bool| {
        let ring = id(&run(
            s,
            "shape.offsetLines",
            json!({"id": square, "spacing": 3, "roundedCorners": rounded}),
        ));
        let corner = v2(200.0, 200.0);
        outline(s, ring)
            .iter()
            .filter(|p| p.x <= corner.x && p.y <= corner.y)
            .map(|p| (*p - corner).length())
            .fold(0.0_f64, f64::max)
    };
    let sharp = reach(&mut s, false);
    let round = reach(&mut s, true);
    // Mitred: the corner sits 3 mm out on both axes, 3√2 ≈ 4.24 mm away. Rounded: 3 mm.
    assert!((sharp - 30.0 * 2f64.sqrt()).abs() < 1.0, "sharp {sharp}");
    assert!((round - 30.0).abs() < 1.0, "rounded {round}");
}

#[test]
fn splitting_a_circle_keeps_curved_pieces() {
    let mut s = Session::new(std::env::temp_dir());
    let circle = id(&run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 20, "ry": 20}),
    ));
    // Cut along the vertical diameter.
    let r = run(
        &mut s,
        "shape.split",
        json!({"id": circle, "points": [[50, 20], [80, 20], [80, 80], [50, 80]]}),
    );
    let piece = r["piece"].as_u64().expect("piece");
    let centre = v2(500.0, 500.0);
    for half in [circle, piece] {
        let d = s.design();
        let o = &d.objects[d.index_of(half).expect("half")];
        assert!(
            o.contours.iter().any(|c| c.has_curves()),
            "object {half}: the arc stays a Bézier curve"
        );
        // Every point is on the circle or on the cut.
        for p in outline(&s, half) {
            let on_circle = ((p - centre).length() - 200.0).abs() <= 0.5;
            let on_cut = (p.x - 500.0).abs() <= 0.5;
            assert!(on_circle || on_cut, "object {half}: {p:?}");
        }
    }
}
