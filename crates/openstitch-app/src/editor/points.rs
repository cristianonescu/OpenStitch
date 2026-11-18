//! Point-level geometry for the Point and Pen tools: vertex / handle / segment hit-testing and
//! the contour edits the engine commands make, mirrored here for live previews.

use openstitch_design::{Bezier, Contour, DesignObject, Vec2};

/// Which control point of a segment's cubic Bézier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Control {
    /// `c1`, attached to the segment's start vertex.
    Start,
    /// `c2`, attached to the segment's end vertex.
    End,
}

/// A vertex of one contour of a shape.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PointRef {
    pub id: u64,
    pub contour: usize,
    pub index: usize,
}

/// What the pointer is over in point-editing mode, nearest first.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum PointHit {
    Vertex {
        contour: usize,
        index: usize,
    },
    Handle {
        contour: usize,
        segment: usize,
        control: Control,
    },
    Segment {
        contour: usize,
        segment: usize,
        t: f64,
    },
}

/// One visible curve handle: the line from `anchor` (a vertex) to `control`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct HandleLine {
    pub segment: usize,
    pub control: Control,
    pub anchor: Vec2,
    pub at: Vec2,
}

/// Point on the cubic Bézier `a, b, c, d` at parameter `t`.
pub fn cubic_at(a: Vec2, b: Vec2, c: Vec2, d: Vec2, t: f64) -> Vec2 {
    let u = 1.0 - t;
    a * (u * u * u) + b * (3.0 * u * u * t) + c * (3.0 * u * t * t) + d * (t * t * t)
}

/// Point on segment `i` at parameter `t` (the Bézier parameter for curves, as
/// `object.insertPoint` takes it).
pub fn segment_at(c: &Contour, i: usize, t: f64) -> Vec2 {
    let (a, b) = c.segment_ends(i);
    match c.curve(i) {
        None => a.lerp(b, t),
        Some(z) => cubic_at(a, z.c1, z.c2, b, t),
    }
}

/// Parameter and distance of the point of segment `i` nearest to `p`.
pub fn nearest_on_segment(c: &Contour, i: usize, p: Vec2) -> (f64, f64) {
    let (a, b) = c.segment_ends(i);
    if c.curve(i).is_none() {
        let ab = b - a;
        let len2 = ab.x * ab.x + ab.y * ab.y;
        let t = if len2 <= 0.0 {
            0.0
        } else {
            (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
        };
        return (t, p.distance(a.lerp(b, t)));
    }
    // Coarse samples, then golden-section search around the best one.
    const SAMPLES: usize = 48;
    let d = |t: f64| p.distance(segment_at(c, i, t));
    let best = (0..=SAMPLES)
        .map(|k| k as f64 / SAMPLES as f64)
        .min_by(|x, y| d(*x).total_cmp(&d(*y)))
        .unwrap_or(0.0);
    let step = 1.0 / SAMPLES as f64;
    let (mut lo, mut hi) = ((best - step).max(0.0), (best + step).min(1.0));
    let g = 0.618_033_988_75;
    for _ in 0..40 {
        let m1 = hi - g * (hi - lo);
        let m2 = lo + g * (hi - lo);
        if d(m1) <= d(m2) {
            hi = m2;
        } else {
            lo = m1;
        }
    }
    let t = (lo + hi) / 2.0;
    (t, d(t))
}

/// Nearest segment of `c` to `p`: `(segment, t, distance)`.
pub fn nearest_segment(c: &Contour, p: Vec2) -> Option<(usize, f64, f64)> {
    (0..c.segment_count())
        .map(|i| {
            let (t, d) = nearest_on_segment(c, i, p);
            (i, t, d)
        })
        .min_by(|a, b| a.2.total_cmp(&b.2))
}

/// The curve handles of `c`: two per curved segment, less those a straight point borrows.
pub fn handle_lines(c: &Contour) -> Vec<HandleLine> {
    let mut out = Vec::new();
    for i in 0..c.segment_count() {
        if let Some(z) = c.curve(i) {
            let (a, b) = c.segment_ends(i);
            // A straight point beside a curve point has no handle (PE-DESIGN shows none).
            if !c.borrows_handle(i, true) {
                out.push(HandleLine {
                    segment: i,
                    control: Control::Start,
                    anchor: a,
                    at: z.c1,
                });
            }
            if !c.borrows_handle(i, false) {
                out.push(HandleLine {
                    segment: i,
                    control: Control::End,
                    anchor: b,
                    at: z.c2,
                });
            }
        }
    }
    out
}

/// What of `o` lies under `p`: a vertex or handle within `radius`, the nearer winning (a vertex
/// on a tie), else a segment within `tolerance`.
pub fn point_at(o: &DesignObject, p: Vec2, radius: f64, tolerance: f64) -> Option<PointHit> {
    let mut best: Option<(f64, PointHit)> = None;
    let mut offer = |d: f64, hit: PointHit| {
        if d <= radius && best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, hit));
        }
    };
    for (ci, c) in o.contours.iter().enumerate() {
        for (index, &v) in c.points.iter().enumerate() {
            offer(v.distance(p), PointHit::Vertex { contour: ci, index });
        }
    }
    for (ci, c) in o.contours.iter().enumerate() {
        for h in handle_lines(c) {
            offer(
                h.at.distance(p),
                PointHit::Handle {
                    contour: ci,
                    segment: h.segment,
                    control: h.control,
                },
            );
        }
    }
    if let Some((_, hit)) = best {
        return Some(hit);
    }
    o.contours
        .iter()
        .enumerate()
        .filter_map(|(ci, c)| nearest_segment(c, p).map(|(s, t, d)| (ci, s, t, d)))
        .filter(|&(.., d)| d <= tolerance)
        .min_by(|a, b| a.3.total_cmp(&b.3))
        .map(|(contour, segment, t, _)| PointHit::Segment {
            contour,
            segment,
            t,
        })
}

