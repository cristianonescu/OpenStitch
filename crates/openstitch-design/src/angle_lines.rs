//! Stitch angle lines (#534, beyond PE-DESIGN): several lines across one fill set the direction
//! of its rows, which turn smoothly from one line to the next, so a U- or Z-shaped region (or a
//! ring) is sewn in one object with no seam.
//!
//! - Each line is taken as the **chord** of the shape through it: from the line's middle
//!   outwards both ways to where it first meets the outline (or a hole).
//! - Between consecutive chords the outline runs along two **sides**. Row t (0 at the first
//!   chord, 1 at the next) joins the point at fraction t along one side with the point at
//!   fraction t along the other, so the rows turn with the shape and stay inside it. There are
//!   as many rows as the longer side needs at the row spacing; where the shorter side (the
//!   inside of a bend) crowds them, rows are left out in pairs (every other one, then every
//!   fourth…) so they stay at least 0.85 of a spacing apart. The row count is rounded up to a
//!   multiple of that thinning, so the kept rows reach the next chord as evenly as they leave
//!   this one.
//! - Before the first chord and after the last, rows run parallel to it, a spacing apart.
//! - A shape with a hole and three or more lines (a ring) wraps from the last line to the first.
//!
//! Lines that cross inside the shape, or whose middle is not inside it, are refused (see
//! [`check`]). Lengths are 0.1 mm.

use crate::fmath::Det;
use crate::generate::Run;
use crate::geometry::{Contour, Vec2, contains, v2};

/// Rows of one band closer than this many spacings are thinned (in pairs).
const CROWDED: f64 = 0.85;

/// Why angle lines cannot be sewn, if they cannot.
pub fn check(lines: &[[Vec2; 2]], closed: &[Contour], holes: &[Contour]) -> Result<(), String> {
    if lines.len() < 2 {
        return Err("angle lines need at least two lines".into());
    }
    let inside = |p: Vec2| contains(p, closed) && !contains(p, holes);
    for (i, l) in lines.iter().enumerate() {
        if l[0].distance(l[1]) < 1.0 {
            return Err(format!("angle line {} has no length", i + 1));
        }
        if !inside(l[0].lerp(l[1], 0.5)) {
            return Err(format!(
                "angle line {} does not cross the shape at its middle",
                i + 1
            ));
        }
    }
    let chords: Vec<[Vec2; 2]> = lines.iter().map(|l| chord(*l, closed, holes)).collect();
    for i in 0..chords.len() {
        for j in i + 1..chords.len() {
            if let Some(p) = crossing(chords[i], chords[j])
                && inside(p)
            {
                return Err(format!(
                    "angle lines {} and {} cross inside the shape",
                    i + 1,
                    j + 1
                ));
            }
        }
    }
    Ok(())
}

/// Why PE-DESIGN's direction lines (`AngleRule::Fan`, #1273) cannot be sewn, if they cannot:
/// what [`check`] refuses, two neighbouring lines on one straight line (there is no zone
/// between them and none on either side: the rows before the first and after the second would
/// be the same ones), and lines that do not lie one after another across the shape (each line
/// but the first and last has its two neighbours on its two sides, going round the outline).
pub fn check_fan(lines: &[[Vec2; 2]], closed: &[Contour], holes: &[Contour]) -> Result<(), String> {
    check(lines, closed, holes)?;
    let chords: Vec<[Vec2; 2]> = lines.iter().map(|l| chord(*l, closed, holes)).collect();
    for i in 1..chords.len() {
        if collinear(chords[i - 1], chords[i]) {
            return Err(format!(
                "angle lines {} and {} lie on one straight line",
                i,
                i + 1
            ));
        }
    }
    if let Some(arcs) = arcs(&chords, closed) {
        for i in 1..arcs.len().saturating_sub(1) {
            if arcs[i].holds(arcs[i - 1].a) == arcs[i].holds(arcs[i + 1].a) {
                return Err(format!(
                    "angle line {} does not lie between lines {} and {}: the lines must follow one another across the shape",
                    i + 1,
                    i,
                    i + 2
                ));
            }
        }
    }
    Ok(())
}

/// Whether chord `b` lies on the straight line through chord `a` (within 0.001 mm).
fn collinear(a: [Vec2; 2], b: [Vec2; 2]) -> bool {
    let d = a[1] - a[0];
    let len = d.length().max(1e-9);
    let off = |q: Vec2| ((q.x - a[0].x) * d.y - (q.y - a[0].y) * d.x).abs() / len;
    off(b[0]) < 0.01 && off(b[1]) < 0.01
}

/// A chord's two ends as arc-length positions on the outline it crosses.
#[derive(Clone, Copy, Debug)]
struct Arc {
    a: f64,
    b: f64,
    len: f64,
}

impl Arc {
    /// Whether outline position `s` lies on the way round from `a` forward to `b`.
    fn holds(&self, s: f64) -> bool {
        (s - self.a).rem_euclid(self.len) < (self.b - self.a).rem_euclid(self.len)
    }
}

/// The chords as [`Arc`]s when all their ends lie on one outline; `None` otherwise.
fn arcs(chords: &[[Vec2; 2]], closed: &[Contour]) -> Option<Vec<Arc>> {
    let rings: Vec<Ring> = closed.iter().filter_map(Ring::new).collect();
    let mut on = None;
    let mut out = Vec::with_capacity(chords.len());
    for c in chords {
        let (i, a, da) = nearest(&rings, c[0])?;
        let (j, b, db) = nearest(&rings, c[1])?;
        if i != j || *on.get_or_insert(i) != i || da > 1.0 || db > 1.0 {
            return None;
        }
        out.push(Arc {
            a,
            b,
            len: rings[i].len(),
        });
    }
    Some(out)
}

/// The chord of the shape through line `l`: from its middle both ways to the nearest outline
/// or hole crossing (the line itself where it meets nothing).
pub fn chord(l: [Vec2; 2], closed: &[Contour], holes: &[Contour]) -> [Vec2; 2] {
    let mid = l[0].lerp(l[1], 0.5);
    let d = l[1] - l[0];
    let len = d.length();
    if len <= 0.0 {
        return l;
    }
    let u = d * (1.0 / len);
    let (mut back, mut ahead) = (f64::INFINITY, f64::INFINITY);
    for c in closed.iter().chain(holes).filter(|c| c.closed) {
        for (a, b) in c.edges() {
            let e = b - a;
            let den = u.x * e.y - u.y * e.x;
            if den.abs() < 1e-12 {
                continue;
            }
            let w = a - mid;
            let t = (w.x * e.y - w.y * e.x) / den;
            let s = (w.x * u.y - w.y * u.x) / den;
            if !(0.0..=1.0).contains(&s) {
                continue;
            }
            if t > 0.0 {
                ahead = ahead.min(t);
            } else {
                back = back.min(-t);
            }
        }
    }
    let half = len / 2.0;
    let back = if back.is_finite() { back } else { half };
    let ahead = if ahead.is_finite() { ahead } else { half };
    [mid - u * back, mid + u * ahead]
}

/// Where segments `a` and `b` cross, if they do.
fn crossing(a: [Vec2; 2], b: [Vec2; 2]) -> Option<Vec2> {
    let (p, r) = (a[0], a[1] - a[0]);
    let (q, s) = (b[0], b[1] - b[0]);
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let w = q - p;
    let t = (w.x * s.y - w.y * s.x) / den;
    let v = (w.x * r.y - w.y * r.x) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&v)).then(|| p + r * t)
}

/// A closed outline as a polygon with arc-length positions of its vertices.
struct Ring {
    pts: Vec<Vec2>,
    at: Vec<f64>,
}

impl Ring {
    fn new(c: &Contour) -> Option<Ring> {
        let mut pts = c.outline_points();
        if pts.len() < 3 {
            return None;
        }
        pts.push(pts[0]);
        let mut at = vec![0.0];
        for w in pts.windows(2) {
            at.push(at[at.len() - 1] + w[0].distance(w[1]));
        }
        Some(Ring { pts, at })
    }

    fn len(&self) -> f64 {
        self.at[self.at.len() - 1]
    }

