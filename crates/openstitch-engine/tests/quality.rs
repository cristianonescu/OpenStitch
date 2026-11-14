//! Quality checks (#503 Check Design, #504 density map, #505 Review Sewing Order, #457 export
//! read-back): known-bad designs are flagged, the kit's designs pass.

use openstitch_core::{Command, Stitch, StitchPattern, Thread};
use openstitch_engine::Session;
use openstitch_engine::quality::{Traced, verify, verify_file};
use openstitch_formats::FileType;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::path::Path;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn rules(r: &Value) -> Vec<(String, Vec<u64>)> {
    r["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .map(|f| {
            let ids = f["ids"]
                .as_array()
                .expect("value")
                .iter()
                .map(|i| i.as_u64().expect("value"))
                .collect();
            (f["rule"].as_str().expect("value").to_string(), ids)
        })
        .collect()
}

/// A stitch file made of `stitches` (0.1 mm), imported as one stitch object.
fn import(s: &mut Session, dir: &Path, name: &str, stitches: Vec<Stitch>) {
    let pattern = StitchPattern {
        stitches,
        threads: vec![Thread::rgb(0xff0000)],
        ..StitchPattern::default()
    };
    let path = dir.join(name);
    std::fs::write(&path, FileType::Pes.write(&pattern).expect("value")).expect("value");
    run(
        s,
        "stitches.import",
        json!({"path": path.display().to_string()}),
    );
}

fn tatami(s: &mut Session, x: f64) -> u64 {
    let id = run(
        s,
        "shape.rectangle",
        json!({"x": x, "y": 10, "width": 20, "height": 20, "fill": "Red"}),
    )["id"]
        .as_u64()
        .expect("value");
    run(
        s,
        "object.setRegion",
        json!({"ids": [id], "underlay": false}),
    );
    id
}

fn inner_layers(d: &Value) -> Vec<f64> {
    let (w, h) = (
        d["width"].as_u64().expect("value") as usize,
        d["height"].as_u64().expect("value") as usize,
    );
    let v = d["values"].as_array().expect("value");
    (3..h - 3)
        .flat_map(|y| (3..w - 3).map(move |x| y * w + x))
        .map(|k| v[k].as_f64().expect("value"))
        .collect()
}

#[test]
fn density_reads_one_layer_per_fill_and_stacks() {
    let mut s = Session::new(std::env::temp_dir());
    tatami(&mut s, 10.0);
    let one = run(&mut s, "design.density", json!({}));
    let l = inner_layers(&one);
    assert!(l.iter().all(|v| (0.9..=1.1).contains(v)), "{l:?}");
    assert_eq!(one["hotspots"], json!([]));
    tatami(&mut s, 10.0);
    let two = run(&mut s, "design.density", json!({}));
    let l = inner_layers(&two);
    assert!(l.iter().all(|v| (1.8..=2.2).contains(v)), "{l:?}");
    assert_eq!(two["hotspots"], json!([]));
    // A lighter fabric judges the same stitches heavier, without recompiling them.
    let thin = run(&mut s, "design.density", json!({"fabric": "thin"}));
    assert!(
        thin["classes"]["heavy"].as_f64() > two["classes"]["heavy"].as_f64(),
        "{thin}"
    );
    tatami(&mut s, 10.0);
    let three = run(&mut s, "design.density", json!({}));
    let hot = three["hotspots"].as_array().expect("value");
    assert_eq!(hot.len(), 1, "{three}");
    assert_eq!(hot[0]["ids"], json!([1, 2, 3]));
    assert!(hot[0]["maxLayers"].as_f64().expect("value") > 2.7);
    assert!(
        three["classes"]["tooDense"].as_f64().expect("value") > 0.5,
        "{three}"
    );
    // The check names the overlap; one object's own dense spots are left out.
    let c = run(&mut s, "design.check", json!({"rules": ["stacked"]}));
    assert_eq!(rules(&c), [("stacked".to_string(), vec![1, 2, 3])], "{c}");
    assert!(
        s.execute("design.density", Some(&json!({"fabric": "silk"})))
            .is_err()
    );
}

