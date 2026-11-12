//! Satin shaping and under sewing beyond PE-DESIGN through the commands (#520-#523): corner
//! styles, short stitches, compensation per side and per cent, automatic and second under
//! sewing on Manual Punch columns and zigzag lines; set, read back, undone, saved.

use openstitch_core::Command;
use openstitch_design::column::{CornerStyle, SecondUnderlay};
use openstitch_design::{Compiler, UnderlayStyle};
use openstitch_engine::{Session, project};
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn stitches(s: &Session) -> usize {
    Compiler::default()
        .compile(s.design())
        .count(Command::Stitch)
}

/// A 6 mm Manual Punch column with a 60 degree corner at (30, 63), its corner line on the
/// bisector.
fn vee_column(s: &mut Session) {
    run(
        s,
        "shape.column",
        json!({"lines": [
            [[0, 66], [0, 60]],
            [[24.8, 66], [35.2, 60]],
            [[12.4, 87.5], [17.6, 90.5]]
        ], "fill": "Red"}),
    );
}

#[test]
fn column_shaping_is_set_read_undone_and_saved() {
    let mut s = Session::new(std::env::temp_dir());
    vee_column(&mut s);
    let before = Compiler::default().compile(s.design());
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "cornerStyle": "mitre", "mitreBelow": 80, "cornerOverlap": 0.4,
               "shortStitchDistance": 0.25, "shortStitchInset": 20,
               "pullLeft": 0.2, "pullPercent": 3, "pullMax": 0.5,
               "underlayStyle": "auto", "underlayAuto": {"centreBelow": 1.5, "edgeBelow": 3},
               "underlay2": {"style": "centreWalk", "inset": 0.8}}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    let sh = r.satin_shaping;
    assert_eq!(sh.corners.style, CornerStyle::Mitre);
    assert_eq!((sh.corners.mitre_below, sh.corners.overlap), (80.0, 0.4));
    assert_eq!(
        (sh.short_stitches.distance, sh.short_stitches.inset),
        (0.25, 20.0)
    );
    assert_eq!(sh.compensation.left, Some(0.2));
    assert_eq!(sh.compensation.right, None);
    assert_eq!(
        (sh.compensation.percent, sh.compensation.max),
        (3.0, Some(0.5))
    );
    assert_eq!(r.underlay_style, UnderlayStyle::Auto);
    assert_eq!(
        (r.underlay_auto.centre_below, r.underlay_auto.edge_below),
        (1.5, 3.0)
    );
    assert_eq!(
        r.underlay2,
        Some(SecondUnderlay {
            style: UnderlayStyle::CentreWalk,
            inset: 0.8,
            ..SecondUnderlay::default()
        })
    );
    let after = Compiler::default().compile(s.design());
    assert_ne!(
        after.stitches, before.stitches,
        "the column sews differently"
    );
    // object.get reads every option back.
    let got = run(&mut s, "object.get", json!({"id": 1}));
    let region = &got["region"];
    assert_eq!(region["satinShaping"]["corners"]["style"], "mitre", "{got}");
    assert_eq!(region["satinShaping"]["shortStitches"]["distance"], 0.25);
    assert_eq!(region["underlay2"]["style"], "centreWalk");
    assert_eq!(region["underlayStyle"], "auto");
    // Saved and reopened unchanged.
    let back = project::decode(&project::encode(s.design()).expect("encode")).expect("decode");
    assert_eq!(&back, s.design());
    // One undo step restores PE-DESIGN's column, stitch for stitch.
    run(&mut s, "history.undo", json!({}));
    assert_eq!(
        Compiler::default().compile(s.design()).stitches,
        before.stitches
    );
    // null removes the per-side values and the second layer.
    run(&mut s, "history.redo", json!({}));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "pullLeft": null, "pullMax": null, "underlay2": null, "cornerStyle": "turn"}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.satin_shaping.compensation.left, None);
    assert_eq!(r.underlay2, None);
    assert_eq!(r.satin_shaping.corners.style, CornerStyle::Turn);
}

#[test]
fn zigzag_line_shaping_through_set_line() {
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.path",
        json!({"points": [[0, 0], [30, 0], [4, 15]], "closed": false, "outline": "Black"}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "zigzag", "width": 4}),
    );
    let plain = stitches(&s);
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "cornerStyle": "auto", "shortStitchDistance": 0.25,
               "pullRight": 0.3, "underlayStyle": "auto"}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert_eq!(l.satin_shaping.corners.style, CornerStyle::Auto);
    assert_eq!(l.satin_shaping.compensation.right, Some(0.3));
    assert_eq!(l.underlay_style, UnderlayStyle::Auto);
    let p = Compiler::default().compile(s.design());
    // The 30 degree corner sews with no stitch over 8 mm.
    let pts: Vec<(f64, f64)> = p
        .stitches
        .iter()
        .filter(|x| x.command == Command::Stitch)
        .map(|x| (x.x, x.y))
        .collect();
    let longest = pts
        .windows(2)
        .map(|w| ((w[0].0 - w[1].0).powi(2) + (w[0].1 - w[1].1).powi(2)).sqrt())
        .fold(0.0, f64::max);
    assert!(longest <= 80.0, "{longest}");
    assert_ne!(p.count(Command::Stitch), plain);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(stitches(&s), plain);
}

#[test]
fn bad_shaping_values_are_refused() {
    let mut s = Session::new(std::env::temp_dir());
    vee_column(&mut s);
    for bad in [
        json!({"id": 1, "cornerStyle": "round"}),
        json!({"id": 1, "mitreBelow": 200}),
        json!({"id": 1, "shortStitchDistance": 2}),
        json!({"id": 1, "pullPercent": 50}),
        json!({"id": 1, "underlayAuto": {"centreBelow": 5, "edgeBelow": 3}}),
        json!({"id": 1, "underlay2": {"style": "cross"}}),
        json!({"id": 1, "underlay2": {"inset": 9}}),
    ] {
        assert!(s.execute("object.setRegion", Some(&bad)).is_err(), "{bad}");
    }
    // The fabric selector gives automatic under sewing its thresholds.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "underlayStyle": "auto"}),
    );
    run(
        &mut s,
        "fabric.apply",
        json!({"fabric": "knit", "ids": [1]}),
    );
    let r = s.design().objects[0].region.clone().expect("region");
    assert_eq!(r.underlay_style, UnderlayStyle::Auto);
    assert_eq!(r.underlay_auto.centre_below, 1.5);
    let list = run(&mut s, "fabric.list", json!({}));
    assert!(list.to_string().contains("autoUnderlay"), "{list}");
}

