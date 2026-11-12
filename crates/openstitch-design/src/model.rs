use crate::fonts::DesignFont;
use crate::geometry::{Contour, Rect, Transform, Vec2, v2};
use crate::machine::MachineSettings;
use crate::patterns::{self, Pattern, PatternKind};
use crate::sewing::SewingStitch;
use crate::text::{self, Layout, TextError, TextSpec};
use openstitch_core::{Command, Stitch, StitchPattern, Thread};
use serde::{Deserialize, Serialize};

/// Editable embroidery document, distinct from the generated [`StitchPattern`]: it keeps
/// outlines and sewing attributes so stitches can be regenerated after edits.
/// Coordinates are 0.1 mm, +y down, origin at the Design Page's top-left corner.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Design {
    pub name: String,
    pub page: DesignPage,
    pub objects: Vec<DesignObject>,
    /// Serialized as `nextObjectID` (the `.osd` field name).
    #[serde(rename = "nextObjectID")]
    pub next_object_id: u64,
    /// Design Settings: machine type, hoop, sewing area, trimming. Omitted while default.
    #[serde(default, skip_serializing_if = "MachineSettings::is_default")]
    pub machine: MachineSettings,
    /// Reference image shown behind the design (never sewn). Omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub background: Option<BackgroundImage>,
    /// Fonts the design carries (user TrueType fonts), referred to by text objects. Omitted
    /// while empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub fonts: Vec<DesignFont>,
    /// User stitch patterns (Programmable Stitch Creator) the design uses or carries, so the
    /// project sews the same anywhere. Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patterns: Vec<Pattern>,
    /// Sewing-machine stitch patterns made in the Stitch Design Factory, kept with the
    /// project (never sewn by the embroidery compiler). Omitted when empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sewing: Vec<SewingStitch>,
    /// Guidelines (view aid, never sewn). Omitted while empty and unlocked.
    #[serde(default, skip_serializing_if = "Guides::is_default")]
    pub guides: Guides,
    /// Design Property: category, author, keywords, comments. Omitted while empty.
    #[serde(default, skip_serializing_if = "DesignProperties::is_empty")]
    pub properties: DesignProperties,
    /// How Optimize Sewing Order (`order.optimize`) orders the objects. Omitted while it is
    /// PE-DESIGN's.
    #[serde(default, skip_serializing_if = "is_default")]
    pub optimize_order: OptimizeOrder,
    /// Production aids beyond PE-DESIGN (basting, stops at jumps, nearest start; #509, #512,
    /// #547). Omitted while all off.
    #[serde(
        default,
        skip_serializing_if = "crate::production::Production::is_default"
    )]
    pub production: crate::production::Production,
    /// The job the design is sewn for (customer, order, due date, notes), printed on the job
    /// sheets (#513, beyond PE-DESIGN). Saved in the project only, never written to machine
    /// files. Omitted when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub job: Option<JobInfo>,
    /// The machine file the design was opened from, written back unchanged when the design is
    /// exported to it again without a change (File ▸ Open of a PE-DESIGN PES, #375). Omitted
    /// when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<crate::source::SourceFile>,
}

/// How Optimize Sewing Order arranges the objects.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum OptimizeOrder {
    /// As PE-DESIGN 11.50 does it (reference `optimize-entry-exit`, `machine-rbr`,
    /// `machine-rbgb`): same colours together in order of first use, drawing order within a
    /// colour, overlapping objects keep their order; entry and exit points link same-colour
    /// neighbours only.
    #[default]
    PeDesign,
    /// Beyond PE-DESIGN: the same colour blocks, but within each the objects are arranged for
    /// the shortest total travel between outlines (greedy nearest, then 2-opt), and entry and
    /// exit points link every pair of neighbours.
    MinJump,
}

impl OptimizeOrder {
    pub const ALL: [OptimizeOrder; 2] = [OptimizeOrder::PeDesign, OptimizeOrder::MinJump];

    /// The JSON / command name.
    pub fn id(self) -> &'static str {
        match self {
            OptimizeOrder::PeDesign => "peDesign",
            OptimizeOrder::MinJump => "minJump",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            OptimizeOrder::PeDesign => "PE-DESIGN order",
            OptimizeOrder::MinJump => "Shortest jumps",
        }
    }
}

/// A raster image placed on the Design Page as a tracing reference (PE-DESIGN's background
/// image). The file is referenced, not embedded; units are 0.1 mm like the page.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackgroundImage {
    /// Absolute path of the image file.
    pub path: String,
    /// Pixel size of the image when it was placed.
    pub pixel_width: u32,
    pub pixel_height: u32,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    /// 0 (invisible) to 1 (opaque).
    pub opacity: f64,
    pub visible: bool,
    /// Rotation, crop and colour changes applied to the file's pixels (Image ▸ Rotate Right /
    /// Left, Modify Image); the file itself is never changed. Omitted while unchanged.
    #[serde(default, skip_serializing_if = "is_default")]
    pub adjust: ImageAdjust,
    /// Where the image sits in the drawing order (#518, beyond PE-DESIGN): behind the design
    /// (PE-DESIGN's place, the default), above an object, or on top. It never sews.
    #[serde(default, skip_serializing_if = "is_default")]
    pub layer: BackgroundLayer,
}

/// The background image's place among the objects when drawn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "position")]
pub enum BackgroundLayer {
    /// Behind every object, as PE-DESIGN draws it.
    #[default]
    Bottom,
    /// Above everything, shown with its opacity.
    Top,
    /// Above the stitches of object `after`, below the objects sewn after it.
    AfterObject { after: u64 },
}

/// How the background image is shown, applied in this order: turned, cropped, then colours.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ImageAdjust {
    /// Quarter turns clockwise, 0–3.
    pub rotation: u8,
    /// Cut off the turned image: left, top, right, bottom, each a fraction of its side.
    pub crop: [f64; 4],
    /// −100 to 100; 0 = as in the file.
    pub brightness: f64,
    pub contrast: f64,
    pub saturation: f64,
}

impl ImageAdjust {
    /// Largest crop on one side, and the smallest part of the image a crop keeps.
    pub const MAX_CROP: f64 = 0.9;
    pub const MIN_KEPT: f64 = 0.05;
}

/// Guidelines dragged out of the rulers (View ▸ Guideline), saved with the design; 0.1 mm
/// from the Design Page origin. Omitted while there are none.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Guides {
    /// x of the vertical guidelines (dragged from the top ruler).
    pub vertical: Vec<f64>,
    /// y of the horizontal guidelines (dragged from the left ruler).
    pub horizontal: Vec<f64>,
    /// View ▸ Guides ▸ Lock Guides (#799): they stay shown and snapped to, but are not moved,
    /// edited or deleted one by one. Omitted while unlocked.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
}

impl Guides {
    /// At most this many guidelines in each direction.
    pub const MAX: usize = 100;

    /// No guidelines.
    pub fn is_empty(&self) -> bool {
        self.vertical.is_empty() && self.horizontal.is_empty()
    }

    /// No guidelines and unlocked: nothing to save.
    pub fn is_default(&self) -> bool {
        self.is_empty() && !self.locked
    }
}

/// New Guide Layout… (#799): columns and rows with gutters, inside margins, on the Design
/// Page. Lengths in 0.1 mm.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GuideLayout {
    pub columns: u32,
    pub rows: u32,
    pub column_gutter: f64,
    pub row_gutter: f64,
    pub margin_top: f64,
    pub margin_left: f64,
    pub margin_bottom: f64,
    pub margin_right: f64,
}

impl GuideLayout {
    /// The guidelines on a `width` × `height` page: (x of the vertical, y of the horizontal).
    /// Each margin gives a line; each column (row) its two edges, shared where there is no
    /// gutter. Err when the margins or gutters leave no room.
    pub fn lines(&self, width: f64, height: f64) -> Result<(Vec<f64>, Vec<f64>), &'static str> {
        let axis = |n: u32, gutter: f64, a: f64, b: f64, size: f64| {
            let (start, end) = (a, size - b);
            let mut v = Vec::new();
            if n == 0 && a <= 0.0 && b <= 0.0 {
                return Ok(v);
            }
            if end <= start {
                return Err("the margins leave no room");
            }
            if a > 0.0 {
                v.push(start);
            }
            if n > 0 {
                let cell = (end - start - gutter * f64::from(n - 1)) / f64::from(n);
                if cell <= 0.0 {
                    return Err("the gutters leave no room");
                }
                for i in 0..n {
                    let l = start + f64::from(i) * (cell + gutter);
                    v.push(l);
                    v.push(l + cell);
                }
            }
            if b > 0.0 {
                v.push(end);
            }
            v.dedup_by(|x, y| (*x - *y).abs() < 0.5);
            Ok(v)
        };
        Ok((
            axis(
                self.columns,
                self.column_gutter,
                self.margin_left,
                self.margin_right,
                width,
            )?,
            axis(
                self.rows,
                self.row_gutter,
                self.margin_top,
                self.margin_bottom,
                height,
            )?,
        ))
    }
}

/// The job a design is sewn for (#513): what the client approval sheet and the operator sheet
/// print. Kept in the project (`.osd`) only; no machine format carries it. Each field is
/// omitted while empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct JobInfo {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub customer: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub order: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub due: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub notes: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub fabric: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub stabiliser: String,
}

impl JobInfo {
    /// Field names, as `design.setJob` parameters.
    pub const KEYS: [&str; 6] = ["customer", "order", "due", "notes", "fabric", "stabiliser"];

    pub fn is_empty(&self) -> bool {
        *self == JobInfo::default()
    }

    /// The field named `key` (one of [`JobInfo::KEYS`]).
    pub fn field_mut(&mut self, key: &str) -> Option<&mut String> {
        Some(match key {
            "customer" => &mut self.customer,
            "order" => &mut self.order,
            "due" => &mut self.due,
            "notes" => &mut self.notes,
            "fabric" => &mut self.fabric,
            "stabiliser" => &mut self.stabiliser,
            _ => return None,
        })
    }
}

/// Design Property text fields, written to the PES v4+ header next to the design name and
/// read back from it. Each field is omitted while empty.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DesignProperties {
    #[serde(skip_serializing_if = "String::is_empty")]
    pub category: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub author: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub keywords: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub comments: String,
}

impl DesignProperties {
    /// Field names, as `StitchPattern` metadata keys and command parameters.
    pub const KEYS: [&str; 4] = ["category", "author", "keywords", "comments"];
    /// A PES header string holds at most this many bytes.
    pub const MAX_BYTES: usize = 255;

    pub fn is_empty(&self) -> bool {
        self.fields().iter().all(|(_, v)| v.is_empty())
    }

    /// `(key, value)` in [`DesignProperties::KEYS`] order.
    pub fn fields(&self) -> [(&'static str, &str); 4] {
        [
            ("category", &self.category),
            ("author", &self.author),
            ("keywords", &self.keywords),
            ("comments", &self.comments),
        ]
    }

    pub fn field_mut(&mut self, key: &str) -> Option<&mut String> {
        match key {
            "category" => Some(&mut self.category),
            "author" => Some(&mut self.author),
            "keywords" => Some(&mut self.keywords),
            "comments" => Some(&mut self.comments),
            _ => None,
        }
    }

    /// The fields found in a stitch pattern's metadata (as a PES file supplies them).
    pub fn from_metadata(metadata: &std::collections::BTreeMap<String, String>) -> Self {
        let mut p = Self::default();
        for key in Self::KEYS {
            if let (Some(v), Some(f)) = (metadata.get(key), p.field_mut(key)) {
                f.clone_from(v);
            }
        }
        p
    }
}

impl Default for Design {
    fn default() -> Self {
        Self::new("Untitled", DesignPage::default())
    }
}

impl Design {
    pub fn new(name: impl Into<String>, page: DesignPage) -> Self {
        Design {
            name: name.into(),
            page,
            objects: Vec::new(),
            next_object_id: 1,
            machine: MachineSettings::default(),
            background: None,
            fonts: Vec::new(),
            patterns: Vec::new(),
            sewing: Vec::new(),
            guides: Guides::default(),
            properties: DesignProperties::default(),
            optimize_order: OptimizeOrder::default(),
            production: Default::default(),
            job: None,
            source: None,
        }
    }

    /// Adds `object` with a fresh id and returns that id.
    pub fn add(&mut self, mut object: DesignObject) -> u64 {
        object.id = self.next_object_id;
        self.next_object_id += 1;
        let id = object.id;
        self.objects.push(object);
        id
    }

    /// User pattern `id` of `kind` carried by this design.
    pub fn user_pattern(&self, kind: PatternKind, id: &str) -> Option<&Pattern> {
        self.patterns.iter().find(|p| p.kind == kind && p.id == id)
    }

