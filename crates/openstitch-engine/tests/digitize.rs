//! Image conversion commands: placement, undo, sandboxing and hostile input.
#![cfg(feature = "digitize")]

use openstitch_digitize::Raster;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn temp_dir() -> TempDir {
    TempDir::new("digitize")
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// 60×40 px: a red block, a thin blue bar and a black outline square on white.
fn write_sample(dir: &std::path::Path) {
    let img = Raster::from_fn(60, 40, |x, y| {
        let inside = |x0, y0, x1, y1| x >= x0 && x < x1 && y >= y0 && y < y1;
        if inside(5, 5, 25, 35) {
            [220, 20, 30, 255]
        } else if inside(35, 10, 55, 14) {
            [20, 40, 200, 255]
        } else if inside(35, 18, 55, 36) && !inside(38, 21, 52, 33) {
            [0, 0, 0, 255]
        } else {
            [255, 255, 255, 255]
        }
    })
    .expect("size");
    std::fs::write(dir.join("sample.png"), img.encode_png().expect("png")).expect("write");
}

#[test]
fn conversions_add_objects_in_one_undo_step() {
    let dir = temp_dir();
    write_sample(&dir);
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    let r = run(
        &mut s,
        "digitize.autoPunch",
        json!({"path": "sample.png", "colors": 4, "width": 60, "x": 20, "y": 30}),
    );
    let ids = r["ids"].as_array().expect("ids");
    assert_eq!(ids.len(), 3, "{r}");
    assert_eq!(r["regions"], json!(3));
    let list = run(&mut s, "object.list", json!({}));
    let b = &list[0]["bounds"];
    assert!(b["x"].as_f64().expect("x") >= 20.0 && b["y"].as_f64().expect("y") >= 30.0);
    run(&mut s, "history.undo", json!({}));
    assert!(
        s.design().objects.is_empty(),
        "one undo removes the whole conversion"
    );

    let r = run(
        &mut s,
        "digitize.crossStitch",
        json!({"path": "sample.png", "cellSize": 2, "width": 60}),
    );
    assert_eq!(r["columns"], json!(30));
    assert_eq!(r["rows"], json!(20));
    assert!(r["ids"].as_array().expect("ids").len() >= 3);
    // OpenStitch mode: the same grid, its cells filled with many more needle points.
    let count = |s: &Session, ids: &Value| -> usize {
        ids.as_array()
            .expect("ids")
            .iter()
            .filter_map(|id| {
                let id = id.as_u64().expect("id");
                s.design().objects.iter().find(|o| o.id == id)
            })
            .filter_map(|o| o.stitch_block.as_ref())
            .map(|b| b.stitches.len())
            .sum()
    };
    let pe = count(&s, &r["ids"]);
    let os = run(
        &mut s,
        "digitize.crossStitch",
        json!({"path": "sample.png", "cellSize": 2, "width": 60, "compatMode": "openStitch"}),
    );
    assert_eq!(
        (os["columns"].clone(), os["rows"].clone()),
        (json!(30), json!(20))
    );
    assert!(count(&s, &os["ids"]) > pe, "filled crosses");
    let plain = run(
        &mut s,
        "digitize.crossStitch",
        json!({"path": "sample.png", "cellSize": 2, "width": 60, "compatMode": "openStitch", "fill": false}),
    );
    // Without fill: PE-DESIGN's crosses, every needle point a cell corner or centre (1 mm
    // apart on 2 mm cells), where filled crosses also step between them.
    let on_half_cells = |s: &Session, ids: &Value| -> bool {
        let pts: Vec<(f64, f64)> = ids
            .as_array()
            .expect("ids")
            .iter()
            .filter_map(|id| {
                let id = id.as_u64().expect("id");
                s.design().objects.iter().find(|o| o.id == id)
            })
            .filter_map(|o| o.stitch_block.as_ref())
            // Without the runs' tie-ins and locks (1 mm steps along a diagonal).
            .flat_map(|b| {
                openstitch_design::stitch_edit::unlocked(&b.stitches)
                    .into_iter()
                    .map(|p| (p.x, p.y))
            })
            .collect();
        let (x0, y0) = pts[0];
        pts.iter().all(|&(x, y)| {
            let (u, v) = ((x - x0) / 10.0, (y - y0) / 10.0);
            (u - u.round()).abs() < 1e-6 && (v - v.round()).abs() < 1e-6
        })
    };
    assert!(on_half_cells(&s, &plain["ids"]));
    assert!(!on_half_cells(&s, &os["ids"]));

    let r = run(
        &mut s,
        "digitize.photoStitch",
        json!({"path": "sample.png", "mode": "mono", "style": "lines", "width": 30}),
    );
    assert_eq!(r["ids"].as_array().expect("ids").len(), 1);
    let r = run(
        &mut s,
        "digitize.photoStitch",
        json!({"path": "sample.png", "colors": 4}),
    );
    assert!(r["ids"].as_array().expect("ids").len() >= 2);
    let stats = run(&mut s, "design.generate", json!({}));
    assert!(stats["stitches"].as_u64().expect("count") > 1000, "{stats}");
}

#[test]
fn auto_punch_open_stitch_mode_works_at_800_pixels() {
    // 1600 px wide: a 4 px dark line under a red block. OpenStitch mode's defaults are an
    // 800 px working raster and a 2 mm² region floor; at PE-DESIGN mode's 400 px the line is
    // one working pixel wide, and the census's 3×3 modal filter removes it.
    let dir = temp_dir();
    let img = Raster::from_fn(1600, 400, |x, y| {
        if (100..1500).contains(&x) && (200..204).contains(&y) {
            [20, 20, 20, 255]
        } else if (400..1200).contains(&x) && (40..160).contains(&y) {
            [220, 20, 30, 255]
        } else {
            [255, 255, 255, 255]
        }
    })
    .expect("size");
    std::fs::write(dir.join("line.png"), img.encode_png().expect("png")).expect("write");
    let mut s = Session::new(&dir);
    let mut convert = |extra: Value| {
        run(&mut s, "design.new", json!({"width": 130, "height": 50}));
        let mut p =
            json!({"path": "line.png", "width": 120, "colors": 3, "compatMode": "openStitch"});
        for (k, v) in extra.as_object().expect("object") {
            p[k] = v.clone();
        }
        let r = run(&mut s, "digitize.autoPunch", p);
        (r["regions"].clone(), r["colors"].clone())
    };
    let defaults = convert(json!({}));
    assert_eq!(defaults.0, json!(2), "the block and the line: {defaults:?}");
    assert_eq!(defaults, convert(json!({"resolution": 800, "minArea": 2})));
    assert_eq!(convert(json!({"resolution": 400})).0, json!(1));
}

#[test]
fn design_center_stages() {
    let dir = temp_dir();
    write_sample(&dir);
    let mut s = Session::new(&dir);
    let r = run(
        &mut s,
        "digitize.lineImage",
        json!({"path": "sample.png", "out": "lines.png"}),
    );
    assert_eq!(r["width"], json!(60));
    assert!(r["linePixels"].as_u64().expect("count") > 100);
    let line_png = std::fs::read(dir.join("lines.png")).expect("written");
    assert_eq!(Raster::decode(&line_png).expect("png").width(), 60);

    let fig = run(
        &mut s,
        "digitize.figure",
        json!({"path": "sample.png", "width": 60, "x": 0, "y": 0}),
    );
    assert!(!fig["lines"].as_array().expect("lines").is_empty());
    assert!(s.design().objects.is_empty(), "figure is read-only");

    let r = run(
        &mut s,
        "digitize.designCenter",
        json!({"path": "sample.png", "width": 60, "lineSew": "triple", "lineColor": "Red"}),
    );
    let ids = r["ids"].as_array().expect("ids");
    assert!(!ids.is_empty());
    let last = s.design().objects.last().expect("lines object");
    assert_eq!(
        last.line.as_ref().map(|l| l.thread.name.as_deref()),
        Some(Some("Red"))
    );

    // OpenStitch mode: the sample's 3 mm strokes sew as parallel running lines, one open path
    // per line, unless lineSew is set.
    let pe_points = s.design().objects.last().expect("lines").contours[0]
        .points
        .len();
    let r = run(
        &mut s,
        "digitize.designCenter",
        json!({"path": "sample.png", "width": 60, "compatMode": "openStitch"}),
    );
    assert!(!r["ids"].as_array().expect("ids").is_empty());
    let last = s.design().objects.last().expect("lines object");
    assert_eq!(
        last.line.as_ref().map(|l| l.kind),
        Some(openstitch_design::LineType::Running)
    );
    assert!(!last.contours[0].closed);
    assert!(last.contours[0].points.len() > pe_points);
    run(
        &mut s,
        "digitize.designCenter",
        json!({"path": "sample.png", "width": 60, "compatMode": "openStitch", "lineSew": "zigzag"}),
    );
    let last = s.design().objects.last().expect("lines object");
    assert_eq!(
        last.line.as_ref().map(|l| l.kind),
        Some(openstitch_design::LineType::Zigzag)
    );
    // #627: running lines that meet are one path by default; joinLines false keeps one piece
    // per figure line.
    let joined = run(
        &mut s,
        "digitize.designCenter",
        json!({"path": "sample.png", "width": 60, "regionSew": "none"}),
    );
    let pieces = s.design().objects.last().expect("lines").contours.len();
    let apart = run(
        &mut s,
        "digitize.designCenter",
        json!({"path": "sample.png", "width": 60, "regionSew": "none", "joinLines": false}),
    );
    let lines = apart["lines"].as_u64().expect("lines") as usize;
    assert_eq!(
        s.design().objects.last().expect("lines").contours.len(),
        lines
    );
    assert!(pieces <= lines, "{pieces} {lines} {joined}");
}

#[test]
fn sandbox_confines_image_reads_and_writes() {
    let root = temp_dir();
    write_sample(&root);
    let outside = temp_dir();
    write_sample(&outside);
    let mut s = Session::sandboxed(&root);
    assert!(
        s.execute("digitize.autoPunch", Some(&json!({"path": "sample.png"})))
            .is_ok()
    );
    let escape = outside.join("sample.png").display().to_string();
    assert!(
        s.execute("digitize.autoPunch", Some(&json!({"path": escape})))
            .is_err()
    );
    assert!(
        s.execute(
            "digitize.autoPunch",
            Some(&json!({"path": "../sample.png"}))
        )
        .is_err()
    );
    let out = outside.join("x.png").display().to_string();
    assert!(
        s.execute(
            "digitize.lineImage",
            Some(&json!({"path": "sample.png", "out": out}))
        )
        .is_err()
    );
}

#[test]
fn bad_digitize_parameters_fail_cleanly() {
    let dir = temp_dir();
    write_sample(&dir);
    std::fs::write(dir.join("corrupt.png"), b"\x89PNG\r\n\x1a\n garbage").expect("write");
    std::fs::write(dir.join("empty.png"), b"").expect("write");
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 0, "y": 0, "width": 10, "height": 10}),
    );
    let before = s.design().clone();
    let bad: Vec<(&str, Value)> = vec![
        ("digitize.autoPunch", json!({})),
        ("digitize.autoPunch", json!({"path": "missing.png"})),
        ("digitize.autoPunch", json!({"path": "corrupt.png"})),
        ("digitize.autoPunch", json!({"path": "empty.png"})),
        (
            "digitize.autoPunch",
            // 0 is the automatic count; 65 is out of range.
            json!({"path": "sample.png", "colors": 65}),
        ),
        (
            "digitize.autoPunch",
            json!({"path": "sample.png", "colors": 65}),
        ),
        (
            "digitize.autoPunch",
            json!({"path": "sample.png", "colors": 2.5}),
        ),
        (
            "digitize.autoPunch",
            json!({"path": "sample.png", "resolution": 100000}),
        ),
        (
            "digitize.autoPunch",
            json!({"path": "sample.png", "width": 0}),
        ),
        (
            "digitize.autoPunch",
            json!({"path": "sample.png", "width": 1e300}),
        ),
        (
            "digitize.autoPunch",
            json!({"path": "sample.png", "simplify": -1}),
        ),
        (
            "digitize.autoPunch",
            json!({"path": "sample.png", "curves": "yes"}),
        ),
        (
            "digitize.autoPunch",
            json!({"path": "sample.png", "x": "left"}),
        ),
        (
            "digitize.crossStitch",
            json!({"path": "sample.png", "cellSize": 0.1}),
        ),
        (
            "digitize.crossStitch",
            json!({"path": "sample.png", "type": "triple"}),
        ),
        (
            "digitize.crossStitch",
            json!({"path": "sample.png", "compatMode": "pe"}),
        ),
        (
            "digitize.crossStitch",
            json!({"path": "sample.png", "fill": "yes"}),
        ),
        (
            "digitize.crossStitch",
            json!({"path": "sample.png", "cellSize": 1, "width": 2000}),
        ),
        (
            "digitize.photoStitch",
            json!({"path": "sample.png", "style": "oil"}),
        ),
        (
            "digitize.photoStitch",
            json!({"path": "sample.png", "mode": "mono", "thread": "#zzzzzz"}),
        ),
        (
            "digitize.photoStitch",
            json!({"path": "sample.png", "rowSpacing": 0}),
        ),
        (
            "digitize.photoStitch",
            json!({"path": "sample.png", "rowSpacing": 0.2, "stitchLength": 0.3, "width": 2000}),
        ),
        (
            "digitize.designCenter",
            json!({"path": "sample.png", "lineSew": "chain"}),
        ),
        (
            "digitize.designCenter",
            json!({"path": "sample.png", "compatMode": "pe"}),
        ),
        (
            "digitize.designCenter",
            json!({"path": "sample.png", "threshold": 300}),
        ),
        (
            "digitize.designCenter",
            json!({"path": "sample.png", "threshold": 0}),
        ),
        (
            "digitize.designCenter",
            json!({"path": "sample.png", "lineWidth": 50}),
        ),
        (
            "digitize.designCenter",
            json!({"path": "sample.png", "lineSew": "none", "regionSew": "none"}),
        ),
        ("digitize.figure", json!({"path": 7})),
        (
            "digitize.lineImage",
            json!({"path": "sample.png", "out": 3}),
        ),
    ];
    for (id, params) in bad {
        let r = s.execute(id, Some(&params));
        assert!(r.is_err(), "{id} {params} should fail, got {r:?}");
        assert_eq!(s.design(), &before, "{id} {params} changed the design");
    }
    assert!(s.can_undo());
}