#[test]
fn designs_without_shaping_save_byte_identically() {
    // Columns, satin and fill regions and zigzag lines at their defaults write none of the
    // new keys, so an .osd saved before #520-#523 decodes and saves again byte for byte.
    let mut s = Session::new(std::env::temp_dir());
    vee_column(&mut s);
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 60, "cy": 30, "rx": 15, "ry": 8, "fill": "Blue", "outline": "Black"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 2, "type": "satin"}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 2, "type": "zigzag", "width": 3}),
    );
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 50, "y": 60, "width": 30, "height": 20, "fill": "#00a000"}),
    );
    let saved = project::encode(s.design()).expect("encode");
    let text = String::from_utf8(saved.clone()).expect("utf8");
    for key in [
        "satinShaping",
        "underlayAuto",
        "underlay2",
        "widthKeys",
        "taperStart",
        "taperEnd",
        "slant",
    ] {
        assert!(!text.contains(key), "{key} written at its default");
    }
    // Lines keep PE-DESIGN's under sewing, so they write no underlayStyle.
    let line =
        serde_json::to_value(s.design().objects[1].line.as_ref().expect("line")).expect("json");
    assert!(line.get("underlayStyle").is_none(), "{line}");
    let again = project::encode(&project::decode(&saved).expect("decode")).expect("encode");
    assert_eq!(again, saved, "decode and save again changes no byte");
    // With options set, they survive the round trip.
    run(
        &mut s,
        "object.setLine",
        json!({"id": 2, "cornerStyle": "cap", "underlayStyle": "centreWalk"}),
    );
    let shaped = project::encode(s.design()).expect("encode");
    assert_eq!(&project::decode(&shaped).expect("decode"), s.design());
}

#[test]
fn satin_styles_jagged_edge_spacing_and_raised_through_the_commands() {
    use openstitch_design::column::{JaggedMode, JaggedSide, SatinStyle};
    let mut s = Session::new(std::env::temp_dir());
    vee_column(&mut s);
    let plain = stitches(&s);
    for (style, more) in [("double", true), ("sStitch", true), ("open", false)] {
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "satinStyle": style}),
        );
        let n = stitches(&s);
        assert_eq!(n > plain, more, "{style}: {n} vs {plain}");
        run(&mut s, "history.undo", json!({}));
    }
    assert_eq!(stitches(&s), plain);
    let r = run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "satinStyle": "double", "spacingAt": 0.5,
               "jaggedEdge": {"mode": "out", "side": "right", "amount": 0.8, "seed": 3},
               "raised": {"laneSpacing": 2}}),
    );
    assert_eq!(r, Value::Null, "a 6 mm column needs no hint");
    let sh = s.design().objects[0]
        .region
        .as_ref()
        .expect("r")
        .satin_shaping;
    assert_eq!(sh.style, SatinStyle::Double);
    assert_eq!(sh.spacing_at, Some(0.5));
    let j = sh.jagged.expect("jagged");
    assert_eq!(
        (j.mode, j.side, j.amount, j.seed),
        (JaggedMode::Out, JaggedSide::Right, 0.8, 3)
    );
    assert_eq!(sh.raised.expect("raised").lane_spacing, 2.0);
    let got = run(&mut s, "object.get", json!({"id": 1}));
    assert_eq!(
        got["region"]["satinShaping"]["jagged"]["mode"], "out",
        "{got}"
    );
    // null removes; true keeps raised on with its settings; spacingAt 1 is PE-DESIGN's
    // (unset).
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "jaggedEdge": null, "raised": true, "spacingAt": 1, "satinStyle": "normal"}),
    );
    let sh = s.design().objects[0]
        .region
        .as_ref()
        .expect("r")
        .satin_shaping;
    assert!(sh.jagged.is_none() && sh.spacing_at.is_none());
    assert_eq!(sh.raised.expect("raised").lane_spacing, 2.0);
    run(&mut s, "object.setRegion", json!({"id": 1, "raised": null}));
    run(&mut s, "object.setRegion", json!({"id": 1, "raised": true}));
    let sh = s.design().objects[0]
        .region
        .as_ref()
        .expect("r")
        .satin_shaping;
    assert_eq!(sh.raised, Some(Default::default()));
    // The inspector's shorthand: a mode name, or none.
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "jaggedEdge": "both"}),
    );
    let j = s.design().objects[0]
        .region
        .as_ref()
        .expect("r")
        .satin_shaping
        .jagged;
    assert_eq!(j.map(|j| j.mode), Some(JaggedMode::Both));
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "jaggedEdge": "none"}),
    );
    assert!(
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .satin_shaping
            .jagged
            .is_none()
    );
    for bad in [
        json!({"id": 1, "jaggedEdge": "zigzag"}),
        json!({"id": 1, "satinStyle": "wavy"}),
        json!({"id": 1, "spacingAt": 2}),
        json!({"id": 1, "jaggedEdge": {"mode": "sideways"}}),
        json!({"id": 1, "jaggedEdge": {"amount": 9}}),
        json!({"id": 1, "raised": {"inset": 5}}),
        json!({"id": 1, "raised": 3}),
    ] {
        assert!(s.execute("object.setRegion", Some(&bad)).is_err(), "{bad}");
    }
    // A raised narrow zigzag line gets a hint, not an error.
    run(
        &mut s,
        "shape.path",
        json!({"points": [[60, 10], [90, 10]], "closed": false, "outline": "Black"}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 2, "type": "zigzag", "width": 2}),
    );
    let r = run(&mut s, "object.setLine", json!({"id": 2, "raised": true}));
    assert!(
        r["hints"][0].as_str().expect("hint").contains("3 mm"),
        "{r}"
    );
}

