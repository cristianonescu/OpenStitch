//! Embroidery font folders in Ink/Stitch's published format (#529, beyond PE-DESIGN), read
//! from its user documentation (inkstitch.org/docs/font-tools, /tutorials/font-creation) and
//! published sample folders only; no Ink/Stitch code. Nothing is bundled: the user points
//! OpenStitch at fonts they installed, and the design keeps a copy (as it keeps TrueType fonts).
//!
//! **The folder.** `font.json` and a font variant: `ltr.svg` (older folders: `→.svg`), or the
//! first SVG in an `ltr` folder; a LICENSE file is shown when there is one.
//!
//! **font.json** (all optional but `name`): `name`, `font_license`, `leading` (px; 0 = from the
//! font), `horiz_adv_x` (advance per glyph, px), `horiz_adv_x_default`, `horiz_adv_x_space`
//! (the space's width), `kerning_pairs` (`"a b": px`, a positive value brings the two closer),
//! `min_scale` and `max_scale` (how far glyphs scale well: the size range), `letter_case`
//! (`upper` / `lower`: the other case uses the same glyphs).
//!
//! **The SVG.** One layer per glyph, labelled `GlyphLayer-A` for A; guides named `baseline`
//! (the line written on), `caps`, `ascender` and `descender`. A glyph's objects are sewn as the
//! documentation's stitch types say: a closed path with a fill colour is a fill, two or more
//! combined stroked paths (rails and rungs) a satin column, one stroked path a running
//! stitch. Satin columns become closed outlines between their two rails, sewn by the text's
//! satin; running stitch stays an open line along which the text's running line is sewn. The
//! font is sewn with the stitch type most of its pieces have; Ink/Stitch's per-object
//! parameters (`inkstitch:*`: angles, spacing, underlay, ...) are not read and are listed.

use crate::CommandSpec;
use crate::commands::{Effect, MM, opt, req, spec};
use crate::imports::{SvgOptions, SvgShape, parse_svg_with};
use crate::params::{CommandError, Params};
use crate::sandbox::Access;
use crate::session::EngineState;
use openstitch_design::fonts::{
    self, DesignFont, EmbroideryFont, EmbroideryGlyph, FontData, MAX_FONT_GLYPHS,
};
use openstitch_design::{Contour, Vec2, text, v2};
use serde_json::{Map, Value, json};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Largest font.json read.
const MAX_JSON_BYTES: usize = 4 << 20;
/// Largest glyph SVG read.
const MAX_SVG_BYTES: usize = 32 << 20;
/// Largest licence text shown.
const MAX_LICENSE_CHARS: usize = 4000;
/// Most font folders a scan lists.
const MAX_SCAN: usize = 1000;
/// One SVG user unit (CSS px) in 0.1 mm.
const PX: f64 = 254.0 / 96.0;

const INKSCAPE: &str = "http://www.inkscape.org/namespaces/inkscape";
const SODIPODI: &str = "http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd";
const INKSTITCH: &str = "http://inkstitch.org/namespace";

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "fonts.importInkStitch",
            "Load an embroidery font folder in Ink/Stitch's format (font.json and ltr.svg glyph layers) into the design for lettering; the design keeps its copy",
            Effect::Edit,
            vec![
                req("path", "string", "The font's folder (holding font.json)"),
                opt(
                    "id",
                    "string",
                    "Font id for text objects: a-z, 0-9, - and _ (default from the font's name)",
                ),
            ],
            import_font,
        ),
        spec(
            "fonts.scanFolder",
            "List the Ink/Stitch-format font folders in a folder (the user's font folder), to import one with fonts.importInkStitch; run again after adding or removing fonts",
            Effect::Read,
            vec![req("path", "string", "Folder holding font folders")],
            scan_folder,
        ),
    ]
}

