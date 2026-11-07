//! Fabric Selector (Application menu ▸ Fabric Selector): sewing attributes suited to a kind of
//! fabric, applied to shape and text patterns.
//!
//! The guide ("Using Fabric Selector") describes the dialog (pick a fabric, read a description
//! and advice, optionally apply to every shape and text pattern) but not its numbers. The values
//! below are OpenStitch's own approximations from common machine-embroidery practice: denser
//! fill and satin with heavier underlay and more pull compensation on stretchy or lofty fabrics,
//! lighter settings on thin and firm ones, longer stitches and no underlay on leather and vinyl
//! (every needle hole stays). They are a starting point; sew a test first.

use crate::commands::{CommandSpec, Effect, indexes, opt, req, spec};
use crate::params::Params;
use crate::session::EngineState;
use openstitch_design::column::AutoUnderlay;
use openstitch_design::{ObjectKind, RegionType, UnderlayStyle};
use serde_json::{Value, json};

/// One fabric of the selector and the attributes it recommends.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fabric {
    pub id: &'static str,
    pub name: &'static str,
    pub description: &'static str,
    pub advice: &'static str,
    /// Rows per mm of fill (and programmable fill) regions.
    pub fill_density: f64,
    /// Rows per mm of satin regions.
    pub satin_density: f64,
    /// Longest fill stitch, mm.
    pub stitch_length: f64,
    /// Row end extension, mm.
    pub pull_compensation: f64,
    /// `None` = no underlay.
    pub underlay: Option<UnderlayStyle>,
    /// Width thresholds of automatic under sewing (#523): stretchy and piled fabrics switch to
    /// the stronger under sewing on narrower columns.
    pub auto_underlay: AutoUnderlay,
}

/// Automatic under sewing on firm fabrics: a centre walk below 2 mm, the edge run to 4 mm.
const AUTO_FIRM: AutoUnderlay = AutoUnderlay {
    centre_below: 2.0,
    edge_below: 4.0,
};
/// On knits, fleece and towels: a centre walk below 1.5 mm, the edge run to 3 mm.
const AUTO_STRETCH: AutoUnderlay = AutoUnderlay {
    centre_below: 1.5,
    edge_below: 3.0,
};

