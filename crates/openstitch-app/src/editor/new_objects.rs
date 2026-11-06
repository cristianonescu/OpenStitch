//! What new shapes start with (#553, beyond PE-DESIGN): fill angle, density, stitch length, pull
//! compensation, underlay and the outline's stitch length. Each value is optional; an unset one
//! keeps what the drawing tools always gave (PE-DESIGN's 45° fill, 4.5 rows/mm, …), so with
//! nothing set new shapes are exactly as before. The drawing tools add the shape, then set these
//! with `object.setRegion` / `object.setLine` in the same undo step; existing objects never
//! change. Kept in `ui.json`.
//!
//! Nothing here depends on egui.

use openstitch_design::{LineSewing, RegionSewing};
use serde_json::{Map, Value, json};

/// The drawing tools' commands that add a new shape; the defaults apply to what they add.
pub const DRAWN: &[&str] = &[
    "shape.rectangle",
    "shape.ellipse",
    "shape.polygon",
    "shape.path",
    "shape.freehand",
];

/// One default: key (as in `object.setRegion` / `object.setLine`), label, range, and whether it
/// belongs to the region (else the outline).
pub struct Field {
    pub key: &'static str,
    pub label: &'static str,
    pub range: (f64, f64),
    pub suffix: &'static str,
    pub region: bool,
}

pub const FIELDS: &[Field] = &[
    Field {
        key: "angle",
        label: "Fill angle",
        range: (0.0, 359.0),
        suffix: "°",
        region: true,
    },
    Field {
        key: "density",
        label: "Density",
        range: (0.5, 10.0),
        suffix: " rows/mm",
        region: true,
    },
    Field {
        key: "stitchLength",
        label: "Fill stitch length",
        range: (1.0, 10.0),
        suffix: " mm",
        region: true,
    },
    Field {
        key: "pullCompensation",
        label: "Pull compensation",
        range: (0.0, 2.0),
        suffix: " mm",
        region: true,
    },
    Field {
        key: "pitch",
        label: "Outline stitch length",
        range: (1.0, 10.0),
        suffix: " mm",
        region: false,
    },
];

/// The user's defaults: the fields they set, and underlay on or off when set.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NewObjects {
    pub values: Vec<(&'static str, f64)>,
    pub underlay: Option<bool>,
}

fn field(key: &str) -> Option<&'static Field> {
    FIELDS.iter().find(|f| f.key == key)
}

impl NewObjects {
    pub fn is_empty(&self) -> bool {
        self.values.is_empty() && self.underlay.is_none()
    }

    pub fn get(&self, key: &str) -> Option<f64> {
        self.values.iter().find(|(k, _)| *k == key).map(|(_, v)| *v)
    }

    /// Sets (or with `None` clears) one value, checked against its range.
    pub fn set(&mut self, key: &str, value: Option<f64>) -> Result<(), String> {
        let f = field(key).ok_or_else(|| format!("no default '{key}'"))?;
        let (lo, hi) = f.range;
        if value.is_some_and(|v| !v.is_finite() || v < lo || v > hi) {
            return Err(format!("{key} takes {lo}-{hi}"));
        }
        self.values.retain(|(k, _)| *k != f.key);
        if let Some(v) = value {
            self.values.push((f.key, v));
            self.values
                .sort_by_key(|(k, _)| FIELDS.iter().position(|f| f.key == *k));
        }
        Ok(())
    }

    /// The `object.setRegion` parameters (without ids), when any region default is set.
    pub fn region_params(&self) -> Option<Map<String, Value>> {
        let mut m: Map<String, Value> = self
            .values
            .iter()
            .filter(|(k, _)| field(k).is_some_and(|f| f.region))
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect();
        if let Some(u) = self.underlay {
            m.insert("underlay".into(), json!(u));
        }
        (!m.is_empty()).then_some(m)
    }

    /// The `object.setLine` parameters (without ids), when an outline default is set.
    pub fn line_params(&self) -> Option<Map<String, Value>> {
        let m: Map<String, Value> = self
            .values
            .iter()
            .filter(|(k, _)| field(k).is_some_and(|f| !f.region))
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect();
        (!m.is_empty()).then_some(m)
    }

    /// "Save as Default": the selected shape's own values.
    pub fn from_object(region: Option<&RegionSewing>, line: Option<&LineSewing>) -> NewObjects {
        let mut n = NewObjects::default();
        if let Some(r) = region {
            for (k, v) in [
                ("angle", r.angle.rem_euclid(360.0).round()),
                ("density", r.density),
                ("stitchLength", r.stitch_length),
                ("pullCompensation", r.pull_compensation),
            ] {
                // A value outside the page's ranges (a satin's density) is left unset.
                let _ = n.set(k, Some(v));
            }
            n.underlay = Some(r.underlay);
        }
        if let Some(l) = line {
            let _ = n.set("pitch", Some(l.pitch));
        }
        n
    }

    pub fn to_json(&self) -> Value {
        let mut m: Map<String, Value> = self
            .values
            .iter()
            .map(|(k, v)| (k.to_string(), json!(v)))
            .collect();
        if let Some(u) = self.underlay {
            m.insert("underlay".into(), json!(u));
        }
        Value::Object(m)
    }

    /// From `{ angle: 30, density: 4.5, underlay: false, … }` (`ui.set newObjectDefaults`);
    /// `{}` clears them all. Refused whole on the first bad entry.
    pub fn from_json(v: &Value) -> Result<NewObjects, String> {
        let obj = v
            .as_object()
            .ok_or("newObjectDefaults takes an object, e.g. { \"angle\": 30, \"density\": 4.5 }")?;
        let mut n = NewObjects::default();
        for (k, v) in obj {
            if k == "underlay" {
                n.underlay = Some(v.as_bool().ok_or("underlay takes true or false")?);
            } else {
                n.set(
                    k,
                    Some(v.as_f64().ok_or_else(|| format!("{k} takes a number"))?),
                )?;
            }
        }
        Ok(n)
    }

    /// For `ui.json`: a bad entry gives no defaults.
    pub fn load(v: &Value) -> NewObjects {
        NewObjects::from_json(v).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_checks_and_round_trips() {
        let mut n = NewObjects::default();
        assert!(n.is_empty() && n.region_params().is_none() && n.line_params().is_none());
        n.set("density", Some(4.5)).expect("density");
        n.set("angle", Some(30.0)).expect("angle");
        assert!(n.set("angle", Some(400.0)).is_err() && n.set("nope", Some(1.0)).is_err());
        n.underlay = Some(false);
        n.set("pitch", Some(2.0)).expect("pitch");
        assert_eq!(
            Value::Object(n.region_params().expect("region")),
            json!({ "angle": 30.0, "density": 4.5, "underlay": false })
        );
        assert_eq!(
            Value::Object(n.line_params().expect("line")),
            json!({ "pitch": 2.0 })
        );
        assert_eq!(NewObjects::from_json(&n.to_json()), Ok(n.clone()));
        n.set("pitch", None).expect("clear");
        assert!(n.line_params().is_none());
        assert!(NewObjects::from_json(&json!({ "density": "x" })).is_err());
        assert_eq!(NewObjects::load(&json!(null)), NewObjects::default());
        assert_eq!(NewObjects::from_json(&json!({})), Ok(NewObjects::default()));
    }
}
