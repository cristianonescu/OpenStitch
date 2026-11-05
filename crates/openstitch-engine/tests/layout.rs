//! Align, distribute (#472), skew (#474) and the `object.list` Select Same filters (#475).

use openstitch_core::Command;
use openstitch_design::{Compiler, Rect, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// Fails with a message, and leaves the design as it was.
fn fails(s: &mut Session, id: &str, params: Value) -> String {
    let before = s.design().clone();
    let e = s.execute(id, Some(&params)).expect_err(id).to_string();
    assert_eq!(s.design(), &before, "{id} changed the design");
    e
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64) -> u64 {
    run(
        s,
        "shape.rectangle",
        json!({ "x": x, "y": y, "width": w, "height": h }),
    )["id"]
        .as_u64()
        .expect("id")
}

fn bounds(s: &Session, id: u64) -> Rect {
    let d = s.design();
    d.objects[d.index_of(id).expect("object")]
        .bounds()
        .expect("bounds")
}

/// Three rectangles of different sizes, scattered.
fn three(s: &mut Session) -> [u64; 3] {
    [
        rect(s, 10.0, 10.0, 20.0, 10.0),
        rect(s, 45.0, 30.0, 10.0, 25.0),
        rect(s, 70.0, 5.0, 15.0, 15.0),
    ]
}

const EPS: f64 = 0.1; // 0.01 mm in model units

#[test]
fn align_every_edge_to_selection_key_and_page() {
    for edge in ["left", "right", "top", "bottom", "centerX", "centerY"] {
        let line = |b: Rect| match edge {
            "left" => b.min_x,
            "right" => b.max_x,
            "top" => b.min_y,
            "bottom" => b.max_y,
            "centerX" => b.center().x,
            _ => b.center().y,
        };
        for to in ["selection", "key", "page"] {
            let mut s = Session::new(std::env::temp_dir());
            let ids = three(&mut s);
            let before: Vec<Rect> = ids.iter().map(|&i| bounds(&s, i)).collect();
            let all = Rect::enclosing(
                before
                    .iter()
                    .flat_map(|b| [v2(b.min_x, b.min_y), v2(b.max_x, b.max_y)]),
            )
            .expect("bounds");
            let page = s.design().page;
            let page = Rect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: page.width,
                max_y: page.height,
            };
            let target = match to {
                "selection" => line(all),
                "key" => line(before[1]),
                _ => line(page),
            };
            let mut params = json!({ "ids": ids, "edge": edge, "to": to });
            if to == "key" {
                params["keyId"] = json!(ids[1]);
            }
            run(&mut s, "object.align", params);
            for (k, &id) in ids.iter().enumerate() {
                let b = bounds(&s, id);
                assert!(
                    (line(b) - target).abs() < EPS,
                    "{edge} to {to}: {} vs {target}",
                    line(b)
                );
                // Only the aligned axis moves, and sizes stay.
                let horizontal = matches!(edge, "left" | "right" | "centerX");
                let (a, c) = (before[k], b);
                if horizontal {
                    assert!((a.min_y - c.min_y).abs() < 1e-9, "{edge}");
                } else {
                    assert!((a.min_x - c.min_x).abs() < 1e-9, "{edge}");
                }
                assert!((a.width() - c.width()).abs() < 1e-9);
            }
            assert_eq!(
                bounds(&s, ids[1]) == before[1],
                to == "key" || line(before[1]) == target
            );
            // One undo step puts everything back.
            run(&mut s, "history.undo", json!({}));
            for (k, &id) in ids.iter().enumerate() {
                assert_eq!(bounds(&s, id), before[k], "undo {edge} {to}");
            }
        }
    }
}

