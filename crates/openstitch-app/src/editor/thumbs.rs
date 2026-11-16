//! The small picture of an object in the Sewing Order list (#1185): what the page shows of it,
//! its own stitches cut out of the design's compiled pattern, drawn as the file-manager
//! thumbnails draw stitches (`openstitch_render::thumbnail`). An object with no stitches to show
//! (hidden, nothing to sew, or failing to compile) is drawn from its outlines instead: filled
//! even-odd in its region colour, its line on top. The editor caches the pictures as textures
//! (`thumbs_ui`); this module is the part without egui.

use openstitch_core::{Command, Stitch, StitchPattern, Thread};
use openstitch_design::DesignObject;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::ops::Range;

/// Line colour of an outline drawn without a line sewing (the page's grey for unsewn outlines).
pub const NO_LINE: [u8; 3] = [90, 90, 90];

/// Each object's stitch ranges in the compiled pattern, from the compiler's trace (one object
/// may own several ranges).
pub fn ranges_by_object(spans: &[(u64, Range<usize>)]) -> HashMap<u64, Vec<Range<usize>>> {
    let mut out: HashMap<u64, Vec<Range<usize>>> = HashMap::new();
    for (id, r) in spans {
        out.entry(*id).or_default().push(r.clone());
    }
    out
}

/// The index of each colour change in `pattern`, in order: the colour block a stitch is in is
/// the number of these before it.
pub fn colour_changes(pattern: &StitchPattern) -> Vec<usize> {
    pattern
        .stitches
        .iter()
        .enumerate()
        .filter(|(_, s)| s.command == Command::ColorChange)
        .map(|(i, _)| i)
        .collect()
}

/// An object's own stitches as a pattern of their own: each range from a jump to its first
/// point, in the threads the design sews them in. Empty when the ranges hold no stitch.
pub fn object_pattern(
    pattern: &StitchPattern,
    changes: &[usize],
    ranges: &[Range<usize>],
) -> StitchPattern {
    let mut out = StitchPattern::new();
    let mut block: Option<usize> = None;
    for r in ranges {
        let r = r.start.min(pattern.stitches.len())..r.end.min(pattern.stitches.len());
        let mut b = changes.partition_point(|&c| c < r.start);
        let mut first = true;
        for s in &pattern.stitches[r] {
            match s.command {
                Command::ColorChange => {
                    b += 1;
                    continue;
                }
                Command::End => continue,
                // Travel and cuts before the object's first stitch in a colour block (the move
                // from the object before, still in that one's colour) are not its own.
                Command::Stitch => {}
                _ if block != Some(b) => continue,
                _ => {}
            }
            if block != Some(b) {
                if block.is_some() {
                    out.stitches
                        .push(Stitch::new(s.x, s.y, Command::ColorChange));
                }
                out.threads.push(thread(pattern, b));
                block = Some(b);
            }
            if first {
                out.stitches.push(Stitch::new(s.x, s.y, Command::Jump));
                first = false;
            }
            out.stitches.push(*s);
        }
    }
    out
}

fn thread(pattern: &StitchPattern, block: usize) -> Thread {
    pattern.threads.get(block).cloned().unwrap_or_else(|| {
        let [r, g, b] = crate::editor::render::UNKNOWN_THREAD;
        Thread::rgb((u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b))
    })
}

/// What a picture is drawn from, and a key that changes whenever the picture would.
#[derive(Clone, Debug, PartialEq)]
pub enum Source {
    /// The object's stitches.
    Stitches(StitchPattern),
    /// Its outlines: closed ones filled in `fill` (even-odd), all drawn in `line`.
    Outlines {
        fill: Option<[u8; 3]>,
        line: [u8; 3],
    },
}

impl Source {
    /// What to draw for `o`, whose compiled stitches (from [`object_pattern`]) are `sewn`.
    pub fn of(o: &DesignObject, sewn: StitchPattern) -> Source {
        if sewn.count(Command::Stitch) > 0 {
            return Source::Stitches(sewn);
        }
        // Hidden objects are not compiled; a stitch object still has its own needle points.
        if let Some(block) = &o.stitch_block {
            let p = StitchPattern {
                stitches: block.stitches.clone(),
                threads: block.threads.clone(),
                metadata: Default::default(),
            };
            if p.count(Command::Stitch) > 0 {
                return Source::Stitches(p);
            }
        }
        let rgb = |t: &Thread| [t.red, t.green, t.blue];
        Source::Outlines {
            fill: o.region.as_ref().map(|r| rgb(&r.thread)),
            line: o.line.as_ref().map_or(NO_LINE, |l| rgb(&l.thread)),
        }
    }

