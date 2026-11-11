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
