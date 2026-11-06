//! Free text envelopes (#535) and Ink/Stitch-format font folders (#529) through engine
//! commands.

use openstitch_core::Stitch;
use openstitch_design::compile::Compiler;
use openstitch_design::geometry::{Vec2, v2};
use openstitch_design::{ObjectKind, RegionType};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn compiled(s: &Session) -> Vec<Stitch> {
    Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
}

fn handles(s: &mut Session, id: u64) -> Value {
    run(s, "text.envelopeHandles", json!({"id": id}))
}

fn xy(v: &Value) -> Vec2 {
    v2(v[0].as_f64().expect("x"), v[1].as_f64().expect("y"))
}

#[test]
fn a_preset_as_a_free_envelope_sews_the_same_stitches() {
    let dir = TempDir::new("envelopes");
    for n in 3..=26 {
        for level in [None, Some(35.0)] {
            let mut s = Session::new(&dir);
            let mut p = json!({"text": "OpenStitch", "size": 10, "envelope": n});
            if let Some(l) = level {
                p["envelopeLevel"] = json!(if matches!(n, 9 | 10) { 15.0 } else { l });
            }
            let id = run(&mut s, "text.add", p)["id"].as_u64().expect("id");
            let before = compiled(&s);
            run(
                &mut s,
                "text.setEnvelope",
                json!({"ids": [id], "shape": "preset"}),
            );
            let t = s.design().objects[0].text.clone().expect("text");
            assert!(t.envelope.is_none() && t.custom_envelope.is_some());
            assert_eq!(compiled(&s), before, "preset {n} at {level:?}");
        }
    }
    let mut s = Session::new(&dir);
    let id = run(&mut s, "text.add", json!({"text": "Arc", "envelope": 1}))["id"]
        .as_u64()
        .expect("id");
    let e = s
        .execute(
            "text.setEnvelope",
            Some(&json!({"ids": [id], "shape": "preset"})),
        )
        .expect_err("arches have no envelope");
    assert!(e.to_string().contains("arches"), "{e}");
}

#[test]
fn handles_bend_the_envelope_and_shift_keeps_it_symmetric() {
    let dir = TempDir::new("envelopes-bend");
    let mut s = Session::new(&dir);
    let id = run(&mut s, "text.add", json!({"text": "HHHHHH", "size": 10}))["id"]
        .as_u64()
        .expect("id");
    for shape in ["bridge", "pennant", "perspective", "diamond"] {
        run(
            &mut s,
            "text.setEnvelope",
            json!({"ids": [id], "shape": shape}),
        );
        assert!(!compiled(&s).is_empty(), "{shape}");
    }
    run(
        &mut s,
        "text.setEnvelope",
        json!({"ids": [id], "shape": "none"}),
    );
    assert!(
        s.design().objects[0]
            .text
            .as_ref()
            .expect("t")
            .custom_envelope
            .is_none()
    );
    run(
        &mut s,
        "text.setEnvelope",
        json!({"ids": [id], "shape": "bridge", "amount": 0}),
    );
    let h0 = handles(&mut s, id);
    // The middle of the top edge 3 mm up.
    run(
        &mut s,
        "text.bendEnvelope",
        json!({"id": id, "edge": "top", "handle": "middle", "dy": -3}),
    );
    let h1 = handles(&mut s, id);
    let moved = xy(&h1["top"]["middle"]) - xy(&h0["top"]["middle"]);
    assert!(
        moved.x.abs() < 1e-6 && (moved.y + 3.0).abs() < 1e-6,
        "{moved:?}"
    );
    // The ends stay.
    assert_eq!(h1["top"]["left"], h0["top"]["left"]);
    // Shift-drag of the bottom left handle: the right one mirrors it.
    run(
        &mut s,
        "text.bendEnvelope",
        json!({"id": id, "edge": "bottom", "handle": "left", "dx": 2, "dy": 1.5, "symmetric": true}),
    );
    let h2 = handles(&mut s, id);
    let l = xy(&h2["bottom"]["left"]) - xy(&h1["bottom"]["left"]);
    let r = xy(&h2["bottom"]["right"]) - xy(&h1["bottom"]["right"]);
    assert!(
        (l.x - 2.0).abs() < 1e-6 && (l.y - 1.5).abs() < 1e-6,
        "{l:?}"
    );
    assert!(
        (r.x + 2.0).abs() < 1e-6 && (r.y - 1.5).abs() < 1e-6,
        "{r:?}"
    );
    // The letters stay inside the envelope (capitals: no descenders), within 0.3 mm.
    let spec = s.design().objects[0].text.clone().expect("text");
    let [top, bottom] =
        openstitch_design::text::envelope_outline(&spec, &s.design().fonts, 200).expect("edges");
    let y_at = |edge: &[Vec2], x: f64| {
        edge.windows(2)
            .find(|w| (w[0].x - x) * (w[1].x - x) <= 0.0)
            .map(|w| {
                let t = if (w[1].x - w[0].x).abs() < 1e-9 {
                    0.0
                } else {
                    (x - w[0].x) / (w[1].x - w[0].x)
                };
                w[0].y + t * (w[1].y - w[0].y)
            })
    };
    for c in &s.design().objects[0].contours {
        for p in &c.points {
            if let (Some(t), Some(b)) = (y_at(&top, p.x), y_at(&bottom, p.x)) {
                assert!(p.y >= t - 3.0 && p.y <= b + 3.0, "{p:?} outside {t}..{b}");
            }
        }
    }
    // Choosing a preset replaces the free envelope; PES 0110 refuses a free one.
    run(
        &mut s,
        "text.setEnvelope",
        json!({"ids": [id], "shape": "diamond"}),
    );
    let e = s
        .execute(
            "design.export",
            Some(&json!({"path": "e.pes", "pesVersion": "11"})),
        )
        .expect_err("PES 0110 holds only presets");
    assert!(e.to_string().contains("free text envelope"), "{e}");
    run(
        &mut s,
        "object.setText",
        json!({"ids": [id], "envelope": 5}),
    );
    assert!(
        s.design().objects[0]
            .text
            .as_ref()
            .expect("t")
            .custom_envelope
            .is_none()
    );
    let e = s
        .execute(
            "text.bendEnvelope",
            Some(&json!({"id": id, "edge": "top", "handle": "left", "dy": 1})),
        )
        .expect_err("no free envelope");
    assert!(e.to_string().contains("text.setEnvelope"), "{e}");
}

