//! OpenStitch mode's perceptual colour (docs/research/beyond/image-digitizing.md).

use openstitch_digitize::autopunch::{self, AutoPunch};
use openstitch_digitize::palette::{self, Palette};
use openstitch_digitize::perceptual::{
    SelectCost, de2000, kmeans, lab, nearest, rgb, select_colors, select_threads, select_threads_by,
};
use openstitch_digitize::{Frame, Mode, Raster};

/// CIEDE2000 against the published test data (Sharma, Wu and Dalal 2005, pairs 1, 7 and 25).
#[test]
fn de2000_matches_the_published_test_pairs() {
    let pairs = [
        ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
        ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
        (
            [60.2574, -34.0099, 36.2677],
            [60.4626, -34.1751, 39.4387],
            1.2644,
        ),
    ];
    for (p, q, want) in pairs {
        let d = de2000(p, q);
        assert!((d - want).abs() < 1e-3, "{p:?} {q:?}: {d} != {want}");
        assert!((de2000(q, p) - d).abs() < 1e-9, "symmetric");
    }
    assert_eq!(de2000([40.0, 10.0, -5.0], [40.0, 10.0, -5.0]), 0.0);
}

#[test]
fn lab_round_trips_and_has_the_white_point() {
    let w = lab([255, 255, 255]);
    assert!((w[0] - 100.0).abs() < 0.01 && w[1].abs() < 0.05 && w[2].abs() < 0.05);
    for c in [[0, 0, 0], [253, 120, 11], [20, 40, 200], [128, 128, 128]] {
        assert_eq!(rgb(lab(c)), c);
    }
}

#[test]
fn kmeans_moves_centres_to_their_weighted_members() {
    let pts = [
        ([10.0, 0.0, 0.0], 1.0),
        ([12.0, 0.0, 0.0], 3.0),
        ([80.0, 0.0, 0.0], 2.0),
    ];
    let c = kmeans(&pts, &[[0.0, 0.0, 0.0], [100.0, 0.0, 0.0]], 10);
    assert!((c[0][0] - 11.5).abs() < 1e-9);
    assert!((c[1][0] - 80.0).abs() < 1e-9);
    assert_eq!(nearest(&c, [30.0, 0.0, 0.0]), 0);
}

fn flat(w: u32, h: u32, f: impl Fn(u32, u32) -> [u8; 3]) -> Raster {
    Raster::from_fn(w, h, |x, y| {
        let [r, g, b] = f(x, y);
        [r, g, b, 255]
    })
    .expect("size")
}

/// A dark-green patch next to a dark grey one: RGB distance sends both to grey threads in PE
/// mode's matching, CIEDE2000 keeps the green a green thread.
#[test]
fn perceptual_palette_keeps_hues_apart() {
    let img = flat(
        40,
        20,
        |x, _| if x < 20 { [40, 90, 50] } else { [70, 70, 70] },
    );
    let census = palette::census(&img);
    let pal = Palette::from_census_perceptual(&census, 2);
    assert_eq!(
        pal.threads.len(),
        2,
        "two distinct colours keep two threads"
    );
    let green = &pal.threads[pal.classify([40, 90, 50])];
    let v = green.rgb_value();
    let (r, g, b) = ((v >> 16) & 255, (v >> 8) & 255, v & 255);
    assert!(
        g > r && g > b,
        "the green patch is sewn in a green thread, got #{v:06x}"
    );
    assert_eq!(
        pal,
        Palette::from_census_perceptual(&census, 2),
        "deterministic"
    );
}

/// OpenStitch mode merges regions under 3 mm²: specks that PE mode keeps as their own objects
/// (each sewn with a jump and trim) join the region around them.
#[test]
fn open_stitch_mode_merges_specks() {
    // 100 × 100 px at 0.5 mm/px: a red field with 2 × 2 px (1 mm²) and 3 × 3 px (2.25 mm²)
    // blue specks, on a white border.
    let img = flat(100, 100, |x, y| {
        let border = x < 5 || y < 5 || x >= 95 || y >= 95;
        let speck =
            (x % 20 < 3 && y % 20 < 3 && x > 10 && y > 10) || (x % 20 == 15 && y % 20 == 15);
        if border {
            [255, 255, 255]
        } else if speck {
            [20, 40, 200]
        } else {
            [220, 20, 30]
        }
    });
    let frame = Frame::new(0.0, 0.0, 50.0, 100);
    let pe = autopunch::convert(&img, &frame, &AutoPunch::default()).expect("pe");
    let os = autopunch::convert(
        &img,
        &frame,
        &AutoPunch {
            mode: Mode::OpenStitch,
            min_area_mm2: 3.0,
            ..AutoPunch::default()
        },
    )
    .expect("openstitch");
    assert!(pe.objects.len() > 1, "PE mode keeps the specks");
    assert_eq!(os.objects.len(), 1, "OpenStitch mode sews the field alone");
}

