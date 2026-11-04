//! Shaping, kerning, vertical text and per-character edits through the engine.

use openstitch_design::Rect;
use openstitch_design::text::{self, TextDirection, TextSpec};
use openstitch_engine::{Session, project};
use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::path::Path;

fn temp_dir() -> TempDir {
    TempDir::new("shaping")
}

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn bounds(s: &Session, id: u64) -> Rect {
    let d = s.design();
    d.objects[d.index_of(id).expect("object")]
        .bounds()
        .expect("bounds")
}

fn spec(s: &Session, id: u64) -> TextSpec {
    let d = s.design();
    d.objects[d.index_of(id).expect("object")]
        .text
        .clone()
        .expect("text")
}

#[test]
fn kerning_and_direction_are_text_options() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(&mut s, "text.add", json!({"text": "AVAVAV", "size": 20}));
    let plain = bounds(&s, 1);
    run(&mut s, "object.setText", json!({"id": 1, "kerning": true}));
    let kerned = bounds(&s, 1);
    assert!(
        kerned.width() < plain.width() - 20.0,
        "{kerned:?} vs {plain:?}"
    );
    let info = run(&mut s, "object.get", json!({"id": 1}));
    assert_eq!(info["text"]["kerning"], true);
    let listed = run(&mut s, "object.list", json!({}));
    assert_eq!(listed[0]["kerning"], true, "{listed}");

    run(
        &mut s,
        "object.setText",
        json!({"id": 1, "kerning": false, "direction": "vertical"}),
    );
    let tall = bounds(&s, 1);
    assert!(tall.height() > 4.0 * tall.width(), "{tall:?}");
    assert_eq!(spec(&s, 1).direction, TextDirection::Vertical);
    run(&mut s, "history.undo", json!({}));
    assert_eq!(spec(&s, 1).direction, TextDirection::Horizontal);
    assert!(spec(&s, 1).kerning);

    for bad in [
        json!({"id": 1, "direction": "diagonal"}),
        json!({"id": 1, "kerning": 1}),
    ] {
        assert!(s.execute("object.setText", Some(&bad)).is_err(), "{bad}");
    }
}

#[test]
fn single_characters_are_edited_saved_and_reset() {
    let dir = temp_dir();
    let mut s = Session::new(&dir);
    run(&mut s, "text.add", json!({"text": "HIH", "size": 10}));
    let before = bounds(&s, 1);
    let r = run(
        &mut s,
        "text.setCharacter",
        json!({"id": 1, "index": 1, "y": -5, "rotation": 30, "size": 15}),
    );
    assert_eq!(r["characters"], 1, "{r}");
    let after = bounds(&s, 1);
    assert!(after.min_y < before.min_y - 50.0, "{after:?}");
    assert!(
        after.width() > before.width(),
        "a bigger I pushes the last H"
    );
    let e = spec(&s, 1).char_edit(1).cloned().expect("edit");
    assert_eq!((e.y, e.rotation, e.size), (-5.0, 30.0, Some(15.0)));

    // Unnamed values are kept; null size returns to the text's size.
    run(
        &mut s,
        "text.setCharacter",
        json!({"id": 1, "index": 1, "size": null}),
    );
    let e = spec(&s, 1).char_edit(1).cloned().expect("edit");
    assert_eq!((e.y, e.rotation, e.size), (-5.0, 30.0, None));

    // Saved in the .osd and read back.
    run(&mut s, "project.save", json!({"path": "c.osd"}));
    let mut t = Session::new(&dir);
    run(&mut t, "project.open", json!({"path": "c.osd"}));
    assert_eq!(t.design(), s.design());
    let doc: Value =
        serde_json::from_slice(&project::encode(s.design()).expect("encode")).expect("json");
    assert_eq!(
        doc["design"]["objects"][0]["text"]["characters"],
        json!([{"index": 1, "y": -5.0, "rotation": 30.0}])
    );

    // Reset, then shortening the text drops edits past its end.
    run(
        &mut s,
        "text.setCharacter",
        json!({"id": 1, "index": 0, "count": 3, "reset": true}),
    );
    assert!(spec(&s, 1).characters.is_empty());
    let b = bounds(&s, 1);
    assert!(
        (b.width() - before.width()).abs() < 1e-6 && (b.height() - before.height()).abs() < 1e-6
    );
    run(
        &mut s,
        "text.setCharacter",
        json!({"id": 1, "index": 2, "x": 1}),
    );
    run(&mut s, "object.setText", json!({"id": 1, "text": "HI"}));
    assert!(spec(&s, 1).characters.is_empty());

    for bad in [
        json!({"id": 1, "index": 2}),
        json!({"id": 1, "index": 0, "count": 3}),
        json!({"id": 1, "index": 0, "rotation": 400}),
        json!({"id": 1, "index": 0, "size": 0}),
        json!({"id": 9, "index": 0}),
    ] {
        assert!(s.execute("text.setCharacter", Some(&bad)).is_err(), "{bad}");
    }
}