    /// Arc-length position of the outline point nearest `p`, and its distance.
    fn locate(&self, p: Vec2) -> (f64, f64) {
        let mut best = (0.0, f64::INFINITY);
        for i in 0..self.pts.len() - 1 {
            let (a, b) = (self.pts[i], self.pts[i + 1]);
            let e = b - a;
            let l2 = e.x * e.x + e.y * e.y;
            let t = if l2 > 0.0 {
                (((p.x - a.x) * e.x + (p.y - a.y) * e.y) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let d = p.distance(a + e * t);
            if d < best.1 {
                best = (self.at[i] + (self.at[i + 1] - self.at[i]) * t, d);
            }
        }
        best
    }

    /// The outline point at arc-length position `s` (wrapping).
    fn point(&self, s: f64) -> Vec2 {
        let s = s.rem_euclid(self.len());
        let i = self
            .at
            .partition_point(|&a| a <= s)
            .clamp(1, self.pts.len() - 1);
        let span = self.at[i] - self.at[i - 1];
        let f = if span > 0.0 {
            (s - self.at[i - 1]) / span
        } else {
            0.0
        };
        self.pts[i - 1].lerp(self.pts[i], f)
    }
}

/// One side of a band: the outline from one chord end to the next, as positions on `ring`
/// (from `s0`, `len` along it, `dir` ±1).
struct Side<'a> {
    ring: &'a Ring,
    s0: f64,
    len: f64,
    dir: f64,
}

impl Side<'_> {
    fn at(&self, t: f64) -> Vec2 {
        self.ring.point(self.s0 + self.dir * self.len * t)
    }
}

/// The outline nearest `p`: its index, `p`'s arc-length position on it and its distance.
fn nearest(rings: &[Ring], p: Vec2) -> Option<(usize, f64, f64)> {
    rings
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let (s, d) = r.locate(p);
            (i, s, d)
        })
        .min_by(|x, y| x.2.total_cmp(&y.2))
}

/// The side from `a` to `b` on the same outline, the way round that passes none of `others`
/// (or the shorter), if they are on one outline.
fn side<'a>(rings: &'a [Ring], a: Vec2, b: Vec2, others: &[Vec2]) -> Option<Side<'a>> {
    let (ia, sa, _) = nearest(rings, a)?;
    let (ib, sb, _) = nearest(rings, b)?;
    if ia != ib {
        return None;
    }
    let ring = &rings[ia];
    let total = ring.len();
    let fwd = (sb - sa).rem_euclid(total);
    let passes = |dir: f64, len: f64| {
        others.iter().any(|&o| match nearest(rings, o) {
            Some((io, so, d)) if io == ia && d < 1.0 => {
                let k = (dir * (so - sa)).rem_euclid(total);
                k > 1e-6 && k < len - 1e-6
            }
            _ => false,
        })
    };
    let (dir, len) = match (passes(1.0, fwd), passes(-1.0, total - fwd)) {
        (false, true) => (1.0, fwd),
        (true, false) => (-1.0, total - fwd),
        _ if fwd <= total - fwd => (1.0, fwd),
        _ => (-1.0, total - fwd),
    };
    Some(Side {
        ring,
        s0: sa,
        len,
        dir,
    })
}

/// The rows (segments or polylines, in sewing order) the angle lines give, `spacing` apart.
pub fn rows(lines: &[[Vec2; 2]], closed: &[Contour], holes: &[Contour], spacing: f64) -> Vec<Run> {
    if lines.len() < 2 || spacing <= 0.0 {
        return Vec::new();
    }
    let rings: Vec<Ring> = closed
        .iter()
        .chain(holes)
        .filter(|c| c.closed)
        .filter_map(Ring::new)
        .collect();
    let mut chords: Vec<[Vec2; 2]> = lines.iter().map(|l| chord(*l, closed, holes)).collect();
    // Consecutive chords paired end to end so the rows do not twist between them.
    for i in 1..chords.len() {
        let (a, b) = (chords[i - 1], chords[i]);
        if a[0].distance(b[1]) + a[1].distance(b[0]) < a[0].distance(b[0]) + a[1].distance(b[1]) {
            chords[i] = [b[1], b[0]];
        }
    }
    let ring = rings.len() >= 2 && chords.len() >= 3;
    let ends: Vec<Vec2> = chords.iter().flatten().copied().collect();
    let mut out: Vec<Run> = Vec::new();
    // Before the first chord: parallel rows stepping away from the second.
    if !ring {
        out.extend(
            parallel(chords[0], chords[1], closed, holes, spacing)
                .into_iter()
                .rev(),
        );
    }
    let pairs = chords.len() - 1 + usize::from(ring);
    for i in 0..pairs {
        let (a, b) = (chords[i], chords[(i + 1) % chords.len()]);
        let last = i + 1 == pairs && !ring;
        let others: Vec<Vec2> = ends
            .iter()
            .copied()
            .filter(|p| ![a[0], a[1], b[0], b[1]].contains(p))
            .collect();
        match (
            side(&rings, a[0], b[0], &others),
            side(&rings, a[1], b[1], &others),
        ) {
            (Some(s1), Some(s2)) => out.extend(band(&s1, &s2, spacing, last)),
            // Ends on different outlines: straight from chord to chord.
            _ => out.extend(straight(a, b, spacing, last)),
        }
    }
    if !ring {
        let n = chords.len();
        out.extend(parallel(
            chords[n - 1],
            chords[n - 2],
            closed,
            holes,
            spacing,
        ));
    }
    out
}

/// Rows parallel to chord `c` on the side away from `other`, `spacing` apart, while their
/// middle is inside the shape.
fn parallel(
    c: [Vec2; 2],
    other: [Vec2; 2],
    closed: &[Contour],
    holes: &[Contour],
    spacing: f64,
) -> Vec<Run> {
    let d = c[1] - c[0];
    let len = d.length().max(1e-9);
    let mut n = v2(-d.y / len, d.x / len);
    let to_other = other[0].lerp(other[1], 0.5) - c[0].lerp(c[1], 0.5);
    if n.x * to_other.x + n.y * to_other.y > 0.0 {
        n = n * -1.0;
    }
    let inside = |p: Vec2| contains(p, closed) && !contains(p, holes);
    let mut out = Vec::new();
    for k in 1..10_000 {
        let shift = n * (spacing * k as f64);
        let mid = c[0].lerp(c[1], 0.5) + shift;
        if !inside(mid) {
            break;
        }
        let ch = chord([c[0] + shift, c[1] + shift], closed, holes);
        out.push(vec![ch[0], ch[1]]);
    }
    out
}

/// Where row `k` of a band of `n` is sewn, as fractions along it: where the row gap there
/// (from `g0` at one end to `g1` at the other) times its thinning reaches [`CROWDED`]
/// spacings. Every row is sewn where the gap allows; only every 2^m-th where it is 2^m times
/// too close; and none closer than that to the band's far end (the next band's first row).
fn kept(k: usize, n: usize, g0: f64, g1: f64, spacing: f64) -> impl Fn(f64) -> bool {
    move |u: f64| {
        let gap = g0 + (g1 - g0) * u;
        if gap <= 1e-9 {
            return k == 0;
        }
        if k != 0 && k < n && (n - k) as f64 * gap < CROWDED * spacing {
            return false;
        }
        let mut m = 0u32;
        while gap * f64::from(1u32 << m) < CROWDED * spacing && m < 30 {
            m += 1;
        }
        k.is_multiple_of(1usize << m)
    }
}

/// Splits row `p(u)`, u in 0…1, into the pieces where `keep` holds.
fn pieces(p: impl Fn(f64) -> Vec2, keep: impl Fn(f64) -> bool) -> Vec<Run> {
    const STEPS: usize = 32;
    let mut out = Vec::new();
    let mut piece: Run = Vec::new();
    for s in 0..=STEPS {
        let u = s as f64 / STEPS as f64;
        if keep(u) {
            piece.push(p(u));
        } else if piece.len() >= 2 {
            out.push(std::mem::take(&mut piece));
        } else {
            piece.clear();
        }
    }
    if piece.len() >= 2 {
        out.push(piece);
    }
    out
}

/// How many row steps a band whose sides are `l0` and `l1` long is laid in: as many as the
/// longer side needs at `spacing`, rounded up to a multiple of the coarsest thinning ([`kept`])
/// its shorter side needs, so every thinning ends on the next band's first row as evenly as it
/// starts (#890: an odd count left the last kept row three steps short of the next line where
/// the rows were thinned to every other one, a thin streak along it).
fn steps(l0: f64, l1: f64, spacing: f64) -> usize {
    let n = ((l0.max(l1) / spacing).ceil() as usize).max(1);
    let gap = l0.min(l1) / n as f64;
    let mut step = 1usize;
    while gap * (step as f64) < CROWDED * spacing && step * 2 <= n / 2 {
        step *= 2;
    }
    n.div_ceil(step) * step
}