pub const FABRICS: &[Fabric] = &[
    Fabric {
        id: "standard",
        name: "Standard woven (cotton, broadcloth)",
        description: "Medium-weight woven fabric that does not stretch.",
        advice: "Hoop with a tear-away stabiliser. These are OpenStitch's ordinary defaults with an edge underlay.",
        fill_density: 2.5,
        satin_density: 3.0,
        stitch_length: 4.0,
        pull_compensation: 0.2,
        underlay: Some(UnderlayStyle::Edge),
        auto_underlay: AUTO_FIRM,
    },
    Fabric {
        id: "thin",
        name: "Thin woven (linen, poplin, silk)",
        description: "Light, fine woven fabric that puckers under dense stitching.",
        advice: "Use a light cut-away or wash-away stabiliser and a fine needle; keep designs light.",
        fill_density: 2.2,
        satin_density: 2.8,
        stitch_length: 3.5,
        pull_compensation: 0.1,
        underlay: Some(UnderlayStyle::Edge),
        auto_underlay: AUTO_FIRM,
    },
    Fabric {
        id: "heavy",
        name: "Heavy woven (denim, canvas, twill)",
        description: "Thick, firm woven fabric with a visible weave.",
        advice: "Use a sharp or denim needle and a tear-away stabiliser; extra underlay keeps the weave from showing through.",
        fill_density: 2.8,
        satin_density: 3.4,
        stitch_length: 4.0,
        pull_compensation: 0.3,
        underlay: Some(UnderlayStyle::EdgeZigzag),
        auto_underlay: AUTO_FIRM,
    },
    Fabric {
        id: "knit",
        name: "Knit (T-shirt, jersey, polo)",
        description: "Stretchy knitted fabric that pulls in under stitching.",
        advice: "Use a ballpoint needle and a cut-away stabiliser; do not stretch the fabric when hooping.",
        fill_density: 2.4,
        satin_density: 3.0,
        stitch_length: 3.5,
        pull_compensation: 0.4,
        underlay: Some(UnderlayStyle::EdgeZigzag),
        auto_underlay: AUTO_STRETCH,
    },
    Fabric {
        id: "fleece",
        name: "Fleece, sweatshirt",
        description: "Soft, thick knit with a pile that stitches sink into.",
        advice: "Use a cut-away stabiliser below and a wash-away topping above so stitches stay on the surface.",
        fill_density: 2.8,
        satin_density: 3.4,
        stitch_length: 4.0,
        pull_compensation: 0.4,
        underlay: Some(UnderlayStyle::Double),
        auto_underlay: AUTO_STRETCH,
    },
    Fabric {
        id: "towel",
        name: "Towel (terry cloth)",
        description: "Looped pile fabric; loops poke through sparse stitching.",
        advice: "Use a wash-away topping and a tear-away stabiliser; avoid very small lettering.",
        fill_density: 3.0,
        satin_density: 3.6,
        stitch_length: 4.0,
        pull_compensation: 0.4,
        underlay: Some(UnderlayStyle::Double),
        auto_underlay: AUTO_STRETCH,
    },
    Fabric {
        id: "felt",
        name: "Felt",
        description: "Non-woven, firm fabric that neither frays nor stretches.",
        advice: "A light tear-away stabiliser is enough; felt suits appliqué and patches.",
        fill_density: 2.3,
        satin_density: 2.8,
        stitch_length: 4.0,
        pull_compensation: 0.1,
        underlay: Some(UnderlayStyle::Edge),
        auto_underlay: AUTO_FIRM,
    },
    Fabric {
        id: "leather",
        name: "Leather, vinyl",
        description: "Every needle hole stays; dense stitching can cut the material.",
        advice: "Use a leather needle and a cut-away stabiliser; tape instead of hooping where possible.",
        fill_density: 2.0,
        satin_density: 2.5,
        stitch_length: 5.0,
        pull_compensation: 0.0,
        underlay: None,
        auto_underlay: AUTO_FIRM,
    },
    // Beyond PE-DESIGN (#508): five more fabrics after PE-DESIGN's eight, with the same kind of
    // approximations (no maker publishes density or pull compensation per fabric); their
    // backing, topping and needle advice is sourced (data/fabric_advice.json).
    Fabric {
        id: "polyester",
        name: "Polyester woven (sportswear, linings)",
        description: "Smooth, often slippery woven polyester that puckers if the stitches pull.",
        advice: "Stabilise firmly and keep the hoop tight; an edge and zigzag underlay holds the fabric.",
        fill_density: 2.5,
        satin_density: 3.0,
        stitch_length: 3.5,
        pull_compensation: 0.25,
        underlay: Some(UnderlayStyle::EdgeZigzag),
        auto_underlay: AUTO_FIRM,
    },
    Fabric {
        id: "pique",
        name: "Piqué knit (polo shirts)",
        description: "A textured knit with a honeycomb surface; it stretches and the stitches sink into it.",
        advice: "Treat it as a knit: a soft cut-away below and a film topping over the texture.",
        fill_density: 2.3,
        satin_density: 3.0,
        stitch_length: 3.5,
        pull_compensation: 0.4,
        underlay: Some(UnderlayStyle::EdgeZigzag),
        auto_underlay: AUTO_STRETCH,
    },
    Fabric {
        id: "organza",
        name: "Organza and sheers",
        description: "A see-through fabric: the back of the design and any stabiliser left behind show.",
        advice: "Use a wash-away stabiliser and a light design; avoid heavy fills.",
        fill_density: 2.0,
        satin_density: 2.6,
        stitch_length: 3.0,
        pull_compensation: 0.1,
        underlay: Some(UnderlayStyle::Edge),
        auto_underlay: AUTO_FIRM,
    },
    Fabric {
        id: "canvas",
        name: "Canvas, duck (bags)",
        description: "A heavy, firm woven fabric that holds dense stitching well.",
        advice: "Use a strong needle and slow the machine on thick seams.",
        fill_density: 2.8,
        satin_density: 3.4,
        stitch_length: 4.0,
        pull_compensation: 0.2,
        underlay: Some(UnderlayStyle::EdgeZigzag),
        auto_underlay: AUTO_FIRM,
    },
    Fabric {
        id: "cap",
        name: "Caps (structured)",
        description: "A stiff, curved front panel sewn on a cap frame, often across a centre seam.",
        advice: "Sew from the centre outwards and bottom to top; an edge and zigzag underlay keeps the design on the curve.",
        fill_density: 2.8,
        satin_density: 3.4,
        stitch_length: 4.0,
        pull_compensation: 0.3,
        underlay: Some(UnderlayStyle::EdgeZigzag),
        auto_underlay: AUTO_FIRM,
    },
];

