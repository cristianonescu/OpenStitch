//! `object.setFinish` (#548): trim after, ties, and a text's trims between letters, through
//! the command.

use openstitch_core::Command;
use openstitch_design::Compiler;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn trims(s: &Session) -> usize {
    Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
        .iter()
        .filter(|st| st.command == Command::Trim)
        .count()
}

fn needles(s: &Session) -> usize {
    Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
        .iter()
        .filter(|st| st.command == Command::Stitch)
        .count()
}

/// Two red squares 20 mm apart (ids 1, 2).
fn squares(s: &mut Session) {
    for k in 0..2 {
        run(
            s,
            "shape.rectangle",
            json!({"x": k * 30, "y": 0, "width": 10, "height": 10, "fill": "Red"}),
        );
    }
}

#[test]
fn trim_after_and_ties_through_the_command() {
    let dir = TempDir::new("finish");
    let mut s = Session::new(dir.path());
    squares(&mut s);
    let pe = s.design().clone();
    let (pe_trims, pe_needles) = (trims(&s), needles(&s));
    let rows = run(&mut s, "object.setFinish", json!({"id": 1, "trim": "off"}));
    assert_eq!(rows[0]["trim"], "off");
    assert_eq!(rows[0]["between"], "auto");
    assert_eq!(
        trims(&s),
        pe_trims - 1,
        "the move to the second square jumps"
    );
    // One undo step puts it back.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &pe);
    run(
        &mut s,
        "object.setFinish",
        json!({"ids": [1], "tie": "off"}),
    );
    assert!(
        needles(&s) < pe_needles,
        "no tie-in or lock on the first square"
    );
    run(
        &mut s,
        "object.setFinish",
        json!({"ids": [1], "tie": "auto", "tieCount": 2, "tieLength": 0.6}),
    );
    assert!(needles(&s) > pe_needles, "two steps instead of one");
    // Back to every default: no finish stored, PE-DESIGN's stitches.
    run(&mut s, "object.setFinish", json!({"id": 1, "reset": true}));
    assert!(s.design().objects[0].finish.is_none());
    assert_eq!(s.design(), &pe);
    run(
        &mut s,
        "object.setFinish",
        json!({"id": 1, "tieCount": 3, "trimDistance": 25}),
    );
    run(
        &mut s,
        "object.setFinish",
        json!({"id": 1, "tieCount": null, "trimDistance": null}),
    );
    assert!(s.design().objects[0].finish.is_none());
}

#[test]
fn text_trims_between_words() {
    let dir = TempDir::new("finish-text");
    let mut s = Session::new(dir.path());
    let id = run(
        &mut s,
        "text.add",
        json!({"text": "HI HO", "font": "lato-bold", "size": 10, "fill": "Red"}),
    )["id"]
        .as_u64()
        .expect("id");
    let count = |s: &mut Session, between: &str| {
        run(s, "object.setFinish", json!({"id": id, "between": between}));
        trims(s)
    };
    let never = count(&mut s, "never");
    assert_eq!(count(&mut s, "word") - never, 1);
    assert_eq!(count(&mut s, "letter") - never, 3);
    assert_eq!(count(&mut s, "line"), never, "one line");
    let rows = run(
        &mut s,
        "object.setFinish",
        json!({"id": id, "between": null}),
    );
    assert_eq!(rows[0]["between"], "auto");
}

#[test]
fn bad_finish_parameters_fail_cleanly() {
    let dir = TempDir::new("finish-bad");
    let mut s = Session::new(dir.path());
    squares(&mut s);
    let before = s.design().clone();
    for params in [
        json!({"id": 1, "trim": "sometimes"}),
        json!({"id": 1, "tie": 1}),
        json!({"id": 1, "trimDistance": 51}),
        json!({"id": 1, "tieCount": 0}),
        json!({"id": 1, "tieCount": 1.5}),
        json!({"id": 1, "tieLength": 0.1}),
        json!({"id": 1, "between": "word"}),
        json!({"id": 99, "trim": "off"}),
        json!({"ids": [], "trim": "off"}),
        json!({"ids": [1, 2], "between": "word"}),
    ] {
        assert!(
            s.execute("object.setFinish", Some(&params)).is_err(),
            "{params}"
        );
        assert_eq!(s.design(), &before, "{params}");
    }
}
