//! Stitch settings read from an SVG's layer and object names (#455): `import.svg` with
//! `settings: true`. Off by default; the default import is unchanged.
//!
//! **Where names come from.** For each shape, the names of its named groups (outermost first,
//! Inkscape layers included) and then its own. An element's name is its `inkscape:label`, else
//! Illustrator's `data-name`, else its `id` (Illustrator's `_xHH_` escapes decoded; in an id,
//! `_` also separates words). Inner names override outer ones, word by word.
//!
//! **Syntax** (words separated by spaces or commas, any case):
//! - a sew type: `fill`, `satin` (a satin region; on a shape with no fill, a satin line),
//!   `run`, `triple`, `zigzag` (lines), `skip` (the shape is not imported); with `stroke.` in
//!   front it is the outline's: `stroke.satin` (a satin line, PE-DESIGN's zigzag), `stroke.run`,
//!   `stroke.triple`, `stroke.zigzag`, `stroke.skip` (no outline);
//! - `key=value` settings: `a=` angle (degrees), `s=` row spacing (mm) or `d=` density (rows
//!   per mm), `l=` stitch length (mm), `pc=` pull compensation (mm), `u=` underlay (`0`/`1`),
//!   `c=` colour (`#rrggbb` or a Brother thread name) for the region; `w=` width (mm), `r=`
//!   passes (1–5) for the line. `l=`, `d=` and `c=` belong to the region when the shape has a
//!   fill and to the line otherwise; a `stroke.` prefix (`stroke.w=3`) always means the line.
//!
//! A name with no recognised word (such as Inkscape's `path1234`) is not a settings name and
//! is ignored. In a settings name, unknown words are listed in the result, and values the
//! sewing commands refuse are listed with the reason; nothing is guessed.
//!
//! With `settings: true` the import also follows two SVG stroke properties: a dashed stroke
//! becomes one line per dash, and `paint-order: stroke` sews the outline before the fill.

use crate::imports::SvgShape;
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_design::{Contour, Vec2};
use serde_json::{Map, Value, json};
use std::collections::HashMap;

/// Most dash pieces one stroke may become; above it the stroke stays whole and is listed.
const MAX_DASHES: usize = 2_000;

const INKSCAPE: &str = "http://www.inkscape.org/namespaces/inkscape";

/// The settings a chain of names gives one shape.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Settings {
    pub skip: bool,
    pub skip_line: bool,
    pub region_type: Option<&'static str>,
    pub line_type: Option<&'static str>,
    /// `object.setRegion` / `object.setLine` parameters, with the word each came from.
    pub region: Vec<(&'static str, Value, String)>,
    pub line: Vec<(&'static str, Value, String)>,
    /// `satin` without `stroke.`: a satin region, or a satin line on a shape with no fill.
    satin: bool,
}

/// Each element's name by id: `inkscape:label`, else `data-name`, else the id itself.
pub fn names_by_id(data: &[u8]) -> HashMap<String, (String, bool)> {
    let mut out = HashMap::new();
    let Ok(text) = std::str::from_utf8(data) else {
        return out;
    };
    let opt = roxmltree::ParsingOptions {
        allow_dtd: true,
        ..roxmltree::ParsingOptions::default()
    };
    let Ok(doc) = roxmltree::Document::parse_with_options(text, opt) else {
        return out;
    };
    for n in doc.descendants().filter(roxmltree::Node::is_element) {
        let Some(id) = n.attribute("id") else {
            continue;
        };
        let label = n
            .attribute((INKSCAPE, "label"))
            .or_else(|| n.attribute("data-name"));
        let entry = match label {
            Some(l) => (decode(l), false),
            None => (decode(id), true),
        };
        out.insert(id.to_string(), entry);
    }
    out
}

/// Illustrator's `_xHH_` escapes (`_x3D_` is `=`) decoded.
fn decode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find("_x") {
        out.push_str(&rest[..i]);
        let tail = &rest[i + 2..];
        let hex = tail.get(..2);
        let code = hex
            .filter(|_| tail.as_bytes().get(2) == Some(&b'_'))
            .and_then(|h| u8::from_str_radix(h, 16).ok())
            .filter(|c| c.is_ascii_graphic() || *c == b' ');
        match code {
            Some(c) => {
                out.push(char::from(c));
                rest = &tail[3..];
            }
            None => {
                out.push_str("_x");
                rest = tail;
            }
        }
    }
    out.push_str(rest);
    out
}