/// PE-DESIGN's Fabric Selector fabrics: the first eight of [`FABRICS`].
pub const PE_FABRICS: usize = 8;

// MARK: advice (#508)

/// One maker source of the advice.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct Source {
    pub id: String,
    pub maker: String,
    pub title: String,
    pub url: String,
}

/// The sourced backing, topping and needle advice for one fabric.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FabricAdvice {
    pub backing: String,
    pub topping: String,
    pub needle_point: String,
    /// The smallest needle a maker gives for the fabric (metric), if any.
    pub needle_min: Option<u32>,
    #[serde(default)]
    pub disagree: Option<String>,
    pub sources: Vec<String>,
}

/// Needle and smallest text for one thread weight.
#[derive(Clone, Debug, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ThreadAdvice {
    /// Needle size (metric).
    pub needle: u32,
    /// The maker's stitch spacing for the weight, mm.
    pub spacing_mm: f64,
    /// Smallest legible block-letter height, mm.
    pub min_text_mm: f64,
    /// `published`, or how the value was estimated.
    pub min_text_basis: String,
    pub sources: Vec<String>,
}

#[derive(Debug, serde::Deserialize)]
pub struct Advice {
    pub sources: Vec<Source>,
    pub fabrics: std::collections::BTreeMap<String, FabricAdvice>,
    pub threads: std::collections::BTreeMap<String, ThreadAdvice>,
}

/// The thread weights the advice covers (No. 12 thickest, No. 60 finest).
pub const WEIGHTS: [u32; 4] = [12, 30, 40, 60];

/// The advice table (`data/fabric_advice.json`), parsed once.
pub fn advice() -> &'static Advice {
    static ADVICE: std::sync::OnceLock<Advice> = std::sync::OnceLock::new();
    ADVICE.get_or_init(|| {
        serde_json::from_str(include_str!("../data/fabric_advice.json"))
            .expect("data/fabric_advice.json is valid")
    })
}

/// A metric needle size with its US number ("75/11").
pub fn needle_name(metric: u32) -> String {
    let us = match metric {
        60 => "8",
        65 => "9",
        70 => "10",
        75 => "11",
        80 => "12",
        90 => "14",
        100 => "16",
        110 => "18",
        _ => return metric.to_string(),
    };
    format!("{metric}/{us}")
}

/// The advice for fabric `id` (a custom fabric takes its base's) sewn with thread No. `weight`.
pub fn advise(s: &EngineState, id: &str, weight: u32) -> Option<Value> {
    let f = resolve(s, id)?;
    let a = advice();
    let fa = a.fabrics.get(&f.base_id)?;
    let t = a.threads.get(&weight.to_string())?;
    let size = fa.needle_min.map_or(t.needle, |m| m.max(t.needle));
    let mut ids: Vec<&String> = fa.sources.iter().chain(&t.sources).collect();
    ids.sort();
    ids.dedup();
    let sources: Vec<Value> = ids
        .iter()
        .filter_map(|id| a.sources.iter().find(|s| &&s.id == id))
        .map(|s| json!({ "id": s.id, "maker": s.maker, "title": s.title, "url": s.url }))
        .collect();
    Some(json!({
        "fabric": f.id,
        "weight": weight,
        "backing": fa.backing,
        "topping": fa.topping,
        "needle": {
            "point": fa.needle_point,
            "size": needle_name(size),
            "basis": if fa.needle_min.is_some_and(|m| m > t.needle) {
                "the fabric's own minimum"
            } else {
                "the thread weight's needle"
            },
        },
        "minTextMm": t.min_text_mm,
        "minTextBasis": t.min_text_basis,
        "disagree": fa.disagree,
        "sources": sources,
    }))
}

