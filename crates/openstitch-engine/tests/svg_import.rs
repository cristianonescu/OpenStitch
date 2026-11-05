//! SVG import of real-world artwork (#487): clip paths, masks, opacity, gradients, and the
//! report of what was left out or approximated. Fixtures in tests/fixtures/svg/ (written for
//! these tests).
#![cfg(feature = "formats")]

use openstitch_design::Rect;
use openstitch_engine::Session;
use openstitch_engine::imports::{Gradients, SvgImport, SvgOptions, parse_svg, parse_svg_with};
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

/// One SVG unit (CSS pixel) in 0.1 mm.
const PX: f64 = 254.0 / 96.0;

fn fixture(name: &str) -> Vec<u8> {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/svg")
        .join(name);
    std::fs::read(p).expect("fixture")
}

fn import(name: &str, opts: &SvgOptions) -> SvgImport {
    parse_svg_with(&fixture(name), opts).unwrap_or_else(|e| panic!("{name}: {e}"))
}

fn bounds(i: &SvgImport, k: usize) -> Rect {
    Rect::enclosing(
        i.shapes[k]
            .contours
            .iter()
            .flat_map(openstitch_design::Contour::outline_points),
    )
    .expect("bounds")
}

fn kinds(
    map: &std::collections::BTreeMap<&'static str, (usize, Vec<String>)>,
) -> Vec<&'static str> {
    map.keys().copied().collect()
}

/// Relative luminance (Rec. 709 weights on the 8-bit values).
fn lightness(t: &openstitch_core::Thread) -> f64 {
    0.2126 * f64::from(t.red) + 0.7152 * f64::from(t.green) + 0.0722 * f64::from(t.blue)
}

#[test]
fn clip_paths_cut_fills_and_strokes_to_the_clip() {
    let i = import("clipped-icon.svg", &SvgOptions::default());
    // The square (cut to the disc) and the slash's visible piece; the corner square is
    // clipped away entirely and listed.
    assert_eq!(i.shapes.len(), 2, "{:?}", i.shapes);
    let disc = |p: openstitch_design::Vec2| {
        let (x, y) = (p.x / PX - 50.0, p.y / PX - 50.0);
        (x * x + y * y).sqrt()
    };
    for s in &i.shapes {
        for c in &s.contours {
            for p in c.path() {
                assert!(disc(p) <= 30.0 + 0.05, "outside the clip: {p:?}");
            }
        }
    }
    let fill = bounds(&i, 0);
    assert!(
        (fill.width() / PX - 60.0).abs() < 0.5,
        "{}",
        fill.width() / PX
    );
    assert!(i.shapes[0].fill.is_some() && i.shapes[1].stroke.is_some());
    let slash = &i.shapes[1].contours;
    assert_eq!(slash.len(), 1);
    let len = slash[0].points[0].distance(slash[0].points[slash[0].points.len() - 1]) / PX;
    assert!((len - 60.0).abs() < 0.5, "{len}");
    assert_eq!(i.report.skipped["clipped"].1, ["away"]);
}

/// Through the command: no stitch lands outside the clip circle (60 SVG units across, drawn at
/// the SVG's own size).
#[test]
fn no_stitch_outside_the_clip() {
    let dir = TempDir::new("svg-import");
    std::fs::write(dir.join("icon.svg"), fixture("clipped-icon.svg")).expect("write");
    let mut s = Session::new(&dir);
    s.execute("import.svg", Some(&json!({"path": "icon.svg"})))
        .expect("import");
    let info = s.execute("design.info", None).expect("info");
    let st = &info["stitchStats"];
    let (w, h) = (st["widthMM"].as_f64(), st["heightMM"].as_f64());
    let across = 60.0 * PX / 10.0;
    assert!(w.is_some_and(|w| w <= across + 0.3), "{w:?} > {across}");
    assert!(h.is_some_and(|h| h <= across + 0.3), "{h:?} > {across}");
}

#[test]
fn masks_keep_what_is_opaque_and_cut_what_is_dark() {
    let i = import("masked-logo.svg", &SvgOptions::default());
    assert_eq!(i.shapes.len(), 1);
    let b = bounds(&i, 0);
    // Left half of the logo only: x 10–50.
    assert!((b.min_x / PX - 10.0).abs() < 0.1 && (b.max_x / PX - 50.0).abs() < 0.1);
    // The black bar is a hole: its centre is outside the region.
    let centre = openstitch_design::v2(25.0 * PX, 50.0 * PX);
    assert!(!openstitch_design::geometry::contains(
        centre,
        &i.shapes[0].contours
    ));
    let inside = openstitch_design::v2(15.0 * PX, 50.0 * PX);
    assert!(openstitch_design::geometry::contains(
        inside,
        &i.shapes[0].contours
    ));
    assert_eq!(kinds(&i.report.approximated), ["mask"]);
}

#[test]
fn symbols_used_twice_and_css_variables_come_through() {
    let i = import("symbol-star.svg", &SvgOptions::default());
    assert_eq!(i.shapes.len(), 2);
    assert!((bounds(&i, 1).min_x / PX - 50.0).abs() < 0.1);
    assert!(i.report.skipped.is_empty() && i.report.approximated.is_empty());
    let i = import("style-variable.svg", &SvgOptions::default());
    let hex: Vec<String> = i
        .shapes
        .iter()
        .map(|s| s.fill.as_ref().expect("fill").hex())
        .collect();
    assert_eq!(hex, ["#10a050", "#1428c8"]);
}

