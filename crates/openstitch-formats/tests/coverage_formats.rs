//! Coverage for format claims no other test asserts (#329): the machine stitches of a PES
//! version 6 written for a turned hoop, and the PE-DESIGN reference files the comparison tests
//! in `formats.rs` read (they pass without comparing when a file is missing).

mod common;

use common::data;
use openstitch_core::{Command, StitchPattern};
use openstitch_formats::pes;
use std::path::PathBuf;

/// Moves between consecutive needle and jump positions, in 0.1 mm.
fn moves(p: &StitchPattern) -> Vec<(f64, f64)> {
    let points: Vec<(f64, f64)> = p
        .stitches
        .iter()
        .filter(|s| matches!(s.command, Command::Stitch | Command::Jump))
        .map(|s| (s.x, s.y))
        .collect();
    points
        .windows(2)
        .map(|w| (w[1].0 - w[0].0, w[1].1 - w[0].1))
        .collect()
}

#[test]
fn a_turned_hoop_writes_the_pec_stitches_turned_a_quarter() {
    let design = pes::read(&data("machine-formats.pes")).expect("read");
    let write = |hoop_rotated| {
        let options = pes::WriteOptions {
            hoop_rotated,
            ..pes::WriteOptions::default()
        };
        pes::read(&pes::write_with(&design, &options).expect("write")).expect("read back")
    };
    let (upright, turned) = (write(false), write(true));
    assert_eq!(
        upright.count(Command::Stitch),
        turned.count(Command::Stitch)
    );
    assert_eq!(upright.threads, turned.threads, "colours unchanged");

    // Every move turned a quarter: (dx, dy) becomes (-dy, dx), as `pec::turned_for_hoop`
    // turns the design about its centre.
    let (a, b) = (moves(&upright), moves(&turned));
    assert_eq!(a.len(), b.len());
    for (i, (u, t)) in a.iter().zip(&b).enumerate() {
        assert!(
            (t.0 + u.1).abs() <= 1.0 && (t.1 - u.0).abs() <= 1.0,
            "move {i}: {u:?} turned is {t:?}"
        );
    }
    let (ub, tb) = (upright.bounds(), turned.bounds());
    let (uw, uh) = (ub.max_x - ub.min_x, ub.max_y - ub.min_y);
    let (tw, th) = (tb.max_x - tb.min_x, tb.max_y - tb.min_y);
    assert!(
        (tw - uh).abs() <= 2.0 && (th - uw).abs() <= 2.0,
        "width and height swap: {uw} × {uh} → {tw} × {th}"
    );
}

#[test]
fn the_pe_design_reference_files_the_comparison_tests_read_are_present() {
    // `formats.rs` skips its PE-DESIGN comparisons when one of these is missing; a missing
    // file must fail here instead of passing there.
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../compat/reference/machine");
    let mut names: Vec<String> = ["dst", "exp", "hus", "jef", "pcs", "sew", "vip", "xxx"]
        .iter()
        .map(|ext| format!("machine-formats.{ext}"))
        .collect();
    names.extend(
        [
            "sew-a-moved10.pes",
            "sew-a-moved10.sew",
            "sew-square-centre.pes",
            "sew-square-centre.sew",
        ]
        .map(String::from),
    );
    for name in names {
        let path = dir.join(&name);
        let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        assert!(len > 0, "PE-DESIGN reference {} is missing", path.display());
    }
}