const TINY_JSON: &str = r#"{
    "name": "Test Tiny",
    "font_license": "SIL Open Font License 1.1",
    "leading": 100,
    "min_scale": 0.5,
    "max_scale": 2.0,
    "horiz_adv_x": {"H": 60, "e": 40, "l": 18, "o": 40},
    "horiz_adv_x_space": 20,
    "kerning_pairs": {"H e": 5, "x y z": 3},
    "glyphs": ["H", "e", "l", "o"]
}"#;

/// A font made for this test: an H of three satin columns, a filled e and o, a running l, and
/// a two-character layer (a ligature) that is left out.
const TINY_SVG: &str = r##"<?xml version="1.0" encoding="UTF-8"?>
<svg width="100" height="100" viewBox="0 0 100 100"
   xmlns="http://www.w3.org/2000/svg"
   xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"
   xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd"
   xmlns:inkstitch="http://inkstitch.org/namespace">
  <sodipodi:namedview id="base">
    <sodipodi:guide position="0,20" orientation="0,1" inkscape:label="baseline" id="g1"/>
    <sodipodi:guide position="0,80" orientation="0,1" inkscape:label="caps" id="g2"/>
  </sodipodi:namedview>
  <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-H" style="display:none" id="lH">
    <path id="h1" d="M 10,20 L 10,80 M 18,20 L 18,80 M 8,50 L 20,50" style="fill:none;stroke:#000000"
       inkstitch:satin_column="True" inkstitch:zigzag_spacing_mm="0.4"/>
    <path id="h2" d="M 42,20 L 42,80 M 50,20 L 50,80" style="fill:none;stroke:#000000"
       inkstitch:satin_column="True"/>
    <path id="h3" d="M 18,46 L 42,46 M 18,54 L 42,54" style="fill:none;stroke:#000000"/>
  </g>
  <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-e" style="display:none" id="le">
    <path id="e1" d="M 5,80 L 35,80 L 35,50 L 5,50 Z" style="fill:#000000" inkstitch:angle="30"/>
  </g>
  <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-l" style="display:none" id="ll">
    <path id="l1" d="M 8,20 L 8,80" style="fill:none;stroke:#000000"/>
  </g>
  <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-o" style="display:none" id="lo">
    <path id="o1" d="M 5,50 L 35,50 L 35,80 L 5,80 Z M 12,57 L 28,57 L 28,73 L 12,73 Z" style="fill:#000000;fill-rule:evenodd"/>
  </g>
  <g inkscape:groupmode="layer" inkscape:label="GlyphLayer-fi" style="display:none" id="lfi">
    <path id="f1" d="M 5,80 L 35,80 L 35,50 Z" style="fill:#000000"/>
  </g>
</svg>
"##;

