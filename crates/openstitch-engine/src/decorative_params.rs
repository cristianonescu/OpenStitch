//! `object.setLine`'s settings of the decorative hand-embroidery lines and the E stitch's prong
//! angle (#543, beyond PE-DESIGN, off unless chosen).

use crate::commands::{ParamDoc, opt};
use crate::params::{CommandError, Params};
use openstitch_design::{LineSewing, LineType};

/// The most stripe gaps.
pub const MAX_GAPS: usize = 12;

pub(crate) const PARAMS: [ParamDoc; 3] = [
    opt(
        "strands",
        "int",
        "Lattice: strands crossing the band, 3-7 (an even count sews the next odd one, so the line ends at its far end; default 3); stripes: rows across the band, 2-12 (default 3)",
    ),
    opt(
        "gaps",
        "[mm] | null",
        "Stripes: the gaps between neighbouring rows, 1-12 gaps of 0.2-10 mm (pinstripes 1-2-1-2-1), centred on the line, instead of even rows; null: even rows",
    ),
    opt(
        "prongAngle",
        "degrees",
        "E stitch: prongs slanted forward from square to the line, -60...60 (default 0, PE-DESIGN's square prongs)",
    ),
];

/// Applies the decorative settings of `p` to `l` (after its type is set).
pub(crate) fn apply(p: &Params, l: &mut LineSewing) -> Result<(), CommandError> {
    if p.has("strands") {
        match l.kind {
            LineType::Lattice => l.strands = Some(p.int("strands", None, 3, 7)? as u32),
            LineType::Stripes => l.strands = Some(p.int("strands", None, 2, 12)? as u32),
            _ => return Err(p.invalid("strands", "needs a lattice or stripes line")),
        }
    }
    if p.is_null("gaps") {
        l.gaps.clear();
    } else if p.has("gaps") {
        if l.kind != LineType::Stripes {
            return Err(p.invalid("gaps", "needs a stripes line"));
        }
        let arr = p.array("gaps")?;
        if arr.is_empty() || arr.len() > MAX_GAPS {
            return Err(p.invalid("gaps", format!("needs 1-{MAX_GAPS} gaps")));
        }
        l.gaps = arr
            .iter()
            .map(|v| {
                v.as_f64()
                    .filter(|g| (0.2..=10.0).contains(g))
                    .ok_or_else(|| p.invalid("gaps", "each gap must be 0.2-10 mm"))
            })
            .collect::<Result<_, _>>()?;
    }
    if p.has("prongAngle") {
        if l.kind != LineType::EStitch {
            return Err(p.invalid("prongAngle", "needs an E stitch line"));
        }
        let a = p.req("prongAngle", -60.0, 60.0)?;
        l.prong_angle = (a != 0.0).then_some(a);
    }
    Ok(())
}
