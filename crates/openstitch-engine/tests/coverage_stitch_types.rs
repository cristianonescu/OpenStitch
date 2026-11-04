//! Coverage for stitch-type parameters FEATURE_MATRIX claims that no other test or compat spec
//! exercises (#328): zigzag line under sewing, satin `underlayParts`, fill pull compensation,
//! a curved direction on piping and motif fill, the pattern reference point on motif fill, and
//! contour fill ignoring the region's entry and exit points.

use openstitch_core::Command;
use openstitch_design::Compiler;
use openstitch_engine::Session;
use serde_json::{Value, json};
use std::path::PathBuf;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn fails(s: &mut Session, id: &str, params: Value) {
    assert!(s.execute(id, Some(&params)).is_err(), "{id} {params} fails");
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openstitch-coverage-stitch-types-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

/// Needle points of the whole design, in 0.1 mm.
fn needle_points(s: &Session) -> Vec<(f64, f64)> {
    Compiler::default()
        .compile(s.design())
        .stitches
        .iter()
        .filter(|st| st.command == Command::Stitch)
        .map(|st| (st.x, st.y))
        .collect()
}

/// Saves the design as `.osd` and opens it in a new session.
fn reopened(s: &mut Session, dir: &PathBuf) -> Session {
    run(s, "project.save", json!({"path": "t.osd"}));
    let mut t = Session::new(dir);
    run(&mut t, "project.open", json!({"path": "t.osd"}));
    t
}

/// A 40 × 30 mm rectangle with no line, sewn as `region`.
fn rectangle(s: &mut Session, region: Value) -> u64 {
    let r = id(&run(
        s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 40, "height": 30}),
    ));
    run(s, "object.setLine", json!({"id": r, "type": "none"}));
    let mut params = region;
    params["id"] = json!(r);
    run(s, "object.setRegion", params);
    r
}