#[test]
fn rungs_are_added_moved_deleted_swapped_and_untangled() {
    let mut s = Session::new(std::env::temp_dir());
    // Mixed click order: the middle line clicked right-then-left is turned round.
    run(
        &mut s,
        "shape.column",
        json!({"lines": [[[0, 0], [0, 6]], [[20, 6], [20, 0]], [[40, 0], [40, 6]]]}),
    );
    let lines = |s: &Session| {
        s.design().objects[0]
            .region
            .as_ref()
            .expect("r")
            .direction_lines
            .clone()
    };
    assert_eq!(lines(&s)[1][0].y, 0.0, "untangled on finish");
    let before = stitches(&s);
    let r = run(&mut s, "object.insertRung", json!({"id": 1, "at": [10, 3]}));
    assert_eq!(r["index"], 1);
    assert_eq!(lines(&s).len(), 4);
    assert_eq!(lines(&s)[1][0].x, 100.0);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(lines(&s).len(), 3);
    run(
        &mut s,
        "object.moveRung",
        json!({"id": 1, "index": 1, "side": "right", "point": [22, 8]}),
    );
    assert_eq!(lines(&s)[1][1].y, 80.0);
    // The outline follows the lines.
    assert!(
        s.design().objects[0].contours[0]
            .points
            .iter()
            .any(|q| q.y == 80.0)
    );
    assert!(stitches(&s) != before);
    run(&mut s, "object.swapSides", json!({"id": 1}));
    assert_eq!(lines(&s)[0][0].y, 60.0);
    run(&mut s, "object.swapSides", json!({"id": 1}));
    run(&mut s, "object.deleteRung", json!({"id": 1, "index": 1}));
    assert_eq!(lines(&s).len(), 2);
    assert!(
        s.execute("object.deleteRung", Some(&json!({"id": 1, "index": 0})))
            .is_err()
    );
    assert!(
        s.execute("object.deleteRung", Some(&json!({"id": 1, "index": 9})))
            .is_err()
    );
    // Moving an end so the pair twists is refused; untangle mends a twisted column.
    run(&mut s, "object.insertRung", json!({"id": 1, "at": [20, 3]}));
    let twisted = s.execute(
        "object.moveRung",
        Some(&json!({"id": 1, "index": 1, "side": "left", "point": [20, 9]})),
    );
    assert!(twisted.is_err());
    let r = run(&mut s, "object.untangle", json!({"id": 1}));
    assert_eq!(r["turned"], 0);
    // Not a column.
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 50, "y": 50, "width": 10, "height": 10}),
    );
    assert!(
        s.execute("object.swapSides", Some(&json!({"id": 2})))
            .is_err()
    );
}

#[test]
fn satin_lines_with_width_keys_tapers_and_slant() {
    use openstitch_design::skeleton::{self, Skeleton, WidthKey};
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.path",
        json!({"points": [[10, 10], [50, 10], [70, 30]], "closed": false, "outline": "Black"}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "satin", "width": 3, "underSewing": false}),
    );
    // The command's own list offers satin, and its help names the satin line's parameters.
    let commands = s.commands();
    let spec = commands
        .iter()
        .find(|c| c.id == "object.setLine")
        .expect("setLine");
    let ty = spec.params.iter().find(|d| d.name == "type").expect("type");
    assert!(ty.kind.split('|').any(|k| k == "satin"), "{}", ty.kind);
    for name in ["widthKeys", "taperStart", "taperEnd", "slant"] {
        assert!(ty.summary.contains(name), "{name}");
        assert!(
            spec.params.iter().any(|d| d.name == name),
            "{name} documented"
        );
    }
    // One width all along: the skeleton satin of that width.
    let path = s.design().objects[0].contours[0].path();
    let plain = Compiler::default().compile(s.design());
    let pts: Vec<_> = plain
        .stitches
        .iter()
        .filter(|x| x.command == Command::Stitch)
        .map(|x| openstitch_design::v2(x.x, x.y))
        .collect();
    let sk = skeleton::satin(
        &Skeleton {
            points: path.clone(),
            widths: vec![3.0; path.len()],
        },
        10.0 / 4.5,
        0.0,
    );
    // The same needle points (rounded to the 0.1 mm grid), plus the tie-in and tie-off.
    assert!(
        pts.len() >= sk.len() && pts.len() <= sk.len() + 8,
        "{} {}",
        pts.len(),
        sk.len()
    );
    for b in &sk {
        assert!(pts.iter().any(|a| a.distance(*b) <= 0.71), "{b:?}");
    }
    // Keys, tapers and a slant.
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "widthKeys": [[0, 0.5, 0.5], {"at": 0.6, "left": 0, "right": 3}],
               "taperStart": 4, "taperEnd": 6, "slant": 20}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert_eq!(
        l.width_keys[1],
        WidthKey {
            at: 0.6,
            left: 0.0,
            right: 3.0
        }
    );
    assert_eq!(
        (l.taper_start, l.taper_end, l.slant),
        (Some(4.0), Some(6.0), Some(20.0))
    );
    assert_ne!(stitches(&s), plain.count(Command::Stitch));
    let got = run(&mut s, "object.get", json!({"id": 1}));
    assert_eq!(got["line"]["widthKeys"][1]["right"], 3.0, "{got}");
    // Width keys one at a time: a key near an existing one replaces it.
    let r = run(
        &mut s,
        "object.setWidthKey",
        json!({"id": 1, "at": 0.3, "width": 4}),
    );
    assert_eq!(r["index"], 1);
    run(
        &mut s,
        "object.setWidthKey",
        json!({"id": 1, "at": 0.302, "left": 1}),
    );
    let keys = &s.design().objects[0].line.as_ref().expect("l").width_keys;
    assert_eq!(keys.len(), 3);
    assert_eq!((keys[1].left, keys[1].right), (1.0, 2.0));
    run(
        &mut s,
        "object.deleteWidthKey",
        json!({"id": 1, "index": 1}),
    );
    assert_eq!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("l")
            .width_keys
            .len(),
        2
    );
    run(&mut s, "history.undo", json!({}));
    assert_eq!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("l")
            .width_keys
            .len(),
        3
    );
    // null clears; 0 removes a taper or slant.
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "widthKeys": null, "taperStart": 0, "slant": 0}),
    );
    let l = s.design().objects[0].line.clone().expect("line");
    assert!(l.width_keys.is_empty() && l.taper_start.is_none() && l.slant.is_none());
    let back = project::decode(&project::encode(s.design()).expect("encode")).expect("decode");
    assert_eq!(&back, s.design());
    for (cmd, bad) in [
        ("object.setLine", json!({"id": 1, "widthKeys": [[2, 1, 1]]})),
        (
            "object.setLine",
            json!({"id": 1, "widthKeys": [[0, 20, 1]]}),
        ),
        ("object.setLine", json!({"id": 1, "slant": 80})),
        ("object.setWidthKey", json!({"id": 1, "at": 0.5})),
        (
            "object.setWidthKey",
            json!({"id": 1, "at": 0.5, "width": 3, "left": 1}),
        ),
        ("object.deleteWidthKey", json!({"id": 1, "index": 0})),
    ] {
        assert!(s.execute(cmd, Some(&bad)).is_err(), "{cmd} {bad}");
    }
}

