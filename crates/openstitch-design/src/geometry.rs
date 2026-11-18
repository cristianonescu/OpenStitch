use crate::fmath::Det;
use serde::{Deserialize, Serialize};
use std::ops::{Add, Mul, RangeInclusive, Sub};

/// 2-D point in embroidery units (0.1 mm), +y down.
#[derive(Clone, Copy, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Vec2 {
    pub x: f64,
    pub y: f64,
}

pub const fn v2(x: f64, y: f64) -> Vec2 {
    Vec2 { x, y }
}

impl Add for Vec2 {
    type Output = Vec2;
    fn add(self, o: Vec2) -> Vec2 {
        v2(self.x + o.x, self.y + o.y)
    }
}

impl Sub for Vec2 {
    type Output = Vec2;
    fn sub(self, o: Vec2) -> Vec2 {
        v2(self.x - o.x, self.y - o.y)
    }
}

impl Mul<f64> for Vec2 {
    type Output = Vec2;
    fn mul(self, k: f64) -> Vec2 {
        v2(self.x * k, self.y * k)
    }
}

impl Vec2 {
    pub fn length(self) -> f64 {
        self.x.dhypot(self.y)
    }

    pub fn distance(self, o: Vec2) -> f64 {
        (self - o).length()
    }

    /// Rotation by `radians` about the origin.
    pub fn rotated(self, radians: f64) -> Vec2 {
        let (s, c) = radians.dsin_cos();
        v2(self.x * c - self.y * s, self.x * s + self.y * c)
    }

    pub fn lerp(self, o: Vec2, t: f64) -> Vec2 {
        self + (o - self) * t
    }
}

/// Affine map `x' = a·x + c·y + tx`, `y' = b·x + d·y + ty`.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Transform {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Transform {
    pub const IDENTITY: Transform = Transform {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    pub fn translation(dx: f64, dy: f64) -> Self {
        Transform {
            tx: dx,
            ty: dy,
            ..Self::IDENTITY
        }
    }

    pub fn scaling(sx: f64, sy: f64, about: Vec2) -> Self {
        Transform {
            a: sx,
            b: 0.0,
            c: 0.0,
            d: sy,
            tx: about.x - sx * about.x,
            ty: about.y - sy * about.y,
        }
    }

    /// Clockwise on screen (y down) for positive degrees.
    pub fn rotation(degrees: f64, about: Vec2) -> Self {
        let (sn, cs) = degrees.to_radians().dsin_cos();
        Transform {
            a: cs,
            b: sn,
            c: -sn,
            d: cs,
            tx: about.x - cs * about.x + sn * about.y,
            ty: about.y - sn * about.x - cs * about.y,
        }
    }

    /// `self` followed by `next`.
    pub fn then(&self, next: &Transform) -> Transform {
        Transform {
            a: next.a * self.a + next.c * self.b,
            b: next.b * self.a + next.d * self.b,
            c: next.a * self.c + next.c * self.d,
            d: next.b * self.c + next.d * self.d,
            tx: next.a * self.tx + next.c * self.ty + next.tx,
            ty: next.b * self.tx + next.d * self.ty + next.ty,
        }
    }

    pub fn is_finite(&self) -> bool {
        [self.a, self.b, self.c, self.d, self.tx, self.ty]
            .iter()
            .all(|v| v.is_finite())
    }

    pub fn apply(&self, p: Vec2) -> Vec2 {
        v2(
            self.a * p.x + self.c * p.y + self.tx,
            self.b * p.x + self.d * p.y + self.ty,
        )
    }
}

/// Cubic Bézier control points of one contour segment (the segment's end points are the
/// contour vertices on either side).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Bezier {
    pub c1: Vec2,
    pub c2: Vec2,
}

/// A handle nearer its vertex than this (0.1 mm units) is no handle: the point is straight.
const HANDLE_EPS: f64 = 1e-6;