    /// Pattern `id` of `kind` that this design can sew: its own, or a bundled one.
    pub fn find_pattern(&self, kind: PatternKind, id: &str) -> Option<&Pattern> {
        self.user_pattern(kind, id)
            .or_else(|| patterns::find(kind, id))
    }

    /// Ids of objects whose sewing uses pattern `id` of `kind`.
    pub fn pattern_users(&self, kind: PatternKind, id: &str) -> Vec<u64> {
        self.objects
            .iter()
            .filter(|o| {
                let region = o.region.as_ref().is_some_and(|r| {
                    r.pattern.as_deref() == Some(id)
                        && match kind {
                            PatternKind::Fill => r.kind == RegionType::ProgrammableFill,
                            PatternKind::Motif => r.kind == RegionType::MotifFill,
                            PatternKind::Decorative => r.kind == RegionType::DecorativeFill,
                            PatternKind::Meander => r.kind == RegionType::Meander,
                        }
                });
                let line = o.line.as_ref().is_some_and(|l| {
                    kind == PatternKind::Motif
                        && l.kind == LineType::Motif
                        && l.motif.as_deref() == Some(id)
                });
                region || line
            })
            .map(|o| o.id)
            .collect()
    }

    pub fn index_of(&self, id: u64) -> Option<usize> {
        self.objects.iter().position(|o| o.id == id)
    }

    /// A fresh group id (shares the object id counter, so it never collides with either).
    pub fn new_group_id(&mut self) -> u64 {
        let id = self.next_object_id;
        self.next_object_id += 1;
        id
    }

    /// `ids` plus every other member of their groups, in sewing order, without duplicates.
    /// Unknown ids are dropped.
    pub fn with_groups(&self, ids: &[u64]) -> Vec<u64> {
        let groups: Vec<u64> = ids
            .iter()
            .filter_map(|&id| self.objects[self.index_of(id)?].group)
            .collect();
        self.objects
            .iter()
            .filter(|o| ids.contains(&o.id) || o.group.is_some_and(|g| groups.contains(&g)))
            .map(|o| o.id)
            .collect()
    }

    /// Clears groups left with fewer than two members (after deletes or shape operations).
    pub fn dissolve_single_groups(&mut self) {
        let mut counts = std::collections::HashMap::new();
        for g in self.objects.iter().filter_map(|o| o.group) {
            *counts.entry(g).or_insert(0usize) += 1;
        }
        for o in &mut self.objects {
            if o.group.is_some_and(|g| counts[&g] < 2) {
                o.group = None;
            }
        }
    }

    /// The contour text on a path follows: the first contour of shape object `id`, from the
    /// shape's start point (its line's entry, else its region's), as in PE-DESIGN, where the
    /// outline's start is where left-aligned fitted text begins.
    pub fn path_contour(&self, id: u64) -> Option<Contour> {
        let o = &self.objects[self.index_of(id)?];
        let c = (o.kind == ObjectKind::Shape).then(|| o.contours.first())??;
        let start = o
            .line
            .as_ref()
            .and_then(|l| l.entry)
            .or_else(|| o.region.as_ref().and_then(|r| r.entry));
        Some(match start {
            Some(p) if c.closed => c.starting_at(p),
            _ => c.clone(),
        })
    }

    /// Lays the text of object `index` out again from its [`TextSpec`], following its path
    /// object if it has one. A text whose path object no longer exists is detached first.
    pub fn layout_text(&mut self, index: usize) -> Result<Layout, TextError> {
        let Some(mut spec) = self.objects[index].text.clone() else {
            return Ok(Layout::default());
        };
        if spec.path.is_some_and(|id| self.path_contour(id).is_none()) {
            self.detach_text(index);
            return self.layout_text(index);
        }
        let path = spec.path.and_then(|id| self.path_contour(id));
        let layout = text::layout_in(&spec, path.as_ref(), &self.fonts)?;
        spec.glyphs = layout.glyph_counts();
        let o = &mut self.objects[index];
        o.contours = layout.contours();
        o.stitch_block = layout.stitch_block();
        o.text = Some(spec);
        Ok(layout)
    }

    /// How far each outlined letter of text object `index` is turned (degrees, see
    /// [`text::Glyph::turn`]), from its layout: `None` for a warped letter, and an empty list
    /// when the object is not text or its contours are not its layout's (edited, or laid out
    /// with a font no longer at hand).
    pub fn text_turns(&self, index: usize) -> Vec<Option<f64>> {
        let Some(o) = self.objects.get(index) else {
            return Vec::new();
        };
        let Some(spec) = o.text.as_ref().filter(|_| o.kind == ObjectKind::Text) else {
            return Vec::new();
        };
        let path = spec.path.and_then(|id| self.path_contour(id));
        let Ok(layout) = text::layout_in(spec, path.as_ref(), &self.fonts) else {
            return Vec::new();
        };
        let laid = layout.contours();
        let same = laid.len() == o.contours.len()
            && layout.glyph_counts() == spec.glyphs
            && laid.iter().zip(&o.contours).all(|(a, b)| {
                a.points.len() == b.points.len()
                    && a.points
                        .first()
                        .zip(b.points.first())
                        .is_none_or(|(p, q)| p.distance(*q) < 1.0)
            });
        if !same {
            return Vec::new();
        }
        layout.outlined().map(text::Glyph::turn).collect()
    }

    /// For each outlined letter of text object `index` after the first, what lies between it
    /// and the letter before: a space (a new word), a line break or nothing. Empty when the
    /// object's outlines no longer match its layout (as [`Self::text_turns`]).
    pub fn text_breaks(&self, index: usize) -> Vec<crate::finish::LetterBreak> {
        use crate::finish::LetterBreak;
        let Some(o) = self.objects.get(index) else {
            return Vec::new();
        };
        let Some(spec) = o.text.as_ref().filter(|_| o.kind == ObjectKind::Text) else {
            return Vec::new();
        };
        let path = spec.path.and_then(|id| self.path_contour(id));
        let Ok(layout) = text::layout_in(spec, path.as_ref(), &self.fonts) else {
            return Vec::new();
        };
        if layout.glyph_counts() != spec.glyphs {
            return Vec::new();
        }
        let chars: Vec<char> = spec.text.chars().collect();
        let mut out = Vec::new();
        let mut before: Option<usize> = None;
        for g in layout.outlined() {
            if let Some(b) = before {
                let (lo, hi) = (b.min(g.index), b.max(g.index));
                let gap = chars.get(lo + 1..hi).unwrap_or(&[]);
                out.push(if gap.contains(&'\n') {
                    LetterBreak::Line
                } else if gap.iter().any(|c| c.is_whitespace()) {
                    LetterBreak::Word
                } else {
                    LetterBreak::Letter
                });
            }
            before = Some(g.index);
        }
        out
    }

    /// Takes the text of object `index` off its path; it continues as straight text whose
    /// baseline starts (or, centred / right-aligned, centres or ends) where it stood.
    pub fn detach_text(&mut self, index: usize) {
        let o = &mut self.objects[index];
        let bounds = Rect::enclosing(o.contours.iter().flat_map(Contour::outline_points));
        let Some(spec) = &mut o.text else { return };
        spec.path = None;
        if let Some(b) = bounds {
            let x = match spec.align {
                text::TextAlign::Left => b.min_x,
                text::TextAlign::Center => b.center().x,
                text::TextAlign::Right => b.max_x,
            };
            spec.transform = Transform::translation(x, b.max_y);
        }
    }

    /// Re-lays out every text attached to a path, so it follows edits to the path. Texts whose
    /// font is unknown keep their contours.
    pub fn relayout_text(&mut self) {
        for i in 0..self.objects.len() {
            let o = &self.objects[i];
            if o.kind == ObjectKind::Text && o.text.as_ref().is_some_and(|t| t.path.is_some()) {
                let _ = self.layout_text(i);
            }
        }
    }
}

/// Design Page (PE-DESIGN term): the hoop-sized working area, 0.1 mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct DesignPage {
    pub width: f64,
    pub height: f64,
}

impl Default for DesignPage {
    fn default() -> Self {
        DesignPage {
            width: 1000.0,
            height: 1000.0,
        }
    }
}

