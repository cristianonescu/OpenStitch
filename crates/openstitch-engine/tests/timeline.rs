//! `design.timeline` (#516): colour blocks, trims and object starts match the compiled pattern,
//! and the time at the end matches `design.info`'s sewing time.

use openstitch_core::Command;
use openstitch_design::Compiler;
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn design() -> Session {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "machine.set", json!({"hoop": "130x180"}));
    for (x, color) in [(10, "Red"), (80, "Red"), (40, "Blue")] {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 20, "width": 25, "height": 25, "fill": color}),
        );
    }
    s
}

#[test]
fn blocks_and_trims_match_the_compiled_pattern() {
    let mut s = design();
    let t = run(&mut s, "design.timeline", json!({}));
    let pattern = Compiler::for_design(s.design()).compile(s.design());
    let stitches = pattern.count(Command::Stitch);
    assert_eq!(t["totalStitches"], json!(stitches));
    let blocks = t["blocks"].as_array().expect("blocks");
    assert_eq!(blocks.len(), pattern.count(Command::ColorChange) + 1, "{t}");
    // Block boundaries and trims, as stitch counts, straight from the pattern.
    let (mut n, mut ends, mut trims) = (0, Vec::new(), Vec::new());
    for st in &pattern.stitches {
        match st.command {
            Command::Stitch => n += 1,
            Command::ColorChange => ends.push(n),
            Command::Trim if trims.last() != Some(&n) => trims.push(n),
            _ => {}
        }
    }
    ends.push(n);
    let got: Vec<usize> = blocks
        .iter()
        .map(|b| b["endStitch"].as_u64().expect("end") as usize)
        .collect();
    assert_eq!(got, ends);
    let got_trims: Vec<usize> = blocks
        .iter()
        .flat_map(|b| b["trims"].as_array().expect("trims").clone())
        .map(|v| v.as_u64().expect("trim") as usize)
        .collect();
    assert_eq!(got_trims, trims);
    assert!(!trims.is_empty(), "the red squares are far apart: a trim");
    assert_eq!(blocks[0]["endKind"], "colorChange");
    assert_eq!(blocks.last().expect("last")["endKind"], "end");
    assert!(
        blocks[0]["color"]
            .as_str()
            .is_some_and(|c| c.starts_with('#'))
    );
    let objects: Vec<_> = blocks
        .iter()
        .flat_map(|b| b["objects"].as_array().expect("objects").clone())
        .collect();
    assert_eq!(objects.len(), 3, "{t}");
    assert_eq!(objects[0]["startStitch"], json!(0));
}

#[test]
fn time_at_the_end_equals_the_sewing_time() {
    let mut s = design();
    run(
        &mut s,
        "machine.set",
        json!({"machine": "multi", "speed": 700}),
    );
    let t = run(&mut s, "design.timeline", json!({"at": 10}));
    let info = run(&mut s, "design.info", json!({}));
    let minutes = info["estimates"]["sewingMinutes"]
        .as_f64()
        .expect("minutes");
    let total = t["totalSeconds"].as_f64().expect("seconds");
    assert_eq!((total / 6.0).round() / 10.0, minutes, "{t} {info}");
    let pattern = Compiler::for_design(s.design()).compile(s.design());
    let e = openstitch_design::estimate(&pattern, &s.design().machine);
    assert!(
        (total - e.seconds).abs() <= 0.05,
        "to the second: {total} vs {}",
        e.seconds
    );
    assert_eq!(t["at"]["stitch"], json!(10));
    assert_eq!(t["at"]["block"], json!(0));
    assert!(t["at"]["seconds"].as_f64().expect("at") > 0.0);
    assert!(
        s.execute("design.timeline", Some(&json!({"at": 1_000_000})))
            .is_err()
    );
}