#[test]
fn align_key_defaults_to_the_last_id_and_groups_move_whole() {
    let mut s = Session::new(std::env::temp_dir());
    let [a, b, c] = three(&mut s);
    run(&mut s, "object.group", json!({ "ids": [a, b] }));
    let (ba, bb) = (bounds(&s, a), bounds(&s, b));
    let key = bounds(&s, c);
    // Naming one member of the group brings the whole group.
    run(
        &mut s,
        "object.align",
        json!({ "ids": [a, c], "edge": "top", "to": "key" }),
    );
    assert_eq!(bounds(&s, c), key, "the key object stays put");
    let group_top = bounds(&s, a).min_y.min(bounds(&s, b).min_y);
    assert!((group_top - key.min_y).abs() < EPS);
    // The group kept its shape: both members moved by the same amount.
    let da = bounds(&s, a).min_y - ba.min_y;
    let db = bounds(&s, b).min_y - bb.min_y;
    assert!((da - db).abs() < 1e-9 && da.abs() > 1.0);
}

#[test]
fn align_and_distribute_refuse_below_the_minimum() {
    let mut s = Session::new(std::env::temp_dir());
    let [a, b, _] = three(&mut s);
    let e = fails(
        &mut s,
        "object.align",
        json!({ "ids": [a], "edge": "left" }),
    );
    assert!(e.contains("at least two"), "{e}");
    // One object to the page is fine.
    run(
        &mut s,
        "object.align",
        json!({ "ids": [a], "edge": "left", "to": "page" }),
    );
    assert!(bounds(&s, a).min_x.abs() < EPS);
    let e = fails(
        &mut s,
        "object.distribute",
        json!({ "ids": [a, b], "axis": "x" }),
    );
    assert!(e.contains("at least three"), "{e}");
    fails(
        &mut s,
        "object.align",
        json!({ "ids": [a, b], "edge": "middle" }),
    );
    fails(
        &mut s,
        "object.align",
        json!({ "ids": [a, b], "edge": "left", "to": "hoop" }),
    );
    fails(
        &mut s,
        "object.align",
        json!({ "ids": [a], "edge": "left", "to": "key", "keyId": b }),
    );
    fails(
        &mut s,
        "object.distribute",
        json!({ "ids": [1, 2, 3], "axis": "z" }),
    );
}

#[test]
fn distribute_gives_equal_gaps_or_centre_spacing_and_keeps_the_ends() {
    for n in 3..=10 {
        for by in ["gaps", "centers"] {
            for axis in ["x", "y"] {
                let mut s = Session::new(std::env::temp_dir());
                let ids: Vec<u64> = (0..n)
                    .map(|k| {
                        let k = k as f64;
                        let (w, h) = (5.0 + (k * 7.0) % 9.0, 4.0 + (k * 5.0) % 7.0);
                        let at = 3.0 + k * 9.0 + (k * k) % 5.0;
                        if axis == "x" {
                            rect(&mut s, at, 20.0 + k, w, h)
                        } else {
                            rect(&mut s, 20.0 + k, at, w, h)
                        }
                    })
                    .collect();
                let before: Vec<Rect> = ids.iter().map(|&i| bounds(&s, i)).collect();
                run(
                    &mut s,
                    "object.distribute",
                    json!({ "ids": ids, "axis": axis, "by": by }),
                );
                let after: Vec<Rect> = ids.iter().map(|&i| bounds(&s, i)).collect();
                assert_eq!(after[0], before[0], "first stays");
                assert_eq!(after[n - 1], before[n - 1], "last stays");
                let span = |b: &Rect| {
                    if axis == "x" {
                        (b.min_x, b.max_x)
                    } else {
                        (b.min_y, b.max_y)
                    }
                };
                let steps: Vec<f64> = after
                    .windows(2)
                    .map(|w| {
                        let (a, b) = (span(&w[0]), span(&w[1]));
                        if by == "gaps" {
                            b.0 - a.1
                        } else {
                            (b.0 + b.1 - a.0 - a.1) / 2.0
                        }
                    })
                    .collect();
                for st in &steps {
                    assert!((st - steps[0]).abs() < EPS, "{n} {by} {axis}: {steps:?}");
                }
                // The other axis never moves.
                for (a, b) in before.iter().zip(&after) {
                    let other = |r: &Rect| if axis == "x" { r.min_y } else { r.min_x };
                    assert_eq!(other(a), other(b));
                }
            }
        }
    }
}

