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

/// #455: the long key names (`angle=`, `spacing=`, `density=`, `width=`, `length=`,
/// `repeats=`, ...) import exactly as the short ones do, and the settings really apply. The SVG
/// is test input written here (each element needs an `id` for its name to be read).
#[test]
fn long_key_names_import_like_the_short_ones() {
    let svg = |fill: &str, satin: &str, run: &str| {
        format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" width="80mm" height="80mm" viewBox="0 0 80 80">
              <rect id="r1" inkscape:label="{fill}" x="0" y="0" width="30" height="30" fill="#2060c0"/>
              <path id="p1" inkscape:label="{satin}" d="M 40 10 L 75 10" stroke="#c02020" stroke-width="3" fill="none"/>
              <path id="p2" inkscape:label="{run}" d="M 0 60 L 70 60" stroke="#000000" fill="none"/>
            </svg>"##
        )
    };
    let import = |text: String| {
        let dir = TempDir::new("svg-settings-long");
        std::fs::write(dir.join("a.svg"), text).expect("write");
        let mut s = Session::new(&dir);
        let r = run(
            &mut s,
            "import.svg",
            json!({"path": "a.svg", "settings": true}),
        );
        assert_eq!(r["settings"]["unknown"], json!([]), "{r}");
        assert_eq!(r["settings"]["invalid"], json!([]), "{r}");
        s.design().objects.clone()
    };
    let long = import(svg(
        "fill angle=30 spacing=0.4 pullCompensation=0.3",
        "stroke.satin stroke.width=3",
        "run repeats=3 length=2.5",
    ));
    let short = import(svg(
        "fill a=30 s=0.4 pc=0.3",
        "stroke.satin stroke.w=3",
        "run r=3 l=2.5",
    ));
    assert_eq!(long.len(), 3);
    for (a, b) in long.iter().zip(&short) {
        assert_eq!(a.region, b.region);
        assert_eq!(a.line, b.line);
    }
    let fill = long[0].region.as_ref().expect("fill");
    assert!(
        (fill.angle - 30.0).abs() < 1e-9 && (fill.density - 2.5).abs() < 1e-9,
        "angle {}, density {}",
        fill.angle,
        fill.density
    );
    assert_eq!(
        long[1].line.as_ref().map(|l| l.kind),
        Some(LineType::Zigzag)
    );
    let line = long[2].line.as_ref().expect("run");
    assert_eq!((line.kind, line.run_times), (LineType::Running, 3));
}
