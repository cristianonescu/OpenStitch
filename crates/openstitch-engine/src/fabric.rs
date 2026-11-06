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
];

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
    })
}

pub fn find(id: &str) -> Option<&'static Fabric> {
    FABRICS.iter().find(|f| f.id == id)
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "fabric.list",
            "Fabric types of the Fabric Selector with their description, advice and recommended sewing attributes (OpenStitch's approximations), then the user's custom fabrics from calibration cards (`custom: true`, #453)",
            Effect::Read,
            vec![],
            |s, _| {
                let mut v: Vec<Value> = FABRICS.iter().map(describe).collect();
                v.extend(s.user.custom_fabrics.iter().map(describe_custom));
                Ok(Value::Array(v))
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
