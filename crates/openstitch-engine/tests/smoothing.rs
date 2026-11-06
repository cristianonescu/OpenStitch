//! Smooth Shape and Smooth Stitches (#478).

use openstitch_core::{Command, Stitch};
use openstitch_design::simplify::hausdorff;
use openstitch_design::{Compiler, DesignObject, StitchBlock, v2};
use openstitch_engine::Session;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fails(s: &mut Session, id: &str, params: Value) -> String {
    match s.execute(id, Some(&params)) {
        Ok(v) => panic!("{id} should fail, gave {v}"),
        Err(e) => e.to_string(),
    }
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

/// A session whose design holds `blocks` as stitch patterns 1, 2, … (opened from a saved file).
fn with_stitches(blocks: Vec<StitchBlock>) -> (Session, Vec<u64>, openstitch_testutil::TempDir) {
    let dir = openstitch_testutil::TempDir::new("shape-edit");
    let mut d = openstitch_design::Design::default();
    let ids = blocks
        .into_iter()
        .map(|b| d.add(DesignObject::stitches(b)))
        .collect();
    std::fs::write(
        dir.path().join("s.osd"),
        openstitch_design::project::encode(&d).expect("encode"),
    )
    .expect("write");
    let mut s = Session::new(dir.path());
    run(&mut s, "project.open", json!({ "path": "s.osd" }));
    (s, ids, dir)
}

fn block(stitches: Vec<Stitch>) -> StitchBlock {
    StitchBlock {
        stitches,
        threads: vec![openstitch_core::Thread::rgb(0)],
    }
}

fn object(s: &Session, id: u64) -> &DesignObject {
    let d = s.design();
    &d.objects[d.index_of(id).expect("object")]
}

/// A traced circle of radius 20 mm: 240 vertices with ±0.05 mm noise.
fn traced(s: &mut Session) -> u64 {
    let pts: Vec<Value> = (0..240)
        .map(|i| {
            let t = (i as f64 * 1.5).to_radians();
            let r = 20.0 + if i % 2 == 0 { 0.05 } else { -0.05 };
            json!([50.0 + r * t.cos(), 50.0 + r * t.sin()])
        })
        .collect();
    id(&run(
        s,
        "shape.polygon",
        json!({ "points": pts, "name": "Traced" }),
    ))
}

#[test]
fn smooth_shape_lowers_the_node_count_within_the_tolerance() {
    let mut s = Session::new(std::env::temp_dir());
    let o = traced(&mut s);
    let before = object(&s, o).contours[0].clone();
    let region = object(&s, o).region.clone();
    let r = run(
        &mut s,
        "shape.simplify",
        json!({ "ids": [o], "tolerance": 0.2 }),
    );
    assert_eq!(r["objects"][0]["before"], 240);
    let after = object(&s, o).contours[0].clone();
    assert!(after.points.len() < 80, "{}", after.points.len());
    assert!(hausdorff(&before, &after) <= 2.0 + 1e-9);
    // Sewing attributes and the object stay; one undo brings the nodes back.
    assert_eq!(object(&s, o).region, region);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(object(&s, o).contours[0], before);
    run(&mut s, "history.redo", json!({}));
    // A second pass at the same tolerance removes little.
    let r = run(&mut s, "shape.simplify", json!({ "ids": [o] }));
    let (b, a) = (
        r["objects"][0]["before"].as_u64().expect("value"),
        r["objects"][0]["after"].as_u64().expect("value"),
    );
    assert!(a * 10 >= b * 9, "{b} -> {a}");
}

#[test]
fn smooth_shape_takes_one_direction_line_per_pass_from_a_column() {
    let mut s = Session::new(std::env::temp_dir());
    let lines: Vec<Value> = (0..5)
        .map(|i| json!([[10.0 + i as f64 * 4.0, 10.0], [10.0 + i as f64 * 4.0, 14.0]]))
        .collect();
    let c = id(&run(&mut s, "shape.column", json!({ "lines": lines })));
    for left in [4, 3, 2, 2] {
        run(&mut s, "shape.simplify", json!({ "ids": [c] }));
        let o = object(&s, c);
        let r = o.region.as_ref().expect("region");
        assert_eq!(r.direction_lines.len(), left);
        assert_eq!(
            o.contours[0],
            openstitch_design::column::outline(&r.direction_lines)
        );
    }
    let d = s.design();
    assert!(Compiler::for_design(d).compile(d).stitches.len() > 10);
}

#[test]
fn smooth_shape_refuses_text_and_stitches() {
    let (mut s, ids, _dir) = with_stitches(vec![block(vec![
        Stitch::new(0.0, 0.0, Command::Stitch),
        Stitch::new(10.0, 0.0, Command::Stitch),
    ])]);
    let st = ids[0];
    assert!(fails(&mut s, "shape.simplify", json!({ "ids": [st] })).contains("stitches.smooth"));
}

/// A jagged run: a straight line with every other point 1 mm off, then a trim and a jump.
fn jagged() -> StitchBlock {
    let mut stitches = vec![Stitch::new(0.0, 0.0, Command::Jump)];
    for i in 0..=20 {
        let y = if i % 2 == 1 { 10.0 } else { 0.0 };
        stitches.push(Stitch::new(i as f64 * 20.0, y, Command::Stitch));
    }
    stitches.push(Stitch::new(400.0, 0.0, Command::Trim));
    stitches.push(Stitch::new(500.0, 50.0, Command::Jump));
    stitches.push(Stitch::new(520.0, 50.0, Command::Stitch));
    block(stitches)
}

fn turning(points: &[Stitch]) -> f64 {
    let p: Vec<_> = points.iter().map(|s| v2(s.x, s.y)).collect();
    let mut total = 0.0;
    for w in p.windows(3) {
        let (a, b) = (w[1] - w[0], w[2] - w[1]);
        let cross = a.x * b.y - a.y * b.x;
        let dot = a.x * b.x + a.y * b.y;
        total += cross.atan2(dot).abs();
    }
    total / (p.len() - 2) as f64
}

#[test]
fn smooth_stitches_keeps_points_and_fixed_ends_and_evens_the_run() {
    let (mut s, ids, _dir) = with_stitches(vec![jagged()]);
    let st = ids[0];
    let before = object(&s, st).stitch_block.clone().expect("block").stitches;
    let r = run(
        &mut s,
        "stitches.smooth",
        json!({ "id": st, "iterations": 3 }),
    );
    // Every stitch but the last of the run (the jump's landing is the run's start).
    assert_eq!(r["moved"], 20);
    let after = object(&s, st).stitch_block.clone().expect("block").stitches;
    assert_eq!(after.len(), before.len());
    for k in [0, 21, 22, 23, 24] {
        assert_eq!(
            (after[k].x, after[k].y),
            (before[k].x, before[k].y),
            "entry {k} stays"
        );
    }
    assert!(turning(&after[1..22]) < turning(&before[1..22]) / 2.0);
    // No stitch got longer.
    let longest = |v: &[Stitch]| {
        v[1..22]
            .windows(2)
            .map(|w| v2(w[0].x, w[0].y).distance(v2(w[1].x, w[1].y)))
            .fold(0.0, f64::max)
    };
    assert!(longest(&after) <= longest(&before) + 1e-9);
    // One undo step.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(
        object(&s, st)
            .stitch_block
            .as_ref()
            .expect("value")
            .stitches,
        before
    );
    // A sub-range moves only its inside points.
    let r = run(
        &mut s,
        "stitches.smooth",
        json!({ "id": st, "from": 5, "to": 9 }),
    );
    assert_eq!(r["moved"], 3);
}

#[test]
fn a_closed_run_smooths_all_round() {
    let mut stitches: Vec<Stitch> = (0..12)
        .map(|i| {
            let t = (i as f64 * 30.0).to_radians();
            let r = if i % 2 == 0 { 100.0 } else { 80.0 };
            Stitch::new(r * t.cos(), r * t.sin(), Command::Stitch)
        })
        .collect();
    stitches.push(stitches[0]);
    let (mut s, ids, _dir) = with_stitches(vec![block(stitches)]);
    let st = ids[0];
    let r = run(&mut s, "stitches.smooth", json!({ "id": st }));
    assert_eq!(r["moved"], 13);
    let b = object(&s, st).stitch_block.clone().expect("value").stitches;
    assert_eq!((b[0].x, b[0].y), (b[12].x, b[12].y));
    assert!(v2(b[0].x, b[0].y).length() < 100.0);
}