impl DesignPage {
    pub fn center(&self) -> Vec2 {
        v2(self.width / 2.0, self.height / 2.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ObjectKind {
    /// Outline-based: stitches are generated from contours and sewing attributes.
    Shape,
    /// Fixed needle points (imported embroidery data); only transforms apply.
    Stitches,
    /// Lettering: contours are laid out from [`DesignObject::text`] and sewn like a shape,
    /// glyph by glyph.
    Text,
}

/// One editable pattern (PE-DESIGN's shape, text and stitch patterns).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DesignObject {
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub kind: ObjectKind,
    #[serde(default)]
    pub contours: Vec<Contour>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stitch_block: Option<StitchBlock>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub region: Option<RegionSewing>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub line: Option<LineSewing>,
    #[serde(default = "yes")]
    pub visible: bool,
    /// Hole sewing: the region is sewn around its paired inner shape ([`Self::hole_inner`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hole_sewing: bool,
    /// Hole sewing: the inner object of the pair, whose shape this region is sewn around
    /// (`None` in designs saved before pairs were stored; see [`crate::holes::paired_inner`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hole_inner: Option<u64>,
    /// Text objects only: what the contours were laid out from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<TextSpec>,
    /// Group this object belongs to (PE-DESIGN grouping): selected, moved and transformed with
    /// the other members. Group ids come from the same counter as object ids.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<u64>,
    /// Jump-stitch trimming for this object on multi-needle machines (`None` = on, PE-DESIGN's
    /// default). Off: travel into and within the object is never trimmed; a colour change
    /// still cuts the thread.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub trim_jumps: Option<bool>,
    /// The first letter of a monogram keeps what the monogram was made from, so
    /// `monogram.edit` can rebuild its group.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub monogram: Option<Box<crate::monogram::MonogramSpec>>,
    /// Locked in the editor (#502): the canvas does not pick, move, scale or point-edit it;
    /// it stays selectable from the Sewing Order list. Commands are not affected.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub locked: bool,
    /// Sew the outline before the region (#502, beyond PE-DESIGN; `false` = PE-DESIGN's
    /// region-then-outline order).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub line_first: bool,
    /// How the object finishes and connects (#548, beyond PE-DESIGN): trim after it, its
    /// ties, and a text's trims between letters. `None` = PE-DESIGN's rules.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub finish: Option<crate::finish::Finish>,
    /// The machine stops after this object (#512, beyond PE-DESIGN): to place appliqué fabric
    /// or a patch without a colour change. A colour change right after it is the stop.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub stop_after: bool,
    /// A member of a branch (#546, beyond PE-DESIGN): the members of its group are sewn in
    /// one go, each entered where the one before it touches it; re-linked after every edit.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub branch: bool,
    /// Leave out this region's stitching hidden under later fills and satins, keeping this
    /// overlap (mm) under their edges (#511, beyond PE-DESIGN). The shape is not cut.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hide_under: Option<f64>,
    /// The move to the next object of the same thread is sewn as a running-stitch connector
    /// instead of a jump and trim (#545, beyond PE-DESIGN).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub connector_run: bool,
    /// A knockdown fill made by `wizard.knockdown` (#510, beyond PE-DESIGN): the parameters it
    /// was made with, so it can be rebuilt and kept before what it covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub knockdown: Option<serde_json::Value>,
    /// Needle points edited by hand on this live object (#501, beyond PE-DESIGN), sewn instead
    /// of the generated ones while they still apply ([`crate::hand_edits`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand_edits: Option<Box<crate::hand_edits::HandEdits>>,
}

fn yes() -> bool {
    true
}

fn is_true(v: &bool) -> bool {
    *v
}

impl DesignObject {
    pub fn shape(
        contours: Vec<Contour>,
        region: Option<RegionSewing>,
        line: Option<LineSewing>,
    ) -> Self {
        DesignObject {
            id: 0,
            name: None,
            kind: ObjectKind::Shape,
            contours,
            stitch_block: None,
            region,
            line,
            visible: true,
            hole_sewing: false,
            hole_inner: None,
            text: None,
            group: None,
            trim_jumps: None,
            monogram: None,
            locked: false,
            line_first: false,
            finish: None,
            stop_after: false,
            branch: false,
            hide_under: None,
            connector_run: false,
            knockdown: None,
            hand_edits: None,
        }
    }

    pub fn stitches(block: StitchBlock) -> Self {
        DesignObject {
            id: 0,
            name: None,
            kind: ObjectKind::Stitches,
            contours: Vec::new(),
            stitch_block: Some(block),
            region: None,
            line: None,
            visible: true,
            hole_sewing: false,
            hole_inner: None,
            text: None,
            group: None,
            trim_jumps: None,
            monogram: None,
            locked: false,
            line_first: false,
            finish: None,
            stop_after: false,
            branch: false,
            hide_under: None,
            connector_run: false,
            knockdown: None,
            hand_edits: None,
        }
    }

    /// A text object laid out from `spec` (straight, `spec.path` ignored: use
    /// [`Design::relayout_text`] for text on a path).
    pub fn text(
        spec: TextSpec,
        region: Option<RegionSewing>,
        line: Option<LineSewing>,
    ) -> Result<(Self, Layout), TextError> {
        let layout = text::layout(&spec, None)?;
        let mut object = DesignObject::shape(layout.contours(), region, line);
        object.kind = ObjectKind::Text;
        object.stitch_block = layout.stitch_block();
        object.text = Some(TextSpec {
            glyphs: layout.glyph_counts(),
            ..spec
        });
        Ok((object, layout))
    }

    /// Outline-based (shape or text): sewn from contours and sewing attributes.
    pub fn is_outline(&self) -> bool {
        matches!(self.kind, ObjectKind::Shape | ObjectKind::Text)
    }

    /// The contours of each glyph of a text object (one group holding every contour for a
    /// shape, or when the stored glyph counts do not match the contours).
    pub fn glyph_groups(&self) -> Vec<&[Contour]> {
        let counts = self
            .text
            .as_ref()
            .map(|t| t.glyphs.as_slice())
            .unwrap_or(&[]);
        let total: u64 = counts.iter().map(|&n| u64::from(n)).sum();
        if counts.is_empty() || total != self.contours.len() as u64 {
            return vec![&self.contours];
        }
        let mut rest = self.contours.as_slice();
        counts
            .iter()
            .map(|&n| {
                let (head, tail) = rest.split_at(n as usize);
                rest = tail;
                head
            })
            .collect()
    }

    pub fn has_closed_contour(&self) -> bool {
        self.contours.iter().any(|c| c.closed)
    }

    /// Every geometric point, for bounds and transforms.
    pub fn points(&self) -> Vec<Vec2> {
        match self.kind {
            ObjectKind::Shape => self
                .contours
                .iter()
                .flat_map(|c| c.outline_points())
                .collect(),
            // Text: outlines plus the needle points of stitch glyphs (user-mapped text).
            ObjectKind::Text => self
                .contours
                .iter()
                .flat_map(|c| c.outline_points())
                .chain(
                    self.stitch_block
                        .iter()
                        .flat_map(|b| b.stitches.iter().map(|s| v2(s.x, s.y))),
                )
                .collect(),
            ObjectKind::Stitches => self
                .stitch_block
                .as_ref()
                .map(|b| b.stitches.iter().map(|s| v2(s.x, s.y)).collect())
                .unwrap_or_default(),
        }
    }

    pub fn bounds(&self) -> Option<Rect> {
        Rect::enclosing(self.points())
    }

    pub fn apply(&mut self, t: &Transform) {
        for c in &mut self.contours {
            *c = c.transformed(t);
        }
        if let Some(spec) = &mut self.text {
            spec.transform = spec.transform.then(t);
        }
        if let Some(m) = &mut self.monogram {
            m.center = t.apply(m.center);
        }
        if let Some(r) = &mut self.region {
            r.entry = r.entry.map(|p| t.apply(p));
            r.exit = r.exit.map(|p| t.apply(p));
            r.center = r.center.map(|p| t.apply(p));
            r.reference = r.reference.map(|p| t.apply(p));
            r.direction_curve = r.direction_curve.as_ref().map(|c| c.transformed(t));
            for st in &mut r.stamps {
                *st = st.transformed(t);
            }
            for l in &mut r.direction_lines {
                *l = [t.apply(l[0]), t.apply(l[1])];
            }
            // Stitch angle lines and a gradient's axis move with the shape (#859): the compiler
            // sews every object moved to its bounds' corner, and moving a shape moves them.
            for l in &mut r.angle_lines {
                *l = [t.apply(l[0]), t.apply(l[1])];
            }
            if let Some(g) = &mut r.gradient {
                g.from = t.apply(g.from);
                g.to = t.apply(g.to);
            }
            r.skeleton = r.skeleton.as_ref().map(|s| s.transformed(t));
        }
        if let Some(l) = &mut self.line {
            l.entry = l.entry.map(|p| t.apply(p));
            l.exit = l.exit.map(|p| t.apply(p));
            l.target = l.target.map(|p| t.apply(p));
        }
        if let Some(block) = &mut self.stitch_block {
            for s in &mut block.stitches {
                let p = t.apply(v2(s.x, s.y));
                s.x = p.x;
                s.y = p.y;
            }
        }
    }
}

/// Imported, fixed stitches with one thread per colour block.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StitchBlock {
    pub stitches: Vec<Stitch>,
    pub threads: Vec<Thread>,
}

impl From<&StitchPattern> for StitchBlock {
    fn from(p: &StitchPattern) -> Self {
        StitchBlock {
            stitches: p
                .stitches
                .iter()
                .filter(|s| s.command != Command::End)
                .copied()
                .collect(),
            threads: p.threads.clone(),
        }
    }
}

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RegionType {
    Fill,
    Satin,
    ProgrammableFill,
    MotifFill,
    Piping,
    CrossStitch,
    ConcentricCircle,
    Radial,
    Spiral,
    FlexibleSpiral,
    Stippling,
    NetFill,
    ZigzagNetFill,
    DecorativeFill,
    /// Beyond PE-DESIGN: rows that follow the outline at a constant distance (rings, single or
    /// double spiral, echo lines; see [`crate::contour`]).
    Contour,
    /// Beyond PE-DESIGN (#533): one closed path that never crosses itself, long straight
    /// corridors with square turns, its passes a spacing apart (see `regions::stipple`).
    Maze,
    /// Beyond PE-DESIGN (#533): a meander tile (`PatternKind::Meander`) chained into rows,
    /// clipped to the region and joined along the outline.
    Meander,
}

impl RegionType {
    /// Accepted density, rows (lines) per mm. Fill and satin take PE-DESIGN 11.50's range,
    /// 0.1–14.0: its Density field clamps there (MANIFEST entries `sweep-fill-density-min`,
    /// `sweep-satin-density-min`: typing 0 shows 0,1, typing 99 shows 14,0, and both files sew
    /// at 0.1 lines/mm). Issue #139 asked for a 0.2 floor; the references show 0.1 (the 0.15 →
    /// 0.2 note there is the field's one-decimal rounding). The other types keep 0.5–10.
    pub fn density_range(self) -> (f64, f64) {
        match self {
            RegionType::Fill | RegionType::Satin => (0.1, 14.0),
            _ => (0.5, 10.0),
        }
    }

    pub const ALL: [RegionType; 17] = [
        RegionType::Fill,
        RegionType::Satin,
        RegionType::ProgrammableFill,
        RegionType::MotifFill,
        RegionType::Piping,
        RegionType::CrossStitch,
        RegionType::ConcentricCircle,
        RegionType::Radial,
        RegionType::Spiral,
        RegionType::FlexibleSpiral,
        RegionType::Stippling,
        RegionType::NetFill,
        RegionType::ZigzagNetFill,
        RegionType::DecorativeFill,
        RegionType::Contour,
        RegionType::Maze,
        RegionType::Meander,
    ];

    /// The JSON / command name (`"programmableFill"`).
    pub fn id(self) -> &'static str {
        match self {
            RegionType::Fill => "fill",
            RegionType::Satin => "satin",
            RegionType::ProgrammableFill => "programmableFill",
            RegionType::MotifFill => "motifFill",
            RegionType::Piping => "piping",
            RegionType::CrossStitch => "crossStitch",
            RegionType::ConcentricCircle => "concentricCircle",
            RegionType::Radial => "radial",
            RegionType::Spiral => "spiral",
            RegionType::FlexibleSpiral => "flexibleSpiral",
            RegionType::Stippling => "stippling",
            RegionType::NetFill => "netFill",
            RegionType::ZigzagNetFill => "zigzagNetFill",
            RegionType::DecorativeFill => "decorativeFill",
            RegionType::Contour => "contour",
            RegionType::Maze => "maze",
            RegionType::Meander => "meander",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// Display name (PE-DESIGN's term).
    pub fn label(self) -> &'static str {
        match self {
            RegionType::Fill => "Fill",
            RegionType::Satin => "Satin",
            RegionType::ProgrammableFill => "Programmable fill",
            RegionType::MotifFill => "Motif fill",
            RegionType::Piping => "Piping",
            RegionType::CrossStitch => "Cross stitch",
            RegionType::ConcentricCircle => "Concentric circle",
            RegionType::Radial => "Radial",
            RegionType::Spiral => "Spiral",
            RegionType::FlexibleSpiral => "Flexible spiral",
            RegionType::Stippling => "Stippling",
            RegionType::NetFill => "Net fill",
            RegionType::ZigzagNetFill => "Zigzag net fill",
            RegionType::DecorativeFill => "Decorative fill",
            RegionType::Contour => "Contour fill",
            RegionType::Maze => "Maze fill",
            RegionType::Meander => "Meander fill",
        }
    }
}

/// One colour of a tartan (plaid) fill (#539, beyond PE-DESIGN): the sett, shared by the
/// tartan's objects, and the colour this object sews.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Tartan {
    pub sett: Vec<TartanStripe>,
    /// Mirrored about its first and last stripes (the usual sett).
    #[serde(default)]
    pub symmetric: bool,
    /// The colour index (into the tartan's colours) this object sews.
    pub layer: usize,
}

/// One stripe of a tartan sett: a colour index and a width in mm.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TartanStripe {
    pub color: usize,
    pub width: f64,
}

/// One colour of a colour blend or gradient fill (#532, beyond PE-DESIGN): the gradient's
/// objects share the shape and its fill settings and differ in `layer` and thread.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradient {
    #[serde(default)]
    pub kind: GradientKind,
    /// Where the gradient starts (position 0) and ends (position 1), 0.1 mm; for a radial one
    /// the centre and a point on the outer circle.
    pub from: Vec2,
    pub to: Vec2,
    /// Positions of the colour stops, 0…1, ascending: one per object of the gradient.
    pub stops: Vec<f64>,
    /// The stop this object sews.
    pub layer: usize,
    #[serde(default)]
    pub dither: GradientDither,
    #[serde(default)]
    pub curve: GradientCurve,
    /// Seed of the noise dither.
    #[serde(default)]
    pub seed: u64,
    /// The layers of this gradient sewn after this object, set by the compiler from the
    /// sewing order and never saved: travel may hide under their rows (#901).
    #[serde(skip)]
    pub later: Vec<usize>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientKind {
    #[default]
    Linear,
    Radial,
}

/// How the stitches between two stops are shared out (see `crate::gradient`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientDither {
    /// Whole rows: complementary row spacing, the fewest extra stitches.
    #[default]
    Rows,
    /// A 4 × 4 ordered (Bayer) pattern of stitches.
    Ordered,
    /// Seeded random stitches.
    Noise,
    /// Hard edges halfway between the stops.
    Bands,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradientCurve {
    #[default]
    Linear,
    EaseIn,
    EaseOut,
    SCurve,
}

impl GradientDither {
    pub const ALL: [GradientDither; 4] = [
        GradientDither::Rows,
        GradientDither::Ordered,
        GradientDither::Noise,
        GradientDither::Bands,
    ];
    pub fn id(self) -> &'static str {
        match self {
            GradientDither::Rows => "rows",
            GradientDither::Ordered => "ordered",
            GradientDither::Noise => "noise",
            GradientDither::Bands => "bands",
        }
    }
}

impl GradientCurve {
    pub const ALL: [GradientCurve; 4] = [
        GradientCurve::Linear,
        GradientCurve::EaseIn,
        GradientCurve::EaseOut,
        GradientCurve::SCurve,
    ];
    pub fn id(self) -> &'static str {
        match self {
            GradientCurve::Linear => "linear",
            GradientCurve::EaseIn => "easeIn",
            GradientCurve::EaseOut => "easeOut",
            GradientCurve::SCurve => "sCurve",
        }
    }
}

impl GradientKind {
    pub fn id(self) -> &'static str {
        match self {
            GradientKind::Linear => "linear",
            GradientKind::Radial => "radial",
        }
    }
}

/// Contour fill (beyond PE-DESIGN): how the rings that follow the outline are sewn.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContourStrategy {
    /// Every ring a closed loop of its own.
    #[default]
    Rings,
    /// One continuous spiral per branch.
    SingleSpiral,
    /// Inwards and back out, ending next to the start.
    DoubleSpiral,
}

impl ContourStrategy {
    pub const ALL: [ContourStrategy; 3] = [
        ContourStrategy::Rings,
        ContourStrategy::SingleSpiral,
        ContourStrategy::DoubleSpiral,
    ];

    /// The JSON / command name.
    pub fn id(self) -> &'static str {
        match self {
            ContourStrategy::Rings => "rings",
            ContourStrategy::SingleSpiral => "singleSpiral",
            ContourStrategy::DoubleSpiral => "doubleSpiral",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            ContourStrategy::Rings => "Rings",
            ContourStrategy::SingleSpiral => "Single spiral",
            ContourStrategy::DoubleSpiral => "Double spiral",
        }
    }
}