#[test]
fn zigzag_line_under_sewing_adds_a_run_beside_the_line_and_is_saved() {
    let dir = temp_dir("zigzag");
    let mut s = Session::new(&dir);
    let line = id(&run(
        &mut s,
        "shape.polygon",
        json!({"points": [[10, 50], [90, 50]], "closed": false}),
    ));
    run(
        &mut s,
        "object.setLine",
        json!({"id": line, "type": "zigzag", "width": 4, "underSewing": false}),
    );
    let plain = needle_points(&s);
    run(
        &mut s,
        "object.setLine",
        json!({"id": line, "underSewing": true}),
    );
    assert!(s.design().objects[0].line.as_ref().expect("line").underlay);
    let under = needle_points(&s);
    assert!(
        under.len() > plain.len(),
        "under sewing adds stitches: {} vs {}",
        under.len(),
        plain.len()
    );
    // The extra run lies within the zigzag's width, a quarter of it to each side.
    let extra: Vec<_> = under.iter().filter(|p| !plain.contains(p)).collect();
    assert!(!extra.is_empty());
    assert!(
        extra.iter().all(|p| (p.1 - 500.0).abs() <= 20.0 + 1.0),
        "under sewing stays inside the 4 mm zigzag"
    );
    assert!(
        extra.iter().any(|p| (p.1 - 500.0).abs() > 5.0),
        "and runs beside the line, not on it"
    );

    let t = reopened(&mut s, &dir);
    assert!(t.design().objects[0].line.as_ref().expect("line").underlay);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(needle_points(&s), plain);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn satin_underlay_parts_apply_refuse_unknown_parts_undo_and_save() {
    let dir = temp_dir("parts");
    let mut s = Session::new(&dir);
    let r = rectangle(
        &mut s,
        json!({"type": "satin", "underlay": true, "underlayParts": ["edge"]}),
    );
    let edge_only = needle_points(&s);
    let parts = |s: &Session| {
        s.design().objects[0]
            .region
            .as_ref()
            .and_then(|g| g.underlay_parts)
    };
    let p = parts(&s).expect("explicit parts");
    assert!(p.edge && !p.lattice && !p.zigzag);

    run(
        &mut s,
        "object.setRegion",
        json!({"id": r, "underlayParts": ["edge", "lattice"]}),
    );
    let p = parts(&s).expect("explicit parts");
    assert!(p.edge && p.lattice);
    let with_lattice = needle_points(&s);
    assert!(
        with_lattice.len() > edge_only.len(),
        "the lattice adds stitches: {} vs {}",
        with_lattice.len(),
        edge_only.len()
    );

    fails(
        &mut s,
        "object.setRegion",
        json!({"id": r, "underlayParts": ["edge", "sideways"]}),
    );
    assert_eq!(
        needle_points(&s),
        with_lattice,
        "a refused change changes nothing"
    );

    let t = reopened(&mut s, &dir);
    assert_eq!(
        t.design().objects[0]
            .region
            .as_ref()
            .and_then(|g| g.underlay_parts),
        parts(&s)
    );
    run(&mut s, "history.undo", json!({}));
    assert_eq!(needle_points(&s), edge_only);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn fill_pull_compensation_lengthens_the_rows_at_both_ends() {
    let width = |pull: f64| {
        let mut s = Session::new(std::env::temp_dir());
        rectangle(
            &mut s,
            json!({"type": "fill", "angle": 0, "underlay": false, "pullCompensation": pull}),
        );
        let xs: Vec<f64> = needle_points(&s).iter().map(|p| p.0).collect();
        xs.iter().cloned().fold(f64::MIN, f64::max) - xs.iter().cloned().fold(f64::MAX, f64::min)
    };
    let none = width(0.0);
    let some = width(0.5);
    // 0.5 mm at each end of the horizontal rows: about 10 units wider.
    assert!(
        (some - none - 10.0).abs() <= 3.0,
        "0 mm: {none}, 0.5 mm: {some}"
    );
}

#[test]
fn piping_and_motif_fill_follow_a_curved_direction_and_keep_it_in_osd() {
    let dir = temp_dir("curve");
    for kind in ["piping", "motifFill"] {
        let mut s = Session::new(&dir);
        let r = rectangle(&mut s, json!({"type": kind, "underlay": false}));
        let straight = needle_points(&s);
        run(
            &mut s,
            "object.setDirectionCurve",
            json!({"id": r, "points": [[10, 20], [30, 32], [50, 20]]}),
        );
        let curved = needle_points(&s);
        assert!(!curved.is_empty());
        assert_ne!(curved, straight, "{kind} rows follow the curve");
        let curve = s.design().objects[0]
            .region
            .as_ref()
            .and_then(|g| g.direction_curve.clone())
            .expect("curve");
        let t = reopened(&mut s, &dir);
        assert_eq!(
            t.design().objects[0]
                .region
                .as_ref()
                .and_then(|g| g.direction_curve.clone()),
            Some(curve),
            "{kind}: the curve survives .osd"
        );
        assert_eq!(
            needle_points(&t),
            curved,
            "{kind} sews the same after reopening"
        );
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn motif_fill_reference_point_moves_the_pattern_and_is_saved() {
    let dir = temp_dir("reference");
    let mut s = Session::new(&dir);
    let r = rectangle(&mut s, json!({"type": "motifFill", "underlay": false}));
    let default = needle_points(&s);
    run(
        &mut s,
        "object.setReferencePoint",
        json!({"id": r, "point": [11.3, 12.7]}),
    );
    let moved = needle_points(&s);
    assert_ne!(
        moved, default,
        "the pattern grid starts at the reference point"
    );
    let reference = s.design().objects[0]
        .region
        .as_ref()
        .and_then(|g| g.reference);
    assert!(reference.is_some());

    let t = reopened(&mut s, &dir);
    assert_eq!(
        t.design().objects[0]
            .region
            .as_ref()
            .and_then(|g| g.reference),
        reference
    );
    assert_eq!(needle_points(&t), moved);

    run(
        &mut s,
        "object.setReferencePoint",
        json!({"id": r, "point": null}),
    );
    assert_eq!(needle_points(&s), default, "null restores the default grid");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn contour_fill_starts_its_rings_at_the_entry_without_travel_and_ends_in_the_middle() {
    let mut s = Session::new(std::env::temp_dir());
    let r = rectangle(&mut s, json!({"type": "contour", "underlay": false}));
    let without = needle_points(&s);
    // R1 at the top-right corner of the 40 × 30 mm rectangle, R2 at its bottom-left corner.
    run(
        &mut s,
        "object.setEntryExit",
        json!({"id": r, "regionEntry": [50, 40], "regionExit": [10, 10]}),
    );
    let g = s.design().objects[0].region.clone().expect("region");
    assert!(g.entry.is_some() && g.exit.is_some(), "the points are kept");
    let with = needle_points(&s);
    let dist = |a: (f64, f64), b: (f64, f64)| ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt();

    // No travel from R1: the first needle point is already on the outermost ring, half the
    // 2 mm spacing inside the outline (x 11 or 49 mm, y 11 or 39 mm), near R1.
    let first = with[0];
    let on_ring = |v: f64, lo: f64, hi: f64| (v - lo).abs() < 1.0 || (v - hi).abs() < 1.0;
    assert!(
        on_ring(first.0, 110.0, 490.0) || on_ring(first.1, 110.0, 390.0),
        "first needle point {first:?} is on the outer ring"
    );
    assert!(dist(first, (500.0, 400.0)) < 50.0, "and near R1: {first:?}");
    // No travel to R2: the pattern ends on its innermost ring, around the centre (30, 25) mm,
    // as it does without the points.
    let last = *with.last().expect("stitches");
    assert!(dist(last, (100.0, 100.0)) > 100.0, "no run to R2: {last:?}");
    for end in [last, *without.last().expect("stitches")] {
        assert!(
            dist(end, (300.0, 250.0)) < 80.0,
            "ends in the middle: {end:?}"
        );
    }
    // The same rings, started elsewhere: about as many needle points, not a travel run more.
    let (a, b) = (with.len() as f64, without.len() as f64);
    assert!((a - b).abs() <= 0.05 * b, "{a} vs {b} needle points");
}
