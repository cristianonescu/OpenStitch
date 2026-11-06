//! Open-time benchmark for large files (#558): ignored by default, run with
//! `cargo test -p openstitch-app --release open_bench -- --ignored --nocapture`.

use super::*;
use openstitch_core::{Command, StitchPattern, Thread};
use std::time::Instant;

/// A DST-sized pattern of `n` stitches in a 100 x 100 mm zigzag, a colour change every 50k.
fn big_pattern(n: usize) -> StitchPattern {
    let mut p = StitchPattern::new();
    p.threads.push(Thread::rgb(0x20_40_80));
    for i in 0..n {
        if i > 0 && i % 50_000 == 0 {
            p.push(Command::ColorChange);
            p.threads.push(Thread::rgb(0x20_40_80));
        }
        let row = (i / 400) as f64 * 0.4;
        let col = (i % 400) as f64 * 0.25;
        p.stitch_to(col, (row % 100.0) + if i % 2 == 0 { 0.0 } else { 0.4 });
    }
    p.push(Command::End);
    p
}

#[test]
#[ignore = "benchmark: run by hand with --ignored --nocapture"]
fn open_bench_large_dst_and_osd() {
    for n in [500_000, 1_000_000] {
        open_bench(n);
    }
}

fn open_bench(n: usize) {
    let dir = openstitch_testutil::TempDir::new("app-open-bench");
    let dst = dir.join("big.dst");
    let bytes = openstitch_formats::dst::write(&big_pattern(n)).expect("dst");
    std::fs::write(&dst, bytes).expect("write dst");

    let mut app = EditorApp::new(Launch::default());
    let t = Instant::now();
    app.open_path(&dst);
    let dst_open = t.elapsed();
    assert!(!app.session.design().objects.is_empty(), "{}", app.status);

    let osd = dir.join("big.osd");
    app.execute(&CommandRequest::new(
        "project.save",
        json!({ "path": osd.display().to_string() }),
    ))
    .expect("save");
    let mut app = EditorApp::new(Launch::default());
    let t = Instant::now();
    app.open_path(&osd);
    let osd_open = t.elapsed();
    println!(
        "open {n}-stitch DST: {dst_open:?}; reopen as .osd ({} bytes): {osd_open:?}",
        std::fs::metadata(&osd).map_or(0, |m| m.len())
    );
}