/// `sew_check`'s limits are errors here too, so every kit file it fails fails this check.
#[test]
fn sew_check_failures_are_errors() {
    let dir = TempDir::new("quality-sewcheck");
    // 25 drops in one 0.5 mm spot.
    let mut s = Session::new(&dir);
    let mut st = vec![Stitch::new(0.0, 0.0, Command::Stitch)];
    for k in 0..25 {
        st.push(Stitch::new(400.0, 400.0, Command::Stitch));
        st.push(Stitch::new(450.0 + f64::from(k), 450.0, Command::Stitch));
    }
    import(&mut s, &dir, "stacked.pes", st);
    let c = run(&mut s, "design.check", json!({}));
    assert_eq!(c["ready"], json!(false), "{c}");
    assert!(rules(&c).iter().any(|r| r.0 == "stacked"), "{c}");
    let d = run(&mut s, "design.density", json!({}));
    assert_eq!(d["hotspots"][0]["stackedDrops"], json!(true), "{d}");
    // Eight stitches of 0.1 mm in a row: stitching in place.
    let mut s = Session::new(&dir);
    let mut st: Vec<_> = (0..20)
        .map(|k| Stitch::new(f64::from(k) * 20.0, 0.0, Command::Stitch))
        .collect();
    st.extend((0..8).map(|k| Stitch::new(400.0 + f64::from(k), 0.0, Command::Stitch)));
    import(&mut s, &dir, "inplace.pes", st);
    assert!(
        rules(&run(&mut s, "design.check", json!({})))
            .iter()
            .any(|r| r.0 == "inPlace")
    );
    // Larger than the hoop either way round.
    let mut s = Session::new(&dir);
    run(&mut s, "machine.set", json!({"hoop": "100x100"}));
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": -10, "y": 10, "width": 120, "height": 20, "fill": "Red"}),
    );
    let c = run(&mut s, "design.check", json!({}));
    assert_eq!(c["findings"][0]["rule"], json!("outsideHoop"));
    assert_eq!(c["findings"][0]["severity"], json!("error"), "{c}");
}

/// The fixture of #503: each problem is found once, on its own object, and its fix clears it
/// as one undo step.
#[test]
fn check_design_finds_the_six_problems_and_fixes_them() {
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "machine.set",
        json!({"machine": "multi", "hoop": "130x180"}),
    );
    let rect = |s: &mut Session, x: f64, y: f64, w: f64, h: f64, satin: bool| {
        let id = run(
            s,
            "shape.rectangle",
            json!({"x": x, "y": y, "width": w, "height": h, "fill": "Blue"}),
        )["id"]
            .as_u64()
            .expect("value");
        if satin {
            run(s, "object.setRegion", json!({"ids": [id], "type": "satin"}));
        }
        id
    };
    let narrow = rect(&mut s, 10.0, 10.0, 0.8, 20.0, true);
    let thin = rect(&mut s, 20.0, 10.0, 1.0, 2.0, false);
    // 15 mm from the thin fill, same thread, trimming off on this object.
    let jump = rect(&mut s, 36.0, 10.0, 8.0, 8.0, false);
    run(
        &mut s,
        "object.setJumpTrimming",
        json!({"ids": [jump], "enabled": false}),
    );
    let outside = rect(&mut s, 125.0, 60.0, 10.0, 10.0, false);
    let text = run(
        &mut s,
        "text.add",
        json!({"text": "Hi", "size": 3, "x": 40, "y": 120}),
    )["id"]
        .as_u64()
        .expect("value");
    let wide = rect(&mut s, 60.0, 30.0, 13.0, 40.0, true);
    let before = serde_json::to_string(s.design()).expect("value");
    let c = run(&mut s, "design.check", json!({}));
    let mut found = rules(&c);
    found.sort();
    let mut want = vec![
        ("fillTooThin".to_string(), vec![thin]),
        ("outsideHoop".to_string(), vec![outside]),
        ("satinTooNarrow".to_string(), vec![narrow]),
        ("satinTooWide".to_string(), vec![wide]),
        ("textTooSmall".to_string(), vec![text]),
        ("untrimmedJump".to_string(), vec![thin, jump]),
    ];
    want.sort();
    assert_eq!(found, want, "{c}");
    assert_eq!(c["ready"], json!(false), "the 13 mm satin is an error");
    assert_eq!(
        c["findings"][0]["severity"],
        json!("error"),
        "errors come first"
    );
    for f in c["findings"].as_array().expect("value") {
        let fix = &f["fix"];
        let rule = f["rule"].as_str().expect("value");
        run(
            &mut s,
            fix["command"].as_str().expect("value"),
            fix["params"].clone(),
        );
        let again = run(&mut s, "design.check", json!({"rules": [rule]}));
        assert!(
            !rules(&again)
                .iter()
                .any(|r| r.1 == rules(&json!({"findings": [f]}))[0].1),
            "{rule} still found after its fix: {again}"
        );
    }
    for _ in 0..6 {
        run(&mut s, "history.undo", json!({}));
    }
    assert_eq!(serde_json::to_string(s.design()).expect("value"), before);
    // Filters.
    let only = run(&mut s, "design.check", json!({"ids": [wide]}));
    assert_eq!(rules(&only), [("satinTooWide".to_string(), vec![wide])]);
}

