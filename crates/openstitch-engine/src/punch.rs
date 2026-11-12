//! Manual Punch: shapes made of several sewing-direction lines (PE-DESIGN's "Using Manual
//! Punch Tool" and "Specifying the straight sewing direction" with more than one line), and
//! satin columns along a skeleton (centre line with per-point width). Curved Block columns,
//! Feed patterns and Reverse Direction Line.

use crate::commands::{
    CommandSpec, Effect, IDS, default_fill, default_line, indexes, opt, req, spec, thread,
};
use crate::params::{CommandError, Params};
use openstitch_core::{Command, Stitch};
use openstitch_design::column::{self, RungError};
use openstitch_design::skeleton::{self, Skeleton, SkeletonError};
use openstitch_design::{
    DesignObject, LineSewing, LineType, RegionSewing, RegionType, StitchBlock, Vec2,
};
use serde_json::json;

/// Most direction lines per shape.
pub const MAX_LINES: usize = 1_000;

fn rungs(p: &Params) -> Result<Vec<[Vec2; 2]>, CommandError> {
    let arr = p.array("lines")?;
    if arr.len() < 2 || arr.len() > MAX_LINES {
        return Err(p.invalid("lines", format!("needs 2...{MAX_LINES} lines")));
    }
    let lines = arr
        .iter()
        .map(|v| match v.as_array() {
            Some(a) if a.len() == 2 => Ok([
                p.point_value("lines", &a[0])?,
                p.point_value("lines", &a[1])?,
            ]),
            _ => Err(p.invalid("lines", "each line must be [[x1, y1], [x2, y2]] in mm")),
        })
        .collect::<Result<Vec<_>, _>>()?;
    // A pair clicked right-then-left is turned round (#525); a real overlap still fails.
    let (lines, _) = column::untangle(&lines);
    column::validate(&lines).map_err(|e| rung_error(p, e))?;
    Ok(lines)
}

/// The Manual Punch column `id`'s direction lines changed by `edit`, checked, and its outline
/// redrawn from them (#525). `edit` returns the command's result.
fn edit_rungs(
    s: &mut crate::session::EngineState,
    p: &Params,
    edit: impl FnOnce(&mut Vec<[Vec2; 2]>) -> Result<serde_json::Value, CommandError>,
) -> Result<serde_json::Value, CommandError> {
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    let o = &mut s.design.objects[i];
    let r = o
        .region
        .as_mut()
        .filter(|r| r.direction_lines.len() >= 2 && o.contours.len() == 1)
        .ok_or_else(|| p.failed(format!("object {id} is not a Manual Punch column")))?;
    let mut lines = r.direction_lines.clone();
    let out = edit(&mut lines)?;
    column::validate(&lines).map_err(|e| rung_error(p, e))?;
    if r.curved_lines && column::validate(&column::curved_rungs(&lines)).is_err() {
        return Err(p.failed("the curved sides would cross"));
    }
    o.contours[0] = if r.curved_lines {
        column::curved_outline(&lines)
    } else {
        column::outline(&lines)
    };
    r.direction_lines = lines;
    Ok(out)
}

fn rung_index(p: &Params, lines: &[[Vec2; 2]]) -> Result<usize, CommandError> {
    Ok(p.int("index", None, 0, lines.len() as i64 - 1)? as usize)
}

/// The user-facing message for direction lines that cannot make a column.
pub(crate) fn rung_error(p: &Params, e: RungError) -> CommandError {
    match e {
        RungError::TooFew => p.invalid("lines", "needs at least 2 lines"),
        RungError::Degenerate(i) => p.invalid("lines", format!("line {i} is too short")),
        RungError::Crossing(i) => p.invalid(
            "lines",
            format!("lines {i} and {} (or the sides between them) cross", i + 1),
        ),
    }
}

