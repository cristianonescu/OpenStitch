//! Colour blends and gradient fills (#532, beyond PE-DESIGN): `shape.gradient` and
//! `shape.blend` turn a filled shape into one fill object per colour stop, grouped and next
//! to each other in the sewing order; together they sew the shape as one even layer (see
//! `openstitch_design::gradient`).

use crate::commands::{CommandSpec, Effect, MM, choice, opt, req, spec, thread};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_core::Thread;
use openstitch_design::{
    DesignObject, Gradient, GradientCurve, GradientDither, GradientKind, RegionSewing, RegionType,
    Vec2, v2,
};
use serde_json::{Value, json};

/// Most colours of one gradient.
pub const MAX_STOPS: usize = 8;

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "shape.gradient",
            "Gradient fill (beyond PE-DESIGN): the shape becomes one fill object per colour, grouped and sewn one after another, each sewing its share of one shared set of rows so together they are one even layer",
            Effect::Edit,
            vec![
                req(
                    "id",
                    "int",
                    "A closed shape (its fill settings are kept; it becomes a fill)",
                ),
                req(
                    "threads",
                    "[color]",
                    "2-8 colours, from the gradient's start to its end",
                ),
                opt(
                    "stops",
                    "[0-1]",
                    "Where each colour is pure, ascending (default evenly spaced from 0 to 1)",
                ),
                opt(
                    "kind",
                    "linear|radial",
                    "Linear (default) along the direction, or radial from the centre outwards",
                ),
                opt(
                    "angle",
                    "degrees",
                    "Linear: direction the gradient runs, counter-clockwise from left to right (default 0); it spans the shape",
                ),
                opt(
                    "from",
                    "[x, y] mm",
                    "Where the gradient starts (radial: its centre); with `to`, instead of the angle",
                ),
                opt(
                    "to",
                    "[x, y] mm",
                    "Where it ends (radial: a point on its outer circle)",
                ),
                opt(
                    "dither",
                    "rows|ordered|noise|bands",
                    "How stitches are shared where colours meet: rows (default: whole rows, complementary spacing, fewest jumps), ordered (an even woven pattern), noise (seeded random), bands (hard edges)",
                ),
                opt(
                    "curve",
                    "linear|easeIn|easeOut|sCurve",
                    "Transition between stops (default linear)",
                ),
                opt("seed", "integer", "Noise dither seed (default 1)"),
            ],
            |s, p| {
                let threads = threads(p)?;
                gradient(s, p, threads, GradientDither::Rows)
            },
        ),
        spec(
            "shape.blend",
            "Two-colour blend (beyond PE-DESIGN): a fill fades from its own thread into thread2; rows of the two colours have complementary spacing, so the coverage stays one layer (shape.gradient with two colours)",
            Effect::Edit,
            vec![
                req("id", "int", "A filled shape"),
                req("thread2", "color", "The colour it blends into"),
                opt(
                    "mix",
                    "[a, b]",
                    "Where the first colour stops being solid and where the second becomes solid, 0-1 along the blend (default [0, 1])",
                ),
                opt("kind", "linear|radial", "As shape.gradient"),
                opt("angle", "degrees", "As shape.gradient"),
                opt("from", "[x, y] mm", "As shape.gradient"),
                opt("to", "[x, y] mm", "As shape.gradient"),
                opt(
                    "dither",
                    "rows|ordered|noise|bands",
                    "As shape.gradient (default rows)",
                ),
                opt("curve", "linear|easeIn|easeOut|sCurve", "As shape.gradient"),
                opt("seed", "integer", "As shape.gradient"),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let first = s.design.objects[i]
                    .region
                    .as_ref()
                    .map(|r| r.thread.clone())
                    .ok_or_else(|| p.failed(format!("object {id} has no region to blend")))?;
                let second =
                    thread(p, "thread2")?.ok_or_else(|| p.invalid("thread2", "is required"))?;
                gradient(s, p, vec![first, second], GradientDither::Rows)
            },
        ),
    ]
}

