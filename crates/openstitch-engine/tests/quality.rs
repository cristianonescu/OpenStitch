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

/// #1135: stitching in place is counted as the machine file holds it, between needle points
/// rounded to the 0.1 mm grid. Ten 0.296 mm steps are each under 0.3 mm in the design but 0.3 mm
/// in the file, so they are no run in place; ten 0.2 mm steps still are. The kit's
/// fonts-and-text "OpenStitch" text read 9 in a row before (its file 6) and reads no error now.
#[test]
fn stitching_in_place_is_counted_on_the_file_grid() {
    let dir = TempDir::new("check-in-place-grid");
    let in_place = |step: f64| {
        let mut s = Session::new(&dir);
        // A plain run, then the steps: imported, then moved off the grid as a design holds them.
        let mut st: Vec<Stitch> = (0..10)
            .map(|k| Stitch::new(f64::from(k) * 20.0, 0.0, Command::Stitch))
            .collect();
        st.extend((1..=10).map(|k| Stitch::new(180.0 + f64::from(k) * 3.0, 0.0, Command::Stitch)));
        st.extend((1..=5).map(|k| Stitch::new(210.0 + f64::from(k) * 20.0, 0.0, Command::Stitch)));
        import(&mut s, &dir, "steps.pes", st);
        let mut d = s.design().clone();
        let block = d
            .objects
            .iter_mut()
            .find_map(|o| o.stitch_block.as_mut())
            .expect("block");
        // The import moves the design onto the page: the steps start where a 3-unit step does.
        let first = block
            .stitches
            .windows(2)
            .position(|w| (w[1].x - w[0].x - 3.0).abs() < 1e-9)
            .expect("the steps");
        let x0 = block.stitches[first].x;
        for k in 1..=10 {
            block.stitches[first + k].x = x0 + k as f64 * step;
        }
        for k in 11..=15 {
            block.stitches[first + k].x = x0 + 10.0 * step + (k - 10) as f64 * 20.0;
        }
        assert!(s.adopt_design(d));
        let c = run(&mut s, "design.check", json!({"rules": ["inPlace"]}));
        found(&c, "inPlace").cloned()
    };
    assert!(
        in_place(2.96).is_none(),
        "0.296 mm steps are 0.3 mm in the file"
    );
    let f = in_place(2.0).expect("0.2 mm steps stay in place");
    assert_eq!(f["value"], json!(10.0), "{f}");
    // The kit's fonts-and-text title (tools/sew_kit.py): no run in place over 6.
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    run(
        &mut s,
        "text.add",
        json!({"text": "OpenStitch", "size": 10, "x": 50, "y": 18, "align": "center"}),
    );
    let c = run(&mut s, "design.check", json!({"rules": ["inPlace"]}));
    assert!(found(&c, "inPlace").is_none(), "{c}");
}

/// #1157: a fill letter that is not cut into strokes (Lato Bold's O under Text Transform
/// preset 22 or 24 at a high level) is sewn as a region fill, whose travel between sections
/// walked the warped outline with a needle point on every sample, 0.2-0.3 mm apart: 16 and 13
/// stitches in place in a row in the exported file, where PE-DESIGN 11.50's references have 2
/// and 3. The walk is thinned as the fill's rows are: no run over 6 in the file, and Check
/// Design reports none.
#[test]
fn a_warped_fill_letters_travel_sews_no_stitches_in_place() {
    let dir = TempDir::new("warped-fill-travel");
    for (preset, level) in [(22, 94.0), (24, 84.0)] {
        let mut s = Session::new(&dir);
        run(&mut s, "design.new", json!({"width": 100, "height": 100}));
        run(
            &mut s,
            "text.add",
            json!({"text": "OpenStitch", "font": "lato-bold", "size": 10, "x": 50, "y": 50,
                   "fill": "Red", "envelope": preset, "envelopeLevel": level}),
        );
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "type": "fill", "underlay": false, "density": 5}),
        );
        let c = run(&mut s, "design.check", json!({"rules": ["inPlace"]}));
        assert!(found(&c, "inPlace").is_none(), "preset {preset}: {c}");
        run(&mut s, "design.export", json!({"path": "w.pes"}));
        let back = FileType::Pes
            .read(&std::fs::read(dir.join("w.pes")).expect("w.pes"))
            .expect("pes");
        let grid = openstitch_core::round_coordinate;
        let (mut run_len, mut longest) = (0, 0);
        for (i, st) in back.stitches.iter().enumerate() {
            let tiny = i > 0 && st.command == Command::Stitch && {
                let p = &back.stitches[i - 1];
                ((grid(p.x) - grid(st.x)) as f64).hypot((grid(p.y) - grid(st.y)) as f64) < 3.0
            };
            run_len = if tiny { run_len + 1 } else { 0 };
            longest = longest.max(run_len);
        }
        assert!(
            longest <= 6,
            "preset {preset}: {longest} in a row under 0.3 mm"
        );
    }
}

