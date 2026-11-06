//! Knife: one open cut line splits every object it crosses (#477).

use openstitch_core::Command;
use openstitch_design::boolean::{region_area, simplify};
use openstitch_design::{Compiler, DesignObject, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

fn object(s: &Session, id: u64) -> &DesignObject {
    let d = s.design();
    &d.objects[d.index_of(id).expect("object")]
}

fn area(o: &DesignObject) -> f64 {
    simplify(&o.contours).iter().map(|r| region_area(r)).sum()
}

fn session() -> Session {
    Session::new(std::env::temp_dir())
}

#[test]
fn one_cut_across_a_fill_a_column_and_a_line_gives_six_objects() {
    let mut s = session();
    let fill = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 20, "height": 20, "fill": "Red" }),
    ));
    let col = id(&run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[40, 10], [44, 10]], [[40, 20], [44, 20]], [[40, 30], [44, 30]]] }),
    ));
    let line = id(&run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[50, 10], [50, 30]], "closed": false, "outline": "Blue" }),
    ));
    let region = object(&s, fill).region.clone().expect("region");
    let before_area = area(object(&s, fill));
    let r = run(
        &mut s,
        "shape.knife",
        json!({ "line": [[5, 15], [60, 15]] }),
    );
    assert_eq!(r["cut"].as_array().expect("cut").len(), 3, "{r}");
    assert_eq!(s.design().objects.len(), 6);
    // The fill's halves keep its attributes and its area.
    let fills: Vec<&DesignObject> = s
        .design()
        .objects
        .iter()
        .filter(|o| {
            o.region.as_ref().is_some_and(|r| r.thread == region.thread)
                && o.region
                    .as_ref()
                    .is_some_and(|r| r.direction_lines.is_empty())
        })
        .collect();
    assert_eq!(fills.len(), 2);
    let total: f64 = fills.iter().map(|o| area(o)).sum();
    assert!(
        (total - before_area).abs() < before_area * 0.002,
        "{total} vs {before_area}"
    );
    for f in &fills {
        let r = f.region.as_ref().expect("region");
        assert_eq!(
            (r.kind, r.density, r.angle),
            (region.kind, region.density, region.angle)
        );
        // Entry and exit stay where they are still on the piece's outline (#652), or reset.
        assert!(
            r.entry.is_none() || r.entry == region.entry,
            "{:?}",
            r.entry
        );
        assert!(r.exit.is_none() || r.exit == region.exit, "{:?}", r.exit);
    }
    // The original points are on the bottom edge: the lower half keeps them, the upper resets.
    assert_eq!(
        fills
            .iter()
            .filter(|f| f.region.as_ref().expect("region").entry.is_some())
            .count(),
        usize::from(region.entry.is_some())
    );
    // The first piece keeps the id; the rest follow it in sewing order.
    let order: Vec<u64> = s.design().objects.iter().map(|o| o.id).collect();
    assert_eq!(order[0], fill);
    assert_eq!(order[2], col);
    assert_eq!(order[4], line);
    // One undo step brings the three objects back.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 3);
}

#[test]
fn column_halves_keep_their_line_order_and_share_the_cut() {
    let mut s = session();
    let col = id(&run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[40, 10], [44, 10]], [[40, 20], [44, 20]], [[40, 30], [44, 30]]] }),
    ));
    let r = run(
        &mut s,
        "shape.knife",
        json!({ "line": [[30, 25], [50, 25]] }),
    );
    let ids: Vec<u64> = r["cut"][0]["pieces"]
        .as_array()
        .expect("pieces")
        .iter()
        .map(|v| v.as_u64().expect("id"))
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], col);
    let a = object(&s, ids[0])
        .region
        .clone()
        .expect("region")
        .direction_lines;
    let b = object(&s, ids[1])
        .region
        .clone()
        .expect("region")
        .direction_lines;
    assert_eq!(a.len(), 3);
    assert_eq!(b.len(), 2);
    assert_eq!(a.last(), b.first(), "shared cut line");
    assert_eq!(a[0], [v2(400.0, 100.0), v2(440.0, 100.0)]);
    assert_eq!(b[1], [v2(400.0, 300.0), v2(440.0, 300.0)]);
    // Both halves sew; the second starts where the first's cut line is.
    let d = s.design();
    let pattern = Compiler::for_design(d).compile(d);
    assert!(
        pattern
            .stitches
            .iter()
            .filter(|x| x.command == Command::Stitch)
            .count()
            > 20
    );
    let ys: Vec<f64> = pattern
        .stitches
        .iter()
        .filter(|x| x.command == Command::Stitch)
        .map(|x| x.y)
        .collect();
    let covered = |y: f64| ys.iter().any(|&v| (v - y).abs() < 4.0);
    for y in [110.0, 200.0, 250.0, 280.0] {
        assert!(covered(y), "stitches near y = {y}");
    }
}

