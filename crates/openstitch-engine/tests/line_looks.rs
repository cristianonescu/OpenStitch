//! Hand-made line looks through the commands (#540, #542): back stitch, a running stitch along
//! a thickness profile, the hand-stitch look on lines and satins, the scribble line.

use openstitch_core::{Command, Stitch};
use openstitch_design::Compiler;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn stitches(s: &Session) -> Vec<Stitch> {
    Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
}

fn needles(s: &Session) -> usize {
    stitches(s)
        .iter()
        .filter(|st| st.command == Command::Stitch)
        .count()
}

/// A 40 mm open line (id 1).
fn line(s: &mut Session) {
    run(
        s,
        "shape.path",
        json!({"points": [[10, 50], [50, 50]], "closed": false}),
    );
}

#[test]
fn back_stitch_and_a_profiled_running_line() {
    let dir = TempDir::new("line-looks");
    let mut s = Session::new(dir.path());
    line(&mut s);
    let plain = needles(&s);
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "backStitch", "pitch": 2}),
    );
    assert_eq!(
        s.design().objects[0].line.as_ref().map(|l| l.kind.id()),
        Some("backStitch")
    );
    // Two forward and one back: about three times as many needle points as plain running
    // stitch at its 2 mm pitch.
    assert!(needles(&s) > 2 * plain, "{} vs {plain}", needles(&s));
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "running"}),
    );
    let even = needles(&s);
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "profile": "arc", "minPasses": 1, "maxPasses": 5, "passSpacing": 0.3}),
    );
    assert!(
        needles(&s) > 2 * even,
        "the middle is sewn up to five times"
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert_eq!(l.profile.as_ref().map(|p| p.max_passes), Some(5));
    // One undo step back to the even line; "none" turns the profile off.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(needles(&s), even);
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "profile": "linear"}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "profile": "none"}),
    );
    assert!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .profile
            .is_none()
    );
}

#[test]
fn hand_stitch_look_and_scribble_follow_their_seed() {
    let dir = TempDir::new("line-hand");
    let mut s = Session::new(dir.path());
    line(&mut s);
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "hand": true, "handLength": 30, "seed": 7}),
    );
    let a = stitches(&s);
    run(&mut s, "object.setLine", json!({"id": 1, "seed": 7}));
    assert_eq!(stitches(&s), a, "the same seed sews the same stitches");
    run(&mut s, "object.setLine", json!({"id": 1, "shuffle": true}));
    assert_ne!(stitches(&s), a, "shuffle picks another variant");
    run(&mut s, "history.undo", json!({}));
    assert_eq!(stitches(&s), a, "one undo step");
    run(&mut s, "object.setLine", json!({"id": 1, "hand": false}));
    assert!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .hand
            .is_none()
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "scribble", "width": 4, "runTimes": 3, "noiseScale": 6, "seed": 3}),
    );
    let sc = stitches(&s);
    // Inside the 4 mm band round the line at y = 50 mm (500 in 0.1 mm), ties included.
    assert!(
        sc.iter()
            .filter(|st| st.command == Command::Stitch)
            .all(|st| (st.y - 500.0).abs() <= 20.5),
        "{sc:?}"
    );
    run(&mut s, "object.setLine", json!({"id": 1, "shuffle": true}));
    assert_ne!(stitches(&s), sc);
}

#[test]
fn satin_hand_stitch_moves_one_side() {
    let dir = TempDir::new("satin-hand");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 40, "height": 6, "fill": "Red"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "satin"}),
    );
    let crisp = stitches(&s);
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "handSide": "b", "handAmount": 40, "seed": 2}),
    );
    let hand = stitches(&s);
    assert_ne!(hand, crisp);
    // The same crossings, moved; the tie-in and lock may take a step more or less, since
    // they follow the first and last stitch's length.
    assert!(
        hand.len().abs_diff(crisp.len()) <= 4,
        "{} vs {}",
        hand.len(),
        crisp.len()
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "handSide": "none"}),
    );
    assert_eq!(stitches(&s), crisp);
}

#[test]
fn wrong_line_looks_are_refused() {
    let dir = TempDir::new("line-looks-bad");
    let mut s = Session::new(dir.path());
    line(&mut s);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 60, "y": 10, "width": 10, "height": 10, "fill": "Blue"}),
    );
    let before = s.design().clone();
    for (cmd, params) in [
        ("object.setLine", json!({"id": 1, "profile": "wobbly"})),
        (
            "object.setLine",
            json!({"id": 1, "profile": "arc", "maxPasses": 4}),
        ),
        (
            "object.setLine",
            json!({"id": 1, "profile": "arc", "minPasses": 5, "maxPasses": 3}),
        ),
        (
            "object.setLine",
            json!({"id": 1, "profilePoints": [[0.5, 0.2], [0.1, 1]]}),
        ),
        (
            "object.setLine",
            json!({"id": 1, "type": "zigzag", "hand": true}),
        ),
        ("object.setLine", json!({"id": 1, "handLength": 80})),
        ("object.setLine", json!({"id": 1, "seed": 3})),
        (
            "object.setLine",
            json!({"id": 1, "type": "scribble", "noiseScale": 0.2}),
        ),
        (
            "object.setLine",
            json!({"id": 1, "profile": "arc", "hand": true}),
        ),
        ("object.setRegion", json!({"id": 2, "handSide": "a"})),
        ("object.setRegion", json!({"id": 2, "handAmount": 10})),
        (
            "object.setRegion",
            json!({"id": 2, "type": "satin", "handSide": "left"}),
        ),
    ] {
        assert!(s.execute(cmd, Some(&params)).is_err(), "{cmd} {params}");
        assert_eq!(s.design(), &before, "{cmd} {params}");
    }
}