#[test]
fn a_branching_shape_becomes_one_satin_column_per_arm() {
    let mut s = Session::new(std::env::temp_dir());
    let star: Vec<[f64; 2]> = (0..10)
        .map(|i| {
            let a = std::f64::consts::PI * i as f64 / 5.0 - std::f64::consts::FRAC_PI_2;
            let r = if i % 2 == 0 { 25.0 } else { 4.0 };
            [50.0 + r * a.cos(), 50.0 + r * a.sin()]
        })
        .collect();
    run(
        &mut s,
        "shape.polygon",
        json!({"points": star, "fill": "#e0a000", "name": "star"}),
    );
    let before = s.design().clone();
    let r = run(
        &mut s,
        "object.deriveSkeleton",
        json!({"id": 1, "branches": true, "joinOverlap": 0.4}),
    );
    let ids = r["ids"].as_array().expect("ids");
    assert_eq!(ids.len(), 4, "{r}");
    let d = s.design();
    assert_eq!(d.objects.len(), 5);
    assert!(!d.objects[0].visible, "the shape is kept, hidden");
    let group = r["group"].as_u64().expect("group");
    for o in &d.objects[1..] {
        assert_eq!(o.group, Some(group));
        let region = o.region.as_ref().expect("region");
        assert!(region.skeleton.is_some());
        assert_eq!(
            region.thread,
            before.objects[0].region.as_ref().expect("r").thread
        );
    }
    assert!(stitches(&s) > 200);
    assert_eq!(r["warnings"], json!([]));
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before);
    // A star with a broad body warns about the wide column through its middle.
    run(&mut s, "object.scale", json!({"id": 1, "sx": 3, "sy": 3}));
    let r = run(
        &mut s,
        "object.deriveSkeleton",
        json!({"id": 1, "branches": true}),
    );
    assert!(
        r["warnings"][0].as_str().expect("w").contains("10 mm"),
        "{r}"
    );
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    // Not kept: the shape goes.
    run(
        &mut s,
        "object.deriveSkeleton",
        json!({"id": 1, "branches": true, "keepOriginal": false}),
    );
    assert_eq!(s.design().objects.len(), 4);
    run(&mut s, "history.undo", json!({}));
    // A bar has no branches: one skeleton on the shape itself, as without branches.
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 80, "width": 40, "height": 4, "fill": "Red"}),
    );
    let r = run(
        &mut s,
        "object.deriveSkeleton",
        json!({"id": 2, "branches": true}),
    );
    assert!(r.get("points").is_some(), "{r}");
    assert!(
        s.design().objects[1]
            .region
            .as_ref()
            .expect("r")
            .skeleton
            .is_some()
    );
    assert!(
        s.execute(
            "object.deriveSkeleton",
            Some(&json!({"id": 1, "branches": true, "joinOverlap": 5}))
        )
        .is_err()
    );
}

