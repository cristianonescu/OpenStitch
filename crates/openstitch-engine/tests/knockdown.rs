//! Knockdown fill (#510, beyond PE-DESIGN) through `wizard.knockdown`.

use openstitch_core::{Command, StitchPattern};
use openstitch_design::{Compiler, Design, v2};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn compile(d: &Design) -> StitchPattern {
    Compiler::for_design(d).compile(d)
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64, fill: &str) -> u64 {
    let r = run(
        s,
        "shape.rectangle",
        json!({"x": x, "y": y, "width": w, "height": h, "fill": fill}),
    );
    r["id"].as_u64().expect("id")
}

#[test]
fn knockdown_covers_the_union_and_stays_first() {
    let tmp = TempDir::new("knockdown");
    let mut s = Session::new(tmp.path());
    let a = rect(&mut s, 20.0, 20.0, 20.0, 20.0, "#ff0000");
    rect(&mut s, 42.0, 20.0, 20.0, 20.0, "#ff0000");
    let r = run(&mut s, "wizard.knockdown", json!({}));
    let ids = r["ids"].as_array().expect("present");
    assert_eq!(ids.len(), 1);
    // 46 x 24 mm with the four outer corners rounded at the 2 mm reach; the 2 mm gap closed.
    let expected = 46.0 * 24.0 - 4.0 * (4.0 - std::f64::consts::PI);
    let area = r["areaMM2"].as_f64().expect("present");
    assert!(
        (area - expected).abs() / expected < 0.03,
        "{area} vs {expected}"
    );
    assert_eq!(r["contours"], 1);
    let d = s.design();
    assert_eq!(d.objects[0].name.as_deref(), Some("Knockdown"));
    let region = d.objects[0].region.as_ref().expect("present");
    assert!(!region.underlay);
    assert!((region.density - 1.0).abs() < 1e-9);
    // One undo step.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 2);

    // Selection, two crossed layers.
    let r = run(
        &mut s,
        "wizard.knockdown",
        json!({"ids": [a], "layers": 2, "angle": 30}),
    );
    let area = r["areaMM2"].as_f64().expect("present");
    let expected = 24.0 * 24.0 - (4.0 - std::f64::consts::PI) * 4.0;
    assert!(
        (area - expected).abs() / expected < 0.03,
        "{area} vs {expected}"
    );
    let d = s.design();
    let angles: Vec<f64> = d.objects[..2]
        .iter()
        .map(|o| o.region.as_ref().expect("present").angle)
        .collect();
    assert_eq!(angles, vec![30.0, 120.0]);
    assert!(d.objects[0].group.is_some() && d.objects[0].group == d.objects[1].group);
    let p = compile(d);
    for w in p.stitches.windows(2) {
        if w[1].command == Command::Stitch && w[0].command == Command::Stitch {
            assert!(v2(w[0].x, w[0].y).distance(v2(w[1].x, w[1].y)) <= 120.5);
        }
    }
    run(&mut s, "history.undo", json!({}));

    // Whole design: Optimize keeps it first, update covers a newly added object.
    let first = run(&mut s, "wizard.knockdown", json!({"color": "#00ff00"}));
    let kd = first["ids"][0].as_u64().expect("present");
    rect(&mut s, 20.0, 60.0, 20.0, 10.0, "#00ff00");
    run(&mut s, "order.optimize", json!({"mode": "minJump"}));
    assert_eq!(s.design().objects[0].id, kd);
    let r = run(&mut s, "wizard.knockdown", json!({"update": kd}));
    assert!(
        r["areaMM2"].as_f64().expect("present")
            > first["areaMM2"].as_f64().expect("present") + 300.0,
        "{r} {first}"
    );
    assert_eq!(
        s.design()
            .objects
            .iter()
            .filter(|o| o.knockdown.is_some())
            .count(),
        1
    );
    assert_eq!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("present")
            .thread
            .hex(),
        "#00ff00"
    );
}

#[test]
fn knockdown_keeps_large_holes() {
    let tmp = TempDir::new("knockdown");
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 25, "ry": 25, "fill": "#ff0000"}),
    );
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 15, "ry": 15, "fill": "#ff0000"}),
    );
    run(
        &mut s,
        "shape.combine",
        json!({"ids": [1, 2], "op": "difference"}),
    );
    let r = run(&mut s, "wizard.knockdown", json!({}));
    assert_eq!(r["contours"], 2, "outer and hole");
    let p = compile(s.design());
    let centre = v2(500.0, 500.0);
    for st in &p.stitches {
        if st.command == Command::Stitch {
            assert!(
                v2(st.x, st.y).distance(centre) > 125.0,
                "needle in the hole"
            );
        }
    }
    let r = run(
        &mut s,
        "wizard.knockdown",
        json!({"update": r["ids"][0], "keepHoles": false}),
    );
    assert_eq!(r["contours"], 1);
}