/// What an import read and what it left out.
#[derive(Debug)]
pub struct InkFont {
    pub name: String,
    pub license: Option<String>,
    pub font: EmbroideryFont,
    /// Ink/Stitch parameters on the glyphs' objects that are not read, with their counts.
    pub not_read: BTreeMap<String, usize>,
    /// Glyph layers left out (names of more than one character: ligatures, alternates).
    pub skipped: Vec<String>,
    /// What was approximated, with counts.
    pub approximated: BTreeMap<&'static str, usize>,
}

fn num(v: &Value) -> Option<f64> {
    v.as_f64().filter(|x| x.is_finite())
}

/// A length attribute (`30`, `30px`, `5.08mm`, ...) in px.
fn length_px(s: &str) -> Option<f64> {
    let s = s.trim();
    let split = s
        .find(|c: char| c.is_ascii_alphabetic() || c == '%')
        .unwrap_or(s.len());
    let v: f64 = s[..split].trim().parse().ok()?;
    let k = match &s[split..] {
        "" | "px" => 1.0,
        "mm" => 96.0 / 25.4,
        "cm" => 96.0 / 2.54,
        "in" => 96.0,
        "pt" => 96.0 / 72.0,
        "pc" => 16.0,
        _ => return None,
    };
    Some(v * k).filter(|x| x.is_finite() && *x > 0.0)
}

/// End of an element's start tag in `text` (the `>` outside quotes), from its start.
fn start_tag_end(text: &str, from: usize) -> Option<usize> {
    let mut quote: Option<char> = None;
    for (i, c) in text[from..].char_indices() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '>') => return Some(from + i + 1),
            _ => {}
        }
    }
    None
}