/// B16.1's `fidelity` parameter: High needs OpenStitch mode, lets Auto Punch's automatic count
/// pass PE-DESIGN's 10 and defaults Photo Stitch 1 to 24 colours.
#[test]
fn fidelity_high_raises_the_colour_counts() {
    let dir = temp_dir();
    // Sixteen flat stripes of clearly different colours.
    let img = Raster::from_fn(160, 40, |x, _| {
        let k = x / 10;
        [
            (k * 53 % 256) as u8,
            (k * 97 % 256) as u8,
            (255 - k * 15) as u8,
            255,
        ]
    })
    .expect("size");
    std::fs::write(dir.join("stripes.png"), img.encode_png().expect("png")).expect("write");
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 200, "height": 100}));
    let colours = |s: &mut Session, cmd: &str, extra: Value| -> usize {
        let mut p = json!({"path": "stripes.png", "width": 160, "compatMode": "openStitch"});
        p.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        run(s, cmd, p)["colors"].as_array().expect("colors").len()
    };
    let standard = colours(
        &mut s,
        "digitize.autoPunch",
        json!({"skipBackground": false}),
    );
    let high = colours(
        &mut s,
        "digitize.autoPunch",
        json!({"skipBackground": false, "fidelity": "high"}),
    );
    assert!(
        standard <= 10,
        "Standard keeps PE-DESIGN's ceiling: {standard}"
    );
    assert!(
        high > standard,
        "High may use more colours: {high} against {standard}"
    );
    assert!(high <= 24);
    let ps1 = colours(&mut s, "digitize.photoStitch", json!({"fidelity": "high"}));
    assert!(ps1 > 10 && ps1 <= 25, "Photo Stitch 1 at High: {ps1}");
    let e = s
        .execute(
            "digitize.photoStitch",
            Some(&json!({"path": "stripes.png", "width": 160, "fidelity": "high"})),
        )
        .expect_err("High needs OpenStitch mode");
    assert!(e.to_string().contains("fidelity"), "{e}");
}

