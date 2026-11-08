//! Production aids beyond PE-DESIGN, through the commands: basting box (#509), stops after an
//! object and at jumps (#512), nearest start and the Optimize report (#547).

use openstitch_core::{Command, StitchPattern};
use openstitch_design::machine::{BROTHER_HOOPS, OTHER_HOOPS};
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

/// The needle points before the first trim: the basting run when it is sewn first.
fn first_run(p: &StitchPattern) -> Vec<Vec2> {
    p.stitches
        .iter()
        .take_while(|s| s.command != Command::Trim)
        .filter(|s| s.command == Command::Stitch)
        .map(|s| v2(s.x, s.y))
        .collect()
}

#[test]
fn basting_box_sews_first_in_the_first_thread() {
    let tmp = TempDir::new("production");
    let mut s = Session::new(tmp.path());
    rect(&mut s, 30.0, 30.0, 40.0, 30.0, "#ff0000");
    let plain = compile(s.design());
    let info = run(&mut s, "design.setBasting", json!({"on": true}));
    assert_eq!(info["basting"]["shape"], "box");
    let p = compile(s.design());
    assert_eq!(
        p.count(Command::ColorChange),
        plain.count(Command::ColorChange),
        "no colour change added"
    );
    assert_eq!(p.threads, plain.threads);
    let run_ = first_run(&p);
    // The box at 5 mm round the 30..70 x 30..60 mm rectangle.
    for q in &run_ {
        let on_x = (q.x - 250.0).abs() < 0.5 || (q.x - 750.0).abs() < 0.5;
        let on_y = (q.y - 250.0).abs() < 0.5 || (q.y - 650.0).abs() < 0.5;
        assert!(on_x || on_y, "{q:?} on the box");
    }
    assert!(run_.len() > 20);
    for w in run_.windows(2) {
        let d = w[0].distance(w[1]);
        assert!((d - 60.0).abs() <= 6.0 && d <= 121.0, "stitch {d}");
    }
    // Off again: byte-identical to never having had it.
    run(&mut s, "design.setBasting", json!({"on": false}));
    assert_eq!(compile(s.design()), plain);
    // Its own thread: one more colour block.
    run(
        &mut s,
        "design.setBasting",
        json!({"on": true, "thread": "own", "margin": 3}),
    );
    let own = compile(s.design());
    assert_eq!(own.threads.len(), plain.threads.len() + 1);
    let info = run(&mut s, "design.info", json!({}));
    assert_eq!(info["production"]["basting"]["marginMM"], 3.0);
    assert!(
        info["production"]["basting"]["stitches"]
            .as_u64()
            .expect("present")
            > 20
    );
}

#[test]
fn basting_box_stays_inside_every_hoop() {
    let tmp = TempDir::new("production");
    let mut s = Session::new(tmp.path());
    let id = rect(&mut s, 0.0, 0.0, 50.0, 40.0, "#0000ff");
    run(
        &mut s,
        "design.setBasting",
        json!({"on": true, "margin": 10}),
    );
    let mut ovals = 0;
    for h in BROTHER_HOOPS.iter().chain(OTHER_HOOPS) {
        run(&mut s, "machine.set", json!({"hoop": h.id}));
        run(&mut s, "object.center", json!({"id": id}));
        let d = s.design();
        let (shape, field) = openstitch_design::production::hoop_field(d);
        let p = compile(d);
        let pts = first_run(&p);
        assert!(pts.len() > 3, "{}: basting sewn", h.id);
        for q in &pts {
            assert!(
                shape.contains(
                    field.width(),
                    field.height(),
                    q.x - field.min_x,
                    q.y - field.min_y,
                    0.1
                ),
                "{}: {q:?} outside the field",
                h.id
            );
        }
        if shape != openstitch_design::machine::HoopShape::Rectangle {
            ovals += 1;
        }
    }
    assert!(ovals >= 2, "oval and round hoops checked");
}

