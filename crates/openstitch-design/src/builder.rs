//! Shape builder (beyond PE-DESIGN, #497): the overlapping shapes are cut into the pieces of
//! their planar arrangement ([`crate::boolean::divide`]); a drag across pieces merges the ones
//! it touches into one, or removes them. Own code. Lengths are 0.1 mm.

use crate::boolean::{self, Region};
use crate::geometry::{Contour, Vec2, contains};

/// What a drag does to the pieces it crosses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Build {
    Merge,
    Remove,
}

/// Points along the drag `path` at most 0.2 mm apart.
fn samples(path: &[Vec2]) -> Vec<Vec2> {
    let mut out: Vec<Vec2> = path.first().copied().into_iter().collect();
    for w in path.windows(2) {
        let n = (w[0].distance(w[1]) / 2.0).ceil().max(1.0) as usize;
        for k in 1..=n {
            out.push(w[0].lerp(w[1], k as f64 / n as f64));
        }
    }
    out
}

/// The pieces of `objects` (in sewing order) after a drag along `path`: each with the index of
/// the object whose attributes it takes. Merged pieces take the topmost owner among them.
/// `None` when the drag touches no piece.
pub fn build(objects: &[&[Contour]], path: &[Vec2], mode: Build) -> Option<Vec<(usize, Region)>> {
    let pieces = boolean::divide(objects);
    let pts = samples(path);
    let hit: Vec<bool> = pieces
        .iter()
        .map(|(_, r)| pts.iter().any(|&p| contains(p, r)))
        .collect();
    if !hit.iter().any(|&h| h) {
        return None;
    }
    let mut out: Vec<(usize, Region)> = pieces
        .iter()
        .zip(&hit)
        .filter(|(_, h)| !**h)
        .map(|(p, _)| p.clone())
        .collect();
    if mode == Build::Merge {
        let chosen: Vec<&(usize, Region)> = pieces
            .iter()
            .zip(&hit)
            .filter(|(_, h)| **h)
            .map(|(p, _)| p)
            .collect();
        let owner = chosen
            .iter()
            .map(|(o, _)| *o)
            .max()
            .expect("one hit at least");
        let merged = boolean::union_all(chosen.iter().map(|(_, r)| &r[..]));
        out.extend(merged.into_iter().map(|r| (owner, r)));
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::boolean::region_area;
    use crate::geometry::v2;

    fn squares() -> (Vec<Contour>, Vec<Contour>) {
        (
            vec![Contour::rectangle(0.0, 0.0, 100.0, 100.0)],
            vec![Contour::rectangle(50.0, 50.0, 100.0, 100.0)],
        )
    }

    #[test]
    fn a_drag_merges_or_removes_the_pieces_it_crosses() {
        let (a, b) = squares();
        // Across the first square's own part and the overlap: those two become one.
        let r = build(&[&a, &b], &[v2(20.0, 20.0), v2(70.0, 70.0)], Build::Merge).expect("hit");
        assert_eq!(r.len(), 2);
        let areas: Vec<f64> = r.iter().map(|(_, x)| region_area(x)).collect();
        assert!(
            areas.iter().any(|a| (a - 10_000.0).abs() < 1.0),
            "{areas:?}"
        );
        assert!(areas.iter().any(|a| (a - 7_500.0).abs() < 1.0), "{areas:?}");
        // Removing the overlap leaves the two outer parts.
        let r = build(&[&a, &b], &[v2(70.0, 70.0)], Build::Remove).expect("hit");
        let total: f64 = r.iter().map(|(_, x)| region_area(x)).sum();
        assert!((total - 15_000.0).abs() < 1.0);
        assert_eq!(r.len(), 2);
        // Missing every piece does nothing.
        assert!(build(&[&a, &b], &[v2(300.0, 300.0)], Build::Merge).is_none());
    }
}