/// The `threads` list.
fn threads(p: &Params) -> Result<Vec<Thread>, CommandError> {
    let list = p
        .get("threads")
        .and_then(Value::as_array)
        .ok_or_else(|| p.invalid("threads", "must be a list of 2-8 colours"))?;
    if !(2..=MAX_STOPS).contains(&list.len()) {
        return Err(p.invalid("threads", "must be a list of 2-8 colours"));
    }
    list.iter()
        .enumerate()
        .map(|(k, v)| {
            let one = Params::new(&p.command, Some(&json!({ "thread": v })))?;
            thread(&one, "thread").ok().flatten().ok_or_else(|| {
                p.invalid(
                    "threads",
                    format!("item {k} must be #rrggbb, a Brother thread name or chart number 1-64"),
                )
            })
        })
        .collect()
}

/// A point `[x, y]` in mm, as 0.1 mm.
fn point(p: &Params, key: &str) -> Result<Option<Vec2>, CommandError> {
    let Some(v) = p.get(key) else {
        return Ok(None);
    };
    let xy = v
        .as_array()
        .filter(|a| a.len() == 2)
        .and_then(|a| Some((a[0].as_f64()?, a[1].as_f64()?)))
        .filter(|(x, y)| x.is_finite() && y.is_finite() && x.abs() <= 5000.0 && y.abs() <= 5000.0)
        .ok_or_else(|| p.invalid(key, "must be [x, y] in mm"))?;
    Ok(Some(v2(xy.0 * MM, xy.1 * MM)))
}

fn gradient(
    s: &mut EngineState,
    p: &Params,
    threads: Vec<Thread>,
    default_dither: GradientDither,
) -> Result<Value, CommandError> {
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    let o = &s.design.objects[i];
    if !o.contours.iter().any(|c| c.closed) || o.text.is_some() {
        return Err(p.failed(format!("object {id} is not a closed shape")));
    }
    let bounds = o
        .bounds()
        .ok_or_else(|| p.failed(format!("object {id} is empty")))?;
    let n = threads.len();
    // Stops: given, or evenly spaced; with `mix` (blend) the two handles.
    let stops: Vec<f64> = if let Some(v) = p.get("stops").or_else(|| p.get("mix")) {
        let key = if p.has("stops") { "stops" } else { "mix" };
        v.as_array()
            .and_then(|a| a.iter().map(Value::as_f64).collect::<Option<Vec<f64>>>())
            .filter(|l| l.len() == n)
            .filter(|l| l.iter().all(|x| (0.0..=1.0).contains(x)))
            .filter(|l| l.windows(2).all(|w| w[0] <= w[1]))
            .ok_or_else(|| {
                p.invalid(key, format!("must be {n} positions 0-1 in ascending order"))
            })?
    } else {
        (0..n).map(|k| k as f64 / (n - 1) as f64).collect()
    };
    let kind = choice(
        p,
        "kind",
        &[
            ("linear", GradientKind::Linear),
            ("radial", GradientKind::Radial),
        ],
    )?
    .unwrap_or_default();
    let dither =
        choice(p, "dither", &GradientDither::ALL.map(|d| (d.id(), d)))?.unwrap_or(default_dither);
    let curve = choice(p, "curve", &GradientCurve::ALL.map(|c| (c.id(), c)))?.unwrap_or_default();
    let seed = p.opt("seed", 1.0, 0.0, 9_007_199_254_740_992.0)?;
    if seed.fract() != 0.0 {
        return Err(p.invalid("seed", "must be a whole number"));
    }
    let (from, to) = match (point(p, "from")?, point(p, "to")?) {
        (Some(a), Some(b)) => {
            if a.distance(b) < 1.0 {
                return Err(p.invalid("to", "must be at least 0.1 mm from `from`"));
            }
            (a, b)
        }
        (None, None) => {
            let c = v2(
                (bounds.min_x + bounds.max_x) / 2.0,
                (bounds.min_y + bounds.max_y) / 2.0,
            );
            match kind {
                GradientKind::Radial => (c, v2(bounds.max_x, bounds.max_y)),
                GradientKind::Linear => {
                    // Counter-clockwise on screen (y down), spanning the shape.
                    let a = p.opt("angle", 0.0, -360.0, 360.0)?.to_radians();
                    let d = v2(a.cos(), -a.sin());
                    let corners = [
                        v2(bounds.min_x, bounds.min_y),
                        v2(bounds.max_x, bounds.min_y),
                        v2(bounds.min_x, bounds.max_y),
                        v2(bounds.max_x, bounds.max_y),
                    ];
                    let half = corners
                        .iter()
                        .map(|q| ((q.x - c.x) * d.x + (q.y - c.y) * d.y).abs())
                        .fold(0.0, f64::max)
                        .max(1.0);
                    (c - d * half, c + d * half)
                }
            }
        }
        _ => return Err(p.invalid("to", "from and to come together")),
    };
    // The object becomes layer 0; copies for the other colours follow it in the sewing order.
    let base = s.design.objects[i].clone();
    let group = match base.group {
        Some(g) => g,
        None => s.design.new_group_id(),
    };
    let template = Gradient {
        kind,
        from,
        to,
        stops,
        layer: 0,
        dither,
        curve,
        seed: seed as u64,
        later: Vec::new(),
    };
    let mut ids = Vec::with_capacity(n);
    for (k, obj) in layer_objects(&base, &threads, &template, Some(group))
        .into_iter()
        .enumerate()
    {
        if k == 0 {
            s.design.objects[i] = obj;
            ids.push(id);
        } else {
            let new = s.design.add(obj);
            let from_index = s.design.index_of(new).expect("just added");
            let moved = s.design.objects.remove(from_index);
            s.design.objects.insert(i + k, moved);
            ids.push(new);
        }
    }
    Ok(json!({ "ids": ids, "group": group }))
}

