//! File-manager thumbnails: the stitches of an `.osd` project or a machine embroidery file
//! (PES, DST, JEF, ... — every format `openstitch-formats` reads), cropped to the design and
//! centred on a transparent square.

use crate::preview::{RunKind, runs};
use openstitch_core::{Command, StitchPattern};
use openstitch_design::Compiler;
use openstitch_formats::FileType;
use std::path::Path;
use tiny_skia::{Color, LineCap, LineJoin, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// Files larger than this are not read (the same cap the engine uses for imports).
pub const MAX_INPUT: u64 = 64 << 20;
pub const MIN_SIZE: u32 = 16;
pub const MAX_SIZE: u32 = 1024;

/// The stitches of `path`: a compiled `.osd` project, or a machine file by its extension.
pub fn load(path: &Path) -> Result<StitchPattern, String> {
    let len = std::fs::metadata(path)
        .map_err(|e| format!("{}: {e}", path.display()))?
        .len();
    if len > MAX_INPUT {
        return Err(format!(
            "{} is larger than {} MiB",
            path.display(),
            MAX_INPUT >> 20
        ));
    }
    let data = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
    load_bytes(&data, ext).map_err(|e| format!("{}: {e}", path.display()))
}

/// The stitches of a file's contents, its format named by `extension` (without the dot):
/// `osd` for a project, else a machine format. For callers that hand over the bytes instead
/// of a path, such as the sandboxed macOS Quick Look extension (#557).
pub fn load_bytes(data: &[u8], extension: &str) -> Result<StitchPattern, String> {
    if data.len() as u64 > MAX_INPUT {
        return Err(format!("larger than {} MiB", MAX_INPUT >> 20));
    }
    if extension.eq_ignore_ascii_case(openstitch_engine::project::EXTENSION) {
        let design = openstitch_engine::project::decode(data).map_err(|e| e.to_string())?;
        return Ok(Compiler::for_design(&design).compile(&design));
    }
    let ty = FileType::from_extension(extension).ok_or("not an embroidery file")?;
    ty.read(data).map_err(|e| e.to_string())
}

/// A `size`×`size` RGBA PNG of the sewn stitches, scaled to fit with a small margin. Travel is
/// left out; thread is drawn at about 0.4 mm, but never thinner than one pixel.
pub fn png(pattern: &StitchPattern, size: u32) -> Result<Vec<u8>, String> {
    let size = size.clamp(MIN_SIZE, MAX_SIZE);
    let pixmap =
        stitches_pixmap(pattern, size).ok_or_else(|| format!("cannot create a {size}px bitmap"))?;
    encode_rgba(&pixmap)
}

/// [`png`]'s picture as straight-alpha RGBA bytes, `size`×`size` (clamped to
/// [`MIN_SIZE`]..=[`MAX_SIZE`]) row by row, transparent where no thread is: the editor's small
/// picture of an object in the Sewing Order list (#1185), drawn as the file-manager thumbnails
/// are. `None` if the bitmap cannot be made.
pub fn stitches_rgba(pattern: &StitchPattern, size: u32) -> Option<Vec<u8>> {
    stitches_pixmap(pattern, size.clamp(MIN_SIZE, MAX_SIZE)).map(|p| straight(&p))
}

/// A shape's outlines as RGBA bytes, like [`stitches_rgba`], for an object that has no stitches
/// to show (hidden, nothing to sew, or failing to compile; #1185): the closed contours filled
/// even-odd in `fill` (inner contours are holes, as the page treats them), then every contour
/// drawn in `line` about 1 px wide. Bézier segments are drawn as curves.
pub fn contours_rgba(
    contours: &[openstitch_design::Contour],
    fill: Option<[u8; 3]>,
    line: [u8; 3],
    size: u32,
) -> Option<Vec<u8>> {
    let size = size.clamp(MIN_SIZE, MAX_SIZE);
    let mut pixmap = Pixmap::new(size, size)?;
    pixmap.fill(Color::TRANSPARENT);
    let pts = contours.iter().flat_map(|c| c.points.iter());
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for p in pts.clone().chain(
        contours
            .iter()
            .flat_map(|c| c.curves.iter().flatten().flat_map(|b| [&b.c1, &b.c2])),
    ) {
        (x0, y0, x1, y1) = (x0.min(p.x), y0.min(p.y), x1.max(p.x), y1.max(p.y));
    }
    if pts.count() == 0 {
        return Some(straight(&pixmap));
    }
    let transform = fit(x0, y0, x1, y1, size);
    let scale = f64::from(transform.sx);
    let path_of = |closed_only: bool| {
        let mut pb = PathBuilder::new();
        for c in contours {
            if c.points.len() < 2 || (closed_only && !c.closed) {
                continue;
            }
            let p0 = c.points[0];
            pb.move_to(p0.x as f32, p0.y as f32);
            let n = c.points.len();
            let segments = if c.closed { n } else { n - 1 };
            for i in 0..segments {
                let b = c.points[(i + 1) % n];
                match c.curves.get(i).copied().flatten() {
                    Some(z) => pb.cubic_to(
                        z.c1.x as f32,
                        z.c1.y as f32,
                        z.c2.x as f32,
                        z.c2.y as f32,
                        b.x as f32,
                        b.y as f32,
                    ),
                    None => pb.line_to(b.x as f32, b.y as f32),
                }
            }
            if c.closed {
                pb.close();
            }
        }
        pb.finish()
    };
    let mut paint = Paint {
        anti_alias: true,
        ..Paint::default()
    };
    if let (Some(rgb), Some(path)) = (fill, path_of(true)) {
        paint.set_color_rgba8(rgb[0], rgb[1], rgb[2], 255);
        pixmap.fill_path(&path, &paint, tiny_skia::FillRule::EvenOdd, transform, None);
    }
    if let Some(path) = path_of(false) {
        paint.set_color_rgba8(line[0], line[1], line[2], 255);
        let stroke = Stroke {
            width: (1.0 / scale) as f32,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        pixmap.stroke_path(&path, &paint, &stroke, transform, None);
    }
    Some(straight(&pixmap))
}

/// Model units to a `size` px square: the box centred, with a 4 % margin each side.
fn fit(x0: f64, y0: f64, x1: f64, y1: f64, size: u32) -> Transform {
    let extent = (x1 - x0).max(y1 - y0).max(1.0);
    let margin = f64::from(size) * 0.04;
    let scale = (f64::from(size) - 2.0 * margin) / extent;
    let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
    let half = f64::from(size) / 2.0;
    Transform::from_row(
        scale as f32,
        0.0,
        0.0,
        scale as f32,
        (half - cx * scale) as f32,
        (half - cy * scale) as f32,
    )
}

fn stitches_pixmap(pattern: &StitchPattern, size: u32) -> Option<Pixmap> {
    let mut pixmap = Pixmap::new(size, size)?;
    pixmap.fill(Color::TRANSPARENT);
    if pattern.count(Command::Stitch) > 0 {
        let b = pattern.bounds();
        let transform = fit(b.min_x, b.min_y, b.max_x, b.max_y, size);
        let scale = f64::from(transform.sx);
        let stroke = Stroke {
            width: ((4.0 * scale).max(1.0) / scale) as f32,
            line_cap: LineCap::Round,
            line_join: LineJoin::Round,
            ..Stroke::default()
        };
        let threads: Vec<_> = pattern.color_blocks().into_iter().map(|(t, _)| t).collect();
        let mut paint = Paint {
            anti_alias: true,
            ..Paint::default()
        };
        for run in runs(pattern)
            .into_iter()
            .filter(|r| r.kind == RunKind::Sewn)
        {
            let mut pb = PathBuilder::new();
            let mut points = run.points.iter();
            let Some(&(x, y)) = points.next() else {
                continue;
            };
            pb.move_to(x as f32, y as f32);
            for &(x, y) in points {
                pb.line_to(x as f32, y as f32);
            }
            let Some(path) = pb.finish() else { continue };
            let t = threads.get(run.block).copied().flatten();
            paint.set_color_rgba8(
                t.map_or(0, |t| t.red),
                t.map_or(0, |t| t.green),
                t.map_or(0, |t| t.blue),
                255,
            );
            pixmap.stroke_path(&path, &paint, &stroke, transform, None);
        }
    }
    Some(pixmap)
}

/// Straight-alpha RGBA bytes from tiny-skia's premultiplied pixels.
fn straight(pixmap: &Pixmap) -> Vec<u8> {
    pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect()
}

/// Straight-alpha RGBA PNG from tiny-skia's premultiplied pixels.
fn encode_rgba(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
    let rgba = straight(pixmap);
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder
        .write_header()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    writer
        .write_image_data(&rgba)
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    writer
        .finish()
        .map_err(|e| format!("PNG encoding failed: {e}"))?;
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_core::Thread;

    fn square() -> StitchPattern {
        let mut p = StitchPattern::new();
        p.threads.push(Thread::rgb(0xFF0000));
        p.jump_to(0.0, 0.0);
        for (x, y) in [
            (0.0, 0.0),
            (200.0, 0.0),
            (200.0, 100.0),
            (0.0, 100.0),
            (0.0, 0.0),
        ] {
            p.stitch_to(x, y);
        }
        p.push(Command::End);
        p
    }

    fn decode(bytes: &[u8]) -> (u32, u32, Vec<u8>) {
        let mut reader = png::Decoder::new(std::io::Cursor::new(bytes))
            .read_info()
            .expect("png");
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("frame");
        assert_eq!(info.color_type, png::ColorType::Rgba);
        buf.truncate(info.buffer_size());
        (info.width, info.height, buf)
    }

    #[test]
    fn thumbnail_is_square_transparent_and_draws_threads() {
        let (w, h, px) = decode(&png(&square(), 128).expect("png"));
        assert_eq!((w, h), (128, 128));
        let at = |x: u32, y: u32| &px[((y * w + x) * 4) as usize..][..4];
        assert_eq!(at(64, 64)[3], 0, "inside the outline is empty");
        assert_eq!(at(0, 0)[3], 0, "corner is transparent");
        // The left edge of the 2:1 rectangle sits at the margin, vertically centred.
        let edge = (0..20)
            .map(|x| at(x, 64))
            .find(|p| p[3] == 255)
            .expect("edge");
        assert_eq!(&edge[..3], &[255, 0, 0]);
    }

    #[test]
    fn contours_are_filled_even_odd_with_the_line_on_top() {
        use openstitch_design::Contour;
        let at = |px: &[u8], size: u32, x: u32, y: u32| -> [u8; 4] {
            px[((y * size + x) * 4) as usize..][..4]
                .try_into()
                .expect("4")
        };
        let red = [200, 0, 0];
        let grey = [90, 90, 90];
        // A filled square with a square hole: the ring is red, the hole stays open.
        let outer = Contour::rectangle(0.0, 0.0, 300.0, 300.0);
        let hole = Contour::rectangle(100.0, 100.0, 100.0, 100.0);
        let px = contours_rgba(&[outer.clone(), hole], Some(red), grey, 60).expect("rgba");
        assert_eq!(at(&px, 60, 30, 30)[3], 0, "the hole is open");
        assert_eq!(at(&px, 60, 12, 30), [200, 0, 0, 255], "the ring is filled");
        // Line only: nothing inside, the outline drawn.
        let px = contours_rgba(std::slice::from_ref(&outer), None, grey, 60).expect("rgba");
        assert_eq!(at(&px, 60, 30, 30)[3], 0, "no fill without a region");
        assert!(
            px.chunks(4).any(|p| p[3] > 0 && p[..3] == grey),
            "the line shows"
        );
        // A curve is drawn as a curve: a segment bowing out passes well outside its chord.
        let mut bow = Contour::new(
            vec![
                openstitch_design::v2(0.0, 0.0),
                openstitch_design::v2(300.0, 0.0),
            ],
            false,
        );
        bow.curves = vec![Some(openstitch_design::Bezier {
            c1: openstitch_design::v2(0.0, 300.0),
            c2: openstitch_design::v2(300.0, 300.0),
        })];
        let px = contours_rgba(&[bow], None, grey, 60).expect("rgba");
        let lowest = (0..60u32)
            .rev()
            .find(|&y| (0..60).any(|x| at(&px, 60, x, y)[3] > 0))
            .expect("drawn");
        let highest = (0..60u32)
            .find(|&y| (0..60).any(|x| at(&px, 60, x, y)[3] > 0))
            .expect("drawn");
        assert!(lowest - highest > 30, "{highest}..{lowest}");
        assert!(contours_rgba(&[], Some(red), grey, 60).is_some_and(|p| p.iter().all(|&b| b == 0)));
    }

    #[test]
    fn stitches_rgba_draws_what_png_draws() {
        let rgba = stitches_rgba(&square(), 64).expect("rgba");
        let (_, _, px) = decode(&png(&square(), 64).expect("png"));
        assert_eq!(rgba, px);
    }

    #[test]
    fn size_is_clamped_and_empty_patterns_render_blank() {
        let (w, _, px) = decode(&png(&StitchPattern::new(), 4).expect("png"));
        assert_eq!(w, MIN_SIZE);
        assert!(px.chunks(4).all(|p| p[3] == 0));
        let (w, _, _) = decode(&png(&square(), 100_000).expect("png"));
        assert_eq!(w, MAX_SIZE);
    }

    #[test]
    fn loads_projects_and_machine_files_and_rejects_others() {
        let dir = openstitch_testutil::TempDir::new("thumb");
        let pes = dir.join("a.pes");
        std::fs::write(&pes, FileType::Pes.write(&square()).expect("pes")).expect("write");
        assert!(load(&pes).expect("pes").count(Command::Stitch) >= 5);

        let mut design = openstitch_design::Design::default();
        design.add(openstitch_design::DesignObject::shape(
            vec![openstitch_design::Contour::rectangle(
                100.0, 100.0, 200.0, 200.0,
            )],
            Some(openstitch_design::RegionSewing::new(
                openstitch_design::RegionType::Fill,
                Thread::rgb(0x00AA00),
            )),
            None,
        ));
        let osd = dir.join("b.osd");
        std::fs::write(
            &osd,
            openstitch_engine::project::encode(&design).expect("osd"),
        )
        .expect("write");
        assert!(load(&osd).expect("osd").count(Command::Stitch) > 50);

        let txt = dir.join("c.txt");
        std::fs::write(&txt, "hello").expect("write");
        assert!(load(&txt).is_err());
        std::fs::write(dir.join("d.pes"), b"#PES0001 garbage").expect("write");
        assert!(load(&dir.join("d.pes")).is_err());
        assert!(load(&dir.join("missing.pes")).is_err());
        // The same from bytes, the Quick Look extension's way (#557).
        let bytes = std::fs::read(&pes).expect("read");
        assert_eq!(
            load_bytes(&bytes, "PES").expect("pes bytes"),
            load(&pes).expect("pes")
        );
        assert!(load_bytes(b"hello", "txt").is_err());
        assert!(load_bytes(b"#PES0001 garbage", "pes").is_err());
    }
}
