//! #879: `machine.useSmallestHoop` and `design.fitToHoop`, one-click hoop commands.

use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// The design's sewn size, mm.
fn size(s: &mut Session) -> (f64, f64) {
    let info = run(s, "design.info", json!({}));
    (
        info["stitchStats"]["widthMM"].as_f64().expect("width"),
        info["stitchStats"]["heightMM"].as_f64().expect("height"),
    )
}

#[test]
fn the_smallest_hoop_that_fits_is_chosen_and_the_design_centred_in_one_undo_step() {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "machine.set", json!({"hoop": "130x180"}));
    // A 90 × 80 mm fill near the page's corner.
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 5, "y": 5, "width": 90, "height": 80, "fill": "Red"}),
    );
    let before = s.design().clone();
    let r = run(&mut s, "machine.useSmallestHoop", json!({}));
    assert_eq!(r["hoop"], "100x100", "{r}");
    assert_eq!(r["hoopMM"], json!([100.0, 100.0]), "{r}");
    let d = s.design();
    assert!((d.page.width - 1000.0).abs() < 1e-6 && (d.page.height - 1000.0).abs() < 1e-6);
    // Centred: the fill's box sits in the middle of the 100 mm page.
    let b = d.objects[0].contours[0]
        .points
        .iter()
        .fold((f64::MAX, f64::MIN), |(lo, hi), p| {
            (lo.min(p.x), hi.max(p.x))
        });
    assert!(((b.0 + b.1) / 2.0 - 500.0).abs() < 5.0, "{b:?}");
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before, "one undo step");
}

#[test]
fn a_round_hoop_is_judged_by_its_real_field() {
    let mut s = Session::new(std::env::temp_dir());
    // An 85 mm disc: its box (85 × 85) is outside a 90 mm round field, its needle points are not.
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 60, "cy": 60, "rx": 42.5, "fill": "Blue"}),
    );
    let r = run(
        &mut s,
        "machine.useSmallestHoop",
        json!({"brand": "Commercial round"}),
    );
    assert_eq!(r["hoop"], "round-12cm", "{r}");
}

#[test]
fn the_machine_profile_limits_the_hoops_and_nothing_fitting_is_refused() {
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "machine.setProfile",
        json!({"brand": "Brother", "hoop": "100x100"}),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 120, "height": 60, "fill": "Red"}),
    );
    let e = s
        .execute("machine.useSmallestHoop", Some(&json!({})))
        .expect_err("larger than the machine's hoop");
    let msg = e.to_string();
    assert!(msg.contains("multi-hooping"), "{msg}");
    assert!(msg.contains("120.") || msg.contains("12"), "{msg}");
    // An unknown brand is refused.
    assert!(
        s.execute("machine.useSmallestHoop", Some(&json!({"brand": "Nobody"})))
            .is_err()
    );
}

#[test]
fn fit_to_hoop_fills_the_hoop_less_the_margin_in_one_undo_step() {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "machine.set", json!({"hoop": "100x100"}));
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 40, "height": 20, "fill": "Red"}),
    );
    let before = s.design().clone();
    let r = run(&mut s, "design.fitToHoop", json!({}));
    assert_eq!(r["warnings"], json!([]), "{r}");
    let (w, h) = size(&mut s);
    assert!(
        (w - 90.0).abs() < 1.0,
        "fills the 100 mm hoop less 5 mm a side: {w}"
    );
    assert!((h - 45.0).abs() < 1.5, "keeps its aspect: {h}");
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before, "one undo step");
    // Without keepAspect a rectangle field is filled both ways.
    run(
        &mut s,
        "design.fitToHoop",
        json!({"keepAspect": false, "marginMm": 10}),
    );
    let (w, h) = size(&mut s);
    assert!(
        (w - 80.0).abs() < 1.0 && (h - 80.0).abs() < 1.5,
        "{w} × {h}"
    );
}

#[test]
fn fit_to_hoop_warns_when_stitch_objects_change_density() {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "machine.set", json!({"hoop": "100x100"}));
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 30, "fill": "Red"}),
    );
    run(&mut s, "object.toStitches", json!({"ids": [1]}));
    let r = run(&mut s, "design.fitToHoop", json!({}));
    let w = r["warnings"][0].as_str().unwrap_or_default();
    assert!(w.contains("density"), "{r}");
    // An oval or round field keeps the aspect: keepAspect false is refused there.
    run(&mut s, "machine.set", json!({"hoop": "round-12cm"}));
    assert!(
        s.execute("design.fitToHoop", Some(&json!({"keepAspect": false})))
            .is_err()
    );
}