/// Contour fill (beyond PE-DESIGN): the corners of the offset rings.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ContourJoin {
    #[default]
    Round,
    Miter,
}

/// Underlay (under sewing) of fill and satin regions, when `underlay` is on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnderlayStyle {
    /// Running stitch along the outline.
    #[default]
    Edge,
    /// Sparse rows across the region at the sewing direction, inset from the edge.
    Zigzag,
    /// Edge run, then zigzag rows.
    EdgeZigzag,
    /// Two crossing layers of sparse rows (±45° to the sewing direction).
    Double,
    /// PE-DESIGN's fill under sewing: a running stitch 1 mm inside the outline, then tatami
    /// rows across the sewing direction, 1 mm inside it (see [`crate::region`]).
    EdgeFill,
    /// PE-DESIGN's second under sewing type (fill and satin): the running stitch 1 mm inside
    /// the outline, then two layers of lines 1 mm inside it at the sewing direction minus and
    /// plus 45°, each sewn in two halves from the region's far corners towards the start point
    /// (see [`crate::region`]).
    Cross,
    /// OpenStitch's automatic under sewing (#523, beyond PE-DESIGN): on a Manual Punch column
    /// and a zigzag line, a centre walk, the inset edge run, or the edge run and the zigzag,
    /// chosen from the average width each time it sews (see [`crate::column::AutoUnderlay`]);
    /// on other regions the edge run and rows, as `edgeZigzag`.
    Auto,
    /// A running stitch down the middle and back (#523, beyond PE-DESIGN): for thin columns
    /// and zigzag lines; a column's piping alone.
    CentreWalk,
}

/// How a fill's needle grid moves from row to row (see [`crate::generate::Stagger`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StaggerKind {
    /// PE-DESIGN's Frequency: a fixed fraction of a stitch per row (regular brick).
    #[default]
    Regular,
    /// OpenStitch's own: a seeded pseudo-random fraction per row, reproducible from
    /// `stagger_seed`. PE-DESIGN has no such option.
    Random,
    /// OpenStitch's stepped texture (#537): `stagger_steps` offsets, each held for
    /// `stagger_repeat` rows.
    Steps,
    /// OpenStitch's curved textures (#537): the shift follows a wave, a triangle or an arc of
    /// `stagger_amplitude` over `stagger_period` rows.
    Wave,
    Triangle,
    Arc,
}

impl StaggerKind {
    pub const ALL: [StaggerKind; 6] = [
        StaggerKind::Regular,
        StaggerKind::Random,
        StaggerKind::Steps,
        StaggerKind::Wave,
        StaggerKind::Triangle,
        StaggerKind::Arc,
    ];

    /// The name the engine's `stagger` option takes.
    pub fn id(self) -> &'static str {
        match self {
            StaggerKind::Regular => "regular",
            StaggerKind::Random => "random",
            StaggerKind::Steps => "steps",
            StaggerKind::Wave => "wave",
            StaggerKind::Triangle => "triangle",
            StaggerKind::Arc => "arc",
        }
    }
}

/// A named tatami texture (#537, beyond PE-DESIGN): a preset of the fill's stagger settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FillTexture {
    /// The name the engine's `texture` option takes.
    pub id: &'static str,
    /// What the inspector shows.
    pub label: &'static str,
    pub stagger: StaggerKind,
    /// Steps: offsets and rows each; curves: amplitude (% of a stitch) and period (rows).
    pub steps: u32,
    pub repeat: u32,
    pub amplitude: f64,
    pub period: u32,
}

const fn texture(
    id: &'static str,
    label: &'static str,
    stagger: StaggerKind,
    steps: u32,
    repeat: u32,
    amplitude: f64,
    period: u32,
) -> FillTexture {
    FillTexture {
        id,
        label,
        stagger,
        steps,
        repeat,
        amplitude,
        period,
    }
}

/// OpenStitch's named tatami textures. `smooth` is PE-DESIGN's own Frequency brick.
pub const FILL_TEXTURES: [FillTexture; 14] = [
    texture(
        "smooth",
        "Smooth (Frequency)",
        StaggerKind::Regular,
        3,
        1,
        50.0,
        8,
    ),
    texture("scattered", "Scattered", StaggerKind::Random, 3, 1, 50.0, 8),
    texture("ribs", "Ribs", StaggerKind::Steps, 1, 1, 50.0, 8),
    texture("brick", "Brick", StaggerKind::Steps, 2, 1, 50.0, 8),
    texture("diagonal", "Diagonal", StaggerKind::Steps, 4, 1, 50.0, 8),
    texture(
        "steepDiagonal",
        "Steep diagonal",
        StaggerKind::Steps,
        8,
        1,
        50.0,
        8,
    ),
    texture("steps", "Steps", StaggerKind::Steps, 3, 3, 50.0, 8),
    texture("stairs", "Stairs", StaggerKind::Steps, 4, 2, 50.0, 8),
    texture("blocks", "Blocks", StaggerKind::Steps, 2, 4, 50.0, 8),
    texture("waves", "Waves", StaggerKind::Wave, 3, 1, 50.0, 12),
    texture("ripples", "Ripples", StaggerKind::Wave, 3, 1, 30.0, 6),
    texture("zigzag", "Zigzag", StaggerKind::Triangle, 3, 1, 50.0, 6),
    texture("chevron", "Chevron", StaggerKind::Triangle, 3, 1, 80.0, 16),
    texture("scallops", "Scallops", StaggerKind::Arc, 3, 1, 50.0, 8),
];

/// Cross stitch Times, as PE-DESIGN 11.50 offers it (Single | Double | Triple, #614): how many
/// times every cross is sewn. PE-DESIGN covers each diagonal 2, 4 or 6 times (the round8
/// references 614-cross-single/-double/-triple: 410, 666 and 922 stitches on a 20 mm square,
/// 256 more per step for its 64 cells).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CrossTimes {
    #[default]
    Single,
    Double,
    Triple,
}

impl CrossTimes {
    pub const ALL: [CrossTimes; 3] = [CrossTimes::Single, CrossTimes::Double, CrossTimes::Triple];

    /// The name in commands and files.
    pub fn id(self) -> &'static str {
        match self {
            CrossTimes::Single => "single",
            CrossTimes::Double => "double",
            CrossTimes::Triple => "triple",
        }
    }

    /// 1, 2 or 3.
    pub fn count(self) -> usize {
        match self {
            CrossTimes::Single => 1,
            CrossTimes::Double => 2,
            CrossTimes::Triple => 3,
        }
    }

    /// From 1, 2 or 3 (anything above 3 is Triple, below 2 Single).
    pub fn from_count(n: usize) -> Self {
        match n {
            0 | 1 => CrossTimes::Single,
            2 => CrossTimes::Double,
            _ => CrossTimes::Triple,
        }
    }
}

/// The stitch drawn in every cell of a cross stitch region. PE-DESIGN 11.50 sews only the full
/// cross (X); the others are OpenStitch's own (beyond PE-DESIGN, #614).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CrossKind {
    /// An X: both diagonals (PE-DESIGN's cross stitch).
    #[default]
    Full,
    /// Half cross: the "/" diagonal only.
    Half,
    /// Upright cross: a "+" through the cell's middle.
    Upright,
    /// Smyrna (double) cross: an X with a "+" over it.
    Smyrna,
}

impl CrossKind {
    pub const ALL: [CrossKind; 4] = [
        CrossKind::Full,
        CrossKind::Half,
        CrossKind::Upright,
        CrossKind::Smyrna,
    ];

    /// The name in commands and files.
    pub fn id(self) -> &'static str {
        match self {
            CrossKind::Full => "full",
            CrossKind::Half => "half",
            CrossKind::Upright => "upright",
            CrossKind::Smyrna => "smyrna",
        }
    }
}

impl UnderlayStyle {
    pub const ALL: [UnderlayStyle; 8] = [
        UnderlayStyle::Edge,
        UnderlayStyle::Zigzag,
        UnderlayStyle::EdgeZigzag,
        UnderlayStyle::Double,
        UnderlayStyle::EdgeFill,
        UnderlayStyle::Cross,
        UnderlayStyle::Auto,
        UnderlayStyle::CentreWalk,
    ];

    /// The style's name in commands and files.
    pub fn id(self) -> &'static str {
        match self {
            UnderlayStyle::Edge => "edge",
            UnderlayStyle::Zigzag => "zigzag",
            UnderlayStyle::EdgeZigzag => "edgeZigzag",
            UnderlayStyle::Double => "double",
            UnderlayStyle::EdgeFill => "edgeFill",
            UnderlayStyle::Cross => "cross",
            UnderlayStyle::Auto => "auto",
            UnderlayStyle::CentreWalk => "centreWalk",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.id() == id)
    }

    /// The components this style sews on a Manual Punch column (several direction lines), as
    /// PE-DESIGN 11.50's five exclusive under sewing buttons do on its own column (round-4
    /// captures satin-col-us-type-1..5): its first-row buttons (type 1, the default, and
    /// type 2) sew the edge run, the piping and the zigzag; the second row's outline button
    /// (type 3) and lines button (type 5) the edge run alone; its zigzag button (type 4) the
    /// piping and the zigzag. `edge` stands for types 3 and 5, `zigzag` for type 4, every
    /// other style for types 1 and 2.
    pub fn column_parts(self) -> UnderlayParts {
        match self {
            UnderlayStyle::Edge => UnderlayParts {
                edge: true,
                ..UnderlayParts::default()
            },
            UnderlayStyle::Zigzag => UnderlayParts {
                zigzag: true,
                center: true,
                ..UnderlayParts::default()
            },
            UnderlayStyle::CentreWalk => UnderlayParts {
                center: true,
                ..UnderlayParts::default()
            },
            _ => UnderlayParts {
                edge: true,
                zigzag: true,
                center: true,
                lattice: false,
            },
        }
    }

    /// The components this style stands for (see [`UnderlayParts`]).
    pub fn parts(self) -> UnderlayParts {
        let p = |edge, zigzag, lattice| UnderlayParts {
            edge,
            zigzag,
            lattice,
            center: false,
        };
        match self {
            UnderlayStyle::Edge => p(true, false, false),
            UnderlayStyle::Zigzag => p(false, true, false),
            UnderlayStyle::EdgeZigzag | UnderlayStyle::EdgeFill | UnderlayStyle::Auto => {
                p(true, true, false)
            }
            UnderlayStyle::CentreWalk => UnderlayParts {
                center: true,
                ..UnderlayParts::default()
            },
            UnderlayStyle::Double => p(false, false, true),
            UnderlayStyle::Cross => p(true, false, true),
        }
    }
}

/// The components of a region's under sewing, each on or off, combinable (OpenStitch's own
/// extension: PE-DESIGN's under sewing types are exclusive radio buttons, and `UnderlayStyle`
/// is that exclusive choice). PE-DESIGN 11.50's types are combinations of them: its first type for fill and satin regions is
/// the edge run with rows across the sewing direction (`edge` + `zigzag`), its second type the
/// edge run with the two crossing layers of lines at ±45° (`edge` + `lattice`); a Manual Punch
/// column offers "Edge only", "Region only (single layer of zigzag shaped stitches)",
/// "Region only (single layer of piping stitches)", "Edge and region (single layer of zigzag
/// shaped stitches)" and "Region only (double layer of zigzag shaped and piping stitches)"
/// (the guide's names; piping = `center`, the running stitch down the middle of the column).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UnderlayParts {
    /// Running stitch along the outline, inset.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub edge: bool,
    /// Sparse rows across the sewing direction (fill, satin regions), or the zigzag between the
    /// inset rails of a column.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub zigzag: bool,
    /// The lattice: two crossing layers of lines at the sewing direction ∓ 45°.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub lattice: bool,
    /// Piping: a running stitch along the middle of the column (or the region's middle row).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub center: bool,
}

impl UnderlayParts {
    /// The part names: `edge`, `zigzag`, `lattice`, `center`.
    pub const NAMES: [&'static str; 4] = ["edge", "zigzag", "lattice", "center"];

    /// The parts from their names; `None` on an unknown name.
    pub fn from_names<'a>(names: impl IntoIterator<Item = &'a str>) -> Option<Self> {
        let mut parts = UnderlayParts::default();
        for n in names {
            match n {
                "edge" => parts.edge = true,
                "zigzag" => parts.zigzag = true,
                "lattice" => parts.lattice = true,
                "center" => parts.center = true,
                _ => return None,
            }
        }
        Some(parts)
    }

    /// The names of the parts that are on, in [`Self::NAMES`] order.
    pub fn names(self) -> Vec<&'static str> {
        [self.edge, self.zigzag, self.lattice, self.center]
            .into_iter()
            .zip(Self::NAMES)
            .filter_map(|(on, name)| on.then_some(name))
            .collect()
    }

    pub fn any(self) -> bool {
        self.edge || self.zigzag || self.lattice || self.center
    }
}

