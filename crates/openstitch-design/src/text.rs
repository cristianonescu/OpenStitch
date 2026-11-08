//! Lettering: bundled open-licence fonts and the layout of a [`TextSpec`] into glyph contours.
//!
//! Glyph outlines come from the font's `glyf`/`CFF` data through `ttf-parser`; TrueType
//! quadratic segments are converted exactly to cubic Béziers, so text contours use the same
//! [`Contour`] model as drawn shapes. Inner contours (the counter of an "o") are holes under the
//! even-odd rule the region generators use. Each glyph is filled on its own, so glyphs that touch
//! or overlap (script joins) never cut holes into each other.

use crate::fmath::Det;
use crate::fonts::{
    DesignFont, EmbroideryFont, FontData, MAX_GLYPH_SEGMENTS, MAX_LAYOUT_SEGMENTS, parse_face,
    shaping_note,
};
use crate::geometry::{Bezier, Contour, Rect, Transform, Vec2, v2};
use crate::model::StitchBlock;
use crate::shaping;
use openstitch_core::{Command, Stitch};
use serde::{Deserialize, Serialize};
use ttf_parser::{Face, GlyphId, OutlineBuilder};

/// What a bundled font is made of (#527).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontKind {
    /// A TrueType outline font, sewn as regions (satin or fill) letter by letter.
    TrueType,
    /// A single-line font ([`crate::stroke_fonts::line_font`]): open strokes along each
    /// letter's centre, sewn by the text's line sewing (running, triple or a narrow satin).
    SingleLine,
    /// A pixel font ([`crate::stroke_fonts::pixel_font`]): each pixel sewn as a short run, a
    /// cross or a small block ([`PixelStyle`]).
    Pixel,
}

impl FontKind {
    pub fn as_str(self) -> &'static str {
        match self {
            FontKind::TrueType => "trueType",
            FontKind::SingleLine => "singleLine",
            FontKind::Pixel => "pixel",
        }
    }
}

/// A font bundled with OpenStitch: SIL OFL TrueType fonts, unmodified (see
/// THIRD_PARTY_NOTICES.md and `crates/openstitch-design/fonts/`), and OpenStitch's own
/// single-line and pixel fonts (`stroke_fonts.rs`, MIT OR Apache-2.0 like the code).
#[derive(Debug)]
pub struct FontInfo {
    /// Stable identifier stored in `.osd` files.
    pub id: &'static str,
    pub family: &'static str,
    pub style: &'static str,
    pub license: &'static str,
    /// The TrueType file; empty for the other kinds.
    pub data: &'static [u8],
    pub kind: FontKind,
    /// Text sizes (mm, PE-DESIGN's size) the font sews well at with its first stitch type,
    /// measured by `stroke_fonts::tests::size_ranges_hold` against `tools/sew_check.py`'s
    /// limits: below it a TrueType font's strokes turn into hairline satins, above it satin
    /// stitches pass 12 mm or needle drops pile up. OpenStitch Line's range is its design
    /// (3-6 mm labels, legible up to 15 mm).
    pub size_range: (f64, f64),
    /// Stitch types the font is made for, in `object.setRegion` / `object.setLine` names.
    pub stitch_types: &'static [&'static str],
    /// PE-DESIGN's Font List group: "text" or "smallText".
    pub group: &'static str,
}

pub static FONTS: [FontInfo; 4] = [
    FontInfo {
        id: "lato-bold",
        family: "Lato",
        style: "Bold",
        license: "OFL-1.1",
        data: include_bytes!("../fonts/Lato-Bold.ttf"),
        kind: FontKind::TrueType,
        size_range: (6.0, 20.0),
        stitch_types: &["satin", "fill"],
        group: "text",
    },
    FontInfo {
        id: "pacifico",
        family: "Pacifico",
        style: "Regular",
        license: "OFL-1.1",
        data: include_bytes!("../fonts/Pacifico-Regular.ttf"),
        kind: FontKind::TrueType,
        size_range: (7.0, 10.0),
        stitch_types: &["satin", "fill"],
        group: "text",
    },
    FontInfo {
        id: "openstitch-line",
        family: "OpenStitch Line",
        style: "Single line",
        license: "MIT OR Apache-2.0",
        data: &[],
        kind: FontKind::SingleLine,
        size_range: (3.0, 15.0),
        stitch_types: &["running", "triple", "zigzag"],
        group: "smallText",
    },
    FontInfo {
        id: "openstitch-pixel",
        family: "OpenStitch Pixel",
        style: "5x7",
        license: "MIT OR Apache-2.0",
        data: &[],
        kind: FontKind::Pixel,
        size_range: (5.0, 40.0),
        stitch_types: &["pixelRun", "pixelCross", "pixelBlock"],
        group: "text",
    },
];

/// How each pixel of a pixel-font text is sewn (#536).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum PixelStyle {
    /// A short run across the pixel's middle.
    #[default]
    Run,
    /// A cross stitch corner to corner.
    Cross,
    /// Rows filling the pixel.
    Block,
}

impl PixelStyle {
    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "run" => PixelStyle::Run,
            "cross" => PixelStyle::Cross,
            "block" => PixelStyle::Block,
            _ => return None,
        })
    }

    pub fn as_str(self) -> &'static str {
        match self {
            PixelStyle::Run => "run",
            PixelStyle::Cross => "cross",
            PixelStyle::Block => "block",
        }
    }

    fn is_run(&self) -> bool {
        *self == PixelStyle::Run
    }
}

/// The kind of the bundled font `name` refers to; `None` for design fonts.
pub fn bundled_kind(name: &str) -> Option<FontKind> {
    font(name).map(|f| f.kind)
}

/// The sizes (mm) font `name` sews well at, and its family: a bundled font's measured range,
/// or the range an embroidery font's maker states (#529).
pub fn size_range(name: &str, fonts: &[DesignFont]) -> Option<(String, (f64, f64))> {
    match resolve(name, fonts)? {
        FontRef::Bundled(f) => Some((f.family.to_string(), f.size_range)),
        FontRef::Design(f) => match &f.data {
            FontData::Embroidery(e) => e.size_range.map(|r| (f.family.clone(), r)),
            FontData::TrueType { .. } => None,
        },
    }
}

/// A warning when `size` (mm) lies outside the sizes bundled font `name` sews well at (#527);
/// never a refusal.
pub fn size_warning(name: &str, size: f64) -> Option<String> {
    size_warning_in(name, &[], size)
}

/// [`size_warning`] for a bundled font or one of `fonts`.
pub fn size_warning_in(name: &str, fonts: &[DesignFont], size: f64) -> Option<String> {
    let (family, (lo, hi)) = size_range(name, fonts)?;
    (size < lo - 1e-9 || size > hi + 1e-9).then(|| {
        format!(
            "{family} sews best at {lo}-{hi} mm; {size} mm is {} its range",
            if size < lo { "below" } else { "above" }
        )
    })
}

pub const DEFAULT_FONT: &str = "lato-bold";

/// The font with this id, or whose "family" / "family style" matches, ignoring case.
pub fn font(name: &str) -> Option<&'static FontInfo> {
    let name = name.trim();
    FONTS.iter().find(|f| {
        f.id.eq_ignore_ascii_case(name)
            || f.family.eq_ignore_ascii_case(name)
            || format!("{} {}", f.family, f.style).eq_ignore_ascii_case(name)
    })
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextAlign {
    #[default]
    Left,
    Center,
    Right,
}

/// What a text object says and how it is set. Its contours (on the owning
/// [`DesignObject`](crate::DesignObject)) are derived from this by [`layout`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextSpec {
    pub text: String,
    /// [`FontInfo::id`].
    pub font: String,
    /// Text size, mm. For TrueType/OpenType fonts it is PE-DESIGN's size: 0.7 of the em, so
    /// Latin capitals come out about this tall (Lato Bold's 'H' is 1.03 × size). For embroidery
    /// fonts, and texts with [`TextMetrics::CapHeight`], it is the capital height.
    pub size: f64,
    /// What `size` and the line spacing % measure. Texts from files before format version 3
    /// keep [`TextMetrics::CapHeight`] so they lay out as they did.
    #[serde(default, skip_serializing_if = "TextMetrics::is_pe")]
    pub metrics: TextMetrics,
    /// Extra space between letters, mm (may be negative).
    #[serde(default)]
    pub letter_spacing: f64,
    /// Line spacing, % of the character height (PE-DESIGN's "% of char height"): at 100 the
    /// lines are the font's Windows ascent apart (its line height for
    /// [`TextMetrics::CapHeight`] texts and embroidery fonts).
    #[serde(default = "hundred", skip_serializing_if = "is_hundred")]
    pub line_spacing_percent: f64,
    /// Extra space between lines, mm (may be negative), added to the spacing in %. Files
    /// written before line spacing was in % use it alone.
    #[serde(default)]
    pub line_spacing: f64,
    #[serde(default)]
    pub align: TextAlign,
    /// Shape object whose first contour the text follows (text on a path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<u64>,
    /// How text on a path sits on it (Fit Text to Outline settings).
    #[serde(default, skip_serializing_if = "PathFit::is_default")]
    pub fit: PathFit,
    /// Placement after layout. Straight text is laid out with the first baseline on y = 0
    /// and the alignment point at x = 0; text on a path is laid out in page coordinates.
    #[serde(default = "identity")]
    pub transform: Transform,
    /// Number of contours of each outlined glyph, in order: compiling fills glyph by glyph.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub glyphs: Vec<u32>,
    /// Name Drop attribute: the text is fitted into a fixed box, so a replacement keeps the
    /// layout (Name Drop lists, template slots).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name_drop: Option<NameDrop>,
    /// Apply the font's kerning (TrueType/OpenType fonts; embroidery fonts keep their own
    /// spacing). Off in files written before it existed.
    #[serde(default, skip_serializing_if = "is_false")]
    pub kerning: bool,
    /// Horizontal lines, or vertical columns (top to bottom, columns right to left).
    #[serde(default, skip_serializing_if = "is_horizontal")]
    pub direction: TextDirection,
    /// Per-character edits, by character index, sorted and unique.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub characters: Vec<CharEdit>,
    /// Text Transform: one of PE-DESIGN's 26 envelope presets (1..=26, its palette in reading
    /// order), applied to straight horizontal text. See [`ENVELOPE_PRESETS`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope: Option<u8>,
    /// Text Transform strength, as PE-DESIGN 11.50 stores it (its canvas handle; there is no
    /// numeric field): percent of the font size for the straight-edged presets, a percentage
    /// for the bows and waves, degrees for the slants and fans, the arch's half-sweep in
    /// degrees for 1 and 2. `None` is the preset's default ([`envelope_default_level`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub envelope_level: Option<f64>,
    /// How a pixel font's pixels are sewn (#536); ignored by other fonts.
    #[serde(default, skip_serializing_if = "PixelStyle::is_run")]
    pub pixel_style: PixelStyle,
    /// A free envelope (#535), edited with handles; it replaces a Text Transform preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub custom_envelope: Option<CustomEnvelope>,
    /// Arched text (#456, beyond PE-DESIGN): the first line's baseline radius, mm. Positive
    /// arches up (the text over the top of a circle), negative arches down (under the bottom,
    /// read upright). Every line is a ring around the same centre, each further line one line
    /// spacing inward (arch up) or outward (arch down). Horizontal text without a path or a
    /// Text Transform only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arc: Option<f64>,
}

/// The smallest and largest arc radius, mm ([`TextSpec::arc`]).
pub const MIN_ARC: f64 = 5.0;
pub const MAX_ARC: f64 = 1000.0;

/// The most knots one edge of a free envelope may have.
pub const MAX_ENVELOPE_KNOTS: usize = 129;

/// One of a free envelope's edges.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeEdge {
    Top,
    Bottom,
}

/// One of an envelope edge's three handles: its ends and its middle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EnvelopeHandle {
    Left,
    Middle,
    Right,
}

impl EnvelopeHandle {
    /// How much of a handle's move a knot at `u` (0..=1 along the edge) takes: all of it at the
    /// handle, fading smoothly to none at the far end (left `(1-u)²`, right `u²`, middle
    /// `4u(1-u)`), so dragging bends the edge instead of kinking it.
    fn weight(self, u: f64) -> f64 {
        match self {
            EnvelopeHandle::Left => (1.0 - u) * (1.0 - u),
            EnvelopeHandle::Middle => 4.0 * u * (1.0 - u),
            EnvelopeHandle::Right => u * u,
        }
    }

    fn mirrored(self) -> Self {
        match self {
            EnvelopeHandle::Left => EnvelopeHandle::Right,
            EnvelopeHandle::Middle => EnvelopeHandle::Middle,
            EnvelopeHandle::Right => EnvelopeHandle::Left,
        }
    }
}

/// A free text envelope (#535): the top and bottom edge as knots evenly spaced along the text
/// (knot `k` of `n` at `u = k / (n - 1)`), each a point `(x, y)` in fractions of the straight
/// text's capital-height box (x from its left, y from its top, y down), with `scale` how far
/// they bow from the straight box, as a Text Transform level scales its preset. Laid out by the
/// same warp as the presets, so a preset turned into a free envelope sews the same stitches
/// until a handle moves.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEnvelope {
    pub top: Vec<(f64, f64)>,
    pub bottom: Vec<(f64, f64)>,
    #[serde(default = "one")]
    pub scale: f64,
}

