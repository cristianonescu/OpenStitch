//! The motif band's settings (#544, beyond PE-DESIGN, off unless chosen): on `object.setRegion`
//! for skeleton satin and Manual Punch satin regions, and on `object.setLine` for satin lines.

use crate::commands::{ParamDoc, opt};
use crate::params::{CommandError, Params};
use openstitch_design::Design;
use openstitch_design::motif_band::MotifBand;
use openstitch_design::patterns::PatternKind;

pub(crate) const PARAMS: [ParamDoc; 3] = [
    opt(
        "motifBand",
        "string | none",
        "Satin regions sewn along a skeleton or as a Manual Punch column, and satin lines (beyond PE-DESIGN, #544): a motif (an id from patterns.list) repeated along the column instead of the satin, each copy stretched across the column's local width so it narrows and widens with a taper; none: the satin",
    ),
    opt(
        "bandTile",
        "mm | null",
        "Motif band: each copy's length along the column, 1-50 mm, a whole number of copies stretched to fill it; null or 0: the column's mean width",
    ),
    opt(
        "bandMirror",
        "bool",
        "Motif band: every other copy mirrored across the column",
    ),
];

/// Checks before any object changes that a named motif exists.
pub(crate) fn check(p: &Params, design: &Design) -> Result<(), CommandError> {
    if let Some(id) = p.get("motifBand").and_then(|v| v.as_str())
        && id != "none"
        && design.find_pattern(PatternKind::Motif, id).is_none()
    {
        return Err(p.invalid(
            "motifBand",
            format!("'{id}' is not a motif; see patterns.list"),
        ));
    }
    Ok(())
}

/// Applies the band's settings; `column` tells whether the object can sew one, `what` names it
/// in the refusal.
pub(crate) fn apply(
    p: &Params,
    column: bool,
    what: &str,
    band: &mut Option<MotifBand>,
) -> Result<(), CommandError> {
    if p.is_null("motifBand") {
        *band = None;
    } else if p.has("motifBand") {
        let id = p.string("motifBand", None)?;
        if id == "none" {
            *band = None;
        } else {
            if !column {
                return Err(p.invalid("motifBand", format!("needs {what}")));
            }
            match band {
                Some(b) => b.motif = id,
                None => {
                    *band = Some(MotifBand {
                        motif: id,
                        tile: None,
                        mirror: false,
                    })
                }
            }
        }
    }
    let has = |k: &str| p.has(k) || p.is_null(k);
    if (has("bandTile") || has("bandMirror")) && band.is_none() {
        let k = if has("bandTile") {
            "bandTile"
        } else {
            "bandMirror"
        };
        return Err(p.invalid(k, "needs a motif band (motifBand)"));
    }
    if let Some(b) = band {
        if p.is_null("bandTile") {
            b.tile = None;
        } else if p.has("bandTile") {
            let t = p.req("bandTile", 0.0, 50.0)?;
            if t > 0.0 && t < 1.0 {
                return Err(p.invalid("bandTile", "must be 0 or 1...50"));
            }
            b.tile = (t > 0.0).then_some(t);
        }
        b.mirror = p.bool("bandMirror", Some(b.mirror))?;
    }
    Ok(())
}
