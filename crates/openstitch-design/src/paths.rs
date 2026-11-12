//! Polyline helpers shared by the line and region generators: arc-length walking, clipping a
//! polyline to a region, ordering pieces, and travel paths that stay inside a region.
//! Lengths are 0.1 mm.

use crate::geometry::{Contour, Rect, Vec2, contains, v2};

pub fn dot(a: Vec2, b: Vec2) -> f64 {
    a.x * b.x + a.y * b.y
}

pub fn cross(a: Vec2, b: Vec2) -> f64 {
    a.x * b.y - a.y * b.x
}

/// Unit vector, or zero for a zero vector.
pub fn unit(a: Vec2) -> Vec2 {
    let l = a.length();
    if l > 0.0 { a * (1.0 / l) } else { v2(0.0, 0.0) }
}

/// Left normal of a direction (+90° in a y-down frame points to the right of travel on screen;
/// generators only need a consistent side).
pub fn normal(d: Vec2) -> Vec2 {
    v2(-d.y, d.x)
}

/// Total length of a polyline.
pub fn length(poly: &[Vec2]) -> f64 {
    poly.windows(2).map(|w| w[0].distance(w[1])).sum()
}

/// Arc-length parametrisation of a polyline.
pub struct Walker<'a> {
    poly: &'a [Vec2],
    cum: Vec<f64>,
}

impl<'a> Walker<'a> {
    pub fn new(poly: &'a [Vec2]) -> Self {
        let mut cum = vec![0.0];
        for w in poly.windows(2) {
            let last = *cum.last().expect("non-empty");
            cum.push(last + w[0].distance(w[1]));
        }
        Walker { poly, cum }
    }

    pub fn length(&self) -> f64 {
        *self.cum.last().expect("non-empty")
    }

    fn segment(&self, s: f64) -> usize {
        let n = self.poly.len();
        if n < 2 {
            return 0;
        }
        match self.cum.binary_search_by(|c| c.total_cmp(&s)) {
            Ok(i) => i.min(n - 2),
            Err(i) => i.saturating_sub(1).min(n - 2),
        }
    }

    /// Point at arc length `s` (clamped to the ends).
    pub fn at(&self, s: f64) -> Vec2 {
        if self.poly.len() < 2 {
            return self.poly.first().copied().unwrap_or_default();
        }
        let s = s.clamp(0.0, self.length());
        let i = self.segment(s);
        let seg = self.cum[i + 1] - self.cum[i];
        let t = if seg > 0.0 {
            (s - self.cum[i]) / seg
        } else {
            0.0
        };
        self.poly[i].lerp(self.poly[i + 1], t)
    }

    /// Unit tangent at arc length `s` (skipping zero-length segments).
    pub fn tangent(&self, s: f64) -> Vec2 {
        if self.poly.len() < 2 {
            return v2(1.0, 0.0);
        }
        let i = self.segment(s.clamp(0.0, self.length()));
        for j in (i..self.poly.len() - 1).chain((0..i).rev()) {
            let d = self.poly[j + 1] - self.poly[j];
            if d.length() > 1e-9 {
                return unit(d);
            }
        }
        v2(1.0, 0.0)
    }

    /// The polyline between arc lengths `a` ≤ `b`, vertices included.
    pub fn slice(&self, a: f64, b: f64) -> Vec<Vec2> {
        let mut out = vec![self.at(a)];
        for (i, &c) in self.cum.iter().enumerate() {
            if c > a && c < b {
                out.push(self.poly[i]);
            }
        }
        out.push(self.at(b));
        out
    }
}

/// Splits `poly` into pieces no longer than `max_len` per stitch, keeping vertices.
pub fn stitches(poly: &[Vec2], max_len: f64) -> Vec<Vec2> {
    crate::generate::subdivide(poly, max_len)
}

/// Parts of `poly` inside the region `contours` minus `holes` (even-odd), each a polyline.
pub fn clip(poly: &[Vec2], contours: &[Contour], holes: &[Contour]) -> Vec<Vec<Vec2>> {
    let edges: Vec<(Vec2, Vec2)> = contours
        .iter()
        .chain(holes)
        .filter(|c| c.closed)
        .flat_map(Contour::edges)
        .collect();
    let inside = |p: Vec2| contains(p, contours) && !contains(p, holes);
    let mut pieces = Vec::new();
    let mut current: Vec<Vec2> = Vec::new();
    for w in poly.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let mut ts: Vec<f64> = vec![0.0, 1.0];
        for &(p, q) in &edges {
            let e = q - p;
            let den = cross(d, e);
            if den.abs() < 1e-12 {
                continue;
            }
            let t = cross(p - a, e) / den;
            let u = cross(p - a, d) / den;
            if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                ts.push(t);
            }
        }
        ts.sort_by(f64::total_cmp);
        ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
        for t in ts.windows(2) {
            let (p, q) = (a.lerp(b, t[0]), a.lerp(b, t[1]));
            if inside(p.lerp(q, 0.5)) {
                if current.last().is_none_or(|&l| l.distance(p) > 1e-6) {
                    if current.len() >= 2 {
                        pieces.push(std::mem::take(&mut current));
                    }
                    current = vec![p];
                }
                current.push(q);
            } else if current.len() >= 2 {
                pieces.push(std::mem::take(&mut current));
            } else {
                current.clear();
            }
        }
    }
    if current.len() >= 2 {
        pieces.push(current);
    }
    pieces
}

