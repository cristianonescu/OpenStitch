//! Shape builder and line gaps (#497, part 2).

use openstitch_core::Command;
use openstitch_design::boolean::{region_area, simplify};
use openstitch_design::{Compiler, DesignObject, Vec2, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

fn area(o: &DesignObject) -> f64 {
    simplify(&o.contours).iter().map(|r| region_area(r)).sum()
}

fn session() -> Session {
    Session::new(std::env::temp_dir())
}

#[test]
fn a_drag_merges_the_pieces_it_crosses_and_alt_removes_them() {
    let mut s = session();
    let a = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 10, "height": 10, "fill": "Red" }),
    ));
    let b = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 5, "y": 5, "width": 10, "height": 10, "fill": "Blue" }),
    ));
    let blue = s.design().objects[1].region.clone().expect("region").thread;
    // From the red square's own part into the overlap: one shape of 75 + 25 mm², blue.
    let r = run(
        &mut s,
        "shape.build",
        json!({ "ids": [a, b], "path": [[2, 2], [7, 7]] }),
    );
    assert_eq!(r["ids"].as_array().expect("ids").len(), 2);
    let objs = &s.design().objects;
    assert_eq!(objs.len(), 2);
    let merged = objs
        .iter()
        .find(|o| (area(o) - 10_000.0).abs() < 1.0)
        .expect("merged");
    assert_eq!(merged.region.as_ref().expect("region").thread, blue);
    assert!(objs.iter().any(|o| (area(o) - 7_500.0).abs() < 1.0));
    // One undo step.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 2);
    assert_eq!(s.design().objects[0].id, a);
    // Remove the overlap: two L shapes stay.
    run(
        &mut s,
        "shape.build",
        json!({ "ids": [a, b], "path": [[7, 7]], "mode": "remove" }),
    );
    let total: f64 = s.design().objects.iter().map(area).sum();
    assert!((total - 15_000.0).abs() < 1.0);
    assert!(
        s.execute(
            "shape.build",
            Some(&json!({ "ids": [a], "path": [[1, 1]] }))
        )
        .is_err()
    );
}

#[test]
fn a_drag_that_misses_is_refused() {
    let mut s = session();
    let a = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 10, "height": 10 }),
    ));
    let b = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 5, "y": 5, "width": 10, "height": 10 }),
    ));
    let e = s
        .execute(
            "shape.build",
            Some(&json!({ "ids": [a, b], "path": [[50, 50]] })),
        )
        .expect_err("misses");
    assert!(e.to_string().contains("crosses no piece"), "{e}");
}

fn needles(s: &Session) -> Vec<Vec2> {
    let d = s.design();
    Compiler::for_design(d)
        .compile(d)
        .stitches
        .iter()
        .filter(|x| x.command == Command::Stitch)
        .map(|x| v2(x.x, x.y))
        .collect()
}

#[test]
fn a_gap_leaves_its_span_unsewn_and_ties_off_at_both_ends() {
    let mut s = session();
    // A 40 × 20 mm outline sewn as a running line; the top edge runs from (10, 10) to (50, 10).
    let r = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 40, "height": 20, "outline": "Black" }),
    ));
    assert!(s.design().objects[0].region.is_none());
    let before = needles(&s);
    assert!(
        before
            .iter()
            .any(|p| p.y < 101.0 && p.x > 250.0 && p.x < 350.0)
    );
    // 10 mm open from 15 mm along the outline: x 25..35 mm on the top edge.
    run(
        &mut s,
        "object.setLineGap",
        json!({ "ids": [r], "offset": 15, "length": 10 }),
    );
    let after = needles(&s);
    let in_gap = |p: &Vec2| p.y < 101.0 && p.x > 251.0 && p.x < 349.0;
    assert!(!after.iter().any(in_gap), "no needle point inside the gap");
    // Sewing starts at the gap's far end and ends at its near end, each with a tie.
    let first = after[0];
    let last = *after.last().expect("stitches");
    assert!(first.distance(v2(350.0, 100.0)) < 5.0, "{first:?}");
    assert!(last.distance(v2(250.0, 100.0)) < 5.0, "{last:?}");
    let near = |q: Vec2| after.iter().filter(|p| p.distance(q) < 5.0).count();
    // Tie-in: 0.3 mm out and back from the landing; lock: back and forth at the end.
    assert!(near(v2(350.0, 100.0)) >= 2 && near(v2(250.0, 100.0)) >= 3);
    assert!(after[0].distance(after[1]) > 1.0 && after[0].distance(after[1]) < 5.0);
    // Other line types with a gap; the gap stays when the type changes.
    run(
        &mut s,
        "object.setLine",
        json!({ "ids": [r], "type": "zigzag" }),
    );
    assert!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .gap
            .is_some()
    );
    assert!(
        !needles(&s)
            .iter()
            .any(|p| p.y < 95.0 && p.x > 270.0 && p.x < 330.0)
    );
    // null removes it; one undo per command.
    run(
        &mut s,
        "object.setLineGap",
        json!({ "ids": [r], "length": null }),
    );
    assert!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .gap
            .is_none()
    );
    run(&mut s, "history.undo", json!({}));
    assert!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .gap
            .is_some()
    );
}

#[test]
fn gaps_are_refused_where_they_cannot_work() {
    let mut s = session();
    let r = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 10, "height": 10, "outline": "Black" }),
    ));
    let e = s
        .execute(
            "object.setLineGap",
            Some(&json!({ "ids": [r], "length": 40 })),
        )
        .expect_err("too long");
    assert!(e.to_string().contains("shorter than the outline"), "{e}");
    run(
        &mut s,
        "object.setLine",
        json!({ "ids": [r], "type": "stem" }),
    );
    assert!(
        s.execute(
            "object.setLineGap",
            Some(&json!({ "ids": [r], "length": 5 }))
        )
        .is_err()
    );
    let open = id(&run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[0, 0], [10, 0]], "closed": false }),
    ));
    assert!(
        s.execute(
            "object.setLineGap",
            Some(&json!({ "ids": [open], "length": 2 }))
        )
        .is_err()
    );
    let fill = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 10, "height": 10 }),
    ));
    assert!(
        s.execute(
            "object.setLineGap",
            Some(&json!({ "ids": [fill], "length": 2 }))
        )
        .is_err()
    );
}
