//! Stitch settings from SVG layer and object names (#455): `import.svg {settings: true}`.
#![cfg(feature = "formats")]

use openstitch_design::{LineType, RegionType};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

const MM: f64 = 10.0;

fn session() -> (TempDir, Session) {
    let dir = TempDir::new("svg-settings");
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/svg/named-layers.svg");
    std::fs::copy(src, dir.join("named.svg")).expect("copy");
    let s = Session::new(&dir);
    (dir, s)
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

#[test]
fn layer_and_object_names_set_the_sewing() {
    let (_dir, mut s) = session();
    let r = run(
        &mut s,
        "import.svg",
        json!({"path": "named.svg", "settings": true, "exactColors": true}),
    );
    let set = &r["settings"];
    // skip_me is not imported; the 5 dashes are 5 lines; the bordered square is two objects,
    // the border first.
    assert_eq!(set["skipped"], json!(["skip"]));
    assert_eq!(set["dashed"], json!(1));
    assert_eq!(set["unknown"], json!([{"id": "path30", "token": "wobble"}]));
    assert_eq!(set["invalid"], json!([]));
    let d = s.design();
    assert_eq!(d.objects.len(), 3 + 5 + 2 + 1);
    // Layer 1: fill at 45°, rows 0.4 mm apart.
    let fill = d.objects[0].region.as_ref().expect("fill");
    assert_eq!(fill.kind, RegionType::Fill);
    assert!((fill.angle - 45.0).abs() < 1e-9, "{}", fill.angle);
    assert!((fill.density - 2.5).abs() < 1e-9, "{}", fill.density);
    // Layer 2: satin.
    assert_eq!(
        d.objects[1].region.as_ref().map(|r| r.kind),
        Some(RegionType::Satin)
    );
    // Layer 3: running stitch, three passes.
    let run3 = d.objects[2].line.as_ref().expect("line");
    assert_eq!((run3.kind, run3.run_times), (LineType::Running, 3));
    // Dashes: five running lines of 10 mm.
    for o in &d.objects[3..8] {
        let b = o.bounds().expect("bounds");
        assert!((b.width() - 10.0 * MM).abs() < 0.01, "{}", b.width());
        assert!(o.line.is_some() && o.region.is_none());
    }
    // paint-order: stroke fill: the outline sews before the fill.
    assert!(d.objects[8].line.is_some() && d.objects[8].region.is_none());
    assert_eq!(
        d.objects[9].region.as_ref().map(|r| r.kind),
        Some(RegionType::Fill)
    );
    assert!(d.objects[9].line.is_none());
    // stroke.satin w=3: a satin (zigzag) line 3 mm wide.
    let band = d.objects[10].line.as_ref().expect("satin line");
    assert_eq!(band.kind, LineType::Zigzag);
    let mut reference = Session::new(_dir.path());
    run(
        &mut reference,
        "shape.path",
        json!({"points": [[0, 0], [10, 0]], "closed": false}),
    );
    run(
        &mut reference,
        "object.setLine",
        json!({"ids": [1], "type": "zigzag", "width": 3}),
    );
    let want = reference.design().objects[0]
        .line
        .as_ref()
        .expect("l")
        .width;
    assert!((band.width - want).abs() < 1e-9, "{} vs {want}", band.width);
    // One undo step removes it all.
    run(&mut s, "history.undo", json!({}));
    assert!(s.design().objects.is_empty());
}

/// Without `settings` the import is what it always was: names are not read, dashes and
/// paint-order are not followed.
#[test]
fn default_import_ignores_names() {
    let (_dir, mut s) = session();
    let r = run(
        &mut s,
        "import.svg",
        json!({"path": "named.svg", "exactColors": true}),
    );
    assert!(r.get("settings").is_none());
    let d = s.design();
    assert_eq!(d.objects.len(), 7);
    // The layer named "Fill a=45 s=0.4" gets the default fill, as any shape does.
    let fill = d.objects[0].region.clone().expect("fill");
    let default = openstitch_design::RegionSewing::new(RegionType::Fill, fill.thread.clone());
    assert_eq!(fill, default);
    assert_eq!(
        d.objects[1].region.as_ref().map(|r| r.kind),
        Some(RegionType::Fill)
    );
    let bordered = &d.objects[5];
    assert!(bordered.region.is_some() && bordered.line.is_some());
}

#[test]
fn refused_values_are_listed_and_the_rest_applies() {
    let dir = TempDir::new("svg-settings");
    std::fs::write(
        dir.join("bad.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="50mm" height="50mm" viewBox="0 0 50 50">
          <rect id="fill_a_x3D_30_s_x3D_0" x="0" y="0" width="20" height="20" fill="#ff0000"/>
          <path id="run_r_x3D_9" d="M 0 40 L 40 40" stroke="#000000" fill="none"/>
        </svg>"##,
    )
    .expect("write");
    let mut s = Session::new(&dir);
    let r = run(
        &mut s,
        "import.svg",
        json!({"path": "bad.svg", "settings": true}),
    );
    let invalid: Vec<&str> = r["settings"]["invalid"]
        .as_array()
        .expect("invalid")
        .iter()
        .map(|v| v["token"].as_str().expect("token"))
        .collect();
    assert_eq!(invalid, ["s=0", "r=9"]);
    let fill = s.design().objects[0].region.as_ref().expect("fill");
    assert!((fill.angle - 30.0).abs() < 1e-9, "a=30 still applies");
}
