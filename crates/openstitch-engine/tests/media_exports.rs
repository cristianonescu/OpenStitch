//! Export extras, part 2 (#488): picture, stitch-out video (animated PNG) and ZIP bundle.

#![cfg(feature = "formats")]

use openstitch_core::Command;
use openstitch_design::Compiler;
use openstitch_engine::Session;
use openstitch_testutil::TempDir;
use serde_json::{Value, json};
use std::io::Read;

fn run(s: &mut Session, id: &str, params: Value) -> Value {
    s.execute(id, Some(&params))
        .unwrap_or_else(|e| panic!("{id}: {e}"))
}

fn design(tag: &str) -> (Session, TempDir) {
    let dir = TempDir::new(&format!("media-exports-{tag}"));
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 20, "fill": "#ff0000"}),
    );
    run(
        &mut s,
        "shape.ellipse",
        json!({"cx": 60, "cy": 25, "rx": 10, "ry": 10, "fill": "#0000ff"}),
    );
    (s, dir)
}

/// One frame: its rectangle in the image and its RGB bytes.
struct Frame {
    x: usize,
    y: usize,
    w: usize,
    rgb: Vec<u8>,
}

/// Width, height and every frame of a PNG or animated PNG (frame 1 is the whole image).
fn frames(bytes: &[u8]) -> (u32, u32, Vec<Frame>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("png");
    let (w, h) = (reader.info().width, reader.info().height);
    let count = reader
        .info()
        .animation_control()
        .map_or(1, |a| a.num_frames as usize);
    let mut out = Vec::new();
    for _ in 0..count {
        let mut buf = vec![0; reader.output_buffer_size().expect("size")];
        let info = reader.next_frame(&mut buf).expect("frame");
        buf.truncate(info.buffer_size());
        let (x, y, fw) = match reader.info().frame_control() {
            Some(fc) => (
                fc.x_offset as usize,
                fc.y_offset as usize,
                fc.width as usize,
            ),
            None => (0, 0, w as usize),
        };
        out.push(Frame {
            x,
            y,
            w: fw,
            rgb: buf,
        });
    }
    (w, h, out)
}

/// The image after showing `frames` in order (each replaces its rectangle).
fn composite(width: usize, height: usize, frames: &[Frame]) -> Vec<u8> {
    let mut img = vec![255; width * height * 3];
    for f in frames {
        for (r, row) in f.rgb.chunks(f.w * 3).enumerate() {
            let at = ((f.y + r) * width + f.x) * 3;
            img[at..at + row.len()].copy_from_slice(row);
        }
    }
    img
}

#[test]
fn a_picture_is_the_design_at_the_asked_size() {
    let (mut s, dir) = design("picture");
    run(
        &mut s,
        "design.exportPicture",
        json!({"path": "p.png", "size": 400}),
    );
    let (w, h, f) = frames(&std::fs::read(dir.join("p.png")).expect("png"));
    assert_eq!(w.max(h), 400);
    assert_eq!(f.len(), 1);
    // Some thread red and blue is drawn on white.
    let px: Vec<&[u8]> = f[0].rgb.chunks(3).collect();
    assert!(px.iter().any(|p| p[0] > 200 && p[1] < 60 && p[2] < 60));
    assert!(px.iter().any(|p| p[2] > 200 && p[0] < 60 && p[1] < 60));
}

#[test]
fn a_video_has_one_frame_per_step_and_ends_on_the_picture() {
    let (mut s, dir) = design("video");
    let pattern = Compiler::for_design(s.design()).compile(s.design());
    let stitches = pattern.count(Command::Stitch);
    let (fps, speed) = (8.0, 250.0);
    let out = run(
        &mut s,
        "design.exportVideo",
        json!({"path": "v.png", "fps": fps, "speed": speed, "size": 300}),
    );
    let expected = (stitches as f64 / speed * fps).ceil() as usize;
    assert_eq!(out["frames"].as_u64(), Some(expected as u64), "{out}");
    let (w, h, video) = frames(&std::fs::read(dir.join("v.png")).expect("apng"));
    assert_eq!(video.len(), expected);
    run(
        &mut s,
        "design.exportPicture",
        json!({"path": "p.png", "size": 300}),
    );
    let (pw, ph, picture) = frames(&std::fs::read(dir.join("p.png")).expect("png"));
    assert_eq!((w, h), (pw, ph));
    let last = &video.last().expect("frame").rgb;
    assert!(last == &picture[0].rgb, "the last frame is the picture");
    let (w, h) = (w as usize, h as usize);
    // The design appears as it is sewn: the first frame shows less thread than the last.
    let inked = |f: &[u8]| f.chunks(3).filter(|p| p != &[255, 255, 255]).count();
    assert!(inked(&video[0].rgb) < inked(last));
    // Halfway through, about half of the thread is on.
    let half = composite(w, h, &video[..video.len() / 2]);
    let ratio = inked(&half) as f64 / inked(last) as f64;
    assert!((0.25..0.75).contains(&ratio), "{ratio}");
    // Too many frames is refused, not written.
    let err = s
        .execute(
            "design.exportVideo",
            Some(&json!({"path": "slow.png", "fps": 60, "speed": 10})),
        )
        .expect_err("too many frames");
    assert!(err.to_string().contains("frames"), "{err}");
    assert!(!dir.join("slow.png").exists());
}

