//! Stitching hidden under later objects, through the commands: leave out hidden stitching
//! (#511), travel under later fills and run connectors (#545).

use openstitch_core::{Command, StitchPattern};
use openstitch_design::{Compiler, Design, Vec2, v2};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn compile(d: &Design) -> StitchPattern {
    Compiler::for_design(d).compile(d)
}

fn rect(s: &mut Session, x: f64, y: f64, w: f64, h: f64, fill: &str) -> u64 {
    let r = run(
        s,
        "shape.rectangle",
        json!({"x": x, "y": y, "width": w, "height": h, "fill": fill}),
    );
    r["id"].as_u64().expect("id")
}

/// Trims before the last needle point: the moves between parts (not the final cut).
fn inner_trims(p: &StitchPattern) -> usize {
    let last = p
        .stitches
        .iter()
        .rposition(|s| s.command == Command::Stitch)
        .expect("stitches");
    p.stitches[..last]
        .iter()
        .filter(|s| s.command == Command::Trim)
        .count()
}

/// The needle points object `id` sews.
fn points_of(d: &Design, id: u64) -> Vec<Vec2> {
    let (p, spans) = Compiler::for_design(d).compile_traced(d);
    let (_, range) = spans.into_iter().find(|(i, _)| *i == id).expect("span");
    p.stitches[range]
        .iter()
        .filter(|s| s.command == Command::Stitch)
        .map(|s| v2(s.x, s.y))
        .collect()
}

#[test]
fn hidden_stitching_is_left_out_with_an_overlap() {
    let tmp = TempDir::new("routing");
    let mut s = Session::new(tmp.path());
    let a = rect(&mut s, 20.0, 20.0, 30.0, 30.0, "#ff0000");
    let b = rect(&mut s, 35.0, 35.0, 30.0, 30.0, "#0000ff");
    let plain = compile(s.design());
    let whole = points_of(s.design(), a).len();
    let shape = s.design().objects[0].contours.clone();
    let r = run(
        &mut s,
        "object.setHideUnder",
        json!({"ids": [a], "on": true, "overlap": 1}),
    );
    let saved = r["stitchesSaved"].as_u64().expect("saved") as f64;
    // 14 x 14 mm of the 30 x 30 mm square is hidden beyond the 1 mm overlap: about that area
    // times the rows per mm over the stitch length.
    let region = s.design().objects[0].region.clone().expect("fill");
    let expected = 14.0 * 14.0 * region.density / region.stitch_length;
    assert!(whole > 0);
    assert!(
        (saved - expected).abs() / expected <= 0.10,
        "saved {saved}, expected about {expected}"
    );
    let pts = points_of(s.design(), a);
    // None deeper than the overlap + 0.2 mm inside the upper square ...
    for q in &pts {
        assert!(
            !(q.x > 362.0 && q.y > 362.0),
            "{q:?} deep under the upper square"
        );
    }
    // ... but some in the overlap band under its edge.
    assert!(pts.iter().any(|q| q.x > 351.0 && q.y > 351.0));
    assert_eq!(s.design().objects[0].contours, shape, "shape not cut");
    let info = run(&mut s, "design.info", json!({}));
    assert_eq!(info["production"]["hiddenStitchesSaved"], saved as u64);
    // Moving the upper square away gives the whole fill back.
    run(&mut s, "object.move", json!({"id": b, "dx": 40}));
    assert_eq!(points_of(s.design(), a).len(), whole);
    run(&mut s, "object.move", json!({"id": b, "dx": -40}));
    // Off: as before.
    run(
        &mut s,
        "object.setHideUnder",
        json!({"ids": [a], "on": false}),
    );
    assert_eq!(compile(s.design()), plain);
    // Design-wide, text and satin excepted.
    let r = run(&mut s, "design.setRemoveHidden", json!({"on": true}));
    assert!(r["stitchesSaved"].as_u64().expect("saved") > 0);
}