#[test]
fn align_and_distribute_only_translate_like_object_move() {
    let mut s = Session::new(std::env::temp_dir());
    let ids = three(&mut s);
    run(
        &mut s,
        "object.setRegion",
        json!({ "ids": ids, "type": "satin" }),
    );
    let before = s.design().clone();
    run(
        &mut s,
        "object.align",
        json!({ "ids": ids, "edge": "bottom" }),
    );
    run(
        &mut s,
        "object.distribute",
        json!({ "ids": ids, "axis": "x", "by": "gaps" }),
    );
    for (k, &id) in ids.iter().enumerate() {
        let (b0, b1) = (before.objects[k].bounds().expect("b"), bounds(&s, id));
        // The same object, attributes and all, moved by the delta: what object.move gives.
        let mut m = Session::new(std::env::temp_dir());
        three(&mut m);
        run(
            &mut m,
            "object.setRegion",
            json!({ "ids": ids, "type": "satin" }),
        );
        run(
            &mut m,
            "object.move",
            json!({ "id": id, "dx": (b1.min_x - b0.min_x) / 10.0, "dy": (b1.min_y - b0.min_y) / 10.0 }),
        );
        let (a, e) = (&s.design().objects[k], &m.design().objects[k]);
        assert_eq!(a.region, e.region, "sewing attributes unchanged");
        for (p, q) in a.contours[0].points.iter().zip(&e.contours[0].points) {
            assert!(p.distance(*q) < 1e-6, "{p:?} {q:?}");
        }
    }
}

#[test]
fn skew_leans_the_outline_and_turns_the_fill_direction() {
    let mut s = Session::new(std::env::temp_dir());
    let id = rect(&mut s, 10.0, 10.0, 20.0, 20.0);
    run(
        &mut s,
        "object.setRegion",
        json!({ "id": id, "type": "fill", "angle": 0 }),
    );
    let before = s.design().objects[0].clone();
    run(&mut s, "object.skew", json!({ "id": id, "angleY": 20 }));
    let o = &s.design().objects[0];
    let b = o.bounds().expect("bounds");
    // Horizontal edges now slant: the right side moved down by 200·tan 20°.
    let rise = 200.0 * 20f64.to_radians().tan();
    assert!((b.height() - (200.0 + rise)).abs() < 0.5, "{b:?}");
    assert!((b.width() - 200.0).abs() < 1e-6);
    let angle = o.region.as_ref().expect("region").angle;
    assert!(
        (angle - 340.0).abs() < 0.01,
        "rows lean with the skew: {angle}"
    );
    // angleX keeps horizontal rows horizontal but leans vertical ones.
    run(&mut s, "object.setRegion", json!({ "id": id, "angle": 90 }));
    run(
        &mut s,
        "object.skew",
        json!({ "id": id, "angleX": 20, "originX": 0, "originY": 0 }),
    );
    let angle = s.design().objects[0].region.as_ref().expect("region").angle;
    assert!((angle - 110.0).abs() < 0.01, "{angle}");
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects[0], before, "one undo step each");
    fails(&mut s, "object.skew", json!({ "id": id, "angleX": 85 }));
}

#[test]
fn skew_transforms_lines_columns_and_groups() {
    let mut s = Session::new(std::env::temp_dir());
    let line = run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[0, 0], [10, 0], [10, 10]], "closed": false }),
    )["id"]
        .as_u64()
        .expect("id");
    let col = run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[20, 0], [24, 0]], [[20, 10], [24, 10]]] }),
    )["id"]
        .as_u64()
        .expect("id");
    run(&mut s, "object.group", json!({ "ids": [line, col] }));
    run(
        &mut s,
        "object.skew",
        json!({ "ids": [line, col], "angleX": 45, "originX": 0, "originY": 0 }),
    );
    let d = s.design();
    let pts = d.objects[0].contours[0].points.clone();
    assert!(
        (pts[2].x - 200.0).abs() < 1e-6 && (pts[2].y - 100.0).abs() < 1e-6,
        "{pts:?}"
    );
    let c = &d.objects[1];
    let r = c.region.as_ref().expect("column region");
    if let Some(l) = r.direction_lines.get(1) {
        assert!((l[0].x - 300.0).abs() < 1e-6, "direction lines lean: {l:?}");
    }
    assert!(Compiler::for_design(d).compile(d).stitches.len() > 10);
}

