//! Image to Embroidery wizard model: the conversion chosen in the dialog and its settings,
//! turned into one `digitize.*` command. The conversions are approximations of PE-DESIGN's
//! Auto Punch, Cross Stitch, Photo Stitch 1/2 and Design Center (see `openstitch-digitize`).

use crate::editor::CommandRequest;
use serde_json::{Map, Value, json};
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Method {
    AutoPunch,
    CrossStitch,
    PhotoFill,
    PhotoLines,
    DesignCenter,
}

impl Method {
    pub const ALL: [Method; 5] = [
        Method::AutoPunch,
        Method::CrossStitch,
        Method::PhotoFill,
        Method::PhotoLines,
        Method::DesignCenter,
    ];

    /// Short name for command lines (`--convert`).
    pub fn from_key(key: &str) -> Option<Method> {
        Some(match key {
            "autopunch" => Method::AutoPunch,
            "cross" => Method::CrossStitch,
            "photofill" => Method::PhotoFill,
            "photolines" => Method::PhotoLines,
            "designcenter" => Method::DesignCenter,
            _ => return None,
        })
    }

    pub fn label(self) -> &'static str {
        match self {
            Method::AutoPunch => "Auto Punch",
            Method::CrossStitch => "Cross Stitch",
            Method::PhotoFill => "Photo Stitch 1 (fill)",
            Method::PhotoLines => "Photo Stitch 2 (lines)",
            Method::DesignCenter => "Design Center (line drawing)",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Method::AutoPunch => "Colour areas become fill and satin shapes you can edit.",
            Method::CrossStitch => "One cross per grid cell, one stitch pattern per colour.",
            Method::PhotoFill => "Photos: tone becomes stitch coverage in fill rows.",
            Method::PhotoLines => {
                "Photos: a line screen, one angle per thread, zigzag where its colour is needed."
            }
            Method::DesignCenter => {
                "Line drawings: lines become line shapes, enclosed areas regions."
            }
        }
    }
}

impl Method {
    /// PE-DESIGN 11.50's Max. Number of Colors default for the method's wizard (round-4
    /// captures): Auto Punch and Cross Stitch count the image's colours (0 = automatic), Photo
    /// Stitch 1 10, Photo Stitch 2 4 (its Color Set).
    pub fn default_colors(self) -> u32 {
        match self {
            Method::PhotoFill => 10,
            Method::PhotoLines => 4,
            _ => 0,
        }
    }
}

pub const LINE_SEWS: [&str; 4] = ["running", "triple", "zigzag", "none"];
pub const REGION_SEWS: [&str; 4] = ["auto", "fill", "satin", "none"];

