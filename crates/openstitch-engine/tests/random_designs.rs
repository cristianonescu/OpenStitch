//! Seeded random designs (#506): random shapes, stitch types, settings and node moves, each
//! compiled and held to rules that must always hold:
//!
//! 1. no panic (a command may refuse its parameters, nothing more);
//! 2. no stitch longer than 12.1 mm, jumps excepted; a satin crossing may be longer (it stays
//!    whole, as PE-DESIGN 11 writes it; wide satins are #460) only when Check Design reports
//!    it as an error;
//! 3. every needle drop within its object's bounds plus max(2 mm, pull compensation + 1 mm);
//! 4. the same stitches compiled twice and after an `.osd` save and open;
//! 5. a PES export that reads back as written (#457).
//!
//! CI runs 200 designs. A long run, or one seed again:
//!
//! ```text
//! OPENSTITCH_FUZZ_DESIGNS=100000 cargo test --release -p openstitch-engine --test integration random_designs
//! OPENSTITCH_FUZZ_SEED=1234 OPENSTITCH_FUZZ_DESIGNS=1 cargo test -p openstitch-engine --test integration random_designs
//! ```
//!
//! A failure prints its seed and writes the design to `target/fuzz-designs/seed-N.json`.

use openstitch_core::Command;
use openstitch_engine::Session;
use openstitch_engine::quality::{CheckOptions, MAX_STITCH_MM, Severity, Traced, check};
use openstitch_testutil::TempDir;
use serde_json::{Value, json};

/// Seeds that fail because of an open bug: (seed, issue). Only seeds an open issue names:
/// #373 zigzag corners reach past the outline (no miter limit). The other seeds up to 1,000
/// pass (#570 and #571's seeds 150, 154, 339 and 398 since start points snap to a reshaped
/// outline), except 993, which aborts the process (#638) and so cannot be listed here.
const KNOWN: &[(u64, u32)] = &[
    (13, 373),
    (42, 373),
    (57, 373),
    (74, 373),
    (110, 373),
    (258, 373),
    (276, 373),
    (316, 373),
    (365, 373),
    (389, 373),
    (419, 373),
    (423, 373),
    (434, 373),
    (473, 373),
    (479, 373),
    (515, 373),
    (518, 373),
    (544, 373),
    (548, 373),
    (578, 373),
    (593, 373),
    (643, 373),
    (682, 373),
    (703, 373),
    (709, 373),
    (735, 373),
    (737, 373),
    (750, 373),
    (762, 373),
    (869, 373),
    (978, 373),
    (989, 373),
    (996, 373),
];

/// xorshift64*: a plain, fixed generator (no dependency).
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }
    fn range(&mut self, lo: f64, hi: f64) -> f64 {
        lo + (self.next() >> 11) as f64 / (1u64 << 53) as f64 * (hi - lo)
    }
    fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
        &items[(self.next() % items.len() as u64) as usize]
    }
    fn chance(&mut self, p: f64) -> bool {
        self.range(0.0, 1.0) < p
    }
}

const REGIONS: &[&str] = &[
    "fill",
    "satin",
    "programmableFill",
    "motifFill",
    "piping",
    "crossStitch",
    "concentricCircle",
    "radial",
    "spiral",
    "flexibleSpiral",
    "stippling",
    "netFill",
    "zigzagNetFill",
    "contour",
];
const LINES: &[&str] = &[
    "running",
    "triple",
    "zigzag",
    "motif",
    "stem",
    "candlewick",
    "eStitch",
    "vStitch",
    "chain",
];

