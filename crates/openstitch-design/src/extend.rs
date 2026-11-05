//! Continue digitizing (#479): more points added to either end of an open outline, a satin
//! skeleton or a Manual Punch column, keeping it one object. Points added at the start are
//! given in drawing order, outward from the start, and prepended reversed. Own code. Lengths
//! are 0.1 mm.

use crate::column::{self, RungError};
use crate::geometry::{Bezier, Contour, Vec2};
use crate::skeleton::Skeleton;

/// Which end of the object grows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Start,
    End,
}

impl End {
    pub fn from_id(id: &str) -> Option<End> {
        match id {
            "start" => Some(End::Start),
            "end" => Some(End::End),
            _ => None,
        }
    }
}

/// The open contour `c` continued from `end` through `points`; `curves` (empty, or one entry
/// per new segment, in drawing order) makes new segments Béziers. Segment `k` runs from the
/// previous point (the old end for `k = 0`) to `points[k]`. `None` for a closed contour, no
/// points, or a curve count that does not match.
pub fn path(c: &Contour, end: End, points: &[Vec2], curves: &[Option<Bezier>]) -> Option<Contour> {
    if c.closed || c.points.is_empty() || points.is_empty() {
        return None;
    }
    if !curves.is_empty() && curves.len() != points.len() {
        return None;
    }
    let old: Vec<Option<Bezier>> = (0..c.segment_count()).map(|i| c.curve(i)).collect();
    let new = |k: usize| curves.get(k).copied().flatten();
    let (pts, segs): (Vec<Vec2>, Vec<Option<Bezier>>) = match end {
        End::End => (
            c.points.iter().chain(points).copied().collect(),
            old.into_iter().chain((0..points.len()).map(new)).collect(),
        ),
        End::Start => (
            points.iter().rev().chain(&c.points).copied().collect(),
            // Drawn outward, sewn inward: reversed order, handles swapped.
            (0..points.len())
                .rev()
                .map(|k| new(k).map(|z| Bezier { c1: z.c2, c2: z.c1 }))
                .chain(old)
                .collect(),
        ),
    };
    let mut out = Contour::new(pts, false);
    if segs.iter().any(Option::is_some) {
        out.curves = segs;
    }
    Some(out)
}

/// The skeleton `s` continued from `end` through `points` with `widths` (mm, one per point).
pub fn skeleton(s: &Skeleton, end: End, points: &[Vec2], widths: &[f64]) -> Skeleton {
    match end {
        End::End => Skeleton {
            points: s.points.iter().chain(points).copied().collect(),
            widths: s.widths.iter().chain(widths).copied().collect(),
        },
        End::Start => Skeleton {
            points: points.iter().rev().chain(&s.points).copied().collect(),
            widths: widths.iter().rev().chain(&s.widths).copied().collect(),
        },
    }
}

/// The direction lines `rungs` continued from `end` with `lines` (drawn outward), validated as
/// a new column is (`curved`: as a Curved Block). Errors name lines by their index in the
/// result.
pub fn column(
    rungs: &[[Vec2; 2]],
    end: End,
    lines: &[[Vec2; 2]],
    curved: bool,
) -> Result<Vec<[Vec2; 2]>, RungError> {
    let out: Vec<[Vec2; 2]> = match end {
        End::End => rungs.iter().chain(lines).copied().collect(),
        End::Start => lines.iter().rev().chain(rungs).copied().collect(),
    };
    column::validate(&out)?;
    if curved {
        column::validate(&column::curved_rungs(&out))?;
    }
    Ok(out)
}

/// Whether `a` and `b` are the same outline within `tolerance` (same points and curves).
pub fn same_outline(a: &Contour, b: &Contour, tolerance: f64) -> bool {
    let close = |p: Vec2, q: Vec2| p.distance(q) <= tolerance;
    a.closed == b.closed
        && a.points.len() == b.points.len()
        && a.points.iter().zip(&b.points).all(|(p, q)| close(*p, *q))
        && (0..a.segment_count()).all(|i| match (a.curve(i), b.curve(i)) {
            (None, None) => true,
            (Some(x), Some(y)) => close(x.c1, y.c1) && close(x.c2, y.c2),
            _ => false,
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    #[test]
    fn paths_grow_at_either_end() {
        let c = Contour::new(vec![v2(0.0, 0.0), v2(10.0, 0.0)], false);
        let e = path(&c, End::End, &[v2(20.0, 0.0), v2(30.0, 5.0)], &[]).expect("open");
        assert_eq!(
            e.points,
            vec![v2(0.0, 0.0), v2(10.0, 0.0), v2(20.0, 0.0), v2(30.0, 5.0)]
        );
        let curve = Bezier {
            c1: v2(-2.0, 3.0),
            c2: v2(-8.0, 3.0),
        };
        let s = path(
            &c,
            End::Start,
            &[v2(-10.0, 0.0), v2(-20.0, 0.0)],
            &[Some(curve), None],
        )
        .expect("open");
        assert_eq!(s.points[0], v2(-20.0, 0.0));
        assert_eq!(s.points[2], v2(0.0, 0.0));
        // The first drawn segment (old start to -10) is now segment 1, run backwards.
        assert_eq!(s.curve(0), None);
        assert_eq!(
            s.curve(1),
            Some(Bezier {
                c1: v2(-8.0, 3.0),
                c2: v2(-2.0, 3.0)
            })
        );
        assert_eq!(s.curve(2), None);
        let closed = Contour::rectangle(0.0, 0.0, 10.0, 10.0);
        assert!(path(&closed, End::End, &[v2(1.0, 1.0)], &[]).is_none());
        assert!(path(&c, End::End, &[v2(1.0, 1.0)], &[None, None]).is_none());
    }

    #[test]
    fn columns_grow_and_crossing_sides_are_refused() {
        let rungs = [
            [v2(0.0, 0.0), v2(0.0, 30.0)],
            [v2(20.0, 0.0), v2(20.0, 30.0)],
        ];
        let more =
            column(&rungs, End::End, &[[v2(40.0, 0.0), v2(40.0, 30.0)]], false).expect("valid");
        assert_eq!(more.len(), 3);
        let before = column(
            &rungs,
            End::Start,
            &[[v2(-20.0, 0.0), v2(-20.0, 30.0)]],
            false,
        )
        .expect("valid");
        assert_eq!(before[0], [v2(-20.0, 0.0), v2(-20.0, 30.0)]);
        // Sides swapped: the new line's ends cross the old sides.
        let crossed = column(&rungs, End::End, &[[v2(40.0, 30.0), v2(40.0, 0.0)]], false);
        assert!(matches!(crossed, Err(RungError::Crossing(_))));
    }
}
