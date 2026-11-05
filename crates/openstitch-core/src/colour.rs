//! Perceptual colour: sRGB to CIELAB (D65) and Oklab, the CIEDE2000 colour difference, and the
//! thread-matching metrics built on them (#484).
//!
//! The PE-DESIGN-compatible paths match by [`crate::color_distance`] (equal-weight RGB, #119)
//! and never use this module unless a command asks for a perceptual metric. The image wizards'
//! OpenStitch mode uses the same functions (re-exported by `openstitch_digitize::perceptual`).
//!
//! Every transcendental function comes from the `libm` crate (a pure-Rust port of musl's libm),
//! so a match is the same on every platform.

use crate::Thread;

/// CIELAB coordinates (L*, a*, b*), D65 white.
pub type Lab = [f64; 3];

fn linear(v: u8) -> f64 {
    let c = f64::from(v) / 255.0;
    if c <= 0.04045 {
        c / 12.92
    } else {
        libm::pow((c + 0.055) / 1.055, 2.4)
    }
}

/// sRGB to linear-light RGB, each channel 0–1.
pub fn linear_rgb(c: [u8; 3]) -> [f64; 3] {
    [linear(c[0]), linear(c[1]), linear(c[2])]
}

/// sRGB to CIELAB (D65).
pub fn lab(c: [u8; 3]) -> Lab {
    lab_linear(linear_rgb(c))
}

