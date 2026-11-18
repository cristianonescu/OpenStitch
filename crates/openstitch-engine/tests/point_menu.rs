//! The Select Point tool's menu as commands (#1258): `object.setPointKind`,
//! `object.splitAtPoint`, `object.deletePoint` with several points, and the handle rule of
//! `object.setSegment` on smooth and symmetric points.

use openstitch_design::{Compiler, Contour, DesignObject, PointKind, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fails(s: &mut Session, id: &str, params: Value) -> String {
    match s.execute(id, Some(&params)) {
        Ok(v) => panic!("{id} should fail, gave {v}"),
        Err(e) => e.to_string(),
    }
}

fn undo(s: &mut Session) {
    s.execute("history.undo", None).expect("undo");
}

fn object(s: &Session, id: u64) -> &DesignObject {
    let d = s.design();
    &d.objects[d.index_of(id).expect("object")]
}

fn contour(s: &Session, id: u64) -> &Contour {
    &object(s, id).contours[0]
}

/// A 40 × 40 mm square sewn as a zigzag line only (the issue's example), id returned.
fn square(s: &mut Session) -> u64 {
    run(
        s,
        "shape.polygon",
        json!({ "points": [[0, 0], [40, 0], [40, 40], [0, 40]], "closed": true, "outline": "#c00000" }),
    )["id"]
        .as_u64()
        .expect("id")
}

fn open_line(s: &mut Session) -> u64 {
    run(
        s,
        "shape.polygon",
        json!({ "points": [[0, 0], [30, 0], [30, 30]], "closed": false, "outline": "#203040" }),
    )["id"]
        .as_u64()
        .expect("id")
}

fn stitches(s: &Session) -> Vec<openstitch_core::Stitch> {
    Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
}

/// Drags the leaving handle of vertex `index` as the editor does: `object.setSegment` with
/// the side's control points as they are after the drag (a straight neighbour's copy too).
fn drag_leaving(s: &mut Session, id: u64, index: usize, to: [f64; 2]) {
    let c = contour(s, id);
    let z = c.curve(index).expect("a curve leaves the point");
    let far = if c.borrows_handle(index, false) {
        to
    } else {
        [z.c2.x / 10.0, z.c2.y / 10.0]
    };
    run(
        s,
        "object.setSegment",
        json!({ "id": id, "segment": index, "curve": [to, far] }),
    );
}

#[test]
fn to_curve_then_the_three_kinds_then_to_straight() {
    let mut s = Session::new(std::env::temp_dir());
    let id = square(&mut s);
    let before = stitches(&s);
    // A straight point takes To Curve only.
    for kind in ["smooth", "symmetric", "cusp"] {
        let e = fails(
            &mut s,
            "object.setPointKind",
            json!({ "id": id, "index": 0, "kind": kind }),
        );
        assert!(
            e.contains("straight point") && e.contains("kind: curve"),
            "{e}"
        );
    }
    // To Curve: PE-DESIGN's handles, point ∓ (next − previous) / 4, and a smooth point.
    let r = run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 0, "kind": "curve" }),
    );
    assert_eq!(r["changed"], 1);
    let c = contour(&s, id);
    assert!(c.curve(0).is_some() && c.curve(3).is_some() && c.curve(1).is_none());
    assert_eq!(c.kind(0), PointKind::Smooth);
    assert_eq!(
        c.point_handles(0),
        (Some(v2(-100.0, 100.0)), Some(v2(100.0, -100.0)))
    );
    assert!(c.is_curve_point(0) && !c.is_curve_point(1) && !c.is_curve_point(3));
    assert_ne!(stitches(&s), before, "the outline bends at once");
    let again = run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 0, "kind": "curve" }),
    );
    assert_eq!(again["changed"], 0, "a curve point is left as it is");

    // A smooth point: dragging one handle turns the other, which keeps its length.
    drag_leaving(&mut s, id, 0, [0.0, -20.0]);
    let c = contour(&s, id);
    let (a, l) = c.point_handles(0);
    assert_eq!(l, Some(v2(0.0, -200.0)));
    assert!(
        a.expect("a").distance(v2(0.0, 20000.0_f64.sqrt())) < 1e-6,
        "{a:?}"
    );
    assert!(
        c.borrows_handle(0, false),
        "the straight neighbour still has no handle"
    );

    // Symmetric: the mean length at once, then a mirror image.
    run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 0, "kind": "symmetric" }),
    );
    let mean = (200.0 + 20000.0_f64.sqrt()) / 2.0;
    let (a, l) = contour(&s, id).point_handles(0);
    assert!(
        a.expect("a").distance(v2(0.0, mean)) < 1e-6
            && l.expect("l").distance(v2(0.0, -mean)) < 1e-6
    );
    drag_leaving(&mut s, id, 0, [3.0, -4.0]);
    let c = contour(&s, id);
    assert_eq!(c.kind(0), PointKind::Symmetric);
    let (a, l) = c.point_handles(0);
    assert!(l.expect("l").distance(v2(30.0, -40.0)) < 1e-6);
    assert!(a.expect("a").distance(v2(-30.0, 40.0)) < 1e-6, "mirrored");

    // Cusp: nothing moves, and a drag leaves the other handle.
    let curves = contour(&s, id).curves.clone();
    run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 0, "kind": "cusp" }),
    );
    assert_eq!(contour(&s, id).curves, curves);
    assert_eq!(contour(&s, id).kind(0), PointKind::Cusp);
    drag_leaving(&mut s, id, 0, [10.0, 0.0]);
    assert!(
        contour(&s, id)
            .point_handles(0)
            .0
            .expect("a")
            .distance(v2(-30.0, 40.0))
            < 1e-6,
        "a cusp's other handle stays"
    );

    // Moving a straight neighbour leaves the curve point's handle; moving the curve point
    // takes its handles along.
    run(
        &mut s,
        "object.setPoint",
        json!({ "id": id, "index": 1, "x": 42, "y": 1 }),
    );
    assert_eq!(contour(&s, id).point_handles(0).1, Some(v2(100.0, 0.0)));
    run(
        &mut s,
        "object.setPoint",
        json!({ "id": id, "index": 1, "x": 40, "y": 0 }),
    );
    run(
        &mut s,
        "object.setPoint",
        json!({ "id": id, "index": 0, "x": 1, "y": 1 }),
    );
    assert_eq!(
        contour(&s, id).point_handles(0),
        (Some(v2(-20.0, 50.0)), Some(v2(110.0, 10.0)))
    );
    run(
        &mut s,
        "object.setPoint",
        json!({ "id": id, "index": 0, "x": 0, "y": 0 }),
    );

    run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 0, "kind": "straight" }),
    );
    let c = contour(&s, id);
    assert!(c.curves.is_empty() && c.kinds.is_empty());
    assert_eq!(stitches(&s), before, "back to the square");
    assert!(
        fails(
            &mut s,
            "object.setPointKind",
            json!({ "id": id, "index": 0, "kind": "round" })
        )
        .contains("kind")
    );
}

