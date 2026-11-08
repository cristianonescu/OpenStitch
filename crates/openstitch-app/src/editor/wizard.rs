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

/// Auto Punch's purpose presets (#489), the engine's `preset` values with their labels, in the
/// order `digitize.presets` lists them.
pub const PRESETS: [(&str, &str); 6] = [
    ("logo", "Logo"),
    ("patch", "Patch"),
    ("lineArt", "Line art"),
    ("illustration", "Illustration / cartoon"),
    ("photo", "Photo"),
    ("pixelArt", "Pixel art"),
];

/// The stitch choices of the region table (#489): Auto Punch's own, or one of the engine's
/// override types.
pub const REGION_TYPES: [&str; 5] = ["auto", "fill", "satin", "line", "skip"];

/// A "review" note shows when more than this share of the picture's region area is left out.
pub const REVIEW_SHARE: f64 = 0.25;

/// One region of an Auto Punch preview (#489), as the engine's `plan` gives it.
#[derive(Clone, Debug, PartialEq)]
pub struct PlanRegion {
    pub id: usize,
    pub color: String,
    pub thread: String,
    /// The code on the region's spool (#798: Brother 007, a maker's or the user's code).
    pub code: String,
    pub area_mm2: f64,
    pub width_mm: f64,
    /// fill, satin, line or skip.
    pub kind: String,
    pub reason: String,
    pub why: String,
}

/// An Auto Punch preview (#489): its regions, the region under each map cell, and the request
/// it was made for (without the overrides), so the overrides apply only while it matches.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Plan {
    pub regions: Vec<PlanRegion>,
    pub map_size: (usize, usize),
    /// Region per map cell, row by row; -1 where none is.
    pub map: Vec<i64>,
    /// The threads in sewing order.
    pub colors: Vec<String>,
    pub made_for: Value,
}
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
    /// `brother` (the PEC chart), `all` (every built-in chart), a built-in chart's id or one of
    /// the user's charts. Follows the user's default chart (`threads.defaultChart`, #796) until
    /// they choose here ([`ImageWizard::threads_chosen`]).
    pub threads: String,
    /// The Threads choice was made in the wizard, so it no longer follows the default chart.
    pub threads_chosen: bool,
    /// OpenStitch mode: the user's chart of threads they own (`owned`); the result then lists
    /// the chosen threads not in it.
    pub owned: Option<String>,
    /// Auto Punch, OpenStitch mode: sew the background too (the owner's rule, 2026-10-31:
    /// everything in the picture is sewn unless the user turns it off). Off sends
    /// `skipBackground: true`; on leaves the engine's rule (only a fabric-coloured background is
    /// left out).
    pub sew_background: bool,
    /// Auto Punch, OpenStitch mode: find the subject by a graph cut and leave out everything
    /// around it, whatever its colour (`isolateSubject`, #127; off by default).
    pub isolate_subject: bool,
    /// With `isolate_subject`: the strokes marked on the image preview (#350), in the order
    /// drawn; sent as `subjectStrokes` and `backgroundStrokes`.
    pub marks: Vec<Mark>,
    /// The marking brush: subject (true) or background.
    pub mark_subject: bool,
    /// The marking brush's width, a fraction of the image's shorter side.
    pub mark_width: f64,
    /// The marking window is open.
    pub marking: bool,
    /// Auto Punch, OpenStitch mode: touching fills turned apart (`angleRule: varyNeighbours`,
    /// #489; off by default).
    pub vary_angles: bool,
    /// Auto Punch, OpenStitch mode: a purpose preset (`preset`, #489); `None` is none.
    pub preset: Option<&'static str>,
    /// Auto Punch: the last preview (`dryRun`), for the region table and click-to-fix (#489).
    pub plan: Option<Plan>,
    /// Auto Punch: the user's stitch per region number (`overrides`, #489), sent while the
    /// preview still matches the settings.
    pub overrides: std::collections::BTreeMap<usize, &'static str>,
    /// Auto Punch: the colours to sew first (`order`, `#rrggbb`, #489).
    pub order: Vec<String>,
    /// The click-to-fix window is open.
    pub fixing: bool,
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