/// B16.1 T5's thread sources: OpenStitch mode chooses only from the user's own chart when asked,
/// lists the threads to buy against it, and PE-DESIGN mode takes no source.
#[test]
fn thread_sources_choose_from_the_users_threads_and_list_the_rest() {
    let dir = temp_dir();
    write_sample(&dir);
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    run(&mut s, "threads.createChart", json!({"name": "My Threads"}));
    for (c, code) in [("#c80a1e", "1"), ("#1428c8", "2"), ("#000000", "3")] {
        run(
            &mut s,
            "threads.addColor",
            json!({"chart": "My Threads", "color": c, "code": code, "brand": "Mine"}),
        );
    }
    let base = json!({"path": "sample.png", "width": 60, "compatMode": "openStitch"});
    let with = |extra: Value| {
        let mut p = base.clone();
        p.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        p
    };
    let mine = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"threads": "My Threads", "owned": "My Threads"})),
    );
    let own = ["#c80a1e", "#1428c8", "#000000"];
    for c in mine["colors"].as_array().expect("colors") {
        assert!(own.contains(&c.as_str().expect("hex")), "{c} is not mine");
    }
    assert_eq!(mine["toBuy"], json!([]), "everything chosen is owned");
    let all = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"threads": "all", "owned": "My Threads"})),
    );
    let buy = all["toBuy"].as_array().expect("toBuy");
    let chosen = all["colors"].as_array().expect("colors");
    assert!(!buy.is_empty() && buy.len() <= chosen.len(), "{all}");
    assert!(all.get("toBuy").is_some());
    assert!(
        run(&mut s, "digitize.autoPunch", base.clone())
            .get("toBuy")
            .is_none(),
        "no shopping list without an owned chart"
    );
    let e = s
        .execute(
            "digitize.autoPunch",
            Some(&json!({"path": "sample.png", "width": 60, "threads": "all"})),
        )
        .expect_err("PE-DESIGN mode takes no thread source");
    assert!(e.to_string().contains("threads"), "{e}");
    let e = s
        .execute(
            "digitize.photoStitch",
            Some(&with(json!({"threads": "Nope"}))),
        )
        .expect_err("an unknown chart");
    assert!(e.to_string().contains("Nope"), "{e}");
}