#[test]
fn just_inside_the_limits_is_clean() {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "machine.set", json!({"hoop": "100x100"}));
    // Satin stitches run at an angle, so a 6 mm column sews stitches under 10 mm.
    for (x, w) in [(10.0, 1.2), (20.0, 6.0)] {
        let id = run(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 10, "width": w, "height": 30, "fill": "Red"}),
        )["id"]
            .clone();
        run(
            &mut s,
            "object.setRegion",
            json!({"ids": [id], "type": "satin"}),
        );
    }
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 40, "y": 10, "width": 2, "height": 30, "fill": "Red"}),
    );
    run(
        &mut s,
        "text.add",
        json!({"text": "Hi", "size": 4, "x": 50, "y": 60}),
    );
    let c = run(&mut s, "design.check", json!({}));
    assert_eq!(c["findings"], json!([]), "{c}");
    assert_eq!(c["summary"], json!("Ready to sew"));
}

#[test]
fn cleanup_removes_only_tiny_or_empty_objects() {
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 10, "height": 10}),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 30, "y": 10, "width": 1, "height": 1}),
    );
    // An outline that sews nothing on purpose stays.
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 40, "y": 10, "width": 1, "height": 1}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": [3], "type": "none"}),
    );
    let c = run(&mut s, "design.check", json!({}));
    assert_eq!(rules(&c), [("tinyObject".to_string(), vec![2])], "{c}");
    let r = run(&mut s, "design.cleanup", json!({}));
    assert_eq!(r["removed"], json!([2]));
    let ids: Vec<u64> = s.design().objects.iter().map(|o| o.id).collect();
    assert_eq!(ids, [1, 3]);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design().objects.len(), 3);
}