/// Dialog state. Defaults match the engine commands' defaults.
#[derive(Clone, Debug, PartialEq)]
pub struct ImageWizard {
    pub open: bool,
    pub path: Option<PathBuf>,
    pub method: Method,
    /// `None` fits the image into 80 % of the Design Page.
    pub width_mm: Option<f64>,
    pub colors: u32,
    pub skip_background: bool,
    /// Auto Punch and Photo Stitch 1 and 2 in colour: OpenStitch's improved method
    /// (`compatMode: openStitch`) instead of PE-DESIGN's.
    pub open_stitch: bool,
    /// OpenStitch mode's High fidelity (`fidelity: high`): more colours by default and the
    /// costlier passes; the dialog starts on Standard (the owner's choice, 2026-10-31).
    pub high_fidelity: bool,
    /// OpenStitch mode: the threads to choose from (B16.1 T5, the engine's `threads`):
    /// `brother` (the PEC chart), `all` (every built-in chart) or one of the user's charts.
    pub threads: String,
    /// OpenStitch mode: the user's chart of threads they own (`owned`); the result then lists
    /// the chosen threads not in it.
    pub owned: Option<String>,
    /// Auto Punch, OpenStitch mode: sew the background too (the owner's rule, 2026-10-31:
    /// everything in the picture is sewn unless the user turns it off). Off sends
    /// `skipBackground: true`; on leaves the engine's rule (only a fabric-coloured background is
    /// left out).
    pub sew_background: bool,
    /// Cross Stitch, OpenStitch mode: cover each cell with parallel diagonals (`fill`).
    pub cross_fill: bool,
    /// Cross Stitch, OpenStitch mode: three-quarter and quarter crosses where a cell is split by
    /// an edge (`partCrosses`, default off, the owner's choice).
    pub part_crosses: bool,
    /// Photo Stitch 1, OpenStitch mode: sew every jump with a trim for the best likeness
    /// (`travel: allTrims`) instead of short travel stitches (`fewerTrims`); `None` follows the
    /// fidelity (Standard fewer trims, High all).
    pub all_trims: Option<bool>,
    /// Design Center, OpenStitch mode: the threshold by Otsu's method and the line sewing by the
    /// drawing's stroke widths (neither `threshold` nor `lineSew` sent).
    pub auto_lines: bool,
    // Auto Punch
    pub min_area: f64,
    /// The user moved the small-region slider: `minArea` is sent in either mode. Until then
    /// OpenStitch mode leaves it to the engine's own default.
    pub min_area_set: bool,
    pub satin_max: f64,
    /// The user moved the "Satin up to" slider: `satinMaxWidth` is sent in either mode. Until
    /// then OpenStitch mode leaves it to the engine's own default.
    pub satin_set: bool,
    pub curves: bool,
    // Cross Stitch
    pub cell: f64,
    pub half_cross: bool,
    // Photo Stitch
    pub mono: bool,
    pub row_spacing: Option<f64>,
    // Design Center
    pub threshold: u32,
    pub line_sew: &'static str,
    pub region_sew: &'static str,
}

impl Default for ImageWizard {
    fn default() -> Self {
        ImageWizard {
            open: false,
            path: None,
            method: Method::AutoPunch,
            width_mm: None,
            colors: Method::AutoPunch.default_colors(),
            skip_background: true,
            open_stitch: false,
            high_fidelity: false,
            threads: "brother".into(),
            owned: None,
            sew_background: true,
            cross_fill: true,
            part_crosses: false,
            all_trims: None,
            auto_lines: true,
            min_area: 1.0,
            min_area_set: false,
            satin_max: 6.0,
            satin_set: false,
            curves: true,
            cell: 2.5,
            half_cross: false,
            mono: false,
            row_spacing: None,
            threshold: 128,
            line_sew: "running",
            region_sew: "auto",
        }
    }
}

/// Auto Punch's small-region area the dialog shows before the user sets one, mm²: PE-DESIGN's 1,
/// or OpenStitch mode's 2 (the engine's default there, the owner's choice).
pub fn min_area_default(open_stitch: bool) -> f64 {
    if open_stitch { 2.0 } else { 1.0 }
}

/// Auto Punch's widest satin the dialog shows before the user sets one, mm: the dialog's 6.0 in
/// PE-DESIGN mode, or OpenStitch mode's 2.5 (the engine's default there: narrower satin scores
/// closer to the image, 83.3 against 82.5 at 6.5).
pub fn satin_default(open_stitch: bool) -> f64 {
    if open_stitch { 2.5 } else { 6.0 }
}

/// Photo Stitch 1's default colour count at High fidelity (the engine's `HIGH_COLORS`).
pub const HIGH_COLORS: u32 = 24;

impl ImageWizard {
    /// Whether the chosen conversion has an OpenStitch mode (`compatMode`): every one but Photo
    /// Stitch in Mono.
    pub fn improvable(&self) -> bool {
        !(matches!(self.method, Method::PhotoFill | Method::PhotoLines) && self.mono)
    }

    /// Whether the chosen conversion has a fidelity and a thread source: Auto Punch and Photo
    /// Stitch in colour.
    pub fn has_fidelity(&self) -> bool {
        self.improvable()
            && matches!(
                self.method,
                Method::AutoPunch | Method::PhotoFill | Method::PhotoLines
            )
    }

    /// Whether High fidelity applies: chosen, in OpenStitch mode, on a conversion that has it.
    pub fn high(&self) -> bool {
        self.high_fidelity && self.open_stitch && self.has_fidelity()
    }