/// Length of a polyline.
fn length(p: &[Vec2]) -> f64 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Reads a font from its `font.json` and glyph SVG.
pub fn parse(json_bytes: &[u8], svg: &[u8]) -> Result<InkFont, String> {
    if json_bytes.len() > MAX_JSON_BYTES || svg.len() > MAX_SVG_BYTES {
        return Err("font files too large".into());
    }
    let meta: Value = serde_json::from_slice(json_bytes).map_err(|e| format!("font.json: {e}"))?;
    let meta = meta.as_object().ok_or("font.json is not an object")?;
    let name: String = meta
        .get("name")
        .and_then(Value::as_str)
        .map(|s| s.trim().chars().take(64).collect())
        .filter(|s: &String| !s.is_empty())
        .ok_or("font.json has no name")?;
    let text_svg = std::str::from_utf8(svg).map_err(|_| "the SVG is not UTF-8")?;
    let doc = roxmltree::Document::parse(text_svg).map_err(|e| format!("the SVG: {e}"))?;
    let root = doc.root_element();
    // User units → px: the viewBox mapped onto width and height.
    let vb: Vec<f64> = root
        .attribute("viewBox")
        .unwrap_or("")
        .split(|c: char| c == ',' || c.is_whitespace())
        .filter_map(|t| t.parse().ok())
        .collect();
    let (vb_w, vb_h) = match vb[..] {
        [_, _, w, h] if w > 0.0 && h > 0.0 => (w, h),
        _ => (
            root.attribute("width").and_then(length_px).unwrap_or(1.0),
            root.attribute("height").and_then(length_px).unwrap_or(1.0),
        ),
    };
    let scale_x = root.attribute("width").and_then(length_px).unwrap_or(vb_w) / vb_w;
    let scale_y = root.attribute("height").and_then(length_px).unwrap_or(vb_h) / vb_h;
    // Guides: Inkscape keeps their position from the page's bottom-left corner, in user units.
    let mut guides: BTreeMap<String, f64> = BTreeMap::new();
    for g in doc
        .descendants()
        .filter(|n| n.has_tag_name((SODIPODI, "guide")))
    {
        let (Some(label), Some(pos)) = (g.attribute((INKSCAPE, "label")), g.attribute("position"))
        else {
            continue;
        };
        if let Some(y) = pos
            .split(',')
            .nth(1)
            .and_then(|y| y.trim().parse::<f64>().ok())
            .filter(|y| y.is_finite())
        {
            guides.insert(label.to_ascii_lowercase(), (vb_h - y) * scale_y * PX);
        }
    }
    let baseline = *guides
        .get("baseline")
        .ok_or("the SVG has no guide named baseline")?;
    let root_tag = &text_svg[root.range().start
        ..start_tag_end(text_svg, root.range().start).ok_or("the SVG's root tag does not end")?];
    let defs: String = root
        .children()
        .filter(|n| n.has_tag_name("defs"))
        .map(|n| &text_svg[n.range()])
        .collect();
    let mut out = InkFont {
        name,
        license: None,
        font: EmbroideryFont::new(100.0),
        not_read: BTreeMap::new(),
        skipped: Vec::new(),
        approximated: BTreeMap::new(),
    };
    let px = PX * scale_x;
    let adv = |key: &str| {
        meta.get("horiz_adv_x")
            .and_then(|m| m.get(key))
            .and_then(num)
    };
    let mut counts = [0usize; 3]; // satin, fill, running pieces
    let mut glyphs: Vec<EmbroideryGlyph> = Vec::new();
    for layer in doc.descendants().filter(|n| {
        n.has_tag_name("g")
            && n.attribute((INKSCAPE, "groupmode")) == Some("layer")
            && n.attribute((INKSCAPE, "label"))
                .is_some_and(|l| l.starts_with("GlyphLayer-"))
    }) {
        let glyph_name =
            &layer.attribute((INKSCAPE, "label")).expect("label")["GlyphLayer-".len()..];
        let mut chars = glyph_name.chars();
        let (Some(ch), None) = (chars.next(), chars.next()) else {
            out.skipped.push(glyph_name.chars().take(32).collect());
            continue;
        };
        if ch.is_control() || glyphs.iter().any(|g| g.ch == ch) {
            out.skipped.push(glyph_name.into());
            continue;
        }
        if glyphs.len() >= MAX_FONT_GLYPHS {
            return Err(format!("more than {MAX_FONT_GLYPHS} glyphs"));
        }
        if layer.parent_element() != Some(root) {
            *out.approximated
                .entry("glyph layers inside groups (the groups' transforms are left out)")
                .or_default() += 1;
        }
        // Ink/Stitch parameters on the layer's objects, by element id.
        let mut satin_ids: Vec<String> = Vec::new();
        for n in layer.descendants().filter(roxmltree::Node::is_element) {
            for a in n.attributes().filter(|a| a.namespace() == Some(INKSTITCH)) {
                if a.name() == "satin_column" {
                    if a.value().eq_ignore_ascii_case("true")
                        && let Some(id) = n.attribute("id")
                    {
                        satin_ids.push(id.into());
                    }
                } else {
                    *out.not_read.entry(a.name().into()).or_default() += 1;
                }
            }
        }
        // The layer alone, shown, as an SVG of its own.
        let range = layer.range();
        let tag_end = start_tag_end(text_svg, range.start).ok_or("a layer tag does not end")?;
        let tag = text_svg[range.start..tag_end].replacen("display:none", "display:inline", 1);
        let mini = format!(
            "{root_tag}{defs}{tag}{}</svg>",
            &text_svg[tag_end..range.end]
        );
        let import = parse_svg_with(mini.as_bytes(), &SvgOptions::default())
            .map_err(|e| format!("glyph {ch:?}: {e}"))?;
        let mut contours: Vec<Contour> = Vec::new();
        for shape in import.shapes {
            let piece = classify(&shape, &satin_ids);
            counts[piece.0] += 1;
            contours.extend(piece.1);
        }
        // Baseline at y = 0; x from the layer's left edge.
        let place = |p: Vec2| v2(p.x, p.y - baseline);
        let contours: Vec<Contour> = contours
            .into_iter()
            .map(|c| {
                Contour::new(
                    c.path().into_iter().map(place).collect::<Vec<_>>(),
                    c.closed,
                )
            })
            .filter(|c| c.points.len() >= 2)
            .collect();
        let (min_x, max_x) = contours
            .iter()
            .flat_map(|c| &c.points)
            .fold((f64::MAX, f64::MIN), |(a, b), p| (a.min(p.x), b.max(p.x)));
        let advance = adv(glyph_name)
            .or_else(|| {
                meta.get("horiz_adv_x_default")
                    .and_then(num)
                    .filter(|&a| a > 0.0)
            })
            .map(|a| a * px)
            .unwrap_or(if min_x <= max_x {
                max_x + min_x.max(0.0)
            } else {
                0.0
            });
        glyphs.push(EmbroideryGlyph {
            ch,
            advance: advance.max(0.0),
            contours,
            stitches: None,
        });
    }
    if glyphs.is_empty() {
        return Err("the SVG has no GlyphLayer-… layers".into());
    }
    // Metrics: the guides, else the capitals' height, else font.json's size (mm).
    let height_of = |c: char| {
        glyphs.iter().find(|g| g.ch == c).and_then(|g| {
            g.contours
                .iter()
                .flat_map(|c| &c.points)
                .map(|p| -p.y)
                .fold(None, |m: Option<f64>, y| Some(m.map_or(y, |m| m.max(y))))
        })
    };
    let cap = guides
        .get("caps")
        .map(|c| baseline - c)
        .filter(|&h| h > 1.0)
        .or_else(|| {
            height_of('H')
                .or_else(|| height_of('M'))
                .filter(|&h| h > 1.0)
        })
        .or_else(|| meta.get("size").and_then(num).map(|s| s * 10.0))
        .filter(|&h| (10.0..=5000.0).contains(&h))
        .ok_or("cannot tell the font's capital height (no caps guide, H or M)")?;
    let mut f = EmbroideryFont::new(cap);
    if let Some(a) = guides
        .get("ascender")
        .map(|a| baseline - a)
        .filter(|&a| a > 0.0)
    {
        f.ascender = a;
    }
    if let Some(d) = guides
        .get("descender")
        .map(|d| d - baseline)
        .filter(|&d| d > 0.0)
    {
        f.descender = d;
    }
    if let Some(leading) = meta.get("leading").and_then(num).filter(|&l| l > 0.0) {
        f.line_gap = (leading * PX * scale_y - f.ascender - f.descender).max(0.0);
    }
    // The other case shares the glyphs (letter_case).
    let case = meta
        .get("letter_case")
        .and_then(Value::as_str)
        .unwrap_or("");
    if matches!(case, "upper" | "lower") {
        let extra: Vec<EmbroideryGlyph> = glyphs
            .iter()
            .filter_map(|g| {
                let other = if case == "upper" {
                    g.ch.to_lowercase().next()
                } else {
                    g.ch.to_uppercase().next()
                }?;
                (other != g.ch && !glyphs.iter().any(|h| h.ch == other)).then(|| EmbroideryGlyph {
                    ch: other,
                    ..g.clone()
                })
            })
            .collect();
        glyphs.extend(extra);
    }
    if !glyphs.iter().any(|g| g.ch == ' ') {
        let space = meta
            .get("horiz_adv_x_space")
            .and_then(num)
            .filter(|&w| w > 0.0)
            .map_or(cap * 0.4, |w| w * px);
        glyphs.push(EmbroideryGlyph {
            ch: ' ',
            advance: space,
            contours: Vec::new(),
            stitches: None,
        });
    }
    for g in glyphs {
        f.set_glyph(g);
    }
    // Kerning pairs "a b": a positive value brings the pair closer.
    if let Some(pairs) = meta.get("kerning_pairs").and_then(Value::as_object) {
        let mut k: Vec<(char, char, f64)> = pairs
            .iter()
            .filter_map(|(key, v)| {
                let mut it = key.split(' ');
                let (l, r) = (it.next()?, it.next()?);
                if it.next().is_some() {
                    return None;
                }
                let one = |s: &str| {
                    let mut c = s.chars();
                    match (c.next(), c.next()) {
                        (Some(c), None) => Some(c),
                        _ => None,
                    }
                };
                Some((one(l)?, one(r)?, -num(v)? * px))
            })
            .take(fonts::MAX_KERNING_PAIRS)
            .collect();
        k.sort_by_key(|a| (a.0, a.1));
        k.dedup_by(|a, b| (a.0, a.1) == (b.0, b.1));
        f.kerning = k;
    }
    if let (Some(lo), Some(hi)) = (
        meta.get("min_scale").and_then(num),
        meta.get("max_scale").and_then(num),
    ) && lo > 0.0
        && hi >= lo
    {
        f.size_range = Some((cap / 10.0 * lo, cap / 10.0 * hi)).filter(|r| r.1 <= 1000.0);
    }
    let kinds = ["satin", "fill", "running"];
    let main = (0..3).max_by_key(|&i| counts[i]).unwrap_or(0);
    f.sewing = Some(kinds[main].to_string());
    let others: usize = (0..3).filter(|&i| i != main).map(|i| counts[i]).sum();
    if others > 0 {
        *out.approximated
            .entry("pieces of another stitch type, sewn with the font's main one")
            .or_default() += others;
    }
    f.validate().map_err(|e| format!("the font: {e}"))?;
    out.license = meta
        .get("font_license")
        .and_then(Value::as_str)
        .map(|s| s.chars().take(200).collect());
    out.font = f;
    Ok(out)
}