/// The rows between two sides: row t joins `s1.at(t)` and `s2.at(t)`.
fn band(s1: &Side, s2: &Side, spacing: f64, last: bool) -> Vec<Run> {
    let n = steps(s1.len, s2.len, spacing);
    let mut out = Vec::new();
    for k in 0..n + usize::from(last) {
        let t = k as f64 / n as f64;
        let (p, q) = (s1.at(t), s2.at(t));
        out.extend(pieces(
            |u| p.lerp(q, u),
            kept(k, n, s1.len / n as f64, s2.len / n as f64, spacing),
        ));
    }
    out
}

/// The rows between chords `a` and `b` by straight interpolation (ends on different outlines).
fn straight(a: [Vec2; 2], b: [Vec2; 2], spacing: f64, last: bool) -> Vec<Run> {
    let (l0, l1) = (a[0].distance(b[0]), a[1].distance(b[1]));
    let n = steps(l0, l1, spacing);
    let mut out = Vec::new();
    for k in 0..n + usize::from(last) {
        let t = k as f64 / n as f64;
        let (p, q) = (a[0].lerp(b[0], t), a[1].lerp(b[1], t));
        out.extend(pieces(
            |u| p.lerp(q, u),
            kept(k, n, l0 / n as f64, l1 / n as f64, spacing),
        ));
    }
    out
}

/// Needle points along `piece` every `stitch`, starting `phase` (0…1 of a stitch) in, with
/// both ends kept and no stitch under a quarter stitch at the ends (a tatami row).
pub fn needles(piece: &[Vec2], stitch: f64, phase: f64) -> Run {
    let mut at = vec![0.0];
    for w in piece.windows(2) {
        at.push(at[at.len() - 1] + w[0].distance(w[1]));
    }
    let total = at[at.len() - 1];
    if piece.len() < 2 || total <= 0.0 {
        return piece.to_vec();
    }
    let point = |d: f64| {
        let i = at.partition_point(|&a| a < d).clamp(1, piece.len() - 1);
        let span = at[i] - at[i - 1];
        let f = if span > 0.0 {
            (d - at[i - 1]) / span
        } else {
            0.0
        };
        piece[i - 1].lerp(piece[i], f.clamp(0.0, 1.0))
    };
    let mut out = vec![piece[0]];
    let mut d = phase.rem_euclid(1.0) * stitch;
    if d < stitch * 0.25 {
        d += stitch;
    }
    while d < total - stitch * 0.25 {
        out.push(point(d));
        d += stitch;
    }
    out.push(piece[piece.len() - 1]);
    out
}

// MARK: PE-DESIGN's direction lines (#1273)

/// Where the infinite lines through chords `a` and `b` meet; `None` when they are parallel or
/// meet farther than 100 m away (they are sewn as parallel then).
fn meeting(a: [Vec2; 2], b: [Vec2; 2]) -> Option<Vec2> {
    let (r, s) = (a[1] - a[0], b[1] - b[0]);
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-9 * r.length() * s.length() {
        return None;
    }
    let w = b[0] - a[0];
    let t = (w.x * s.y - w.y * s.x) / den;
    let p = a[0] + r * t;
    (p.distance(a[0]) < 1.0e6).then_some(p)
}

/// The cuts of the zone between chords `a` and `b` (paired end to end), in order from `a` to
/// `b`. The zone has two outline runs: from `a`'s first end to `b`'s, and from `a`'s second
/// end to `b`'s. Every vertex of the outline strictly inside one of the runs cuts the zone,
/// by the straight line from that vertex to the point at the same fraction, by length, along
/// the other run. This is how PE-DESIGN 11.50 sews it (#1273; its files `g1` to `g11`): with
/// no vertex in either run the zone is one piece, a fan through the two lines' meeting
/// point; a corner between the lines, or a point inserted on a side, makes two pieces with
/// a fan each. On a curved outline the vertices are those of its flattened path, at most
/// [`MAX_CUTS`] of them. Empty when the chords' ends are not on one outline.
fn cuts(rings: &[Ring], closed: &[Contour], a: [Vec2; 2], b: [Vec2; 2]) -> Vec<[Vec2; 2]> {
    // Lines that end on a hole have no outline run between their ends.
    if ![a[0], a[1], b[0], b[1]]
        .iter()
        .all(|p| on_outline(rings, *p))
    {
        return Vec::new();
    }
    let (Some(first), Some(second)) = (
        side(rings, a[0], b[0], &[a[1], b[1]]),
        side(rings, a[1], b[1], &[a[0], b[0]]),
    ) else {
        return Vec::new();
    };
    // Vertices nearer than this to a run's end are the lines' own crossings (0.01 mm).
    const AT_END: f64 = 0.1;
    let mut at: Vec<(f64, bool)> = Vec::new();
    for run in [&first, &second] {
        if run.len <= 2.0 * AT_END {
            continue;
        }
        let total = run.ring.len();
        for (i, &v) in run.ring.at[..run.ring.at.len() - 1].iter().enumerate() {
            let along = (run.dir * (v - run.s0)).rem_euclid(total);
            if along > AT_END && along < run.len - AT_END {
                at.push((along / run.len, corner(run.ring, i)));
            }
        }
    }
    let at = thinned(at);
    let found: Vec<[Vec2; 2]> = at
        .into_iter()
        .map(|f| [first.at(f), second.at(f)])
        .filter(|c| c[0].distance(c[1]) > 1.0)
        .collect();
    lying_inside(found, closed, &[a, b])
}

/// Whether `p` lies on one of the outlines (within 0.1 mm).
fn on_outline(rings: &[Ring], p: Vec2) -> bool {
    nearest(rings, p).is_some_and(|(_, _, d)| d < 1.0)
}

/// `found` when every cut lies inside the shape (a point every 0.5 mm of each is looked
/// at) and no two of them, nor a cut and one of the zone's `lines`, cross each other; else
/// none: the zone then stays in one piece. Not only the cut that leaves the shape
/// is dropped, because the cuts only make sense together: without one of them the pieces
/// beside it span its vertex, and their bounding lines (the cuts left on either side) can
/// pass outside the shape or cross each other there, which sewed a U with direction lines
/// across both arms twice over in a test. PE-DESIGN's sewing where a cut would leave the
/// shape is not captured.
fn lying_inside(found: Vec<[Vec2; 2]>, closed: &[Contour], lines: &[[Vec2; 2]]) -> Vec<[Vec2; 2]> {
    let inside = |c: &[Vec2; 2]| {
        let steps = ((c[0].distance(c[1]) / 5.0).ceil() as usize).max(4);
        (1..steps).all(|k| contains(c[0].lerp(c[1], k as f64 / steps as f64), closed))
    };
    // Two segments crossing away from their ends (a shared end is not a crossing).
    let cross = |p: &[Vec2; 2], q: &[Vec2; 2]| {
        crossing(*p, *q)
            .is_some_and(|m| [p[0], p[1], q[0], q[1]].iter().all(|e| e.distance(m) > 0.1))
    };
    let tangled = found.iter().enumerate().any(|(i, c)| {
        found[i + 1..].iter().any(|d| cross(c, d)) || lines.iter().any(|l| cross(c, l))
    });
    if !tangled && found.iter().all(inside) {
        found
    } else {
        Vec::new()
    }
}

/// Whether vertex `i` of `ring` is a corner (the outline turns there by more than 5
/// degrees), and not just a point on a straight or gently curved run.
fn corner(ring: &Ring, i: usize) -> bool {
    let n = ring.pts.len() - 1;
    let (p, q, r) = (
        ring.pts[(i + n - 1) % n],
        ring.pts[i % n],
        ring.pts[(i + 1) % n],
    );
    let (a, b) = (q - p, r - q);
    let (la, lb) = (a.length(), b.length());
    la > 0.0 && lb > 0.0 && (a.x * b.x + a.y * b.y) / (la * lb) < 0.996
}

