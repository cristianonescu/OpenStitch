//! #652: every shape cut keeps the cut shape's sewing angle and its other sewing attributes
//! (region and line sewing, colour), and its entry and exit points where they are still on the
//! new outline. The fills are set to 135°, not the 45° default, so a reset shows.

use openstitch_design::{DesignObject, RegionSewing, Vec2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

/// A red 20 mm square at the origin, set to 135°, density 5, stitch length 3 mm, a running
/// outline, region entry at its top-left corner and region exit at its bottom-right corner;
/// a blue 20 mm square over its bottom-right quarter, sewn after it, at 30°.
fn two_squares() -> (Session, u64, u64) {
    let mut s = Session::new(std::env::temp_dir());
    let a = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 20, "height": 20, "fill": "Red"}),
    ));
    let b = id(&run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20, "fill": "Blue"}),
    ));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": a, "angle": 135, "density": 5, "stitchLength": 3}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"ids": [a], "type": "running"}),
    );
    run(
        &mut s,
        "object.setEntryExit",
        json!({"id": a, "regionEntry": [0, 0], "regionExit": [20, 20]}),
    );
    run(&mut s, "object.setRegion", json!({"id": b, "angle": 30}));
    (s, a, b)
}

fn object(s: &Session, id: u64) -> DesignObject {
    let d = s.design();
    d.objects[d.index_of(id).expect("object")].clone()
}

/// The square's region attributes, as set in `two_squares`.
fn assert_kept(r: &RegionSewing, red: &RegionSewing, what: &str) {
    assert_eq!(r.angle, 135.0, "{what}: angle");
    assert_eq!(r.kind, red.kind, "{what}: type");
    assert_eq!(r.thread, red.thread, "{what}: colour");
    assert_eq!(r.density, red.density, "{what}: density");
    assert_eq!(r.stitch_length, red.stitch_length, "{what}: stitch length");
}

/// The red square's pieces: every object sewn in its colour.
fn red_pieces(s: &Session, red: &RegionSewing) -> Vec<DesignObject> {
    s.design()
        .objects
        .iter()
        .filter(|o| o.region.as_ref().is_some_and(|r| r.thread == red.thread))
        .cloned()
        .collect()
}

/// One cut operation, given the session and the red and blue squares' ids.
type Cut = Box<dyn Fn(&mut Session, u64, u64)>;

#[test]
fn every_cut_keeps_the_sewing_angle_and_attributes() {
    let origin = Vec2 { x: 0.0, y: 0.0 };
    let cases: Vec<(&str, Cut)> = vec![
        (
            "shape.removeOverlap",
            Box::new(|s, a, b| {
                run(s, "shape.removeOverlap", json!({"ids": [a, b]}));
            }),
        ),
        (
            "shape.combine difference",
            Box::new(|s, a, b| {
                run(
                    s,
                    "shape.combine",
                    json!({"ids": [a, b], "op": "difference", "keepOperands": true}),
                );
            }),
        ),
        (
            "shape.combine divide",
            Box::new(|s, a, b| {
                run(s, "shape.combine", json!({"ids": [a, b], "op": "divide"}));
            }),
        ),
        (
            "shape.split",
            Box::new(|s, a, _| {
                run(
                    s,
                    "shape.split",
                    json!({"id": a, "points": [[5, -5], [25, -5], [25, 25], [5, 25]]}),
                );
            }),
        ),
        (
            "shape.knife",
            Box::new(|s, _, _| {
                run(s, "shape.knife", json!({"line": [[5, -5], [5, 35]]}));
            }),
        ),
        (
            "shape.build remove",
            Box::new(|s, a, b| {
                run(
                    s,
                    "shape.build",
                    json!({"ids": [a, b], "path": [[15, 15]], "mode": "remove"}),
                );
            }),
        ),
        (
            "shape.effect",
            Box::new(|s, a, _| {
                run(
                    s,
                    "shape.effect",
                    json!({"ids": [a], "kind": "roughen", "seed": 3}),
                );
            }),
        ),
    ];
    for (what, cut) in cases {
        let (mut s, a, b) = two_squares();
        let before = object(&s, a);
        let red = before.region.clone().expect("region");
        cut(&mut s, a, b);
        let pieces = red_pieces(&s, &red);
        assert!(!pieces.is_empty(), "{what}: no piece left");
        for p in &pieces {
            assert_kept(p.region.as_ref().expect("region"), &red, what);
            let no_points = |l: &Option<openstitch_design::LineSewing>| {
                l.clone().map(|mut l| {
                    l.entry = None;
                    l.exit = None;
                    l
                })
            };
            assert_eq!(
                no_points(&p.line),
                no_points(&before.line),
                "{what}: line sewing"
            );
        }
        // The top-left corner is on the outline of the piece that keeps it: its entry point
        // stays there (the effect moves the outline, so it is left out of this check).
        if what != "shape.effect" {
            let corner = pieces
                .iter()
                .filter_map(|p| p.region.as_ref().and_then(|r| r.entry))
                .collect::<Vec<_>>();
            assert_eq!(corner, vec![origin], "{what}: entry kept");
        }
    }
}

#[test]
fn a_cut_away_exit_point_returns_to_the_default() {
    let (mut s, a, b) = two_squares();
    run(&mut s, "shape.removeOverlap", json!({"ids": [a, b]}));
    let r = object(&s, a).region.expect("region");
    assert_eq!(r.entry, Some(Vec2 { x: 0.0, y: 0.0 }));
    // The bottom-right corner is under the blue square now, off the red outline.
    assert_eq!(r.exit, None);
}

/// Merging takes the topmost parent's attributes (the shape sewn last), as `shape.merge` always
/// has: here the blue square's 30°.
#[test]
fn merges_take_the_topmost_parents_angle() {
    let (mut s, a, b) = two_squares();
    run(&mut s, "shape.merge", json!({"ids": [a, b]}));
    let r = object(&s, b).region.expect("region");
    assert_eq!(r.angle, 30.0);
    let (mut s, a, b) = two_squares();
    // The shape builder: a drag from the red square's own part into the overlap.
    run(
        &mut s,
        "shape.build",
        json!({"ids": [a, b], "path": [[5, 5], [15, 15]]}),
    );
    let merged = s
        .design()
        .objects
        .iter()
        .find(|o| o.region.as_ref().is_some_and(|r| r.angle == 30.0))
        .cloned()
        .expect("the merged piece, sewn like the blue square");
    assert_eq!(merged.region.expect("region").thread, object_thread_blue());
    // Turned the other way round: the red square sewn last gives its 135°.
    let (mut s, a, b) = two_squares();
    run(&mut s, "order.move", json!({"ids": [a], "to": "last"}));
    run(&mut s, "shape.merge", json!({"ids": [a, b]}));
    let top = s.design().objects.last().cloned().expect("object");
    assert_eq!(top.region.expect("region").angle, 135.0);
}

fn object_thread_blue() -> openstitch_core::Thread {
    let (s, _, b) = two_squares();
    object(&s, b).region.expect("region").thread
}
