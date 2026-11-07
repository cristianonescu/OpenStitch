//! Branching and auto-routing through the commands (#546).

use openstitch_core::{Command, StitchPattern};
use openstitch_design::{Compiler, Design};
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

/// Trims before the last needle point: the moves between parts (not the final cut).
fn inner_trims(p: &StitchPattern) -> usize {
    let last = p
        .stitches
        .iter()
        .rposition(|s| s.command == Command::Stitch)
        .expect("stitches");
    p.stitches[..last]
        .iter()
        .filter(|s| s.command == Command::Trim)
        .count()
}

fn line(s: &mut Session, points: Value, colour: &str) -> u64 {
    run(
        s,
        "shape.path",
        json!({"points": points, "closed": false, "outline": colour}),
    )["id"]
        .as_u64()
        .expect("id")
}

fn length(d: &Design, id: u64) -> f64 {
    let o = &d.objects[d.index_of(id).expect("object")];
    openstitch_design::graph::length(&o.contours[0].path())
}

#[test]
fn a_tree_of_lines_sews_with_no_trims() {
    let tmp = TempDir::new("branching");
    let mut s = Session::new(tmp.path());
    // A "Y" of three running lines meeting at (50, 50), drawn apart.
    let a = line(&mut s, json!([[30, 30], [50, 50]]), "#ff0000");
    let b = line(&mut s, json!([[70, 30], [50, 50]]), "#ff0000");
    let c = line(&mut s, json!([[50, 50], [50, 80]]), "#ff0000");
    assert!(inner_trims(&compile(s.design())) >= 1);
    let r = run(&mut s, "shape.autoRoute", json!({"ids": [a, b, c]}));
    assert_eq!(r["runs"], 1);
    let id = r["ids"][0].as_u64().expect("id");
    assert_eq!(s.design().objects.len(), 1);
    assert_eq!(inner_trims(&compile(s.design())), 0);
    // Every line once, plus the shortest leg once more (four odd vertices).
    let leg = 200.0 * std::f64::consts::SQRT_2;
    let expected = 2.0 * leg + 300.0 + leg;
    assert!(
        (length(s.design(), id) - expected).abs() < 0.5,
        "{}",
        length(s.design(), id)
    );
    // One undo step brings the three lines back.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 3);
}

#[test]
fn redwork_sews_each_line_twice_and_threads_route_apart() {
    let tmp = TempDir::new("branching");
    let mut s = Session::new(tmp.path());
    let a = line(&mut s, json!([[10, 10], [40, 10]]), "#ff0000");
    let b = line(&mut s, json!([[40, 10], [40, 40]]), "#ff0000");
    let c = line(&mut s, json!([[60, 60], [90, 60]]), "#0000ff");
    let r = run(
        &mut s,
        "shape.autoRoute",
        json!({"ids": [a, b, c], "redwork": true, "keep": true}),
    );
    assert_eq!(r["runs"], 2);
    let red = r["ids"][0].as_u64().expect("id");
    assert!((length(s.design(), red) - 2.0 * 600.0).abs() < 0.5);
    let d = s.design();
    let path = d.objects[d.index_of(red).expect("red")].contours[0].path();
    assert_eq!(path.first(), path.last(), "redwork returns to its start");
    // Kept originals are hidden.
    assert_eq!(d.objects.iter().filter(|o| !o.visible).count(), 3);
    // A region is refused.
    let sq = run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 5, "height": 5, "fill": "#ff0000"}),
    )["id"]
        .clone();
    assert!(
        s.execute("shape.autoRoute", Some(&json!({"ids": [sq]})))
            .is_err()
    );
}

#[test]
fn a_branch_sews_touching_shapes_in_one_go() {
    let tmp = TempDir::new("branching");
    let mut s = Session::new(tmp.path());
    let rect = |s: &mut Session, x: f64, colour: &str| {
        run(
            s,
            "shape.rectangle",
            json!({"x": x, "y": 20, "width": 20, "height": 10, "fill": colour}),
        )["id"]
            .as_u64()
            .expect("id")
    };
    // A chain of touching squares, drawn out of order and in two colours.
    let a = rect(&mut s, 10.0, "#ff0000");
    let c = rect(&mut s, 50.0, "#0000ff");
    let b = rect(&mut s, 30.0, "#ff0000");
    let plain = compile(s.design());
    assert_eq!(plain.count(Command::ColorChange), 2);
    let r = run(&mut s, "object.branch", json!({"ids": [a, b, c]}));
    assert_eq!(r["order"], json!([a, b, c]));
    assert_eq!(r["gapsMM"], json!([0.0, 0.0]));
    let p = compile(s.design());
    assert_eq!(p.count(Command::ColorChange), 0, "one thread");
    assert_eq!(inner_trims(&p), 0);
    let listed = run(&mut s, "object.list", json!({}));
    assert!(
        listed
            .as_array()
            .expect("list")
            .iter()
            .all(|o| o["branch"] == true)
    );
    // Editing a member re-links the branch: still no trim.
    run(&mut s, "object.move", json!({"id": c, "dy": 3}));
    assert_eq!(inner_trims(&compile(s.design())), 0);
    // A member that no longer touches trims as usual.
    run(&mut s, "object.move", json!({"id": c, "dx": 20}));
    assert_eq!(inner_trims(&compile(s.design())), 1);
    run(&mut s, "object.unbranch", json!({"ids": [a]}));
    assert!(
        s.design()
            .objects
            .iter()
            .all(|o| !o.branch && o.group.is_none())
    );
}
