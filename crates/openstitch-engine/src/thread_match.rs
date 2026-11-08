//! Matching a design's threads to a chart and merging similar ones (#484, #485), and finding
//! threads across every chart by code or name (#486).
//!
//! `threads.remap` keeps PE-DESIGN's rule by default: the nearest chart colour by equal-weight
//! RGB distance (#119; earlier chart entry on ties, as the command always did). A perceptual
//! metric (CIEDE2000 or Oklab, [`openstitch_core::colour`]), the dry-run plan and the
//! keep-distinct rule are opt-in. `threads.mergeSimilar` has no PE-DESIGN equivalent.

use crate::commands::{CommandSpec, Effect, indexes, opt, req, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use crate::user::{builtin_charts, builtin_entries};
use openstitch_core::colour::{Metric, delta_e, ranked_by};
use openstitch_core::{Command, PEC_BASIC_COLORS, Thread};
use openstitch_design::{Compiler, Design};
use serde_json::{Value, json};
use std::collections::HashMap;

/// Two design colours further apart than this (ΔE00) are "clearly different": `keepDistinct`
/// keeps them on different threads.
pub const DISTINCT_DELTA_E: f64 = 10.0;
/// Most results `threads.find` returns.
pub const MAX_FIND: usize = 500;

/// One built-in chart: id, display name, kind (`machine` or `brand`) and its numbered entries.
pub struct Builtin {
    pub id: &'static str,
    pub name: &'static str,
    pub kind: &'static str,
    pub entries: Vec<(usize, Thread)>,
}

/// Every built-in chart: the machine charts ([`builtin_charts`]) and the thread makers' charts
/// (`openstitch_core::brands`), in the order they are offered.
pub fn builtins() -> Vec<Builtin> {
    let mut v: Vec<Builtin> = builtin_charts()
        .into_iter()
        .map(|(id, name, chart)| Builtin {
            id,
            name,
            kind: "machine",
            entries: builtin_entries(id, chart),
        })
        .collect();
    v.extend(
        openstitch_core::brands::brand_charts()
            .into_iter()
            .map(|(id, name, threads)| Builtin {
                id,
                name,
                kind: "brand",
                entries: threads.iter().cloned().enumerate().collect(),
            }),
    );
    v
}

/// The threads of built-in chart `id` that a colour may be matched to: for the Brother chart
/// entries 1–61, as PE-DESIGN matches ([`openstitch_core::pec_basic_chart`]).
pub fn builtin_match_threads(id: &str) -> Option<Vec<Thread>> {
    builtins().into_iter().find(|b| b.id == id).map(|b| {
        b.entries
            .into_iter()
            .filter(|(i, _)| id != "brother" || *i <= PEC_BASIC_COLORS)
            .map(|(_, t)| t)
            .collect()
    })
}

/// The chart a command names: a user chart (`chart`) or a built-in one (`builtin`).
fn target_chart(s: &EngineState, p: &Params) -> Result<(String, Vec<Thread>), CommandError> {
    match (p.has("chart"), p.has("builtin")) {
        (true, false) => {
            let n = p.string("chart", None)?;
            let threads = s
                .user
                .chart_index(&n)
                .map(|i| s.user.thread_charts[i].threads.clone())
                .ok_or_else(|| p.not_found(format!("thread chart '{n}'")))?;
            Ok((n, threads))
        }
        (false, true) => {
            let id = p.string("builtin", None)?;
            let threads = builtin_match_threads(&id).ok_or_else(|| {
                let ids: Vec<&str> = builtins().iter().map(|b| b.id).collect();
                p.invalid("builtin", format!("use one of {}", ids.join(", ")))
            })?;
            Ok((id, threads))
        }
        _ => Err(p.invalid("chart", "give either chart or builtin")),
    }
}

fn metric(p: &Params, default: Metric, allowed: &[Metric]) -> Result<Metric, CommandError> {
    if !p.has("metric") {
        return Ok(default);
    }
    let m = p.string("metric", None)?;
    Metric::parse(&m)
        .filter(|m| allowed.contains(m))
        .ok_or_else(|| {
            let names: Vec<&str> = allowed.iter().map(|m| m.name()).collect();
            p.invalid("metric", format!("use one of {}", names.join(", ")))
        })
}

/// The objects a command works on: `ids`, or every object.
fn chosen(p: &Params, d: &Design) -> Result<Vec<usize>, CommandError> {
    if p.has("ids") || p.has("id") {
        let mut v = indexes(p, d)?;
        v.sort_unstable();
        v.dedup();
        Ok(v)
    } else {
        Ok((0..d.objects.len()).collect())
    }
}

/// Every thread slot of an object, in the order the object sews them.
fn threads_of(o: &openstitch_design::DesignObject) -> Vec<&Thread> {
    let mut v: Vec<&Thread> = Vec::new();
    if let Some(b) = &o.stitch_block {
        v.extend(b.threads.iter());
    }
    if let Some(r) = &o.region {
        v.push(&r.thread);
    }
    if let Some(l) = &o.line {
        v.push(&l.thread);
    }
    v
}

/// The distinct threads of `idx`, in order of first use, with how many objects use each.
fn design_colours(d: &Design, idx: &[usize]) -> Vec<(Thread, usize)> {
    let mut out: Vec<(Thread, usize)> = Vec::new();
    for &i in idx {
        let mut seen: Vec<&Thread> = Vec::new();
        for t in threads_of(&d.objects[i]) {
            if seen.contains(&t) {
                continue;
            }
            seen.push(t);
            match out.iter_mut().find(|(x, _)| x == t) {
                Some((_, n)) => *n += 1,
                None => out.push((t.clone(), 1)),
            }
        }
    }
    out
}

/// Replaces every thread of `idx` that `map` has a new thread for.
fn apply(d: &mut Design, idx: &[usize], map: &[(Thread, Thread)]) {
    let swap = |t: &mut Thread| {
        if let Some((_, to)) = map.iter().find(|(from, _)| from == t) {
            *t = to.clone();
        }
    };
    for &i in idx {
        let o = &mut d.objects[i];
        if let Some(r) = &mut o.region {
            swap(&mut r.thread);
        }
        if let Some(l) = &mut o.line {
            swap(&mut l.thread);
        }
        if let Some(b) = &mut o.stitch_block {
            b.threads.iter_mut().for_each(swap);
        }
    }
}

/// A thread as every command reports it (#798): colour, the code on its spool (`code`:
/// Brother's 007 for a PEC chart thread, else its own catalogue number; null for a custom
/// colour), `brand`, `description`, the PEC chart number a PES/PEC file gives it (`pecIndex`)
/// and other makers' codes printed for it (`crossReferences`, #796).
pub(crate) fn describe(t: &Thread) -> Value {
    let mut v = json!({
        "color": t.hex(),
        "code": openstitch_core::thread_code(t),
        "brand": t.brand,
        "description": t.name,
        "pecIndex": openstitch_core::pec_index(t),
    });
    let refs = crate::user::cross_references(t);
    if !refs.is_empty() {
        v["crossReferences"] = Value::Array(
            refs.iter()
                .map(|(brand, code)| json!({ "brand": brand, "code": code }))
                .collect(),
        );
    }
    v
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// The thread of `chart` each design colour goes to. With `keep_distinct`, a colour does not
/// take a thread already given to a colour more than [`DISTINCT_DELTA_E`] away from it; it
/// takes its next-nearest thread instead, and only when no thread is left does it share (the
/// returned list of forced merges names those colours).
fn plan_remap(
    colours: &[(Thread, usize)],
    chart: &[Thread],
    metric: Metric,
    keep_distinct: bool,
) -> (Vec<usize>, Vec<usize>) {
    let mut picks: Vec<usize> = Vec::with_capacity(colours.len());
    let mut forced = Vec::new();
    for (k, (c, _)) in colours.iter().enumerate() {
        let ranked = ranked_by(c, chart, metric);
        let free = |j: usize| {
            picks
                .iter()
                .zip(colours)
                .all(|(&p, (o, _))| p != j || delta_e(c, o) <= DISTINCT_DELTA_E)
        };
        let pick = if keep_distinct {
            ranked.iter().copied().find(|&j| free(j))
        } else {
            None
        };
        let pick = pick.unwrap_or_else(|| {
            if keep_distinct {
                forced.push(k);
            }
            ranked[0]
        });
        picks.push(pick);
    }
    (picks, forced)
}

fn remap(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let (name, chart) = target_chart(s, p)?;
    if chart.is_empty() {
        return Err(p.failed("the chart is empty"));
    }
    let metric = metric(
        p,
        Metric::Rgb,
        &[Metric::Rgb, Metric::Ciede2000, Metric::Oklab],
    )?;
    let keep_distinct = p.bool("keepDistinct", Some(false))?;
    let dry_run = p.bool("dryRun", Some(false))?;
    let idx = chosen(p, &s.design)?;
    let colours = design_colours(&s.design, &idx);
    let (picks, forced) = plan_remap(&colours, &chart, metric, keep_distinct);
    let mut map = Vec::new();
    let mut plan = Vec::new();
    let mut total = 0.0;
    for ((c, objects), &j) in colours.iter().zip(&picks) {
        let to = &chart[j];
        let de = delta_e(c, to);
        total += de;
        plan.push(json!({
            "from": describe(c),
            "to": describe(to),
            "deltaE": round2(de),
            "objects": objects,
        }));
        map.push((c.clone(), to.clone()));
    }
    // Clearly different design colours that still share a thread.
    let mut shared = Vec::new();
    for (j, to) in chart.iter().enumerate() {
        let from: Vec<&Thread> = colours
            .iter()
            .zip(&picks)
            .filter(|&(_, &p)| p == j)
            .map(|((c, _), _)| c)
            .collect();
        let apart = from.iter().enumerate().any(|(a, x)| {
            from[a + 1..]
                .iter()
                .any(|y| delta_e(x, y) > DISTINCT_DELTA_E)
        });
        if apart {
            shared.push(json!({
                "to": describe(to),
                "from": from.iter().map(|t| t.hex()).collect::<Vec<_>>(),
                "forced": forced.iter().any(|&k| picks[k] == j),
            }));
        }
    }
    if !dry_run {
        apply(&mut s.design, &idx, &map);
    }
    Ok(json!({
        "chart": name,
        "metric": metric.name(),
        "applied": !dry_run,
        "plan": plan,
        "meanDeltaE": if colours.is_empty() { 0.0 } else { round2(total / colours.len() as f64) },
        "merged": shared,
    }))
}

/// Stitches sewn in each thread of the design.
fn stitches_by_thread(d: &Design) -> (HashMap<Thread, usize>, usize) {
    let pattern = Compiler::for_design(d).compile(d);
    let mut m = HashMap::new();
    for (t, stitches) in pattern.color_blocks() {
        if let Some(t) = t {
            let n = stitches
                .iter()
                .filter(|s| s.command == Command::Stitch)
                .count();
            *m.entry(t.clone()).or_insert(0) += n;
        }
    }
    (m, pattern.count(Command::ColorChange))
}

fn color_changes(d: &Design) -> usize {
    Compiler::for_design(d)
        .compile(d)
        .count(Command::ColorChange)
}

/// Groups of the design's colours closer than `max` to a kept colour. The colours are taken
/// in order of stitches sewn (then first use); each joins the nearest kept colour within `max`
/// of it, or is kept itself. So every merged colour is within `max` of the colour it becomes,
/// and the colour that sews most in a group is the one kept. Returns, per colour, the index of
/// the colour it becomes.
fn plan_merge(colours: &[Thread], weights: &[usize], metric: Metric, max: f64) -> Vec<usize> {
    let mut order: Vec<usize> = (0..colours.len()).collect();
    order.sort_by(|&a, &b| weights[b].cmp(&weights[a]));
    let mut target: Vec<usize> = (0..colours.len()).collect();
    let mut kept: Vec<usize> = Vec::new();
    for i in order {
        let best = kept
            .iter()
            .map(|&k| (metric.distance(&colours[i], &colours[k]), k))
            .filter(|&(d, _)| d <= max)
            .min_by(|a, b| a.0.total_cmp(&b.0));
        match best {
            Some((_, k)) => target[i] = k,
            None => kept.push(i),
        }
    }
    target
}

fn merge_similar(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let max = p.opt("maxDeltaE", 5.0, 0.1, 50.0)?;
    let metric = metric(p, Metric::Ciede2000, &[Metric::Ciede2000, Metric::Oklab])?;
    let dry_run = p.bool("dryRun", Some(false))?;
    let idx = chosen(p, &s.design)?;
    let colours: Vec<Thread> = design_colours(&s.design, &idx)
        .into_iter()
        .map(|(t, _)| t)
        .collect();
    let (sewn, before) = stitches_by_thread(&s.design);
    let weights: Vec<usize> = colours
        .iter()
        .map(|t| sewn.get(t).copied().unwrap_or(0))
        .collect();
    let target = plan_merge(&colours, &weights, metric, max);
    let map: Vec<(Thread, Thread)> = target
        .iter()
        .enumerate()
        .filter(|&(i, &k)| i != k)
        .map(|(i, &k)| (colours[i].clone(), colours[k].clone()))
        .collect();
    let mut groups = Vec::new();
    for (k, kept) in colours.iter().enumerate() {
        let from: Vec<Value> = (0..colours.len())
            .filter(|&i| i != k && target[i] == k)
            .map(|i| {
                json!({
                    "thread": describe(&colours[i]),
                    "deltaE": round2(metric.distance(&colours[i], kept)),
                })
            })
            .collect();
        if !from.is_empty() {
            groups.push(json!({ "keep": describe(kept), "merge": from }));
        }
    }
    let mut merged = s.design.clone();
    apply(&mut merged, &idx, &map);
    let after = color_changes(&merged);
    let mut sorted = merged.clone();
    let ids: Vec<u64> = idx.iter().map(|&i| sorted.objects[i].id).collect();
    crate::drawing::group_by_colour(&mut sorted, &ids);
    let by_color = color_changes(&sorted);
    let kept = target.iter().enumerate().filter(|&(i, &k)| i == k).count();
    if !dry_run {
        s.design = merged;
    }
    Ok(json!({
        "metric": metric.name(),
        "maxDeltaE": max,
        "applied": !dry_run,
        "colors": { "before": colours.len(), "after": kept },
        "groups": groups,
        "colorChanges": { "before": before, "after": after, "afterSortByColor": by_color },
        "suggestSortByColor": by_color < after,
    }))
}

/// How well `t` matches a `threads.find` query (lower is better): 0 the exact code, 1 a code
/// prefix, 2 a name containing it, 3 the code or brand containing it; `None` when it does not
/// match. The editor's chart pickers filter with it too (#486).
pub fn find_rank(t: &Thread, query: &str) -> Option<u8> {
    let code = t.catalog_number.as_deref().unwrap_or("");
    let q = query.to_lowercase();
    if !code.is_empty() && code.eq_ignore_ascii_case(query) {
        return Some(0);
    }
    if !code.is_empty() && code.to_lowercase().starts_with(&q) {
        return Some(1);
    }
    let has = |s: Option<&str>| s.is_some_and(|s| s.to_lowercase().contains(&q));
    if has(t.name.as_deref()) {
        return Some(2);
    }
    // Another maker's code printed for the thread (#796: "1801" finds New Brothread 001).
    let refs = crate::user::cross_references(t);
    if refs.iter().any(|(_, c)| c.eq_ignore_ascii_case(query)) {
        return Some(1);
    }
    if has(Some(code)) || has(t.brand.as_deref()) {
        return Some(3);
    }
    if refs.iter().any(|(_, c)| c.to_lowercase().starts_with(&q)) {
        return Some(3);
    }
    None
}

fn find(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let query = p.string("query", None)?;
    let query = query.trim();
    if query.is_empty() || query.chars().count() > 100 || query.chars().any(char::is_control) {
        return Err(p.invalid("query", "must be 1-100 printable characters"));
    }
    let limit = p.int("limit", Some(50), 1, MAX_FIND as i64)? as usize;
    let only = if p.has("charts") {
        let v = p.array("charts")?;
        let mut names = Vec::new();
        for x in v {
            names.push(
                x.as_str()
                    .ok_or_else(|| p.invalid("charts", "must be chart ids or names"))?
                    .to_string(),
            );
        }
        Some(names)
    } else {
        None
    };
    let wanted = |id: &str| only.as_ref().is_none_or(|o| o.iter().any(|x| x == id));
    let mut hits: Vec<(u8, usize, Value)> = Vec::new();
    let mut order = 0;
    let mut push = |rank: u8, v: Value, hits: &mut Vec<(u8, usize, Value)>| {
        hits.push((rank, order, v));
        order += 1;
    };
    for b in builtins() {
        if !wanted(b.id) {
            continue;
        }
        for (i, t) in &b.entries {
            if let Some(r) = find_rank(t, query) {
                let mut v = describe(t);
                v["chart"] = json!(b.id);
                v["chartName"] = json!(b.name);
                v["builtin"] = json!(true);
                v["index"] = json!(i);
                push(r, v, &mut hits);
            }
        }
    }
    for c in &s.user.thread_charts {
        if !wanted(&c.name) {
            continue;
        }
        for (i, t) in c.threads.iter().enumerate() {
            if let Some(r) = find_rank(t, query) {
                let mut v = describe(t);
                v["chart"] = json!(c.name);
                v["chartName"] = json!(c.name);
                v["builtin"] = json!(false);
                v["index"] = json!(i);
                push(r, v, &mut hits);
            }
        }
    }
    hits.sort_by_key(|h| (h.0, h.1));
    let total = hits.len();
    hits.truncate(limit);
    Ok(json!({
        "total": total,
        "threads": hits.into_iter().map(|h| h.2).collect::<Vec<_>>(),
    }))
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "threads.remap",
            "Change every thread of the design (or of the given objects) to the nearest colour of a user chart (chart) or a built-in chart (builtin). The default metric is PE-DESIGN's RGB distance; ciede2000 or oklab match by perceived colour. dryRun returns the plan (design colour → thread, ΔE00, objects) without changing anything",
            Effect::Edit,
            vec![
                opt("chart", "string", "User chart name"),
                opt(
                    "builtin",
                    "string",
                    "Built-in chart id (see threads.builtinCharts)",
                ),
                opt("ids", "[int]", "Objects (default all)"),
                opt(
                    "metric",
                    "\"rgb\" | \"ciede2000\" | \"oklab\"",
                    "How colours are compared (default rgb: PE-DESIGN's rule)",
                ),
                opt(
                    "keepDistinct",
                    "bool",
                    "Two design colours more than ΔE00 10 apart never share a thread unless the chart forces it (default false)",
                ),
                opt(
                    "dryRun",
                    "bool",
                    "Only return the plan; the design is not changed (default false)",
                ),
            ],
            remap,
        ),
        spec(
            "threads.mergeSimilar",
            "Merge the design's near-identical thread colours (within maxDeltaE) into the one sewn most, to save thread changes; returns the merged groups and the colour-change counts before and after (and after order.byColor)",
            Effect::Edit,
            vec![
                opt(
                    "maxDeltaE",
                    "0.1–50",
                    "How close two colours must be to merge, in ΔE (default 5)",
                ),
                opt("ids", "[int]", "Objects (default all)"),
                opt(
                    "metric",
                    "\"ciede2000\" | \"oklab\"",
                    "Colour difference (default ciede2000)",
                ),
                opt(
                    "dryRun",
                    "bool",
                    "Only return the plan; the design is not changed (default false)",
                ),
            ],
            merge_similar,
        ),
        spec(
            "threads.find",
            "Find threads by code or name across the built-in charts and the user's charts (exact code first, then codes starting with the query, then names, brands or codes containing it); each with code, brand, description and pecIndex as separate fields",
            Effect::Read,
            vec![
                req("query", "string", "Code (\"1800\") or name (\"ruby\")"),
                opt(
                    "charts",
                    "[string]",
                    "Only these charts (built-in ids or user chart names; default all)",
                ),
                opt("limit", "1–500", "Most threads returned (default 50)"),
            ],
            find,
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keep_distinct_moves_the_second_distinct_colour_to_the_next_thread() {
        let t = Thread::rgb;
        // Two blues far apart and a chart with one blue near both and one further away.
        let colours = [(t(0x2040c0), 1), (t(0x6090f0), 1)];
        let chart = [t(0x4070e0), t(0xa0d0ff), t(0x000000)];
        let (p, forced) = plan_remap(&colours, &chart, Metric::Ciede2000, false);
        assert_eq!(p, [0, 0]);
        assert!(forced.is_empty());
        let (p, forced) = plan_remap(&colours, &chart, Metric::Ciede2000, true);
        assert_eq!(p, [0, 1]);
        assert!(forced.is_empty());
        // A one-thread chart forces the merge and says so.
        let (p, forced) = plan_remap(&colours, &chart[..1], Metric::Ciede2000, true);
        assert_eq!((p, forced), (vec![0, 0], vec![1]));
    }

    #[test]
    fn merge_keeps_the_colour_sewn_most() {
        let t = Thread::rgb;
        let colours = [t(0x2050c0), t(0x2252c2), t(0xe01010)];
        assert_eq!(
            plan_merge(&colours, &[10, 50, 30], Metric::Ciede2000, 5.0),
            [1, 1, 2]
        );
        assert_eq!(
            plan_merge(&colours, &[10, 50, 30], Metric::Ciede2000, 0.1),
            [0, 1, 2]
        );
    }

    #[test]
    fn find_ranks_codes_before_names() {
        let mut a = Thread::rgb(0);
        a.catalog_number = Some("1800".into());
        a.name = Some("Ruby".into());
        assert_eq!(find_rank(&a, "1800"), Some(0));
        assert_eq!(find_rank(&a, "18"), Some(1));
        assert_eq!(find_rank(&a, "rub"), Some(2));
        assert_eq!(find_rank(&a, "800"), Some(3));
        assert_eq!(find_rank(&a, "blue"), None);
    }
}
