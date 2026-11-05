//! Other brands' hoops and round hoops (#514): grouped lists, selection, rotation, the
//! hoop-fit check on oval and round fields, and the JEF hoop code.

use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn hoops_are_grouped_by_brand_with_brother_first() {
    let mut s = Session::new(std::env::temp_dir());
    let r = run(&mut s, "machine.hoops", json!({}));
    let groups = r["groups"].as_array().expect("groups");
    assert_eq!(groups[0]["brand"], "Brother");
    assert_eq!(
        groups[0]["hoops"], r["brother"],
        "Brother's list is unchanged"
    );
    for brand in [
        "Janome",
        "Bernina",
        "Husqvarna Viking",
        "Pfaff",
        "Baby Lock",
        "Elna",
        "Singer",
        "Commercial round",
    ] {
        let g = groups
            .iter()
            .find(|g| g["brand"] == brand)
            .unwrap_or_else(|| panic!("{brand}"));
        for h in g["hoops"].as_array().expect("hoops") {
            assert!(h["widthMM"].as_f64().is_some_and(|w| w > 0.0), "{h}");
            assert!(h["source"].as_str().is_some_and(|s| s.len() > 10), "{h}");
            assert_eq!(h["brand"], brand);
        }
    }
    let round = run(
        &mut s,
        "machine.hoops",
        json!({"brand": "commercial round"}),
    );
    let only = round["groups"].as_array().expect("groups");
    assert_eq!(only.len(), 1);
    let r15 = only[0]["hoops"]
        .as_array()
        .expect("hoops")
        .iter()
        .find(|h| h["id"] == "round-15cm")
        .expect("15 cm")
        .clone();
    assert_eq!(r15["shape"], "round");
    assert_eq!(r15["insideDiameterMM"], json!(150.0));
    assert_eq!(r15["widthMM"], json!(120.0));
    assert_eq!(r15["canRotate"], json!(false));
    assert!(
        s.execute("machine.hoops", Some(&json!({"brand": "Acme"})))
            .is_err()
    );
}

#[test]
fn another_brands_hoop_sets_the_design_page_and_rotates_where_it_can() {
    let mut s = Session::new(std::env::temp_dir());
    assert_eq!(s.design().machine.hoop, None, "custom page by default");
    let r = run(&mut s, "machine.set", json!({"hoop": "janome-re20b"}));
    assert_eq!(r["brand"], "Janome");
    assert_eq!(
        (r["pageWidthMM"].clone(), r["pageHeightMM"].clone()),
        (json!(140.0), json!(200.0))
    );
    let r = run(&mut s, "machine.set", json!({"rotate90": true}));
    assert_eq!(
        (r["pageWidthMM"].clone(), r["pageHeightMM"].clone()),
        (json!(200.0), json!(140.0))
    );
    // A round hoop drops the rotation and refuses it.
    let r = run(&mut s, "machine.set", json!({"hoop": "round-18cm"}));
    assert_eq!(r["rotate90"], json!(false));
    assert_eq!(r["shape"], "round");
    assert_eq!(r["pageWidthMM"], json!(150.0));
    let e = s
        .execute("machine.set", Some(&json!({"rotate90": true})))
        .expect_err("round hoops do not rotate");
    assert!(e.to_string().contains("round"), "{e}");
    let r = run(
        &mut s,
        "machine.set",
        json!({"hoop": "bernina-large-oval", "rotate90": true}),
    );
    assert_eq!(r["shape"], "oval");
    assert_eq!(r["pageWidthMM"], json!(255.0));
    // Brother stays the default list; its hoops behave as before.
    let r = run(
        &mut s,
        "machine.set",
        json!({"hoop": "130x180", "rotate90": false}),
    );
    assert_eq!(r["brand"], "Brother");
    assert_eq!(r["shape"], "rectangle");
}

#[test]
fn the_hoop_fit_check_uses_an_oval_or_round_field() {
    let dir = TempDir::new("hoop-oval");
    let mut s = Session::new(dir.path());
    run(&mut s, "machine.set", json!({"hoop": "bernina-large-oval"}));
    // A square in a corner of the 145 × 255 mm page: inside the page, outside the oval.
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 2, "y": 2, "width": 20, "height": 20}),
    );
    let r = run(&mut s, "design.export", json!({"path": "corner.dst"}));
    assert_eq!(r["fitsPage"], json!(false), "{r}");
    let w = r["warnings"][0].as_str().expect("warning");
    assert!(
        w.contains("oval hoop field") && w.contains("Bernina"),
        "{w}"
    );
    // The same square in the middle fits.
    let mut c = Session::new(dir.path());
    run(&mut c, "machine.set", json!({"hoop": "bernina-large-oval"}));
    run(
        &mut c,
        "shape.rectangle",
        json!({"x": 62, "y": 117, "width": 20, "height": 20}),
    );
    let r = run(&mut c, "design.export", json!({"path": "middle.dst"}));
    assert_eq!(r["fitsPage"], json!(true), "{r}");
    assert_eq!(r["warnings"], json!([]));
    // The smallest hoop comes from the chosen hoop's brand.
    assert_eq!(r["smallestHoop"], json!("bernina-medium"), "{r}");
}

/// JEF's header hoop code (offset 32): a Janome hoop with a JEF code writes it; without one the
/// writer still picks the smallest code the design fits.
#[test]
fn a_janome_hoop_writes_its_jef_hoop_code() {
    let dir = TempDir::new("hoop-jef");
    let code = |hoop: Option<&str>, name: &str| {
        let mut s = Session::new(dir.path());
        if let Some(h) = hoop {
            run(&mut s, "machine.set", json!({"hoop": h}));
        }
        run(
            &mut s,
            "shape.rectangle",
            json!({"x": 30, "y": 30, "width": 20, "height": 20}),
        );
        run(&mut s, "design.export", json!({"path": name}));
        let b = std::fs::read(dir.path().join(name)).expect("jef");
        u32::from_le_bytes([b[32], b[33], b[34], b[35]])
    };
    assert_eq!(code(None, "plain.jef"), 1, "50 × 50 fits the 20 mm square");
    assert_eq!(
        code(Some("100x100"), "brother.jef"),
        1,
        "Brother hoops: unchanged"
    );
    assert_eq!(code(Some("janome-re20b"), "re20b.jef"), 2, "140 × 200");
    assert_eq!(code(Some("elna-sq20b"), "sq20b.jef"), 4, "200 × 200");
    assert_eq!(
        code(Some("janome-sq14b"), "sq14b.jef"),
        1,
        "no JEF code for 140 × 140"
    );
}
