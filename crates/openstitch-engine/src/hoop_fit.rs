//! One-click hoop commands (#879, beyond PE-DESIGN): put the design in the smallest hoop of
//! the user's machine that holds it, or scale it to fill the current hoop. Each is one undo
//! step: the steps run inside the command.

use crate::commands::{CommandSpec, Effect, MM, opt, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_core::Command;
use openstitch_design::machine::{self, Hoop, HoopShape};
use openstitch_design::{Compiler, ObjectKind};
use serde_json::{Value, json};

/// A needle point may lie this far outside a field and still count as in it (0.1 mm units):
/// half a unit, as coordinates are written rounded to 0.1 mm.
const TOL: f64 = 0.5;

/// A stitch-only object scaled beyond these factors changes its density noticeably.
const STITCH_SCALE_WARN: (f64, f64) = (0.8, 1.2);

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "machine.useSmallestHoop",
            "Switch the Design Page to the smallest single-position hoop of the machine that holds the design (its needle points in the hoop's real field, either orientation) and centre the design in it (beyond PE-DESIGN, #879); one undo step",
            Effect::Edit,
            vec![opt(
                "brand",
                "string",
                "Hoop brand from machine.hoops (default: the machine profile's brand, then the selected hoop's, then Brother); with a machine profile of that brand, hoops larger than its hoop are left out",
            )],
            use_smallest_hoop,
        ),
        spec(
            "design.fitToHoop",
            "Scale the design about its centre so its needle points fill the current hoop's field less a margin, then centre it (beyond PE-DESIGN, #879); shapes and text keep their densities, stitch-only objects scaled beyond 0.8-1.2 warn that theirs changes; one undo step",
            Effect::Edit,
            vec![
                opt(
                    "marginMm",
                    "mm",
                    "Space left between the design and the field's edge, 0-50 (default 5)",
                ),
                opt(
                    "keepAspect",
                    "bool",
                    "Scale both directions alike (default true); false fills a rectangular field in each direction (refused for oval and round hoops)",
                ),
            ],
            fit_to_hoop,
        ),
    ]
}

/// Runs another command inside this one, so the whole is one undo step.
fn call(s: &mut EngineState, id: &str, params: Value) -> crate::CommandResult {
    let spec = crate::commands::all()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("hoop_fit: unknown command {id}"));
    let p = Params::new(id, Some(&params))?;
    (spec.run)(s, &p)
}

/// The design's needle points (0.1 mm), every object as it sews.
fn needle_points(s: &EngineState) -> Vec<(f64, f64)> {
    Compiler::for_design(&s.design)
        .compile(&s.design)
        .stitches
        .iter()
        .filter(|st| st.command == Command::Stitch)
        .map(|st| (st.x, st.y))
        .collect()
}

/// The bounds of `pts`: (min x, min y, max x, max y).
fn bounds(pts: &[(f64, f64)]) -> Option<(f64, f64, f64, f64)> {
    let first = pts.first()?;
    Some(
        pts.iter()
            .fold((first.0, first.1, first.0, first.1), |b, &(x, y)| {
                (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y))
            }),
    )
}

/// Whether `pts`, their bounding box centred in a `w` × `h` mm field of `shape`, all lie in it.
fn fits_field(
    pts: &[(f64, f64)],
    b: (f64, f64, f64, f64),
    shape: HoopShape,
    w: f64,
    h: f64,
) -> bool {
    let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    let (fw, fh) = (w * MM, h * MM);
    pts.iter()
        .all(|&(x, y)| shape.contains(fw, fh, x - cx + fw / 2.0, y - cy + fh / 2.0, TOL))
}

/// The orientation (`false`: as listed, `true`: turned 90°) in which `pts` fit hoop `h`.
fn fits_hoop(pts: &[(f64, f64)], b: (f64, f64, f64, f64), h: &Hoop) -> Option<bool> {
    if fits_field(pts, b, h.shape, h.width, h.height) {
        Some(false)
    } else if h.can_rotate() && fits_field(pts, b, h.shape, h.height, h.width) {
        Some(true)
    } else {
        None
    }
}