/// Maximum distance (0.1 mm units) between a flattened curve and the true curve: 0.05 mm.
pub const FLATNESS: f64 = 0.5;

/// Polyline or polygon whose segments may be cubic Béziers. Closed contours describe regions
/// (even-odd across a shape's contours, so inner contours are holes).
///
/// Segment `i` runs from `points[i]` to `points[i + 1]` (wrapping to `points[0]` for the closing
/// segment of a closed contour). `curves[i]`, when present and `Some`, makes that segment a
/// cubic Bézier; an empty `curves` means every segment is straight.
#[derive(Clone, Debug, PartialEq, Default, Serialize, Deserialize)]
pub struct Contour {
    pub points: Vec<Vec2>,
    pub closed: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub curves: Vec<Option<Bezier>>,
    /// How each curve point's two handles are tied when one is dragged (see [`PointKind`]),
    /// one per vertex; empty means none is stored, as before kinds existed. Only
    /// editing reads it: the outline, and so the sewing, comes from `points` and `curves`.
    /// A list whose length is not `points.len()` is ignored.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub kinds: Vec<PointKind>,
}

/// What kind of curve point a vertex is: how its two handles are tied when one is dragged
/// (PE-DESIGN's To Smooth, To Symmetry and To Cusp; its PES 0110 files store the same three).
/// A point is *straight* when it has no handle of its own (see [`Contour::is_curve_point`]);
/// that is not stored here.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PointKind {
    /// No kind stored, as in every design made before kinds existed: the handles move
    /// independently.
    #[default]
    Unset,
    /// The handles move independently.
    Cusp,
    /// The handles stay on one line through the point; each keeps its own length.
    Smooth,
    /// The handles stay on one line through the point at the same length.
    Symmetric,
}

impl Contour {
    pub fn new(points: Vec<Vec2>, closed: bool) -> Self {
        Contour {
            points,
            closed,
            curves: Vec::new(),
            kinds: Vec::new(),
        }
    }

    pub fn rectangle(x: f64, y: f64, width: f64, height: f64) -> Self {
        Contour::new(
            vec![
                v2(x, y),
                v2(x + width, y),
                v2(x + width, y + height),
                v2(x, y + height),
            ],
            true,
        )
    }

    pub fn ellipse(center: Vec2, rx: f64, ry: f64, segments: usize) -> Self {
        let n = segments.max(8);
        let points = (0..n)
            .map(|i| {
                let t = i as f64 / n as f64 * std::f64::consts::TAU;
                v2(center.x + rx * t.dcos(), center.y + ry * t.dsin())
            })
            .collect();
        Contour::new(points, true)
    }

    /// Number of segments: one per vertex pair, plus the closing one when closed.
    pub fn segment_count(&self) -> usize {
        match (self.points.len(), self.closed) {
            (0 | 1, _) => 0,
            (n, true) => n,
            (n, false) => n - 1,
        }
    }

    /// End points of segment `i`.
    pub fn segment_ends(&self, i: usize) -> (Vec2, Vec2) {
        let n = self.points.len();
        (self.points[i], self.points[(i + 1) % n])
    }

    pub fn curve(&self, i: usize) -> Option<Bezier> {
        self.curves.get(i).copied().flatten()
    }

    pub fn has_curves(&self) -> bool {
        self.curves.iter().any(Option::is_some)
    }

    /// Sets (or with `None` clears) the curve of segment `i`, keeping `curves` empty when no
    /// segment is curved.
    pub fn set_curve(&mut self, i: usize, curve: Option<Bezier>) {
        if self.curves.len() < self.segment_count() {
            self.curves.resize(self.segment_count(), None);
        }
        self.curves.truncate(self.segment_count());
        if let Some(slot) = self.curves.get_mut(i) {
            *slot = curve;
        }
        if !self.has_curves() {
            self.curves.clear();
        }
    }