#[test]
fn route_labels_moves_and_warns() {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "machine.set", json!({"machine": "multi"}));
    for x in [10, 40] {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 10, "width": 10, "height": 10, "fill": "Red"}),
        );
    }
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 40, "width": 10, "height": 10, "fill": "Blue"}),
    );
    let r = run(&mut s, "design.route", json!({}));
    let steps = r["steps"].as_array().expect("value");
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0]["move"]["kind"], json!("trim"), "{r}");
    assert_eq!(
        steps[0]["warnings"][0]["rule"],
        json!("longSameThreadMove"),
        "{r}"
    );
    let label = steps[0]["move"]["label"].as_str().expect("value");
    assert!(
        label.starts_with("Trim, then ") && label.ends_with(" mm"),
        "{label}"
    );
    assert_eq!(steps[1]["move"]["kind"], json!("colorChange"));
    assert_eq!(steps[1]["move"]["to"], json!(3));
    assert_eq!(steps[2]["move"]["kind"], json!("none"));
    assert_eq!(steps[0]["color"], steps[1]["color"]);
    let info = run(&mut s, "design.info", json!({}));
    for (a, b) in [
        ("trims", "trims"),
        ("colorChanges", "colorChanges"),
        ("jumps", "jumps"),
    ] {
        assert_eq!(r["totals"][a], info["stitchStats"][b], "{a}");
    }
    // Untrimmed: one untrimmedJump, no longSameThreadMove.
    run(
        &mut s,
        "object.setJumpTrimming",
        json!({"ids": [2], "enabled": false}),
    );
    let r = run(&mut s, "design.route", json!({}));
    assert_eq!(r["steps"][0]["move"]["kind"], json!("jump"), "{r}");
    assert_eq!(
        r["steps"][0]["warnings"].as_array().expect("value").len(),
        1
    );
    assert_eq!(r["steps"][0]["warnings"][0]["rule"], json!("untrimmedJump"));
    assert_eq!(r["totals"]["warnings"], json!(1));
}

/// Every kit design: no errors, no route warnings. Their warnings are listed in the PR (#295).
#[test]
fn kit_designs_pass() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/sew");
    let manifest: Value =
        serde_json::from_str(&std::fs::read_to_string(root.join("manifest.json")).expect("value"))
            .expect("value");
    let mut files = Vec::new();
    let mut todo = vec![root.clone()];
    while let Some(d) = todo.pop() {
        for e in std::fs::read_dir(d).expect("value").flatten() {
            let p = e.path();
            if p.is_dir() {
                todo.push(p);
            } else if p.extension().is_some_and(|x| x == "pes") {
                files.push(p);
            }
        }
    }
    assert!(files.len() >= 30, "{files:?}");
    for f in files {
        let rel = f
            .strip_prefix(&root)
            .expect("value")
            .to_string_lossy()
            .replace('\\', "/");
        let hoop = manifest[&rel]["hoop"]
            .as_str()
            .unwrap_or("100 × 100 mm")
            .replace('×', "x")
            .replace([' ', 'm'], "");
        let mut s = Session::new(std::env::temp_dir());
        let (w, h) = hoop.split_once('x').expect("value");
        match hoop.as_str() {
            "100x100" => run(&mut s, "machine.set", json!({"hoop": "100x100"})),
            // The 260 × 160 mm hoop is used upright on the NV880E.
            "260x160" => run(&mut s, "machine.set", json!({"hoop": "160x260"})),
            _ => run(
                &mut s,
                "design.new",
                json!({"width": w.parse::<f64>().expect("value"), "height": h.parse::<f64>().expect("value")}),
            ),
        };
        run(
            &mut s,
            "stitches.import",
            json!({"path": f.display().to_string()}),
        );
        let c = run(&mut s, "design.check", json!({}));
        assert_eq!(c["errors"], json!(0), "{rel}: {c}");
        // The section rules (#866) are warnings; listed for the PR.
        for f in c["findings"].as_array().expect("findings") {
            if matches!(f["rule"].as_str(), Some("untied" | "tinySection")) {
                eprintln!("{rel}: {} {}", f["rule"], f["message"]);
            }
        }
        let r = run(&mut s, "design.route", json!({}));
        assert_eq!(r["totals"]["warnings"], json!(0), "{rel}: {r}");
    }
}

