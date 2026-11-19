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

    // Delete goes down to two points on a closed outline, as PE-DESIGN's (#1260); below
    // that it is refused whole.
    let e = fails(
        &mut s,
        "object.deletePoint",
        json!({ "id": id, "indexes": [0, 1, 2] }),
    );
    assert!(e.contains("at least 2"), "{e}");
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
    undo(&mut s);
    run(
        &mut s,
        "object.deletePoint",
        json!({ "id": id, "indexes": [2, 3] }),
    );
    let c = contour(&s, id);
    assert!(c.closed && c.points == vec![v2(0.0, 0.0), v2(400.0, 0.0)]);
    // A two-point closed outline still compiles (PE-DESIGN sews its side there and back).
    let _ = stitches(&s);
    let e = fails(
        &mut s,
        "object.deletePoint",
        json!({ "id": id, "index": 0 }),
    );
    assert!(e.contains("at least 2"), "{e}");
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
        json!({ "ids": [id], "opened": true, "regionOff": false, "regionToLine": false })
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
    // Region only: the region becomes line sewing in its thread, a running stitch, as
    // object.regionToLine gives it; one undo step.
    let only = run(
        &mut s,
        "shape.rectangle",
        json!({ "x": 50, "y": 0, "width": 40, "height": 40, "fill": "#008000" }),
    )["id"]
        .as_u64()
        .expect("id");
    assert!(object(&s, only).line.is_none());
    let thread = object(&s, only)
        .region
        .as_ref()
        .expect("region")
        .thread
        .clone();
    let r = run(
        &mut s,
        "object.splitAtPoint",
        json!({ "id": only, "index": 0 }),
    );
    assert_eq!(
        (&r["regionOff"], &r["regionToLine"]),
        (&json!(true), &json!(true))
    );
    let o = object(&s, only);
    assert!(!o.contours[0].closed && o.region.is_none());
    let line = o.line.as_ref().expect("line sewing");
    assert_eq!(
        (line.kind, &line.thread),
        (openstitch_design::LineType::Running, &thread)
    );
    undo(&mut s);
    let o = object(&s, only);
    assert!(
        o.contours[0].closed && o.region.is_some() && o.line.is_none(),
        "one undo step"
    );
    // Open does the same.
    let r = run(&mut s, "object.openOutline", json!({ "id": only }));
    assert_eq!(
        r,
        json!({ "id": only, "regionOff": true, "regionToLine": true })
    );
    let o = object(&s, only);
    assert!(!o.contours[0].closed && o.region.is_none() && o.line.is_some());
    assert!(!stitches(&s).is_empty());
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
        "is removed",
        "region only gets line sewing",
        "regionToLine",
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
/// stitches with a kind stored on each of its points, and the kinds are saved.
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

/// `object.setSegment {straight: true}` forgets the kind of a point left without a curve on
/// either side, and stores none on a plain corner (review of #1263).
#[test]
fn setting_sides_straight_leaves_no_kind_behind() {
    let mut s = Session::new(std::env::temp_dir());
    let id = square(&mut s);
    let plain = contour(&s, id).clone();
    // A corner between straight sides: making a side "straight" again stores nothing.
    run(
        &mut s,
        "object.setSegment",
        json!({ "id": id, "segment": 0, "straight": true }),
    );
    assert_eq!(
        *contour(&s, id),
        plain,
        "nothing stored: the same outline, the same file"
    );
    assert!(contour(&s, id).kinds.is_empty());
    assert!(!contour(&s, id).is_curve_point(0) && !contour(&s, id).is_curve_point(1));
    // A legacy curve set straight again: the same.
    run(
        &mut s,
        "object.setSegment",
        json!({ "id": id, "segment": 0 }),
    );
    run(
        &mut s,
        "object.setSegment",
        json!({ "id": id, "segment": 0, "straight": true }),
    );
    assert_eq!(*contour(&s, id), plain);
    // A smooth point: with one side straight it keeps its kind (a curve still arrives);
    // with both sides straight it has none, and is a straight point that takes To Curve.
    run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 0, "kind": "curve" }),
    );
    assert_eq!(contour(&s, id).kind(0), PointKind::Smooth);
    run(
        &mut s,
        "object.setSegment",
        json!({ "id": id, "segment": 0, "straight": true }),
    );
    assert_eq!(contour(&s, id).kind(0), PointKind::Smooth);
    run(
        &mut s,
        "object.setSegment",
        json!({ "id": id, "segment": 3, "straight": true }),
    );
    assert_eq!(*contour(&s, id), plain);
    assert!(!contour(&s, id).is_curve_point(0));
    let r = run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 0, "kind": "curve" }),
    );
    assert_eq!(r["changed"], 1, "To Curve is offered again");
}