/// Orders pieces greedily: from `start`, the nearest unsewn piece end next, reversing a piece
/// when its far end is nearer. Deterministic (ties keep the earlier piece).
pub fn order(mut pieces: Vec<Vec<Vec2>>, start: Option<Vec2>) -> Vec<Vec<Vec2>> {
    let mut out = Vec::with_capacity(pieces.len());
    let Some(first) = pieces.first().and_then(|p| p.first()).copied() else {
        return out;
    };
    let mut pos = start.unwrap_or(first);
    while !pieces.is_empty() {
        let (mut best, mut rev, mut best_d) = (0, false, f64::INFINITY);
        for (i, p) in pieces.iter().enumerate() {
            let (a, b) = (p[0], *p.last().expect("non-empty piece"));
            if pos.distance(a) < best_d {
                (best, rev, best_d) = (i, false, pos.distance(a));
            }
            if pos.distance(b) < best_d {
                (best, rev, best_d) = (i, true, pos.distance(b));
            }
        }
        let mut p = pieces.remove(best);
        if rev {
            p.reverse();
        }
        pos = *p.last().expect("non-empty piece");
        out.push(p);
    }
    out
}

/// Shortest path from `a` to `b` that stays inside the region (`contours` minus `holes`),
/// through the region's (simplified) vertices. `None` when no such path is found or the
/// region is too complex for the search (more than 600 vertices after simplification).
pub fn inside_path(a: Vec2, b: Vec2, contours: &[Contour], holes: &[Contour]) -> Option<Vec<Vec2>> {
    let mut graph = TravelGraph::new(contours, holes);
    graph.path(a, b)
}

/// Visibility graph over a region's outline vertices, reused for many travel queries.
pub struct TravelGraph {
    nodes: Vec<Vec2>,
    edges: Vec<(Vec2, Vec2)>,
    regions: Vec<Contour>,
    holes: Vec<Contour>,
    adjacency: Option<Vec<Vec<(usize, f64)>>>,
    bounds: Option<Rect>,
    /// Prefer walking along the outline when both ends lie on the same contour (PE-DESIGN's
    /// "minimize feed": travel around the object edges, where the stitches stay hidden).
    pub along_edges: bool,
    /// Running Stitch Path: travel on the outline, or along it just inside, whenever both ends
    /// lie on one contour (the shortest inside path otherwise).
    pub outline: Option<OutlineTravel>,
    /// How far (0.1 mm) an end may be from the outline and still count as on it.
    pub outline_tolerance: f64,
    /// Outline travel goes forward along the contour (in its vertex order) from the start,
    /// whichever way round is shorter (PE-DESIGN's Auto satin start travel).
    pub outline_forward: bool,
}

/// Travel that follows the outline (see [`TravelGraph::outline`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum OutlineTravel {
    /// On the outline itself.
    On,
    /// Along the outline, this far (0.1 mm) inside it.
    Inset(f64),
}

const MAX_NODES: usize = 600;

impl TravelGraph {
    pub fn new(contours: &[Contour], holes: &[Contour]) -> Self {
        // Curved outlines flattened once (#679): every visibility test, inside test and outline
        // walk reads the polygon through `Contour::path`, which flattened the Béziers again on
        // each call. The flattened contour's path is the same points, so routing is unchanged.
        let flat = |c: &Contour| {
            if c.has_curves() {
                Contour::new(c.outline_points(), true)
            } else {
                c.clone()
            }
        };
        let closed =
            |cs: &[Contour]| -> Vec<Contour> { cs.iter().filter(|c| c.closed).map(flat).collect() };
        let (regions, holes) = (closed(contours), closed(holes));
        let mut nodes = Vec::new();
        let mut edges = Vec::new();
        for c in regions.iter().chain(&holes) {
            let pts = simplify_closed(&c.outline_points(), 3.0);
            nodes.extend(pts.iter().copied());
            edges.extend(c.edges());
        }
        let bounds = Rect::enclosing(nodes.iter().copied());
        TravelGraph {
            nodes,
            edges,
            regions,
            holes,
            adjacency: None,
            bounds,
            along_edges: false,
            outline: None,
            outline_tolerance: 1.0,
            outline_forward: false,
        }
    }