/// The stitch type of one SVG object (0 satin, 1 fill, 2 running) and its contours.
fn classify(shape: &SvgShape, satin_ids: &[String]) -> (usize, Vec<Contour>) {
    let paths: Vec<Vec<Vec2>> = shape.contours.iter().map(Contour::path).collect();
    let marked = shape.names.last().is_some_and(|n| satin_ids.contains(n));
    if shape.fill.is_some() && !marked {
        let closed = paths
            .into_iter()
            .filter(|p| p.len() >= 3)
            .map(|p| Contour::new(p, true))
            .collect();
        return (1, closed);
    }
    if (marked || paths.len() >= 2) && paths.len() >= 2 {
        // The two longest subpaths are the rails, the rest rungs.
        let mut by_length: Vec<&Vec<Vec2>> = paths.iter().collect();
        by_length.sort_by(|a, b| length(b).total_cmp(&length(a)));
        let (a, b) = (by_length[0], by_length[1]);
        let same = a[0].distance(b[0]) + a[a.len() - 1].distance(b[b.len() - 1])
            <= a[0].distance(b[b.len() - 1]) + a[a.len() - 1].distance(b[0]);
        let mut outline = a.clone();
        if same {
            outline.extend(b.iter().rev());
        } else {
            outline.extend(b.iter());
        }
        return (0, vec![Contour::new(outline, true)]);
    }
    (
        2,
        paths.into_iter().map(|p| Contour::new(p, false)).collect(),
    )
}