/// Line art keeps its lines in OpenStitch mode (the round-5 mandala, g11): short, thin pieces
/// of a dark line with light-grey edges, on white, are not merged as specks, and their light
/// edges are not left out with the white background that shares their thread.
#[test]
fn open_stitch_mode_keeps_thin_lines() {
    // 200 × 200 px at 0.25 mm/px: dashes 1 px (0.25 mm) wide and 12 px (3 mm) long, each with
    // a light-grey shade on both sides, on white.
    let img = flat(200, 200, |x, y| {
        let dash = (x / 16) % 2 == 0 && x > 8 && x < 192;
        match y % 40 {
            20 if dash => [30, 30, 30],
            19 | 21 if dash => [215, 215, 215],
            _ => [255, 255, 255],
        }
    });
    let frame = Frame::new(0.0, 0.0, 50.0, 200);
    let os = autopunch::convert(
        &img,
        &frame,
        &AutoPunch {
            mode: Mode::OpenStitch,
            min_area_mm2: 3.0,
            ..AutoPunch::default()
        },
    )
    .expect("the dashes are sewn");
    // Five lines of six dashes each.
    assert_eq!(os.objects.len(), 30, "every dash of every line stays");
}

/// Photo Stitch 1's OpenStitch mode dithers in CIELAB: a grey of lightness 50 between black and
/// white threads mixes half and half by lightness, where RGB dithering mixes by RGB value
/// (119 / 255 = 47 % white).
#[test]
fn perceptual_dither_mixes_by_lightness() {
    let grey = [119.0, 119.0, 119.0];
    assert!((lab([119, 119, 119])[0] - 50.0).abs() < 0.5);
    let cells = vec![Some(grey); 60 * 60];
    let targets = [[0.0; 3], [255.0; 3]];
    let white =
        |d: Vec<Option<usize>>| d.iter().filter(|t| **t == Some(1)).count() as f64 / d.len() as f64;
    let rgb = white(palette::dither(&cells, 60, 60, &targets));
    let perceptual = white(palette::dither_perceptual(&cells, 60, 60, &targets));
    assert!(
        (rgb - 119.0 / 255.0).abs() < 0.02,
        "RGB dithering mixes {rgb}"
    );
    assert!(
        (perceptual - 0.5).abs() < 0.02,
        "CIELAB dithering mixes {perceptual}"
    );
}

/// Photo Stitch 2's OpenStitch mode solves the line screen's mix in linear light: a grey that
/// reflects half the light of white (sRGB 188) is half black thread over white fabric, where
/// the sRGB mix gives it about a quarter.
#[test]
fn linear_mix_covers_a_mid_grey_by_half() {
    use openstitch_digitize::screen::Shares;
    let frame = Frame::new(0.0, 0.0, 40.0, 40);
    let black = [[0.0, 0.0, 0.0]];
    let white = [255.0; 3];
    let share = |grey: u8, linear: bool| {
        let img = flat(40, 40, |_, _| [grey; 3]);
        Shares::colour_in(&img, &frame, &black, white, linear).usage(0)
    };
    // A black image is full coverage in both: the share per cell is usage over this.
    let full = share(0, true);
    assert!((share(0, false) - full).abs() < 1e-6);
    let linear = share(188, true) / full;
    let srgb = share(188, false) / full;
    assert!((linear - 0.5).abs() < 0.02, "linear light mixes {linear}");
    assert!(
        (srgb - (255.0 - 188.0) / 255.0).abs() < 0.02,
        "sRGB mixes {srgb}"
    );
}