    /// The outline walk for [`TravelGraph::outline`], checked to stay inside the region.
    fn outline_path(&self, a: Vec2, b: Vec2, mode: OutlineTravel) -> Option<Vec<Vec2>> {
        let on = self.edge_path_within(a, b, self.outline_tolerance)?;
        let OutlineTravel::Inset(d) = mode else {
            return Some(on);
        };
        // Each interior point moves to where the walk's two edges, offset `d` inwards, meet
        // (a miter); the walk stays on the outline where the inset would leave the region.
        let inward = |p: Vec2, q: Vec2| -> Option<Vec2> {
            let t = q - p;
            if t.length() < 1e-9 {
                return None;
            }
            let n = v2(-t.y, t.x) * (1.0 / t.length());
            let m = p.lerp(q, 0.5);
            [n, n * -1.0].into_iter().find(|&n| self.inside(m + n * d))
        };
        let mut inset = on.clone();
        for i in 1..on.len().saturating_sub(1) {
            let offset = match (inward(on[i - 1], on[i]), inward(on[i], on[i + 1])) {
                (Some(a), Some(b)) => {
                    let s = 1.0 + dot(a, b);
                    if s < 0.2 {
                        continue;
                    }
                    (a + b) * (d / s)
                }
                (Some(a), None) | (None, Some(a)) => a * d,
                (None, None) => continue,
            };
            inset[i] = on[i] + offset;
        }
        // The walk steps from its start straight onto the inset line first (PE-DESIGN's Auto
        // satin travel: (0, 0) → (0, 0.5) → along the edge 0.5 mm inside), then each inset
        // point is kept only where the walk stays inside; elsewhere (a sharp far corner whose
        // mitre lands past the end) the walk stays on the outline there.
        let mut out = vec![on[0]];
        if on.len() > 2
            && let Some(nrm) = inward(on[0], on[1])
            && self.visible(on[0], on[0] + nrm * d)
            && self.visible(on[0] + nrm * d, inset[1])
        {
            out.push(on[0] + nrm * d);
        }
        for i in 1..on.len() {
            let candidate = if i + 1 == on.len() { on[i] } else { inset[i] };
            let from = out[out.len() - 1];
            if self.visible(from, candidate) {
                out.push(candidate);
            } else if self.visible(from, on[i]) {
                out.push(on[i]);
            } else {
                // Back off the previous point to the outline and try again.
                let k = out.len() - 1;
                out[k] = on[i - 1];
                out.push(if self.visible(on[i - 1], candidate) {
                    candidate
                } else {
                    on[i]
                });
            }
        }
        Some(out)
    }

    /// Walks along the closed contour that both `a` and `b` lie on (within 0.1 mm), the
    /// shorter way round; `None` when they are not on one contour.
    pub fn edge_path(&self, a: Vec2, b: Vec2) -> Option<Vec<Vec2>> {
        self.edge_path_within(a, b, 1.0)
    }

    fn edge_path_within(&self, a: Vec2, b: Vec2, tolerance: f64) -> Option<Vec<Vec2>> {
        for c in self.regions.iter().chain(&self.holes) {
            let poly = c.path();
            let (Some((sa, da)), Some((sb, db))) = (project(&poly, a), project(&poly, b)) else {
                continue;
            };
            if da > tolerance || db > tolerance {
                continue;
            }
            let w = Walker::new(&poly);
            let total = w.length();
            let forward = (sb - sa).rem_euclid(total);
            let mut path = if forward <= total - forward || self.outline_forward {
                if sa + forward <= total {
                    w.slice(sa, sa + forward)
                } else {
                    let mut p = w.slice(sa, total);
                    p.extend(w.slice(0.0, sb).into_iter().skip(1));
                    p
                }
            } else if sb <= sa {
                let mut p = w.slice(sb, sa);
                p.reverse();
                p
            } else {
                let mut p = w.slice(0.0, sa);
                p.reverse();
                let mut q = w.slice(sb, total);
                q.reverse();
                p.extend(q.into_iter().skip(1));
                p
            };
            path.insert(0, a);
            path.push(b);
            path.dedup_by(|x, y| x.distance(*y) < 1e-9);
            return Some(path);
        }
        None
    }

    fn inside(&self, p: Vec2) -> bool {
        contains(p, &self.regions) && !contains(p, &self.holes)
    }