#[test]
fn several_points_change_in_one_undo_step() {
    let mut s = Session::new(std::env::temp_dir());
    let id = square(&mut s);
    let r = run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "indexes": [0, 2], "kind": "curve" }),
    );
    assert_eq!(r["changed"], 2);
    assert!(contour(&s, id).curves.iter().all(Option::is_some));
    run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "indexes": [2, 0], "kind": "symmetric" }),
    );
    assert_eq!(contour(&s, id).kind(2), PointKind::Symmetric);
    undo(&mut s);
    assert_eq!(
        contour(&s, id).kind(2),
        PointKind::Smooth,
        "one step for both points"
    );
    assert_eq!(contour(&s, id).kind(0), PointKind::Smooth);
    undo(&mut s);
    assert!(contour(&s, id).curves.is_empty() && contour(&s, id).kinds.is_empty());
    let e = fails(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 0, "indexes": [1], "kind": "curve" }),
    );
    assert!(e.contains("index"), "{e}");
    assert!(
        fails(
            &mut s,
            "object.setPointKind",
            json!({ "id": id, "indexes": [9], "kind": "curve" })
        )
        .contains("0...3")
    );

    // Delete: two corners at once would leave a 2-point closed straight outline; refused whole.
    let e = fails(
        &mut s,
        "object.deletePoint",
        json!({ "id": id, "indexes": [0, 1] }),
    );
    assert!(e.contains("at least 3"), "{e}");
    assert_eq!(contour(&s, id).points.len(), 4, "nothing was deleted");
    run(
        &mut s,
        "object.insertPoint",
        json!({ "id": id, "segment": 0 }),
    );
    run(
        &mut s,
        "object.deletePoint",
        json!({ "id": id, "indexes": [1, 3] }),
    );
    assert_eq!(
        contour(&s, id).points,
        vec![v2(0.0, 0.0), v2(400.0, 0.0), v2(0.0, 400.0)]
    );
    undo(&mut s);
    assert_eq!(contour(&s, id).points.len(), 5, "one undo step");
}