#[test]
fn faint_layers_stay_and_nearly_invisible_ones_are_listed() {
    let i = import("opacity-layer.svg", &SvgOptions::default());
    assert_eq!(i.shapes.len(), 2, "the 30 % layer stays, the 5 % one goes");
    assert_eq!(
        i.report.skipped["transparent"],
        (1, vec!["ghost".to_string()])
    );
    let strict = SvgOptions {
        min_opacity: 0.5,
        ..SvgOptions::default()
    };
    assert_eq!(import("opacity-layer.svg", &strict).shapes.len(), 1);
}

#[test]
fn gradients_give_their_middle_colour_or_bands_of_increasing_lightness() {
    let one = import("gradient-badge.svg", &SvgOptions::default());
    assert_eq!(one.shapes.len(), 2);
    // Half way between #101040 and #d0d0ff.
    assert_eq!(
        one.shapes[0].fill.as_ref().map(|t| t.hex()),
        Some("#7070a0".into())
    );
    assert_eq!(one.report.approximated["gradient"].0, 2);
    let bands = import(
        "gradient-badge.svg",
        &SvgOptions {
            gradients: Gradients::Bands(3),
            ..SvgOptions::default()
        },
    );
    // Three bands of the bar, three rings of the sun.
    assert_eq!(bands.shapes.len(), 6);
    let l: Vec<f64> = bands.shapes[..3]
        .iter()
        .map(|s| lightness(s.fill.as_ref().expect("fill")))
        .collect();
    assert!(l[0] < l[1] && l[1] < l[2], "{l:?}");
    // The bands sit side by side, left to right, a third of the bar each.
    for k in 0..3 {
        let b = bounds(&bands, k);
        assert!(
            (b.min_x / PX - 30.0 * k as f64).abs() < 0.1,
            "{k}: {}",
            b.min_x / PX
        );
        assert!((b.width() / PX - 30.0).abs() < 0.1);
    }
    // Rings: lightest in the middle, then darker outwards; the innermost is a disc.
    let r: Vec<f64> = bands.shapes[3..]
        .iter()
        .map(|s| lightness(s.fill.as_ref().expect("fill")))
        .collect();
    assert!(r[0] > r[1] && r[1] > r[2], "{r:?}");
    assert_eq!(bands.shapes[3].contours.len(), 1);
}

#[test]
fn text_images_and_filters_are_listed() {
    let i = import("text-and-image.svg", &SvgOptions::default());
    assert_eq!(i.shapes.len(), 2, "the grey square and the blurred circle");
    assert_eq!(i.report.skipped["text"], (1, vec!["caption".to_string()]));
    assert_eq!(i.report.skipped["image"], (1, vec!["photo".to_string()]));
    assert_eq!(i.report.approximated["filter"].1, ["blurred"]);
}

#[test]
fn import_svg_reports_and_takes_the_options() {
    let dir = TempDir::new("svg-import");
    std::fs::write(dir.join("badge.svg"), fixture("gradient-badge.svg")).expect("write");
    std::fs::write(dir.join("text.svg"), fixture("text-and-image.svg")).expect("write");
    let mut s = Session::new(&dir);
    let run = |s: &mut Session, p: Value| {
        s.execute("import.svg", Some(&p))
            .unwrap_or_else(|e| panic!("{p}: {e}"))
    };
    let r = run(
        &mut s,
        json!({"path": "badge.svg", "gradients": "bands", "bands": 4, "exactColors": true}),
    );
    assert_eq!(r["ids"].as_array().map(Vec::len), Some(8));
    assert_eq!(r["approximated"][0]["kind"], json!("gradient"));
    let r = run(&mut s, json!({"path": "text.svg"}));
    let skipped: Vec<&str> = r["skipped"]
        .as_array()
        .expect("skipped")
        .iter()
        .map(|k| k["kind"].as_str().expect("kind"))
        .collect();
    assert_eq!(skipped, ["image", "text"]);
    for bad in [
        json!({"path": "badge.svg", "gradients": "rainbow"}),
        json!({"path": "badge.svg", "gradients": "bands", "bands": 9}),
        json!({"path": "badge.svg", "minOpacity": 2}),
    ] {
        assert!(s.execute("import.svg", Some(&bad)).is_err(), "{bad}");
    }
}

/// The default import of plain artwork is what it was: no clip, mask, opacity or gradient in
/// it, nothing listed.
#[test]
fn plain_artwork_imports_as_before() {
    let svg =
        br##"<svg xmlns="http://www.w3.org/2000/svg" width="40" height="20" viewBox="0 0 40 20">
      <rect x="0" y="0" width="20" height="20" fill="#e01010"/>
      <circle cx="30" cy="10" r="8" fill="#1040c0" stroke="#000000"/>
    </svg>"##;
    let shapes = parse_svg(svg).expect("svg");
    assert_eq!(shapes.len(), 2);
    assert!(shapes[1].fill.is_some() && shapes[1].stroke.is_some());
    let i = parse_svg_with(svg, &SvgOptions::default()).expect("svg");
    assert!(i.report.skipped.is_empty() && i.report.approximated.is_empty());
    assert_eq!(i.shapes, shapes);
}