/// The wizard's `threads` value for a default chart (#796): a built-in chart's id or the
/// user chart's name.
pub fn threads_value(c: &openstitch_engine::user::ChartRef) -> String {
    match c {
        openstitch_engine::user::ChartRef::Builtin(id) => id.clone(),
        openstitch_engine::user::ChartRef::Chart(n) => n.clone(),
    }
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
            threads_chosen: false,
            owned: None,
            sew_background: true,
            isolate_subject: false,
            marks: Vec::new(),
            mark_subject: true,
            mark_width: MARK_WIDTH,
            marking: false,
            vary_angles: false,
            preset: None,
            plan: None,
            overrides: std::collections::BTreeMap::new(),
            order: Vec::new(),
            fixing: false,
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

/// A brush stroke on the image preview (#350): subject or background for certain, along
/// `points` in fractions of the image's width and height, `width` a fraction of its shorter side.
#[derive(Clone, Debug, PartialEq)]
pub struct Mark {
    pub subject: bool,
    pub points: Vec<[f64; 2]>,
    pub width: f64,
}

/// The marking brush's default width, a fraction of the image's shorter side.
pub const MARK_WIDTH: f64 = 0.03;
/// The brush widths the dialog offers (the engine takes 0.002–0.25).
pub const MARK_WIDTHS: std::ops::RangeInclusive<f64> = 0.005..=0.15;

/// Where `pos` falls on an image drawn at `min` with `size` (screen points), as fractions of
/// its width and height; `None` outside it.
pub fn image_fraction(pos: [f32; 2], min: [f32; 2], size: [f32; 2]) -> Option<[f64; 2]> {
    if size[0] <= 0.0 || size[1] <= 0.0 {
        return None;
    }
    let f = [
        ((pos[0] - min[0]) / size[0]) as f64,
        ((pos[1] - min[1]) / size[1]) as f64,
    ];
    f.iter().all(|v| (0.0..=1.0).contains(v)).then_some(f)
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

    /// Switches to `m` with its default colour count, as choosing it in the dialog does (opening
    /// Photo Stitch 1 from the ribbon kept Auto Punch's 0 = Auto, which the dialog's 1..=32
    /// slider showed as 1).
    pub fn set_method(&mut self, m: Method) {
        self.method = m;
        self.colors = self.colors_default();
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
                // Always named: the engine's default is the user's default chart (#810), so
                // "brother" is passed too.
                p.insert("threads".into(), json!(self.threads));
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
                // OpenStitch mode's subject isolation replaces the background rule.
                if !self.open_stitch {
                    p.insert("skipBackground".into(), json!(self.skip_background));
                } else if self.isolate_subject {
                    p.insert("isolateSubject".into(), json!(true));
                    for (key, subject) in [("subjectStrokes", true), ("backgroundStrokes", false)] {
                        let strokes: Vec<Value> = self
                            .marks
                            .iter()
                            .filter(|m| m.subject == subject && !m.points.is_empty())
                            .map(|m| json!({"points": m.points, "width": m.width}))
                            .collect();
                        if !strokes.is_empty() {
                            p.insert(key.into(), Value::Array(strokes));
                        }
                    }
                } else if !self.sew_background {
                    p.insert("skipBackground".into(), json!(true));
                }
                if self.open_stitch && self.vary_angles {
                    p.insert("angleRule".into(), json!("varyNeighbours"));
                }
                if let (true, Some(preset)) = (self.open_stitch, self.preset) {
                    p.insert("preset".into(), json!(preset));
                }
                if !self.order.is_empty() {
                    p.insert("order".into(), json!(self.order));
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
        let mut request = CommandRequest::new(command, Value::Object(p));
        // The table's choices, while the preview they were made on still matches.
        if self.method == Method::AutoPunch
            && !self.overrides.is_empty()
            && self
                .plan
                .as_ref()
                .is_some_and(|plan| plan.made_for == request.params)
        {
            let list: Vec<Value> = self
                .overrides
                .iter()
                .map(|(region, kind)| json!({"region": region, "type": kind}))
                .collect();
            request.params["overrides"] = Value::Array(list);
        }
        Some(request)
    }

    /// Auto Punch's preview: the conversion as a dry run with its region map (#489), nothing
    /// added to the design.
    pub fn preview_request(&self) -> Option<CommandRequest> {
        if self.method != Method::AutoPunch {
            return None;
        }
        let mut r = self.request()?;
        r.params["dryRun"] = json!(true);
        r.params["regionMap"] = json!(true);
        Some(r)
    }

    /// Whether the preview was made with today's settings (the overrides aside).
    pub fn plan_current(&self) -> bool {
        let Some(plan) = &self.plan else {
            return false;
        };
        let mut now = self.clone();
        now.overrides.clear();
        now.request().is_some_and(|r| r.params == plan.made_for)
    }

    /// Keeps a preview's result (`preview_request`'s), as of the settings it was made with.
    pub fn set_plan(&mut self, result: &Value) {
        let made_for = {
            let mut now = self.clone();
            now.overrides.clear();
            now.request().map_or(Value::Null, |r| r.params)
        };
        let s = |v: &Value, k: &str| v[k].as_str().unwrap_or_default().to_string();
        let f = |v: &Value, k: &str| v[k].as_f64().unwrap_or(0.0);
        let regions: Vec<PlanRegion> = result["plan"]
            .as_array()
            .map(|a| {
                a.iter()
                    .map(|r| PlanRegion {
                        id: r["region"].as_u64().unwrap_or(0) as usize,
                        color: s(r, "color"),
                        thread: s(r, "thread"),
                        code: s(r, "code"),
                        area_mm2: f(r, "areaMm2"),
                        width_mm: f(r, "widthMm"),
                        kind: s(r, "type"),
                        reason: s(r, "reason"),
                        why: s(r, "why"),
                    })
                    .collect()
            })
            .unwrap_or_default();
        let m = &result["regionMap"];
        let size = (
            m["width"].as_u64().unwrap_or(0) as usize,
            m["height"].as_u64().unwrap_or(0) as usize,
        );
        let cells = size.0 * size.1;
        let mut map = Vec::with_capacity(cells);
        for run in m["runs"].as_array().into_iter().flatten() {
            let region = run[0].as_i64().unwrap_or(-1);
            let count = run[1].as_u64().unwrap_or(0) as usize;
            map.extend(std::iter::repeat_n(region, count.min(cells - map.len())));
        }
        map.resize(size.0 * size.1, -1);
        let colors = result["colors"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(Value::as_str)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        // Choices for regions the preview no longer has are dropped.
        self.overrides.retain(|id, _| *id < regions.len());
        self.plan = Some(Plan {
            regions,
            map_size: size,
            map,
            colors,
            made_for,
        });
    }

    /// The region under `at` (fractions of the image) in the preview, if any.
    pub fn region_at(&self, at: [f64; 2]) -> Option<usize> {
        let plan = self.plan.as_ref()?;
        let (w, h) = plan.map_size;
        if w == 0 || h == 0 {
            return None;
        }
        let x = ((at[0].clamp(0.0, 1.0) * w as f64) as usize).min(w - 1);
        let y = ((at[1].clamp(0.0, 1.0) * h as f64) as usize).min(h - 1);
        usize::try_from(plan.map[y * w + x]).ok()
    }

    /// The stitch a region is sewn with: the user's choice, or the preview's.
    pub fn region_kind(&self, id: usize) -> &str {
        match self.overrides.get(&id) {
            Some(k) => k,
            None => self
                .plan
                .as_ref()
                .and_then(|p| p.regions.get(id))
                .map_or("", |r| r.kind.as_str()),
        }
    }

    /// Sets region `id`'s stitch (`auto` drops the user's choice).
    pub fn set_region(&mut self, id: usize, kind: &'static str) {
        if kind == "auto" {
            self.overrides.remove(&id);
        } else {
            self.overrides.insert(id, kind);
        }
    }

    /// Click-to-fix (#489): flips the region under `at` between sewn and left out; returns it.
    pub fn toggle_at(&mut self, at: [f64; 2]) -> Option<usize> {
        let id = self.region_at(at)?;
        let skipped = self.region_kind(id) == "skip";
        let auto_skips = self
            .plan
            .as_ref()
            .and_then(|p| p.regions.get(id))
            .is_some_and(|r| r.kind == "skip");
        if skipped {
            // Back to Auto Punch's own stitch, or a fill if it left the region out itself.
            self.set_region(id, if auto_skips { "fill" } else { "auto" });
        } else {
            self.set_region(id, if auto_skips { "auto" } else { "skip" });
        }
        Some(id)
    }

    /// The "review" note (#489): when more than [`REVIEW_SHARE`] of the regions' area is left
    /// out, how much.
    pub fn review_note(&self) -> Option<String> {
        let plan = self.plan.as_ref()?;
        let total: f64 = plan.regions.iter().map(|r| r.area_mm2).sum();
        let left: f64 = plan
            .regions
            .iter()
            .filter(|r| self.region_kind(r.id) == "skip")
            .map(|r| r.area_mm2)
            .sum();
        (total > 0.0 && left > REVIEW_SHARE * total).then(|| {
            format!(
                "Review: {:.0} % of the picture's regions are left out.",
                100.0 * left / total
            )
        })
    }

    /// The colours in sewing order: the user's order, else the preview's. With a preview, only
    /// its colours (the threads change with the mode and the colour count).
    /// What a colour of the preview shows (#798): the code of the thread a region of that
    /// colour got, or the colour's `#rrggbb` when no region names one.
    pub fn colour_label(&self, hex: &str) -> String {
        self.plan
            .as_ref()
            .and_then(|p| {
                p.regions
                    .iter()
                    .find(|r| r.color == hex && !r.code.is_empty())
            })
            .map_or_else(|| hex.to_string(), |r| r.code.clone())
    }

    pub fn colour_order(&self) -> Vec<String> {
        let mut out = self.order.clone();
        if let Some(plan) = &self.plan {
            out.retain(|c| plan.colors.contains(c));
            for c in &plan.colors {
                if !out.contains(c) {
                    out.push(c.clone());
                }
            }
        }
        out
    }

    /// Moves colour `i` of [`ImageWizard::colour_order`] one place earlier (`earlier`) or
    /// later. Region numbers follow the sewing order, so the region choices are cleared.
    pub fn move_colour(&mut self, i: usize, earlier: bool) {
        let mut all = self.colour_order();
        let j = if earlier {
            i.checked_sub(1)
        } else {
            Some(i + 1)
        };
        let Some(j) = j.filter(|&j| j < all.len() && i < all.len()) else {
            return;
        };
        all.swap(i, j);
        self.order = all;
        self.overrides.clear();
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

    /// The marking preview maps the pointer to fractions of the image, and nothing outside it.
    #[test]
    fn image_fraction_maps_the_pointer_onto_the_image() {
        let (min, size) = ([100.0, 50.0], [400.0, 200.0]);
        assert_eq!(image_fraction([100.0, 50.0], min, size), Some([0.0, 0.0]));
        assert_eq!(image_fraction([300.0, 100.0], min, size), Some([0.5, 0.25]));
        assert_eq!(image_fraction([500.0, 250.0], min, size), Some([1.0, 1.0]));
        assert_eq!(image_fraction([99.0, 100.0], min, size), None);
        assert_eq!(image_fraction([300.0, 251.0], min, size), None);
        assert_eq!(image_fraction([300.0, 100.0], min, [0.0, 200.0]), None);
    }
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
        let dir = openstitch_testutil::TempDir::new("wizard");
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
        // Isolating the subject replaces the background rule; PE-DESIGN mode never sends it.
        assert!(
            ap.request()
                .expect("request")
                .params
                .get("isolateSubject")
                .is_none()
        );
        let isolate = ImageWizard {
            isolate_subject: true,
            sew_background: false,
            ..ap.clone()
        };
        let r = isolate.request().expect("request");
        assert_eq!(r.params["isolateSubject"], true);
        assert!(r.params.get("skipBackground").is_none());
        assert!(r.params.get("subjectStrokes").is_none());
        // Strokes marked on the preview go with it, each kind in drawing order (#350).
        let marked = ImageWizard {
            marks: vec![
                Mark {
                    subject: true,
                    points: vec![[0.3, 0.6], [0.7, 0.6]],
                    width: 0.05,
                },
                Mark {
                    subject: false,
                    points: vec![[0.1, 0.9]],
                    width: 0.02,
                },
                Mark {
                    subject: true,
                    points: Vec::new(),
                    width: 0.05,
                },
            ],
            ..isolate.clone()
        };
        let r = marked.request().expect("request");
        assert_eq!(
            r.params["subjectStrokes"],
            json!([{"points": [[0.3, 0.6], [0.7, 0.6]], "width": 0.05}])
        );
        assert_eq!(
            r.params["backgroundStrokes"],
            json!([{"points": [[0.1, 0.9]], "width": 0.02}])
        );
        // Not isolating, the strokes stay in the dialog and are not sent.
        let off = ImageWizard {
            isolate_subject: false,
            ..marked
        };
        assert!(
            off.request()
                .expect("request")
                .params
                .get("subjectStrokes")
                .is_none()
        );
        let pe = ImageWizard {
            open_stitch: false,
            isolate_subject: true,
            ..ap
        };
        let r = pe.request().expect("request");
        assert!(r.params.get("isolateSubject").is_none());
        assert_eq!(r.params["skipBackground"], true);
        assert_eq!(
            r.params["minArea"], 1.0,
            "PE-DESIGN mode sends it as before"
        );
        assert_eq!(r.params["satinMaxWidth"], 6.0);
        assert!(r.params.get("angleRule").is_none());
        // OpenStitch mode: touching fills turned apart only when chosen (#489).
        let vary = ImageWizard {
            open_stitch: true,
            vary_angles: true,
            ..base.clone()
        };
        assert_eq!(
            vary.request().expect("request").params["angleRule"],
            "varyNeighbours"
        );
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
    fn opening_on_a_method_starts_from_its_default_colours() {
        let mut w = ImageWizard::default();
        w.set_method(Method::PhotoFill);
        assert_eq!(w.colors, 10);
        w.set_method(Method::PhotoLines);
        assert_eq!(w.colors, 4);
        w.set_method(Method::AutoPunch);
        assert_eq!(w.colors, 0);
    }

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

    /// #489: the region table and click-to-fix. A preview adds nothing and fills the table and
    /// the region map; a click on a region leaves it out (and back), the choice reaches the
    /// conversion only while the preview matches the settings, the review note shows when most
    /// is left out, and the colour order and the preset reach the command.
    #[test]
    fn preview_table_and_click_to_fix() {
        let dir = openstitch_testutil::TempDir::new("wizard-plan");
        let mut w = ImageWizard {
            path: Some(sample(&dir)),
            width_mm: Some(60.0),
            colors: 4,
            ..Default::default()
        };
        let mut s = Session::new(&dir);
        let r = w.preview_request().expect("preview");
        assert_eq!(r.params["dryRun"], true);
        let result = s.execute(r.command, Some(&r.params)).expect("preview runs");
        assert!(s.design().objects.is_empty(), "a preview adds nothing");
        w.set_plan(&result);
        assert!(w.plan_current());
        let plan = w.plan.clone().expect("plan");
        assert_eq!(plan.regions.len(), 3, "{result}");
        assert_eq!(plan.map.len(), plan.map_size.0 * plan.map_size.1);
        // The red block's middle (15, 20 of 60 × 40) is a region; the white corner none.
        let red = w.region_at([15.0 / 60.0, 20.0 / 40.0]).expect("red region");
        assert_eq!(plan.regions[red].color, "#ed171f");
        assert_eq!(w.region_at([0.01, 0.01]), None);
        // Click: left out; the conversion leaves it out.
        assert_eq!(w.toggle_at([0.25, 0.5]), Some(red));
        assert_eq!(w.region_kind(red), "skip");
        let r = w.request().expect("request");
        assert_eq!(
            r.params["overrides"],
            json!([{"region": red, "type": "skip"}])
        );
        let done = s.execute(r.command, Some(&r.params)).expect("converts");
        assert_eq!(CommandRequest::created_ids(&done).len(), 2);
        // Click again: back to Auto Punch's own stitch.
        w.toggle_at([0.25, 0.5]);
        assert!(w.overrides.is_empty());
        // Most of the area left out: the review note.
        w.set_region(red, "skip");
        assert!(w.review_note().is_some_and(|n| n.contains("left out")));
        w.set_region(red, "auto");
        assert!(w.review_note().is_none());
        // A changed setting: the preview no longer matches, and its choices are not sent.
        w.set_region(red, "satin");
        w.colors = 3;
        assert!(!w.plan_current());
        assert!(
            w.request()
                .expect("request")
                .params
                .get("overrides")
                .is_none()
        );
        w.colors = 4;
        // The colour order: moving a colour sends `order` and clears the choices.
        let order = w.colour_order();
        assert_eq!(order, plan.colors);
        w.move_colour(1, true);
        assert!(w.overrides.is_empty());
        let r = w.request().expect("request");
        assert_eq!(r.params["order"], json!([order[1], order[0], order[2]]));
        let done = s
            .execute(r.command, Some(&r.params))
            .expect("order converts");
        assert_eq!(done["colors"][0], json!(order[1]));
        // The preset, in OpenStitch mode only.
        w.preset = Some("logo");
        assert!(w.request().expect("request").params.get("preset").is_none());
        w.open_stitch = true;
        let r = w.request().expect("request");
        assert_eq!(r.params["preset"], "logo");
        assert!(s.execute(r.command, Some(&r.params)).is_ok());
    }

    /// The dialog's presets are the engine's, in its order.
    #[test]
    fn presets_are_the_engines() {
        let mut s = Session::new(".");
        let list = s.execute("digitize.presets", None).expect("presets");
        let engine: Vec<&str> = list
            .as_array()
            .expect("list")
            .iter()
            .map(|p| p["name"].as_str().expect("name"))
            .collect();
        let ours: Vec<&str> = PRESETS.iter().map(|p| p.0).collect();
        assert_eq!(ours, engine);
    }
}
