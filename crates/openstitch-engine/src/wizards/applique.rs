//! Appliqué wizard (PE-DESIGN Tutorial 9-1): from the selected outlines, the appliqué material
//! line (sewn or cut), a stop, the position line, a stop, the tack-down, a stop, then the
//! covering stitch. Each stage is a line object; the stops are colour changes between threads
//! named after the stages, in PE-DESIGN's appliqué colours: PEC 62 material, 63 position, 64
//! appliqué for the tack-down and the covering stitch, which so sew as one block (PE-DESIGN
//! 11.50's Applique Wizard files a01-a05: PEC colours 1, 62, 63, 64).

use super::{
    MAX_PIECES, closed_outlines, footprint, insert_stages, outer, outline_regions, selected, stage,
};
use crate::commands::{CommandSpec, Effect, MM, opt, req, spec, thread};
use crate::params::{CommandError, CommandResult, Params};
use crate::session::EngineState;
use openstitch_core::{PEC_CHART, Thread};
use openstitch_design::boolean;
use openstitch_design::{ChainShape, Contour, DesignObject, LineSewing, LineType};
use serde_json::json;

/// Thread names of the appliqué stages; each is its own colour block, so the machine stops
/// before the next stage.
pub const MATERIAL: &str = "Appliqué material";
pub const POSITION: &str = "Appliqué position";
pub const TACK_DOWN: &str = "Appliqué tack-down";
/// Thread name of the tack-down and, by default, the covering stitch: PE-DESIGN's "Applique".
pub const APPLIQUE: &str = "Appliqué";

/// PE-DESIGN's appliqué colour at PEC chart index `pec` (62–64), named `name`.
fn applique_colour(pec: usize, name: &'static str) -> Thread {
    let mut t = PEC_CHART[pec].clone().expect("PEC 62-64");
    t.name = Some(name.into());
    t
}

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "wizard.applique",
        "Appliqué wizard: material line, stop, position line, stop, tack-down, stop, covering stitch (one group)",
        Effect::Edit,
        vec![
            req(
                "ids",
                "[int]",
                "Closed shapes or text to make the appliqué from",
            ),
            opt(
                "material",
                "running|cutting|none",
                "Appliqué material: sewn cutting line, cut by cutwork needles, or none (default running)",
            ),
            opt(
                "tackDown",
                "zigzag|eStitch|vStitch|none",
                "Tack-down stitch (default zigzag)",
            ),
            opt(
                "tackDownWidth",
                "mm",
                "Tack-down zigzag width / E-V stroke 0.5-10 (default 2)",
            ),
            opt(
                "tackDownDensity",
                "lines/mm",
                "Tack-down zigzag density 0.2-5 (default 1)",
            ),
            opt(
                "tackDownInterval",
                "mm",
                "Tack-down E/V interval 0.5-10 (default 3)",
            ),
            opt(
                "cover",
                "zigzag|eStitch|vStitch|chain",
                "Covering stitch (default zigzag)",
            ),
            opt(
                "coverWidth",
                "mm",
                "Zigzag width, E/V stroke or chain size 0.5-10 (default 3)",
            ),
            opt(
                "coverDensity",
                "lines/mm",
                "Zigzag density 1-10 (default 5)",
            ),
            opt("coverInterval", "mm", "E/V interval 0.5-10 (default 3)"),
            opt("coverPitch", "mm", "E/V run pitch 1-10 (default 2.5)"),
            opt(
                "chainShape",
                "triangle|diamond",
                "Chain stitch type (default triangle)",
            ),
            opt("coverTimes", "int 1-5", "Chain / E-V passes (default 1)"),
            opt(
                "output",
                "add|replace",
                "Add the appliqué around the patterns, or sew it along their outlines and delete them (default add)",
            ),
            opt(
                "distance",
                "mm",
                "Add: distance from the patterns 0-50 (default 0)",
            ),
            opt(
                "holes",
                "bool",
                "Replace: keep the patterns' holes as empty areas (default true)",
            ),
            opt(
                "color",
                "color",
                "Covering thread (default PE-DESIGN's Applique colour, PEC 64, as the tack-down)",
            ),
        ],
        applique,
    )]
}

