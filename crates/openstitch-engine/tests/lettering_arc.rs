//! Arched text as concentric rings (#456), through the commands.

use openstitch_design::Compiler;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn a_three_line_badge_arches_and_straightens_again() {
    let dir = TempDir::new("arc");
    let mut s = Session::new(dir.path());
    let id = run(
        &mut s,
        "text.add",
        json!({"text": "CLUB\n2026\nSTITCH", "size": 8, "arc": 35, "x": 60, "y": 60}),
    )["id"]
        .as_u64()
        .expect("id");
    let o = &s.design().objects[0];
    assert_eq!(o.text.as_ref().and_then(|t| t.arc), Some(35.0));
    let arched = o.contours.clone();
    assert!(
        !Compiler::for_design(s.design())
            .compile(s.design())
            .stitches
            .is_empty()
    );
    // Arching down sews the same letters in another place.
    run(&mut s, "object.setText", json!({"id": id, "arc": -35}));
    assert_ne!(s.design().objects[0].contours, arched);
    // A Text Transform preset replaces the arc; null straightens it.
    run(&mut s, "object.setText", json!({"id": id, "envelope": 3}));
    assert_eq!(s.design().objects[0].text.as_ref().expect("text").arc, None);
    run(&mut s, "object.setText", json!({"id": id, "arc": 35}));
    let t = s.design().objects[0].text.clone().expect("text");
    assert_eq!((t.arc, t.envelope), (Some(35.0), None));
    run(&mut s, "object.setText", json!({"id": id, "arc": null}));
    assert_eq!(s.design().objects[0].text.as_ref().expect("text").arc, None);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(
        s.design().objects[0].text.as_ref().expect("text").arc,
        Some(35.0)
    );
}

#[test]
fn too_tight_or_wrong_arcs_are_refused_with_the_radius_needed() {
    let dir = TempDir::new("arc-bad");
    let mut s = Session::new(dir.path());
    let id = run(
        &mut s,
        "text.add",
        json!({"text": "A LONG LINE OF LETTERS ON A RING", "size": 10}),
    )["id"]
        .as_u64()
        .expect("id");
    let before = s.design().clone();
    let err = s
        .execute("object.setText", Some(&json!({"id": id, "arc": 10})))
        .expect_err("too tight");
    assert!(err.to_string().contains("at least"), "{err}");
    assert_eq!(s.design(), &before);
    for params in [
        json!({"id": id, "arc": 2}),
        json!({"id": id, "arc": 2000}),
        json!({"id": id, "arc": "up"}),
        json!({"id": id, "arc": 40, "envelope": 3}),
        json!({"id": id, "arc": 40, "direction": "vertical"}),
    ] {
        assert!(
            s.execute("object.setText", Some(&params)).is_err(),
            "{params}"
        );
        assert_eq!(s.design(), &before, "{params}");
    }
}