    /// The kind of vertex `i` ([`PointKind::Unset`] when none is stored).
    pub fn kind(&self, i: usize) -> PointKind {
        if self.kinds.len() == self.points.len() {
            self.kinds.get(i).copied().unwrap_or_default()
        } else {
            PointKind::default()
        }
    }

    /// Stores the kind of vertex `i`, keeping `kinds` empty when no point has one. The
    /// handles are not moved: see [`Contour::apply_kind`].
    pub fn set_kind(&mut self, i: usize, kind: PointKind) {
        let mut kinds = self.kinds_or_default();
        if let Some(slot) = kinds.get_mut(i) {
            *slot = kind;
        }
        self.store_kinds(kinds);
    }

    /// Every vertex's kind, one per point.
    pub fn kinds_or_default(&self) -> Vec<PointKind> {
        (0..self.points.len()).map(|i| self.kind(i)).collect()
    }

    /// Stores one kind per vertex (for an operation that rebuilt the points and knows where
    /// each came from); a list of another length, or without any kind, stores nothing.
    pub fn store_kinds(&mut self, kinds: Vec<PointKind>) {
        self.kinds =
            if kinds.len() == self.points.len() && kinds.iter().any(|k| *k != PointKind::Unset) {
                kinds
            } else {
                Vec::new()
            };
    }

    /// The segment ending at vertex `i`, if any.
    pub fn incoming(&self, i: usize) -> Option<usize> {
        let segs = self.segment_count();
        if i > 0 {
            (i - 1 < segs).then_some(i - 1)
        } else if self.closed && segs > 0 {
            Some(segs - 1)
        } else {
            None
        }
    }

    /// The segment leaving vertex `i`, if any.
    pub fn outgoing(&self, i: usize) -> Option<usize> {
        (i < self.segment_count()).then_some(i)
    }

    /// Whether the control point of curved side `s` at its start (`start`) or end vertex is
    /// only a copy of the other end's handle. PE-DESIGN keeps handles per point, and draws a
    /// side from a curve point to a straight point as a cubic with BOTH control points at the
    /// curve point's handle (#1258: the saved bounds of every capture fit that curve). Here
    /// that is a side whose two control points coincide, between a vertex with a stored kind
    /// and one without: the one without borrows.
    pub fn borrows_handle(&self, s: usize, start: bool) -> bool {
        let Some(z) = self.curve(s) else {
            return false;
        };
        let n = self.points.len();
        let (near, far) = if start {
            (s, (s + 1) % n)
        } else {
            ((s + 1) % n, s)
        };
        z.c1.distance(z.c2) <= HANDLE_EPS
            && self.kind(near) == PointKind::Unset
            && self.kind(far) != PointKind::Unset
    }

    /// Whether the control point of curved side `s` at that end is the vertex's own handle:
    /// not borrowed, and not lying on the vertex.
    pub fn owns_handle(&self, s: usize, start: bool) -> bool {
        let Some(z) = self.curve(s) else {
            return false;
        };
        let n = self.points.len();
        let (vertex, mine) = if start {
            (self.points[s], z.c1)
        } else {
            (self.points[(s + 1) % n], z.c2)
        };
        !self.borrows_handle(s, start) && mine.distance(vertex) > HANDLE_EPS
    }

    /// Whether vertex `i` is a curve point: it has a stored kind, or a handle of its own.
    /// Otherwise it is a straight point, even when a curve arrives from a curve point next
    /// to it: PE-DESIGN keeps the kind per point, and a side is a curve when either of its
    /// ends is a curve point.
    pub fn is_curve_point(&self, i: usize) -> bool {
        i < self.points.len()
            && (self.kind(i) != PointKind::Unset
                || self.incoming(i).is_some_and(|s| self.owns_handle(s, false))
                || self.outgoing(i).is_some_and(|s| self.owns_handle(s, true)))
    }