fn applique(s: &mut EngineState, p: &Params) -> CommandResult {
    let idx = selected(p, &s.design)?;
    let material = match p.string("material", Some("running"))?.as_str() {
        "running" => Some(false),
        "cutting" => Some(true),
        "none" => None,
        _ => return Err(p.invalid("material", "must be running, cutting or none")),
    };
    let replace = match p.string("output", Some("add"))?.as_str() {
        "add" => false,
        "replace" => true,
        _ => return Err(p.invalid("output", "must be add or replace")),
    };
    let distance = p.opt("distance", 0.0, 0.0, 50.0)? * MM;
    if replace && p.has("distance") {
        return Err(p.invalid("distance", "applies to output add only"));
    }
    let holes = p.bool("holes", Some(true))?;
    let tack = tack_down(p)?;
    let cover = cover(
        p,
        thread(p, "color")?.unwrap_or_else(|| applique_colour(64, APPLIQUE)),
    )?;

    // Where PE-DESIGN 11.50's Applique Wizard puts each line (a01-a05, a 20 mm square; w the
    // covering width): the covering stitch's centre on the outline (Replace) or w / 2 + 0.3 mm
    // beyond the distance (Add: its inner edge 0.3 mm past it), the position line w / 2 - 1 mm
    // outside the covering's centre, the material line and the tack-down 0.5 mm further out.
    // (a04, w = 4: PE's position and material lines are 0.1 mm inside these.)
    let w = cover.width;
    let keep_holes = replace && holes;
    let cover_line = if replace {
        closed_outlines(p, &s.design, &idx)?;
        let regions = outline_regions(&s.design, &idx);
        if holes {
            boolean::contours(&regions)
        } else {
            outer(&regions)
        }
    } else {
        grow(
            &footprint(&s.design, &idx),
            distance / MM + w / 2.0 + 0.3,
            false,
        )
    };
    let position_line = grow(&cover_line, w / 2.0 - 1.0, keep_holes);
    let outside_line = grow(&position_line, 0.5, keep_holes);
    for outline in [&cover_line, &position_line, &outside_line] {
        if outline.is_empty() {
            return Err(p.failed("the patterns have no outline to make an appliqué from"));
        }
        if outline.iter().map(|c| c.points.len()).sum::<usize>() > MAX_PIECES * 10 {
            return Err(p.failed("the outline is too complex"));
        }
    }

    let mut stages: Vec<DesignObject> = Vec::new();
    if let Some(cut) = material {
        let mut line = LineSewing::new(
            if cut {
                LineType::Cutting
            } else {
                LineType::Running
            },
            applique_colour(62, MATERIAL),
        );
        // Cut lines keep short bridges (cut by hand afterwards), as PE-DESIGN does.
        line.leave_uncut = cut;
        stages.push(stage(MATERIAL, outside_line.clone(), line));
    }
    let position = LineSewing::new(LineType::Running, applique_colour(63, POSITION));
    stages.push(stage(POSITION, position_line, position));
    if let Some(t) = tack {
        stages.push(stage(TACK_DOWN, outside_line, t));
    }
    stages.push(stage("Appliqué cover", cover_line, cover));

    let mut removed = Vec::new();
    let ids = if replace {
        let at = idx[0];
        removed = idx.iter().map(|&i| s.design.objects[i].id).collect();
        crate::shape_ops::remove(&mut s.design, &idx);
        insert_stages(&mut s.design, at, stages)
    } else {
        // Add: the patterns sew first and form one group with their appliqué, as in
        // PE-DESIGN's a01-a04 (Sewing Order: the square, then the appliqué; one group of all).
        let ids = insert_stages(&mut s.design, idx[idx.len() - 1] + 1, stages);
        let group = s
            .design
            .objects
            .iter()
            .find(|o| o.id == ids[0])
            .and_then(|o| o.group);
        for &i in &idx {
            s.design.objects[i].group = group;
        }
        ids
    };
    Ok(json!({ "ids": ids, "removed": removed }))
}

