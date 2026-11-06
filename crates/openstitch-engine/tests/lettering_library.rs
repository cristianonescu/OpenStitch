//! The lettering library (#527, #530, #536) through engine commands: `fonts.list` metadata, the
//! single-line Small Text font, the pixel font and the size warning.

use openstitch_core::{Command, Stitch};
use openstitch_design::compile::Compiler;
use openstitch_design::geometry::{Vec2, v2};
use openstitch_design::{LineType, ObjectKind};
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn session() -> (Session, TempDir) {
    let dir = TempDir::new("lettering-library");
    (Session::new(&dir), dir)
}

fn compiled(s: &Session) -> Vec<Stitch> {
    Compiler::for_design(s.design())
        .compile(s.design())
        .stitches
}

fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= 1e-18 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The text object's letters: each letter's polylines (0.1 mm).
fn letters(s: &Session) -> Vec<Vec<Vec<Vec2>>> {
    let o = s
        .design()
        .objects
        .iter()
        .find(|o| o.kind == ObjectKind::Text)
        .expect("text");
    o.glyph_groups()
        .into_iter()
        .map(|g| {
            g.iter()
                .map(|c| {
                    let mut p = c.path();
                    if c.closed
                        && let Some(&f) = p.first()
                    {
                        p.push(f);
                    }
                    p
                })
                .collect()
        })
        .collect()
}

fn near_letter(p: Vec2, letter: &[Vec<Vec2>], tol: f64) -> bool {
    letter
        .iter()
        .any(|l| l.windows(2).any(|w| dist_to_segment(p, w[0], w[1]) <= tol))
}

#[test]
fn fonts_list_tells_sizes_stitch_types_and_groups() {
    let (mut s, _d) = session();
    let rows = run(&mut s, "fonts.list", json!({}));
    let row = |id: &str| {
        rows.as_array()
            .expect("rows")
            .iter()
            .find(|r| r["id"] == id)
            .cloned()
            .unwrap_or_else(|| panic!("{id} listed"))
    };
    for id in [
        "lato-bold",
        "pacifico",
        "openstitch-line",
        "openstitch-pixel",
    ] {
        let r = row(id);
        let (lo, hi) = (
            r["sizeRange"]["minMm"].as_f64().expect("min"),
            r["sizeRange"]["maxMm"].as_f64().expect("max"),
        );
        assert!(0.0 < lo && lo < hi, "{id}");
        assert!(!r["stitchTypes"].as_array().expect("types").is_empty());
    }
    assert_eq!(row("openstitch-line")["kind"], "singleLine");
    assert_eq!(row("openstitch-line")["group"], "smallText");
    assert_eq!(row("openstitch-pixel")["kind"], "pixel");
    assert_eq!(row("lato-bold")["kind"], "trueType");
}

#[test]
fn text_outside_the_fonts_sizes_gets_a_warning_not_a_refusal() {
    let (mut s, _d) = session();
    let r = run(&mut s, "text.add", json!({"text": "Ab", "size": 3}));
    let w = r["sizeWarning"].as_str().expect("warning below the range");
    assert!(w.contains("below") && w.contains("Lato"), "{w}");
    let r = run(&mut s, "text.add", json!({"text": "Ab", "size": 10}));
    assert!(r.get("sizeWarning").is_none(), "{r}");
    let id = r["id"].as_u64().expect("id");
    let r = run(
        &mut s,
        "object.setText",
        json!({"ids": [id], "font": "openstitch-line", "size": 40}),
    );
    let w = &r["sizeWarnings"][0];
    assert_eq!(w["id"], id);
    assert!(
        w["warning"].as_str().expect("warning").contains("above"),
        "{r}"
    );
    let i = s.design().index_of(id).expect("object");
    assert_eq!(
        s.design().objects[i].text.as_ref().expect("text").size,
        40.0
    );
}

/// "abc ijt 0123 Hello" in OpenStitch Line at 3, 4 and 6 mm: a running line along each letter's
/// centre, every stitch on a stroke (travel only along strokes already sewn) or joining one
/// letter to the next, the i and j keep their dots, no stitch longer than the pitch, and few
/// trims: letters on a line are joined, as PE-DESIGN's Small Text joins them (#530).
#[test]
fn small_text_sews_along_the_centre_lines() {
    for size in [3.0, 4.0, 6.0] {
        let (mut s, _d) = session();
        let r = run(
            &mut s,
            "text.add",
            json!({"text": "abc ijt 0123 Hello", "font": "openstitch-line", "size": size}),
        );
        assert!(r.get("sizeWarning").is_none(), "{size}: {r}");
        let o = &s.design().objects[0];
        assert!(o.region.is_none(), "single-line text has no region");
        let line = o.line.as_ref().expect("running line");
        assert_eq!(line.kind, LineType::Running);
        assert!((line.pitch - 1.5).abs() < 1e-9);
        let letters = letters(&s);
        let stitches = compiled(&s);
        // Stitching off the strokes is only the straight connectors from one letter to the
        // next (a connector crossing a stroke shows as two pieces): never bent travel.
        let mut connectors = 0;
        let mut check_chain = |chain: &[Vec2]| {
            if chain.len() < 2 {
                return;
            }
            connectors += 1;
            let (a, b) = (chain[0], chain[chain.len() - 1]);
            assert!(
                chain.iter().all(|&q| dist_to_segment(q, a, b) < 0.5),
                "{size} mm: bent travel off the strokes: {chain:?}"
            );
        };
        let mut prev: Option<Vec2> = None;
        let mut chain: Vec<Vec2> = Vec::new();
        let mut trims = 0;
        for st in &stitches {
            match st.command {
                Command::Stitch => {
                    let p = v2(st.x, st.y);
                    if let Some(a) = prev {
                        // No stitch longer than the pitch: on a stroke, or a connector.
                        let len = a.distance(p);
                        assert!(len <= 15.0 + 0.5, "{size} mm: stitch {len} at {p:?}");
                        // Curves sew with stitches within 0.2 mm of the line.
                        let mid = (a + p) * 0.5;
                        if letters.iter().any(|l| near_letter(mid, l, 2.5)) {
                            check_chain(&chain);
                            chain.clear();
                        } else {
                            if chain.is_empty() {
                                chain.push(a);
                            }
                            chain.push(p);
                        }
                    }
                    prev = Some(p);
                }
                Command::Trim => {
                    trims += 1;
                    prev = None;
                    check_chain(&chain);
                    chain.clear();
                }
                _ => prev = None,
            }
        }
        check_chain(&chain);
        assert!(
            connectors <= 2 * letters.len(),
            "{size} mm: {connectors} pieces of travel"
        );
        // The dots of i and j, the final trim and at most a few long gaps.
        assert!(trims <= 6, "{size} mm: {trims} trims");
        // The dots are sewn.
        let needles: Vec<Vec2> = stitches
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .map(|s| v2(s.x, s.y))
            .collect();
        for (k, letter) in letters.iter().enumerate() {
            for stroke in letter {
                assert!(
                    stroke
                        .iter()
                        .all(|q| needles.iter().any(|n| n.distance(*q) < 8.0)),
                    "{size} mm: letter {k} has a stroke left unsewn"
                );
            }
        }
    }
}