    /// The control points at vertex `i`: the arriving curve's `c2` and the leaving curve's
    /// `c1` (a straight point's are borrowed, see [`Contour::borrows_handle`]).
    pub fn point_handles(&self, i: usize) -> (Option<Vec2>, Option<Vec2>) {
        (
            self.incoming(i).and_then(|s| self.curve(s)).map(|b| b.c2),
            self.outgoing(i).and_then(|s| self.curve(s)).map(|b| b.c1),
        )
    }

    /// Sets one control point of curved side `s`; a copy the other end borrows moves with it.
    fn set_handle(&mut self, s: usize, start: bool, to: Vec2) {
        let Some(mut z) = self.curve(s) else {
            return;
        };
        if self.borrows_handle(s, !start) {
            z.c1 = to;
            z.c2 = to;
        } else if start {
            z.c1 = to;
        } else {
            z.c2 = to;
        }
        self.set_curve(s, Some(z));
    }

    fn set_point_handles(&mut self, i: usize, arriving: Option<Vec2>, leaving: Option<Vec2>) {
        if let (Some(s), Some(h)) = (self.incoming(i), arriving) {
            self.set_handle(s, false, h);
        }
        if let (Some(s), Some(h)) = (self.outgoing(i), leaving) {
            self.set_handle(s, true, h);
        }
    }

    /// Moves vertex `i` to `to`. Its own handles move with it (and the copies its straight
    /// neighbours borrow); a control point it only borrows stays with the handle it copies.
    pub fn move_point(&mut self, i: usize, to: Vec2) {
        if i >= self.points.len() {
            return;
        }
        let delta = to - self.points[i];
        for (s, start) in [(self.incoming(i), false), (self.outgoing(i), true)] {
            let Some(s) = s else { continue };
            let Some(z) = self.curve(s) else { continue };
            if self.borrows_handle(s, start) {
                continue;
            }
            let mine = if start { z.c1 } else { z.c2 };
            self.set_handle(s, start, mine + delta);
        }
        self.points[i] = to;
    }

    /// To Straight: vertex `i` loses its handles and its kind. A side whose other end has no
    /// handle of its own becomes a straight line; a side to a curve point stays a curve drawn
    /// from that point's handle alone, as PE-DESIGN draws it (its circle with one point made
    /// straight still bends on both sides of the corner).
    pub fn make_straight(&mut self, i: usize) {
        if i >= self.points.len() {
            return;
        }
        let n = self.points.len();
        for (s, start) in [(self.incoming(i), false), (self.outgoing(i), true)] {
            let Some(s) = s else { continue };
            let Some(z) = self.curve(s) else { continue };
            let far = if start { (s + 1) % n } else { s };
            if far != i && self.owns_handle(s, !start) {
                // The other end's handle draws the side; it is a curve point for good.
                let theirs = if start { z.c2 } else { z.c1 };
                if self.kind(far) == PointKind::Unset {
                    self.set_kind(far, PointKind::Cusp);
                }
                self.set_curve(
                    s,
                    Some(Bezier {
                        c1: theirs,
                        c2: theirs,
                    }),
                );
            } else {
                self.set_curve(s, None);
            }
        }
        self.set_kind(i, PointKind::Unset);
    }

