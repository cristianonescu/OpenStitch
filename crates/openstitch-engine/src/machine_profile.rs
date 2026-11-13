//! The user's machine (#508 part 3, beyond PE-DESIGN): brand, hoop, whether it cuts threads,
//! and the longest stitch it sews, kept in the user settings and set once by the first-run
//! machine setup.
//!
//! A profile holds only what the user states; OpenStitch adds no per-model numbers of its own
//! (makers publish them in each machine's manual, and they differ by model). It changes three
//! things, and nothing about the files written:
//! - `design.new` without a size starts on the profile's hoop;
//! - every export adds warnings: stitches longer than the machine's longest, and trims a
//!   machine without a thread cutter will not cut (unless Stop at Jumps is on);
//! - `machine.profile` names the brand's own file format, for the export a client offers first.
//!
//! With no profile, everything is as before (Brother, PE-DESIGN's behaviour).

use crate::commands::{CommandSpec, Effect, opt, req, spec};
use crate::session::EngineState;
use openstitch_core::{Command, StitchPattern};
use openstitch_design::Design;
use openstitch_design::machine::{BRANDS, find_hoop};
use serde_json::{Value, json};

/// The machine file format each brand's home machines read natively.
pub const BRAND_FORMATS: [(&str, &str); 9] = [
    ("Brother", "pes"),
    ("Baby Lock", "pes"),
    ("Bernina", "exp"),
    ("Elna", "jef"),
    ("Husqvarna Viking", "vp3"),
    ("Janome", "jef"),
    ("Pfaff", "vp3"),
    ("Singer", "xxx"),
    ("Commercial round", "dst"),
];

/// The user's machine.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MachineProfile {
    pub brand: String,
    /// A hoop id of that brand (`machine.hoops`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hoop: Option<String>,
    /// Whether the machine cuts threads at trims.
    #[serde(default)]
    pub cutter: bool,
    /// The longest stitch the machine sews, mm, from its manual.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_stitch_mm: Option<f64>,
    /// Needles on a multi-needle machine (1–15); unset on a single-needle machine. Check
    /// Design notes designs with more thread colours (#866).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub needles: Option<usize>,
}

pub fn format_of(brand: &str) -> &'static str {
    BRAND_FORMATS
        .iter()
        .find(|(b, _)| *b == brand)
        .map_or("pes", |(_, f)| f)
}

pub fn profile_json(p: Option<&MachineProfile>) -> Value {
    match p {
        None => Value::Null,
        Some(p) => json!({
            "brand": p.brand,
            "hoop": p.hoop,
            "cutter": p.cutter,
            "maxStitchMm": p.max_stitch_mm,
            "needles": p.needles,
            "format": format_of(&p.brand),
        }),
    }
}

/// The hoop `design.new` starts on: the profile's, when it is still listed.
pub fn new_design_hoop(s: &EngineState) -> Option<&'static str> {
    let id = s.user.machine_profile.as_ref()?.hoop.as_deref()?;
    find_hoop(id).map(|h| h.id)
}