#[test]
fn a_bundle_unpacks_to_the_files_of_the_individual_exports() {
    let (mut s, dir) = design("bundle");
    std::fs::create_dir_all(dir.join("shop")).expect("dir");
    run(
        &mut s,
        "design.exportBundle",
        json!({"path": "shop/badge.zip"}),
    );
    std::fs::create_dir_all(dir.join("single")).expect("dir");
    for ext in ["pes", "dst", "jef"] {
        run(
            &mut s,
            "design.export",
            json!({"path": format!("single/badge.{ext}"), "split": true}),
        );
    }
    run(
        &mut s,
        "design.exportThreads",
        json!({"path": "single/badge threads.csv"}),
    );
    run(
        &mut s,
        "design.exportPicture",
        json!({"path": "single/badge.png"}),
    );

    let file = std::fs::File::open(dir.join("shop/badge.zip")).expect("zip");
    let mut zip = zip::ZipArchive::new(file).expect("archive");
    let mut names: Vec<String> = zip
        .file_names()
        .map(|n| n.expect("name").into_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "badge threads.csv",
            "badge.dst",
            "badge.jef",
            "badge.pes",
            "badge.png"
        ]
    );
    for name in &names {
        let mut inside = Vec::new();
        zip.by_name(name)
            .expect("entry")
            .read_to_end(&mut inside)
            .expect("read");
        let mut single = std::fs::read(dir.join("single").join(name)).expect("single");
        if name.ends_with(".jef") {
            // JEF stamps the time it was written (bytes 8–21); everything else is the same.
            inside[8..22].fill(b'0');
            single[8..22].fill(b'0');
        }
        assert!(
            inside == single,
            "{name} differs from its individual export"
        );
    }
    // No parts are left beside the bundle.
    let left: Vec<_> = std::fs::read_dir(dir.join("shop"))
        .expect("dir")
        .map(|e| e.expect("entry").file_name())
        .collect();
    assert_eq!(left, ["badge.zip"]);
}

#[test]
fn a_bundle_takes_other_formats_and_can_leave_out_the_list_and_picture() {
    let (mut s, dir) = design("bundle-options");
    let out = run(
        &mut s,
        "design.exportBundle",
        json!({"path": "b.zip", "formats": ["vp3", "exp"], "threadList": "inf", "picture": false}),
    );
    assert_eq!(out["files"], json!(["b.vp3", "b.exp", "b threads.inf"]));
    let zip =
        zip::ZipArchive::new(std::fs::File::open(dir.join("b.zip")).expect("zip")).expect("a");
    assert_eq!(zip.len(), 3);
    let err = s
        .execute(
            "design.exportBundle",
            Some(&json!({"path": "c.zip", "formats": ["phc"]})),
        )
        .expect_err("read-only format");
    assert!(err.to_string().contains("phc"), "{err}");
    assert!(!dir.join("c.zip").exists());
}

/// RGBA pixels of a PNG (any colour type expanded).
fn rgba(bytes: &[u8]) -> (usize, usize, Vec<[u8; 4]>) {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND);
    let mut reader = decoder.read_info().expect("png");
    let mut buf = vec![0; reader.output_buffer_size().expect("size")];
    let info = reader.next_frame(&mut buf).expect("frame");
    assert_eq!(
        info.color_type,
        png::ColorType::Rgba,
        "a mockup keeps alpha"
    );
    let px = buf[..info.buffer_size()]
        .chunks_exact(4)
        .map(|p| [p[0], p[1], p[2], p[3]])
        .collect();
    (info.width as usize, info.height as usize, px)
}

