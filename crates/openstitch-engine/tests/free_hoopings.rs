//! Free multi-hooping (#515): `sections.set {mode: free}`, `sections.autoLayout`, whole objects
//! per hooping, files named "Name - hooping k of n", and unplaceable objects blocking export.

use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

/// A 220 × 100 mm banner: a square at each end and a word across the middle.
fn banner(dir: &TempDir) -> Session {
    let mut s = Session::new(dir.path());
    run(
        &mut s,
        "design.new",
        json!({"name": "Banner", "width": 220, "height": 100}),
    );
    for x in [10, 170] {
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": x, "y": 30, "width": 40, "height": 40, "fill": "Red"}),
        );
    }
    run(
        &mut s,
        "text.add",
        json!({"text": "AB", "size": 20, "x": 100, "y": 60}),
    );
    run(&mut s, "sections.set", json!({"hoop": "130x180"}));
    s
}

#[test]
fn auto_layout_places_two_hoopings_and_no_object_is_cut() {
    let dir = TempDir::new("free-hoop");
    let mut s = banner(&dir);
    let grid = run(&mut s, "sections.list", json!({}));
    assert_eq!(
        grid["settings"]["mode"], "grid",
        "PE-DESIGN's grid stays the default"
    );
    let r = run(&mut s, "sections.autoLayout", json!({}));
    assert_eq!(r["settings"]["mode"], "free");
    assert_eq!(r["errors"], json!([]), "{r}");
    let sections = r["sections"].as_array().expect("sections");
    assert_eq!(sections.len(), 2, "the fewest hoopings: {r}");
    // Every object in exactly one hooping, whole.
    let mut ids: Vec<u64> = sections
        .iter()
        .flat_map(|x| x["objects"].as_array().expect("objects").clone())
        .map(|v| v.as_u64().expect("id"))
        .collect();
    ids.sort_unstable();
    let mut all: Vec<u64> = s.design().objects.iter().map(|o| o.id).collect();
    all.sort_unstable();
    assert_eq!(ids, all);
    assert_eq!(
        sections[0]["alignmentAfter"],
        sections[1]["alignmentBefore"]
    );
    assert_eq!(sections[1]["stopAfterMarks"], json!(true));
    let out = run(&mut s, "sections.export", json!({"path": "banner.pes"}));
    let files = out["files"].as_array().expect("files");
    assert_eq!(files.len(), 2);
    assert!(
        dir.path().join("banner - hooping 1 of 2.pes").exists(),
        "{out}"
    );
    assert!(dir.path().join("banner - hooping 2 of 2.pes").exists());
    // Back to the grid: PE-DESIGN's sections again.
    let g = run(&mut s, "sections.set", json!({"mode": "grid"}));
    assert_eq!(g["sections"][0]["label"], "Aa");
}

#[test]
fn an_object_larger_than_any_hooping_blocks_export_with_its_name() {
    let dir = TempDir::new("free-hoop-big");
    let mut s = banner(&dir);
    run(&mut s, "sections.autoLayout", json!({}));
    let big = run(
        &mut s,
        "shape.rectangle",
        json!({"x": 20, "y": 80, "width": 180, "height": 10, "fill": "Blue"}),
    );
    let id = big["created"][0]
        .as_u64()
        .unwrap_or_else(|| s.design().objects.last().expect("object").id);
    let list = run(&mut s, "sections.list", json!({}));
    assert!(
        list["errors"][0]
            .as_str()
            .is_some_and(|e| e.contains(&format!("object {id}"))),
        "{list}"
    );
    let e = s
        .execute("sections.export", Some(&json!({"path": "b.pes"})))
        .expect_err("blocked");
    assert!(e.to_string().contains(&format!("object {id}")), "{e}");
    assert!(!dir.path().join("b - hooping 1 of 2.pes").exists());
}

#[test]
fn free_settings_are_checked_and_saved() {
    let dir = TempDir::new("free-hoop-set");
    let mut s = banner(&dir);
    let r = run(
        &mut s,
        "sections.set",
        json!({"mode": "free", "hoopings": [{"x": 5, "y": -35}, {"x": 95, "y": -35, "rotated": false}], "marks": "cross", "minOverlap": 20}),
    );
    assert_eq!(r["settings"]["marks"], "cross");
    assert_eq!(r["settings"]["hoopings"].as_array().map(Vec::len), Some(2));
    for bad in [
        json!({"mode": "spiral"}),
        json!({"mode": "free", "marks": "star"}),
        json!({"mode": "free", "hoopings": [{"x": 5}]}),
        json!({"mode": "grid", "marks": "cross"}),
    ] {
        assert!(s.execute("sections.set", Some(&bad)).is_err(), "{bad}");
    }
    run(&mut s, "project.save", json!({"path": "b.osd"}));
    let mut o = Session::new(dir.path());
    run(&mut o, "project.open", json!({"path": "b.osd"}));
    assert_eq!(o.design().machine.sections, s.design().machine.sections);
}