fn use_smallest_hoop(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let profile = s.user.machine_profile.clone();
    let brand = match p.has("brand") {
        true => p.string("brand", None)?,
        false => profile
            .as_ref()
            .map(|pr| pr.brand.clone())
            .or_else(|| s.design.machine.listed_hoop().map(|h| h.brand.to_string()))
            .unwrap_or_else(|| "Brother".to_string()),
    };
    if machine::brand_hoops(&brand).next().is_none() {
        return Err(p.invalid("brand", "is not a hoop brand from machine.hoops"));
    }
    // The profile's hoop is the largest its machine takes.
    let limit = profile
        .as_ref()
        .filter(|pr| pr.brand == brand)
        .and_then(|pr| pr.hoop.as_deref())
        .and_then(machine::find_hoop);
    let within = |h: &Hoop| {
        limit.is_none_or(|l| {
            let (big, small) = (l.width.max(l.height), l.width.min(l.height));
            h.width.max(h.height) <= big + 1e-9 && h.width.min(h.height) <= small + 1e-9
        })
    };
    let pts = needle_points(s);
    let Some(b) = bounds(&pts) else {
        return Err(p.failed("the design sews nothing"));
    };
    let candidates: Vec<&Hoop> = machine::brand_hoops(&brand)
        .filter(|h| !h.multi_position && within(h))
        .collect();
    let best = candidates
        .iter()
        .filter_map(|h| fits_hoop(&pts, b, h).map(|turned| (*h, turned)))
        .min_by(|a, c| {
            (a.0.width * a.0.height)
                .total_cmp(&(c.0.width * c.0.height))
                .then_with(|| a.0.id.cmp(c.0.id))
        });
    let (dw, dh) = ((b.2 - b.0) / MM, (b.3 - b.1) / MM);
    let Some((hoop, turned)) = best else {
        let largest = candidates
            .iter()
            .max_by(|a, c| (a.width * a.height).total_cmp(&(c.width * c.height)))
            .map_or_else(String::new, |h| format!(" (the largest is {})", h.label));
        return Err(p.failed(format!(
            "the design ({dw:.1} × {dh:.1} mm) fits no single-position {brand} hoop{largest}{}; split it over several hoopings (multi-hooping) or scale it down",
            if limit.is_some() { " the machine takes" } else { "" }
        )));
    };
    let mut set = json!({ "hoop": hoop.id });
    if hoop.can_rotate() {
        set["rotate90"] = json!(turned);
    }
    call(s, "machine.set", set)?;
    let ids: Vec<u64> = s.design.objects.iter().map(|o| o.id).collect();
    centre_on_page(s, &ids)?;
    Ok(json!({
        "hoop": hoop.id,
        "label": hoop.label,
        "rotate90": turned,
        "hoopMM": if turned { [hoop.height, hoop.width] } else { [hoop.width, hoop.height] },
        "designMM": [dw, dh],
    }))
}

/// Moves `ids` so the design's needle points are centred on the Design Page.
fn centre_on_page(s: &mut EngineState, ids: &[u64]) -> Result<(), CommandError> {
    let Some(b) = bounds(&needle_points(s)) else {
        return Ok(());
    };
    let page = s.design.page.center();
    let (dx, dy) = (
        (page.x - (b.0 + b.2) / 2.0) / MM,
        (page.y - (b.1 + b.3) / 2.0) / MM,
    );
    if !ids.is_empty() && (dx.abs() > 1e-9 || dy.abs() > 1e-9) {
        call(s, "object.move", json!({ "ids": ids, "dx": dx, "dy": dy }))?;
    }
    Ok(())
}

fn fit_to_hoop(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let margin = p.opt("marginMm", 5.0, 0.0, 50.0)? * MM;
    let keep = p.bool("keepAspect", Some(true))?;
    let shape = s.design.machine.hoop_shape();
    if !keep && shape != HoopShape::Rectangle {
        return Err(p.invalid("keepAspect", "false needs a rectangular hoop field"));
    }
    let pts = needle_points(s);
    let Some(b) = bounds(&pts) else {
        return Err(p.failed("the design sews nothing"));
    };
    let (w, h) = (s.design.page.width, s.design.page.height);
    let (cx, cy) = ((b.0 + b.2) / 2.0, (b.1 + b.3) / 2.0);
    let (dw, dh) = (b.2 - b.0, b.3 - b.1);
    let (sx, sy) = match shape {
        HoopShape::Rectangle => {
            let (aw, ah) = (w - 2.0 * margin, h - 2.0 * margin);
            if aw <= 0.0 || ah <= 0.0 {
                return Err(p.invalid("marginMm", "leaves no room in the hoop"));
            }
            let fx = if dw > 0.0 { aw / dw } else { f64::INFINITY };
            let fy = if dh > 0.0 { ah / dh } else { f64::INFINITY };
            let f = fx.min(fy);
            if !f.is_finite() {
                return Err(p.failed("the design has no size to scale"));
            }
            if keep {
                (f, f)
            } else {
                (fx.min(100.0), fy.min(100.0))
            }
        }
        HoopShape::Oval | HoopShape::Round => {
            let (a, bb) = (w / 2.0 - margin, h / 2.0 - margin);
            if a <= 0.0 || bb <= 0.0 {
                return Err(p.invalid("marginMm", "leaves no room in the hoop"));
            }
            // The farthest needle point, in units of the shrunk ellipse, goes onto it.
            let r = pts
                .iter()
                .map(|&(x, y)| ((x - cx) / a).powi(2) + ((y - cy) / bb).powi(2))
                .fold(0.0_f64, f64::max)
                .sqrt();
            if r <= 0.0 {
                return Err(p.failed("the design has no size to scale"));
            }
            (1.0 / r, 1.0 / r)
        }
    };
    let (sx, sy) = (sx.clamp(0.01, 100.0), sy.clamp(0.01, 100.0));
    let ids: Vec<u64> = s.design.objects.iter().map(|o| o.id).collect();
    call(
        s,
        "object.scale",
        json!({ "ids": ids, "sx": sx, "sy": sy, "originX": cx / MM, "originY": cy / MM }),
    )?;
    centre_on_page(s, &ids)?;
    let mut warnings = Vec::new();
    let outside = |f: f64| f < STITCH_SCALE_WARN.0 || f > STITCH_SCALE_WARN.1;
    if (outside(sx) || outside(sy))
        && s.design
            .objects
            .iter()
            .any(|o| o.kind == ObjectKind::Stitches)
    {
        warnings.push(format!(
            "stitch-only objects were scaled by {:.0} %{}: their needle points moved apart or together, so their density changed; digitize them as shapes to keep it",
            sx * 100.0,
            if (sx - sy).abs() > 1e-9 { format!(" × {:.0} %", sy * 100.0) } else { String::new() }
        ));
    }
    Ok(json!({ "scale": [sx, sy], "warnings": warnings }))
}