/// `c` with vertex `index` moved to `to`, its handles following (as `object.setPoint` does).
pub fn with_vertex(c: &Contour, index: usize, to: Vec2) -> Contour {
    let mut c = c.clone();
    c.move_point(index, to);
    c
}

/// `c` with one control point of curved segment `segment` moved to `to`.
/// The other handle at that vertex follows when the point is smooth or symmetric, as
/// `object.setSegment` moves it.
pub fn with_control(c: &Contour, segment: usize, control: Control, to: Vec2) -> Contour {
    let mut c = c.clone();
    c.move_handle(segment, control == Control::Start, to);
    c
}

/// Handles that turn straight segment `i` into a smooth curve: Catmull-Rom tangents through
/// the neighbouring vertices (the chord at an open path's ends).
pub fn smooth_curve(c: &Contour, i: usize) -> Bezier {
    let n = c.points.len();
    let (a, b) = c.segment_ends(i);
    let j = (i + 1) % n;
    let prev = if i > 0 || c.closed {
        Some(c.points[(i + n - 1) % n])
    } else {
        None
    };
    let next = if j + 1 < n || c.closed {
        Some(c.points[(j + 1) % n])
    } else {
        None
    };
    // Tangents are only borrowed from a neighbour that is not the segment's other end.
    let ta = prev.filter(|_| n > 2).map_or(b - a, |p| (b - p) * 0.5);
    let tb = next.filter(|_| n > 2).map_or(b - a, |q| (q - a) * 0.5);
    Bezier {
        c1: a + ta * (1.0 / 3.0),
        c2: b - tb * (1.0 / 3.0),
    }
}

// MARK: the point menu (#1258)

/// One entry of the Select Point tool's right-click menu, PE-DESIGN's seven.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointAction {
    ToStraight,
    ToCurve,
    ToSmooth,
    ToSymmetry,
    ToCusp,
    Split,
    Delete,
}

impl PointAction {
    /// The menu's entries in PE-DESIGN's order.
    pub const MENU: [PointAction; 7] = [
        PointAction::ToStraight,
        PointAction::ToCurve,
        PointAction::ToSmooth,
        PointAction::ToSymmetry,
        PointAction::ToCusp,
        PointAction::Split,
        PointAction::Delete,
    ];

