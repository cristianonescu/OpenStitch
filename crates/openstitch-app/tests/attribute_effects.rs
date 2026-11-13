//! Every sewing setting must change the stitches (#880, part 1). For every region and line type
//! the inspector offers, each control it shows (the same `inspector::region_controls` /
//! `line_controls` table, so a new setting is tested as soon as the inspector shows it) is set
//! on three small fixtures (a rectangle, a concave U and a five-point star, so corner, travel and
//! path settings have something to act on) and the compiled stitches are compared; a setting
//! passes when it changes them on at least one:
//!
//! - a number at 30 % and at 70 % of its range must give different stitches, and where
//!   [`EXPECT`] defines a direction, more stitches (density) or fewer (stitch length);
//! - every choice option must give different stitches from every other (an `auto` option may
//!   sew as the option it resolves to);
//! - a toggle on must give different stitches from off.
//!
//! Controls a toggle or a choice reveals are tested in that state. Every parameter needs an
//! entry in [`EXPECT`]: a new setting without one fails. Settings that change nothing today are
//! in [`KNOWN`] with the reason; a number that changes only near one end of its range is printed
//! as a NOTE. `ATTRIBUTE_SWEEP=1` prints an 11-value sweep of every setting that sews the same.

use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::inspector::{self, Control, InspectorModel};
use openstitch_design::{Compiler, Design};
use openstitch_engine::Session;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

/// What changing a setting must do to the stitches.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Expect {
    /// They change.
    Changes,
    /// A higher value sews more stitches.
    More,
    /// A higher value sews fewer stitches.
    Fewer,
}