/// Linear-light RGB (each channel 0–1) to CIELAB (D65).
pub fn lab_linear(c: [f64; 3]) -> Lab {
    let [r, g, b] = c;
    let x = (0.4124 * r + 0.3576 * g + 0.1805 * b) / 0.95047;
    let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let z = (0.0193 * r + 0.1192 * g + 0.9505 * b) / 1.08883;
    let f = |t: f64| {
        if t > 216.0 / 24389.0 {
            libm::cbrt(t)
        } else {
            (24389.0 / 27.0 * t + 16.0) / 116.0
        }
    };
    let (fx, fy, fz) = (f(x), f(y), f(z));
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// CIELAB back to sRGB, clamped to the gamut.
pub fn rgb(l: Lab) -> [u8; 3] {
    let fy = (l[0] + 16.0) / 116.0;
    let (fx, fz) = (fy + l[1] / 500.0, fy - l[2] / 200.0);
    let inv = |t: f64| {
        if t * t * t > 216.0 / 24389.0 {
            t * t * t
        } else {
            (116.0 * t - 16.0) * 27.0 / 24389.0
        }
    };
    let (x, y, z) = (inv(fx) * 0.95047, inv(fy), inv(fz) * 1.08883);
    let lin = [
        3.2406 * x - 1.5372 * y - 0.4986 * z,
        -0.9689 * x + 1.8758 * y + 0.0415 * z,
        0.0557 * x - 0.2040 * y + 1.0570 * z,
    ];
    lin.map(|c| {
        let c = c.clamp(0.0, 1.0);
        let s = if c <= 0.0031308 {
            12.92 * c
        } else {
            1.055 * libm::pow(c, 1.0 / 2.4) - 0.055
        };
        (s * 255.0).round() as u8
    })
}

/// The CIEDE2000 colour difference (Sharma, Wu and Dalal 2005).
pub fn de2000(p: Lab, q: Lab) -> f64 {
    let (l1, a1, b1) = (p[0], p[1], p[2]);
    let (l2, a2, b2) = (q[0], q[1], q[2]);
    let cb7 = ((libm::hypot(a1, b1) + libm::hypot(a2, b2)) / 2.0).powi(7);
    let g = 0.5 * (1.0 - (cb7 / (cb7 + 25f64.powi(7))).sqrt());
    let (a1p, a2p) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (c1p, c2p) = (libm::hypot(a1p, b1), libm::hypot(a2p, b2));
    let hue = |b: f64, a: f64| {
        if b == 0.0 && a == 0.0 {
            0.0
        } else {
            libm::atan2(b, a).to_degrees().rem_euclid(360.0)
        }
    };
    let (h1p, h2p) = (hue(b1, a1p), hue(b2, a2p));
    let (dlp, dcp) = (l2 - l1, c2p - c1p);
    let mut dh = h2p - h1p;
    if c1p * c2p == 0.0 {
        dh = 0.0;
    } else if dh > 180.0 {
        dh -= 360.0;
    } else if dh < -180.0 {
        dh += 360.0;
    }
    let dhp = 2.0 * (c1p * c2p).sqrt() * libm::sin((dh / 2.0).to_radians());
    let (lbp, cbp) = ((l1 + l2) / 2.0, (c1p + c2p) / 2.0);
    let hbp = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() > 180.0 {
        if h1p + h2p < 360.0 {
            (h1p + h2p + 360.0) / 2.0
        } else {
            (h1p + h2p - 360.0) / 2.0
        }
    } else {
        (h1p + h2p) / 2.0
    };
    let cos = |deg: f64| libm::cos(deg.to_radians());
    let t = 1.0 - 0.17 * cos(hbp - 30.0) + 0.24 * cos(2.0 * hbp) + 0.32 * cos(3.0 * hbp + 6.0)
        - 0.20 * cos(4.0 * hbp - 63.0);
    let sl = 1.0 + 0.015 * (lbp - 50.0).powi(2) / (20.0 + (lbp - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cbp;
    let sh = 1.0 + 0.015 * cbp * t;
    let cbp7 = cbp.powi(7);
    let rt = -2.0
        * (cbp7 / (cbp7 + 25f64.powi(7))).sqrt()
        * libm::sin((60.0 * libm::exp(-((hbp - 275.0) / 25.0).powi(2))).to_radians());
    ((dlp / sl).powi(2) + (dcp / sc).powi(2) + (dhp / sh).powi(2) + rt * (dcp / sc) * (dhp / sh))
        .sqrt()
}

/// sRGB to Oklab (Björn Ottosson, 2020), L 0–1.
pub fn oklab(c: [u8; 3]) -> [f64; 3] {
    let [r, g, b] = linear_rgb(c);
    let l = libm::cbrt(0.412_221_470_8 * r + 0.536_332_536_3 * g + 0.051_445_992_9 * b);
    let m = libm::cbrt(0.211_903_498_2 * r + 0.680_699_545_1 * g + 0.107_396_956_6 * b);
    let s = libm::cbrt(0.088_302_461_9 * r + 0.281_718_837_6 * g + 0.629_978_700_5 * b);
    [
        0.210_454_255_3 * l + 0.793_617_785_0 * m - 0.004_072_046_8 * s,
        1.977_998_495_1 * l - 2.428_592_205_0 * m + 0.450_593_709_9 * s,
        0.025_904_037_1 * l + 0.782_771_766_2 * m - 0.808_675_766_0 * s,
    ]
}

/// How thread colours are compared when one is matched to a chart.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Metric {
    /// PE-DESIGN's rule: equal-weight squared Euclidean distance on 8-bit sRGB (#119).
    Rgb,
    /// CIEDE2000 in CIELAB (D65).
    Ciede2000,
    /// Euclidean distance in Oklab, times 100 (so 1 is about one just-noticeable step).
    Oklab,
}

impl Metric {
    /// The metric named `rgb`, `ciede2000` or `oklab`.
    pub fn parse(s: &str) -> Option<Metric> {
        match s {
            "rgb" => Some(Metric::Rgb),
            "ciede2000" => Some(Metric::Ciede2000),
            "oklab" => Some(Metric::Oklab),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Metric::Rgb => "rgb",
            Metric::Ciede2000 => "ciede2000",
            Metric::Oklab => "oklab",
        }
    }

    /// The distance between two threads under this metric (for [`Metric::Rgb`], the squared
    /// RGB distance as a float).
    pub fn distance(self, a: &Thread, b: &Thread) -> f64 {
        match self {
            Metric::Rgb => crate::color_distance(a, b) as f64,
            Metric::Ciede2000 => delta_e(a, b),
            Metric::Oklab => {
                let (p, q) = (oklab(rgb_of(a)), oklab(rgb_of(b)));
                100.0
                    * ((p[0] - q[0]).powi(2) + (p[1] - q[1]).powi(2) + (p[2] - q[2]).powi(2)).sqrt()
            }
        }
    }
}

fn rgb_of(t: &Thread) -> [u8; 3] {
    [t.red, t.green, t.blue]
}

/// CIEDE2000 between two threads' colours.
pub fn delta_e(a: &Thread, b: &Thread) -> f64 {
    de2000(lab(rgb_of(a)), lab(rgb_of(b)))
}

/// Index of the thread of `chart` nearest `color` under `metric`. Ties go to the later index
/// for [`Metric::Rgb`] (PE-DESIGN's rule, [`crate::nearest_index`]) and to the earlier one
/// otherwise. `None` for an empty chart.
pub fn nearest_by(color: &Thread, chart: &[Thread], metric: Metric) -> Option<usize> {
    let mut best: Option<(f64, usize)> = None;
    for (i, c) in chart.iter().enumerate() {
        let d = metric.distance(color, c);
        let better = match best {
            None => true,
            Some((b, _)) if metric == Metric::Rgb => d <= b,
            Some((b, _)) => d < b,
        };
        if better {
            best = Some((d, i));
        }
    }
    best.map(|(_, i)| i)
}

/// Chart indices of `chart` ordered nearest first from `color` under `metric` (stable, so
/// ties keep chart order).
pub fn ranked_by(color: &Thread, chart: &[Thread], metric: Metric) -> Vec<usize> {
    let d: Vec<f64> = chart.iter().map(|c| metric.distance(color, c)).collect();
    let mut idx: Vec<usize> = (0..chart.len()).collect();
    idx.sort_by(|&a, &b| d[a].total_cmp(&d[b]));
    idx
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Sharma, Wu and Dalal (2005), pairs 1, 7, 17 and 25.
    #[test]
    fn de2000_matches_the_published_test_pairs() {
        let pairs = [
            ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
            ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
            ([50.0, 2.5, 0.0], [56.0, -27.0, -3.0], 31.9030),
            (
                [60.2574, -34.0099, 36.2677],
                [60.4626, -34.1751, 39.4387],
                1.2644,
            ),
        ];
        for (p, q, want) in pairs {
            let d = de2000(p, q);
            assert!((d - want).abs() < 1e-3, "{p:?} {q:?}: {d} != {want}");
            assert!((de2000(q, p) - d).abs() < 1e-9);
        }
    }

    #[test]
    fn lab_and_oklab_white_points_and_round_trip() {
        let w = lab([255, 255, 255]);
        assert!((w[0] - 100.0).abs() < 0.01 && w[1].abs() < 0.05 && w[2].abs() < 0.05);
        for c in [[0, 0, 0], [253, 120, 11], [20, 40, 200], [128, 128, 128]] {
            assert_eq!(rgb(lab(c)), c);
        }
        let o = oklab([255, 255, 255]);
        assert!((o[0] - 1.0).abs() < 1e-3 && o[1].abs() < 1e-3 && o[2].abs() < 1e-3);
        assert!(oklab([0, 0, 0])[0].abs() < 1e-9);
    }

    /// Pairs where plain RGB distance and CIEDE2000 pick different threads (a dark navy and a
    /// dark green): RGB picks the thread further away by ΔE00.
    #[test]
    fn rgb_and_ciede2000_disagree_where_rgb_looks_wrong() {
        let t = Thread::rgb;
        for (colour, chart) in [
            (0x000060, [0x202040, 0x0000c0]),
            (0x104020, [0x203040, 0x007000]),
        ] {
            let c = t(colour);
            let chart = chart.map(t);
            assert_eq!(nearest_by(&c, &chart, Metric::Rgb), Some(0));
            assert_eq!(nearest_by(&c, &chart, Metric::Ciede2000), Some(1));
            assert!(delta_e(&c, &chart[1]) < delta_e(&c, &chart[0]));
        }
    }

    #[test]
    fn rgb_metric_keeps_pe_ties_and_ranking_is_stable() {
        let chart = [Thread::rgb(0), Thread::rgb(0)];
        assert_eq!(nearest_by(&Thread::rgb(1), &chart, Metric::Rgb), Some(1));
        assert_eq!(
            nearest_by(&Thread::rgb(1), &chart, Metric::Ciede2000),
            Some(0)
        );
        assert_eq!(nearest_by(&Thread::rgb(1), &[], Metric::Oklab), None);
        assert_eq!(
            ranked_by(
                &Thread::rgb(0xff0000),
                &[Thread::rgb(0), Thread::rgb(0xee0000)],
                Metric::Ciede2000
            ),
            [1, 0]
        );
        for m in ["rgb", "ciede2000", "oklab"] {
            assert_eq!(Metric::parse(m).map(Metric::name), Some(m));
        }
        assert_eq!(Metric::parse("lab"), None);
    }
}