    /// To Curve, as PE-DESIGN 11.50 does it (#1258, captures of 2026-11-18): straight point
    /// `i` becomes a smooth curve point whose two handles lie a quarter of the way from the
    /// previous vertex to the next one on either side of it (`p ∓ (next − previous) / 4`; at
    /// an end of an open line the missing neighbour is the point itself). Both sides at it
    /// become curves, so the outline bends at once; a side's other end keeps its own handle,
    /// or borrows this one when it is a straight point. Returns whether anything changed (a
    /// curve point is left alone).
    pub fn make_curve(&mut self, i: usize) -> bool {
        if i >= self.points.len() || self.is_curve_point(i) {
            return false;
        }
        let p = self.points[i];
        let n = self.points.len();
        let previous = self.incoming(i).map_or(p, |_| self.points[(i + n - 1) % n]);
        let next = self.outgoing(i).map_or(p, |_| self.points[(i + 1) % n]);
        let h = (next - previous) * 0.25;
        for (s, start) in [(self.incoming(i), false), (self.outgoing(i), true)] {
            let Some(s) = s else { continue };
            let mine = if start { p + h } else { p - h };
            let far = if start { (s + 1) % n } else { s };
            let theirs = match self.curve(s) {
                // The other end's own handle stays.
                Some(z) if self.owns_handle(s, !start) => {
                    if start {
                        z.c2
                    } else {
                        z.c1
                    }
                }
                // A straight point borrows this one; a point with a kind but no handle on
                // this side keeps none.
                _ if self.kind(far) == PointKind::Unset || far == i => mine,
                _ => self.points[far],
            };
            self.set_curve(
                s,
                Some(if start {
                    Bezier {
                        c1: mine,
                        c2: theirs,
                    }
                } else {
                    Bezier {
                        c1: theirs,
                        c2: mine,
                    }
                }),
            );
        }
        self.set_kind(i, PointKind::Smooth);
        true
    }

    /// To Smooth, To Symmetry, To Cusp: stores `kind` on curve point `i` and lines its two
    /// handles up for it. Symmetric turns both onto the line through the point that is
    /// parallel to the line between them and gives both their mean length (measured on
    /// PE-DESIGN 11.50: 11.18 and 18.31 mm became 14.75 mm each, direction kept); Smooth does
    /// the same with each keeping its own length; Cusp moves nothing (measured). With fewer
    /// than two handles only the kind is stored.
    pub fn apply_kind(&mut self, i: usize, kind: PointKind) {
        self.set_kind(i, kind);
        if !matches!(kind, PointKind::Smooth | PointKind::Symmetric) {
            return;
        }
        let (Some(arriving), Some(leaving)) = self.point_handles(i) else {
            return;
        };
        let p = self.points[i];
        let (la, ll) = (arriving.distance(p), leaving.distance(p));
        let along = leaving - arriving;
        let len = along.length();
        if len <= HANDLE_EPS {
            return;
        }
        let dir = along * (1.0 / len);
        let (la, ll) = match kind {
            PointKind::Symmetric => ((la + ll) / 2.0, (la + ll) / 2.0),
            _ => (la, ll),
        };
        self.set_point_handles(i, Some(p - dir * la), Some(p + dir * ll));
    }

    /// Moves one control point of curved segment `segment` (`start`: its `c1`, else `c2`) to
    /// `to`, and the other handle at the same vertex as that vertex's kind asks (measured on
    /// PE-DESIGN 11.50): a smooth point turns it to stay opposite at its own length, a
    /// symmetric point mirrors it, a cusp leaves it. A copy a straight neighbour borrows
    /// moves with its handle.
    pub fn move_handle(&mut self, segment: usize, start: bool, to: Vec2) {
        if self.curve(segment).is_none() {
            return;
        }
        self.set_handle(segment, start, to);
        let n = self.points.len();
        let vertex = if start { segment } else { (segment + 1) % n };
        let p = self.points[vertex];
        let (arriving, leaving) = self.point_handles(vertex);
        let other = if start { arriving } else { leaving };
        let Some(other) = other else {
            return;
        };
        let reach = to.distance(p);
        if reach <= HANDLE_EPS {
            return;
        }
        let back = (p - to) * (1.0 / reach);
        let moved = match self.kind(vertex) {
            PointKind::Unset | PointKind::Cusp => return,
            PointKind::Smooth => p + back * other.distance(p),
            PointKind::Symmetric => p + back * reach,
        };
        if start {
            self.set_point_handles(vertex, Some(moved), None);
        } else {
            self.set_point_handles(vertex, None, Some(moved));
        }
    }