/// #1135: the run in place Check Design reads on a design is the run in the PES OpenStitch
/// exports from it, read back. These are the three PE-DESIGN references whose finding changed
/// when the count moved onto the 0.1 mm grid (before: 3, 8 and 7). PE-DESIGN's own files read 5,
/// 3 and 1. All 508 references agree this way (a release run of 325 s, too slow to keep here).
/// Since #1151 a squeezed fill stroke sews no stitches in place: text-transform-19's run is 3
/// (its O) and text-transform-25's 4.
#[test]
fn stitching_in_place_reads_the_same_on_the_design_and_its_file() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../compat/reference");
    let dir = TempDir::new("check-in-place-file");
    let longest = |st: &[Stitch]| {
        let (mut run, mut best) = (0, 0);
        for (i, s) in st.iter().enumerate() {
            let tiny = i > 0 && s.command == Command::Stitch && {
                let p = &st[i - 1];
                let grid = openstitch_core::round_coordinate;
                ((grid(p.x) - grid(s.x)) as f64).hypot((grid(p.y) - grid(s.y)) as f64) < 3.0
            };
            run = if tiny { run + 1 } else { 0 };
            best = best.max(run);
        }
        best
    };
    // The run Check Design reports (only over its limit of 6) and the run in the exported file.
    let both = |rel: &str, turned: bool| {
        let mut s = Session::new(&dir);
        run(&mut s, "design.new", json!({"width": 400, "height": 400}));
        if turned {
            run(
                &mut s,
                "machine.set",
                json!({"hoop": "160x260", "rotate90": true}),
            );
        }
        let path = root.join(rel).display().to_string();
        run(&mut s, "import.pes", json!({"path": path}));
        let c = run(&mut s, "design.check", json!({"rules": ["inPlace"]}));
        let flagged = found(&c, "inPlace").map(|f| f["value"].as_f64().expect("value"));
        run(&mut s, "design.export", json!({"path": "x.pes"}));
        let back = FileType::Pes
            .read(&std::fs::read(dir.join("x.pes")).expect("x.pes"))
            .expect("pes");
        (flagged, longest(&back.stitches))
    };
    for (rel, want) in [
        ("text/text-transform-19-openstitch-l100.pes", 3),
        ("text/text-transform-25-openstitch-l100.pes", 4),
        ("stitches/line-zigzag-miter-v-b2-m50.pes", 3),
    ] {
        let (flagged, file) = both(rel, false);
        assert_eq!(file, want, "{rel}: the file");
        assert_eq!(flagged, (want > 6).then_some(want as f64), "{rel}");
        // A hoop turned 90°: the PES holds the stitches turned about the design's centre, on
        // another grid, and Check Design counts them as written.
        let (flagged, file) = both(rel, true);
        assert_eq!(
            flagged,
            (file > 6).then_some(file as f64),
            "{rel}, turned: file {file}"
        );
    }
}