/// #454: the design at true size on the garment, in the fabric colour, at the placement.
#[test]
fn a_mockup_puts_the_design_at_true_size_on_the_garment() {
    let dir = TempDir::new("mockup-tshirt");
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 30, "height": 20, "fill": "#ff0000"}),
    );
    let out = run(
        &mut s,
        "design.mockup",
        json!({"path": "m.png", "garment": "tshirt", "color": "#1f3a68"}),
    );
    assert_eq!(out["garment"], "tshirt");
    assert_eq!(out["placement"], "centre");
    assert_eq!(out["fits"], true, "{out}");
    let (w, h, px) = rgba(&std::fs::read(dir.join("m.png")).expect("png"));
    assert_eq!((w, h), (1024, 1024));
    let at = |x: usize, y: usize| px[y * w + x];
    assert_eq!(at(2, 2), [0xf2, 0xf2, 0xf2, 255], "the background");
    // The shirt's hem area is the fabric colour (shaded by the folds).
    let hem = at(512, 880);
    assert!(
        (i32::from(hem[0]) - 0x1f).abs() < 40 && (i32::from(hem[2]) - 0x68).abs() < 50,
        "{hem:?}"
    );
    // The red stitches, centred on the placement and 30 mm wide at the shirt's scale.
    let g = openstitch_engine::mockup::garment("tshirt").expect("tshirt");
    let k = 1024.0 / 768.0;
    let (px_mm, place) = (g.px_per_mm * k, g.placements[0]);
    let red: Vec<(usize, usize)> = (0..h)
        .flat_map(|y| (0..w).map(move |x| (x, y)))
        .filter(|&(x, y)| {
            let p = at(x, y);
            p[0] > 180 && p[1] < 80 && p[2] < 80
        })
        .collect();
    assert!(!red.is_empty());
    let (x0, x1) = (
        red.iter().map(|p| p.0).min().expect("x"),
        red.iter().map(|p| p.0).max().expect("x"),
    );
    let (y0, y1) = (
        red.iter().map(|p| p.1).min().expect("y"),
        red.iter().map(|p| p.1).max().expect("y"),
    );
    let width_mm = (x1 - x0) as f64 / px_mm;
    assert!((width_mm - 30.0).abs() < 3.0, "{width_mm} mm");
    let (cx, cy) = ((x0 + x1) as f64 / 2.0, (y0 + y1) as f64 / 2.0);
    assert!(
        (cx - place.x * k).abs() < 4.0 && (cy - place.y * k).abs() < 4.0,
        "{cx},{cy}"
    );
}

#[test]
fn mockups_offer_placements_transparency_and_say_when_a_design_is_too_big() {
    let dir = TempDir::new("mockup-options");
    let mut s = Session::new(&dir);
    run(
        &mut s,
        "shape.rectangle",
        json!({"x": 10, "y": 10, "width": 150, "height": 40, "fill": "#00aa00"}),
    );
    let cap = run(
        &mut s,
        "design.mockup",
        json!({"path": "cap.png", "garment": "cap", "background": "transparent", "size": 512}),
    );
    assert_eq!(
        cap["fits"], false,
        "150 mm is wider than a cap front: {cap}"
    );
    assert_eq!(cap["suitsMm"], json!([110.0, 55.0]));
    let (w, _, px) = rgba(&std::fs::read(dir.join("cap.png")).expect("png"));
    assert_eq!(w, 512);
    assert_eq!(px[0][3], 0, "transparent around the cap");
    let left = run(
        &mut s,
        "design.mockup",
        json!({"path": "chest.png", "garment": "tshirt", "placement": "leftChest"}),
    );
    assert_eq!(
        (left["placement"].as_str(), left["fits"].as_bool()),
        (Some("leftChest"), Some(false))
    );
    run(
        &mut s,
        "design.mockup",
        json!({"path": "tote.png", "garment": "tote"}),
    );
    for bad in [
        json!({"path": "x.png", "garment": "sock"}),
        json!({"path": "x.png", "garment": "cap", "placement": "leftChest"}),
        json!({"path": "x.png", "color": "red"}),
        json!({"path": "x.png", "background": "#12"}),
        json!({"path": "x.png", "size": 100}),
    ] {
        assert!(s.execute("design.mockup", Some(&bad)).is_err(), "{bad}");
    }
    assert!(!dir.join("x.png").exists());
    // Every placement is on the garment picture.
    for g in openstitch_engine::mockup::GARMENTS {
        let place = g.placements[0];
        assert!(place.x > 0.0 && place.x < 768.0 && place.y > 0.0 && place.y < 768.0);
    }
    let mut empty = Session::new(&dir);
    assert!(
        empty
            .execute("design.mockup", Some(&json!({"path": "e.png"})))
            .is_err()
    );
}
