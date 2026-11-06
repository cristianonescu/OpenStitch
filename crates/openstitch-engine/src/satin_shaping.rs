//! The satin shaping and under sewing attributes beyond PE-DESIGN (#520 corner styles, #521
//! short stitches, #522 compensation per side and per cent, #523 automatic and second under
//! sewing), read by `object.setRegion` (Manual Punch columns) and `object.setLine` (zigzag
//! lines). All are off by default; a column or line without them sews as PE-DESIGN's.

use crate::commands::{ParamDoc, opt};
use crate::params::{CommandError, Params};
use openstitch_design::UnderlayStyle;
use openstitch_design::column::{
    AutoUnderlay, CornerStyle, Jagged, JaggedMode, JaggedSide, RAISED_MIN_WIDTH, Raised,
    SatinShaping, SatinStyle, SecondUnderlay,
};

/// The parameters shared by `object.setRegion` and `object.setLine`.
pub(crate) const SHAPING_PARAMS: [ParamDoc; 16] = [
    opt(
        "cornerStyle",
        "turn|auto|mitre|cap",
        "Satin corners (beyond PE-DESIGN; Manual Punch columns and zigzag lines): turn (default, PE-DESIGN's fan), auto (cap below capBelow, mitre below mitreBelow), mitre (a seam from the inner corner to the tip, each leg's stitches square to it) or cap (both legs end on a square line through the inner corner, the tip sewn as its own piece over a hidden run)",
    ),
    opt(
        "mitreBelow",
        "degrees",
        "Corners whose interior angle is below this are mitred or capped, 10-170 (default 75)",
    ),
    opt(
        "capBelow",
        "degrees",
        "cornerStyle auto: corners below this are capped, 5-120 (default 45)",
    ),
    opt(
        "cornerOverlap",
        "mm",
        "How far each mitre leg (and a cap into its leg) runs past the cut, 0-2 (default 0.5)",
    ),
    opt(
        "capMaxLength",
        "mm",
        "A cap whose stitches would be longer than this is mitred instead, 2-12 (default 8)",
    ),
    opt(
        "shortStitchDistance",
        "mm",
        "Short stitches on the inside of curves (beyond PE-DESIGN): a needle point closer than this to the last full one on its edge stops short, 0-1 (0 = off, the default; 0.25 suits most satins)",
    ),
    opt(
        "shortStitchInset",
        "percent",
        "How far a short stitch stops short, % of the stitch's width, 5-80 (default 15)",
    ),
    opt(
        "pullLeft",
        "mm",
        "Compensation of the left side (the first stitch's start rail), 0-2; null returns it to pullCompensation (columns) or 0 (zigzag lines)",
    ),
    opt(
        "pullRight",
        "mm",
        "Compensation of the right side, 0-2; null returns it to pullCompensation or 0",
    ),
    opt(
        "pullPercent",
        "percent",
        "Compensation added to each side as % of each stitch's own width, 0-20 (default 0; 3 suits tapered columns)",
    ),
    opt(
        "pullMax",
        "mm",
        "Largest compensation of one side, 0-3; null removes the limit",
    ),
    opt(
        "satinStyle",
        "normal|sStitch|open|double",
        "Satin look (beyond PE-DESIGN; Manual Punch columns and zigzag lines): normal (default), sStitch (each crossing three short stitches bent into an S, a corded edge), open (rows three times as far apart) or double (sewn twice, the second pass back from the end midway between the first's needle points: an X lattice)",
    ),
    opt(
        "jaggedEdge",
        "{mode: in|out|both, side: both|left|right, amount, seed} | in|out|both|none | null",
        "Jagged edge for fur, grass and feathers (beyond PE-DESIGN): needle points on the chosen edges move up to amount mm (0.1-5, default 1) into the column, out, or both, the same for the same seed (default 1); side left is the first stitch's start rail; null removes it",
    ),
    opt(
        "spacingAt",
        "0-1",
        "Manual Punch column: where across a curved column the density is measured, 0 (shorter side) to 1 (longer side, PE-DESIGN's, the default); 0.5 evens it at the centre",
    ),
    opt(
        "raised",
        "{laneSpacing, inset, stitchLength} | true | null",
        "Raised (padded) satin (beyond PE-DESIGN): running-stitch lanes along the column under the satin, about one per laneSpacing mm of width (0.5-5, default 1.5; an even number, at least two), inset mm from the edges (0-3, default 0.5), stitchLength mm (1-10, default 2.5); true turns it on (with its earlier settings, else the defaults), null removes it. Meant for columns of 3 mm and wider (the result gives a hint below that)",
    ),
    opt(
        "underlayAuto",
        "{centreBelow, edgeBelow}",
        "underlayStyle auto: a centre walk below centreBelow mm (default 2), the edge run below edgeBelow (default 4), the edge run and zigzag above; fabric.apply sets them for the fabric",
    ),
];