/// PE-DESIGN's under sewing Density: how far apart the rows, lines and zigzag of the under
/// sewing lie (measured in its 11.50 output, sweep-fill-us-density-*: Dense 1.4 mm, Medium
/// 2.0 mm, Light 3.5 mm nominal; the rows lie on both inset edges, the gaps stretched or
/// squeezed to the nearest whole count).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum UnderlayDensity {
    Dense,
    #[default]
    Medium,
    Light,
}

impl UnderlayDensity {
    /// Nominal spacing, mm.
    pub fn spacing_mm(self) -> f64 {
        match self {
            UnderlayDensity::Dense => 1.4,
            UnderlayDensity::Medium => 2.0,
            UnderlayDensity::Light => 3.5,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            UnderlayDensity::Dense => "dense",
            UnderlayDensity::Medium => "medium",
            UnderlayDensity::Light => "light",
        }
    }
}

/// PE-DESIGN's fill Stitch Type, its two buttons in the Fill pane: One, the default, sews flat
/// rows (halves from the start point, brick row ends); Two sews sloped rows, each pass rising one
/// row spacing from edge to edge so the passes join in one continuous zigzag (see
/// [`crate::generate::fill_edge_to_edge`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FillStitchType {
    #[default]
    One,
    Two,
}

/// Which row ends a feathered edge roughens.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeatherSide {
    #[default]
    Start,
    End,
    Both,
}

/// Which way a feathered edge moves its row ends (#685, beyond PE-DESIGN): `In` shortens the
/// rows as PE-DESIGN does; `Out` lengthens them past the edge (fur and grass); `Both` moves
/// each row end in or out, up to half the width either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum FeatherMode {
    #[default]
    In,
    Out,
    Both,
}

/// How fill and satin move between separate parts of one region (PE-DESIGN Running Stitch
/// Path). Every value travels with running stitches; none jumps.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum RunningStitchPath {
    /// The shortest path inside the region.
    #[default]
    Inside,
    /// Along the outline, just inside it.
    AlongOutline,
    /// On the outline.
    OnOutline,
}

impl RunningStitchPath {
    pub fn id(self) -> &'static str {
        match self {
            RunningStitchPath::Inside => "inside",
            RunningStitchPath::AlongOutline => "alongOutline",
            RunningStitchPath::OnOutline => "onOutline",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        [
            RunningStitchPath::Inside,
            RunningStitchPath::AlongOutline,
            RunningStitchPath::OnOutline,
        ]
        .into_iter()
        .find(|p| p.id() == id)
    }
}

/// Reads `runningStitchPath`, or the old `travel` flag (true: inside, false: jumps).
fn running_stitch_path<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<RunningStitchPath>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Stored {
        Travel(bool),
        Path(RunningStitchPath),
    }
    Ok(match Option::<Stored>::deserialize(d)? {
        None | Some(Stored::Travel(false)) => None,
        Some(Stored::Travel(true)) => Some(RunningStitchPath::Inside),
        Some(Stored::Path(p)) => Some(p),
    })
}

/// Gradation pattern (PE-DESIGN's Gradation Pattern Setting): `steps` values, evenly spaced
/// across the rows from `max` % of the region's density at the start side to `min` % at the
/// far side, with the density interpolated between them. PE-DESIGN 11.50's default pattern
/// sews that way: its rows' density falls linearly from 53 % to 5 % across the region
/// (region-travel-u-gradation reference), not in equal bands. With evenly spaced step values
/// the steps do not change the density; they are kept for the pattern's own values. Without
/// `steps` the gradation is how designs saved before patterns (a single far-side percentage,
/// stored as a number) sew, and still read: from 100 % down to that percentage.
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Gradation {
    /// Density at the start side, % of the region's density (5–100).
    pub max: f64,
    /// Density at the far side, % (5–100).
    pub min: f64,
    /// Number of step values (2–20); `None`: a gradation saved before patterns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<u32>,
    /// How the density goes from `max` to `min` across the region (#532, beyond PE-DESIGN):
    /// PE-DESIGN's straight line by default.
    #[serde(default, skip_serializing_if = "is_default")]
    pub shape: GradationShape,
}

/// A named density profile of [`Gradation`] (#532, beyond PE-DESIGN): `g(t)` from 1 (`max`)
/// to 0 (`min`) at position `t` across the region (0 the start side, 1 the far side).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GradationShape {
    /// PE-DESIGN's: straight from `max` to `min`, `1 − t`.
    #[default]
    Linear,
    /// Opens quickly, then slowly: `(1 − t)²`.
    Convex,
    /// Stays dense, then opens: `1 − t²`.
    Concave,
    /// Dense in the middle, open at both sides: `1 − |2t − 1|`.
    Centre,
    /// Dense at both sides, open in the middle: `|2t − 1|`.
    Edges,
    /// Two dense bands: `½ + ½ cos 2πt`.
    Wave,
}

impl GradationShape {
    pub const ALL: [GradationShape; 6] = [
        GradationShape::Linear,
        GradationShape::Convex,
        GradationShape::Concave,
        GradationShape::Centre,
        GradationShape::Edges,
        GradationShape::Wave,
    ];

    pub fn id(self) -> &'static str {
        match self {
            GradationShape::Linear => "linear",
            GradationShape::Convex => "convex",
            GradationShape::Concave => "concave",
            GradationShape::Centre => "centre",
            GradationShape::Edges => "edges",
            GradationShape::Wave => "wave",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            GradationShape::Linear => "Linear (PE-DESIGN)",
            GradationShape::Convex => "Convex (+)",
            GradationShape::Concave => "Concave (+)",
            GradationShape::Centre => "Centre dense (+)",
            GradationShape::Edges => "Centre open (+)",
            GradationShape::Wave => "Wave (+)",
        }
    }

    /// `g(t)`, 1 at `max` and 0 at `min`.
    pub fn at(self, t: f64) -> f64 {
        let t = t.clamp(0.0, 1.0);
        match self {
            GradationShape::Linear => 1.0 - t,
            GradationShape::Convex => (1.0 - t) * (1.0 - t),
            GradationShape::Concave => 1.0 - t * t,
            GradationShape::Centre => 1.0 - (2.0 * t - 1.0).abs(),
            GradationShape::Edges => (2.0 * t - 1.0).abs(),
            GradationShape::Wave => 0.5 + 0.5 * crate::fmath::Det::dcos(std::f64::consts::TAU * t),
        }
    }
}

impl Gradation {
    /// PE-DESIGN 11's default pattern (the first one): Max 53 %, Min 5 %, nine steps
    /// (53/47/41/35/29/23/17/11/5 %), as its reference file sews it (fill-analysis).
    pub const PE_DEFAULT: Gradation = Gradation {
        max: 53.0,
        min: 5.0,
        steps: Some(9),
        shape: GradationShape::Linear,
    };

    /// The gradation of designs saved before patterns: 100 % falling continuously to `far` %.
    pub fn continuous(far: f64) -> Self {
        Gradation {
            max: 100.0,
            min: far,
            steps: None,
            shape: GradationShape::Linear,
        }
    }

    /// Density at `f` (0 = start side, 1 = far side) as a fraction of the region's density:
    /// linear between the evenly spaced step values, so linear from `max` to `min`; with
    /// another [`GradationShape`], `min + (max − min)·g(f)`.
    pub fn fraction(&self, f: f64) -> f64 {
        let t = f.clamp(0.0, 1.0);
        let v = match self.shape {
            GradationShape::Linear => self.max + (self.min - self.max) * t,
            s => self.min + (self.max - self.min) * s.at(t),
        };
        (v / 100.0).clamp(0.05, 1.0)
    }
}

impl<'de> Deserialize<'de> for Gradation {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Pattern {
            max: f64,
            min: f64,
            #[serde(default)]
            steps: Option<u32>,
            #[serde(default)]
            shape: GradationShape,
        }
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Stored {
            Far(f64),
            Pattern(Pattern),
        }
        Ok(match Stored::deserialize(d)? {
            Stored::Far(g) => Gradation::continuous(g),
            Stored::Pattern(p) => Gradation {
                max: p.max,
                min: p.min,
                steps: p.steps,
                shape: p.shape,
            },
        })
    }
}

/// Feathered edge: row ends shortened by a deterministic pseudo-random amount up to `width`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Feather {
    pub side: FeatherSide,
    /// mm.
    pub width: f64,
    /// In (PE-DESIGN's), out or both ways (#685); saved only when not in.
    #[serde(default, skip_serializing_if = "is_feather_in")]
    pub mode: FeatherMode,
    /// A seed for a pseudo-random sequence of moves instead of PE-DESIGN's fixed 12-row
    /// pattern (#685); none keeps PE-DESIGN's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u64>,
}

fn is_feather_in(m: &FeatherMode) -> bool {
    *m == FeatherMode::In
}

impl Default for Feather {
    /// PE-DESIGN 11.50's feathered edge: the start side, 3.0 mm, its own pattern.
    fn default() -> Self {
        Feather {
            side: FeatherSide::Start,
            width: 3.0,
            mode: FeatherMode::In,
            seed: None,
        }
    }
}

impl Feather {
    /// Whether it sews as PE-DESIGN's feathered edge (in, PE-DESIGN's pattern).
    pub fn is_pe_design(&self) -> bool {
        self.mode == FeatherMode::In && self.seed.is_none()
    }
}

