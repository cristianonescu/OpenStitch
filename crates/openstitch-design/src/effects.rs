//! Shape effects (beyond PE-DESIGN, #497): warp, twist, roughen, pucker & bloat and zigzag
//! applied to outlines, giving ordinary straight-segment contours the user can edit further.
//! Each outline is flattened and resampled along its length, then every sample is moved; the
//! result is deterministic for a seed. Own algorithms. Lengths are 0.1 mm.

use crate::fmath::Det;
use crate::geometry::{Contour, Rect, Vec2, contains, v2};
use crate::paths::{Rng, Walker};
use std::f64::consts::{PI, TAU};

/// Most points one effect may give an outline.
const MAX_POINTS: usize = 20_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    /// A wave across the shape: `amount` mm high, two waves over its width.
    Warp,
    /// Turned by up to `amount` degrees, growing from the centre outwards.
    Twist,
    /// Random sideways steps up to `amount` mm, one every `size` mm.
    Roughen,
    /// The middle of every edge pushed out (`amount` > 0, % of the edge length) or pulled in.
    PuckerBloat,
    /// Points `size` mm apart moved `amount` mm out and in alternately.
    Zigzag,
}

impl Effect {
    pub const ALL: [Effect; 5] = [
        Effect::Warp,
        Effect::Twist,
        Effect::Roughen,
        Effect::PuckerBloat,
        Effect::Zigzag,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Effect::Warp => "warp",
            Effect::Twist => "twist",
            Effect::Roughen => "roughen",
            Effect::PuckerBloat => "puckerBloat",
            Effect::Zigzag => "zigzag",
        }
    }

    pub fn from_id(id: &str) -> Option<Effect> {
        Self::ALL.into_iter().find(|e| e.id() == id)
    }

    pub fn label(self) -> &'static str {
        match self {
            Effect::Warp => "Warp",
            Effect::Twist => "Twist",
            Effect::Roughen => "Roughen",
            Effect::PuckerBloat => "Pucker & Bloat",
            Effect::Zigzag => "Zigzag",
        }
    }

    /// The amount's unit and default.
    pub fn amount(self) -> (&'static str, f64) {
        match self {
            Effect::Twist => ("degrees", 45.0),
            Effect::PuckerBloat => ("%", 20.0),
            _ => ("mm", 1.0),
        }
    }
}

/// Points along `c` about `step` apart (its vertices kept), closed contours without the
/// closing repeat.
fn resample(c: &Contour, step: f64) -> Vec<Vec2> {
    let mut out = Vec::new();
    for i in 0..c.segment_count() {
        let poly = c.segment_polyline(i);
        let walker = Walker::new(&poly);
        let len = walker.length();
        let n = ((len / step).ceil() as usize).max(1);
        for k in 0..n {
            out.push(walker.at(len * k as f64 / n as f64));
        }
    }
    if !c.closed {
        out.extend(c.points.last().copied());
    }
    out
}

/// Unit normal at sample `i` (left of the direction of travel).
fn normal(p: &[Vec2], i: usize, closed: bool) -> Vec2 {
    let n = p.len();
    let prev = if i > 0 {
        p[i - 1]
    } else if closed {
        p[n - 1]
    } else {
        p[i]
    };
    let next = if i + 1 < n {
        p[i + 1]
    } else if closed {
        p[0]
    } else {
        p[i]
    };
    let d = next - prev;
    let l = d.length();
    if l < 1e-12 {
        v2(0.0, 0.0)
    } else {
        v2(d.y / l, -d.x / l)
    }
}

/// The outlines `contours` with `effect` applied. `amount` in the effect's unit, `size` mm
/// (Roughen and Zigzag step), `seed` for Roughen.
pub fn apply(
    contours: &[Contour],
    effect: Effect,
    amount: f64,
    size: f64,
    seed: u64,
) -> Vec<Contour> {
    let Some(b) = Rect::enclosing(contours.iter().flat_map(|c| c.outline_points())) else {
        return contours.to_vec();
    };
    let centre = b.center();
    let reach = contours
        .iter()
        .flat_map(|c| c.outline_points())
        .map(|p| p.distance(centre))
        .fold(0.0, f64::max)
        .max(1e-9);
    let step = (size * 10.0).max(1.0);
    let fine = (b.width().max(b.height()) / 400.0).max(2.0);
    let mut rng = Rng::new(seed);
    contours
        .iter()
        .map(|c| {
            let pts: Vec<Vec2> = match effect {
                Effect::Warp => {
                    let wave = (b.width() / 2.0).max(10.0);
                    resample(c, fine)
                        .into_iter()
                        .map(|p| {
                            v2(
                                p.x,
                                p.y + amount * 10.0 * (TAU * (p.x - b.min_x) / wave).dsin(),
                            )
                        })
                        .collect()
                }
                Effect::Twist => resample(c, fine)
                    .into_iter()
                    .map(|p| {
                        let a = (amount * p.distance(centre) / reach).to_radians();
                        centre + (p - centre).rotated(a)
                    })
                    .collect(),
                Effect::Roughen => {
                    let s = resample(c, step);
                    (0..s.len())
                        .map(|i| {
                            let r = (rng.next_f64() * 2.0 - 1.0) * amount * 10.0;
                            s[i] + normal(&s, i, c.closed) * r
                        })
                        .collect()
                }
                Effect::Zigzag => {
                    let s = resample(c, step);
                    (0..s.len())
                        .map(|i| {
                            let sign = if i % 2 == 0 { 1.0 } else { -1.0 };
                            s[i] + normal(&s, i, c.closed) * (sign * amount * 10.0)
                        })
                        .collect()
                }
                Effect::PuckerBloat => pucker_bloat(c, contours, amount, fine),
            };
            let mut out = Contour::new(pts, c.closed);
            if out.points.len() > MAX_POINTS {
                let keep = out.points.len().div_ceil(MAX_POINTS);
                out.points = out.points.into_iter().step_by(keep).collect();
            }
            out
        })
        .filter(|c| c.points.len() >= if c.closed { 3 } else { 2 })
        .collect()
}