    /// Photo Stitch 1 in OpenStitch mode: whether every jump is trimmed (the user's choice, or
    /// the fidelity's: High all trims, Standard fewer).
    pub fn all_trims(&self) -> bool {
        self.all_trims.unwrap_or(self.high())
    }

    /// The colour count the dialog starts from for the current settings:
    /// [`Method::default_colors`], or [`HIGH_COLORS`] for Photo Stitch 1 at High fidelity.
    pub fn colors_default(&self) -> u32 {
        if self.method == Method::PhotoFill && self.high() {
            HIGH_COLORS
        } else {
            self.method.default_colors()
        }
    }

    /// Applies `change` to the settings; a colour count left at its default follows the new
    /// default (Photo Stitch 1: 10 at Standard, 24 at High).
    pub fn change(&mut self, change: impl FnOnce(&mut Self)) {
        let before = self.colors_default();
        change(self);
        if self.colors == before {
            self.colors = self.colors_default();
        }
    }

    /// The conversion command, or `None` until an image is chosen.
    pub fn request(&self) -> Option<CommandRequest> {
        let path = self.path.as_ref()?;
        let mut p = Map::new();
        p.insert("path".into(), json!(path.display().to_string()));
        if let Some(w) = self.width_mm {
            p.insert("width".into(), json!(w));
        }
        // 0 asks Auto Punch and Cross Stitch for the automatic count.
        let colors = json!(self.colors.min(64));
        if self.open_stitch && self.improvable() {
            p.insert("compatMode".into(), json!("openStitch"));
            // Photo Stitch 2 keeps its own Auto Select.
            if matches!(self.method, Method::AutoPunch | Method::PhotoFill) {
                if self.threads != "brother" {
                    p.insert("threads".into(), json!(self.threads));
                }
                if let Some(o) = &self.owned {
                    p.insert("owned".into(), json!(o));
                }
            }
        }
        if self.high() {
            p.insert("fidelity".into(), json!("high"));
        }
        let command = match self.method {
            Method::AutoPunch => {
                p.insert("colors".into(), colors);
                // OpenStitch mode sews the background unless turned off; PE-DESIGN mode keeps
                // its "Leave out the background colour".
                if !self.open_stitch {
                    p.insert("skipBackground".into(), json!(self.skip_background));
                } else if !self.sew_background {
                    p.insert("skipBackground".into(), json!(true));
                }
                if !self.open_stitch || self.min_area_set {
                    p.insert("minArea".into(), json!(self.min_area));
                }
                if !self.open_stitch || self.satin_set {
                    p.insert("satinMaxWidth".into(), json!(self.satin_max));
                }
                p.insert("curves".into(), json!(self.curves));
                "digitize.autoPunch"
            }
            Method::CrossStitch => {
                p.insert("colors".into(), colors);
                if self.open_stitch && !self.cross_fill {
                    p.insert("fill".into(), json!(false));
                }
                if self.open_stitch && self.part_crosses {
                    p.insert("partCrosses".into(), json!(true));
                }
                p.insert("skipBackground".into(), json!(self.skip_background));
                p.insert("cellSize".into(), json!(self.cell));
                p.insert(
                    "type".into(),
                    json!(if self.half_cross { "half" } else { "full" }),
                );
                "digitize.crossStitch"
            }
            Method::PhotoFill | Method::PhotoLines => {
                let lines = self.method == Method::PhotoLines;
                p.insert("style".into(), json!(if lines { "lines" } else { "fill" }));
                if self.mono {
                    p.insert("mode".into(), json!("mono"));
                } else {
                    p.insert("colors".into(), json!(self.colors.clamp(1, 32)));
                }
                if let Some(r) = self.row_spacing {
                    p.insert("rowSpacing".into(), json!(r));
                }
                if !lines && !self.mono && self.open_stitch {
                    let travel = if self.all_trims() {
                        "allTrims"
                    } else {
                        "fewerTrims"
                    };
                    p.insert("travel".into(), json!(travel));
                }
                "digitize.photoStitch"
            }
            Method::DesignCenter => {
                if !(self.open_stitch && self.auto_lines) {
                    p.insert("threshold".into(), json!(self.threshold.min(255)));
                    p.insert("lineSew".into(), json!(self.line_sew));
                }
                p.insert("regionSew".into(), json!(self.region_sew));
                p.insert("curves".into(), json!(self.curves));
                "digitize.designCenter"
            }
        };
        Some(CommandRequest::new(command, Value::Object(p)))
    }