/// Region sewing of a closed shape. mm and degrees; defaults are placeholders until calibrated
/// against PE-DESIGN reference output. Every attribute after `underlay` is optional and omitted
/// from JSON when unset, so designs that do not use it serialize as before.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RegionSewing {
    #[serde(rename = "type")]
    pub kind: RegionType,
    pub thread: Thread,
    /// Sewing direction, degrees counter-clockwise on screen as in PE-DESIGN (0 = horizontal
    /// rows, 45 = rows rising to the right). `.osd` version 1 stored it clockwise.
    pub angle: f64,
    /// Rows per mm.
    pub density: f64,
    /// Longest fill stitch, mm.
    pub stitch_length: f64,
    /// Row end extension, mm.
    pub pull_compensation: f64,
    /// Underlay (style in `underlay_style`, or its components in `underlay_parts`).
    pub underlay: bool,
    #[serde(default, skip_serializing_if = "is_default")]
    pub underlay_style: UnderlayStyle,
    /// The under sewing's components when set explicitly (see [`UnderlayParts`]); otherwise
    /// `underlay_style` says which (see [`RegionSewing::underlay_parts`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underlay_parts: Option<UnderlayParts>,
    /// PE-DESIGN's under sewing Density (Dense / Medium / Light), the nominal spacing of the
    /// under sewing's rows unless `underlay_spacing` sets it.
    #[serde(default, skip_serializing_if = "is_default")]
    pub underlay_density: UnderlayDensity,
    /// Distance between underlay rows, mm (default: the Density's spacing, Medium 2 mm).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underlay_spacing: Option<f64>,
    /// Satin and fill: choose the direction from the region's shape (stitches across its
    /// narrow side) instead of `angle`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub auto_direction: bool,
    /// Gradation: the density changes across the region, from `max` % of `density` at the
    /// start side to `min` % at the far side (see [`Gradation`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradation: Option<Gradation>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feather: Option<Feather>,
    /// Satin: rows longer than this (mm) are split into staggered stitches.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub split_length: Option<f64>,
    /// Auto satin: PE-DESIGN 11.50's Half Stitch. Where a sharp bend crowds the needle points
    /// on its inner side, every other one is pulled back along its crossing (see
    /// `region::half_stitch`). On by default, as PE-DESIGN's drawing tools set it; saved only
    /// when off.
    #[serde(default = "yes", skip_serializing_if = "is_true")]
    pub half_stitch: bool,
    /// Move between separated parts with running stitches (Running Stitch Path); `None` jumps,
    /// as fill and satin did before (designs saved with `travel` off or unset).
    #[serde(
        default,
        alias = "travel",
        deserialize_with = "running_stitch_path",
        skip_serializing_if = "Option::is_none"
    )]
    pub running_stitch_path: Option<RunningStitchPath>,
    /// Pattern id: programmable fill, motif fill, net, zigzag net and decorative fill.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
    /// Pattern size, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_width: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern_height: Option<f64>,
    /// Spacing, mm: cross size, stippling, net and flexible spiral spacing, motif gaps.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<f64>,
    /// Running stitch length of line-based fills, mm (default 2).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run_pitch: Option<f64>,
    /// Fill, piping and programmable-fill base: needle shift per row, % of a stitch (0-50).
    /// New fills take PE-DESIGN 11's 30; unset (older files) keeps the ⅓ stagger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frequency: Option<f64>,
    /// Fill: how the needle grid moves from row to row, the regular Frequency brick (PE-DESIGN)
    /// or OpenStitch's seeded random stagger.
    #[serde(default, skip_serializing_if = "is_default")]
    pub stagger: StaggerKind,
    /// Seed of the random stagger (default [`RegionSewing::DEFAULT_STAGGER_SEED`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger_seed: Option<u64>,
    /// Steps texture: how many offsets (default 3) and rows held at each (default 1).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger_steps: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger_repeat: Option<u32>,
    /// Curve textures: the shift's height, % of a stitch (default 50), and its period in rows
    /// (default 8).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger_amplitude: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stagger_period: Option<u32>,
    /// Fill: PE-DESIGN's Stitch Type (flat or sloped rows).
    #[serde(default, skip_serializing_if = "is_default")]
    pub stitch_type: FillStitchType,
    /// Programmable / motif fill: shift of every other tile row (column), % of the tile.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub row_offset: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column_offset: Option<f64>,
    /// Concentric circle, spiral and flexible spiral start at the centre instead of outside.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub start_inside: bool,
    /// Spirals turn counter-clockwise.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub counterclockwise: bool,
    /// Cross stitch: the old "triple" switch (each diagonal sewn three times whole), which is
    /// Times Double's sewing; read from older files only (see [`RegionSewing::cross_times`]).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub triple: bool,
    /// Cross stitch Times (PE-DESIGN's Single, Double, Triple).
    #[serde(default, skip_serializing_if = "is_default")]
    pub times: CrossTimes,
    /// Cross stitch kind: full (PE-DESIGN), half, upright or Smyrna (beyond PE-DESIGN).
    #[serde(default, skip_serializing_if = "is_default")]
    pub cross_kind: CrossKind,
    /// Decorative fill: random shift of needle points, % of the run pitch.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub random_shift: Option<f64>,
    /// Programmable fill: sew a plain fill underneath (base sewing).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub base_sewing: bool,
    /// Contour fill: rings, single or double spiral.
    #[serde(default, skip_serializing_if = "is_default")]
    pub contour_strategy: ContourStrategy,
    /// Contour fill: corners of the offset rings.
    #[serde(default, skip_serializing_if = "is_default")]
    pub contour_join: ContourJoin,
    /// Contour fill: rings outside the shape (0–30).
    #[serde(default, skip_serializing_if = "is_default")]
    pub echo_lines: u32,
    /// Maze fill (#533): how often a corridor runs straight on where it can, % (default
    /// [`RegionSewing::DEFAULT_MAZE_STRAIGHTNESS`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub straightness: Option<f64>,
    /// Maze fill: the seed of its layout (default [`RegionSewing::DEFAULT_MAZE_SEED`]); another
    /// seed gives another maze with the same spacing and look.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maze_seed: Option<u64>,
    /// Region entry point (PE-DESIGN R1), 0.1 mm; snapped to the outline when sewn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Vec2>,
    /// Region exit point (PE-DESIGN R2), 0.1 mm; snapped to the outline when sewn.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<Vec2>,
    /// Centre point of concentric circle, radial and spiral stitches, 0.1 mm (default: the
    /// centre of the region's bounding box).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub center: Option<Vec2>,
    /// Reference point that anchors the pattern grid of programmable fill, motif fill, cross
    /// stitch, net fill, zigzag net fill and decorative fill, 0.1 mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reference: Option<Vec2>,
    /// Curved sewing direction ("round fill"): rows follow this open curve, shifted across
    /// the region. It must not turn back along its own chord.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub direction_curve: Option<Contour>,
    /// Embossing, engraving and stamps (fill, satin, programmable fill), applied in order.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub stamps: Vec<crate::stamp::Stamp>,
    /// Several sewing-direction lines (Manual Punch): consecutive lines are the rungs of a
    /// column sewn along them (satin and fill). Each is [left end, right end].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direction_lines: Vec<[Vec2; 2]>,
    /// Curved Block (Manual Punch): the column's sides are smooth curves through the ends of
    /// [`RegionSewing::direction_lines`] instead of straight lines between them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub curved_lines: bool,
    /// Satin along a centre line with a width at each point: the stitch direction follows
    /// the line (variable-direction satin). Takes precedence over the other direction options.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skeleton: Option<crate::skeleton::Skeleton>,
    /// Satin corner styles, short stitches and compensation per side (#520-#522, beyond
    /// PE-DESIGN): Manual Punch satin columns. Off by default.
    #[serde(default, skip_serializing_if = "is_default")]
    pub satin_shaping: crate::column::SatinShaping,
    /// Thresholds of `UnderlayStyle::Auto` (#523).
    #[serde(default, skip_serializing_if = "is_default")]
    pub underlay_auto: crate::column::AutoUnderlay,
    /// A second under sewing layer on a Manual Punch column (#523, beyond PE-DESIGN).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub underlay2: Option<crate::column::SecondUnderlay>,
    /// Satin: the hand-stitch look on one side (#540, beyond PE-DESIGN).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<crate::line_looks::SatinHand>,
    /// Fill: rows that ripple as a sine across the row direction (#538, beyond PE-DESIGN): the
    /// wave's height, crest to trough, mm. Unset or 0: straight rows (PE-DESIGN's fill).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wave_height: Option<f64>,
    /// Fill: the wave's length, crest to crest, mm (default [`RegionSewing::DEFAULT_WAVE_LENGTH`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub wave_length: Option<f64>,
    /// Fill: one colour of a tartan (#539, beyond PE-DESIGN); the object sews the warp and weft
    /// rows of its colour's stripes (see `crate::tartan`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tartan: Option<Tartan>,
    /// Fill: push compensation (#677, beyond PE-DESIGN), mm (0-2): the top stitching's sides
    /// parallel to its rows move in by this much; the row ends keep `pull_compensation`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub push_compensation: Option<f64>,
    /// Fill: stitch angle lines (#534, beyond PE-DESIGN), 0.1 mm: the rows turn smoothly from
    /// one line's direction to the next (see `crate::angle_lines`). Fewer than two: none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub angle_lines: Vec<[Vec2; 2]>,
    /// Fill: rows that follow this direction field (#538, beyond PE-DESIGN), as evenly spaced
    /// streamlines (see `crate::flow`). Unset: straight rows (PE-DESIGN's fill).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow_field: Option<crate::flow::FlowField>,
    /// Fill: one colour of a colour blend or gradient (#532, beyond PE-DESIGN); the object
    /// sews only its share of the rows the gradient's objects share (see `crate::gradient`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gradient: Option<Gradient>,
    /// Satin: a motif repeated along the column instead of the satin, stretched across its
    /// local width (#544, beyond PE-DESIGN).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motif_band: Option<crate::motif_band::MotifBand>,
}

impl RegionSewing {
    /// Maze fill: default straightness, %.
    pub const DEFAULT_MAZE_STRAIGHTNESS: f64 = 70.0;
    /// Maze fill: default seed.
    pub const DEFAULT_MAZE_SEED: u64 = 533;

    /// Maze fill: the chance (0–1) that a corridor runs straight on where it can.
    pub fn maze_straightness(&self) -> f64 {
        self.straightness
            .unwrap_or(Self::DEFAULT_MAZE_STRAIGHTNESS)
            .clamp(0.0, 100.0)
            / 100.0
    }

    /// Wave fill: default wave length, mm.
    pub const DEFAULT_WAVE_LENGTH: f64 = 16.0;
    /// Wave fill: the steepest the rows may slope, tan 40°. The rows are copies of one sine a
    /// set step apart along the wave's height; across a row that slopes θ they are the step
    /// × cos θ apart, so the step is chosen between the two (see [`RegionSewing::wave`]) and up
    /// to 40° that keeps the spacing across the rows within ±15 % of the set density. A higher
    /// wave is sewn at this slope.
    pub const MAX_WAVE_SLOPE: f64 = 0.839_099_631_177_280;

    /// Wave fill: `(amplitude, wave length)` in mm, the amplitude (half the height) capped at
    /// [`RegionSewing::MAX_WAVE_SLOPE`]; `None` for straight rows (not a fill, or no height).
    pub fn wave(&self) -> Option<(f64, f64)> {
        if self.kind != RegionType::Fill {
            return None;
        }
        let height = self.wave_height.filter(|h| *h > 0.0)?.min(20.0);
        let length = self
            .wave_length
            .unwrap_or(Self::DEFAULT_WAVE_LENGTH)
            .clamp(2.0, 100.0);
        let max = Self::MAX_WAVE_SLOPE * length / std::f64::consts::TAU;
        Some(((height / 2.0).min(max), length))
    }

    pub fn new(kind: RegionType, thread: Thread) -> Self {
        RegionSewing {
            kind,
            thread,
            angle: Self::default_angle(kind),
            // PE-DESIGN 11.50 Sewing Attributes (reference files, fill-analysis): fill and satin
            // 4.5 lines/mm with under sewing on.
            density: match kind {
                RegionType::Fill | RegionType::Satin => 4.5,
                RegionType::ProgrammableFill => 2.5,
                RegionType::Piping => 1.0,
                // PE-DESIGN 11.50's Concentric Circle and Spiral Stitch: 2.0 lines/mm.
                RegionType::ConcentricCircle | RegionType::Spiral => 2.0,
                // PE-DESIGN 11.50's zigzag net: a rung every 0.44 mm.
                RegionType::ZigzagNetFill => 2.25,
                _ => 1.5,
            },
            stitch_length: 4.0,
            pull_compensation: 0.0,
            underlay: matches!(
                kind,
                RegionType::Fill | RegionType::ProgrammableFill | RegionType::Satin
            ),
            underlay_style: match kind {
                RegionType::Fill | RegionType::ProgrammableFill => UnderlayStyle::EdgeFill,
                RegionType::Satin => UnderlayStyle::EdgeZigzag,
                _ => UnderlayStyle::Edge,
            },
            underlay_parts: None,
            underlay_density: UnderlayDensity::Medium,
            underlay_spacing: None,
            auto_direction: false,
            gradation: None,
            feather: None,
            split_length: None,
            half_stitch: true,
            running_stitch_path: Some(RunningStitchPath::Inside),
            pattern: None,
            pattern_width: None,
            pattern_height: None,
            spacing: None,
            run_pitch: None,
            // PE-DESIGN 11's Frequency default (its field shows 30 %).
            frequency: (kind == RegionType::Fill).then_some(30.0),
            stagger: StaggerKind::Regular,
            stitch_type: FillStitchType::One,
            stagger_seed: None,
            stagger_steps: None,
            stagger_repeat: None,
            stagger_amplitude: None,
            stagger_period: None,
            row_offset: None,
            column_offset: None,
            start_inside: false,
            counterclockwise: false,
            triple: false,
            times: CrossTimes::Single,
            cross_kind: CrossKind::Full,
            random_shift: None,
            base_sewing: false,
            contour_strategy: ContourStrategy::default(),
            contour_join: ContourJoin::default(),
            echo_lines: 0,
            straightness: None,
            maze_seed: None,
            entry: None,
            exit: None,
            center: None,
            reference: None,
            direction_curve: None,
            stamps: Vec::new(),
            direction_lines: Vec::new(),
            curved_lines: false,
            skeleton: None,
            satin_shaping: Default::default(),
            underlay_auto: Default::default(),
            underlay2: None,
            hand: None,
            wave_height: None,
            wave_length: None,
            tartan: None,
            push_compensation: None,
            angle_lines: Vec::new(),
            flow_field: None,
            gradient: None,
            motif_band: None,
        }
    }

    /// Whether Running Stitch Path can be chosen: fill and satin without Auto direction or a
    /// feathered edge (and not on text, which the caller knows).
    pub fn running_stitch_path_available(&self) -> bool {
        matches!(self.kind, RegionType::Fill | RegionType::Satin)
            && !self.auto_direction
            && self.sewn_feather().is_none()
    }