/// `at` (fractions with whether each is a corner's) thinned to about [`MAX_CUTS`]: every
/// corner stays, of the other points (those of a flattened curve) evenly spaced ones.
fn thinned(mut at: Vec<(f64, bool)>) -> Vec<f64> {
    at.sort_by(|p, q| p.0.total_cmp(&q.0));
    at.dedup_by(|p, q| {
        let same = (p.0 - q.0).abs() < 1e-6;
        if same {
            q.1 |= p.1;
        }
        same
    });
    let plain = at.iter().filter(|v| !v.1).count();
    if at.len() <= MAX_CUTS || plain == 0 {
        return at.into_iter().map(|v| v.0).collect();
    }
    let keep = MAX_CUTS.saturating_sub(at.len() - plain).max(1);
    let step = plain as f64 / keep as f64;
    let mut seen = 0_usize;
    let mut next = 0.0_f64;
    at.into_iter()
        .filter(|v| {
            if v.1 {
                return true;
            }
            let take = seen as f64 >= next;
            if take {
                next += step;
            }
            seen += 1;
            take
        })
        .map(|v| v.0)
        .collect()
}

/// The cuts of the part of the shape beyond chord `x`, on the side away from its neighbour
/// `other`, in order outward from `x`. That part runs as far as the outline vertex farthest
/// from `x`'s line (the outline side farthest from it, when that side is parallel to `x`),
/// along two runs: from `x`'s first end, and from its second end the other way round. The
/// vertices inside the runs cut it as [`cuts`] does a zone between two lines: from the
/// vertex to the point at the same fraction, by length, along the other run. PE-DESIGN
/// 11.50's file `g11` (#1273): beyond a line from the left side to the right side of a
/// square the rows first fan as far as the row through the nearer top corner, then run
/// parallel to that row. A corner cut off by `x` has no vertex but the corner: no cuts, and
/// rows parallel to `x`.
fn end_cuts(rings: &[Ring], closed: &[Contour], x: [Vec2; 2], other: [Vec2; 2]) -> Vec<[Vec2; 2]> {
    if ![x[0], x[1]].iter().all(|p| on_outline(rings, *p)) {
        return Vec::new();
    }
    let Some(arc) = side(rings, x[0], x[1], &[other[0], other[1]]) else {
        return Vec::new();
    };
    const AT_END: f64 = 0.1;
    // Vertices this near the same distance from the line are one far side (0.005 mm).
    const LEVEL: f64 = 0.05;
    let total = arc.ring.len();
    let d = x[1] - x[0];
    let len = d.length().max(1e-9);
    let signed = |p: Vec2| ((p.x - x[0].x) * d.y - (p.y - x[0].y) * d.x) / len;
    // (distance along the arc from `x`'s first end, distance from `x`'s line)
    let mut inside: Vec<(f64, f64, bool)> = Vec::new();
    let mut rest: Vec<f64> = Vec::new();
    for (i, &v) in arc.ring.at[..arc.ring.at.len() - 1].iter().enumerate() {
        let along = (arc.dir * (v - arc.s0)).rem_euclid(total);
        if along > AT_END && along < arc.len - AT_END {
            inside.push((along, signed(arc.ring.pts[i]), corner(arc.ring, i)));
        } else if along > arc.len + AT_END && along < total - AT_END {
            rest.push(signed(arc.ring.pts[i]));
        }
    }
    // The line must part this end of the shape from the rest of it: where the rest reaches
    // over to this side of the line as well (the other arm of a U) nothing is cut, and the
    // rows beyond the line stay parallel to it. PE-DESIGN's sewing there is not captured.
    let side = inside.iter().map(|v| v.1).sum::<f64>().signum();
    if inside.iter().any(|v| v.1 * side < -LEVEL) || rest.iter().any(|h| h * side > LEVEL) {
        return Vec::new();
    }
    for v in &mut inside {
        v.1 = v.1.abs();
    }
    inside.sort_by(|p, q| p.0.total_cmp(&q.0));
    let top = inside.iter().map(|v| v.1).fold(0.0_f64, f64::max);
    let far: Vec<f64> = inside
        .iter()
        .filter(|v| v.1 >= top - LEVEL)
        .map(|v| v.0)
        .collect();
    let (Some(&first), Some(&last)) = (far.first(), far.last()) else {
        return Vec::new();
    };
    let (run1, run2) = (first, arc.len - last);
    if run1 <= AT_END || run2 <= AT_END {
        return Vec::new();
    }
    let at: Vec<(f64, bool)> = inside
        .iter()
        .filter_map(|&(along, _, turns)| {
            if along < first - AT_END {
                Some((along / run1, turns))
            } else if along > last + AT_END {
                Some(((arc.len - along) / run2, turns))
            } else {
                None
            }
        })
        .filter(|f| f.0 < 0.999)
        .collect();
    let at = thinned(at);
    let found: Vec<[Vec2; 2]> = at
        .into_iter()
        .map(|f| {
            [
                arc.at(f * run1 / arc.len),
                arc.at((arc.len - f * run2) / arc.len),
            ]
        })
        .filter(|c| c[0].distance(c[1]) > 1.0)
        .collect();
    lying_inside(found, closed, &[x])
}

/// About the most cuts between two lines: a curved outline has a point every fraction of a
/// millimetre of its flattened path, and those are thinned to this many; corners are all
/// kept, however many.
const MAX_CUTS: usize = 96;

/// The farthest crossing of the ray from `p` along unit `u` with the outlines, as a distance
/// from `p`; `None` when the ray meets none.
fn ray_reach(p: Vec2, u: Vec2, closed: &[Contour], holes: &[Contour]) -> Option<f64> {
    let mut far = 0.0_f64;
    for c in closed.iter().chain(holes).filter(|c| c.closed) {
        for (a, b) in c.edges() {
            let e = b - a;
            let den = u.x * e.y - u.y * e.x;
            if den.abs() < 1e-12 {
                continue;
            }
            let w = a - p;
            let t = (w.x * e.y - w.y * e.x) / den;
            let s = (w.x * u.y - w.y * u.x) / den;
            if t > 1e-6 && (0.0..=1.0).contains(&s) {
                far = far.max(t);
            }
        }
    }
    (far > 0.0).then_some(far)
}

/// How far a warped outline may stray from the exact one, 0.1 mm units (0.005 mm).
const WARP_TOLERANCE: f64 = 0.05;

/// The closest the rows of a fan come to each other before some end early, as a share of
/// their spacing at the far end. PE-DESIGN 11.50 keeps every row in its two-line reference
/// design, where the rows are 0.30 of their spacing apart at the corner nearest the meeting
/// point; closer than that is not captured, so the limit is set just under it and only keeps
/// needle points from piling up where the lines meet on or just outside the outline.
const CLOSEST: f64 = 0.25;

/// Needle points of a thinned fan closer than this to the one before are left out (0.05 mm):
/// the turn of a pair of rows that are nearly on each other is one needle point.
const SAME_POINT: f64 = 0.5;

/// Steps the far-end distance of a fan is summed over.
const FAN_SAMPLES: usize = 128;

/// One zone of a [`FanWarp`]: where rows are parallel, or a fan.
#[derive(Clone, Debug)]
enum Zone {
    /// Rows parallel to a direction line. `u` runs along `along` from `origin` (plus
    /// `shift`), `v` across from `v0` at the line.
    Flat {
        origin: Vec2,
        along: Vec2,
        across: Vec2,
        shift: f64,
        v0: f64,
        /// `v - v0` of the zone's two edges (infinite beyond the first and last line).
        from: f64,
        to: f64,
    },
    /// Rows through `p`, from angle `t0` over `sweep`. `u` is the distance from `p` (times
    /// `sigma`, plus `off`); `v` is `v0` plus the far-end distance summed over the angle
    /// (`sum`, at [`FAN_SAMPLES`] steps), so equal steps of `v` are rows a spacing apart at
    /// their far ends.
    Fan {
        p: Vec2,
        t0: f64,
        sweep: f64,
        sum: Vec<f64>,
        sigma: f64,
        off: f64,
        v0: f64,
    },
}

