//! Decorative hand-embroidery lines through the commands (#543).

use openstitch_core::Command;
use openstitch_design::{Compiler, LineType};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn every_decorative_line_sews_within_seven_millimetres_and_undoes() {
    let dir = TempDir::new("decorative");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "shape.path",
        json!({"points": [[10, 30], [90, 30]], "closed": false, "curves": [[[35, 15], [65, 45]]]}),
    );
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 70, "rx": 25, "ry": 15}),
    );
    run(&mut s, "object.setRegion", json!({"id": 2, "type": "none"}));
    let decorative: Vec<LineType> = LineType::ALL
        .into_iter()
        .filter(|k| k.is_decorative())
        .collect();
    assert_eq!(decorative.len(), 8);
    for kind in decorative {
        let before = s.design().clone();
        run(
            &mut s,
            "object.setLine",
            json!({"ids": [1, 2], "type": kind.id(), "width": 4, "spacing": 3}),
        );
        let p = Compiler::for_design(s.design()).compile(s.design());
        let needles: Vec<_> = p
            .stitches
            .windows(2)
            .filter(|w| w[0].command == Command::Stitch && w[1].command == Command::Stitch)
            .map(|w| ((w[0].x - w[1].x).powi(2) + (w[0].y - w[1].y).powi(2)).sqrt())
            .collect();
        assert!(needles.len() > 50, "{kind:?}");
        assert!(needles.iter().all(|&d| d <= 70.0 + 1e-6), "{kind:?}");
        run(&mut s, "history.undo", json!({}));
        assert_eq!(s.design(), &before, "{kind:?}: one undo step");
    }
}

#[test]
fn stripes_lattice_and_slanted_e_stitch_take_their_settings() {
    let dir = TempDir::new("decorative-settings");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "shape.path",
        json!({"points": [[10, 30], [90, 30]], "closed": false}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "stripes", "width": 6, "gaps": [1, 2, 1, 2, 1]}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert_eq!(l.gaps, vec![1.0, 2.0, 1.0, 2.0, 1.0]);
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "gaps": null, "strands": 5}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert!(l.gaps.is_empty());
    assert_eq!(l.strands, Some(5));
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "lattice", "strands": 5}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "eStitch", "prongAngle": 45}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert_eq!(l.prong_angle, Some(45.0));
    run(&mut s, "object.setLine", json!({"id": 1, "prongAngle": 0}));
    assert_eq!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .prong_angle,
        None
    );
    let before = s.design().clone();
    for params in [
        json!({"id": 1, "type": "lattice", "strands": 4}),
        json!({"id": 1, "type": "stripes", "strands": 13}),
        json!({"id": 1, "type": "stripes", "gaps": [0.1]}),
        json!({"id": 1, "type": "stripes", "gaps": []}),
        json!({"id": 1, "type": "running", "strands": 3}),
        json!({"id": 1, "type": "running", "gaps": [1]}),
        json!({"id": 1, "prongAngle": 70}),
        json!({"id": 1, "type": "zigzag", "prongAngle": 10}),
    ] {
        assert!(
            s.execute("object.setLine", Some(&params)).is_err(),
            "{params}"
        );
        assert_eq!(s.design(), &before, "{params}");
    }
}
