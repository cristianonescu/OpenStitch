//! Coverage for `digitize.*` parameters FEATURE_MATRIX claims that no test sends (#334):
//! Photo Stitch 1's `densityMean`, `densityWindow` and `travel` in OpenStitch mode, one built-in
//! brand chart as the thread source, and Photo Stitch 1 with an `owned` chart. Small images, so
//! the unoptimised CI build stays quick (#307).
#![cfg(feature = "digitize")]

use openstitch_core::Command;
use openstitch_design::Compiler;
use openstitch_digitize::Raster;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::path::Path;

fn temp_dir(tag: &str) -> TempDir {
    TempDir::new(&format!("coverage-digitize-{tag}"))
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// 20 × 20 px: a red-to-yellow gradient with a blue block, tonal enough for Photo Stitch.
fn write_photo(dir: &Path) {
    let img = Raster::from_fn(20, 20, |x, y| {
        if (12..18).contains(&x) && (2..8).contains(&y) {
            [30, 50, 190, 255]
        } else {
            [200, (x * 12) as u8, (y * 6) as u8, 255]
        }
    })
    .expect("size");
    std::fs::write(dir.join("photo.png"), img.encode_png().expect("png")).expect("write");
}

/// Needle points and trims of the design as sewn.
fn sewn(s: &Session) -> (usize, usize) {
    let p = Compiler::default().compile(s.design());
    (p.count(Command::Stitch), p.count(Command::Trim))
}

/// Photo Stitch 1, colour, OpenStitch mode, 12 mm wide, with `extra` parameters, in a new design.
fn photo_stitch(dir: &Path, extra: Value) -> Result<(Value, (usize, usize)), String> {
    let mut s = Session::new(dir);
    let mut params = json!({"path": "photo.png", "width": 12, "colors": 4,
                            "compatMode": "openStitch"});
    params
        .as_object_mut()
        .expect("object")
        .extend(extra.as_object().expect("object").clone());
    let r = s
        .execute("digitize.photoStitch", Some(&params))
        .map_err(|e| e.to_string())?;
    let counts = sewn(&s);
    Ok((r, counts))
}

#[test]
fn photo_stitch_density_mean_and_window_set_how_densely_it_sews() {
    let dir = temp_dir("density");
    write_photo(&dir);
    let (_, (sparse, _)) =
        photo_stitch(&dir, json!({"densityMean": 0.6, "densityWindow": 1.0})).expect("sparse");
    let (_, (dense, _)) =
        photo_stitch(&dir, json!({"densityMean": 2.1, "densityWindow": 3.0})).expect("dense");
    assert!(
        dense > sparse,
        "a higher mean and window sew more needle points: {dense} vs {sparse}"
    );
    for bad in [
        json!({"densityMean": 0.4}),
        json!({"densityMean": 10.5}),
        json!({"densityWindow": 0.0}),
        json!({"densityWindow": 11}),
        json!({"travel": "sometimes"}),
    ] {
        assert!(photo_stitch(&dir, bad.clone()).is_err(), "{bad} refused");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn photo_stitch_travel_fewer_trims_never_trims_more_than_all_trims() {
    let dir = temp_dir("travel");
    write_photo(&dir);
    let (_, (_, fewer)) = photo_stitch(&dir, json!({"travel": "fewerTrims"})).expect("fewer");
    let (_, (_, all)) = photo_stitch(&dir, json!({"travel": "allTrims"})).expect("all");
    assert!(fewer <= all, "fewerTrims {fewer} vs allTrims {all}");
    let (_, (_, default)) = photo_stitch(&dir, json!({})).expect("default");
    assert_eq!(default, fewer, "Standard fidelity defaults to fewer trims");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn one_brand_chart_supplies_every_thread() {
    let dir = temp_dir("brand");
    write_photo(&dir);
    for (id, _, chart) in openstitch_core::brands::brand_charts() {
        let hexes: Vec<String> = chart.iter().map(|t| t.hex()).collect();
        let mut s = Session::new(&dir);
        let r = run(
            &mut s,
            "digitize.autoPunch",
            json!({"path": "photo.png", "width": 12, "colors": 3,
                   "compatMode": "openStitch", "threads": id}),
        );
        let chosen = r["colors"].as_array().expect("colors");
        assert!(!chosen.is_empty(), "{id}");
        for c in chosen {
            let c = c.as_str().expect("hex");
            assert!(
                hexes.iter().any(|h| h == c),
                "{id}: {c} is not in the chart"
            );
        }
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn photo_stitch_lists_the_threads_to_buy_against_an_owned_chart() {
    let dir = temp_dir("owned");
    write_photo(&dir);
    let mut s = Session::new(&dir);
    run(&mut s, "threads.createChart", json!({"name": "My Threads"}));
    run(
        &mut s,
        "threads.addColor",
        json!({"chart": "My Threads", "color": "#1e32be", "code": "1", "brand": "Mine"}),
    );
    // All brands, owning one blue: the threads chosen that are not owned are to buy.
    let r = run(
        &mut s,
        "digitize.photoStitch",
        json!({"path": "photo.png", "width": 12, "colors": 4, "compatMode": "openStitch",
               "threads": "all", "owned": "My Threads"}),
    );
    let chosen = r["colors"].as_array().expect("colors");
    let buy = r["toBuy"].as_array().expect("a shopping list");
    assert!(!chosen.is_empty());
    assert!(buy.len() <= chosen.len(), "{r}");
    assert!(!buy.is_empty(), "the reds and yellows are not owned: {r}");
    // Only the owned chart: nothing to buy.
    let mine = run(
        &mut s,
        "digitize.photoStitch",
        json!({"path": "photo.png", "width": 12, "colors": 4, "compatMode": "openStitch",
               "threads": "My Threads", "owned": "My Threads"}),
    );
    assert_eq!(mine["toBuy"], json!([]), "{mine}");
    for c in mine["colors"].as_array().expect("colors") {
        assert_eq!(c, "#1e32be");
    }
    let _ = std::fs::remove_dir_all(&dir);
}
