//! Inspector model for the selection and the commands its controls issue.

use super::request::CommandRequest;
use openstitch_core::{PEC_CHART, Thread, nearest_index, pec_basic_chart};
use openstitch_design::patterns::{self, Pattern, PatternKind};
use openstitch_design::{
    Arrange, ChainShape, Design, Feather, FeatherSide, FillStitchType, LineSewing, LineType,
    RegionSewing, RegionType, RunningStitchPath, UnderlayStyle,
    column::{CornerStyle, JaggedMode, SatinShaping},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// A property across several objects.
#[derive(Clone, Debug, PartialEq)]
pub enum Common<T> {
    Empty,
    Same(T),
    Mixed,
}

impl<T: PartialEq> Common<T> {
    pub fn of(values: impl IntoIterator<Item = T>) -> Self {
        let mut out = Common::Empty;
        for v in values {
            out = match out {
                Common::Empty => Common::Same(v),
                Common::Same(s) if s == v => Common::Same(s),
                _ => return Common::Mixed,
            };
        }
        out
    }

    pub fn same(&self) -> Option<&T> {
        match self {
            Common::Same(v) => Some(v),
            _ => None,
        }
    }
}

/// The rows the Basic view of Sewing Attributes keeps (#553); Advanced shows every row.
pub const BASIC_PARAMS: &[&str] = &[
    "angle",
    "density",
    "stitchLength",
    "pullCompensation",
    "underlay",
    "pitch",
    "runTimes",
    "width",
    "pattern",
    "motif",
];

/// Every region sew type, preceded by "None" (not sewn).
pub fn region_types() -> Vec<(Option<RegionType>, &'static str)> {
    std::iter::once((None, "None"))
        .chain(RegionType::ALL.into_iter().map(|k| (Some(k), k.label())))
        .collect()
}

/// Every line sew type, preceded by "None".
pub fn line_types() -> Vec<(Option<LineType>, &'static str)> {
    std::iter::once((None, "None"))
        .chain(LineType::ALL.into_iter().map(|k| (Some(k), k.label())))
        .collect()
}

pub fn region_name(k: Option<RegionType>) -> &'static str {
    k.map_or("none", RegionType::id)
}

pub fn line_name(k: Option<LineType>) -> &'static str {
    k.map_or("none", LineType::id)
}

/// One inspector control for a sewing attribute; changing it sends `{param: value}`.
#[derive(Clone, Debug, PartialEq)]
pub enum Control {
    Number {
        label: &'static str,
        /// Draft key (unique per control).
        key: &'static str,
        param: &'static str,
        value: f64,
        range: (f64, f64),
        speed: f64,
        suffix: &'static str,
        integer: bool,
    },
    Choice {
        label: &'static str,
        param: &'static str,
        value: String,
        /// (parameter value, label).
        options: Vec<(String, String)>,
    },
    Toggle {
        label: &'static str,
        param: &'static str,
        value: bool,
    },
}

impl Control {
    /// The label shown beside the control.
    pub fn label(&self) -> &'static str {
        match self {
            Control::Number { label, .. }
            | Control::Choice { label, .. }
            | Control::Toggle { label, .. } => label,
        }
    }

    /// Whether the Basic view of Sewing Attributes shows this control (#553): the settings most
    /// designs change.
    pub fn basic(&self) -> bool {
        BASIC_PARAMS.contains(&self.param())
    }

    /// Whether the property search `filter` shows this control (#550).
    pub fn shown(&self, filter: &str) -> bool {
        matches_filter(filter, &[self.label(), self.param()])
    }

    /// The command parameter this control sets.
    pub fn param(&self) -> &'static str {
        match self {
            Control::Number { param, .. }
            | Control::Choice { param, .. }
            | Control::Toggle { param, .. } => param,
        }
    }

    /// The command parameters for a new value of this control.
    pub fn params(&self, value: Value) -> Value {
        let param = self.param();
        let value = match (self, value.as_f64()) {
            (Control::Number { integer: true, .. }, Some(v)) => json!(v.round() as i64),
            _ => value,
        };
        json!({ param: value })
    }
}

#[allow(clippy::too_many_arguments)]
fn num(
    label: &'static str,
    key: &'static str,
    param: &'static str,
    value: f64,
    range: (f64, f64),
    speed: f64,
    suffix: &'static str,
) -> Control {
    Control::Number {
        label,
        key,
        param,
        value,
        range,
        speed,
        suffix,
        integer: false,
    }
}

/// A whole-number field (sent as an integer).
fn whole(
    label: &'static str,
    key: &'static str,
    param: &'static str,
    value: u32,
    range: (u32, u32),
    suffix: &'static str,
) -> Control {
    Control::Number {
        label,
        key,
        param,
        value: f64::from(value),
        range: (f64::from(range.0), f64::from(range.1)),
        speed: 0.05,
        suffix,
        integer: true,
    }
}

fn times(
    label: &'static str,
    key: &'static str,
    param: &'static str,
    value: u32,
    max: u32,
) -> Control {
    Control::Number {
        label,
        key,
        param,
        value: f64::from(value),
        range: (1.0, f64::from(max)),
        speed: 0.05,
        suffix: "×",
        integer: true,
    }
}

fn arrange(value: Arrange) -> Control {
    let id = |a: Arrange| match a {
        Arrange::Normal => "normal",
        Arrange::Mirror => "mirror",
        Arrange::Alternate => "alternate",
    };
    Control::Choice {
        label: "Arrange",
        param: "arrange",
        value: id(value).into(),
        options: [
            (Arrange::Normal, "Normal"),
            (Arrange::Mirror, "Mirror"),
            (Arrange::Alternate, "Alternate"),
        ]
        .iter()
        .map(|(a, l)| (id(*a).to_string(), l.to_string()))
        .collect(),
    }
}

/// Library patterns of `kind` as choice options.
pub fn pattern_options(kind: PatternKind) -> Vec<(String, String)> {
    pattern_options_with(kind, &[])
}