#[test]
fn exports_read_back_in_every_writable_format() {
    let dir = TempDir::new("quality-readback");
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 20, "fill": "Red"}),
    );
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 70, "cy": 40, "rx": 12, "ry": 8, "fill": "Blue"}),
    );
    let id = run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 60, "width": 15, "height": 30, "fill": "Black"}),
    )["id"]
        .clone();
    run(
        &mut s,
        "object.setRegion",
        json!({"ids": [id], "type": "satin"}),
    );
    for ty in FileType::writable() {
        let path = format!("out.{}", ty.extension());
        let r = run(
            &mut s,
            "design.export",
            json!({"path": path, "split": true}),
        );
        assert_eq!(r["verified"], json!(true), "{ty}: {r}");
        for rb in r["readBack"].as_array().expect("value") {
            assert!(
                rb["maxDeviationMm"].as_f64().expect("value") <= 0.1,
                "{ty}: {rb}"
            );
        }
    }
    // A hoop turned 90°: the PES holds the stitches turned, and still reads back as written.
    run(
        &mut s,
        "machine.set",
        json!({"hoop": "130x180", "rotate90": true}),
    );
    let r = run(&mut s, "design.export", json!({"path": "turned.pes"}));
    assert_eq!(r["verified"], json!(true), "{r}");
    let r = run(
        &mut s,
        "design.export",
        json!({"path": "off.pes", "verify": "off"}),
    );
    assert_eq!(r["verified"], Value::Null);
    assert!(
        s.execute(
            "design.export",
            Some(&json!({"path": "x.pes", "verify": "maybe"}))
        )
        .is_err()
    );

    // A writer that drops one needle drop, a truncated file and a short write are all caught.
    let t = Traced::new(s.design());
    let mut dropped = t.pattern.clone();
    let k = dropped
        .stitches
        .iter()
        .rposition(|st| st.command == Command::Stitch)
        .expect("value")
        - 40;
    dropped.stitches.remove(k);
    for ty in [FileType::Pes, FileType::Dst, FileType::Jef] {
        let bytes = ty.write(&dropped).expect("value");
        let v = verify(ty, &t.pattern, &bytes);
        assert_eq!(v["verified"], json!(false), "{ty}: {v}");
        let good = ty.write(&t.pattern).expect("value");
        assert_eq!(verify(ty, &t.pattern, &good)["verified"], json!(true));
        let v = verify(ty, &t.pattern, &good[..good.len() / 2]);
        assert_eq!(v["verified"], json!(false), "{ty} truncated: {v}");
        let path = dir.join(format!("short.{}", ty.extension()));
        std::fs::write(&path, &good[..good.len() - 1]).expect("value");
        assert_eq!(
            verify_file(ty, &t.pattern, &path, &good)["verified"],
            json!(false)
        );
    }
}

#[test]
fn cheap_rules_are_fast_on_a_large_design() {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "design.new", json!({"width": 200, "height": 200}));
    for k in 0..5 {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": 10, "y": 10 + k * 35, "width": 180, "height": 30, "fill": "Red"}),
        );
    }
    // #633: a wall-clock budget alone failed on a busy machine (0.08 s against 50 ms while
    // other builds ran). The check is measured against compiling the same design in the same
    // run, which a loaded machine slows alike: the cheap rules must cost less than one
    // compile, the best of three runs each. On an idle machine the fixed budget still holds
    // on its own.
    let mut traced = std::time::Duration::MAX;
    let mut t = None;
    for _ in 0..3 {
        let start = std::time::Instant::now();
        t = Some(Traced::new(s.design()));
        traced = traced.min(start.elapsed());
    }
    let t = t.expect("traced");
    assert!(
        t.pattern.count(Command::Stitch) > 30_000,
        "{}",
        t.pattern.count(Command::Stitch)
    );
    let mut took = std::time::Duration::MAX;
    let mut found = Vec::new();
    for _ in 0..3 {
        let start = std::time::Instant::now();
        found = openstitch_engine::quality::check(
            s.design(),
            &t,
            &openstitch_engine::quality::CheckOptions { fabric: (2.0, 3.0) },
        );
        took = took.min(start.elapsed());
    }
    // 50 ms in a release build; a debug build is about ten times slower.
    let budget = std::time::Duration::from_millis(if cfg!(debug_assertions) { 1000 } else { 50 });
    assert!(
        took < budget.max(traced),
        "the cheap rules took {took:?}; compiling the design took {traced:?}"
    );
    assert!(found.is_empty(), "{found:?}");
}