/// The commands that build one random design (some may be refused).
fn script(seed: u64) -> Vec<(String, Value)> {
    let mut r = Rng::new(seed);
    let mut out: Vec<(String, Value)> = Vec::new();
    let n = 1 + r.next() % 4;
    for id in 1..=n {
        let (x, y) = (r.range(5.0, 60.0), r.range(5.0, 60.0));
        let shape = match r.next() % 6 {
            0 => {
                json!(["shape.rectangle", {"x": x, "y": y, "width": r.range(0.2, 35.0), "height": r.range(0.2, 35.0)}])
            }
            1 => {
                json!(["shape.ellipse", {"cx": x + 15.0, "cy": y + 15.0, "rx": r.range(0.5, 18.0), "ry": r.range(0.5, 18.0)}])
            }
            2 => {
                // A star, or any polygon (it may touch or cross itself).
                let k = 3 + r.next() % 7;
                let pts: Vec<[f64; 2]> = (0..k * 2)
                    .map(|i| {
                        let a = i as f64 * std::f64::consts::PI / k as f64;
                        let rad = if i % 2 == 0 {
                            r.range(5.0, 18.0)
                        } else {
                            r.range(1.0, 18.0)
                        };
                        [x + 18.0 + rad * a.cos(), y + 18.0 + rad * a.sin()]
                    })
                    .collect();
                json!(["shape.polygon", {"points": pts, "closed": true}])
            }
            3 => {
                let k = 2 + r.next() % 5;
                let pts: Vec<[f64; 2]> = (0..k)
                    .map(|_| [r.range(5.0, 80.0), r.range(5.0, 80.0)])
                    .collect();
                json!(["shape.path", {"points": pts, "closed": false}])
            }
            4 => {
                json!(["shape.rectangle", {"x": x, "y": y, "width": r.range(0.1, 1.0), "height": r.range(10.0, 30.0)}])
            }
            _ => {
                json!(["text.add", {"text": *r.pick(&["A", "Hi", "ok!", "Wg"]), "size": r.range(2.0, 25.0), "x": x, "y": y}])
            }
        };
        out.push((shape[0].as_str().expect("value").into(), shape[1].clone()));
        if r.chance(0.8) {
            let mut p = json!({"ids": [id], "type": *r.pick(REGIONS)});
            for (key, lo, hi) in [
                ("density", 1.0, 7.0),
                ("angle", 0.0, 180.0),
                ("pullCompensation", 0.0, 1.0),
                ("stitchLength", 2.0, 8.0),
            ] {
                if r.chance(0.5) {
                    p[key] = json!(r.range(lo, hi));
                }
            }
            if r.chance(0.3) {
                p["underlay"] = json!(r.chance(0.5));
            }
            if r.chance(0.3) {
                p["splitLength"] = json!(*r.pick(&[0.0, 4.0, 7.0, 12.0]));
            }
            out.push(("object.setRegion".into(), p));
        }
        if r.chance(0.5) {
            out.push(("object.setLine".into(), json!({"ids": [id], "type": *r.pick(LINES), "width": r.range(0.5, 6.0), "pitch": r.range(1.0, 5.0)})));
        }
        for _ in 0..r.next() % 3 {
            out.push(("object.setPoint".into(), json!({"id": id, "index": r.next() % 6, "x": r.range(0.0, 90.0), "y": r.range(0.0, 90.0)})));
        }
    }
    out
}

/// The longest stitch over 12.1 mm (needle drop to needle drop) of each object that has one.
fn long_stitches(_: &openstitch_design::Design, t: &Traced) -> Vec<(u64, f64)> {
    let st = &t.pattern.stitches;
    t.spans
        .iter()
        .filter_map(|(id, range)| {
            let longest = range
                .clone()
                .filter(|&i| {
                    i > 0
                        && st[i].command == Command::Stitch
                        && st[i - 1].command == Command::Stitch
                })
                .map(|i| (st[i].x - st[i - 1].x).hypot(st[i].y - st[i - 1].y) / 10.0)
                .fold(0.0, f64::max);
            (longest > MAX_STITCH_MM).then_some((*id, longest))
        })
        .collect()
}

/// The rules one design breaks, empty when it keeps them all.
fn violations(seed: u64, dir: &std::path::Path) -> (Vec<String>, Value) {
    let mut s = Session::new(dir);
    for (id, params) in script(seed) {
        // A refused command (a validation error) is allowed; a panic is not.
        let _ = s.execute(&id, Some(&params));
    }
    let design = serde_json::to_value(s.design()).expect("value");
    let mut bad = Vec::new();
    let t = Traced::new(s.design());
    let st = &t.pattern.stitches;
    let flagged: Vec<u64> = check(s.design(), &t, &CheckOptions { fabric: (1.5, 2.5) })
        .into_iter()
        .filter(|f| f.rule == "satinTooWide" && f.severity == Severity::Error)
        .flat_map(|f| f.ids)
        .collect();
    for (id, len) in long_stitches(s.design(), &t) {
        if !flagged.contains(&id) {
            bad.push(format!("object {id}: a {len:.1} mm stitch"));
        }
    }
    // Allowed reach past the outline: pull compensation, a line's width (chain, E, V and
    // candlewick stitches reach it on one side) or a motif's height, a cross stitch's cell;
    // 1 mm beyond each, never less than 2 mm.
    let reach = |o: &openstitch_design::DesignObject| {
        let region = o.region.as_ref().map_or(0.0, |g| {
            let cell = if g.kind == openstitch_design::RegionType::CrossStitch {
                g.stitch_length
            } else {
                0.0
            };
            g.pull_compensation.max(cell)
        });
        let line = o
            .line
            .as_ref()
            .map_or(0.0, |l| l.width.max(l.height.unwrap_or(0.0)));
        (region.max(line) + 1.0).max(2.0) * 10.0
    };
    let inside = |o: &openstitch_design::DesignObject, p: &openstitch_core::Stitch| {
        let (b, m) = (o.bounds().expect("value"), reach(o));
        p.x >= b.min_x - m && p.x <= b.max_x + m && p.y >= b.min_y - m && p.y <= b.max_y + m
    };
    let object = |id: u64| {
        s.design()
            .objects
            .iter()
            .find(|o| o.id == id)
            .expect("value")
    };
    for (k, (id, range)) in t.spans.iter().enumerate() {
        // The previous sewn object's tie-off is written when the frame leaves it, so it falls in
        // this object's range.
        let previous = t.spans[..k]
            .iter()
            .rev()
            .find(|(_, r)| !r.is_empty())
            .map(|(i, _)| object(*i));
        if let Some(p) = range
            .clone()
            .map(|i| &st[i])
            .filter(|p| p.command == Command::Stitch)
            .find(|p| !inside(object(*id), p) && !previous.is_some_and(|o| inside(o, p)))
        {
            let o = object(*id);
            bad.push(format!(
                "object {id} ({:?} {:?}, line {:?}): needle drop ({:.1}, {:.1}) mm outside its bounds",
                o.kind,
                o.region.as_ref().map(|g| g.kind),
                o.line.as_ref().map(|l| l.kind),
                p.x / 10.0,
                p.y / 10.0
            ));
        }
    }
    if Traced::new(s.design()).pattern != t.pattern {
        bad.push("two compiles differ".into());
    }
    let bytes = openstitch_engine::project::encode(s.design()).expect("value");
    let reopened = openstitch_engine::project::decode(&bytes).expect("value");
    if Traced::new(&reopened).pattern.stitches != t.pattern.stitches {
        bad.push("the .osd round trip changes the stitches".into());
    }
    if t.pattern.count(Command::Stitch) > 0 {
        match s.execute(
            "design.export",
            Some(&json!({"path": "r.pes", "split": true})),
        ) {
            Ok(r) if r["verified"] == json!(true) => {}
            Ok(r) => bad.push(format!("PES read-back: {}", r["readBack"])),
            Err(e) => bad.push(format!("PES export failed: {e}")),
        }
    }
    (bad, design)
}