/// Bundled patterns of `kind`, then the design's own (marked "(user)").
pub fn pattern_options_with(kind: PatternKind, user: &[Pattern]) -> Vec<(String, String)> {
    patterns::library()
        .iter()
        .filter(|p| p.kind == kind)
        .map(|p| (p.id.clone(), p.name.clone()))
        .chain(
            user.iter()
                .filter(|p| p.kind == kind)
                .map(|p| (p.id.clone(), format!("{} (user)", p.name))),
        )
        .collect()
}

fn toggle(label: &'static str, param: &'static str, value: bool) -> Control {
    Control::Toggle {
        label,
        param,
        value,
    }
}

fn choice(
    label: &'static str,
    param: &'static str,
    value: &str,
    options: &[(&str, &str)],
) -> Control {
    Control::Choice {
        label,
        param,
        value: value.into(),
        options: options
            .iter()
            .map(|(v, l)| (v.to_string(), l.to_string()))
            .collect(),
    }
}

/// The motif band rows (#544, beyond PE-DESIGN) of a satin region (`line` false) or a satin
/// line: the motif, and once one is chosen each copy's length and mirroring.
fn band_controls(
    band: Option<&openstitch_design::motif_band::MotifBand>,
    user: &[Pattern],
    line: bool,
) -> Vec<Control> {
    let mut v = vec![Control::Choice {
        label: "Motif band",
        param: "motifBand",
        value: band.map_or_else(|| "none".into(), |b| b.motif.clone()),
        options: std::iter::once(("none".to_string(), "None (satin)".to_string()))
            .chain(pattern_options_with(PatternKind::Motif, user))
            .collect(),
    }];
    if let Some(b) = band {
        v.push(num(
            "Band copy length",
            if line {
                "line.bandTile"
            } else {
                "region.bandTile"
            },
            "bandTile",
            b.tile.unwrap_or(0.0),
            (0.0, 50.0),
            0.1,
            " mm",
        ));
        v.push(toggle("Mirror every other copy", "bandMirror", b.mirror));
    }
    v
}

/// The satin shaping rows (#520-#522, beyond PE-DESIGN) of a Manual Punch column (`line`
/// false, `pull` its single compensation) or a zigzag line.
fn shaping_controls(s: &SatinShaping, pull: f64, line: bool) -> Vec<Control> {
    let key = |region: &'static str, line_key: &'static str| if line { line_key } else { region };
    let c = &s.corners;
    let mut v = vec![choice(
        "Corners",
        "cornerStyle",
        c.style.id(),
        &[
            ("turn", "Turned (PE-DESIGN)"),
            ("auto", "By angle (mitre, cap)"),
            ("mitre", "Mitre"),
            ("cap", "Cap"),
        ],
    )];
    if c.style != CornerStyle::Turn {
        v.push(num(
            "Mitre below",
            key("region.mitreBelow", "line.mitreBelow"),
            "mitreBelow",
            c.mitre_below,
            (10.0, 170.0),
            1.0,
            "°",
        ));
        if c.style == CornerStyle::Auto {
            v.push(num(
                "Cap below",
                key("region.capBelow", "line.capBelow"),
                "capBelow",
                c.cap_below,
                (5.0, 120.0),
                1.0,
                "°",
            ));
        }
        v.push(num(
            "Corner overlap",
            key("region.cornerOverlap", "line.cornerOverlap"),
            "cornerOverlap",
            c.overlap,
            (0.0, 2.0),
            0.01,
            " mm",
        ));
        if c.style != CornerStyle::Mitre {
            v.push(num(
                "Cap max length",
                key("region.capMaxLength", "line.capMaxLength"),
                "capMaxLength",
                c.cap_max_length,
                (2.0, 12.0),
                0.05,
                " mm",
            ));
        }
    }
    v.push(num(
        "Short stitches below",
        key("region.shortStitchDistance", "line.shortStitchDistance"),
        "shortStitchDistance",
        s.short_stitches.distance,
        (0.0, 1.0),
        0.01,
        " mm",
    ));
    if s.short_stitches.distance > 0.0 {
        v.push(num(
            "Short stitch inset",
            key("region.shortStitchInset", "line.shortStitchInset"),
            "shortStitchInset",
            s.short_stitches.inset,
            (5.0, 80.0),
            0.5,
            " %",
        ));
    }
    v.push(choice(
        "Satin style",
        "satinStyle",
        s.style.id(),
        &[
            ("normal", "Normal"),
            ("sStitch", "S-stitch"),
            ("open", "Open zigzag"),
            ("double", "Double"),
        ],
    ));
    v.push(choice(
        "Jagged edge",
        "jaggedEdge",
        match s.jagged.map(|j| j.mode) {
            None => "none",
            Some(JaggedMode::In) => "in",
            Some(JaggedMode::Out) => "out",
            Some(JaggedMode::Both) => "both",
        },
        &[
            ("none", "None"),
            ("in", "In"),
            ("out", "Out"),
            ("both", "In and out"),
        ],
    ));
    if !line {
        v.push(num(
            "Density measured at",
            "region.spacingAt",
            "spacingAt",
            s.spacing_at.unwrap_or(1.0),
            (0.0, 1.0),
            0.01,
            "",
        ));
    }
    v.push(Control::Toggle {
        label: "Raised (padding)",
        param: "raised",
        value: s.raised.is_some(),
    });
    let k = &s.compensation;
    v.push(num(
        "Pull left",
        key("region.pullLeft", "line.pullLeft"),
        "pullLeft",
        k.left.unwrap_or(pull),
        (0.0, 2.0),
        0.01,
        " mm",
    ));
    v.push(num(
        "Pull right",
        key("region.pullRight", "line.pullRight"),
        "pullRight",
        k.right.unwrap_or(pull),
        (0.0, 2.0),
        0.01,
        " mm",
    ));
    v.push(num(
        "Pull % of width",
        key("region.pullPercent", "line.pullPercent"),
        "pullPercent",
        k.percent,
        (0.0, 20.0),
        0.1,
        " %",
    ));
    v
}

/// The controls the inspector shows for region sewing `r`, by sew type.
pub fn region_controls(r: &RegionSewing) -> Vec<Control> {
    region_controls_with(r, &[])
}

