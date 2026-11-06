//! `stitchtool bench-assistant` (B17 A5, docs/research/beyond/ai-integration.md section 5): fixed
//! embroidery tasks sent to a model or coding agent through the assistant's own tools, each
//! scored on the design it leaves (`object.list`, `design.info`), never on the model's words.
//!
//! The editor is headless: an engine [`Session`] behind `openstitch-mcp --assistant`, with
//! `ui.render`, `ui.scratchWrite` and the notes commands answered here. The Assistant's notes
//! (#491) carry from one task to the next, as they do between conversations in the editor, so
//! `remember` then `remembered-heart` checks the memory. A coding agent reaches the same session
//! through `openstitch-mcp --assistant --endpoint` on a private loopback socket.

use std::path::{Path, PathBuf};

use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use base64::Engine;
use openstitch_assistant as ai;
use openstitch_assistant::notes::{MAX_NOTES_BYTES, Notes, WriteMode};
use openstitch_engine::Session;
use serde_json::{Value, json};

/// One task: the design it starts from, what is asked, and what the design must show after.
pub struct Task {
    pub id: &'static str,
    setup: &'static [(&'static str, &'static str)],
    pub prompt: &'static str,
    check: fn(&Design) -> Result<(), String>,
}

/// What a task is scored on.
#[derive(Default)]
pub struct Design {
    pub objects: Vec<Value>,
    pub info: Value,
    /// The reply text, for the one task that asks a question.
    pub reply: String,
    /// The Assistant's notes after the task (#491).
    pub notes: String,
    /// Each object's `object.get`, in `objects` order (#493: fill angles, line repeats).
    pub full: Vec<Value>,
    /// `design.check` after the task (#493: the sew_check-like rules).
    pub check: Value,
    /// `fonts.list` (#493: the small-lettering task's size ranges).
    pub fonts: Value,
    /// Every tool call, `name input`, uncut (#493: the Auto Punch task's plan and override).
    pub calls: Vec<String>,
}

