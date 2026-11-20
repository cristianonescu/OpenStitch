//! Stitch pattern editing commands.

use openstitch_core::Command;
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// Two filled squares (Red, Blue) converted into stitch patterns 1 and 2.
fn session() -> Session {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20, "fill": "Red"}),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 50, "y": 10, "width": 20, "height": 20, "fill": "Blue"}),
    );
    run(&mut s, "object.toStitches", json!({"ids": [1, 2]}));
    s
}

fn points(s: &mut Session, id: u64) -> Vec<Value> {
    run(s, "stitches.points", json!({"id": id}))["points"]
        .as_array()
        .expect("points")
        .clone()
}

fn stitch_count(s: &Session) -> usize {
    s.design()
        .objects
        .iter()
        .flat_map(|o| o.stitch_block.iter().flat_map(|b| b.stitches.iter()))
        .filter(|st| st.command == Command::Stitch)
        .count()
}

#[test]
fn points_move_insert_delete() {
    let mut s = session();
    let pts = points(&mut s, 1);
    let k = pts
        .iter()
        .position(|p| p["command"] == "stitch")
        .expect("a stitch")
        + 1;
    let (x, y) = (
        pts[k]["x"].as_f64().expect("x"),
        pts[k]["y"].as_f64().expect("y"),
    );
    run(
        &mut s,
        "stitches.movePoints",
        json!({"id": 1, "indexes": [k], "dx": 1.5, "dy": -2}),
    );
    let moved = &points(&mut s, 1)[k];
    assert!((moved["x"].as_f64().expect("x") - x - 1.5).abs() < 1e-9);
    assert!((moved["y"].as_f64().expect("y") - y + 2.0).abs() < 1e-9);
    run(
        &mut s,
        "stitches.setPoint",
        json!({"id": 1, "index": k, "x": 12, "y": 13}),
    );
    assert_eq!(points(&mut s, 1)[k]["x"], 12.0);

    let before = stitch_count(&s);
    let r = run(&mut s, "stitches.insertPoint", json!({"id": 1, "index": k}));
    assert_eq!(r["index"], json!(k));
    assert_eq!(stitch_count(&s), before + 1);
    run(
        &mut s,
        "stitches.deletePoints",
        json!({"id": 1, "indexes": [k, k + 1]}),
    );
    assert_eq!(stitch_count(&s), before - 1);
    // Each edit is one undo step.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(stitch_count(&s), before + 1);
}

#[test]
fn split_connect_and_divide() {
    let mut s = session();
    let pts = points(&mut s, 1);
    let mid = pts.len() / 2;
    let mid = (mid..pts.len())
        .find(|&i| pts[i]["command"] == "stitch")
        .expect("stitch");
    let before = stitch_count(&s);
    let r = run(&mut s, "stitches.splitAt", json!({"id": 1, "index": mid}));
    let new = r["ids"][1].as_u64().expect("id");
    assert_eq!(s.design().objects.len(), 3);
    assert_eq!(
        s.design().objects[1].id,
        new,
        "the second part follows the first"
    );
    assert_eq!(
        stitch_count(&s),
        before + 1,
        "the split point is in both parts"
    );

    run(&mut s, "stitches.connect", json!({"ids": [new, 1]}));
    assert_eq!(s.design().objects.len(), 2);
    assert_eq!(stitch_count(&s), before + 1);

    // Connect the two colours, then divide them again.
    let r = run(&mut s, "stitches.connect", json!({"ids": [1, 2]}));
    assert_eq!(r["id"], json!(1));
    let b = s.design().objects[0].stitch_block.clone().expect("block");
    assert_eq!(
        b.threads.len(),
        1,
        "the joined pattern takes the first one's colour"
    );
    let stats = run(&mut s, "design.generate", json!({}));
    assert_eq!(stats["colors"].as_array().expect("colors").len(), 1);
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));

    // A two-colour pattern divides into two.
    let mut d = s.design().clone();
    let a = d.objects[0].stitch_block.clone().expect("a");
    let b = d.objects[1].stitch_block.clone().expect("b");
    let mut stitches = a.stitches.clone();
    stitches.push(openstitch_core::Stitch::new(0.0, 0.0, Command::ColorChange));
    stitches.extend(b.stitches);
    d.objects[0].stitch_block = Some(openstitch_design::StitchBlock {
        stitches,
        threads: [a.threads, b.threads].concat(),
    });
    d.objects.truncate(1);
    let path_tmp = openstitch_testutil::TempDir::new("stitch-edit");
    let path = path_tmp.join("stitch-edit.osd");
    std::fs::write(
        &path,
        openstitch_engine::project::encode(&d).expect("encode"),
    )
    .expect("write");
    run(
        &mut s,
        "project.open",
        json!({"path": path.display().to_string()}),
    );
    std::fs::remove_file(&path).ok();
    let r = run(&mut s, "stitches.divideByColor", json!({"id": 1}));
    assert_eq!(r["ids"].as_array().expect("ids").len(), 2);
    let names: Vec<_> = s
        .design()
        .objects
        .iter()
        .map(|o| o.stitch_block.as_ref().expect("b").threads[0].name.clone())
        .collect();
    assert_eq!(names, vec![Some("Red".into()), Some("Blue".into())]);
}

