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
        spec(
            "object.setHideUnder",
            "Leave out stitching hidden under later objects (beyond PE-DESIGN): the region skips what later fills and satins cover, keeping an overlap under their edges so no fabric shows; the shape is not cut, so moving the top object brings the stitches back. Satin regions are not clipped",
            Effect::Edit,
            vec![
                IDS,
                req("on", "bool", "Leave out the hidden stitching"),
                opt(
                    "overlap",
                    "mm",
                    "How far the region still reaches under the covering edges 0-5 (default 1)",
                ),
            ],
            |s, p| {
                let on = p.bool("on", None)?;
                let overlap = p.opt("overlap", 1.0, 0.0, 5.0)?;
                for i in indexes(p, &s.design)? {
                    s.design.objects[i].hide_under = on.then_some(overlap);
                }
                Ok(json!({ "hideUnder": on, "stitchesSaved": hidden_saved(&s.design) }))
            },
        ),
        spec(
            "design.setRemoveHidden",
            "Leave out the stitching later fills and satins hide in every shape's region (beyond PE-DESIGN; text only with object.setHideUnder), keeping an overlap under their edges; shapes are not cut",
            Effect::Edit,
            vec![
                req("on", "bool", "Leave out hidden stitching design-wide"),
                opt(
                    "overlap",
                    "mm",
                    "How far regions still reach under covering edges 0-5 (default 1)",
                ),
            ],
            |s, p| {
                let on = p.bool("on", None)?;
                let overlap = p.opt("overlap", 1.0, 0.0, 5.0)?;
                s.design.production.remove_hidden = on.then_some(overlap);
                Ok(json!({ "removeHidden": on, "stitchesSaved": hidden_saved(&s.design) }))
            },
        ),
        spec(
            "design.setTravelUnder",
            "Travel under later fills (beyond PE-DESIGN): a move into an object of the same thread that lies inside fills sewn later (0.5 mm in from their edges, straight or round one bend) is sewn as 2 mm running stitches hidden under them instead of a jump and trim",
            Effect::Edit,
            vec![req("on", "bool", "Sew hidden travel instead of trims")],
            |s, p| {
                let before = order_numbers(&s.design);
                s.design.production.travel_under = p.bool("on", None)?;
                let after = order_numbers(&s.design);
                Ok(json!({
                    "travelUnder": s.design.production.travel_under,
                    "trims": { "before": before["trims"], "after": after["trims"] },
                }))
            },
        ),
        spec(
            "object.setConnector",
            "What follows an object before the next part of the same thread (beyond PE-DESIGN): jump (a jump and trim, as PE-DESIGN) or run (a 2 mm running-stitch connector, visible on the fabric). Only between objects of one thread with no stop between; connectors of 15 mm or more are listed in longConnectors",
            Effect::Edit,
            vec![
                IDS,
                req(
                    "connector",
                    "jump|run",
                    "Jump and trim, or a running connector",
                ),
            ],
            |s, p| {
                let run = match p.string("connector", None)?.as_str() {
                    "jump" => false,
                    "run" => true,
                    _ => return Err(p.invalid("connector", "must be jump or run")),
                };
                for i in indexes(p, &s.design)? {
                    s.design.objects[i].connector_run = run;
                }
                Ok(
                    json!({ "connector": if run { "run" } else { "jump" }, "longConnectors": long_connectors(&s.design) }),
                )
            },
        ),
    ]
}

/// Stitches the design saves by leaving out hidden stitching (#511).
pub fn hidden_saved(d: &Design) -> usize {
    let on = Compiler::for_design(d).compile(d).count(Command::Stitch);
    let mut plain = d.clone();
    plain.production.remove_hidden = None;
    plain.objects.iter_mut().for_each(|o| o.hide_under = None);
    if plain == *d {
        return 0;
    }
    let off = Compiler::for_design(&plain)
        .compile(&plain)
        .count(Command::Stitch);
    off.saturating_sub(on)
}

/// Run connectors of 15 mm or more: they show on the fabric (#545).
pub fn long_connectors(d: &Design) -> Value {
    let long: Vec<Value> = Compiler::for_design(d)
        .connectors(d)
        .into_iter()
        .filter(|&(_, len)| len >= openstitch_design::hidden::LONG_CONNECTOR)
        .map(|(id, len)| json!({ "after": id, "lengthMM": (len / MM * 10.0).round() / 10.0 }))
        .collect();
    Value::Array(long)
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
        "travelUnder": d.production.travel_under,
        "removeHidden": d.production.remove_hidden,
        "hideUnder": d.objects.iter().filter(|o| o.hide_under.is_some()).map(|o| o.id).collect::<Vec<_>>(),
        "hiddenStitchesSaved": hidden_saved(d),
        "runConnectors": d.objects.iter().filter(|o| o.connector_run).map(|o| o.id).collect::<Vec<_>>(),
        "longConnectors": long_connectors(d),
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