    /// Why Gradation cannot be set on this region, if it cannot (the guide: a manual straight
    /// direction, no feathered edge; text, which the caller knows, is excluded too). PE-DESIGN
    /// also offers it for programmable fill, which OpenStitch does not sew yet.
    pub fn gradation_unavailable(&self) -> Option<&'static str> {
        if !matches!(self.kind, RegionType::Fill | RegionType::Satin) {
            Some("is only for fill and satin")
        } else if self.auto_direction {
            Some("is not available with Auto direction")
        } else if self.feather.is_some() {
            Some("is not available with a feathered edge")
        } else {
            None
        }
    }

    /// Why Feathered edge cannot be set on this region, if it cannot (the guide: not with
    /// gradation or Auto direction; text and hole sewing, which the caller knows, are excluded
    /// too). PE-DESIGN also offers it for programmable fill, which OpenStitch does not sew yet.
    pub fn feather_unavailable(&self) -> Option<&'static str> {
        if !matches!(self.kind, RegionType::Fill | RegionType::Satin) {
            Some("is only for fill and satin")
        } else if self.auto_direction {
            Some("is not available with Auto direction")
        } else if self.gradation.is_some() {
            Some("is not available with gradation")
        } else {
            None
        }
    }

    /// The gradation actually sewn: none where it is unavailable (another sew type, Auto
    /// direction). A stored gradation stays, so switching back restores it.
    pub fn sewn_gradation(&self) -> Option<&Gradation> {
        self.gradation.as_ref().filter(|_| {
            matches!(self.kind, RegionType::Fill | RegionType::Satin) && !self.auto_direction
        })
    }

    /// The feathered edge actually sewn: none where it is unavailable (another sew type, Auto
    /// direction, or with gradation, which wins in files that hold both).
    pub fn sewn_feather(&self) -> Option<Feather> {
        self.feather.filter(|_| {
            matches!(self.kind, RegionType::Fill | RegionType::Satin)
                && !self.auto_direction
                && self.gradation.is_none()
        })
    }

    /// The travel actually sewn between parts, `None` for jumps. Where the attribute is not
    /// available the region travels inside; gradation rules out Inside (Along outline then).
    pub fn travel_path(&self) -> Option<RunningStitchPath> {
        let chosen = self.running_stitch_path?;
        if !matches!(self.kind, RegionType::Fill | RegionType::Satin) {
            return Some(RunningStitchPath::Inside);
        }
        let path = if self.running_stitch_path_available() {
            chosen
        } else {
            RunningStitchPath::Inside
        };
        Some(match path {
            RunningStitchPath::Inside if self.sewn_gradation().is_some() => {
                RunningStitchPath::AlongOutline
            }
            p => p,
        })
    }

    /// Fill: needle shift from one row to the next, fraction of a stitch (default ⅓).
    /// The sewing direction a new region of `kind` starts with, degrees: PE-DESIGN 11.50's
    /// shape tools start Fill Stitch at 45° (reference pes-objects r01: Direction 45 on a fresh
    /// design) and the guide gives programmable fill "45° (default)"; the other types sew at
    /// 0°. Saved designs keep the angle they store (every `.osd` region stores one).
    pub fn default_angle(kind: RegionType) -> f64 {
        match kind {
            RegionType::Fill | RegionType::ProgrammableFill => 45.0,
            _ => 0.0,
        }
    }

    pub fn fill_shift(&self) -> Option<f64> {
        self.frequency.map(|f| f.clamp(0.0, 50.0) / 100.0)
    }

    /// Seed of the random stagger when none is set.
    pub const DEFAULT_STAGGER_SEED: u64 = 0x5747_4747;

    /// The fill's row-to-row stagger: the random one when chosen (seeded), else the
    /// Frequency's regular brick, else the ⅓ stagger of files without a Frequency.
    pub fn fill_stagger(&self) -> crate::generate::Stagger {
        use crate::generate::Stagger;
        match self.stagger {
            StaggerKind::Random => {
                Stagger::Random(self.stagger_seed.unwrap_or(Self::DEFAULT_STAGGER_SEED))
            }
            StaggerKind::Regular => self.fill_shift().map_or(Stagger::Third, Stagger::Regular),
            StaggerKind::Steps => Stagger::Steps {
                n: self.stagger_steps.unwrap_or(3).clamp(1, 16),
                repeat: self.stagger_repeat.unwrap_or(1).clamp(1, 16),
            },
            StaggerKind::Wave | StaggerKind::Triangle | StaggerKind::Arc => Stagger::Curve {
                kind: match self.stagger {
                    StaggerKind::Wave => crate::generate::StaggerCurve::Wave,
                    StaggerKind::Triangle => crate::generate::StaggerCurve::Triangle,
                    _ => crate::generate::StaggerCurve::Arc,
                },
                amplitude: self.stagger_amplitude.unwrap_or(50.0).clamp(0.0, 100.0) / 100.0,
                period: self.stagger_period.unwrap_or(8).clamp(2, 64),
            },
        }
    }

    /// Sets the fill's stagger settings to a named texture (see [`FILL_TEXTURES`]).
    pub fn set_texture(&mut self, t: &FillTexture) {
        self.stagger = t.stagger;
        let (steps, curve) = match t.stagger {
            StaggerKind::Steps => (true, false),
            StaggerKind::Wave | StaggerKind::Triangle | StaggerKind::Arc => (false, true),
            _ => (false, false),
        };
        self.stagger_steps = steps.then_some(t.steps);
        self.stagger_repeat = steps.then_some(t.repeat);
        self.stagger_amplitude = curve.then_some(t.amplitude);
        self.stagger_period = curve.then_some(t.period);
    }

    /// The named texture the fill's stagger settings match, if any.
    pub fn texture(&self) -> Option<&'static FillTexture> {
        FILL_TEXTURES.iter().find(|t| {
            let mut r = self.clone();
            r.set_texture(t);
            r.stagger == self.stagger
                && r.stagger_steps == self.stagger_steps
                && r.stagger_repeat == self.stagger_repeat
                && r.stagger_amplitude == self.stagger_amplitude
                && r.stagger_period == self.stagger_period
        })
    }

    /// Spacing in mm, or the type's default.
    pub fn spacing_mm(&self) -> f64 {
        self.spacing.unwrap_or(match self.kind {
            RegionType::CrossStitch => 2.5,
            RegionType::Stippling => 2.0,
            RegionType::NetFill => 4.0,
            // PE-DESIGN 11.50's zigzag net (o-region-zigzag-net): bands 8 mm apart.
            RegionType::ZigzagNetFill => 8.0,
            RegionType::FlexibleSpiral => 1.0,
            // The contour fill sketch's default row spacing (echo-quilting look).
            RegionType::Contour => 2.0,
            RegionType::Maze => 2.0,
            _ => 1.0,
        })
    }

    pub fn run_pitch_mm(&self) -> f64 {
        self.run_pitch.unwrap_or(2.0)
    }

    /// Spacing of the under sewing's rows, lines and zigzag in mm: `underlay_spacing` when
    /// set, else the Density's nominal spacing.
    pub fn underlay_spacing_mm(&self) -> f64 {
        self.underlay_spacing
            .unwrap_or_else(|| self.underlay_density.spacing_mm())
    }

    /// Whether the under sewing is one of OpenStitch's own styles from before PE-DESIGN's
    /// types were measured (`edge`, `double` on any region; `zigzag`, `edgeZigzag` on fill
    /// kinds) with no explicit `underlay_parts`, on a region without direction lines: those
    /// keep their original geometry (the edge run on the outline itself, rows 0.5 mm in, the
    /// two crossing layers without halves), so designs saved with them sew as before.
    pub fn legacy_underlay(&self) -> bool {
        self.underlay_parts.is_none()
            && self.direction_lines.len() < 2
            && match self.underlay_style {
                UnderlayStyle::Edge | UnderlayStyle::Double => true,
                UnderlayStyle::Zigzag | UnderlayStyle::EdgeZigzag => self.kind != RegionType::Satin,
                UnderlayStyle::EdgeFill
                | UnderlayStyle::Cross
                | UnderlayStyle::Auto
                | UnderlayStyle::CentreWalk => false,
            }
    }

    /// Cross stitch Times: `times`, or Double for a region saved with the old `triple` switch
    /// (which sewed what Double sews).
    pub fn cross_times(&self) -> CrossTimes {
        if self.triple && self.times == CrossTimes::Single {
            CrossTimes::Double
        } else {
            self.times
        }
    }

    /// The under sewing's components: `underlay_parts` when set, else those of
    /// `underlay_style` (on a column with several direction lines, PE-DESIGN's column
    /// under sewing for that style: [`UnderlayStyle::column_parts`]).
    pub fn underlay_parts(&self) -> UnderlayParts {
        self.underlay_parts.unwrap_or_else(|| {
            if self.direction_lines.len() >= 2 {
                self.underlay_style.column_parts()
            } else {
                self.underlay_style.parts()
            }
        })
    }

    /// Pattern size in mm (width, height), defaults per type.
    pub fn pattern_size_mm(&self) -> (f64, f64) {
        let (w, h) = match self.kind {
            RegionType::MotifFill => (4.0, 3.0),
            RegionType::DecorativeFill => (10.0, 8.0),
            // PE-DESIGN 11.50's zigzag net band: 3.0 mm wide.
            RegionType::ZigzagNetFill => (3.0, 3.0),
            _ => (8.0, 8.0),
        };
        (
            self.pattern_width.unwrap_or(w),
            self.pattern_height.unwrap_or(h),
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum LineType {
    Running,
    Triple,
    Zigzag,
    Motif,
    Stem,
    Candlewick,
    EStitch,
    VStitch,
    Chain,
    Cutting,
    /// OpenStitch's satin line (#524, beyond PE-DESIGN): a satin column along the line whose
    /// width can change along it (width keys, tapered ends, per-side widths, a slant).
    Satin,
    /// Two stitches forward, one back (#542, beyond PE-DESIGN).
    BackStitch,
    /// A zigzag sewn several times with smooth random side offsets (#540, beyond PE-DESIGN).
    Scribble,
    /// Decorative hand-embroidery line (#543, beyond PE-DESIGN): chevron stitch.
    Chevron,
    /// Decorative hand-embroidery line (#543, beyond PE-DESIGN): cretan stitch.
    Cretan,
    /// Decorative hand-embroidery line (#543, beyond PE-DESIGN): feather stitch.
    Feather,
    /// Decorative hand-embroidery line (#543, beyond PE-DESIGN): overlock stitch.
    Overlock,
    /// Decorative hand-embroidery line (#543, beyond PE-DESIGN): alternating triangles stitch.
    AlternatingTriangles,
    /// Decorative hand-embroidery line (#543, beyond PE-DESIGN): double zigzag stitch.
    DoubleZigzag,
    /// Decorative hand-embroidery line (#543, beyond PE-DESIGN): lattice stitch.
    Lattice,
    /// Decorative hand-embroidery line (#543, beyond PE-DESIGN): stripes stitch.
    Stripes,
    /// Copies shrinking towards a target point, a closed line spiralling in (#541, beyond
    /// PE-DESIGN).
    Ripple,
}

impl LineType {
    pub const ALL: [LineType; 22] = [
        LineType::Running,
        LineType::Triple,
        LineType::Zigzag,
        LineType::Motif,
        LineType::Stem,
        LineType::Candlewick,
        LineType::EStitch,
        LineType::VStitch,
        LineType::Chain,
        LineType::Cutting,
        LineType::Satin,
        LineType::BackStitch,
        LineType::Scribble,
        LineType::Chevron,
        LineType::Cretan,
        LineType::Feather,
        LineType::Overlock,
        LineType::AlternatingTriangles,
        LineType::DoubleZigzag,
        LineType::Lattice,
        LineType::Stripes,
        LineType::Ripple,
    ];

    pub fn id(self) -> &'static str {
        match self {
            LineType::Running => "running",
            LineType::Triple => "triple",
            LineType::Zigzag => "zigzag",
            LineType::Motif => "motif",
            LineType::Stem => "stem",
            LineType::Candlewick => "candlewick",
            LineType::EStitch => "eStitch",
            LineType::VStitch => "vStitch",
            LineType::Chain => "chain",
            LineType::Cutting => "cutting",
            LineType::Satin => "satin",
            LineType::BackStitch => "backStitch",
            LineType::Scribble => "scribble",
            LineType::Chevron => "chevron",
            LineType::Cretan => "cretan",
            LineType::Feather => "feather",
            LineType::Overlock => "overlock",
            LineType::AlternatingTriangles => "alternatingTriangles",
            LineType::DoubleZigzag => "doubleZigzag",
            LineType::Lattice => "lattice",
            LineType::Stripes => "stripes",
            LineType::Ripple => "ripple",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|k| k.id() == id)
    }

    /// A decorative hand-embroidery line (#543): sewn by [`crate::decorative`].
    pub fn is_decorative(self) -> bool {
        matches!(
            self,
            LineType::Chevron
                | LineType::Cretan
                | LineType::Feather
                | LineType::Overlock
                | LineType::AlternatingTriangles
                | LineType::DoubleZigzag
                | LineType::Lattice
                | LineType::Stripes
        )
    }

    pub fn label(self) -> &'static str {
        match self {
            LineType::Running => "Running",
            LineType::Triple => "Triple",
            LineType::Zigzag => "Zigzag",
            LineType::Motif => "Motif",
            LineType::Stem => "Stem",
            LineType::Candlewick => "Candlewicking",
            LineType::EStitch => "E stitch",
            LineType::VStitch => "V stitch",
            LineType::Chain => "Chain",
            LineType::Cutting => "Cutting",
            LineType::Satin => "Satin line",
            LineType::BackStitch => "Back stitch",
            LineType::Scribble => "Scribble",
            LineType::Chevron => "Chevron",
            LineType::Cretan => "Cretan",
            LineType::Feather => "Feather",
            LineType::Overlock => "Overlock",
            LineType::AlternatingTriangles => "Alternating triangles",
            LineType::DoubleZigzag => "Double zigzag",
            LineType::Lattice => "Lattice",
            LineType::Stripes => "Stripes",
            LineType::Ripple => "Ripple",
        }
    }
}