/// Every segment's middle moved along its outward normal by `percent` % of its chord, easing
/// to nothing at the vertices (half a sine).
fn pucker_bloat(c: &Contour, all: &[Contour], percent: f64, step: f64) -> Vec<Vec2> {
    let mut out = Vec::new();
    for i in 0..c.segment_count() {
        let (a, b) = c.segment_ends(i);
        let chord = b - a;
        let len = chord.length();
        if len < 1e-9 {
            continue;
        }
        let mut n = v2(chord.y / len, -chord.x / len);
        // Outward: away from the region (for closed contours).
        let mid = a.lerp(b, 0.5);
        if c.closed && contains(mid + n * 0.5, all) {
            n = n * -1.0;
        }
        let poly = c.segment_polyline(i);
        let walker = Walker::new(&poly);
        let total = walker.length();
        let k = ((total / step).ceil() as usize).max(2);
        for j in 0..k {
            let t = j as f64 / k as f64;
            let p = walker.at(total * t);
            out.push(p + n * (percent / 100.0 * len * (PI * t).dsin()));
        }
    }
    if !c.closed {
        out.extend(c.points.last().copied());
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square() -> Vec<Contour> {
        vec![Contour::rectangle(0.0, 0.0, 200.0, 200.0)]
    }

    #[test]
    fn effects_are_deterministic_and_keep_closed_outlines_closed() {
        for e in Effect::ALL {
            let a = apply(&square(), e, e.amount().1, 2.0, 7);
            let b = apply(&square(), e, e.amount().1, 2.0, 7);
            assert_eq!(a, b, "{e:?}");
            assert!(a[0].closed && a[0].points.len() >= 4, "{e:?}");
            assert_ne!(a, square(), "{e:?}");
            assert_eq!(Effect::from_id(e.id()), Some(e));
        }
        let other = apply(&square(), Effect::Roughen, 1.0, 2.0, 8);
        assert_ne!(apply(&square(), Effect::Roughen, 1.0, 2.0, 7), other);
    }

    #[test]
    fn each_effect_moves_points_as_described() {
        // Roughen and zigzag stay within the amount of the outline.
        for e in [Effect::Roughen, Effect::Zigzag] {
            let r = apply(&square(), e, 1.0, 2.0, 1);
            for p in &r[0].points {
                let d = [
                    p.x.abs(),
                    (p.x - 200.0).abs(),
                    p.y.abs(),
                    (p.y - 200.0).abs(),
                ]
                .into_iter()
                .fold(f64::INFINITY, f64::min);
                assert!(d <= 10.0 + 1e-9, "{e:?} {p:?}");
            }
        }
        // Bloat pushes the middle of the top edge up (outwards), pucker pulls it in.
        let top = |c: &[Contour]| {
            c[0].points
                .iter()
                .filter(|p| (p.x - 100.0).abs() < 3.0)
                .map(|p| p.y)
                .fold(f64::INFINITY, f64::min)
        };
        assert!((top(&apply(&square(), Effect::PuckerBloat, 20.0, 0.0, 0)) + 40.0).abs() < 1.0);
        assert!(top(&apply(&square(), Effect::PuckerBloat, -20.0, 0.0, 0)) > 30.0);
        // Twist turns the corners (the farthest points) by the full amount about the centre.
        let t = apply(&square(), Effect::Twist, 90.0, 0.0, 0);
        assert!(
            t[0].points
                .iter()
                .any(|p| p.distance(v2(200.0, 0.0)) < 1e-6)
        );
        // Warp moves points up and down only.
        let w = apply(&square(), Effect::Warp, 1.0, 0.0, 0);
        assert!(
            w[0].points
                .iter()
                .all(|p| p.x >= -1e-9 && p.x <= 200.0 + 1e-9)
        );
        assert!(w[0].points.iter().any(|p| p.y < -5.0));
    }
}