/// #1036: the crowding count (the spot with the most pairs of needle points under 0.25 mm
/// within 3 mm) is the same here as in `tools/sew_check.py`, whose self-test asserts these
/// numbers too: PE-DESIGN's radial circle at its highest density, the kit's most crowded design
/// and a plain one.
#[test]
fn crowding_counts_agree_with_sew_check() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lim = &openstitch_engine::quality::LIMITS;
    for (rel, want) in [
        ("compat/reference/stitches/radial-circle40-dmax.pes", 591),
        (
            "examples/sew/image-tools/L-autopunch-storybook-openstitch.pes",
            227,
        ),
        ("examples/sew/fills/fill-options.pes", 29),
    ] {
        let p = FileType::Pes
            .read(&std::fs::read(root.join(rel)).expect("read"))
            .expect("pes");
        let c = openstitch_engine::quality::crowding(
            &p.stitches,
            lim.crowd_check_mm,
            lim.crowd_close_mm,
            0,
        )
        .expect("stitches");
        assert_eq!(c.pairs, want, "{rel}");
    }
}

/// #1036: the quick bound that lets most designs skip the count is never below the count (so it
/// never hides a crowded spot), and a design just over the warning is counted, not skipped.
#[test]
fn the_crowding_bound_never_hides_a_crowded_spot() {
    use openstitch_engine::quality::{crowding, crowding_bound};
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    for rel in [
        "compat/reference/stitches/radial-circle40-dmax.pes",
        "examples/sew/image-tools/L-autopunch-storybook-openstitch.pes",
        "examples/sew/fills/fill-options.pes",
    ] {
        let p = FileType::Pes
            .read(&std::fs::read(root.join(rel)).expect("read"))
            .expect("pes");
        let exact = crowding(&p.stitches, 3.0, 0.25, 0).expect("stitches").pairs;
        let bound = crowding_bound(&p.stitches, 3.0, 0.25).expect("bound");
        assert!(bound >= exact, "{rel}: bound {bound} under {exact}");
        // Skipped under the floor exactly when the count is under it too.
        assert_eq!(
            crowding(&p.stitches, 3.0, 0.25, 600).is_none(),
            exact <= 600,
            "{rel}"
        );
    }
    let mut s = Session::new(std::env::temp_dir());
    radial_pair(&mut s, WARN_DENSITY, WARN_OFF);
    let t = Traced::new(s.design());
    let exact = crowding(&t.pattern.stitches, 3.0, 0.25, 0)
        .expect("stitches")
        .pairs;
    assert!(exact > 600, "{exact}");
    assert!(crowding_bound(&t.pattern.stitches, 3.0, 0.25).expect("bound") >= exact);
    assert_eq!(
        crowding(&t.pattern.stitches, 3.0, 0.25, 600).map(|c| c.pairs),
        Some(exact),
        "just over the warning: counted, not skipped"
    );
}

/// #1036: no PE-DESIGN reference file and no kit design reaches the crowding warning (PE-DESIGN's
/// own most crowded file, its radial circle at density 7, has 591 pairs; the warning is over 600).
#[test]
fn no_reference_or_kit_design_is_crowded() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let lim = &openstitch_engine::quality::LIMITS;
    let mut seen = 0;
    for dir in ["compat/reference", "examples/sew"] {
        let mut todo = vec![root.join(dir)];
        while let Some(d) = todo.pop() {
            for e in std::fs::read_dir(d).expect("dir").flatten() {
                let f = e.path();
                if f.is_dir() {
                    todo.push(f);
                } else if f.extension().is_some_and(|x| x.eq_ignore_ascii_case("pes")) {
                    let Ok(p) = FileType::Pes.read(&std::fs::read(&f).expect("read")) else {
                        continue;
                    };
                    seen += 1;
                    let c = openstitch_engine::quality::crowding(
                        &p.stitches,
                        lim.crowd_check_mm,
                        lim.crowd_close_mm,
                        lim.max_crowd_pairs_warn,
                    );
                    assert!(
                        c.is_none(),
                        "{}: {:?} pairs",
                        f.display(),
                        c.map(|c| c.pairs)
                    );
                }
            }
        }
    }
    assert!(seen > 500, "{seen} files");
}

/// Radial circles just over the crowding warning: 662 pairs (two at density 3 0.1 mm apart
/// have 574, just under it; two at density 5 0.2 mm apart 1,242, over the error).
const WARN_DENSITY: f64 = 3.5;
const WARN_OFF: f64 = 0.2;

