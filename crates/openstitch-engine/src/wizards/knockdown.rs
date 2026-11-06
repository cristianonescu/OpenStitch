//! Knockdown (laydown) fill (#510, beyond PE-DESIGN): a light, open fill sewn first under the
//! design or the selection, slightly larger than it, that flattens the pile of towels, fleece
//! and velvet so the real stitches sit on top. The result is an ordinary Fill object (two,
//! grouped, for two crossed layers) named "Knockdown", sewn before what it covers; it keeps
//! the parameters it was made with so `update` can rebuild it after edits, and
//! `order.optimize` keeps it before what it covers ([`keep_first`]).

use super::{insert_stages, pattern_colour};
use crate::commands::{CommandSpec, Effect, MM, opt, spec, thread};
use crate::params::{CommandResult, Params};
use crate::session::EngineState;
use openstitch_design::boolean;
use openstitch_design::{
    Contour, Design, DesignObject, ObjectKind, Rect, RegionSewing, RegionType,
};
use serde_json::{Map, Value, json};

const NAME: &str = "Knockdown";

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "wizard.knockdown",
        "Knockdown fill (beyond PE-DESIGN): a light open fill sewn first under the design or the selection, reaching past its edge, so stitches do not sink into towel, fleece or velvet pile. Makes an editable Fill object named Knockdown (two grouped for two layers) before the first covered object; give update to rebuild one after edits",
        Effect::Edit,
        vec![
            opt(
                "ids",
                "[int]",
                "Objects to cover (default: the whole design, every visible object but other knockdowns)",
            ),
            opt(
                "update",
                "int",
                "A knockdown object to rebuild with its saved settings (the other parameters override them); it then covers what it covered, the whole design including new objects when made for it",
            ),
            opt(
                "reach",
                "mm",
                "How far it reaches past the objects' edge 0-10 (default 2)",
            ),
            opt(
                "gapClose",
                "mm",
                "Gaps between objects narrower than this are covered too 0-20 (default 3)",
            ),
            opt(
                "spacing",
                "mm",
                "Distance between fill rows 0.5-5 (default 1)",
            ),
            opt("stitch", "mm", "Longest stitch 1-10 (default 4)"),
            opt(
                "angle",
                "degrees",
                "Row direction, counter-clockwise (default 0)",
            ),
            opt(
                "layers",
                "int 1-2",
                "1, or 2 crossed layers at angle and angle + 90 (default 1)",
            ),
            opt(
                "color",
                "color",
                "Thread (default: the first covered object's colour, so no colour change is added; choose the fabric colour for contrast designs)",
            ),
            opt(
                "shape",
                "outline|rectangle",
                "outline: the objects' outline grown by reach; rectangle: their bounding box grown by reach (default outline)",
            ),
            opt(
                "keepHoles",
                "bool",
                "Leave holes larger than the reach unsewn, such as a ring's centre (default true)",
            ),
        ],
        knockdown,
    )]
}