#[test]
fn split_at_point_opens_a_closed_outline_and_cuts_an_open_line_in_two() {
    let mut s = Session::new(std::env::temp_dir());
    let id = square(&mut s);
    let r = run(
        &mut s,
        "object.splitAtPoint",
        json!({ "id": id, "index": 3 }),
    );
    assert_eq!(
        r,
        json!({ "ids": [id], "opened": true, "regionOff": false })
    );
    let c = contour(&s, id);
    assert!(!c.closed);
    assert_eq!(c.points.first(), Some(&v2(0.0, 400.0)));
    assert_eq!(c.points.last(), Some(&v2(0.0, 400.0)));
    assert_eq!(c.points.len(), 5);
    assert!(object(&s, id).line.is_some());
    undo(&mut s);
    assert!(contour(&s, id).closed, "one undo step");

    let line = open_line(&mut s);
    for end in [0, 2] {
        let e = fails(
            &mut s,
            "object.splitAtPoint",
            json!({ "id": line, "index": end }),
        );
        assert!(e.contains("first or last point"), "{e}");
    }
    let count = s.design().objects.len();
    let r = run(
        &mut s,
        "object.splitAtPoint",
        json!({ "id": line, "index": 1 }),
    );
    let ids: Vec<u64> = r["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 2);
    assert_eq!(ids[0], line, "the first piece keeps the id");
    assert_eq!(s.design().objects.len(), count + 1);
    assert_eq!(
        contour(&s, ids[0]).points,
        vec![v2(0.0, 0.0), v2(300.0, 0.0)]
    );
    assert_eq!(
        contour(&s, ids[1]).points,
        vec![v2(300.0, 0.0), v2(300.0, 300.0)]
    );
    let d = s.design();
    assert_eq!(
        d.index_of(ids[1]),
        d.index_of(ids[0]).map(|i| i + 1),
        "sewn right after the first piece"
    );
    assert_eq!(
        object(&s, ids[1]).line,
        object(&s, ids[0]).line,
        "same attributes"
    );
    undo(&mut s);
    assert_eq!(s.design().objects.len(), count, "one undo step");
}

#[test]
fn opening_the_outline_of_a_region() {
    let mut s = Session::new(std::env::temp_dir());
    // Region and line: the region is switched off, the line stays.
    let both = run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 0, "y": 0, "width": 40, "height": 40, "fill": "#008000", "outline": "#000000" }),
    )["id"]
        .as_u64()
        .expect("id");
    assert!(object(&s, both).region.is_some());
    let n = contour(&s, both).points.len() as u64;
    let r = run(
        &mut s,
        "object.splitAtPoint",
        json!({ "id": both, "index": n - 1 }),
    );
    assert_eq!(r["regionOff"], true);
    assert!(object(&s, both).region.is_none() && object(&s, both).line.is_some());
    // Region only: refused, nothing changes.
    let only = run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 50, "y": 0, "width": 40, "height": 40, "fill": "#008000" }),
    )["id"]
        .as_u64()
        .expect("id");
    if object(&s, only).line.is_none() {
        let e = fails(
            &mut s,
            "object.splitAtPoint",
            json!({ "id": only, "index": 0 }),
        );
        assert!(e.contains("nothing to sew"), "{e}");
        assert!(contour(&s, only).closed);
    }
}

