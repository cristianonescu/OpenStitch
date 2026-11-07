//! Decorative hand-embroidery lines (#543, beyond PE-DESIGN, off unless chosen): chevron,
//! Cretan, feather and overlock stitch, alternating triangles, double zigzag, lattice and
//! stripes, each along any open or closed line. A pattern is laid out in the line's own frame
//! (distance along it, offset across it) and placed along the path, so it follows curves; no
//! stitch is longer than [`MAX_STITCH`]. Lengths are in 0.1 mm.

use crate::generate::subdivide;
use crate::geometry::Vec2;
use crate::paths::{Walker, normal};

/// The longest stitch a decorative line sews (0.1 mm): longer ones are split.
pub const MAX_STITCH: f64 = 70.0;

/// Which decorative pattern, and its own settings.
#[derive(Clone, Debug, PartialEq)]
pub enum Pattern {
    /// A zigzag with a short bar sewn along the line at every peak.
    Chevron,
    /// A zigzag with a short stitch pointing in from every peak.
    Cretan,
    /// Like Cretan, the side stitches turned forward: the branching hand feather stitch.
    Feather,
    /// A serger-like edge: up the band slanting forward, straight back down.
    Overlock,
    /// A zigzag of triangles between two outline rails.
    AlternatingTriangles,
    /// A zigzag sewn there and back, the return a mirror image: a chain of diamonds.
    DoubleZigzag,
    /// `strands` zigzags across the band, each shifted along, crossing into a braid.
    Lattice { strands: u32 },
    /// Running rows along the band: `strands` evenly across it, or at the `gaps` (mm) given.
    Stripes { strands: u32, gaps: Vec<f64> },
}

/// A decorative line's size: band `width`, pattern `spacing` along the line (one zigzag
/// period: peak to the next peak on the same side), side-stitch `length`, running `pitch`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Size {
    pub width: f64,
    pub spacing: f64,
    pub length: f64,
    pub pitch: f64,
}

/// The line's frame: a point `along` the path moved `across` it (positive to the left of
/// the drawing direction, as [`normal`] turns).
struct Frame<'a> {
    w: Walker<'a>,
    len: f64,
}

impl Frame<'_> {
    fn at(&self, along: f64, across: f64) -> Vec2 {
        let s = along.clamp(0.0, self.len);
        self.w.at(s) + normal(self.w.tangent(s)) * across
    }

    /// `n + 1` stations from 0 to the end, `n` the nearest whole number of `step`s (≥ 1).
    fn stations(&self, step: f64) -> Vec<f64> {
        let n = (self.len / step.max(1.0)).round().max(1.0) as usize;
        (0..=n).map(|k| self.len * k as f64 / n as f64).collect()
    }
}

