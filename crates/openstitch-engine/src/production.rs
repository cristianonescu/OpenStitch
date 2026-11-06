//! Production aids beyond PE-DESIGN, all off by default: the basting box (#509), stops after an
//! object and at jumps (#512), and the nearest-start rule plus the before/after report of
//! Optimize (#547). The compiler applies them (`openstitch_design::production`).

use crate::commands::{CommandSpec, Effect, IDS, MM, indexes, opt, req, spec, thread};
use openstitch_core::{Command, StitchPattern, Thread};
use openstitch_design::production::{Basting, BastingShape};
use openstitch_design::{Compiler, Design};
use serde_json::{Value, json};

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "design.setBasting",
            "Basting box (beyond PE-DESIGN): a line of long running stitches sewn first round the design that tacks floated fabric to the stabiliser; it is pulled out after sewing and stays inside the hoop's sewing field",
            Effect::Edit,
            vec![
                req("on", "bool", "Sew the basting run (false removes it)"),
                opt(
                    "shape",
                    "box|outline",
                    "box: the design's bounding box grown by the margin (shrunk to fit oval and round hoops); outline: the outline of all objects grown by the margin (default box)",
                ),
                opt("margin", "mm", "Distance from the design 0-30 (default 5)"),
                opt("stitch", "mm", "Running stitch length 2-12 (default 6)"),
                opt(
                    "thread",
                    "first|own",
                    "first: sewn in the first thread, no added colour change (default); own: its own colour block",
                ),
                opt(
                    "color",
                    "color",
                    "thread own: the basting thread (default a light grey)",
                ),
            ],
            |s, p| {
                if !p.bool("on", None)? {
                    s.design.production.basting = None;
                    return Ok(json!({ "basting": null }));
                }
                let old = s.design.production.basting.clone().unwrap_or_default();
                let shape = BastingShape::from_id(&p.string("shape", Some(old.shape.id()))?)
                    .ok_or_else(|| p.invalid("shape", "must be box or outline"))?;
                let own = match p
                    .string(
                        "thread",
                        Some(if old.thread.is_some() { "own" } else { "first" }),
                    )?
                    .as_str()
                {
                    "first" => false,
                    "own" => true,
                    _ => return Err(p.invalid("thread", "must be first or own")),
                };
                let colour = thread(p, "color")?;
                let b = Basting {
                    shape,
                    margin: p.opt("margin", old.margin, 0.0, 30.0)?,
                    stitch: p.opt("stitch", old.stitch, 2.0, 12.0)?,
                    thread: own
                        .then(|| colour.or(old.thread.clone()).unwrap_or_else(basting_thread)),
                };
                s.design.production.basting = Some(b);
                Ok(json!({ "basting": basting_json(&s.design) }))
            },
        ),
        spec(
            "design.setStopAtJumps",
            "Stop at Jumps (beyond PE-DESIGN), for machines without a thread cutter: the machine stops before every jump between parts so the thread can be snipped; off removes exactly those stops (stops set with object.setStopAfter stay)",
            Effect::Edit,
            vec![req("on", "bool", "Stop before every trimmed jump")],
            |s, p| {
                s.design.production.stop_at_jumps = p.bool("on", None)?;
                let pattern = Compiler::for_design(&s.design).compile(&s.design);
                Ok(json!({
                    "stopAtJumps": s.design.production.stop_at_jumps,
                    "stops": pattern.count(Command::Stop),
                }))
            },
        ),
        spec(
            "object.setStopAfter",
            "Stop after an object (beyond PE-DESIGN): the machine pauses after its last stitch, e.g. to place appliqué fabric, without a colour change. A colour change right after it is the stop. PES and most home formats write a stop as a colour change to the same thread",
            Effect::Edit,
            vec![IDS, req("on", "bool", "Stop after these objects")],
            |s, p| {
                let on = p.bool("on", None)?;
                for i in indexes(p, &s.design)? {
                    s.design.objects[i].stop_after = on;
                }
                Ok(json!({ "stopAfter": on }))
            },
        ),
    ]
}

/// The default basting thread: light grey, easy to see and pull out.
fn basting_thread() -> Thread {
    let mut t = Thread::rgb(0x00C8_C8C8);
    t.name = Some("Basting".into());
    t
}

/// The basting settings and what the run adds, for `design.info` and `design.setBasting`.
pub fn basting_json(d: &Design) -> Value {
    let Some(b) = &d.production.basting else {
        return Value::Null;
    };
    let runs = openstitch_design::production::basting_runs(d, b);
    let length: f64 = runs
        .iter()
        .flat_map(|r| r.windows(2).map(|w| w[0].distance(w[1])))
        .sum();
    json!({
        "shape": b.shape.id(),
        "marginMM": b.margin,
        "stitchMM": b.stitch,
        "thread": b.thread.as_ref().map_or("first".to_string(), Thread::hex),
        "stitches": runs.iter().map(|r| r.len().saturating_sub(1)).sum::<usize>(),
        "lengthMM": (length / MM * 10.0).round() / 10.0,
    })
}

/// The production settings as `design.info` reports them.
pub fn info(d: &Design, pattern: &StitchPattern) -> Value {
    json!({
        "basting": basting_json(d),
        "stopAtJumps": d.production.stop_at_jumps,
        "stopAfter": d.objects.iter().filter(|o| o.stop_after).map(|o| o.id).collect::<Vec<_>>(),
        "stops": pattern.count(Command::Stop),
        "autoStart": d.production.auto_start.id(),
    })
}

/// Total length of the frame moves (jumps), mm.
pub fn travel_mm(p: &StitchPattern) -> f64 {
    let mut last: Option<(f64, f64)> = None;
    let mut total = 0.0;
    for s in &p.stitches {
        if s.command == Command::Jump
            && let Some((x, y)) = last
        {
            total += ((s.x - x).powi(2) + (s.y - y).powi(2)).sqrt();
        }
        if matches!(s.command, Command::Stitch | Command::Jump) {
            last = Some((s.x, s.y));
        }
    }
    (total / MM * 10.0).round() / 10.0
}

/// What Optimize is judged on: colour changes, jumps, trims and travel.
pub fn order_numbers(d: &Design) -> Value {
    let p = Compiler::for_design(d).compile(d);
    json!({
        "colorChanges": p.count(Command::ColorChange),
        "jumps": p.count(Command::Jump),
        "trims": p.count(Command::Trim),
        "travelMM": travel_mm(&p),
    })
}

/// "colour changes 6 → 5, jumps 42 → 18, trims 40 → 16, travel 131 → 86 mm".
pub fn order_summary(before: &Value, after: &Value) -> String {
    let pair = |k: &str| (before[k].clone(), after[k].clone());
    let parts = [
        ("colour changes", "colorChanges", ""),
        ("jumps", "jumps", ""),
        ("trims", "trims", ""),
        ("travel", "travelMM", " mm"),
    ];
    parts
        .iter()
        .map(|(label, key, unit)| {
            let (a, b) = pair(key);
            format!("{label} {a} → {b}{unit}")
        })
        .collect::<Vec<_>>()
        .join(", ")
}