/// `points` (mm) with `widths` (mm, one per point) or one `width` (default 3 mm).
fn skeleton_param(p: &Params) -> Result<Skeleton, CommandError> {
    let points = p.points("points", 2)?;
    if points.len() > skeleton::MAX_POINTS {
        return Err(p.invalid("points", format!("at most {} points", skeleton::MAX_POINTS)));
    }
    if p.has("widths") && p.has("width") {
        return Err(p.invalid("widths", "give widths or width, not both"));
    }
    let widths = if p.has("widths") {
        p.array("widths")?
            .iter()
            .map(|v| {
                v.as_f64()
                    .ok_or_else(|| p.invalid("widths", "must be numbers (mm)"))
            })
            .collect::<Result<Vec<_>, _>>()?
    } else {
        let w = p.opt("width", skeleton::DEFAULT_WIDTH, 0.1, skeleton::MAX_WIDTH)?;
        vec![w; points.len()]
    };
    let s = Skeleton { points, widths };
    s.validate().map_err(|e| match e {
        SkeletonError::TooFew => p.invalid("points", "needs at least 2 points"),
        SkeletonError::TooMany => {
            p.invalid("points", format!("at most {} points", skeleton::MAX_POINTS))
        }
        SkeletonError::WidthCount => p.invalid("widths", "needs one width per point"),
        SkeletonError::Width(i) => p.invalid(
            "widths",
            format!("width {i} must be 0...{} mm", skeleton::MAX_WIDTH),
        ),
        SkeletonError::Thin => p.invalid("widths", "at least one width must be 0.1 mm or more"),
        SkeletonError::Short => p.invalid("points", "the centre line is too short"),
    })?;
    Ok(s)
}

/// The open curve `c` from its other end.
fn reversed(c: &openstitch_design::Contour) -> openstitch_design::Contour {
    let mut points = c.points.clone();
    points.reverse();
    let mut out = openstitch_design::Contour::new(points, c.closed);
    if c.has_curves() {
        let n = c.segment_count();
        out.curves = (0..n)
            .rev()
            .map(|i| {
                c.curve(i)
                    .map(|z| openstitch_design::Bezier { c1: z.c2, c2: z.c1 })
            })
            .collect();
    }
    out
}

fn kind(p: &Params) -> Result<RegionType, CommandError> {
    match p.string("type", Some("satin"))?.as_str() {
        "satin" => Ok(RegionType::Satin),
        "fill" => Ok(RegionType::Fill),
        _ => Err(p.invalid("type", "must be satin or fill")),
    }
}

/// Widest satin column (mm) a branch split makes without a warning.
const BRANCH_WARN_WIDTH: f64 = 10.0;

/// A branch column's ends this near the shape's outline (0.1 mm) lie on it: [`skeleton::branches`]
/// runs a free end on to 0.1 mm inside it (#526).
const ON_OUTLINE: f64 = 2.0;
/// How much wider than its skeleton (mm, both sides together) a branch column's outline is
/// taken before the shape cuts it: its rungs reach on to the shape's edge (up to 1.5 mm past
/// the skeleton's half width, [`skeleton::satin_to_outline`]) where the derived width falls
/// short or the centre line lies off the stroke's middle.
const BRANCH_REACH: f64 = 3.0;