/// The decorative line `pattern` along `path`, as one run from its start (and, for every
/// pattern but stripes with an even count, ending at its far end).
pub fn sew(path: &[Vec2], pattern: &Pattern, size: Size) -> Vec<Vec2> {
    let w = Walker::new(path);
    let len = w.length();
    if len <= 1e-9 || path.len() < 2 {
        return path.to_vec();
    }
    let f = Frame { w, len };
    let half = size.width / 2.0;
    let pts = match pattern {
        Pattern::Chevron => zigzag_with(&f, size, |f, s, y, out| {
            // A bar along the line at the peak, centred on it.
            let l = size.length / 2.0;
            out.extend([f.at(s + l, y), f.at(s - l, y), f.at(s, y)]);
        }),
        Pattern::Cretan => zigzag_with(&f, size, |f, s, y, out| {
            out.extend([f.at(s, y - y.signum() * size.length), f.at(s, y)]);
        }),
        Pattern::Feather => zigzag_with(&f, size, |f, s, y, out| {
            let l = size.length * std::f64::consts::FRAC_1_SQRT_2;
            out.extend([f.at(s + l, y - y.signum() * l), f.at(s, y)]);
        }),
        Pattern::Overlock => {
            let st = f.stations(size.spacing / 2.0);
            let mut out = vec![f.at(0.0, -half)];
            for w in st.windows(2) {
                out.push(f.at(w[1], half));
                out.push(f.at(w[1], -half));
            }
            out
        }
        Pattern::AlternatingTriangles => {
            let mut out = zigzag(&f, size.spacing, half, 0.0);
            // Back along one rail and forward along the other: the double outline.
            let rail = |y: f64, back: bool| {
                let mut st = f.stations(size.pitch);
                if back {
                    st.reverse();
                }
                st.into_iter().map(move |s| (s, y))
            };
            out.extend(rail(half, true).skip(1).map(|(s, y)| f.at(s, y)));
            out.push(f.at(0.0, -half));
            out.extend(rail(-half, false).skip(1).map(|(s, y)| f.at(s, y)));
            out
        }
        Pattern::DoubleZigzag => {
            let mut out = zigzag(&f, size.spacing, half, 0.0);
            // The return pass mirrors the first: each of its points midway along between
            // two of the first pass's on the same side.
            let mut back = zigzag(&f, size.spacing, -half, 0.0);
            back.reverse();
            out.extend(back.into_iter().skip(1));
            // An odd number of passes ends at the far end: a third pass as the first.
            let third = zigzag(&f, size.spacing, half, 0.0);
            out.extend(third.into_iter().skip(1));
            out
        }
        Pattern::Lattice { strands } => {
            let n = (*strands).clamp(2, 7) | 1;
            let mut out: Vec<Vec2> = Vec::new();
            for p in 0..n {
                let shift = size.spacing * p as f64 / (2 * n) as f64;
                let mut pass = zigzag(&f, size.spacing, half, shift);
                if !p.is_multiple_of(2) {
                    pass.reverse();
                }
                let skip = usize::from(!out.is_empty());
                out.extend(pass.into_iter().skip(skip));
            }
            out
        }
        Pattern::Stripes { strands, gaps } => {
            let rows = stripe_offsets(size.width, *strands, gaps);
            let mut out: Vec<Vec2> = Vec::new();
            for (k, y) in rows.into_iter().enumerate() {
                let mut st = f.stations(size.pitch);
                if !k.is_multiple_of(2) {
                    st.reverse();
                }
                out.extend(st.into_iter().map(|s| f.at(s, y)));
            }
            out
        }
    };
    let mut run = subdivide(&pts, MAX_STITCH);
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

/// Offsets across the band of the stripes: `gaps` (mm, between neighbouring rows) when
/// given, centred on the line; else `strands` rows evenly from edge to edge.
pub fn stripe_offsets(width: f64, strands: u32, gaps: &[f64]) -> Vec<f64> {
    if !gaps.is_empty() {
        let mut at = vec![0.0];
        for g in gaps {
            at.push(at[at.len() - 1] + g * 10.0);
        }
        let mid = at[at.len() - 1] / 2.0;
        return at.into_iter().map(|y| y - mid).collect();
    }
    let n = strands.clamp(2, 12);
    (0..n)
        .map(|k| -width / 2.0 + width * k as f64 / (n - 1) as f64)
        .collect()
}

/// A zigzag between `±half` (starting on `+half`, i.e. `half` may be negative for the
/// mirror), peaks `spacing / 2` apart along the line from `shift` on, starting and ending on
/// the line's ends.
fn zigzag(f: &Frame, spacing: f64, half: f64, shift: f64) -> Vec<Vec2> {
    let st = f.stations(spacing / 2.0);
    let mut out = Vec::with_capacity(st.len() + 2);
    if shift > 0.0 {
        out.push(f.at(0.0, 0.0));
    }
    for (k, &s) in st.iter().enumerate() {
        let s = (s + shift).min(f.len);
        let y = if k.is_multiple_of(2) { half } else { -half };
        out.push(f.at(s, y));
    }
    if shift > 0.0 {
        out.push(f.at(f.len, 0.0));
    }
    out
}

/// A zigzag between the band's sides with `extra` sewn at each inner peak.
fn zigzag_with(
    f: &Frame,
    size: Size,
    extra: impl Fn(&Frame, f64, f64, &mut Vec<Vec2>),
) -> Vec<Vec2> {
    let half = size.width / 2.0;
    let st = f.stations(size.spacing / 2.0);
    let mut out = Vec::new();
    for (k, &s) in st.iter().enumerate() {
        let y = if k.is_multiple_of(2) { half } else { -half };
        out.push(f.at(s, y));
        if k > 0 && k + 1 < st.len() {
            extra(f, s, y, &mut out);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    fn size() -> Size {
        Size {
            width: 40.0,
            spacing: 40.0,
            length: 15.0,
            pitch: 20.0,
        }
    }

    fn all() -> Vec<Pattern> {
        vec![
            Pattern::Chevron,
            Pattern::Cretan,
            Pattern::Feather,
            Pattern::Overlock,
            Pattern::AlternatingTriangles,
            Pattern::DoubleZigzag,
            Pattern::Lattice { strands: 3 },
            Pattern::Stripes {
                strands: 3,
                gaps: Vec::new(),
            },
        ]
    }

    fn paths() -> Vec<Vec<Vec2>> {
        let curve: Vec<Vec2> = (0..=40)
            .map(|k| {
                let t = k as f64 / 40.0;
                v2(t * 600.0, 150.0 * (t * std::f64::consts::PI).sin())
            })
            .collect();
        let closed = vec![
            v2(0.0, 0.0),
            v2(400.0, 0.0),
            v2(400.0, 300.0),
            v2(0.0, 300.0),
            v2(0.0, 0.0),
        ];
        vec![vec![v2(0.0, 0.0), v2(600.0, 0.0)], curve, closed]
    }

    #[test]
    fn every_pattern_sews_straight_curved_and_closed_lines_in_bounds() {
        for path in paths() {
            for p in all() {
                let run = sew(&path, &p, size());
                assert!(run.len() > 10, "{p:?}");
                for w in run.windows(2) {
                    let d = w[0].distance(w[1]);
                    assert!(d <= MAX_STITCH + 1e-6, "{p:?}: {d}");
                }
                // No needle point sewn three times in a row (a stacked drop).
                for w in run.windows(3) {
                    assert!(
                        !(w[0].distance(w[1]) < 1e-6 && w[1].distance(w[2]) < 1e-6),
                        "{p:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn double_zigzag_returns_midway_without_a_jump() {
        let path = vec![v2(0.0, 0.0), v2(400.0, 0.0)];
        let run = sew(&path, &Pattern::DoubleZigzag, size());
        // Pass 1 forward, pass 2 back (mirrored), pass 3 forward: ends at the far end.
        assert!((run[run.len() - 1].x - 400.0).abs() < 1e-6);
        // The first pass: up to and with its point at the far end.
        let end = run
            .iter()
            .position(|p| p.x > 400.0 - 1e-6)
            .expect("far end");
        let first: Vec<Vec2> = run[..=end].to_vec();
        let pass1_top: Vec<f64> = first.iter().filter(|p| p.y > 19.0).map(|p| p.x).collect();
        let pass2_top: Vec<f64> = run[first.len()..]
            .iter()
            .take_while(|p| p.x > 1e-6)
            .filter(|p| p.y > 19.0)
            .map(|p| p.x)
            .collect();
        // Each return point on the top side sits midway between two first-pass top points.
        for x in pass2_top {
            assert!(
                pass1_top
                    .windows(2)
                    .any(|w| ((w[0] + w[1]) / 2.0 - x).abs() < 1e-6),
                "{x}"
            );
        }
        assert!(
            run.windows(2)
                .all(|w| w[0].distance(w[1]) <= MAX_STITCH + 1e-6)
        );
    }

    #[test]
    fn stripes_keep_their_count_and_gaps() {
        let path = vec![v2(0.0, 0.0), v2(300.0, 0.0)];
        let run = sew(
            &path,
            &Pattern::Stripes {
                strands: 4,
                gaps: Vec::new(),
            },
            Size {
                width: 30.0,
                ..size()
            },
        );
        let mut rows: Vec<i64> = run.iter().map(|p| (p.y * 10.0).round() as i64).collect();
        rows.sort_unstable();
        rows.dedup();
        assert_eq!(rows.len(), 4, "{rows:?}");
        // Pinstripes 1-2-1-2-1 mm: rows at those gaps, ±0.2 mm.
        let offs = stripe_offsets(30.0, 3, &[1.0, 2.0, 1.0, 2.0, 1.0]);
        let gaps: Vec<f64> = offs.windows(2).map(|w| (w[1] - w[0]) / 10.0).collect();
        for (g, want) in gaps.iter().zip([1.0, 2.0, 1.0, 2.0, 1.0]) {
            assert!((g - want).abs() <= 0.02, "{gaps:?}");
        }
        let pin = sew(
            &path,
            &Pattern::Stripes {
                strands: 3,
                gaps: vec![1.0, 2.0, 1.0, 2.0, 1.0],
            },
            size(),
        );
        let mut ys: Vec<i64> = pin.iter().map(|p| (p.y * 10.0).round() as i64).collect();
        ys.sort_unstable();
        ys.dedup();
        assert_eq!(ys.len(), 6);
    }

    #[test]
    fn cretan_and_feather_side_stitches_point_in() {
        let path = vec![v2(0.0, 0.0), v2(400.0, 0.0)];
        for p in [Pattern::Cretan, Pattern::Feather, Pattern::Chevron] {
            let run = sew(&path, &p, size());
            // Inner points (side stitches) lie inside the band, not past it.
            assert!(run.iter().all(|q| q.y.abs() <= 20.0 + 1e-6), "{p:?}");
            assert!(
                run.iter().any(|q| q.y.abs() > 1.0 && q.y.abs() < 19.0) || p == Pattern::Chevron,
                "{p:?}"
            );
        }
    }
}