fn words(name: &str, from_id: bool) -> Vec<String> {
    name.split(|c: char| c.is_whitespace() || c == ',' || (from_id && c == '_'))
        .filter(|w| !w.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// What one word means, if it is a settings word.
enum Word {
    Skip,
    StrokeSkip,
    Region(&'static str),
    Line(&'static str),
    Satin,
    /// A key for the region (`true`) or the line, or for whichever the shape has (`None`).
    Key(&'static str, Option<bool>, Value),
}

/// A key's short form: the long names (`angle=45`, `spacing=0.4`, `density=4.5`,
/// `length=2.5`, `pullcompensation=0.2`, `underlay=on`, `color=red`, `width=3`, `repeats=3`)
/// mean the same as `a=`, `s=`, `d=`, `l=`, `pc=`, `u=`, `c=`, `w=` and `r=` (#455). Words are
/// lower-cased first, so `pullCompensation` and `runTimes` are long names too.
fn long_key(k: &str) -> &str {
    match k {
        "angle" | "direction" => "a",
        "spacing" | "rowspacing" => "s",
        "density" => "d",
        "length" | "stitchlength" | "pitch" => "l",
        "pull" | "pullcompensation" => "pc",
        "underlay" => "u",
        "color" | "colour" => "c",
        "width" => "w",
        "repeats" | "runs" | "runtimes" | "passes" => "r",
        short => short,
    }
}

fn parse_word(w: &str) -> Option<Word> {
    let (stroke, w) = match w.strip_prefix("stroke.") {
        Some(r) => (true, r),
        None => (false, w),
    };
    if let Some((k, v)) = w.split_once('=') {
        let k = long_key(k);
        let num = || v.parse::<f64>().ok().filter(|n| n.is_finite());
        let line = if stroke { Some(false) } else { None };
        return Some(match k {
            "a" if !stroke => Word::Key("angle", Some(true), json!(num()?)),
            "s" if !stroke => {
                let s = num()?;
                Word::Key(
                    "density",
                    Some(true),
                    json!(if s > 0.0 { 1.0 / s } else { -1.0 }),
                )
            }
            "pc" if !stroke => Word::Key("pullCompensation", Some(true), json!(num()?)),
            "u" if !stroke => Word::Key(
                "underlay",
                Some(true),
                json!(match v {
                    "1" | "true" | "yes" | "on" => true,
                    "0" | "false" | "no" | "off" => false,
                    _ => return None,
                }),
            ),
            "d" => Word::Key("density", line, json!(num()?)),
            "l" => Word::Key("length", line, json!(num()?)),
            "c" => Word::Key("color", line, json!(v)),
            "w" => Word::Key("width", Some(false), json!(num()?)),
            "r" => Word::Key("runTimes", Some(false), json!(v.parse::<i64>().ok()?)),
            _ => return None,
        });
    }
    Some(match (stroke, w) {
        (false, "skip") => Word::Skip,
        (true, "skip") => Word::StrokeSkip,
        (false, "fill") => Word::Region("fill"),
        (false, "satin") => Word::Satin,
        (true, "satin" | "zigzag") | (false, "zigzag") => Word::Line("zigzag"),
        (_, "run" | "running") => Word::Line("running"),
        (_, "triple") => Word::Line("triple"),
        _ => return None,
    })
}

/// The settings `names` (outermost first) give a shape with or without a fill, and the
/// unknown words of its settings names.
pub fn settings_for(names: &[(String, bool)], has_fill: bool) -> (Settings, Vec<String>) {
    let mut s = Settings::default();
    let mut unknown = Vec::new();
    for (name, from_id) in names {
        let ws = words(name, *from_id);
        let parsed: Vec<(String, Option<Word>)> = ws
            .into_iter()
            .map(|w| (w.clone(), parse_word(&w)))
            .collect();
        if parsed.iter().all(|(_, p)| p.is_none()) {
            continue;
        }
        for (w, p) in parsed {
            let Some(p) = p else {
                unknown.push(w);
                continue;
            };
            match p {
                Word::Skip => s.skip = true,
                Word::StrokeSkip => s.skip_line = true,
                Word::Region(t) => {
                    s.region_type = Some(t);
                    s.satin = false;
                }
                Word::Satin => s.satin = true,
                Word::Line(t) => s.line_type = Some(t),
                Word::Key(k, region, v) => {
                    let region = region.unwrap_or(has_fill);
                    let (list, key) = if region {
                        (
                            &mut s.region,
                            if k == "length" { "stitchLength" } else { k },
                        )
                    } else {
                        (&mut s.line, if k == "length" { "pitch" } else { k })
                    };
                    list.retain(|(x, _, _)| *x != key);
                    list.push((key, v, w));
                }
            }
        }
    }
    if s.satin {
        if has_fill {
            s.region_type = Some("satin");
        } else {
            s.line_type = Some("zigzag");
        }
    }
    (s, unknown)
}

/// The pieces of `c` that a dash pattern draws, as open contours (curves flattened). `None`
/// when the pattern is unusable or gives too many pieces.
pub fn dash(c: &Contour, pattern: &[f64], offset: f64) -> Option<Vec<Contour>> {
    let mut pat: Vec<f64> = pattern.to_vec();
    if pat.len() % 2 == 1 {
        pat.extend_from_slice(pattern);
    }
    let period: f64 = pat.iter().sum();
    if pat.is_empty() || period <= 1e-6 || pat.iter().any(|v| !v.is_finite() || *v < 0.0) {
        return None;
    }
    let path = c.path();
    // Where in the pattern the path starts.
    let mut pos = offset.rem_euclid(period);
    let mut k = 0;
    while pos >= pat[k] {
        pos -= pat[k];
        k = (k + 1) % pat.len();
    }
    let mut left = pat[k] - pos;
    let mut on = k % 2 == 0;
    let mut pieces: Vec<Vec<Vec2>> = Vec::new();
    let mut current: Vec<Vec2> = if on { vec![path[0]] } else { Vec::new() };
    for w in path.windows(2) {
        let (mut a, b) = (w[0], w[1]);
        let mut seg = a.distance(b);
        while seg > 1e-9 {
            if left >= seg {
                left -= seg;
                if on {
                    current.push(b);
                }
                seg = 0.0;
            } else {
                let p = a + (b - a) * (left / seg);
                seg -= left;
                a = p;
                if on {
                    current.push(p);
                    pieces.push(std::mem::take(&mut current));
                    if pieces.len() > MAX_DASHES {
                        return None;
                    }
                } else {
                    current = vec![p];
                }
                on = !on;
                k = (k + 1) % pat.len();
                left = pat[k];
            }
        }
    }
    if on && current.len() >= 2 {
        pieces.push(current);
    }
    Some(
        pieces
            .into_iter()
            .filter(|p| p.len() >= 2 && p[0].distance(p[p.len() - 1]) > 1e-6)
            .map(|p| Contour::new(p, false))
            .collect(),
    )
}

/// The report of a settings import.
#[derive(Default)]
pub struct Report {
    pub skipped: Vec<String>,
    pub unknown: Vec<Value>,
    pub invalid: Vec<Value>,
    pub dashed: usize,
    pub applied: usize,
}

/// Turns parsed shapes into the shapes to place and the settings of each: `skip` shapes go,
/// dashed strokes become one shape per dash, `paint-order: stroke` puts the stroke shape
/// first.
pub fn prepare(
    shapes: Vec<SvgShape>,
    names: &HashMap<String, (String, bool)>,
    report: &mut Report,
) -> Vec<(SvgShape, Settings)> {
    let mut out = Vec::new();
    for mut shape in shapes {
        let chain: Vec<(String, bool)> = shape
            .names
            .iter()
            .map(|id| names.get(id).cloned().unwrap_or((id.clone(), true)))
            .collect();
        let id = shape.names.last().cloned().unwrap_or_default();
        let closed = shape.contours.iter().all(|c| c.closed);
        let has_fill = shape.fill.is_some() && closed;
        let (settings, unknown) = settings_for(&chain, has_fill);
        for w in unknown {
            report.unknown.push(json!({ "id": id, "token": w }));
        }
        if settings.skip {
            report.skipped.push(id);
            continue;
        }
        if settings.skip_line {
            shape.stroke = None;
            if shape.fill.is_none() {
                report.skipped.push(id);
                continue;
            }
        }
        let fill_part = SvgShape {
            stroke: None,
            ..shape.clone()
        };
        let mut stroke_parts = Vec::new();
        if shape.stroke.is_some() && (shape.dashes.is_some() || (has_fill && shape.stroke_first)) {
            let pieces = shape.dashes.as_ref().and_then(|(p, o)| {
                let mut all = Vec::new();
                for c in &shape.contours {
                    all.extend(dash(c, p, *o)?);
                }
                Some(all)
            });
            match pieces {
                Some(pieces) => {
                    report.dashed += 1;
                    for c in pieces {
                        stroke_parts.push(SvgShape {
                            contours: vec![c],
                            fill: None,
                            ..shape.clone()
                        });
                    }
                }
                None => stroke_parts.push(SvgShape {
                    fill: None,
                    ..shape.clone()
                }),
            }
        }
        if stroke_parts.is_empty() {
            out.push((shape, settings));
            continue;
        }
        let fill = shape.fill.is_some().then_some(fill_part);
        let line_only = Settings {
            region: Vec::new(),
            region_type: None,
            ..settings.clone()
        };
        let region_only = Settings {
            line: Vec::new(),
            line_type: None,
            ..settings.clone()
        };
        if shape.stroke_first {
            out.extend(stroke_parts.into_iter().map(|s| (s, line_only.clone())));
            out.extend(fill.map(|f| (f, region_only.clone())));
        } else {
            out.extend(fill.map(|f| (f, region_only.clone())));
            out.extend(stroke_parts.into_iter().map(|s| (s, line_only.clone())));
        }
    }
    out
}

/// Runs `id` with `params` as the user would.
fn run(s: &mut EngineState, id: &str, params: Value) -> Result<Value, CommandError> {
    let spec = crate::commands::all()
        .into_iter()
        .find(|c| c.id == id)
        .ok_or_else(|| CommandError::UnknownCommand(id.into()))?;
    let p = Params::new(id, Some(&params))?;
    (spec.run)(s, &p)
}

/// Applies each object's settings: the sew type first, then each setting on its own, so a
/// refused value is listed and the others still apply.
pub fn apply(s: &mut EngineState, placed: &[(u64, Settings)], report: &mut Report) {
    for (object, set) in placed {
        let has_region = s
            .design
            .index_of(*object)
            .is_some_and(|i| s.design.objects[i].region.is_some());
        let mut any = false;
        for (command, kind, values) in [
            ("object.setRegion", set.region_type, &set.region),
            ("object.setLine", set.line_type, &set.line),
        ] {
            if command == "object.setRegion"
                && !has_region
                && (kind.is_some() || !values.is_empty())
            {
                for (_, _, w) in values {
                    report.invalid.push(
                        json!({ "id": object, "token": w, "reason": "the shape has no fill" }),
                    );
                }
                continue;
            }
            let mut params: Vec<(Map<String, Value>, String)> = Vec::new();
            if let Some(k) = kind {
                let mut m = Map::new();
                m.insert("type".into(), json!(k));
                params.push((m, k.to_string()));
            }
            for (key, v, w) in values {
                let mut m = Map::new();
                m.insert((*key).into(), v.clone());
                params.push((m, w.clone()));
            }
            for (mut m, word) in params {
                m.insert("ids".into(), json!([object]));
                match run(s, command, Value::Object(m)) {
                    Ok(_) => any = true,
                    Err(e) => report
                        .invalid
                        .push(json!({ "id": object, "token": word, "reason": e.to_string() })),
                }
            }
        }
        if any {
            report.applied += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::v2;

    fn named(n: &[&str]) -> Vec<(String, bool)> {
        n.iter().map(|s| (s.to_string(), false)).collect()
    }

    #[test]
    fn words_set_types_and_keys_and_inner_names_win() {
        let (s, unknown) = settings_for(&named(&["Fill a=45 s=0.4", "a=30 c=#ff0000"]), true);
        assert!(unknown.is_empty());
        assert_eq!(s.region_type, Some("fill"));
        let keys: Vec<(&str, Value)> = s.region.iter().map(|(k, v, _)| (*k, v.clone())).collect();
        assert_eq!(
            keys,
            [
                ("density", json!(2.5)),
                ("angle", json!(30.0)),
                ("color", json!("#ff0000"))
            ]
        );
        let (s, _) = settings_for(&named(&["satin"]), true);
        assert_eq!((s.region_type, s.line_type), (Some("satin"), None));
        let (s, _) = settings_for(&named(&["satin"]), false);
        assert_eq!((s.region_type, s.line_type), (None, Some("zigzag")));
        let (s, _) = settings_for(&named(&["run r=3 l=2"]), false);
        assert_eq!(s.line_type, Some("running"));
        assert_eq!(s.line[0].0, "runTimes");
        assert_eq!(s.line[1].0, "pitch");
        let (s, _) = settings_for(&named(&["fill stroke.satin stroke.w=3"]), true);
        assert_eq!((s.region_type, s.line_type), (Some("fill"), Some("zigzag")));
        assert_eq!(s.line[0].0, "width");
    }

    /// #455: the long key names mean what the short ones do.
    #[test]
    fn long_key_names_are_the_short_ones() {
        let pairs = [
            ("fill angle=45 spacing=0.4", "fill a=45 s=0.4"),
            (
                "fill density=4.5 pullCompensation=0.3 underlay=off",
                "fill d=4.5 pc=0.3 u=off",
            ),
            ("fill colour=#ff0000", "fill c=#ff0000"),
            ("run repeats=3 length=2.5", "run r=3 l=2.5"),
            ("run runTimes=3 stitchLength=2.5", "run r=3 l=2.5"),
            (
                "fill stroke.satin stroke.width=3",
                "fill stroke.satin stroke.w=3",
            ),
        ];
        // The same settings; each keeps the word as written, for the report.
        let keys = |s: &Settings| {
            let kv = |v: &[(&'static str, Value, String)]| -> Vec<(&'static str, Value)> {
                v.iter().map(|(k, v, _)| (*k, v.clone())).collect()
            };
            (
                s.region_type,
                s.line_type,
                s.satin,
                kv(&s.region),
                kv(&s.line),
            )
        };
        for (long, short) in pairs {
            for fill in [true, false] {
                let (a, unknown) = settings_for(&named(&[long]), fill);
                let (b, _) = settings_for(&named(&[short]), fill);
                assert_eq!(keys(&a), keys(&b), "{long} vs {short}");
                assert!(unknown.is_empty(), "{long}: {unknown:?}");
            }
        }
        // A long name with a bad value is refused as the short one is.
        assert_eq!(settings_for(&named(&["fill angle=x"]), true).1, ["angle=x"]);
    }

    #[test]
    fn plain_ids_are_ignored_and_unknown_words_listed() {
        let ids = vec![("layer1".to_string(), true), ("path1234".to_string(), true)];
        assert_eq!(settings_for(&ids, true), (Settings::default(), vec![]));
        let (s, unknown) = settings_for(&named(&["fill wobble a=x"]), true);
        assert_eq!(s.region_type, Some("fill"));
        assert_eq!(unknown, ["wobble", "a=x"]);
    }

    #[test]
    fn illustrator_ids_are_decoded() {
        assert_eq!(decode("fill_a_x3D_45"), "fill_a=45");
        assert_eq!(words(&decode("fill_a_x3D_45"), true), ["fill", "a=45"]);
        assert_eq!(decode("_xZZ_ok"), "_xZZ_ok");
        let svg = br#"<svg xmlns="http://www.w3.org/2000/svg" xmlns:inkscape="http://www.inkscape.org/namespaces/inkscape"><g id="layer1" inkscape:label="Fill a=45"/><path id="p" data-name="run r=3"/><path id="satin_x20_w_x3D_2"/></svg>"#;
        let n = names_by_id(svg);
        assert_eq!(n["layer1"], ("Fill a=45".to_string(), false));
        assert_eq!(n["p"], ("run r=3".to_string(), false));
        assert_eq!(n["satin_x20_w_x3D_2"], ("satin w=2".to_string(), true));
    }

    #[test]
    fn dashes_cut_a_line_into_pieces() {
        let line = Contour::new(vec![v2(0.0, 0.0), v2(90.0, 0.0)], false);
        let d = dash(&line, &[10.0, 10.0], 0.0).expect("dashes");
        assert_eq!(d.len(), 5);
        assert_eq!(d[0].points, [v2(0.0, 0.0), v2(10.0, 0.0)]);
        assert_eq!(d[4].points, [v2(80.0, 0.0), v2(90.0, 0.0)]);
        // An offset starts inside the pattern; an odd list repeats.
        let d = dash(&line, &[10.0, 10.0], 5.0).expect("dashes");
        assert_eq!(d[0].points, [v2(0.0, 0.0), v2(5.0, 0.0)]);
        assert_eq!(dash(&line, &[10.0], 0.0).map(|d| d.len()), Some(5));
        assert!(dash(&line, &[0.0, 0.0], 0.0).is_none());
        assert!(dash(&line, &[0.001, 0.001], 0.0).is_none());
    }
}
