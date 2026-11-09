//! The satin shaping and under sewing attributes beyond PE-DESIGN (#520 corner styles, #521
//! short stitches, #522 compensation per side and per cent, #523 automatic and second under
//! sewing), read by `object.setRegion` (Manual Punch columns, skeleton satin and Auto satin
//! regions, #675) and `object.setLine` (zigzag lines). All are off by default; a column or line without them sews as PE-DESIGN's.

use crate::commands::{CommandSpec, Effect, ParamDoc, opt, req, spec};
use crate::params::{CommandError, Params};
use openstitch_design::column::{
    AutoUnderlay, CornerStyle, Jagged, JaggedMode, JaggedSide, RAISED_MIN_WIDTH, Raised,
    SatinShaping, SatinStyle, SecondUnderlay,
};
use openstitch_design::skeleton::{self, WidthKey};
use openstitch_design::{LineSewing, LineType, UnderlayStyle};
use serde_json::json;

/// The parameters shared by `object.setRegion` and `object.setLine`.
pub(crate) const SHAPING_PARAMS: [ParamDoc; 16] = [
    opt(
        "cornerStyle",
        "turn|auto|mitre|cap",
        "Satin corners (beyond PE-DESIGN; Manual Punch columns, skeleton satin, Auto satin regions and zigzag lines; on satin text at the corners where two strokes are sewn one after the other (Z, V, W; not at stems or junctions); on skeleton and Auto satin at the centre line's points): turn (default, PE-DESIGN's fan), auto (cap below capBelow, mitre below mitreBelow), mitre (a seam from the inner corner to the tip, each leg's stitches square to it) or cap (both legs end on a square line through the inner corner, the tip sewn as its own piece over a hidden run)",
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
        "Short stitches on the inside of curves (beyond PE-DESIGN; also satin text): a needle point closer than this to the last full one on its edge stops short, 0-1 (0 = off, the default; 0.25 suits most satins)",
    ),
    opt(
        "shortStitchInset",
        "percent",
        "How far a short stitch stops short, % of the stitch's width, 5-80 (default 15)",
    ),
    opt(
        "pullLeft",
        "mm",
        "Compensation of the left side (the first stitch's start rail), 0-2; null returns it to pullCompensation (columns, skeleton and Auto satin) or 0 (zigzag lines)",
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
        "Satin look (beyond PE-DESIGN; every satin: fixed-direction satin regions, Manual Punch columns, skeleton satin, Auto satin regions, satin text and zigzag lines): normal (default), sStitch (each crossing three short stitches bent into an S, a corded edge), open (rows three times as far apart) or double (sewn twice, the second pass back from the end midway between the first's needle points: an X lattice)",
    ),
    opt(
        "jaggedEdge",
        "{mode: in|out|both, side: both|left|right, amount, seed} | in|out|both|none | null",
        "Jagged edge for fur, grass and feathers (beyond PE-DESIGN; every satin: fixed-direction satin regions, Manual Punch columns, skeleton and Auto satin, satin text and zigzag lines): needle points on the chosen edges move up to amount mm (0.1-5, default 1) into the column, out, or both, the same for the same seed (default 1); side left is the first stitch's start rail; null removes it",
    ),
    opt(
        "spacingAt",
        "0-1",
        "Manual Punch columns, skeleton satin and Auto satin regions (not fixed-direction satin): where across a curved column the density is measured, 0 (shorter side) to 1 (longer side); 0.5 evens it at the centre. Unset keeps each kind's own: the longer side on columns and Auto satin (PE-DESIGN's, = 1), the centre line on skeleton satin (= 0.5)",
    ),
    opt(
        "raised",
        "{laneSpacing, inset, stitchLength} | true | null",
        "Raised (padded) satin (beyond PE-DESIGN; Manual Punch columns, skeleton satin and Auto satin along their column, fixed-direction satin regions under each section, satin text under each stroke, zigzag lines): running-stitch lanes along the column under the satin, about one per laneSpacing mm of width (0.5-5, default 1.5; an even number, at least two), inset mm from the edges (0-3, default 0.5), stitchLength mm (1-10, default 2.5); true turns it on (with its earlier settings, else the defaults), null removes it. Meant for columns of 3 mm and wider (the result gives a hint below that)",
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
    "{style, inset, spacing, stitchLength, angle} | null",
    "A second under sewing layer after the first (beyond PE-DESIGN), on a Manual Punch column, skeleton satin or Auto satin region (along the column), or on a fill or fixed-direction satin region (rows at angle degrees from the fill angle, -180-180, default 0: square to the first fill under sewing layer, whose rows run at 90°, i.e. crossed tatami; edge: the outline run inset in): style edge|zigzag|edgeZigzag|centreWalk|auto (default zigzag; auto on skeleton and Auto satin: rows that follow the width), inset mm 0-3 (0.5), spacing mm 0.5-10 (2), stitchLength mm 1-10 (2.5); null removes it",
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
            "must be {style, inset, spacing, stitchLength, angle} or null",
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
        ("angle", &mut l.angle, -180.0, 180.0),
    ] {
        if let Some(x) = v.get(key) {
            *field = x
                .as_f64()
                .filter(|x| (lo..=hi).contains(x))
                .ok_or_else(|| p.invalid("underlay2", format!("{key} must be {lo}-{hi}")))?;
        }
    }
    *u = Some(l);
    Ok(())
}