/// [`region_controls`] offering the design's own patterns too.
pub fn region_controls_with(r: &RegionSewing, user: &[Pattern]) -> Vec<Control> {
    let mut v = Vec::new();
    let row_based = matches!(r.kind, RegionType::Fill | RegionType::Satin);
    if row_based {
        v.push(toggle("Auto direction", "autoDirection", r.auto_direction));
    }
    // PE-DESIGN offers Half Stitch with Auto direction only (greyed otherwise, #372).
    if r.kind == RegionType::Satin && r.auto_direction {
        v.push(toggle("Half Stitch", "halfStitch", r.half_stitch));
    }
    let centred = matches!(
        r.kind,
        RegionType::ConcentricCircle
            | RegionType::Radial
            | RegionType::Spiral
            | RegionType::FlexibleSpiral
            | RegionType::Contour
    );
    // PE-DESIGN 11.50 has no Direction for cross stitch: its grid is on the page axes (#614).
    if !r.auto_direction && !centred && r.kind != RegionType::CrossStitch {
        v.push(num(
            "Direction",
            "region.angle",
            "angle",
            r.angle,
            (-360.0, 360.0),
            1.0,
            "°",
        ));
    }
    let uses_density = !matches!(
        r.kind,
        RegionType::CrossStitch
            | RegionType::Stippling
            | RegionType::MotifFill
            | RegionType::DecorativeFill
            | RegionType::NetFill
            | RegionType::FlexibleSpiral
            | RegionType::Contour
    );
    if uses_density {
        let unit = if r.kind == RegionType::ZigzagNetFill {
            " /mm"
        } else {
            " rows/mm"
        };
        v.push(num(
            "Density",
            "region.density",
            "density",
            r.density,
            r.kind.density_range(),
            0.05,
            unit,
        ));
    }
    if r.kind == RegionType::Fill {
        v.push(num(
            "Stitch length",
            "region.stitchLength",
            "stitchLength",
            r.stitch_length,
            (1.0, 10.0),
            0.05,
            " mm",
        ));
    }
    if row_based {
        v.push(num(
            "Pull compensation",
            "region.pullCompensation",
            "pullCompensation",
            r.pull_compensation,
            (0.0, 2.0),
            0.01,
            " mm",
        ));
        v.push(toggle("Underlay", "underlay", r.underlay));
        if r.underlay {
            let style = r.underlay_style.id();
            v.push(choice(
                "Underlay style",
                "underlayStyle",
                style,
                &[
                    ("edge", "Edge"),
                    ("zigzag", "Zigzag"),
                    ("edgeZigzag", "Edge and zigzag"),
                    ("double", "Double"),
                    ("edgeFill", "Edge and fill"),
                    ("cross", "Edge and cross-hatch (PE-DESIGN type 2)"),
                    ("auto", "Automatic (by width)"),
                    ("centreWalk", "Centre walk"),
                ],
            ));
            v.push(choice(
                "Underlay density",
                "underlayDensity",
                r.underlay_density.name(),
                &[("dense", "Dense"), ("medium", "Medium"), ("light", "Light")],
            ));
            if r.underlay_style != UnderlayStyle::Edge {
                v.push(num(
                    "Underlay spacing",
                    "region.underlaySpacing",
                    "underlaySpacing",
                    r.underlay_spacing_mm(),
                    (1.0, 10.0),
                    0.05,
                    " mm",
                ));
            }
        }
        // OpenStitch's satin shaping (#520-#522) on a Manual Punch satin column.
        if r.kind == RegionType::Satin && r.direction_lines.len() >= 2 {
            v.extend(shaping_controls(
                &r.satin_shaping,
                r.pull_compensation,
                false,
            ));
        }
        // The motif band (#544, beyond PE-DESIGN) on a skeleton or Manual Punch satin.
        if r.kind == RegionType::Satin && (r.skeleton.is_some() || r.direction_lines.len() >= 2) {
            v.extend(band_controls(r.motif_band.as_ref(), user, false));
        }
        // The guide's exclusions: Gradation and Feathered edge exclude each other and Auto
        // direction.
        if r.gradation_unavailable().is_none() {
            v.push(toggle(
                "Gradation",
                "gradationPattern",
                r.gradation.is_some(),
            ));
            match r.gradation {
                // Saved before gradation patterns: one far-side percentage.
                Some(g) if g.steps.is_none() => v.push(num(
                    "Gradation (far side)",
                    "region.gradation",
                    "gradation",
                    g.min,
                    (5.0, 100.0),
                    0.5,
                    " %",
                )),
                Some(g) => {
                    v.push(num(
                        "Gradation max",
                        "region.gradationMax",
                        "gradationMax",
                        g.max,
                        (5.0, 100.0),
                        1.0,
                        " %",
                    ));
                    v.push(num(
                        "Gradation min",
                        "region.gradationMin",
                        "gradationMin",
                        g.min,
                        (5.0, 100.0),
                        1.0,
                        " %",
                    ));
                    v.push(num(
                        "Gradation steps",
                        "region.gradationSteps",
                        "gradationSteps",
                        g.steps.unwrap_or(9) as f64,
                        (2.0, 20.0),
                        1.0,
                        "",
                    ));
                }
                None => {}
            }
        }
        if r.feather_unavailable().is_none() {
            let feather = r.feather.unwrap_or(Feather {
                side: FeatherSide::Start,
                width: 0.0,
            });
            v.push(num(
                "Feathered edge",
                "region.featherWidth",
                "featherWidth",
                feather.width,
                (0.0, 10.0),
                0.05,
                " mm",
            ));
            if r.feather.is_some() {
                let side = match feather.side {
                    FeatherSide::Start => "start",
                    FeatherSide::End => "end",
                    FeatherSide::Both => "both",
                };
                v.push(choice(
                    "Feather side",
                    "featherSide",
                    side,
                    &[("start", "Start"), ("end", "End"), ("both", "Both")],
                ));
            }
        }
        if r.kind == RegionType::Satin {
            v.push(num(
                "Split stitches over",
                "region.splitLength",
                "splitLength",
                r.split_length.unwrap_or(0.0),
                (0.0, 12.0),
                0.05,
                " mm",
            ));
        }
    }
    v.extend(region_type_controls(r, user));
    v.extend(travel_control(r));
    v
}