fn describe(f: &Fabric) -> Value {
    json!({
        "id": f.id,
        "name": f.name,
        "description": f.description,
        "advice": f.advice,
        "fillDensity": f.fill_density,
        "satinDensity": f.satin_density,
        "stitchLength": f.stitch_length,
        "pullCompensation": f.pull_compensation,
        "underlay": f.underlay.map(UnderlayStyle::id),
        // Thresholds of automatic under sewing on this fabric (#523), mm.
        "autoUnderlay": {
            "centreBelow": f.auto_underlay.centre_below,
            "edgeBelow": f.auto_underlay.edge_below,
        },
        // Beyond PE-DESIGN (#508): the fabrics after PE-DESIGN's eight, and the sourced advice
        // for No. 40 thread (fabric.advise gives it for other weights, with the sources).
        "beyondPeDesign": FABRICS.iter().position(|x| x.id == f.id).is_some_and(|i| i >= PE_FABRICS),
        "backing": advice().fabrics.get(f.id).map(|a| a.backing.as_str()),
        "topping": advice().fabrics.get(f.id).map(|a| a.topping.as_str()),
        "needle": advice().fabrics.get(f.id).map(|a| {
            let forty = advice().threads.get("40").map_or(75, |t| t.needle);
            let size = a.needle_min.map_or(forty, |m| m.max(forty));
            format!("{} {}", a.needle_point, needle_name(size))
        }),
        "minTextMm": advice().threads.get("40").map(|t| t.min_text_mm),
    })
}

pub fn find(id: &str) -> Option<&'static Fabric> {
    FABRICS.iter().find(|f| f.id == id)
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "fabric.list",
            "Fabric types of the Fabric Selector with their description, advice and recommended sewing attributes (OpenStitch's approximations): PE-DESIGN's eight, then five more (`beyondPeDesign: true`, #508), then the user's custom fabrics from calibration cards (`custom: true`, #453). Each also gives the makers' backing, topping, needle and smallest text height for No. 40 thread (`backing`, `topping`, `needle`, `minTextMm`; fabric.advise for other weights and the sources)",
            Effect::Read,
            vec![],
            |s, _| {
                let mut v: Vec<Value> = FABRICS.iter().map(describe).collect();
                v.extend(s.user.custom_fabrics.iter().map(describe_custom));
                Ok(Value::Array(v))
            },
        ),
        spec(
            "fabric.advise",
            "What to sew a fabric with (beyond PE-DESIGN, #508): backing, topping, needle point and size, and the smallest legible block-letter height for a thread weight, from the stabiliser, needle and thread makers' published guidance (each source listed; `disagree` names where makers differ, `minTextBasis` says whether the height is published or estimated)",
            Effect::Read,
            vec![
                req(
                    "fabric",
                    "string",
                    "Fabric id from fabric.list (a custom fabric takes its base fabric's advice)",
                ),
                opt(
                    "weight",
                    "12|30|40|60",
                    "Thread weight number (default 40; a higher number is a finer thread)",
                ),
            ],
            |s, p| {
                let id = p.string("fabric", None)?;
                let weight = p.int("weight", Some(40), 1, 200)? as u32;
                if !WEIGHTS.contains(&weight) {
                    return Err(p.invalid("weight", "use 12, 30, 40 or 60"));
                }
                advise(s, &id, weight).ok_or_else(|| p.not_found(format!("fabric '{id}'")))
            },
        ),
        spec(
            "fabric.deleteCustom",
            "Delete a custom fabric saved from a calibration card (#453)",
            Effect::Settings,
            vec![req(
                "fabric",
                "string",
                "Custom fabric id (custom-…) from fabric.list",
            )],
            |s, p| {
                let id = p.string("fabric", None)?;
                let before = s.user.custom_fabrics.len();
                s.user.custom_fabrics.retain(|c| c.id != id);
                if s.user.custom_fabrics.len() == before {
                    return Err(p.not_found(format!("custom fabric '{id}'")));
                }
                Ok(json!({ "deleted": id }))
            },
        ),
        spec(
            "fabric.apply",
            "Give shape and text patterns the sewing attributes recommended for a fabric: fill and satin density, stitch length, pull compensation and underlay (colours and sew types stay)",
            Effect::Edit,
            vec![
                req("fabric", "string", "Fabric id from fabric.list"),
                opt(
                    "ids",
                    "[int]",
                    "Objects to change (default: every shape and text pattern)",
                ),
            ],
            |s, p| {
                let id = p.string("fabric", None)?;
                let f = resolve(s, &id).ok_or_else(|| p.not_found(format!("fabric '{id}'")))?;
                let targets: Vec<usize> = if p.has("ids") || p.has("id") {
                    indexes(p, &s.design)?
                } else {
                    (0..s.design.objects.len()).collect()
                };
                let mut changed = Vec::new();
                for i in targets {
                    let o = &mut s.design.objects[i];
                    if !matches!(o.kind, ObjectKind::Shape | ObjectKind::Text) {
                        if all_objects(p) {
                            continue;
                        }
                        return Err(p.failed(format!("object {} is a stitch pattern", o.id)));
                    }
                    let Some(r) = o.region.as_mut() else { continue };
                    let density = match r.kind {
                        RegionType::Fill | RegionType::ProgrammableFill => f.fill_density,
                        RegionType::Satin => f.satin_density,
                        _ => continue,
                    };
                    r.density = density;
                    r.stitch_length = f.stitch_length;
                    r.pull_compensation = f.pull_compensation;
                    r.underlay = f.underlay.is_some();
                    // Automatic under sewing stays automatic, with the fabric's thresholds.
                    if r.underlay_style == UnderlayStyle::Auto {
                        r.underlay_auto = f.auto_underlay;
                    } else if let Some(u) = f.underlay {
                        r.underlay_style = u;
                    }
                    changed.push(o.id);
                }
                Ok(json!({ "fabric": f.id, "changed": changed }))
            },
        ),
    ]
}