/// PE-DESIGN's direction lines as a change of coordinates (`AngleRule::Fan`, #1273). In the
/// warped plane every row of the fill is a horizontal line. The zone between two consecutive
/// lines is cut into pieces at the outline's vertices ([`cuts`]), and so is the part of the
/// shape before the first line and beyond the last one ([`end_cuts`]), beyond whose
/// outermost cut the rows are parallel to it (to the line, where there is no cut: a corner).
/// In each piece the rows are a
/// fan through the point where the piece's two bounding lines meet when extended (parallel
/// rows when they are parallel), a row spacing apart at each row's far end from that point
/// and closer toward it. PE-DESIGN 11.50's rule, read from its files `g1` to `g11`: the rows
/// of every piece pass its meeting point within 0.05 to 0.2 mm. Distances along a row are kept. The shape is warped, the plain fill
/// is laid out on it with horizontal rows, and its needle points are brought back, so the
/// stitch pattern and the travel of the top stitching are the plain fill's. (The under
/// sewing is not laid out through the plain fill: see `region::fan_zigzag`.)
#[derive(Clone, Debug)]
pub struct FanWarp {
    zones: Vec<Zone>,
    /// A point in the middle of each zone, to tell which zone a point is in.
    anchors: Vec<Vec2>,
}

impl FanWarp {
    /// The warp for `lines` on the shape; `None` with fewer than two lines.
    pub fn new(lines: &[[Vec2; 2]], closed: &[Contour], holes: &[Contour]) -> Option<FanWarp> {
        if lines.len() < 2 {
            return None;
        }
        let mut chords: Vec<[Vec2; 2]> = lines.iter().map(|l| chord(*l, closed, holes)).collect();
        // Consecutive chords paired end to end so the rows do not twist between them.
        for i in 1..chords.len() {
            let (a, b) = (chords[i - 1], chords[i]);
            if a[0].distance(b[1]) + a[1].distance(b[0]) < a[0].distance(b[0]) + a[1].distance(b[1])
            {
                chords[i] = [b[1], b[0]];
            }
        }
        // Two neighbouring lines on one straight line have no zone between them, and none
        // on either side ([`check_fan`] refuses such lines).
        if chords.windows(2).any(|w| collinear(w[0], w[1])) {
            return None;
        }
        // The zone between two lines is cut at every outline vertex inside one of its two
        // runs ([`cuts`]); each cut bounds a piece as a line does.
        let rings: Vec<Ring> = closed.iter().filter_map(Ring::new).collect();
        // Before the first line and beyond the last one the shape is cut the same way, as
        // far as the vertex farthest from that line ([`end_cuts`]); the rows beyond the
        // outermost cut are parallel to it.
        let mut pieces: Vec<[Vec2; 2]> = end_cuts(&rings, closed, chords[0], chords[1]);
        pieces.reverse();
        pieces.push(chords[0]);
        for w in chords.windows(2) {
            pieces.extend(cuts(&rings, closed, w[0], w[1]));
            pieces.push(w[1]);
        }
        pieces.extend(end_cuts(
            &rings,
            closed,
            chords[chords.len() - 1],
            chords[chords.len() - 2],
        ));
        let chords = pieces;
        let n = chords.len();
        let mid = |c: [Vec2; 2]| c[0].lerp(c[1], 0.5);
        let dir = |c: [Vec2; 2]| {
            let d = c[1] - c[0];
            d * (1.0 / d.length().max(1e-9))
        };
        // The unit normal of chord `c` on the side of point `q`.
        let normal_toward = |c: [Vec2; 2], q: Vec2| {
            let d = dir(c);
            let nrm = v2(-d.y, d.x);
            let w = q - c[0];
            if nrm.x * w.x + nrm.y * w.y < 0.0 {
                nrm * -1.0
            } else {
                nrm
            }
        };
        let mut zones = Vec::new();
        let mut anchors = Vec::new();
        // Before the first line: v runs toward the second line, so it is negative here.
        let across0 = normal_toward(chords[0], mid(chords[1]));
        zones.push(Zone::Flat {
            origin: chords[0][0],
            along: dir(chords[0]),
            across: across0,
            shift: 0.0,
            v0: 0.0,
            from: f64::NEG_INFINITY,
            to: 0.0,
        });
        anchors.push(mid(chords[0]) - across0 * 1.0);
        let (mut shift, mut v0) = (0.0, 0.0);
        for i in 0..n - 1 {
            let (a, b) = (chords[i], chords[i + 1]);
            anchors.push(mid(a).lerp(mid(b), 0.5));
            // A meeting point inside the shape, or between the two chords (in a concave
            // outline's notch), would have the rows of the piece cross each other there:
            // such a piece has parallel rows instead.
            let between = |p: Vec2| {
                contains(p, &[Contour::new(vec![a[0], a[1], b[1], b[0]], true)])
                    || contains(p, &[Contour::new(vec![a[0], a[1], b[0], b[1]], true)])
            };
            let fan = meeting(a, b)
                .filter(|p| !contains(*p, closed) && !between(*p))
                .and_then(|p| {
                    let toward = |c: [Vec2; 2]| {
                        let d = mid(c) - p;
                        d.y.datan2(d.x)
                    };
                    let (t0, t1) = (toward(a), toward(b));
                    let sweep = (t1 - t0 + std::f64::consts::PI).rem_euclid(std::f64::consts::TAU)
                        - std::f64::consts::PI;
                    (sweep.abs() > 1e-9).then_some((p, t0, sweep))
                });
            match fan {
                Some((p, t0, sweep)) => {
                    let fallback = a[0]
                        .distance(p)
                        .max(a[1].distance(p))
                        .max(b[0].distance(p))
                        .max(b[1].distance(p));
                    let far = |t: f64| {
                        ray_reach(p, v2(t.dcos(), t.dsin()), closed, holes).unwrap_or(fallback)
                    };
                    let mut sum = vec![0.0; FAN_SAMPLES + 1];
                    let mut before = far(t0);
                    for k in 1..=FAN_SAMPLES {
                        let here = far(t0 + sweep * k as f64 / FAN_SAMPLES as f64);
                        sum[k] =
                            sum[k - 1] + (before + here) / 2.0 * sweep.abs() / FAN_SAMPLES as f64;
                        before = here;
                    }
                    // u runs the way the chords do: outward from the meeting point when
                    // the chord's first end is the nearer one.
                    let sigma = if a[0].distance(p) <= a[1].distance(p) {
                        1.0
                    } else {
                        -1.0
                    };
                    let off = shift - sigma * a[0].distance(p);
                    let total = sum[FAN_SAMPLES];
                    zones.push(Zone::Fan {
                        p,
                        t0,
                        sweep,
                        sum,
                        sigma,
                        off,
                        v0,
                    });
                    shift = sigma * b[0].distance(p) + off;
                    v0 += total;
                }
                None => {
                    // Parallel lines: straight rows from one to the other.
                    let across = normal_toward(a, mid(b));
                    let w = b[0] - a[0];
                    let width = (w.x * across.x + w.y * across.y).abs();
                    let along = dir(a);
                    zones.push(Zone::Flat {
                        origin: a[0],
                        along,
                        across,
                        shift,
                        v0,
                        from: 0.0,
                        to: width,
                    });
                    shift += w.x * along.x + w.y * along.y;
                    v0 += width;
                }
            }
        }
        // After the last line: v goes on away from the line before it.
        let last = chords[n - 1];
        let across = normal_toward(last, mid(chords[n - 2])) * -1.0;
        zones.push(Zone::Flat {
            origin: last[0],
            along: dir(last),
            across,
            shift,
            v0,
            from: 0.0,
            to: f64::INFINITY,
        });
        anchors.push(mid(last) + across * 1.0);
        Some(FanWarp { zones, anchors })
    }

    /// Whether `q` lies in zone `z`.
    fn holds(z: &Zone, q: Vec2) -> bool {
        match z {
            Zone::Flat {
                origin,
                across,
                from,
                to,
                ..
            } => {
                let w = q - *origin;
                let d = w.x * across.x + w.y * across.y;
                (*from..=*to).contains(&d)
            }
            Zone::Fan { p, t0, sweep, .. } => {
                let d = q - *p;
                let rel = (d.y.datan2(d.x) - t0 + std::f64::consts::PI)
                    .rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI;
                if *sweep >= 0.0 {
                    (0.0..=*sweep).contains(&rel)
                } else {
                    (*sweep..=0.0).contains(&rel)
                }
            }
        }
    }