/// The parameters of `object.setRegion` only.
pub(crate) const REGION_PARAMS: [ParamDoc; 1] = [opt(
    "underlay2",
    "{style, inset, spacing, stitchLength} | null",
    "Manual Punch column: a second under sewing layer after the first (beyond PE-DESIGN): style edge|zigzag|edgeZigzag|centreWalk|auto (default zigzag), inset mm 0-3 (0.5), spacing mm 0.5-10 (2), stitchLength mm 1-10 (2.5); null removes it",
)];

/// Applies the shaping parameters present in `p` to `s`.
pub(crate) fn apply_shaping(p: &Params, s: &mut SatinShaping) -> Result<(), CommandError> {
    if p.has("cornerStyle") {
        let id = p.string("cornerStyle", None)?;
        s.corners.style = CornerStyle::from_id(&id)
            .ok_or_else(|| p.invalid("cornerStyle", "must be turn, auto, mitre or cap"))?;
    }
    let c = &mut s.corners;
    c.mitre_below = p.opt("mitreBelow", c.mitre_below, 10.0, 170.0)?;
    c.cap_below = p.opt("capBelow", c.cap_below, 5.0, 120.0)?;
    c.overlap = p.opt("cornerOverlap", c.overlap, 0.0, 2.0)?;
    c.cap_max_length = p.opt("capMaxLength", c.cap_max_length, 2.0, 12.0)?;
    let ss = &mut s.short_stitches;
    ss.distance = p.opt("shortStitchDistance", ss.distance, 0.0, 1.0)?;
    ss.inset = p.opt("shortStitchInset", ss.inset, 5.0, 80.0)?;
    let k = &mut s.compensation;
    for (key, side, hi) in [
        ("pullLeft", &mut k.left, 2.0),
        ("pullRight", &mut k.right, 2.0),
        ("pullMax", &mut k.max, 3.0),
    ] {
        if p.is_null(key) {
            *side = None;
        } else if p.has(key) {
            *side = Some(p.req(key, 0.0, hi)?);
        }
    }
    k.percent = p.opt("pullPercent", k.percent, 0.0, 20.0)?;
    if p.has("satinStyle") {
        let id = p.string("satinStyle", None)?;
        s.style = SatinStyle::from_id(&id)
            .ok_or_else(|| p.invalid("satinStyle", "must be normal, sStitch, open or double"))?;
    }
    if p.has("spacingAt") {
        let a = p.req("spacingAt", 0.0, 1.0)?;
        s.spacing_at = (a < 1.0).then_some(a);
    }
    apply_jagged(p, &mut s.jagged)?;
    apply_raised(p, &mut s.raised)?;
    Ok(())
}

/// A number field of an object parameter, within `lo..=hi`.
fn field(
    p: &Params,
    key: &str,
    v: &serde_json::Value,
    name: &str,
    cur: f64,
    lo: f64,
    hi: f64,
) -> Result<f64, CommandError> {
    match v.get(name) {
        None => Ok(cur),
        Some(x) => x
            .as_f64()
            .filter(|x| (lo..=hi).contains(x))
            .ok_or_else(|| p.invalid(key, format!("{name} must be {lo}-{hi}"))),
    }
}

fn apply_jagged(p: &Params, j: &mut Option<Jagged>) -> Result<(), CommandError> {
    const KEY: &str = "jaggedEdge";
    if p.is_null(KEY) {
        *j = None;
        return Ok(());
    }
    let Some(v) = p.get(KEY) else {
        return Ok(());
    };
    // A mode name alone (the inspector's choice), or none.
    if let Some(m) = v.as_str() {
        let mode = match m {
            "none" => {
                *j = None;
                return Ok(());
            }
            "in" => JaggedMode::In,
            "out" => JaggedMode::Out,
            "both" => JaggedMode::Both,
            _ => return Err(p.invalid(KEY, "must be none, in, out, both or an object")),
        };
        *j = Some(Jagged {
            mode,
            ..j.unwrap_or_default()
        });
        return Ok(());
    }
    if !v.is_object() {
        return Err(p.invalid(KEY, "must be {mode, side, amount, seed}, a mode or null"));
    }
    let mut out = j.unwrap_or_default();
    if let Some(m) = v.get("mode") {
        out.mode = match m.as_str() {
            Some("in") => JaggedMode::In,
            Some("out") => JaggedMode::Out,
            Some("both") => JaggedMode::Both,
            _ => return Err(p.invalid(KEY, "mode must be in, out or both")),
        };
    }
    if let Some(m) = v.get("side") {
        out.side = match m.as_str() {
            Some("both") => JaggedSide::Both,
            Some("left") => JaggedSide::Left,
            Some("right") => JaggedSide::Right,
            _ => return Err(p.invalid(KEY, "side must be both, left or right")),
        };
    }
    out.amount = field(p, KEY, v, "amount", out.amount, 0.1, 5.0)?;
    if let Some(x) = v.get("seed") {
        out.seed = x
            .as_u64()
            .ok_or_else(|| p.invalid(KEY, "seed must be a whole number 0 or more"))?;
    }
    *j = Some(out);
    Ok(())
}