    /// Split at Point. A closed contour opens at vertex `i`: one open contour that starts and
    /// ends there. An open contour splits at an inner vertex into two that share it. `None`
    /// at the ends of an open contour, where there is nothing to split.
    pub fn split_at(&self, i: usize) -> Option<Vec<Contour>> {
        let n = self.points.len();
        if i >= n || n < 2 {
            return None;
        }
        let piece = |from: usize, segments: usize| {
            let mut c = Contour::new(
                (0..=segments)
                    .map(|k| self.points[(from + k) % n])
                    .collect(),
                false,
            );
            for k in 0..segments {
                c.set_curve(k, self.curve((from + k) % self.segment_count()));
            }
            let kinds = (0..=segments).map(|k| self.kind((from + k) % n)).collect();
            c.store_kinds(kinds);
            c
        };
        if self.closed {
            return Some(vec![piece(i, n)]);
        }
        (i > 0 && i + 1 < n).then(|| vec![piece(0, i), piece(i, n - 1 - i)])
    }

    /// Segment `i` as a polyline from its start to its end vertex (inclusive); curved segments
    /// are flattened to within [`FLATNESS`].
    pub fn segment_polyline(&self, i: usize) -> Vec<Vec2> {
        let (a, b) = self.segment_ends(i);
        match self.curve(i) {
            None => vec![a, b],
            Some(c) => {
                let mut out = vec![a];
                flatten_cubic(a, c.c1, c.c2, b, 0, &mut out);
                out
            }
        }
    }

    /// Points in drawing order with curves flattened, repeating the first at the end when closed.
    pub fn path(&self) -> Vec<Vec2> {
        if !self.has_curves() {
            let mut p = self.points.clone();
            if self.closed && !p.is_empty() {
                p.push(p[0]);
            }
            return p;
        }
        let mut out: Vec<Vec2> = self.points.first().copied().into_iter().collect();
        for i in 0..self.segment_count() {
            out.extend(self.segment_polyline(i).into_iter().skip(1));
        }
        out
    }

    /// Flattened outline points without the closing repeat (for bounds).
    pub fn outline_points(&self) -> Vec<Vec2> {
        let mut p = self.path();
        if self.closed && p.len() > 1 {
            p.pop();
        }
        p
    }

    pub fn edges(&self) -> Vec<(Vec2, Vec2)> {
        self.path().windows(2).map(|w| (w[0], w[1])).collect()
    }

    pub fn transformed(&self, t: &Transform) -> Contour {
        Contour {
            points: self.points.iter().map(|&p| t.apply(p)).collect(),
            closed: self.closed,
            curves: self
                .curves
                .iter()
                .map(|c| {
                    c.map(|c| Bezier {
                        c1: t.apply(c.c1),
                        c2: t.apply(c.c2),
                    })
                })
                .collect(),
            kinds: self.kinds.clone(),
        }
    }

    /// Splits segment `i` at parameter `t` (0 < t < 1), inserting a vertex there. A curved
    /// segment is split exactly (de Casteljau) into two curves. Returns the new vertex index.
    pub fn insert_point(&mut self, i: usize, t: f64) -> usize {
        let (a, b) = self.segment_ends(i);
        let n = self.segment_count();
        let mut curves: Vec<Option<Bezier>> = (0..n).map(|k| self.curve(k)).collect();
        let (p, left, right) = match self.curve(i) {
            None => (a.lerp(b, t), None, None),
            Some(c) => {
                let (ab, bc, cd) = (a.lerp(c.c1, t), c.c1.lerp(c.c2, t), c.c2.lerp(b, t));
                let (abc, bcd) = (ab.lerp(bc, t), bc.lerp(cd, t));
                let p = abc.lerp(bcd, t);
                (
                    p,
                    Some(Bezier { c1: ab, c2: abc }),
                    Some(Bezier { c1: bcd, c2: cd }),
                )
            }
        };
        let mut kinds = self.kinds_or_default();
        // A point put on a curve leaves the outline as it was: its handles are in line.
        kinds.insert(
            i + 1,
            if left.is_some() {
                PointKind::Smooth
            } else {
                PointKind::Unset
            },
        );
        self.points.insert(i + 1, p);
        self.store_kinds(kinds);
        curves[i] = left;
        curves.insert(i + 1, right);
        self.curves = curves;
        if !self.has_curves() {
            self.curves.clear();
        }
        i + 1
    }