    /// The colours the picture is drawn in.
    pub fn colours(&self) -> Vec<[u8; 3]> {
        match self {
            Source::Stitches(p) => p.threads.iter().map(|t| [t.red, t.green, t.blue]).collect(),
            Source::Outlines { fill, line } => fill.iter().copied().chain([*line]).collect(),
        }
    }

    /// A hash of everything the picture of `o` depends on, with the pixel size.
    pub fn key(&self, o: &DesignObject, size: u32) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        size.hash(&mut h);
        match self {
            Source::Stitches(p) => {
                0u8.hash(&mut h);
                for s in &p.stitches {
                    s.x.to_bits().hash(&mut h);
                    s.y.to_bits().hash(&mut h);
                    (s.command as u8).hash(&mut h);
                }
                for t in &p.threads {
                    [t.red, t.green, t.blue].hash(&mut h);
                }
            }
            Source::Outlines { fill, line } => {
                1u8.hash(&mut h);
                (fill, line).hash(&mut h);
                for c in &o.contours {
                    c.closed.hash(&mut h);
                    for p in &c.points {
                        p.x.to_bits().hash(&mut h);
                        p.y.to_bits().hash(&mut h);
                    }
                    for b in c.curves.iter().flatten() {
                        for v in [b.c1.x, b.c1.y, b.c2.x, b.c2.y] {
                            v.to_bits().hash(&mut h);
                        }
                    }
                }
            }
        }
        h.finish()
    }

    /// Straight-alpha RGBA, `size`×`size`, transparent where nothing is drawn.
    pub fn draw(&self, o: &DesignObject, size: u32) -> Option<Vec<u8>> {
        match self {
            Source::Stitches(p) => openstitch_render::thumbnail::stitches_rgba(p, size),
            Source::Outlines { fill, line } => {
                openstitch_render::thumbnail::contours_rgba(&o.contours, *fill, *line, size)
            }
        }
    }
}