/// Running Stitch Path for fill and satin where the guide offers it (Inside not with
/// gradation; nothing with Auto direction or a feathered edge); a travel on/off toggle for
/// the other types and for fill and satin that jump (designs saved before the attribute).
fn travel_control(r: &RegionSewing) -> Option<Control> {
    match r.running_stitch_path {
        Some(_) if r.running_stitch_path_available() => Some(Control::Choice {
            label: "Running stitch path",
            param: "runningStitchPath",
            value: r.travel_path().unwrap_or_default().id().into(),
            options: [
                (RunningStitchPath::Inside, "Inside of region"),
                (RunningStitchPath::AlongOutline, "Along outline"),
                (RunningStitchPath::OnOutline, "On outline"),
            ]
            .into_iter()
            .filter(|(p, _)| r.gradation.is_none() || *p != RunningStitchPath::Inside)
            .map(|(p, l)| (p.id().to_string(), l.to_string()))
            .collect(),
        }),
        Some(_) if matches!(r.kind, RegionType::Fill | RegionType::Satin) => None,
        travel => Some(toggle("Travel inside", "travel", travel.is_some())),
    }
}

/// Net patterns as choice options (zigzag net offers the first four).
fn net_options(zigzag: bool) -> Vec<(String, String)> {
    let n = if zigzag {
        patterns::ZIGZAG_NET_PATTERNS
    } else {
        patterns::NET_PATTERNS.len()
    };
    patterns::NET_PATTERNS[..n]
        .iter()
        .map(|p| (p.id.to_string(), p.name.to_string()))
        .collect()
}

/// Controls specific to the region types beyond fill and satin.
fn region_type_controls(r: &RegionSewing, user: &[Pattern]) -> Vec<Control> {
    let (pw, ph) = r.pattern_size_mm();
    let pattern = |kind: PatternKind| Control::Choice {
        label: "Pattern",
        param: "pattern",
        value: r
            .pattern
            .clone()
            .unwrap_or_else(|| patterns::default_for(kind).id.clone()),
        options: pattern_options_with(kind, user),
    };
    let size = || {
        vec![
            num(
                "Pattern width",
                "region.patternWidth",
                "patternWidth",
                pw,
                (0.5, 50.0),
                0.05,
                " mm",
            ),
            num(
                "Pattern height",
                "region.patternHeight",
                "patternHeight",
                ph,
                (0.5, 50.0),
                0.05,
                " mm",
            ),
        ]
    };
    let spacing = |label, lo| {
        num(
            label,
            "region.spacing",
            "spacing",
            r.spacing_mm(),
            (lo, 20.0),
            0.05,
            " mm",
        )
    };
    let pitch = num(
        "Run pitch",
        "region.runPitch",
        "runPitch",
        r.run_pitch_mm(),
        (1.0, 10.0),
        0.05,
        " mm",
    );
    let percent = |label, key, param, value: Option<f64>, default: f64, max: f64| {
        num(
            label,
            key,
            param,
            value.unwrap_or(default),
            (0.0, max),
            0.5,
            " %",
        )
    };
    let mut v = Vec::new();
    match r.kind {
        RegionType::Fill => {
            v.push(percent(
                "Frequency",
                "region.frequency",
                "frequency",
                r.frequency,
                33.0,
                50.0,
            ));
            v.push(choice(
                "Stagger",
                "stagger",
                r.stagger.id(),
                &[
                    ("regular", "Regular (Frequency)"),
                    ("random", "Random (seeded) (+)"),
                    ("steps", "Steps (+)"),
                    ("wave", "Wave (+)"),
                    ("triangle", "Triangle (+)"),
                    ("arc", "Arc (+)"),
                ],
            ));
            // OpenStitch's named needle-hole textures (#537); "custom" when the settings
            // match none of them.
            let mut textures: Vec<(&str, &str)> = openstitch_design::FILL_TEXTURES
                .iter()
                .map(|t| (t.id, t.label))
                .collect();
            let texture = r.texture().map_or("custom", |t| t.id);
            if texture == "custom" {
                textures.push(("custom", "Custom"));
            }
            v.push(choice("Texture", "texture", texture, &textures));
            match r.stagger {
                openstitch_design::StaggerKind::Steps => {
                    v.push(whole(
                        "Steps",
                        "region.staggerSteps",
                        "staggerSteps",
                        r.stagger_steps.unwrap_or(3),
                        (1, 16),
                        "",
                    ));
                    v.push(whole(
                        "Rows each",
                        "region.staggerRepeat",
                        "staggerRepeat",
                        r.stagger_repeat.unwrap_or(1),
                        (1, 16),
                        "",
                    ));
                }
                openstitch_design::StaggerKind::Wave
                | openstitch_design::StaggerKind::Triangle
                | openstitch_design::StaggerKind::Arc => {
                    v.push(percent(
                        "Shift height",
                        "region.staggerAmplitude",
                        "staggerAmplitude",
                        r.stagger_amplitude,
                        50.0,
                        100.0,
                    ));
                    v.push(whole(
                        "Period",
                        "region.staggerPeriod",
                        "staggerPeriod",
                        r.stagger_period.unwrap_or(8),
                        (2, 64),
                        " rows",
                    ));
                }
                _ => {}
            }
            v.push(choice(
                "Stitch type",
                "stitchType",
                match r.stitch_type {
                    FillStitchType::One => "1",
                    FillStitchType::Two => "2",
                },
                &[("1", "1: flat rows"), ("2", "2: sloped rows")],
            ));
        }
        RegionType::Satin => v.extend(satin_hand_controls(r)),
        RegionType::ProgrammableFill => {
            v.push(pattern(PatternKind::Fill));
            v.extend(size());
            v.push(percent(
                "Row offset",
                "region.rowOffset",
                "rowOffset",
                r.row_offset,
                0.0,
                100.0,
            ));
            v.push(percent(
                "Column offset",
                "region.columnOffset",
                "columnOffset",
                r.column_offset,
                0.0,
                100.0,
            ));
            v.push(percent(
                "Frequency",
                "region.frequency",
                "frequency",
                r.frequency,
                33.0,
                50.0,
            ));
            v.push(toggle("Base sewing", "baseSewing", r.base_sewing));
        }
        RegionType::MotifFill => {
            v.push(pattern(PatternKind::Motif));
            v.extend(size());
            v.push(spacing("Spacing", 0.0));
            v.push(percent(
                "Row offset",
                "region.rowOffset",
                "rowOffset",
                r.row_offset,
                50.0,
                100.0,
            ));
            v.push(pitch);
        }
        RegionType::Piping => {
            v.push(pitch);
            v.push(percent(
                "Frequency",
                "region.frequency",
                "frequency",
                r.frequency,
                33.0,
                50.0,
            ));
        }
        RegionType::CrossStitch => {
            v.push(spacing("Size", 0.5));
            v.push(choice(
                "Times",
                "times",
                r.cross_times().id(),
                &[
                    ("single", "Single"),
                    ("double", "Double"),
                    ("triple", "Triple"),
                ],
            ));
            v.push(choice(
                "Cross",
                "crossKind",
                r.cross_kind.id(),
                &[
                    ("full", "Full X"),
                    ("half", "Half / (+)"),
                    ("upright", "Upright + (+)"),
                    ("smyrna", "Smyrna X+ (+)"),
                ],
            ));
        }
        RegionType::ConcentricCircle | RegionType::Radial | RegionType::Spiral => {
            v.push(pitch);
            if r.kind != RegionType::Radial {
                v.push(toggle("Start inside", "startInside", r.start_inside));
            }
            if r.kind == RegionType::Spiral {
                v.push(toggle(
                    "Counter-clockwise",
                    "counterclockwise",
                    r.counterclockwise,
                ));
            }
        }
        RegionType::FlexibleSpiral => {
            v.push(spacing("Spacing", 0.5));
            v.push(pitch);
            v.push(toggle("Start inside", "startInside", r.start_inside));
            v.push(toggle(
                "Counter-clockwise",
                "counterclockwise",
                r.counterclockwise,
            ));
        }
        RegionType::Stippling => {
            v.push(spacing("Spacing", 0.5));
            v.push(pitch);
        }
        RegionType::NetFill | RegionType::ZigzagNetFill => {
            let zigzag = r.kind == RegionType::ZigzagNetFill;
            v.push(Control::Choice {
                label: "Pattern",
                param: "pattern",
                value: r
                    .pattern
                    .clone()
                    .unwrap_or_else(|| patterns::NET_PATTERNS[0].id.to_string()),
                options: net_options(zigzag),
            });
            v.push(spacing("Spacing", 0.5));
            v.push(pitch);
            if zigzag {
                v.push(num(
                    "Zigzag width",
                    "region.patternWidth",
                    "patternWidth",
                    pw,
                    (0.5, 50.0),
                    0.05,
                    " mm",
                ));
                v.push(toggle("Under sewing", "underlay", r.underlay));
            }
        }
        RegionType::DecorativeFill => {
            v.push(pattern(PatternKind::Decorative));
            v.extend(size());
            v.push(pitch);
            v.push(percent(
                "Random shift",
                "region.randomShift",
                "randomShift",
                r.random_shift,
                0.0,
                50.0,
            ));
        }
        RegionType::Contour => {
            v.push(choice(
                "Contour",
                "contourStrategy",
                r.contour_strategy.id(),
                &[
                    ("rings", "Rings"),
                    ("singleSpiral", "Single spiral"),
                    ("doubleSpiral", "Double spiral"),
                ],
            ));
            v.push(spacing("Spacing", 0.5));
            v.push(pitch);
            v.push(choice(
                "Corners",
                "contourJoin",
                match r.contour_join {
                    openstitch_design::ContourJoin::Round => "round",
                    openstitch_design::ContourJoin::Miter => "miter",
                },
                &[("round", "Round"), ("miter", "Mitred")],
            ));
            v.push(num(
                "Echo lines",
                "region.echoLines",
                "echoLines",
                r.echo_lines as f64,
                (0.0, 30.0),
                1.0,
                "",
            ));
            v.push(toggle("Start inside", "startInside", r.start_inside));
            v.push(toggle(
                "Counter-clockwise",
                "counterclockwise",
                r.counterclockwise,
            ));
        }
    }
    v
}