/// #1258's rule: a command says what it stores, read from the registry as a model reads it.
#[test]
fn the_registry_says_what_each_point_command_stores() {
    let s = Session::new(std::env::temp_dir());
    let commands = s.commands();
    let spec = |id: &str| {
        commands
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} is registered"))
    };
    let param = |id: &str, name: &str| {
        spec(id)
            .params
            .iter()
            .find(|p| p.name == name)
            .unwrap_or_else(|| panic!("{id} has {name}"))
            .summary
    };
    let kind = param("object.setPointKind", "kind");
    for words in [
        "the point loses its handles and its kind",
        "stays a curve",
        "stored as smooth",
        "(next - previous) / 4",
        "the outline bends at once",
        "a curve point is left as it is",
        "stored as symmetric",
        "mean length",
        "stored as a cusp",
        "refused on a straight point",
        "object.setSegment",
        "0x11 smooth, 0x15 symmetric, 0x19 cusp",
    ] {
        assert!(kind.contains(words), "kind names {words:?}: {kind}");
    }
    assert!(spec("object.setPointKind").summary.contains("'changed'"));
    assert!(param("object.setPoint", "index").contains("Vertex"));
    assert!(
        spec("object.setPoint")
            .summary
            .contains("a straight point beside a curve point has none")
    );
    for name in ["straight", "curve", "smooth", "symmetric", "cusp"] {
        assert!(
            spec("object.setPointKind")
                .params
                .iter()
                .any(|p| p.kind.contains(name))
        );
    }
    for id in ["object.setPointKind", "object.deletePoint"] {
        assert!(param(id, "indexes").contains("one undo step"), "{id}");
    }
    let split = spec("object.splitAtPoint").summary;
    for words in [
        "closed outline is opened",
        "region",
        "switched off",
        "two objects",
        "new id",
        "first and last point",
        "One undo step",
    ] {
        assert!(split.contains(words), "split names {words:?}: {split}");
    }
    let curve = param("object.setSegment", "curve");
    for words in ["smooth", "symmetric", "mirrored", "cusp"] {
        assert!(
            curve.contains(words),
            "setSegment curve names {words:?}: {curve}"
        );
    }
    assert!(
        spec("object.deletePoint")
            .summary
            .contains("nothing is deleted")
    );
    for id in ["object.setPointKind", "object.splitAtPoint"] {
        assert_eq!(
            spec(id).effect,
            openstitch_engine::commands::Effect::Edit,
            "{id}"
        );
    }
}