#[test]
fn split_off_an_area() {
    let mut s = session();
    let before = stitch_count(&s);
    // The left half of the red square (x 10-20 mm).
    let r = run(
        &mut s,
        "stitches.splitArea",
        json!({"id": 1, "points": [[0, 0], [20, 0], [20, 40], [0, 40]]}),
    );
    let new = r["ids"][0].as_u64().expect("id");
    assert_eq!(
        s.design().objects.last().map(|o| o.id),
        Some(new),
        "sewn last"
    );
    let inside = &s.design().objects[2];
    assert!(
        inside
            .stitch_block
            .as_ref()
            .expect("block")
            .stitches
            .iter()
            .all(|st| st.x <= 200.0)
    );
    assert_eq!(stitch_count(&s), before);
}

#[test]
fn bad_stitch_edit_parameters_fail_cleanly() {
    let mut s = session();
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 50, "width": 10, "height": 10}),
    );
    let before = s.design().clone();
    let n = points(&mut s, 1).len();
    let trim = points(&mut s, 1)
        .iter()
        .position(|p| p["command"] == "trim");
    // Object 3, a live rectangle, takes needle point edits since #501 (hand edits); objects
    // with no sewing are refused (hand_edits.rs).
    let mut bad: Vec<(&str, Value)> = vec![
        ("stitches.points", json!({"id": 99})),
        (
            "stitches.movePoints",
            json!({"id": 1, "indexes": [], "dx": 1}),
        ),
        (
            "stitches.movePoints",
            json!({"id": 1, "indexes": [n], "dx": 1}),
        ),
        ("stitches.movePoints", json!({"id": 1, "indexes": [-1]})),
        ("stitches.movePoints", json!({"id": 1, "indexes": "0"})),
        (
            "stitches.movePoints",
            json!({"id": 1, "indexes": [1], "dx": 1e9}),
        ),
        ("stitches.setPoint", json!({"id": 1, "index": 1, "x": 1})),
        (
            "stitches.setPoint",
            json!({"id": 1, "index": 1, "x": "a", "y": 1}),
        ),
        ("stitches.insertPoint", json!({"id": 1, "index": 0})),
        ("stitches.insertPoint", json!({"id": 1, "index": 2, "x": 1})),
        (
            "stitches.insertPoint",
            json!({"id": 1, "index": 2, "jump": "yes"}),
        ),
        (
            "stitches.deletePoints",
            json!({"id": 1, "indexes": (0..n).collect::<Vec<_>>()}),
        ),
        ("stitches.splitAt", json!({"id": 1, "index": n})),
        ("stitches.connect", json!({"ids": [1]})),
        ("stitches.connect", json!({"ids": [1, 1]})),
        ("stitches.connect", json!({"ids": [1, 3]})),
        ("stitches.connect", json!({"ids": [1, 2, 3]})),
        ("stitches.divideByColor", json!({"id": 3})),
        (
            "stitches.splitArea",
            json!({"id": 1, "points": [[0, 0], [1, 1]]}),
        ),
        (
            "stitches.splitArea",
            json!({"id": 1, "points": [[90, 90], [95, 90], [95, 95]]}),
        ),
        (
            "stitches.splitArea",
            json!({"id": 1, "points": [[-100, -100], [500, -100], [500, 500], [-100, 500]]}),
        ),
    ];
    if let Some(t) = trim {
        bad.push((
            "stitches.movePoints",
            json!({"id": 1, "indexes": [t], "dx": 1}),
        ));
    }
    for (id, params) in bad {
        assert!(s.execute(id, Some(&params)).is_err(), "{id} {params}");
        assert_eq!(s.design(), &before, "{id} {params} changed the design");
    }
}