/// What an export adds for the user's machine; empty without a profile.
pub fn export_warnings(
    d: &Design,
    pattern: &StitchPattern,
    p: Option<&MachineProfile>,
) -> Vec<String> {
    let Some(p) = p else {
        return Vec::new();
    };
    let mut out = Vec::new();
    if let Some(max) = p.max_stitch_mm {
        let (mut long, mut last) = (0usize, None::<(f64, f64)>);
        for st in &pattern.stitches {
            match st.command {
                Command::Stitch | Command::Jump => {
                    if st.command == Command::Stitch
                        && let Some((x, y)) = last
                        && ((st.x - x).hypot(st.y - y)) / 10.0 > max + 1e-9
                    {
                        long += 1;
                    }
                    last = Some((st.x, st.y));
                }
                _ => {}
            }
        }
        if long > 0 {
            out.push(format!(
                "{long} stitch(es) are longer than {max} mm, the longest your {} machine sews (machine.profile): shorten the stitch length of those patterns, or the machine may refuse the file or sew them loose.",
                p.brand
            ));
        }
    }
    let trims = pattern.count(Command::Trim);
    if !p.cutter && trims > 0 && !d.production.stop_at_jumps {
        out.push(format!(
            "Your {} machine has no thread cutter (machine.profile): the design's {trims} trim(s) are not cut, so cut those threads by hand, or turn on Stop at Jumps (design.setStopAtJumps) to stop the machine at each.",
            p.brand
        ));
    }
    out
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "machine.profile",
            "The user's machine (user settings): brand, hoop, thread cutter, longest stitch and the brand's own file format; null when none is set (first-run machine setup, #508)",
            Effect::Read,
            vec![],
            |s, _| {
                Ok(
                    json!({ "profile": profile_json(s.user.machine_profile.as_ref()), "brands": brands() }),
                )
            },
        ),
        spec(
            "machine.setProfile",
            "Set the user's machine (user settings, beyond PE-DESIGN): new designs start on its hoop, and exports warn about stitches longer than it sews and trims it cannot cut; brand none clears it",
            Effect::Settings,
            vec![
                req(
                    "brand",
                    "string",
                    "Brother, Baby Lock, Bernina, Elna, Husqvarna Viking, Janome, Pfaff, Singer, Commercial round, or none",
                ),
                opt(
                    "hoop",
                    "string",
                    "A hoop id of that brand from machine.hoops",
                ),
                opt(
                    "cutter",
                    "bool",
                    "Whether the machine cuts threads at trims (default false)",
                ),
                opt(
                    "maxStitchMm",
                    "mm",
                    "The longest stitch the machine sews, from its manual (1–15 mm)",
                ),
                opt(
                    "needles",
                    "int",
                    "Needles on a multi-needle machine, 1–15 (leave it out for a single-needle machine): design.check notes designs with more thread colours (moreColoursThanNeedles)",
                ),
            ],
            |s, p| {
                let brand = p.string("brand", None)?;
                if brand == "none" {
                    s.user.machine_profile = None;
                    return Ok(json!({ "profile": Value::Null }));
                }
                let brand = BRANDS
                    .iter()
                    .find(|b| b.eq_ignore_ascii_case(&brand))
                    .ok_or_else(|| {
                        p.invalid("brand", format!("takes {} or none", BRANDS.join(", ")))
                    })?;
                let hoop = if p.has("hoop") {
                    let id = p.string("hoop", None)?;
                    let h = find_hoop(&id)
                        .filter(|h| h.brand == *brand)
                        .ok_or_else(|| {
                            p.invalid("hoop", format!("is not a {brand} hoop from machine.hoops"))
                        })?;
                    Some(h.id.to_string())
                } else {
                    None
                };
                let max_stitch_mm = if p.has("maxStitchMm") {
                    Some(p.req("maxStitchMm", 1.0, 15.0)?)
                } else {
                    None
                };
                let needles = if p.has("needles") {
                    let n = p.req("needles", 1.0, 15.0)?;
                    if n.fract() != 0.0 {
                        return Err(p.invalid("needles", "must be a whole number"));
                    }
                    Some(n as usize)
                } else {
                    None
                };
                let profile = MachineProfile {
                    brand: brand.to_string(),
                    hoop,
                    cutter: p.bool("cutter", Some(false))?,
                    max_stitch_mm,
                    needles,
                };
                s.user.machine_profile = Some(profile);
                Ok(json!({ "profile": profile_json(s.user.machine_profile.as_ref()) }))
            },
        ),
    ]
}

fn brands() -> Value {
    json!(
        BRAND_FORMATS
            .iter()
            .map(|(b, f)| json!({ "brand": b, "format": f }))
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_brand_has_a_writable_format() {
        assert_eq!(BRAND_FORMATS.len(), BRANDS.len());
        for b in BRANDS {
            let f = format_of(b);
            assert!(
                BRAND_FORMATS.iter().any(|(x, _)| *x == b),
                "{b} has no format"
            );
            assert!(
                ["pes", "exp", "jef", "vp3", "xxx", "dst"].contains(&f),
                "{b}: {f}"
            );
        }
    }
}
