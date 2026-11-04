//! Coverage for import and export claims no other test asserts (#329): SVG `<ellipse>`,
//! `<line>` (#341), `<polygon>`, `<polyline>` and CSS-styled paint, and the Text Transform preset and
//! level OpenStitch writes into a PE-DESIGN 11 (`#PES0110`) file.

#![cfg(feature = "formats")]

use openstitch_design::LineType;
use openstitch_engine::Session;
use openstitch_formats::pes;
use serde_json::{Value, json};
use std::path::PathBuf;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn temp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "openstitch-coverage-formats-{tag}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).expect("temp dir");
    dir
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