/// A stitch file of sections `runs` (0.1 mm) with a trim between each two, imported as one
/// stitch object.
fn sections(s: &mut Session, dir: &Path, name: &str, runs: &[Vec<(f64, f64)>]) {
    let mut st = Vec::new();
    for (k, run) in runs.iter().enumerate() {
        if k > 0 {
            st.push(Stitch::new(run[0].0, run[0].1, Command::Trim));
            st.push(Stitch::new(run[0].0, run[0].1, Command::Jump));
        }
        st.extend(run.iter().map(|&(x, y)| Stitch::new(x, y, Command::Stitch)));
    }
    import(s, dir, name, st);
}

/// `n` stitches of 2 mm along y = `y` from x = `x`, with PE-DESIGN's 0.3 mm tie-in and lock
/// when `tied`.
fn row(x: f64, y: f64, n: usize, tied: bool) -> Vec<(f64, f64)> {
    let mut r: Vec<(f64, f64)> = (0..n).map(|k| (x + 20.0 * k as f64, y)).collect();
    if tied {
        let (a, b) = (r[0], r[r.len() - 1]);
        r.splice(1..1, [(a.0 + 3.0, y), a]);
        r.extend([(b.0 - 3.0, y), b]);
    }
    r
}

fn found<'a>(c: &'a Value, rule: &str) -> Option<&'a Value> {
    c["findings"]
        .as_array()
        .expect("findings")
        .iter()
        .find(|f| f["rule"] == rule)
}

/// #866: a thread end without a tie is `untied` (a warning whose fix ties the objects with
/// `object.setFinish tie always`, one undo step); PE-DESIGN's tie, and a stitch in place at
/// the end point, count as ties.
#[test]
fn untied_thread_ends_are_found_and_the_fix_ties_them() {
    // A default rectangle: PE-DESIGN's tie-in and lock at both ends.
    let mut s = Session::new(std::env::temp_dir());
    let id = tatami(&mut s, 10.0);
    let c = run(&mut s, "design.check", json!({}));
    assert!(found(&c, "untied").is_none(), "{c}");
    // Ties off: both ends of its one section are untied.
    run(
        &mut s,
        "object.setFinish",
        json!({"ids": [id], "tie": "off"}),
    );
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    let f = found(&c, "untied").unwrap_or_else(|| panic!("{c}"));
    assert_eq!(
        (f["severity"].as_str(), f["value"].as_f64()),
        (Some("warning"), Some(2.0)),
        "{f}"
    );
    assert_eq!(f["ids"], json!([id]));
    assert_eq!(f["fix"]["command"], json!("object.setFinish"), "{f}");
    // The fix, as the editor runs it: one undo step that clears the finding.
    let fix = f["fix"].clone();
    run(
        &mut s,
        fix["command"].as_str().expect("command"),
        fix["params"].clone(),
    );
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    assert!(found(&c, "untied").is_none(), "{c}");
    run(&mut s, "history.undo", json!({}));
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    assert!(
        found(&c, "untied").is_some(),
        "the undo brings the untied ends back: {c}"
    );

    // Stitch files: tied sections pass; in a file with one tied and one untied section, only
    // the untied section's two ends are found.
    let dir = TempDir::new("check-untied");
    let mut s = Session::new(&dir);
    sections(
        &mut s,
        &dir,
        "tied.pes",
        &[row(0.0, 0.0, 10, true), row(0.0, 100.0, 10, true)],
    );
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    assert!(found(&c, "untied").is_none(), "{c}");
    let mut s = Session::new(&dir);
    sections(
        &mut s,
        &dir,
        "half.pes",
        &[row(0.0, 0.0, 10, true), row(0.0, 100.0, 10, false)],
    );
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    let f = found(&c, "untied").unwrap_or_else(|| panic!("{c}"));
    assert_eq!(f["value"], json!(2.0), "{f}");
}