#[test]
fn basting_outline_follows_the_objects_at_the_margin() {
    let tmp = TempDir::new("production");
    let mut s = Session::new(tmp.path());
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 50, "cy": 50, "rx": 20, "ry": 12, "fill": "#00ff00"}),
    );
    run(
        &mut s,
        "design.setBasting",
        json!({"on": true, "shape": "outline", "margin": 4, "stitch": 3}),
    );
    let d = s.design();
    let outline = d.objects[0].contours[0].path();
    let p = compile(d);
    let pts = first_run(&p);
    assert!(pts.len() > 20);
    for q in &pts {
        let (dist, _) = openstitch_design::knife::nearest(*q, &outline);
        assert!((dist - 40.0).abs() <= 3.0, "{q:?} at {dist}");
    }
}

#[test]
fn stop_after_an_object_and_at_jumps() {
    let tmp = TempDir::new("production");
    let mut s = Session::new(tmp.path());
    let a = rect(&mut s, 10.0, 10.0, 10.0, 10.0, "#ff0000");
    let b = rect(&mut s, 40.0, 10.0, 10.0, 10.0, "#ff0000");
    rect(&mut s, 70.0, 10.0, 10.0, 10.0, "#ff0000");
    let plain = compile(s.design());
    run(
        &mut s,
        "object.setStopAfter",
        json!({"ids": [b], "on": true}),
    );
    let p = compile(s.design());
    assert_eq!(p.count(Command::Stop), 1);
    // The stop follows the second square's last stitch, before the move to the third.
    let at = p
        .stitches
        .iter()
        .position(|s| s.command == Command::Stop)
        .expect("present");
    let before = p.stitches[..at]
        .iter()
        .rev()
        .find(|s| s.command == Command::Stitch)
        .expect("present");
    assert!((400.0..=500.0).contains(&before.x), "{before:?}");
    assert!(p.stitches[at + 1..].iter().any(|s| s.x >= 700.0));
    assert!(
        p.stitches[at + 1..]
            .iter()
            .all(|s| s.x >= 400.0 || s.command == Command::End)
    );
    let listed = run(&mut s, "object.list", json!({}));
    assert_eq!(listed[1]["stopAfter"], true);

    // Stop at Jumps: one more stop, before the move from the first square; the manual stop
    // stays and off removes exactly the added one.
    let r = run(&mut s, "design.setStopAtJumps", json!({"on": true}));
    assert_eq!(r["stops"], 2);
    let p = compile(s.design());
    for (i, st) in p.stitches.iter().enumerate() {
        if st.command == Command::Stop {
            assert_eq!(p.stitches[i - 1].command, Command::Trim);
        }
    }
    let r = run(&mut s, "design.setStopAtJumps", json!({"on": false}));
    assert_eq!(r["stops"], 1);
    run(
        &mut s,
        "object.setStopAfter",
        json!({"ids": [b], "on": false}),
    );
    assert_eq!(compile(s.design()), plain);

    // A colour change right after is the stop.
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": [a], "color": "#0000ff"}),
    );
    run(
        &mut s,
        "object.setStopAfter",
        json!({"ids": [a], "on": true}),
    );
    assert_eq!(compile(s.design()).count(Command::Stop), 0);

    // PES writes the stop as a colour change to the same thread.
    run(
        &mut s,
        "object.setStopAfter",
        json!({"ids": [a], "on": false}),
    );
    run(
        &mut s,
        "object.setStopAfter",
        json!({"ids": [b], "on": true}),
    );
    run(&mut s, "design.export", json!({"path": "stop.pes"}));
    let bytes = std::fs::read(tmp.path().join("stop.pes")).expect("present");
    let back = openstitch_formats::pes::read(&bytes).expect("present");
    assert_eq!(back.count(Command::ColorChange), 2);
    assert_eq!(back.threads.len(), 3);
    assert_eq!(back.threads[1].hex(), back.threads[2].hex());
    // PES 0110 objects cannot hold a stop.
    assert!(
        s.execute(
            "design.export",
            Some(&json!({"path": "stop11.pes", "pesVersion": 11}))
        )
        .is_err()
    );
}

