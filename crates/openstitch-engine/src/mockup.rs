//! Garment mockups (#454): the design's stitches drawn at true size on a T-shirt, cap or tote
//! bag picture in a chosen fabric colour, written as a PNG to show a customer before sewing.
//!
//! The garment pictures are OpenStitch's own (generated with Higgsfield, `assets/mockups`, with
//! their job ids in `higgsfield-jobs.tsv`): white garments cut out on a transparent background,
//! stored as grey + alpha, 768 × 768 px. Each is tinted by multiplying its grey with the fabric
//! colour, so folds and seams stay. Each garment has a known scale (pixels per mm in its
//! picture, from the garment's real size) and named placements: the point the design's centre
//! goes to and the largest design that suits it there. The stitches are the flat preview's
//! thread strokes (`crate::preview`) at the garment's scale, over a soft shadow in the fabric's
//! own darker colour.

use crate::commands::{CommandSpec, Effect, PATH, opt, spec};
use crate::preview::{self, View};
use crate::sandbox::Access;
use openstitch_core::{Command, StitchPattern};
use openstitch_design::{Compiler, Design};
use serde_json::{Value, json};
use tiny_skia::{FilterQuality, IntSize, Pixmap, PixmapPaint, Transform};

/// Side of every garment picture, in pixels.
const PICTURE_PX: f64 = 768.0;

/// Where a design goes on a garment: its centre at (`x`, `y`) in the garment picture's
/// pixels, at most `max_w` × `max_h` mm to suit that place.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub key: &'static str,
    pub x: f64,
    pub y: f64,
    pub max_w: f64,
    pub max_h: f64,
}

/// A garment picture and its scale.
#[derive(Clone, Copy, Debug)]
pub struct Garment {
    pub key: &'static str,
    pub label: &'static str,
    png: &'static [u8],
    /// Picture pixels per millimetre of the real garment.
    pub px_per_mm: f64,
    /// The first is the default.
    pub placements: &'static [Placement],
}

/// The garments. Scales come from real sizes: an adult M T-shirt 520 mm across the chest
/// (636 px of the 1024 px original); a cap front 115 mm from the brim seam to the button
/// (550 px); a tote bag 380 mm wide (612 px); the 768 px pictures are those scaled by 0.75.
pub const GARMENTS: &[Garment] = &[
    Garment {
        key: "tshirt",
        label: "T-shirt",
        png: include_bytes!("../../../assets/mockups/tshirt.png"),
        px_per_mm: 636.0 / 520.0 * 0.75,
        placements: &[
            // 150 mm below the collar, on the centre line.
            Placement {
                key: "centre",
                x: 384.0,
                y: 142.5 + 150.0 * (636.0 / 520.0 * 0.75),
                max_w: 280.0,
                max_h: 300.0,
            },
            // The wearer's left chest (the picture's right): 95 mm off centre, 80 mm down.
            Placement {
                key: "leftChest",
                x: 384.0 + 95.0 * (636.0 / 520.0 * 0.75),
                y: 142.5 + 80.0 * (636.0 / 520.0 * 0.75),
                max_w: 100.0,
                max_h: 100.0,
            },
        ],
    },
    Garment {
        key: "cap",
        label: "Cap",
        png: include_bytes!("../../../assets/mockups/cap.png"),
        px_per_mm: 550.0 / 115.0 * 0.75,
        // The front panel, its centre 30 mm above the brim seam.
        placements: &[Placement {
            key: "centre",
            x: 384.0,
            y: 468.75 - 30.0 * (550.0 / 115.0 * 0.75),
            max_w: 110.0,
            max_h: 55.0,
        }],
    },
    Garment {
        key: "tote",
        label: "Tote bag",
        png: include_bytes!("../../../assets/mockups/tote.png"),
        px_per_mm: 612.0 / 380.0 * 0.75,
        // The middle of the bag's body, below the handles.
        placements: &[Placement {
            key: "centre",
            x: 379.5,
            y: 503.25,
            max_w: 280.0,
            max_h: 300.0,
        }],
    },
];

pub fn garment(key: &str) -> Option<&'static Garment> {
    GARMENTS.iter().find(|g| g.key == key)
}

/// `#rrggbb` as RGB.
pub fn parse_colour(s: &str) -> Option<[u8; 3]> {
    let h = s.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
    Some([c(0)?, c(2)?, c(4)?])
}