impl Design {
    fn sizes(&self) -> Vec<(f64, f64)> {
        let f = |o: &Value, k: &str| o["bounds"][k].as_f64().unwrap_or(0.0);
        self.objects
            .iter()
            .map(|o| (f(o, "width"), f(o, "height")))
            .collect()
    }
    fn colors(&self) -> Vec<String> {
        let mut c: Vec<String> = self
            .objects
            .iter()
            .flat_map(|o| [o["regionColor"].as_str(), o["lineColor"].as_str()])
            .flatten()
            .map(str::to_string)
            .collect();
        c.sort();
        c.dedup();
        c
    }
    fn has(&self, key: &str, value: &str) -> bool {
        self.objects.iter().any(|o| {
            o[key]
                .as_str()
                .is_some_and(|v| v.eq_ignore_ascii_case(value))
        })
    }
    fn text(&self) -> String {
        self.objects
            .iter()
            .filter_map(|o| o["text"].as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn stitches(&self) -> u64 {
        self.info["stitchStats"]["stitches"].as_u64().unwrap_or(0)
    }
}

fn need(ok: bool, what: &str) -> Result<(), String> {
    if ok { Ok(()) } else { Err(what.to_string()) }
}

/// A colour's red, green and blue (0-255).
fn rgb(hex: &str) -> [i32; 3] {
    let v = |i: usize| {
        u8::from_str_radix(
            hex.trim_start_matches('#').get(i..i + 2).unwrap_or("00"),
            16,
        )
        .unwrap_or(0) as i32
    };
    [v(0), v(2), v(4)]
}

/// Yellow or gold: red and green high, blue well below both.
fn yellowish(hex: &str) -> bool {
    let [r, g, b] = rgb(hex);
    r > 170 && g > 120 && b < g - 50 && b < r - 70
}

/// Brown, orange or golden: red above green above blue.
fn brownish(hex: &str) -> bool {
    let [r, g, b] = rgb(hex);
    r > 90 && r > g + 15 && g > b + 15
}

/// Near black or a very dark colour.
fn darkish(hex: &str) -> bool {
    rgb(hex).iter().all(|&c| c < 90)
}

/// Near white (the fabric).
fn whitish(hex: &str) -> bool {
    rgb(hex).iter().all(|&c| c > 225)
}

/// The design's extent: the larger side of the union of every object's bounds, mm.
fn extent(d: &Design) -> f64 {
    let b = |o: &Value, k: &str| o["bounds"][k].as_f64();
    let boxes: Vec<[f64; 4]> = d
        .objects
        .iter()
        .filter_map(|o| {
            let (x, y) = (b(o, "x")?, b(o, "y")?);
            Some([x, y, x + b(o, "width")?, y + b(o, "height")?])
        })
        .collect();
    if boxes.is_empty() {
        return 0.0;
    }
    let lo = |i: usize| boxes.iter().map(|q| q[i]).fold(f64::INFINITY, f64::min);
    let hi = |i: usize| boxes.iter().map(|q| q[i]).fold(f64::NEG_INFINITY, f64::max);
    (hi(2) - lo(0)).max(hi(3) - lo(1))
}

/// The fill regions: `(bounds [x0, y0, x1, y1], angle in degrees)`.
fn fills(d: &Design) -> Vec<([f64; 4], f64)> {
    d.objects
        .iter()
        .zip(d.full.iter())
        .filter(|(o, _)| o["region"] == "fill")
        .filter_map(|(o, f)| {
            let b = |k: &str| o["bounds"][k].as_f64();
            let (x, y) = (b("x")?, b("y")?);
            Some((
                [x, y, x + b("width")?, y + b("height")?],
                f["region"]["angle"].as_f64().unwrap_or(45.0),
            ))
        })
        .collect()
}

/// How far apart two sewing directions are, 0-90°.
fn angle_gap(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(180.0);
    d.min(180.0 - d)
}

/// Fill pairs whose bounds touch (within 0.5 mm) but whose rows run within 20° of each other
/// (#493: touching panels must read apart). A fill wholly inside another's bounds (a cheek on a
/// face) is a detail on it, not a neighbour, so it is not counted.
fn same_angle_neighbours(d: &Design) -> Vec<(usize, usize)> {
    let f = fills(d);
    let inside =
        |a: &[f64; 4], b: &[f64; 4]| a[0] >= b[0] && a[1] >= b[1] && a[2] <= b[2] && a[3] <= b[3];
    let mut out = Vec::new();
    for i in 0..f.len() {
        for j in i + 1..f.len() {
            let (a, b) = (&f[i].0, &f[j].0);
            let touch = a[0] <= b[2] + 0.5
                && b[0] <= a[2] + 0.5
                && a[1] <= b[3] + 0.5
                && b[1] <= a[3] + 0.5;
            if touch && !inside(a, b) && !inside(b, a) && angle_gap(f[i].1, f[j].1) < 20.0 {
                out.push((i, j));
            }
        }
    }
    out
}

/// The sewing checks a machine sew-out shows (tools/sew_check.py's rules, as `design.check`
/// errors: long stitches, untrimmed long jumps, out of the hoop).
fn sews_clean(d: &Design) -> Result<(), String> {
    let errors: Vec<String> = d.check["findings"]
        .as_array()
        .map(|f| {
            f.iter()
                .filter(|f| f["severity"] == "error")
                .map(|f| f["message"].as_str().unwrap_or("?").to_string())
                .collect()
        })
        .unwrap_or_default();
    need(
        errors.is_empty(),
        &format!("design.check without errors (has {errors:?})"),
    )
}

/// The structural rules every illustrated task shares (#493): at least `parts` objects, the
/// size (largest side) within `size`, touching fills apart in angle, and clean sewing.
fn illustrated(
    d: &Design,
    parts: usize,
    size: std::ops::RangeInclusive<f64>,
) -> Result<(), String> {
    need(
        d.objects.len() >= parts,
        &format!("at least {parts} parts (has {})", d.objects.len()),
    )?;
    need(
        size.contains(&extent(d)),
        &format!(
            "{:.0}-{:.0} mm across (is {:.1})",
            size.start(),
            size.end(),
            extent(d)
        ),
    )?;
    let same = same_angle_neighbours(d);
    need(
        same.is_empty(),
        &format!(
            "touching fills at least 20° apart ({} pairs are not)",
            same.len()
        ),
    )?;
    sews_clean(d)
}

/// A satin edge: a zigzag (satin) line, or a satin region.
fn has_satin(d: &Design, color: fn(&str) -> bool) -> bool {
    d.objects.iter().any(|o| {
        (o["line"] == "zigzag" && o["lineColor"].as_str().is_some_and(color))
            || (o["region"] == "satin" && o["regionColor"].as_str().is_some_and(color))
    })
}

/// Objects with a line and no region (whiskers, a smile, line art).
fn open_lines(d: &Design) -> usize {
    d.objects
        .iter()
        .filter(|o| o["region"].is_null() && !o["line"].is_null())
        .count()
}

/// The design's overall width: the union of the objects' bounds (a monogram is one object per
/// letter, so the widest single object is not the design's width).
fn widest(d: &Design) -> f64 {
    let b = |o: &Value, k: &str| o["bounds"][k].as_f64();
    let spans: Vec<(f64, f64)> = d
        .objects
        .iter()
        .filter_map(|o| Some((b(o, "x")?, b(o, "x")? + b(o, "width")?)))
        .collect();
    let left = spans.iter().map(|s| s.0).fold(f64::INFINITY, f64::min);
    let right = spans.iter().map(|s| s.1).fold(f64::NEG_INFINITY, f64::max);
    if spans.is_empty() { 0.0 } else { right - left }
}

/// Whether `hex` is a Madeira Polyneon chart colour.
fn madeira_polyneon(hex: &str) -> bool {
    openstitch_core::brands::MADEIRA_POLYNEON
        .iter()
        .any(|t| t.hex().eq_ignore_ascii_case(hex))
}

/// Red-ish: R well above G and B.
fn reddish(hex: &str) -> bool {
    let v = |i: usize| {
        u8::from_str_radix(
            hex.trim_start_matches('#').get(i..i + 2).unwrap_or("00"),
            16,
        )
        .unwrap_or(0) as i32
    };
    v(0) > 140 && v(0) > v(2) + 60 && v(0) > v(4) + 60
}

/// The user's own chart for the `my-threads` task (#495): eight threads, codes that are in no
/// built-in chart, so a quoted code is either one of these or invented.
pub const SHELF: &[(&str, &str, &str)] = &[
    ("#c8102e", "7101", "Holly Red"),
    ("#0b5d1e", "7102", "Holly Green"),
    ("#4caf50", "7103", "Leaf"),
    ("#ffffff", "7104", "Snow"),
    ("#000000", "7105", "Night"),
    ("#d4a017", "7106", "Gold"),
    ("#1f2a44", "7107", "Navy"),
    ("#6b4226", "7108", "Bark"),
];

/// Green-ish: G well above R and B.
fn greenish(hex: &str) -> bool {
    let v = |i: usize| {
        u8::from_str_radix(
            hex.trim_start_matches('#').get(i..i + 2).unwrap_or("00"),
            16,
        )
        .unwrap_or(0) as i32
    };
    v(2) > 60 && v(2) > v(0) + 30 && v(2) > v(4) + 20
}

/// The catalogue numbers `reply` quotes that are in no chart (#495): numbers of 3 to 6 digits,
/// optionally after up to two letters, that are not a measurement or count (`100 mm`, `30 %`,
/// `100 × 100`, `2400 stitches`) or part of a colour (`#c8102e`) or decimal. `known` holds every
/// built-in and user chart code.
pub fn invented_codes(reply: &str, known: &std::collections::HashSet<String>) -> Vec<String> {
    const UNITS: &[&str] = &[
        "mm", "cm", "in", "inch", "inches", "px", "pixels", "stitch", "stitches", "spm", "rpm",
        "object", "objects", "colour", "colours", "color", "colors", "min", "minutes", "s", "sec",
        "seconds", "x", "bytes", "kb", "lines", "hoop", "hoops",
    ];
    let norm = |c: &str| c.trim_start_matches('0').to_ascii_lowercase();
    let known: std::collections::HashSet<String> = known.iter().map(|c| norm(c)).collect();
    let words: Vec<(usize, &str)> = reply
        .match_indices(|c: char| c.is_alphanumeric())
        .fold(Vec::<(usize, usize)>::new(), |mut spans, (at, m)| {
            match spans.last_mut() {
                Some(last) if last.1 == at => last.1 = at + m.len(),
                _ => spans.push((at, at + m.len())),
            }
            spans
        })
        .into_iter()
        .map(|(a, b)| (a, &reply[a..b]))
        .collect();
    let mut out = Vec::new();
    for (k, &(at, word)) in words.iter().enumerate() {
        let letters = word.chars().take_while(char::is_ascii_alphabetic).count();
        let digits = &word[letters..];
        if letters > 2
            || !(3..=6).contains(&digits.len())
            || !digits.bytes().all(|b| b.is_ascii_digit())
        {
            continue;
        }
        let before = reply[..at].chars().next_back();
        let end = at + word.len();
        let after = reply[end..].chars().next();
        let next_word = words
            .get(k + 1)
            .filter(|(n, _)| reply[end..*n].trim().is_empty())
            .map(|(_, w)| w.to_lowercase());
        // A decimal or thousands part (`4.500`, `1,200`) follows a digit and a point or comma.
        let part = matches!(before, Some('.' | ','))
            && reply[..at]
                .chars()
                .rev()
                .nth(1)
                .is_some_and(|c| c.is_ascii_digit());
        let measured = part
            || matches!(before, Some('#' | '×'))
            // `100 × 100`: the second number, after the sign and a space.
            || reply[..at].trim_end().ends_with('×')
            || matches!(after, Some('%' | '°' | '×'))
            || next_word.is_some_and(|w| UNITS.contains(&w.as_str()))
            || reply[end..].trim_start().starts_with('×');
        if measured {
            continue;
        }
        if !known.contains(&norm(word)) && !known.contains(&norm(digits)) {
            out.push(word.to_string());
        }
    }
    out
}

/// Every catalogue number in the built-in charts and the bench's user chart.
pub fn known_codes() -> std::collections::HashSet<String> {
    let mut known: std::collections::HashSet<String> = openstitch_engine::thread_match::builtins()
        .into_iter()
        .flat_map(|b| b.entries.into_iter())
        .filter_map(|(_, t)| t.catalog_number.map(|c| c.to_string()))
        .collect();
    known.extend(SHELF.iter().map(|(_, code, _)| code.to_string()));
    known
}

const RECT: &str = r##"{"x": 20, "y": 20, "width": 30, "height": 20, "fill": "#1f3a93"}"##;
const PICTURE: &str = "{PICTURE}";
/// The Auto Punch task's picture (#493): the Higgsfield emblem, 128 px (examples/images,
/// PROVENANCE.md `d02-emblem-small.png`), written to the scratch folder as `emblem.png`.
const EMBLEM: &str = "{EMBLEM}";
const EMBLEM_PNG: &[u8] = include_bytes!("../../../examples/images/d02-emblem-small.png");

pub const TASKS: &[Task] = &[
    Task {
        id: "badge",
        setup: &[],
        prompt: "Make a 70 mm round badge: a navy fill circle with a satin border.",
        check: |d| {
            need((60.0..=80.0).contains(&widest(d)), "about 70 mm wide")?;
            need(
                d.has("region", "satin") || d.has("line", "zigzag"),
                "a satin border",
            )?;
            need(d.stitches() > 500, "stitches")
        },
    },
    Task {
        id: "monogram",
        setup: &[],
        prompt: "Make a monogram of the letters ABC, about 60 mm wide.",
        check: |d| {
            need(
                d.text().to_uppercase().contains("ABC") || d.text().to_uppercase().contains('A'),
                "lettering with ABC",
            )?;
            need((40.0..=80.0).contains(&widest(d)), "about 60 mm wide")
        },
    },
    Task {
        id: "arc-text",
        setup: &[],
        prompt: "Write OPENSTITCH around a circle about 50 mm across.",
        check: |d| {
            // The text, or (a real run, #446: Lato Bold's N sews broken) the text.add that made
            // it, once turned into shapes.
            let converted = called_steps(d)
                .iter()
                .any(|s| s["command"] == "text.toShapes")
                && called_steps(d).iter().any(|s| {
                    s["command"] == "text.add"
                        && s["params"]["text"]
                            .as_str()
                            .is_some_and(|t| t.to_uppercase().contains("OPENSTITCH"))
                });
            need(
                d.text().to_uppercase().contains("OPENSTITCH") || converted,
                "the word OPENSTITCH",
            )?;
            need(widest(d) >= 30.0, "at least 30 mm wide")
        },
    },
    Task {
        id: "applique",
        setup: &[],
        prompt: "Make an appliqué patch from a 60 mm heart.",
        check: |d| {
            need(d.objects.len() >= 2, "placement and cover objects")?;
            need((45.0..=75.0).contains(&widest(d)), "about 60 mm")
        },
    },
    Task {
        id: "star",
        setup: &[],
        prompt: "Draw a red five-pointed star 40 mm across, filled.",
        check: |d| {
            need((30.0..=50.0).contains(&widest(d)), "about 40 mm")?;
            need(d.colors().iter().any(|c| reddish(c)), "red")
        },
    },
    Task {
        id: "two-colours",
        setup: &[],
        prompt: "Make a 50 mm green filled square with a 20 mm yellow filled circle in its middle.",
        check: |d| {
            need(d.objects.len() >= 2, "two objects")?;
            need(d.colors().len() >= 2, "two colours")
        },
    },
    Task {
        id: "wider",
        setup: &[("shape.rectangle", RECT)],
        prompt: "Make the rectangle twice as wide.",
        check: |d| {
            need(
                d.sizes().iter().any(|s| (55.0..=65.0).contains(&s.0)),
                "a rectangle about 60 mm wide",
            )
        },
    },
    Task {
        id: "recolour",
        setup: &[
            ("shape.rectangle", RECT),
            (
                "shape.ellipse",
                r##"{"cx": 70, "cy": 30, "rx": 10, "fill": "#1f3a93"}"##,
            ),
        ],
        prompt: "Make every object red.",
        check: |d| {
            need(
                !d.objects.is_empty()
                    && d.objects
                        .iter()
                        .all(|o| o["regionColor"].as_str().is_none_or(reddish)),
                "every fill red",
            )
        },
    },
    Task {
        id: "delete",
        setup: &[
            ("shape.rectangle", RECT),
            ("shape.ellipse", r##"{"cx": 64, "cy": 24, "rx": 4}"##),
            ("shape.ellipse", r##"{"cx": 92, "cy": 32, "rx": 12.5}"##),
        ],
        prompt: "Delete the smallest object.",
        check: |d| {
            need(d.objects.len() == 2, "two objects left")?;
            need(d.sizes().iter().all(|s| s.0 > 9.0), "the 8 mm circle gone")
        },
    },
    Task {
        id: "picture",
        setup: &[],
        prompt: "Turn the picture at {PICTURE} into embroidery, about 50 mm wide.",
        check: |d| {
            need(!d.objects.is_empty(), "objects")?;
            need((35.0..=65.0).contains(&widest(d)), "about 50 mm")?;
            need(d.colors().iter().any(|c| reddish(c)), "the red disc")
        },
    },
    Task {
        id: "vector",
        setup: &[],
        prompt: "Draw a simple house (walls, roof, door) as an SVG and import it as shapes.",
        check: |d| need(d.objects.len() >= 2, "several shapes"),
    },
    Task {
        id: "question",
        setup: &[
            ("shape.rectangle", RECT),
            ("shape.ellipse", r##"{"cx": 70, "cy": 30, "rx": 10}"##),
        ],
        prompt: "How many objects does the design have? Do not change anything.",
        check: |d| {
            need(d.objects.len() == 2, "the design unchanged")?;
            need(
                d.reply.contains('2') || d.reply.to_lowercase().contains("two"),
                "the answer 2",
            )
        },
    },
    // #495: the user's own threads, and no invented catalogue numbers. Before `remember`, so
    // the notes do not yet say Madeira.
    Task {
        id: "my-threads",
        setup: &[
            ("threads.createChart", r#"{"name": "My shelf"}"#),
            (
                "threads.addColor",
                r##"{"chart": "My shelf", "color": "#c8102e", "code": "7101", "description": "Holly Red"}"##,
            ),
            (
                "threads.addColor",
                r##"{"chart": "My shelf", "color": "#0b5d1e", "code": "7102", "description": "Holly Green"}"##,
            ),
            (
                "threads.addColor",
                r##"{"chart": "My shelf", "color": "#4caf50", "code": "7103", "description": "Leaf"}"##,
            ),
            (
                "threads.addColor",
                r##"{"chart": "My shelf", "color": "#ffffff", "code": "7104", "description": "Snow"}"##,
            ),
            (
                "threads.addColor",
                r##"{"chart": "My shelf", "color": "#000000", "code": "7105", "description": "Night"}"##,
            ),
            (
                "threads.addColor",
                r##"{"chart": "My shelf", "color": "#d4a017", "code": "7106", "description": "Gold"}"##,
            ),
            (
                "threads.addColor",
                r##"{"chart": "My shelf", "color": "#1f2a44", "code": "7107", "description": "Navy"}"##,
            ),
            (
                "threads.addColor",
                r##"{"chart": "My shelf", "color": "#6b4226", "code": "7108", "description": "Bark"}"##,
            ),
        ],
        prompt: "Make a red and green holly badge about 60 mm wide with my threads, then tell me which threads to put on the machine.",
        check: |d| {
            need(d.objects.len() >= 2, "a badge of several objects")?;
            let mine = |c: &str| SHELF.iter().any(|(hex, ..)| hex.eq_ignore_ascii_case(c));
            let colors = d.colors();
            need(
                colors.iter().all(|c| mine(c)),
                "every colour from the user's chart",
            )?;
            need(
                colors.iter().any(|c| reddish(c)) && colors.iter().any(|c| greenish(c)),
                "red and green",
            )?;
            let invented = invented_codes(&d.reply, &known_codes());
            need(
                invented.is_empty(),
                &format!("no invented catalogue numbers (quoted {invented:?})"),
            )
        },
    },
    // #491: a pair. The first conversation is told a lasting fact; the second, a new
    // conversation, must use it without being told again.
    Task {
        id: "remember",
        setup: &[],
        prompt: "Please remember for our later conversations that I only own Madeira Polyneon threads. There is nothing to embroider now.",
        check: |d| {
            need(
                d.notes.to_lowercase().contains("madeira"),
                "Madeira Polyneon in the notes",
            )?;
            need(d.objects.is_empty(), "nothing drawn")
        },
    },
    Task {
        id: "remembered-heart",
        setup: &[],
        prompt: "Make a red heart about 60 mm wide.",
        check: |d| {
            need(!d.objects.is_empty(), "a heart")?;
            need(
                d.colors().iter().any(|c| reddish(c) && madeira_polyneon(c)),
                "a red Madeira Polyneon thread (the remembered brand)",
            )
        },
    },
    // #493: illustrated subjects, scored on structure (and on the render with --judge).
    Task {
        id: "strawberry",
        setup: &[],
        prompt: "Make a strawberry with green leaves and small yellow seeds, about 60 mm tall.",
        check: |d| {
            illustrated(d, 5, 45.0..=75.0)?;
            let c = d.colors();
            need(c.iter().any(|c| reddish(c)), "a red berry")?;
            need(c.iter().any(|c| greenish(c)), "green leaves")?;
            need(
                d.objects
                    .iter()
                    .filter(|o| {
                        o["regionColor"].as_str().is_some_and(yellowish)
                            && o["bounds"]["width"].as_f64().unwrap_or(99.0) < 6.0
                    })
                    .count()
                    >= 3,
                "at least 3 small yellow seeds",
            )
        },
    },
    Task {
        id: "mushroom",
        setup: &[],
        prompt: "Make a mushroom about 60 mm wide: a red cap made of three panels sewn in different fill directions, on a pale stem.",
        check: |d| {
            illustrated(d, 4, 45.0..=75.0)?;
            let red: Vec<f64> = d
                .objects
                .iter()
                .zip(d.full.iter())
                .filter(|(o, _)| {
                    o["region"] == "fill" && o["regionColor"].as_str().is_some_and(reddish)
                })
                .map(|(_, f)| f["region"]["angle"].as_f64().unwrap_or(45.0))
                .collect();
            need(
                red.len() >= 3,
                &format!("three red cap fills (has {})", red.len()),
            )?;
            let mut distinct: Vec<f64> = Vec::new();
            for a in red {
                if distinct.iter().all(|&b| angle_gap(a, b) >= 20.0) {
                    distinct.push(a);
                }
            }
            need(
                distinct.len() >= 3,
                "three cap panels in different directions (20° apart)",
            )
        },
    },
    Task {
        id: "watermelon",
        setup: &[],
        prompt: "Make a watermelon slice about 70 mm wide: red flesh with black seeds and a green satin rind.",
        check: |d| {
            illustrated(d, 3, 55.0..=85.0)?;
            let c = d.colors();
            need(c.iter().any(|c| reddish(c)), "red flesh")?;
            need(c.iter().any(|c| darkish(c)), "black seeds")?;
            need(has_satin(d, greenish), "a green satin rind")
        },
    },
    Task {
        id: "lightbulb",
        setup: &[],
        prompt: "Make a yellow lightbulb with a smiling face, about 55 mm tall.",
        check: |d| {
            illustrated(d, 4, 40.0..=70.0)?;
            let c = d.colors();
            need(c.iter().any(|c| yellowish(c)), "a yellow bulb")?;
            need(c.iter().any(|c| darkish(c)), "a dark face")
        },
    },
    Task {
        id: "croissant",
        setup: &[],
        prompt: "Make a croissant about 70 mm wide in golden browns, its segments separated by satin outlines.",
        check: |d| {
            illustrated(d, 3, 55.0..=85.0)?;
            need(
                d.objects.iter().filter(|o| o["region"] == "fill").count() >= 3,
                "at least 3 filled segments",
            )?;
            need(d.colors().iter().any(|c| brownish(c)), "golden browns")?;
            need(has_satin(d, |_| true), "satin outlines")
        },
    },
    Task {
        id: "cat-face",
        setup: &[],
        prompt: "Make a cartoon cat face about 60 mm wide with ears, eyes, a nose and whiskers.",
        check: |d| {
            illustrated(d, 6, 45.0..=75.0)?;
            need(open_lines(d) >= 2, "whiskers as lines")
        },
    },
    // #493 (from #637): Auto Punch through its plan and overrides, on a Higgsfield test image.
    Task {
        id: "auto-punch",
        setup: &[],
        prompt: "Digitize the emblem picture at {EMBLEM} with Auto Punch, about 60 mm wide. Look at its region plan first, then sew the largest region with fill rows at 30 degrees, and leave the white background unsewn as fabric.",
        check: |d| {
            let punch: Vec<&String> = d
                .calls
                .iter()
                .filter(|c| c.contains("digitize.autoPunch"))
                .collect();
            need(
                punch.iter().any(|c| c.contains("dryRun")),
                "the region plan read first (dryRun)",
            )?;
            need(
                punch.iter().any(|c| c.contains("overrides")),
                "an override of the plan",
            )?;
            need(d.objects.len() >= 3, "the emblem's regions")?;
            need((50.0..=70.0).contains(&widest(d)), "about 60 mm wide")?;
            need(
                !d.objects
                    .iter()
                    .any(|o| o["regionColor"].as_str().is_some_and(whitish)),
                "the white background left unsewn",
            )?;
            need(
                d.full
                    .iter()
                    .any(|f| (f["region"]["angle"].as_f64().unwrap_or(-1.0) - 30.0).abs() < 1.0),
                "a region at 30°",
            )
        },
    },
    // #493 (from #639): small lettering in bean stitch, a Small Text font within its range.
    Task {
        id: "small-name",
        setup: &[],
        prompt: "Write the name Mia about 5 mm tall in bean stitch, for a baby's bib label.",
        check: |d| {
            // The text; or one text per letter, left to right (a real run, to trim between
            // letters, #548); or, once turned into shapes (`text.toShapes`, a real run: to give
            // the i's dot its own trim), the `text.add` that made it and every line it left.
            let mut texts: Vec<(&Value, &Value)> = d
                .objects
                .iter()
                .zip(d.full.iter())
                .filter(|(o, _)| o["text"].is_string())
                .collect();
            texts.sort_by(|a, b| {
                let x = |o: &Value| o["bounds"]["x"].as_f64().unwrap_or(0.0);
                x(a.0).total_cmp(&x(b.0))
            });
            let whole = texts
                .iter()
                .find(|(o, _)| o["text"].as_str().is_some_and(|t| t.contains("Mia")));
            let joined: String = texts
                .iter()
                .filter_map(|(o, _)| o["text"].as_str())
                .collect();
            let used: Vec<(String, f64, bool)> = match whole {
                Some(t) => vec![*t],
                None if joined.contains("Mia") => texts,
                None => Vec::new(),
            }
            .into_iter()
            .map(|(o, f)| {
                (
                    o["font"].as_str().unwrap_or("").to_string(),
                    o["sizeMM"].as_f64().unwrap_or(0.0),
                    o["line"] == "running" && f["line"]["runTimes"].as_u64() == Some(3),
                )
            })
            .collect();
            let used = if used.is_empty() {
                let steps = called_steps(d);
                need(
                    steps.iter().any(|s| s["command"] == "text.toShapes"),
                    "the text Mia",
                )?;
                let add = steps
                    .iter()
                    .rev()
                    .find(|s| {
                        s["command"] == "text.add"
                            && s["params"]["text"]
                                .as_str()
                                .is_some_and(|t| t.contains("Mia"))
                    })
                    .ok_or("the text Mia")?;
                let lines: Vec<(&Value, &Value)> = d
                    .objects
                    .iter()
                    .zip(d.full.iter())
                    .filter(|(o, _)| !o["line"].is_null())
                    .collect();
                vec![(
                    add["params"]["font"].as_str().unwrap_or("").to_string(),
                    add["params"]["size"].as_f64().unwrap_or(0.0),
                    !lines.is_empty()
                        && lines.iter().all(|(o, f)| {
                            o["line"] == "running" && f["line"]["runTimes"].as_u64() == Some(3)
                        }),
                )]
            } else {
                used
            };
            for (font, size, bean) in &used {
                small_name_part(d, font, *size, *bean)?;
            }
            Ok(())
        },
    },
];

/// One part of the small-name task (#493): about 5 mm, in a font whose `sizeRange` holds the
/// size, sewn as bean stitch.
fn small_name_part(d: &Design, font: &str, size: f64, bean: bool) -> Result<(), String> {
    need(
        (3.0..=7.0).contains(&size),
        &format!("about 5 mm (is {size})"),
    )?;
    let entry = d.fonts["fonts"]
        .as_array()
        .or(d.fonts.as_array())
        .and_then(|l| l.iter().find(|e| e["id"] == font))
        .cloned()
        .unwrap_or_default();
    let range = &entry["sizeRange"];
    let (lo, hi) = (
        range["minMm"].as_f64().unwrap_or(f64::NAN),
        range["maxMm"].as_f64().unwrap_or(f64::NAN),
    );
    need(
        (lo..=hi).contains(&size),
        &format!("a font whose sizeRange holds the size ({font}: {lo}-{hi} mm)"),
    )?;
    need(bean, "bean stitch (a running line, runTimes 3)")
}

/// Every engine command the model sent, `{command, params}`, from `execute` and `run_batch`
/// calls in order (tool names may carry an MCP prefix).
fn called_steps(d: &Design) -> Vec<Value> {
    d.calls
        .iter()
        .filter_map(|c| {
            let (name, input) = c.split_once(' ')?;
            let input: Value = serde_json::from_str(input).ok()?;
            Some(if name.ends_with("run_batch") {
                input["steps"].as_array().cloned().unwrap_or_default()
            } else if name.ends_with("execute") {
                vec![input]
            } else {
                Vec::new()
            })
        })
        .flatten()
        .collect()
}

/// The headless editor: engine commands, plus the `ui.*` commands the meta tools use.
pub struct Headless {
    pub session: Session,
    pub scratch: PathBuf,
    /// The Assistant's notes (#491); the bench has no permission gate, so writes go through.
    pub notes: Notes,
}

impl Headless {
    pub fn new(scratch: PathBuf) -> Headless {
        Headless {
            session: Session::new(&scratch),
            scratch,
            notes: Notes::default(),
        }
    }

    fn notes_json(&self) -> Value {
        json!({ "text": self.notes.text(), "bytes": self.notes.bytes(), "limit": MAX_NOTES_BYTES })
    }

    fn notes_write(&mut self, p: &Value) -> Result<Value, String> {
        let mode = match p["mode"].as_str() {
            None => WriteMode::Append,
            Some(m) => WriteMode::from_key(m).ok_or("mode takes append or replace")?,
        };
        self.notes.write(
            p["section"].as_str().unwrap_or(""),
            p["text"].as_str().unwrap_or(""),
            mode,
        )?;
        Ok(self.notes_json())
    }

    pub fn handle(&mut self, request: &Value) -> Value {
        let params = &request["params"];
        let result = match request["command"].as_str().unwrap_or("") {
            "ui.render" => {
                let d = self.session.design();
                let pattern = openstitch_design::Compiler::for_design(d).compile(d);
                let size = params["size"].as_u64().unwrap_or(800).clamp(64, 2048) as u32;
                openstitch_render::preview::png(d, &pattern, size, false).map(|png| {
                    let stats = self.session.execute("design.info", None).map(|i| i["stitchStats"].clone()).unwrap_or_default();
                    json!({ "png": base64::engine::general_purpose::STANDARD.encode(png), "stats": stats })
                })
            }
            "ui.scratchWrite" => self.scratch_write(params),
            "ui.notesRead" => Ok(self.notes_json()),
            "ui.notesWrite" => self.notes_write(params),
            _ => return self.session.handle(request),
        };
        match result {
            Ok(v) => json!({ "ok": true, "result": v }),
            Err(e) => json!({ "ok": false, "error": e }),
        }
    }

    fn scratch_write(&self, p: &Value) -> Result<Value, String> {
        let name = p["name"]
            .as_str()
            .filter(|n| {
                !n.is_empty()
                    && n.chars()
                        .all(|c| c.is_ascii_alphanumeric() || "-_. ".contains(c))
                    && !n.starts_with('.')
            })
            .ok_or("name must be a plain file name")?;
        let bytes = match (p["text"].as_str(), p["base64"].as_str()) {
            (Some(t), None) => t.as_bytes().to_vec(),
            (None, Some(b)) => base64::engine::general_purpose::STANDARD
                .decode(b.trim())
                .map_err(|_| "base64 is not valid")?,
            _ => return Err("give either text or base64".into()),
        };
        let path = self.scratch.join(name);
        std::fs::write(&path, &bytes).map_err(|e| e.to_string())?;
        Ok(json!({ "path": path.display().to_string(), "bytes": bytes.len() }))
    }
}

struct Link(Arc<Mutex<Headless>>);

impl openstitch_mcp::EditorLink for Link {
    fn handle(&mut self, request: &Value) -> Value {
        self.0
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .handle(request)
    }
}

/// The assistant server's meta tools, as the panel offers them.
struct McpTools(openstitch_mcp::Server);

impl ai::Tools for McpTools {
    fn definitions(&self) -> Vec<Value> {
        self.0
            .tools()
            .iter()
            .map(|t| t.definition.clone())
            .collect()
    }

    fn call(&mut self, name: &str, input: &Value) -> ai::ToolResult {
        match self
            .0
            .tools()
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.kind.clone())
        {
            Some(kind) => ai::from_mcp_result(&self.0.call(&kind, input)),
            None => ai::from_mcp_result(&openstitch_mcp::server::tool_failure(&format!(
                "unknown tool '{name}'"
            ))),
        }
    }
}

/// How the tasks reach a model.
pub enum Backend {
    Model {
        config: ai::Config,
        transport: Box<dyn ai::Transport>,
    },
    Agent {
        config: ai::AcpConfig,
    },
}

/// One task's outcome.
#[derive(Debug, Default)]
pub struct Outcome {
    pub task: String,
    pub passed: bool,
    pub category: String,
    pub detail: String,
    pub tool_calls: Vec<String>,
    pub tool_errors: Vec<String>,
    pub reply: String,
    /// Every tool call uncut, for the checks (`tool_calls` is cut for the report).
    pub calls: Vec<String>,
    /// The judge's scores of the render (recognisable, parts, colours; 1-5 each), with --judge.
    pub judge: Option<[u8; 3]>,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub seconds: f64,
}

/// Sorts a failure (SolidCraft's categories, plan section 2.2).
fn classify(o: &Outcome, end: &ai::StopKind, check: &str) -> String {
    let errors = o.tool_errors.join(" ").to_lowercase();
    let r = o.reply.to_lowercase();
    match end {
        ai::StopKind::Error(e) => format!("request failed: {e}"),
        _ if o.tool_calls.is_empty()
            && ["\"command\"", "execute(", "run_batch", "<tool_call>"]
                .iter()
                .any(|t| r.contains(t)) =>
        {
            "wrote the call as text".into()
        }
        _ if o.tool_calls.is_empty() => "only talked".into(),
        _ if errors.contains("unknown command") || errors.contains("unknown tool") => {
            "unknown command".into()
        }
        _ if !o.tool_errors.is_empty() && o.tool_errors.len() * 2 >= o.tool_calls.len() => {
            "gave up after errors".into()
        }
        _ if !o.tool_errors.is_empty()
            && (errors.contains("invalid")
                || errors.contains("must")
                || errors.contains("missing")) =>
        {
            "wrong arguments".into()
        }
        _ => format!("wrong result: {check}"),
    }
}

/// Runs the tasks, one fresh design and conversation each.
pub fn run(
    backend: &Backend,
    tasks: &[&Task],
    scratch: &Path,
    picture: &Path,
    judge: Option<&Backend>,
    log: &mut dyn FnMut(&Outcome),
) -> Vec<Outcome> {
    let mut out = Vec::new();
    // What the Assistant remembers, carried from task to task like conversations (#491).
    let mut notes = Notes::default();
    for task in tasks {
        let mut fresh = Headless::new(scratch.to_path_buf());
        fresh.notes = notes.clone();
        let memory = notes.prompt_block().unwrap_or_default();
        let ed = Arc::new(Mutex::new(fresh));
        let setup: Result<Vec<Value>, String> = task
            .setup
            .iter()
            .map(|(command, params)| {
                let params = serde_json::from_str::<Value>(params).map_err(|e| e.to_string())?;
                let mut ed = ed.lock().unwrap_or_else(|e| e.into_inner());
                ed.session
                    .execute(command, Some(&params))
                    .map_err(|e| e.to_string())
            })
            .collect();
        if let Err(e) = setup {
            let o = Outcome {
                task: task.id.into(),
                category: format!("setup failed: {e}"),
                ..Outcome::default()
            };
            log(&o);
            out.push(o);
            continue;
        }
        let emblem = scratch.join("emblem.png");
        if task.prompt.contains(EMBLEM) && !emblem.exists() {
            let _ = std::fs::write(&emblem, EMBLEM_PNG);
        }
        let prompt = task
            .prompt
            .replace(PICTURE, &picture.display().to_string())
            .replace(EMBLEM, &emblem.display().to_string());
        let mut o = Outcome {
            task: task.id.into(),
            ..Outcome::default()
        };
        let start = Instant::now();
        let stop = AtomicBool::new(false);
        let mut on_event = |e: ai::Event| match e {
            ai::Event::Text(t) => o.reply.push_str(&t),
            ai::Event::ToolCall { name, input, .. } => {
                o.tool_calls
                    .push(format!("{name} {}", cut(&input.to_string(), 300)));
                o.calls.push(format!("{name} {input}"));
            }
            ai::Event::ToolDone {
                is_error: true,
                summary,
                ..
            } => o.tool_errors.push(summary),
            ai::Event::Usage {
                input_tokens,
                output_tokens,
                cache_read_input_tokens,
                cache_creation_input_tokens,
            } => {
                o.input_tokens +=
                    input_tokens + cache_read_input_tokens + cache_creation_input_tokens;
                o.output_tokens += output_tokens;
            }
            _ => {}
        };
        let end = match backend {
            Backend::Model { config, transport } => {
                let server = match openstitch_mcp::Server::new(openstitch_mcp::Config::default()) {
                    Ok(s) => s.with_assistant(Link(ed.clone())),
                    Err(e) => {
                        return vec![Outcome {
                            category: format!("no tool server: {e}"),
                            ..Outcome::default()
                        }];
                    }
                };
                let mut tools = ai::WithImages {
                    inner: McpTools(server),
                    image: None,
                    transport: transport.as_ref(),
                };
                let mut config = config.clone();
                config.system.push_str(&memory);
                ai::Conversation::new().run_turn(
                    &config,
                    transport.as_ref(),
                    &mut tools,
                    ai::user_content(&prompt, &[]),
                    &stop,
                    &mut on_event,
                )
            }
            Backend::Agent { config } => agent_turn(
                &ai::AcpConfig {
                    system: format!("{}{memory}", config.system),
                    ..config.clone()
                },
                &mut ed.lock().unwrap_or_else(|e| e.into_inner()),
                &prompt,
                &mut on_event,
            ),
        };
        let mut ed = ed.lock().unwrap_or_else(|e| e.into_inner());
        o.seconds = start.elapsed().as_secs_f64();
        notes = ed.notes.clone();
        let design = scored(&mut ed.session, &o, notes.text());
        if let Some(judge) = judge {
            o.judge = judge_render(judge, &mut ed, task.prompt);
        }
        match (task.check)(&design) {
            Ok(()) if !matches!(end, ai::StopKind::Error(_)) => o.passed = true,
            Ok(()) => o.category = classify(&o, &end, ""),
            Err(why) => {
                o.category = classify(&o, &end, &why);
                o.detail = why;
            }
        }
        log(&o);
        out.push(o);
    }
    out
}

/// What a task is scored on, read from the session after the turn.
fn scored(session: &mut Session, o: &Outcome, notes: &str) -> Design {
    let objects: Vec<Value> = session
        .execute("object.list", None)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default();
    let full = objects
        .iter()
        .map(|ob| {
            session
                .execute("object.get", Some(&json!({ "id": ob["id"] })))
                .unwrap_or_default()
        })
        .collect();
    Design {
        objects,
        info: session.execute("design.info", None).unwrap_or_default(),
        reply: o.reply.clone(),
        notes: notes.to_string(),
        full,
        check: session.execute("design.check", None).unwrap_or_default(),
        fonts: session.execute("fonts.list", None).unwrap_or_default(),
        calls: o.calls.clone(),
    }
}

/// The judge's fixed rubric (#493 `--judge`): it sees the render and the request only.
pub const JUDGE_RUBRIC: &str = "You judge a machine-embroidery design from its rendered picture. \
The request was: \"{REQUEST}\". Score each from 1 (poor) to 5 (excellent): recognisable (is the \
subject recognisable), parts (are the requested parts present), colours (are the colours \
plausible for the subject). Answer with one line of JSON only, for example \
{\"recognisable\": 4, \"parts\": 3, \"colours\": 5}.";

/// The three scores from the judge's reply: the first JSON object in it with all three keys,
/// each 1-5. Deterministic for a given reply.
pub fn judge_scores(reply: &str) -> Option<[u8; 3]> {
    let mut from = 0;
    while let Some(start) = reply[from..].find('{').map(|i| i + from) {
        let end = reply[start..].find('}').map(|i| i + start + 1)?;
        if let Ok(v) = serde_json::from_str::<Value>(&reply[start..end]) {
            let score = |k: &str| {
                v[k].as_u64()
                    .filter(|n| (1..=5).contains(n))
                    .map(|n| n as u8)
            };
            if let (Some(a), Some(b), Some(c)) =
                (score("recognisable"), score("parts"), score("colours"))
            {
                return Some([a, b, c]);
            }
        }
        from = start + 1;
    }
    None
}

/// No tools: the judge only looks.
struct NoTools;

impl ai::Tools for NoTools {
    fn definitions(&self) -> Vec<Value> {
        Vec::new()
    }
    fn call(&mut self, name: &str, _: &Value) -> ai::ToolResult {
        ai::from_mcp_result(&openstitch_mcp::server::tool_failure(&format!(
            "the judge has no tool '{name}'"
        )))
    }
}

/// Asks the judge model to score the design's render (a model backend; an agent cannot judge).
fn judge_render(judge: &Backend, ed: &mut Headless, request: &str) -> Option<[u8; 3]> {
    let Backend::Model { config, transport } = judge else {
        return None;
    };
    let render = ed.handle(&json!({ "command": "ui.render", "params": { "size": 512 } }));
    let png = render["result"]["png"].as_str()?.to_string();
    let request = request
        .replace(EMBLEM, "a picture")
        .replace(PICTURE, "a picture");
    let mut reply = String::new();
    let stop = AtomicBool::new(false);
    ai::Conversation::new().run_turn(
        config,
        transport.as_ref(),
        &mut NoTools,
        ai::user_content(
            &JUDGE_RUBRIC.replace("{REQUEST}", &request),
            &[("image/png".into(), png)],
        ),
        &stop,
        &mut |e| {
            if let ai::Event::Text(t) = e {
                reply.push_str(&t);
            }
        },
    );
    judge_scores(&reply)
}

/// A coding agent's turn: it runs on its own thread while this one serves its MCP bridge's
/// requests from the private socket.
fn agent_turn(
    config: &ai::AcpConfig,
    ed: &mut Headless,
    prompt: &str,
    on_event: &mut dyn FnMut(ai::Event),
) -> ai::StopKind {
    let endpoint = ed.scratch.join("bench-endpoint.json");
    let listener = match openstitch_remote::Server::start(openstitch_remote::ServerConfig {
        write_roots: vec![ed.scratch.clone()],
        endpoint_file: Some(endpoint.clone()),
        ..openstitch_remote::ServerConfig::default()
    }) {
        Ok(l) => l,
        Err(e) => return ai::StopKind::Error(format!("no private socket: {e}")),
    };
    let mut config = config.clone();
    config.mcp.args = vec![
        "--assistant".into(),
        "--endpoint".into(),
        endpoint.display().to_string(),
    ];
    let (tx, rx) = std::sync::mpsc::channel();
    let text = prompt.to_string();
    let worker = std::thread::spawn(move || {
        let never = AtomicBool::new(false);
        let mut agent = match ai::AcpConversation::start(&config) {
            Ok(a) => a,
            Err(e) => return ai::StopKind::Error(format!("{e:?}")),
        };
        let end = agent.prompt(&text, &never, &mut |e| {
            let _ = tx.send(e);
        });
        agent.stop();
        end
    });
    while !worker.is_finished() {
        while let Ok(e) = rx.try_recv() {
            on_event(e);
        }
        if let Some(job) = listener.wait_next(Duration::from_millis(20)) {
            let reply = ed.handle(job.request());
            job.respond(reply);
        }
    }
    rx.try_iter().for_each(&mut *on_event);
    worker
        .join()
        .unwrap_or(ai::StopKind::Error("the agent thread panicked".into()))
}

fn cut(s: &str, n: usize) -> String {
    match s.char_indices().nth(n) {
        Some((i, _)) => format!("{}…", &s[..i]),
        None => s.to_string(),
    }
}

/// The JSON and Markdown reports.
pub fn reports(label: &str, outcomes: &[Outcome]) -> (Value, String) {
    let passed = outcomes.iter().filter(|o| o.passed).count();
    let (tin, tout) = outcomes
        .iter()
        .fold((0, 0), |a, o| (a.0 + o.input_tokens, a.1 + o.output_tokens));
    let json = json!({
        "backend": label, "passed": passed, "tasks": outcomes.len(), "inputTokens": tin, "outputTokens": tout,
        "results": outcomes.iter().map(|o| json!({
            "task": o.task, "passed": o.passed, "category": o.category, "detail": o.detail,
            "toolCalls": o.tool_calls, "toolErrors": o.tool_errors, "reply": o.reply, "judge": o.judge, "calls": o.calls,
            "inputTokens": o.input_tokens, "outputTokens": o.output_tokens, "seconds": (o.seconds * 10.0).round() / 10.0,
        })).collect::<Vec<_>>(),
    });
    let mut md = format!(
        "# bench-assistant: {label}\n\n{passed} of {} tasks passed; {tin} input and {tout} output tokens.\n\n| Task | Result | Tool calls (errors) | Tokens in / out | Seconds |\n|---|---|---|---|---|\n",
        outcomes.len()
    );
    for o in outcomes {
        let result = if o.passed {
            "pass".to_string()
        } else {
            format!("fail: {}", o.category)
        };
        md.push_str(&format!(
            "| {} | {} | {} ({}) | {} / {} | {:.1} |\n",
            o.task,
            match o.judge {
                Some([a, b, c]) => format!("{} (judge {a}/{b}/{c})", result.replace('|', "/")),
                None => result.replace('|', "/"),
            },
            o.tool_calls.len(),
            o.tool_errors.len(),
            o.input_tokens,
            o.output_tokens,
            o.seconds
        ));
    }
    (json, md)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::VecDeque;

    /// Replies with scripted Claude Messages streams, one per request; no network.
    struct Scripted(Mutex<VecDeque<String>>);

    impl ai::Transport for Scripted {
        fn post(
            &self,
            _: &str,
            _: &[(String, String)],
            _: &[u8],
            _: Duration,
        ) -> Result<Box<dyn std::io::Read + Send>, ai::HttpError> {
            let body = self.0.lock().expect("lock").pop_front().unwrap_or_default();
            Ok(Box::new(std::io::Cursor::new(body.into_bytes())))
        }
    }

    fn sse(data: Value) -> String {
        format!(
            "event: {}\ndata: {data}\n\n",
            data["type"].as_str().unwrap_or("")
        )
    }

    /// One assistant message: text, then optional `execute` calls.
    fn message(text: &str, calls: &[Value]) -> String {
        let mut s = sse(
            json!({"type": "message_start", "message": {"id": "m", "type": "message", "role": "assistant", "content": [],
            "model": "scripted", "stop_reason": null, "usage": {"input_tokens": 100, "output_tokens": 1}}}),
        );
        let mut blocks: Vec<Value> = vec![json!({"type": "text", "text": text})];
        blocks.extend(calls.iter().enumerate().map(|(i, c)| json!({"type": "tool_use", "id": format!("t{i}"), "name": "execute", "input": c})));
        for (i, b) in blocks.iter().enumerate() {
            let (start, delta) = if b["type"] == "text" {
                (
                    json!({"type": "text", "text": ""}),
                    json!({"type": "text_delta", "text": b["text"]}),
                )
            } else {
                (
                    json!({"type": "tool_use", "id": b["id"], "name": "execute", "input": {}}),
                    json!({"type": "input_json_delta", "partial_json": b["input"].to_string()}),
                )
            };
            s += &sse(json!({"type": "content_block_start", "index": i, "content_block": start}));
            s += &sse(json!({"type": "content_block_delta", "index": i, "delta": delta}));
            s += &sse(json!({"type": "content_block_stop", "index": i}));
        }
        let stop = if calls.is_empty() {
            "end_turn"
        } else {
            "tool_use"
        };
        s += &sse(
            json!({"type": "message_delta", "delta": {"stop_reason": stop, "stop_sequence": null}, "usage": {"output_tokens": 20}}),
        );
        s + &sse(json!({"type": "message_stop"}))
    }

    #[test]
    fn the_benchmark_scores_the_design_against_a_scripted_endpoint() {
        let replies = [
            // wider: the model draws the wider rectangle.
            message(
                "",
                &[
                    json!({"command": "shape.rectangle", "params": {"x": 20, "y": 50, "width": 60, "height": 20}}),
                ],
            ),
            message("Done.", &[]),
            // star: only talks.
            message("I would draw a red star.", &[]),
            // question: answers without changing anything.
            message("The design has 2 objects.", &[]),
        ];
        let backend = Backend::Model {
            config: ai::Config {
                api_key: "sk-test".into(),
                ..ai::Config::default()
            },
            transport: Box::new(Scripted(Mutex::new(replies.into_iter().collect()))),
        };
        let pick = |id| TASKS.iter().find(|t| t.id == id).expect("task");
        let dir = openstitch_testutil::TempDir::new("bench-test");
        let out = run(
            &backend,
            &[pick("wider"), pick("star"), pick("question")],
            &dir,
            &dir.join("none.bmp"),
            None,
            &mut |_| {},
        );
        let _ = std::fs::remove_dir_all(&dir);
        assert!(out[0].passed, "{:?}", out[0]);
        assert_eq!(out[0].tool_calls.len(), 1);
        assert_eq!(
            (out[1].passed, out[1].category.as_str()),
            (false, "only talked")
        );
        assert!(out[2].passed, "{:?}", out[2]);
        assert!(out[0].input_tokens >= 200, "usage is counted");
        let (json, md) = reports("scripted", &out);
        assert_eq!(json["passed"], 2);
        assert!(md.contains("| star | fail: only talked |"));
        assert_eq!(TASKS.len(), 23);
    }

    #[test]
    fn quoted_catalogue_numbers_are_checked_against_the_charts() {
        let known = known_codes();
        assert!(
            known.contains("7101") && known.len() > 1000,
            "{}",
            known.len()
        );
        let good = "Use Holly Red 7101 and Holly Green (7102) from My shelf; the badge is \
                    60 mm wide, 100 × 100 mm hoop, 4.5 lines/mm, colour #c8102e, 2400 stitches.";
        assert!(
            invented_codes(good, &known).is_empty(),
            "{:?}",
            invented_codes(good, &known)
        );
        let madeira = openstitch_core::brands::MADEIRA_POLYNEON[0]
            .catalog_number
            .as_deref()
            .expect("a Madeira code")
            .to_string();
        let reply = format!("Thread 7199 and Madeira {madeira}, then 7108. Also PN7123.");
        assert_eq!(invented_codes(&reply, &known), ["7199", "PN7123"]);
        assert!(
            invented_codes("about 120 mm, 30 %, 45°, 200x300, 1,250 and 4.500", &known).is_empty()
        );
        // A real reply (2026-11-06): the hoop's second side.
        assert!(invented_codes("It's set up for the 100 × 100 hoop.", &known).is_empty());
    }

    #[test]
    fn the_my_threads_task_needs_the_users_chart_and_real_codes() {
        let task = TASKS.iter().find(|t| t.id == "my-threads").expect("task");
        let obj = |c: &str| json!({"regionColor": c, "bounds": {"x": 0, "y": 0, "width": 60, "height": 60}});
        let design = |colors: &[&str], reply: &str| Design {
            objects: colors.iter().map(|c| obj(c)).collect(),
            info: json!({}),
            reply: reply.into(),
            ..Design::default()
        };
        assert!(
            (task.check)(&design(
                &["#C8102E", "#0b5d1e"],
                "Holly Red 7101, Holly Green 7102."
            ))
            .is_ok()
        );
        let off_chart = (task.check)(&design(&["#ff0000", "#0b5d1e"], "")).expect_err("not mine");
        assert!(off_chart.contains("user's chart"));
        let invented = (task.check)(&design(&["#c8102e", "#0b5d1e"], "Use Holly Red 7111."))
            .expect_err("invented");
        assert!(invented.contains("7111"), "{invented}");
    }

    /// One assistant message calling the `notes_write` meta tool.
    fn remember_call(section: &str, text: &str) -> String {
        let input = json!({ "section": section, "text": text });
        let mut s = sse(
            json!({"type": "message_start", "message": {"id": "m", "type": "message", "role": "assistant", "content": [],
            "model": "scripted", "stop_reason": null, "usage": {"input_tokens": 100, "output_tokens": 1}}}),
        );
        s += &sse(json!({"type": "content_block_start", "index": 0,
            "content_block": {"type": "tool_use", "id": "n0", "name": "notes_write", "input": {}}}));
        s += &sse(json!({"type": "content_block_delta", "index": 0,
            "delta": {"type": "input_json_delta", "partial_json": input.to_string()}}));
        s += &sse(json!({"type": "content_block_stop", "index": 0}));
        s += &sse(
            json!({"type": "message_delta", "delta": {"stop_reason": "tool_use", "stop_sequence": null}, "usage": {"output_tokens": 20}}),
        );
        s + &sse(json!({"type": "message_stop"}))
    }

    /// Records each request body, then replies like [`Scripted`].
    struct Recording(Mutex<VecDeque<String>>, Mutex<Vec<Value>>);

    impl ai::Transport for Recording {
        fn post(
            &self,
            _: &str,
            _: &[(String, String)],
            body: &[u8],
            _: Duration,
        ) -> Result<Box<dyn std::io::Read + Send>, ai::HttpError> {
            self.1
                .lock()
                .expect("lock")
                .push(serde_json::from_slice(body).unwrap_or(Value::Null));
            let next = self.0.lock().expect("lock").pop_front().unwrap_or_default();
            Ok(Box::new(std::io::Cursor::new(next.into_bytes())))
        }
    }

    /// #491: the memory pair against a scripted model. The note written in `remember` is in the
    /// system prompt of `remembered-heart`, and a Madeira red heart passes.
    #[test]
    fn the_memory_pair_carries_the_notes_into_the_next_conversation() {
        // Madeira Polyneon 1747 is a red on the chart; take its colour from the chart itself.
        let red = openstitch_core::brands::MADEIRA_POLYNEON
            .iter()
            .map(|t| t.hex())
            .find(|h| reddish(h))
            .expect("a red Madeira thread");
        let replies = [
            remember_call("Threads", "Owns only Madeira Polyneon threads"),
            message("I'll remember.", &[]),
            message(
                "",
                &[
                    json!({"command": "shape.preset", "params": {"kind": "heart", "cx": 60, "cy": 50, "width": 60, "fill": red}}),
                ],
            ),
            message("A red heart.", &[]),
        ];
        let transport = Arc::new(Recording(
            Mutex::new(replies.into_iter().collect()),
            Mutex::new(Vec::new()),
        ));
        struct Shared(Arc<Recording>);
        impl ai::Transport for Shared {
            fn post(
                &self,
                url: &str,
                headers: &[(String, String)],
                body: &[u8],
                timeout: Duration,
            ) -> Result<Box<dyn std::io::Read + Send>, ai::HttpError> {
                self.0.post(url, headers, body, timeout)
            }
        }
        let backend = Backend::Model {
            config: ai::Config {
                api_key: "sk-test".into(),
                system: "instructions".into(),
                ..ai::Config::default()
            },
            transport: Box::new(Shared(transport.clone())),
        };
        let pick = |id| TASKS.iter().find(|t| t.id == id).expect("task");
        let dir = openstitch_testutil::TempDir::new("bench-memory");
        let out = run(
            &backend,
            &[pick("remember"), pick("remembered-heart")],
            &dir,
            &dir.join("none.bmp"),
            None,
            &mut |_| {},
        );
        assert!(out[0].passed, "{:?}", out[0]);
        assert!(out[1].passed, "{:?}", out[1]);
        let requests = transport.1.lock().expect("lock");
        assert!(!requests[0]["system"].to_string().contains("Madeira"));
        assert!(
            requests[2]["system"]
                .to_string()
                .contains("Owns only Madeira Polyneon threads"),
            "{}",
            requests[2]["system"]
        );
    }

    #[test]
    fn every_task_setup_runs() {
        for t in TASKS {
            let mut s = Session::new(std::env::temp_dir());
            for (command, params) in t.setup {
                let p: Value = serde_json::from_str(params).expect("json");
                s.execute(command, Some(&p))
                    .unwrap_or_else(|e| panic!("{}: {e}", t.id));
            }
        }
    }

    #[test]
    fn failures_are_sorted_into_categories() {
        let o = |calls: &[&str], errors: &[&str], reply: &str| Outcome {
            tool_calls: calls.iter().map(|s| s.to_string()).collect(),
            tool_errors: errors.iter().map(|s| s.to_string()).collect(),
            reply: reply.into(),
            ..Outcome::default()
        };
        let end = ai::StopKind::EndTurn;
        assert_eq!(
            classify(&o(&[], &[], r#"{"command": "shape.star"}"#), &end, ""),
            "wrote the call as text"
        );
        assert_eq!(
            classify(&o(&["a"], &["unknown command 'x'"], ""), &end, ""),
            "unknown command"
        );
        assert_eq!(
            classify(&o(&["a", "b", "c"], &["width: must be > 0"], ""), &end, ""),
            "wrong arguments"
        );
        assert_eq!(
            classify(&o(&["a"], &["failed"], ""), &end, ""),
            "gave up after errors"
        );
        assert_eq!(
            classify(&o(&["a"], &[], ""), &end, "red"),
            "wrong result: red"
        );
        assert!(
            classify(&o(&[], &[], ""), &ai::StopKind::Error("401".into()), "")
                .starts_with("request failed")
        );
        assert!(reddish("#c81414") && !reddish("#1f3a93"));
        // A monogram's letters are separate objects; the width is their union (real run, monogram).
        let letters = |x: f64| json!({"bounds": {"x": x, "width": 20.0, "height": 20.0}});
        let d = Design {
            objects: vec![letters(20.0), letters(41.0), letters(63.0)],
            info: Value::Null,
            ..Design::default()
        };
        assert!((widest(&d) - 63.0).abs() < 1e-9);
    }

    /// A design from `(x, y, w, h, region, colour, angle)` parts, as object.list and object.get
    /// give them.
    fn parts(list: &[(f64, f64, f64, f64, &str, &str, f64)]) -> Design {
        let (objects, full) = list
            .iter()
            .map(|&(x, y, w, h, region, color, angle)| {
                let mut o = json!({"bounds": {"x": x, "y": y, "width": w, "height": h}});
                let mut f = json!({});
                match region {
                    "line" => {
                        o["line"] = json!("running");
                        o["lineColor"] = json!(color);
                    }
                    "zigzag" => {
                        o["region"] = json!("fill");
                        o["regionColor"] = json!("#e0a458");
                        o["line"] = json!("zigzag");
                        o["lineColor"] = json!(color);
                        f["region"] = json!({"angle": angle});
                    }
                    _ => {
                        o["region"] = json!(region);
                        o["regionColor"] = json!(color);
                        f["region"] = json!({"angle": angle});
                    }
                }
                (o, f)
            })
            .unzip();
        Design {
            objects,
            full,
            check: json!({"ready": true, "findings": []}),
            ..Design::default()
        }
    }

    fn check(id: &str, d: &Design) -> Result<(), String> {
        (TASKS.iter().find(|t| t.id == id).expect("task").check)(d)
    }

    /// #493: each illustrated rule on hand-made passing and failing designs.
    #[test]
    fn the_illustrated_tasks_score_structure() {
        let seed = |x: f64| (x, 50.0, 2.0, 3.4, "satin", "#f6d55c", 0.0);
        let mut berry = vec![
            (25.0, 27.0, 50.0, 58.0, "fill", "#d7263d", 45.0),
            (31.0, 22.0, 38.0, 16.0, "fill", "#2e8b3a", 135.0),
        ];
        berry.extend([seed(40.0), seed(50.0), seed(60.0)]);
        assert_eq!(check("strawberry", &parts(&berry)), Ok(()));
        // Leaves sewn in the berry's direction: the panels do not read apart.
        let mut flat = berry.clone();
        flat[1].6 = 50.0;
        let e = check("strawberry", &parts(&flat)).expect_err("same angle");
        assert!(e.contains("20°"), "{e}");
        // Two seeds only.
        let e = check("strawberry", &parts(&berry[..4])).expect_err("seeds");
        assert!(e.contains("parts") || e.contains("seeds"), "{e}");
        // A design.check error fails it.
        let mut bad = parts(&berry);
        bad.check = json!({"ready": false, "findings": [{"severity": "error", "message": "untrimmed jump 14 mm"}]});
        let e = check("strawberry", &bad).expect_err("check");
        assert!(e.contains("untrimmed jump"), "{e}");
        // Too big for "about 60 mm".
        let mut big = berry.clone();
        big[0].3 = 95.0;
        assert!(check("strawberry", &parts(&big)).is_err());

        let cap = [
            (40.0, 50.0, 20.0, 30.0, "fill", "#efe3c8", 90.0),
            (16.0, 20.0, 24.0, 34.0, "fill", "#c0392b", 120.0),
            (36.0, 17.0, 28.0, 37.0, "fill", "#e74c3c", 0.0),
            (60.0, 20.0, 24.0, 34.0, "fill", "#c0392b", 60.0),
        ];
        assert_eq!(check("mushroom", &parts(&cap)), Ok(()));
        let mut two = cap;
        two[3].6 = 130.0;
        assert!(check("mushroom", &parts(&two)).is_err());

        let melon = [
            (16.0, 34.0, 68.0, 34.0, "fill", "#e8475f", 0.0),
            (13.0, 34.0, 74.0, 38.0, "zigzag", "#2e7d32", 90.0),
            (35.0, 45.0, 3.0, 5.0, "satin", "#1b1b1b", 0.0),
        ];
        assert_eq!(check("watermelon", &parts(&melon)), Ok(()));
        let mut plain = melon;
        plain[1].4 = "line";
        let e = check("watermelon", &parts(&plain)).expect_err("rind");
        assert!(e.contains("satin rind"), "{e}");

        let cat = [
            (20.0, 18.0, 60.0, 62.0, "fill", "#f7b267", 0.0),
            (37.0, 45.0, 6.0, 9.0, "satin", "#2d6a4f", 0.0),
            (57.0, 45.0, 6.0, 9.0, "satin", "#2d6a4f", 0.0),
            (47.0, 58.0, 6.0, 4.0, "satin", "#e56b6f", 0.0),
            (20.0, 60.0, 18.0, 2.0, "line", "#8a4b14", 0.0),
            (62.0, 60.0, 18.0, 2.0, "line", "#8a4b14", 0.0),
        ];
        assert_eq!(check("cat-face", &parts(&cat)), Ok(()));
        assert!(check("cat-face", &parts(&cat[..5])).is_err());
        assert!((angle_gap(170.0, 10.0) - 20.0).abs() < 1e-9);
        assert!(yellowish("#ffd23f") && !yellowish("#e8475f") && brownish("#c98b3c"));
    }

    #[test]
    fn the_auto_punch_and_small_name_tasks() {
        let mut d = parts(&[
            (20.0, 20.0, 60.0, 60.0, "fill", "#1f2a44", 30.0),
            (30.0, 30.0, 20.0, 20.0, "fill", "#f28c28", 90.0),
            (40.0, 40.0, 10.0, 10.0, "fill", "#ffd23f", 0.0),
        ]);
        d.calls = vec![
            r#"execute {"command":"digitize.autoPunch","params":{"dryRun":true}}"#.into(),
            r#"execute {"command":"digitize.autoPunch","params":{"overrides":[{"region":1,"angle":30}]}}"#.into(),
        ];
        assert_eq!(check("auto-punch", &d), Ok(()));
        d.objects[2]["regionColor"] = json!("#ffffff");
        assert!(
            check("auto-punch", &d)
                .expect_err("white")
                .contains("background")
        );
        d.calls.remove(0);
        assert!(
            check("auto-punch", &d)
                .expect_err("plan")
                .contains("dryRun")
        );

        let fonts = json!([{"id": "openstitch-line", "sizeRange": {"minMm": 3.0, "maxMm": 15.0}}, {"id": "openstitch-sans", "sizeRange": {"minMm": 8.0, "maxMm": 40.0}}]);
        let name = |font: &str, runs: u64| Design {
            objects: vec![json!({"text": "Mia", "font": font, "sizeMM": 5.0, "line": "running"})],
            full: vec![json!({"line": {"runTimes": runs}})],
            fonts: fonts.clone(),
            ..Design::default()
        };
        assert_eq!(check("small-name", &name("openstitch-line", 3)), Ok(()));
        assert!(
            check("small-name", &name("openstitch-sans", 3))
                .expect_err("range")
                .contains("sizeRange")
        );
        assert!(
            check("small-name", &name("openstitch-line", 1))
                .expect_err("bean")
                .contains("bean")
        );
        // Turned into shapes (a real run): scored on the text.add call and the lines left.
        let shapes = |font: &str| {
            Design {
            objects: vec![json!({"line": "running"}), json!({"line": "running"})],
            full: vec![json!({"line": {"runTimes": 3}}), json!({"line": {"runTimes": 3}})],
            fonts: fonts.clone(),
            calls: vec![
                format!(r#"mcp__openstitch-app__execute {{"command":"text.add","params":{{"text":"Mia","font":"{font}","size":5}}}}"#),
                r#"mcp__openstitch-app__run_batch {"steps":[{"command":"text.toShapes","params":{"id":1}}]}"#.into(),
            ],
            ..Design::default()
        }
        };
        assert_eq!(check("small-name", &shapes("openstitch-line")), Ok(()));
        // One text per letter, left to right (a real run).
        let letter = |t: &str, x: f64| json!({"text": t, "font": "openstitch-line", "sizeMM": 5.0, "line": "running", "bounds": {"x": x}});
        let letters = Design {
            objects: vec![letter("a", 52.0), letter("i", 50.0), letter("M", 45.0)],
            full: vec![json!({"line": {"runTimes": 3}}); 3],
            fonts: fonts.clone(),
            ..Design::default()
        };
        assert_eq!(check("small-name", &letters), Ok(()));
        assert!(check("small-name", &shapes("openstitch-sans")).is_err());
        assert!(
            check(
                "small-name",
                &Design {
                    calls: Vec::new(),
                    ..shapes("openstitch-line")
                }
            )
            .is_err()
        );
    }

    #[test]
    fn the_judge_scores_are_read_from_its_reply() {
        assert_eq!(
            judge_scores(r#"Here: {"recognisable": 4, "parts": 3, "colours": 5}"#),
            Some([4, 3, 5])
        );
        assert_eq!(
            judge_scores(r#"{"note": 1} then {"recognisable": 2, "parts": 2, "colours": 1}"#),
            Some([2, 2, 1])
        );
        assert_eq!(
            judge_scores(r#"{"recognisable": 9, "parts": 3, "colours": 5}"#),
            None
        );
        assert_eq!(judge_scores("no idea"), None);
    }

    /// --judge against a scripted model: the render goes to it, its answer is the score.
    #[test]
    fn the_judge_scores_the_render_of_each_task() {
        let backend = Backend::Model {
            config: ai::Config { api_key: "sk-test".into(), ..ai::Config::default() },
            transport: Box::new(Scripted(Mutex::new(
                [
                    message("", &[json!({"command": "shape.rectangle", "params": {"x": 20, "y": 50, "width": 60, "height": 20}})]),
                    message("Done.", &[]),
                ]
                .into_iter()
                .collect(),
            ))),
        };
        let transport = Arc::new(Recording(
            Mutex::new(
                [message(
                    r#"{"recognisable": 5, "parts": 4, "colours": 3}"#,
                    &[],
                )]
                .into_iter()
                .collect(),
            ),
            Mutex::new(Vec::new()),
        ));
        struct Shared(Arc<Recording>);
        impl ai::Transport for Shared {
            fn post(
                &self,
                url: &str,
                headers: &[(String, String)],
                body: &[u8],
                timeout: Duration,
            ) -> Result<Box<dyn std::io::Read + Send>, ai::HttpError> {
                self.0.post(url, headers, body, timeout)
            }
        }
        let judge = Backend::Model {
            config: ai::Config {
                api_key: "sk-test".into(),
                ..ai::Config::default()
            },
            transport: Box::new(Shared(transport.clone())),
        };
        let dir = openstitch_testutil::TempDir::new("bench-judge");
        let wider = TASKS.iter().find(|t| t.id == "wider").expect("task");
        let out = run(
            &backend,
            &[wider],
            &dir,
            &dir.join("none.bmp"),
            Some(&judge),
            &mut |_| {},
        );
        assert_eq!(out[0].judge, Some([5, 4, 3]), "{:?}", out[0]);
        let request = transport.1.lock().expect("lock")[0].to_string();
        assert!(
            request.contains("image/png") && request.contains("twice as wide"),
            "{request}"
        );
        let (json, md) = reports("scripted", &out);
        assert_eq!(json["results"][0]["judge"], json!([5, 4, 3]));
        assert!(md.contains("(judge 5/4/3)"), "{md}");
    }
}