#[test]
fn name_drop_resets_character_edits() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    run(&mut s, "text.add", json!({"text": "Name"}));
    run(
        &mut s,
        "text.setCharacter",
        json!({"id": 1, "index": 0, "size": 14}),
    );
    run(&mut s, "text.setNameDrop", json!({"ids": [1]}));
    assert!(spec(&s, 1).characters.is_empty());
}

/// The bundled test font `file` (tests/fixtures/fonts, see PROVENANCE.md there), loaded as `id`.
fn load_fixture_font(s: &mut Session, file: &str, id: &str) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fonts")
        .join(file);
    run(
        s,
        "fonts.loadTrueType",
        json!({ "path": path.display().to_string(), "id": id }),
    );
}

/// Hebrew runs right to left beside a Latin word, and Arabic letters join, in the fonts loaded
/// as `hebrew` and `arabic`; both texts are added and sewn without warnings.
fn assert_right_to_left(s: &mut Session, hebrew: &str, arabic: &str) {
    let fonts = s.design().fonts.clone();
    let lay = |t: &str, font: &str| {
        text::layout_in(&TextSpec::new(t, font, 10.0), None, &fonts).expect("layout")
    };
    // A character may be drawn as several glyphs (Noto's Arabic letters: a base and its dots).
    let contours = |l: &text::Layout, index: usize| {
        l.glyphs
            .iter()
            .filter(|g| g.index == index)
            .flat_map(|g| g.contours.clone())
            .collect::<Vec<_>>()
    };
    let centre = |l: &text::Layout, index: usize| {
        Rect::enclosing(contours(l, index).iter().flat_map(|c| c.outline_points()))
            .expect("bounds")
            .center()
            .x
    };

    // Hebrew runs right to left; a Latin word before it stays on the left.
    let l = lay("ab \u{05E9}\u{05DC}\u{05D5}\u{05DD}", hebrew);
    assert!(
        l.missing.is_empty() && l.shaping.is_empty(),
        "{:?} {:?}",
        l.missing,
        l.shaping
    );
    assert!(centre(&l, 0) < centre(&l, 1), "Latin left to right");
    assert!(
        centre(&l, 3) > centre(&l, 4) && centre(&l, 4) > centre(&l, 6),
        "Hebrew right to left"
    );
    assert!(
        centre(&l, 1) < centre(&l, 6),
        "Latin before (left of) the Hebrew"
    );

    // Arabic letters join: a medial beh is not the isolated beh, and the word is right to left.
    let isolated = lay("\u{0628}", arabic);
    let word = lay("\u{0628}\u{0628}\u{0628}", arabic);
    assert!(word.missing.is_empty(), "{:?}", word.missing);
    let drawn: std::collections::BTreeSet<usize> = word.outlined().map(|g| g.index).collect();
    assert_eq!(drawn, [0, 1, 2].into(), "every letter drawn");
    let shift = |c: &openstitch_design::Contour| {
        let b = Rect::enclosing(c.outline_points()).expect("b");
        (b.width().round(), b.height().round())
    };
    assert_ne!(
        contours(&word, 1).iter().map(shift).collect::<Vec<_>>(),
        contours(&isolated, 0).iter().map(shift).collect::<Vec<_>>(),
        "contextual form"
    );
    assert!(centre(&word, 0) > centre(&word, 2));

    // Through the engine: added, sewn, no warnings.
    for (t, font) in [
        ("\u{0633}\u{0644}\u{0627}\u{0645}", arabic),
        ("\u{05E9}\u{05DC}\u{05D5}\u{05DD}", hebrew),
    ] {
        let r = run(s, "text.add", json!({"text": t, "font": font, "size": 15}));
        assert_eq!(r["missing"], json!([]), "{r}");
        assert_eq!(r["shaping"], json!([]), "{r}");
    }
    let stats = run(s, "design.generate", json!({}));
    assert!(stats["stitches"].as_u64() > Some(200), "{stats}");
}