    /// Removes vertex `index`, joining its two neighbouring segments. The joined segment stays a
    /// curve (outer handles kept) if either neighbour was curved.
    pub fn delete_point(&mut self, index: usize) {
        let segs = self.segment_count();
        let mut curves: Vec<Option<Bezier>> = (0..segs).map(|k| self.curve(k)).collect();
        let before = if index > 0 {
            Some(index - 1)
        } else if self.closed {
            Some(segs - 1)
        } else {
            None
        };
        let after = if index < segs { Some(index) } else { None };
        match (before, after) {
            (Some(b), Some(a)) => {
                let (pa, _) = self.segment_ends(b);
                let (_, pb) = self.segment_ends(a);
                let joined = match (curves[b], curves[a]) {
                    (None, None) => None,
                    (cb, ca) => Some(Bezier {
                        c1: cb.map_or(pa, |c| c.c1),
                        c2: ca.map_or(pb, |c| c.c2),
                    }),
                };
                curves[b] = joined;
                curves.remove(a);
            }
            (Some(b), None) => {
                curves.remove(b);
            }
            (None, Some(a)) => {
                curves.remove(a);
            }
            (None, None) => {}
        }
        let mut kinds = self.kinds_or_default();
        kinds.remove(index);
        self.points.remove(index);
        self.store_kinds(kinds);
        self.curves = curves;
        self.curves.truncate(self.segment_count());
        if !self.has_curves() {
            self.curves.clear();
        }
    }
}

/// Adaptive de Casteljau flattening; pushes points after `a` up to and including `d`.
fn flatten_cubic(a: Vec2, b: Vec2, c: Vec2, d: Vec2, depth: u32, out: &mut Vec<Vec2>) {
    let flat = distance_to_line(b, a, d).max(distance_to_line(c, a, d));
    if depth >= 16 || flat <= FLATNESS {
        out.push(d);
        return;
    }
    let (ab, bc, cd) = (a.lerp(b, 0.5), b.lerp(c, 0.5), c.lerp(d, 0.5));
    let (abc, bcd) = (ab.lerp(bc, 0.5), bc.lerp(cd, 0.5));
    let mid = abc.lerp(bcd, 0.5);
    flatten_cubic(a, ab, abc, mid, depth + 1, out);
    flatten_cubic(mid, bcd, cd, d, depth + 1, out);
}

fn distance_to_line(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len = ab.length();
    if len == 0.0 {
        return p.distance(a);
    }
    ((p.x - a.x) * ab.y - (p.y - a.y) * ab.x).abs() / len
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub min_x: f64,
    pub min_y: f64,
    pub max_x: f64,
    pub max_y: f64,
}

impl Rect {
    pub fn enclosing(points: impl IntoIterator<Item = Vec2>) -> Option<Rect> {
        let mut it = points.into_iter();
        let f = it.next()?;
        Some(it.fold(
            Rect {
                min_x: f.x,
                min_y: f.y,
                max_x: f.x,
                max_y: f.y,
            },
            |r, p| Rect {
                min_x: r.min_x.min(p.x),
                min_y: r.min_y.min(p.y),
                max_x: r.max_x.max(p.x),
                max_y: r.max_y.max(p.y),
            },
        ))
    }
    pub fn width(&self) -> f64 {
        self.max_x - self.min_x
    }
    pub fn height(&self) -> f64 {
        self.max_y - self.min_y
    }
    pub fn center(&self) -> Vec2 {
        v2(
            (self.min_x + self.max_x) / 2.0,
            (self.min_y + self.max_y) / 2.0,
        )
    }
}

pub type Span = RangeInclusive<f64>;