/// Every parameter the inspector's region (`r:`) and line (`l:`) controls set, and what
/// changing it must do. Directions only where they hold for every type that shows the setting.
const EXPECT: &[(&str, Expect)] = &[
    ("l:angle", Expect::Changes),
    ("l:arrange", Expect::Changes),
    ("l:bandMirror", Expect::Changes),
    ("l:bandTile", Expect::Changes),
    ("l:capBelow", Expect::Changes),
    ("l:capMaxLength", Expect::Changes),
    ("l:chainShape", Expect::Changes),
    ("l:cornerOverlap", Expect::Changes),
    ("l:cornerStyle", Expect::Changes),
    ("l:density", Expect::More),
    ("l:freeEnd", Expect::Changes),
    ("l:freeStart", Expect::Changes),
    ("l:groupSize", Expect::Changes),
    ("l:halfStitch", Expect::Changes),
    ("l:hand", Expect::Changes),
    ("l:handAngle", Expect::Changes),
    ("l:handExtra", Expect::Changes),
    ("l:handLength", Expect::Changes),
    ("l:handPasses", Expect::Changes),
    ("l:height", Expect::Changes),
    ("l:jaggedEdge", Expect::Changes),
    ("l:leaveUncut", Expect::Changes),
    ("l:maxPasses", Expect::Changes),
    ("l:minPasses", Expect::Changes),
    ("l:miterLimit", Expect::Changes),
    ("l:mitreBelow", Expect::Changes),
    ("l:motif", Expect::Changes),
    ("l:motifBand", Expect::Changes),
    ("l:motifs", Expect::Changes),
    ("l:noiseScale", Expect::Changes),
    ("l:passSpacing", Expect::Changes),
    ("l:pitch", Expect::Changes),
    ("l:profile", Expect::Changes),
    ("l:prongAngle", Expect::Changes),
    ("l:pullLeft", Expect::Changes),
    ("l:pullPercent", Expect::Changes),
    ("l:pullRight", Expect::Changes),
    ("l:raised", Expect::Changes),
    ("l:runTimes", Expect::Changes),
    ("l:satinStyle", Expect::Changes),
    ("l:seed", Expect::Changes),
    ("l:sharpCorners", Expect::Changes),
    ("l:shortStitchDistance", Expect::Changes),
    ("l:single", Expect::Changes),
    ("l:slant", Expect::Changes),
    ("l:spacing", Expect::Changes),
    ("l:strands", Expect::Changes),
    ("l:strokeTimes", Expect::Changes),
    ("l:taperEnd", Expect::Changes),
    ("l:taperStart", Expect::Changes),
    ("l:underSewing", Expect::Changes),
    ("l:underlayStyle", Expect::Changes),
    ("l:width", Expect::Changes),
    ("r:angle", Expect::Changes),
    ("r:autoDirection", Expect::Changes),
    ("r:baseSewing", Expect::Changes),
    ("r:columnOffset", Expect::Changes),
    ("r:contourJoin", Expect::Changes),
    ("r:contourStrategy", Expect::Changes),
    ("r:cornerStyle", Expect::Changes),
    ("r:counterclockwise", Expect::Changes),
    ("r:crossKind", Expect::Changes),
    ("r:density", Expect::More),
    ("r:echoLines", Expect::Changes),
    ("r:featherWidth", Expect::Changes),
    ("r:flowField", Expect::Changes),
    ("r:frequency", Expect::Changes),
    ("r:gradationMax", Expect::Changes),
    ("r:gradationMin", Expect::Changes),
    ("r:gradationPattern", Expect::Changes),
    ("r:gradationShape", Expect::Changes),
    ("r:gradationSteps", Expect::Changes),
    ("r:halfStitch", Expect::Changes),
    ("r:handAmount", Expect::Changes),
    ("r:handSide", Expect::Changes),
    ("r:jaggedEdge", Expect::Changes),
    ("r:mazeSeed", Expect::Changes),
    ("r:pattern", Expect::Changes),
    ("r:patternHeight", Expect::Changes),
    ("r:patternWidth", Expect::Changes),
    ("r:pullCompensation", Expect::Changes),
    ("r:pullLeft", Expect::Changes),
    ("r:pullPercent", Expect::Changes),
    ("r:pullRight", Expect::Changes),
    ("r:pushCompensation", Expect::Changes),
    ("r:raised", Expect::Changes),
    ("r:randomShift", Expect::Changes),
    ("r:rowOffset", Expect::Changes),
    ("r:runPitch", Expect::Changes),
    ("r:runningStitchPath", Expect::Changes),
    ("r:satinStyle", Expect::Changes),
    ("r:seed", Expect::Changes),
    ("r:shortStitchDistance", Expect::Changes),
    ("r:spacing", Expect::Changes),
    ("r:spacingAt", Expect::Changes),
    ("r:splitLength", Expect::Changes),
    ("r:stagger", Expect::Changes),
    ("r:staggerAmplitude", Expect::Changes),
    ("r:staggerPeriod", Expect::Changes),
    ("r:staggerRepeat", Expect::Changes),
    ("r:staggerSteps", Expect::Changes),
    ("r:startInside", Expect::Changes),
    ("r:stitchLength", Expect::Fewer),
    ("r:stitchType", Expect::Changes),
    ("r:straightness", Expect::Changes),
    ("r:texture", Expect::Changes),
    ("r:times", Expect::Changes),
    ("r:travel", Expect::Changes),
    ("r:underlay", Expect::Changes),
    ("r:underlayDensity", Expect::Changes),
    ("r:underlaySpacing", Expect::Changes),
    ("r:underlayStyle", Expect::Changes),
    ("r:waveHeight", Expect::Changes),
];