// MARK: #1260, the rest of the Select Point tool

/// One click on a side (#1260, capture e5): a straight point on a straight side; on a curve
/// an exact split whose new point is smooth, and a straight end stays a straight point.
#[test]
fn a_point_inserted_on_a_side_is_straight_or_smooth_as_pe_design_makes_it() {
    let mut s = Session::new(std::env::temp_dir());
    // PE-DESIGN's top side of e5: (35, 30.5) … (70, 30.5) with a point at 50.1 made a curve
    // point (handles ∓8.75 mm on the side).
    let id = run(
        &mut s,
        "shape.polygon",
        json!({ "points": [[35, 30.5], [50.1, 30.5], [70, 30.5]], "closed": false, "outline": "#203040" }),
    )["id"]
        .as_u64()
        .expect("id");
    let r = run(
        &mut s,
        "object.insertPoint",
        json!({ "id": id, "segment": 0, "t": 0.5 }),
    );
    assert_eq!(r["index"], 1);
    assert_eq!(contour(&s, id).kind(1), PointKind::Unset);
    assert!(!contour(&s, id).is_curve_point(1) && contour(&s, id).curves.is_empty());
    undo(&mut s);
    run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 1, "kind": "curve" }),
    );
    let (a, l) = contour(&s, id).point_handles(1);
    assert!(
        a.expect("a").distance(v2(413.5, 305.0)) < 1e-6
            && l.expect("l").distance(v2(588.5, 305.0)) < 1e-6
    );
    // PE-DESIGN's click landed at t = 0.826 of the curved piece: the new point at 65.1 mm
    // with handles at 58.59 and 66.47, the curve point's handle drawn back to 57.33.
    let r = run(
        &mut s,
        "object.insertPoint",
        json!({ "id": id, "segment": 1, "t": 0.826 }),
    );
    assert_eq!(r["index"], 2);
    let c = contour(&s, id);
    assert!(
        (c.points[2].x - 651.0).abs() < 0.3 && (c.points[2].y - 305.0).abs() < 1e-9,
        "{:?}",
        c.points[2]
    );
    assert_eq!(c.kind(2), PointKind::Smooth);
    let (a, l) = c.point_handles(2);
    assert!(
        (a.expect("a").x - 585.9).abs() < 0.3 && (l.expect("l").x - 664.7).abs() < 0.3,
        "{a:?} {l:?}"
    );
    assert!((c.point_handles(1).1.expect("leaving").x - 573.3).abs() < 0.3);
    assert_eq!(
        c.kind(1),
        PointKind::Smooth,
        "the curve point keeps its kind"
    );
    assert!(
        !c.is_curve_point(3),
        "the straight end stays a straight point"
    );
    assert!(c.borrows_handle(2, false));
    undo(&mut s);
    assert_eq!(contour(&s, id).points.len(), 3, "one undo step");
}