fn one() -> f64 {
    1.0
}

impl CustomEnvelope {
    /// Whether the envelope can be laid out: 2-129 finite knots per edge, values within -10..10.
    pub fn is_valid(&self) -> bool {
        let edge = |e: &[(f64, f64)]| {
            (2..=MAX_ENVELOPE_KNOTS).contains(&e.len())
                && e.iter().all(|&(x, y)| {
                    x.is_finite() && y.is_finite() && x.abs() <= 10.0 && y.abs() <= 10.0
                })
        };
        edge(&self.top) && edge(&self.bottom) && self.scale.is_finite() && self.scale.abs() <= 10.0
    }

    /// Text Transform preset `n` at `level` (`None`: its default) as a free envelope, its knots
    /// and scale exactly the preset's. `None` for the arches (1, 2: text set on a circle, not
    /// warped into an envelope) and presets OpenStitch does not lay out.
    pub fn from_preset(n: u8, level: Option<f64>) -> Option<Self> {
        let (_, top, bottom) = crate::envelope_presets::ENVELOPES
            .iter()
            .find(|(k, _, _)| *k == n)?;
        Some(CustomEnvelope {
            top: top.to_vec(),
            bottom: bottom.to_vec(),
            scale: level.map_or(1.0, |l| envelope_scale(n, l)),
        })
    }

    /// A starting shape (#535), `amount` (0-0.45) of the box: `bridge` (the top bows down in
    /// the middle, the baseline stays straight), `pennant` (tapering from the left to the
    /// right), `perspective` (the top edge shorter than the baseline) and `diamond` (tall in
    /// the middle, short at both ends).
    pub fn shape(name: &str, amount: f64) -> Option<Self> {
        let a = amount;
        let u = |k: usize| k as f64 / 32.0;
        let edge = |f: &dyn Fn(f64) -> (f64, f64)| (0..=32).map(|k| f(u(k))).collect::<Vec<_>>();
        let (top, bottom) = match name {
            "bridge" => (edge(&|u| (u, a * 4.0 * u * (1.0 - u))), edge(&|u| (u, 1.0))),
            "pennant" => (edge(&|u| (u, a * u)), edge(&|u| (u, 1.0 - a * u))),
            "perspective" => (
                edge(&|u| (a / 2.0 + u * (1.0 - a), 0.0)),
                edge(&|u| (u, 1.0)),
            ),
            "diamond" => (
                edge(&|u| (u, a * (2.0 * u - 1.0).abs())),
                edge(&|u| (u, 1.0 - a * (2.0 * u - 1.0).abs())),
            ),
            _ => return None,
        };
        Some(CustomEnvelope {
            top,
            bottom,
            scale: 1.0,
        })
    }

    /// The point of `edge` at `u` as laid out (the scale applied), in box fractions.
    pub fn point(&self, edge: EnvelopeEdge, u: f64) -> Vec2 {
        let k = self.scale;
        match edge {
            EnvelopeEdge::Top => {
                let t = edge_at(&self.top, u);
                v2(u + k * (t.x - u), k * t.y)
            }
            EnvelopeEdge::Bottom => {
                let b = edge_at(&self.bottom, u);
                v2(u + k * (b.x - u), 1.0 + k * (b.y - 1.0))
            }
        }
    }

    /// Moves `handle` of `edge` by `(dx, dy)` (box fractions, as laid out); with `symmetric`
    /// the opposite handle mirrors the move about the text's centre line (the middle handle
    /// then moves only up or down).
    pub fn bend(
        &mut self,
        edge: EnvelopeEdge,
        handle: EnvelopeHandle,
        (dx, dy): (f64, f64),
        symmetric: bool,
    ) {
        if self.scale.abs() < 1e-6 {
            // A flattened envelope: start again from its straight box.
            let straight = |y: f64, e: &[(f64, f64)]| {
                (0..e.len())
                    .map(|k| (k as f64 / (e.len() - 1) as f64, y))
                    .collect::<Vec<_>>()
            };
            self.top = straight(0.0, &self.top);
            self.bottom = straight(1.0, &self.bottom);
            self.scale = 1.0;
        }
        let dx = if symmetric && handle == EnvelopeHandle::Middle {
            0.0
        } else {
            dx
        };
        let mut moves = vec![(handle, dx)];
        if symmetric && handle != EnvelopeHandle::Middle {
            moves.push((handle.mirrored(), -dx));
        }
        let k = self.scale;
        let knots = match edge {
            EnvelopeEdge::Top => &mut self.top,
            EnvelopeEdge::Bottom => &mut self.bottom,
        };
        let n = knots.len();
        for (h, mx) in moves {
            for (i, knot) in knots.iter_mut().enumerate() {
                let w = h.weight(i as f64 / (n - 1) as f64);
                knot.0 += w * mx / k;
                knot.1 += w * dy / k;
            }
        }
    }
}

/// Text Transform presets OpenStitch lays out: all 26, 1 and 2 (arches over and under) by
/// construction, the rest (3–26, the fans 13 and 14 included) as the envelopes in
/// `envelope_presets.rs`.
pub const ENVELOPE_PRESETS: [u8; 26] = [
    1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26,
];

/// PE-DESIGN 11.50's level of Text Transform preset `n` (1–26) when the preset is clicked
/// (round-4 captures, read from the owner's files): 30 (degrees) for the arches 1 and 2 and
/// the fans 13 and 14, 20 for the bows and waves 5, 6, 15, 16, 19, 20, 23 and 24, 10 (degrees)
/// for the slants 9 and 10, 50 for the straight-edged presets.
pub fn envelope_default_level(n: u8) -> f64 {
    match n {
        1 | 2 | 13 | 14 => 30.0,
        5 | 6 | 15 | 16 | 19 | 20 | 23 | 24 => 20.0,
        9 | 10 => 10.0,
        _ => 50.0,
    }
}

/// The levels accepted for preset `n`: PE-DESIGN's handle gives -100…100 for the
/// straight-edged presets (100 is its stop), -100…100 for the bows, ±80 degrees for the slants
/// and fans, and a half-sweep of 1…90 degrees for the arches (1.15 degrees is a 1000 mm
/// radius, PE-DESIGN's stop).
pub fn envelope_level_range(n: u8) -> (f64, f64) {
    match n {
        1 | 2 => (1.0, 90.0),
        9 | 10 | 13 | 14 => (-80.0, 80.0),
        _ => (-100.0, 100.0),
    }
}

/// How far preset `n` at `level` bows from straight text, as a multiple of its default
/// envelope: the level over the default (straight-edged, bows, fans), the shear's tangent over
/// the default's (slants).
fn envelope_scale(n: u8, level: f64) -> f64 {
    let default = envelope_default_level(n);
    match n {
        9 | 10 => level.to_radians().dtan() / default.to_radians().dtan(),
        _ => level / default,
    }
}

/// The arches' angle: the straight text's advance width (its letters' advances, from the first
/// pen position to the last, as PE-DESIGN measures its text box) over the radius of its inner
/// line. PE-DESIGN 11.50 stores the circle: HHHHHHH, OpenStitch and Agjy 123 at size 10 give
/// 770 / 408.0, 724 / 383.7 and 570 / 301.9 (0.1 mm), all 1.887 (compat text-transform-1 and the
/// round-4 Text Transform captures).
const ARCH_ANGLE: f64 = 1.887;

/// PE-DESIGN sets arched text a little left of the straight text's centre, by this share of
/// its width (0.3-0.45 mm of 74.6 mm in both arches). -0.004 with [`ARCH_LETTER_SPACING`]: the
/// sweep of shifts -0.0085..0 and spacings 0..-0.1 mm over every arch spec, with Fill text by
/// stroke (#428, 2026-11-08), passes them all from -0.007 to 0 at -0.05 mm.
const ARCH_SHIFT: f64 = -0.004;
/// Letters along an arch sit this much closer (mm) than straight text: PE-DESIGN's arches 1 and
/// 2 place the left letters up to 0.6 mm less far along the arc than the plain spacing would
/// (#428).
const ARCH_LETTER_SPACING: f64 = -0.05;

/// What a text's size and line spacing measure.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextMetrics {
    /// PE-DESIGN's: size is 0.7 em, 100 % line spacing is the font's Windows ascent
    /// (measured from PE-DESIGN 11.50's Lato Bold text, compat/reference/text).
    #[default]
    Pe,
    /// OpenStitch's before format version 3: size is the capital height, 100 % line spacing
    /// the font's line height (ascender − descender + line gap).
    CapHeight,
}

impl TextMetrics {
    pub fn is_pe(&self) -> bool {
        *self == TextMetrics::Pe
    }
}

/// Which way text runs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TextDirection {
    #[default]
    Horizontal,
    /// CJK vertical writing: characters top to bottom, lines as columns from right to left.
    /// Text on a path is always laid out along the path.
    Vertical,
}

fn hundred() -> f64 {
    100.0
}

fn is_hundred(v: &f64) -> bool {
    *v == 100.0
}

fn is_false(b: &bool) -> bool {
    !*b
}

fn is_horizontal(d: &TextDirection) -> bool {
    *d == TextDirection::Horizontal
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

/// An edit of one character of a text: resized about the middle of its advance on the baseline
/// and turned about its middle (half the capital height up; the middle of its cell in vertical
/// text), then moved. Glyphs that stand for
/// several characters (a cluster) follow the edit of their first character.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CharEdit {
    /// Character index in [`TextSpec::text`] (Unicode scalar values, line breaks included).
    pub index: usize,
    /// Offset, mm (+y down), in the text's own frame (along the path for text on a path).
    #[serde(default, skip_serializing_if = "is_zero")]
    pub x: f64,
    #[serde(default, skip_serializing_if = "is_zero")]
    pub y: f64,
    /// Rotation, degrees clockwise.
    #[serde(default, skip_serializing_if = "is_zero")]
    pub rotation: f64,
    /// Capital height of this character, mm, instead of the text's size; its advance scales
    /// with it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<f64>,
}

impl CharEdit {
    /// True when the edit changes nothing.
    pub fn is_noop(&self) -> bool {
        self.x == 0.0 && self.y == 0.0 && self.rotation == 0.0 && self.size.is_none()
    }

    /// The scale factor against a text of capital height `size`.
    fn factor(&self, size: f64) -> f64 {
        match self.size {
            Some(s) if s.is_finite() && s > 0.0 && size > 0.0 => (s / size).clamp(0.01, 100.0),
            _ => 1.0,
        }
    }
}

/// Fit Text to Outline settings: where text on a path sits across it, which way its letters
/// stand, and which side of the path it runs on. Along the path it is placed by
/// [`TextSpec::align`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PathFit {
    /// Across the path: the baseline on it (`Bottom`), the capitals centred on it (`Middle`),
    /// or their tops on it (`Top`).
    #[serde(default = "bottom")]
    pub valign: VAlign,
    /// Letters stay upright instead of turning with the path.
    #[serde(default, skip_serializing_if = "is_false")]
    pub upright: bool,
    /// Spread along the whole path instead of being placed by [`TextSpec::align`].
    #[serde(default, skip_serializing_if = "Spread::is_none")]
    pub spread: Spread,
    /// The path is followed the other way, so the text runs on its other side.
    #[serde(default, skip_serializing_if = "is_false")]
    pub other_side: bool,
}

impl Default for PathFit {
    fn default() -> Self {
        PathFit {
            valign: VAlign::Bottom,
            upright: false,
            spread: Spread::None,
            other_side: false,
        }
    }
}

/// Fit Text to Outline's last two horizontal alignments: the text spread over the whole path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Spread {
    /// Placed by the text's alignment (left, center, right).
    #[default]
    None,
    /// Characters spaced evenly, each in the middle of an equal share of the path.
    Even,
    /// Letters scaled up (or down) until the text fills the path.
    Fill,
}

impl Spread {
    pub fn is_none(&self) -> bool {
        *self == Spread::None
    }
}

impl PathFit {
    pub fn is_default(&self) -> bool {
        *self == PathFit::default()
    }
}

fn bottom() -> VAlign {
    VAlign::Bottom
}

/// Vertical placement in a Name Drop box.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum VAlign {
    Top,
    #[default]
    Middle,
    Bottom,
}

/// PE-DESIGN's Name Drop attribute: whatever the text says, it is fitted into `box` (laid-out
/// coordinates, before [`TextSpec::transform`], 0.1 mm), uniformly when `maintain_aspect`
/// (then placed by `align` and `valign`), else stretched to fill it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct NameDrop {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    #[serde(default = "yes")]
    pub maintain_aspect: bool,
    #[serde(default = "center")]
    pub align: TextAlign,
    #[serde(default)]
    pub valign: VAlign,
    /// Optional label (template slot name).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub label: Option<String>,
}

fn yes() -> bool {
    true
}

fn center() -> TextAlign {
    TextAlign::Center
}