#[test]
fn travel_under_a_later_fill_replaces_the_trim() {
    let tmp = TempDir::new("routing");
    let mut s = Session::new(tmp.path());
    // A small square, then a large one of the same thread over it and beyond.
    rect(&mut s, 10.0, 10.0, 10.0, 10.0, "#ff0000");
    rect(&mut s, 5.0, 5.0, 60.0, 25.0, "#ff0000");
    let off = compile(s.design());
    assert_eq!(inner_trims(&off), 1);
    let r = run(&mut s, "design.setTravelUnder", json!({"on": true}));
    assert_eq!(r["trims"]["before"], 2);
    let on = compile(s.design());
    assert_eq!(inner_trims(&on), 0);
    // The travel runs inside the large square, 0.5 mm in from its edge, in short stitches.
    for w in on.stitches.windows(2) {
        if w[0].command == Command::Stitch && w[1].command == Command::Stitch {
            let d = v2(w[0].x, w[0].y).distance(v2(w[1].x, w[1].y));
            assert!(d <= 120.5, "stitch {d}");
        }
    }
    // Another thread: a colour change, not travel.
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": [2], "color": "#0000ff"}),
    );
    assert_eq!(compile(s.design()).count(Command::ColorChange), 1);
    run(&mut s, "design.setTravelUnder", json!({"on": false}));
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": [2], "color": "#ff0000"}),
    );
    assert_eq!(compile(s.design()), off);
}

#[test]
fn travel_runs_along_a_running_line_of_its_thread() {
    // #545: a T drawn as two running lines of one thread, the stem starting on the bar 20 mm
    // from where the bar ends. Off: a trim between them. On: the needle runs back along the
    // bar to the stem, on the line already sewn, and nothing shows outside the two lines.
    let tmp = TempDir::new("routing");
    let mut s = Session::new(tmp.path());
    let line = |s: &mut Session, points: Value| {
        run(
            s,
            "shape.path",
            json!({"points": points, "closed": false, "outline": "#ff0000"}),
        );
    };
    line(&mut s, json!([[10, 10], [50, 10]]));
    line(&mut s, json!([[30, 10], [30, 40]]));
    let off = compile(s.design());
    assert_eq!(inner_trims(&off), 1);
    run(&mut s, "design.setTravelUnder", json!({"on": true}));
    let on = compile(s.design());
    assert_eq!(inner_trims(&on), 0);
    for st in on
        .stitches
        .iter()
        .filter(|st| st.command == Command::Stitch)
    {
        let on_bar = (st.y - 100.0).abs() < 6.0 && (95.0..=505.0).contains(&st.x);
        let on_stem = (st.x - 300.0).abs() < 6.0 && (95.0..=405.0).contains(&st.y);
        assert!(on_bar || on_stem, "off the lines: {st:?}");
    }
    run(&mut s, "design.setTravelUnder", json!({"on": false}));
    assert_eq!(compile(s.design()), off);
}

#[test]
fn run_connectors_join_objects_of_one_thread() {
    let tmp = TempDir::new("routing");
    let mut s = Session::new(tmp.path());
    let a = rect(&mut s, 10.0, 10.0, 10.0, 10.0, "#ff0000");
    rect(&mut s, 50.0, 10.0, 10.0, 10.0, "#ff0000");
    assert_eq!(inner_trims(&compile(s.design())), 1);
    let r = run(
        &mut s,
        "object.setConnector",
        json!({"ids": [a], "connector": "run"}),
    );
    assert_eq!(r["connector"], "run");
    let p = compile(s.design());
    assert_eq!(inner_trims(&p), 0);
    // A connector of 15 mm or more is listed.
    let long = r["longConnectors"].as_array().expect("list");
    assert_eq!(long.len(), 1);
    assert_eq!(long[0]["after"], a);
    assert!(long[0]["lengthMM"].as_f64().expect("mm") >= 15.0);
    let listed = run(&mut s, "object.list", json!({}));
    assert_eq!(listed[0]["connector"], "run");
    run(
        &mut s,
        "object.setConnector",
        json!({"ids": [a], "connector": "jump"}),
    );
    assert_eq!(inner_trims(&compile(s.design())), 1);
}