/// Point kinds change editing, not sewing: every example design compiles to the same
/// stitches with a kind stored on each of its points, and saves without a `kinds` key until
/// one is stored.
#[test]
fn example_designs_sew_the_same_stitches_whatever_kinds_their_points_have() {
    let examples = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples");
    let scratch =
        std::env::temp_dir().join(format!("openstitch-point-kinds-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).expect("scratch folder");
    let before_run: Vec<_> = std::fs::read_dir(&examples)
        .expect("examples")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    let mut designs = 0;
    let mut points = 0;
    let mut scripts: Vec<_> = std::fs::read_dir(&examples)
        .expect("examples")
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json"))
        .collect();
    scripts.sort();
    for path in scripts {
        let Ok(script) = serde_json::from_slice::<Value>(&std::fs::read(&path).expect("read"))
        else {
            continue;
        };
        let Some(steps) = script.as_array() else {
            continue;
        };
        // The scripts run in a scratch folder, and no step that reads or writes a file runs:
        // this test is about the designs, and must leave `examples/` alone.
        let mut s = Session::new(&scratch);
        for step in steps {
            let command = step["command"].as_str().unwrap_or_default().to_lowercase();
            let text = step.to_string();
            let touches_files = [
                "export", "save", "import", "open", "load", "digitize", "transfer", "send", "write",
            ]
            .iter()
            .any(|w| command.contains(w))
                || text.contains("\"path\"")
                || text.contains("\"file\"")
                || text.contains("\"image\"");
            if touches_files {
                continue;
            }
            let _ = s.handle(step);
        }
        let mut d = s.design().clone();
        if d.objects.iter().all(|o| o.contours.is_empty()) {
            continue;
        }
        let saved = openstitch_design::project::encode(&d).expect("encode");
        assert!(
            !String::from_utf8_lossy(&saved).contains("\"kinds\""),
            "{}: no kinds until one is stored",
            path.display()
        );
        let before = Compiler::for_design(&d).compile(&d).stitches;
        for o in &mut d.objects {
            for c in &mut o.contours {
                for i in 0..c.points.len() {
                    c.set_kind(
                        i,
                        if i % 2 == 0 {
                            PointKind::Smooth
                        } else {
                            PointKind::Symmetric
                        },
                    );
                    points += 1;
                }
            }
        }
        let after = Compiler::for_design(&d).compile(&d).stitches;
        assert!(
            before == after,
            "{}: stitches changed with point kinds",
            path.display()
        );
        let back = openstitch_design::project::decode(
            &openstitch_design::project::encode(&d).expect("encode"),
        )
        .expect("decode");
        assert_eq!(
            back.objects,
            d.objects,
            "{}: kinds are saved",
            path.display()
        );
        designs += 1;
    }
    let after_run: Vec<_> = std::fs::read_dir(&examples)
        .expect("examples")
        .flatten()
        .map(|e| e.file_name())
        .collect();
    assert_eq!(before_run, after_run, "the test wrote into examples/");
    let _ = std::fs::remove_dir_all(&scratch);
    assert!(
        designs >= 10 && points >= 100,
        "{designs} designs, {points} points"
    );
}

/// Join Lines keeps each point's kind, whichever way the lines are chained or turned.
#[test]
fn join_lines_keeps_each_points_kind() {
    for first_is_a in [true, false] {
        let mut s = Session::new(std::env::temp_dir());
        let a = open_line(&mut s);
        let b = run(
            &mut s,
            "shape.polygon",
            json!({ "points": [[60, 60], [60, 30], [30, 30]], "closed": false, "outline": "#203040" }),
        )["id"]
            .as_u64()
            .expect("id");
        for kind in ["curve", "symmetric"] {
            run(
                &mut s,
                "object.setPointKind",
                json!({ "id": b, "index": 1, "kind": kind }),
            );
        }
        let handles = contour(&s, b).point_handles(1);
        let ids = if first_is_a { [a, b] } else { [b, a] };
        let joined = run(&mut s, "shape.joinLines", json!({ "ids": ids }))["id"]
            .as_u64()
            .expect("id");
        let c = contour(&s, joined);
        assert_eq!(c.points.len(), 5);
        let at = c
            .points
            .iter()
            .position(|p| p.distance(v2(600.0, 300.0)) < 1e-6)
            .expect("the corner");
        let kinds: Vec<_> = (0..5).map(|i| c.kind(i)).collect();
        assert_eq!(
            kinds.iter().filter(|k| **k != PointKind::Unset).count(),
            1,
            "{kinds:?}"
        );
        assert_eq!(c.kind(at), PointKind::Symmetric);
        let (x, y) = c.point_handles(at);
        assert!(
            (x, y) == handles || (y, x) == handles,
            "its handles came along: {x:?} {y:?}"
        );
    }
}