impl NameDrop {
    /// The transform fitting laid-out bounds `b` into the box.
    pub fn fit(&self, b: &Rect) -> Transform {
        let (w, h) = (b.width().max(1e-9), b.height().max(1e-9));
        let (sx, sy) = if self.maintain_aspect {
            let k = (self.width / w).min(self.height / h);
            (k, k)
        } else {
            (self.width / w, self.height / h)
        };
        let (fw, fh) = (w * sx, h * sy);
        let x = match self.align {
            TextAlign::Left => self.x,
            TextAlign::Center => self.x + (self.width - fw) / 2.0,
            TextAlign::Right => self.x + self.width - fw,
        };
        let y = match self.valign {
            VAlign::Top => self.y,
            VAlign::Middle => self.y + (self.height - fh) / 2.0,
            VAlign::Bottom => self.y + self.height - fh,
        };
        Transform::translation(-b.min_x, -b.min_y)
            .then(&Transform::scaling(sx, sy, v2(0.0, 0.0)))
            .then(&Transform::translation(x, y))
    }
}

fn identity() -> Transform {
    Transform::IDENTITY
}

impl TextSpec {
    pub fn new(text: impl Into<String>, font: &str, size: f64) -> Self {
        TextSpec {
            text: text.into(),
            font: font.into(),
            size,
            metrics: TextMetrics::Pe,
            letter_spacing: 0.0,
            line_spacing_percent: 100.0,
            line_spacing: 0.0,
            align: TextAlign::Left,
            path: None,
            fit: PathFit::default(),
            transform: Transform::IDENTITY,
            glyphs: Vec::new(),
            name_drop: None,
            kerning: false,
            direction: TextDirection::Horizontal,
            characters: Vec::new(),
            envelope: None,
            envelope_level: None,
            pixel_style: PixelStyle::Run,
            custom_envelope: None,
            arc: None,
        }
    }

    /// The edit of character `index`, if any.
    pub fn char_edit(&self, index: usize) -> Option<&CharEdit> {
        self.characters
            .binary_search_by_key(&index, |e| e.index)
            .ok()
            .map(|i| &self.characters[i])
    }

    /// Sets (or, when it changes nothing, removes) the edit of `edit.index`.
    pub fn set_char_edit(&mut self, edit: CharEdit) {
        match self
            .characters
            .binary_search_by_key(&edit.index, |e| e.index)
        {
            Ok(i) if edit.is_noop() => {
                self.characters.remove(i);
            }
            Ok(i) => self.characters[i] = edit,
            Err(_) if edit.is_noop() => {}
            Err(i) => self.characters.insert(i, edit),
        }
    }

    /// Drops edits of characters the text no longer has.
    pub fn trim_char_edits(&mut self) {
        let n = self.text.chars().count();
        self.characters.retain(|e| e.index < n);
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TextError {
    UnknownFont(String),
    BadFont(String),
    /// The laid-out outlines exceed [`MAX_LAYOUT_SEGMENTS`].
    TooComplex,
    /// An arched text's ring is too tight for its line: the first line's radius needs to be
    /// at least this, mm ([`TextSpec::arc`]).
    ArcTooTight(f64),
}

impl std::fmt::Display for TextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TextError::UnknownFont(n) => write!(f, "unknown font '{n}'"),
            TextError::BadFont(n) => write!(f, "font '{n}' cannot be read"),
            TextError::TooComplex => write!(
                f,
                "the text's outlines are too complex (more than {MAX_LAYOUT_SEGMENTS} segments)"
            ),
            TextError::ArcTooTight(r) => write!(
                f,
                "the arc is too tight for the text: its radius needs to be at least {r:.1} mm"
            ),
        }
    }
}

impl std::error::Error for TextError {}

/// One placed glyph of a layout: outlines, stitch data (user-mapped text), or both.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub ch: char,
    /// Index of `ch` in [`TextSpec::text`], in characters.
    pub index: usize,
    pub contours: Vec<Contour>,
    pub stitches: Option<StitchBlock>,
    /// Where the font's glyph was placed (font units to 0.1 mm on the page) when it was only
    /// moved, turned and scaled, `None` once a Text Transform envelope warped it: compiling
    /// cuts a turned letter into strokes upright, as PE-DESIGN 11.50 does (#444).
    pub placement: Option<Transform>,
}

impl Glyph {
    /// How far the letter is turned, degrees (counter-clockwise in its coordinates): the
    /// angle of its placed x axis. `None` for a warped letter.
    pub fn turn(&self) -> Option<f64> {
        self.placement.map(|t| t.b.datan2(t.a).to_degrees())
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    /// Glyphs with an outline or stitches (spaces and missing characters have none), placed in
    /// 0.1 mm.
    pub glyphs: Vec<Glyph>,
    /// Characters the font has no glyph for (each once, in order of appearance). Glyphs over
    /// [`MAX_GLYPH_SEGMENTS`] count as missing.
    pub missing: Vec<char>,
    /// Scripts in the text that need shaping this layout did not do ([`shaping_note`]): text
    /// in an embroidery font, which is set one character after another (bidi order only).
    pub shaping: Vec<&'static str>,
}

impl Layout {
    pub fn contours(&self) -> Vec<Contour> {
        self.glyphs
            .iter()
            .flat_map(|g| g.contours.iter().cloned())
            .collect()
    }

    /// Number of contours of each glyph that has an outline.
    pub fn glyph_counts(&self) -> Vec<u32> {
        self.outlined().map(|g| g.contours.len() as u32).collect()
    }

    /// Glyphs with an outline, in order.
    pub fn outlined(&self) -> impl Iterator<Item = &Glyph> {
        self.glyphs.iter().filter(|g| !g.contours.is_empty())
    }

    /// The stitch glyphs joined into one block, in text order: each glyph starts after a trim,
    /// and its thread list follows the previous glyph's.
    pub fn stitch_block(&self) -> Option<StitchBlock> {
        let mut out = StitchBlock {
            stitches: Vec::new(),
            threads: Vec::new(),
        };
        for b in self.glyphs.iter().filter_map(|g| g.stitches.as_ref()) {
            if let Some(last) = out.stitches.last().copied() {
                out.stitches
                    .push(Stitch::new(last.x, last.y, Command::Trim));
                out.stitches
                    .push(Stitch::new(last.x, last.y, Command::ColorChange));
            }
            out.stitches.extend(
                b.stitches
                    .iter()
                    .filter(|s| !matches!(s.command, Command::End | Command::Stop)),
            );
            out.threads.extend(b.threads.iter().cloned());
        }
        (!out.stitches.is_empty()).then_some(out)
    }
}

/// Collects a glyph outline as contours in font units (y up).
#[derive(Default)]
struct Builder {
    contours: Vec<Contour>,
    points: Vec<Vec2>,
    curves: Vec<Option<Bezier>>,
    /// Segments seen; past [`MAX_GLYPH_SEGMENTS`] the glyph is abandoned.
    segments: usize,
}

impl Builder {
    fn last(&self) -> Vec2 {
        self.points.last().copied().unwrap_or_default()
    }

    /// Counts one segment; false once the glyph is over the limit (the rest is ignored).
    fn take(&mut self) -> bool {
        self.segments += 1;
        self.segments <= MAX_GLYPH_SEGMENTS
    }

    fn too_complex(&self) -> bool {
        self.segments > MAX_GLYPH_SEGMENTS
    }

    fn finish(&mut self) {
        let mut points = std::mem::take(&mut self.points);
        let mut curves = std::mem::take(&mut self.curves);
        if points.len() > 1 && points.last() == points.first() {
            points.pop();
        } else if !points.is_empty() {
            curves.push(None);
        }
        let mut contour = Contour::new(points, true);
        if curves.iter().any(Option::is_some) {
            curves.truncate(contour.segment_count());
            contour.curves = curves;
        }
        let degenerate =
            contour.points.len() < 2 || (contour.points.len() < 3 && !contour.has_curves());
        if !degenerate {
            self.contours.push(contour);
        }
    }
}

impl OutlineBuilder for Builder {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.take() {
            return;
        }
        if !self.points.is_empty() {
            self.finish();
        }
        self.points.push(v2(x.into(), y.into()));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        if !self.take() {
            return;
        }
        self.curves.push(None);
        self.points.push(v2(x.into(), y.into()));
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        if !self.take() {
            return;
        }
        // Exact degree elevation: the cubic handles sit 2/3 of the way to the quadratic one.
        let (p0, q, p) = (
            self.last(),
            v2(x1.into(), y1.into()),
            v2(x.into(), y.into()),
        );
        self.curves.push(Some(Bezier {
            c1: p0.lerp(q, 2.0 / 3.0),
            c2: p.lerp(q, 2.0 / 3.0),
        }));
        self.points.push(p);
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        if !self.take() {
            return;
        }
        self.curves.push(Some(Bezier {
            c1: v2(x1.into(), y1.into()),
            c2: v2(x2.into(), y2.into()),
        }));
        self.points.push(v2(x.into(), y.into()));
    }

    fn close(&mut self) {
        if !self.too_complex() && !self.points.is_empty() {
            self.finish();
        }
    }
}

/// A bundled font or a font stored in the design.
#[derive(Clone, Copy, Debug)]
pub enum FontRef<'a> {
    Bundled(&'static FontInfo),
    Design(&'a DesignFont),
}

impl FontRef<'_> {
    pub fn id(&self) -> &str {
        match self {
            FontRef::Bundled(f) => f.id,
            FontRef::Design(f) => &f.id,
        }
    }
}

/// The font `name` refers to: a bundled font ([`font`]) first, then a design font by id or
/// family, ignoring case.
pub fn resolve<'a>(name: &str, fonts: &'a [DesignFont]) -> Option<FontRef<'a>> {
    if let Some(f) = font(name) {
        return Some(FontRef::Bundled(f));
    }
    let name = name.trim();
    fonts
        .iter()
        .find(|f| f.id.eq_ignore_ascii_case(name))
        .or_else(|| {
            fonts.iter().find(|f| {
                f.family.eq_ignore_ascii_case(name)
                    || format!("{} {}", f.family, f.style).eq_ignore_ascii_case(name)
            })
        })
        .map(FontRef::Design)
}

/// Where a font's glyphs come from.
enum Source<'a> {
    Face(Box<Face<'a>>),
    Embroidery(&'a EmbroideryFont),
}

/// A parsed font with the metrics layout needs.
pub struct LoadedFont<'a> {
    source: Source<'a>,
    /// 0.1 mm per font unit for a given size, per mm of size, with size the capital height.
    unit_per_mm: f64,
    /// The same with PE-DESIGN's size (0.7 em); equal to `unit_per_mm` for embroidery fonts.
    pe_unit_per_mm: f64,
}

/// A glyph's outline (font units) and stitches, or `None` when it has neither.
struct Outline {
    contours: Vec<Contour>,
    stitches: Option<StitchBlock>,
    segments: usize,
}

impl LoadedFont<'static> {
    /// A bundled font.
    pub fn load(name: &str) -> Result<Self, TextError> {
        LoadedFont::load_in(name, &[])
    }
}

impl<'a> LoadedFont<'a> {
    /// A bundled font or one of `fonts`.
    pub fn load_in(name: &str, fonts: &'a [DesignFont]) -> Result<Self, TextError> {
        let found = resolve(name, fonts).ok_or_else(|| TextError::UnknownFont(name.into()))?;
        let bad = || TextError::BadFont(found.id().to_string());
        let face = match found {
            FontRef::Bundled(info) => match info.kind {
                FontKind::TrueType => Face::parse(info.data, 0).map_err(|_| bad())?,
                FontKind::SingleLine | FontKind::Pixel => {
                    let e = if info.kind == FontKind::Pixel {
                        crate::stroke_fonts::pixel_font()
                    } else {
                        crate::stroke_fonts::line_font()
                    };
                    return Ok(LoadedFont {
                        source: Source::Embroidery(e),
                        unit_per_mm: 10.0 / e.cap_height,
                        pe_unit_per_mm: 10.0 / e.cap_height,
                    });
                }
            },
            FontRef::Design(f) => match &f.data {
                FontData::TrueType { face, bytes } => {
                    parse_face(&bytes.0, *face).ok_or_else(bad)?
                }
                FontData::Embroidery(e) => {
                    if !(e.cap_height.is_finite() && e.cap_height > 0.0) {
                        return Err(bad());
                    }
                    return Ok(LoadedFont {
                        source: Source::Embroidery(e),
                        unit_per_mm: 10.0 / e.cap_height,
                        pe_unit_per_mm: 10.0 / e.cap_height,
                    });
                }
            },
        };
        let em = f64::from(face.units_per_em());
        let cap = face
            .capital_height()
            .filter(|&h| h > 0)
            .map(f64::from)
            .or_else(|| {
                face.glyph_index('H')
                    .and_then(|g| face.glyph_bounding_box(g))
                    .map(|b| f64::from(b.y_max))
                    .filter(|&h| h > 0.0)
            })
            // CJK fonts have no Latin capitals; their ideographs fill most of the em.
            .unwrap_or(0.7 * em)
            .clamp(0.05 * em, 4.0 * em);
        Ok(LoadedFont {
            source: Source::Face(Box::new(face)),
            unit_per_mm: 10.0 / cap,
            pe_unit_per_mm: 10.0 / (0.7 * em),
        })
    }

    pub fn glyph_count(&self) -> usize {
        match &self.source {
            Source::Face(f) => usize::from(f.number_of_glyphs()),
            Source::Embroidery(e) => e.glyphs.len(),
        }
    }