/// Dragging one of several selected points, and the arrow keys (#1260, captures e1, e2):
/// `object.movePoints`. Ten presses of 0.5 mm move PE-DESIGN's corner exactly 5.0 mm.
#[test]
fn several_points_move_together_in_one_undo_step() {
    let mut s = Session::new(std::env::temp_dir());
    let id = square(&mut s);
    for _ in 0..10 {
        run(
            &mut s,
            "object.movePoints",
            json!({ "id": id, "index": 0, "dx": 0.5, "dy": 0 }),
        );
    }
    assert_eq!(contour(&s, id).points[0], v2(50.0, 0.0));
    let r = run(
        &mut s,
        "object.movePoints",
        json!({ "id": id, "indexes": [3, 2], "dx": -5, "dy": 5 }),
    );
    assert_eq!(r["moved"], 2);
    let c = contour(&s, id);
    assert_eq!(
        (c.points[2], c.points[3]),
        (v2(350.0, 450.0), v2(-50.0, 450.0))
    );
    assert_eq!(c.points[1], v2(400.0, 0.0));
    undo(&mut s);
    assert_eq!(
        contour(&s, id).points[2],
        v2(400.0, 400.0),
        "one undo step for both"
    );
    // A curve point takes its handles (and the copy its straight neighbour borrows) along.
    run(
        &mut s,
        "object.setPointKind",
        json!({ "id": id, "index": 2, "kind": "curve" }),
    );
    let before = contour(&s, id).point_handles(2);
    run(
        &mut s,
        "object.movePoints",
        json!({ "id": id, "indexes": [2], "dx": 1, "dy": 1 }),
    );
    let after = contour(&s, id).point_handles(2);
    let d = v2(10.0, 10.0);
    assert_eq!(after, (before.0.map(|h| h + d), before.1.map(|h| h + d)));
    assert!(contour(&s, id).borrows_handle(2, false) && contour(&s, id).borrows_handle(1, true));
    assert!(
        fails(
            &mut s,
            "object.movePoints",
            json!({ "id": id, "indexes": [9], "dx": 1, "dy": 1 })
        )
        .contains("0...3")
    );
}

/// The object menu's Close and Open (#1260, captures e7, e8): Close makes the open line a
/// closed shape and gives it a region, Fill Stitch at 45°; Open removes the closing side and
/// the region.
#[test]
fn close_adds_a_region_and_open_removes_it_as_pe_design_does() {
    let mut s = Session::new(std::env::temp_dir());
    let id = open_line(&mut s);
    assert!(object(&s, id).region.is_none());
    let before = stitches(&s).len();
    let e = fails(&mut s, "object.openOutline", json!({ "id": id }));
    assert!(e.contains("open already"), "{e}");
    let r = run(&mut s, "object.closeOutline", json!({ "id": id }));
    assert_eq!(r, json!({ "id": id, "regionAdded": true }));
    let o = object(&s, id);
    assert!(o.contours[0].closed && o.contours[0].points.len() == 3);
    let region = o.region.as_ref().expect("a region");
    assert_eq!(
        (region.kind, region.angle),
        (openstitch_design::RegionType::Fill, 45.0)
    );
    assert_eq!(Some(&region.thread), o.line.as_ref().map(|l| &l.thread));
    assert!(stitches(&s).len() > 2 * before, "the triangle is filled");
    let e = fails(&mut s, "object.closeOutline", json!({ "id": id }));
    assert!(e.contains("closed already"), "{e}");
    let r = run(&mut s, "object.openOutline", json!({ "id": id }));
    assert_eq!(
        r,
        json!({ "id": id, "regionOff": true, "regionToLine": false })
    );
    let o = object(&s, id);
    assert!(!o.contours[0].closed && o.region.is_none() && o.line.is_some());
    assert_eq!(
        o.contours[0].points,
        vec![v2(0.0, 0.0), v2(300.0, 0.0), v2(300.0, 300.0)]
    );
    assert_eq!(stitches(&s).len(), before, "the line as it was");
    undo(&mut s);
    assert!(object(&s, id).region.is_some(), "one undo step");
    undo(&mut s);
    // Without a region, and with ends that lie on each other.
    run(
        &mut s,
        "object.setPoint",
        json!({ "id": id, "index": 2, "x": 0, "y": 0 }),
    );
    let r = run(
        &mut s,
        "object.closeOutline",
        json!({ "id": id, "region": false }),
    );
    assert_eq!(r["regionAdded"], false);
    let o = object(&s, id);
    assert!(o.region.is_none() && o.contours[0].closed);
    assert_eq!(
        o.contours[0].points.len(),
        2,
        "the doubled end is one point"
    );
}