/// Settings that change nothing today, found by this test (#880): the start of each problem
/// line and why, with the issue that tracks it (#1039-#1049). Settings that change only near
/// one end of their range are printed as NOTEs and tracked in #1050. A problem not listed here fails, and so does an entry that no longer
/// reproduces (remove it once its issue is fixed).
const KNOWN: &[(&str, &str)] = &[
    (
        r#"r:fill gradationSteps"#,
        "#1039: Gradation steps change nothing with a gradation pattern on (2-20 steps)",
    ),
    (
        r#"r:satin gradationSteps"#,
        "#1039: Gradation steps change nothing with a gradation pattern on (2-20 steps)",
    ),
    (
        r#"r:satin spacingAt (with {"autoDirection":true})"#,
        "#1040: Auto-direction satin ignores the column spacing profile",
    ),
    (
        r#"r:satin pullLeft (with {"autoDirection":true})"#,
        "#1040: Auto-direction satin ignores pull left",
    ),
    (
        r#"r:satin pullRight (with {"autoDirection":true})"#,
        "#1040: Auto-direction satin ignores pull right",
    ),
    (
        r#"r:satin pullPercent (with {"autoDirection":true})"#,
        "#1040: Auto-direction satin ignores pull % of width",
    ),
    (
        r#"r:satin halfStitch (with {"autoDirection":true})"#,
        "#1040: Half Stitch changes nothing on these auto-direction satins",
    ),
    (
        r#"r:satin cornerStyle (with {"autoDirection":true}): turn and mitre"#,
        "#1040: Auto-direction satin corners: turn, mitre and cap sew the same",
    ),
    (
        r#"r:satin cornerStyle (with {"autoDirection":true}): turn and cap"#,
        "#1040: Auto-direction satin corners: turn, mitre and cap sew the same",
    ),
    (
        r#"r:satin cornerStyle (with {"autoDirection":true}): mitre and cap"#,
        "#1040: Auto-direction satin corners: turn, mitre and cap sew the same",
    ),
    (
        r#"r:satin underlayStyle: edgeZigzag and edgeFill"#,
        "#1041: Satin region: edge-fill under sewing sews as edge zigzag",
    ),
    (
        r#"r:programmableFill frequency"#,
        "#1042: Programmable fill frequency changes nothing (0-50 %)",
    ),
    (
        r#"r:zigzagNetFill angle"#,
        "#1043: Zigzag net fill direction changes nothing (-360 to 360)",
    ),
    (
        r#"r:fill stitchType"#,
        "#1044: Fill stitch type 1 (flat rows) and 2 (sloped rows) sew the same on these shapes",
    ),
    (
        r#"r:crossStitch travel"#,
        "#1045: Cross stitch travel on and off sew the same on these shapes",
    ),
    (
        r#"r:fill runningStitchPath: inside and insideHidden"#,
        "Not a bug: insideHidden differs from inside only on an open fill (low density, #628)",
    ),
    (
        r#"r:satin runningStitchPath: inside and insideHidden"#,
        "Not a bug: insideHidden differs from inside only on an open fill (low density, #628)",
    ),
    (
        r#"l:running profile: linear and smoothstep"#,
        "#1046: Thickness profiles other than arc sew the same with the default passes",
    ),
    (
        r#"l:running profile: linear and stairs"#,
        "#1046: Thickness profiles other than arc sew the same with the default passes",
    ),
    (
        r#"l:running profile: smoothstep and stairs"#,
        "#1046: Thickness profiles other than arc sew the same with the default passes",
    ),
    (
        r#"l:zigzag halfStitch"#,
        "#1047: Half stitch on a zigzag line changes nothing on these shapes",
    ),
    (
        r#"l:satin cornerStyle: mitre and cap"#,
        "#1048: Satin line corners: mitre and cap sew the same",
    ),
    (
        r#"l:satin capBelow (with {"cornerStyle":"auto"})"#,
        "#1048: Satin line: cap below changes nothing with auto corners",
    ),
    (
        r#"l:satin capMaxLength (with {"cornerStyle":"auto"})"#,
        "#1048: Satin line: cap max length changes nothing with auto corners",
    ),
    (
        r#"l:stripes spacing"#,
        "#1049: Stripes line: spacing changes nothing (0.5-20 mm)",
    ),
    (
        r#"l:ripple spacing"#,
        "#1049: Ripple line: spacing changes nothing (0.5-20 mm)",
    ),
];

fn expect(key: &str) -> Option<Expect> {
    EXPECT.iter().find(|(k, _)| *k == key).map(|(_, e)| *e)
}

#[derive(Clone, Copy, PartialEq)]
enum Side {
    Region,
    Line,
}

impl Side {
    fn prefix(self) -> &'static str {
        match self {
            Side::Region => "r:",
            Side::Line => "l:",
        }
    }
}

/// The compiled stitches, as comparable values (0.01 mm).
fn stitches(d: &Design) -> Vec<(i64, i64, u8)> {
    Compiler::default()
        .compile(d)
        .stitches
        .iter()
        .map(|s| {
            (
                (s.x * 10.0).round() as i64,
                (s.y * 10.0).round() as i64,
                s.command as u8,
            )
        })
        .collect()
}

struct Fixture {
    session: Session,
    id: u64,
    side: Side,
}