    pub fn has_char(&self, c: char) -> bool {
        self.glyph(c).is_some()
    }

    /// The glyph of `c`: a glyph id, or an index into an embroidery font's glyphs.
    fn glyph(&self, c: char) -> Option<u32> {
        match &self.source {
            // Glyph 0 is .notdef: drawn by no character.
            Source::Face(f) => f
                .glyph_index(c)
                .filter(|g| g.0 != 0)
                .map(|g| u32::from(g.0)),
            Source::Embroidery(e) => e
                .glyphs
                .binary_search_by_key(&c, |g| g.ch)
                .ok()
                .map(|i| i as u32),
        }
    }

    /// Pen advance of `glyph` (the missing-glyph advance for `None`), font units.
    fn advance(&self, glyph: Option<u32>) -> f64 {
        match &self.source {
            Source::Face(f) => f64::from(
                f.glyph_hor_advance(GlyphId(glyph.unwrap_or(0) as u16))
                    .unwrap_or(0),
            ),
            Source::Embroidery(e) => glyph
                .and_then(|i| e.glyphs.get(i as usize))
                .map_or(e.cap_height * 0.5, |g| g.advance),
        }
    }

    /// The shaper for a TrueType/OpenType font; embroidery fonts have none.
    fn shaper(&self, kerning: bool) -> Option<rustybuzz::Face<'a>> {
        match &self.source {
            Source::Face(f) => {
                let face = if kerning {
                    None
                } else {
                    shaping::without_kern_table(f)
                };
                Some(rustybuzz::Face::from_face(
                    face.unwrap_or_else(|| (**f).clone()),
                ))
            }
            Source::Embroidery(_) => None,
        }
    }

    /// Height above the baseline and the height of a vertical cell, font units.
    fn vertical_metrics(&self) -> (f64, f64) {
        match &self.source {
            Source::Face(f) => {
                let (a, d) = (f64::from(f.ascender()), f64::from(f.descender()));
                (a, a - d)
            }
            Source::Embroidery(e) => (e.ascender, e.ascender + e.descender),
        }
    }

    /// `chars` set one glyph per character through the character map, with the font's own
    /// advances (right-to-left pieces reversed), in the shaper's conventions.
    fn unshaped(&self, chars: &[(char, usize)], options: shaping::Options) -> Vec<shaping::Shaped> {
        let (ascender, cell) = self.vertical_metrics();
        let mut out: Vec<shaping::Shaped> = chars
            .iter()
            .map(|&(ch, index)| {
                let glyph = self.glyph(ch);
                let advance = self.advance(glyph);
                let (x_advance, y_advance, x_offset, y_offset) = if options.vertical {
                    (0.0, -cell, -advance / 2.0, -ascender)
                } else {
                    (advance, 0.0, 0.0, 0.0)
                };
                shaping::Shaped {
                    glyph,
                    cluster: index,
                    x_advance,
                    y_advance,
                    x_offset,
                    y_offset,
                }
            })
            .collect();
        // An embroidery font's kerning pairs (#529), between neighbours on a line.
        if let Source::Embroidery(e) = &self.source
            && !e.kerning.is_empty()
            && !options.vertical
        {
            for i in 1..out.len() {
                let (l, r) = (chars[i - 1].0, chars[i].0);
                out[i - 1].x_advance += e.kern(l, r);
            }
        }
        if options.rtl && !options.vertical {
            out.reverse();
        }
        out
    }

    /// Baseline to baseline at 100 % line spacing, font units.
    fn line_height(&self, metrics: TextMetrics) -> f64 {
        match &self.source {
            Source::Face(f) => {
                let hhea =
                    i32::from(f.ascender()) - i32::from(f.descender()) + i32::from(f.line_gap());
                let win = f
                    .tables()
                    .os2
                    .map(|t| i32::from(t.windows_ascender()))
                    .filter(|&a| a > 0);
                f64::from(match (metrics, win) {
                    (TextMetrics::Pe, Some(a)) => a,
                    _ => hhea,
                })
            }
            Source::Embroidery(e) => e.ascender + e.descender + e.line_gap,
        }
    }

    /// 0.1 mm per font unit per mm of size.
    fn per_mm(&self, metrics: TextMetrics) -> f64 {
        match metrics {
            TextMetrics::Pe => self.pe_unit_per_mm,
            TextMetrics::CapHeight => self.unit_per_mm,
        }
    }

    /// Font units → 0.1 mm at size 1 mm: TrueType is +y up, embroidery fonts +y down.
    fn units(&self, scale: f64) -> Transform {
        let flip = match self.source {
            Source::Face(_) => -1.0,
            Source::Embroidery(_) => 1.0,
        };
        Transform {
            a: scale,
            b: 0.0,
            c: 0.0,
            d: flip * scale,
            tx: 0.0,
            ty: 0.0,
        }
    }

    /// The outline and stitches of `glyph`; `Err` when it is over [`MAX_GLYPH_SEGMENTS`].
    fn outline(&self, glyph: u32) -> Result<Option<Outline>, ()> {
        match &self.source {
            Source::Face(f) => {
                let mut b = Builder::default();
                // Glyph data is untrusted: a parser panic loses the glyph, not the application.
                let outlined = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    f.outline_glyph(GlyphId(glyph as u16), &mut b).is_some()
                }));
                if !matches!(outlined, Ok(true)) {
                    return Ok(None);
                }
                if b.too_complex() {
                    return Err(());
                }
                Ok(Some(Outline {
                    segments: b.segments,
                    contours: b.contours,
                    stitches: None,
                }))
            }
            Source::Embroidery(e) => {
                let Some(g) = e.glyphs.get(glyph as usize) else {
                    return Ok(None);
                };
                let segments: usize = g.contours.iter().map(Contour::segment_count).sum();
                if segments > MAX_GLYPH_SEGMENTS {
                    return Err(());
                }
                Ok(
                    (!g.contours.is_empty() || g.stitches.is_some()).then(|| Outline {
                        contours: g.contours.clone(),
                        stitches: g.stitches.clone(),
                        segments: segments + g.stitches.as_ref().map_or(0, |b| b.stitches.len()),
                    }),
                )
            }
        }
    }
}

/// The outline of `ch` in font `name` set at `size` mm, in 0.1 mm with the origin on the
/// baseline at the pen position (+y down), and its advance (0.1 mm); a blank glyph (space)
/// has no contours. `None` when the font has no glyph for it. Font Creator converts TrueType
/// glyphs with this.
pub fn glyph_outline(
    name: &str,
    fonts: &[DesignFont],
    ch: char,
    size: f64,
) -> Result<Option<(Vec<Contour>, f64)>, TextError> {
    let font = LoadedFont::load_in(name, fonts)?;
    let Some(glyph) = font.glyph(ch) else {
        return Ok(None);
    };
    let scale = size * font.unit_per_mm;
    let advance = font.advance(Some(glyph)) * scale;
    let at = font.units(scale);
    match font.outline(glyph) {
        Ok(Some(outline)) => Ok(Some((
            outline
                .contours
                .iter()
                .map(|c| c.transformed(&at))
                .collect(),
            advance,
        ))),
        Ok(None) => Ok(Some((Vec::new(), advance))),
        Err(()) => Ok(None),
    }
}

/// Where each glyph sits: font-unit origin → design transform.
struct Placed {
    ch: char,
    index: usize,
    glyph: Option<u32>,
    at: Transform,
}

/// Characters a shaper hides (format controls, variation selectors, …): never "missing".
fn is_default_ignorable(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{202A}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

/// One glyph of a [`Run`]: its transform from the scaled glyph to its anchor, and the anchor
/// (0.1 mm, relative to the line's start on the baseline, or the column's top centre).
struct RunGlyph {
    local: Transform,
    anchor: Vec2,
}

/// A line or column of shaped glyphs placed one after another, with letter spacing and
/// per-character edits.
struct Run {
    glyphs: Vec<RunGlyph>,
    /// Advance of the whole run, 0.1 mm (along x, or down for vertical text).
    length: f64,
}

impl Run {
    fn along(
        line: &[shaping::Shaped],
        spec: &TextSpec,
        scale: f64,
        spacing: f64,
        vertical: bool,
        cap: f64,
    ) -> Run {
        // Pen position and glyph origin of each glyph, along the run.
        let mut pen = 0.0;
        let mut started = false;
        let mut at = Vec::with_capacity(line.len());
        for g in line {
            let k = spec
                .char_edit(g.cluster)
                .map_or(1.0, |e| e.factor(spec.size));
            let advance = if vertical { -g.y_advance } else { g.x_advance } * scale * k;
            if advance != 0.0 {
                if started {
                    pen += spacing;
                }
                started = true;
            }
            let offset = v2(g.x_offset, -g.y_offset) * (scale * k);
            at.push((pen, advance, offset, k));
            pen += advance;
        }
        // Each cluster turns and scales about the middle of the advance its glyphs span.
        let middle = |cluster: usize| {
            let (lo, hi) = line
                .iter()
                .zip(&at)
                .filter(|(g, _)| g.cluster == cluster)
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), (_, a)| {
                    (lo.min(a.0), hi.max(a.0 + a.1))
                });
            (lo + hi) / 2.0
        };
        let glyphs = line
            .iter()
            .zip(&at)
            .map(|(g, &(pen, _, offset, k))| {
                let mid = middle(g.cluster);
                let (anchor, origin) = if vertical {
                    (v2(0.0, mid), v2(offset.x, pen + offset.y - mid))
                } else {
                    (v2(mid, 0.0), v2(pen + offset.x - mid, offset.y))
                };
                let mut local = Transform::scaling(k, k, v2(0.0, 0.0))
                    .then(&Transform::translation(origin.x, origin.y));
                if let Some(e) = spec.char_edit(g.cluster) {
                    // Turned about the middle of the letter: half its capital height up.
                    let pivot = if vertical {
                        v2(0.0, 0.0)
                    } else {
                        v2(0.0, -cap * k / 2.0)
                    };
                    local = local
                        .then(&Transform::rotation(e.rotation, pivot))
                        .then(&Transform::translation(e.x * 10.0, e.y * 10.0));
                }
                RunGlyph { local, anchor }
            })
            .collect();
        Run {
            glyphs,
            length: pen,
        }
    }
}

/// Lays `spec` out with a bundled font; see [`layout_in`].
pub fn layout(spec: &TextSpec, path: Option<&Contour>) -> Result<Layout, TextError> {
    layout_in(spec, path, &[])
}