/// Select Point on a stitch pattern (#1272): Ctrl+Shift+click inserts a point with the stitch
/// up to it as a jump; deleting either end of a jump keeps the jump on the neighbour
/// (PE-DESIGN captures k11, k14).
#[test]
fn insert_as_a_jump_and_delete_a_jump_end() {
    let mut s = session();
    let pts = points(&mut s, 1);
    let k = pts
        .iter()
        .enumerate()
        .skip(5)
        .find(|(_, p)| p["command"] == "stitch")
        .map(|(k, _)| k)
        .expect("a stitch");
    let jumps = |s: &mut Session| {
        points(s, 1)
            .iter()
            .filter(|p| p["command"] == "jump")
            .count()
    };
    let before = jumps(&mut s);
    let r = run(
        &mut s,
        "stitches.insertPoint",
        json!({"id": 1, "index": k, "jump": true}),
    );
    let new = r["index"].as_u64().expect("index") as usize;
    assert_eq!(new, k + 1, "the stitch after the new jump entry");
    assert_eq!(jumps(&mut s), before + 1);
    let now = points(&mut s, 1);
    assert_eq!(now[k]["command"], "jump");
    assert_eq!(
        (now[k]["x"].clone(), now[k]["y"].clone()),
        (now[new]["x"].clone(), now[new]["y"].clone())
    );
    // Deleting the point the jump lands on keeps the jump: it lands on the next point now
    // (PE-DESIGN captures k11, k14), and nothing is sewn from the point before.
    run(
        &mut s,
        "stitches.deletePoints",
        json!({"id": 1, "indexes": [k]}),
    );
    assert_eq!(jumps(&mut s), before + 1, "the jump is kept");
    assert_eq!(points(&mut s, 1)[k]["command"], "jump", "on the next point");
    run(&mut s, "history.undo", json!({}));
    // Deleting the point a jump starts from: the jump starts from the point before.
    let start = k - 1;
    let was = points(&mut s, 1)[start - 1].clone();
    run(
        &mut s,
        "stitches.deletePoints",
        json!({"id": 1, "indexes": [start]}),
    );
    let now = points(&mut s, 1);
    assert_eq!(
        now[start]["command"], "jump",
        "the jump follows the point before"
    );
    assert_eq!(now[start - 1], was);
    assert_eq!(jumps(&mut s), before + 1);
}