/// OpenStitch Line as a narrow satin along the centre line: every letter and digit at 5 mm in
/// at most two runs (one trim, an i's or j's dot).
#[test]
fn centre_line_satin_sews_each_letter_in_one_piece() {
    let all: String = ('a'..='z').chain('A'..='Z').chain('0'..='9').collect();
    for c in all.chars() {
        let (mut s, _d) = session();
        let r = run(
            &mut s,
            "text.add",
            json!({"text": c.to_string(), "font": "openstitch-line", "size": 5}),
        );
        let id = r["id"].as_u64().expect("id");
        run(
            &mut s,
            "object.setLine",
            json!({"ids": [id], "type": "zigzag", "width": 1.2, "density": 4}),
        );
        let trims = compiled(&s)
            .iter()
            .filter(|s| s.command == Command::Trim)
            .count();
        // The design's own final trim is not between pieces.
        assert!(trims <= 2, "{c}: {trims} trims");
    }
}

#[test]
fn switching_to_and_from_the_single_line_font_keeps_it_sewn() {
    let (mut s, _d) = session();
    let id = run(&mut s, "text.add", json!({"text": "Hi"}))["id"]
        .as_u64()
        .expect("id");
    let o = |s: &Session| s.design().objects[0].clone();
    assert!(o(&s).region.is_some() && o(&s).line.is_none());
    run(
        &mut s,
        "object.setText",
        json!({"ids": [id], "font": "openstitch-line"}),
    );
    assert!(o(&s).region.is_none());
    assert_eq!(o(&s).line.expect("line").kind, LineType::Running);
    assert!(!compiled(&s).is_empty());
    run(
        &mut s,
        "object.setText",
        json!({"ids": [id], "font": "lato-bold"}),
    );
    assert!(
        o(&s).region.is_some(),
        "outline font gets a satin region back"
    );
}

/// "HELLO 2026" in the 5 × 7 pixel font at 2 mm per pixel (size 14), in each pixel style: every
/// needle point lies in a lit pixel, and the only travel outside the pixels is from one letter
/// to the next (#536).
#[test]
fn pixel_text_sews_inside_its_pixels() {
    for style in ["run", "cross", "block"] {
        let (mut s, _d) = session();
        run(
            &mut s,
            "text.add",
            json!({"text": "HELLO 2026", "font": "openstitch-pixel", "size": 14, "pixelStyle": style}),
        );
        assert_eq!(
            s.design().objects[0]
                .text
                .as_ref()
                .expect("text")
                .pixel_style
                .as_str(),
            style
        );
        let letters = letters(&s);
        let inside = |p: Vec2| {
            s.design().objects[0].contours.iter().any(|c| {
                let pts = &c.points;
                let (x0, x1) = (pts[0].x.min(pts[2].x), pts[0].x.max(pts[2].x));
                let (y0, y1) = (pts[0].y.min(pts[2].y), pts[0].y.max(pts[2].y));
                p.x >= x0 - 0.5 && p.x <= x1 + 0.5 && p.y >= y0 - 0.5 && p.y <= y1 + 0.5
            })
        };
        let stitches = compiled(&s);
        let mut prev: Option<Vec2> = None;
        let mut trims = 0;
        for st in &stitches {
            match st.command {
                Command::Stitch => {
                    let p = v2(st.x, st.y);
                    assert!(inside(p), "{style}: needle outside the pixels at {p:?}");
                    if let Some(a) = prev
                        && !inside((a + p) * 0.5)
                    {
                        let la = letters.iter().position(|l| near_letter(a, l, 0.5));
                        let lb = letters.iter().position(|l| near_letter(p, l, 0.5));
                        assert_ne!(la, lb, "{style}: travel inside a letter at {p:?}");
                    }
                    prev = Some(p);
                }
                Command::Trim => {
                    trims += 1;
                    prev = None;
                }
                _ => prev = None,
            }
        }
        // Nine letters: at most one trim before each and the design's last.
        assert!(trims <= 10, "{style}: {trims}");
    }
    let (mut s, _d) = session();
    let e = s
        .execute(
            "text.add",
            Some(&json!({"text": "A", "font": "openstitch-pixel", "pixelStyle": "dots"})),
        )
        .expect_err("unknown style");
    assert!(e.to_string().contains("pixelStyle"), "{e}");
}