/// The controls the inspector shows for line sewing `l`, by sew type.
pub fn line_controls(l: &LineSewing) -> Vec<Control> {
    line_controls_with(l, &[])
}

/// [`line_controls`] offering the design's own motifs too.
pub fn line_controls_with(l: &LineSewing, user: &[Pattern]) -> Vec<Control> {
    let pitch = num(
        "Run pitch",
        "line.pitch",
        "pitch",
        l.pitch,
        (1.0, 10.0),
        0.05,
        " mm",
    );
    let width = |label| {
        num(
            label,
            "line.width",
            "width",
            l.width,
            (0.5, 10.0),
            0.05,
            " mm",
        )
    };
    let spacing = |label, lo| {
        num(
            label,
            "line.spacing",
            "spacing",
            l.spacing_mm(),
            (lo, 20.0),
            0.05,
            " mm",
        )
    };
    let length = num(
        "Stitch length",
        "line.pitch",
        "pitch",
        l.pitch,
        (1.0, 10.0),
        0.05,
        " mm",
    );
    match l.kind {
        LineType::Running => {
            let mut v = vec![
                length,
                times("Run times", "line.runTimes", "runTimes", l.run_times, 5),
            ];
            v.extend(profile_controls(l));
            v.extend(hand_controls(l, true));
            v
        }
        LineType::Triple => {
            let mut v = vec![length];
            v.extend(hand_controls(l, false));
            v
        }
        LineType::BackStitch => vec![length],
        LineType::Scribble => vec![
            width("Width"),
            num(
                "Point spacing",
                "line.pitch",
                "pitch",
                l.pitch,
                (1.0, 10.0),
                0.05,
                " mm",
            ),
            times("Repetitions", "line.runTimes", "runTimes", l.run_times, 5),
            num(
                "Wave length",
                "line.noiseScale",
                "noiseScale",
                l.noise_scale.unwrap_or(5.0),
                (1.0, 50.0),
                0.1,
                " mm",
            ),
            seed_control("line.seed", l.seed.unwrap_or(0)),
        ],
        LineType::Zigzag => vec![
            width("Width"),
            num(
                "Density",
                "line.density",
                "density",
                l.density,
                (0.1, 14.0),
                0.05,
                " /mm",
            ),
            Control::Toggle {
                label: "Under sewing",
                param: "underSewing",
                value: l.underlay,
            },
            Control::Toggle {
                label: "Half stitch",
                param: "halfStitch",
                value: l.half_stitch,
            },
            choice(
                "Under sewing style",
                "underlayStyle",
                l.underlay_style.id(),
                &[
                    ("edge", "Edge runs (PE-DESIGN)"),
                    ("centreWalk", "Centre walk"),
                    ("edgeZigzag", "Edge and zigzag"),
                    ("auto", "Automatic (by width)"),
                ],
            ),
        ]
        .into_iter()
        .chain(shaping_controls(&l.satin_shaping, 0.0, true))
        .collect(),
        LineType::Motif => {
            let mut v = vec![
                Control::Choice {
                    label: "Motif",
                    param: "motif",
                    value: l
                        .motif
                        .clone()
                        .unwrap_or_else(|| patterns::default_for(PatternKind::Motif).id.clone()),
                    options: pattern_options_with(PatternKind::Motif, user),
                },
                width("Width"),
                num(
                    "Height",
                    "line.height",
                    "height",
                    l.height.unwrap_or(3.0),
                    (0.5, 20.0),
                    0.05,
                    " mm",
                ),
                spacing("Spacing", 0.0),
                pitch,
                arrange(l.arrange),
                // #544, beyond PE-DESIGN: a second motif in turn, one repeating group, free ends.
                Control::Choice {
                    label: "Motif in turn",
                    param: "motifs",
                    value: l.motifs.first().cloned().unwrap_or_else(|| "none".into()),
                    options: std::iter::once(("none".to_string(), "None".to_string()))
                        .chain(pattern_options_with(PatternKind::Motif, user))
                        .collect(),
                },
                Control::Number {
                    label: "Group size",
                    key: "line.groupSize",
                    param: "groupSize",
                    value: f64::from(l.motif_groups.first().map_or(0, |g| g.0)),
                    range: (0.0, 20.0),
                    speed: 0.05,
                    suffix: "",
                    integer: true,
                },
                num(
                    "Free start",
                    "line.freeStart",
                    "freeStart",
                    l.free_start.unwrap_or(0.0),
                    (0.0, 100.0),
                    0.05,
                    " mm",
                ),
                num(
                    "Free end",
                    "line.freeEnd",
                    "freeEnd",
                    l.free_end.unwrap_or(0.0),
                    (0.0, 100.0),
                    0.05,
                    " mm",
                ),
            ];
            if let Some(&(_, g)) = l.motif_groups.first() {
                v.push(num(
                    "Group gap",
                    "line.groupGap",
                    "groupGap",
                    g,
                    (0.0, 50.0),
                    0.05,
                    " mm",
                ));
            }
            v
        }
        LineType::Stem => vec![
            width("Width"),
            spacing("Spacing", 0.5),
            num(
                "Angle",
                "line.angle",
                "angle",
                l.angle.unwrap_or(45.0),
                (45.0, 135.0),
                1.0,
                "°",
            ),
            Control::Toggle {
                label: "Single stitch",
                param: "single",
                value: l.single,
            },
        ],
        LineType::Candlewick => vec![
            width("Size"),
            num(
                "Density",
                "line.density",
                "density",
                l.density,
                (0.5, 10.0),
                0.05,
                " /mm",
            ),
            spacing("Spacing", 0.5),
            pitch,
        ],
        LineType::EStitch | LineType::VStitch => {
            let mut v = vec![
                spacing("Interval", 0.5),
                width("Stroke width"),
                pitch,
                times("Run times", "line.runTimes", "runTimes", l.run_times, 6),
            ];
            if l.kind == LineType::EStitch {
                v.push(times(
                    "Stroke times",
                    "line.strokeTimes",
                    "strokeTimes",
                    l.stroke_times.unwrap_or(1),
                    3,
                ));
                // #543: prongs slanted from square (beyond PE-DESIGN; 0 is PE-DESIGN's).
                v.push(num(
                    "Prong angle",
                    "line.prongAngle",
                    "prongAngle",
                    l.prong_angle.unwrap_or(0.0),
                    (-60.0, 60.0),
                    0.5,
                    "°",
                ));
            }
            v.push(arrange(l.arrange));
            v
        }
        LineType::Chain => vec![
            Control::Choice {
                label: "Stitch type",
                param: "chainShape",
                value: match l.chain_shape {
                    ChainShape::Triangle => "triangle".into(),
                    ChainShape::Diamond => "diamond".into(),
                },
                options: vec![
                    ("triangle".into(), "Triangle".into()),
                    ("diamond".into(), "Diamond".into()),
                ],
            },
            width("Size"),
            times("Times", "line.runTimes", "runTimes", l.run_times, 6),
            arrange(l.arrange),
        ],
        k @ (LineType::Chevron
        | LineType::Cretan
        | LineType::Feather
        | LineType::Overlock
        | LineType::AlternatingTriangles
        | LineType::DoubleZigzag
        | LineType::Lattice
        | LineType::Stripes) => {
            // Decorative hand-embroidery lines (#543, beyond PE-DESIGN).
            let mut v = vec![width("Band width"), spacing("Period", 0.5)];
            match k {
                LineType::Chevron | LineType::Cretan | LineType::Feather => v.push(num(
                    if k == LineType::Chevron {
                        "Bar length"
                    } else {
                        "Side stitch"
                    },
                    "line.height",
                    "height",
                    l.height.unwrap_or(1.5),
                    (0.5, 20.0),
                    0.05,
                    " mm",
                )),
                LineType::Lattice => v.push(Control::Number {
                    label: "Strands",
                    key: "line.strands",
                    param: "strands",
                    value: f64::from(l.strands.unwrap_or(3)),
                    range: (3.0, 7.0),
                    speed: 0.05,
                    suffix: "",
                    integer: true,
                }),
                LineType::Stripes => {
                    v.push(Control::Number {
                        label: "Stripes",
                        key: "line.strands",
                        param: "strands",
                        value: f64::from(l.strands.unwrap_or(3)),
                        range: (2.0, 12.0),
                        speed: 0.05,
                        suffix: "",
                        integer: true,
                    });
                    v.push(pitch.clone());
                }
                LineType::AlternatingTriangles => v.push(pitch.clone()),
                _ => {}
            }
            v
        }
        LineType::Ripple => vec![
            // #541, beyond PE-DESIGN: the target is dragged on the canvas.
            Control::Number {
                label: "Copies",
                key: "line.strands",
                param: "strands",
                value: f64::from(l.strands.unwrap_or(5)),
                range: (2.0, 30.0),
                speed: 0.05,
                suffix: "",
                integer: true,
            },
            spacing("Spacing (open lines)", 0.5),
            pitch,
        ],
        LineType::Cutting => vec![Control::Toggle {
            label: "Leave uncut sections",
            param: "leaveUncut",
            value: l.leave_uncut,
        }],
        // OpenStitch's satin line (#524): width keys are edited with object.setWidthKey.
        LineType::Satin => [
            width("Width"),
            num(
                "Density",
                "line.density",
                "density",
                l.density,
                (0.1, 14.0),
                0.05,
                " /mm",
            ),
            Control::Toggle {
                label: "Under sewing",
                param: "underSewing",
                value: l.underlay,
            },
            num(
                "Taper start",
                "line.taperStart",
                "taperStart",
                l.taper_start.unwrap_or(0.0),
                (0.0, 50.0),
                0.1,
                " mm",
            ),
            num(
                "Taper end",
                "line.taperEnd",
                "taperEnd",
                l.taper_end.unwrap_or(0.0),
                (0.0, 50.0),
                0.1,
                " mm",
            ),
            num(
                "Slant",
                "line.slant",
                "slant",
                l.slant.unwrap_or(0.0),
                (-60.0, 60.0),
                1.0,
                "°",
            ),
        ]
        .into_iter()
        .chain(shaping_controls(&l.satin_shaping, 0.0, true))
        .chain(band_controls(l.motif_band.as_ref(), user, true))
        .collect(),
    }
}