    /// One line summarising a conversion's result for the status bar.
    pub fn summary(&self, result: &Value) -> String {
        let n = CommandRequest::created_ids(result).len();
        let colors = result
            .get("colors")
            .and_then(Value::as_array)
            .map_or(0, Vec::len);
        match self.method {
            Method::DesignCenter => format!(
                "{}: {} lines, {} regions → {n} objects",
                self.method.label(),
                result["lines"],
                result["regions"]
            ),
            _ => {
                let line = format!("{}: {n} objects in {colors} colours", self.method.label());
                // The chosen threads the user does not own (B16.1 T5).
                match result.get("toBuy").and_then(Value::as_array) {
                    Some(buy) if !buy.is_empty() => {
                        let names: Vec<String> = buy
                            .iter()
                            .map(|t| {
                                let part = |k: &str| t.get(k).and_then(Value::as_str).unwrap_or("");
                                let name = format!("{} {}", part("brand"), part("code"));
                                match name.trim() {
                                    "" => part("color").to_string(),
                                    n => n.to_string(),
                                }
                            })
                            .collect();
                        format!("{line}; to buy: {}", names.join(", "))
                    }
                    _ => line,
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_engine::Session;

    /// A 60×40 PNG: red block, blue bar, black outline square, on white.
    fn sample(dir: &std::path::Path) -> PathBuf {
        let mut rgba = Vec::new();
        for y in 0..40u32 {
            for x in 0..60u32 {
                let inside = |x0, y0, x1, y1| x >= x0 && x < x1 && y >= y0 && y < y1;
                rgba.extend_from_slice(&if inside(5, 5, 25, 35) {
                    [220, 20, 30, 255]
                } else if inside(35, 10, 55, 14) {
                    [20, 40, 200, 255]
                } else if inside(35, 18, 55, 36) && !inside(38, 21, 52, 33) {
                    [0, 0, 0, 255]
                } else {
                    [255, 255, 255, 255]
                });
            }
        }
        let path = dir.join("wizard.png");
        std::fs::write(&path, crate::png::encode_rgba(60, 40, &rgba).expect("png")).expect("write");
        path
    }

    #[test]
    fn every_method_converts_in_one_command() {
        let dir = std::env::temp_dir().join(format!("openstitch-wizard-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        let mut w = ImageWizard::default();
        assert!(w.request().is_none(), "no image yet");
        w.path = Some(sample(&dir));
        w.width_mm = Some(60.0);
        for m in Method::ALL {
            for mono in [false, true] {
                w.method = m;
                w.mono = mono;
                let r = w.request().expect("request");
                let mut s = Session::new(&dir);
                let result = s
                    .execute(r.command, Some(&r.params))
                    .unwrap_or_else(|e| panic!("{m:?}: {e}"));
                let ids = CommandRequest::created_ids(&result);
                assert!(!ids.is_empty(), "{m:?}");
                assert_eq!(s.design().objects.len(), ids.len(), "{m:?}");
                assert!(w.summary(&result).starts_with(m.label()));
                s.execute("history.undo", None).expect("undo");
                assert!(s.design().objects.is_empty(), "{m:?} is one undo step");
            }
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn requests_carry_the_settings() {
        let w = ImageWizard {
            path: Some(PathBuf::from("a.png")),
            method: Method::CrossStitch,
            colors: 5,
            cell: 3.0,
            half_cross: true,
            skip_background: false,
            ..Default::default()
        };
        let r = w.request().expect("request");
        assert_eq!(r.command, "digitize.crossStitch");
        assert_eq!(
            r.params,
            json!({"path": "a.png", "colors": 5, "cellSize": 3.0, "type": "half", "skipBackground": false})
        );
        let photo = ImageWizard {
            method: Method::PhotoLines,
            mono: true,
            row_spacing: Some(1.0),
            ..w
        };
        let r = photo.request().expect("request");
        assert_eq!(r.params["style"], "lines");
        assert_eq!(r.params["mode"], "mono");
        assert_eq!(r.params["rowSpacing"], 1.0);
        assert!(r.params.get("colors").is_none());
        let open = ImageWizard {
            method: Method::AutoPunch,
            open_stitch: true,
            ..photo
        };
        let r = open.request().expect("request");
        assert_eq!(r.params["compatMode"], "openStitch");
        let photo = ImageWizard {
            method: Method::PhotoFill,
            mono: false,
            ..open
        };
        assert_eq!(
            photo.request().expect("request").params["compatMode"],
            "openStitch"
        );
        let lines = ImageWizard {
            method: Method::PhotoLines,
            ..photo.clone()
        };
        assert_eq!(
            lines.request().expect("request").params["compatMode"],
            "openStitch"
        );
        let mono = ImageWizard {
            mono: true,
            ..lines
        };
        assert!(
            mono.request()
                .expect("request")
                .params
                .get("compatMode")
                .is_none()
        );
    }

    /// The OpenStitch-mode options other tracks asked for (B16.1): Cross Stitch and Design
    /// Center take the Mode, Auto Punch sews the background unless turned off, Cross Stitch fills
    /// its cells, Photo Stitch 1's trims follow the fidelity unless chosen.
    #[test]
    fn open_stitch_options_reach_every_tool() {
        let base = ImageWizard {
            path: Some("a.png".into()),
            open_stitch: true,
            ..Default::default()
        };
        // Auto Punch: the background is sewn unless turned off; PE mode keeps its checkbox. The
        // small-region area is the engine's unless the user set it.
        let ap = base.clone();
        let r = ap.request().expect("request");
        assert!(r.params.get("skipBackground").is_none());
        assert!(r.params.get("minArea").is_none());
        assert!(r.params.get("satinMaxWidth").is_none());
        let set = ImageWizard {
            min_area: 4.0,
            min_area_set: true,
            satin_max: 3.0,
            satin_set: true,
            ..ap.clone()
        };
        let r = set.request().expect("request");
        assert_eq!(r.params["minArea"], 4.0);
        assert_eq!(r.params["satinMaxWidth"], 3.0);
        let off = ImageWizard {
            sew_background: false,
            ..ap.clone()
        };
        assert_eq!(
            off.request().expect("request").params["skipBackground"],
            true
        );
        let pe = ImageWizard {
            open_stitch: false,
            ..ap
        };
        let r = pe.request().expect("request");
        assert_eq!(r.params["skipBackground"], true);
        assert_eq!(
            r.params["minArea"], 1.0,
            "PE-DESIGN mode sends it as before"
        );
        assert_eq!(r.params["satinMaxWidth"], 6.0);
        // Cross Stitch: Mode, and fill sent only when turned off.
        let cs = ImageWizard {
            method: Method::CrossStitch,
            ..base.clone()
        };
        let r = cs.request().expect("request");
        assert_eq!(r.params["compatMode"], "openStitch");
        assert!(r.params.get("fill").is_none() && r.params.get("fidelity").is_none());
        assert!(
            r.params.get("partCrosses").is_none(),
            "part crosses are off by default"
        );
        let plain = ImageWizard {
            cross_fill: false,
            part_crosses: true,
            ..cs
        };
        let r = plain.request().expect("request");
        assert_eq!(r.params["fill"], false);
        assert_eq!(r.params["partCrosses"], true);
        // Design Center: Mode; threshold and line sewing automatic unless chosen.
        let dc = ImageWizard {
            method: Method::DesignCenter,
            ..base.clone()
        };
        let r = dc.request().expect("request");
        assert_eq!(r.params["compatMode"], "openStitch");
        assert!(r.params.get("threshold").is_none() && r.params.get("lineSew").is_none());
        let manual = ImageWizard {
            auto_lines: false,
            ..dc
        };
        assert_eq!(manual.request().expect("request").params["threshold"], 128);
        // Photo Stitch 1: trims follow the fidelity unless chosen.
        let mut ps = ImageWizard {
            method: Method::PhotoFill,
            ..base
        };
        assert_eq!(
            ps.request().expect("request").params["travel"],
            "fewerTrims"
        );
        ps.high_fidelity = true;
        assert_eq!(ps.request().expect("request").params["travel"], "allTrims");
        ps.all_trims = Some(false);
        assert_eq!(
            ps.request().expect("request").params["travel"],
            "fewerTrims"
        );
        let lines = ImageWizard {
            method: Method::PhotoLines,
            ..ps
        };
        assert!(
            lines
                .request()
                .expect("request")
                .params
                .get("travel")
                .is_none()
        );
    }

    /// Thread sources (B16.1 T5): sent only in OpenStitch mode, not for Photo Stitch 2, and the
    /// summary lists the threads to buy.
    #[test]
    fn thread_source_round_trips_and_lists_threads_to_buy() {
        let mut w = ImageWizard {
            path: Some("a.png".into()),
            threads: "all".into(),
            owned: Some("My Threads".into()),
            ..Default::default()
        };
        assert!(
            w.request()
                .expect("request")
                .params
                .get("threads")
                .is_none()
        );
        w.open_stitch = true;
        let r = w.request().expect("request");
        assert_eq!(r.params["threads"], "all");
        assert_eq!(r.params["owned"], "My Threads");
        let lines = ImageWizard {
            method: Method::PhotoLines,
            ..w.clone()
        };
        assert!(
            lines
                .request()
                .expect("request")
                .params
                .get("threads")
                .is_none()
        );
        let result = json!({"ids": [1, 2], "colors": ["#ff0000", "#00ff00"],
            "toBuy": [{"color": "#00ff00", "brand": "Janome", "code": "202"}]});
        assert_eq!(
            w.summary(&result),
            "Auto Punch: 2 objects in 2 colours; to buy: Janome 202"
        );
    }

    /// Fidelity (B16.1): High is sent only in OpenStitch mode, and Photo Stitch 1's colour
    /// count follows it while it is left at its default.
    #[test]
    fn fidelity_round_trips_and_moves_the_default_colours() {
        let mut w = ImageWizard {
            path: Some("a.png".into()),
            method: Method::PhotoFill,
            colors: Method::PhotoFill.default_colors(),
            ..Default::default()
        };
        assert!(!w.high_fidelity, "the dialog starts on Standard");
        w.change(|w| w.high_fidelity = true);
        assert_eq!(w.colors, 10, "PE-DESIGN mode has no fidelity");
        assert!(
            w.request()
                .expect("request")
                .params
                .get("fidelity")
                .is_none()
        );
        w.change(|w| w.open_stitch = true);
        assert_eq!(w.colors, HIGH_COLORS);
        let r = w.request().expect("request");
        assert_eq!(r.params["fidelity"], "high");
        assert_eq!(r.params["compatMode"], "openStitch");
        assert_eq!(r.params["colors"], 24);
        w.change(|w| w.high_fidelity = false);
        assert_eq!(w.colors, 10, "back to Standard's default");
        assert!(
            w.request()
                .expect("request")
                .params
                .get("fidelity")
                .is_none()
        );
        // A count the user chose is kept.
        w.colors = 7;
        w.change(|w| w.high_fidelity = true);
        assert_eq!(w.colors, 7);
        // Auto Punch keeps its automatic count; the engine raises its ceiling.
        let mut ap = ImageWizard {
            method: Method::AutoPunch,
            colors: 0,
            ..w
        };
        ap.change(|w| w.high_fidelity = true);
        assert_eq!(ap.colors, 0);
        assert_eq!(ap.request().expect("request").params["fidelity"], "high");
    }
}