/// A branch column's outline and which of its ends lie on the shape's outline (#526). Its
/// outline is the part of the shape it covers: its rails, a little wider, run on past an end
/// on the outline far enough to take in that end edge's corners, cut by the shape, so its satin
/// is sewn up to the shape's edges ([`skeleton::satin_to_outline`]) and never past them.
fn branch_outline(
    sk: &Skeleton,
    shape: &[openstitch_design::Contour],
    edges: &[(Vec2, Vec2)],
) -> Option<(Vec<openstitch_design::Contour>, Option<[bool; 2]>)> {
    let rails = skeleton::outline(sk)?;
    let near = |p: Vec2| {
        edges.iter().any(|&(a, b)| {
            let d = b - a;
            let len2 = d.x * d.x + d.y * d.y;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            p.distance(a + d * t) <= ON_OUTLINE
        })
    };
    let n = sk.points.len();
    if n < 2 {
        return Some((vec![rails], None));
    }
    let ends = [near(sk.points[0]), near(sk.points[n - 1])];
    // The rails, a little wider, run on past each end on the outline by twice the column's
    // widest.
    let reach = 20.0 * sk.widths.iter().copied().fold(0.0, f64::max);
    let mut long = sk.clone();
    for w in &mut long.widths {
        *w += BRANCH_REACH;
    }
    let n_long = long.widths.len();
    let on = |a: Vec2, b: Vec2| {
        let d = a - b;
        let l = d.length();
        (l > 1e-9).then(|| a + d * (reach / l))
    };
    if ends[1]
        && let Some(p) = on(sk.points[n - 1], sk.points[n - 2])
    {
        long.points.push(p);
        long.widths.push(long.widths[n_long - 1]);
    }
    if ends[0]
        && let Some(p) = on(sk.points[0], sk.points[1])
    {
        long.points.insert(0, p);
        long.widths.insert(0, long.widths[0]);
    }
    let wide = skeleton::outline(&long).unwrap_or_else(|| rails.clone());
    // Every piece of it: a column crossing a notch (a star's) is cut into several.
    let cut: Vec<openstitch_design::Contour> = openstitch_design::boolean::combine(
        std::slice::from_ref(&wide),
        shape,
        openstitch_design::boolean::BoolOp::Intersection,
    )
    .into_iter()
    .filter_map(|r| r.into_iter().next())
    .collect();
    Some(if cut.is_empty() {
        (vec![rails], None)
    } else {
        (cut, Some(ends))
    })
}