/// `isolateSubject` (#127): the graph cut leaves out a background of any colour; `subjectRect`
/// is checked; PE-DESIGN-compatible mode ignores the option.
#[test]
fn auto_punch_isolates_the_subject() {
    let dir = temp_dir();
    // A red block with a yellow stripe on a sky blue the white fabric does not show.
    let img = Raster::from_fn(80, 60, |x, y| {
        if (20..60).contains(&x) && (12..48).contains(&y) {
            if (27..33).contains(&y) {
                [250, 200, 20, 255]
            } else {
                [220, 20, 30, 255]
            }
        } else {
            [150, 200, 240, 255]
        }
    })
    .expect("size");
    std::fs::write(dir.join("subject.png"), img.encode_png().expect("png")).expect("write");
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    let base = json!({"path": "subject.png", "width": 80, "colors": 3, "compatMode": "openStitch"});
    let with = |extra: Value| {
        let mut p = base.clone();
        for (k, v) in extra.as_object().expect("object") {
            p[k] = v.clone();
        }
        p
    };
    let plain = run(&mut s, "digitize.autoPunch", base.clone());
    assert!(plain["background"].is_null(), "the sky is sewn: {plain}");
    assert_eq!(plain["colors"].as_array().expect("colors").len(), 3);
    for extra in [
        json!({"isolateSubject": true}),
        json!({"isolateSubject": true, "subjectRect": [0.1, 0.1, 0.9, 0.9]}),
    ] {
        let r = run(&mut s, "digitize.autoPunch", with(extra.clone()));
        assert_eq!(
            r["colors"].as_array().expect("colors").len(),
            2,
            "{extra} {r}"
        );
        assert!(r["background"].is_string(), "{extra} {r}");
    }
    // A background stroke over the yellow stripe leaves only the red; a subject stroke over a
    // patch of sky sews it.
    let r = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"isolateSubject": true,
            "backgroundStrokes": [{"points": [[0.2, 0.5], [0.8, 0.5]], "width": 0.07}]})),
    );
    assert_eq!(r["colors"].as_array().expect("colors").len(), 1, "{r}");
    let r = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"isolateSubject": true,
            "subjectStrokes": [{"points": [[0.1, 0.1]], "width": 0.1}]})),
    );
    assert_eq!(r["colors"].as_array().expect("colors").len(), 3, "{r}");
    // A stroke that also covers red rows teaches the model that red is background too; the cut
    // then finds no subject, and that is an error rather than the whole picture sewn (#355).
    let e = s
        .execute(
            "digitize.autoPunch",
            Some(&with(json!({"isolateSubject": true,
                "backgroundStrokes": [{"points": [[0.2, 0.5], [0.8, 0.5]], "width": 0.12}]}))),
        )
        .expect_err("no subject");
    assert!(e.to_string().contains("found no subject"), "{e}");
    for bad in [
        json!({"isolateSubject": true, "subjectRect": [0.5, 0.1, 0.4, 0.9]}),
        json!({"isolateSubject": true, "subjectRect": [0.1, 0.1, 0.9]}),
        json!({"isolateSubject": true, "subjectRect": [0.1, 0.1, 0.9, 1.5]}),
        json!({"subjectRect": [0.1, 0.1, 0.9, 0.9]}),
        json!({"isolateSubject": "yes"}),
        json!({"subjectStrokes": [{"points": [[0.5, 0.5]], "width": 0.05}]}),
        json!({"isolateSubject": true, "subjectStrokes": [{"points": [[0.5, 1.5]], "width": 0.05}]}),
        json!({"isolateSubject": true, "subjectStrokes": [{"points": [], "width": 0.05}]}),
        json!({"isolateSubject": true, "backgroundStrokes": [{"points": [[0.5, 0.5]], "width": 0.5}]}),
        json!({"isolateSubject": true, "backgroundStrokes": [{"points": [[0.5]], "width": 0.05}]}),
        json!({"isolateSubject": true, "backgroundStrokes": {"points": [[0.5, 0.5]]}}),
    ] {
        assert!(
            s.execute("digitize.autoPunch", Some(&with(bad.clone())))
                .is_err(),
            "{bad}"
        );
    }
    let pe = json!({"path": "subject.png", "width": 80, "colors": 3});
    let a = run(&mut s, "digitize.autoPunch", pe.clone());
    let mut pe_on = pe.clone();
    pe_on["isolateSubject"] = json!(true);
    let b = run(&mut s, "digitize.autoPunch", pe_on);
    assert_eq!(a["colors"], b["colors"]);
    assert_eq!(a["regions"], b["regions"]);
}