    /// `q` in the warped plane: (along the row, across the rows).
    pub fn forward(&self, q: Vec2) -> Vec2 {
        // The zone that holds the point; of several (wedges reach far), the nearest one.
        let near = |k: &usize| self.anchors[*k].distance(q);
        let k = (0..self.zones.len())
            .filter(|&k| Self::holds(&self.zones[k], q))
            .min_by(|a, b| near(a).total_cmp(&near(b)))
            .or_else(|| (0..self.zones.len()).min_by(|a, b| near(a).total_cmp(&near(b))))
            .unwrap_or(0);
        match &self.zones[k] {
            Zone::Flat {
                origin,
                along,
                across,
                shift,
                v0,
                ..
            } => {
                let w = q - *origin;
                v2(
                    w.x * along.x + w.y * along.y + shift,
                    v0 + w.x * across.x + w.y * across.y,
                )
            }
            Zone::Fan {
                p,
                t0,
                sweep,
                sum,
                sigma,
                off,
                v0,
            } => {
                let d = q - *p;
                let rel = (d.y.datan2(d.x) - t0 + std::f64::consts::PI)
                    .rem_euclid(std::f64::consts::TAU)
                    - std::f64::consts::PI;
                let f = (rel / sweep).clamp(0.0, 1.0) * FAN_SAMPLES as f64;
                let i = (f.floor() as usize).min(FAN_SAMPLES - 1);
                let g = sum[i] + (sum[i + 1] - sum[i]) * (f - i as f64);
                v2(sigma * d.length() + off, v0 + g)
            }
        }
    }

    /// The point of the design at `w` of the warped plane.
    pub fn back(&self, w: Vec2) -> Vec2 {
        // Zones are in order of v; the first whose range holds it (the last one otherwise).
        let end = |z: &Zone| match z {
            Zone::Flat { v0, to, .. } => v0 + to,
            Zone::Fan { v0, sum, .. } => v0 + sum[FAN_SAMPLES],
        };
        let k = self
            .zones
            .iter()
            .position(|z| w.y <= end(z))
            .unwrap_or(self.zones.len() - 1);
        match &self.zones[k] {
            Zone::Flat {
                origin,
                along,
                across,
                shift,
                v0,
                ..
            } => *origin + *along * (w.x - shift) + *across * (w.y - v0),
            Zone::Fan {
                p,
                t0,
                sweep,
                sum,
                sigma,
                off,
                v0,
            } => {
                let g = (w.y - v0).clamp(0.0, sum[FAN_SAMPLES]);
                let i = sum.partition_point(|&x| x < g).clamp(1, FAN_SAMPLES);
                let span = sum[i] - sum[i - 1];
                let f = if span > 0.0 {
                    (g - sum[i - 1]) / span
                } else {
                    0.0
                };
                let t = t0 + sweep * (i as f64 - 1.0 + f) / FAN_SAMPLES as f64;
                *p + v2(t.dcos(), t.dsin()) * (sigma * (w.x - off))
            }
        }
    }

    /// How close the rows are at warped point `w`, as a share of their spacing: 1 where they
    /// are parallel; in a fan the distance from the meeting point over the far end's.
    fn closeness(&self, w: Vec2) -> f64 {
        let end = |z: &Zone| match z {
            Zone::Flat { v0, to, .. } => v0 + to,
            Zone::Fan { v0, sum, .. } => v0 + sum[FAN_SAMPLES],
        };
        let k = self
            .zones
            .iter()
            .position(|z| w.y <= end(z))
            .unwrap_or(self.zones.len() - 1);
        match &self.zones[k] {
            Zone::Flat { .. } => 1.0,
            Zone::Fan {
                sweep,
                sum,
                sigma,
                off,
                v0,
                ..
            } => {
                let g = (w.y - v0).clamp(0.0, sum[FAN_SAMPLES]);
                let i = sum.partition_point(|&x| x < g).clamp(1, FAN_SAMPLES);
                let far = (sum[i] - sum[i - 1]) / (sweep.abs() / FAN_SAMPLES as f64);
                let r = (sigma * (w.x - off)).abs();
                if far > 0.0 { (r / far).min(1.0) } else { 1.0 }
            }
        }
    }

    /// A run of needle points of the warped plane with the rows of a fan thinned where they
    /// come closer than [`CLOSEST`] of their spacing: of the trips the run makes into that
    /// part (a row in, the turn at the outline, the next row out) every other one is left
    /// out, so those two rows end early; again at half that closeness, and so on. Needle
    /// points left within [`SAME_POINT`] of the one before, in that part, are dropped. A run
    /// that never comes that close is returned as it is.
    pub fn thinned(&self, run: Vec<Vec2>) -> Vec<Vec2> {
        if !run.iter().any(|&w| self.closeness(w) < CLOSEST) {
            return run;
        }
        let mut run = run;
        let mut limit = CLOSEST;
        for _ in 0..8 {
            let mut out = Vec::with_capacity(run.len());
            let (mut i, mut skip, mut trips) = (0, false, 0);
            while i < run.len() {
                if self.closeness(run[i]) >= limit {
                    out.push(run[i]);
                    i += 1;
                    continue;
                }
                let start = i;
                while i < run.len() && self.closeness(run[i]) < limit {
                    i += 1;
                }
                // A run that starts or ends in there keeps that part.
                let open = start == 0 || i == run.len();
                if open || !skip {
                    out.extend_from_slice(&run[start..i]);
                }
                if !open {
                    skip = !skip;
                    trips += 1;
                }
            }
            run = out;
            if trips == 0 {
                break;
            }
            limit /= 2.0;
        }
        let mut out: Vec<Vec2> = Vec::with_capacity(run.len());
        for w in run {
            let same = out.last().is_some_and(|l| {
                self.closeness(w) < CLOSEST && self.back(*l).distance(self.back(w)) < SAME_POINT
            });
            if !same {
                out.push(w);
            }
        }
        out
    }

    /// Whether the shape folds over itself in the warped plane: its warped outlines cross
    /// themselves or each other. On a concave shape (a U with lines in both arms) the zones
    /// of the warp can overlap, and rows laid out on a folded outline would cross each other
    /// when brought back; such a shape is not sewn through the warp (see `region`).
    pub fn folds(&self, closed: &[Contour], holes: &[Contour]) -> bool {
        let edges: Vec<(Vec2, Vec2)> = closed
            .iter()
            .chain(holes)
            .map(|c| self.contour(c))
            .flat_map(|c| c.edges())
            .collect();
        crosses_itself(&edges)
    }

    /// How many pieces the shape is cut into: the zones between lines, before the first and
    /// beyond the last, and the pieces their cuts make. Each piece that fans casts
    /// [`FAN_SAMPLES`] rays at the outline once, and every needle point is placed by looking
    /// through the pieces, so this is what a fill with direction lines costs.
    pub fn pieces(&self) -> usize {
        self.zones.len()
    }

    /// `c` in the warped plane, as a polygon: its outline is followed in steps of at most
    /// 0.3 mm, since a straight side is a curve there.
    pub fn contour(&self, c: &Contour) -> Contour {
        const STEP: f64 = 3.0;
        let path = c.path();
        let mut out: Vec<Vec2> = Vec::new();
        for w in path.windows(2) {
            let n = ((w[0].distance(w[1]) / STEP).ceil() as usize).max(1);
            for k in 0..n {
                out.push(self.forward(w[0].lerp(w[1], k as f64 / n as f64)));
            }
        }
        if !c.closed
            && let Some(&last) = path.last()
        {
            out.push(self.forward(last));
        }
        // Points that add nothing are dropped again (a straight side stays one side where
        // the rows are parallel): the plain fill puts needle points at outline corners, so
        // needless ones would add stitches.
        crate::simplify::contour(&Contour::new(out, c.closed), WARP_TOLERANCE)
    }
}