/// Whether a picture in these colours needs a darker tile than white to be seen: every colour is
/// within WCAG contrast 1.5 of white (white or cream thread on a white tile). The swatches' grey
/// edge alone does not show a white object's shape (#1185).
pub fn needs_dark_tile(colours: &[[u8; 3]]) -> bool {
    !colours.is_empty()
        && colours
            .iter()
            .all(|&c| crate::editor::accent::contrast(c, [255, 255, 255]) < 1.5)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::{
        Compiler, Contour, Design, LineSewing, LineType, RegionSewing, RegionType,
    };

    fn compiled(d: &Design, id: u64) -> StitchPattern {
        let (pattern, spans) = Compiler::for_design(d).compile_traced(d);
        let ranges = ranges_by_object(&spans);
        object_pattern(
            &pattern,
            &colour_changes(&pattern),
            ranges.get(&id).map_or(&[][..], Vec::as_slice),
        )
    }

    fn pixel(px: &[u8], size: u32, x: u32, y: u32) -> [u8; 4] {
        px[((y * size + x) * 4) as usize..][..4]
            .try_into()
            .expect("4")
    }

    fn red() -> Thread {
        Thread::rgb(0xC81E1E)
    }

    fn shape(contours: Vec<Contour>, region: bool, line: bool) -> DesignObject {
        DesignObject::shape(
            contours,
            region.then(|| RegionSewing::new(RegionType::Fill, red())),
            line.then(|| LineSewing::new(LineType::Running, Thread::rgb(0x000000))),
        )
    }

    fn picture(o: DesignObject) -> Vec<u8> {
        let mut d = Design::default();
        let id = d.add(o);
        let o = &d.objects[d.index_of(id).expect("added")];
        let src = Source::of(o, compiled(&d, id));
        src.draw(o, 60).expect("drawn")
    }

    #[test]
    fn a_filled_rectangle_is_filled_and_a_line_only_one_is_not() {
        let rect = || Contour::rectangle(100.0, 100.0, 400.0, 300.0);
        let px = picture(shape(vec![rect()], true, false));
        let c = pixel(&px, 60, 30, 30);
        assert!(
            c[3] > 200 && c[0] > 150 && c[1] < 80,
            "the fill colour at the centre: {c:?}"
        );
        let px = picture(shape(vec![rect()], false, true));
        assert_eq!(
            pixel(&px, 60, 30, 30)[3],
            0,
            "a line-only rectangle is empty inside"
        );
        assert!(px.chunks(4).any(|p| p[3] > 0), "its line is drawn");
    }

    #[test]
    fn a_hole_stays_open() {
        let outer = Contour::rectangle(0.0, 0.0, 600.0, 600.0);
        let hole = Contour::rectangle(200.0, 200.0, 200.0, 200.0);
        let px = picture(shape(vec![outer, hole], true, false));
        assert_eq!(pixel(&px, 60, 30, 30)[3], 0, "the hole is open");
        assert!(pixel(&px, 60, 10, 30)[3] > 200, "the ring is sewn");
    }

    #[test]
    fn a_hidden_object_is_drawn_from_its_outlines() {
        let mut o = shape(
            vec![Contour::rectangle(0.0, 0.0, 400.0, 400.0)],
            true,
            false,
        );
        o.visible = false;
        let mut d = Design::default();
        let id = d.add(o);
        let o = &d.objects[d.index_of(id).expect("added")];
        let src = Source::of(o, compiled(&d, id));
        assert!(
            matches!(src, Source::Outlines { fill: Some(_), .. }),
            "{src:?}"
        );
        let px = src.draw(o, 60).expect("drawn");
        assert_eq!(
            &pixel(&px, 60, 30, 30)[..3],
            &[0xC8, 0x1E, 0x1E],
            "filled, not blank"
        );
    }

    #[test]
    fn an_object_of_two_colours_is_cut_out_in_both() {
        let mut d = Design::default();
        let a = d.add(shape(
            vec![Contour::rectangle(0.0, 0.0, 300.0, 300.0)],
            true,
            false,
        ));
        let mut o = shape(
            vec![Contour::rectangle(400.0, 0.0, 300.0, 300.0)],
            true,
            true,
        );
        o.region = Some(RegionSewing::new(RegionType::Fill, Thread::rgb(0x1E64C8)));
        let b = d.add(o);
        let sewn = compiled(&d, b);
        let rgb: Vec<_> = sewn
            .threads
            .iter()
            .map(|t| [t.red, t.green, t.blue])
            .collect();
        assert_eq!(
            rgb,
            vec![[0x1E, 0x64, 0xC8], [0, 0, 0]],
            "its own fill and line, not a's red"
        );
        let first = compiled(&d, a);
        assert_eq!(first.threads.len(), 1);
        assert!(
            sewn.stitches.iter().all(|s| s.x >= 390.0),
            "only b's needle points"
        );
    }

    #[test]
    fn the_key_changes_with_the_stitches_the_colours_and_the_size() {
        let mut d = Design::default();
        let id = d.add(shape(
            vec![Contour::rectangle(0.0, 0.0, 300.0, 300.0)],
            true,
            false,
        ));
        let o = d.objects[0].clone();
        let src = Source::of(&o, compiled(&d, id));
        let k = src.key(&o, 60);
        assert_eq!(k, Source::of(&o, compiled(&d, id)).key(&o, 60), "stable");
        assert_ne!(k, src.key(&o, 90), "a new display scale draws again");
        d.objects[0].region = Some(RegionSewing::new(RegionType::Fill, Thread::rgb(0x00FF00)));
        let o2 = d.objects[0].clone();
        assert_ne!(
            k,
            Source::of(&o2, compiled(&d, id)).key(&o2, 60),
            "a new colour"
        );
    }

    #[test]
    fn only_light_pictures_get_a_dark_tile() {
        assert!(needs_dark_tile(&[[255, 255, 255]]));
        assert!(needs_dark_tile(&[[250, 245, 230], [255, 255, 255]]));
        assert!(
            !needs_dark_tile(&[[255, 255, 255], [200, 0, 0]]),
            "a red part shows"
        );
        assert!(!needs_dark_tile(&[[90, 90, 90]]));
        assert!(!needs_dark_tile(&[]));
    }
}