/// Lays `spec` out into glyph contours and stitches (0.1 mm, `spec.transform` applied), its
/// font taken from the bundled fonts or `fonts`. `path` is the contour of the path object when
/// the text is attached to one; lines are then joined by spaces.
pub fn layout_in(
    spec: &TextSpec,
    path: Option<&Contour>,
    fonts: &[DesignFont],
) -> Result<Layout, TextError> {
    if let (Some(nd), None) = (&spec.name_drop, path) {
        // Laid out untransformed, fitted into the box, then placed.
        let plain = TextSpec {
            name_drop: None,
            transform: Transform::IDENTITY,
            ..spec.clone()
        };
        let mut l = layout_in(&plain, None, fonts)?;
        let points = l
            .glyphs
            .iter()
            .flat_map(|g| {
                g.contours.iter().flat_map(Contour::outline_points).chain(
                    g.stitches
                        .iter()
                        .flat_map(|b| b.stitches.iter().map(|s| v2(s.x, s.y))),
                )
            })
            .collect::<Vec<_>>();
        if let Some(b) = Rect::enclosing(points) {
            let t = nd.fit(&b).then(&spec.transform);
            for g in &mut l.glyphs {
                g.placement = g.placement.map(|p| p.then(&t));
                for c in &mut g.contours {
                    *c = c.transformed(&t);
                }
                if let Some(block) = &mut g.stitches {
                    for st in &mut block.stitches {
                        let p = t.apply(v2(st.x, st.y));
                        (st.x, st.y) = (p.x, p.y);
                    }
                }
            }
        }
        return Ok(l);
    }
    if let (Some(e), None, TextDirection::Horizontal) =
        (&spec.custom_envelope, path, spec.direction)
    {
        return custom_envelope_layout(spec, e, fonts);
    }
    if let (Some(n), None, TextDirection::Horizontal) = (spec.envelope, path, spec.direction) {
        return envelope_layout(spec, n, fonts);
    }
    let font = LoadedFont::load_in(&spec.font, fonts)?;
    let scale = spec.size * font.per_mm(spec.metrics);
    // Capital height, 0.1 mm.
    let cap = scale * 10.0 / font.unit_per_mm;
    let spacing = spec.letter_spacing * 10.0;
    let line_height = font.line_height(spec.metrics) * scale * spec.line_spacing_percent / 100.0
        + spec.line_spacing * 10.0;
    let vertical = spec.direction == TextDirection::Vertical && path.is_none();
    let shaper = font.shaper(spec.kerning);
    let mut missing = Vec::new();
    let mut shaping = Vec::new();

    // Lines of (character, index in spec.text); text on a path is one line.
    let mut lines: Vec<Vec<(char, usize)>> = vec![Vec::new()];
    let all: Vec<char> = spec.text.chars().collect();
    for (i, &ch) in all.iter().enumerate() {
        let ch = match ch {
            '\r' | '\n' if path.is_some() => ' ',
            '\n' => {
                lines.push(Vec::new());
                continue;
            }
            '\t' => ' ',
            c if c.is_control() => continue,
            c => c,
        };
        if let Some(line) = lines.last_mut() {
            line.push((ch, i));
        }
    }

    // Shaped glyphs of each line, in visual order (top to bottom for vertical text).
    let options = |rtl| shaping::Options {
        rtl,
        vertical,
        kerning: spec.kerning,
    };
    let mut shaped_lines = Vec::with_capacity(lines.len());
    for line in &lines {
        let text: String = line.iter().map(|c| c.0).collect();
        let pieces = if vertical {
            // Columns are not reordered: characters run top to bottom in logical order.
            vec![shaping::Piece {
                range: 0..text.len(),
                rtl: false,
            }]
        } else {
            shaping::visual_pieces(&text)
        };
        // Byte offset in `text` → position in `line`.
        let at: Vec<usize> = text.char_indices().map(|(b, _)| b).collect();
        let pos = |b: usize| at.partition_point(|&x| x < b);
        let mut out = Vec::new();
        for piece in pieces {
            let part = &line[pos(piece.range.start)..pos(piece.range.end)];
            let shaped = shaper
                .as_ref()
                .and_then(|face| shaping::shape(face, part, options(piece.rtl)));
            match shaped {
                Some(glyphs) => out.extend(glyphs),
                None => {
                    for &(ch, _) in part {
                        if let Some(note) = shaping_note(ch)
                            && !shaping.contains(&note)
                        {
                            shaping.push(note);
                        }
                    }
                    out.extend(font.unshaped(part, options(piece.rtl)));
                }
            }
        }
        // Characters whose cluster came out as .notdef.
        let mut unmapped = Vec::new();
        let mut clusters: Vec<usize> = out.iter().map(|g| g.cluster).collect();
        clusters.sort_unstable();
        clusters.dedup();
        for g in out.iter().filter(|g| g.glyph.is_none()) {
            let end = clusters
                .iter()
                .find(|&&c| c > g.cluster)
                .copied()
                .unwrap_or_else(|| line.last().map_or(0, |c| c.1 + 1));
            unmapped.extend(g.cluster..end);
        }
        // In logical order, each once.
        unmapped.sort_unstable();
        for &(ch, i) in line {
            if unmapped.binary_search(&i).is_ok()
                && !font.has_char(ch)
                && !ch.is_whitespace()
                && !is_default_ignorable(ch)
                && !missing.contains(&ch)
            {
                missing.push(ch);
            }
        }
        shaped_lines.push(out);
    }
    let char_of = |index: usize| all.get(index).copied().unwrap_or(' ');

    let glyph_scale = font.units(scale);
    let mut placed = Vec::new();
    match path.map(|c| PathWalker::new(c, spec.fit.other_side)) {
        Some(walker) => {
            let line = shaped_lines.concat();
            let run = Run::along(&line, spec, scale, spacing, false, cap);
            let shift = match spec.align {
                TextAlign::Left => 0.0,
                TextAlign::Center => (walker.length - run.length) / 2.0,
                TextAlign::Right => walker.length - run.length,
            };
            // Stretched: the letters grow until the text fills the whole path.
            let grow = match spec.fit.spread {
                Spread::Fill if run.length > 0.0 => walker.length / run.length,
                _ => 1.0,
            };
            // Glyphs stand on y = 0 with capitals up (−y): move them down across the path, by
            // the size (PE-DESIGN 11.50 hangs 8 mm Lato Bold 8.0 mm below an outline, not its
            // 8.26 mm capital height; compat text-fit-valign-*).
            let drop = match spec.fit.valign {
                VAlign::Bottom => 0.0,
                VAlign::Middle => spec.size * 10.0 / 2.0,
                VAlign::Top => spec.size * 10.0,
            };
            // Spaced evenly: each character in the middle of its share of the path.
            let mut clusters: Vec<usize> = line.iter().map(|g| g.cluster).collect();
            clusters.dedup();
            for (g, p) in line.iter().zip(&run.glyphs) {
                let along = match spec.fit.spread {
                    Spread::None => shift + p.anchor.x,
                    Spread::Fill => p.anchor.x * grow,
                    Spread::Even => {
                        let k = clusters.iter().position(|&c| c == g.cluster).unwrap_or(0);
                        walker.length * (k as f64 + 0.5) / clusters.len().max(1) as f64
                    }
                };
                let (q, angle) = walker.at(along);
                let turn = if spec.fit.upright { 0.0 } else { angle };
                let at = glyph_scale
                    .then(&p.local)
                    .then(&Transform::translation(0.0, drop))
                    .then(&Transform::scaling(grow, grow, v2(0.0, 0.0)))
                    .then(&Transform::rotation(turn.to_degrees(), v2(0.0, 0.0)))
                    .then(&Transform::translation(q.x, q.y));
                placed.push(Placed {
                    ch: char_of(g.cluster),
                    index: g.cluster,
                    glyph: g.glyph,
                    at,
                });
            }
        }
        None if spec.arc.is_some_and(|r| r.abs() >= MIN_ARC) && !vertical => {
            let r = spec.arc.unwrap_or_default();
            let runs: Vec<Run> = shaped_lines
                .iter()
                .map(|line| Run::along(line, spec, scale, spacing, false, cap))
                .collect();
            let lengths: Vec<f64> = runs.iter().map(|r| r.length).collect();
            let need = arc_radius_needed(&lengths, line_height, cap, r > 0.0);
            if r.abs() * 10.0 < need - 1e-6 {
                return Err(TextError::ArcTooTight((need / 10.0 * 10.0).ceil() / 10.0));
            }
            for (row, (line, run)) in shaped_lines.iter().zip(&runs).enumerate() {
                let shift = match spec.align {
                    TextAlign::Left => 0.0,
                    TextAlign::Center => -run.length / 2.0,
                    TextAlign::Right => -run.length,
                };
                for (g, p) in line.iter().zip(&run.glyphs) {
                    let along = shift + p.anchor.x;
                    let (q, turn) = arc_point(r * 10.0, row as f64 * line_height, along);
                    let at = glyph_scale
                        .then(&p.local)
                        .then(&Transform::translation(0.0, p.anchor.y))
                        .then(&Transform::rotation(turn.to_degrees(), v2(0.0, 0.0)))
                        .then(&Transform::translation(q.x, q.y));
                    placed.push(Placed {
                        ch: char_of(g.cluster),
                        index: g.cluster,
                        glyph: g.glyph,
                        at,
                    });
                }
            }
        }
        None => {
            for (row, line) in shaped_lines.iter().enumerate() {
                let run = Run::along(line, spec, scale, spacing, vertical, cap);
                let shift = match spec.align {
                    TextAlign::Left => 0.0,
                    TextAlign::Center => -run.length / 2.0,
                    TextAlign::Right => -run.length,
                };
                // Rows go down; vertical columns go from right to left.
                let origin = if vertical {
                    v2(-(row as f64) * line_height, shift)
                } else {
                    v2(shift, row as f64 * line_height)
                };
                for (g, p) in line.iter().zip(&run.glyphs) {
                    let at = glyph_scale.then(&p.local).then(&Transform::translation(
                        origin.x + p.anchor.x,
                        origin.y + p.anchor.y,
                    ));
                    placed.push(Placed {
                        ch: char_of(g.cluster),
                        index: g.cluster,
                        glyph: g.glyph,
                        at,
                    });
                }
            }
        }
    }

    let mut glyphs = Vec::new();
    let mut segments = 0usize;
    for g in placed {
        let Some(glyph) = g.glyph else { continue };
        let outline = match font.outline(glyph) {
            Ok(Some(o)) => o,
            Ok(None) => continue,
            Err(()) => {
                if !missing.contains(&g.ch) {
                    missing.push(g.ch);
                }
                continue;
            }
        };
        segments += outline.segments;
        if segments > MAX_LAYOUT_SEGMENTS {
            return Err(TextError::TooComplex);
        }
        let at = g.at.then(&spec.transform);
        let contours: Vec<Contour> = outline
            .contours
            .iter()
            .map(|c| c.transformed(&at))
            .collect();
        let stitches = outline.stitches.map(|mut b| {
            for s in &mut b.stitches {
                let p = at.apply(v2(s.x, s.y));
                (s.x, s.y) = (p.x, p.y);
            }
            b
        });
        glyphs.push(Glyph {
            ch: g.ch,
            index: g.index,
            contours,
            stitches,
            placement: Some(at),
        });
    }
    Ok(Layout {
        glyphs,
        missing,
        shaping,
    })
}

/// The point `along` (0.1 mm, from the top or bottom of the ring) on the baseline of an arched
/// text's line `down` (0.1 mm) below the first, and the baseline's angle there (radians).
/// `r` is the first line's signed radius (0.1 mm, [`TextSpec::arc`]): the centre lies `r`
/// below the first baseline's middle when arching up, `-r` above it when arching down. Lines
/// below the first sit on smaller rings arching up, larger ones arching down, so every line
/// keeps its place below the one before.
pub fn arc_point(r: f64, down: f64, along: f64) -> (Vec2, f64) {
    if r > 0.0 {
        let ring = r - down;
        let a = along / ring;
        (v2(ring * a.dsin(), r - ring * a.dcos()), a)
    } else {
        let ring = -r + down;
        let a = along / ring;
        (v2(ring * a.dsin(), r + ring * a.dcos()), -a)
    }
}

/// The smallest first-line radius (0.1 mm) at which every line of an arched text fits its
/// ring: each line no longer than its ring's circumference, and each ring larger than the
/// capitals' height (the letters would cross the centre). `lengths` are the lines' advances,
/// `step` the line spacing, `up` whether the text arches up (inner rings) or down.
pub fn arc_radius_needed(lengths: &[f64], step: f64, cap: f64, up: bool) -> f64 {
    lengths
        .iter()
        .enumerate()
        .map(|(k, len)| {
            let ring = (len / std::f64::consts::TAU).max(cap);
            if up {
                ring + k as f64 * step
            } else {
                ring - k as f64 * step
            }
        })
        .fold(cap, f64::max)
}

/// Ink bounds of a layout's outlines and stitches.
fn layout_bounds(l: &Layout) -> Option<Rect> {
    Rect::enclosing(l.glyphs.iter().flat_map(|g| {
        g.contours.iter().flat_map(Contour::outline_points).chain(
            g.stitches
                .iter()
                .flat_map(|b| b.stitches.iter().map(|s| v2(s.x, s.y))),
        )
    }))
}

/// Applies `t` to every outline and stitch of `l`.
fn transform_layout(l: &mut Layout, t: &Transform) {
    for g in &mut l.glyphs {
        g.placement = g.placement.map(|p| p.then(t));
        for c in &mut g.contours {
            *c = c.transformed(t);
        }
        for s in g.stitches.iter_mut().flat_map(|b| b.stitches.iter_mut()) {
            let p = t.apply(v2(s.x, s.y));
            (s.x, s.y) = (p.x, p.y);
        }
    }
}

/// The straight text's advance width and where it starts along x (layout coordinates, the
/// alignment point at 0): the shift between a left and a right aligned layout of the text is
/// its advance, as the alignment moves the pen, not the ink.
fn advance(plain: &TextSpec, fonts: &[DesignFont]) -> Result<(f64, f64), TextError> {
    let min_x = |align: TextAlign| -> Result<f64, TextError> {
        let probe = TextSpec {
            align,
            ..plain.clone()
        };
        Ok(layout_bounds(&layout_in(&probe, None, fonts)?).map_or(0.0, |b| b.min_x))
    };
    let adv = min_x(TextAlign::Left)? - min_x(TextAlign::Right)?;
    let start = match plain.align {
        TextAlign::Left => 0.0,
        TextAlign::Center => -adv / 2.0,
        TextAlign::Right => -adv,
    };
    Ok((adv, start))
}