/// #866 with #1013: PE-DESIGN's stitch in place at a one-step tie-in on a Hoop Size page is
/// intended (#1055), so `inPlace`, `stacked`, `shortStitch` and `untied` must not report it.
/// PE-DESIGN's default 100 × 100 mm Hoop Size page with an 8 mm Lato Bold K, as #1055's own test
/// (`a_one_step_tie_in_starts_on_a_stitch_in_place_on_a_hoop_size_page`): its strokes start
/// with one-step tie-ins, each on a stitch in place.
#[test]
fn a_stitch_in_place_tie_in_is_not_reported() {
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "design.new",
        json!({"width": 100, "height": 100, "pageMode": "hoop"}),
    );
    run(
        &mut s,
        "text.add",
        json!({"text": "K", "font": "lato-bold", "size": 8, "x": 20, "y": 70, "fill": "Red"}),
    );
    assert!(s.design().hoop_size_page());
    // The stitch in place is there: a needle drop within 0.05 mm of the point the frame just
    // moved to.
    let st = openstitch_design::Compiler::for_design(s.design())
        .compile(s.design())
        .stitches;
    let in_place = st.windows(2).any(|w| {
        matches!(w[0].command, Command::Jump | Command::Trim)
            && w[1].command == Command::Stitch
            && (w[0].x - w[1].x).hypot(w[0].y - w[1].y) < 0.5
    });
    assert!(
        in_place,
        "a Hoop Size page sews the stitch in place (#1055)"
    );
    let c = run(
        &mut s,
        "design.check",
        json!({"rules": ["inPlace", "stacked", "shortStitch", "untied"]}),
    );
    assert_eq!(c["findings"], json!([]), "{c}");
}

/// #866: fewer than 3 needle drops between two thread cuts is `tinySection`; 3 is not.
#[test]
fn sections_of_fewer_than_three_stitches_are_found() {
    let dir = TempDir::new("check-tiny-section");
    let mut s = Session::new(&dir);
    sections(
        &mut s,
        &dir,
        "tiny.pes",
        &[
            row(0.0, 0.0, 10, true),
            row(0.0, 100.0, 2, false),
            row(0.0, 200.0, 10, true),
        ],
    );
    let c = run(&mut s, "design.check", json!({"rules": ["tinySection"]}));
    let f = found(&c, "tinySection").unwrap_or_else(|| panic!("{c}"));
    assert_eq!(
        (f["value"].as_f64(), f["limit"].as_f64()),
        (Some(1.0), Some(3.0)),
        "{f}"
    );
    assert_eq!(f["severity"], json!("warning"));
    let mut s = Session::new(&dir);
    sections(
        &mut s,
        &dir,
        "three.pes",
        &[
            row(0.0, 0.0, 10, true),
            row(0.0, 100.0, 3, false),
            row(0.0, 200.0, 10, true),
        ],
    );
    let c = run(&mut s, "design.check", json!({"rules": ["tinySection"]}));
    assert!(
        found(&c, "tinySection").is_none(),
        "three stitches hold: {c}"
    );
}

