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

/// The first records of satin K at 8 mm (stroke-satin-K-8) on a `width` x `height` mm page:
/// the move onto the letter and the stitches after it, in 0.1 mm. `hoop` is chosen with
/// `machine.set`; `mode` is `design.new`'s `pageMode`.
fn k8_start(width: u32, height: u32, hoop: Option<&str>, mode: &str) -> Vec<(Command, f64, f64)> {
    let dir = TempDir::new("tie-in-page");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "design.new",
        json!({"width": width, "height": height, "pageMode": mode}),
    );
    if let Some(hoop) = hoop {
        run(&mut s, "machine.set", json!({ "hoop": hoop }));
    }
    assert_eq!(
        s.design().hoop_size_page(),
        mode == "hoop" && (hoop.is_some() || (width, height) == (100, 100))
    );
    run(
        &mut s,
        "text.add",
        json!({"text": "K", "font": "lato-bold", "size": 8, "x": 20, "y": 70, "fill": "Red"}),
    );
    let pattern = Compiler::for_design(s.design()).compile(s.design());
    let first = pattern
        .stitches
        .iter()
        .position(|st| st.command == Command::Stitch)
        .expect("stitches");
    pattern.stitches[first - 1..first + 4]
        .iter()
        .map(|st| (st.command, st.x, st.y))
        .collect()
}

#[test]
fn a_one_step_tie_in_starts_on_a_stitch_in_place_on_a_hoop_size_page() {
    // stroke-satin-K-8 and every one-step tie-in on PE-DESIGN 11.50's Hoop Size pages: the
    // move onto the start, a stitch in place there, 0.3 mm out and straight back (#1013). On a
    // Custom Size page (every reference before 2026-10-30) the tie-in starts on the landing.
    let d = |a: (Command, f64, f64), b: (Command, f64, f64)| (a.1 - b.1).hypot(a.2 - b.2);
    // PE-DESIGN's default New page, and a chosen hoop on a larger page.
    for hoop in [
        k8_start(100, 100, None, "hoop"),
        k8_start(200, 200, Some("130x180"), "hoop"),
    ] {
        assert!(
            matches!(hoop[0].0, Command::Jump | Command::Trim),
            "{hoop:?}"
        );
        assert!(
            hoop[1..4].iter().all(|st| st.0 == Command::Stitch),
            "{hoop:?}"
        );
        assert!(d(hoop[0], hoop[1]) < 0.05, "a stitch in place: {hoop:?}");
        assert!((d(hoop[1], hoop[2]) - 3.0).abs() < 0.5, "{hoop:?}");
        assert!(d(hoop[1], hoop[3]) < 0.05, "back to the start: {hoop:?}");
        assert!(d(hoop[3], hoop[4]) > 6.0, "then the first stitch: {hoop:?}");
    }
    // A larger page with no hoop, and a 100 x 100 page set to Custom Size.
    for custom in [
        k8_start(200, 200, None, "hoop"),
        k8_start(100, 100, None, "custom"),
    ] {
        assert!(
            (d(custom[0], custom[1]) - 3.0).abs() < 0.5,
            "no stitch in place: {custom:?}"
        );
        assert!(d(custom[0], custom[2]) < 0.05, "{custom:?}");
    }
    let dir = TempDir::new("tie-in-page-bad");
    let mut s = Session::new(dir.path());
    assert!(
        s.execute("design.new", Some(&json!({"pageMode": "square"})))
            .is_err()
    );
}

/// The first records after a DST's opening jumps: (stitch?, dx, dy) in 0.1 mm.
fn dst_start(mode: &str) -> Vec<(bool, i32, i32)> {
    let dir = TempDir::new("tie-in-dst");
    let mut s = Session::new(dir.path());
    run(&mut s, "design.new", json!({ "pageMode": mode }));
    run(
        &mut s,
        "text.add",
        json!({"text": "K", "font": "lato-bold", "size": 8, "x": 20, "y": 70, "fill": "Red"}),
    );
    run(&mut s, "design.export", json!({"path": "k.dst"}));
    let bytes = std::fs::read(dir.path().join("k.dst")).expect("dst");
    let bit = |b: u8, k: u8| i32::from(b >> k & 1);
    let (frames, _) = bytes[512..].as_chunks::<3>();
    let records: Vec<(bool, i32, i32)> = frames
        .iter()
        .map(|&[b0, b1, b2]| {
            let dx = bit(b0, 0) - bit(b0, 1)
                + 9 * (bit(b0, 2) - bit(b0, 3))
                + 3 * (bit(b1, 0) - bit(b1, 1))
                + 27 * (bit(b1, 2) - bit(b1, 3))
                + 81 * (bit(b2, 2) - bit(b2, 3));
            let dy = bit(b0, 7) - bit(b0, 6)
                + 9 * (bit(b0, 5) - bit(b0, 4))
                + 3 * (bit(b1, 7) - bit(b1, 6))
                + 27 * (bit(b1, 5) - bit(b1, 4))
                + 81 * (bit(b2, 5) - bit(b2, 4));
            (b2 & 0xC0 == 0, dx, dy)
        })
        .collect();
    let first = records.iter().position(|r| r.0).expect("a stitch");
    records[first..first + 3].to_vec()
}

#[test]
fn a_dst_export_carries_the_stitch_in_place_of_a_hoop_size_page() {
    // The stitch in place is in the compiled stitches, so every machine format writes it: on a
    // Hoop Size page the DST's first stitch after the move is zero-length, then 0.3 mm out and
    // back (#1013); on a Custom Size page it starts with the 0.3 mm step.
    let hoop = dst_start("hoop");
    assert_eq!(hoop[0], (true, 0, 0), "{hoop:?}");
    assert_eq!((hoop[1].1.abs() + hoop[1].2.abs()), 3, "{hoop:?}");
    assert_eq!(
        (hoop[1].1 + hoop[2].1, hoop[1].2 + hoop[2].2),
        (0, 0),
        "{hoop:?}"
    );
    let custom = dst_start("custom");
    assert_eq!((custom[0].1.abs() + custom[0].2.abs()), 3, "{custom:?}");
    assert_eq!(
        (custom[0].1 + custom[1].1, custom[0].2 + custom[1].2),
        (0, 0),
        "{custom:?}"
    );
}