fn apply_raised(p: &Params, r: &mut Option<Raised>) -> Result<(), CommandError> {
    const KEY: &str = "raised";
    if p.is_null(KEY) {
        *r = None;
        return Ok(());
    }
    let Some(v) = p.get(KEY) else {
        return Ok(());
    };
    if let Some(on) = v.as_bool() {
        *r = on.then(|| r.unwrap_or_default());
        return Ok(());
    }
    if !v.is_object() {
        return Err(p.invalid(
            KEY,
            "must be {laneSpacing, inset, stitchLength}, true or null",
        ));
    }
    let mut out = r.unwrap_or_default();
    out.lane_spacing = field(p, KEY, v, "laneSpacing", out.lane_spacing, 0.5, 5.0)?;
    out.inset = field(p, KEY, v, "inset", out.inset, 0.0, 3.0)?;
    out.stitch_length = field(p, KEY, v, "stitchLength", out.stitch_length, 1.0, 10.0)?;
    *r = Some(out);
    Ok(())
}

/// The hint for raised satin on a satin narrower than [`RAISED_MIN_WIDTH`] (mm wide).
pub(crate) fn raised_hint(s: &SatinShaping, width_mm: f64) -> Option<String> {
    (s.raised.is_some() && width_mm < RAISED_MIN_WIDTH).then(|| {
        format!(
            "raised satin is meant for columns {RAISED_MIN_WIDTH} mm and wider; this one is {width_mm:.1} mm, so the padding may show"
        )
    })
}

/// Applies `underlayAuto` when present.
pub(crate) fn apply_auto(p: &Params, a: &mut AutoUnderlay) -> Result<(), CommandError> {
    let Some(v) = p.get("underlayAuto") else {
        return Ok(());
    };
    let num = |key: &str, cur: f64| -> Result<f64, CommandError> {
        match v.get(key) {
            None => Ok(cur),
            Some(x) => x
                .as_f64()
                .filter(|x| (0.5..=10.0).contains(x))
                .ok_or_else(|| p.invalid("underlayAuto", format!("{key} must be 0.5-10 mm"))),
        }
    };
    if !v.is_object() {
        return Err(p.invalid("underlayAuto", "must be {centreBelow, edgeBelow}"));
    }
    let (c, e) = (
        num("centreBelow", a.centre_below)?,
        num("edgeBelow", a.edge_below)?,
    );
    if c > e {
        return Err(p.invalid("underlayAuto", "centreBelow must not exceed edgeBelow"));
    }
    *a = AutoUnderlay {
        centre_below: c,
        edge_below: e,
    };
    Ok(())
}

/// Applies `underlay2` when present (`null` removes the layer).
pub(crate) fn apply_underlay2(
    p: &Params,
    u: &mut Option<SecondUnderlay>,
) -> Result<(), CommandError> {
    if p.is_null("underlay2") {
        *u = None;
        return Ok(());
    }
    let Some(v) = p.get("underlay2") else {
        return Ok(());
    };
    if !v.is_object() {
        return Err(p.invalid(
            "underlay2",
            "must be {style, inset, spacing, stitchLength} or null",
        ));
    }
    let mut l = u.unwrap_or_default();
    if let Some(s) = v.get("style") {
        l.style = s
            .as_str()
            .and_then(UnderlayStyle::from_id)
            .filter(|s| {
                matches!(
                    s,
                    UnderlayStyle::Edge
                        | UnderlayStyle::Zigzag
                        | UnderlayStyle::EdgeZigzag
                        | UnderlayStyle::CentreWalk
                        | UnderlayStyle::Auto
                )
            })
            .ok_or_else(|| {
                p.invalid(
                    "underlay2",
                    "style must be edge, zigzag, edgeZigzag, centreWalk or auto",
                )
            })?;
    }
    for (key, field, lo, hi) in [
        ("inset", &mut l.inset, 0.0, 3.0),
        ("spacing", &mut l.spacing, 0.5, 10.0),
        ("stitchLength", &mut l.stitch_length, 1.0, 10.0),
    ] {
        if let Some(x) = v.get(key) {
            *field = x
                .as_f64()
                .filter(|x| (lo..=hi).contains(x))
                .ok_or_else(|| p.invalid("underlay2", format!("{key} must be {lo}-{hi} mm")))?;
        }
    }
    *u = Some(l);
    Ok(())
}
