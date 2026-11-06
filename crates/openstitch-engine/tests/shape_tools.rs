//! More shape tools (#497): per-corner radii, frame and spiral presets, Divide and shape
//! effects.

use openstitch_core::Command;
use openstitch_design::boolean::{BoolOp, combine, region_area, simplify};
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

fn object(s: &Session, id: u64) -> &DesignObject {
    let d = s.design();
    &d.objects[d.index_of(id).expect("object")]
}

fn session() -> Session {
    Session::new(std::env::temp_dir())
}

fn stitches(s: &Session) -> Vec<Vec2> {
    let d = s.design();
    Compiler::for_design(d)
        .compile(d)
        .stitches
        .iter()
        .filter(|x| x.command == Command::Stitch)
        .map(|x| v2(x.x, x.y))
        .collect()
}

fn distance_to(p: Vec2, path: &[Vec2]) -> f64 {
    path.windows(2)
        .map(|w| {
            let d = w[1] - w[0];
            let l2 = d.x * d.x + d.y * d.y;
            let t = if l2 == 0.0 {
                0.0
            } else {
                (((p.x - w[0].x) * d.x + (p.y - w[0].y) * d.y) / l2).clamp(0.0, 1.0)
            };
            p.distance(w[0] + d * t)
        })
        .fold(f64::INFINITY, f64::min)
}

#[test]
fn frames_leave_the_middle_open_with_the_band_width() {
    for of in ["rectangle", "ellipse", "roundedRectangle", "heart"] {
        let mut s = session();
        let f = id(&run(
            &mut s,
            "shape.preset",
            json!({ "kind": "frame", "frameOf": of, "cx": 50, "cy": 50, "width": 60, "height": 40, "bandWidth": 4 }),
        ));
        let o = object(&s, f);
        assert_eq!(o.contours.len(), 2, "{of}: outer and inner");
        assert!(o.region.is_some());
        let (outer, inner) = (o.contours[0].path(), o.contours[1].path());
        for p in o.contours[1].outline_points() {
            let d = distance_to(p, &outer);
            assert!(
                (d - 40.0).abs() <= 0.5,
                "{of}: inner point {d} from the outer outline"
            );
        }
        // The band's middle is sewn, the open middle is not.
        let st = stitches(&s);
        assert!(st.len() > 50, "{of}");
        let middle = v2(500.0, 500.0);
        assert!(
            st.iter().all(|p| p.distance(middle) > 30.0),
            "{of}: nothing in the middle"
        );
        assert!(inner.len() > 3);
    }
    let mut s = session();
    let e = s.execute(
        "shape.preset",
        Some(&json!({ "kind": "frame", "cx": 50, "cy": 50, "width": 10, "bandWidth": 6 })),
    );
    assert!(e.is_err(), "a band wider than the shape is refused");
}

#[test]
fn rectangles_take_a_radius_per_corner() {
    let mut s = session();
    let r = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 40, "height": 20, "cornerRadii": [0, 2, 4, 6] }),
    ));
    let c = &object(&s, r).contours[0];
    assert!(
        c.points.contains(&v2(100.0, 100.0)),
        "sharp top-left corner"
    );
    assert!(c.points.contains(&v2(480.0, 100.0)) && c.points.contains(&v2(500.0, 120.0)));
    assert!(c.points.contains(&v2(500.0, 260.0)) && c.points.contains(&v2(460.0, 300.0)));
    assert!(c.points.contains(&v2(160.0, 300.0)) && c.points.contains(&v2(100.0, 240.0)));
    assert!(
        s.execute(
            "shape.rectangle",
            Some(&json!({ "x": 0, "y": 0, "width": 4, "height": 4, "cornerRadii": [1, 1, 1] }))
        )
        .is_err()
    );
    assert!(s
        .execute("shape.rectangle", Some(&json!({ "x": 0, "y": 0, "width": 4, "height": 4, "cornerRadius": 1, "cornerRadii": [1, 1, 1, 1] })))
        .is_err());
}