#[test]
fn an_ink_stitch_font_folder_loads_sews_and_survives_its_removal() {
    let dir = TempDir::new("inkstitch");
    let folder = dir.join("fonts").join("test_tiny");
    std::fs::create_dir_all(&folder).expect("folder");
    std::fs::write(folder.join("font.json"), TINY_JSON).expect("json");
    std::fs::write(folder.join("ltr.svg"), TINY_SVG).expect("svg");
    std::fs::write(folder.join("LICENSE"), "SIL Open Font License 1.1 (test)").expect("lic");
    std::fs::create_dir_all(dir.join("fonts").join("not_a_font")).expect("other");
    let mut s = Session::new(&dir);
    let scan = run(&mut s, "fonts.scanFolder", json!({"path": "fonts"}));
    assert_eq!(scan.as_array().expect("rows").len(), 1, "{scan}");
    assert_eq!(scan[0]["name"], "Test Tiny");
    let r = run(
        &mut s,
        "fonts.importInkStitch",
        json!({"path": "fonts/test_tiny"}),
    );
    assert_eq!(r["id"], "test-tiny");
    assert_eq!(
        r["sewing"], "satin",
        "three satin pieces outnumber the rest"
    );
    assert!(
        r["notRead"]["zigzag_spacing_mm"] == 1 && r["notRead"]["angle"] == 1,
        "{r}"
    );
    assert_eq!(r["skipped"], json!(["fi"]));
    assert_eq!(r["kerningPairs"], 1, "the pair of three names is left out");
    assert!(r["license"].as_str().expect("licence").contains("SIL"));
    // Capitals 60 px tall: 15.875 mm, so min_scale 0.5 .. max_scale 2.
    let cap = 60.0 * 25.4 / 96.0;
    assert!((r["heightMM"].as_f64().expect("h") - cap).abs() < 1e-9);
    assert!((r["sizeRange"]["minMm"].as_f64().expect("lo") - cap * 0.5).abs() < 1e-9);

    let t = run(
        &mut s,
        "text.add",
        json!({"text": "Hello", "font": "test-tiny", "size": cap, "align": "left", "x": 0, "y": 50}),
    );
    assert!(t.get("sizeWarning").is_none(), "{t}");
    let o = s.design().objects[0].clone();
    assert_eq!(o.kind, ObjectKind::Text);
    assert_eq!(o.region.as_ref().expect("region").kind, RegionType::Satin);
    let groups = o.glyph_groups();
    // The H's satin columns are three closed outlines between their rails, not fills of the
    // strokes; the l is an open line.
    assert_eq!(groups[0].len(), 3);
    assert!(groups[0].iter().all(|c| c.closed));
    assert!(groups[2].iter().all(|c| !c.closed));
    // Kerning "H e" 5 px: the e starts at 60 - 5 + 5 (its own left edge) = 60 px.
    let px = 254.0 / 96.0;
    let e_left = groups[1]
        .iter()
        .flat_map(|c| &c.points)
        .map(|p| p.x)
        .fold(f64::MAX, f64::min);
    assert!((e_left - 60.0 * px).abs() < 0.01, "{e_left}");
    assert!(!compiled(&s).is_empty());
    let w = run(&mut s, "object.setText", json!({"ids": [o.id], "size": 40}));
    assert!(w.is_object());
    let again = run(
        &mut s,
        "text.add",
        json!({"text": "He", "font": "test-tiny", "size": 50}),
    );
    assert!(
        again["sizeWarning"]
            .as_str()
            .expect("warning")
            .contains("Test Tiny")
    );

    // The folder goes: a rescan no longer lists it, and the design keeps its copy.
    std::fs::remove_dir_all(&folder).expect("remove");
    let scan = run(&mut s, "fonts.scanFolder", json!({"path": "fonts"}));
    assert!(scan.as_array().expect("rows").is_empty());
    let list = run(&mut s, "fonts.list", json!({}));
    assert!(
        list.as_array()
            .expect("rows")
            .iter()
            .any(|r| r["id"] == "test-tiny")
    );
    run(
        &mut s,
        "object.setText",
        json!({"ids": [o.id], "text": "oh"}),
    );
    assert!(!compiled(&s).is_empty());

    // A broken folder is refused with the reason.
    std::fs::create_dir_all(dir.join("bad")).expect("bad");
    std::fs::write(dir.join("bad").join("font.json"), "{\"name\": \"Bad\"}").expect("json");
    std::fs::write(
        dir.join("bad").join("ltr.svg"),
        "<svg xmlns='http://www.w3.org/2000/svg'/>",
    )
    .expect("svg");
    let e = s
        .execute("fonts.importInkStitch", Some(&json!({"path": "bad"})))
        .expect_err("no baseline");
    assert!(e.to_string().contains("baseline"), "{e}");
}