/// Text Transform: the straight text set along an arch (1, 2) or warped into an envelope.
/// Presets OpenStitch does not lay out yet lay the text out straight.
fn envelope_layout(spec: &TextSpec, preset: u8, fonts: &[DesignFont]) -> Result<Layout, TextError> {
    let plain = TextSpec {
        envelope: None,
        custom_envelope: None,
        transform: Transform::IDENTITY,
        ..spec.clone()
    };
    let mut l = layout_in(&plain, None, fonts)?;
    let Some(b) = layout_bounds(&l) else {
        return Ok(l);
    };
    if matches!(preset, 1 | 2) && b.width() > 0.0 {
        // The inner line keeps the text's width: the baseline over the top of a circle (1),
        // the capital line under the bottom of one (2), letters radial and turned with it.
        let (adv, start) = advance(&plain, fonts)?;
        // The level is PE-DESIGN's half-sweep: the radius keeps the text length, so the arch's
        // angle grows with it (30 degrees: 383.7, 41.2 degrees: 279.3 for OpenStitch at 10 mm).
        let level = spec
            .envelope_level
            .unwrap_or_else(|| envelope_default_level(preset));
        let angle = ARCH_ANGLE * level / envelope_default_level(preset);
        let r = adv / angle;
        let cx = start + adv / 2.0 + ARCH_SHIFT * adv;
        let reach = (angle / 2.0 + 0.6).min(std::f64::consts::PI * 0.98);
        let steps = 240;
        let at = |k: usize| -reach + 2.0 * reach * k as f64 / steps as f64;
        let (points, fit) = if preset == 1 {
            let c = v2(cx, r);
            let p = (0..=steps).map(|k| c + v2(r * at(k).dsin(), -r * at(k).dcos()));
            (p.collect(), PathFit::default())
        } else {
            let c = v2(cx, -spec.size * 10.0 - r);
            let p = (0..=steps).map(|k| c + v2(r * at(k).dsin(), r * at(k).dcos()));
            let fit = PathFit {
                valign: VAlign::Top,
                ..PathFit::default()
            };
            (p.collect(), fit)
        };
        let arched = TextSpec {
            align: TextAlign::Center,
            fit,
            letter_spacing: plain.letter_spacing + ARCH_LETTER_SPACING,
            ..plain
        };
        l = layout_in(&arched, Some(&Contour::new(points, false)), fonts)?;
    } else if let Some(e) = CustomEnvelope::from_preset(preset, spec.envelope_level) {
        // The level scales how far the edges bow from the straight box (`envelope_scale`).
        warp_layout(&mut l, &b, spec, &plain, fonts, &e);
    }
    transform_layout(&mut l, &spec.transform);
    Ok(l)
}

/// A free envelope (#535): the straight text warped by `e` as a preset's envelope warps it.
fn custom_envelope_layout(
    spec: &TextSpec,
    e: &CustomEnvelope,
    fonts: &[DesignFont],
) -> Result<Layout, TextError> {
    let plain = TextSpec {
        envelope: None,
        custom_envelope: None,
        transform: Transform::IDENTITY,
        ..spec.clone()
    };
    let mut l = layout_in(&plain, None, fonts)?;
    if e.is_valid()
        && let Some(b) = layout_bounds(&l)
    {
        warp_layout(&mut l, &b, spec, &plain, fonts, e);
    }
    transform_layout(&mut l, &spec.transform);
    Ok(l)
}

/// The box an envelope warps: the straight text's left and width and its capital-height box
/// (capital line to baseline; a text of several lines keeps its own box), 0.1 mm.
fn envelope_box(b: &Rect, spec: &TextSpec, plain: &TextSpec, fonts: &[DesignFont]) -> (f64, f64) {
    // PE-DESIGN warps the cap-height box, from the capital line to the baseline (y = 0):
    // the envelopes were fitted on HHHHHHH, whose box it is, and letters below the
    // baseline go on below the bottom edge (round-4 captures: "OpenStitch", "Agjy 123").
    let cap = TextSpec {
        text: "H".into(),
        characters: Vec::new(),
        ..plain.clone()
    };
    let cap_top = layout_in(&cap, None, fonts)
        .ok()
        .as_ref()
        .and_then(layout_bounds)
        .map(|c| c.min_y);
    match cap_top {
        Some(t) if t < 0.0 && !spec.text.contains('\n') => (t, -t),
        _ => (b.min_y, b.height()),
    }
}

/// Warps the straight layout `l` (bounds `b`) into envelope `e`.
fn warp_layout(
    l: &mut Layout,
    b: &Rect,
    spec: &TextSpec,
    plain: &TextSpec,
    fonts: &[DesignFont],
    e: &CustomEnvelope,
) {
    let (top, bottom, k) = (&e.top, &e.bottom, e.scale);
    let (cap_y, cap_h) = envelope_box(b, spec, plain, fonts);
    let warp = |p: Vec2| {
        let u = ((p.x - b.min_x) / b.width().max(1e-9)).clamp(0.0, 1.0);
        let v = (p.y - cap_y) / cap_h.max(1e-9);
        let (t, bt) = (edge_at(top, u), edge_at(bottom, u));
        let (t, bt) = (
            v2(u + k * (t.x - u), k * t.y),
            v2(u + k * (bt.x - u), 1.0 + k * (bt.y - 1.0)),
        );
        v2(
            b.min_x + b.width() * (t.x + v * (bt.x - t.x)),
            cap_y + cap_h * (t.y + v * (bt.y - t.y)),
        )
    };
    for g in &mut l.glyphs {
        g.placement = None;
        for c in &mut g.contours {
            *c = warp_contour(c, &warp);
        }
        for s in g.stitches.iter_mut().flat_map(|b| b.stitches.iter_mut()) {
            let p = warp(v2(s.x, s.y));
            (s.x, s.y) = (p.x, p.y);
        }
    }
}

/// The handles of `spec`'s free envelope on the page (0.1 mm, the text's transform applied):
/// `[top, bottom]`, each `[left, middle, right]`. `None` without a free envelope.
pub fn envelope_handles(spec: &TextSpec, fonts: &[DesignFont]) -> Option<[[Vec2; 3]; 2]> {
    let e = spec.custom_envelope.as_ref().filter(|e| e.is_valid())?;
    let (b, box_y) = envelope_frame(spec, fonts)?;
    let at = |edge, u| {
        let p = e.point(edge, u);
        spec.transform
            .apply(v2(b.0 + b.1 * p.x, box_y.0 + box_y.1 * p.y))
    };
    let edge = |e| [at(e, 0.0), at(e, 0.5), at(e, 1.0)];
    Some([edge(EnvelopeEdge::Top), edge(EnvelopeEdge::Bottom)])
}

/// `spec`'s free envelope edges on the page, `steps + 1` points each (0.1 mm): `[top, bottom]`.
pub fn envelope_outline(
    spec: &TextSpec,
    fonts: &[DesignFont],
    steps: usize,
) -> Option<[Vec<Vec2>; 2]> {
    let e = spec.custom_envelope.as_ref().filter(|e| e.is_valid())?;
    let (b, box_y) = envelope_frame(spec, fonts)?;
    let edge = |edge| {
        (0..=steps)
            .map(|i| {
                let p = e.point(edge, i as f64 / steps as f64);
                spec.transform
                    .apply(v2(b.0 + b.1 * p.x, box_y.0 + box_y.1 * p.y))
            })
            .collect()
    };
    Some([edge(EnvelopeEdge::Top), edge(EnvelopeEdge::Bottom)])
}

/// The straight text's left and width, and its envelope box's top and height (0.1 mm, before
/// the text's transform): what box fractions of an envelope measure.
pub fn envelope_frame(spec: &TextSpec, fonts: &[DesignFont]) -> Option<((f64, f64), (f64, f64))> {
    let plain = TextSpec {
        envelope: None,
        custom_envelope: None,
        transform: Transform::IDENTITY,
        ..spec.clone()
    };
    let l = layout_in(&plain, None, fonts).ok()?;
    let b = layout_bounds(&l)?;
    let (y, h) = envelope_box(&b, spec, &plain, fonts);
    Some(((b.min_x, b.width()), (y, h)))
}

/// The top and bottom edge of Text Transform preset `n` in a unit box (x right, y down), for
/// drawing its palette button; `None` for presets OpenStitch does not lay out.
pub fn envelope_edges(n: u8) -> Option<(Vec<Vec2>, Vec<Vec2>)> {
    if matches!(n, 1 | 2) {
        // The arches: a 108° arc, the text two fifths of the box thick.
        let arc = |r: f64| -> Vec<Vec2> {
            (0..=24)
                .map(|k| {
                    let a = (k as f64 / 24.0 - 0.5) * ARCH_ANGLE;
                    let (x, y) = (0.5 + r * a.dsin(), r * (1.0 - a.dcos()));
                    if n == 1 {
                        v2(x, y + 0.1)
                    } else {
                        v2(x, 0.9 - y)
                    }
                })
                .collect()
        };
        let (outer, inner) = (arc(0.6), arc(0.4));
        return Some(if n == 1 {
            (outer, inner)
        } else {
            (inner, outer)
        });
    }
    let (_, top, bottom) = crate::envelope_presets::ENVELOPES
        .iter()
        .find(|(k, _, _)| *k == n)?;
    let pts = |e: &crate::envelope_presets::Edge| e.iter().map(|&(x, y)| v2(x, y)).collect();
    Some((pts(top), pts(bottom)))
}

/// The point of an envelope edge at `u` (0..=1), between its knots.
fn edge_at(edge: &[(f64, f64)], u: f64) -> Vec2 {
    let f = u * (edge.len() - 1) as f64;
    let k = (f.floor() as usize).min(edge.len() - 2);
    let t = f - k as f64;
    let (a, b) = (edge[k], edge[k + 1]);
    v2(a.0 + t * (b.0 - a.0), a.1 + t * (b.1 - a.1))
}

/// `c` flattened, its segments split to 0.5 mm, and every point moved by `warp`.
fn warp_contour(c: &Contour, warp: &impl Fn(Vec2) -> Vec2) -> Contour {
    let mut path = c.path();
    if c.closed && path.len() > 1 {
        path.pop();
    }
    let mut out = Vec::with_capacity(path.len() * 2);
    let n = path.len();
    let segments = if c.closed { n } else { n.saturating_sub(1) };
    for i in 0..n {
        out.push(warp(path[i]));
        if i < segments {
            let (a, b) = (path[i], path[(i + 1) % n]);
            let pieces = (a.distance(b) / 5.0).ceil() as usize;
            for k in 1..pieces {
                out.push(warp(a + (b - a) * (k as f64 / pieces as f64)));
            }
        }
    }
    Contour::new(out, c.closed)
}

/// Arc-length lookup along a flattened contour.
struct PathWalker {
    points: Vec<Vec2>,
    /// Cumulative length at each point.
    at: Vec<f64>,
    length: f64,
}

impl PathWalker {
    /// Walks `contour` from its start, or from its end backwards when `reverse`.
    fn new(contour: &Contour, reverse: bool) -> Self {
        let mut points = contour.path();
        points.dedup();
        if reverse {
            points.reverse();
        }
        let mut at = Vec::with_capacity(points.len());
        let mut walked = 0.0;
        for (i, p) in points.iter().enumerate() {
            if i > 0 {
                walked += points[i - 1].distance(*p);
            }
            at.push(walked);
        }
        PathWalker {
            points,
            at,
            length: walked,
        }
    }