/// The satin line's parameters of `object.setLine` (#524).
pub(crate) const SATIN_LINE_PARAMS: [ParamDoc; 4] = [
    opt(
        "widthKeys",
        "[[at, left, right]] | [{at, left, right}] | null",
        "Satin line (type satin, beyond PE-DESIGN): the column's width along the line, at = 0 (start) to 1 (end), left and right mm from the line (0-15; left is to the left of the drawing direction); widths change smoothly between keys; null or [] uses width all along",
    ),
    opt(
        "taperStart",
        "mm",
        "Satin line: the start tapers to a point over this length, 0-50 (0 = none)",
    ),
    opt(
        "taperEnd",
        "mm",
        "Satin line: the end tapers to a point over this length, 0-50 (0 = none)",
    ),
    opt(
        "slant",
        "degrees",
        "Satin line: the crossings turned from square to the line, -60 to 60 (0 = square, the default), a calligraphic slant",
    ),
];

/// One width key from `[at, left, right]` or `{at, left, right}`.
fn width_key(p: &Params, key: &str, v: &serde_json::Value) -> Result<WidthKey, CommandError> {
    let get = |i: usize, name: &str| {
        v.get(i)
            .or_else(|| v.get(name))
            .and_then(serde_json::Value::as_f64)
    };
    let (Some(at), Some(left), Some(right)) = (get(0, "at"), get(1, "left"), get(2, "right"))
    else {
        return Err(p.invalid(key, "each key is [at, left, right] or {at, left, right}"));
    };
    if !(0.0..=1.0).contains(&at) {
        return Err(p.invalid(key, "at must be 0-1"));
    }
    let half = skeleton::MAX_WIDTH / 2.0;
    if !(0.0..=half).contains(&left) || !(0.0..=half).contains(&right) {
        return Err(p.invalid(key, format!("left and right must be 0-{half} mm")));
    }
    Ok(WidthKey { at, left, right })
}

/// Applies the satin line's parameters present in `p` to `l`.
pub(crate) fn apply_satin_line(p: &Params, l: &mut LineSewing) -> Result<(), CommandError> {
    if p.is_null("widthKeys") {
        l.width_keys.clear();
    } else if p.has("widthKeys") {
        let keys = p
            .array("widthKeys")?
            .iter()
            .map(|v| width_key(p, "widthKeys", v))
            .collect::<Result<Vec<_>, _>>()?;
        if keys.len() > 1000 {
            return Err(p.invalid("widthKeys", "at most 1000 keys"));
        }
        l.width_keys = keys;
        l.width_keys.sort_by(|a, b| a.at.total_cmp(&b.at));
    }
    for (key, field) in [
        ("taperStart", &mut l.taper_start),
        ("taperEnd", &mut l.taper_end),
    ] {
        if p.has(key) {
            let v = p.req(key, 0.0, 50.0)?;
            *field = (v > 0.0).then_some(v);
        }
    }
    if p.has("slant") {
        let v = p.req("slant", -60.0, 60.0)?;
        l.slant = (v != 0.0).then_some(v);
    }
    Ok(())
}