/// #298: PE-DESIGN-compatible Photo Stitch 1 travels hidden between a thread's runs (under its
/// own cells and the darker layers sewn after it) instead of trimming: the sunset photo at
/// 60 mm trims 53 times (141 before), as PE-DESIGN sews a colour in one to five paths.
#[test]
fn compatible_photo_stitch_travels_instead_of_trimming() {
    let dir = temp_dir();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/images/p06-sunset.jpg");
    std::fs::copy(src, dir.join("sunset.jpg")).expect("copy");
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    run(
        &mut s,
        "digitize.photoStitch",
        json!({"path": "sunset.jpg", "width": 60}),
    );
    let stats = run(&mut s, "design.info", json!({}))["stitchStats"].clone();
    let trims = stats["trims"].as_u64().expect("trims");
    let stitches = stats["stitches"].as_u64().expect("stitches");
    assert!(trims <= 80, "{trims} trims in {stitches} stitches");
}

/// #489: `dryRun` returns the region plan and adds nothing; `overrides` change one region;
/// `angleRule` needs OpenStitch mode.
#[test]
fn auto_punch_plan_dry_run_and_overrides() {
    let dir = temp_dir();
    write_sample(&dir);
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    let base = json!({"path": "sample.png", "colors": 4, "width": 60});
    let with = |extra: Value| {
        let mut p = base.clone();
        for (k, v) in extra.as_object().expect("object") {
            p[k] = v.clone();
        }
        p
    };
    let r = run(&mut s, "digitize.autoPunch", with(json!({"dryRun": true})));
    assert!(
        r.get("ids").is_none() && s.design().objects.is_empty(),
        "{r}"
    );
    let plan = r["plan"].as_array().expect("plan");
    assert_eq!(plan.len(), 3, "{r}");
    for (i, region) in plan.iter().enumerate() {
        assert_eq!(region["region"], json!(i));
        assert!(region["why"].as_str().is_some_and(|w| !w.is_empty()));
        assert!(
            ["fill", "satin", "line", "skip"].contains(&region["type"].as_str().expect("type"))
        );
    }
    let satin = plan
        .iter()
        .position(|r| r["type"] == "satin")
        .expect("the blue bar is satin");
    assert_eq!(plan[satin]["reason"], "satin_width");

    // Skip the satin bar, and set the first region's fill angle.
    let r = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"overrides": [{"region": satin, "type": "skip"}, {"region": 0, "angle": 30}]})),
    );
    assert_eq!(r["ids"].as_array().expect("ids").len(), 2, "{r}");
    assert_eq!(r["plan"][satin]["type"], "skip");
    assert_eq!(r["plan"][satin]["reason"], "override");
    assert_eq!(r["plan"][0]["angle"], json!(30.0));

    for (extra, needle) in [
        (
            json!({"overrides": [{"region": 99, "type": "skip"}]}),
            "region 99",
        ),
        (
            json!({"overrides": [{"region": 0, "type": "zigzag"}]}),
            "overrides",
        ),
        (json!({"overrides": [{"region": 0}]}), "overrides"),
        (json!({"angleRule": "varyNeighbours"}), "openStitch"),
        (json!({"angleRule": "leastTravel"}), "openStitch"),
        (json!({"angleRule": "shortest"}), "angleRule"),
    ] {
        let e = s
            .execute("digitize.autoPunch", Some(&with(extra.clone())))
            .expect_err("rejected");
        assert!(e.to_string().contains(needle), "{extra}: {e}");
    }
    let r = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"compatMode": "openStitch", "angleRule": "varyNeighbours", "dryRun": true})),
    );
    assert!(!r["plan"].as_array().expect("plan").is_empty());
}

