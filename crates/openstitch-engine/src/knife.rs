//! Knife (#477): one open cut line splits every object it crosses in one step: regions into
//! their pieces on either side, Manual Punch columns and satin skeletons across the column,
//! lines at the crossings (a closed outline cut once opens there). Every piece keeps the
//! object's sewing attributes; entry and exit points stay where they are still on its outline
//! (#652). The first piece keeps the object's id and place, the others are sewn right after it. Beyond PE-DESIGN; its
//! Split with Outline (`shape.split`) is unchanged. Geometry in `openstitch_design::knife`.

use crate::commands::{CommandSpec, Effect, MM, opt, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_design::boolean::{self, Region};
use openstitch_design::{Contour, Design, DesignObject, ObjectKind, Vec2, column, knife, skeleton};
use serde_json::{Value, json};

/// Most points a cut line may have.
const MAX_POINTS: usize = 2_000;
/// Outlines this close (0.1 mm) count as derived from their direction lines or skeleton.
const SAME: f64 = 0.05;

fn same_outline(a: &Contour, b: &Contour) -> bool {
    let close = |p: Vec2, q: Vec2| p.distance(q) <= SAME;
    a.closed == b.closed
        && a.points.len() == b.points.len()
        && a.points.iter().zip(&b.points).all(|(p, q)| close(*p, *q))
        && (0..a.segment_count()).all(|i| match (a.curve(i), b.curve(i)) {
            (None, None) => true,
            (Some(x), Some(y)) => close(x.c1, y.c1) && close(x.c2, y.c2),
            _ => false,
        })
}

fn column_outline(lines: &[[Vec2; 2]], curved: bool) -> Contour {
    if curved {
        column::curved_outline(lines)
    } else {
        column::outline(lines)
    }
}

/// The piece of `o` with new geometry: sewing attributes kept, entry and exit points kept where
/// they are still on the piece's outline (#652).
fn piece(o: &DesignObject, contours: Vec<Contour>) -> DesignObject {
    let mut p = o.clone();
    p.contours = contours;
    crate::shape_ops::keep_points_on_outline(&mut p);
    p
}

/// The pieces `o` is cut into along `line`, or why it is not cut.
fn cut(o: &DesignObject, line: &[Vec2]) -> Result<Vec<DesignObject>, String> {
    match o.kind {
        ObjectKind::Shape => {}
        ObjectKind::Text => return Err("text; convert it to shapes first (text.toShapes)".into()),
        ObjectKind::Stitches => return Err("a stitch pattern".into()),
    }
    if o.hole_sewing {
        return Err("shapes with hole sewing cannot be split".into());
    }
    if o.contours.is_empty() {
        return Err("no outline".into());
    }
    if let Some(r) = &o.region {
        // A Manual Punch column: cut across the column, the cut a direction line of both.
        if r.direction_lines.len() >= 2
            && o.contours.len() == 1
            && same_outline(
                &o.contours[0],
                &column_outline(&r.direction_lines, r.curved_lines),
            )
        {
            let halves = knife::column_cut(&r.direction_lines, r.curved_lines, line)
                .ok_or("the line does not cross the column from side to side")?;
            return Ok(halves
                .into_iter()
                .map(|h| {
                    let mut p = piece(o, vec![column_outline(&h, r.curved_lines)]);
                    p.region.as_mut().expect("region").direction_lines = h;
                    p
                })
                .collect());
        }
        // A satin skeleton: cut across the centre line.
        if let Some(sk) = &r.skeleton
            && o.contours.len() == 1
            && skeleton::outline(sk).is_some_and(|c| same_outline(&o.contours[0], &c))
        {
            let parts =
                knife::skeleton_cut(sk, line).ok_or("the line does not cross the centre line")?;
            return parts
                .into_iter()
                .map(|k| {
                    let c = skeleton::outline(&k).ok_or("a piece would be too short")?;
                    let mut p = piece(o, vec![c]);
                    p.region.as_mut().expect("region").skeleton = Some(k);
                    Ok(p)
                })
                .collect();
        }
        if o.contours.iter().all(|c| c.closed) {
            let pieces: Vec<Region> =
                knife::region(&o.contours, line).ok_or("the line does not cross it fully")?;
            return Ok(pieces
                .iter()
                .map(|r| piece(o, boolean::contours(std::slice::from_ref(r))))
                .collect());
        }
    }
    // Lines: every contour cut where crossed; uncut contours stay with the first piece.
    let mut kept = Vec::new();
    let mut pieces = Vec::new();
    for c in &o.contours {
        match knife::path(c, line) {
            Some(parts) => pieces.extend(parts),
            None => kept.push(c.clone()),
        }
    }
    if pieces.is_empty() {
        return Err("the line does not cross it".into());
    }
    let mut out = Vec::new();
    if !kept.is_empty() {
        out.push(piece(o, kept));
    }
    out.extend(pieces.into_iter().map(|c| piece(o, vec![c])));
    Ok(out)
}

/// Replaces object `i` with `pieces`: the first keeps its id and place, the rest follow it.
fn replace(d: &mut Design, i: usize, mut pieces: Vec<DesignObject>) -> Vec<u64> {
    let id = d.objects[i].id;
    let mut rest = pieces.split_off(1);
    let mut first = pieces.pop().expect("one piece at least");
    first.id = id;
    d.objects[i] = first;
    let mut ids = vec![id];
    for (k, p) in rest.iter_mut().enumerate() {
        p.id = d.next_object_id;
        p.name = None;
        d.next_object_id += 1;
        ids.push(p.id);
        d.objects.insert(i + 1 + k, p.clone());
    }
    // Text fitted to a cut outline comes off it.
    for t in 0..d.objects.len() {
        if d.objects[t].text.as_ref().and_then(|x| x.path) == Some(id) {
            d.detach_text(t);
        }
    }
    ids
}

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "shape.knife",
        "Knife: cut every object an open line crosses (regions into both sides, Manual Punch columns and satin skeletons across the column, lines at the crossings; a closed outline cut once opens there), or with `at` split the line under a point; sewing attributes kept (angle included), entry/exit kept where still on the piece's outline, the first piece keeps the id",
        Effect::Edit,
        vec![
            opt(
                "line",
                "[[x, y]] mm",
                "Cut line, 2 or more points (give line or at)",
            ),
            opt(
                "at",
                "[x, y] mm",
                "Split the nearest line (open or closed outline without a region) at this point",
            ),
            opt(
                "reach",
                "mm",
                "With at: how far the line may be from the point (default 0.5)",
            ),
            opt(
                "ids",
                "[int]",
                "Objects to cut (default every visible object the line crosses or lies in; those it does not cut through are reported in skipped)",
            ),
        ],
        |s, p| {
            if p.has("line") == p.has("at") {
                return Err(p.invalid("line", "give a cut line or a point (at), not both"));
            }
            if p.has("at") {
                return split_at(s, p);
            }
            let line = p.points("line", 2)?;
            if line.len() > MAX_POINTS {
                return Err(p.invalid("line", format!("at most {MAX_POINTS} points")));
            }
            if line.windows(2).all(|w| w[0].distance(w[1]) < 1e-6) {
                return Err(p.invalid("line", "needs two different points"));
            }
            let d = &s.design;
            let targets: Vec<u64> = if p.has("ids") {
                let ids = p.ids()?;
                for &id in &ids {
                    d.index_of(id)
                        .ok_or_else(|| p.not_found(format!("object {id}")))?;
                }
                ids
            } else {
                d.objects
                    .iter()
                    .filter(|o| {
                        o.visible
                            && (knife::crosses(&o.contours, &line)
                                || line.iter().any(|&q| {
                                    openstitch_design::geometry::contains(q, &o.contours)
                                }))
                    })
                    .map(|o| o.id)
                    .collect()
            };
            let mut done = Vec::new();
            let mut skipped = Vec::new();
            for id in targets {
                let i = s.design.index_of(id).expect("checked");
                match cut(&s.design.objects[i], &line) {
                    Ok(pieces)
                        if pieces.len() >= 2
                            || s.design.objects[i].contours != pieces[0].contours =>
                    {
                        let ids = replace(&mut s.design, i, pieces);
                        done.push(json!({ "id": id, "pieces": ids }));
                    }
                    Ok(_) => {
                        skipped.push(json!({ "id": id, "reason": "the line does not cut it" }))
                    }
                    Err(reason) => skipped.push(json!({ "id": id, "reason": reason })),
                }
            }
            if done.is_empty() {
                let why: Vec<String> = skipped
                    .iter()
                    .map(|v| format!("object {}: {}", v["id"], v["reason"].as_str().unwrap_or("")))
                    .collect();
                return Err(p.failed(if why.is_empty() {
                    "the line crosses no object".to_string()
                } else {
                    format!("nothing was cut ({})", why.join("; "))
                }));
            }
            let ids: Vec<Value> = done
                .iter()
                .flat_map(|v| v["pieces"].as_array().cloned().unwrap_or_default())
                .collect();
            Ok(json!({ "cut": done, "skipped": skipped, "ids": ids }))
        },
    )]
}