#[test]
fn a_spiral_line_sews_with_every_line_type() {
    let mut s = session();
    let sp = id(&run(
        &mut s,
        "shape.preset",
        json!({ "kind": "spiral", "cx": 50, "cy": 50, "width": 40, "turns": 4, "innerRadius": 2 }),
    ));
    let c = object(&s, sp).contours[0].clone();
    assert!(!c.closed);
    assert!((c.points[0].distance(v2(500.0, 500.0)) - 20.0).abs() < 1e-6);
    assert!((c.points.last().expect("end").distance(v2(500.0, 500.0)) - 200.0).abs() < 1e-6);
    assert!(object(&s, sp).region.is_none() && object(&s, sp).line.is_some());
    for t in [
        "running",
        "triple",
        "zigzag",
        "motif",
        "stem",
        "candlewick",
        "eStitch",
        "vStitch",
        "chain",
    ] {
        run(&mut s, "object.setLine", json!({ "ids": [sp], "type": t }));
        assert!(stitches(&s).len() > 20, "{t}");
    }
    assert!(
        s.execute(
            "shape.preset",
            Some(&json!({ "kind": "spiral", "cx": 0, "cy": 0, "width": 10, "innerRadius": 6 }))
        )
        .is_err()
    );
}

#[test]
fn divide_cuts_overlaps_into_their_own_shapes() {
    let mut s = session();
    let a = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 20, "height": 20, "fill": "Red" }),
    ));
    let b = id(&run(
        &mut s,
        "shape.ellipse",
        json!({ "cx": 20, "cy": 20, "rx": 10, "fill": "Blue" }),
    ));
    let blue = object(&s, b).region.clone().expect("region").thread;
    let union: f64 = openstitch_design::boolean::union_all([
        &object(&s, a).contours[..],
        &object(&s, b).contours[..],
    ])
    .iter()
    .map(|r| region_area(r))
    .sum();
    let r = run(
        &mut s,
        "shape.combine",
        json!({ "ids": [a, b], "op": "divide" }),
    );
    assert_eq!(r["pieces"], 3);
    let objs = &s.design().objects;
    assert_eq!(objs.len(), 3);
    let total: f64 = objs
        .iter()
        .map(|o| {
            simplify(&o.contours)
                .iter()
                .map(|x| region_area(x))
                .sum::<f64>()
        })
        .sum();
    assert!((total - union).abs() < union * 0.001, "{total} vs {union}");
    for i in 0..objs.len() {
        for j in i + 1..objs.len() {
            assert!(combine(&objs[i].contours, &objs[j].contours, BoolOp::Intersection).is_empty());
        }
    }
    // The overlap is sewn like the topmost shape: two pieces in its thread.
    let in_blue = objs
        .iter()
        .filter(|o| o.region.as_ref().is_some_and(|r| r.thread == blue))
        .count();
    assert_eq!(in_blue, 2);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 2);
    // Other combine modes are unchanged.
    run(
        &mut s,
        "shape.combine",
        json!({ "ids": [a, b], "op": "union" }),
    );
    assert_eq!(s.design().objects.len(), 1);
}

#[test]
fn shape_effects_are_deterministic_and_one_undo_each() {
    let make = || {
        let mut s = session();
        let r = id(&run(
            &mut s,
            "shape.rectangle",
            json!({ "x": 10, "y": 10, "width": 30, "height": 30 }),
        ));
        (s, r)
    };
    for kind in ["warp", "twist", "roughen", "puckerBloat", "zigzag"] {
        let (mut s1, r1) = make();
        let (mut s2, r2) = make();
        let before = object(&s1, r1).contours.clone();
        run(
            &mut s1,
            "shape.effect",
            json!({ "ids": [r1], "kind": kind, "seed": 3 }),
        );
        run(
            &mut s2,
            "shape.effect",
            json!({ "ids": [r2], "kind": kind, "seed": 3 }),
        );
        assert_eq!(object(&s1, r1).contours, object(&s2, r2).contours, "{kind}");
        assert_ne!(object(&s1, r1).contours, before, "{kind}");
        assert!(stitches(&s1).len() > 50, "{kind} sews");
        run(&mut s1, "history.undo", json!({}));
        assert_eq!(object(&s1, r1).contours, before, "{kind}: one undo");
    }
    let (mut s, r) = make();
    assert!(
        s.execute("shape.effect", Some(&json!({ "ids": [r], "kind": "melt" })))
            .is_err()
    );
    let c = id(&run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[0, 0], [0, 4]], [[10, 0], [10, 4]]] }),
    ));
    assert!(
        s.execute("shape.effect", Some(&json!({ "ids": [c], "kind": "warp" })))
            .is_err()
    );
}
