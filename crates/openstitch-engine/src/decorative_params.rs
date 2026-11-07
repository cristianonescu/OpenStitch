//! `object.setLine`'s settings of the decorative hand-embroidery lines and the E stitch's prong
//! angle (#543, beyond PE-DESIGN, off unless chosen).

use crate::commands::{ParamDoc, opt};
use crate::params::{CommandError, Params};
use openstitch_design::{LineSewing, LineType};

/// The most stripe gaps.
pub const MAX_GAPS: usize = 12;

pub(crate) const PARAMS: [ParamDoc; 10] = [
    opt(
        "strands",
        "int",
        "Ripple: copies, 2-30 (default 5). Lattice: strands crossing the band, 3-7 (an even count sews the next odd one, so the line ends at its far end; default 3); stripes: rows across the band, 2-12 (default 3)",
    ),
    opt(
        "gaps",
        "[mm] | null",
        "Stripes: the gaps between neighbouring rows, 1-12 gaps of 0.2-10 mm (pinstripes 1-2-1-2-1), centred on the line, instead of even rows; null: even rows",
    ),
    opt(
        "target",
        "[x, y] mm | null",
        "Ripple (beyond PE-DESIGN, #541): the point the copies shrink towards (a closed line spirals in to it as one run; an open line's copies step towards its side); null: the line's centre",
    ),
    opt(
        "motifGroups",
        "[[count, gap mm]] | null",
        "Motif line (beyond PE-DESIGN, #544): motifs in groups, each [motifs in the group 1-20, gap after the group 0-50 mm], repeated along the line; plain running stitch over the gaps. 1-8 groups; null: PE-DESIGN's even motifs",
    ),
    opt(
        "groupSize",
        "int",
        "Motif line: one repeating group of this many motifs, 0-20 (0: no groups; the editor's form of motifGroups)",
    ),
    opt(
        "groupGap",
        "mm",
        "Motif line: the gap after each group, 0-50 (default 5)",
    ),
    opt(
        "freeStart",
        "mm | null",
        "Motif line: running stitch only for this long at the start, 0-100 (#544)",
    ),
    opt(
        "freeEnd",
        "mm | null",
        "Motif line: running stitch only for this long at the end, 0-100 (#544)",
    ),
    opt(
        "motifs",
        "[string] | string | null",
        "Motif line: more motifs (ids from patterns.list) used in turn after motif, 1-8 (#544); one id alone or \"none\" (no others) as the editor sends it; arrange alternate mirrors every other one",
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
            LineType::Ripple => l.strands = Some(p.int("strands", None, 2, 30)? as u32),
            _ => return Err(p.invalid("strands", "needs a ripple, lattice or stripes line")),
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
    if p.is_null("target") {
        l.target = None;
    } else if p.has("target") {
        if l.kind != LineType::Ripple {
            return Err(p.invalid("target", "needs a ripple line"));
        }
        let xy = p
            .get("target")
            .and_then(|v| v.as_array())
            .filter(|a| a.len() == 2)
            .and_then(|a| a[0].as_f64().zip(a[1].as_f64()))
            .filter(|(x, y)| x.abs() <= 5000.0 && y.abs() <= 5000.0)
            .ok_or_else(|| p.invalid("target", "must be [x, y] in mm"))?;
        l.target = Some(openstitch_design::v2(xy.0 * 10.0, xy.1 * 10.0));
    }
    if p.is_null("motifGroups") {
        l.motif_groups.clear();
    } else if p.has("motifGroups") {
        motif_only(p, l, "motifGroups")?;
        let arr = p.array("motifGroups")?;
        if arr.is_empty() || arr.len() > 8 {
            return Err(p.invalid("motifGroups", "needs 1-8 groups"));
        }
        l.motif_groups = arr
            .iter()
            .map(|v| {
                v.as_array()
                    .filter(|a| a.len() == 2)
                    .and_then(|a| a[0].as_u64().zip(a[1].as_f64()))
                    .filter(|(n, g)| (1..=20).contains(n) && (0.0..=50.0).contains(g))
                    .map(|(n, g)| (n as u32, g))
                    .ok_or_else(|| {
                        p.invalid(
                            "motifGroups",
                            "each group must be [count 1-20, gap 0-50 mm]",
                        )
                    })
            })
            .collect::<Result<_, _>>()?;
    }
    if p.has("groupSize") || p.has("groupGap") {
        motif_only(p, l, "groupSize")?;
        let (n0, g0) = l.motif_groups.first().copied().unwrap_or((0, 5.0));
        let n = p.int("groupSize", Some(i64::from(n0)), 0, 20)? as u32;
        let g = p.opt("groupGap", g0, 0.0, 50.0)?;
        l.motif_groups = if n == 0 { Vec::new() } else { vec![(n, g)] };
    }
    for (key, slot) in [
        ("freeStart", &mut l.free_start),
        ("freeEnd", &mut l.free_end),
    ] {
        if p.is_null(key) {
            *slot = None;
        } else if p.has(key) {
            if l.kind != LineType::Motif {
                return Err(p.invalid(key, "needs a motif line"));
            }
            let v = p.req(key, 0.0, 100.0)?;
            *slot = (v > 0.0).then_some(v);
        }
    }
    let one = p.get("motifs").and_then(|v| v.as_str()).map(str::to_string);
    if p.is_null("motifs") || one.as_deref() == Some("none") {
        l.motifs.clear();
    } else if let Some(id) = one {
        motif_only(p, l, "motifs")?;
        l.motifs = vec![id];
    } else if p.has("motifs") {
        motif_only(p, l, "motifs")?;
        let arr = p.array("motifs")?;
        if arr.is_empty() || arr.len() > 8 {
            return Err(p.invalid("motifs", "needs 1-8 motif ids"));
        }
        l.motifs = arr
            .iter()
            .map(|v| {
                v.as_str().map(str::to_string).ok_or_else(|| {
                    p.invalid("motifs", "each motif must be an id from patterns.list")
                })
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

fn motif_only(p: &Params, l: &LineSewing, key: &str) -> Result<(), CommandError> {
    if l.kind == LineType::Motif {
        Ok(())
    } else {
        Err(p.invalid(key, "needs a motif line"))
    }
}