/// How repeated elements (motifs, E/V strokes, chain links) sit on the line.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Arrange {
    #[default]
    Normal,
    /// Flipped to the other side of the line.
    Mirror,
    /// Every other element flipped.
    Alternate,
}

/// How motif, stem, candlewicking and chain lines get from the entry point (L1) into the
/// pattern when the points were moved (PE-DESIGN's Start/End Type).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StartEnd {
    /// Start with the pattern, which begins and ends at the exit point (PE-DESIGN's default).
    #[default]
    Feed,
    /// Start at the entry point (L1) and run along the line to where the pattern begins.
    Run,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ChainShape {
    #[default]
    Triangle,
    Diamond,
}

/// Line (outline) sewing. Attributes after `density` are optional and omitted from JSON when
/// unset.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LineSewing {
    #[serde(rename = "type")]
    pub kind: LineType,
    pub thread: Thread,
    /// Running/triple stitch length, mm.
    pub pitch: f64,
    /// Running passes (1–5); chain and E/V passes.
    pub run_times: u32,
    /// Width, mm: zigzag, motif, stem, candlewick size, E/V stroke, chain size.
    pub width: f64,
    /// Zigzag stitches per mm; candlewick spokes per knot ÷ 2.
    pub density: f64,
    /// Motif id from the library (motif lines).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motif: Option<String>,
    /// Motif height, mm (default 3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub height: Option<f64>,
    /// Gap between motifs, stem stitches, knots or E/V strokes, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub spacing: Option<f64>,
    /// Stem stitch angle to the line, degrees counter-clockwise on screen (45–135, default
    /// 45). `.osd` version 1 stored it clockwise.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub angle: Option<f64>,
    /// Stem stitch: single instead of triple.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub single: bool,
    /// E stitch: times each stroke is sewn (1–3).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stroke_times: Option<u32>,
    #[serde(default, skip_serializing_if = "is_default")]
    pub arrange: Arrange,
    #[serde(default, skip_serializing_if = "is_default")]
    pub chain_shape: ChainShape,
    /// Cutting lines: leave short uncut bridges so the fabric does not shift.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub leave_uncut: bool,
    /// Zigzag: pull every other inner-side point to the centre where a bend crowds them.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub half_stitch: bool,
    /// Zigzag: under sewing, a running stitch a quarter of the width to each side of the line
    /// (PE-DESIGN's default). Older files have none.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub underlay: bool,
    /// Line entry point (PE-DESIGN L1), 0.1 mm: closed outlines start at the nearest point,
    /// open ones from the nearer end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<Vec2>,
    /// Line exit point (PE-DESIGN L2), 0.1 mm, on the outline. The pattern of a closed stem,
    /// candlewick or chain line begins and ends there.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub exit: Option<Vec2>,
    /// Start/End Type of closed motif, stem, candlewick and chain lines with an entry point.
    #[serde(default, skip_serializing_if = "is_default")]
    pub start_end: StartEnd,
    /// A part of the first closed outline left unsewn (beyond PE-DESIGN, #497): the turning
    /// gap of in-the-hoop pieces. Running, triple, zigzag, E and V lines only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub gap: Option<LineGap>,
    /// Zigzag: corner styles, short stitches and compensation per side (#520-#522, beyond
    /// PE-DESIGN). Off by default.
    #[serde(default, skip_serializing_if = "is_default")]
    pub satin_shaping: crate::column::SatinShaping,
    /// Zigzag: the under sewing when `underlay` is on: `edge` (PE-DESIGN's two runs a quarter
    /// of the width to each side, the default), `centreWalk`, `edgeZigzag` or `auto` (#523).
    #[serde(default, skip_serializing_if = "is_default")]
    pub underlay_style: UnderlayStyle,
    /// Zigzag: thresholds of `auto` under sewing (#523).
    #[serde(default, skip_serializing_if = "is_default")]
    pub underlay_auto: crate::column::AutoUnderlay,
    /// Satin line (#524): the width along the line, by keys (empty: `width` all along).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub width_keys: Vec<crate::skeleton::WidthKey>,
    /// Satin line: the start tapers to a point over this length, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taper_start: Option<f64>,
    /// Satin line: the end tapers to a point over this length, mm.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub taper_end: Option<f64>,
    /// Satin line: the crossings turned this many degrees from square to the line (−60…60).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub slant: Option<f64>,
    /// Running: passes that follow a thickness profile (#542, beyond PE-DESIGN).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub profile: Option<crate::line_looks::RunProfile>,
    /// Running, triple and back stitch: the hand-stitch look (#540, beyond PE-DESIGN).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hand: Option<crate::line_looks::HandStitch>,
    /// Satin line: a motif repeated along the column instead of the satin, stretched across its
    /// local width (#544, beyond PE-DESIGN).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub motif_band: Option<crate::motif_band::MotifBand>,
    /// Scribble: length of its smooth side waves, mm (default 5).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub noise_scale: Option<f64>,
    /// Scribble: the variant (another seed sews other stitches).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<u32>,
    /// Lattice: strands crossing the band; stripes: rows across it (#543).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub strands: Option<u32>,
    /// Stripes: the gaps between neighbouring rows, mm, instead of even rows (#543).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub gaps: Vec<f64>,
    /// E stitch: its prongs slanted this many degrees from square to the line (#543).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub prong_angle: Option<f64>,
    /// Ripple: the point the copies shrink towards, 0.1 mm (`None`: the line's centre) (#541).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<Vec2>,
    /// Motif line: groups of motifs, `[count, gap after the group in mm]`, repeated (#544).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub motif_groups: Vec<(u32, f64)>,
    /// Motif line: running stitch only for this long at the start and at the end, mm (#544).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_start: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub free_end: Option<f64>,
    /// Motif line: more motifs used in turn after `motif` (#544).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub motifs: Vec<String>,
    /// Zigzag: PE-DESIGN's Sharp Corners button and Miter Limit (#373).
    #[serde(default, skip_serializing_if = "is_default")]
    pub sharp_corners: SharpCorners,
}

/// PE-DESIGN 11's Sharp Corners and Miter Limit of a Zigzag Stitch line (#373, measured from
/// crates/openstitch-formats/tests/fixtures/pe11/captures-2026-11-07/miter/ and the 2026-11-08
/// angle captures `line-zigzag-miter-v45/v60/v75/v120-*`). A corner sharper than 67.5 degrees
/// is sewn with its point cut off `miter_limit` mm from the corner along the bisector (0: no
/// point; the full point once the limit reaches it). Button 1 sews each leg up to where the two
/// legs cross and fills the point in a separate wedge; button 2 splits the corner along its
/// bisector, with short stitches into the point. Corners of 67.5 degrees and wider keep their
/// full mitre with either button (PE-DESIGN's buttons differ at 59.9° and agree at 75.3°).
/// PES 0110 stores the button and both limits (CEmbLine body +62, +64, +66).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SharpCorners {
    /// The Sharp Corners button, 1 or 2.
    pub button: u8,
    /// The Miter Limit each button keeps, mm (0-50): PE-DESIGN shows 0.0 for button 1 and
    /// 7.0 for button 2 on a new line.
    pub miter_limit: [f64; 2],
}

impl Default for SharpCorners {
    fn default() -> Self {
        SharpCorners {
            button: 1,
            miter_limit: [0.0, 7.0],
        }
    }
}

impl SharpCorners {
    /// The largest Miter Limit, mm (PE-DESIGN clamps a larger value to it).
    pub const MAX_LIMIT: f64 = 50.0;

    /// Index 0 or 1 of the chosen button.
    fn index(&self) -> usize {
        usize::from(self.button == 2)
    }

    /// The chosen button's Miter Limit, mm.
    pub fn limit(&self) -> f64 {
        self.miter_limit[self.index()].clamp(0.0, Self::MAX_LIMIT)
    }

    /// Sets the chosen button's Miter Limit, mm, clamped to 0-50.
    pub fn set_limit(&mut self, mm: f64) {
        self.miter_limit[self.index()] = mm.clamp(0.0, Self::MAX_LIMIT);
    }

    /// Whether button 2 (the corner split along its bisector) is chosen.
    pub fn split(&self) -> bool {
        self.button == 2
    }
}

/// An opening in a closed line: `length` mm left unsewn from `offset` mm along the outline
/// (from its first point, in its drawing direction).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct LineGap {
    pub offset: f64,
    pub length: f64,
}

impl LineGap {
    /// The line types a gap applies to.
    pub fn applies_to(kind: LineType) -> bool {
        matches!(
            kind,
            LineType::Running
                | LineType::Triple
                | LineType::Zigzag
                | LineType::EStitch
                | LineType::VStitch
        )
    }

    /// The closed outline `c` without the gap, as an open line from the gap's far end round
    /// to its near end; `None` when the gap is not shorter than the outline less 1 mm.
    pub fn open(&self, c: &Contour) -> Option<Contour> {
        if !c.closed {
            return None;
        }
        let path = c.path();
        let walker = crate::paths::Walker::new(&path);
        let total = walker.length();
        let len = self.length * 10.0;
        if len.is_nan() || len <= 0.0 || len >= total - 10.0 {
            return None;
        }
        let start = (self.offset * 10.0 + len).rem_euclid(total);
        let end = start + total - len;
        // Round the outline twice so the kept part never wraps.
        let mut twice = path.clone();
        twice.extend(path.iter().skip(1).copied());
        let w2 = crate::paths::Walker::new(&twice);
        let mut pts = w2.slice(start, end);
        pts.dedup_by(|a, b| a.distance(*b) < 1e-9);
        (pts.len() >= 2).then(|| Contour::new(pts, false))
    }
}

impl LineSewing {
    /// A line of `kind` with the attributes PE-DESIGN 11.50 gives that type (measured from
    /// compat/reference/stitches/line-*-default.pes): run pitch 2.0 mm for running, triple
    /// and E/V; zigzag 2.0 mm wide at 4.5 lines/mm with under sewing and half stitch; E/V
    /// strokes 2.5 mm every 5.0 mm; stem 2.0 mm every 1.5 mm at 45°; candlewicking knots of
    /// 5.0 mm (Medium: 24 points) 1.0 mm apart; chain triangles of 4.0 mm sewn 5 times.
    pub fn new(kind: LineType, thread: Thread) -> Self {
        let zigzag = kind == LineType::Zigzag;
        let satin = kind == LineType::Satin;
        let stroke = matches!(kind, LineType::EStitch | LineType::VStitch);
        LineSewing {
            kind,
            thread,
            pitch: match kind {
                LineType::Running | LineType::Triple | LineType::Zigzag | LineType::Satin => 2.0,
                LineType::Scribble => 1.0,
                _ if stroke => 2.0,
                _ => 2.5,
            },
            run_times: match kind {
                LineType::Chain => 5,
                LineType::Scribble => 3,
                _ => 1,
            },
            width: match kind {
                LineType::Candlewick => 5.0,
                LineType::Chain => 4.0,
                LineType::Scribble => 3.0,
                _ if stroke => 2.5,
                _ => 2.0,
            },
            density: match kind {
                LineType::Zigzag | LineType::Satin => 4.5,
                LineType::Candlewick => 12.0,
                _ => 3.0,
            },
            motif: None,
            height: None,
            spacing: None,
            angle: None,
            single: false,
            stroke_times: None,
            arrange: Arrange::Normal,
            chain_shape: ChainShape::Triangle,
            leave_uncut: false,
            half_stitch: zigzag,
            underlay: zigzag || satin,
            entry: None,
            exit: None,
            start_end: StartEnd::Feed,
            gap: None,
            satin_shaping: Default::default(),
            underlay_style: UnderlayStyle::Edge,
            underlay_auto: Default::default(),
            width_keys: Vec::new(),
            taper_start: None,
            taper_end: None,
            slant: None,
            profile: None,
            hand: None,
            motif_band: None,
            noise_scale: None,
            seed: None,
            strands: None,
            gaps: Vec::new(),
            prong_angle: None,
            target: None,
            motif_groups: Vec::new(),
            free_start: None,
            free_end: None,
            motifs: Vec::new(),
            sharp_corners: SharpCorners::default(),
        }
    }

    /// Spacing in mm, or the type's default.
    pub fn spacing_mm(&self) -> f64 {
        self.spacing.unwrap_or(match self.kind {
            LineType::Motif => 0.0,
            LineType::Stem => 1.5,
            LineType::Candlewick => 1.0,
            LineType::EStitch | LineType::VStitch => 5.0,
            k if k.is_decorative() => 3.0,
            LineType::Ripple => 1.5,
            _ => 1.0,
        })
    }
}