/// The variant SVG of a font folder: `ltr.svg`, `→.svg`, or the first SVG in `ltr/`.
fn variant(dir: &Path) -> Option<PathBuf> {
    for name in ["ltr.svg", "→.svg"] {
        let p = dir.join(name);
        if p.is_file() {
            return Some(p);
        }
    }
    let mut svgs: Vec<PathBuf> = std::fs::read_dir(dir.join("ltr"))
        .ok()?
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("svg")))
        .collect();
    svgs.sort();
    svgs.into_iter().next()
}

fn read_limited(path: &Path, max: usize) -> Result<Vec<u8>, String> {
    let len = std::fs::metadata(path)
        .map_err(|_| format!("cannot read {}", path.display()))?
        .len();
    if len > max as u64 {
        return Err(format!("{} is too large", path.display()));
    }
    std::fs::read(path).map_err(|_| format!("cannot read {}", path.display()))
}

/// The licence file of a font folder, if any (at most 4,000 characters).
fn license_text(dir: &Path) -> Option<String> {
    let entry = std::fs::read_dir(dir)
        .ok()?
        .filter_map(Result::ok)
        .find(|e| {
            let n = e.file_name().to_string_lossy().to_ascii_uppercase();
            n.starts_with("LICENSE") || n.starts_with("LICENCE") || n.starts_with("OFL")
        })?;
    let bytes = read_limited(&entry.path(), 256 << 10).ok()?;
    Some(
        String::from_utf8_lossy(&bytes)
            .chars()
            .take(MAX_LICENSE_CHARS)
            .collect(),
    )
}

