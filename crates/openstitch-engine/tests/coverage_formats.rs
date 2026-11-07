//! Coverage for import and export claims no other test asserts (#329): SVG `<ellipse>`,
//! `<line>` (#341), `<polygon>`, `<polyline>` and CSS-styled paint, and the Text Transform preset and
//! level OpenStitch writes into a PE-DESIGN 11 (`#PES0110`) file.

#![cfg(feature = "formats")]

use openstitch_design::LineType;
use openstitch_engine::Session;
use openstitch_formats::pes;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp_dir(tag: &str) -> TempDir {
    TempDir::new(&format!("coverage-formats-{tag}"))
}

#[test]
fn svg_ellipse_polygon_polyline_and_css_paint_import_as_shapes() {
    let dir = temp_dir("svg");
    // 100 × 100 user units on a 100 mm page: one unit is 1 mm.
    std::fs::write(
        dir.join("e.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
  <style>.blue { fill: #0000ff; stroke: none }</style>
  <ellipse cx="20" cy="20" rx="10" ry="5" fill="#ff0000" stroke="none"/>
  <polygon points="10,60 30,60 20,80" style="fill:#00ff00;stroke:none"/>
  <polyline points="50,60 60,80 70,60 80,80" fill="none" stroke="#000000"/>
  <rect class="blue" x="40" y="30" width="20" height="10"/>
</svg>"##,
    )
    .expect("write");
    let mut s = Session::new(&dir);
    let out = run(
        &mut s,
        "import.svg",
        json!({"path": "e.svg", "exactColors": true}),
    );
    assert_eq!(out["ids"].as_array().map(Vec::len), Some(4), "{out}");
    let d = s.design();
    let o = &d.objects;
    assert_eq!(o.len(), 4);
    let bounds = |i: usize| o[i].bounds().expect("bounds");
    let fill = |i: usize| o[i].region.as_ref().map(|r| r.thread.hex());
    let near = |a: f64, b: f64| (a - b).abs() <= 2.0;

    // <ellipse>: a closed fill region, 20 × 10 mm.
    assert!(o[0].contours[0].closed);
    assert_eq!(fill(0).as_deref(), Some("#ff0000"));
    assert!(o[0].line.is_none(), "stroke none: no line");
    let b = bounds(0);
    assert!(near(b.width(), 200.0) && near(b.height(), 100.0), "{b:?}");

    // <polygon> with paint from a style attribute: a closed green region with 3 corners.
    assert!(o[1].contours[0].closed);
    assert_eq!(o[1].contours[0].points.len(), 3);
    assert_eq!(fill(1).as_deref(), Some("#00ff00"));
    assert!(o[1].line.is_none());

    // <polyline> with fill none: an open line through its 4 points.
    assert!(!o[2].contours[0].closed);
    assert_eq!(o[2].contours[0].points.len(), 4);
    assert!(o[2].region.is_none());
    assert!(o[2].line.is_some());

    // Paint from a <style> class.
    assert_eq!(fill(3).as_deref(), Some("#0000ff"));
    assert!(o[3].line.is_none());
    let b = bounds(3);
    assert!(near(b.width(), 200.0) && near(b.height(), 100.0), "{b:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn svg_line_elements_import_as_running_lines() {
    let dir = temp_dir("svg-line");
    std::fs::write(
        dir.join("l.svg"),
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="100mm" height="100mm" viewBox="0 0 100 100">
  <line x1="50" y1="10" x2="90" y2="10" stroke="#000000"/>
  <line x1="10" y1="10" x2="40" y2="50" stroke="#000000"/>
  <line x1="10" y1="90" x2="10" y2="60" stroke="#ff0000" stroke-width="2"/>
  <path d="M 60 90 L 95 90" stroke="#000000" fill="none"/>
</svg>"##,
    )
    .expect("write");
    let mut s = Session::new(&dir);
    let out = run(&mut s, "import.svg", json!({"path": "l.svg"}));
    assert_eq!(out["ids"].as_array().map(Vec::len), Some(4), "{out}");
    for o in &s.design().objects {
        assert!(!o.contours[0].closed && o.region.is_none());
        assert_eq!(o.line.as_ref().map(|l| l.kind), Some(LineType::Running));
    }
    let b = s.design().objects[0].bounds().expect("bounds");
    assert!((b.width() - 400.0).abs() <= 2.0, "the 40 mm line: {b:?}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn text_transform_preset_and_level_are_written_to_pes_0110() {
    let dir = temp_dir("envelope");
    // Preset → (PE-DESIGN's code, default strength), from the reference files read by
    // `text_transform_presets_read`: preset 1 is code 24 at 30, 3 is 0 at 50, 26 is 15 at 50.
    let cases = [
        (1, None, 24, 30.0),
        (3, None, 0, 50.0),
        (3, Some(80.0), 0, 80.0),
        (26, Some(10.0), 15, 10.0),
    ];
    for (preset, level, code, strength) in cases {
        let mut s = Session::new(&dir);
        let mut params = json!({"text": "HHH", "size": 10, "x": 50, "y": 50, "envelope": preset});
        if let Some(l) = level {
            params["envelopeLevel"] = json!(l);
        }
        run(&mut s, "text.add", params);
        run(
            &mut s,
            "design.export",
            json!({"path": "t.pes", "pesVersion": "11"}),
        );
        let section =
            pes::read_objects(&std::fs::read(dir.join("t.pes")).expect("read")).expect("objects");
        assert_eq!(section.version, "0110");
        assert_eq!(section.texts.len(), 1, "preset {preset}");
        let e = section.texts[0].envelope.expect("CEmbDText");
        assert_eq!(
            (e.code, e.strength),
            (code, strength as f32),
            "preset {preset} level {level:?}"
        );
    }

    // Without a preset, no envelope is written.
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "text.add",
        json!({"text": "HHH", "size": 10, "x": 50, "y": 50}),
    );
    run(
        &mut s,
        "design.export",
        json!({"path": "plain.pes", "pesVersion": "11"}),
    );
    let section =
        pes::read_objects(&std::fs::read(dir.join("plain.pes")).expect("read")).expect("objects");
    assert!(section.texts[0].envelope.is_none());
    let _ = std::fs::remove_dir_all(&dir);
}

/// VP3 (#480) goes through the same `design.export` and `stitches.import` as every format:
/// the export check reports the hoop, and the file opens again with the same stitches.
#[test]
fn vp3_exports_and_imports_through_the_engine() {
    let dir = temp_dir("vp3");
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20}),
    );
    let out = run(&mut s, "design.export", json!({"path": "a.vp3"}));
    assert!(out["hoopMM"].is_array(), "{out}");
    assert_eq!(out["fitsPage"], json!(true), "{out}");
    let written = out["stats"]["stitches"].as_u64().expect("stitches");
    let bytes = std::fs::read(dir.join("a.vp3")).expect("vp3");
    assert!(bytes.starts_with(b"%vsm%"));
    let pattern = openstitch_formats::vp3::read(&bytes).expect("read");
    assert_eq!(
        pattern.count(openstitch_core::Command::Stitch) as u64,
        written,
        "{out}"
    );
    let mut t = Session::new(&dir);
    let imported = run(&mut t, "stitches.import", json!({"path": "a.vp3"}));
    assert!(imported["id"].is_u64(), "{imported}");
    assert_eq!(t.design().objects.len(), 1);
}

