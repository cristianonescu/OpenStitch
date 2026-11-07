//! Hand edits kept on live objects (#501): needle points of a fill moved with the stitches.*
//! commands while it stays a fill; they survive a move and a save, are dropped (and reported)
//! when a sewing setting changes, and object.regenerate drops them.

use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, p: Value) -> Value {
    s.execute(id, Some(&p))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fill(s: &mut Session) -> u64 {
    let r = run(
        s,
        "shape.rectangle",
        json!({ "x": 10, "y": 10, "width": 20, "height": 20 }),
    );
    r["id"].as_u64().expect("id")
}

fn sews(s: &Session, x: f64, y: f64) -> bool {
    let d = s.design();
    let p = openstitch_design::Compiler::for_design(d).compile(d);
    p.stitches
        .iter()
        .any(|st| (st.x - x * 10.0).abs() < 1e-6 && (st.y - y * 10.0).abs() < 1e-6)
}

#[test]
fn a_moved_needle_point_on_a_live_fill_survives_a_move_and_a_save() {
    let dir = openstitch_testutil::TempDir::new("engine-hand-edits");
    let mut s = Session::new(dir.to_path_buf());
    let id = fill(&mut s);
    let points = run(&mut s, "stitches.points", json!({ "id": id }));
    assert!(
        points["points"].as_array().is_some_and(|a| a.len() > 50),
        "the fill's stitches"
    );
    assert!(
        s.design().objects[0].hand_edits.is_none(),
        "listing changes nothing"
    );
    run(
        &mut s,
        "stitches.setPoint",
        json!({ "id": id, "index": 5, "x": 12.3, "y": 45.6 }),
    );
    let o = &s.design().objects[0];
    assert!(
        o.hand_edits.is_some() && o.region.is_some(),
        "still a live fill, with edits"
    );
    assert!(sews(&s, 12.3, 45.6), "the edited point is sewn");
    // Moved: the edited point moves with the fill.
    run(
        &mut s,
        "object.move",
        json!({ "ids": [id], "dx": 5, "dy": 0 }),
    );
    assert!(sews(&s, 17.3, 45.6));
    assert!(s.take_hand_edits_dropped().is_empty());
    // Saved and opened again.
    let file = dir.join("edits.osd");
    run(
        &mut s,
        "project.save",
        json!({ "path": file.display().to_string() }),
    );
    let mut t = Session::new(dir.to_path_buf());
    run(
        &mut t,
        "project.open",
        json!({ "path": file.display().to_string() }),
    );
    assert!(sews(&t, 17.3, 45.6), "kept in the .osd");
    // Exported: the edited point is in the machine file.
    let pes = dir.join("edits.dst");
    run(
        &mut t,
        "design.export",
        json!({ "path": pes.display().to_string() }),
    );
    assert!(pes.exists());
}

#[test]
fn a_setting_change_drops_the_edits_and_says_so_and_regenerate_drops_them_too() {
    let dir = openstitch_testutil::TempDir::new("engine-hand-edits");
    let mut s = Session::new(dir.to_path_buf());
    let id = fill(&mut s);
    run(
        &mut s,
        "stitches.setPoint",
        json!({ "id": id, "index": 5, "x": 12.3, "y": 45.6 }),
    );
    let r = run(
        &mut s,
        "object.setRegion",
        json!({ "ids": [id], "density": 2.0 }),
    );
    assert!(
        s.design().objects[0].hand_edits.is_none(),
        "another density: dropped"
    );
    assert!(!sews(&s, 12.3, 45.6));
    let reported = s.take_hand_edits_dropped();
    assert_eq!(reported, vec![id]);
    if r.is_object() {
        assert_eq!(r["handEditsDropped"], json!([id]));
    }
    s.execute("history.undo", None).expect("undo");
    assert!(sews(&s, 12.3, 45.6), "Undo brings the edits back");
    let r = run(&mut s, "object.regenerate", json!({ "ids": [id] }));
    assert_eq!(r["regenerated"], json!([id]));
    assert!(!sews(&s, 12.3, 45.6));
    let r = run(&mut s, "object.regenerate", json!({ "ids": [id] }));
    assert_eq!(r["regenerated"], json!([]), "nothing left to drop");
}

#[test]
fn objects_without_sewing_have_no_points_to_edit() {
    let dir = openstitch_testutil::TempDir::new("engine-hand-edits");
    let mut s = Session::new(dir.to_path_buf());
    let id = fill(&mut s);
    run(
        &mut s,
        "object.setRegion",
        json!({ "ids": [id], "type": "none" }),
    );
    run(
        &mut s,
        "object.setLine",
        json!({ "ids": [id], "type": "none" }),
    );
    assert!(
        s.execute(
            "stitches.setPoint",
            Some(&json!({ "id": id, "index": 0, "x": 1, "y": 1 }))
        )
        .is_err()
    );
}