/// Running lines: passes along a thickness profile (#542, beyond PE-DESIGN).
fn profile_controls(l: &LineSewing) -> Vec<Control> {
    let mut v = vec![Control::Choice {
        label: "Thickness profile",
        param: "profile",
        value: l
            .profile
            .as_ref()
            .map_or("none", |p| p.profile.preset.id())
            .into(),
        options: [
            ("none", "None (even)"),
            ("arc", "Arc"),
            ("sine", "Sine"),
            ("linear", "Thin to thick"),
            ("smoothstep", "Smooth thin to thick"),
            ("stairs", "Stairs"),
            ("constant", "Constant (max passes)"),
        ]
        .iter()
        .map(|(a, b)| ((*a).into(), (*b).into()))
        .collect(),
    }];
    if let Some(p) = &l.profile {
        v.extend([
            times(
                "Fewest passes (odd)",
                "line.minPasses",
                "minPasses",
                p.min_passes,
                9,
            ),
            times(
                "Most passes (odd)",
                "line.maxPasses",
                "maxPasses",
                p.max_passes,
                9,
            ),
            num(
                "Pass spacing",
                "line.passSpacing",
                "passSpacing",
                p.pass_spacing,
                (0.1, 1.0),
                0.01,
                " mm",
            ),
        ]);
    }
    v
}