/// TAP, U01 and TBF (#481) export through `design.export` and import again; DSB, DSZ and KSM
/// are read only, and exporting to them names the formats that can be written.
#[test]
fn more_machine_formats_export_and_import_through_the_engine() {
    let dir = temp_dir("more-formats");
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 20, "height": 20}),
    );
    for ext in ["tap", "u01", "tbf"] {
        let name = format!("a.{ext}");
        let out = run(&mut s, "design.export", json!({"path": name}));
        assert!(out["hoopMM"].is_array(), "{out}");
        let ty = openstitch_formats::FileType::from_extension(ext).expect("type");
        let pattern = ty
            .read(&std::fs::read(dir.join(&name)).expect("file"))
            .expect("read");
        assert_eq!(
            pattern.count(openstitch_core::Command::Stitch) as u64,
            out["stats"]["stitches"].as_u64().expect("stitches"),
            "{ext}"
        );
        let mut t = Session::new(&dir);
        let imported = run(&mut t, "stitches.import", json!({"path": name}));
        assert!(imported["id"].is_u64(), "{imported}");
    }
    for ext in ["dsb", "dsz", "ksm", "zxy", "stx"] {
        let err = s
            .execute("design.export", Some(&json!({"path": format!("a.{ext}")})))
            .expect_err("read only");
        assert!(err.to_string().contains("read only"), "{err}");
    }
}

/// #595: the read-only ZXY and STX files open through `stitches.import` (the command the
/// Assistant uses), every needle drop of the independent reader's oracle kept.
#[test]
fn zxy_and_stx_open_through_stitches_import() {
    let fixtures = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../openstitch-formats/tests/fixtures");
    for ext in ["zxy", "stx"] {
        let dir = temp_dir(&format!("import-{ext}"));
        let name = format!("oracle.{ext}");
        std::fs::copy(fixtures.join(&name), dir.join(&name)).expect("fixture");
        let oracle: Value = serde_json::from_str(
            &std::fs::read_to_string(fixtures.join(format!("oracle_{ext}_read.json")))
                .expect("oracle"),
        )
        .expect("oracle json");
        let want = oracle["stitches"]
            .as_array()
            .expect("stitches")
            .iter()
            .filter(|s| s[2] == "stitch")
            .count();
        let mut s = Session::new(&dir);
        let r = run(&mut s, "stitches.import", json!({"path": name}));
        let id = r["id"].as_u64().expect("id");
        let d = s.design();
        let block = d.objects[d.index_of(id).expect("object")]
            .stitch_block
            .as_ref()
            .expect("a stitch block");
        let got = block
            .stitches
            .iter()
            .filter(|st| st.command == openstitch_core::Command::Stitch)
            .count();
        assert_eq!(got, want, "{ext}");
    }
}

/// #596: `library.convert`'s format help names every format OpenStitch writes.
#[cfg(feature = "library")]
#[test]
fn library_convert_help_names_every_writable_format() {
    let s = Session::new(std::env::temp_dir());
    let spec = s
        .commands()
        .into_iter()
        .find(|c| c.id == "library.convert")
        .expect("library.convert");
    let help = spec
        .params
        .iter()
        .find(|p| p.name == "format")
        .expect("format")
        .summary;
    for t in openstitch_formats::FileType::writable() {
        assert!(
            help.contains(t.extension()),
            "{} missing from: {help}",
            t.extension()
        );
    }
}