/// The fixtures' outlines, mm: a 20 × 14 mm rectangle, a concave U (two arms, so a fill splits
/// and travels) and a five-point star (acute and reflex corners).
fn outline(shape: usize) -> Vec<[f64; 2]> {
    match shape {
        0 => vec![[10.0, 10.0], [30.0, 10.0], [30.0, 24.0], [10.0, 24.0]],
        1 => vec![
            [10.0, 10.0],
            [17.0, 10.0],
            [17.0, 26.0],
            [27.0, 26.0],
            [27.0, 10.0],
            [34.0, 10.0],
            [34.0, 33.0],
            [10.0, 33.0],
        ],
        _ => (0..10)
            .map(|k| {
                let a = std::f64::consts::PI * (k as f64 / 5.0 - 0.5);
                let r = if k % 2 == 0 { 13.0 } else { 5.5 };
                [25.0 + r * a.cos(), 25.0 + r * a.sin()]
            })
            .collect(),
    }
}

const SHAPES: usize = 3;

impl Fixture {
    /// Fixture `shape` sewn only as `side`, of type `kind` (an inspector type id).
    fn new(side: Side, kind: &str, shape: usize) -> Option<Self> {
        let mut session = Session::new(std::env::temp_dir());
        let params = match side {
            Side::Region => json!({"points": outline(shape), "fill": "Red"}),
            Side::Line => json!({"points": outline(shape), "outline": "Black"}),
        };
        let id = session.execute("shape.polygon", Some(&params)).ok()?["id"].as_u64()?;
        let mut f = Fixture { session, id, side };
        f.set(&json!({ "type": kind })).ok()?;
        Some(f)
    }