/// Two radial circles 40 mm across, the second `off` mm to the right: a real generator setting
/// whose needle points crowd where the two centres overlap.
fn radial_pair(s: &mut Session, density: f64, off: f64) {
    for (id, dx) in [(1, 0.0), (2, off)] {
        run(
            s,
            "shape.ellipse",
            json!({"cx": 50.0 + dx, "cy": 50, "rx": 20, "ry": 20, "fill": "Red"}),
        );
        run(
            s,
            "object.setRegion",
            json!({"ids": [id], "type": "radial", "density": density}),
        );
    }
}

/// #1036: needle crowding is a warning over 600 pairs (beyond anything PE-DESIGN's own files sew)
/// and an error over 1,200 (twice that), on real generator settings: radial circles laid over each
/// other. OpenStitch's own radial at its highest density stays under PE-DESIGN's 591.
#[test]
fn crowded_needles_warn_then_stop_a_strict_export() {
    let crowd = |density: f64, off: Option<f64>| {
        let mut s = Session::new(std::env::temp_dir());
        run(&mut s, "machine.set", json!({"hoop": "100x100"}));
        match off {
            Some(off) => radial_pair(&mut s, density, off),
            None => {
                run(
                    &mut s,
                    "shape.ellipse",
                    json!({"cx": 50, "cy": 50, "rx": 20, "ry": 20, "fill": "Red"}),
                );
                run(
                    &mut s,
                    "object.setRegion",
                    json!({"ids": [1], "type": "radial", "density": density}),
                );
            }
        }
        let c = run(&mut s, "design.check", json!({"rules": ["crowdedNeedles"]}));
        (s, found(&c, "crowdedNeedles").cloned())
    };
    // OpenStitch's radial at its highest density (#1112): clear of the warning.
    assert!(crowd(7.0, None).1.is_none());
    // Just under the warning: none.
    assert!(crowd(3.0, Some(0.1)).1.is_none());
    // Just over: a warning naming both circles and the spot.
    let (_, f) = crowd(WARN_DENSITY, Some(WARN_OFF));
    let f = f.expect("over 600 pairs");
    assert_eq!(
        (f["severity"].as_str(), f["limit"].as_f64()),
        (Some("warning"), Some(600.0)),
        "{f}"
    );
    assert!(
        f["value"]
            .as_f64()
            .is_some_and(|v| v > 600.0 && v <= 1200.0),
        "{f}"
    );
    assert_eq!(f["ids"], json!([1, 2]), "{f}");
    assert!(f["bbox"].is_array() && f["fix"].is_null(), "{f}");
    assert!(
        f["message"]
            .as_str()
            .is_some_and(|m| m.contains("no sew-out evidence")),
        "{f}"
    );
    // Over 1,200: an error, which a strict export refuses; the default export writes and lists it.
    let (mut s, f) = crowd(5.0, Some(0.2));
    let f = f.expect("over 1,200 pairs");
    assert_eq!(
        (f["severity"].as_str(), f["limit"].as_f64()),
        (Some("error"), Some(1200.0)),
        "{f}"
    );
    let dir = TempDir::new("check-crowd-export");
    let strict = dir.join("strict.pes");
    let e = s
        .execute(
            "design.export",
            Some(&json!({"path": strict.display().to_string(), "checks": "strict"})),
        )
        .expect_err("strict refuses a design with an error");
    assert!(e.to_string().contains("nothing was written"), "{e}");
    assert!(!strict.exists());
    let warn = dir.join("warn.pes");
    let r = run(
        &mut s,
        "design.export",
        json!({"path": warn.display().to_string()}),
    );
    assert!(warn.exists());
    assert!(
        r["problems"].to_string().contains("pairs of needle points"),
        "{r}"
    );
    // A design without errors exports under strict (two radial centres 0.1 mm apart also stack
    // 30 needle drops in one spot, an error of its own, so a plain fill here).
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "machine.set", json!({"hoop": "100x100"}));
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 20, "y": 20, "width": 30, "height": 20, "fill": "Blue"}),
    );
    let clean = dir.join("clean.pes");
    run(
        &mut s,
        "design.export",
        json!({"path": clean.display().to_string(), "checks": "strict"}),
    );
    assert!(clean.exists());
    assert!(
        s.execute(
            "design.export",
            Some(&json!({"path": "x.pes", "checks": "loose"}))
        )
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

/// #1036: a plain fill whose rows hide less than 40 % of the fabric (on the coverage curve
/// measured on the T1 sew-outs) gets the `lowCoverage` note, without a fix: about 1.06 lines/mm
/// hides 40 %, so 0.9 lines/mm is just under and 1.3 just over. A gradation (open on purpose) is
/// not judged.
#[test]
fn a_fill_the_fabric_shows_through_gets_a_low_coverage_note() {
    let check = |params: Value| {
        let mut s = Session::new(std::env::temp_dir());
        run(&mut s, "machine.set", json!({"hoop": "100x100"}));
        let id = run(
            &mut s,
            "shape.rectangle",
            json!({"x": 20, "y": 20, "width": 40, "height": 30, "fill": "Blue"}),
        )["id"]
            .as_u64()
            .expect("id");
        let mut p = params;
        p["ids"] = json!([id]);
        run(&mut s, "object.setRegion", p);
        let c = run(&mut s, "design.check", json!({"rules": ["lowCoverage"]}));
        (id, c.clone(), found(&c, "lowCoverage").cloned())
    };
    let (id, c, f) = check(json!({"density": 0.9, "underlay": false}));
    let f = f.expect("0.9 lines/mm shows the fabric");
    assert_eq!(f["ids"], json!([id]));
    assert_eq!(
        (f["severity"].as_str(), f["limit"].as_f64()),
        (Some("info"), Some(40.0)),
        "{f}"
    );
    assert!(f["value"].as_f64().is_some_and(|v| v < 40.0), "{f}");
    assert!(
        f["fix"].is_null(),
        "no fix: the density is the user's choice: {f}"
    );
    assert!(
        f["message"]
            .as_str()
            .is_some_and(|m| m.contains("1.1 lines/mm")),
        "{f}"
    );
    // A note: the design stays ready, with no warning.
    assert_eq!(
        (c["ready"].as_bool(), c["warnings"].as_u64()),
        (Some(true), Some(0)),
        "{c}"
    );
    assert!(
        check(json!({"density": 1.3, "underlay": false}))
            .2
            .is_none()
    );
    assert!(
        check(json!({"density": 0.5, "underlay": false, "gradationPattern": true}))
            .2
            .is_none(),
        "a gradation is open on purpose"
    );
}

/// #1036: small text is judged by its own font's smallest good size (fonts.list sizeRange), not
/// one size for every font. The fix offers the Small Text font where the text is big enough
/// for it, else the font's smallest size, and clears the finding.
#[test]
fn small_text_is_judged_by_its_own_font() {
    let small = |font: &str, size: f64| {
        let mut s = Session::new(std::env::temp_dir());
        run(&mut s, "machine.set", json!({"hoop": "100x100"}));
        run(
            &mut s,
            "text.add",
            json!({"text": "Hi", "font": font, "size": size, "x": 50, "y": 50}),
        );
        let c = run(&mut s, "design.check", json!({"rules": ["textTooSmall"]}));
        let f = found(&c, "textTooSmall").cloned();
        if let Some(f) = &f {
            let fix = f["fix"].clone();
            run(
                &mut s,
                fix["command"].as_str().expect("command"),
                fix["params"].clone(),
            );
            let c = run(&mut s, "design.check", json!({"rules": ["textTooSmall"]}));
            assert!(found(&c, "textTooSmall").is_none(), "{font} {size}: {c}");
        }
        f
    };
    // Lato Bold sews well from 6 mm: 5 mm is small (the old single limit, 4 mm, let it pass).
    let f = small("lato-bold", 5.0).expect("5 mm Lato Bold");
    assert_eq!(
        (f["value"].as_f64(), f["limit"].as_f64()),
        (Some(5.0), Some(6.0)),
        "{f}"
    );
    assert_eq!(f["fix"]["params"]["font"], json!("openstitch-line"), "{f}");
    assert!(small("lato-bold", 6.0).is_none());
    // Pacifico from 7 mm.
    let f = small("pacifico", 6.5).expect("6.5 mm Pacifico");
    assert_eq!(f["limit"], json!(7.0), "{f}");
    assert!(small("pacifico", 7.0).is_none());
    // The Small Text font itself, under its own 3 mm: the fix is its smallest size.
    let f = small("openstitch-line", 2.5).expect("2.5 mm Small Text");
    assert_eq!(f["fix"]["params"]["size"], json!(3.0), "{f}");
    assert!(small("openstitch-line", 3.0).is_none());
}

/// #1036: `design.check` names the readiness level from its counts alone: ready to sew with no
/// errors or warnings (notes never count), sewable with small risks with warnings only, fix this
/// first with any error. A level, not a score.
#[test]
fn the_readiness_level_follows_the_errors_and_warnings() {
    let level = |steps: &[(&str, Value)]| {
        let mut s = Session::new(std::env::temp_dir());
        run(&mut s, "machine.set", json!({"hoop": "100x100"}));
        for (c, p) in steps {
            run(&mut s, c, p.clone());
        }
        let c = run(&mut s, "design.check", json!({}));
        (c["level"].as_str().unwrap_or_default().to_string(), c)
    };
    let rect = |x: f64| {
        (
            "shape.rectangle",
            json!({"x": x, "y": 20, "width": 30, "height": 20, "fill": "Blue"}),
        )
    };
    let (l, c) = level(&[rect(20.0)]);
    assert_eq!(l, "readyToSew", "{c}");
    // A note only (an open fill, #1036): still ready.
    let (l, c) = level(&[
        rect(20.0),
        (
            "object.setRegion",
            json!({"ids": [1], "density": 0.9, "underlay": false}),
        ),
    ]);
    assert_eq!(
        (l.as_str(), c["notes"].as_u64()),
        ("readyToSew", Some(1)),
        "{c}"
    );
    // A warning only (3 mm text).
    let (l, c) = level(&[
        rect(20.0),
        (
            "text.add",
            json!({"text": "Hi", "size": 3, "x": 50, "y": 70}),
        ),
    ]);
    assert_eq!(
        (l.as_str(), c["errors"].as_u64()),
        ("sewableWithSmallRisks", Some(0)),
        "{c}"
    );
    // An error (past the hoop), whatever else.
    let (l, c) = level(&[
        rect(80.0),
        (
            "text.add",
            json!({"text": "Hi", "size": 3, "x": 50, "y": 70}),
        ),
    ]);
    assert_eq!(l, "fixThisFirst", "{c}");
    assert!(c.get("score").is_none(), "no number: {c}");
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
    // Lato Bold's smallest good size, its own limit since #1036 (4 mm for every font before).
    run(
        &mut s,
        "text.add",
        json!({"text": "Hi", "size": 6, "x": 50, "y": 60}),
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

/// #1032 through `design.check` itself: a lock counts only when it comes back exactly to its
/// point. A lock 0.02 mm off (what Auto Punch rows did, and what a 0.1 mm stitch file can turn
/// into no lock at all) is an untied end; the same lock made exact, as the compiler now snaps
/// it, is not.
#[test]
fn a_lock_a_few_hundredths_off_is_untied_until_it_is_made_exact() {
    let dir = TempDir::new("check-near-lock");
    let mut s = Session::new(&dir);
    sections(
        &mut s,
        &dir,
        "near.pes",
        &[row(0.0, 0.0, 10, true), row(0.0, 100.0, 10, true)],
    );
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    assert!(found(&c, "untied").is_none(), "{c}");
    let exact = s.design().clone();
    // The first row's lock: its last needle point, just before the trim, moved 0.02 mm.
    let mut near = exact.clone();
    let block = near
        .objects
        .iter_mut()
        .find_map(|o| o.stitch_block.as_mut())
        .expect("the stitch object");
    let trim = block
        .stitches
        .iter()
        .position(|st| st.command == Command::Trim)
        .expect("a trim");
    let last = block.stitches[..trim]
        .iter()
        .rposition(|st| st.command == Command::Stitch)
        .expect("the lock's last point");
    block.stitches[last].x += 0.2;
    assert!(s.adopt_design(near));
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    let f = found(&c, "untied").unwrap_or_else(|| panic!("a lock 0.02 mm off: {c}"));
    assert_eq!(f["value"], json!(1.0), "{f}");
    // Made exact again: tied.
    assert!(s.adopt_design(exact));
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    assert!(found(&c, "untied").is_none(), "the exact lock: {c}");
}

/// #1032 through `design.check`: a stitch in place at a start is a tie only with its tie-in
/// after it (PE-DESIGN's one-step tie on a Hoop Size page, #1013). Alone it is an untied start.
#[test]
fn a_stitch_in_place_without_a_tie_in_is_an_untied_start() {
    let dir = TempDir::new("check-in-place-start");
    // Row ends locked; the second row starts on a stitch in place, then its 0.3 mm tie-in.
    let locked_end = |mut r: Vec<(f64, f64)>| {
        let b = r[r.len() - 1];
        r.extend([(b.0 - 3.0, b.1), b]);
        r
    };
    let tied = row(0.0, 0.0, 10, true);
    let plain = row(0.0, 100.0, 10, false);
    let mut with_tie_in = plain.clone();
    with_tie_in.splice(1..1, [(0.0, 100.0), (3.0, 100.0), (0.0, 100.0)]);
    let mut alone = plain.clone();
    alone.insert(1, (0.0, 100.0));
    let mut s = Session::new(&dir);
    sections(
        &mut s,
        &dir,
        "in-place-tie-in.pes",
        &[tied.clone(), locked_end(with_tie_in)],
    );
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    assert!(
        found(&c, "untied").is_none(),
        "a stitch in place with its tie-in: {c}"
    );
    let mut s = Session::new(&dir);
    sections(
        &mut s,
        &dir,
        "in-place-alone.pes",
        &[tied, locked_end(alone)],
    );
    let c = run(&mut s, "design.check", json!({"rules": ["untied"]}));
    let f = found(&c, "untied").unwrap_or_else(|| panic!("a stitch in place alone: {c}"));
    assert_eq!(f["value"], json!(1.0), "{f}");
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

/// #1043 (from the AI-coverage review of #1085): a turned zigzag net steps along the outline
/// where it cuts a rung and the diagonal back, down to 0.1 mm, as PE-DESIGN 11.50 does (round 13:
/// 37 stitches under 0.3 mm in its 1,141 at 45°). Those steps stay far under `shortStitch`'s 160
/// in 1,000, so a net with them is not reported.
#[test]
fn a_turned_zigzag_nets_steps_along_the_outline_are_not_short_stitch_findings() {
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 35, "y": 40, "width": 30, "height": 20, "fill": "Red"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "zigzagNetFill", "spacing": 5.0, "density": 2.25,
            "patternWidth": 3.0, "underlay": true, "angle": 45}),
    );
    let st = openstitch_design::Compiler::for_design(s.design())
        .compile(s.design())
        .stitches;
    let moves = st
        .windows(2)
        .filter(|w| w[0].command == Command::Stitch && w[1].command == Command::Stitch);
    let short = moves
        .clone()
        .filter(|w| {
            let d = (w[1].x - w[0].x).hypot(w[1].y - w[0].y);
            d > 1e-6 && d < 3.0
        })
        .count();
    let all = moves.count();
    assert!(
        short > 0,
        "the net has steps under 0.3 mm along the outline"
    );
    assert!(
        (short as f64) * 1000.0 / (all as f64) < 160.0,
        "{short} in {all}"
    );
    let c = run(&mut s, "design.check", json!({"rules": ["shortStitch"]}));
    assert!(found(&c, "shortStitch").is_none(), "{short} in {all}: {c}");
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

/// A satin skeleton bent 90° (the kit's V in shapes/shape-tools.pes) has an outline that goes
/// round the bend once, so Check Design finds no selfCrossing on it (#1136).
#[test]
fn a_bent_satin_skeleton_is_not_a_self_crossing() {
    let mut s = Session::new(std::env::temp_dir());
    run(&mut s, "design.new", json!({ "width": 100, "height": 100 }));
    run(
        &mut s,
        "shape.skeletonSatin",
        json!({ "points": [[8, 82], [20, 70], [32, 82]], "widths": [1.5, 4, 1.5] }),
    );
    let c = run(&mut s, "design.check", json!({}));
    assert!(
        rules(&c).iter().all(|(rule, _)| rule != "selfCrossing"),
        "{c}"
    );
}