fn derive_branches(
    s: &mut crate::session::EngineState,
    p: &Params,
) -> Result<Option<serde_json::Value>, CommandError> {
    let overlap = p.opt("joinOverlap", 0.3, 0.0, 1.0)?;
    let keep = p.bool("keepOriginal", Some(true))?;
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    let o = &s.design.objects[i];
    if o.kind != openstitch_design::ObjectKind::Shape || o.hole_sewing {
        return Ok(None);
    }
    let Some(template) = o.region.clone() else {
        return Ok(None);
    };
    let arms = skeleton::branches(&o.contours, overlap);
    if arms.len() < 2 {
        return Ok(None);
    }
    let name = o.name.clone().unwrap_or_else(|| "Shape".into());
    let shape = o.contours.clone();
    let edges: Vec<(Vec2, Vec2)> = shape
        .iter()
        .filter(|c| c.closed)
        .flat_map(openstitch_design::Contour::edges)
        .collect();
    let group = s.design.new_group_id();
    let mut objects = Vec::new();
    let mut warnings = Vec::new();
    for (n, sk) in arms.into_iter().enumerate() {
        let Some((outline, ends)) = branch_outline(&sk, &shape, &edges) else {
            continue;
        };
        let widest = sk.widths.iter().copied().fold(0.0, f64::max);
        if widest > BRANCH_WARN_WIDTH {
            warnings.push(format!(
                "column {} is {widest:.1} mm wide: satin over {BRANCH_WARN_WIDTH} mm sews loose; consider fill",
                n + 1
            ));
        }
        let mut region = template.clone();
        region.kind = RegionType::Satin;
        region.direction_lines.clear();
        region.curved_lines = false;
        region.entry = None;
        region.exit = None;
        region.skeleton = Some(sk);
        region.outline_ends = ends;
        let mut obj = DesignObject::shape(outline, Some(region), None);
        obj.name = Some(format!("{name} {}", n + 1));
        obj.group = Some(group);
        objects.push(obj);
    }
    // The columns sew where the shape sewed: inserted right after it.
    let mut ids = Vec::new();
    for (k, mut obj) in objects.into_iter().enumerate() {
        obj.id = s.design.new_group_id();
        ids.push(obj.id);
        s.design.objects.insert(i + 1 + k, obj);
    }
    if keep {
        s.design.objects[i].visible = false;
    } else {
        s.design.objects.remove(i);
    }
    Ok(Some(
        json!({ "ids": ids, "group": group, "warnings": warnings }),
    ))
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "shape.column",
            "Add a Manual Punch shape: a column through direction lines, sewn along them",
            Effect::Edit,
            vec![
                req(
                    "lines",
                    "[[[x1, y1], [x2, y2]]] mm",
                    "Direction lines in order (2-1000); first ends make one side, second ends the other",
                ),
                opt("type", "satin|fill", "Region sew type (default satin)"),
                opt(
                    "curved",
                    "bool",
                    "Curved Block: the sides curve through the line ends (default false, Straight Block)",
                ),
                opt("fill", "color", "Region thread (default Brother Blue)"),
                opt(
                    "outline",
                    "color",
                    "Outline running-stitch thread (default none)",
                ),
                opt("name", "string", "Object name"),
            ],
            |s, p| {
                let lines = rungs(p)?;
                let curved = p.bool("curved", Some(false))?;
                if curved && column::validate(&column::curved_rungs(&lines)).is_err() {
                    return Err(p.invalid("lines", "the curved sides cross"));
                }
                let mut region =
                    RegionSewing::new(kind(p)?, thread(p, "fill")?.unwrap_or_else(default_fill));
                region.direction_lines = lines.clone();
                region.curved_lines = curved;
                let line = thread(p, "outline")?.map(|t| LineSewing::new(LineType::Running, t));
                let outline = if curved {
                    column::curved_outline(&lines)
                } else {
                    column::outline(&lines)
                };
                let mut o = DesignObject::shape(vec![outline], Some(region), line);
                o.name = Some(if p.has("name") {
                    p.string("name", None)?
                } else if curved {
                    "Curved block".into()
                } else {
                    "Manual punch".into()
                });
                Ok(json!({ "id": s.design.add(o) }))
            },
        ),
        spec(
            "object.setDirectionLines",
            "Sew a satin or fill region along several direction lines (null = back to one direction)",
            Effect::Edit,
            vec![
                req("id", "int", "Object with satin or fill region sewing"),
                req(
                    "lines",
                    "[[[x1, y1], [x2, y2]]] mm | null",
                    "Direction lines in order",
                ),
            ],
            |s, p| {
                let lines = if p.is_null("lines") {
                    Vec::new()
                } else {
                    rungs(p)?
                };
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let o = &mut s.design.objects[i];
                if o.hole_sewing && !lines.is_empty() {
                    return Err(
                        p.failed("direction lines cannot be added to a shape with hole sewing")
                    );
                }
                let r = o
                    .region
                    .as_mut()
                    .ok_or_else(|| p.failed(format!("object {id} has no region sewing")))?;
                if !lines.is_empty() && !matches!(r.kind, RegionType::Satin | RegionType::Fill) {
                    return Err(p.failed(format!(
                        "object {id}: direction lines work with satin and fill"
                    )));
                }
                if lines.is_empty() {
                    r.curved_lines = false;
                } else if r.curved_lines && column::validate(&column::curved_rungs(&lines)).is_err()
                {
                    return Err(p.invalid("lines", "the curved sides cross"));
                }
                r.direction_lines = lines;
                Ok(json!(null))
            },
        ),
        spec(
            "shape.feed",
            "Add a Manual Punch feed: jump stitches through the points, nothing sewn (moves the frame without cutting the thread)",
            Effect::Edit,
            vec![
                req("points", "[[x, y]] mm", "Points in order (2-1000)"),
                opt("name", "string", "Object name"),
            ],
            |s, p| {
                let points = p.points("points", 2)?;
                if points.len() > MAX_LINES {
                    return Err(p.invalid("points", format!("at most {MAX_LINES} points")));
                }
                if points.windows(2).all(|w| w[0] == w[1]) {
                    return Err(p.invalid("points", "needs two different points"));
                }
                let block = StitchBlock {
                    stitches: points
                        .iter()
                        .map(|q| Stitch::new(q.x, q.y, Command::Jump))
                        .collect(),
                    threads: vec![default_line()],
                };
                let mut o = DesignObject::stitches(block);
                o.name = Some(if p.has("name") {
                    p.string("name", None)?
                } else {
                    "Feed".into()
                });
                Ok(json!({ "id": s.design.add(o) }))
            },
        ),
        spec(
            "object.reverseDirection",
            "Reverse Direction Line: sew the region's sewing direction the other way (every direction line, the curved direction, or the angle turned 180°)",
            Effect::Edit,
            vec![IDS],
            |s, p| {
                let idx = indexes(p, &s.design)?;
                if let Some(&i) = idx.iter().find(|&&i| s.design.objects[i].region.is_none()) {
                    let id = s.design.objects[i].id;
                    return Err(p.failed(format!("object {id} has no region sewing")));
                }
                for i in idx {
                    let r = s.design.objects[i].region.as_mut().expect("checked");
                    for l in &mut r.direction_lines {
                        l.swap(0, 1);
                    }
                    if let Some(c) = &mut r.direction_curve {
                        *c = reversed(c);
                    }
                    r.angle = (r.angle + 180.0).rem_euclid(360.0);
                }
                Ok(json!(null))
            },
        ),
        spec(
            "shape.skeletonSatin",
            "Add a satin column along a centre line (skeleton): stitches stay perpendicular to the line, width given per point",
            Effect::Edit,
            vec![
                req("points", "[[x, y]] mm", "Centre line (2-10000 points)"),
                opt("widths", "[mm]", "Column width at each point (0-30)"),
                opt("width", "mm", "One width for every point (default 3)"),
                opt("density", "lines/mm", "Satin density (default 3)"),
                opt("fill", "color", "Satin thread (default Brother Blue)"),
                opt(
                    "outline",
                    "color",
                    "Outline running-stitch thread (default none)",
                ),
                opt("name", "string", "Object name"),
            ],
            |s, p| {
                let sk = skeleton_param(p)?;
                let outline = skeleton::outline(&sk)
                    .ok_or_else(|| p.invalid("points", "the centre line is too short"))?;
                let mut region = RegionSewing::new(
                    RegionType::Satin,
                    thread(p, "fill")?.unwrap_or_else(default_fill),
                );
                let (lo, hi) = region.kind.density_range();
                region.density = p.opt("density", region.density, lo, hi)?;
                region.skeleton = Some(sk);
                let line = thread(p, "outline")?.map(|t| LineSewing::new(LineType::Running, t));
                let mut o = DesignObject::shape(vec![outline], Some(region), line);
                o.name = Some(if p.has("name") {
                    p.string("name", None)?
                } else {
                    "Skeleton satin".into()
                });
                Ok(json!({ "id": s.design.add(o) }))
            },
        ),
        spec(
            "object.setSkeleton",
            "Sew a satin region along a centre line with per-point width (points null = back to normal satin)",
            Effect::Edit,
            vec![
                req("id", "int", "Object with satin region sewing"),
                req("points", "[[x, y]] mm | null", "Centre line"),
                opt("widths", "[mm]", "Column width at each point (0-30)"),
                opt("width", "mm", "One width for every point (default 3)"),
                opt(
                    "reshape",
                    "bool",
                    "Replace the outline with the column's (default false)",
                ),
            ],
            |s, p| {
                let sk = if p.is_null("points") {
                    None
                } else {
                    Some(skeleton_param(p)?)
                };
                let reshape = p.bool("reshape", Some(false))?;
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let o = &mut s.design.objects[i];
                if o.hole_sewing && sk.is_some() {
                    return Err(p.failed("a skeleton cannot be added to a shape with hole sewing"));
                }
                let r = o
                    .region
                    .as_mut()
                    .ok_or_else(|| p.failed(format!("object {id} has no region sewing")))?;
                if sk.is_some() && r.kind != RegionType::Satin {
                    return Err(p.failed(format!("object {id}: a skeleton works with satin")));
                }
                let outline = match (&sk, reshape) {
                    (Some(sk), true) => Some(
                        skeleton::outline(sk)
                            .ok_or_else(|| p.invalid("points", "the centre line is too short"))?,
                    ),
                    _ => None,
                };
                r.skeleton = sk;
                if let Some(c) = outline {
                    o.contours = vec![c];
                }
                Ok(json!(null))
            },
        ),
        spec(
            "object.deriveSkeleton",
            "Derive a satin skeleton from the shape's closed outline (an approximation of its medial axis): the region becomes satin sewn along it",
            Effect::Edit,
            vec![
                req("id", "int", "Shape object with region sewing"),
                opt(
                    "reshape",
                    "bool",
                    "Replace the outline with the column's (default false)",
                ),
                opt(
                    "branches",
                    "bool",
                    "Split branches (beyond PE-DESIGN): a branching shape (Y, star, K, stick figure) becomes one satin column along its main stroke and one per further arm, each a new skeleton satin in one group after the shape; a shape without branches is sewn as without (default false)",
                ),
                opt(
                    "joinOverlap",
                    "mm",
                    "With branches: how far each arm's column runs into the column it leaves, 0-1 (default 0.3)",
                ),
                opt(
                    "keepOriginal",
                    "bool",
                    "With branches: keep the shape, hidden, under the columns (default true); false removes it",
                ),
            ],
            |s, p| {
                if p.bool("branches", Some(false))?
                    && let Some(out) = derive_branches(s, p)?
                {
                    return Ok(out);
                }
                let reshape = p.bool("reshape", Some(false))?;
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let o = &mut s.design.objects[i];
                if o.kind != openstitch_design::ObjectKind::Shape {
                    return Err(p.failed(format!("object {id} is not a shape")));
                }
                if o.hole_sewing {
                    return Err(p.failed("a skeleton cannot be added to a shape with hole sewing"));
                }
                if o.region.is_none() {
                    return Err(p.failed(format!("object {id} has no region sewing")));
                }
                let sk = skeleton::derive(&o.contours).ok_or_else(|| {
                    p.failed(format!(
                        "object {id}: no centre line found (needs a closed outline wider than 0.2 mm)"
                    ))
                })?;
                let outline = if reshape {
                    Some(
                        skeleton::outline(&sk)
                            .ok_or_else(|| p.failed("the derived centre line is too short"))?,
                    )
                } else {
                    None
                };
                let result = json!({
                    "points": sk.points.iter().map(|q| [q.x / 10.0, q.y / 10.0]).collect::<Vec<_>>(),
                    "widths": sk.widths,
                });
                let r = o.region.as_mut().expect("checked");
                r.kind = RegionType::Satin;
                r.skeleton = Some(sk);
                if let Some(c) = outline {
                    o.contours = vec![c];
                }
                Ok(result)
            },
        ),
        spec(
            "object.insertRung",
            "Manual Punch column: add a direction line (rung) through the point, between the two it lies between, at the same slant (beyond PE-DESIGN)",
            Effect::Edit,
            vec![
                req("id", "int", "Manual Punch column"),
                req("at", "[x, y] mm", "Point on or near the column"),
            ],
            |s, p| {
                let at = p.point("at")?;
                edit_rungs(s, p, |lines| {
                    // The section nearest the point, and where along it the point lies.
                    let mut best = (f64::INFINITY, 0, 0.0);
                    for (j, w) in lines.windows(2).enumerate() {
                        let (a, b) = (w[0][0].lerp(w[0][1], 0.5), w[1][0].lerp(w[1][1], 0.5));
                        let ab = b - a;
                        let len2 = (ab.x * ab.x + ab.y * ab.y).max(1e-12);
                        let t =
                            (((at.x - a.x) * ab.x + (at.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
                        let d = a.lerp(b, t).distance(at);
                        if d < best.0 {
                            best = (d, j, t);
                        }
                    }
                    let (_, j, t) = best;
                    if !(0.01..=0.99).contains(&t) {
                        return Err(p.invalid("at", "is at a direction line already"));
                    }
                    let rung = [
                        lines[j][0].lerp(lines[j + 1][0], t),
                        lines[j][1].lerp(lines[j + 1][1], t),
                    ];
                    lines.insert(j + 1, rung);
                    Ok(json!({ "index": j + 1 }))
                })
            },
        ),
        spec(
            "object.deleteRung",
            "Manual Punch column: remove direction line (rung) `index` (0 = first); at least two stay (beyond PE-DESIGN)",
            Effect::Edit,
            vec![
                req("id", "int", "Manual Punch column"),
                req("index", "int", "Direction line, 0 = first"),
            ],
            |s, p| {
                edit_rungs(s, p, |lines| {
                    let i = rung_index(p, lines)?;
                    if lines.len() <= 2 {
                        return Err(p.failed("a column needs at least two direction lines"));
                    }
                    lines.remove(i);
                    Ok(json!(null))
                })
            },
        ),
        spec(
            "object.moveRung",
            "Manual Punch column: move one end of direction line (rung) `index` (beyond PE-DESIGN); refused when the column would cross itself",
            Effect::Edit,
            vec![
                req("id", "int", "Manual Punch column"),
                req("index", "int", "Direction line, 0 = first"),
                req(
                    "side",
                    "left|right",
                    "Which end: left is the end on the first stitch's start rail",
                ),
                req("point", "[x, y] mm", "New position of that end"),
            ],
            |s, p| {
                let side = match p.string("side", None)?.as_str() {
                    "left" => 0,
                    "right" => 1,
                    _ => return Err(p.invalid("side", "must be left or right")),
                };
                let to = p.point("point")?;
                edit_rungs(s, p, |lines| {
                    let i = rung_index(p, lines)?;
                    lines[i][side] = to;
                    Ok(json!(null))
                })
            },
        ),
        spec(
            "object.swapSides",
            "Manual Punch column: swap its two sides, so every satin crossing starts from the other rail; the needle points stay (beyond PE-DESIGN)",
            Effect::Edit,
            vec![req("id", "int", "Manual Punch column")],
            |s, p| {
                edit_rungs(s, p, |lines| {
                    *lines = column::swap_sides(lines);
                    Ok(json!(null))
                })
            },
        ),
        spec(
            "object.untangle",
            "Manual Punch column: turn round every direction line whose ends were swapped (sides crossing between it and the one before), e.g. after moving a point; a real overlap is still refused (beyond PE-DESIGN). Returns how many were turned",
            Effect::Edit,
            vec![req("id", "int", "Manual Punch column")],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let lines = s.design.objects[i]
                    .region
                    .as_ref()
                    .map(|r| r.direction_lines.clone())
                    .unwrap_or_default();
                let (fixed, turned) = column::untangle(&lines);
                if let Err(e) = column::validate(&fixed) {
                    let RungError::Crossing(k) = e else {
                        return Err(p.failed(format!("object {id} is not a Manual Punch column")));
                    };
                    return Err(p.failed(format!(
                        "direction lines {k} and {} overlap: move one of them",
                        k + 1
                    )));
                }
                edit_rungs(s, p, |l| {
                    *l = fixed;
                    Ok(json!({ "turned": turned }))
                })
            },
        ),
    ]
}