#[test]
fn branch_columns_on_the_outline_are_sewn_to_it_and_into_its_corners() {
    // #526: K at 25 mm split into columns. A column end on the outline (the stem's ends, the
    // legs' feet, the arm's top) is flagged in `outlineEnds`; an end at a junction is not.
    // Every needle point stays inside the letter, and every sharp corner of it gets one
    // within 0.2 mm.
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "text.add",
        json!({"text": "K", "size": 25, "x": 50, "y": 70, "fill": "Red"}),
    );
    let r = run(&mut s, "text.toShapes", json!({"id": 1}));
    let shape_id = r["ids"][0].as_u64().expect("shape");
    let shape = s
        .design()
        .objects
        .iter()
        .find(|o| o.id == shape_id)
        .expect("shape")
        .contours
        .clone();
    let r = run(
        &mut s,
        "object.deriveSkeleton",
        json!({"id": shape_id, "branches": true}),
    );
    let ids: Vec<u64> = r["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 3, "{r}");
    let ends: Vec<[bool; 2]> = ids
        .iter()
        .map(|id| {
            let o = s
                .design()
                .objects
                .iter()
                .find(|o| o.id == *id)
                .expect("column");
            o.region
                .as_ref()
                .and_then(|r| r.outline_ends)
                .expect("a branch column carries outlineEnds")
        })
        .collect();
    assert_eq!(ends[0], [true, true], "the stem runs from edge to edge");
    assert!(
        ends[1..]
            .iter()
            .all(|e| e.iter().filter(|&&b| b).count() == 1),
        "each arm has one end on the outline and one at a junction: {ends:?}"
    );
    // Read back through the command as `outlineEnds`.
    let got = run(&mut s, "object.get", json!({"id": ids[0]}));
    assert_eq!(got["region"]["outlineEnds"], json!([true, true]), "{got}");
    // The stitches: inside the letter (or on its edge), and into every sharp corner.
    let pattern = Compiler::default().compile(s.design());
    let points: Vec<openstitch_design::Vec2> = pattern
        .stitches
        .iter()
        .filter(|st| st.command == Command::Stitch)
        .map(|st| openstitch_design::v2(st.x, st.y))
        .collect();
    assert!(points.len() > 300, "{}", points.len());
    let edges: Vec<(openstitch_design::Vec2, openstitch_design::Vec2)> =
        shape.iter().flat_map(|c| c.edges()).collect();
    let to_edge = |p: openstitch_design::Vec2| {
        edges
            .iter()
            .map(|&(a, b)| {
                let d = b - a;
                let len2 = d.x * d.x + d.y * d.y;
                let t = if len2 > 0.0 {
                    (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                p.distance(a + d * t)
            })
            .fold(f64::INFINITY, f64::min)
    };
    // Design units (0.1 mm): a needle point on the outline may lie this far outside it, and a
    // sharp corner's nearest needle point this far from the corner.
    const ON_EDGE_UNITS: f64 = 0.5;
    const CORNER_REACH_UNITS: f64 = 2.0;
    for &p in &points {
        assert!(
            openstitch_design::geometry::contains(p, &shape) || to_edge(p) <= ON_EDGE_UNITS,
            "{p:?} is sewn outside the K"
        );
    }
    for c in &shape {
        let pts = c.outline_points();
        let n = pts.len();
        for k in 0..n {
            let (a, b, e) = (pts[(k + n - 1) % n], pts[k], pts[(k + 1) % n]);
            let (u, v) = (b - a, e - b);
            if u.length() < 2.0 || v.length() < 2.0 {
                continue;
            }
            let turn = (u.x * v.y - u.y * v.x)
                .atan2(u.x * v.x + u.y * v.y)
                .abs()
                .to_degrees();
            if turn < 60.0
                || !openstitch_design::geometry::contains(b + (u * -0.05 + v * 0.05), &shape)
            {
                continue;
            }
            let near = points
                .iter()
                .map(|p| p.distance(b))
                .fold(f64::INFINITY, f64::min);
            assert!(
                near <= CORNER_REACH_UNITS,
                "corner {b:?}: nearest needle point {near:.2}"
            );
        }
    }
    // Undo and redo keep the flags; so does a save and a reload.
    let after = s.design().clone();
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.redo", json!({}));
    assert_eq!(s.design(), &after);
    let saved = project::encode(s.design()).expect("encode");
    assert!(
        String::from_utf8(saved.clone())
            .expect("utf8")
            .contains("outlineEnds")
    );
    assert_eq!(&project::decode(&saved).expect("decode"), s.design());
}

#[test]
fn every_branch_column_is_sewn_out_to_the_shapes_edges() {
    // #526: R at 25 mm. Every column is a branch column (`outlineEnds` set: the stem `[true,
    // true]`, each arm one end on the edge and one at its junction), sewn within its part of
    // the R and out to the R's edges, where a column at its derived width left slivers unsewn
    // along the bowl and the leg.
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "text.add",
        json!({"text": "R", "size": 25, "x": 50, "y": 70, "fill": "Red"}),
    );
    let r = run(&mut s, "text.toShapes", json!({"id": 1}));
    let shape_id = r["ids"][0].as_u64().expect("shape");
    let shape = s
        .design()
        .objects
        .iter()
        .find(|o| o.id == shape_id)
        .expect("shape")
        .contours
        .clone();
    let r = run(
        &mut s,
        "object.deriveSkeleton",
        json!({"id": shape_id, "branches": true}),
    );
    let ids: Vec<u64> = r["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert!(ids.len() >= 3, "{r}");
    let flags: Vec<Value> = ids
        .iter()
        .map(|id| run(&mut s, "object.get", json!({"id": id}))["region"]["outlineEnds"].clone())
        .collect();
    assert!(
        flags.iter().all(Value::is_array),
        "every column carries outlineEnds: {flags:?}"
    );
    assert_eq!(flags[0], json!([true, true]), "the stem: {flags:?}");
    assert!(
        flags[1..]
            .iter()
            .all(|f| f == &json!([false, true]) || f == &json!([true, false])),
        "each arm: one end on the edge, one at its junction: {flags:?}"
    );
    // Out to the edges: needle points within 0.3 mm of the R's outline all the way round, but
    // for the inner corners where two columns meet.
    let pattern = Compiler::default().compile(s.design());
    let points: Vec<openstitch_design::Vec2> = pattern
        .stitches
        .iter()
        .filter(|st| st.command == Command::Stitch)
        .map(|st| openstitch_design::v2(st.x, st.y))
        .collect();
    let mut samples = 0;
    let mut reached = 0;
    for c in &shape {
        let pts = c.outline_points();
        let n = pts.len();
        for k in 0..n {
            let (a, b) = (pts[k], pts[(k + 1) % n]);
            let steps = (a.distance(b) / 2.0).ceil().max(1.0) as usize;
            for t in 0..steps {
                let q = a.lerp(b, t as f64 / steps as f64);
                samples += 1;
                if points.iter().any(|p| p.distance(q) <= 3.0) {
                    reached += 1;
                }
            }
        }
    }
    let share = reached as f64 / samples as f64;
    assert!(
        share >= REACHED,
        "{reached} of {samples} outline points reached ({share:.3})"
    );
}

/// The share of R's outline [`every_branch_column_is_sewn_out_to_the_shapes_edges`] needs
/// reached: 0.965 measured (2026-11-12); a needle point within 0.3 mm, so an edge point between
/// two of the satin's needle points on that side may count as not reached though it is sewn.
const REACHED: f64 = 0.95;

#[test]
fn a_crossbar_joined_at_both_ends_is_one_column_of_its_own() {
    // #526: A at 25 mm. Its crossbar touches both legs and has no free end: it is one column
    // from leg to leg (`outlineEnds` [false, false], both ends at junctions), not an arm from
    // its middle into one leg that left the other half unsewn. The bar's whole length is
    // sewn: needle points across it all the way from leg to leg.
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "text.add",
        json!({"text": "A", "size": 25, "x": 50, "y": 70, "fill": "Red"}),
    );
    let r = run(&mut s, "text.toShapes", json!({"id": 1}));
    let shape_id = r["ids"][0].as_u64().expect("shape");
    let r = run(
        &mut s,
        "object.deriveSkeleton",
        json!({"id": shape_id, "branches": true}),
    );
    let ids: Vec<u64> = r["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), 3, "two legs and the bar: {r}");
    let bars: Vec<Value> = ids
        .iter()
        .map(|id| run(&mut s, "object.get", json!({"id": id})))
        .filter(|o| o["region"]["outlineEnds"] == json!([false, false]))
        .collect();
    assert_eq!(bars.len(), 1, "one column joined at both ends");
    // The bar runs across the letter: its skeleton spans most of the gap between the legs.
    let pts = bars[0]["region"]["skeleton"]["points"]
        .as_array()
        .expect("points");
    let xs: Vec<f64> = pts.iter().filter_map(|p| p["x"].as_f64()).collect();
    let span = xs.iter().copied().fold(f64::NEG_INFINITY, f64::max)
        - xs.iter().copied().fold(f64::INFINITY, f64::min);
    assert!(span > 40.0, "the bar's column spans {span:.0} (0.1 mm)");
}

#[test]
fn a_at_10_mm_meets_the_boxes_of_526_on_its_stitches() {
    // #526's boxes on A at 10 mm, on the compiled stitches: A's crossbar is a column of its own
    // and its legs' width spikes are capped.
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "text.add",
        json!({"text": "A", "size": 10, "x": 50, "y": 70, "fill": "Red"}),
    );
    let r = run(&mut s, "text.toShapes", json!({"id": 1}));
    let shape_id = r["ids"][0].as_u64().expect("shape");
    meets_the_boxes_of_526(&mut s, shape_id, 3, 0.96);
}

#[test]
fn a_at_25_mm_meets_the_boxes_of_526_on_its_stitches() {
    // A's left leg past the crossbar: its centre line no longer turns back into the junction,
    // so the leg's satin reaches its outer edge at the bar (1.25 mm of it was bare).
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "text.add",
        json!({"text": "A", "size": 25, "x": 50, "y": 70, "fill": "Red"}),
    );
    let r = run(&mut s, "text.toShapes", json!({"id": 1}));
    let shape_id = r["ids"][0].as_u64().expect("shape");
    meets_the_boxes_of_526(&mut s, shape_id, 3, 0.98);
}

#[test]
fn a_ring_at_25_mm_meets_the_boxes_of_526_on_its_stitches() {
    // A ring's quarters meet on a curve: a quarter joining the ring's narrower main column runs
    // on to where its centre lies deepest in it, so the join is past joinOverlap and the
    // outline where they meet is sewn (it was 0.24 mm deep, 0.36 mm of the outer edge bare).
    let mut s = Session::new(std::env::temp_dir());
    let r = run(
        &mut s,
        "shape.preset",
        json!({"kind": "frame", "frameOf": "ellipse", "cx": 50, "cy": 50, "width": 25,
               "height": 25, "bandWidth": 5, "fill": "Red"}),
    );
    let shape_id = r["id"].as_u64().expect("shape");
    meets_the_boxes_of_526(&mut s, shape_id, 4, 0.98);
}

