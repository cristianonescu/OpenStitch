//! Arrange Copies along a line, scattered inside a shape and as a kaleidoscope (#476).

use openstitch_core::Command;
use openstitch_design::{Compiler, Vec2, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

fn centre(s: &Session, id: u64) -> Vec2 {
    let d = s.design();
    d.objects[d.index_of(id).expect("object")]
        .bounds()
        .expect("bounds")
        .center()
}

fn copies(v: &Value) -> Vec<u64> {
    v["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .map(|x| x.as_u64().expect("id"))
        .collect()
}

/// Signed area of an object's first contour: its sign tells mirrored copies apart.
fn signed_area(s: &Session, id: u64) -> f64 {
    let d = s.design();
    let p = &d.objects[d.index_of(id).expect("object")].contours[0].points;
    (0..p.len())
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % p.len()]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

fn colour_changes(s: &Session) -> usize {
    let d = s.design();
    Compiler::for_design(d)
        .compile(d)
        .stitches
        .iter()
        .filter(|x| x.command == Command::ColorChange)
        .count()
}

fn triangle(s: &mut Session, x: f64, y: f64) -> u64 {
    // Asymmetric: mirroring it flips the sign of its area.
    id(&run(
        s,
        "shape.polygon",
        json!({ "points": [[x, y], [x + 6.0, y], [x, y + 3.0]] }),
    ))
}

#[test]
fn path_copies_are_evenly_spaced_by_arc_length_and_follow_the_tangent() {
    let mut s = Session::new(std::env::temp_dir());
    let guide = id(&run(
        &mut s,
        "shape.ellipse",
        json!({ "cx": 50, "cy": 50, "rx": 30, "outline": "Red" }),
    ));
    let motif = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 10, "height": 2 }),
    ));
    let before = s.design().clone();
    let r = run(
        &mut s,
        "object.arrangeCopies",
        json!({ "ids": [motif, guide], "mode": "path", "pathId": guide, "count": 12 }),
    );
    let ids = copies(&r);
    assert_eq!(ids.len(), 12);
    assert_eq!(r["placed"], json!(12));
    // Equal steps on a circle: equal chords between neighbours (to 0.05 mm), all on it.
    let cs: Vec<Vec2> = ids.iter().map(|&i| centre(&s, i)).collect();
    let chord = |a: Vec2, b: Vec2| a.distance(b);
    for k in 0..12 {
        let c = cs[k];
        assert!((c.distance(v2(500.0, 500.0)) - 300.0).abs() < 1.0, "{c:?}");
        let step = chord(c, cs[(k + 1) % 12]);
        assert!((step - chord(cs[0], cs[1])).abs() < 0.5, "step {k}: {step}");
    }
    // Follow: each copy's long side runs along the tangent (perpendicular to the radius).
    for &i in &ids {
        let d = s.design();
        let p = &d.objects[d.index_of(i).expect("copy")].contours[0].points;
        let long = if p[0].distance(p[1]) > p[1].distance(p[2]) {
            p[1] - p[0]
        } else {
            p[2] - p[1]
        };
        let radius = centre(&s, i) - v2(500.0, 500.0);
        let cos = (long.x * radius.x + long.y * radius.y) / (long.length() * radius.length());
        // Within about 1°: the circle is flattened to 0.05 mm, so the tangent is that of
        // a polygon with ~3.5 mm sides, taken over ±1 mm of it.
        assert!(cos.abs() < 0.02, "long side along the tangent: {cos}");
    }
    // The guide sews nothing now and stays an editable shape.
    let g = &s.design().objects[s.design().index_of(guide).expect("guide")];
    assert!(g.region.is_none() && g.line.is_none() && !g.contours.is_empty());
    // One undo step.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before);
}

#[test]
fn open_paths_include_both_ends_and_sides_and_mirroring_alternate() {
    let mut s = Session::new(std::env::temp_dir());
    let guide = id(&run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[10, 50], [110, 50]], "closed": false }),
    ));
    let motif = triangle(&mut s, 0.0, 0.0);
    let r = run(
        &mut s,
        "object.arrangeCopies",
        json!({ "ids": [motif], "mode": "path", "pathId": guide, "count": 5,
                "side": "alternate", "offset": 4, "mirrorAlternate": true, "orient": "upright" }),
    );
    let ids = copies(&r);
    let area0 = signed_area(&s, motif);
    for (k, &i) in ids.iter().enumerate() {
        let c = centre(&s, i);
        assert!((c.x - (100.0 + 250.0 * k as f64)).abs() < 0.5, "{k}: {c:?}");
        // Above (smaller y) on even copies, below on odd ones, 4 mm from the line.
        let want = if k % 2 == 0 { 460.0 } else { 540.0 };
        assert!((c.y - want).abs() < 0.5, "{k}: {c:?}");
        let mirrored = signed_area(&s, i).signum() != area0.signum();
        assert_eq!(mirrored, k % 2 == 1, "copy {k}");
    }
    // A bad guide or option is refused and changes nothing.
    let before = s.design().clone();
    for bad in [
        json!({ "ids": [motif], "mode": "path", "pathId": 999 }),
        json!({ "ids": [motif], "mode": "path", "pathId": guide, "side": "left" }),
        json!({ "ids": [guide], "mode": "path", "pathId": guide }),
        json!({ "ids": [motif], "mode": "path", "pathId": guide, "count": 201 }),
    ] {
        assert!(
            s.execute("object.arrangeCopies", Some(&bad)).is_err(),
            "{bad}"
        );
        assert_eq!(s.design(), &before);
    }
}