/// What [`compose`] made: the PNG and how the design sits.
#[derive(Clone, Debug, PartialEq)]
pub struct Mockup {
    pub png: Vec<u8>,
    pub size: u32,
    /// The design's stitched width and height, mm.
    pub design_mm: (f64, f64),
    /// Whether it is no larger than the placement suits.
    pub fits: bool,
}

/// The garment picture tinted with `fabric`, premultiplied RGBA at its own size.
fn tinted(g: &Garment, fabric: [u8; 3]) -> Result<Pixmap, String> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(g.png));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    let mut buf = vec![
        0;
        reader
            .output_buffer_size()
            .ok_or("garment picture too large")?
    ];
    let info = reader.next_frame(&mut buf).map_err(|e| e.to_string())?;
    let channels = info.color_type.samples();
    let (w, h) = (info.width, info.height);
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for px in buf[..info.buffer_size()].chunks_exact(channels) {
        let (grey, alpha) = match channels {
            2 => (px[0], px[1]),
            4 => (px[0], px[3]),
            1 => (px[0], 255),
            _ => (px[0], 255),
        };
        let a = u32::from(alpha);
        for c in fabric {
            // Multiply, then premultiply by alpha.
            let v = u32::from(c) * u32::from(grey) / 255;
            rgba.push((v * a / 255) as u8);
        }
        rgba.push(alpha);
    }
    let size = IntSize::from_wh(w, h).ok_or("empty garment picture")?;
    Pixmap::from_vec(rgba, size).ok_or_else(|| "garment picture is not RGBA".into())
}

/// The pattern with every thread `colour` (for the shadow pass).
fn recoloured(pattern: &StitchPattern, colour: [u8; 3]) -> StitchPattern {
    let mut p = pattern.clone();
    for t in &mut p.threads {
        (t.red, t.green, t.blue) = (colour[0], colour[1], colour[2]);
    }
    p
}

/// Draws `pattern` on `garment` at `placement` in `fabric` colour, `size` pixels square, on
/// `background` (`None` = transparent).
pub fn compose(
    pattern: &StitchPattern,
    garment: &Garment,
    placement: &Placement,
    fabric: [u8; 3],
    background: Option<[u8; 3]>,
    size: u32,
) -> Result<Mockup, String> {
    if pattern.count(Command::Stitch) == 0 {
        return Err("design has no stitches".into());
    }
    let mut out =
        Pixmap::new(size, size).ok_or_else(|| format!("cannot create a {size}px picture"))?;
    if let Some([r, g, b]) = background {
        out.fill(tiny_skia::Color::from_rgba8(r, g, b, 255));
    }
    let k = f64::from(size) / PICTURE_PX;
    out.draw_pixmap(
        0,
        0,
        tinted(garment, fabric)?.as_ref(),
        &PixmapPaint {
            quality: FilterQuality::Bicubic,
            ..PixmapPaint::default()
        },
        Transform::from_scale(k as f32, k as f32),
        None,
    );
    // Design units are 0.1 mm.
    let scale = garment.px_per_mm * k / 10.0;
    let b = pattern.bounds();
    let (cx, cy) = ((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0);
    let view = |dx: f64, dy: f64| View {
        min_x: cx - (placement.x * k + dx) / scale,
        min_y: cy - (placement.y * k + dy) / scale,
        scale,
        width: size,
        height: size,
    };
    // A shadow 0.3 mm down and right, in the fabric darkened.
    let off = 0.3 * garment.px_per_mm * k;
    let shade = fabric.map(|c| (f64::from(c) * 0.55) as u8);
    preview::stroke_runs(
        &mut out,
        &recoloured(pattern, shade),
        &view(off, off),
        false,
    );
    preview::stroke_runs(&mut out, pattern, &view(0.0, 0.0), false);
    let design_mm = (b.width() / 10.0, b.height() / 10.0);
    Ok(Mockup {
        png: encode_rgba(&out)?,
        size,
        design_mm,
        fits: design_mm.0 <= placement.max_w + 1e-9 && design_mm.1 <= placement.max_h + 1e-9,
    })
}

/// PNG with alpha (the pixmap is premultiplied; PNG is not).
fn encode_rgba(pixmap: &Pixmap) -> Result<Vec<u8>, String> {
    let data: Vec<u8> = pixmap
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    let err = |e: png::EncodingError| format!("PNG encoding failed: {e}");
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, pixmap.width(), pixmap.height());
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(err)?;
    writer.write_image_data(&data).map_err(err)?;
    writer.finish().map_err(err)?;
    Ok(out)
}

fn design_pattern(design: &Design) -> StitchPattern {
    Compiler::for_design(design).compile(design)
}

