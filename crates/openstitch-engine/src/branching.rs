//! Branching and auto-routing (#546, roadmap B4, beyond PE-DESIGN):
//! - `shape.autoRoute` joins running lines that touch or cross into one walk per connected
//!   piece (doubling back only where it must) or sews each twice (Redwork);
//! - `object.branch` sews touching objects of one thread in one go: each starts where the one
//!   before it touches it, so the move between them is nothing (no jump, no trim); re-linked
//!   after every edit ([`relink`]).

use crate::commands::{CommandSpec, Effect, MM, indexes, opt, req, spec};
use openstitch_design::{Contour, Design, DesignObject, ObjectKind, Vec2, v2};
use serde_json::json;

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "shape.autoRoute",
            "Auto-route running lines (beyond PE-DESIGN): lines that touch or cross (ends within join of another line count) become one line per connected piece, sewn in one walk that doubles back only where it must; redwork sews every line twice and returns to its start. The lines' first object keeps its line settings; one undo step",
            Effect::Edit,
            vec![
                req(
                    "ids",
                    "[int]",
                    "Line objects to route (shapes with a line and no region)",
                ),
                opt(
                    "redwork",
                    "bool",
                    "Sew every line twice, out and back (default false)",
                ),
                opt(
                    "join",
                    "mm",
                    "Ends this close to another line join it 0-5 (default 0.5)",
                ),
                opt(
                    "keep",
                    "bool",
                    "Keep the original lines, hidden (default false)",
                ),
            ],
            |s, p| {
                let mut idx = indexes(p, &s.design)?;
                idx.sort_unstable();
                idx.dedup();
                for &i in &idx {
                    let o = &s.design.objects[i];
                    if o.kind != ObjectKind::Shape || o.region.is_some() || o.line.is_none() {
                        return Err(p.failed(format!(
                            "object {} is not a line (a shape with a line and no region)",
                            o.id
                        )));
                    }
                }
                let redwork = p.bool("redwork", Some(false))?;
                let join = p.opt("join", 0.5, 0.0, 5.0)? * MM;
                let keep = p.bool("keep", Some(false))?;
                let d = &mut s.design;
                // Each thread is routed on its own.
                let mut threads: Vec<openstitch_core::Thread> = Vec::new();
                for &i in &idx {
                    let t = d.objects[i].line.as_ref().expect("line").thread.clone();
                    if !threads.contains(&t) {
                        threads.push(t);
                    }
                }
                let at = idx[0];
                let mut made = Vec::new();
                let mut start: Option<Vec2> = None;
                for t in threads {
                    let members: Vec<usize> = idx
                        .iter()
                        .copied()
                        .filter(|&i| d.objects[i].line.as_ref().is_some_and(|l| l.thread == t))
                        .collect();
                    let lines: Vec<Vec<Vec2>> = members
                        .iter()
                        .flat_map(|&i| d.objects[i].contours.iter().map(Contour::path))
                        .filter(|l| l.len() >= 2)
                        .collect();
                    let mut line = d.objects[members[0]].line.clone().expect("line");
                    line.entry = None;
                    for walk in openstitch_design::graph::route(&lines, join, redwork, start) {
                        start = walk.last().copied();
                        let mut o = DesignObject::shape(
                            vec![Contour::new(walk, false)],
                            None,
                            Some(line.clone()),
                        );
                        o.name = Some(if redwork { "Redwork" } else { "Routed line" }.into());
                        made.push(o);
                    }
                }
                let count = made.len();
                if keep {
                    for &i in &idx {
                        d.objects[i].visible = false;
                    }
                } else {
                    for &i in idx.iter().rev() {
                        d.objects.remove(i);
                    }
                }
                let ids: Vec<u64> = made
                    .into_iter()
                    .enumerate()
                    .map(|(k, o)| crate::wizards::insert_at(d, at + k, o))
                    .collect();
                Ok(json!({ "ids": ids, "runs": count }))
            },
        ),
        spec(
            "object.branch",
            "Branch (beyond PE-DESIGN): sew touching objects in one go in the first one's thread. They are grouped and ordered nearest-next from the first (or from start); each starts where the one before it touches it, so the needle moves no further between them, and objects that do not touch trim as usual. Re-linked after every edit; object.unbranch takes it apart",
            Effect::Edit,
            vec![
                req("ids", "[int]", "Objects to branch (2 or more shapes)"),
                opt(
                    "start",
                    "[x, y] mm",
                    "Start from the object nearest this point (default the first listed)",
                ),
            ],
            |s, p| {
                let mut idx: Vec<usize> = Vec::new();
                for i in indexes(p, &s.design)? {
                    if !idx.contains(&i) {
                        idx.push(i);
                    }
                }
                if idx.len() < 2 {
                    return Err(p.invalid("ids", "needs at least 2 objects"));
                }
                if let Some(o) = idx
                    .iter()
                    .map(|&i| &s.design.objects[i])
                    .find(|o| o.kind != ObjectKind::Shape)
                {
                    return Err(p.failed(format!("object {} is not a shape", o.id)));
                }
                let d = &mut s.design;
                let first = if p.has("start") {
                    let xy = p
                        .get("start")
                        .and_then(|v| v.as_array())
                        .filter(|a| a.len() == 2);
                    let pt = xy
                        .and_then(|a| Some(v2(a[0].as_f64()? * MM, a[1].as_f64()? * MM)))
                        .ok_or_else(|| p.invalid("start", "must be [x, y] in mm"))?;
                    *idx.iter()
                        .min_by(|&&a, &&b| {
                            let dist = |i: usize| {
                                d.objects[i]
                                    .bounds()
                                    .map_or(f64::INFINITY, |r| r.center().distance(pt))
                            };
                            dist(a).total_cmp(&dist(b))
                        })
                        .expect("two or more")
                } else {
                    idx[0]
                };
                // Nearest-next by the gap between outlines.
                let mut order = vec![first];
                let mut left: Vec<usize> = idx.iter().copied().filter(|&i| i != first).collect();
                while !left.is_empty() {
                    let last = *order.last().expect("non-empty");
                    let (k, _) = left
                        .iter()
                        .enumerate()
                        .map(|(k, &i)| {
                            (
                                k,
                                crate::order::travel_cost(&d.objects[last], &d.objects[i], false),
                            )
                        })
                        .min_by(|a, b| a.1.total_cmp(&b.1))
                        .expect("non-empty");
                    order.push(left.remove(k));
                }
                let thread = crate::wizards::pattern_colour(&d.objects[first]);
                let group = d.new_group_id();
                let at = *idx.iter().min().expect("non-empty");
                let mut slots: Vec<Option<DesignObject>> = std::mem::take(&mut d.objects)
                    .into_iter()
                    .map(Some)
                    .collect();
                let mut members: Vec<DesignObject> = order
                    .iter()
                    .map(|&i| slots[i].take().expect("each once"))
                    .collect();
                for o in &mut members {
                    o.branch = true;
                    o.group = Some(group);
                    if let Some(r) = &mut o.region {
                        r.thread = thread.clone();
                    }
                    if let Some(l) = &mut o.line {
                        l.thread = thread.clone();
                    }
                }
                let mut rest: Vec<DesignObject> = slots.into_iter().flatten().collect();
                let at = at.min(rest.len());
                let ids: Vec<u64> = members.iter().map(|o| o.id).collect();
                rest.splice(at..at, members);
                d.objects = rest;
                relink(d);
                let gaps: Vec<f64> = ids
                    .windows(2)
                    .map(|w| {
                        let (a, b) = (
                            d.index_of(w[0]).expect("kept"),
                            d.index_of(w[1]).expect("kept"),
                        );
                        (crate::order::travel_cost(&d.objects[a], &d.objects[b], false) / MM * 10.0)
                            .round()
                            / 10.0
                    })
                    .collect();
                Ok(json!({ "order": ids, "group": group, "gapsMM": gaps }))
            },
        ),
        spec(
            "object.unbranch",
            "Take a branch apart: its objects stay where they are in the sewing order but are no longer grouped or linked",
            Effect::Edit,
            vec![req(
                "ids",
                "[int] | id: int",
                "Any members of the branches to take apart",
            )],
            |s, p| {
                let groups: Vec<u64> = indexes(p, &s.design)?
                    .into_iter()
                    .filter(|&i| s.design.objects[i].branch)
                    .filter_map(|i| s.design.objects[i].group)
                    .collect();
                if groups.is_empty() {
                    return Err(p.failed("none of these objects is in a branch"));
                }
                let mut n = 0;
                for o in &mut s.design.objects {
                    if o.branch && o.group.is_some_and(|g| groups.contains(&g)) {
                        o.branch = false;
                        o.group = None;
                        n += 1;
                    }
                }
                Ok(json!({ "unbranched": n }))
            },
        ),
    ]
}

/// Links every branch again: each member that sews right after another member of its branch
/// is entered where that one is left, at the closest points of their outlines. Called after
/// every edit, so a branch follows its members' changes.
pub fn relink(d: &mut Design) {
    if !d.objects.iter().any(|o| o.branch) {
        return;
    }
    for k in 0..d.objects.len().saturating_sub(1) {
        let (a, b) = (&d.objects[k], &d.objects[k + 1]);
        if a.branch && b.branch && a.group.is_some() && a.group == b.group {
            crate::order::link_entry_exit_with(d, k..k + 1, false);
        }
    }
}