/// Whether any two of `edges` cross away from their ends. The edges are put into a grid of
/// about one cell per edge, and only edges that share a cell are compared, so a shape of
/// tens of thousands of edges (a 200 mm outline followed in 0.3 mm steps) is checked in
/// about as many steps, not their square.
fn crosses_itself(edges: &[(Vec2, Vec2)]) -> bool {
    let n = edges.len();
    if n < 4 {
        return false;
    }
    let (mut lo, mut hi) = (
        v2(f64::INFINITY, f64::INFINITY),
        v2(f64::NEG_INFINITY, f64::NEG_INFINITY),
    );
    for (a, b) in edges {
        for p in [a, b] {
            lo = v2(lo.x.min(p.x), lo.y.min(p.y));
            hi = v2(hi.x.max(p.x), hi.y.max(p.y));
        }
    }
    let side = (n as f64).sqrt().ceil().max(1.0) as usize;
    let (w, h) = ((hi.x - lo.x).max(1e-9), (hi.y - lo.y).max(1e-9));
    let cell = |p: Vec2| {
        (
            (((p.x - lo.x) / w * side as f64) as usize).min(side - 1),
            (((p.y - lo.y) / h * side as f64) as usize).min(side - 1),
        )
    };
    let mut grid: Vec<Vec<usize>> = vec![Vec::new(); side * side];
    for (i, (a, b)) in edges.iter().enumerate() {
        let (c0, c1) = (
            cell(v2(a.x.min(b.x), a.y.min(b.y))),
            cell(v2(a.x.max(b.x), a.y.max(b.y))),
        );
        for gx in c0.0..=c1.0 {
            for gy in c0.1..=c1.1 {
                grid[gy * side + gx].push(i);
            }
        }
    }
    for bucket in &grid {
        for (k, &i) in bucket.iter().enumerate() {
            for &j in &bucket[k + 1..] {
                let (i, j) = (i.min(j), i.max(j));
                // Neighbours share an end; the first and last edge of a ring do too.
                if j == i + 1 || (i == 0 && j == n - 1) {
                    continue;
                }
                let (a, b) = (edges[i], edges[j]);
                if let Some(m) = crossing([a.0, a.1], [b.0, b.1])
                    && [a.0, a.1, b.0, b.1].iter().all(|e| e.distance(m) > 0.01)
                {
                    return true;
                }
            }
        }
    }
    false
}

/// How far outside the shape's bounds a direction line is drawn and stored when it is made
/// from the fill angle: 5 mm, as PE-DESIGN draws its ends.
pub const LINE_REACH: f64 = 50.0;