pub fn commands() -> Vec<CommandSpec> {
    vec![spec(
        "design.mockup",
        "Write a PNG of the design at true size on a garment (T-shirt, cap or tote bag) in a fabric colour, to show before sewing; the result says whether the design suits the placement's size",
        Effect::File,
        vec![
            PATH,
            opt("garment", "string", "tshirt, cap or tote (default tshirt)"),
            opt(
                "placement",
                "string",
                "Where the design's centre goes: tshirt centre or leftChest, cap centre, tote centre (default the first)",
            ),
            opt("color", "string", "Fabric colour #rrggbb (default #ffffff)"),
            opt(
                "background",
                "string",
                "Behind the garment: #rrggbb or transparent (default #f2f2f2)",
            ),
            opt(
                "size",
                "int",
                "Picture side in pixels, 256–4096 (default 1024)",
            ),
        ],
        |s, p| {
            let path = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
            let key = p.string("garment", Some("tshirt"))?;
            let g = garment(&key)
                .ok_or_else(|| p.invalid("garment", format!("takes {}", garment_keys())))?;
            let placement = match p.has("placement") {
                false => &g.placements[0],
                true => {
                    let k = p.string("placement", None)?;
                    g.placements.iter().find(|pl| pl.key == k).ok_or_else(|| {
                        let keys: Vec<_> = g.placements.iter().map(|pl| pl.key).collect();
                        p.invalid(
                            "placement",
                            format!("the {} takes {}", g.key, keys.join(", ")),
                        )
                    })?
                }
            };
            let fabric = parse_colour(&p.string("color", Some("#ffffff"))?)
                .ok_or_else(|| p.invalid("color", "takes #rrggbb"))?;
            let background = match p.string("background", Some("#f2f2f2"))?.as_str() {
                "transparent" => None,
                c => Some(
                    parse_colour(c)
                        .ok_or_else(|| p.invalid("background", "takes #rrggbb or transparent"))?,
                ),
            };
            let size = p.int("size", Some(1024), 256, 4096)? as u32;
            let pattern = design_pattern(&s.design);
            let m = compose(&pattern, g, placement, fabric, background, size)
                .map_err(|e| p.failed(e))?;
            crate::fsio::write(&path, &m.png).map_err(|e| p.failed(e.to_string()))?;
            Ok(json!({
                "path": path.display().to_string(),
                "garment": g.key,
                "placement": placement.key,
                "size": m.size,
                "designMm": [round1(m.design_mm.0), round1(m.design_mm.1)],
                "suitsMm": [placement.max_w, placement.max_h],
                "fits": m.fits,
            }))
        },
    )]
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

fn garment_keys() -> String {
    GARMENTS
        .iter()
        .map(|g| g.key)
        .collect::<Vec<_>>()
        .join(", ")
}

/// The garments and their placements, for a client listing the choices.
pub fn describe() -> Value {
    json!(
        GARMENTS
            .iter()
            .map(|g| json!({
                "garment": g.key,
                "label": g.label,
                "placements": g.placements.iter().map(|p| json!({
                    "placement": p.key, "maxMm": [p.max_w, p.max_h],
                })).collect::<Vec<_>>(),
            }))
            .collect::<Vec<_>>()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every garment picture is a 768 px cut-out: transparent corners, opaque at its
    /// placements, and tinted by the fabric colour.
    #[test]
    fn garment_pictures_are_cut_outs_that_take_the_fabric_colour() {
        for g in GARMENTS {
            let p = tinted(g, [200, 40, 40]).expect(g.key);
            assert_eq!((p.width(), p.height()), (768, 768), "{}", g.key);
            assert_eq!(p.pixel(0, 0).expect("corner").alpha(), 0, "{}", g.key);
            for place in g.placements {
                let c = p
                    .pixel(place.x as u32, place.y as u32)
                    .expect("placement")
                    .demultiply();
                assert_eq!(c.alpha(), 255, "{} {}", g.key, place.key);
                assert!(
                    c.red() > 120 && c.green() < 40 && c.blue() < 40,
                    "{} {c:?}",
                    g.key
                );
            }
        }
        assert_eq!(parse_colour("#1f3A68"), Some([0x1f, 0x3a, 0x68]));
        assert!(parse_colour("1f3a68").is_none() && parse_colour("#1f3a6").is_none());
        assert!(
            describe()
                .as_array()
                .is_some_and(|a| a.len() == GARMENTS.len())
        );
    }
}
