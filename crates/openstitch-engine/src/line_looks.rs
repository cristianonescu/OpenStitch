//! The hand-made line looks beyond PE-DESIGN (#540, #542), read by `object.setLine` (running
//! passes along a thickness profile, the hand-stitch look, the scribble's waves and seed) and
//! `object.setRegion` (the hand-stitch look on one side of a satin). All are off by default.

use crate::commands::{ParamDoc, opt};
use crate::params::{CommandError, Params};
use openstitch_design::LineType;
use openstitch_design::line_looks::{ProfilePreset, SatinHand, SatinSide};

/// `object.setLine`'s parameters for these looks.
pub(crate) const LINE_PARAMS: [ParamDoc; 14] = [
    opt(
        "profile",
        "constant|linear|arc|sine|smoothstep|stairs|none|null",
        "Running (beyond PE-DESIGN, #542): sew each stitch back and forth minPasses times where the profile is 0 and maxPasses where it is 1, passes side by side, so the line swells like a brush stroke (arc: thick in the middle; linear: thin to thick; stairs: in profileSteps steps). null: plain running stitch",
    ),
    opt(
        "profilePoints",
        "[[t, value]]",
        "A custom profile instead of the preset: 2-32 points, t and value 0-1, t increasing",
    ),
    opt(
        "profileSteps",
        "int 2-10",
        "Stairs profile: steps (default 4)",
    ),
    opt(
        "minPasses",
        "odd int 1-9",
        "Profile: passes where the profile is 0 (default 1)",
    ),
    opt(
        "maxPasses",
        "odd int 1-9",
        "Profile: passes where the profile is 1 (default 3)",
    ),
    opt(
        "passSpacing",
        "mm",
        "Profile: distance between neighbouring passes, 0.1-1 (default 0.25)",
    ),
    opt(
        "hand",
        "bool",
        "Hand-stitch look on running and triple lines (beyond PE-DESIGN, #540): stitch lengths and tilts vary smoothly, now and then a stitch is sewn again. Off: PE-DESIGN's even stitches",
    ),
    opt(
        "handLength",
        "%",
        "Hand-stitch look: stitch lengths vary up to this much either way, 0-50 (default 25); stitches stay 0.5-7 mm",
    ),
    opt(
        "handAngle",
        "degrees",
        "Hand-stitch look: needle points tilt off the line up to this, 0-20 (default 6)",
    ),
    opt(
        "handPasses",
        "int 1-3",
        "Hand-stitch look on running lines: passes back and forth (default 1; triple lines sew 3)",
    ),
    opt(
        "handExtra",
        "%",
        "Hand-stitch look: chance that a stitch is sewn again, 0-50 (default 10)",
    ),
    opt(
        "noiseScale",
        "mm",
        "Scribble: length of its smooth side waves, 1-50 (default 5)",
    ),
    opt(
        "seed",
        "int",
        "Hand-stitch look and scribble: the variant, 0-1000000 (the same seed sews the same stitches)",
    ),
    opt(
        "shuffle",
        "bool",
        "Hand-stitch look and scribble: pick another variant (a new seed), one undo step",
    ),
];

/// `object.setRegion`'s parameters for the satin hand-stitch look.
pub(crate) const REGION_PARAMS: [ParamDoc; 4] = [
    opt(
        "handSide",
        "a|b|both|none",
        "Satin (beyond PE-DESIGN, #540): the hand-stitch look on one side: its needle points wander in towards the other side, which stays crisp (a: the first stitch's side). none: off",
    ),
    opt(
        "handAmount",
        "%",
        "Satin hand-stitch look: how far a needle point may move in, % of the satin's width, 0-50 (default 25)",
    ),
    opt(
        "seed",
        "int",
        "Satin hand-stitch look: the variant, 0-1000000",
    ),
    opt(
        "shuffle",
        "bool",
        "Satin hand-stitch look: pick another variant",
    ),
];

/// The next variant after `seed`.
fn shuffled(seed: u32) -> u32 {
    seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) % 1_000_001
}

fn seed(p: &Params, old: u32) -> Result<u32, CommandError> {
    let s = p.int("seed", Some(i64::from(old)), 0, 1_000_000)? as u32;
    Ok(if p.bool("shuffle", Some(false))? {
        shuffled(s)
    } else {
        s
    })
}

fn odd(p: &Params, key: &str, old: u32) -> Result<u32, CommandError> {
    let n = p.int(key, Some(i64::from(old)), 1, 9)? as u32;
    if n.is_multiple_of(2) {
        return Err(p.invalid(key, "must be odd, so the line ends at its far end"));
    }
    Ok(n)
}

