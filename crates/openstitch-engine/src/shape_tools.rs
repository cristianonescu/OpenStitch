//! More shape tools (#497, beyond PE-DESIGN): rectangles with a radius per corner, frame and
//! spiral presets, Divide (`shape.combine` `op: divide`) and shape effects (`shape.effect`).
//! PE-DESIGN's presets and combine modes keep their behaviour. Geometry in
//! `openstitch_design::{shapes, boolean, effects}`.

use crate::commands::{CommandSpec, Effect, IDS, MM, indexes, object_name, opt, req, sewing, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_design::boolean::{self, BoolOp};
use openstitch_design::effects::{self, Effect as ShapeEffect};
use openstitch_design::shapes::{self, ArcMode};
use openstitch_design::{Contour, DesignObject, ObjectKind, v2};
use serde_json::json;

/// `cornerRadii` of `shape.rectangle`: four radii (top left, top right, bottom right, bottom
/// left) in mm, as model units.
pub(crate) fn corner_radii(p: &Params) -> Result<Option<[f64; 4]>, CommandError> {
    if !p.has("cornerRadii") {
        return Ok(None);
    }
    if p.has("cornerRadius") {
        return Err(p.invalid("cornerRadii", "give cornerRadius or cornerRadii, not both"));
    }
    let a = p.array("cornerRadii")?;
    let bad = || {
        p.invalid(
            "cornerRadii",
            "must be four radii [tl, tr, br, bl] in mm, 0-1000",
        )
    };
    if a.len() != 4 {
        return Err(bad());
    }
    let mut r = [0.0; 4];
    for (slot, v) in r.iter_mut().zip(a) {
        let x = v
            .as_f64()
            .filter(|x| (0.0..=1000.0).contains(x))
            .ok_or_else(bad)?;
        *slot = x * MM;
    }
    Ok(Some(r))
}

/// `shape.preset` kinds `frame` and `spiral`.
pub(crate) fn preset(p: &Params, kind: &str) -> Result<DesignObject, CommandError> {
    let w = p.req("width", 0.1, 2000.0)? * MM;
    let h = p.opt("height", w / MM, 0.1, 2000.0)? * MM;
    let center = v2(
        p.req("cx", -5000.0, 5000.0)? * MM,
        p.req("cy", -5000.0, 5000.0)? * MM,
    );
    let rotation = p.opt("rotation", 0.0, -3600.0, 3600.0)?;
    let mut object = if kind == "spiral" {
        let turns = p.opt("turns", 3.0, 0.25, 50.0)?;
        let inner = p.opt("innerRadius", 0.0, 0.0, 1000.0)? * MM;
        let clockwise = match p.string("direction", Some("cw"))?.as_str() {
            "cw" => true,
            "ccw" => false,
            _ => return Err(p.invalid("direction", "must be cw or ccw")),
        };
        let (rx, ry) = (w / 2.0, h / 2.0);
        if inner >= rx.min(ry) {
            return Err(p.invalid("innerRadius", "must be smaller than the outer radius"));
        }
        let c = shapes::spiral(
            center,
            rx,
            ry,
            inner / rx.min(ry),
            turns,
            rotation,
            clockwise,
        );
        let (region, line) = sewing(p, false)?;
        DesignObject::shape(vec![c], region, line)
    } else {
        let band = p.opt("bandWidth", 5.0, 0.1, 500.0)? * MM;
        let unit = match p.string("frameOf", Some("rectangle"))?.as_str() {
            "rectangle" => Contour::rectangle(0.0, 0.0, 1.0, 1.0),
            "roundedRectangle" => shapes::rounded_rectangle(w, h, w.min(h) / 5.0),
            "ellipse" => shapes::arc(v2(0.0, 0.0), 1.0, 1.0, 90.0, 360.0, ArcMode::Chord),
            "heart" => shapes::heart(),
            _ => {
                return Err(p.invalid(
                    "frameOf",
                    "must be rectangle, ellipse, roundedRectangle or heart",
                ));
            }
        };
        let outer = shapes::fit(&unit, center, w, h, rotation);
        let inner = boolean::contours(&boolean::offset(std::slice::from_ref(&outer), -band, true));
        if inner.is_empty() {
            return Err(p.invalid("bandWidth", "the band is wider than the shape allows"));
        }
        let ring = boolean::combine(&[outer], &inner, BoolOp::Difference);
        if ring.is_empty() {
            return Err(p.invalid("bandWidth", "the band leaves nothing"));
        }
        let (region, line) = sewing(p, true)?;
        DesignObject::shape(boolean::contours(&ring), region, line)
    };
    object.name = Some(
        object_name(p)?.unwrap_or_else(|| if kind == "spiral" { "Spiral" } else { "Frame" }.into()),
    );
    Ok(object)
}

/// `shape.combine` `op: divide`: every overlap of the shapes becomes its own shape, sewn like
/// the topmost (last-sewn) shape over it; the pieces take the place of the first shape.
pub(crate) fn divide(s: &mut EngineState, p: &Params) -> Result<serde_json::Value, CommandError> {
    let mut idx = indexes(p, &s.design)?;
    idx.sort_unstable();
    idx.dedup();
    if idx.len() < 2 {
        return Err(p.invalid("ids", "needs at least 2 different objects"));
    }
    for &i in &idx {
        crate::shape_ops::closed_shape(&s.design, p, i)?;
        if s.design.objects[i].hole_sewing {
            return Err(p.failed("shapes with hole sewing cannot be divided"));
        }
    }
    let shapes: Vec<&[Contour]> = idx
        .iter()
        .map(|&i| &s.design.objects[i].contours[..])
        .collect();
    let pieces = boolean::divide(&shapes);
    if pieces.is_empty() {
        return Err(p.failed("the result is empty"));
    }
    let mut out: Vec<DesignObject> = pieces
        .into_iter()
        .map(|(owner, r)| {
            let mut o = s.design.objects[idx[owner]].clone();
            o.contours = r;
            o.name = None;
            if let Some(reg) = &mut o.region {
                reg.entry = None;
                reg.exit = None;
                reg.direction_lines.clear();
                reg.skeleton = None;
            }
            if let Some(l) = &mut o.line {
                l.entry = None;
                l.exit = None;
            }
            o
        })
        .collect();
    // Pieces in the shapes' sewing order.
    out.sort_by_key(|o| idx.iter().position(|&i| s.design.objects[i].id == o.id));
    let at = idx[0];
    crate::shape_ops::remove(&mut s.design, &idx);
    let mut ids = Vec::new();
    for (k, mut o) in out.into_iter().enumerate() {
        o.id = s.design.next_object_id;
        s.design.next_object_id += 1;
        ids.push(o.id);
        s.design
            .objects
            .insert((at + k).min(s.design.objects.len()), o);
    }
    Ok(json!({ "ids": ids, "pieces": ids.len() }))
}

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "shape.effect",
        "Shape effect (beyond PE-DESIGN): warp, twist, roughen, puckerBloat or zigzag the outlines, leaving ordinary editable geometry (deterministic for a seed)",
        Effect::Edit,
        vec![
            IDS,
            req("kind", "warp|twist|roughen|puckerBloat|zigzag", "Effect"),
            opt(
                "amount",
                "mm | degrees | %",
                "Warp: wave height mm; twist: degrees at the farthest point; roughen, zigzag: mm; puckerBloat: % of each edge, negative puckers (defaults 1 mm, 45°, 20 %)",
            ),
            opt(
                "size",
                "mm",
                "Roughen and zigzag: distance between points (0.2-50, default 2)",
            ),
            opt("seed", "int", "Roughen: random seed (default 1)"),
        ],
        |s, p| {
            let kind = p.string("kind", None)?;
            let effect = ShapeEffect::from_id(&kind).ok_or_else(|| {
                p.invalid(
                    "kind",
                    "must be warp, twist, roughen, puckerBloat or zigzag",
                )
            })?;
            let (lo, hi) = match effect {
                ShapeEffect::Twist => (-720.0, 720.0),
                ShapeEffect::PuckerBloat => (-100.0, 100.0),
                ShapeEffect::Warp => (-50.0, 50.0),
                _ => (0.0, 50.0),
            };
            let amount = p.opt("amount", effect.amount().1, lo, hi)?;
            let size = p.opt("size", 2.0, 0.2, 50.0)?;
            let seed = p.int("seed", Some(1), 0, i64::MAX)? as u64;
            let idx = indexes(p, &s.design)?;
            for &i in &idx {
                let o = &s.design.objects[i];
                match o.kind {
                    ObjectKind::Shape => {}
                    ObjectKind::Text => {
                        return Err(p.failed(format!(
                            "object {} is text; convert it to shapes first (text.toShapes)",
                            o.id
                        )));
                    }
                    ObjectKind::Stitches => {
                        return Err(p.failed(format!("object {} is a stitch pattern", o.id)));
                    }
                }
                if o.region
                    .as_ref()
                    .is_some_and(|r| r.direction_lines.len() >= 2 || r.skeleton.is_some())
                {
                    return Err(p.failed(format!(
                        "object {} is sewn along direction lines or a skeleton; effects change only free outlines",
                        o.id
                    )));
                }
            }
            for &i in &idx {
                let o = &mut s.design.objects[i];
                let out = effects::apply(&o.contours, effect, amount, size, seed);
                if out.len() != o.contours.len() {
                    return Err(p.failed(format!("object {}: the effect leaves too little", o.id)));
                }
                o.contours = out;
                if let Some(r) = &mut o.region {
                    r.entry = None;
                    r.exit = None;
                }
                if let Some(l) = &mut o.line {
                    l.entry = None;
                    l.exit = None;
                }
            }
            Ok(json!(null))
        },
    )]
}