/// The satin line `id` to change.
fn satin_line<'a>(
    s: &'a mut crate::session::EngineState,
    p: &Params,
) -> Result<&'a mut LineSewing, CommandError> {
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    s.design.objects[i]
        .line
        .as_mut()
        .filter(|l| l.kind == LineType::Satin)
        .ok_or_else(|| {
            p.failed(format!(
                "object {id} has no satin line (object.setLine type satin)"
            ))
        })
}

/// `object.setWidthKey` and `object.deleteWidthKey` (#524).
pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "object.setWidthKey",
            "Satin line: set the column's width at a place along the line (beyond PE-DESIGN): a key at `at` (0 = start, 1 = end) with `width` (both sides) or `left` and `right` mm; a key already within 0.5 % of `at` is replaced. Returns the key's index",
            Effect::Edit,
            vec![
                req("id", "int", "Object with a satin line"),
                req("at", "0-1", "Where along the line: 0 = start, 1 = end"),
                opt("width", "mm", "Full width, split evenly (0-30)"),
                opt("left", "mm", "Reach to the left of the line (0-15)"),
                opt("right", "mm", "Reach to the right of the line (0-15)"),
            ],
            |s, p| {
                let at = p.req("at", 0.0, 1.0)?;
                let half = skeleton::MAX_WIDTH / 2.0;
                let (left, right) = if p.has("width") {
                    if p.has("left") || p.has("right") {
                        return Err(p.invalid("width", "give width or left and right, not both"));
                    }
                    let w = p.req("width", 0.0, skeleton::MAX_WIDTH)? / 2.0;
                    (w, w)
                } else if p.has("left") || p.has("right") {
                    // The side not given keeps the replaced key's, or the width there now.
                    let l = satin_line(s, p)?;
                    let here = match l.width_keys.iter().find(|k| (k.at - at).abs() <= 0.005) {
                        Some(k) => [k.left, k.right],
                        None => skeleton::SatinLine {
                            width: l.width,
                            keys: l.width_keys.clone(),
                            ..Default::default()
                        }
                        .sides(at, 1.0),
                    };
                    (
                        p.opt("left", here[0], 0.0, half)?,
                        p.opt("right", here[1], 0.0, half)?,
                    )
                } else {
                    return Err(p.invalid("width", "give width, or left and/or right"));
                };
                let l = satin_line(s, p)?;
                let key = WidthKey { at, left, right };
                let i = match l.width_keys.iter().position(|k| (k.at - at).abs() <= 0.005) {
                    Some(i) => {
                        l.width_keys[i] = key;
                        i
                    }
                    None => {
                        let i = l.width_keys.partition_point(|k| k.at < at);
                        l.width_keys.insert(i, key);
                        i
                    }
                };
                Ok(json!({ "index": i }))
            },
        ),
        spec(
            "object.deleteWidthKey",
            "Satin line: remove width key `index` (0 = nearest the start); with none left the line is `width` all along (beyond PE-DESIGN)",
            Effect::Edit,
            vec![
                req("id", "int", "Object with a satin line"),
                req("index", "int", "Width key, 0 = nearest the start"),
            ],
            |s, p| {
                let l = satin_line(s, p)?;
                if l.width_keys.is_empty() {
                    return Err(p.failed("the satin line has no width keys"));
                }
                let i = p.int("index", None, 0, l.width_keys.len() as i64 - 1)? as usize;
                l.width_keys.remove(i);
                Ok(json!(null))
            },
        ),
    ]
}