/// Click to split: the nearest line within reach of `at` is cut there.
fn split_at(s: &mut EngineState, p: &Params) -> Result<Value, CommandError> {
    let at = p.point("at")?;
    let reach = p.opt("reach", 0.5, 0.01, 50.0)? * MM;
    let d = &s.design;
    let mut best: Option<(f64, usize, usize, Vec<Contour>)> = None;
    for (i, o) in d.objects.iter().enumerate() {
        if !o.visible || o.kind != ObjectKind::Shape || o.region.is_some() || o.line.is_none() {
            continue;
        }
        for (k, c) in o.contours.iter().enumerate() {
            let near = knife::nearest(at, &c.path()).0;
            if near <= reach
                && best.as_ref().is_none_or(|b| near < b.0)
                && let Some(parts) = knife::path_at(c, at, reach)
            {
                best = Some((near, i, k, parts));
            }
        }
    }
    let Some((_, i, k, parts)) = best else {
        return Err(p.failed("no line within reach of the point"));
    };
    let o = &s.design.objects[i];
    let id = o.id;
    let mut kept: Vec<Contour> = o.contours.clone();
    kept.remove(k);
    let mut pieces = Vec::new();
    if !kept.is_empty() {
        pieces.push(piece(o, kept));
    }
    pieces.extend(parts.into_iter().map(|c| piece(o, vec![c])));
    let ids = replace(&mut s.design, i, pieces);
    Ok(json!({ "cut": [{ "id": id, "pieces": ids }], "skipped": [], "ids": ids }))
}