/// Horizontal scanline through a region after rotating it by −angle.
#[derive(Clone, Debug, PartialEq)]
pub struct ScanRow {
    pub index: usize,
    /// Position in rotated space.
    pub y: f64,
    /// Inside intervals, sorted, even-odd rule.
    pub spans: Vec<Span>,
}

/// Intersects closed `contours` with lines `spacing` apart at `angle_degrees` (0 = horizontal).
/// Rows are in rotated space (map back with [`from_rotated`]); the first sits half a spacing
/// inside the region. `excluding` contours are cut out of every row (hole sewing).
pub fn scan_rows(
    contours: &[Contour],
    angle_degrees: f64,
    spacing: f64,
    excluding: &[Contour],
) -> Vec<ScanRow> {
    if spacing.is_nan() || spacing <= 0.0 {
        return Vec::new();
    }
    let rad = -angle_degrees.to_radians();
    let rotate = |cs: &[Contour]| -> Vec<Contour> {
        cs.iter()
            .filter(|c| c.closed)
            .map(|c| {
                Contour::new(
                    c.outline_points().iter().map(|p| p.rotated(rad)).collect(),
                    true,
                )
            })
            .collect()
    };
    let rotated = rotate(contours);
    let Some(bbox) = Rect::enclosing(rotated.iter().flat_map(|c| c.points.iter().copied())) else {
        return Vec::new();
    };
    if bbox.height().is_nan() || bbox.height() <= 0.0 {
        return Vec::new();
    }
    let edges: Vec<_> = rotated.iter().flat_map(Contour::edges).collect();
    let hole_edges: Vec<_> = rotate(excluding).iter().flat_map(Contour::edges).collect();
    let mut rows = Vec::new();
    let mut y = bbox.min_y + spacing / 2.0;
    let mut index = 0;
    while y < bbox.max_y {
        let mut spans = spans_at(&edges, y);
        if !hole_edges.is_empty() {
            spans = subtract(&spans_at(&hole_edges, y), &spans);
        }
        rows.push(ScanRow { index, y, spans });
        y += spacing;
        index += 1;
    }
    rows
}

pub fn from_rotated(p: Vec2, angle_degrees: f64) -> Vec2 {
    p.rotated(angle_degrees.to_radians())
}

fn spans_at(edges: &[(Vec2, Vec2)], y: f64) -> Vec<Span> {
    let mut xs: Vec<f64> = edges
        .iter()
        .filter(|(p, q)| (p.y <= y && q.y > y) || (q.y <= y && p.y > y))
        .map(|(p, q)| p.x + (y - p.y) * (q.x - p.x) / (q.y - p.y))
        .collect();
    xs.sort_by(f64::total_cmp);
    xs.as_chunks::<2>()
        .0
        .iter()
        .filter(|w| w[1] > w[0])
        .map(|w| w[0]..=w[1])
        .collect()
}

/// `spans` minus `holes` (both sorted, non-overlapping); zero-width leftovers dropped.
pub fn subtract(holes: &[Span], spans: &[Span]) -> Vec<Span> {
    let mut out = Vec::new();
    for s in spans {
        let mut start = *s.start();
        for h in holes {
            if *h.end() <= start || *h.start() >= *s.end() {
                continue;
            }
            if *h.start() > start {
                out.push(start..=*h.start());
            }
            start = start.max(*h.end());
        }
        if start < *s.end() {
            out.push(start..=*s.end());
        }
    }
    out
}

/// Even-odd point-in-region test.
pub fn contains(p: Vec2, contours: &[Contour]) -> bool {
    let mut inside = false;
    for c in contours.iter().filter(|c| c.closed) {
        for (a, b) in c.edges() {
            if (a.y > p.y) != (b.y > p.y) && p.x < a.x + (p.y - a.y) * (b.x - a.x) / (b.y - a.y) {
                inside = !inside;
            }
        }
    }
    inside
}