/// `contours` grown by `mm` (shrunk when negative) with round corners; holes kept when `holes`.
/// The round corners' vertices are thinned to [`MIN_VERTEX_GAP_MM`]: running stitch keeps every
/// vertex, and a sharp tip grown by 0.5 mm (a heart's point) became an arc of 0.1 mm steps, 15
/// stitches under 0.3 mm in a row on the machine (#450).
fn grow(contours: &[Contour], mm: f64, holes: bool) -> Vec<Contour> {
    if mm == 0.0 {
        return contours.to_vec();
    }
    let regions = boolean::offset(contours, mm * MM, true);
    let grown = if holes {
        boolean::contours(&regions)
    } else {
        outer(&regions)
    };
    grown.into_iter().map(thinned).collect()
}

/// Shortest distance between consecutive vertices of a grown outline (a straight edge's
/// vertices are far apart; only the round corners' arcs are thinned).
const MIN_VERTEX_GAP_MM: f64 = 0.4;

/// `c` without the vertices closer than [`MIN_VERTEX_GAP_MM`] to the last one kept. Only vertices
/// between two straight edges go (a curve's ends stay, and so does the first vertex); the edges
/// they joined become one straight edge.
fn thinned(c: Contour) -> Contour {
    let n = c.points.len();
    if n < 4 {
        return c;
    }
    let curve = |i: usize| c.curves.get(i).copied().flatten();
    let gap = MIN_VERTEX_GAP_MM * MM;
    let mut points = Vec::with_capacity(n);
    let mut curves = Vec::with_capacity(n);
    for i in 0..n {
        let straight_both = i > 0 && curve(i - 1).is_none() && curve(i).is_none();
        let closing_gap = c.closed && i == n - 1 && c.points[i].distance(c.points[0]) < gap;
        if straight_both
            && (closing_gap
                || points
                    .last()
                    .is_some_and(|&q: &openstitch_design::Vec2| q.distance(c.points[i]) < gap))
        {
            continue;
        }
        points.push(c.points[i]);
        curves.push(curve(i));
    }
    if points.len() < 3 {
        return c;
    }
    // A dropped vertex sat between straight edges, so the edge from the kept vertex before it
    // is straight: `curves[k]` (the original curve leaving that kept vertex) is None then.
    let mut out = Contour::new(points, c.closed);
    if curves.iter().any(Option::is_some) {
        out.curves = curves;
    }
    out
}

fn tack_down(p: &Params) -> Result<Option<LineSewing>, CommandError> {
    let kind = match p.string("tackDown", Some("zigzag"))?.as_str() {
        "zigzag" => LineType::Zigzag,
        "eStitch" => LineType::EStitch,
        "vStitch" => LineType::VStitch,
        "none" => return Ok(None),
        _ => {
            return Err(p.invalid("tackDown", "must be zigzag, eStitch, vStitch or none"));
        }
    };
    let mut l = LineSewing::new(kind, applique_colour(64, APPLIQUE));
    l.width = p.opt("tackDownWidth", 2.0, 0.5, 10.0)?;
    l.density = p.opt("tackDownDensity", 1.0, 0.2, 5.0)?;
    l.spacing = Some(p.opt("tackDownInterval", 3.0, 0.5, 10.0)?);
    Ok(Some(l))
}

fn cover(p: &Params, color: Thread) -> Result<LineSewing, CommandError> {
    let kind = match p.string("cover", Some("zigzag"))?.as_str() {
        "zigzag" => LineType::Zigzag,
        "eStitch" => LineType::EStitch,
        "vStitch" => LineType::VStitch,
        "chain" => LineType::Chain,
        _ => return Err(p.invalid("cover", "must be zigzag, eStitch, vStitch or chain")),
    };
    let mut l = LineSewing::new(kind, color);
    l.width = p.opt("coverWidth", 3.0, 0.5, 10.0)?;
    l.density = p.opt("coverDensity", 5.0, 1.0, 10.0)?;
    l.spacing = Some(p.opt("coverInterval", 3.0, 0.5, 10.0)?);
    l.pitch = p.opt("coverPitch", 2.5, 1.0, 10.0)?;
    l.run_times = p.int("coverTimes", Some(1), 1, 5)? as u32;
    l.chain_shape = match p.string("chainShape", Some("triangle"))?.as_str() {
        "triangle" => ChainShape::Triangle,
        "diamond" => ChainShape::Diamond,
        _ => return Err(p.invalid("chainShape", "must be triangle or diamond")),
    };
    Ok(l)
}
