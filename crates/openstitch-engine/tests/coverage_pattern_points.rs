//! Coverage for the pattern reference point on the region types FEATURE_MATRIX lists beyond
//! programmable and motif fill (cross stitch, net, zigzag net, decorative fill), and the centre
//! point on spiral (#351): PE-DESIGN's spiral has no centre point, so the command refuses it (#354).

use openstitch_core::Command;
use openstitch_design::Compiler;
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn needle_points(s: &Session) -> Vec<(i64, i64)> {
    Compiler::default()
        .compile(s.design())
        .stitches
        .iter()
        .filter(|st| st.command == Command::Stitch)
        .map(|st| (st.x.round() as i64, st.y.round() as i64))
        .collect()
}

/// A 40 × 30 mm rectangle with no line, sewn as `kind` without under sewing.
fn region(kind: &str) -> (Session, u64) {
    let mut s = Session::new(std::env::temp_dir());
    let id = run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 40, "height": 30}),
    )["id"]
        .as_u64()
        .expect("id");
    run(&mut s, "object.setLine", json!({"id": id, "type": "none"}));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": id, "type": kind, "underlay": false}),
    );
    (s, id)
}

#[test]
fn the_reference_point_moves_the_pattern_of_every_patterned_fill() {
    for kind in ["crossStitch", "netFill", "zigzagNetFill", "decorativeFill"] {
        let (mut s, id) = region(kind);
        let default = needle_points(&s);
        assert!(!default.is_empty(), "{kind} sews");
        run(
            &mut s,
            "object.setReferencePoint",
            json!({"id": id, "point": [11.7, 13.3]}),
        );
        let moved = needle_points(&s);
        assert_ne!(
            moved, default,
            "{kind}: the pattern starts at the reference point"
        );
        run(
            &mut s,
            "object.setReferencePoint",
            json!({"id": id, "point": null}),
        );
        assert_eq!(needle_points(&s), default, "{kind}: null restores it");
    }
}

#[test]
fn a_spiral_refuses_a_centre_point() {
    let (mut s, id) = region("spiral");
    let default = needle_points(&s);
    assert!(!default.is_empty(), "the spiral sews");
    let err = s
        .execute(
            "object.setCenterPoint",
            Some(&json!({"id": id, "point": [20, 20]})),
        )
        .expect_err("PE-DESIGN's spiral follows its outline and has no centre point");
    assert!(err.to_string().contains("no such setting"), "{err}");
    let centre = s.design().objects[0].region.as_ref().and_then(|r| r.center);
    assert_eq!(centre, None, "nothing is stored");
    assert_eq!(needle_points(&s), default);
    // Clearing it (null) is accepted on any region type.
    run(
        &mut s,
        "object.setCenterPoint",
        json!({"id": id, "point": null}),
    );
}