    /// Point and tangent angle (radians) at arc length `s`; beyond either end the path is
    /// extended along its end tangent.
    fn at(&self, s: f64) -> (Vec2, f64) {
        let n = self.points.len();
        if n < 2 {
            return (self.points.first().copied().unwrap_or_default(), 0.0);
        }
        let seg = match self.at.partition_point(|&a| a <= s) {
            0 => 0,
            k => (k - 1).min(n - 2),
        };
        let (a, b) = (self.points[seg], self.points[seg + 1]);
        let len = self.at[seg + 1] - self.at[seg];
        let d = b - a;
        let angle = d.y.datan2(d.x);
        let p = if len > 0.0 {
            a + d * ((s - self.at[seg]) / len)
        } else {
            a
        };
        (p, angle)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::contains;
    use crate::model::{Design, DesignObject, RegionSewing, RegionType};
    use crate::{Compiler, generate};
    use openstitch_core::{Command, Thread};

    fn lay(text: &str, size: f64) -> Layout {
        layout(&TextSpec::new(text, DEFAULT_FONT, size), None).expect("layout")
    }

    fn bounds(contours: &[Contour]) -> Rect {
        Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("bounds")
    }

    #[test]
    fn bundled_fonts_parse_and_cover_ascii() {
        for f in FONTS.iter().filter(|f| f.kind == FontKind::TrueType) {
            let font = LoadedFont::load(f.id).expect(f.id);
            assert!(font.glyph_count() > 90, "{}", f.id);
            assert!((' '..='~').all(|c| font.has_char(c)), "{}", f.id);
        }
        assert!(font("Lato").is_some() && font("lato bold").is_some());
        assert!(font("Comic Sans").is_none());
    }

    #[test]
    fn quadratic_segments_become_exact_cubics() {
        let mut b = Builder::default();
        b.move_to(0.0, 0.0);
        b.quad_to(10.0, 20.0, 20.0, 0.0);
        b.line_to(0.0, 0.0);
        b.close();
        let c = &b.contours[0];
        assert_eq!(c.points, vec![v2(0.0, 0.0), v2(20.0, 0.0)]);
        let bz = c.curve(0).expect("curve");
        // Cubic at t = 1/2 must equal the quadratic's point (10, 10).
        let mid = (v2(0.0, 0.0) + bz.c1 * 3.0 + bz.c2 * 3.0 + v2(20.0, 0.0)) * 0.125;
        assert!(mid.distance(v2(10.0, 10.0)) < 1e-9, "{mid:?}");
        assert_eq!(c.curve(1), None, "closing line stays straight");
    }

    #[test]
    fn glyph_outlines_have_the_expected_contours() {
        for (ch, n) in [
            ('I', 1),
            ('L', 1),
            ('O', 2),
            ('o', 2),
            ('B', 3),
            ('i', 2),
            ('8', 3),
        ] {
            let l = lay(&ch.to_string(), 10.0);
            assert_eq!(l.glyphs.len(), 1, "{ch}");
            assert_eq!(l.glyphs[0].contours.len(), n, "{ch}");
            assert!(l.glyphs[0].contours.iter().all(|c| c.closed), "{ch}");
        }
        // PE-DESIGN's size is 0.7 em: Lato Bold's 'H' (1446 of 2000 units) sits on the
        // baseline (y = 0) and is 10.33 mm tall at size 10.
        let h = bounds(&lay("H", 10.0).contours());
        assert!(h.max_y.abs() < 0.5, "{h:?}");
        assert!((h.height() - 103.3).abs() < 0.5, "{h:?}");
        // With the capital-height metrics of older files, it is exactly the size.
        let mut old = TextSpec::new("H", DEFAULT_FONT, 10.0);
        old.metrics = TextMetrics::CapHeight;
        let h = bounds(&layout(&old, None).expect("layout").contours());
        assert!((h.height() - 100.0).abs() < 0.5, "{h:?}");
        // 'O' is round: its outline has curves; 'I' is a rectangle without any.
        assert!(lay("O", 10.0).glyphs[0].contours[0].has_curves());
        assert!(!lay("I", 10.0).glyphs[0].contours[0].has_curves());
    }

    #[test]
    fn counters_are_holes_under_even_odd_fill() {
        let o = lay("O", 20.0).contours();
        let b = bounds(&o);
        assert!(!contains(b.center(), &o), "counter of O is a hole");
        let ring = v2(b.min_x + 10.0, b.center().y);
        assert!(contains(ring, &o), "stroke of O is inside");

        let mut d = Design::default();
        let (obj, _) = DesignObject::text(
            TextSpec::new("O", DEFAULT_FONT, 20.0),
            Some(RegionSewing::new(RegionType::Fill, Thread::rgb(0))),
            None,
        )
        .expect("text");
        let inner = obj.contours[1].clone();
        d.add(obj);
        let p = Compiler::default().compile(&d);
        assert!(p.count(Command::Stitch) > 50);
        let edges = inner.edges();
        // Span ends lie on the counter's edge; nothing may sit strictly inside it.
        let inside_counter = p
            .stitches
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .filter(|s| contains(v2(s.x, s.y), std::slice::from_ref(&inner)))
            .map(|s| {
                let q = v2(s.x, s.y);
                edges
                    .iter()
                    .map(|&(a, b)| {
                        let ab = b - a;
                        let t = (((q - a).x * ab.x + (q - a).y * ab.y)
                            / (ab.x * ab.x + ab.y * ab.y))
                            .clamp(0.0, 1.0);
                        q.distance(a + ab * t)
                    })
                    .fold(f64::INFINITY, f64::min)
            })
            .filter(|&d| d > 0.05)
            .count();
        assert_eq!(inside_counter, 0, "no needle point inside the counter of O");
    }

    #[test]
    fn size_scales_linearly() {
        let small = bounds(&lay("Hag", 10.0).contours());
        let big = bounds(&lay("Hag", 25.0).contours());
        assert!((big.width() / small.width() - 2.5).abs() < 1e-6);
        assert!((big.height() / small.height() - 2.5).abs() < 1e-6);
    }

    #[test]
    fn letter_spacing_lines_and_alignment() {
        let tight = bounds(&lay("II", 10.0).contours());
        let mut spec = TextSpec::new("II", DEFAULT_FONT, 10.0);
        spec.letter_spacing = 3.0;
        let wide = bounds(&layout(&spec, None).expect("layout").contours());
        assert!((wide.width() - tight.width() - 30.0).abs() < 1e-6);

        let mut spec = TextSpec::new("H\nH", DEFAULT_FONT, 10.0);
        spec.align = TextAlign::Center;
        let two = layout(&spec, None).expect("layout");
        let (a, b) = (
            bounds(&two.glyphs[0].contours),
            bounds(&two.glyphs[1].contours),
        );
        assert!(b.min_y - a.min_y > 100.0, "second line is lower");
        assert!(a.center().x.abs() < 0.5, "centred on x = 0");
        spec.line_spacing = 5.0;
        let spaced = layout(&spec, None).expect("layout");
        let b2 = bounds(&spaced.glyphs[1].contours);
        assert!((b2.min_y - b.min_y - 50.0).abs() < 1e-6);
    }

    #[test]
    fn missing_characters_are_reported_and_skipped() {
        let l = lay("A\u{4E2D}B\u{4E2D}", 10.0);
        assert_eq!(l.missing, vec!['\u{4E2D}']);
        assert_eq!(
            l.glyphs.iter().map(|g| g.ch).collect::<String>(),
            "AB".to_string()
        );
        assert!(lay(" \t ", 10.0).glyphs.is_empty());
    }

    #[test]
    fn unknown_font_is_an_error() {
        let e = layout(&TextSpec::new("A", "nope", 10.0), None).expect_err("unknown");
        assert_eq!(e, TextError::UnknownFont("nope".into()));
    }

    #[test]
    fn line_spacing_percent_scales_the_line_pitch() {
        let pitch = |percent: f64, mm: f64| {
            let mut spec = TextSpec::new(
                "H
H",
                DEFAULT_FONT,
                10.0,
            );
            spec.line_spacing_percent = percent;
            spec.line_spacing = mm;
            let l = layout(&spec, None).expect("layout");
            let tops: Vec<f64> = l.glyphs.iter().map(|g| bounds(&g.contours).min_y).collect();
            tops[1] - tops[0]
        };
        // PE-DESIGN's 100 %: Lato Bold's Windows ascent (2233 of 2000 units) at 0.7 em = size,
        // 15.95 mm at size 10 (PE-DESIGN 11.50: 15.9, compat/reference/text).
        let base = pitch(100.0, 0.0);
        assert!((base - 159.5).abs() < 0.2, "{base}");
        assert!((pitch(150.0, 0.0) - base * 1.5).abs() < 1e-6);
        assert!((pitch(50.0, 0.0) - base * 0.5).abs() < 1e-6);
        // Files from before the %: the mm spacing still adds on top.
        assert!((pitch(100.0, 2.0) - base - 20.0).abs() < 1e-6);
        let old: TextSpec =
            serde_json::from_str(r#"{"text":"A","font":"lato-bold","size":10,"lineSpacing":2}"#)
                .expect("old");
        assert_eq!(old.line_spacing_percent, 100.0);
        assert!(
            !serde_json::to_string(&old)
                .expect("json")
                .contains("Percent")
        );
    }

    #[test]
    fn text_on_a_straight_path_matches_straight_text() {
        let straight = lay("Ab", 10.0).contours();
        let path = Contour::new(vec![v2(0.0, 300.0), v2(2000.0, 300.0)], false);
        let on = layout(&TextSpec::new("Ab", DEFAULT_FONT, 10.0), Some(&path))
            .expect("layout")
            .contours();
        let shifted: Vec<Contour> = straight
            .iter()
            .map(|c| c.transformed(&Transform::translation(0.0, 300.0)))
            .collect();
        for (a, b) in on.iter().zip(&shifted) {
            for (p, q) in a.points.iter().zip(&b.points) {
                assert!(p.distance(*q) < 1e-6);
            }
        }
    }

    #[test]
    fn fit_options_place_text_across_and_on_the_other_side_of_the_path() {
        let path = Contour::new(vec![v2(0.0, 300.0), v2(2000.0, 300.0)], false);
        let on = |fit: PathFit| {
            let mut spec = TextSpec::new("HH", DEFAULT_FONT, 10.0);
            spec.fit = fit;
            let l = layout(&spec, Some(&path)).expect("layout");
            bounds(&l.contours())
        };
        // Above the line: the capitals (10.33 mm at size 10) stand on the path (y down).
        let base = on(PathFit::default());
        assert!(
            (base.max_y - 300.0).abs() < 1.0 && (base.min_y - 196.7).abs() < 1.0,
            "{base:?}"
        );
        // Centred and below move the text down by half the size and by the size, as PE-DESIGN.
        let middle = on(PathFit {
            valign: VAlign::Middle,
            ..PathFit::default()
        });
        assert!((middle.max_y - 350.0).abs() < 1.0, "{middle:?}");
        let top = on(PathFit {
            valign: VAlign::Top,
            ..PathFit::default()
        });
        assert!((top.max_y - 400.0).abs() < 1.0, "{top:?}");
        // The other side: followed from its end, the text hangs below the path, upside down,
        // starting at the far end.
        let other = on(PathFit {
            other_side: true,
            ..PathFit::default()
        });
        assert!((other.min_y - 300.0).abs() < 1.0, "{other:?}");
        assert!(other.min_x > 1700.0, "{other:?}");
        assert!((other.width() - base.width()).abs() < 1.0);
    }

    #[test]
    fn spaced_and_stretched_text_spread_over_the_whole_path() {
        let path = Contour::new(vec![v2(0.0, 0.0), v2(1000.0, 0.0)], false);
        let glyphs = |spread: Spread| {
            let mut spec = TextSpec::new("HHHH", DEFAULT_FONT, 10.0);
            spec.fit.spread = spread;
            let l = layout(&spec, Some(&path)).expect("layout");
            l.glyphs
                .iter()
                .map(|g| bounds(&g.contours))
                .collect::<Vec<_>>()
        };
        // Spaced: each letter in the middle of a quarter of the 100 mm path.
        for (k, b) in glyphs(Spread::Even).iter().enumerate() {
            assert!(
                (b.center().x - 125.0 - 250.0 * k as f64).abs() < 1.0,
                "{k}: {b:?}"
            );
        }
        // Stretched: the letters grow until the text is as long as the path.
        let plain = glyphs(Spread::None);
        let fill = glyphs(Spread::Fill);
        let k = fill[0].height() / plain[0].height();
        assert!(k > 2.0, "{k}");
        // From end to end of the path, less the letters' side bearings.
        assert!(fill[0].min_x > 0.0 && fill[0].min_x < 40.0, "{:?}", fill[0]);
        assert!(
            fill[3].max_x < 1000.0 && fill[3].max_x > 960.0,
            "{:?}",
            fill[3]
        );
    }

    #[test]
    fn text_transform_presets_warp_the_straight_text() {
        let straight = bounds(&lay("HHHHHHH", 10.0).contours());
        let with = |n: u8| {
            let mut spec = TextSpec::new("HHHHHHH", DEFAULT_FONT, 10.0);
            spec.envelope = Some(n);
            layout(&spec, None).expect("layout")
        };
        // Every listed preset lays out, and every envelope in the table is listed.
        for &(n, _, _) in crate::envelope_presets::ENVELOPES {
            assert!(ENVELOPE_PRESETS.contains(&n), "{n}");
        }
        for &n in &ENVELOPE_PRESETS {
            let l = with(n);
            assert_eq!(l.glyphs.len(), 7, "{n}");
        }
        // 3 grows to the right on a straight baseline: the first H half as tall as the last.
        let g = with(3).glyphs;
        let (first, last) = (bounds(&g[0].contours), bounds(&g[6].contours));
        assert!((last.max_y - first.max_y).abs() < 1.0, "{first:?} {last:?}");
        assert!(first.height() < 0.65 * last.height(), "{first:?} {last:?}");
        assert!((last.height() - straight.height()).abs() < 3.0, "{last:?}");
        // 13 fans out: every H keeps its height, its top corners stay put and its bottom
        // corners draw in towards the centre (PE-DESIGN 11.50: the bottom edge is the straight
        // text at 0.70 about its centre, text-transform-13; 14 does the same to the top).
        let plain = lay("HHHHHHH", 10.0);
        for (n, keep_top) in [(13, true), (14, false)] {
            let g = with(n).glyphs;
            let (c, w) = (straight.center().x, straight.width());
            for (i, glyph) in g.iter().enumerate() {
                let b = bounds(&glyph.contours);
                assert!((b.height() - straight.height()).abs() < 2.0, "{n}: {b:?}");
                let pts: Vec<Vec2> = glyph.contours.iter().flat_map(|c| c.path()).collect();
                let edge = |top: bool| -> (f64, f64) {
                    let y = if top { b.min_y } else { b.max_y };
                    let xs = pts.iter().filter(|p| (p.y - y).abs() < 4.0).map(|p| p.x);
                    (
                        xs.clone().fold(f64::INFINITY, f64::min),
                        xs.fold(f64::NEG_INFINITY, f64::max),
                    )
                };
                let (kept, scaled) = if keep_top {
                    (edge(true), edge(false))
                } else {
                    (edge(false), edge(true))
                };
                let sb = bounds(&plain.glyphs[i].contours);
                assert!(
                    (kept.0 - sb.min_x).abs() < 6.0 && (kept.1 - sb.max_x).abs() < 6.0,
                    "{n} {i}: {kept:?} vs {sb:?}"
                );
                let expect = |x: f64| c + (x - c) * 0.7;
                assert!(
                    (scaled.0 - expect(sb.min_x)).abs() < 0.02 * w,
                    "{n} {i}: {scaled:?}"
                );
                assert!(
                    (scaled.1 - expect(sb.max_x)).abs() < 0.02 * w,
                    "{n} {i}: {scaled:?}"
                );
            }
        }
        // 9 slants: the box keeps its width, the first H leans right.
        let b = bounds(&with(9).contours());
        assert!((b.width() - straight.width()).abs() < 3.0, "{b:?}");
        assert!(bounds(&with(9).glyphs[0].contours).width() > 140.0);
        // 1 arches over the top: the middle H stays, the outer ones drop and turn.
        let g = with(1).glyphs;
        let (mid, end) = (bounds(&g[3].contours), bounds(&g[0].contours));
        assert!((mid.min_y - straight.min_y).abs() < 3.0, "{mid:?}");
        assert!(end.min_y > mid.max_y, "{end:?}");
        // 2 arches under: the outer ones rise.
        let g = with(2).glyphs;
        assert!(bounds(&g[0].contours).max_y < bounds(&g[3].contours).min_y);
        // Off by default, and not written to files then.
        let plain = TextSpec::new("H", DEFAULT_FONT, 10.0);
        assert!(
            !serde_json::to_string(&plain)
                .expect("json")
                .contains("envelope")
        );
    }

    #[test]
    fn upright_letters_do_not_turn_with_the_path() {
        let circle = Contour::ellipse(v2(500.0, 500.0), 300.0, 300.0, 180);
        let mut spec = TextSpec::new("HHHHHHHH", DEFAULT_FONT, 8.0);
        spec.fit.upright = true;
        let l = layout(&spec, Some(&circle)).expect("layout");
        let straight = bounds(&lay("H", 8.0).contours());
        for g in &l.glyphs {
            let b = bounds(&g.contours);
            assert!((b.width() - straight.width()).abs() < 0.5, "{b:?}");
            assert!((b.height() - straight.height()).abs() < 0.5, "{b:?}");
        }
    }

    #[test]
    fn text_on_a_circle_follows_it() {
        let center = v2(500.0, 500.0);
        let r = 300.0;
        let circle = Contour::ellipse(center, r, r, 180);
        let spec = TextSpec::new("OPENSTITCH", DEFAULT_FONT, 8.0);
        let l = layout(&spec, Some(&circle)).expect("layout");
        assert_eq!(l.glyphs.len(), 10);
        let mut last_angle = f64::NEG_INFINITY;
        for g in &l.glyphs {
            let c = bounds(&g.contours).center() - center;
            // Glyphs stand on the circle, up = left of travel = outside... or inside
            // depending on direction; either way within one cap height of the circle.
            let d = c.length();
            assert!((d - r).abs() < 80.0, "{} at {d}", g.ch);
            let angle = c.y.atan2(c.x);
            assert!(angle > last_angle, "glyphs advance along the circle");
            last_angle = angle;
        }
        // Vertical path: an 'I' turned to run along +y is wider than tall.
        let down = Contour::new(vec![v2(0.0, 0.0), v2(0.0, 1000.0)], false);
        let i = bounds(
            &layout(&TextSpec::new("I", DEFAULT_FONT, 10.0), Some(&down))
                .expect("layout")
                .contours(),
        );
        assert!(i.width() > 90.0 && i.height() < 40.0, "{i:?}");
    }

    fn lay_spec(spec: &TextSpec) -> Layout {
        layout(spec, None).expect("layout")
    }

    /// Centre x of each outlined glyph, by character.
    fn centres(l: &Layout) -> Vec<(char, f64)> {
        l.outlined()
            .map(|g| (g.ch, bounds(&g.contours).center().x))
            .collect()
    }

    #[test]
    fn kerning_is_applied_only_when_asked() {
        let mut spec = TextSpec::new("AVAV", DEFAULT_FONT, 10.0);
        let plain = bounds(&lay_spec(&spec).contours());
        // Without kerning the advances are the font's own (as before shaping existed).
        let font = LoadedFont::load(DEFAULT_FONT).expect("font");
        let scale = 10.0 * font.per_mm(TextMetrics::Pe);
        let a = font.advance(font.glyph('A')) * scale;
        let v = font.advance(font.glyph('V')) * scale;
        let first = bounds(&lay("A", 10.0).contours());
        let last = bounds(&lay("V", 10.0).contours());
        let expect = 2.0 * a + v + last.max_x - first.min_x;
        assert!(
            (plain.width() - expect).abs() < 1e-6,
            "{plain:?} vs {expect}"
        );
        spec.kerning = true;
        let kerned = bounds(&lay_spec(&spec).contours());
        assert!(
            kerned.width() < plain.width() - 10.0,
            "A-V pairs pull together: {} vs {}",
            kerned.width(),
            plain.width()
        );
    }

    #[test]
    fn each_letter_stays_its_own_glyph() {
        // Standard ligatures are off: "fi" and "ffl" are separate letters.
        let l = lay("fi ffl", 10.0);
        assert_eq!(l.outlined().map(|g| g.ch).collect::<String>(), "fiffl");
        assert_eq!(
            l.outlined().map(|g| g.index).collect::<Vec<_>>(),
            vec![0, 1, 3, 4, 5]
        );
        // The shaper composes a base letter and a combining accent into the precomposed glyph.
        let composed = lay("e\u{0301}", 10.0);
        let precomposed = lay("\u{00E9}", 10.0);
        assert_eq!(composed.outlined().count(), 1);
        assert_eq!(composed.contours(), precomposed.contours());
        assert!(composed.missing.is_empty() && composed.shaping.is_empty());
    }

    #[test]
    fn right_to_left_runs_are_reversed() {
        // A right-to-left override puts the letters right to left (Lato has no Hebrew glyphs;
        // the bidi algorithm is the same).
        let l = lay("\u{202E}AB\u{202C}CD", 10.0);
        let c = centres(&l);
        let x = |ch| c.iter().find(|g| g.0 == ch).expect("glyph").1;
        assert!(x('B') < x('A'), "{c:?}");
        assert!(x('A') < x('C') && x('C') < x('D'), "{c:?}");
        assert!(
            l.missing.is_empty(),
            "format controls are not missing: {:?}",
            l.missing
        );
        assert_eq!(
            l.outlined().map(|g| g.index).collect::<Vec<_>>(),
            vec![2, 1, 4, 5]
        );
    }

    #[test]
    fn vertical_text_runs_down_in_columns_right_to_left() {
        let mut spec = TextSpec::new("HI\nO", DEFAULT_FONT, 10.0);
        spec.direction = TextDirection::Vertical;
        let l = lay_spec(&spec);
        let b: Vec<Rect> = l.outlined().map(|g| bounds(&g.contours)).collect();
        assert_eq!(b.len(), 3);
        let (h, i, o) = (b[0], b[1], b[2]);
        assert!(i.min_y > h.max_y, "I is below H: {h:?} {i:?}");
        assert!(
            h.center().x.abs() < 1.0 && i.center().x.abs() < 1.0,
            "centred on x = 0"
        );
        assert!(o.max_x < h.min_x, "the second column is to the left");
        assert!(
            (o.min_y - h.min_y).abs() < 20.0,
            "columns start at the same height"
        );
        assert!(h.min_y >= 0.0, "the first column starts at y = 0");
        // Centred columns sit about y = 0.
        spec.align = TextAlign::Center;
        let c = bounds(&lay_spec(&spec).outlined().next().expect("H").contours);
        assert!(c.max_y < 0.0, "{c:?}");
        // Text on a path ignores the direction.
        let path = Contour::new(vec![v2(0.0, 300.0), v2(2000.0, 300.0)], false);
        let along = layout(&spec, Some(&path)).expect("layout");
        let b: Vec<Rect> = along.outlined().map(|g| bounds(&g.contours)).collect();
        assert!(b[1].min_x > b[0].max_x);
    }

    #[test]
    fn character_edits_move_turn_and_resize_one_letter() {
        let base = TextSpec::new("HIH", DEFAULT_FONT, 10.0);
        let before: Vec<Rect> = lay_spec(&base)
            .outlined()
            .map(|g| bounds(&g.contours))
            .collect();
        let mut spec = base.clone();
        spec.set_char_edit(CharEdit {
            index: 1,
            x: 2.0,
            y: -3.0,
            ..Default::default()
        });
        let moved: Vec<Rect> = lay_spec(&spec)
            .outlined()
            .map(|g| bounds(&g.contours))
            .collect();
        assert!((moved[1].min_x - before[1].min_x - 20.0).abs() < 1e-6);
        assert!((moved[1].min_y - before[1].min_y + 30.0).abs() < 1e-6);
        assert_eq!(
            moved[2], before[2],
            "an offset does not move the next letter"
        );

        spec.set_char_edit(CharEdit {
            index: 1,
            rotation: 90.0,
            ..Default::default()
        });
        let turned: Vec<Rect> = lay_spec(&spec)
            .outlined()
            .map(|g| bounds(&g.contours))
            .collect();
        assert!(turned[1].width() > turned[1].height(), "{:?}", turned[1]);
        assert!((turned[1].center().x - before[1].center().x).abs() < 1.0);

        spec.set_char_edit(CharEdit {
            index: 1,
            size: Some(20.0),
            ..Default::default()
        });
        let big: Vec<Rect> = lay_spec(&spec)
            .outlined()
            .map(|g| bounds(&g.contours))
            .collect();
        assert!((big[1].height() - 2.0 * before[1].height()).abs() < 1e-6);
        assert!(
            (big[1].max_y - before[1].max_y).abs() < 1e-6,
            "stands on the baseline"
        );
        assert!(
            big[2].min_x > before[2].min_x + 10.0,
            "a larger letter pushes the next one"
        );

        // A no-op edit removes the entry; edits past the end of a shorter text are dropped.
        spec.set_char_edit(CharEdit {
            index: 1,
            ..Default::default()
        });
        assert!(spec.characters.is_empty());
        spec.set_char_edit(CharEdit {
            index: 2,
            x: 1.0,
            ..Default::default()
        });
        spec.text = "HI".into();
        spec.trim_char_edits();
        assert!(spec.characters.is_empty());
    }

    #[test]
    fn new_text_fields_default_and_stay_out_of_old_files() {
        let old: TextSpec =
            serde_json::from_str(r#"{"text":"A","font":"lato-bold","size":10}"#).expect("old");
        assert!(!old.kerning);
        assert_eq!(old.direction, TextDirection::Horizontal);
        assert!(old.characters.is_empty());
        let json = serde_json::to_string(&old).expect("json");
        assert!(
            !json.contains("kerning")
                && !json.contains("direction")
                && !json.contains("characters")
        );
        let mut new = old.clone();
        new.kerning = true;
        new.direction = TextDirection::Vertical;
        new.set_char_edit(CharEdit {
            index: 0,
            rotation: 15.0,
            ..Default::default()
        });
        let json = serde_json::to_string(&new).expect("json");
        assert!(json.contains(r#""direction":"vertical""#), "{json}");
        assert!(
            json.contains(r#""characters":[{"index":0,"rotation":15.0}]"#),
            "{json}"
        );
        assert_eq!(serde_json::from_str::<TextSpec>(&json).expect("back"), new);
    }

    #[test]
    fn embroidery_fonts_are_set_in_bidi_order_without_shaping() {
        use crate::fonts::{EmbroideryFont, EmbroideryGlyph};
        let square = Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(100.0, 0.0),
                v2(100.0, -100.0),
                v2(0.0, -100.0),
            ],
            true,
        );
        let mut e = EmbroideryFont::new(100.0);
        for ch in ['A', '\u{05D0}', '\u{05D1}'] {
            e.set_glyph(EmbroideryGlyph {
                ch,
                advance: 100.0,
                contours: vec![square.clone()],
                stitches: None,
            });
        }
        let fonts = [DesignFont {
            id: "squares".into(),
            family: "Squares".into(),
            style: String::new(),
            source: None,
            data: FontData::Embroidery(e),
        }];
        let l = layout_in(
            &TextSpec::new("A \u{05D0}\u{05D1}", "squares", 10.0),
            None,
            &fonts,
        )
        .expect("layout");
        let c = centres(&l);
        assert_eq!(
            c.iter().map(|g| g.0).collect::<String>(),
            "A\u{05D1}\u{05D0}"
        );
        assert!(c[0].1 < c[1].1 && c[1].1 < c[2].1, "{c:?}");
        assert_eq!(l.shaping, vec!["Hebrew (right-to-left)"]);
        // Vertical: stacked in logical order.
        let mut spec = TextSpec::new("A\u{05D0}", "squares", 10.0);
        spec.direction = TextDirection::Vertical;
        let v = layout_in(&spec, None, &fonts).expect("layout");
        let b: Vec<Rect> = v.outlined().map(|g| bounds(&g.contours)).collect();
        assert!(b[1].min_y >= b[0].max_y - 1e-6, "{b:?}");
        assert!(b[0].center().x.abs() < 1e-6);
    }

    #[test]
    fn script_font_fills_each_glyph_on_its_own() {
        let (obj, layout) = DesignObject::text(
            TextSpec::new("Hello", "pacifico", 15.0),
            Some(RegionSewing::new(RegionType::Fill, Thread::rgb(0))),
            None,
        )
        .expect("text");
        assert_eq!(layout.glyphs.len(), 5);
        assert_eq!(obj.glyph_groups().len(), 5);
        let closed: Vec<Contour> = obj.glyph_groups()[1].to_vec();
        assert!(
            !generate::fill(
                &closed,
                &generate::RegionParams {
                    angle: 0.0,
                    row_spacing: 4.0,
                    stitch_length: 40.0,
                    pull_compensation: 0.0,
                },
                &[]
            )
            .is_empty()
        );
        let mut d = Design::default();
        d.add(obj);
        assert!(Compiler::default().compile(&d).count(Command::Stitch) > 100);
    }
}