#[test]
fn list_filters_select_same_colour_and_type() {
    let mut s = Session::new(std::env::temp_dir());
    let a = rect(&mut s, 0.0, 0.0, 10.0, 10.0);
    let b = rect(&mut s, 20.0, 0.0, 10.0, 10.0);
    let c = rect(&mut s, 40.0, 0.0, 10.0, 10.0);
    let d = rect(&mut s, 60.0, 0.0, 10.0, 10.0);
    run(
        &mut s,
        "object.setRegion",
        json!({ "ids": [a, b], "type": "satin", "color": "#ff0000" }),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({ "id": c, "type": "fill", "color": "#ff0000" }),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({ "id": d, "type": "satin", "color": "#00ff00" }),
    );
    let line = run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[0, 30], [10, 30]], "closed": false, "outline": "#ff0000" }),
    )["id"]
        .as_u64()
        .expect("id");
    let ids = |v: Value| -> Vec<u64> {
        v.as_array()
            .expect("list")
            .iter()
            .map(|o| o["id"].as_u64().expect("id"))
            .collect()
    };
    assert_eq!(
        ids(run(&mut s, "object.list", json!({}))).len(),
        5,
        "no filter: all"
    );
    assert_eq!(
        ids(run(&mut s, "object.list", json!({ "color": "#ff0000" }))),
        vec![a, b, c, line]
    );
    assert_eq!(
        ids(run(&mut s, "object.list", json!({ "regionType": "satin" }))),
        vec![a, b, d]
    );
    let running = s.design().objects[4].line.as_ref().expect("line").kind.id();
    assert_eq!(
        ids(run(&mut s, "object.list", json!({ "lineType": running }))),
        vec![line]
    );
    assert_eq!(
        ids(run(
            &mut s,
            "object.list",
            json!({ "sameAs": a, "match": "type" })
        )),
        vec![a, b, d]
    );
    assert_eq!(
        ids(run(
            &mut s,
            "object.list",
            json!({ "sameAs": [a], "match": "both" })
        )),
        vec![a, b]
    );
    // Hidden objects are skipped by Select Same.
    run(
        &mut s,
        "object.setVisible",
        json!({ "id": b, "visible": false }),
    );
    assert_eq!(
        ids(run(&mut s, "object.list", json!({ "sameAs": a }))),
        vec![a, c, line]
    );
    assert_eq!(
        ids(run(&mut s, "object.list", json!({ "visible": false }))),
        vec![b]
    );
    // The engine function the editor's menu uses gives the same ids.
    use openstitch_engine::layout::{Same, select_same};
    assert_eq!(
        select_same(s.design(), &[a], Same::Colour),
        vec![a, c, line]
    );
    assert!(
        s.execute("object.list", Some(&json!({ "sameAs": 99 })))
            .is_err()
    );
    assert!(
        s.execute("object.list", Some(&json!({ "sameAs": a, "match": "x" })))
            .is_err()
    );
}

#[test]
fn colour_at_reads_the_block_under_the_pointer() {
    use openstitch_design::{DesignObject, StitchBlock};
    let st = |x: f64, c: Command| openstitch_core::Stitch {
        x,
        y: 0.0,
        command: c,
    };
    let o = DesignObject::stitches(StitchBlock {
        stitches: vec![
            st(0.0, Command::Stitch),
            st(10.0, Command::Stitch),
            st(10.0, Command::ColorChange),
            st(100.0, Command::Stitch),
        ],
        threads: vec![
            openstitch_core::Thread::rgb(0xff0000),
            openstitch_core::Thread::rgb(0x0000ff),
        ],
    });
    use openstitch_engine::layout::colour_at;
    assert_eq!(colour_at(&o, v2(2.0, 1.0)).as_deref(), Some("#ff0000"));
    assert_eq!(colour_at(&o, v2(90.0, 1.0)).as_deref(), Some("#0000ff"));
}