/// Split at Point on the entry or the exit point (#1272, PE-DESIGN capture k12): the entry
/// (or exit) point alone becomes a piece that sews nothing, the two pieces form a group, and
/// the exported stitches are the same bytes as before the split.
#[test]
fn split_at_the_entry_or_exit_point_groups_and_sews_the_same() {
    let dir = openstitch_testutil::TempDir::new("split-ends");
    let mut s = Session::new(dir.path());
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20, "fill": "Red"}),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 50, "y": 10, "width": 20, "height": 20, "fill": "Blue"}),
    );
    run(&mut s, "object.toStitches", json!({"ids": [1, 2]}));
    run(&mut s, "design.export", json!({"path": "k0.dst"}));
    let k0 = std::fs::read(dir.join("k0.dst")).expect("dst");
    let is_point = |p: &Value| p["command"] == "stitch" || p["command"] == "jump";
    let all = points(&mut s, 1);
    let first = all.iter().position(is_point).expect("entry");
    let last = all.iter().rposition(is_point).expect("exit");
    for (at, name) in [(first, "entry"), (last, "exit")] {
        let before = s.design().clone();
        let r = run(&mut s, "stitches.splitAt", json!({"id": 1, "index": at}));
        let new = r["ids"][1].as_u64().expect("id");
        let d = s.design();
        assert_eq!(d.objects.len(), 3, "{name}");
        assert_eq!((d.objects[0].id, d.objects[1].id), (1, new), "{name}");
        assert!(d.objects[0].group.is_some(), "{name}: grouped");
        assert_eq!(d.objects[0].group, d.objects[1].group, "{name}");
        assert_eq!(
            d.objects[2].group, None,
            "{name}: the other pattern stays out"
        );
        let lone = if at == first { 0 } else { 1 };
        let piece = d.objects[lone].stitch_block.as_ref().expect("block");
        assert_eq!(piece.stitches.len(), 1, "{name}: the point alone");
        assert_eq!(
            piece.stitches[0].command,
            Command::Jump,
            "{name}: sews nothing"
        );
        // Where the needle first goes in (at the exit: last comes out), in mm.
        let sewn: Vec<&Value> = all.iter().filter(|p| p["command"] == "stitch").collect();
        let at_needle = if at == first {
            sewn[0]
        } else {
            sewn[sewn.len() - 1]
        };
        assert_eq!(
            (piece.stitches[0].x / 10.0, piece.stitches[0].y / 10.0),
            (
                at_needle["x"].as_f64().expect("x"),
                at_needle["y"].as_f64().expect("y")
            ),
            "{name}: the point, in mm"
        );
        assert_eq!(
            d.objects[1 - lone].stitch_block,
            before.objects[0].stitch_block,
            "{name}: the pattern is unchanged"
        );
        let file = format!("k12-{name}.dst");
        run(&mut s, "design.export", json!({"path": file}));
        // Byte for byte after the header's label (DST's first 20 bytes: the file name).
        assert_eq!(
            std::fs::read(dir.join(&file)).expect("dst")[20..],
            k0[20..],
            "{name}: the file's stitches are the same as before the split"
        );
        run(&mut s, "history.undo", json!({}));
        assert_eq!(s.design(), &before, "{name}: one undo step");
    }
}

/// `first` says which pattern comes first, as the Point tool's Alt+drag of an exit point onto
/// another pattern's entry point gives it; the joined pattern keeps that one's id and takes
/// the earlier place in the sewing order.
#[test]
fn connect_with_first_sews_that_pattern_first() {
    let mut s = session();
    let entry_of = |s: &mut Session, id: u64| {
        let pts = points(s, id);
        let p = pts
            .iter()
            .find(|p| p["command"] == "stitch" || p["command"] == "jump")
            .expect("entry")
            .clone();
        (p["x"].as_f64().expect("x"), p["y"].as_f64().expect("y"))
    };
    let (red, blue) = (entry_of(&mut s, 1), entry_of(&mut s, 2));
    // Blue (sewn second) first: the joined pattern starts with blue, keeps blue's id, and is
    // where red was in the sewing order.
    let r = run(
        &mut s,
        "stitches.connect",
        json!({"ids": [1, 2], "first": 2}),
    );
    assert_eq!(r["id"], json!(2));
    let d = s.design();
    assert_eq!(d.objects.len(), 1);
    assert_eq!(d.objects[0].id, 2);
    assert_eq!(entry_of(&mut s, 2), blue);
    assert_ne!(blue, red);
    run(&mut s, "history.undo", json!({}));
    // Without first: the one sewn earlier.
    let r = run(&mut s, "stitches.connect", json!({"ids": [2, 1]}));
    assert_eq!(r["id"], json!(1));
    assert_eq!(entry_of(&mut s, 1), red);
    run(&mut s, "history.undo", json!({}));
    // first must be one of the two.
    let e = s
        .execute(
            "stitches.connect",
            Some(&json!({"ids": [1, 2], "first": 9})),
        )
        .expect_err("not one of them")
        .to_string();
    assert!(e.contains("not one of ids"), "{e}");
}
