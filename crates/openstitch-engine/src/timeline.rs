//! `design.timeline` (#516): the design as the machine sews it, in colour blocks with their
//! trims and objects and the machine time at each, for the stitch-out player, MCP and the
//! Assistant ("when does the red start?").

use crate::commands::{CommandSpec, Effect, opt, spec};
use openstitch_design::Compiler;
use openstitch_design::timeline::Timeline;
use serde_json::{Value, json};

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "design.timeline",
        "Colour blocks in sewing order with their stitch ranges, trims, stops, the objects that start in them and the machine time at each (seconds, from the Design Settings speed); positions count sewn stitches as the Stitch Simulator does",
        Effect::Read,
        vec![opt(
            "at",
            "int",
            "Also report the block, object and time when this many stitches are sewn",
        )],
        |s, p| {
            let d = &s.design;
            let (pattern, spans) = Compiler::for_design(d).compile_traced(d);
            let t = Timeline::build(&pattern, &spans, &d.machine);
            let mut v = timeline_json(&t, &pattern);
            if p.has("at") {
                let n = p.int("at", None, 0, t.total as i64)? as usize;
                v["at"] = at_json(&t, n);
            }
            Ok(v)
        },
    )]
}

/// Seconds rounded to 0.1 s.
fn secs(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

pub fn timeline_json(t: &Timeline, pattern: &openstitch_core::StitchPattern) -> Value {
    let blocks: Vec<Value> = t
        .blocks
        .iter()
        .map(|b| {
            let thread = pattern.threads.get(b.thread);
            json!({
                "thread": b.thread,
                "color": thread.map(|c| format!("#{:02x}{:02x}{:02x}", c.red, c.green, c.blue)),
                "name": thread.and_then(|c| c.name.as_deref()),
                "brand": thread.and_then(|c| c.brand.as_deref()),
                "number": thread.and_then(|c| c.catalog_number.as_deref()),
                "startStitch": b.start,
                "endStitch": b.end,
                "endKind": b.end_kind.as_str(),
                "trims": b.trims,
                "objects": b.objects.iter().map(|(id, at)| json!({ "id": id, "startStitch": at })).collect::<Vec<_>>(),
                "startSeconds": secs(t.seconds_at(b.start)),
                "endSeconds": secs(t.seconds_at(b.end)),
            })
        })
        .collect();
    json!({
        "totalStitches": t.total,
        "totalSeconds": secs(t.total_seconds()),
        "blocks": blocks,
    })
}

fn at_json(t: &Timeline, n: usize) -> Value {
    let object = t
        .blocks
        .iter()
        .flat_map(|b| b.objects.iter())
        .filter(|o| o.1 <= n)
        .max_by_key(|o| o.1)
        .map(|o| o.0);
    json!({
        "stitch": n,
        "block": t.block_at(n),
        "object": object,
        "seconds": secs(t.seconds_at(n)),
    })
}
