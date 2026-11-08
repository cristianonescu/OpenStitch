//! Snap to Objects (#473): while the selection is dragged, its left/centre/right and
//! top/middle/bottom lines snap to the same lines of the other visible objects, to the Design
//! Page's edges and centre and to the ruler guidelines, within a tolerance in screen points.
//! Object snapping wins over grid snapping on an axis it snaps; the other axis may still snap
//! to the grid. Beyond PE-DESIGN, off unless chosen.

use openstitch_design::{Design, Rect, Vec2, v2};
use std::collections::BTreeSet;

/// Tolerance in screen points, the same at every zoom.
pub const TOLERANCE_POINTS: f64 = 6.0;

/// The lines a dragged selection can snap to, collected once when the drag starts.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SnapLines {
    /// x of vertical lines (model units).
    pub xs: Vec<f64>,
    /// y of horizontal lines.
    pub ys: Vec<f64>,
}

/// A line the selection lined up with, drawn dashed while dragging.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SnapGuide {
    Vertical(f64),
    Horizontal(f64),
}

fn three(lo: f64, hi: f64) -> [f64; 3] {
    [lo, (lo + hi) / 2.0, hi]
}

impl SnapLines {
    /// Edges and centres of every visible object outside `selection`, the Design Page's
    /// edges and centre, and the guidelines.
    pub fn collect(d: &Design, selection: &BTreeSet<u64>) -> Self {
        let mut l = SnapLines::default();
        for b in d
            .objects
            .iter()
            .filter(|o| o.visible && !selection.contains(&o.id))
            .filter_map(|o| o.bounds())
        {
            l.xs.extend(three(b.min_x, b.max_x));
            l.ys.extend(three(b.min_y, b.max_y));
        }
        l.xs.extend(three(0.0, d.page.width));
        l.ys.extend(three(0.0, d.page.height));
        l.xs.extend(&d.guides.vertical);
        l.ys.extend(&d.guides.horizontal);
        for v in [&mut l.xs, &mut l.ys] {
            v.sort_by(f64::total_cmp);
            v.dedup();
        }
        l
    }
}

impl SnapLines {
    /// The guidelines alone (Snap to guides without Snap to Objects, #799).
    pub fn guides(d: &Design) -> Self {
        let mut l = SnapLines {
            xs: d.guides.vertical.clone(),
            ys: d.guides.horizontal.clone(),
        };
        for v in [&mut l.xs, &mut l.ys] {
            v.sort_by(f64::total_cmp);
            v.dedup();
        }
        l
    }
}

/// The nearest candidate to any of `own` within `tol`: (shift to apply, line).
fn nearest(own: [f64; 3], lines: &[f64], tol: f64) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    for o in own {
        // `lines` is sorted: only the neighbours of `o` can be nearest.
        let i = lines.partition_point(|&x| x < o);
        for &c in lines[i.saturating_sub(1)..(i + 1).min(lines.len())].iter() {
            let shift = c - o;
            if shift.abs() <= tol && best.is_none_or(|(b, _)| shift.abs() < b.abs()) {
                best = Some((shift, c));
            }
        }
    }
    best
}

/// Snaps the move `d` of a selection with bounds `b`: per axis, the adjusted delta when one
/// of the moved selection's lines is within `tol` of a candidate (`None` = that axis did not
/// snap), and the guides to draw.
pub fn snap_move(
    b: Rect,
    d: Vec2,
    lines: &SnapLines,
    tol: f64,
) -> (Option<f64>, Option<f64>, Vec<SnapGuide>) {
    let mut guides = Vec::new();
    let x = nearest(three(b.min_x + d.x, b.max_x + d.x), &lines.xs, tol).map(|(s, c)| {
        guides.push(SnapGuide::Vertical(c));
        d.x + s
    });
    let y = nearest(three(b.min_y + d.y, b.max_y + d.y), &lines.ys, tol).map(|(s, c)| {
        guides.push(SnapGuide::Horizontal(c));
        d.y + s
    });
    (x, y, guides)
}

/// The grid point of `p` (step in model units).
pub fn grid_point(p: Vec2, step: f64) -> Vec2 {
    v2((p.x / step).round() * step, (p.y / step).round() * step)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::request::CommandRequest;
    use openstitch_engine::Session;
    use serde_json::json;

    fn design() -> Design {
        let mut s = Session::new(std::env::temp_dir());
        for (x, y) in [(10.0, 10.0), (50.0, 40.0)] {
            let r = CommandRequest::new(
                "shape.rectangle",
                json!({ "x": x, "y": y, "width": 20, "height": 10 }),
            );
            s.execute(r.command, Some(&r.params)).expect("rectangle");
        }
        s.design().clone()
    }

    #[test]
    fn candidates_are_unselected_edges_centres_page_and_guides() {
        let mut d = design();
        d.guides.vertical.push(333.0);
        let l = SnapLines::collect(&d, &BTreeSet::from([2]));
        for x in [100.0, 200.0, 300.0, 0.0, 500.0, 1000.0, 333.0] {
            assert!(l.xs.contains(&x), "{x} in {:?}", l.xs);
        }
        assert!(
            !l.xs.contains(&700.0),
            "the selection itself is no candidate"
        );
        assert!(l.ys.contains(&150.0) && l.ys.contains(&500.0));
        let mut hidden = d.clone();
        hidden.objects[0].visible = false;
        assert!(
            !SnapLines::collect(&hidden, &BTreeSet::new())
                .xs
                .contains(&100.0)
        );
    }

    #[test]
    fn snaps_within_the_tolerance_at_every_zoom_and_exactly() {
        let d = design();
        let l = SnapLines::collect(&d, &BTreeSet::from([2]));
        let b = d.objects[1].bounds().expect("bounds"); // 500..700 × 400..500
        // At 1, 0.25 and 4 model units per point the tolerance is 6 points.
        for upp in [1.0, 0.25, 4.0] {
            let tol = TOLERANCE_POINTS * upp;
            // Left edge 500 dragged to 300 + 0.9·tol: snaps onto rectangle 1's right edge.
            let dx = 300.0 + 0.9 * tol - 500.0;
            let (x, _, g) = snap_move(b, v2(dx, 0.0), &l, tol);
            assert_eq!(x, Some(-200.0), "upp {upp}");
            assert!(g.contains(&SnapGuide::Vertical(300.0)));
            // Just outside: no snap on x.
            let (x, _, _) = snap_move(b, v2(300.0 + 1.1 * tol - 500.0, 0.0), &l, tol);
            assert_ne!(x, Some(-200.0), "upp {upp}");
        }
        // Centres snap to centres: middle 450 near 150 (rectangle 1's middle).
        let only_middle = SnapLines {
            xs: Vec::new(),
            ys: vec![150.0],
        };
        let (_, y, g) = snap_move(b, v2(0.0, -298.0), &only_middle, 6.0);
        assert_eq!(y, Some(-300.0));
        assert_eq!(g, [SnapGuide::Horizontal(150.0)]);
        // Page centre (500).
        let (x, _, _) = snap_move(b, v2(-102.0, 0.0), &l, 6.0);
        assert_eq!(x, Some(-100.0), "the centre lands on the page centre");
    }

    #[test]
    fn grid_point_rounds_to_the_step() {
        assert_eq!(grid_point(v2(14.0, 26.0), 10.0), v2(10.0, 30.0));
    }
}