#[test]
fn optimize_reports_before_and_after() {
    let tmp = TempDir::new("production");
    let mut s = Session::new(tmp.path());
    rect(&mut s, 10.0, 10.0, 10.0, 10.0, "#ff0000");
    rect(&mut s, 70.0, 70.0, 10.0, 10.0, "#0000ff");
    rect(&mut s, 12.0, 30.0, 10.0, 10.0, "#ff0000");
    let r = run(&mut s, "order.optimize", json!({}));
    assert_eq!(r["before"]["colorChanges"], 2);
    assert_eq!(r["after"]["colorChanges"], 1);
    let stats = run(&mut s, "design.info", json!({}))["stitchStats"].clone();
    for k in ["colorChanges", "jumps", "trims", "travelMM"] {
        assert_eq!(r["after"][k], stats[k], "{k}");
    }
    let summary = r["summary"].as_str().expect("present");
    assert!(
        summary.starts_with("colour changes 2 → 1, jumps "),
        "{summary}"
    );
    assert!(summary.ends_with(" mm"));
}

#[test]
fn nearest_start_also_ends_a_fill_toward_the_next_object() {
    // #547: three squares of one thread. A fill without an exit of its own also ends nearest
    // the next object (the guard keeps it only where travel falls and no trim is added); an
    // exit the user set is kept.
    let tmp = TempDir::new("production");
    let mut s = Session::new(tmp.path());
    rect(&mut s, 10.0, 60.0, 20.0, 20.0, "#ff0000");
    let middle = rect(&mut s, 40.0, 60.0, 20.0, 20.0, "#ff0000");
    rect(&mut s, 40.0, 5.0, 20.0, 20.0, "#ff0000");
    let stats = |s: &mut Session| run(s, "design.info", json!({}))["stitchStats"].clone();
    let off = stats(&mut s);
    run(
        &mut s,
        "design.setProperties",
        json!({"autoStart": "nearest"}),
    );
    let on = stats(&mut s);
    assert!(
        on["travelMM"].as_f64() <= off["travelMM"].as_f64(),
        "{off} -> {on}"
    );
    assert!(
        on["trims"].as_u64() <= off["trims"].as_u64(),
        "{off} -> {on}"
    );
    let p = compile(s.design());
    run(
        &mut s,
        "object.setEntryExit",
        json!({"id": middle, "regionExit": [60, 80]}),
    );
    assert_ne!(compile(s.design()), p, "the user's exit wins");
}

#[test]
fn satins_are_entered_part_way_along() {
    // #547 part-way satin start (#545's mid-join): a square ending at (45, 20), then a 60 mm
    // satin along y 25-31 whose ends are 30 mm away. With autoStart nearest each satin kind is
    // entered near (45, 28): a Manual Punch column and a skeleton satin or satin line (a lead
    // run along the centre, under the satin, to where its own stitching starts). Travel falls,
    // no trim is added, every needle point stays on the satin, and off sews as before.
    let kinds: [(&str, Vec<(&str, Value)>); 3] = [
        (
            "column",
            vec![
                (
                    "shape.column",
                    json!({"lines": (0..=6).map(|k| json!([[15 + 10 * k, 25], [15 + 10 * k, 31]]))
                        .collect::<Vec<_>>(), "fill": "#ff0000"}),
                ),
                ("object.setRegion", json!({"ids": [2], "underlay": false})),
            ],
        ),
        (
            "skeleton satin",
            vec![(
                "shape.skeletonSatin",
                json!({"points": [[15, 28], [75, 28]], "widths": [6, 6], "fill": "#ff0000"}),
            )],
        ),
        (
            "satin line",
            vec![
                (
                    "shape.path",
                    json!({"points": [[15, 28], [75, 28]], "closed": false, "outline": "#ff0000"}),
                ),
                (
                    "object.setLine",
                    json!({"id": 2, "type": "satin", "width": 6}),
                ),
            ],
        ),
    ];
    for (name, steps) in kinds {
        let tmp = TempDir::new("production");
        let mut s = Session::new(tmp.path());
        rect(&mut s, 40.0, 10.0, 10.0, 10.0, "#ff0000");
        for (id, params) in steps {
            run(&mut s, id, params);
        }
        let stats = |s: &mut Session| run(s, "design.info", json!({}))["stitchStats"].clone();
        let off = stats(&mut s);
        run(
            &mut s,
            "design.setProperties",
            json!({"autoStart": "nearest"}),
        );
        let on = stats(&mut s);
        let travel = |v: &Value| v["travelMM"].as_f64().expect("travel");
        assert!(travel(&on) + 20.0 < travel(&off), "{name}: {off} -> {on}");
        assert!(
            on["trims"].as_u64() <= off["trims"].as_u64(),
            "{name}: {off} -> {on}"
        );
        let p = compile(s.design());
        let entered = p
            .stitches
            .iter()
            .find(|st| st.command == Command::Stitch && st.y > 225.0)
            .expect("the satin");
        assert!(
            (entered.x - 450.0).abs() < 25.0,
            "{name}: enters at {entered:?}"
        );
        for st in p
            .stitches
            .iter()
            .filter(|st| st.command == Command::Stitch && st.y > 225.0)
        {
            assert!(
                (145.0..=755.0).contains(&st.x) && st.y <= 315.0,
                "{name}: {st:?}"
            );
        }
        run(&mut s, "design.setProperties", json!({"autoStart": "off"}));
        assert_eq!(stats(&mut s), off, "{name}");
    }
}