/// Applies the line look parameters of `p` to `l` (after its type is set).
pub(crate) fn apply_line(
    p: &Params,
    l: &mut openstitch_design::LineSewing,
) -> Result<(), CommandError> {
    // A line has a thickness profile or the hand-stitch look: turning one on turns the other
    // off; asking for both at once is refused.
    let profile_on = p.has("profile") && !p.string("profile", None).is_ok_and(|v| v == "none");
    let hand_on = p.bool("hand", Some(false))?;
    if profile_on && hand_on {
        return Err(p.invalid(
            "hand",
            "a line has a thickness profile or the hand-stitch look, not both",
        ));
    }
    if profile_on {
        l.hand = None;
    }
    if hand_on {
        l.profile = None;
    }
    let profile_given = [
        "profilePoints",
        "profileSteps",
        "minPasses",
        "maxPasses",
        "passSpacing",
    ]
    .iter()
    .any(|k| p.has(k));
    if p.is_null("profile")
        || (p.has("profile") && p.string("profile", None).is_ok_and(|v| v == "none"))
    {
        l.profile = None;
    } else if p.has("profile") || profile_given {
        if l.kind != LineType::Running {
            return Err(p.invalid("profile", "needs a running line"));
        }
        let mut r = l.profile.clone().unwrap_or_default();
        if p.has("profile") {
            let id = p.string("profile", None)?;
            r.profile.preset = ProfilePreset::from_id(&id).ok_or_else(|| {
                p.invalid(
                    "profile",
                    "must be constant, linear, arc, sine, smoothstep, stairs or null",
                )
            })?;
            r.profile.points.clear();
        }
        r.profile.steps = p.int("profileSteps", Some(i64::from(r.profile.steps)), 2, 10)? as u32;
        if p.has("profilePoints") {
            r.profile.points = points(p)?;
        }
        r.min_passes = odd(p, "minPasses", r.min_passes)?;
        r.max_passes = odd(p, "maxPasses", r.max_passes)?;
        if r.max_passes < r.min_passes {
            return Err(p.invalid("maxPasses", "must not be below minPasses"));
        }
        r.pass_spacing = p.opt("passSpacing", r.pass_spacing, 0.1, 1.0)?;
        l.profile = Some(r);
    }
    let hand_given = ["handLength", "handAngle", "handPasses", "handExtra"]
        .iter()
        .any(|k| p.has(k));
    let on = p.bool("hand", Some(l.hand.is_some() || hand_given))?;
    if !on {
        l.hand = None;
    } else {
        if !matches!(l.kind, LineType::Running | LineType::Triple) {
            return Err(p.invalid("hand", "needs a running or triple line"));
        }
        let mut h = l.hand.clone().unwrap_or_default();
        h.length_jitter = p.opt("handLength", h.length_jitter, 0.0, 50.0)?;
        h.angle_jitter = p.opt("handAngle", h.angle_jitter, 0.0, 20.0)?;
        h.passes = p.int("handPasses", Some(i64::from(h.passes)), 1, 3)? as u32;
        h.extra_passes = p.opt("handExtra", h.extra_passes, 0.0, 50.0)?;
        h.seed = seed(p, h.seed)?;
        l.hand = Some(h);
    }
    if l.kind == LineType::Scribble {
        if p.has("noiseScale") {
            l.noise_scale = Some(p.req("noiseScale", 1.0, 50.0)?);
        }
        if p.has("seed") || p.has("shuffle") {
            l.seed = Some(seed(p, l.seed.unwrap_or(0))?);
        }
    } else if l.hand.is_none() && (p.has("seed") || p.has("shuffle")) {
        return Err(p.invalid("seed", "needs the hand-stitch look or a scribble line"));
    }
    Ok(())
}

fn points(p: &Params) -> Result<Vec<[f64; 2]>, CommandError> {
    let arr = p.array("profilePoints")?;
    if !(2..=32).contains(&arr.len()) {
        return Err(p.invalid("profilePoints", "needs 2-32 points"));
    }
    let mut out: Vec<[f64; 2]> = Vec::with_capacity(arr.len());
    for v in arr {
        let xy = v.as_array().filter(|a| a.len() == 2);
        let Some((t, y)) = xy.and_then(|a| a[0].as_f64().zip(a[1].as_f64())) else {
            return Err(p.invalid("profilePoints", "each point must be [t, value]"));
        };
        if !(0.0..=1.0).contains(&t) || !(0.0..=1.0).contains(&y) {
            return Err(p.invalid("profilePoints", "t and value must be 0-1"));
        }
        if out.last().is_some_and(|q| t <= q[0]) {
            return Err(p.invalid("profilePoints", "t must increase"));
        }
        out.push([t, y]);
    }
    Ok(out)
}

/// Applies the satin hand-stitch look parameters of `p` to `hand` (a satin region's).
pub(crate) fn apply_region(
    p: &Params,
    satin: bool,
    hand: &mut Option<SatinHand>,
) -> Result<(), CommandError> {
    if p.has("handSide") {
        let id = p.string("handSide", None)?;
        let side = match id.as_str() {
            "none" => None,
            "a" => Some(SatinSide::A),
            "b" => Some(SatinSide::B),
            "both" => Some(SatinSide::Both),
            _ => return Err(p.invalid("handSide", "must be a, b, both or none")),
        };
        match side {
            None => *hand = None,
            Some(s) => {
                if !satin {
                    return Err(p.invalid("handSide", "needs a satin region"));
                }
                hand.get_or_insert(SatinHand {
                    side: s,
                    amount: 25.0,
                    seed: 1,
                })
                .side = s;
            }
        }
    }
    if let Some(h) = hand.as_mut() {
        h.amount = p.opt("handAmount", h.amount, 0.0, 50.0)?;
        h.seed = seed(p, h.seed)?;
    } else if p.has("handAmount") {
        return Err(p.invalid("handAmount", "needs handSide"));
    }
    Ok(())
}
