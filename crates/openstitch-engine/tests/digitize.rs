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

/// #298: PE-compatible Photo Stitch 1 sews each colour in about one path, travelling in sight
/// to the nearest run when no hidden way leads on, as PE-DESIGN 11.50 does (round-8 capture of
/// the sunset at 90 mm: 11 trims in 8,683 stitches, 1.3 per 1,000). OpenStitch: 108 trims
/// before, 14 now.
#[test]
fn compatible_photo_stitch_trims_about_as_rarely_as_pe_design() {
    let dir = temp_dir();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/images/p06-sunset.jpg");
    std::fs::copy(src, dir.join("sunset.jpg")).expect("copy");
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    run(
        &mut s,
        "digitize.photoStitch",
        json!({"path": "sunset.jpg", "width": 90, "compatMode": "peDesign"}),
    );
    let stats = run(&mut s, "design.info", json!({}))["stitchStats"].clone();
    let trims = stats["trims"].as_f64().expect("trims");
    let stitches = stats["stitches"].as_f64().expect("stitches");
    assert!(
        trims * 1000.0 / stitches <= 2.0,
        "{trims} trims in {stitches} stitches"
    );
}

/// #713: line art's drawn dots stay, without knots, and a colour's small pieces are joined by
/// short running connectors: the mandala sews at most 6 stitches in place and at most 55 trims
/// per 1,000 stitches (77 before), with no preset and each preset but photo (whose satin tips
/// are #732).
#[test]
fn auto_punch_line_art_keeps_its_dots_without_knots_or_trims() {
    let dir = temp_dir();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/images/g11-mandala.png");
    std::fs::copy(src, dir.join("g11.png")).expect("copy");
    for preset in [
        None,
        Some("logo"),
        Some("lineArt"),
        Some("illustration"),
        Some("pixelArt"),
    ] {
        let mut s = Session::new(&dir);
        run(&mut s, "design.new", json!({"width": 100, "height": 100}));
        let mut params =
            json!({"path": "g11.png", "width": 100, "x": 0, "y": 0, "compatMode": "openStitch"});
        if let Some(p) = preset {
            params["preset"] = json!(p);
        }
        let r = run(&mut s, "digitize.autoPunch", params);
        let plan = r["plan"].as_array().expect("plan");
        assert!(
            plan.iter().any(|e| e["reason"] == "dot"),
            "{preset:?}: the drawn dots are kept"
        );
        let p = openstitch_design::Compiler::for_design(s.design()).compile(s.design());
        let (mut run_len, mut most) = (0, 0);
        for w in p.stitches.windows(2) {
            let d = ((w[1].x - w[0].x).powi(2) + (w[1].y - w[0].y).powi(2)).sqrt();
            run_len = if w[1].command == openstitch_core::Command::Stitch && d < 3.0 {
                run_len + 1
            } else {
                0
            };
            most = most.max(run_len);
        }
        assert!(most <= 6, "{preset:?}: {most} stitches in place");
        let trims = p.count(openstitch_core::Command::Trim) as f64;
        let stitches = p.count(openstitch_core::Command::Stitch) as f64;
        assert!(
            trims * 1000.0 / stitches <= 55.0,
            "{preset:?}: {trims} trims in {stitches} stitches"
        );
    }
}

