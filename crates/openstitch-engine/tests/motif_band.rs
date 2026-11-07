//! The motif band (#544, beyond PE-DESIGN): a motif along a satin column, stretched across its
//! local width, through the commands.

use openstitch_core::Command;
use openstitch_design::{Compiler, Vec2, v2};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn stitches(s: &Session) -> Vec<Vec2> {
    Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
        .iter()
        .filter(|st| st.command == Command::Stitch)
        .map(|st| v2(st.x, st.y))
        .collect()
}

/// The skeleton's width at `x` (0.1 mm): 2 mm at x = 10 mm to 8 mm at x = 70 mm.
fn width(x: f64) -> f64 {
    20.0 + 60.0 * (x - 100.0) / 600.0
}

#[test]
fn a_skeleton_satin_band_follows_its_taper() {
    let dir = TempDir::new("motif-band");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "shape.skeletonSatin",
        json!({ "points": [[10, 30], [70, 30]], "widths": [2, 8] }),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({ "id": 1, "underlay": false }),
    );
    let satin = stitches(&s);
    let before = s.design().clone();
    run(
        &mut s,
        "object.setRegion",
        json!({ "id": 1, "motifBand": "zigzag", "bandTile": 6 }),
    );
    let band = s.design().objects[0]
        .region
        .as_ref()
        .expect("region")
        .motif_band
        .clone()
        .expect("band");
    assert_eq!(
        (band.motif.as_str(), band.tile, band.mirror),
        ("zigzag", Some(6.0), false)
    );
    let run_ = stitches(&s);
    assert_ne!(run_, satin, "the band replaces the satin");
    // Inside the column at every point (ties step 0.3 mm), touching both edges along it.
    for p in &run_ {
        assert!((p.y - 300.0).abs() <= width(p.x) / 2.0 + 3.5, "{p:?}");
    }
    let edge = |sign: f64| {
        run_.iter()
            .filter(|p| (p.y - (300.0 + sign * width(p.x) / 2.0)).abs() <= 2.0)
            .count()
    };
    assert!(
        edge(1.0) >= 8 && edge(-1.0) >= 8,
        "{} {}",
        edge(1.0),
        edge(-1.0)
    );
    run(
        &mut s,
        "object.setRegion",
        json!({ "id": 1, "bandMirror": true }),
    );
    assert_ne!(stitches(&s), run_, "mirrored copies");
    run(&mut s, "history.undo", json!({}));
    run(&mut s, "history.undo", json!({}));
    assert_eq!(s.design(), &before, "one undo step per change");
}

#[test]
fn satin_lines_and_columns_take_a_band_and_others_refuse_it() {
    let dir = TempDir::new("motif-band-line");
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "shape.path",
        json!({ "points": [[10, 30], [90, 30]], "closed": false }),
    );
    run(
        &mut s,
        "object.setLine",
        json!({ "id": 1, "type": "satin", "width": 6 }),
    );
    run(
        &mut s,
        "object.setLine",
        json!({ "id": 1, "motifBand": "star", "bandMirror": true }),
    );
    let pts = stitches(&s);
    assert!(pts.len() > 20, "{}", pts.len());
    assert!(
        pts.iter().all(|p| (p.y - 300.0).abs() <= 33.5),
        "inside the 6 mm line"
    );
    run(
        &mut s,
        "object.setLine",
        json!({ "id": 1, "motifBand": "none" }),
    );
    assert!(
        s.design().objects[0]
            .line
            .as_ref()
            .expect("line")
            .motif_band
            .is_none()
    );
    // A Manual Punch column.
    run(
        &mut s,
        "shape.column",
        json!({ "lines": [[[10, 50], [10, 54]], [[40, 50], [40, 58]]] }),
    );
    run(
        &mut s,
        "object.setRegion",
        json!({ "id": 2, "motifBand": "wave" }),
    );
    let before = s.design().clone();
    for (cmd, params) in [
        (
            "object.setLine",
            json!({ "id": 1, "motifBand": "no-such-motif" }),
        ),
        ("object.setLine", json!({ "id": 1, "bandTile": 5 })),
        ("object.setRegion", json!({ "id": 2, "bandTile": 0.5 })),
        ("object.setRegion", json!({ "id": 2, "bandTile": 60 })),
        (
            "object.setLine",
            json!({ "id": 1, "type": "running", "motifBand": "star" }),
        ),
    ] {
        assert!(s.execute(cmd, Some(&params)).is_err(), "{params}");
        assert_eq!(s.design(), &before, "{params}");
    }
}
