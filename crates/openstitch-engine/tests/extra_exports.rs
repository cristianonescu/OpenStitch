//! Export extras (#488): stitch lists, G-code and export-time repeats through `design.export`.

#![cfg(feature = "formats")]

use openstitch_core::Command;
use openstitch_design::Compiler;
use openstitch_engine::Session;
use openstitch_formats::{FileType, gcode, stitchlist};
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A filled square, an outlined circle in another colour, and a satin-like line.
fn design(tag: &str) -> (Session, TempDir) {
    let dir = TempDir::new(&format!("extra-exports-{tag}"));
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 15, "fill": "#ff0000"}),
    );
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 45, "cy": 20, "rx": 8, "ry": 8, "outline": "#0000ff"}),
    );
    (s, dir)
}

#[test]
fn stitch_lists_have_a_row_per_record_that_sum_to_the_end_and_match_design_info() {
    let (mut s, dir) = design("list");
    let pattern = Compiler::for_design(s.design()).compile(s.design());
    let info = run(&mut s, "design.info", json!({}));
    for ext in ["csv", "txt"] {
        let name = format!("stitches.{ext}");
        run(&mut s, "design.export", json!({"path": name}));
        let text = std::fs::read_to_string(dir.join(&name)).expect("list");
        let rows = stitchlist::parse(&text).expect("parse");
        assert_eq!(rows.len(), pattern.stitches.len(), "{ext}");
        let (sx, sy) = rows.iter().fold((0.0, 0.0), |a, r| (a.0 + r.4, a.1 + r.5));
        let end = pattern.stitches.last().expect("end");
        // Exactly the last row's position (the deltas are between rounded positions), and
        // that is the end of the stream to 0.01 mm.
        let last = rows.last().expect("row");
        assert!(
            (sx - last.2).abs() < 1e-6 && (sy - last.3).abs() < 1e-6,
            "{ext}"
        );
        assert!(
            (last.2 - end.x / 10.0).abs() <= 0.005 && (last.3 - end.y / 10.0).abs() <= 0.005,
            "{ext}"
        );
        let count = |name: &str| rows.iter().filter(|r| r.1 == name).count() as u64;
        let stats = &info["stitchStats"];
        assert_eq!(
            count("stitch"),
            stats["stitches"].as_u64().expect("n"),
            "{ext}"
        );
        assert_eq!(count("jump"), stats["jumps"].as_u64().expect("n"), "{ext}");
        assert_eq!(count("trim"), stats["trims"].as_u64().expect("n"), "{ext}");
        assert_eq!(
            count("colorChange"),
            stats["colorChanges"].as_u64().expect("n"),
            "{ext}"
        );
        assert_eq!(rows.last().expect("row").6, 2, "{ext}: two colour blocks");
    }
}

#[test]
fn gcode_reads_back_to_the_same_needle_points_and_colour_changes() {
    let (mut s, dir) = design("gcode");
    let pattern = Compiler::for_design(s.design()).compile(s.design());
    run(
        &mut s,
        "design.export",
        json!({"path": "d.gcode", "gcode": {"needle": "M3", "profile": "bench"}}),
    );
    let parsed =
        gcode::parse(&std::fs::read_to_string(dir.join("d.gcode")).expect("g")).expect("parse");
    let want: Vec<(f64, f64, bool)> = pattern
        .stitches
        .iter()
        .filter(|s| matches!(s.command, Command::Stitch | Command::Jump))
        .map(|s| (s.x / 10.0, -s.y / 10.0, s.command == Command::Stitch))
        .collect();
    assert_eq!(parsed.moves.len(), want.len());
    for (a, b) in parsed.moves.iter().zip(&want) {
        assert!(
            (a.0 - b.0).abs() <= 0.01 && (a.1 - b.1).abs() <= 0.01,
            "{a:?} {b:?}"
        );
        assert_eq!(a.2, b.2);
    }
    assert_eq!(parsed.color_changes, pattern.count(Command::ColorChange));
    let err = s
        .execute(
            "design.export",
            Some(&json!({"path": "d.dst", "gcode": {}})),
        )
        .expect_err("gcode options on DST");
    assert!(err.to_string().contains("gcode"), "{err}");
}

#[test]
fn a_two_by_three_repeat_fits_the_hoop_and_leaves_the_design_unchanged() {
    let (mut s, dir) = design("repeat");
    let before = run(&mut s, "design.info", json!({}));
    let objects = s.design().objects.len();
    let one = Compiler::for_design(s.design()).compile(s.design());
    let b = one.bounds();
    let out = run(
        &mut s,
        "design.export",
        json!({"path": "r.dst", "repeat": {"columns": 2, "rows": 3, "gapMm": 5}}),
    );
    assert_eq!(out["fitsPage"], json!(true), "{out}");
    let back = FileType::Dst
        .read(&std::fs::read(dir.join("r.dst")).expect("dst"))
        .expect("read");
    assert_eq!(back.count(Command::Stitch), 6 * one.count(Command::Stitch));
    // No extra colour changes: each colour is sewn on all six copies first.
    assert_eq!(
        back.count(Command::ColorChange),
        one.count(Command::ColorChange)
    );
    let rb = back.bounds();
    let (w, h) = (2.0 * b.width() + 50.0, 3.0 * b.height() + 100.0);
    assert!(
        (rb.width() - w).abs() <= 2.0 && (rb.height() - h).abs() <= 2.0,
        "{rb:?} vs {w} × {h}"
    );
    assert_eq!(
        out["stats"]["stitches"].as_u64(),
        Some(6 * one.count(Command::Stitch) as u64)
    );
    // The design itself is as it was.
    assert_eq!(s.design().objects.len(), objects);
    assert_eq!(
        run(&mut s, "design.info", json!({}))["stitchStats"],
        before["stitchStats"]
    );

    // Too large for the hoop: refused with the size it needs.
    let err = s
        .execute(
            "design.export",
            Some(&json!({"path": "big.dst", "repeat": {"columns": 20, "rows": 20}})),
        )
        .expect_err("too large");
    assert!(err.to_string().contains("the repeat needs"), "{err}");
    assert!(!dir.join("big.dst").exists());
    for bad in [
        json!({"columns": 0}),
        json!({"rows": 21}),
        json!({"gapMm": -1}),
    ] {
        assert!(
            s.execute(
                "design.export",
                Some(&json!({"path": "x.dst", "repeat": bad}))
            )
            .is_err()
        );
    }
}

#[test]
fn a_repeat_works_for_stitch_lists_too() {
    let (mut s, dir) = design("repeat-list");
    let one = Compiler::for_design(s.design()).compile(s.design());
    run(
        &mut s,
        "design.export",
        json!({"path": "r.csv", "repeat": {"columns": 2}}),
    );
    let rows = stitchlist::parse(&std::fs::read_to_string(dir.join("r.csv")).expect("csv"))
        .expect("parse");
    let stitches = rows.iter().filter(|r| r.1 == "stitch").count();
    assert_eq!(stitches, 2 * one.count(Command::Stitch));
}