#[test]
fn scatter_is_deterministic_inside_and_keeps_the_gap() {
    let place = |seed: u64| {
        let mut s = Session::new(std::env::temp_dir());
        let container = id(&run(
            &mut s,
            "shape.ellipse",
            json!({ "cx": 50, "cy": 50, "rx": 40, "outline": "Black" }),
        ));
        let dot = id(&run(
            &mut s,
            "shape.ellipse",
            json!({ "cx": 5, "cy": 5, "rx": 3 }),
        ));
        let r = run(
            &mut s,
            "object.arrangeCopies",
            json!({ "ids": [dot], "mode": "scatter", "shapeId": container, "count": 40,
                    "gap": 1.5, "sizeMin": 0.8, "sizeMax": 1.2, "randomRotate": true, "seed": seed }),
        );
        (s, r, dot)
    };
    let (s, r, dot) = place(7);
    let ids = copies(&r);
    assert!(ids.len() >= 10, "fitted {}", ids.len());
    assert_eq!(r["placed"], json!(ids.len()), "reports how many fitted");
    assert!(
        s.design().index_of(dot).is_none(),
        "the copies replace the motif"
    );
    let (again, r2, _) = place(7);
    assert_eq!(r, r2);
    assert_eq!(s.design(), again.design(), "same seed, same layout");
    let (other, _, _) = place(8);
    assert_ne!(s.design(), other.design(), "another seed, another layout");
    let d = s.design();
    let outline = |i: u64| d.objects[d.index_of(i).expect("copy")].contours.clone();
    for (n, &a) in ids.iter().enumerate() {
        for q in outline(a).iter().flat_map(|c| c.outline_points()) {
            assert!(q.distance(v2(500.0, 500.0)) <= 400.0 + 0.5, "inside: {q:?}");
        }
        for &b in &ids[n + 1..] {
            let ra = outline(a);
            let rb = outline(b);
            let gap = ra
                .iter()
                .flat_map(|c| c.outline_points())
                .flat_map(|p| {
                    rb.iter()
                        .flat_map(|c| c.outline_points())
                        .map(move |q| p.distance(q))
                })
                .fold(f64::INFINITY, f64::min);
            // Vertex-to-vertex distance is never below the edge distance checked (1.5 mm).
            assert!(gap >= 15.0 - 0.01, "copies {a} and {b}: {gap}");
        }
    }
    let mut bad = Session::new(std::env::temp_dir());
    let line = id(&run(
        &mut bad,
        "shape.polygon",
        json!({ "points": [[0, 0], [10, 0]], "closed": false }),
    ));
    let m = triangle(&mut bad, 20.0, 20.0);
    assert!(
        bad.execute(
            "object.arrangeCopies",
            Some(&json!({ "ids": [m], "mode": "scatter", "shapeId": line }))
        )
        .is_err(),
        "an open line holds nothing"
    );
}

#[test]
fn kaleidoscope_mirrors_every_second_copy_about_its_radial_axis() {
    let mut s = Session::new(std::env::temp_dir());
    let motif = triangle(&mut s, 47.0, 20.0);
    let area0 = signed_area(&s, motif);
    let c0 = centre(&s, motif);
    let r = run(
        &mut s,
        "object.arrangeCopies",
        json!({ "ids": [motif], "mode": "circle", "count": 6, "cx": 50, "cy": 50,
                "radius": 30, "mirrorAlternate": true }),
    );
    let ids = copies(&r);
    assert_eq!(ids.len(), 5);
    let original = s.design().objects[0].contours[0].points.clone();
    for (k, &i) in ids.iter().enumerate() {
        let k = k + 1;
        let mirrored = signed_area(&s, i).signum() != area0.signum();
        assert_eq!(mirrored, k % 2 == 1, "copy {k}");
        // Turned back about the circle centre and, for odd copies, mirrored back about the
        // radial axis (the vertical through the centre), each copy is the original.
        let d = s.design();
        let back = openstitch_design::Transform::rotation(-60.0 * k as f64, v2(500.0, 500.0));
        for (p, q) in d.objects[d.index_of(i).expect("copy")].contours[0]
            .points
            .iter()
            .zip(&original)
        {
            let mut b = back.apply(*p);
            if k % 2 == 1 {
                b.x = 2.0 * c0.x - b.x;
            }
            assert!(b.distance(*q) < 1e-6, "copy {k}: {b:?} vs {q:?}");
        }
    }
    // Without the option the circle is unchanged: no copy is mirrored.
    let mut plain = Session::new(std::env::temp_dir());
    let m = triangle(&mut plain, 47.0, 20.0);
    let r = run(
        &mut plain,
        "object.arrangeCopies",
        json!({ "ids": [m], "mode": "circle", "count": 6, "cx": 50, "cy": 50, "radius": 30 }),
    );
    for i in copies(&r) {
        assert_eq!(signed_area(&plain, i).signum(), area0.signum());
    }
}

#[test]
fn copies_are_sewn_colour_by_colour() {
    let mut s = Session::new(std::env::temp_dir());
    let guide = id(&run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[10, 80], [90, 80]], "closed": false }),
    ));
    let red = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 4, "height": 4, "fill": "Red" }),
    ));
    let blue = id(&run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 4, "y": 0, "width": 4, "height": 4, "fill": "Blue" }),
    ));
    run(
        &mut s,
        "object.setLine",
        json!({ "id": guide, "type": "none" }),
    );
    let source = colour_changes(&s);
    assert_eq!(source, 1, "red then blue");
    run(
        &mut s,
        "object.arrangeCopies",
        json!({ "ids": [red, blue], "mode": "path", "pathId": guide, "count": 6 }),
    );
    assert_eq!(
        colour_changes(&s),
        source,
        "path copies sewn colour by colour"
    );
}
