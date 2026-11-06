//! OpenStitch's own bundled embroidery fonts that are not TrueType outlines (#527, #530, #536):
//!
//! - **OpenStitch Line**, a single-line (centre-line) font: each glyph is a few open strokes
//!   along the letter's centre, sewn as one running line (PE-DESIGN's Small Text idea) or as a
//!   narrow satin along the line. Drawn for OpenStitch: no outline of another font was traced.
//! - **OpenStitch Pixel**, a 5 × 9 pixel font (7 rows for capitals, 2 below the baseline):
//!   each lit pixel is a square sewn as a short run, a cross or a small block.
//!
//! Both are built once from the tables below into [`EmbroideryFont`]s, so layout, Text
//! Transform, text on a path and Name Drop treat them like any embroidery font. Coordinates are
//! font units: the capital height is 100, +y down, the baseline at y = 0.

use crate::fmath::Det;
use crate::fonts::{EmbroideryFont, EmbroideryGlyph};
use crate::geometry::{Contour, Vec2, v2};
use std::sync::OnceLock;

/// Capital height of both fonts, font units.
pub const CAP: f64 = 100.0;

/// OpenStitch Line's glyphs: (character, advance, strokes). Strokes are separated by `;`; a
/// stroke is a list of points `x,y` and arcs `@cx,cy,rx,ry,from,to` (degrees, 0 = +x, 90 = +y
/// down, sampled every 15 degrees or less, both ends included), joined in order. Where a
/// stroke starts or ends on another one, the letter's strokes meet there: the router travels
/// along strokes already sewn instead of trimming.
#[rustfmt::skip]
const LINE_GLYPHS: &[(char, f64, &str)] = &[
    (' ', 40.0, ""),
    ('A', 80.0, "10,0 40,-100 70,0; 19,-30 61,-30"),
    ('B', 75.0, "15,0 15,-100 45,-100 @45,-75,20,25,-90,90 15,-50; 15,-50 48,-50 @48,-25,22,25,-90,90 15,0"),
    ('C', 80.0, "@45,-50,35,50,-40,-320"),
    ('D', 80.0, "15,0 15,-100 35,-100 @35,-50,35,50,-90,90 15,0"),
    ('E', 70.0, "60,-100 15,-100 15,0 60,0; 15,-50 52,-50"),
    ('F', 65.0, "60,-100 15,-100 15,0; 15,-50 50,-50"),
    ('G', 85.0, "@45,-50,35,50,-40,-360 55,-50"),
    ('H', 80.0, "15,0 15,-100; 65,0 65,-100; 15,-50 65,-50"),
    ('I', 30.0, "15,0 15,-100"),
    ('J', 60.0, "45,-100 45,-25 @27,-25,18,25,0,180"),
    ('K', 75.0, "15,0 15,-100; 65,-100 15,-40; 35,-64 67,0"),
    ('L', 65.0, "15,-100 15,0 60,0"),
    ('M', 90.0, "15,0 15,-100 45,-35 75,-100 75,0"),
    ('N', 80.0, "15,0 15,-100 65,0 65,-100"),
    ('O', 90.0, "@45,-50,35,50,-90,270"),
    ('P', 75.0, "15,0 15,-100 45,-100 @45,-75,22,25,-90,90 15,-50"),
    ('Q', 90.0, "@45,-50,35,50,-90,270; 57,-30 69.749,-14.645 82,2"),
    ('R', 75.0, "15,0 15,-100 45,-100 @45,-75,22,25,-90,90 15,-50; 40,-50 67,0"),
    ('S', 75.0, "@38,-75,25,25,-30,-270 @38,-25,27,25,-90,150"),
    ('T', 70.0, "10,-100 60,-100; 35,-100 35,0"),
    ('U', 80.0, "15,-100 15,-30 @40,-30,25,30,180,0 65,-100"),
    ('V', 80.0, "10,-100 40,0 70,-100"),
    ('W', 100.0, "10,-100 30,0 50,-70 70,0 90,-100"),
    ('X', 75.0, "12,-100 37.5,-50 63,0; 63,-100 37.5,-50 12,0"),
    ('Y', 75.0, "10,-100 37.5,-50 65,-100; 37.5,-50 37.5,0"),
    ('Z', 70.0, "12,-100 60,-100 12,0 62,0"),
    ('a', 65.0, "53,-70 53,0; @33,-35,20,35,0,360"),
    ('b', 65.0, "13,-100 13,0; @33,-35,20,35,180,540"),
    ('c', 60.0, "@33,-35,22,35,-40,-320"),
    ('d', 65.0, "52,-100 52,0; @32,-35,20,35,0,360"),
    ('e', 65.0, "12,-35 54,-35 @33,-35,21,35,0,-320"),
    ('f', 45.0, "20,0 20,-70 20,-88 @30,-88,10,12,180,270 42,-100; 8,-70 20,-70 38,-70"),
    ('g', 65.0, "@32,-35,20,35,0,360; 52,-70 52,15 @32,15,20,15,0,160"),
    ('h', 65.0, "13,-100 13,0; 13,-48 @33,-48,20,22,180,360 53,0"),
    ('i', 26.0, "13,0 13,-70; 13,-88 13,-96"),
    ('j', 30.0, "20,-70 20,15 @8,15,12,15,0,150; 20,-88 20,-96"),
    ('k', 60.0, "13,-100 13,0; 50,-70 13,-30; 25,-43 52,0"),
    ('l', 26.0, "13,-100 13,0"),
    ('m', 86.0, "13,-70 13,0; 13,-50 @28,-50,15,20,180,360 43,0; 43,-50 @58,-50,15,20,180,360 73,0"),
    ('n', 65.0, "13,-70 13,0; 13,-48 @33,-48,20,22,180,360 53,0"),
    ('o', 65.0, "@33,-35,21,35,-90,270"),
    ('p', 65.0, "13,-70 13,30; @33,-35,20,35,180,540"),
    ('q', 65.0, "53,-70 53,30; @33,-35,20,35,0,360"),
    ('r', 45.0, "13,-70 13,0; 13,-45 @33,-45,20,25,180,270 40,-70"),
    ('s', 60.0, "@30,-52,18,18,-30,-270 @30,-17,20,17,-90,150"),
    ('t', 45.0, "20,-95 20,-70 20,-12 @32,-12,12,12,180,90 40,-2; 6,-70 20,-70 36,-70"),
    ('u', 65.0, "13,-70 13,-22 @33,-22,20,22,180,0; 53,-70 53,0"),
    ('v', 60.0, "10,-70 30,0 50,-70"),
    ('w', 85.0, "8,-70 24,0 42,-55 60,0 76,-70"),
    ('x', 60.0, "12,-70 31,-35 50,0; 50,-70 31,-35 12,0"),
    ('y', 60.0, "10,-70 30,0; 50,-70 30,0 21.43,30"),
    ('z', 60.0, "12,-70 50,-70 12,0 52,0"),
    ('0', 70.0, "@35,-50,25,50,-90,270"),
    ('1', 50.0, "12,-80 30,-100 30,0"),
    ('2', 70.0, "@35,-72,23,28,-160,20 12,0 60,0"),
    ('3', 70.0, "@33,-75,22,25,-150,90 @33,-25,25,25,-90,150"),
    ('4', 70.0, "48,0 48,-100 10,-30 62,-30"),
    ('5', 70.0, "58,-100 18,-100 15,-55 @36,-32,23,28,-125,145"),
    ('6', 70.0, "50,-100 16.947,-45 @36,-30,22,30,-150,210"),
    ('7', 65.0, "10,-100 58,-100 25,0"),
    ('8', 70.0, "@35,-75,20,25,90,450; @35,-25,24,25,-90,270"),
    ('9', 70.0, "@34,-70,22,30,30,390 40,0"),
    ('.', 25.0, "12,-8 12,0"),
    (',', 25.0, "14,-8 14,0 9,10"),
    ('!', 26.0, "13,-100 13,-30; 13,-8 13,0"),
    ('?', 60.0, "@30,-75,20,22,-160,90 30,-30; 30,-8 30,0"),
    ('-', 50.0, "10,-40 40,-40"),
    (':', 25.0, "12,-62 12,-54; 12,-8 12,0"),
    (';', 25.0, "12,-62 12,-54; 14,-8 14,0 9,10"),
    ('\'', 25.0, "12,-100 12,-75"),
    ('"', 40.0, "12,-100 12,-75; 28,-100 28,-75"),
    ('/', 55.0, "8,0 47,-100"),
    ('+', 60.0, "10,-40 30,-40 50,-40; 30,-60 30,-40 30,-20"),
    ('=', 60.0, "10,-50 50,-50; 10,-30 50,-30"),
    ('(', 40.0, "@60,-40,40,70,-150,-210"),
    (')', 40.0, "@-20,-40,40,70,-30,30"),
    ('&', 80.0, "70,0 20,-65 @35,-80,15,20,-180,0 15,-30 @35,-25,20,25,180,90 70,-40"),
];