/// Reads the font folder `dir`.
pub fn read_folder(dir: &Path) -> Result<InkFont, String> {
    let json_bytes = read_limited(&dir.join("font.json"), MAX_JSON_BYTES)?;
    let svg_path = variant(dir).ok_or("the folder has no ltr.svg font variant")?;
    let svg = read_limited(&svg_path, MAX_SVG_BYTES)?;
    let mut f = parse(&json_bytes, &svg)?;
    if let Some(l) = license_text(dir) {
        f.license = Some(l);
    }
    Ok(f)
}

fn import_font(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let dir = s.resolve(&p.string("path", None)?, Access::Read, &p.command)?;
    if !dir.is_dir() {
        return Err(p.failed(format!("{} is not a folder", dir.display())));
    }
    let wanted = if p.has("id") {
        let id = p.string("id", None)?;
        if !fonts::valid_id(&id) {
            return Err(p.invalid("id", "must be 1-64 of a-z, 0-9, - and _"));
        }
        if crate::fonts::id_taken(s, &id) {
            return Err(p.invalid("id", format!("a font with id '{id}' already exists")));
        }
        Some(id)
    } else {
        None
    };
    let ink = read_folder(&dir).map_err(|e| p.failed(e))?;
    let id = wanted.unwrap_or_else(|| {
        let base = fonts::slug(&ink.name);
        let mut id = base.clone();
        let mut n = 2;
        while crate::fonts::id_taken(s, &id) {
            id = format!("{base}-{n}");
            n += 1;
        }
        id
    });
    let font = DesignFont {
        id: id.clone(),
        family: ink.name.clone(),
        style: "Ink/Stitch".into(),
        source: dir.file_name().map(|n| n.to_string_lossy().into_owned()),
        data: FontData::Embroidery(ink.font.clone()),
    };
    crate::fonts::check_room(s, p, font.byte_size())?;
    s.design.fonts.push(font);
    let glyphs = text::LoadedFont::load_in(&id, &s.design.fonts)
        .map(|f| f.glyph_count())
        .unwrap_or(0);
    let not_read: Map<String, Value> = ink
        .not_read
        .iter()
        .map(|(k, v)| (k.clone(), json!(v)))
        .collect();
    let approximated: Map<String, Value> = ink
        .approximated
        .iter()
        .map(|(k, v)| ((*k).to_string(), json!(v)))
        .collect();
    Ok(json!({
        "id": id, "family": ink.name, "glyphs": glyphs,
        "heightMM": ink.font.cap_height / MM,
        "sizeRange": ink.font.size_range.map(|(lo, hi)| json!({ "minMm": lo, "maxMm": hi })),
        "sewing": ink.font.sewing,
        "kerningPairs": ink.font.kerning.len(),
        "license": ink.license,
        "notRead": not_read,
        "skipped": ink.skipped,
        "approximated": approximated,
    }))
}