/// #866: more than 160 stitches in 1,000 shorter than 0.3 mm is `shortStitch` (a warning
/// naming the object); 152 in 1,000 is not. The limit is the shared table's, as sew_check reads.
#[test]
fn short_stitches_over_the_shared_limit_are_found() {
    let lim = &*openstitch_engine::quality::LIMITS;
    assert_eq!((lim.short_stitch_mm, lim.max_short_per_1000), (0.3, 160.0));
    // 101 needle points (the importer reads the first as the jump to the start, so 99 stitch
    // moves): `short` of them 0.2 mm, the rest 2 mm, never two short ones in a row (so no
    // stitching in place).
    let row = |short: usize| -> Vec<(f64, f64)> {
        let mut x = 0.0;
        let mut r = vec![(x, 0.0)];
        for k in 0..100 {
            x += if k % 6 == 3 && k / 6 < short {
                2.0
            } else {
                20.0
            };
            r.push((x, 0.0));
        }
        r
    };
    let dir = TempDir::new("check-short");
    let mut s = Session::new(&dir);
    sections(&mut s, &dir, "under.pes", &[row(15)]);
    let c = run(&mut s, "design.check", json!({"rules": ["shortStitch"]}));
    assert!(
        found(&c, "shortStitch").is_none(),
        "15 in 99 (151.5 in 1,000) is under the limit: {c}"
    );
    let mut s = Session::new(&dir);
    sections(&mut s, &dir, "over.pes", &[row(17)]);
    let c = run(&mut s, "design.check", json!({"rules": ["shortStitch"]}));
    let f = found(&c, "shortStitch").unwrap_or_else(|| panic!("{c}"));
    assert_eq!(
        (f["value"].as_f64(), f["limit"].as_f64()),
        // 17 in 99: 171.72 in 1,000.
        (Some(171.72), Some(160.0)),
        "{f}"
    );
    assert_eq!(
        (f["severity"].as_str(), f["ids"].clone()),
        (Some("warning"), json!([1]))
    );
}

/// #866: `machine.setProfile {needles}` (1–15, whole) and the `moreColoursThanNeedles` note,
/// which appears only with a needle count and counts as neither error nor warning.
#[test]
fn more_colours_than_needles_is_a_note_with_a_needle_count() {
    let mut s = Session::new(std::env::temp_dir());
    for (x, colour) in [(10, "Red"), (40, "Blue"), (70, "Black")] {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 10, "width": 20, "height": 20, "fill": colour}),
        );
    }
    let rule = json!({"rules": ["moreColoursThanNeedles"]});
    assert!(
        found(
            &run(&mut s, "design.check", rule.clone()),
            "moreColoursThanNeedles"
        )
        .is_none()
    );
    let p = run(
        &mut s,
        "machine.setProfile",
        json!({"brand": "Brother", "needles": 2}),
    );
    assert_eq!(p["profile"]["needles"], json!(2), "{p}");
    assert_eq!(
        run(&mut s, "machine.profile", json!({}))["profile"]["needles"],
        json!(2)
    );
    let c = run(&mut s, "design.check", json!({}));
    let f = found(&c, "moreColoursThanNeedles").unwrap_or_else(|| panic!("{c}"));
    assert_eq!(
        (
            f["severity"].as_str(),
            f["value"].as_f64(),
            f["limit"].as_f64()
        ),
        (Some("info"), Some(3.0), Some(2.0)),
        "{f}"
    );
    assert_eq!(c["notes"], json!(1), "{c}");
    assert_eq!(c["ready"], json!(true), "a note does not stop the export");
    run(
        &mut s,
        "machine.setProfile",
        json!({"brand": "Brother", "needles": 3}),
    );
    assert!(
        found(
            &run(&mut s, "design.check", rule.clone()),
            "moreColoursThanNeedles"
        )
        .is_none()
    );
    // A single-needle profile (no count) never notes it.
    run(&mut s, "machine.setProfile", json!({"brand": "Brother"}));
    assert_eq!(
        run(&mut s, "machine.profile", json!({}))["profile"]["needles"],
        json!(null)
    );
    assert!(found(&run(&mut s, "design.check", rule), "moreColoursThanNeedles").is_none());
    for bad in [json!(0), json!(16), json!(2.5)] {
        assert!(
            s.execute(
                "machine.setProfile",
                Some(&json!({"brand": "Brother", "needles": bad}))
            )
            .is_err(),
            "needles {bad}"
        );
    }
}
