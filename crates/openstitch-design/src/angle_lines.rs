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
/// warped plane every row of the fill is a horizontal line: before the first direction line
/// and after the last one the rows are parallel to it, and between two consecutive lines they
/// are a fan through the point where the two lines meet when extended, a row spacing apart at
/// each row's far end from that point and closer toward it (PE-DESIGN 11.50's rule, measured
/// on its files: 0.19 degrees a row where the far end is 64 mm away, 0.16 where it is 79 mm,
/// at 0.212 mm spacing). Distances along a row are kept. The shape is warped, the plain fill
/// is laid out on it with horizontal rows, and its needle points are brought back, so the
/// under sewing, the stitch pattern and the travel are the plain fill's.
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
            let fan = meeting(a, b).and_then(|p| {
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

/// How far outside the shape's bounds a direction line is drawn and stored when it is made
/// from the fill angle: 5 mm, as PE-DESIGN draws its ends.
pub const LINE_REACH: f64 = 50.0;

/// The one straight direction line of a shape with fill angle `angle` (stored degrees,
/// counter-clockwise on screen): through the middle of the bounds of `closed`, from
/// [`LINE_REACH`] before the bounds to as far behind them, pointing the way the angle does.
/// `None` for a shape without a closed outline.
pub fn angle_line(closed: &[Contour], angle: f64) -> Option<[Vec2; 2]> {
    let b = crate::geometry::Rect::enclosing(
        closed
            .iter()
            .filter(|c| c.closed)
            .flat_map(Contour::outline_points),
    )?;
    let centre = v2((b.min_x + b.max_x) / 2.0, (b.min_y + b.max_y) / 2.0);
    let d = v2(angle.to_radians().dcos(), -angle.to_radians().dsin());
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

/// Where a new direction line goes among `lines` (which are in order across the shape): the
/// place that keeps the lines' middles nearest to each other in turn; after the others when
/// places are equal (a second line goes after the first, as in PE-DESIGN's files).
pub fn place_for(lines: &[[Vec2; 2]], new: [Vec2; 2]) -> usize {
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