    /// True when the segment does not leave the region: it crosses no outline edge properly
    /// and samples along it are inside (or within 0.05 mm of an outline).
    fn visible(&self, a: Vec2, b: Vec2) -> bool {
        let d = b - a;
        if d.length() < 1e-9 {
            return true;
        }
        for &(p, q) in &self.edges {
            let e = q - p;
            let den = cross(d, e);
            if den.abs() < 1e-12 {
                continue;
            }
            let t = cross(p - a, e) / den;
            let u = cross(p - a, d) / den;
            if t > 1e-6 && t < 1.0 - 1e-6 && u > 1e-6 && u < 1.0 - 1e-6 {
                return false;
            }
        }
        let n = ((d.length() / 20.0).ceil() as usize).clamp(1, 64);
        (0..n).all(|k| {
            let p = a.lerp(b, (k as f64 + 0.5) / n as f64);
            self.inside(p) || self.near_edge(p, 0.5)
        })
    }

    fn near_edge(&self, p: Vec2, tolerance: f64) -> bool {
        self.edges.iter().any(|&(a, b)| {
            let d = b - a;
            let l2 = dot(d, d);
            let t = if l2 > 0.0 {
                (dot(p - a, d) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            p.distance(a + d * t) <= tolerance
        })
    }

    fn build(&mut self) -> bool {
        if self.adjacency.is_some() {
            return true;
        }
        if self.nodes.len() > MAX_NODES || self.bounds.is_none() {
            return false;
        }
        let n = self.nodes.len();
        let mut adj = vec![Vec::new(); n];
        for i in 0..n {
            for j in i + 1..n {
                let (a, b) = (self.nodes[i], self.nodes[j]);
                if self.visible(a, b) {
                    let d = a.distance(b);
                    adj[i].push((j, d));
                    adj[j].push((i, d));
                }
            }
        }
        self.adjacency = Some(adj);
        true
    }

    /// Shortest inside path from `a` to `b` (both ends included), or `None`.
    pub fn path(&mut self, a: Vec2, b: Vec2) -> Option<Vec<Vec2>> {
        if let Some(mode) = self.outline
            && let Some(walk) = self.outline_path(a, b, mode)
        {
            return Some(walk);
        }
        if self.along_edges
            && let Some(edge) = self.edge_path(a, b)
            && length(&edge) <= 3.0 * a.distance(b) + 20.0
        {
            return Some(edge);
        }
        if self.visible(a, b) {
            return Some(vec![a, b]);
        }
        if !self.build() {
            return None;
        }
        let adj = self.adjacency.as_ref().expect("built");
        let n = self.nodes.len();
        // Nodes n and n + 1 are a and b.
        let link = |p: Vec2| -> Vec<(usize, f64)> {
            (0..n)
                .filter(|&i| self.visible(p, self.nodes[i]))
                .map(|i| (i, p.distance(self.nodes[i])))
                .collect()
        };
        let from_a = link(a);
        let to_b: Vec<(usize, f64)> = link(b);
        let mut dist = vec![f64::INFINITY; n + 2];
        let mut prev = vec![usize::MAX; n + 2];
        let mut done = vec![false; n + 2];
        dist[n] = 0.0;
        loop {
            let mut u = usize::MAX;
            let mut best = f64::INFINITY;
            for (i, &d) in dist.iter().enumerate() {
                if !done[i] && d < best {
                    (u, best) = (i, d);
                }
            }
            if u == usize::MAX || u == n + 1 {
                break;
            }
            done[u] = true;
            let neighbours: Vec<(usize, f64)> = if u == n {
                from_a.clone()
            } else {
                let mut v = adj[u].clone();
                if let Some(&(_, d)) = to_b.iter().find(|(i, _)| *i == u) {
                    v.push((n + 1, d));
                }
                v
            };
            for (v, w) in neighbours {
                if dist[u] + w < dist[v] {
                    dist[v] = dist[u] + w;
                    prev[v] = u;
                }
            }
        }
        if !dist[n + 1].is_finite() {
            return None;
        }
        let mut out = vec![b];
        let mut cur = prev[n + 1];
        while cur != n {
            out.push(self.nodes[cur]);
            cur = prev[cur];
        }
        out.push(a);
        out.reverse();
        Some(out)
    }
}

/// Arc length along `poly` of the point nearest `p`, and its distance from `p`.
fn project(poly: &[Vec2], p: Vec2) -> Option<(f64, f64)> {
    let mut best: Option<(f64, f64)> = None;
    let mut walked = 0.0;
    for w in poly.windows(2) {
        let d = w[1] - w[0];
        let l2 = dot(d, d);
        let t = if l2 > 0.0 {
            (dot(p - w[0], d) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dist = p.distance(w[0] + d * t);
        if best.is_none_or(|(_, bd)| dist < bd) {
            best = Some((walked + t * l2.sqrt(), dist));
        }
        walked += l2.sqrt();
    }
    best
}

/// Douglas-Peucker on a closed outline (no closing repeat), tolerance in 0.1 mm.
fn simplify_closed(points: &[Vec2], tolerance: f64) -> Vec<Vec2> {
    if points.len() <= 8 {
        return points.to_vec();
    }
    let mut ring = points.to_vec();
    ring.push(points[0]);
    let mut keep = vec![false; ring.len()];
    keep[0] = true;
    let far = (1..ring.len() - 1)
        .max_by(|&i, &j| {
            ring[0]
                .distance(ring[i])
                .total_cmp(&ring[0].distance(ring[j]))
        })
        .unwrap_or(1);
    keep[far] = true;
    *keep.last_mut().expect("non-empty") = true;
    dp(&ring, 0, far, tolerance, &mut keep);
    dp(&ring, far, ring.len() - 1, tolerance, &mut keep);
    ring.pop();
    ring.iter()
        .zip(&keep)
        .filter(|(_, k)| **k)
        .map(|(p, _)| *p)
        .collect()
}

fn dp(p: &[Vec2], i: usize, j: usize, tol: f64, keep: &mut [bool]) {
    if j <= i + 1 {
        return;
    }
    let (a, b) = (p[i], p[j]);
    let d = b - a;
    let l = d.length();
    let (mut best, mut idx) = (0.0, i);
    for (k, &q) in p.iter().enumerate().take(j).skip(i + 1) {
        let dist = if l > 0.0 {
            cross(d, q - a).abs() / l
        } else {
            q.distance(a)
        };
        if dist > best {
            (best, idx) = (dist, k);
        }
    }
    if best > tol {
        keep[idx] = true;
        dp(p, i, idx, tol, keep);
        dp(p, idx, j, tol, keep);
    }
}

/// Joins ordered pieces into runs. With a travel graph, the gap between two pieces is sewn
/// as a running stitch inside the region (`pitch` long stitches); without one (or when no
/// inside path exists) a new run starts, which the compiler turns into a jump.
pub fn join(
    pieces: Vec<Vec<Vec2>>,
    mut travel: Option<&mut TravelGraph>,
    pitch: f64,
) -> Vec<Vec<Vec2>> {
    let mut runs: Vec<Vec<Vec2>> = Vec::new();
    for piece in pieces {
        let Some(last) = runs.last_mut() else {
            runs.push(piece);
            continue;
        };
        let end = *last.last().expect("non-empty run");
        let start = piece[0];
        if end.distance(start) < 1e-6 {
            last.extend(piece.into_iter().skip(1));
            continue;
        }
        match travel.as_deref_mut().and_then(|g| g.path(end, start)) {
            Some(path) => {
                last.extend(stitches(&path, pitch).into_iter().skip(1));
                last.extend(piece.into_iter().skip(1));
            }
            None => runs.push(piece),
        }
    }
    runs
}

/// Travel this close (0.1 mm) to the outline lies under the row ends there (#901).
const OUTLINE_HIDES: f64 = 6.0;
/// Least share of the fabric a layer's own rows must cover under a stretch of its travel
/// for that stretch to be hidden (#901): over thinner cover, the other colours' rows between
/// its own show the travel.
const COVER_HIDES: f64 = 0.7;
/// How finely (0.1 mm) [`join_within`] samples a way for its exposure.
const EXPOSURE_STEP: f64 = 2.0;
/// Furthest (0.1 mm) [`join_within`] travels back along the run just sewn.
const RETRACE: f64 = 300.0;
/// How many of the nearest pieces [`join_within`] tries before it trims to the nearest.
const ORDER_TRIES: usize = 6;
/// How many of a run end's nearest other run ends [`join_within`] tries to link to.
const CHAIN_TRIES: usize = 8;
/// Furthest apart (0.1 mm) two run ends [`join_within`] tries to link.
const CHAIN_REACH: f64 = 200.0;

/// How much of each 1 mm cell a layer's rows cover: their length in the cell over the length
/// rows `spacing` apart would sew in all of it.
struct Cover {
    full: f64,
    length: std::collections::HashMap<(i64, i64), f64>,
}

impl Cover {
    fn key(p: Vec2) -> (i64, i64) {
        ((p.x / 10.0).floor() as i64, (p.y / 10.0).floor() as i64)
    }

    fn new(rows: &[Vec<Vec2>], spacing: f64) -> Cover {
        let mut length = std::collections::HashMap::new();
        for row in rows {
            for w in row.windows(2) {
                let len = w[0].distance(w[1]);
                let n = (len / EXPOSURE_STEP).ceil().max(1.0) as usize;
                for i in 0..n {
                    let p = w[0].lerp(w[1], (i as f64 + 0.5) / n as f64);
                    *length.entry(Self::key(p)).or_insert(0.0) += len / n as f64;
                }
            }
        }
        Cover {
            full: 100.0 / spacing.max(1e-9),
            length,
        }
    }

    /// Whether the layer's rows cover at least [`COVER_HIDES`] of the cell at `p`.
    fn hides(&self, p: Vec2) -> bool {
        self.length
            .get(&Self::key(p))
            .is_some_and(|&l| l >= COVER_HIDES * self.full)
    }
}

/// [`join`] for a sparse layer (a colour's share of a blend or tartan), where nothing but the
/// layer's own stitches covers travel (#891, #901). Each gap takes the least exposed of: the
/// inside path; back along the run just sewn (at most [`RETRACE`]) to its point nearest the
/// next piece, then inside from there; and, with `walks`, along the outline (directly, or out
/// to it and in again). Travel over 1 mm cells where the layer's own rows (`spacing` apart)
/// cover at least [`COVER_HIDES`] of the fabric, or with `walks` within [`OUTLINE_HIDES`] of
/// the outline, is hidden. A way is taken when its exposure in all
/// is at most `reach`; otherwise a new run starts (a trim). Pieces are sewn nearest first, but
/// where the nearest can only be reached by a trim, the nearest of the next [`ORDER_TRIES`]
/// that can be reached hidden goes first. The runs that leaves are then joined end to end into as few
/// chains as hidden ways allow: each run end linked to one of its [`CHAIN_TRIES`] nearest other
/// run ends (within [`CHAIN_REACH`]), links taken shortest first while each run has at most one
/// way in and one way out and no loop forms (a path cover, #901).
///
/// `walks` is for a layer whose outline its own rows cover (a tartan colour's stripes reach the
/// edge along their whole width); a blend colour's travel along the outline showed across the
/// other colours' row ends there (a white line along a blue edge).
pub fn join_within(
    pieces: Vec<Vec<Vec2>>,
    travel: &mut TravelGraph,
    pitch: f64,
    reach: f64,
    walks: bool,
    spacing: f64,
) -> Vec<Vec<Vec2>> {
    let outline = travel.edges.clone();
    let near_outline = |p: Vec2| {
        outline
            .iter()
            .any(|&(a, b)| crate::simplify::segment_distance(p, a, b) <= OUTLINE_HIDES)
    };
    let cover = Cover::new(&pieces, spacing);
    let exposure = |way: &[Vec2]| -> (f64, f64) {
        let (mut longest, mut total, mut run) = (0.0f64, 0.0, 0.0);
        for w in way.windows(2) {
            let len = w[0].distance(w[1]);
            let n = (len / EXPOSURE_STEP).ceil().max(1.0) as usize;
            for i in 0..n {
                let p = w[0].lerp(w[1], (i as f64 + 0.5) / n as f64);
                if (walks && near_outline(p)) || cover.hides(p) {
                    run = 0.0;
                } else {
                    run += len / n as f64;
                    total += len / n as f64;
                    longest = longest.max(run);
                }
            }
        }
        (longest, total)
    };
    // The way from the end of `last` to `start` hidden enough to sew, if any.
    let mut hidden_way = |last: &[Vec2], start: Vec2| -> Option<Vec<Vec2>> {
        let end = *last.last().expect("non-empty run");
        if end.distance(start) < 1e-6 {
            return Some(vec![end, start]);
        }
        let inside = travel.path(end, start);
        // A short way inside is taken at once, as before.
        if let Some(path) = inside.as_ref().filter(|p| length(p) <= reach) {
            return Some(path.clone());
        }
        let mut ways: Vec<Vec<Vec2>> = Vec::new();
        ways.extend(inside);
        if walks {
            ways.extend(travel.edge_path_within(end, start, OUTLINE_HIDES));
        }
        // Out to the outline, along it and in again: only the two legs can show.
        let foot = |p: Vec2| {
            outline
                .iter()
                .map(|&(a, b)| {
                    let d = b - a;
                    let l2 = d.x * d.x + d.y * d.y;
                    let t = if l2 > 0.0 {
                        (((p - a).x * d.x + (p - a).y * d.y) / l2).clamp(0.0, 1.0)
                    } else {
                        0.0
                    };
                    a.lerp(b, t)
                })
                .min_by(|x, y| x.distance(p).total_cmp(&y.distance(p)))
        };
        if walks
            && let (Some(fe), Some(fs)) = (foot(end), foot(start))
            && fe.distance(end) + fs.distance(start) <= reach
            && let Some(walk) = travel.edge_path_within(fe, fs, 1.0)
        {
            let mut way = vec![end];
            way.extend(walk);
            way.push(start);
            ways.push(way);
        }
        // Back along the run just sewn (at most RETRACE), to its point nearest the next piece.
        let mut back = 0.0;
        let mut nearest = (last.len() - 1, end.distance(start));
        for i in (0..last.len() - 1).rev() {
            back += last[i].distance(last[i + 1]);
            if back > RETRACE {
                break;
            }
            if last[i].distance(start) < nearest.1 {
                nearest = (i, last[i].distance(start));
            }
        }
        if nearest.0 + 1 < last.len()
            && let Some(across) = travel.path(last[nearest.0], start)
        {
            let mut way: Vec<Vec2> = last[nearest.0..].iter().rev().copied().collect();
            way.extend(across.into_iter().skip(1));
            ways.push(way);
        }
        ways.into_iter()
            .map(|w| {
                let (longest, total) = exposure(&w);
                (longest, total, length(&w), w)
            })
            .filter(|&(longest, total, _, _)| longest <= reach && total <= reach)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.2.total_cmp(&b.2)))
            .map(|(_, _, _, w)| w)
    };
    // Nearest piece first, as `order`; but where the nearest can only be reached by a trim, the
    // nearest of the next few that can be reached hidden is sewn first (#901).
    let mut left: Vec<Vec<Vec2>> = pieces.into_iter().filter(|p| !p.is_empty()).collect();
    let mut runs: Vec<Vec<Vec2>> = Vec::new();
    if left.is_empty() {
        return runs;
    }
    let first = left.remove(0);
    runs.push(first);
    while !left.is_empty() {
        let last = runs.last_mut().expect("a run");
        let end = *last.last().expect("non-empty run");
        // (piece, reversed, distance), nearest first.
        let mut near: Vec<(usize, bool, f64)> = left
            .iter()
            .enumerate()
            .flat_map(|(i, p)| {
                [
                    (i, false, end.distance(p[0])),
                    (i, true, end.distance(*p.last().expect("non-empty piece"))),
                ]
            })
            .collect();
        near.sort_by(|a, b| a.2.total_cmp(&b.2).then(a.0.cmp(&b.0)));
        let mut chosen = None;
        for &(i, rev, _) in near.iter().take(2 * ORDER_TRIES) {
            let start = if rev {
                *left[i].last().expect("non-empty piece")
            } else {
                left[i][0]
            };
            if let Some(way) = hidden_way(last, start) {
                chosen = Some((i, rev, Some(way)));
                break;
            }
        }
        let (i, rev, way) = chosen.unwrap_or((near[0].0, near[0].1, None));
        let mut piece = left.remove(i);
        if rev {
            piece.reverse();
        }
        match way {
            Some(way) => {
                let travel_stitches = stitches(&way, pitch);
                last.extend(travel_stitches.into_iter().skip(1));
                last.extend(piece.into_iter().skip(1));
            }
            None => {
                runs.push(piece);
            }
        }
    }
    // Chains of runs (#901): the runs above joined end to end wherever a hidden way links one
    // run's end to another's (going back along the whole run first, as from the run just sewn),
    // links taken shortest first while both ends are free and no loop forms: as few runs, and so
    // as few trims, as those links allow. A run may be sewn the other way round.
    let n = runs.len();
    if n < 2 {
        return runs;
    }
    // Node 2i is run i's first point, 2i + 1 its last.
    let at = |node: usize| -> Vec2 {
        let r = &runs[node / 2];
        if node.is_multiple_of(2) {
            r[0]
        } else {
            *r.last().expect("non-empty run")
        }
    };
    let mut links: Vec<(f64, usize, usize, Vec<Vec2>)> = Vec::new();
    for a in 0..2 * n {
        let mut near: Vec<(f64, usize)> = (0..2 * n)
            .filter(|&b| b / 2 != a / 2)
            .map(|b| (at(a).distance(at(b)), b))
            .filter(|&(d, _)| d <= CHAIN_REACH)
            .collect();
        near.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        // Leaving run a / 2 at node a: it was sewn ending there.
        let mut from = runs[a / 2].clone();
        if a.is_multiple_of(2) {
            from.reverse();
        }
        for &(_, b) in near.iter().take(CHAIN_TRIES) {
            if let Some(way) = hidden_way(&from, at(b)) {
                links.push((length(&way), a, b, way));
            }
        }
    }
    links.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)).then(x.2.cmp(&y.2)));
    fn root(parent: &mut [usize], mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    let mut parent: Vec<usize> = (0..n).collect();
    // Leaving at node a, the node entered next and the way there.
    let mut next: Vec<Option<(usize, Vec<Vec2>)>> = vec![None; 2 * n];
    let mut entered = vec![false; 2 * n];
    for (_, a, b, way) in links {
        // Run a / 2 leaves at a: it has no way out yet and is not entered at a. Run b / 2 is
        // entered at b: it has no way in yet and does not leave at b.
        let out_free = next[a].is_none() && next[a ^ 1].is_none() && !entered[a];
        let in_free = !entered[b] && !entered[b ^ 1] && next[b].is_none();
        if !out_free || !in_free {
            continue;
        }
        let (ra, rb) = (root(&mut parent, a / 2), root(&mut parent, b / 2));
        if ra == rb {
            continue;
        }
        parent[ra] = rb;
        next[a] = Some((b, way));
        entered[b] = true;
    }
    // A chain starts at a run entered by no link, at the node that does not lead on: each run's
    // other node either leads on (next) or ends the chain.
    let mut sewn = vec![false; n];
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    let start_of = |i: usize| -> Option<usize> {
        // Run i starts a chain when neither of its nodes is entered by a link; it is sewn from
        // the node that does not lead on (its first point when neither does).
        if entered[2 * i] || entered[2 * i + 1] {
            return None;
        }
        Some(if next[2 * i].is_some() {
            2 * i + 1
        } else {
            2 * i
        })
    };
    // Chains in the runs' own order of their first runs, so the first run stays first.
    for i in 0..n {
        let Some(mut node) = start_of(i) else {
            continue;
        };
        let mut chain: Vec<Vec2> = Vec::new();
        loop {
            let r = node / 2;
            sewn[r] = true;
            let mut run = runs[r].clone();
            if !node.is_multiple_of(2) {
                run.reverse();
            }
            if chain.is_empty() {
                chain = run;
            } else {
                chain.extend(run.into_iter().skip(1));
            }
            match &next[node ^ 1] {
                Some((b, way)) => {
                    chain.extend(stitches(way, pitch).into_iter().skip(1));
                    node = *b;
                }
                None => break,
            }
        }
        out.push(chain);
    }
    debug_assert!(sewn.iter().all(|&s| s), "every run in one chain");
    out
}