#[test]
fn the_registry_says_what_the_1260_commands_store() {
    let s = Session::new(std::env::temp_dir());
    let commands = s.commands();
    let summary = |id: &str| {
        commands
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} is registered"))
            .summary
    };
    for words in [
        "closing side",
        "removed",
        "regionOff",
        "region only gets line sewing",
        "running stitch in the region's thread",
        "regionToLine",
        "One undo step",
    ] {
        assert!(
            summary("object.openOutline").contains(words),
            "open: {words}"
        );
    }
    for words in [
        "straight closing side",
        "Fill Stitch at the default direction of 45 degrees",
        "regionAdded",
        "'region: false'",
        "become one point",
        "One undo step",
    ] {
        assert!(
            summary("object.closeOutline").contains(words),
            "close: {words}"
        );
    }
    for words in [
        "same offset",
        "0.5 mm",
        "own curve handles",
        "One undo step",
    ] {
        assert!(
            summary("object.movePoints").contains(words),
            "move: {words}"
        );
    }
    for words in [
        "one click",
        "straight point on a straight side",
        "stored as smooth",
    ] {
        assert!(
            summary("object.insertPoint").contains(words),
            "insert: {words}"
        );
    }
    assert!(summary("object.deletePoint").contains("keeps 2 points, open or closed"));
    let dx = commands
        .iter()
        .find(|c| c.id == "object.movePoints")
        .and_then(|c| c.params.iter().find(|p| p.name == "dx"))
        .expect("dx");
    assert!(dx.summary.contains("old ones plus this"));
}

/// A new ellipse's four points are smooth curve points, as the points of PE-DESIGN's circle
/// are (kind 0x11, capture c0): a dragged handle turns the other. Its stitches and its PES
/// vertices are what they were before kinds existed.
#[test]
fn a_new_ellipse_has_smooth_points_as_pe_designs_circle() {
    let mut s = Session::new(std::env::temp_dir());
    let id = run(
        &mut s,
        "shape.ellipse",
        json!({ "cx": 50, "cy": 50, "rx": 20, "outline": "#203040" }),
    )["id"]
        .as_u64()
        .expect("id");
    let c = contour(&s, id).clone();
    assert_eq!(c.points.len(), 4);
    assert!((0..4).all(|i| c.kind(i) == PointKind::Smooth && c.is_curve_point(i)));
    // The kinds change nothing that is sewn.
    let mut plain = s.design().clone();
    for o in &mut plain.objects {
        for c in &mut o.contours {
            c.kinds.clear();
        }
    }
    assert!(
        Compiler::for_design(&plain).compile(&plain).stitches == stitches(&s),
        "the same stitches with and without the kinds"
    );
    // Dragging the leaving handle of point 0 turns its arriving handle.
    let (a, l) = c.point_handles(0);
    let (a, l) = (a.expect("a") - c.points[0], l.expect("l") - c.points[0]);
    let to = c.points[0] + v2(l.y, -l.x);
    drag_leaving(&mut s, id, 0, [to.x / 10.0, to.y / 10.0]);
    let c2 = contour(&s, id);
    let a2 = c2.point_handles(0).0.expect("a") - c2.points[0];
    assert!(
        (a2.length() - a.length()).abs() < 1e-6,
        "its length is kept"
    );
    assert!(
        (a2.x * l.x + a2.y * l.y).abs() < 1e-6,
        "turned a quarter with the dragged one"
    );
    assert!(
        s.commands()
            .iter()
            .any(|c| c.id == "shape.ellipse" && c.summary.contains("stored as smooth"))
    );
}

/// #1260: the Point tool's selection spans shapes and outlines; the point commands take one
/// outline a call, and the registry says how several are edited.
#[test]
fn the_registry_says_the_point_commands_take_one_outline_a_call() {
    let s = Session::new(std::env::temp_dir());
    let commands = s.commands();
    for id in [
        "object.movePoints",
        "object.setPointKind",
        "object.deletePoint",
    ] {
        let summary = commands
            .iter()
            .find(|c| c.id == id)
            .unwrap_or_else(|| panic!("{id} is registered"))
            .summary;
        for words in [
            "One outline of one object a call",
            "one call per outline",
            "in one undo step",
        ] {
            assert!(summary.contains(words), "{id}: {words}");
        }
    }
}