/// The one straight direction line of a shape with fill angle `angle` (stored degrees,
/// counter-clockwise on screen): through the middle of the bounds of `closed`, from
/// [`LINE_REACH`] before the bounds to as far behind them, pointing the way the angle does.
/// Where the middle of the bounds is not in the shape (a C, a ring) the line goes through a
/// point that is: the middle of the widest part of the shape level with the middle of the
/// bounds, from [`LINE_REACH`] before the outline there to as far behind it, so that the
/// line is one [`check_fan`] accepts. `None` for a shape without a closed outline.
pub fn angle_line(closed: &[Contour], angle: f64) -> Option<[Vec2; 2]> {
    let b = crate::geometry::Rect::enclosing(
        closed
            .iter()
            .filter(|c| c.closed)
            .flat_map(Contour::outline_points),
    )?;
    let centre = v2((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0);
    let d = v2(angle.to_radians().dcos(), -angle.to_radians().dsin());
    if !contains(centre, closed) {
        // Level with the middle of the bounds: where the outline is crossed, in order.
        let mut xs: Vec<f64> = closed
            .iter()
            .filter(|c| c.closed)
            .flat_map(Contour::edges)
            .filter(|(p, q)| (p.y <= centre.y) != (q.y <= centre.y))
            .map(|(p, q)| p.x + (q.x - p.x) * (centre.y - p.y) / (q.y - p.y))
            .collect();
        xs.sort_by(f64::total_cmp);
        let inside = xs
            .as_chunks::<2>()
            .0
            .iter()
            .max_by(|p, q| (p[1] - p[0]).total_cmp(&(q[1] - q[0])))
            .map(|p| v2((p[0] + p[1]) / 2.0, centre.y))?;
        let c = chord([inside - d, inside + d], closed, &[]);
        return Some([c[0] - d * LINE_REACH, c[1] + d * LINE_REACH]);
    }
    let reach = |half: f64, along: f64| {
        if along.abs() < 1e-9 {
            f64::INFINITY
        } else {
            half / along.abs()
        }
    };
    let half = reach((b.max_x - b.min_x) / 2.0, d.x).min(reach((b.max_y - b.min_y) / 2.0, d.y))
        + LINE_REACH;
    Some([centre - d * half, centre + d * half])
}

/// A direction line drawn from `l[0]` to `l[1]` as it is stored: along the same line, from
/// [`LINE_REACH`] before its first crossing of the outline to as far after its last one, so
/// that it lies across the shape wherever its ends were put. `None` when it does not cross
/// the shape (fewer than two crossings).
pub fn across(l: [Vec2; 2], closed: &[Contour]) -> Option<[Vec2; 2]> {
    let d = l[1] - l[0];
    let len = d.length();
    if len <= 0.0 {
        return None;
    }
    let u = d * (1.0 / len);
    let (mut first, mut last) = (f64::INFINITY, f64::NEG_INFINITY);
    let mut crossings = 0;
    for c in closed.iter().filter(|c| c.closed) {
        for (a, b) in c.edges() {
            let e = b - a;
            let den = u.x * e.y - u.y * e.x;
            if den.abs() < 1e-12 {
                continue;
            }
            let w = a - l[0];
            let t = (w.x * e.y - w.y * e.x) / den;
            let s = (w.x * u.y - w.y * u.x) / den;
            if (0.0..=len).contains(&t) && (0.0..=1.0).contains(&s) {
                first = first.min(t);
                last = last.max(t);
                crossings += 1;
            }
        }
    }
    (crossings >= 2 && last - first > 1.0).then(|| {
        [
            l[0] + u * (first - LINE_REACH),
            l[0] + u * (last + LINE_REACH),
        ]
    })
}

/// The stored angle (degrees, counter-clockwise on screen, 0 to 360) a direction line points
/// at, from its first end to its second.
pub fn line_angle(l: [Vec2; 2]) -> f64 {
    let d = l[1] - l[0];
    let a = (-d.y).datan2(d.x).to_degrees().rem_euclid(360.0);
    ((a * 10.0).round() / 10.0).rem_euclid(360.0)
}

/// Where a new direction line goes among `lines`, which are in order across the shape. A
/// second line goes after the first (as in PE-DESIGN's files). From then on the order is the
/// one the lines have going round the outline: a line that does not cross the others has both
/// its ends on one side of each of them, and it goes after every line that has it on the
/// side its next line is on (for the last line: the side its line before is not on). So a
/// line between two others goes between them, wherever its middle is (PE-DESIGN's three-line
/// file lists its lines this way round the outline). Only for lines whose ends are not all on
/// one outline: the place that keeps the lines' middles nearest to each other in turn.
pub fn place_for(lines: &[[Vec2; 2]], new: [Vec2; 2], closed: &[Contour]) -> usize {
    let n = lines.len();
    if n <= 1 {
        return n;
    }
    let chords: Vec<[Vec2; 2]> = lines
        .iter()
        .chain([&new])
        .map(|l| chord(*l, closed, &[]))
        .collect();
    if let Some(arcs) = arcs(&chords, closed) {
        let at = arcs[n].a;
        return (0..n)
            .filter(|&i| {
                if i + 1 < n {
                    arcs[i].holds(arcs[i + 1].a) == arcs[i].holds(at)
                } else {
                    arcs[i].holds(arcs[i - 1].a) != arcs[i].holds(at)
                }
            })
            .count();
    }
    let mid = |l: &[Vec2; 2]| l[0].lerp(l[1], 0.5);
    let length = |at: usize| {
        let mut all: Vec<Vec2> = lines.iter().map(mid).collect();
        all.insert(at, mid(&new));
        all.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>()
    };
    (0..=lines.len())
        .rev()
        .min_by(|a, b| length(*a).total_cmp(&length(*b)))
        .unwrap_or(lines.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polygon(points: &[(f64, f64)]) -> Contour {
        Contour::new(points.iter().map(|&(x, y)| v2(x, y)).collect(), true)
    }

    /// A corner inside a curved run always cuts the zone, however many points the curve
    /// has: only the curve's own points are thinned.
    #[test]
    fn a_corner_inside_a_curved_run_always_cuts() {
        // A flat bottom, two straight sides, and a top of two arcs of 300 points each that
        // meet in a peak at (500, 700), a corner; every other top point turns the outline
        // by a fraction of a degree.
        let mut pts = vec![(0.0, 0.0), (1000.0, 0.0), (1000.0, 400.0)];
        for k in 1..300 {
            let t = f64::from(k) / 300.0;
            pts.push((1000.0 - 500.0 * t, 400.0 + 300.0 * t * t));
        }
        pts.push((500.0, 700.0));
        for k in 1..300 {
            let t = 1.0 - f64::from(k) / 300.0;
            pts.push((500.0 * t, 400.0 + 300.0 * t * t));
        }
        pts.push((0.0, 400.0));
        let closed = [polygon(&pts)];
        let rings: Vec<Ring> = closed.iter().filter_map(Ring::new).collect();
        let a = chord([v2(100.0, -50.0), v2(100.0, 800.0)], &closed, &[]);
        let b = chord([v2(900.0, -50.0), v2(900.0, 800.0)], &closed, &[]);
        let found = cuts(&rings, &closed, a, b);
        assert!(
            found.len() <= MAX_CUTS + 1,
            "{} cuts for about 480 points between the lines",
            found.len()
        );
        assert!(found.len() > MAX_CUTS / 2, "{}", found.len());
        assert!(
            found
                .iter()
                .any(|c| c.iter().any(|p| p.distance(v2(500.0, 700.0)) < 1e-6)),
            "the peak cuts the zone"
        );
    }

    /// Where one cut would leave the shape none is made: the zone stays one piece (see
    /// [`lying_inside`] for why not only that cut is dropped). The same zone without the
    /// notch is cut at the corners as usual.
    #[test]
    fn a_cut_that_leaves_the_shape_keeps_the_zone_in_one_piece() {
        // A rectangle with a notch from its top that leans right: its tip at (700, 150)
        // lies under the top side.
        let closed = [polygon(&[
            (0.0, 0.0),
            (1000.0, 0.0),
            (1000.0, 500.0),
            (600.0, 500.0),
            (700.0, 150.0),
            (400.0, 500.0),
            (0.0, 500.0),
        ])];
        let rings: Vec<Ring> = closed.iter().filter_map(Ring::new).collect();
        let a = chord([v2(100.0, -50.0), v2(100.0, 550.0)], &closed, &[]);
        let b = chord([v2(900.0, -50.0), v2(900.0, 550.0)], &closed, &[]);
        // The cut from the notch's right corner would pass through the notch.
        assert!(cuts(&rings, &closed, a, b).is_empty());
        // A plain rectangle with a point inserted on its top side: cut there.
        let plain = [polygon(&[
            (0.0, 0.0),
            (1000.0, 0.0),
            (1000.0, 500.0),
            (500.0, 500.0),
            (0.0, 500.0),
        ])];
        let rings: Vec<Ring> = plain.iter().filter_map(Ring::new).collect();
        let a = chord([v2(100.0, -50.0), v2(100.0, 550.0)], &plain, &[]);
        let b = chord([v2(900.0, -50.0), v2(900.0, 550.0)], &plain, &[]);
        let found = cuts(&rings, &plain, a, b);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].iter().any(|p| p.distance(v2(500.0, 500.0)) < 1e-6));
    }

    /// A piece whose two bounding lines meet inside the outline has parallel rows, not a fan
    /// through that point: the rows of a fan would cross each other there.
    #[test]
    fn a_meeting_point_inside_the_outline_gives_no_fan() {
        // A square with a square hole: one line across its left band and one across its
        // right band, leaning toward each other; extended they meet in the middle.
        let closed = [polygon(&[
            (0.0, 0.0),
            (1000.0, 0.0),
            (1000.0, 1000.0),
            (0.0, 1000.0),
        ])];
        let holes = [polygon(&[
            (300.0, 300.0),
            (700.0, 300.0),
            (700.0, 700.0),
            (300.0, 700.0),
        ])];
        let lines = [
            [v2(-50.0, 440.0), v2(350.0, 480.0)],
            [v2(1050.0, 440.0), v2(650.0, 480.0)],
        ];
        let p = meeting(
            chord(lines[0], &closed, &holes),
            chord(lines[1], &closed, &holes),
        )
        .expect("the lines meet");
        assert!(contains(p, &closed), "{p:?}");
        let warp = FanWarp::new(&lines, &closed, &holes).expect("warp");
        assert!(
            warp.zones.iter().all(|z| matches!(z, Zone::Flat { .. })),
            "no fan through a point inside the outline"
        );
        // Lines that meet outside the shape (the two-line reference's): a fan.
        let square = [polygon(&[
            (300.0, 300.0),
            (700.0, 300.0),
            (700.0, 700.0),
            (300.0, 700.0),
        ])];
        let outside = [
            [v2(250.0, 750.0), v2(750.0, 250.0)],
            [v2(252.0, 384.0), v2(501.7, 280.0)],
        ];
        let warp = FanWarp::new(&outside, &square, &[]).expect("warp");
        assert!(warp.zones.iter().any(|z| matches!(z, Zone::Fan { .. })));
    }

    /// The cuts are kept only together: none when two of them cross, or one crosses one of
    /// the zone's lines, though each lies inside the shape.
    #[test]
    fn crossing_cuts_keep_the_zone_whole() {
        let square = [polygon(&[
            (0.0, 0.0),
            (1000.0, 0.0),
            (1000.0, 1000.0),
            (0.0, 1000.0),
        ])];
        let lines = [
            [v2(100.0, 0.0), v2(100.0, 1000.0)],
            [v2(900.0, 0.0), v2(900.0, 1000.0)],
        ];
        let apart = vec![
            [v2(300.0, 0.0), v2(300.0, 1000.0)],
            [v2(600.0, 0.0), v2(600.0, 1000.0)],
        ];
        assert_eq!(lying_inside(apart.clone(), &square, &lines), apart);
        // Two cuts that cross each other.
        let crossed = vec![
            [v2(300.0, 0.0), v2(600.0, 1000.0)],
            [v2(600.0, 0.0), v2(300.0, 1000.0)],
        ];
        assert!(lying_inside(crossed, &square, &lines).is_empty());
        // A cut that crosses one of the zone's lines.
        let over = vec![
            [v2(300.0, 0.0), v2(300.0, 1000.0)],
            [v2(50.0, 0.0), v2(600.0, 1000.0)],
        ];
        assert!(lying_inside(over, &square, &lines).is_empty());
    }

    /// No piece fans through a point between its two chords that is outside the shape (in
    /// a U's notch): the rows of that fan would cross there.
    #[test]
    fn a_meeting_point_in_a_notch_gives_no_fan() {
        // A U: two arms of 200 units, the notch 400 wide; a line across each arm, the two
        // leaning toward each other so that they meet in the notch.
        let u = [polygon(&[
            (100.0, 100.0),
            (300.0, 100.0),
            (300.0, 700.0),
            (700.0, 700.0),
            (700.0, 100.0),
            (900.0, 100.0),
            (900.0, 900.0),
            (100.0, 900.0),
        ])];
        let lines = [
            [v2(50.0, 450.0), v2(350.0, 400.0)],
            [v2(950.0, 450.0), v2(650.0, 400.0)],
        ];
        let p = meeting(chord(lines[0], &u, &[]), chord(lines[1], &u, &[])).expect("meet");
        assert!(!contains(p, &u) && (300.0..700.0).contains(&p.x), "{p:?}");
        if let Some(warp) = FanWarp::new(&lines, &u, &[]) {
            for z in &warp.zones {
                if let Zone::Fan { p: q, .. } = z {
                    assert!(q.distance(p) > 1.0, "a fan through the notch point {q:?}");
                }
            }
        }
    }

    /// The fold check of a large shape is about as many steps as edges: a 200 mm circle of
    /// 2000 points followed in 0.3 mm steps (about 21,000 edges) in well under a second, in
    /// a debug build too.
    #[test]
    fn the_fold_check_of_a_large_shape_is_quick() {
        let circle: Vec<(f64, f64)> = (0..2000)
            .map(|k| {
                let a = std::f64::consts::TAU * f64::from(k) / 2000.0;
                (1000.0 + 1000.0 * a.cos(), 1000.0 + 1000.0 * a.sin())
            })
            .collect();
        let closed = [polygon(&circle)];
        let lines = [
            [v2(100.0, 300.0), v2(600.0, -100.0)],
            [v2(1400.0, 2100.0), v2(2100.0, 1400.0)],
        ];
        let warp = FanWarp::new(&lines, &closed, &[]).expect("warp");
        let started = std::time::Instant::now();
        assert!(!warp.folds(&closed, &[]));
        let took = started.elapsed().as_secs_f64();
        assert!(took < 2.0, "{took:.2} s");
        // And a ring that does cross itself is found.
        let bow = [polygon(&[
            (0.0, 0.0),
            (100.0, 100.0),
            (100.0, 0.0),
            (0.0, 100.0),
        ])];
        let edges: Vec<(Vec2, Vec2)> = bow[0].edges();
        assert!(crosses_itself(&edges));
    }
}