fn knockdown(s: &mut EngineState, p: &Params) -> CommandResult {
    if !p.has("update") {
        return build(s, p, None);
    }
    let id = p.int("update", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    let Some(Value::Object(saved)) = s.design.objects[i].knockdown.clone() else {
        return Err(p.invalid("update", "is not a knockdown fill"));
    };
    let mut merged: Map<String, Value> = saved;
    for (k, v) in p.values() {
        if k != "update" {
            merged.insert(k.clone(), v.clone());
        }
    }
    let merged = Params::new(&p.command, Some(&Value::Object(merged)))?;
    // The old knockdown and the other layer of its group go.
    let group = s.design.objects[i].group;
    s.design.objects.retain(|o| {
        !(o.id == id || (group.is_some() && o.group == group && o.knockdown.is_some()))
    });
    build(s, &merged, Some(id))
}

fn build(s: &mut EngineState, p: &Params, replaces: Option<u64>) -> CommandResult {
    let d = &s.design;
    let chosen = p.has("ids");
    let idx: Vec<usize> = if chosen {
        let mut v = crate::commands::indexes(p, d)?;
        v.sort_unstable();
        v.dedup();
        if v.iter().any(|&i| d.objects[i].knockdown.is_some()) {
            return Err(p.invalid("ids", "must not include a knockdown fill"));
        }
        v
    } else {
        (0..d.objects.len())
            .filter(|&i| d.objects[i].visible && d.objects[i].knockdown.is_none())
            .collect()
    };
    if idx.is_empty() {
        return Err(p.failed("there is nothing to put a knockdown under"));
    }
    let reach = p.opt("reach", 2.0, 0.0, 10.0)? * MM;
    let gap = p.opt("gapClose", 3.0, 0.0, 20.0)? * MM;
    let spacing = p.opt("spacing", 1.0, 0.5, 5.0)?;
    let stitch = p.opt("stitch", 4.0, 1.0, 10.0)?;
    let angle = p.opt("angle", 0.0, -360.0, 360.0)?;
    let layers = p.int("layers", Some(1), 1, 2)?;
    let rectangle = match p.string("shape", Some("outline"))?.as_str() {
        "outline" => false,
        "rectangle" => true,
        _ => return Err(p.invalid("shape", "must be outline or rectangle")),
    };
    let keep_holes = p.bool("keepHoles", Some(true))?;
    let colour = thread(p, "color")?.unwrap_or_else(|| pattern_colour(&d.objects[idx[0]]));
    let area = cover(d, &idx, reach, gap, rectangle, keep_holes);
    if area.is_empty() {
        return Err(p.failed("the objects have no area to put a knockdown under"));
    }
    let saved = json!({
        "ids": chosen.then(|| idx.iter().map(|&i| d.objects[i].id).collect::<Vec<_>>()),
        "reach": reach / MM,
        "gapClose": gap / MM,
        "spacing": spacing,
        "stitch": stitch,
        "angle": angle,
        "layers": layers,
        "color": colour.hex(),
        "shape": if rectangle { "rectangle" } else { "outline" },
        "keepHoles": keep_holes,
    });
    let stages: Vec<DesignObject> = (0..layers)
        .map(|k| {
            let mut r = RegionSewing::new(RegionType::Fill, colour.clone());
            r.angle = angle + 90.0 * k as f64;
            r.density = 1.0 / spacing;
            r.stitch_length = stitch;
            r.underlay = false;
            r.pull_compensation = 0.0;
            let mut o = DesignObject::shape(area.clone(), Some(r), None);
            o.name = Some(if k == 0 {
                NAME.into()
            } else {
                format!("{NAME} 2")
            });
            o.knockdown = Some(saved.clone());
            o
        })
        .collect();
    let at = if chosen { idx[0] } else { 0 };
    let ids = insert_stages(&mut s.design, at, stages);
    // Per connected piece: separate pieces are not each other's holes.
    let area_mm2: f64 = boolean::simplify(&area)
        .iter()
        .map(|r| boolean::region_area(r))
        .sum::<f64>()
        / (MM * MM);
    Ok(json!({
        "ids": ids,
        "replaces": replaces,
        "areaMM2": (area_mm2 * 10.0).round() / 10.0,
        "contours": area.len(),
    }))
}

/// The area under the objects at `idx`: their union grown by `reach`, with gaps narrower than
/// `gap` closed (grown by half of it, then shrunk back), holes kept when `keep_holes`; or
/// their bounding box grown by `reach`.
pub(crate) fn cover(
    d: &Design,
    idx: &[usize],
    reach: f64,
    gap: f64,
    rectangle: bool,
    keep_holes: bool,
) -> Vec<Contour> {
    let mut parts: Vec<Vec<Contour>> = Vec::new();
    for &i in idx {
        let o = &d.objects[i];
        let mut part: Vec<Contour> = Vec::new();
        if o.kind != ObjectKind::Stitches {
            part.extend(o.contours.iter().filter(|c| c.closed).cloned());
        }
        // Open lines and stitch patterns count as their bounding box.
        let loose: Vec<_> = if o.kind == ObjectKind::Stitches {
            o.points()
        } else {
            o.contours
                .iter()
                .filter(|c| !c.closed)
                .flat_map(Contour::outline_points)
                .collect()
        };
        if let Some(b) = Rect::enclosing(loose) {
            part.push(box_contour(b, 5.0));
        }
        if !part.is_empty() {
            parts.push(part);
        }
    }
    if rectangle {
        let b = Rect::enclosing(parts.iter().flatten().flat_map(Contour::outline_points));
        return b.map(|b| vec![box_contour(b, reach)]).unwrap_or_default();
    }
    let mut regions = boolean::union_all(parts.iter().map(|p| &p[..]));
    let grow = reach + gap / 2.0;
    if grow > 0.0 {
        regions = boolean::offset(&boolean::contours(&regions), grow, true);
    }
    if gap > 0.0 {
        regions = boolean::offset(&boolean::contours(&regions), -gap / 2.0, true);
    }
    if keep_holes {
        boolean::contours(&regions)
    } else {
        super::outer(&regions)
    }
}

fn box_contour(b: Rect, margin: f64) -> Contour {
    Contour::rectangle(
        b.min_x - margin,
        b.min_y - margin,
        b.width() + 2.0 * margin,
        b.height() + 2.0 * margin,
    )
}

/// Moves each knockdown in `order` (indexes into `d.objects`) just before the first object it
/// covers, so Optimize never sews it on top: before everything for one made for the whole
/// design.
pub(crate) fn keep_first(d: &Design, order: &mut Vec<usize>) {
    let knockdowns: Vec<usize> = order
        .iter()
        .copied()
        .filter(|&i| d.objects[i].knockdown.is_some())
        .collect();
    if knockdowns.is_empty() {
        return;
    }
    for k in knockdowns {
        let covered: Option<Vec<u64>> = d.objects[k]
            .knockdown
            .as_ref()
            .and_then(|v| v.get("ids"))
            .and_then(|v| serde_json::from_value(v.clone()).ok());
        order.retain(|&i| i != k);
        let at = match covered {
            Some(ids) => order
                .iter()
                .position(|&i| ids.contains(&d.objects[i].id))
                .unwrap_or(0),
            None => order
                .iter()
                .position(|&i| d.objects[i].knockdown.is_none())
                .unwrap_or(order.len()),
        };
        order.insert(at, k);
    }
}