/// #489: `angleRule: leastTravel` sews no more thread than PE-DESIGN's rule, through the command.
#[test]
fn auto_punch_least_travel_sews_no_more_thread() {
    let dir = temp_dir();
    // A U-shaped red region on white: its arms are sections a fill travels between.
    let img = Raster::from_fn(60, 60, |x, y| {
        let inside =
            (6..54).contains(&x) && (6..54).contains(&y) && !((20..40).contains(&x) && y < 40);
        if inside {
            [200, 20, 30, 255]
        } else {
            [255, 255, 255, 255]
        }
    })
    .expect("size");
    std::fs::write(dir.join("u.png"), img.encode_png().expect("png")).expect("write");
    let thread = |rule: &str| {
        let mut s = Session::new(&dir);
        run(&mut s, "design.new", json!({"width": 100, "height": 100}));
        let r = run(
            &mut s,
            "digitize.autoPunch",
            json!({"path": "u.png", "width": 60, "compatMode": "openStitch", "angleRule": rule}),
        );
        let ids = r["ids"].clone();
        run(&mut s, "object.toStitches", json!({"ids": ids}));
        let mut total = 0.0;
        for id in r["ids"].as_array().expect("ids") {
            let pts = run(&mut s, "stitches.points", json!({"id": id}));
            let pts = pts["points"].as_array().expect("points");
            for w in pts.windows(2) {
                if w[1]["command"] == "stitch" {
                    let d = |k: &str| w[1][k].as_f64().expect("x") - w[0][k].as_f64().expect("y");
                    total += d("x").hypot(d("y"));
                }
            }
        }
        (total, r["plan"][0]["angle"].as_f64().expect("angle"))
    };
    let (pe, pe_angle) = thread("pe");
    let (least, least_angle) = thread("leastTravel");
    assert!(least <= pe, "{least} mm against {pe} mm");
    assert_ne!(least_angle, pe_angle, "the U is sewn another way");
}