fn scan_folder(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let dir = s.resolve(&p.string("path", None)?, Access::Read, &p.command)?;
    let entries = std::fs::read_dir(&dir)
        .map_err(|_| p.failed(format!("cannot read the folder {}", dir.display())))?;
    let mut rows: Vec<Value> = Vec::new();
    let mut paths: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|d| d.is_dir() && d.join("font.json").is_file())
        .collect();
    paths.sort();
    for d in paths.into_iter().take(MAX_SCAN) {
        let meta: Value = read_limited(&d.join("font.json"), MAX_JSON_BYTES)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or(Value::Null);
        let folder = d.file_name().map(|n| n.to_string_lossy().into_owned());
        rows.push(json!({
            "folder": folder,
            "name": meta.get("name").and_then(Value::as_str).unwrap_or(""),
            "license": meta.get("font_license").and_then(Value::as_str),
            "glyphs": meta.get("glyphs").and_then(Value::as_array).map(Vec::len),
            "variant": variant(&d).is_some(),
        }));
    }
    Ok(Value::Array(rows))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostile_input_is_refused_not_panicked() {
        let json = br#"{"name": "T"}"#;
        for svg in [
            &b""[..],
            b"<svg",
            b"<svg xmlns='http://www.w3.org/2000/svg'/>",
            b"\xff\xfe",
        ] {
            assert!(parse(json, svg).is_err());
        }
        assert!(parse(b"[]", b"<svg/>").is_err());
        assert!(parse(b"{}", b"<svg/>").is_err());
    }

    /// Mutation fuzzing of the parser (the engine has no cargo-fuzz crate): bytes of a valid
    /// font flipped, cut and repeated; every result is a font or an error, never a panic.
    #[test]
    fn mutated_fonts_never_panic() {
        let json = br#"{"name":"F","horiz_adv_x":{"A":30},"kerning_pairs":{"A A":2},"min_scale":0.5,"max_scale":2,"letter_case":"upper","leading":40}"#;
        let svg = br##"<svg width="40" height="40" viewBox="0 0 40 40" xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape" xmlns:sodipodi="http://sodipodi.sourceforge.net/DTD/sodipodi-0.dtd" xmlns:inkstitch="http://inkstitch.org/namespace"><sodipodi:namedview><sodipodi:guide position="0,5" inkscape:label="baseline"/><sodipodi:guide position="0,35" inkscape:label="caps"/></sodipodi:namedview><g inkscape:groupmode="layer" inkscape:label="GlyphLayer-A" style="display:none"><path id="a" d="M 2,35 L 10,5 L 18,35 M 5,35 L 10,15 L 15,35" style="fill:none;stroke:#000" inkstitch:satin_column="True"/></g></svg>"##;
        assert!(parse(json, svg).is_ok(), "the seed font loads");
        let mut x = 0x9E37_79B9_u32;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 17;
            x ^= x << 5;
            x as usize
        };
        for round in 0..3000 {
            let (mut j, mut v) = (json.to_vec(), svg.to_vec());
            let target = if round % 2 == 0 { &mut j } else { &mut v };
            for _ in 0..1 + next() % 4 {
                let at = next() % target.len().max(1);
                match next() % 3 {
                    0 => target[at] = next() as u8,
                    1 => target.truncate(at),
                    _ => {
                        let piece: Vec<u8> = target[at..].iter().take(16).copied().collect();
                        target.splice(at..at, piece);
                    }
                }
                if target.is_empty() {
                    break;
                }
            }
            let _ = parse(&j, &v);
        }
    }

    #[test]
    fn lengths_convert_to_px() {
        assert_eq!(length_px("30"), Some(30.0));
        assert!((length_px("25.4mm").expect("mm") - 96.0).abs() < 1e-9);
        assert_eq!(length_px("10%"), None);
        assert_eq!(start_tag_end("<a b='>'>x</a>", 0), Some(9));
    }
}