/// Running and triple lines: the hand-stitch look (#540, beyond PE-DESIGN).
fn hand_controls(l: &LineSewing, passes: bool) -> Vec<Control> {
    let mut v = vec![Control::Toggle {
        label: "Hand-stitch look",
        param: "hand",
        value: l.hand.is_some(),
    }];
    if let Some(h) = &l.hand {
        v.extend([
            num(
                "Length variation",
                "line.handLength",
                "handLength",
                h.length_jitter,
                (0.0, 50.0),
                0.5,
                " %",
            ),
            num(
                "Tilt",
                "line.handAngle",
                "handAngle",
                h.angle_jitter,
                (0.0, 20.0),
                0.2,
                "°",
            ),
            num(
                "Extra stitches",
                "line.handExtra",
                "handExtra",
                h.extra_passes,
                (0.0, 50.0),
                0.5,
                " %",
            ),
        ]);
        if passes {
            v.push(times(
                "Hand passes",
                "line.handPasses",
                "handPasses",
                h.passes,
                3,
            ));
        }
        v.push(seed_control("line.seed", h.seed));
    }
    v
}

/// The variant (seed): changing it is the editor's Shuffle; the same number always sews the same
/// stitches.
fn seed_control(key: &'static str, seed: u32) -> Control {
    Control::Number {
        label: "Variant",
        key,
        param: "seed",
        value: f64::from(seed),
        range: (0.0, 1_000_000.0),
        speed: 1.0,
        suffix: "",
        integer: true,
    }
}

/// Satin regions: the hand-stitch look on one side (#540, beyond PE-DESIGN).
pub fn satin_hand_controls(r: &RegionSewing) -> Vec<Control> {
    let side = r.hand.as_ref().map(|h| h.side);
    let mut v = vec![Control::Choice {
        label: "Hand-stitch side",
        param: "handSide",
        value: match side {
            None => "none",
            Some(openstitch_design::line_looks::SatinSide::A) => "a",
            Some(openstitch_design::line_looks::SatinSide::B) => "b",
            Some(openstitch_design::line_looks::SatinSide::Both) => "both",
        }
        .into(),
        options: [
            ("none", "Off (crisp)"),
            ("a", "First side"),
            ("b", "Second side"),
            ("both", "Both sides"),
        ]
        .iter()
        .map(|(a, b)| ((*a).into(), (*b).into()))
        .collect(),
    }];
    if let Some(h) = &r.hand {
        v.push(num(
            "Hand-stitch amount",
            "region.handAmount",
            "handAmount",
            h.amount,
            (0.0, 50.0),
            0.5,
            " %",
        ));
        v.push(seed_control("region.seed", h.seed));
    }
    v
}