/// Hebrew and Arabic through the bundled Noto test fonts, so the check runs on every machine
/// (#345).
#[test]
fn right_to_left_scripts_with_the_bundled_test_fonts() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    load_fixture_font(&mut s, "NotoSansHebrew-TestSubset.ttf", "hebrew");
    load_fixture_font(&mut s, "NotoSansArabic-TestSubset.ttf", "arabic");
    assert_right_to_left(&mut s, "hebrew", "arabic");
}

/// Devanagari is reordered: in \u{0915}\u{093F} (ki) the vowel sign i, typed after the
/// consonant, is drawn before (left of) it; the consonant keeps its own outline.
#[test]
fn devanagari_vowel_sign_i_is_drawn_before_its_consonant() {
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    load_fixture_font(&mut s, "NotoSansDevanagari-TestSubset.ttf", "deva");
    let fonts = s.design().fonts.clone();
    let lay =
        |t: &str| text::layout_in(&TextSpec::new(t, "deva", 10.0), None, &fonts).expect("layout");
    let size = |g: &text::Glyph| {
        let b = Rect::enclosing(g.contours.iter().flat_map(|c| c.outline_points())).expect("b");
        ((b.width() * 100.0).round(), (b.height() * 100.0).round())
    };
    let ka = lay("\u{0915}");
    assert!(ka.missing.is_empty() && ka.shaping.is_empty());
    let ka_size = size(&ka.glyphs[0]);

    let ki = lay("\u{0915}\u{093F}");
    assert!(ki.missing.is_empty(), "{:?}", ki.missing);
    let mut drawn: Vec<&text::Glyph> = ki
        .glyphs
        .iter()
        .filter(|g| !g.contours.is_empty())
        .collect();
    assert_eq!(drawn.len(), 2, "consonant and vowel sign");
    let left = |g: &text::Glyph| {
        Rect::enclosing(g.contours.iter().flat_map(|c| c.outline_points()))
            .expect("b")
            .min_x
    };
    drawn.sort_by(|a, b| left(a).total_cmp(&left(b)));
    assert_ne!(size(drawn[0]), ka_size, "the vowel sign comes first");
    assert_eq!(size(drawn[1]), ka_size, "then the consonant");

    let r = run(
        &mut s,
        "text.add",
        json!({"text": "\u{0915}\u{093F}", "font": "deva", "size": 15}),
    );
    assert_eq!(r["missing"], json!([]), "{r}");
}

/// Hebrew and Arabic through a system font as well, when the machine has one with both. Arial
/// and DejaVu Sans have a legacy `kern` table, which once left unkerned right-to-left text in
/// logical order (see `shaping::without_kern_table`).
#[test]
fn right_to_left_scripts_with_a_system_font() {
    let candidates = [
        "/System/Library/Fonts/Supplemental/Arial.ttf",
        "/System/Library/Fonts/Supplemental/Arial Unicode.ttf",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/usr/share/fonts/dejavu/DejaVuSans.ttf",
        "C:\\Windows\\Fonts\\arial.ttf",
    ];
    // The first font that has every character the test sets.
    let needed = "ab\u{05E9}\u{05DC}\u{05D5}\u{05DD}\u{0628}\u{0633}\u{0644}\u{0627}\u{0645}";
    let covers = |path: &Path| {
        std::fs::read(path).is_ok_and(|bytes| {
            openstitch_design::fonts::parse_face(&bytes, 0).is_some_and(|face| {
                needed
                    .chars()
                    .all(|c| face.glyph_index(c).is_some_and(|g| g.0 != 0))
            })
        })
    };
    let Some(path) = candidates.iter().map(Path::new).find(|p| covers(p)) else {
        eprintln!(
            "no system font with the Hebrew and Arabic test characters; the bundled fonts' test covers this"
        );
        return;
    };
    let tmp = temp_dir();
    let mut s = Session::new(tmp.path());
    if s.execute(
        "fonts.loadTrueType",
        Some(&json!({ "path": path.display().to_string(), "id": "rtl" })),
    )
    .is_err()
    {
        eprintln!("{} is not loadable here; skipped", path.display());
        return;
    }
    assert_right_to_left(&mut s, "rtl", "rtl");
}
