//! Test swatches (#452, #507) and the fabric calibration card (#453), through the commands.

use openstitch_design::{Compiler, Design, RegionType};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn id(v: &Value) -> u64 {
    v["id"].as_u64().expect("id")
}

/// Opens a written `.osd` in a fresh session.
fn open(dir: &TempDir, path: &std::path::Path) -> Design {
    let mut s = Session::new(dir);
    run(
        &mut s,
        "project.open",
        json!({ "path": path.display().to_string() }),
    );
    s.design().clone()
}

/// The sewn extent of a design.
fn sewn(d: &Design) -> openstitch_core::Bounds {
    Compiler::for_design(d).compile(d).bounds()
}

fn square(s: &mut Session, x: f64, colour: &str) -> u64 {
    id(&run(
        s,
        "shape.rectangle",
        json!({ "x": x, "y": 10, "width": 10, "height": 10, "fill": colour }),
    ))
}

#[test]
fn test_swatch_writes_the_objects_alone_in_the_smallest_hoop() {
    let dir = TempDir::new("swatch-452");
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "machine.set",
        json!({ "hoop": "custom", "width": 300, "height": 200 }),
    );
    let a = square(&mut s, 10.0, "Red");
    square(&mut s, 60.0, "Blue");
    run(
        &mut s,
        "object.setRegion",
        json!({ "ids": [a], "angle": 135 }),
    );
    let before = s.design().clone();
    let osd = dir.path().join("swatch.osd");
    let r = run(
        &mut s,
        "design.testSwatch",
        json!({ "ids": [a], "path": osd.display().to_string(), "label": true }),
    );
    // The open design is not changed.
    assert_eq!(s.design(), &before);
    let d = open(&dir, &osd);
    // The square and its label.
    assert_eq!(r["objects"], json!(2), "{r}");
    assert_eq!(d.objects.len(), 2);
    let sq = &d.objects[0];
    assert_eq!(sq.region.as_ref().expect("region").angle, 135.0);
    assert!(d.objects[1].text.is_some(), "the label is text");
    // The smallest Brother hoop (100 × 100 mm), the swatch centred on it.
    assert_eq!(r["hoop"], json!(d.machine.hoop.clone().expect("a hoop")));
    assert!(d.page.width <= 1000.0 + 1e-6 && d.page.height <= 1000.0 + 1e-6);
    let b = sewn(&d);
    assert!(((b.min_x + b.max_x) / 2.0 - d.page.width / 2.0).abs() < 1.0);
    assert!(((b.min_y + b.max_y) / 2.0 - d.page.height / 2.0).abs() < 1.0);
    // A machine file too (PES by default, also without an extension).
    let pes = dir.path().join("swatch.pes");
    run(
        &mut s,
        "design.testSwatch",
        json!({ "ids": [a], "path": pes.display().to_string() }),
    );
    assert!(std::fs::metadata(&pes).expect("written").len() > 100);
    assert!(
        s.execute(
            "design.testSwatch",
            Some(&json!({ "ids": [], "path": pes.display().to_string() }))
        )
        .is_err()
    );
}

#[test]
fn test_swatches_step_one_setting_across_copies() {
    let dir = TempDir::new("swatch-507");
    let mut s = Session::new(&dir);
    let a = square(&mut s, 10.0, "Red");
    let before = s.design().clone();
    let osd = dir.path().join("grid.osd");
    let r = run(
        &mut s,
        "design.testSwatches",
        json!({
            "id": a, "path": osd.display().to_string(),
            "attribute": "density", "start": 3.0, "step": 0.5, "count": 5,
        }),
    );
    assert_eq!(s.design(), &before);
    assert_eq!(
        r["copies"],
        json!([
            "density 3/mm",
            "density 3.5/mm",
            "density 4/mm",
            "density 4.5/mm",
            "density 5/mm"
        ])
    );
    let d = open(&dir, &osd);
    let copies: Vec<_> = d.objects.iter().filter(|o| o.text.is_none()).collect();
    let labels = d.objects.iter().filter(|o| o.text.is_some()).count();
    assert_eq!((copies.len(), labels), (5, 5));
    let original = before.objects[0].region.clone().expect("region");
    for (k, o) in copies.iter().enumerate() {
        let r = o.region.clone().expect("region");
        assert_eq!(r.density, 3.0 + 0.5 * k as f64);
        // Only the stepped setting differs (entry and exit points move with the copy).
        let mut same = r.clone();
        same.density = original.density;
        (same.entry, same.exit) = (original.entry, original.exit);
        assert_eq!(same, original, "copy {k}");
        assert_eq!(
            o.name.as_deref(),
            Some(format!("density {}/mm", r.density).as_str())
        );
    }
    // Top-left first: each copy is right of or below the one before.
    for w in copies.windows(2) {
        let (p, q) = (w[0].bounds().expect("b"), w[1].bounds().expect("b"));
        assert!(q.min_x > p.min_x || q.min_y > p.min_y);
    }
    assert!(d.properties.comments.contains("density 4.5/mm"));
    // Inside the page.
    let b = sewn(&d);
    assert!(b.min_x >= 0.0 && b.min_y >= 0.0);
    assert!(b.max_x <= d.page.width && b.max_y <= d.page.height);

    // Two settings: rows × columns, each copy named by both.
    let r = run(
        &mut s,
        "design.testSwatches",
        json!({
            "id": a, "path": osd.display().to_string(),
            "attribute": "density", "start": 3.0, "step": 1.0, "count": 2,
            "attribute2": "pullCompensation", "start2": 0.0, "step2": 0.2, "count2": 3,
        }),
    );
    assert_eq!(r["grid"], json!([2, 3]));
    assert_eq!(
        r["copies"][5],
        json!("density 4/mm pullCompensation 0.4 mm")
    );
}