/// OpenStitch Pixel's glyphs: 9 rows of 5 pixels (`#` lit), the capital height the first 7.
#[rustfmt::skip]
const PIXEL_GLYPHS: &[(char, [&str; 9])] = &[
    ('A', [".###.", "#...#", "#...#", "#####", "#...#", "#...#", "#...#", ".....", "....."]),
    ('B', ["####.", "#...#", "#...#", "####.", "#...#", "#...#", "####.", ".....", "....."]),
    ('C', [".###.", "#...#", "#....", "#....", "#....", "#...#", ".###.", ".....", "....."]),
    ('D', ["###..", "#..#.", "#...#", "#...#", "#...#", "#..#.", "###..", ".....", "....."]),
    ('E', ["#####", "#....", "#....", "####.", "#....", "#....", "#####", ".....", "....."]),
    ('F', ["#####", "#....", "#....", "####.", "#....", "#....", "#....", ".....", "....."]),
    ('G', [".###.", "#...#", "#....", "#.###", "#...#", "#...#", ".####", ".....", "....."]),
    ('H', ["#...#", "#...#", "#...#", "#####", "#...#", "#...#", "#...#", ".....", "....."]),
    ('I', [".###.", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('J', ["..###", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##..", ".....", "....."]),
    ('K', ["#...#", "#..#.", "#.#..", "##...", "#.#..", "#..#.", "#...#", ".....", "....."]),
    ('L', ["#....", "#....", "#....", "#....", "#....", "#....", "#####", ".....", "....."]),
    ('M', ["#...#", "##.##", "#.#.#", "#.#.#", "#...#", "#...#", "#...#", ".....", "....."]),
    ('N', ["#...#", "#...#", "##..#", "#.#.#", "#..##", "#...#", "#...#", ".....", "....."]),
    ('O', [".###.", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.", ".....", "....."]),
    ('P', ["####.", "#...#", "#...#", "####.", "#....", "#....", "#....", ".....", "....."]),
    ('Q', [".###.", "#...#", "#...#", "#...#", "#.#.#", "#..#.", ".##.#", ".....", "....."]),
    ('R', ["####.", "#...#", "#...#", "####.", "#.#..", "#..#.", "#...#", ".....", "....."]),
    ('S', [".####", "#....", "#....", ".###.", "....#", "....#", "####.", ".....", "....."]),
    ('T', ["#####", "..#..", "..#..", "..#..", "..#..", "..#..", "..#..", ".....", "....."]),
    ('U', ["#...#", "#...#", "#...#", "#...#", "#...#", "#...#", ".###.", ".....", "....."]),
    ('V', ["#...#", "#...#", "#...#", "#...#", "#...#", ".#.#.", "..#..", ".....", "....."]),
    ('W', ["#...#", "#...#", "#...#", "#.#.#", "#.#.#", "#.#.#", ".#.#.", ".....", "....."]),
    ('X', ["#...#", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", "#...#", ".....", "....."]),
    ('Y', ["#...#", "#...#", ".#.#.", "..#..", "..#..", "..#..", "..#..", ".....", "....."]),
    ('Z', ["#####", "....#", "...#.", "..#..", ".#...", "#....", "#####", ".....", "....."]),
    ('a', [".....", ".....", ".###.", "....#", ".####", "#...#", ".####", ".....", "....."]),
    ('b', ["#....", "#....", "####.", "#...#", "#...#", "#...#", "####.", ".....", "....."]),
    ('c', [".....", ".....", ".###.", "#....", "#....", "#....", ".###.", ".....", "....."]),
    ('d', ["....#", "....#", ".####", "#...#", "#...#", "#...#", ".####", ".....", "....."]),
    ('e', [".....", ".....", ".###.", "#...#", "#####", "#....", ".###.", ".....", "....."]),
    ('f', ["..##.", ".#...", "####.", ".#...", ".#...", ".#...", ".#...", ".....", "....."]),
    ('g', [".....", ".....", ".####", "#...#", "#...#", ".####", "....#", "....#", ".###."]),
    ('h', ["#....", "#....", "####.", "#...#", "#...#", "#...#", "#...#", ".....", "....."]),
    ('i', ["..#..", ".....", ".##..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('j', ["...#.", ".....", "..##.", "...#.", "...#.", "...#.", "...#.", "#..#.", ".##.."]),
    ('k', ["#....", "#....", "#..#.", "#.#..", "##...", "#.#..", "#..#.", ".....", "....."]),
    ('l', [".##..", "..#..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('m', [".....", ".....", "##.#.", "#.#.#", "#.#.#", "#.#.#", "#...#", ".....", "....."]),
    ('n', [".....", ".....", "####.", "#...#", "#...#", "#...#", "#...#", ".....", "....."]),
    ('o', [".....", ".....", ".###.", "#...#", "#...#", "#...#", ".###.", ".....", "....."]),
    ('p', [".....", ".....", "####.", "#...#", "#...#", "####.", "#....", "#....", "#...."]),
    ('q', [".....", ".....", ".####", "#...#", "#...#", ".####", "....#", "....#", "....#"]),
    ('r', [".....", ".....", "#.##.", "##..#", "#....", "#....", "#....", ".....", "....."]),
    ('s', [".....", ".....", ".####", "#....", ".###.", "....#", "####.", ".....", "....."]),
    ('t', [".#...", ".#...", "####.", ".#...", ".#...", ".#..#", "..##.", ".....", "....."]),
    ('u', [".....", ".....", "#...#", "#...#", "#...#", "#..##", ".##.#", ".....", "....."]),
    ('v', [".....", ".....", "#...#", "#...#", "#...#", ".#.#.", "..#..", ".....", "....."]),
    ('w', [".....", ".....", "#...#", "#...#", "#.#.#", "#.#.#", ".#.#.", ".....", "....."]),
    ('x', [".....", ".....", "#...#", ".#.#.", "..#..", ".#.#.", "#...#", ".....", "....."]),
    ('y', [".....", ".....", "#...#", "#...#", "#...#", ".####", "....#", "....#", ".###."]),
    ('z', [".....", ".....", "#####", "...#.", "..#..", ".#...", "#####", ".....", "....."]),
    ('0', [".###.", "#...#", "#..##", "#.#.#", "##..#", "#...#", ".###.", ".....", "....."]),
    ('1', ["..#..", ".##..", "..#..", "..#..", "..#..", "..#..", ".###.", ".....", "....."]),
    ('2', [".###.", "#...#", "....#", "...#.", "..#..", ".#...", "#####", ".....", "....."]),
    ('3', ["#####", "...#.", "..#..", "...#.", "....#", "#...#", ".###.", ".....", "....."]),
    ('4', ["...#.", "..##.", ".#.#.", "#..#.", "#####", "...#.", "...#.", ".....", "....."]),
    ('5', ["#####", "#....", "####.", "....#", "....#", "#...#", ".###.", ".....", "....."]),
    ('6', ["..##.", ".#...", "#....", "####.", "#...#", "#...#", ".###.", ".....", "....."]),
    ('7', ["#####", "....#", "...#.", "..#..", ".#...", ".#...", ".#...", ".....", "....."]),
    ('8', [".###.", "#...#", "#...#", ".###.", "#...#", "#...#", ".###.", ".....", "....."]),
    ('9', [".###.", "#...#", "#...#", ".####", "....#", "...#.", ".##..", ".....", "....."]),
    ('.', [".....", ".....", ".....", ".....", ".....", ".....", "..#..", ".....", "....."]),
    (',', [".....", ".....", ".....", ".....", ".....", "..#..", "..#..", ".#...", "....."]),
    ('!', ["..#..", "..#..", "..#..", "..#..", "..#..", ".....", "..#..", ".....", "....."]),
    ('?', [".###.", "#...#", "....#", "...#.", "..#..", ".....", "..#..", ".....", "....."]),
    ('-', [".....", ".....", ".....", "#####", ".....", ".....", ".....", ".....", "....."]),
    (':', [".....", "..#..", ".....", ".....", ".....", "..#..", ".....", ".....", "....."]),
    ('+', [".....", "..#..", "..#..", "#####", "..#..", "..#..", ".....", ".....", "....."]),
    ('/', ["....#", "....#", "...#.", "..#..", ".#...", "#....", "#....", ".....", "....."]),
    ('\'', ["..#..", "..#..", ".....", ".....", ".....", ".....", ".....", ".....", "....."]),
    ('#', [".#.#.", ".#.#.", "#####", ".#.#.", "#####", ".#.#.", ".#.#.", ".....", "....."]),
    ('<', ["...#.", "..#..", ".#...", "#....", ".#...", "..#..", "...#.", ".....", "....."]),
    ('>', [".#...", "..#..", "...#.", "....#", "...#.", "..#..", ".#...", ".....", "....."]),
];

/// Pixel size of OpenStitch Pixel, font units: 7 rows make the capital height.
pub const PIXEL: f64 = CAP / 7.0;

/// Points of one stroke of the path language in [`LINE_GLYPHS`].
fn parse_stroke(s: &str) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = Vec::new();
    for token in s.split_whitespace() {
        let nums: Vec<f64> = token
            .trim_start_matches('@')
            .split(',')
            .map(|n| n.parse().expect("font table number"))
            .collect();
        if token.starts_with('@') {
            let [cx, cy, rx, ry, a0, a1] = nums[..] else {
                panic!("font table arc {token}");
            };
            let steps = ((a1 - a0).abs() / 15.0).ceil().max(1.0) as usize;
            for k in 0..=steps {
                let a = (a0 + (a1 - a0) * k as f64 / steps as f64).to_radians();
                out.push(v2(cx + rx * a.dcos(), cy + ry * a.dsin()));
            }
        } else {
            out.push(v2(nums[0], nums[1]));
        }
    }
    out.dedup_by(|a, b| a.distance(*b) < 1e-6);
    out
}

/// OpenStitch Line, built once.
pub fn line_font() -> &'static EmbroideryFont {
    static FONT: OnceLock<EmbroideryFont> = OnceLock::new();
    FONT.get_or_init(|| {
        let mut f = EmbroideryFont::new(CAP);
        f.ascender = 110.0;
        f.descender = 35.0;
        f.line_gap = 15.0;
        for &(ch, advance, strokes) in LINE_GLYPHS {
            let contours = strokes
                .split(';')
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(|s| Contour::new(parse_stroke(s), false))
                .collect();
            f.set_glyph(EmbroideryGlyph {
                ch,
                advance,
                contours,
                stitches: None,
            });
        }
        f
    })
}

/// OpenStitch Pixel, built once. Each lit pixel is a closed square, its corners in the order
/// top left, top right, bottom right, bottom left (the pixel router finds them again after a
/// Text Transform splits the sides).
pub fn pixel_font() -> &'static EmbroideryFont {
    static FONT: OnceLock<EmbroideryFont> = OnceLock::new();
    FONT.get_or_init(|| {
        let mut f = EmbroideryFont::new(CAP);
        f.ascender = 8.0 * PIXEL;
        f.descender = 2.0 * PIXEL;
        f.line_gap = PIXEL;
        let square = |col: usize, row: usize| {
            let (x, y) = (col as f64 * PIXEL, (row as f64 - 7.0) * PIXEL);
            Contour::new(
                vec![
                    v2(x, y),
                    v2(x + PIXEL, y),
                    v2(x + PIXEL, y + PIXEL),
                    v2(x, y + PIXEL),
                ],
                true,
            )
        };
        f.set_glyph(EmbroideryGlyph {
            ch: ' ',
            advance: 4.0 * PIXEL,
            contours: Vec::new(),
            stitches: None,
        });
        for (ch, rows) in PIXEL_GLYPHS {
            let contours = rows
                .iter()
                .enumerate()
                .flat_map(|(r, row)| {
                    row.bytes()
                        .enumerate()
                        .filter(|&(_, b)| b == b'#')
                        .map(move |(c, _)| square(c, r))
                })
                .collect();
            f.set_glyph(EmbroideryGlyph {
                ch: *ch,
                advance: 6.0 * PIXEL,
                contours,
                stitches: None,
            });
        }
        f
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fonts_build_and_validate() {
        let line = line_font();
        assert!(line.validate().is_ok());
        for c in ('a'..='z').chain('A'..='Z').chain('0'..='9') {
            let g = line
                .glyph(c)
                .unwrap_or_else(|| panic!("line font lacks {c}"));
            assert!(!g.contours.is_empty() && g.contours.iter().all(|c| !c.closed));
            let px = pixel_font()
                .glyph(c)
                .unwrap_or_else(|| panic!("pixel lacks {c}"));
            assert!(px.contours.iter().all(|c| c.closed && c.points.len() == 4));
        }
        assert!(pixel_font().validate().is_ok());
        // Every row of the pixel table is five pixels wide.
        assert!(
            PIXEL_GLYPHS
                .iter()
                .all(|(_, rows)| rows.iter().all(|r| r.len() == 5))
        );
    }

    /// What `tools/sew_check.py` measures on a compiled text: (the longest run of zigzag
    /// stitches under 1 mm wide, the longest stitch in mm, the longest run of stitches under
    /// 0.3 mm, the most needle drops in one 0.5 mm cell).
    fn sew_metrics(font: &str, size: f64, text: &str) -> (usize, f64, usize, usize) {
        use crate::compile::Compiler;
        use crate::model::{Design, DesignObject, LineSewing, LineType, RegionSewing, RegionType};
        use crate::text::{FontKind, TextSpec, bundled_kind};
        use openstitch_core::{Command, Thread};
        let kind = bundled_kind(font).expect("bundled");
        let mut region = RegionSewing::new(RegionType::Satin, Thread::rgb(0));
        region.density = 5.0;
        region.underlay = false;
        region.pull_compensation = 0.0;
        let (region, line) = if kind == FontKind::SingleLine {
            let mut l = LineSewing::new(LineType::Running, Thread::rgb(0));
            l.pitch = 1.5;
            (None, Some(l))
        } else {
            (Some(region), None)
        };
        let (obj, _) =
            DesignObject::text(TextSpec::new(text, font, size), region, line).expect("text");
        let mut d = Design::new("range", crate::model::DesignPage::default());
        d.add(obj);
        let st = Compiler::for_design(&d).compile(&d).stitches;
        let (mut hair, mut run, mut tiny, mut tiny_run, mut longest) = (0, 0, 0, 0, 0.0f64);
        let mut cells = std::collections::HashMap::<(i64, i64), usize>::new();
        let mut step: Option<Vec2> = None;
        let mut prev: Option<Vec2> = None;
        for s in &st {
            let p = v2(s.x, s.y);
            if s.command != Command::Stitch {
                (prev, step, run, tiny_run) = (None, None, 0, 0);
                continue;
            }
            *cells
                .entry(((p.x / 5.0).floor() as i64, (p.y / 5.0).floor() as i64))
                .or_default() += 1;
            if let Some(a) = prev {
                let v = p - a;
                let len = v.x.dhypot(v.y);
                longest = longest.max(len / 10.0);
                let turned = step.is_some_and(|w| v.x * w.x + v.y * w.y < 0.0);
                run = if 0.5 < len && len < 10.0 && turned {
                    run + 1
                } else {
                    0
                };
                hair = hair.max(run);
                tiny_run = if len < 3.0 { tiny_run + 1 } else { 0 };
                tiny = tiny.max(tiny_run);
                step = Some(v);
            }
            prev = Some(p);
        }
        (
            hair,
            longest,
            tiny,
            cells.values().copied().max().unwrap_or(0),
        )
    }

    const SAMPLE: &str = "Hamburgefonts 0123";

    /// Prints the metrics over sizes, to choose the ranges: `cargo test -p openstitch-design
    /// measure_size_ranges -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn measure_size_ranges() {
        for f in &crate::text::FONTS {
            for size in [
                2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 10.0, 15.0, 20.0, 35.0, 40.0, 60.0, 80.0,
            ] {
                let (h, l, t, c) = sew_metrics(f.id, size, SAMPLE);
                println!(
                    "{:18} {size:5} hairline {h:3} longest {l:5.2} tiny {t:2} cell {c:3}",
                    f.id
                );
            }
        }
    }

    /// Every bundled font's size range (#527) holds at both ends against `tools/sew_check.py`'s
    /// limits: at most 45 zigzag stitches in a row under 1 mm (hairline satin), no stitch over
    /// 12 mm, at most 6 stitches under 0.3 mm in a row and at most 20 needle drops in a 0.5 mm
    /// cell. For the TrueType fonts the range is as wide as those limits allow: 1 mm smaller
    /// sews hairlines, and a few millimetres larger has long stitches or piled-up drops.
    #[test]
    fn size_ranges_hold() {
        let ok = |m: (usize, f64, usize, usize)| m.0 <= 45 && m.1 <= 12.0 && m.2 <= 6 && m.3 <= 20;
        for f in &crate::text::FONTS {
            let (lo, hi) = f.size_range;
            for size in [lo, hi] {
                let m = sew_metrics(f.id, size, SAMPLE);
                assert!(ok(m), "{} at {size} mm: {m:?}", f.id);
            }
            if f.kind == crate::text::FontKind::TrueType {
                let below = sew_metrics(f.id, lo - 1.0, SAMPLE);
                let above = sew_metrics(f.id, hi + 3.0, SAMPLE);
                assert!(
                    !ok(below),
                    "{}: {} mm sews well too: {below:?}",
                    f.id,
                    lo - 1.0
                );
                assert!(
                    !ok(above),
                    "{}: {} mm sews well too: {above:?}",
                    f.id,
                    hi + 3.0
                );
            }
        }
    }

    #[test]
    fn arcs_include_both_ends() {
        let p = parse_stroke("@0,0,10,10,0,90");
        assert!(p[0].distance(v2(10.0, 0.0)) < 1e-9);
        assert!(p.last().expect("end").distance(v2(0.0, 10.0)) < 1e-9);
        assert_eq!(p.len(), 7);
    }
}