/// #489: purpose presets fill in options the call leaves unset, need OpenStitch mode, and are
/// listed by `digitize.presets`.
#[test]
fn auto_punch_presets() {
    let dir = temp_dir();
    write_sample(&dir);
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    let list = run(&mut s, "digitize.presets", json!({}));
    let names: Vec<&str> = list
        .as_array()
        .expect("list")
        .iter()
        .map(|p| p["name"].as_str().expect("name"))
        .collect();
    assert_eq!(
        names,
        [
            "logo",
            "patch",
            "lineArt",
            "illustration",
            "photo",
            "pixelArt"
        ]
    );
    let base = json!({"path": "sample.png", "colors": 4, "width": 60, "dryRun": true});
    let with = |extra: Value| {
        let mut p = base.clone();
        for (k, v) in extra.as_object().expect("object") {
            p[k] = v.clone();
        }
        p
    };
    for preset in &names {
        let r = run(
            &mut s,
            "digitize.autoPunch",
            with(json!({"preset": preset})),
        );
        assert!(!r["plan"].as_array().expect("plan").is_empty(), "{preset}");
        // The same as its options given by hand.
        let options = list
            .as_array()
            .expect("list")
            .iter()
            .find(|p| p["name"] == *preset)
            .expect("listed")["options"]
            .clone();
        let mut by_hand = with(options);
        by_hand["compatMode"] = json!("openStitch");
        assert_eq!(r, run(&mut s, "digitize.autoPunch", by_hand), "{preset}");
    }
    // The call's own option wins over the preset's.
    let own = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"preset": "patch", "skipBackground": true})),
    );
    let patch = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"preset": "patch"})),
    );
    assert!(
        own["background"].is_string() && patch["background"].is_null(),
        "{own} {patch}"
    );
    for (extra, needle) in [
        (json!({"preset": "embossed"}), "preset"),
        (
            json!({"preset": "logo", "compatMode": "peDesign"}),
            "openStitch",
        ),
    ] {
        let e = s
            .execute("digitize.autoPunch", Some(&with(extra.clone())))
            .expect_err("rejected");
        assert!(e.to_string().contains(needle), "{extra}: {e}");
    }
    assert!(s.design().objects.is_empty());
}