/// The tie-off drops tiny last needle points (#703); the traced object spans must stay valid
/// when an object before them sewed nothing: exporting the mandala's line art panicked in the
/// export checks ("slice index starts at 2982 but ends at 2981").
#[test]
fn auto_punch_line_art_exports_with_valid_spans_in_every_mode_and_preset() {
    let dir = temp_dir();
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../examples/images/g11-mandala.png");
    std::fs::copy(src, dir.join("g11.png")).expect("copy");
    // PE-compatible mode takes no preset; OpenStitch mode with none and with each (#728).
    let mut runs = vec![
        json!({"compatMode": "peDesign"}),
        json!({"compatMode": "openStitch"}),
    ];
    for preset in [
        "logo",
        "patch",
        "lineArt",
        "illustration",
        "photo",
        "pixelArt",
    ] {
        runs.push(json!({"compatMode": "openStitch", "preset": preset}));
    }
    for extra in runs {
        let mut s = Session::new(&dir);
        run(&mut s, "design.new", json!({"width": 100, "height": 100}));
        let mut params = json!({"path": "g11.png", "width": 100, "x": 0, "y": 0});
        for (k, v) in extra.as_object().expect("object") {
            params[k] = v.clone();
        }
        run(&mut s, "digitize.autoPunch", params);
        run(&mut s, "design.export", json!({"path": "g11.pes"}));
        let (p, spans) =
            openstitch_design::Compiler::for_design(s.design()).compile_traced(s.design());
        let mut end = 0;
        for (id, r) in &spans {
            assert!(
                r.start <= r.end && r.start >= end,
                "{extra}: object {id}: {r:?} after {end}"
            );
            end = r.end;
        }
        assert!(end <= p.stitches.len(), "{extra}");
    }
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
        (json!({"underlap": 0.8}), "openStitch"),
        (
            json!({"compatMode": "openStitch", "underlap": 2.0}),
            "underlap",
        ),
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

/// #490: the no-model picture helpers through their commands: a click's subject mask
/// (`digitize.segment`, Auto Punch's `subjectPoint`) and a line drawing from a photo
/// (`lines: edges`).
#[test]
fn picture_helpers_cut_out_and_draw_lines() {
    let dir = temp_dir();
    // A red disc on mottled green.
    let img = Raster::from_fn(90, 60, |x, y| {
        if (f64::from(x) - 45.0).hypot(f64::from(y) - 30.0) < 15.0 {
            [210, 30, 40, 255]
        } else {
            [
                50 + ((x * 13 + y * 7) % 30) as u8,
                150 + ((x * 5 + y * 11) % 40) as u8,
                50,
                255,
            ]
        }
    })
    .expect("size");
    std::fs::write(dir.join("disc.png"), img.encode_png().expect("png")).expect("write");
    let mut s = Session::sandboxed(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    let r = run(
        &mut s,
        "digitize.segment",
        json!({"path": "disc.png", "point": [0.5, 0.5], "out": "mask.png"}),
    );
    let b = r["box"].as_array().expect("box");
    let f = |i: usize| b[i].as_f64().expect("number");
    assert!(
        f(0) > 0.25 && f(2) < 0.75 && f(1) > 0.15 && f(3) < 0.85,
        "{r}"
    );
    let mask = Raster::decode(&std::fs::read(dir.join("mask.png")).expect("written")).expect("png");
    assert_eq!(
        mask.pixel(mask.width() / 2, mask.height() / 2),
        [0, 0, 0, 255]
    );
    assert_eq!(mask.pixel(1, 1), [255, 255, 255, 255]);
    // Auto Punch with the click sews the disc and leaves the green out.
    let ap = run(
        &mut s,
        "digitize.autoPunch",
        json!({"path": "disc.png", "width": 60, "compatMode": "openStitch", "isolateSubject": true, "subjectPoint": [0.5, 0.5], "colors": 2}),
    );
    // The disc, and at most a thin fringe of the grass's colour round its edge.
    let plan = ap["plan"].as_array().expect("plan");
    let area = |red: bool| -> f64 {
        plan.iter()
            .filter(|r| (r["color"] == "#ed171f") == red)
            .map(|r| r["areaMm2"].as_f64().expect("area"))
            .sum()
    };
    assert!(
        area(true) > 300.0 && area(false) < 0.05 * area(true),
        "{ap}"
    );
    // The edges as a line image: a ring round the disc.
    let li = run(
        &mut s,
        "digitize.lineImage",
        json!({"path": "disc.png", "lines": "edges", "edgeStrength": 30}),
    );
    assert!(li["linePixels"].as_u64().expect("count") > 50, "{li}");
    let dc = run(
        &mut s,
        "digitize.designCenter",
        json!({"path": "disc.png", "lines": "edges", "width": 60, "regionSew": "none"}),
    );
    assert!(!dc["ids"].as_array().expect("ids").is_empty(), "{dc}");
    let base = json!({"path": "disc.png", "width": 60, "compatMode": "openStitch"});
    for (command, extra, needle) in [
        (
            "digitize.autoPunch",
            json!({"subjectPoint": [0.5, 0.5]}),
            "isolateSubject",
        ),
        (
            "digitize.autoPunch",
            json!({"isolateSubject": true, "subjectPoint": [0.5, 0.5], "subjectRect": [0, 0, 1, 1]}),
            "subjectRect",
        ),
        (
            "digitize.autoPunch",
            json!({"isolateSubject": true, "subjectPoint": [1.5, 0.5]}),
            "subjectPoint",
        ),
        ("digitize.segment", json!({"point": [0.5]}), "point"),
        (
            "digitize.lineImage",
            json!({"edgeStrength": 30}),
            "lines: edges",
        ),
        (
            "digitize.lineImage",
            json!({"lines": "edges", "threshold": 100}),
            "edgeStrength",
        ),
        (
            "digitize.segment",
            json!({"point": [0.5, 0.5], "out": "../m.png"}),
            "",
        ),
    ] {
        let mut p = base.clone();
        for (k, v) in extra.as_object().expect("object") {
            p[k] = v.clone();
        }
        let e = s.execute(command, Some(&p)).expect_err("rejected");
        assert!(e.to_string().contains(needle), "{command} {extra}: {e}");
    }
}

/// #489: `order` sews the listed colours first and renumbers the regions; `regionMap` gives
/// the region under each cell.
#[test]
fn auto_punch_colour_order_and_region_map() {
    let dir = temp_dir();
    write_sample(&dir);
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    let base =
        json!({"path": "sample.png", "colors": 4, "width": 60, "dryRun": true, "regionMap": true});
    let r = run(&mut s, "digitize.autoPunch", base.clone());
    let colors: Vec<String> = r["colors"]
        .as_array()
        .expect("colors")
        .iter()
        .map(|c| c.as_str().expect("hex").to_string())
        .collect();
    assert_eq!(colors.len(), 3);
    let map = &r["regionMap"];
    let (w, h) = (
        map["width"].as_u64().expect("w"),
        map["height"].as_u64().expect("h"),
    );
    let cells: u64 = map["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .map(|run| run[1].as_u64().expect("count"))
        .sum();
    assert_eq!(cells, w * h);
    // Every region of the plan is somewhere on the map, and -1 marks the background.
    let mut seen: Vec<i64> = map["runs"]
        .as_array()
        .expect("runs")
        .iter()
        .map(|run| run[0].as_i64().expect("region"))
        .collect();
    seen.sort_unstable();
    seen.dedup();
    assert_eq!(seen, [-1, 0, 1, 2]);
    // The last colour first.
    let mut p = base.clone();
    p["order"] = json!([colors[2]]);
    let reordered = run(&mut s, "digitize.autoPunch", p);
    assert_eq!(reordered["colors"][0], json!(colors[2]));
    assert_eq!(reordered["plan"][0]["color"], json!(colors[2]));
    // A colour this run does not sew is passed over and reported.
    let mut p = base.clone();
    p["order"] = json!(["#123456", colors[2]]);
    let r = run(&mut s, "digitize.autoPunch", p);
    assert_eq!(r["orderIgnored"], json!(["#123456"]));
    assert_eq!(r["colors"][0], json!(colors[2]));
    for (order, needle) in [
        (json!([colors[0], colors[0]]), "twice"),
        (json!(["red"]), "order"),
    ] {
        let mut p = base.clone();
        p["order"] = order.clone();
        let e = s
            .execute("digitize.autoPunch", Some(&p))
            .expect_err("rejected");
        assert!(e.to_string().contains(needle), "{order}: {e}");
    }
}

/// #690 through the command: OpenStitch mode's dry run lists a speck (a region whose stitches
/// would all fall within 0.8 mm) as left out with the reason `speck`; an override sews it.
#[test]
fn auto_punch_specks_are_planned_and_can_be_sewn() {
    let dir = temp_dir();
    // 60 × 40 mm: red and blue halves (not line art) and a 0.5 mm yellow dot in the blue.
    let img = Raster::from_fn(600, 400, |x, y| {
        if (450..455).contains(&x) && (200..205).contains(&y) {
            [250, 220, 20, 255]
        } else if x < 300 {
            [220, 20, 30, 255]
        } else {
            [20, 40, 200, 255]
        }
    })
    .expect("size");
    std::fs::write(dir.join("speck.png"), img.encode_png().expect("png")).expect("write");
    let mut s = Session::new(&dir);
    run(&mut s, "design.new", json!({"width": 100, "height": 100}));
    let base = json!({
        "path": "speck.png", "width": 60, "x": 0, "y": 0, "colors": 3,
        "compatMode": "openStitch", "minArea": 0, "resolution": 600,
    });
    let with = |extra: Value| {
        let mut p = base.clone();
        for (k, v) in extra.as_object().expect("object") {
            p[k] = v.clone();
        }
        p
    };
    let r = run(&mut s, "digitize.autoPunch", with(json!({"dryRun": true})));
    let plan = r["plan"].as_array().expect("plan");
    let speck = plan
        .iter()
        .find(|p| p["reason"] == "speck")
        .unwrap_or_else(|| panic!("a speck in {r}"));
    assert_eq!(speck["type"], "skip");
    assert!(speck["why"].as_str().is_some_and(|w| w.contains("0.8 mm")));
    let id = speck["region"].clone();
    let sewn_before = r["regions"].as_u64().expect("regions");
    assert!(s.design().objects.is_empty(), "a dry run adds nothing");
    // The override sews it: one more object, the region now a fill.
    let r = run(
        &mut s,
        "digitize.autoPunch",
        with(json!({"overrides": [{"region": id, "type": "fill"}]})),
    );
    assert_eq!(r["regions"].as_u64(), Some(sewn_before + 1), "{r}");
    let region = &r["plan"][id.as_u64().expect("id") as usize];
    assert_eq!(region["type"], "fill");
    assert_eq!(region["reason"], "override");
    assert_eq!(s.design().objects.len() as u64, sewn_before + 1);
}