#[test]
fn test_swatches_refuse_settings_the_object_does_not_use_and_grids_too_large() {
    let dir = TempDir::new("swatch-507-refuse");
    let mut s = Session::new(&dir);
    let a = square(&mut s, 10.0, "Red");
    let path = dir.path().join("x.pes").display().to_string();
    let e = s
        .execute(
            "design.testSwatches",
            Some(&json!({
                "id": a, "path": path, "attribute": "linePitch",
                "start": 2, "step": 0.5, "count": 3,
            })),
        )
        .expect_err("a fill without an outline has no line");
    assert!(e.to_string().contains("density"), "{e}");
    let e = s
        .execute(
            "design.testSwatches",
            Some(&json!({
                "id": a, "path": path, "attribute": "density",
                "start": 2, "step": 0.5, "count": 25, "gap": 20,
            })),
        )
        .expect_err("25 copies 30 mm apart do not fit 100 mm");
    assert!(e.to_string().contains("copies fit"), "{e}");
    assert!(!std::path::Path::new(&path).exists());
}

#[test]
fn calibration_card_and_custom_fabric() {
    let dir = TempDir::new("swatch-453");
    let mut s = Session::new(&dir);
    let osd = dir.path().join("card.osd");
    let r = run(
        &mut s,
        "fabric.calibrationSwatch",
        json!({ "fabric": "standard", "path": osd.display().to_string() }),
    );
    assert!(
        s.design().objects.is_empty(),
        "the open design is not changed"
    );
    let rows = r["rows"].as_array().expect("rows");
    assert_eq!(rows.len(), 15);
    // Standard: fill 2.5/mm, satin 3/mm.
    assert_eq!(rows[0]["row"], json!("F1"));
    assert_eq!(rows[0]["fillDensity"], json!(2.0));
    assert_eq!(rows[9]["satinDensity"], json!(3.6));
    assert_eq!(rows[14]["pullCompensation"], json!(0.6));
    let d = open(&dir, &osd);
    let pieces: Vec<_> = d
        .objects
        .iter()
        .filter(|o| o.region.is_some() && o.text.is_none())
        .collect();
    assert_eq!(pieces.len(), 15);
    assert_eq!(
        pieces
            .iter()
            .filter(|o| o
                .region
                .as_ref()
                .is_some_and(|r| r.kind == RegionType::Satin))
            .count(),
        10
    );
    assert!(d.objects.iter().any(|o| o.name.as_deref() == Some("10 mm")));
    assert!(d.page.width <= 1000.0 + 1e-6);

    // The user picks F2, S4 and P5 from the sewn card.
    let r = run(
        &mut s,
        "fabric.calibrate",
        json!({ "fabric": "standard", "name": "My Denim", "fill": 2, "satin": 4, "pull": 5 }),
    );
    let f = &r["fabric"];
    assert_eq!(f["id"], json!("custom-my-denim"));
    assert_eq!(
        (
            &f["fillDensity"],
            &f["satinDensity"],
            &f["pullCompensation"]
        ),
        (&json!(2.25), &json!(3.3), &json!(0.6))
    );
    let list = run(&mut s, "fabric.list", json!({}));
    assert!(
        list.as_array()
            .expect("list")
            .iter()
            .any(|f| f["id"] == "custom-my-denim" && f["custom"] == json!(true))
    );
    // New objects start from it through fabric.apply.
    let a = square(&mut s, 10.0, "Red");
    run(
        &mut s,
        "fabric.apply",
        json!({ "fabric": "custom-my-denim", "ids": [a] }),
    );
    let reg = s.design().objects[0].region.clone().expect("region");
    assert_eq!((reg.density, reg.pull_compensation), (2.25, 0.6));
    // A card from the custom fabric, then deleting it.
    run(
        &mut s,
        "fabric.calibrationSwatch",
        json!({ "fabric": "custom-my-denim", "path": osd.display().to_string() }),
    );
    run(
        &mut s,
        "fabric.deleteCustom",
        json!({ "fabric": "custom-my-denim" }),
    );
    assert!(
        s.execute(
            "fabric.apply",
            Some(&json!({ "fabric": "custom-my-denim" }))
        )
        .is_err()
    );
    for bad in [
        json!({ "fabric": "standard", "name": "" }),
        json!({ "fabric": "standard", "name": "x", "fill": 6 }),
        json!({ "fabric": "nope", "name": "x" }),
    ] {
        assert!(s.execute("fabric.calibrate", Some(&bad)).is_err(), "{bad}");
    }
}