/// #489: `digitize.exportOutlines` writes the traced regions as SVG; `import.svg` reads back
/// one object per region; writes stay in the sandbox.
#[test]
fn auto_punch_outlines_export_as_svg() {
    let dir = temp_dir();
    write_sample(&dir);
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    let r = run(
        &mut s,
        "digitize.exportOutlines",
        json!({"path": "sample.png", "colors": 4, "width": 60, "out": "outlines.svg"}),
    );
    let svg = std::fs::read_to_string(dir.join("outlines.svg")).expect("written");
    assert!(
        svg.starts_with("<svg") && svg.contains("fill-rule=\"evenodd\""),
        "{svg}"
    );
    assert!(s.design().objects.is_empty(), "the export adds nothing");
    let regions = r["fills"].as_u64().expect("fills") + r["lines"].as_u64().expect("lines");
    let converted = run(
        &mut s,
        "digitize.autoPunch",
        json!({"path": "sample.png", "colors": 4, "width": 60, "dryRun": true}),
    );
    assert_eq!(converted["regions"].as_u64(), Some(regions), "{r}");
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    run(&mut s, "import.svg", json!({"path": "outlines.svg"}));
    let shapes = run(&mut s, "object.list", json!({}))
        .as_array()
        .expect("list")
        .iter()
        .filter(|o| o["kind"] == "shape")
        .count();
    assert_eq!(shapes as u64, regions);
    // A sandboxed session writes only inside its folder.
    let outside = temp_dir();
    let mut sandboxed = Session::sandboxed(&dir);
    let escape = outside.join("x.svg").display().to_string();
    assert!(
        sandboxed
            .execute(
                "digitize.exportOutlines",
                Some(&json!({"path": "sample.png", "out": escape})),
            )
            .is_err()
    );
    assert!(!outside.join("x.svg").exists());
}
