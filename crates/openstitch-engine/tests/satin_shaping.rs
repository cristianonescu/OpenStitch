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
    for key in ["satinShaping", "underlayAuto", "underlay2"] {
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