/// Deterministic pseudo-random numbers in [0, 1) (SplitMix64), so generated designs are
/// reproducible.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Bezier;
    use crate::model::{RegionSewing, RegionType};
    use openstitch_core::Thread;

    /// A ring on Bézier curves: a 60 mm round outline with a 30 mm round hole, four cubic
    /// quarters each, so fill rows split and travel is routed round the hole.
    fn curved_ring() -> Vec<Contour> {
        let centre = v2(400.0, 400.0);
        let round = |r: f64, reverse: bool| {
            let k = 0.5523;
            let s = if reverse { -1.0 } else { 1.0 };
            let mut pts = [v2(r, 0.0), v2(0.0, r), v2(-r, 0.0), v2(0.0, -r)];
            if reverse {
                pts.reverse();
            }
            let mut c = Contour::new(pts.iter().map(|&p| p + centre).collect(), true);
            for i in 0..4 {
                let (a, b) = (pts[i], pts[(i + 1) % 4]);
                // Tangents perpendicular to the radius: a quarter circle's control points.
                let ta = v2(-a.y, a.x) * (k * s);
                let tb = v2(-b.y, b.x) * (k * s);
                c.set_curve(
                    i,
                    Some(Bezier {
                        c1: a + ta + centre,
                        c2: b - tb + centre,
                    }),
                );
            }
            c
        };
        vec![round(300.0, false), round(150.0, true)]
    }

    fn flattened(cs: &[Contour]) -> Vec<Contour> {
        cs.iter()
            .map(|c| Contour::new(c.outline_points(), true))
            .collect()
    }

    #[test]
    fn a_curved_outline_routes_as_its_flattened_polygon() {
        let ring = curved_ring();
        let flat = flattened(&ring);
        let (a, b) = (v2(150.0, 400.0), v2(650.0, 400.0));
        let mut g1 = TravelGraph::new(&ring, &[]);
        let mut g2 = TravelGraph::new(&flat, &[]);
        assert_eq!(g1.path(a, b), g2.path(a, b));
        assert!(g1.path(a, b).is_some_and(|p| p.len() > 2), "round the hole");
    }

    /// #679 timing: fill layers on curved outlines against the same outlines flattened first.
    /// `cargo test -p openstitch-design --lib curved_layers_timing -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn curved_layers_timing() {
        let ring = curved_ring();
        let flat = flattened(&ring);
        let r = RegionSewing::new(RegionType::Fill, Thread::rgb(0));
        let time = |cs: &[Contour]| {
            let t = std::time::Instant::now();
            for _ in 0..5 {
                std::hint::black_box(crate::region::layers_with(cs, &r, &[], &[]));
            }
            t.elapsed().as_secs_f64()
        };
        let (curved, flat_first) = (time(&ring), time(&flat));
        eprintln!("curved {curved:.3} s, flattened first {flat_first:.3} s");
    }
}