/// Backward elimination (T4) keeps the threads that serve the image's colours best, never more
/// than asked, and leaves out threads no colour uses.
#[test]
fn thread_selection_keeps_the_threads_that_serve_the_image() {
    let chart = [
        lab([255, 255, 255]),
        lab([250, 250, 250]),
        lab([200, 20, 30]),
        lab([210, 30, 40]),
        lab([20, 40, 200]),
        lab([0, 0, 0]),
    ];
    // Much white, some red, a little blue.
    let colors = [
        (lab([252, 252, 252]), 100.0),
        (lab([205, 25, 35]), 30.0),
        (lab([25, 45, 195]), 5.0),
    ];
    let kept = select_threads(&colors, &chart, 3);
    assert_eq!(kept.len(), 3);
    assert!(kept.contains(&4), "the blue keeps its thread: {kept:?}");
    assert!(kept.iter().any(|&k| k < 2) && kept.iter().any(|&k| k == 2 || k == 3));
    assert!(!kept.contains(&5), "black serves nothing");
    // Ten asked, three colours: three threads.
    assert_eq!(select_threads(&colors, &chart, 10).len(), 3);
    assert_eq!(select_threads(&colors, &chart, 1).len(), 1);
    assert_eq!(kept, select_threads(&colors, &chart, 3), "deterministic");
}

/// A colour list longer than the selection weighs is binned, keeping the total weight.
#[test]
fn selection_colours_are_binned_to_the_limit() {
    let hist: Vec<([u8; 3], u64)> = (0..=255u8)
        .flat_map(|r| (0..=255u8).step_by(4).map(move |g| ([r, g, 128], 1)))
        .collect();
    let colors = select_colors(&hist);
    assert!(colors.len() <= openstitch_digitize::perceptual::SELECT_COLORS);
    let total: f64 = colors.iter().map(|c| c.1).sum();
    assert_eq!(total, hist.len() as f64);
    assert_eq!(
        select_colors(&hist[..10]).len(),
        10,
        "a short list is kept as it is"
    );
}

/// The mix-aware cost chooses threads that span the colours: for a grey ramp and two threads,
/// black and white, which mix to every grey, where the nearest-thread cost takes two greys.
#[test]
fn mix_selection_spans_the_colours() {
    let chart: [[u8; 3]; 5] = [
        [0, 0, 0],
        [70, 70, 70],
        [128, 128, 128],
        [185, 185, 185],
        [255, 255, 255],
    ];
    let ramp: Vec<([u8; 3], f64)> = (40..=215u8).step_by(5).map(|v| ([v; 3], 1.0)).collect();
    let mix = select_threads_by(&ramp, &chart, 2, SelectCost::Mix);
    assert_eq!(mix, vec![0, 4], "black and white span the ramp");
    let near = select_threads_by(&ramp, &chart, 2, SelectCost::Nearest);
    assert!(
        !near.contains(&0) && !near.contains(&4),
        "nearest takes greys: {near:?}"
    );
    assert_eq!(
        mix,
        select_threads_by(&ramp, &chart, 2, SelectCost::Mix),
        "deterministic"
    );
}

/// T6: a flat field and the sharp edge between two flat fields are sewn plainly in their nearest
/// thread; texture stays dithered.
#[test]
fn tonal_dither_leaves_flat_areas_plain() {
    let (w, h) = (40, 40);
    // Left half a flat grey of lightness 50 between black and white threads, right half a
    // texture of five greys.
    let cells: Vec<Option<[f64; 3]>> = (0..w * h)
        .map(|i| {
            let (x, y) = (i % w, i / w);
            let v = if x < 20 {
                119.0
            } else {
                40.0 + 45.0 * ((x * 7 + y * 3) % 5) as f64
            };
            Some([v; 3])
        })
        .collect();
    let targets = [[0.0; 3], [255.0; 3]];
    let tonal = palette::tonal_map(
        &cells
            .iter()
            .map(|c| c.map(|c| lab(c.map(|v| v as u8))))
            .collect::<Vec<_>>(),
        w,
        h,
        (2, 2),
    );
    assert!(!tonal[10 * w + 5], "the flat field is flat");
    assert!(tonal[10 * w + 30], "the texture is tonal");
    let out = palette::dither_tonal(&cells, w, h, &targets, (2, 2));
    let left: Vec<usize> = (0..h)
        .flat_map(|y| (0..15).map(move |x| y * w + x))
        .map(|i| out[i].expect("opaque"))
        .collect();
    assert!(
        left.iter().all(|&t| t == left[0]),
        "the flat grey is one thread, not a dither"
    );
    assert_eq!(out, palette::dither_tonal(&cells, w, h, &targets, (2, 2)));
}