#[test]
fn nearest_start_enters_a_closed_outline_at_its_nearest_point() {
    // #547's closed-outline rule: a closed running outline without an entry point of its own
    // starts at the point of its outline nearest to where the needle stopped, not at its first
    // vertex. The outline is drawn from its far corner (80, 30) mm.
    let tmp = TempDir::new("production");
    let mut s = Session::new(tmp.path());
    rect(&mut s, 10.0, 10.0, 20.0, 20.0, "#ff0000");
    run(
        &mut s,
        "shape.path",
        json!({"points": [[80, 30], [80, 10], [60, 10], [60, 30]], "closed": true,
               "outline": "#ff0000"}),
    );
    let travel = |s: &mut Session| {
        run(s, "design.info", json!({}))["stitchStats"]["travelMM"]
            .as_f64()
            .expect("present")
    };
    // The outline's first needle point: the first stitch right of the square.
    let outline_start = |s: &Session| {
        let p = compile(s.design());
        let st = p
            .stitches
            .iter()
            .find(|st| st.x >= 550.0 && st.command == Command::Stitch)
            .copied()
            .expect("present");
        v2(st.x, st.y)
    };
    let off = travel(&mut s);
    let far = outline_start(&s);
    assert!(far.x > 750.0, "drawn start {far:?}");
    run(
        &mut s,
        "design.setProperties",
        json!({"autoStart": "nearest"}),
    );
    let on = travel(&mut s);
    let near = outline_start(&s);
    assert!(on < off, "travel {off} -> {on}");
    // On the outline's left side (x 60 mm), the side facing the square.
    assert!(near.x < 650.0, "nearest start {near:?}");
    run(&mut s, "design.setProperties", json!({"autoStart": "off"}));
    assert_eq!(travel(&mut s), off);
}

#[test]
fn nearest_start_shortens_travel() {
    let tmp = TempDir::new("production");
    let mut s = Session::new(tmp.path());
    rect(&mut s, 10.0, 10.0, 20.0, 20.0, "#ff0000");
    // An open line drawn away from the square: it starts at its far end unless told.
    run(
        &mut s,
        "shape.path",
        json!({"points": [[90, 20], [40, 20]], "closed": false, "outline": "#ff0000"}),
    );
    rect(&mut s, 60.0, 60.0, 20.0, 20.0, "#ff0000");
    let off = run(&mut s, "design.info", json!({}))["stitchStats"]["travelMM"]
        .as_f64()
        .expect("present");
    let r = run(
        &mut s,
        "design.setProperties",
        json!({"autoStart": "nearest"}),
    );
    assert_eq!(r["autoStart"], "nearest");
    let info = run(&mut s, "design.info", json!({}));
    let on = info["stitchStats"]["travelMM"].as_f64().expect("present");
    assert_eq!(info["production"]["autoStart"], "nearest");
    assert!(on < off * 0.8, "travel {off} -> {on}");
    // The line now starts at its end nearer the square.
    let p = compile(s.design());
    let line_start = p
        .stitches
        .iter()
        .find(|st| st.x >= 350.0 && st.command == Command::Stitch)
        .expect("present");
    assert!(line_start.x < 450.0, "{line_start:?}");
    run(&mut s, "design.setProperties", json!({"autoStart": "off"}));
    let back = run(&mut s, "design.info", json!({}))["stitchStats"]["travelMM"]
        .as_f64()
        .expect("present");
    assert_eq!(back, off);
}
