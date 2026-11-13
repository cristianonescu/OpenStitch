//! PE-DESIGN 11 `#PES0110` objects as the design's editable objects, for `import.pes`: the
//! inverse of [`crate::pes11_export`]. Each `CEmbLine` becomes a shape with its outline, region
//! and line sewing; a Manual Punch block (the Manual Punch tool's Straight and Curve Blocks, and
//! PE-DESIGN's Convert to Blocks) becomes a column sewn along its direction lines; each
//! `CEmbNText` (with its Text Transform or its fit to an outline) becomes a text object laid out
//! again in Lato Bold; each `CEmbOne` becomes a stitch pattern at its place. The file's groups
//! stay groups, its hole sewing pair becomes the outer shape's hole, and its programmable fill
//! and decorative fill patterns become the design's own patterns.

use crate::commands::{CommandSpec, Effect, opt, read_file, req, spec};
use crate::pes11_export::ENVELOPES;
use openstitch_core::{Command, Stitch, Thread};
use openstitch_design::patterns::{Pattern, PatternKind};
use openstitch_design::text::{self, CharEdit, Spread, TextAlign, TextDirection, TextSpec, VAlign};
use openstitch_design::{
    Bezier, Contour, CrossTimes, Design, DesignObject, LineSewing, LineType, RegionSewing,
    RegionType, RunningStitchPath, StitchBlock, Transform, Vec2, v2,
};
use openstitch_formats::pes::ObjectSection;
use openstitch_formats::pes11::{
    KIND_CURVE_AFTER, PAGE_ORIGIN, PesPattern, RegionSewType, Shape, StitchGroup, Text, Vertex,
};
use serde_json::json;

/// Largest PES file accepted.
pub const MAX_PES_BYTES: usize = 64 << 20;

/// What a PES file's object list gives: its objects as design objects in list order, the
/// groups, the hole sewing pairs and fitted texts (as indexes into `objects`), the patterns the
/// objects use, and what was not imported.
#[derive(Clone, Debug, Default)]
pub struct Imported {
    pub objects: Vec<DesignObject>,
    pub groups: Vec<Vec<usize>>,
    /// `(outer, inner)`: the inner object is cut out of the outer one's region.
    pub holes: Vec<(usize, usize)>,
    /// `(text, outline)`: the text is fitted to the outline.
    pub fits: Vec<(usize, usize)>,
    /// Programmable and decorative fill patterns, referenced by the objects' regions.
    pub patterns: Vec<Pattern>,
    /// Objects not imported: `(list position, reason)`.
    pub skipped: Vec<(u16, String)>,
    /// What an imported object lost: `(list position, what)`.
    pub notes: Vec<(u16, String)>,
}