    /// Whether PE-DESIGN's menu has a separator above this entry.
    pub fn separator_before(self) -> bool {
        matches!(
            self,
            PointAction::ToSmooth | PointAction::Split | PointAction::Delete
        )
    }

    /// The entry's stable key (`ui.state`, scripts, tests).
    pub fn key(self) -> &'static str {
        match self {
            PointAction::ToStraight => "toStraight",
            PointAction::ToCurve => "toCurve",
            PointAction::ToSmooth => "toSmooth",
            PointAction::ToSymmetry => "toSymmetry",
            PointAction::ToCusp => "toCusp",
            PointAction::Split => "splitAtPoint",
            PointAction::Delete => "deletePoint",
        }
    }

    /// The `kind` this entry gives `object.setPointKind`; `None` for Split and Delete.
    pub fn kind(self) -> Option<&'static str> {
        match self {
            PointAction::ToStraight => Some("straight"),
            PointAction::ToCurve => Some("curve"),
            PointAction::ToSmooth => Some("smooth"),
            PointAction::ToSymmetry => Some("symmetric"),
            PointAction::ToCusp => Some("cusp"),
            PointAction::Split | PointAction::Delete => None,
        }
    }
}

/// Which menu entries are available for the vertices `selected` of `c`, in
/// [`PointAction::MENU`] order; the menu and the ribbon's Edit Point drop-down share it. The
/// rule is PE-DESIGN 11.50's, read from its menu on 2026-11-18 (docs/COMPATIBILITY_AUDIT.md,
/// #1258):
///
/// - To Straight: one of the points is a curve point. To Curve: one of them is a straight
///   point (a mixed selection offers both).
/// - To Smooth, To Symmetry, To Cusp: one of the points is a curve point with a side on
///   either hand (not the end of an open line) that is not already of that kind.
/// - Split at Point: exactly one point, and not an end of an open line.
/// - Delete: always, with a point selected; where the outline cannot lose a point it does
///   nothing (see [`can_delete_points`]).
pub fn point_actions_enabled(c: &Contour, selected: &[usize]) -> [bool; 7] {
    use openstitch_design::PointKind;
    let valid: Vec<usize> = selected
        .iter()
        .copied()
        .filter(|&i| i < c.points.len())
        .collect();
    if valid.is_empty() {
        return [false; 7];
    }
    let any_curve = valid.iter().any(|&i| c.is_curve_point(i));
    let any_straight = valid.iter().any(|&i| !c.is_curve_point(i));
    let to_kind = |k: PointKind| {
        valid.iter().any(|&i| {
            c.is_curve_point(i)
                && c.incoming(i).is_some()
                && c.outgoing(i).is_some()
                && c.kind(i) != k
        })
    };
    let split = valid.len() == 1 && c.split_at(valid[0]).is_some();
    [
        any_curve,
        any_straight,
        to_kind(PointKind::Smooth),
        to_kind(PointKind::Symmetric),
        to_kind(PointKind::Cusp),
        split,
        true,
    ]
}

/// Whether opening contour `contour` of `o` (Split at Point on a closed outline, Open)
/// would leave nothing to sew: the shape is sewn as a region only and has no other closed
/// outline. The engine refuses that, so the menus grey the entries (PE-DESIGN's behaviour
/// on such a shape is not captured).
pub fn opening_leaves_nothing(o: &DesignObject, contour: usize) -> bool {
    o.contours.get(contour).is_some_and(|c| c.closed)
        && o.region.is_some()
        && o.line.is_none()
        && !o
            .contours
            .iter()
            .enumerate()
            .any(|(k, c)| k != contour && c.closed)
}

/// Whether `c` can lose the vertices `selected` (as `object.deletePoint` checks): an outline
/// keeps 2 points, open or closed, as PE-DESIGN's does.
pub fn can_delete_points(c: &Contour, selected: &[usize]) -> bool {
    let mut order: Vec<usize> = selected
        .iter()
        .copied()
        .filter(|&i| i < c.points.len())
        .collect();
    order.sort_unstable();
    order.dedup();
    if order.is_empty() {
        return false;
    }
    let mut left = c.clone();
    for &i in order.iter().rev() {
        if left.points.len() <= 2 {
            return false;
        }
        left.delete_point(i);
    }
    true
}