/// The objects of a gradient made from `base`: one per thread, each a fill (its fill settings
/// kept, no under sewing) sewing stop `k` of `template` in thread `k`, in `group`.
pub(crate) fn layer_objects(
    base: &DesignObject,
    threads: &[Thread],
    template: &Gradient,
    group: Option<u64>,
) -> Vec<DesignObject> {
    let mut region = base
        .region
        .clone()
        .unwrap_or_else(|| RegionSewing::new(RegionType::Fill, threads[0].clone()));
    if region.kind != RegionType::Fill {
        region = RegionSewing::new(RegionType::Fill, region.thread.clone());
    }
    region.underlay = false;
    region.wave_height = None;
    threads
        .iter()
        .enumerate()
        .map(|(k, t)| {
            let mut r = region.clone();
            r.thread = t.clone();
            r.gradient = Some(Gradient {
                layer: k,
                ..template.clone()
            });
            let mut o = base.clone();
            o.region = Some(r);
            o.group = group;
            o
        })
        .collect()
}

/// `object.setRegion`'s gradient options: dither and curve on the gradient's objects, or
/// `gradient: false` to make them plain fills again.
pub(crate) fn set_region(p: &Params, r: &mut RegionSewing) -> Result<(), CommandError> {
    if p.get("gradient").and_then(Value::as_bool) == Some(false) {
        r.gradient = None;
    } else if p.has("gradient") {
        return Err(p.invalid(
            "gradient",
            "only false (back to a plain fill); use shape.gradient to make one",
        ));
    }
    let dither = choice(
        p,
        "gradientDither",
        &GradientDither::ALL.map(|d| (d.id(), d)),
    )?;
    let curve = choice(p, "gradientCurve", &GradientCurve::ALL.map(|c| (c.id(), c)))?;
    let (from, to) = (point(p, "gradientFrom")?, point(p, "gradientTo")?);
    if from.is_some() != to.is_some() {
        return Err(p.invalid("gradientTo", "gradientFrom and gradientTo come together"));
    }
    if let (Some(a), Some(b)) = (from, to)
        && a.distance(b) < 1.0
    {
        return Err(p.invalid("gradientTo", "must be at least 0.1 mm from gradientFrom"));
    }
    if dither.is_none() && curve.is_none() && from.is_none() {
        return Ok(());
    }
    let Some(g) = r.gradient.as_mut() else {
        return Err(p.failed("this object is not part of a gradient (see shape.gradient)"));
    };
    if let Some(d) = dither {
        g.dither = d;
    }
    if let Some(c) = curve {
        g.curve = c;
    }
    if let (Some(a), Some(b)) = (from, to) {
        g.from = a;
        g.to = b;
    }
    Ok(())
}