fn all_objects(p: &Params) -> bool {
    !p.has("ids") && !p.has("id")
}

// MARK: custom fabrics (#453)

/// Most custom fabrics the user settings hold.
pub const MAX_CUSTOM: usize = 50;

/// A fabric saved from a calibration card (`fabric.calibrate`): the base fabric's underlay and
/// advice with the densities and pull compensation the user picked on the sewn card.
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomFabric {
    /// `custom-` and the name in lower case, letters and digits joined by `-`.
    pub id: String,
    pub name: String,
    /// The Fabric Selector fabric the card was sewn from.
    pub base: String,
    pub fill_density: f64,
    pub satin_density: f64,
    pub pull_compensation: f64,
    pub stitch_length: f64,
}

/// The id of a custom fabric named `name`.
pub fn custom_id(name: &str) -> String {
    let mut id = String::from("custom");
    let mut dash = true;
    for c in name.chars().flat_map(char::to_lowercase) {
        if c.is_alphanumeric() {
            if dash {
                id.push('-');
            }
            id.push(c);
            dash = false;
        } else {
            dash = true;
        }
    }
    id
}

pub fn describe_custom(c: &CustomFabric) -> Value {
    let base = find(&c.base).unwrap_or(&FABRICS[0]);
    let mut v = describe(base);
    v["id"] = json!(c.id);
    v["name"] = json!(c.name);
    v["custom"] = json!(true);
    v["base"] = json!(c.base);
    v["description"] = json!(format!("Calibrated from {} on a sewn card.", base.name));
    v["fillDensity"] = json!(c.fill_density);
    v["satinDensity"] = json!(c.satin_density);
    v["pullCompensation"] = json!(c.pull_compensation);
    v["stitchLength"] = json!(c.stitch_length);
    v
}

/// A Fabric Selector fabric or a custom one, with the attributes it gives.
#[derive(Clone, Debug)]
pub struct Resolved {
    pub id: String,
    pub name: String,
    /// The Fabric Selector fabric it is (or was calibrated from).
    pub base_id: String,
    pub fill_density: f64,
    pub satin_density: f64,
    pub stitch_length: f64,
    pub pull_compensation: f64,
    pub underlay: Option<UnderlayStyle>,
    pub auto_underlay: AutoUnderlay,
}

/// The fabric `id`: one of [`FABRICS`] or the user's custom fabrics.
pub fn resolve(s: &EngineState, id: &str) -> Option<Resolved> {
    if let Some(f) = find(id) {
        return Some(Resolved {
            id: f.id.into(),
            name: f.name.into(),
            base_id: f.id.into(),
            fill_density: f.fill_density,
            satin_density: f.satin_density,
            stitch_length: f.stitch_length,
            pull_compensation: f.pull_compensation,
            underlay: f.underlay,
            auto_underlay: f.auto_underlay,
        });
    }
    let c = s.user.custom_fabrics.iter().find(|c| c.id == id)?;
    let base = find(&c.base).unwrap_or(&FABRICS[0]);
    Some(Resolved {
        id: c.id.clone(),
        name: c.name.clone(),
        base_id: base.id.into(),
        fill_density: c.fill_density,
        satin_density: c.satin_density,
        stitch_length: c.stitch_length,
        pull_compensation: c.pull_compensation,
        underlay: base.underlay,
        auto_underlay: base.auto_underlay,
    })
}