#[test]
fn a_closed_outline_cut_once_opens_and_twice_makes_two_lines() {
    let mut s = session();
    let o = id(&run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[10, 10], [30, 10], [30, 30], [10, 30]], "outline": "Blue" }),
    ));
    // A line-only closed outline (no region): the default outline-only shape has none.
    run(
        &mut s,
        "object.setRegion",
        json!({ "ids": [o], "type": "none" }),
    );
    assert!(object(&s, o).region.is_none());
    run(
        &mut s,
        "shape.knife",
        json!({ "line": [[20, 5], [20, 15]] }),
    );
    assert_eq!(s.design().objects.len(), 1);
    let c = &object(&s, o).contours[0];
    assert!(!c.closed);
    assert_eq!(c.points.first(), c.points.last());
    run(&mut s, "history.undo", json!({}));
    run(
        &mut s,
        "shape.knife",
        json!({ "line": [[20, 5], [20, 35]] }),
    );
    assert_eq!(s.design().objects.len(), 2);
    assert!(s.design().objects.iter().all(|o| !o.contours[0].closed));
}

#[test]
fn lines_that_do_not_cross_fully_leave_shapes_unchanged_and_are_reported() {
    let mut s = session();
    let a = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 20, "height": 20 }),
    ));
    let b = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 50, "y": 10, "width": 20, "height": 20 }),
    ));
    let before = object(&s, a).contours.clone();
    // Ends inside the first square, crosses the second fully.
    let r = run(
        &mut s,
        "shape.knife",
        json!({ "line": [[20, 20], [80, 20]] }),
    );
    assert_eq!(object(&s, a).contours, before);
    assert_eq!(r["skipped"][0]["id"], a);
    assert!(
        r["skipped"][0]["reason"]
            .as_str()
            .expect("reason")
            .contains("fully")
    );
    assert_eq!(r["cut"][0]["id"], b);
    // Nothing cut at all: an error that says why.
    let e = s
        .execute(
            "shape.knife",
            Some(&json!({ "line": [[12, 12], [14, 14]] })),
        )
        .expect_err("inside only");
    assert!(e.to_string().contains("nothing was cut"), "{e}");
    let e = s
        .execute(
            "shape.knife",
            Some(&json!({ "line": [[200, 200], [300, 300]] })),
        )
        .expect_err("misses");
    assert!(e.to_string().contains("crosses no object"), "{e}");
}

#[test]
fn hole_sewing_text_and_stitches_are_refused_and_split_is_unchanged() {
    let mut s = session();
    let outer = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 40, "height": 40 }),
    ));
    let inner = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 20, "height": 20 }),
    ));
    run(
        &mut s,
        "object.setHoleSewing",
        json!({ "ids": [outer, inner], "enabled": true }),
    );
    let r = s.execute(
        "shape.knife",
        Some(&json!({ "line": [[-5, 2], [45, 2]], "ids": [outer] })),
    );
    let e = r.expect_err("hole sewing");
    assert!(e.to_string().contains("hole sewing"), "{e}");
    // shape.split still cuts with a closed outline.
    let sq = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 100, "y": 0, "width": 40, "height": 40 }),
    ));
    let r = run(
        &mut s,
        "shape.split",
        json!({ "id": sq, "points": [[90, -10], [120, -10], [120, 50], [90, 50]] }),
    );
    assert!(r["piece"].as_u64().is_some());
}

#[test]
fn a_click_splits_the_line_under_it() {
    let mut s = session();
    let l = id(&run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[10, 10], [30, 10]], "closed": false }),
    ));
    let r = run(&mut s, "shape.knife", json!({ "at": [20, 10.2] }));
    assert_eq!(r["ids"].as_array().expect("ids").len(), 2);
    assert_eq!(object(&s, l).contours[0].points[1], v2(200.0, 100.0));
    assert!(
        s.execute("shape.knife", Some(&json!({ "at": [20, 20] })))
            .is_err()
    );
    assert!(
        s.execute(
            "shape.knife",
            Some(&json!({ "at": [20, 10], "line": [[0, 0], [1, 1]] }))
        )
        .is_err()
    );
}

#[test]
fn a_satin_skeleton_is_cut_across() {
    let mut s = session();
    let k = id(&run(
        &mut s,
        "shape.skeletonSatin",
        json!({ "points": [[10, 10], [50, 10]], "width": 4 }),
    ));
    let r = run(
        &mut s,
        "shape.knife",
        json!({ "line": [[30, 0], [30, 20]] }),
    );
    let ids = r["ids"].as_array().expect("ids");
    assert_eq!(ids.len(), 2);
    let sk = object(&s, k)
        .region
        .clone()
        .expect("region")
        .skeleton
        .expect("skeleton");
    assert_eq!(sk.points.last(), Some(&v2(300.0, 100.0)));
}