/// #526's boxes on the branch columns `object.deriveSkeleton {branches: true, joinOverlap: 0.3}`
/// makes of `shape_id` (`columns` of them), measured on the compiled stitches as
/// tools/branch_columns.py measures them (rule B'): coverage at least `coverage`, no stretch of
/// outline over 0.3 mm unsewn, every sharp corner within 0.2 mm of a needle point, every join
/// at least 0.3 mm deep. Distances are design units of 0.1 mm.
fn meets_the_boxes_of_526(s: &mut Session, shape_id: u64, columns: usize, coverage_floor: f64) {
    const GAP_UNITS: f64 = 3.0; // 0.3 mm: longest stretch of outline left unsewn
    const NEAR_UNITS: f64 = 1.5; // 0.15 mm: an outline point this near the satin is sewn
    const CORNER_UNITS: f64 = 2.0; // 0.2 mm: a sharp corner's nearest needle point
    const JOIN_UNITS: f64 = 2.9; // 0.3 mm joinOverlap, within the 0.01 mm the grid rounds to
    use openstitch_design::{Vec2, geometry::contains, v2};
    let shape = s
        .design()
        .objects
        .iter()
        .find(|o| o.id == shape_id)
        .expect("shape")
        .contours
        .clone();
    let r = run(
        s,
        "object.deriveSkeleton",
        json!({"id": shape_id, "branches": true, "joinOverlap": 0.3}),
    );
    let ids: Vec<u64> = r["ids"]
        .as_array()
        .expect("ids")
        .iter()
        .filter_map(Value::as_u64)
        .collect();
    assert_eq!(ids.len(), columns, "{r}");
    let pattern = Compiler::default().compile(s.design());
    let st = &pattern.stitches;
    // The satin's triangles: each needle point with the two before it, within one run, the
    // stitches up to 15 mm (longer ones are travel), tie-ins and locks left out (out-and-back
    // steps back to the same point, at most 2 mm, at a run's start or end): a triangle whose own
    // stitch is such a step, so the point a tie-in ends on still starts the satin.
    let is_stitch = |k: usize| st[k].command == Command::Stitch;
    let p = |k: usize| v2(st[k].x, st[k].y);
    let n = st.len();
    let mut pos = vec![0usize; n];
    let mut left = vec![0usize; n];
    for k in 0..n {
        pos[k] = if is_stitch(k) {
            if k > 0 { pos[k - 1] + 1 } else { 1 }
        } else {
            0
        };
    }
    for k in (0..n).rev() {
        left[k] = if is_stitch(k) {
            if k + 1 < n { left[k + 1] + 1 } else { 1 }
        } else {
            0
        };
    }
    let mut tie = vec![false; n];
    for k in 2..n {
        let edge = pos[k] <= 8 || left[k] <= 8;
        if edge
            && is_stitch(k)
            && is_stitch(k - 1)
            && is_stitch(k - 2)
            && p(k).distance(p(k - 2)) < 0.5
            && p(k).distance(p(k - 1)) <= 20.0
        {
            tie[k] = true;
            tie[k - 1] = true;
        }
    }
    let triangles: Vec<[Vec2; 3]> = (2..n)
        .filter(|&k| {
            is_stitch(k)
                && is_stitch(k - 1)
                && is_stitch(k - 2)
                && !(tie[k] || tie[k - 1])
                && p(k).distance(p(k - 1)) <= 150.0
                && p(k - 1).distance(p(k - 2)) <= 150.0
        })
        .map(|k| [p(k - 2), p(k - 1), p(k)])
        .collect();
    let in_triangle = |q: Vec2, t: &[Vec2; 3]| {
        let side = |a: Vec2, b: Vec2| (b.x - a.x) * (q.y - a.y) - (b.y - a.y) * (q.x - a.x);
        let (d1, d2, d3) = (side(t[0], t[1]), side(t[1], t[2]), side(t[2], t[0]));
        !((d1 < 0.0 || d2 < 0.0 || d3 < 0.0) && (d1 > 0.0 || d2 > 0.0 || d3 > 0.0))
    };
    let segment = |q: Vec2, a: Vec2, b: Vec2| {
        let d = b - a;
        let len2 = d.x * d.x + d.y * d.y;
        let t = if len2 > 0.0 {
            (((q.x - a.x) * d.x + (q.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        q.distance(a + d * t)
    };
    let sewn_near = |q: Vec2| {
        triangles.iter().any(|t| {
            in_triangle(q, t)
                || segment(q, t[0], t[1]) <= NEAR_UNITS
                || segment(q, t[1], t[2]) <= NEAR_UNITS
                || segment(q, t[2], t[0]) <= NEAR_UNITS
        })
    };
    // Coverage: the share of the letter's inside, on a 0.1 mm grid, inside a triangle.
    let pts: Vec<Vec2> = shape.iter().flat_map(|c| c.outline_points()).collect();
    let (x0, x1) = pts
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| {
            (a.min(q.x), b.max(q.x))
        });
    let (y0, y1) = pts
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| {
            (a.min(q.y), b.max(q.y))
        });
    let (mut inside, mut covered) = (0usize, 0usize);
    let mut y = y0 + 0.5;
    while y < y1 {
        let mut x = x0 + 0.5;
        while x < x1 {
            let q = v2(x, y);
            if contains(q, &shape) {
                inside += 1;
                if triangles.iter().any(|t| in_triangle(q, t)) {
                    covered += 1;
                }
            }
            x += 1.0;
        }
        y += 1.0;
    }
    let coverage = covered as f64 / inside as f64;
    assert!(coverage >= coverage_floor, "coverage {coverage:.3}");
    // Gap: the longest stretch of outline farther than 0.15 mm from the satin.
    for c in &shape {
        let o = c.outline_points();
        let mut samples = Vec::new();
        for k in 0..o.len() {
            let (a, b) = (o[k], o[(k + 1) % o.len()]);
            let m = (a.distance(b) / 0.5).ceil().max(1.0) as usize;
            for t in 0..m {
                samples.push((a.lerp(b, t as f64 / m as f64), a.distance(b) / m as f64));
            }
        }
        let (mut run_len, mut longest) = (0.0, 0.0f64);
        for _ in 0..2 {
            for &(q, step) in &samples {
                run_len = if sewn_near(q) { 0.0 } else { run_len + step };
                longest = longest.max(run_len);
            }
        }
        assert!(
            longest <= GAP_UNITS,
            "an unsewn stretch of {longest:.1} (0.1 mm)"
        );
    }
    // Corners: every sharp convex corner has a needle point within 0.2 mm.
    let needles: Vec<Vec2> = (0..n).filter(|&k| is_stitch(k)).map(p).collect();
    for c in &shape {
        let o = c.outline_points();
        let m = o.len();
        for k in 0..m {
            let (a, b, e) = (o[(k + m - 1) % m], o[k], o[(k + 1) % m]);
            let (u, v) = (b - a, e - b);
            if u.length() < 2.0 || v.length() < 2.0 {
                continue;
            }
            let turn = (u.x * v.y - u.y * v.x)
                .atan2(u.x * v.x + u.y * v.y)
                .abs()
                .to_degrees();
            if turn < 60.0 || !contains(b + (u * -0.05 + v * 0.05), &shape) {
                continue;
            }
            let near = needles
                .iter()
                .map(|q| q.distance(b))
                .fold(f64::INFINITY, f64::min);
            assert!(near <= CORNER_UNITS, "corner {b:?}: {near:.2}");
        }
    }
    // Joins: every column end that isn't on the outline lies at least 0.3 mm inside another
    // column's satin.
    let skeletons: Vec<(Vec<Vec2>, Vec<f64>)> = ids
        .iter()
        .map(|id| {
            let o = run(s, "object.get", json!({"id": id}));
            let sk = &o["region"]["skeleton"];
            let points = sk["points"]
                .as_array()
                .expect("points")
                .iter()
                .map(|q| v2(q["x"].as_f64().expect("x"), q["y"].as_f64().expect("y")))
                .collect();
            let widths = sk["widths"]
                .as_array()
                .expect("widths")
                .iter()
                .filter_map(Value::as_f64)
                .collect();
            (points, widths)
        })
        .collect();
    let edges: Vec<(Vec2, Vec2)> = shape.iter().flat_map(|c| c.edges()).collect();
    for (i, (sp, _)) in skeletons.iter().enumerate() {
        for end in [sp[0], sp[sp.len() - 1]] {
            if edges.iter().any(|&(a, b)| segment(end, a, b) < 5.0) {
                continue;
            }
            let depth = skeletons
                .iter()
                .enumerate()
                .filter(|&(j, _)| j != i)
                .flat_map(|(_, (op, ow))| {
                    op.windows(2).zip(ow.windows(2)).map(move |(w, ws)| {
                        let d = w[1] - w[0];
                        let len2 = d.x * d.x + d.y * d.y;
                        let t = if len2 > 0.0 {
                            (((end.x - w[0].x) * d.x + (end.y - w[0].y) * d.y) / len2)
                                .clamp(0.0, 1.0)
                        } else {
                            0.0
                        };
                        5.0 * (ws[0] + (ws[1] - ws[0]) * t) - end.distance(w[0] + d * t)
                    })
                })
                .fold(f64::NEG_INFINITY, f64::max);
            assert!(depth >= JOIN_UNITS, "a join only {depth:.2} (0.1 mm) deep");
        }
    }
}

#[test]
fn pes_0110_refuses_a_satin_line_and_the_machine_formats_sew_it() {
    let dir = openstitch_testutil::TempDir::new("satin-line-pes");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "design.new",
        json!({"name": "satin", "width": 100, "height": 100}),
    );
    run(
        &mut s,
        "shape.path",
        json!({"points": [[10, 10], [60, 10]], "closed": false, "outline": "Black"}),
    );
    run(
        &mut s,
        "object.setLine",
        json!({"id": 1, "type": "satin", "widthKeys": [[0, 0.5, 0.5], [1, 2, 2]]}),
    );
    // PES 0110 cannot hold an OpenStitch satin line: refused with the object and the type named.
    let err = s
        .execute(
            "design.export",
            Some(&json!({"path": "a.pes", "pesVersion": "11"})),
        )
        .expect_err("PES 0110 refuses a satin line")
        .to_string();
    assert!(
        err.contains("Satin line") && err.contains("not mapped"),
        "{err}"
    );
    assert!(!dir.path().join("a.pes").exists());
    // The default PES (stitches) and a DST sew it.
    for path in ["b.pes", "b.dst"] {
        run(&mut s, "design.export", json!({ "path": path }));
        assert!(std::fs::metadata(dir.path().join(path)).expect(path).len() > 100);
    }
}

/// The stitches of the design's first object.
fn sewn(s: &Session) -> Vec<openstitch_core::Stitch> {
    Compiler::default().compile(s.design()).stitches
}

#[test]
fn satin_lettering_takes_the_shaping_and_looks_through_the_command() {
    // #676, #686: satin text sews its strokes with the same shaping as a column: short
    // stitches, compensation, the satin looks and raised satin's padding, set with
    // object.setRegion on the text object. Unset, the letters sew as before.
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "text.add",
        json!({"text": "Sous", "font": "lato-bold", "size": 20, "x": 10, "y": 70, "fill": "Red"}),
    );
    let before = sewn(&s);
    let n = stitches(&s);
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "shortStitchDistance": 0.25, "pullPercent": 3}),
    );
    assert_ne!(sewn(&s), before, "short stitches and compensation");
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "shortStitchDistance": 0, "pullPercent": 0, "satinStyle": "open"}),
    );
    let open = stitches(&s);
    assert!(
        open * 2 < n,
        "open: rows three times as far apart ({open} of {n})"
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "satinStyle": "double"}),
    );
    assert!(stitches(&s) > n * 3 / 2, "double: sewn twice");
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "satinStyle": "normal", "raised": true}),
    );
    assert!(
        stitches(&s) > n * 3 / 2,
        "raised: padding lanes under each stroke"
    );
    run(&mut s, "object.setRegion", json!({"id": 1, "raised": null}));
    assert_eq!(sewn(&s), before, "back to the defaults");
}