#[test]
fn random_designs_keep_the_invariants() {
    let count: u64 = std::env::var("OPENSTITCH_FUZZ_DESIGNS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(200);
    let first: u64 = std::env::var("OPENSTITCH_FUZZ_SEED")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let dir = TempDir::new("random-designs");
    let mut failures = Vec::new();
    for seed in first..first + count {
        if KNOWN.iter().any(|(s, _)| *s == seed) {
            continue;
        }
        let outcome = std::panic::catch_unwind(|| violations(seed, &dir));
        let (bad, design) = match outcome {
            Ok(v) => v,
            Err(_) => (vec!["panicked".to_string()], json!(script(seed))),
        };
        if !bad.is_empty() {
            let out =
                std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/fuzz-designs");
            let _ = std::fs::create_dir_all(&out);
            let file = out.join(format!("seed-{seed}.json"));
            let _ = std::fs::write(
                &file,
                serde_json::to_string_pretty(
                    &json!({"seed": seed, "script": script(seed), "design": design}),
                )
                .expect("value"),
            );
            eprintln!(
                "seed {seed}: {}\n  design: {}\n  rerun: OPENSTITCH_FUZZ_SEED={seed} OPENSTITCH_FUZZ_DESIGNS=1 cargo test -p openstitch-engine --test integration random_designs",
                bad.join("; "),
                file.display()
            );
            failures.push(seed);
        }
    }
    assert!(failures.is_empty(), "seeds failing: {failures:?}");
}

/// A deliberately broken design (a 30 mm satin, split off) breaks the stitch-length rule, and
/// Check Design reports it, which is what lets rule 2 pass it.
#[test]
fn a_wide_satin_breaks_the_stitch_length_rule_and_is_reported() {
    let mut s = Session::new(std::env::temp_dir());
    s.execute(
        "shape.rectangle",
        Some(&json!({"x": 10, "y": 10, "width": 30, "height": 30})),
    )
    .expect("value");
    s.execute(
        "object.setRegion",
        Some(&json!({"ids": [1], "type": "satin", "splitLength": 0})),
    )
    .expect("value");
    let t = Traced::new(s.design());
    let long = long_stitches(s.design(), &t);
    assert!(
        long.iter().any(|&(id, len)| id == 1 && len > 30.0),
        "{long:?}"
    );
    let found = check(s.design(), &t, &CheckOptions { fabric: (1.5, 2.5) });
    assert!(
        found
            .iter()
            .any(|f| f.rule == "satinTooWide" && f.severity == Severity::Error && f.ids == [1])
    );
    // Split at 7 mm, it keeps the rule.
    s.execute(
        "object.setRegion",
        Some(&json!({"ids": [1], "splitLength": 7})),
    )
    .expect("value");
    assert_eq!(long_stitches(s.design(), &Traced::new(s.design())), []);
}

#[test]
fn the_generator_is_deterministic() {
    assert_eq!(script(42), script(42));
    assert_ne!(script(42), script(43));
}