/// What the inspector shows for the selection.
#[derive(Clone, Debug, PartialEq)]
pub struct InspectorModel {
    /// Selected shape objects (attributes apply to these).
    pub shapes: BTreeSet<u64>,
    /// Selected imported stitch patterns (transforms only).
    pub stitch_patterns: usize,
    /// Every selected shape has a closed outline, so region sewing is possible.
    pub closed: bool,
    pub region_kind: Common<Option<RegionType>>,
    /// Attribute values of the first selected region (shown when values differ).
    pub region: Option<RegionSewing>,
    pub line_kind: Common<Option<LineType>>,
    pub line: Option<LineSewing>,
    pub hole_sewing: Common<bool>,
}

impl InspectorModel {
    pub fn new(design: &Design, selection: &BTreeSet<u64>) -> Self {
        let selected: Vec<_> = design
            .objects
            .iter()
            .filter(|o| selection.contains(&o.id))
            .collect();
        let shapes: Vec<_> = selected.iter().filter(|o| o.is_outline()).collect();
        InspectorModel {
            shapes: shapes.iter().map(|o| o.id).collect(),
            stitch_patterns: selected.len() - shapes.len(),
            closed: !shapes.is_empty() && shapes.iter().all(|o| o.has_closed_contour()),
            region_kind: Common::of(shapes.iter().map(|o| o.region.as_ref().map(|r| r.kind))),
            region: shapes.iter().find_map(|o| o.region.clone()),
            line_kind: Common::of(shapes.iter().map(|o| o.line.as_ref().map(|l| l.kind))),
            line: shapes.iter().find_map(|o| o.line.clone()),
            hole_sewing: Common::of(
                shapes
                    .iter()
                    .filter(|o| o.region.is_some())
                    .map(|o| o.hole_sewing),
            ),
        }
    }

    fn with_ids(&self, mut params: Value) -> Value {
        params["ids"] = CommandRequest::ids(&self.shapes);
        params
    }

    /// `object.setRegion` with `params` (e.g. `{"angle": 45}`) for every selected shape.
    pub fn set_region(&self, params: Value) -> CommandRequest {
        CommandRequest::new("object.setRegion", self.with_ids(params))
    }

    pub fn set_region_type(&self, k: Option<RegionType>) -> CommandRequest {
        self.set_region(json!({ "type": region_name(k) }))
    }

    pub fn set_line(&self, params: Value) -> CommandRequest {
        CommandRequest::new("object.setLine", self.with_ids(params))
    }

    pub fn set_line_type(&self, k: Option<LineType>) -> CommandRequest {
        self.set_line(json!({ "type": line_name(k) }))
    }

    /// Hole sewing applies only to selected shapes that have region sewing.
    pub fn set_hole_sewing(&self, design: &Design, on: bool) -> Option<CommandRequest> {
        let ids: BTreeSet<u64> = design
            .objects
            .iter()
            .filter(|o| self.shapes.contains(&o.id) && o.region.is_some())
            .map(|o| o.id)
            .collect();
        (!ids.is_empty()).then(|| {
            CommandRequest::new(
                "object.setHoleSewing",
                json!({ "ids": CommandRequest::ids(&ids), "enabled": on }),
            )
        })
    }
}

/// Brother PEC chart entries 1–64 for the colour picker.
pub fn pec_palette() -> Vec<(usize, &'static Thread)> {
    PEC_CHART
        .iter()
        .enumerate()
        .filter_map(|(i, t)| t.as_ref().map(|t| (i, t)))
        .collect()
}

/// The PEC chart number closest to `t`.
pub fn pec_index(t: &Thread) -> Option<usize> {
    nearest_index(t, pec_basic_chart())
}

/// The property search (#550): whether a row whose texts are `texts` (label, parameter name)
/// is shown for `filter`. Every word of the filter must be in one of the texts, any case; an
/// empty filter shows everything.
pub fn matches_filter(filter: &str, texts: &[&str]) -> bool {
    let texts: Vec<String> = texts.iter().map(|t| t.to_lowercase()).collect();
    filter
        .split_whitespace()
        .all(|w| texts.iter().any(|t| t.contains(&w.to_lowercase())))
}

#[cfg(test)]
mod search_tests {
    use super::*;

    #[test]
    fn pull_leaves_only_pull_compensation_rows() {
        let t = Thread::rgb(0);
        for kind in RegionType::ALL {
            let r = RegionSewing::new(kind, t.clone());
            for c in region_controls_with(&r, &[]) {
                let pull = c.label().to_lowercase().contains("pull")
                    || c.param().to_lowercase().contains("pull");
                assert_eq!(c.shown("pull"), pull, "{kind:?} {}", c.label());
                assert!(c.shown(""));
            }
        }
        let shown: Vec<_> = region_controls_with(&RegionSewing::new(RegionType::Fill, t), &[])
            .into_iter()
            .filter(|c| c.shown("PULL"))
            .map(|c| c.label())
            .collect();
        assert!(
            !shown.is_empty() && shown.iter().all(|l| l.contains("Pull")),
            "{shown:?}"
        );
        assert!(matches_filter(
            "under dens",
            &["Underlay density", "underlayDensity"]
        ));
        assert!(!matches_filter("under pull", &["Underlay density", "x"]));
    }

    #[test]
    fn basic_keeps_the_common_rows_and_advanced_all() {
        // #553: Basic shows at most these rows per type; Advanced shows every one.
        let t = Thread::rgb(0);
        for kind in RegionType::ALL {
            let all = region_controls_with(&RegionSewing::new(kind, t.clone()), &[]);
            let basic: Vec<_> = all.iter().filter(|c| c.basic()).collect();
            assert!(basic.len() <= BASIC_PARAMS.len(), "{kind:?}");
            assert!(basic.iter().all(|c| BASIC_PARAMS.contains(&c.param())));
        }
        let fill = region_controls_with(&RegionSewing::new(RegionType::Fill, t.clone()), &[]);
        let basic: Vec<_> = fill
            .iter()
            .filter(|c| c.basic())
            .map(|c| c.param())
            .collect();
        for p in ["angle", "density", "pullCompensation", "underlay"] {
            assert!(basic.contains(&p), "{p}: {basic:?}");
        }
        assert!(
            fill.iter().any(|c| !c.basic()),
            "Fill has advanced rows too"
        );
        let line = line_controls_with(&LineSewing::new(LineType::Running, t), &[]);
        assert!(line.iter().any(|c| c.basic() && c.param() == "pitch"));
    }
}