#[test]
fn skeleton_and_auto_satin_take_the_shaping_through_the_command() {
    // #675: a skeleton satin with a 60 degree corner, and a 90 degree bent band sewn as Auto
    // satin; the same parameters as on a column.
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.skeletonSatin",
        json!({"points": [[0, 0], [30, 0], [15, 25.98]], "widths": [4, 4, 4], "fill": "Red"}),
    );
    let mut band = Session::new(std::env::temp_dir());
    run(
        &mut band,
        "shape.path",
        json!({"points": [[0, 0], [30, 0], [30, 30], [25, 30], [25, 5], [0, 5]],
               "closed": true, "fill": "Red"}),
    );
    run(
        &mut band,
        "object.setRegion",
        json!({"id": 1, "type": "satin", "autoDirection": true, "underlay": false}),
    );
    for s in [&mut s, &mut band] {
        run(s, "object.setRegion", json!({"id": 1, "underlay": false}));
        let before = sewn(s);
        run(
            s,
            "object.setRegion",
            json!({"id": 1, "cornerStyle": "mitre"}),
        );
        let mitred = sewn(s);
        assert_ne!(mitred, before, "mitred");
        run(
            s,
            "object.setRegion",
            json!({"id": 1, "shortStitchDistance": 0.3, "pullLeft": 0.3, "pullPercent": 5}),
        );
        assert_ne!(sewn(s), mitred, "short stitches and compensation");
        // Width-following under sewing along the column, then a second layer.
        run(
            s,
            "object.setRegion",
            json!({"id": 1, "underlay": true, "underlayStyle": "auto", "underlaySpacing": 1.5}),
        );
        let rows = stitches(s);
        run(
            s,
            "object.setRegion",
            json!({"id": 1, "underlay2": {"style": "centreWalk"}}),
        );
        assert!(stitches(s) > rows, "the second layer adds stitches");
        // Back to PE-DESIGN's: the same stitches as before any shaping.
        run(
            s,
            "object.setRegion",
            json!({"id": 1, "cornerStyle": "turn", "shortStitchDistance": 0, "pullLeft": null,
                   "pullPercent": 0, "underlay": false, "underlayStyle": "edgeZigzag",
                   "underlay2": null}),
        );
        assert_eq!(sewn(s), before, "back to the defaults");
    }
}