    fn set(&mut self, params: &Value) -> Result<(), String> {
        let sel = BTreeSet::from([self.id]);
        let m = InspectorModel::new(self.session.design(), &sel);
        let r: CommandRequest = match self.side {
            Side::Region => m.set_region(params.clone()),
            Side::Line => m.set_line(params.clone()),
        };
        self.session
            .execute(r.command, Some(&r.params))
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    fn controls(&self) -> Vec<Control> {
        let o = &self.session.design().objects[0];
        match self.side {
            Side::Region => o.region.as_ref().map(inspector::region_controls),
            Side::Line => o.line.as_ref().map(inspector::line_controls),
        }
        .unwrap_or_default()
    }
}

/// A fresh fixture of `kind` with `state` (a control's parameters) applied first.
fn fixture(side: Side, kind: &str, shape: usize, state: &Value) -> Fixture {
    let mut f = Fixture::new(side, kind, shape).unwrap_or_else(|| panic!("{kind}: no fixture"));
    if !state.as_object().is_some_and(|o| o.is_empty()) {
        f.set(state)
            .unwrap_or_else(|e| panic!("{kind} {state}: {e}"));
    }
    f
}

type Stitches = Vec<(i64, i64, u8)>;

/// The stitches on fixture `shape` with `params` set on top of `state`, or why it was refused.
fn with(
    side: Side,
    kind: &str,
    shape: usize,
    state: &Value,
    params: &Value,
) -> Result<Stitches, String> {
    let mut f = fixture(side, kind, shape, state);
    f.set(params)?;
    Ok(stitches(f.session.design()))
}

/// The stitches on every fixture.
fn on_all(side: Side, kind: &str, state: &Value, params: &Value) -> Result<Vec<Stitches>, String> {
    (0..SHAPES)
        .map(|k| with(side, kind, k, state, params))
        .collect()
}

fn number_values(range: (f64, f64), integer: bool) -> (f64, f64) {
    let at = |t: f64| {
        let v = range.0 + (range.1 - range.0) * t;
        if integer {
            v.round()
        } else {
            (v * 100.0).round() / 100.0
        }
    };
    let (a, mut b) = (at(0.3), at(0.7));
    if (a - b).abs() < 1e-9 {
        b = range.1;
    }
    (a, b)
}

/// Checks one control in `state`; returns the problems found.
fn check(side: Side, kind: &str, state: &Value, c: &Control, want: Expect) -> Vec<String> {
    let plain = state.as_object().is_some_and(|o| o.is_empty());
    let what = format!(
        "{}{kind} {}{}",
        side.prefix(),
        c.param(),
        if plain {
            String::new()
        } else {
            format!(" (with {state})")
        }
    );
    let mut problems = Vec::new();
    match c {
        Control::Number {
            range,
            integer,
            value,
            ..
        } => {
            let p = |v: f64| c.params(json!(v));
            // 30 % and 70 % of the range; when one is refused (a minimum above its maximum),
            // the current value and either end.
            let (a, b) = number_values(*range, *integer);
            let pairs = [(a, b), (range.0, *value), (*value, range.1)];
            let mut tried = Vec::new();
            let found =
                pairs
                    .iter()
                    .filter(|(a, b)| (a - b).abs() > 1e-9)
                    .find_map(|&(a, b)| {
                        match (
                            on_all(side, kind, state, &p(a)),
                            on_all(side, kind, state, &p(b)),
                        ) {
                            (Ok(sa), Ok(sb)) => Some((a, b, sa, sb)),
                            (ra, rb) => {
                                tried.push(format!("{a}/{b}: {:?} {:?}", ra.err(), rb.err()));
                                None
                            }
                        }
                    });
            let found = found.or_else(|| {
                // Some values refused (odd passes only, a minimum at most its maximum): the
                // first and last values of an 11-step sweep that are accepted.
                let ok: Vec<(f64, Vec<Stitches>)> = (0..=10)
                    .filter_map(|k| {
                        let v = range.0 + (range.1 - range.0) * f64::from(k) / 10.0;
                        let v = if *integer { v.round() } else { v };
                        on_all(side, kind, state, &p(v)).ok().map(|st| (v, st))
                    })
                    .collect();
                let (first, last) = (ok.first()?, ok.last()?);
                (first.0 != last.0).then(|| (first.0, last.0, first.1.clone(), last.1.clone()))
            });
            let Some((a, b, sa, sb)) = found else {
                problems.push(format!("{what}: refused: {}", tried.join("; ")));
                return problems;
            };
            let (na, nb) = (sa[0].len(), sb[0].len());
            if sa == sb && std::env::var_os("ATTRIBUTE_SWEEP").is_some() {
                // Diagnosis: 11 values across the range, numbered by the stitches they sew.
                let mut seen: Vec<Vec<Stitches>> = Vec::new();
                let row: Vec<String> = (0..=10)
                    .map(|k| {
                        let v = range.0 + (range.1 - range.0) * f64::from(k) / 10.0;
                        let v = if *integer {
                            v.round()
                        } else {
                            (v * 100.0).round() / 100.0
                        };
                        match on_all(side, kind, state, &p(v)) {
                            Ok(st) => {
                                let n = st[0].len();
                                let id = seen.iter().position(|x| *x == st).unwrap_or_else(|| {
                                    seen.push(st);
                                    seen.len() - 1
                                });
                                format!("{v}:#{id}({n})")
                            }
                            Err(_) => format!("{v}:refused"),
                        }
                    })
                    .collect();
                eprintln!("SWEEP {what}: {}", row.join(" "));
            }
            // The same at 30 % and 70 %: the two ends of the range before calling it dead.
            let ends = sa == sb
                && matches!(
                    (on_all(side, kind, state, &p(range.0)), on_all(side, kind, state, &p(range.1))),
                    (Ok(lo), Ok(hi)) if lo != hi
                );
            if sa == sb && ends {
                // Changes only near an end of its range: recorded, not a failure.
                eprintln!(
                    "NOTE {what}: the same at {a} and {b}, different at {} and {}",
                    range.0, range.1
                );
            } else if sa == sb {
                problems.push(format!(
                    "{what}: {a} and {b} sew the same stitches on every fixture ({na} on the rectangle)"
                ));
            } else if sa[0] != sb[0] && want == Expect::More && nb <= na {
                problems.push(format!(
                    "{what}: {a} -> {b} sews {na} -> {nb} stitches, expected more"
                ));
            } else if sa[0] != sb[0] && want == Expect::Fewer && nb >= na {
                problems.push(format!(
                    "{what}: {a} -> {b} sews {na} -> {nb} stitches, expected fewer"
                ));
            }
        }
        Control::Choice { options, .. } => {
            let mut sewn: Vec<(&str, Vec<Stitches>)> = Vec::new();
            for (option, _) in options {
                match on_all(side, kind, state, &c.params(json!(option))) {
                    Ok(st) => sewn.push((option.as_str(), st)),
                    Err(e) => problems.push(format!("{what} {option}: refused: {e}")),
                }
            }
            for (i, (a, sa)) in sewn.iter().enumerate() {
                for (b, sb) in &sewn[i + 1..] {
                    if sa == sb && *a != "auto" && *b != "auto" {
                        problems.push(format!(
                            "{what}: {a} and {b} sew the same stitches on every fixture"
                        ));
                    }
                }
            }
        }
        Control::Toggle { .. } => {
            let off = on_all(side, kind, state, &c.params(json!(false)));
            let on = on_all(side, kind, state, &c.params(json!(true)));
            match (off, on) {
                (Ok(sa), Ok(sb)) if sa == sb => problems.push(format!(
                    "{what}: off and on sew the same stitches on every fixture"
                )),
                (Ok(_), Ok(_)) => {}
                (ra, rb) => {
                    problems.push(format!("{what}: refused: {:?} {:?}", ra.err(), rb.err()))
                }
            }
        }
    }
    problems
}

/// Every (state, control) of one type: the controls shown at first, then those a toggle turned
/// on or a choice option reveals, each tested once, in the first state that shows it.
fn controls_of(side: Side, kind: &str) -> Vec<(Value, Control)> {
    let base = Fixture::new(side, kind, 0).unwrap_or_else(|| panic!("{kind}: no fixture"));
    let first = base.controls();
    let mut seen: BTreeSet<&'static str> = first.iter().map(|c| c.param()).collect();
    let mut out: Vec<(Value, Control)> = first.iter().map(|c| (json!({}), c.clone())).collect();
    let mut states = Vec::new();
    for c in &first {
        match c {
            Control::Toggle { value, .. } => states.push(c.params(json!(!value))),
            Control::Choice { value, options, .. } => {
                states.extend(
                    options
                        .iter()
                        .filter(|(o, _)| o != value)
                        .map(|(o, _)| c.params(json!(o))),
                );
            }
            Control::Number { .. } => {}
        }
    }
    for state in states {
        let mut f = Fixture::new(side, kind, 0).expect("fixture");
        if f.set(&state).is_err() {
            continue;
        }
        for c in f.controls() {
            if seen.insert(c.param()) {
                out.push((state.clone(), c));
            }
        }
    }
    out
}

#[test]
fn every_sewing_setting_changes_the_stitches() {
    let mut problems = Vec::new();
    let mut missing = BTreeMap::<String, String>::new();
    let mut checked = 0usize;
    let mut params = BTreeSet::new();
    let types: Vec<(Side, &'static str)> = inspector::region_types()
        .into_iter()
        .filter_map(|(k, _)| k.map(|k| (Side::Region, inspector::region_name(Some(k)))))
        .chain(
            inspector::line_types()
                .into_iter()
                .filter_map(|(k, _)| k.map(|k| (Side::Line, inspector::line_name(Some(k))))),
        )
        .collect();
    for (side, kind) in types {
        for (state, c) in controls_of(side, kind) {
            let key = format!("{}{}", side.prefix(), c.param());
            params.insert(key.clone());
            let want = match expect(&key) {
                Some(e) => e,
                None => {
                    missing
                        .entry(key)
                        .or_insert_with(|| format!("{kind}: {}", c.label()));
                    Expect::Changes
                }
            };
            checked += 1;
            problems.extend(check(side, kind, &state, &c, want));
        }
    }
    eprintln!("{checked} control checks over {} parameters", params.len());
    for (k, v) in &missing {
        eprintln!("NO ENTRY {k} ({v})");
    }
    let known = |p: &str| KNOWN.iter().find(|(k, _)| p.starts_with(k));
    let new: Vec<&String> = problems.iter().filter(|p| known(p).is_none()).collect();
    for p in &new {
        eprintln!("PROBLEM {p}");
    }
    let stale: Vec<&str> = KNOWN
        .iter()
        .filter(|(k, _)| !problems.iter().any(|p| p.starts_with(k)))
        .map(|(k, _)| *k)
        .collect();
    for k in &stale {
        eprintln!("FIXED (remove from KNOWN) {k}");
    }
    eprintln!(
        "{} parameters, {checked} checks, {} known settings without an effect",
        params.len(),
        KNOWN.len()
    );
    assert!(
        missing.is_empty(),
        "{} parameters without an EXPECT entry",
        missing.len()
    );
    assert!(new.is_empty(), "{} settings without an effect", new.len());
    assert!(
        stale.is_empty(),
        "{} KNOWN entries no longer reproduce",
        stale.len()
    );
}
