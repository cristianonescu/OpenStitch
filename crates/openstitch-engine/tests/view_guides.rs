//! Guideline commands (#799): new, move, edit (Set Position…, turn), delete, clear, lock and
//! new guides from the selection, each one undo step, saved with the design.

use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn list(s: &mut Session) -> Value {
    run(s, "guides.list", json!({}))
}

#[test]
fn guidelines_are_added_moved_edited_turned_and_deleted() {
    let dir = TempDir::new("view-guides");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "view.newGuide",
        json!({"orientation": "vertical", "position": 25}),
    );
    run(
        &mut s,
        "view.newGuide",
        json!({"orientation": "horizontal", "position": -1.5}),
    );
    // The same place twice is one guideline.
    run(
        &mut s,
        "view.newGuide",
        json!({"orientation": "vertical", "position": 25}),
    );
    assert_eq!(list(&mut s)["vertical"], json!([25.0]));
    assert_eq!(list(&mut s)["horizontal"], json!([-1.5]));
    run(
        &mut s,
        "view.moveGuide",
        json!({"orientation": "vertical", "index": 0, "position": 30}),
    );
    run(
        &mut s,
        "view.editGuide",
        json!({"orientation": "horizontal", "index": 0, "position": 12.7}),
    );
    assert_eq!(list(&mut s)["vertical"], json!([30.0]));
    assert_eq!(list(&mut s)["horizontal"], json!([12.7]));
    // Turned: the horizontal one becomes vertical at the given x.
    run(
        &mut s,
        "view.editGuide",
        json!({"orientation": "horizontal", "index": 0, "turn": true, "position": 40}),
    );
    assert_eq!(list(&mut s)["vertical"], json!([30.0, 40.0]));
    assert_eq!(list(&mut s)["horizontal"], json!([]));
    run(
        &mut s,
        "view.deleteGuide",
        json!({"orientation": "vertical", "index": 0}),
    );
    assert_eq!(list(&mut s)["vertical"], json!([40.0]));
    // Each step is one undo.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(list(&mut s)["vertical"], json!([30.0, 40.0]));
    run(&mut s, "history.undo", json!({}));
    assert_eq!(list(&mut s)["horizontal"], json!([12.7]));
    // Refused without changing anything.
    let before = list(&mut s);
    for (id, params) in [
        (
            "view.newGuide",
            json!({"orientation": "diagonal", "position": 1}),
        ),
        (
            "view.moveGuide",
            json!({"orientation": "vertical", "index": 5, "position": 1}),
        ),
        (
            "view.editGuide",
            json!({"orientation": "vertical", "index": 0}),
        ),
        (
            "view.deleteGuide",
            json!({"orientation": "horizontal", "index": 3}),
        ),
    ] {
        assert!(s.execute(id, Some(&params)).is_err(), "{id} {params}");
    }
    assert_eq!(list(&mut s), before);
}

#[test]
fn locked_guidelines_stay_until_unlocked_and_clear_removes_all_in_one_step() {
    let dir = TempDir::new("view-guides-lock");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "view.newGuide",
        json!({"orientation": "vertical", "position": 10}),
    );
    run(&mut s, "view.lockGuides", json!({"locked": true}));
    assert_eq!(list(&mut s)["locked"], json!(true));
    for (id, params) in [
        (
            "view.moveGuide",
            json!({"orientation": "vertical", "index": 0, "position": 20}),
        ),
        (
            "view.deleteGuide",
            json!({"orientation": "vertical", "index": 0}),
        ),
        (
            "view.editGuide",
            json!({"orientation": "vertical", "index": 0, "turn": true}),
        ),
    ] {
        assert!(s.execute(id, Some(&params)).is_err(), "{id} while locked");
    }
    // A new one may still be added; the lock is saved with the design.
    run(
        &mut s,
        "view.newGuide",
        json!({"orientation": "horizontal", "position": 5}),
    );
    run(&mut s, "project.save", json!({"path": "g.osd"}));
    let mut o = Session::new(dir.path());
    run(&mut o, "project.open", json!({"path": "g.osd"}));
    let back = list(&mut o);
    assert_eq!(back["locked"], json!(true));
    assert_eq!(back["vertical"], json!([10.0]));
    assert_eq!(back["horizontal"], json!([5.0]));
    // Clear Guides removes them all, as one undo step.
    run(&mut o, "view.clearGuides", json!({}));
    assert_eq!(list(&mut o)["vertical"], json!([]));
    run(&mut o, "history.undo", json!({}));
    assert_eq!(list(&mut o)["vertical"], json!([10.0]));
    run(&mut o, "view.lockGuides", json!({"locked": false}));
    run(
        &mut o,
        "view.deleteGuide",
        json!({"orientation": "vertical", "index": 0}),
    );
    assert_eq!(list(&mut o)["vertical"], json!([]));
}

#[test]
fn new_guides_from_the_selection_line_its_edges_and_centre() {
    let dir = TempDir::new("view-guides-selection");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 20, "width": 40, "height": 20}),
    );
    run(&mut s, "view.newGuidesFromSelection", json!({"ids": [1]}));
    let g = list(&mut s);
    assert_eq!(g["vertical"], json!([10.0, 30.0, 50.0]));
    assert_eq!(g["horizontal"], json!([20.0, 30.0, 40.0]));
    // Again: nothing doubled.
    run(&mut s, "view.newGuidesFromSelection", json!({"ids": [1]}));
    assert_eq!(list(&mut s), g);
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    assert_eq!(list(&mut s)["vertical"], json!([]));
}

/// #799: New Guide Layout… lines the margins and each column's and row's edges (shared where
/// there is no gutter), in one undo step; clear drops the old ones first; a layout that does
/// not fit is refused.
#[test]
fn a_guide_layout_lines_margins_columns_and_rows() {
    let dir = TempDir::new("view-guides-layout");
    let mut s = Session::new(dir.path());
    let (w, h) = (s.design().page.width / 10.0, s.design().page.height / 10.0);
    run(
        &mut s,
        "view.newGuide",
        json!({"orientation": "vertical", "position": 1}),
    );
    let out = run(
        &mut s,
        "view.guideLayout",
        json!({"columns": 2, "gutter": 10, "margin": 10, "clear": true}),
    );
    let cell = (w - 20.0 - 10.0) / 2.0;
    let close = |got: &Value, want: &[f64]| {
        let got: Vec<f64> = serde_json::from_value(got.clone()).expect("numbers");
        assert_eq!(got.len(), want.len(), "{got:?} {want:?}");
        for (g, w) in got.iter().zip(want) {
            assert!((g - w).abs() < 1e-9, "{got:?} {want:?}");
        }
    };
    close(
        &out["vertical"],
        &[10.0, 10.0 + cell, 20.0 + cell, w - 10.0],
    );
    close(&out["horizontal"], &[10.0, h - 10.0]);
    run(&mut s, "history.undo", json!({}));
    close(&list(&mut s)["vertical"], &[1.0]);
    // Rows without a gutter share their edges.
    let out = run(
        &mut s,
        "view.guideLayout",
        json!({"rows": 3, "clear": true}),
    );
    close(&out["horizontal"], &[0.0, h / 3.0, h * 2.0 / 3.0, h]);
    assert!(out["vertical"].as_array().expect("list").is_empty());
    for bad in [
        json!({"columns": 2, "margin": 1000}),
        json!({"columns": 50, "gutter": 100}),
        json!({"columns": 51}),
        json!({"gutter": -1}),
    ] {
        assert!(s.execute("view.guideLayout", Some(&bad)).is_err(), "{bad}");
    }
}