/// #685, #793: the satin looks and raised satin on a fixed-direction satin region and raised
/// satin on a skeleton satin, through `object.setRegion`: the stitches change, raised adds a
/// padding layer under the satin, one undo step each.
#[test]
fn satin_regions_and_skeleton_satin_take_the_looks_through_the_command() {
    let layers = |s: &Session| {
        let d = s.design();
        let o = &d.objects[0];
        openstitch_design::region::layers(&o.contours, o.region.as_ref().expect("region"), &[])
            .len()
    };
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 8, "fill": "Red"}),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "type": "satin", "angle": 90, "underlay": false}),
    );
    let plain = stitches(&s);
    let plain_layers = layers(&s);
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "satinStyle": "open"}),
    );
    let open = stitches(&s);
    assert!(open * 2 < plain, "open {open} of {plain}");
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "satinStyle": "normal", "jaggedEdge": "in"}),
    );
    assert_ne!(stitches(&s), open);
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "jaggedEdge": null, "raised": true}),
    );
    assert_eq!(layers(&s), plain_layers + 1, "padding under the satin");
    assert!(stitches(&s) > plain);
    for _ in 0..3 {
        run(&mut s, "history.undo", json!({}));
    }
    assert_eq!(stitches(&s), plain);

    let mut k = Session::new(std::env::temp_dir());
    run(
        &mut k,
        "shape.skeletonSatin",
        json!({"points": [[10, 30], [70, 30]], "widths": [4, 8]}),
    );
    run(
        &mut k,
        "object.setRegion",
        json!({"id": 1, "underlay": false}),
    );
    let (before, before_layers) = (stitches(&k), layers(&k));
    run(&mut k, "object.setRegion", json!({"id": 1, "raised": true}));
    assert_eq!(layers(&k), before_layers + 1);
    assert!(stitches(&k) > before);
}

#[test]
fn satin_text_corners_take_the_corner_style() {
    // #782: satin text is cut at its sharp corners; with a corner style, two strokes sewn one
    // after the other through a corner (Z's) are one column there, mitred or capped. Unset or
    // turn, the letters sew as before.
    let mut s = Session::new(std::env::temp_dir());
    run(
        &mut s,
        "text.add",
        json!({"text": "Z", "font": "lato-bold", "size": 20, "x": 10, "y": 70, "fill": "Red"}),
    );
    let before = sewn(&s);
    let bounds = |st: &[openstitch_core::Stitch]| {
        let xs = st.iter().map(|p| p.x);
        let ys = st.iter().map(|p| p.y);
        (
            xs.clone().fold(f64::INFINITY, f64::min),
            xs.fold(f64::NEG_INFINITY, f64::max),
            ys.clone().fold(f64::INFINITY, f64::min),
            ys.fold(f64::NEG_INFINITY, f64::max),
        )
    };
    let (x0, x1, y0, y1) = bounds(&before);
    let mut seen = vec![before.clone()];
    for style in ["mitre", "cap"] {
        run(
            &mut s,
            "object.setRegion",
            json!({"id": 1, "cornerStyle": style}),
        );
        let now = sewn(&s);
        assert!(!seen.contains(&now), "{style} sews its own corners");
        // The corners stay within the letter (0.5 mm for compensation at the tips).
        let (a0, a1, b0, b1) = bounds(&now);
        assert!(
            a0 >= x0 - 5.0 && a1 <= x1 + 5.0 && b0 >= y0 - 5.0 && b1 <= y1 + 5.0,
            "{style}: {:?} within {:?}",
            (a0, a1, b0, b1),
            (x0, x1, y0, y1)
        );
        seen.push(now);
    }
    run(
        &mut s,
        "object.setRegion",
        json!({"id": 1, "cornerStyle": "turn"}),
    );
    assert_eq!(sewn(&s), before, "turn: as before");
}

#[test]
fn spacing_at_reaches_skeleton_and_auto_satin_through_the_command() {
    // #685, #854: `spacingAt` through `object.setRegion` on a half-circle skeleton satin and a
    // bent Auto satin region. 0 measures the spacing on the inner edge, so fewer
    // crossings are sewn; 1 is each kind's own default again.
    let arc = |r: f64| -> Vec<[f64; 2]> {
        (0..=24)
            .map(|k| {
                let a = std::f64::consts::PI * k as f64 / 24.0;
                [50.0 + r * a.cos(), 50.0 - r * a.sin()]
            })
            .collect()
    };
    let mut skeleton = Session::new(std::env::temp_dir());
    let centre = arc(30.0);
    let widths = vec![8.0; centre.len()];
    run(
        &mut skeleton,
        "shape.skeletonSatin",
        json!({"points": centre, "widths": widths, "fill": "Red"}),
    );
    // The 90 degree bent band of `skeleton_and_auto_satin_take_the_shaping_through_the_command`,
    // which Auto satin sews along its derived centre line.
    let mut auto = Session::new(std::env::temp_dir());
    run(
        &mut auto,
        "shape.path",
        json!({"points": [[0, 0], [30, 0], [30, 30], [25, 30], [25, 5], [0, 5]],
               "closed": true, "fill": "Red"}),
    );
    run(
        &mut auto,
        "object.setRegion",
        json!({"id": 1, "type": "satin", "autoDirection": true}),
    );
    for (kind, s) in [("skeleton", &mut skeleton), ("auto", &mut auto)] {
        run(s, "object.setRegion", json!({"id": 1, "underlay": false}));
        let plain = stitches(s);
        run(s, "object.setRegion", json!({"id": 1, "spacingAt": 0}));
        let inner = stitches(s);
        assert!(
            inner * 100 < plain * 95,
            "{kind}: spacingAt 0 sews fewer crossings: {inner} against {plain}"
        );
        let saved = project::encode(s.design()).expect("encode");
        assert_eq!(
            &project::decode(&saved).expect("decode"),
            s.design(),
            "{kind}"
        );
        run(s, "object.setRegion", json!({"id": 1, "spacingAt": 1}));
        assert_eq!(stitches(s), plain, "{kind}: 1 is the default again");
    }
}