/// One entry of the object list.
enum Entry<'a> {
    Shape(&'a Shape),
    Text(&'a Text),
    Stitches(&'a StitchGroup),
}

/// The design objects of a `#PES0110` object list, in list order.
pub fn objects(section: &ObjectSection) -> Imported {
    let mut out = Imported::default();
    // The shapes fill the list positions no text or stitch group takes, in order.
    let mut list: std::collections::BTreeMap<u16, Entry> = section
        .texts
        .iter()
        .map(|t| (t.position, Entry::Text(t)))
        .chain(
            section
                .stitch_groups
                .iter()
                .map(|g| (g.position, Entry::Stitches(g))),
        )
        .collect();
    let total = list.len() + section.shapes.len();
    let free: Vec<u16> = (0..total)
        .filter_map(|p| u16::try_from(p).ok())
        .filter(|p| !list.contains_key(p))
        .take(section.shapes.len())
        .collect();
    for (s, p) in section.shapes.iter().zip(free) {
        list.insert(p, Entry::Shape(s));
    }
    let mut index_of = std::collections::BTreeMap::new();
    let mut fill_pattern: Option<String> = None;
    for (&position, entry) in &list {
        let made = match entry {
            Entry::Shape(s) => object(s, &section.threads).and_then(|mut o| {
                let motif_line = o.line.as_ref().is_some_and(|l| l.kind == LineType::Motif);
                let motif_fill = o
                    .region
                    .as_ref()
                    .is_some_and(|r| r.kind == RegionType::MotifFill);
                if motif_line || motif_fill {
                    // The header's motif list (the owner's motif: obj-motif-line-owner-v). With
                    // none listed the shape uses PE-DESIGN's built-in motif, which is not
                    // copied: OpenStitch's default motif stands in.
                    let id = match section.motif_patterns.as_slice() {
                        [p] => Some(add_pattern(&mut out.patterns, PatternKind::Motif, p)?),
                        _ => None,
                    };
                    if id.is_none() {
                        out.notes.push((
                            position,
                            "PE-DESIGN's built-in motif: OpenStitch's default motif is used"
                                .into(),
                        ));
                    }
                    if let Some(l) = o.line.as_mut().filter(|_| motif_line) {
                        l.motif = id.clone();
                        // Width along the line, height across it, spacing (#608).
                        if let Some([w, h, gap]) = s.motif_line {
                            l.width = f64::from(w);
                            l.height = Some(f64::from(h));
                            l.spacing = Some(f64::from(gap));
                        }
                    }
                    if let Some(r) = o.region.as_mut().filter(|_| motif_fill) {
                        r.pattern = id;
                        if let Some(([w, h], arrangement)) = s.motif_fill {
                            r.pattern_width = Some(f64::from(w));
                            r.pattern_height = Some(f64::from(h));
                            if arrangement != 0 {
                                out.notes.push((
                                    position,
                                    "the region motif's arrangement and spacing at OpenStitch's (one motif, as PE-DESIGN's Pattern1 only)".into(),
                                ));
                            }
                        }
                    }
                }
                if !s.stamps.is_empty()
                    && let Err(why) = add_stamps(&mut o, s, &section.fill_patterns)
                {
                    out.notes.push((position, format!("its stamps were not imported: {why}")));
                }
                let Some(r) = o.region.as_mut() else {
                    return Ok(o);
                };
                match r.kind {
                    RegionType::ProgrammableFill => {
                        let [p] = section.fill_patterns.as_slice() else {
                            return Err(format!(
                                "a programmable fill with {} patterns in the file (one is mapped)",
                                section.fill_patterns.len()
                            ));
                        };
                        let id = match &fill_pattern {
                            Some(id) => id.clone(),
                            None => {
                                let id = add_pattern(&mut out.patterns, PatternKind::Fill, p)?;
                                fill_pattern = Some(id.clone());
                                id
                            }
                        };
                        use_pattern(r, id, p);
                    }
                    RegionType::DecorativeFill => {
                        let p = s
                            .decorative
                            .as_ref()
                            .ok_or("a decorative fill without its pattern")?;
                        let id = add_pattern(&mut out.patterns, PatternKind::Decorative, p)?;
                        use_pattern(r, id, p);
                    }
                    _ => {}
                }
                Ok(o)
            }),
            Entry::Text(t) => text_object(t, &section.threads, &mut out.notes),
            Entry::Stitches(g) => Ok(stitch_object(g, &section.threads)),
        };
        match made {
            Ok(o) => {
                index_of.insert(position, out.objects.len());
                if let Entry::Text(t) = entry
                    && let Some(f) = &t.fit
                {
                    match index_of.get(&f.path) {
                        Some(&path) => out.fits.push((out.objects.len(), path)),
                        None => out.notes.push((
                            position,
                            format!("its outline (position {}) was not imported", f.path),
                        )),
                    }
                }
                out.objects.push(o);
            }
            Err(reason) => out.skipped.push((position, reason)),
        }
    }
    out.groups = section
        .groups
        .iter()
        .map(|g| g.iter().filter_map(|p| index_of.get(p).copied()).collect())
        .filter(|g: &Vec<usize>| !g.is_empty())
        .collect();
    out.holes = section
        .holes
        .iter()
        .filter_map(|[a, b]| Some((*index_of.get(a)?, *index_of.get(b)?)))
        .collect();
    out
}

/// Adds the imported objects to `design`: their groups (when `group`), hole pairs, fitted texts
/// and patterns. Returns the new objects' ids and the new group ids.
pub fn add_to(design: &mut Design, imported: Imported, group: bool) -> (Vec<u64>, Vec<u64>) {
    for p in imported.patterns {
        if !design.patterns.iter().any(|q| q.id == p.id) {
            design.patterns.push(p);
        }
    }
    let ids: Vec<u64> = imported
        .objects
        .into_iter()
        .map(|o| design.add(o))
        .collect();
    let at = |design: &mut Design, k: usize| design.index_of(ids[k]);
    for (outer, inner) in imported.holes {
        if let Some(i) = at(design, outer) {
            design.objects[i].hole_sewing = true;
            design.objects[i].hole_inner = Some(ids[inner]);
        }
    }
    for (t, path) in imported.fits {
        if let Some(i) = at(design, t) {
            if let Some(spec) = design.objects[i].text.as_mut() {
                spec.path = Some(ids[path]);
            }
            let _ = design.layout_text(i);
        }
    }
    let mut groups = Vec::new();
    if group {
        for members in imported.groups.iter().filter(|g| g.len() > 1) {
            let g = design.new_group_id();
            for &k in members {
                if let Some(i) = at(design, k) {
                    design.objects[i].group = Some(g);
                }
            }
            groups.push(g);
        }
    }
    (ids, groups)
}

/// Design coordinates (0.1 mm from the page's top-left) of a file point.
fn design_xy(x: f32, y: f32) -> Vec2 {
    v2(f64::from(x - PAGE_ORIGIN), f64::from(y - PAGE_ORIGIN))
}

fn thread(table: &[Thread], index: u16) -> Thread {
    table
        .get(usize::from(index))
        .map(openstitch_formats::pes11::with_chart_name)
        .unwrap_or_else(|| Thread::rgb(0))
}

/// One shape as a design object, its vertices placed by its transform.
fn object(s: &Shape, table: &[Thread]) -> Result<DesignObject, String> {
    if s.vertices.len() < 2 {
        return Err("fewer than two vertices".into());
    }
    let vertices = s.placed_vertices();
    let outline = contour(&vertices, s.closed);
    let mut region = match s.region {
        RegionSewType::Fill => Some(RegionType::Fill),
        RegionSewType::Satin => Some(RegionType::Satin),
        RegionSewType::NotSewn => None,
        RegionSewType::ProgrammableFill => Some(RegionType::ProgrammableFill),
        RegionSewType::Decorative => Some(RegionType::DecorativeFill),
        RegionSewType::Motif => Some(RegionType::MotifFill),
        // Size and Times are read below.
        RegionSewType::Cross => Some(RegionType::CrossStitch),
        // The exporter writes these at PE-DESIGN's defaults (their own fields are not mapped),
        // so they are read as the type at OpenStitch's defaults.
        RegionSewType::Concentric => Some(RegionType::ConcentricCircle),
        RegionSewType::Radial => Some(RegionType::Radial),
        RegionSewType::Spiral => Some(RegionType::Spiral),
        RegionSewType::Piping => Some(RegionType::Piping),
        RegionSewType::Stippling => Some(RegionType::Stippling),
        RegionSewType::NetFill => Some(RegionType::NetFill),
        RegionSewType::ZigzagNet => Some(RegionType::ZigzagNetFill),
        RegionSewType::FlexibleSpiral => Some(RegionType::FlexibleSpiral),
        other => return Err(format!("region type {other:?} is not mapped yet")),
    }
    .filter(|_| s.closed)
    .map(|kind| {
        let mut r = RegionSewing::new(kind, thread(table, s.region_color));
        if matches!(
            kind,
            RegionType::Fill | RegionType::Satin | RegionType::ProgrammableFill
        ) {
            r.density = f64::from(s.density);
            r.angle = f64::from(s.direction);
            r.underlay = s.under_sewing;
        }
        if kind == RegionType::CrossStitch {
            // PE-DESIGN's default 2.5 mm stays OpenStitch's default (no spacing set).
            let size = f64::from(s.cross_size);
            if size > 0.0 && (size - r.spacing_mm()).abs() > 0.01 {
                r.spacing = Some(size);
            }
            r.times = CrossTimes::from_count(usize::from(s.cross_passes / 2));
        }
        if kind == RegionType::Fill {
            r.frequency = Some(f64::from(s.frequency));
            r.running_stitch_path = Some(match s.running_path {
                1 => RunningStitchPath::AlongOutline,
                2 => RunningStitchPath::OnOutline,
                _ => RunningStitchPath::Inside,
            });
        }
        r
    });
    if let Some(r) = region.as_mut().filter(|_| !s.punch.is_empty()) {
        r.direction_lines = punch_lines(s, &vertices)?;
        // A Curve Block's sides are Bézier curves through the line ends (o-manual-curve).
        r.curved_lines = s.vertices.iter().any(|v| v.kind != 0);
    }
    let line = if s.line_sewn {
        let kind = match (s.line_type, s.line_variant) {
            (Shape::LINE_ZIGZAG, _) => LineType::Zigzag,
            (Shape::LINE_RUNNING, _) => LineType::Running,
            (Shape::LINE_TRIPLE, _) => LineType::Triple,
            (Shape::LINE_MOTIF, _) => LineType::Motif,
            // +86: 0 E, 1 V (line-e-default, line-v-default).
            (Shape::LINE_E_OR_V, 1) => LineType::VStitch,
            (Shape::LINE_E_OR_V, _) => LineType::EStitch,
            (Shape::LINE_STEM, _) => LineType::Stem,
            (Shape::LINE_CANDLEWICK, _) => LineType::Candlewick,
            (Shape::LINE_CHAIN, _) => LineType::Chain,
            (Shape::LINE_CUTTING, _) => LineType::Cutting,
            (other, _) => return Err(format!("line type {other} is not mapped yet")),
        };
        let mut l = LineSewing::new(kind, thread(table, s.line_color));
        l.width = f64::from(if matches!(kind, LineType::EStitch | LineType::VStitch) {
            s.ev_width
        } else {
            s.zigzag_width
        });
        l.density = f64::from(s.zigzag_density);
        // An E or V line's spacing (+88, #982): kept unset at PE-DESIGN's default 5.0.
        if matches!(kind, LineType::EStitch | LineType::VStitch)
            && s.ev_spacing > 0.0
            && (s.ev_spacing - 5.0).abs() > 1e-6
        {
            l.spacing = Some(f64::from(s.ev_spacing));
        }
        // Sharp Corners and both buttons' Miter Limits (body +62, +64, +66, #373).
        l.sharp_corners.button = if s.sharp_corners == 1 { 2 } else { 1 };
        l.sharp_corners.miter_limit = s
            .miter_limits
            .map(|mm| f64::from(mm).clamp(0.0, openstitch_design::model::SharpCorners::MAX_LIMIT));
        l.pitch = f64::from(s.run_pitch);
        l.underlay = s.line_under_sewing;
        Some(l)
    } else {
        None
    };
    let mut o = DesignObject::shape(vec![outline], region, line);
    if !s.punch.is_empty() {
        o.name = Some("Manual punch".into());
    }
    Ok(o)
}

/// The shape's stamps (Stamp/Emboss > Convert from Outline) as the region's stamps. Each takes
/// its outline from the header's first pattern list, scaled from the 1024-unit cell to the
/// stamp's size around its centre. A region polyline (head `[1, …]` Embossed Region, `[2, …]`
/// Engraved Region) gives that effect on its own outline; a pattern with only plain polylines is
/// an Engraved Line (round-10 `471-*-circle.pes`, #959). Only one listed pattern is mapped:
/// which pattern a stamp uses with several listed has not been captured.
fn add_stamps(o: &mut DesignObject, s: &Shape, listed: &[PesPattern]) -> Result<(), String> {
    use openstitch_design::stamp::{STAMP_TYPES, Stamp, StampEffect};
    let r = o
        .region
        .as_mut()
        .filter(|r| STAMP_TYPES.contains(&r.kind))
        .ok_or("a region type that takes no stamps")?;
    let [p] = listed else {
        return Err(format!(
            "{} patterns listed in the file (one is mapped)",
            listed.len()
        ));
    };
    let effect = |head: [u16; 3]| match head[0] {
        1 => Some(StampEffect::EmbossedRegion),
        2 => Some(StampEffect::EngravedRegion),
        _ => None,
    };
    let regions: Vec<usize> = (0..p.polylines.len())
        .filter(|&k| effect(p.head(k)).is_some())
        .collect();
    let (kind, lines) = match regions.as_slice() {
        [] => (StampEffect::EngravedLine, (0..p.polylines.len()).collect()),
        [k, ..]
            if regions
                .iter()
                .all(|&j| effect(p.head(j)) == effect(p.head(*k))) =>
        {
            (
                effect(p.head(*k)).expect("a region effect"),
                regions.clone(),
            )
        }
        _ => return Err("a pattern with both region effects".into()),
    };
    for stamp in &s.stamps {
        let contours: Vec<Contour> = lines
            .iter()
            .filter_map(|&k| {
                let mut points: Vec<Vec2> = Vec::new();
                for &q in &p.polylines[k] {
                    let (x, y) = stamp.place(q);
                    let v = design_xy(x, y);
                    if points.last().is_none_or(|l| l.distance(v) > 1e-6) {
                        points.push(v);
                    }
                }
                if points.len() > 1 && points[0].distance(points[points.len() - 1]) <= 1e-6 {
                    points.pop();
                }
                (points.len() >= 3).then(|| Contour::new(points, true))
            })
            .collect();
        if contours.is_empty() {
            return Err("a stamp outline of fewer than three points".into());
        }
        r.stamps.push(Stamp {
            effect: kind,
            contours,
            pattern: None,
        });
    }
    Ok(())
}

/// A pattern of the file as a design pattern (lines in the unit tile, as the exporter writes
/// them back), added to `patterns` unless the same one is there. Returns its id.
fn add_pattern(
    patterns: &mut Vec<Pattern>,
    kind: PatternKind,
    p: &PesPattern,
) -> Result<String, String> {
    let paths: Vec<Vec<[f64; 2]>> = match kind {
        // A motif as `.pmf` sources are read (`pattern_text`): its stroke from the first point
        // to the last is one motif length, the points along it 0…1 and across it at that scale.
        PatternKind::Motif => {
            let [line] = p.polylines.as_slice() else {
                return Err(format!(
                    "a motif of {} polylines (one is mapped)",
                    p.polylines.len()
                ));
            };
            let pt = |&(x, y): &(i16, i16)| (f64::from(x), f64::from(y));
            let (a, b) = match (line.first(), line.last()) {
                (Some(a), Some(b)) => (pt(a), pt(b)),
                _ => return Err("an empty motif".into()),
            };
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = dx.hypot(dy);
            if len < 1e-9 {
                return Err("a motif that ends where it starts".into());
            }
            let (ux, uy) = (dx / len, dy / len);
            vec![
                line.iter()
                    .map(|q| {
                        let (x, y) = (pt(q).0 - a.0, pt(q).1 - a.1);
                        [
                            (x * ux + y * uy) / len,
                            ((-x * uy + y * ux) / len).clamp(-1.0, 1.0),
                        ]
                    })
                    .collect(),
            ]
        }
        _ => p
            .polylines
            .iter()
            .map(|l| {
                l.iter()
                    .map(|&(x, y)| [f64::from(x) / 1024.0, f64::from(y) / 1024.0])
                    .collect()
            })
            .collect(),
    };
    let (prefix, name) = match kind {
        PatternKind::Fill => ("pes-fill", "PES programmable fill"),
        PatternKind::Motif => ("pes-motif", "PES motif"),
        PatternKind::Decorative => ("pes-decorative", "PES decorative fill"),
        // PES has no meander fill; no pattern of this kind is read from one.
        PatternKind::Meander => ("pes-meander", "PES meander fill"),
    };
    let make = |id: String| -> Result<Pattern, String> {
        serde_json::from_value(json!({
            "format": "openstitch.pattern",
            "version": 1,
            "kind": kind,
            "id": id,
            "name": name,
            "paths": paths,
            "freeLines": kind == PatternKind::Decorative,
        }))
        .map_err(|e| format!("its pattern: {e}"))
    };
    for n in 1.. {
        let candidate = make(format!("{prefix}-{n}"))?;
        match patterns.iter().find(|q| q.id == candidate.id) {
            Some(q) if q.paths == candidate.paths => return Ok(candidate.id),
            Some(_) => continue,
            None => {
                let id = candidate.id.clone();
                patterns.push(candidate);
                return Ok(id);
            }
        }
    }
    unreachable!("an unused pattern id")
}

/// Region `r` sews pattern `id` at the file's tile size (0.01 mm, square tiles).
fn use_pattern(r: &mut RegionSewing, id: String, p: &PesPattern) {
    let size = f64::from(p.size) / 100.0;
    r.pattern = Some(id);
    r.pattern_width = Some(size);
    r.pattern_height = Some(size);
}

/// A block's direction lines: each `[a, b]` joins vertex a and vertex b.
fn punch_lines(s: &Shape, vertices: &[Vertex]) -> Result<Vec<[Vec2; 2]>, String> {
    s.punch
        .iter()
        .map(|&[a, b]| {
            let at = |k: u32| {
                vertices
                    .get(k as usize)
                    .map(|v| design_xy(v.x, v.y))
                    .ok_or_else(|| format!("direction line to vertex {k} of {}", vertices.len()))
            };
            Ok([at(a)?, at(b)?])
        })
        .collect()
}

/// The vertices as a contour: segment i is a Bézier curve when vertex i curves after it, with
/// vertex i's leaving handle and the next vertex's arriving handle.
fn contour(vertices: &[Vertex], closed: bool) -> Contour {
    let n = vertices.len();
    let mut c = Contour::new(
        vertices.iter().map(|v| design_xy(v.x, v.y)).collect(),
        closed,
    );
    for i in 0..c.segment_count() {
        let (a, b) = (&vertices[i], &vertices[(i + 1) % n]);
        if a.kind & KIND_CURVE_AFTER != 0 {
            c.set_curve(
                i,
                Some(Bezier {
                    c1: design_xy(a.handles[1].0, a.handles[1].1),
                    c2: design_xy(b.handles[0].0, b.handles[0].1),
                }),
            );
        }
    }
    c
}

/// A `CEmbNText` as a text object in Lato Bold (PE-DESIGN's Lato, whatever weight it records:
/// docs/FILE_FORMATS.md letter +144), laid out again from the font at the text's click point.
/// The first letter's attributes are the text's; letters of another size become character
/// edits, other per-letter differences are noted.
fn text_object(
    t: &Text,
    table: &[Thread],
    notes: &mut Vec<(u16, String)>,
) -> Result<DesignObject, String> {
    let first = t.letters.first().ok_or("a text without letters")?;
    if !first.font.eq_ignore_ascii_case("Lato") {
        return Err(format!(
            "font {:?} (only Lato is mapped: OpenStitch lays text out in Lato Bold)",
            first.font
        ));
    }
    let mut chars = String::new();
    let mut edits = Vec::new();
    for (k, l) in t.letters.iter().enumerate() {
        if k > 0 && t.breaks.contains(&(k as u16)) {
            chars.push('\n');
        }
        chars.push(char::from_u32(u32::from(l.ch)).ok_or(format!("character {:#06x}", l.ch))?);
        if l.size != first.size {
            edits.push(CharEdit {
                index: chars.chars().count() - 1,
                x: 0.0,
                y: 0.0,
                rotation: 0.0,
                size: Some(f64::from(l.size)),
            });
        }
        let differs = l.region != first.region
            || l.region_color != first.region_color
            || l.density != first.density
            || l.line_sewn != first.line_sewn
            || l.line_type != first.line_type
            || l.line_color != first.line_color;
        if differs {
            notes.push((
                t.position,
                format!("letter {k} has its own sewing; the text takes the first letter's"),
            ));
        }
    }
    let mut spec = TextSpec::new(chars, text::DEFAULT_FONT, f64::from(first.size));
    spec.characters = edits;
    spec.letter_spacing = f64::from(t.char_spacing) / 10.0;
    spec.line_spacing_percent = f64::from(t.line_spacing);
    if t.vertical {
        spec.direction = TextDirection::Vertical;
    }
    let align = |code: u16| match code {
        1 => TextAlign::Center,
        2 => TextAlign::Right,
        _ => TextAlign::Left,
    };
    spec.align = align(t.align);
    if let Some(f) = &t.fit {
        // The fit's alignment is the text's; spread evenly is code 4.
        if f.halign == 4 {
            spec.fit.spread = Spread::Even;
        } else {
            spec.align = align(f.halign);
        }
        spec.fit.valign = match f.valign {
            0 => VAlign::Bottom,
            1 => VAlign::Middle,
            _ => VAlign::Top,
        };
        spec.fit.upright = f.upright;
        spec.fit.other_side = f.other_side;
    } else {
        // The click point (the first baseline's alignment point), moved by the text's own
        // transform (the agjy Text Transform files: a 0.3 mm / -0.1 mm move).
        let [a, b, c, d, tx, ty] = t.transform.map(f64::from);
        let (ox, oy) = (f64::from(t.origin.0), f64::from(t.origin.1));
        let o = f64::from(PAGE_ORIGIN);
        spec.transform = Transform {
            a,
            b,
            c,
            d,
            tx: a * ox + c * oy + tx - o,
            ty: b * ox + d * oy + ty - o,
        };
        if let Some(e) = &t.envelope {
            let k = ENVELOPES
                .iter()
                .position(|&(code, _)| code == e.code)
                .ok_or(format!("Text Transform code {}", e.code))?;
            spec.envelope = Some(k as u8 + 1);
            if e.strength != ENVELOPES[k].1 {
                spec.envelope_level = Some(f64::from(e.strength));
            }
        }
    }
    let kind = match first.region {
        RegionSewType::Satin => Some(RegionType::Satin),
        RegionSewType::Fill => Some(RegionType::Fill),
        RegionSewType::NotSewn => None,
        other => return Err(format!("text region type {other:?} is not mapped")),
    };
    let region = kind.map(|kind| {
        let mut r = RegionSewing::new(kind, thread(table, first.region_color));
        r.density = f64::from(first.density);
        // PE-DESIGN's text has under sewing off (the Text tool's default; the writer refuses it).
        r.underlay = false;
        r
    });
    let line = if first.line_sewn {
        let kind = match first.line_type {
            Shape::LINE_RUNNING => LineType::Running,
            Shape::LINE_TRIPLE => LineType::Triple,
            Shape::LINE_ZIGZAG => LineType::Zigzag,
            other => return Err(format!("text line type {other} is not mapped")),
        };
        Some(LineSewing::new(kind, thread(table, first.line_color)))
    } else {
        None
    };
    DesignObject::text(spec, region, line)
        .map(|(o, _)| o)
        .map_err(|e| e.to_string())
}

/// A `CEmbOne` as a stitch pattern at its place: each run's points through the group's
/// transform, a jump run as a jump to its end, a new thread at each colour change.
fn stitch_object(g: &StitchGroup, table: &[Thread]) -> DesignObject {
    let [a, b, c, d, tx, ty] = g.transform.map(f64::from);
    let o = f64::from(PAGE_ORIGIN);
    let place = |(x, y): (i16, i16)| {
        let (x, y) = (f64::from(x), f64::from(y));
        (a * x + c * y + tx - o, b * x + d * y + ty - o)
    };
    let mut stitches = Vec::new();
    let mut threads: Vec<Thread> = Vec::new();
    let mut colour = None;
    for seg in &g.segments {
        if colour != Some(seg.color) {
            if colour.is_some() {
                let (x, y) = stitches.last().map_or((0.0, 0.0), |s: &Stitch| (s.x, s.y));
                stitches.push(Stitch::new(x, y, Command::ColorChange));
            }
            colour = Some(seg.color);
            threads.push(thread(table, seg.color));
        }
        if seg.flag == 1 {
            if let Some(&p) = seg.points.last() {
                let (x, y) = place(p);
                stitches.push(Stitch::new(x, y, Command::Jump));
            }
            continue;
        }
        for &p in &seg.points {
            let (x, y) = place(p);
            stitches.push(Stitch::new(x, y, Command::Stitch));
        }
    }
    DesignObject::stitches(StitchBlock { stitches, threads })
}

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "import.pes",
        "Import a PE-DESIGN 11 PES file's objects (shapes with their stamps, Manual Punch blocks, texts and stitch groups) as editable objects; into an empty design it also takes the file's page mode (Hoop Size or Custom Size page, design.info pageMode)",
        Effect::Edit,
        vec![
            req(
                "path",
                "string",
                "PES file (relative to the session directory)",
            ),
            opt("group", "bool", "Keep the file's groups (default true)"),
        ],
        |s, p| {
            let (_, data) = read_file(s, p, MAX_PES_BYTES)?;
            let section = openstitch_formats::pes::read_objects(&data)
                .map_err(|e| p.failed(e.to_string()))?;
            if section.version != "0110" {
                return Err(p.failed(format!(
                    "PES {} has no editable objects (PE-DESIGN 11's 0110 has); use stitches.import",
                    section.version
                )));
            }
            let imported = objects(&section);
            if imported.objects.is_empty() {
                return Err(p.failed("the file has no objects OpenStitch can import"));
            }
            let skipped: Vec<_> = imported
                .skipped
                .iter()
                .map(|(at, why)| json!({ "position": at, "reason": why }))
                .collect();
            let notes: Vec<_> = imported
                .notes
                .iter()
                .map(|(at, what)| json!({ "position": at, "note": what }))
                .collect();
            let (texts, stitch_groups) = (section.texts.len(), section.stitch_groups.len());
            // Into an empty design, the file's page mode too (#1013), as its Design Property.
            if s.design.objects.is_empty() && section.page_mm.is_some() {
                s.design.machine.custom_page = !section.hoop_page;
            }
            let (ids, groups) = add_to(&mut s.design, imported, p.bool("group", Some(true))?);
            // The file's Design Property, into a design that has none (as stitches.import).
            let found = openstitch_design::DesignProperties::from_metadata(&section.metadata);
            let adopt = s.design.properties.is_empty() && !found.is_empty();
            if adopt {
                s.design.properties = found;
            }
            Ok(json!({
                "ids": ids,
                "groups": groups,
                "texts": texts,
                "stitchGroups": stitch_groups,
                "skipped": skipped,
                "notes": notes,
                "unparsed": section.unparsed,
                "propertiesAdopted": adopt,
                "pageMode": if s.design.hoop_size_page() { "hoop" } else { "custom" },
            }))
        },
    )]
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::ObjectKind;

    fn fixture(name: &str) -> ObjectSection {
        let path = format!(
            "{}/../openstitch-formats/tests/fixtures/pe11/{name}.pes",
            env!("CARGO_MANIFEST_DIR")
        );
        let data = std::fs::read(&path).expect("fixture");
        openstitch_formats::pes::read_objects(&data).expect("objects")
    }

    fn reference(name: &str) -> ObjectSection {
        let path = format!(
            "{}/../../compat/reference/{name}.pes",
            env!("CARGO_MANIFEST_DIR")
        );
        let data = std::fs::read(&path).expect("reference");
        openstitch_formats::pes::read_objects(&data).expect("objects")
    }

    fn blocks(i: &Imported) -> Vec<usize> {
        i.objects
            .iter()
            .filter_map(|o| o.region.as_ref())
            .map(|r| r.direction_lines.len())
            .filter(|&n| n > 0)
            .collect()
    }

    #[test]
    fn convert_to_blocks_imports_as_pe_designs_cut() {
        // PE-DESIGN 11.50's Convert to Blocks of Lato Bold H (stroke-blocks-H): the left stem
        // top (3 lines), the left stem bottom (4), the left junction square (2), the bar (4)
        // and the right stem whole (7), with a running piece between the first ones, all in one
        // group. A: left leg upper and lower, the junction, the bar, the right leg with the apex.
        let h = objects(&fixture("stroke-blocks-H"));
        assert!(h.skipped.is_empty(), "{:?}", h.skipped);
        assert_eq!(h.objects.len(), 8);
        assert_eq!(blocks(&h), [3, 4, 2, 4, 7]);
        assert_eq!(h.groups, [(0..8).collect::<Vec<usize>>()]);
        let a = objects(&fixture("stroke-blocks-A"));
        assert!(a.skipped.is_empty(), "{:?}", a.skipped);
        assert_eq!(blocks(&a).len(), 5);
        // The blocks are satin columns on the page (100 x 100 mm), every line inside the
        // block's own outline box.
        for o in h.objects.iter().chain(&a.objects) {
            let Some(r) = o.region.as_ref().filter(|r| !r.direction_lines.is_empty()) else {
                // The running pieces between blocks: open, line only.
                assert!(o.region.is_none() && o.line.is_some());
                continue;
            };
            assert_eq!(r.kind, RegionType::Satin);
            let b = o.bounds().expect("bounds");
            for p in r.direction_lines.iter().flatten() {
                assert!((0.0..=1000.0).contains(&p.x) && (0.0..=1000.0).contains(&p.y));
                assert!(b.min_x - 1.0 <= p.x && p.x <= b.max_x + 1.0, "{p:?} {b:?}");
                assert!(b.min_y - 1.0 <= p.y && p.y <= b.max_y + 1.0, "{p:?} {b:?}");
            }
        }
    }

    #[test]
    fn manual_punch_blocks_keep_their_lines_and_curves() {
        // The Manual Punch tool's Straight Block (o-manual-straight) and Curve Block
        // (o-manual-curve): the lines join vertex k and 2n - 1 - k.
        for (name, curved) in [("o-manual-straight", false), ("o-manual-curve", true)] {
            let section = fixture(name);
            let i = objects(&section);
            let s = &section.shapes[0];
            let o = &i.objects[0];
            let r = o.region.as_ref().expect("region");
            assert_eq!(r.direction_lines.len(), s.punch.len(), "{name}");
            assert_eq!(r.curved_lines, curved, "{name}");
            let n = s.vertices.len();
            let first = design_xy(s.vertices[0].x, s.vertices[0].y);
            let last = design_xy(s.vertices[n - 1].x, s.vertices[n - 1].y);
            assert_eq!(r.direction_lines[0], [first, last], "{name}");
            assert_eq!(o.contours[0].has_curves(), curved, "{name}");
        }
    }

    #[test]
    fn moved_and_resized_shapes_import_where_pe_design_shows_them() {
        // tatami-40-pos0: the 40 mm square moved to the page corner (#398): its vertices are
        // where it was drawn, its transform (scale 1.013, shift) places it at 0..40.1 mm.
        // tatami-40x20: resized to 20 mm high (a y scale of 0.5).
        for (name, bounds) in [
            (
                "stitches/tatami-40-pos0-st1-dir30",
                [0.0, 0.0, 401.0, 400.0],
            ),
            (
                "stitches/tatami-40x20-st1-dir30",
                [100.0, 100.0, 501.0, 300.0],
            ),
        ] {
            let i = objects(&reference(name));
            assert!(i.skipped.is_empty(), "{name}: {:?}", i.skipped);
            let b = i.objects[0].bounds().expect("bounds");
            let got = [b.min_x, b.min_y, b.max_x, b.max_y];
            for (g, w) in got.iter().zip(bounds) {
                assert!((g - w).abs() <= 1.0, "{name}: {got:?} against {bounds:?}");
            }
        }
    }

    #[test]
    fn texts_import_as_lato_text_at_their_click_point() {
        // r12-text-a: PE-DESIGN's Text tool, "A" at size 10 mm, Satin, line not sewn.
        let section = fixture("r12-text-a");
        let i = objects(&section);
        assert!(i.skipped.is_empty(), "{:?}", i.skipped);
        let o = &i.objects[0];
        assert_eq!(o.kind, ObjectKind::Text);
        let spec = o.text.as_ref().expect("text");
        let t = &section.texts[0];
        assert_eq!(spec.text, "A");
        assert_eq!(spec.size, f64::from(t.letters[0].size));
        assert_eq!(spec.font, text::DEFAULT_FONT);
        assert_eq!(
            (spec.transform.tx, spec.transform.ty),
            (
                f64::from(t.origin.0 - PAGE_ORIGIN),
                f64::from(t.origin.1 - PAGE_ORIGIN)
            )
        );
        assert_eq!(o.region.as_ref().map(|r| r.kind), Some(RegionType::Satin));
        assert!(o.line.is_none());
    }

    #[test]
    fn text_transforms_and_fitted_texts_keep_their_settings() {
        // Every Text Transform preset of PE-DESIGN's own files (text-transform-N) imports as
        // that preset at its level.
        for n in 1..=26u8 {
            let section = reference(&format!("text/text-transform-{n}"));
            let i = objects(&section);
            assert!(i.skipped.is_empty(), "{n}: {:?}", i.skipped);
            let spec = i.objects[0].text.as_ref().expect("text");
            assert_eq!(spec.envelope, Some(n), "preset {n}");
        }
        // Fit Text to Outline (text-fit-default): the text follows the circle before it.
        let section = reference("text/text-fit-default");
        let i = objects(&section);
        assert!(i.skipped.is_empty(), "{:?}", i.skipped);
        assert_eq!(i.fits.len(), 1);
        let (t, path) = i.fits[0];
        assert!(path < t);
        assert_eq!(i.objects[t].kind, ObjectKind::Text);
        let mut d = Design::default();
        let (ids, _) = add_to(&mut d, i, true);
        let k = d.index_of(ids[t]).expect("text");
        assert_eq!(
            d.objects[k].text.as_ref().and_then(|s| s.path),
            Some(ids[path])
        );
        assert!(!d.objects[k].contours.is_empty());
    }

    #[test]
    fn stitch_groups_import_at_their_place() {
        // pes-0110-stitches: PE-DESIGN's re-save of a stitch pattern. The imported stitches
        // land inside the group's bounds, with one thread per colour block.
        let section = reference("pes/pes-0110-stitches");
        let i = objects(&section);
        let g = &section.stitch_groups[0];
        let o = i
            .objects
            .iter()
            .find(|o| o.kind == ObjectKind::Stitches)
            .expect("stitches");
        let block = o.stitch_block.as_ref().expect("block");
        let o_ = f64::from(PAGE_ORIGIN);
        let [x0, y0, x1, y1] = g.bounds.map(|v| f64::from(v) - o_);
        let stitched: Vec<_> = block
            .stitches
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .collect();
        assert!(!stitched.is_empty());
        for s in stitched {
            assert!(x0 - 1.0 <= s.x && s.x <= x1 + 1.0 && y0 - 1.0 <= s.y && s.y <= y1 + 1.0);
        }
        let changes = block
            .stitches
            .iter()
            .filter(|s| s.command == Command::ColorChange)
            .count();
        assert_eq!(block.threads.len(), changes + 1);
    }

    #[test]
    fn hole_pairs_and_programmable_fills_import_with_their_shapes() {
        // hole-sewing-pentagon-star: the pentagon is sewn around the star's Not Sewn copy.
        let i = objects(&reference("stitches/hole-sewing-pentagon-star"));
        assert_eq!(i.holes, [(0, 1)]);
        // obj-progfill-owner-v: the owner's V pattern becomes a design pattern at 7.5 mm.
        let i = objects(&fixture("obj-progfill-owner-v"));
        assert!(i.skipped.is_empty(), "{:?}", i.skipped);
        assert_eq!(i.patterns.len(), 1);
        let r = i.objects[0].region.as_ref().expect("region");
        assert_eq!(r.kind, RegionType::ProgrammableFill);
        assert_eq!(r.pattern.as_deref(), Some(i.patterns[0].id.as_str()));
        assert_eq!(r.pattern_size_mm(), (7.5, 7.5));
        // obj-motif-line-owner-v: the owner's motif V (0, 512) (512, 768) (1024, 512) as a
        // motif of one length, its tip a quarter length across.
        let i = objects(&fixture("obj-motif-line-owner-v"));
        assert!(i.skipped.is_empty(), "{:?}", i.skipped);
        let l = i.objects[0].line.as_ref().expect("line");
        assert_eq!(l.kind, LineType::Motif);
        let m = i.patterns.iter().find(|p| Some(&p.id) == l.motif.as_ref());
        let m = m.expect("motif pattern");
        assert_eq!(m.paths, [vec![v2(0.0, 0.0), v2(0.5, 0.25), v2(1.0, 0.0)]]);
    }

    #[test]
    fn exported_designs_import_back() {
        // The exporter's shape, text and stitch pattern import back as the same objects.
        let mut d = Design::default();
        let square = Contour::new(
            vec![
                v2(100.0, 100.0),
                v2(500.0, 100.0),
                v2(500.0, 500.0),
                v2(100.0, 500.0),
            ],
            true,
        );
        let mut region = RegionSewing::new(RegionType::Fill, Thread::rgb(0x0000ff));
        region.angle = 30.0;
        region.density = 5.0;
        let line = LineSewing::new(LineType::Running, Thread::rgb(0xff0000));
        d.add(DesignObject::shape(
            vec![square.clone()],
            Some(region),
            Some(line),
        ));
        let mut spec = TextSpec::new("AB\nC", text::DEFAULT_FONT, 12.0);
        spec.transform = Transform::translation(150.0, 700.0);
        spec.align = TextAlign::Center;
        let mut satin = RegionSewing::new(RegionType::Satin, Thread::rgb(0x00ff00));
        satin.underlay = false;
        let (t, _) = DesignObject::text(spec.clone(), Some(satin), None).expect("layout");
        d.add(t);
        let stitches = vec![
            Stitch::new(600.0, 600.0, Command::Stitch),
            Stitch::new(650.0, 620.0, Command::Stitch),
            Stitch::new(700.0, 600.0, Command::Stitch),
        ];
        d.add(DesignObject::stitches(StitchBlock {
            stitches: stitches.clone(),
            threads: vec![Thread::rgb(0xff00ff)],
        }));
        let pes = crate::pes11_export::objects(&d).expect("objects");
        let section = ObjectSection {
            version: "0110".into(),
            threads: pes.threads.clone(),
            shapes: pes.shapes.clone(),
            texts: pes.texts.clone(),
            stitch_groups: pes.stitch_groups.clone(),
            ..ObjectSection::default()
        };
        let i = objects(&section);
        assert!(i.skipped.is_empty() && i.notes.is_empty(), "{i:?}");
        let o = &i.objects[0];
        assert_eq!(o.contours[0].points, square.points);
        let r = o.region.as_ref().expect("region");
        assert_eq!((r.kind, r.angle, r.density), (RegionType::Fill, 30.0, 5.0));
        assert_eq!(r.thread.rgb_value(), 0x0000ff);
        let l = o.line.as_ref().expect("line");
        assert_eq!(
            (l.kind, l.thread.rgb_value()),
            (LineType::Running, 0xff0000)
        );
        let back = i.objects[1].text.as_ref().expect("text");
        assert_eq!(
            (back.text.as_str(), back.size, back.align, back.transform),
            ("AB\nC", 12.0, TextAlign::Center, spec.transform)
        );
        assert_eq!(i.objects[1].contours.len(), d.objects[1].contours.len());
        let block = i.objects[2].stitch_block.as_ref().expect("stitches");
        let sewn: Vec<(f64, f64)> = block
            .stitches
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .map(|s| (s.x, s.y))
            .collect();
        assert_eq!(
            sewn,
            stitches.iter().map(|s| (s.x, s.y)).collect::<Vec<_>>()
        );
        assert_eq!(block.threads[0].rgb_value(), 0xff00ff);
    }

    #[test]
    fn zigzag_sharp_corners_and_miter_limits_import_and_export_back() {
        // #373: PE-DESIGN 11.50's button 2 capture with 3.0 mm for each button.
        let i = objects(&reference("stitches/line-zigzag-miter-v-b2-m3"));
        let l = i.objects[0].line.as_ref().expect("line");
        assert_eq!(
            (l.sharp_corners.button, l.sharp_corners.miter_limit),
            (2, [3.0, 3.0])
        );
        let mut d = Design::default();
        d.add(i.objects[0].clone());
        let s = &crate::pes11_export::objects(&d).expect("objects").shapes[0];
        assert_eq!((s.sharp_corners, s.miter_limits), (1, [3.0, 3.0]));
        // A new line writes PE-DESIGN's defaults: button 1, 0.0 and 7.0.
        let mut d = Design::default();
        let mut l = l.clone();
        l.sharp_corners = Default::default();
        d.add(DesignObject::shape(
            i.objects[0].contours.clone(),
            None,
            Some(l),
        ));
        let s = &crate::pes11_export::objects(&d).expect("objects").shapes[0];
        assert_eq!((s.sharp_corners, s.miter_limits), (0, [0.0, 7.0]));
    }
}
