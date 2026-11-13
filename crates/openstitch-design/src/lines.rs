//! Line sew types beyond running, triple and zigzag: motif, stem, candlewicking, E/V, chain,
//! cutting lines, and zigzag with half stitch. Independent algorithms specified from the
//! public guide's attribute lists; lengths are 0.1 mm and every generator is deterministic.

use crate::fmath::Det;
use crate::generate::{Run, subdivide};
use crate::geometry::{Contour, Vec2, v2};
use crate::model::{Arrange, ChainShape};
use crate::paths::{Walker, normal};
use crate::patterns::Pattern;

/// Side sign of element `k` under `arrange`.
fn side(arrange: Arrange, k: usize) -> f64 {
    match arrange {
        Arrange::Normal => 1.0,
        Arrange::Mirror => -1.0,
        Arrange::Alternate => {
            if k.is_multiple_of(2) {
                1.0
            } else {
                -1.0
            }
        }
    }
}

/// Appends `pts` to `run`, skipping a first point equal to the run's last.
fn extend(run: &mut Run, pts: impl IntoIterator<Item = Vec2>) {
    for p in pts {
        if run.last().is_none_or(|l| l.distance(p) > 1e-9) {
            run.push(p);
        }
    }
}

/// The side direction at arc length `s`: the normal there, or at a corner the bisector of the
/// two segments' normals, so a stroke at a corner points straight out of it (PE-DESIGN's E and
/// V at the corner of line-e-default.pes / line-v-default.pes).
fn side_at(w: &Walker, s: f64) -> Vec2 {
    const EPS: f64 = 1e-3;
    let (a, b) = (
        normal(w.tangent((s - EPS).max(0.0))),
        normal(w.tangent((s + EPS).min(w.length()))),
    );
    let sum = a + b;
    let len = (sum.x * sum.x + sum.y * sum.y).sqrt();
    if len < 1e-9 { a } else { sum * (1.0 / len) }
}

/// Indices into `c.path()` of the contour's own points (its nodes), for the stem, candlewick and
/// chain lines, which place their elements leg by leg, a leg running from one node to the next.
/// Empty for a contour without curves: there every vertex is a node.
pub fn nodes_of(c: &Contour) -> Vec<usize> {
    if !c.has_curves() {
        return Vec::new();
    }
    let mut nodes = vec![0];
    for i in 0..c.segment_count() {
        let last = *nodes.last().expect("node");
        nodes.push(last + c.segment_polyline(i).len() - 1);
    }
    nodes
}

/// Half the stretch of a curved leg over which its heading is taken (0.1 mm): the chords of a
/// flattened curve turn a little at every vertex, so an element faces the mean direction of the
/// curve around it instead of the chord it happens to sit on.
const HEADING_SPAN: f64 = 5.0;

/// One leg of a line: the path from one node to the next, `len` long, starting at arc `a`.
/// A straight leg (a single segment from `p` to `q`) is measured on that segment itself; a
/// curved one (a flattened Bézier between two nodes) by arc length along it, so elements are
/// spaced evenly along the curve whatever its flattening (#464).
struct Leg {
    a: f64,
    len: f64,
    p: Vec2,
    q: Vec2,
    straight: bool,
}

impl Leg {
    /// The point `d` along the leg.
    fn at(&self, w: &Walker, d: f64) -> Vec2 {
        if self.straight {
            self.p + self.dir(w, d) * d
        } else {
            w.at(self.a + d)
        }
    }

    /// The direction of the leg at `d` along it.
    fn dir(&self, w: &Walker, d: f64) -> Vec2 {
        if self.straight {
            return (self.q - self.p) * (1.0 / self.len);
        }
        let s0 = (d - HEADING_SPAN).max(0.0);
        let s1 = (d + HEADING_SPAN).min(self.len);
        let u = w.at(self.a + s1) - w.at(self.a + s0);
        let l = u.x.dhypot(u.y);
        if l > 1e-9 {
            u * (1.0 / l)
        } else {
            w.tangent(self.a + d)
        }
    }
}

/// The arc length along `path` of each of its vertices.
fn arcs(path: &[Vec2]) -> Vec<f64> {
    let mut cum = vec![0.0];
    for q in path.windows(2) {
        let last = *cum.last().expect("arc");
        cum.push(last + q[0].distance(q[1]));
    }
    cum
}

/// The legs of `path` between its `nodes` (see [`nodes_of`]; empty: every vertex), zero-length
/// ones dropped.
fn legs(path: &[Vec2], nodes: &[usize]) -> Vec<Leg> {
    let cum = arcs(path);
    let every: Vec<usize> = (0..path.len()).collect();
    let nodes = if nodes.is_empty() { &every } else { nodes };
    nodes
        .windows(2)
        .filter(|k| k[1] < path.len() && cum[k[1]] - cum[k[0]] > 1e-9)
        .map(|k| {
            let straight = k[1] == k[0] + 1;
            Leg {
                a: cum[k[0]],
                len: if straight {
                    path[k[0]].distance(path[k[1]])
                } else {
                    cum[k[1]] - cum[k[0]]
                },
                p: path[k[0]],
                q: path[k[1]],
                straight,
            }
        })
        .collect()
}

/// The arc lengths along `path` of its `nodes` (empty: every vertex).
fn node_arcs(path: &[Vec2], nodes: &[usize]) -> Vec<f64> {
    let cum = arcs(path);
    if nodes.is_empty() {
        cum
    } else {
        nodes.iter().filter_map(|&k| cum.get(k).copied()).collect()
    }
}

/// Running stitches along the path between arc lengths `a` and `b` (reversed when `b < a`).
fn run_between(w: &Walker, a: f64, b: f64, pitch: f64) -> Run {
    if a <= b {
        subdivide(&w.slice(a, b), pitch)
    } else {
        let mut r = subdivide(&w.slice(b, a), pitch);
        r.reverse();
        r
    }
}

pub struct MotifLine<'a> {
    pub pattern: &'a Pattern,
    pub width: f64,
    pub height: f64,
    pub gap: f64,
    pub pitch: f64,
    pub arrange: Arrange,
}

/// Motif line: copies of a library motif along the path, joined by running stitches over the
/// gaps. The copies (and gaps) are stretched along the path to fit a whole number of them
/// from end to end, on open and closed paths alike.
pub fn motif(path: &[Vec2], closed: bool, m: &MotifLine) -> Run {
    let w = Walker::new(path);
    let len = w.length();
    let unit = &m.pattern.paths[0];
    let period = m.width + m.gap;
    if len <= 0.0 || period <= 0.0 || m.width > len {
        return subdivide(path, m.pitch);
    }
    // A whole number of motifs, scaled along the line so they fill it from end to end
    // (PE-DESIGN 11.50 sews 8 motifs of 4.94 mm on a 39.5 mm line at the 5 mm size:
    // compat/reference/stitches/line-motif-zigzag-3pt.pes).
    let (n, scale) = if closed {
        let n = (len / period).round().max(1.0);
        (n as usize, len / (n * period))
    } else {
        let n = ((len + m.gap) / period).round().max(1.0);
        (n as usize, (len + m.gap) / (n * period))
    };
    let (mw, mg) = (m.width * scale, m.gap * scale);
    let used = n as f64 * mw + (n as f64 - 1.0) * mg;
    let mut s = if closed { 0.0 } else { (len - used) / 2.0 };
    let mut run = run_between(&w, 0.0, s, m.pitch);
    for k in 0..n {
        let sign = side(m.arrange, k);
        let mapped: Vec<Vec2> = unit
            .iter()
            .map(|q| {
                let at = s + q.x * mw;
                w.at(at) + normal(w.tangent(at)) * (q.y * m.height * sign)
            })
            .collect();
        extend(&mut run, subdivide(&mapped, m.pitch));
        s += mw;
        if k + 1 < n {
            extend(&mut run, run_between(&w, s, s + mg, m.pitch));
            s += mg;
        }
    }
    extend(&mut run, run_between(&w, s, len, m.pitch));
    // A motif's own vertices can lie closer than a stitch should be (a wave's crests, the
    // turns of a small motif): drop those, keeping the line's ends (#617).
    crate::generate::drop_close(&run, crate::generate::MOTIF_MIN_STITCH, m.pitch)
}

/// What a motif line places beyond PE-DESIGN (#544): groups of motifs with longer gaps
/// between groups, motif-free ends, and a list of motifs used in turn.
pub struct MotifPlan<'a> {
    /// (motifs in the group, gap after it, 0.1 mm), repeated along the line.
    pub groups: &'a [(u32, f64)],
    /// Running stitch only, no motif, for this long at the start and at the end (0.1 mm).
    pub free_start: f64,
    pub free_end: f64,
    /// The motifs used in turn (the first is the line's motif).
    pub patterns: Vec<&'a Pattern>,
}

impl MotifPlan<'_> {
    /// Nothing beyond PE-DESIGN's one motif evenly along the line.
    pub fn is_plain(&self) -> bool {
        self.groups.is_empty()
            && self.free_start <= 0.0
            && self.free_end <= 0.0
            && self.patterns.len() <= 1
    }
}

/// [`motif`] with a [`MotifPlan`]: from `free_start` on, motifs of `m.width` with `m.gap`
/// between them, in groups whose own gaps follow; plain running stitch over every gap and the
/// free ends. Unlike PE-DESIGN's motif line, nothing is stretched: what does not fit before
/// `free_end` stays running stitch.
pub fn motif_planned(path: &[Vec2], m: &MotifLine, plan: &MotifPlan) -> Run {
    if plan.is_plain() {
        return motif(path, false, m);
    }
    let w = Walker::new(path);
    let len = w.length();
    let end = (len - plan.free_end.max(0.0)).max(0.0);
    let mut s = plan.free_start.clamp(0.0, len);
    let mut run = run_between(&w, 0.0, s, m.pitch);
    let groups: Vec<(u32, f64)> = if plan.groups.is_empty() {
        vec![(u32::MAX, m.gap)]
    } else {
        plan.groups.to_vec()
    };
    let mut k = 0usize;
    'outer: for g in 0.. {
        let (count, after) = groups[g % groups.len()];
        for i in 0..count.max(1) {
            if s + m.width > end + 1e-6 {
                break 'outer;
            }
            let pattern = plan.patterns[k % plan.patterns.len()];
            let sign = side(m.arrange, k);
            let mapped: Vec<Vec2> = pattern.paths[0]
                .iter()
                .map(|q| {
                    let at = s + q.x * m.width;
                    w.at(at) + normal(w.tangent(at)) * (q.y * m.height * sign)
                })
                .collect();
            extend(&mut run, subdivide(&mapped, m.pitch));
            s += m.width;
            k += 1;
            let gap = if i + 1 < count.max(1) { m.gap } else { after };
            let next = (s + gap).min(len);
            extend(&mut run, run_between(&w, s, next, m.pitch));
            s = next;
            if s >= end {
                break 'outer;
            }
        }
        if g > 100_000 {
            break;
        }
    }
    extend(&mut run, run_between(&w, s, len, m.pitch));
    crate::generate::drop_close(&run, crate::generate::MOTIF_MIN_STITCH, m.pitch)
}

/// Stem stitch, as PE-DESIGN 11.50 sews it (line-stem-default.pes): on every straight leg,
/// stations `leg / round(leg / spacing)` apart, each a stitch of `width` across the line
/// leaning `angle` degrees counter-clockwise from it on screen (45: from the top right to the
/// bottom left), sewn top, bottom, top, bottom (the second pass 0.14 mm beside the first, so
/// the rope reads full), then on to the next station's top; `single` sews it once. The first
/// station is at the line's start, the last at its end, and a corner is sewn as a plain needle
/// point between the legs' stations. The line ends at its end point. A curved leg (between two
/// of the `nodes`, see [`nodes_of`]) is measured by arc length, each station leaning from the
/// curve's direction there, so the slanted stitches overlap evenly all along it (#464).
pub fn stem(
    path: &[Vec2],
    nodes: &[usize],
    width: f64,
    spacing: f64,
    angle: f64,
    single: bool,
) -> Run {
    let Some(pieces) = stem_pieces(path, nodes, width, spacing, angle, single) else {
        return path.to_vec();
    };
    let mut run = vec![pieces[0].lead[0]];
    for p in &pieces {
        extend(&mut run, p.pts.iter().copied());
    }
    extend(&mut run, path.last().copied());
    run
}

/// The stations of [`stem`] in sewing order, each with its centre on the line as its lead; a
/// corner is a piece of its own (the corner point).
fn stem_pieces(
    path: &[Vec2],
    nodes: &[usize],
    width: f64,
    spacing: f64,
    angle: f64,
    single: bool,
) -> Option<Vec<Piece>> {
    let legs = legs(path, nodes);
    if legs.is_empty() || spacing <= 0.0 {
        return None;
    }
    let w = Walker::new(path);
    let a = angle.to_radians();
    let along = width / 2.0 * (a.dcos() / a.dsin());
    let mut pieces = Vec::new();
    for (i, leg) in legs.iter().enumerate() {
        let l = leg.len;
        let n = (l / spacing).round().max(1.0) as usize;
        let last = i + 1 == legs.len();
        if i > 0 {
            pieces.push(Piece {
                at: leg.a,
                lead: vec![leg.p],
                pts: vec![leg.p],
            });
        }
        for j in usize::from(i > 0)..n + usize::from(last) {
            let d = l * j as f64 / n as f64;
            let c = leg.at(&w, d);
            let t = leg.dir(&w, d);
            let nrm = normal(t);
            let top = c + t * along - nrm * (width / 2.0);
            let bottom = c - t * along + nrm * (width / 2.0);
            let u = top - bottom;
            let e = normal(u * (1.0 / u.x.dhypot(u.y))) * 1.4;
            pieces.push(Piece {
                at: leg.a + d,
                lead: vec![c],
                pts: if single {
                    vec![top, bottom]
                } else {
                    vec![top - e, bottom - e, top + e, bottom + e]
                },
            });
        }
    }
    Some(pieces)
}

/// Candlewicking, as PE-DESIGN 11.50 sews it (line-candlewick-default.pes): a knot at both
/// ends of every straight leg and evenly between, `round(leg / (size + spacing))` apart per
/// leg. A knot is a star of `points` chords on a circle of `size` around the line: from the
/// point straight ahead it jumps `points / 2 - 1` points at a time until it is back there (a
/// corner knot faces along the leg it leads into). The first knot starts with a stitch to its
/// centre and back. Between knots, running stitch along the line from one knot's front point
/// to the next one's, every node kept, the short stitch first, then `pitch`. A curved leg
/// (between two of the `nodes`, see [`nodes_of`]) is measured by arc length, so its knots are
/// evenly spaced along the curve (#464).
pub fn candlewick(
    path: &[Vec2],
    nodes: &[usize],
    size: f64,
    points: usize,
    spacing: f64,
    pitch: f64,
) -> Run {
    let Some(pieces) = candlewick_pieces(path, nodes, size, points, spacing, pitch, false) else {
        return path.to_vec();
    };
    let mut run = Vec::new();
    for p in &pieces {
        extend(&mut run, p.lead.iter().copied());
        extend(&mut run, p.pts.iter().copied());
    }
    run
}

/// The knots of [`candlewick`] in sewing order. A knot's lead is the running stitch to its
/// front point from the previous knot's (the first knot's: its front point and centre), its
/// points the star; it begins on the line one radius before its centre. `closed`: the path
/// ends where it starts, so the last knot (on the first one) is dropped and the first knot's
/// lead comes from the knot before it.
fn candlewick_pieces(
    path: &[Vec2],
    nodes: &[usize],
    size: f64,
    points: usize,
    spacing: f64,
    pitch: f64,
    closed: bool,
) -> Option<Vec<Piece>> {
    let w = Walker::new(path);
    let len = w.length();
    if len <= 0.0 || size <= 0.0 || pitch <= 0.0 {
        return None;
    }
    let r = size / 2.0;
    let points = points.max(4);
    let step = (points / 2).saturating_sub(1).max(1);
    // Knot stations (arc length, facing direction).
    let mut stations: Vec<(f64, Vec2)> = Vec::new();
    for leg in legs(path, nodes) {
        let l = leg.len;
        let n = (l / (size + spacing).max(1e-6)).round().max(1.0) as usize;
        for j in 0..=n {
            let d = l * j as f64 / n as f64;
            if j == 0 && !stations.is_empty() {
                stations.last_mut().expect("station").1 = leg.dir(&w, d);
                continue;
            }
            stations.push((leg.a + d, leg.dir(&w, d)));
        }
    }
    // Where the loop closes, the knot there faces along the last leg (it ends the loop) and
    // the first leg's runs on from the corner without one (line-candlewick-startend-*.pes).
    let closed = closed && stations.len() > 2;
    if closed {
        stations.remove(0);
    }
    let arcs = node_arcs(path, nodes);
    let at_node = |s: f64| arcs.iter().any(|&n| (n - s).abs() < 1e-6);
    let front = |&(s, t): &(f64, Vec2)| w.at(s) + t * r;
    // Running stitch along the line from `prev`'s front point to arc `b`, then to `to`. After
    // the closing knot (facing away from the next leg) it runs on from the corner, and a short
    // first stitch is shared with the next (1.6 + 1.6 mm, not 0.7 + 2.5).
    let lead = |prev: &(f64, Vec2), b: f64, to: Vec2| {
        let closing = prev.0 >= len - 1e-9;
        let a = if closing { 0.0 } else { (prev.0 + r).min(len) };
        let mut pts = Vec::new();
        if b > a {
            let slice = w.slice(a, b);
            let total: f64 = slice.windows(2).map(|q| q[0].distance(q[1])).sum();
            let sw = Walker::new(&slice);
            let mut cuts: Vec<f64> = Vec::new();
            let mut d = total % pitch;
            if d < 1e-6 {
                d = pitch;
            }
            while d < total - 1e-6 {
                cuts.push(d);
                d += pitch;
            }
            // The nodes' needle points are kept (a curve's flattening vertices are not, or
            // they would leave short stitches between the pitch's).
            let mut cum = 0.0;
            for (i, q) in slice.windows(2).enumerate() {
                cum += q[0].distance(q[1]);
                if i + 2 == slice.len() || at_node(a + cum) {
                    cuts.push(cum);
                }
            }
            cuts.sort_by(f64::total_cmp);
            cuts.dedup_by(|x, y| (*x - *y).abs() < 1e-6);
            if closing && cuts.len() > 1 && cuts[0] < pitch / 2.0 {
                cuts[0] = cuts[1] / 2.0;
            }
            pts.extend(cuts.iter().map(|&d| sw.at(d)));
        }
        pts.push(to);
        pts
    };
    let pieces = stations
        .iter()
        .enumerate()
        .map(|(k, st)| {
            let (c, t) = (w.at(st.0), st.1);
            let lead = match k {
                0 if closed => lead(&stations[stations.len() - 1], st.0, front(st)),
                0 => vec![front(st), c],
                _ => lead(&stations[k - 1], st.0, front(st)),
            };
            let pts = (0..=points)
                .map(|m| {
                    let j = (m * step) % points;
                    let ang = -std::f64::consts::TAU * j as f64 / points as f64;
                    c + t.rotated(ang) * r
                })
                .collect();
            Piece {
                at: st.0 - r,
                lead,
                pts,
            }
        })
        .collect();
    Some(pieces)
}

pub struct StrokeLine {
    /// Distance between strokes.
    pub interval: f64,
    pub stroke: f64,
    pub pitch: f64,
    /// Baseline passes (odd counts end at the far end; even counts get one more pass).
    pub run_times: u32,
    pub stroke_times: u32,
    pub arrange: Arrange,
    /// E stitch: prongs slanted this many degrees from square, forward along the line (#543).
    pub angle: f64,
}

/// Baseline between `a` and `b`, sewn back and forth an odd number of times ≥ `times`.
fn baseline(run: &mut Run, w: &Walker, a: f64, b: f64, pitch: f64, times: u32) {
    let passes = if times % 2 == 1 { times } else { times + 1 };
    for p in 0..passes {
        if p % 2 == 0 {
            extend(run, run_between(w, a, b, pitch));
        } else {
            extend(run, run_between(w, b, a, pitch));
        }
    }
}

/// E stitch (blanket stitch): a baseline with a stroke out to one side every `interval`.
pub fn e_stitch(path: &[Vec2], e: &StrokeLine) -> Run {
    let w = Walker::new(path);
    let len = w.length();
    if len <= 0.0 || e.interval <= 0.0 {
        return path.to_vec();
    }
    let n = (len / e.interval).floor() as usize;
    let mut run = vec![w.at(0.0)];
    for k in 0..=n {
        let s = (k as f64 * e.interval).min(len);
        if k > 0 {
            baseline(&mut run, &w, s - e.interval, s, e.pitch, e.run_times);
        }
        let c = w.at(s);
        let out = side_at(&w, s) * (e.stroke * side(e.arrange, k));
        // Square to the line unless slanted (#543): turned forward along it by the angle.
        let tip = if e.angle == 0.0 {
            c + out
        } else {
            let (sn, cs) = e.angle.to_radians().dsin_cos();
            let fwd = w.tangent(s) * (out.length() * sn);
            c + out * cs + fwd
        };
        for _ in 0..e.stroke_times.max(1) {
            run.push(tip);
            run.push(c);
        }
    }
    let last = n as f64 * e.interval;
    if last < len {
        baseline(&mut run, &w, last, len, e.pitch, e.run_times);
    }
    run
}

/// V stitch, as PE-DESIGN 11.50 sews it (line-v-default.pes): a V every `interval` along the
/// line, from the start to the end. Each arm runs from the line a fifth of the interval before
/// the V's position out to the apex `stroke` to the side and back to the line a fifth after it;
/// the line between Vs is running stitch, sewn `run_times`. The first and last V have one arm.
pub fn v_stitch(path: &[Vec2], e: &StrokeLine) -> Run {
    let w = Walker::new(path);
    let len = w.length();
    if len <= 0.0 || e.interval <= 0.0 {
        return path.to_vec();
    }
    let arm = e.interval / 5.0;
    let n = (len / e.interval + 1e-9).floor() as usize;
    let mut stations: Vec<f64> = (0..=n).map(|k| k as f64 * e.interval).collect();
    if len - stations[n] > arm {
        stations.push(len);
    }
    let mut run = vec![w.at(0.0)];
    let mut at = 0.0;
    for (k, &s) in stations.iter().enumerate() {
        let before = (s - arm).max(at);
        if before > at {
            baseline(&mut run, &w, at, before, e.pitch, e.run_times);
        }
        let apex = w.at(s) + side_at(&w, s) * (e.stroke * side(e.arrange, k));
        let after = (s + arm).min(len);
        extend(&mut run, [apex, w.at(after)]);
        at = after;
    }
    if at < len {
        baseline(&mut run, &w, at, len, e.pitch, e.run_times);
    }
    run
}

/// Zigzag under sewing, as PE-DESIGN 11.50 sews it (line-zigzag-default.pes): a running
/// stitch along the line `offset` to the right (mitred at corners, starting and ending
/// `offset` in from the line's ends), then across and back the same way on the left side;
/// stitches split evenly at no more than `pitch`. Empty when the line is too short.
pub fn zigzag_underlay(path: &[Vec2], offset: f64, pitch: f64) -> Run {
    let (Some(mut run), Some(mut back)) = (
        underlay_side(path, offset, offset, pitch),
        underlay_side(path, offset, -offset, pitch),
    ) else {
        return Vec::new();
    };
    back.reverse();
    extend(&mut run, back);
    run
}

/// The crossing spacing of a Zigzag Stitch on the closed outline `path`, `width` wide, at the
/// line's own `spacing` (10 / density): PE-DESIGN 11.50 spaces a closed outline's crossings by
/// its outer edge, so along the outline they are closer by its length over the outer edge's
/// (#978: a 40 mm square's sides at 4.5 line/mm sew 4.61, 4.72, 4.86, 4.99 and 5.01 crossings per
/// mm at 1, 2, 3, 3.9 and 4 mm, 4.5 × (40 + w) / 40; the appliqué cover's 29.6 mm square at 3 mm
/// 4.97). The outer edge is the outline offset half the width outwards with mitred corners: each
/// turn of θ adds w · tan(θ / 2) (taken at most w · 2, a very sharp point), a turn the other
/// way takes it off; a circle's is its length plus π w. Open lines keep `spacing` (PE-DESIGN's
/// open-line references: 4.48 per mm at 4.5).
pub fn closed_zigzag_spacing(path: &[Vec2], width: f64, spacing: f64) -> f64 {
    let mut p: Vec<Vec2> = Vec::with_capacity(path.len());
    for &q in path {
        if p.last().is_none_or(|l: &Vec2| l.distance(q) > 1e-6) {
            p.push(q);
        }
    }
    if p.len() > 1 && p[0].distance(p[p.len() - 1]) <= 1e-6 {
        p.pop();
    }
    let n = p.len();
    if n < 3 || width <= 0.0 {
        return spacing;
    }
    let length: f64 = (0..n).map(|i| p[i].distance(p[(i + 1) % n])).sum();
    let turns: Vec<f64> = (0..n)
        .map(|i| {
            let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
            let (u, v) = (b - a, c - b);
            (u.x * v.y - u.y * v.x).datan2(u.x * v.x + u.y * v.y)
        })
        .collect();
    // The outward side turns the way the whole outline turns.
    let sign = turns.iter().sum::<f64>().signum();
    let extra: f64 = turns
        .iter()
        .map(|&t| (t * sign / 2.0).dtan().clamp(-2.0, 2.0) * width)
        .sum();
    let outer = length + extra;
    if outer <= length * 0.5 || length <= 0.0 {
        return spacing;
    }
    spacing * length / outer
}

/// Zigzag Stitch line width (0.1 mm) from which PE-DESIGN 11.50 sews four under sewing runs
/// instead of two (#964: on the 40 mm square 3,283 stitches at 3.9 mm, 3,448 at 4.0 mm).
pub const ZIGZAG_FOUR_RUNS: f64 = 40.0;

/// A Zigzag Stitch line's under sewing as PE-DESIGN 11.50 sews it, `width` wide (#964, captures
/// of a 40 mm square at 1, 2, 3, 3.9 and 4 mm): running stitches `width / 2 − 0.5 mm` to either
/// side of the line (on it below 1 mm), the right one out and the left one back as
/// [`zigzag_underlay`]; from [`ZIGZAG_FOUR_RUNS`] two more between them a third of the way in,
/// four runs 1 mm apart at 4 mm, each sewn the other way from the one before, from the right
/// edge to the left.
pub fn pe_zigzag_underlay(path: &[Vec2], width: f64, pitch: f64) -> Run {
    let edge = (width / 2.0 - 5.0).max(0.0);
    if width < ZIGZAG_FOUR_RUNS - 1e-6 {
        return zigzag_underlay(path, edge, pitch);
    }
    let mut run: Run = Vec::new();
    for (k, d) in [edge, edge / 3.0, -edge / 3.0, -edge]
        .into_iter()
        .enumerate()
    {
        let Some(mut side) = underlay_side(path, edge, d, pitch) else {
            return Vec::new();
        };
        if k % 2 == 1 {
            side.reverse();
        }
        extend(&mut run, side);
    }
    run
}

/// One run of [`zigzag_underlay`], `d` to the right of the line, from `offset` in from its
/// start to `offset` in from its end; `None` when the line is too short.
fn underlay_side(path: &[Vec2], offset: f64, d: f64, pitch: f64) -> Option<Vec<Vec2>> {
    let w = Walker::new(path);
    let len = w.length();
    if path.len() < 2 || len <= 2.0 * offset {
        return None;
    }
    let mut pts = vec![w.at(offset) + normal(w.tangent(offset)) * d];
    let mut s = 0.0;
    for i in 1..path.len() - 1 {
        s += path[i - 1].distance(path[i]);
        if s <= offset || s >= len - offset {
            continue;
        }
        let (n1, n2) = (normal(w.tangent(s - 1e-3)), normal(w.tangent(s + 1e-3)));
        let k = 1.0 + (n1.x * n2.x + n1.y * n2.y);
        // A mitre longer than twice the offset (a turn over 120 degrees: a heart's cusp,
        // a star's point) would spike far out of the column (#466): bevelled instead.
        if k >= 0.5 {
            pts.push(path[i] + (n1 + n2) * (d / k));
        } else {
            pts.extend([path[i] + n1 * d, path[i] + n2 * d]);
        }
    }
    pts.push(w.at(len - offset) + normal(w.tangent(len - offset)) * d);
    Some(subdivide(&pts, pitch))
}

/// Chain stitch. Triangles, as PE-DESIGN 11.50 sews them (line-chain-default.pes): on every
/// straight leg, `n = floor(leg / (0.75 * size))` links `leg / n` apart, each a triangle from
/// its point on the line to a base `size` further on and `size / 3` to either side, sewn point,
/// left, right, point `times` times; then to the left corner, the base's middle and back to
/// the next link's point (links overlap; the last runs past the leg's end). Diamonds: links of
/// `size` along the line, upper halves forward, lower halves back, repeated `times`. A curved
/// triangle leg (between two of the `nodes`, see [`nodes_of`]) is measured by arc length, so
/// its links are evenly spaced and equal along the curve (#464).
pub fn chain(
    path: &[Vec2],
    nodes: &[usize],
    size: f64,
    shape: ChainShape,
    times: u32,
    arrange: Arrange,
) -> Run {
    let w = Walker::new(path);
    let len = w.length();
    if len <= 0.0 || size <= 0.0 {
        return path.to_vec();
    }
    if shape == ChainShape::Triangle {
        let mut run = Vec::new();
        for p in chain_pieces(path, nodes, size, times, arrange) {
            extend(&mut run, p.lead);
            run.extend(p.pts);
        }
        return run;
    }
    let n = (len / size).round().max(1.0) as usize;
    let l = len / n as f64;
    let h = size / 2.0;
    let peak = |k: usize, sign: f64| {
        let s = (k as f64 + 0.5) * l;
        w.at(s) + normal(w.tangent(s)) * (h * sign * side(arrange, k))
    };
    let mut run = vec![w.at(0.0)];
    for _ in 0..times.max(1) {
        for k in 0..n {
            extend(&mut run, [peak(k, 1.0), w.at((k + 1) as f64 * l)]);
        }
        for k in (0..n).rev() {
            extend(&mut run, [peak(k, -1.0), w.at(k as f64 * l)]);
        }
    }
    run
}

/// The triangle links of [`chain`] in sewing order, each with its point on the line as its
/// lead.
fn chain_pieces(
    path: &[Vec2],
    nodes: &[usize],
    size: f64,
    times: u32,
    arrange: Arrange,
) -> Vec<Piece> {
    let w = Walker::new(path);
    let mut pieces = Vec::new();
    for leg in legs(path, nodes) {
        let l = leg.len;
        let n = (l / (0.75 * size) + 1e-9).floor().max(1.0) as usize;
        for j in 0..n {
            let d = l * j as f64 / n as f64;
            let p = leg.at(&w, d);
            // On a curve the link points at the curve one size further on (the chord), so its
            // base stays on the line (#464).
            let ahead = leg.at(&w, (d + size).min(l)) - p;
            let t = match ahead.x.dhypot(ahead.y) {
                h if !leg.straight && h > 1e-6 => ahead * (1.0 / h),
                _ => leg.dir(&w, d),
            };
            let nrm = normal(t) * (size / 3.0 * side(arrange, pieces.len()));
            let base = p + t * size;
            let mut pts = Vec::new();
            for _ in 0..times.max(1) {
                pts.extend([base - nrm, base + nrm, p]);
            }
            pts.extend([base - nrm, base]);
            pieces.push(Piece {
                at: leg.a + d,
                lead: vec![p],
                pts,
            });
        }
    }
    pieces
}

/// One element of a stem, candlewick or chain line: where it begins on the line (`at`, arc
/// length), what is sewn to reach it (`lead`: the point on the line it starts from, or for a
/// candlewick knot the running stitch from the previous knot) and its own stitches.
struct Piece {
    at: f64,
    lead: Run,
    pts: Run,
}

/// A closed line [`start_loop`] can start at its entry point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LoopLine {
    Stem {
        width: f64,
        spacing: f64,
        angle: f64,
        single: bool,
    },
    Candlewick {
        size: f64,
        points: usize,
        spacing: f64,
        pitch: f64,
    },
    /// Triangle links.
    Chain {
        size: f64,
        times: u32,
        arrange: Arrange,
    },
}

/// A closed stem, candlewick or triangle chain line with moved entry and exit points, as
/// PE-DESIGN 11.50 sews it (line-*-startend-*.pes): every element stays where it is without the
/// points; only where the sewing starts and ends changes. An element belongs to a point when it
/// is the last one to begin at or before it. The pattern starts with the element of the exit
/// point (arc `exit`) and goes round the loop to the one before it, so it also ends there; a
/// stem sews that station once more, as an unmoved closed stem does at its start, and a
/// candlewick sews that knot last. Feed (`run_from: None`) starts there. Run starts at the
/// element of the entry point (arc `run_from`) and sews only the leads (the elements' points on
/// the line; a candlewick's running stitch) to the exit's element, the shorter way round.
/// Returns the stitches and whether they start with leads rather than an element.
pub fn start_loop(
    path: &[Vec2],
    nodes: &[usize],
    line: LoopLine,
    exit: f64,
    run_from: Option<f64>,
) -> Option<(Run, bool)> {
    let len = Walker::new(path).length();
    let (pieces, stem, defer) = match line {
        LoopLine::Stem {
            width,
            spacing,
            angle,
            single,
        } => {
            let mut p = stem_pieces(path, nodes, width, spacing, angle, single)?;
            // Where the loop closes PE-DESIGN sews the corner point and the last station's
            // bottom point instead of the whole station (the first station is there too).
            let last = p.last_mut().expect("station");
            let (v, n) = (last.lead[0], last.pts.len());
            let bottom = if n == 4 {
                (last.pts[1] + last.pts[3]) * 0.5
            } else {
                last.pts[n - 1]
            };
            last.pts = vec![v, bottom];
            (p, true, false)
        }
        LoopLine::Candlewick {
            size,
            points,
            spacing,
            pitch,
        } => (
            candlewick_pieces(path, nodes, size, points, spacing, pitch, true)?,
            false,
            true,
        ),
        LoopLine::Chain {
            size,
            times,
            arrange,
        } => (
            chain_pieces(path, nodes, size, times, arrange),
            false,
            false,
        ),
    };
    let n = pieces.len();
    if n < 2 || len <= 0.0 {
        return None;
    }
    let owner = |s: f64| {
        let behind = |k: usize| (s - pieces[k].at + 1e-6).rem_euclid(len);
        (0..n)
            .min_by(|&a, &b| behind(a).total_cmp(&behind(b)))
            .unwrap_or(0)
    };
    let k1 = owner(exit);
    let k2 = run_from.map_or(k1, owner);
    // The leads from the entry's element to the exit's, forwards or (when that is shorter)
    // backwards round the loop.
    let back = (k2 + n - k1) % n < (k1 + n - k2) % n;
    let mut run = Vec::new();
    let mut k = k2;
    loop {
        if back && k != k1 {
            extend(&mut run, pieces[k].lead.iter().rev().copied());
        } else {
            extend(&mut run, pieces[k].lead.iter().copied());
        }
        if k == k1 {
            break;
        }
        k = if back { (k + n - 1) % n } else { (k + 1) % n };
    }
    // A stem's stations follow on from one another without their centres (corners keep
    // their point).
    let sew = |p: &Piece, run: &mut Run| {
        if !stem || p.pts == p.lead {
            extend(run, p.lead.iter().copied());
        }
        extend(run, p.pts.iter().copied());
    };
    if defer {
        for i in 1..=n {
            sew(&pieces[(k1 + i) % n], &mut run);
        }
    } else {
        extend(&mut run, pieces[k1].pts.iter().copied());
        for i in 1..n {
            sew(&pieces[(k1 + i) % n], &mut run);
        }
        if stem {
            extend(&mut run, pieces[k1].pts.iter().copied());
        }
    }
    Some((run, k2 != k1 || defer))
}

/// The elements of a stem, candlewick or triangle chain line in sewing order, each as its lead
/// and its own stitches (for the tests).
#[cfg(test)]
pub(crate) fn elements(path: &[Vec2], nodes: &[usize], line: LoopLine) -> Vec<(Run, Run)> {
    let pieces = match line {
        LoopLine::Stem {
            width,
            spacing,
            angle,
            single,
        } => stem_pieces(path, nodes, width, spacing, angle, single).unwrap_or_default(),
        LoopLine::Candlewick {
            size,
            points,
            spacing,
            pitch,
        } => {
            candlewick_pieces(path, nodes, size, points, spacing, pitch, false).unwrap_or_default()
        }
        LoopLine::Chain {
            size,
            times,
            arrange,
        } => chain_pieces(path, nodes, size, times, arrange),
    };
    pieces.into_iter().map(|p| (p.lead, p.pts)).collect()
}

/// Arc length along `path` of the point on it nearest `p`.
pub fn arc_of(path: &[Vec2], p: Vec2) -> f64 {
    let (mut best, mut at, mut s0) = (f64::INFINITY, 0.0, 0.0);
    for q in path.windows(2) {
        let d = q[1] - q[0];
        let l2 = d.x * d.x + d.y * d.y;
        let t = if l2 > 0.0 {
            (((p - q[0]).x * d.x + (p - q[0]).y * d.y) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dist = (q[0] + d * t).distance(p);
        if dist < best {
            (best, at) = (dist, s0 + l2.sqrt() * t);
        }
        s0 += l2.sqrt();
    }
    at
}

/// Cutwork needle penetration spacing (0.1 mm).
pub const CUT_PITCH: f64 = 10.0;
/// Length of an uncut bridge (0.1 mm).
pub const CUT_BRIDGE: f64 = 20.0;

/// Cutting line for the four cutwork needles (blades at 0°, 45°, 90°, 135°): penetrations every
/// [`CUT_PITCH`], each stretch going to the needle whose blade best matches its direction.
/// Returns runs per needle. `leave_uncut` keeps short bridges (four on a closed line, two on
/// an open one).
pub fn cutting(path: &[Vec2], closed: bool, leave_uncut: bool) -> [Vec<Run>; 4] {
    let mut out: [Vec<Run>; 4] = Default::default();
    let w = Walker::new(path);
    let len = w.length();
    if len <= 0.0 {
        return out;
    }
    let bridges: Vec<f64> = if leave_uncut {
        let count = if closed { 4 } else { 2 };
        (0..count)
            .map(|k| len * (k as f64 + 0.5) / count as f64)
            .collect()
    } else {
        Vec::new()
    };
    let pts = subdivide(path, CUT_PITCH);
    let mut s = 0.0;
    let mut current: Option<(usize, Run)> = None;
    for seg in pts.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let d = a.distance(b);
        let mid = s + d / 2.0;
        s += d;
        if d <= 1e-9 {
            continue;
        }
        let bridged = bridges.iter().any(|&c| (mid - c).abs() < CUT_BRIDGE / 2.0);
        let deg = (b.y - a.y).datan2(b.x - a.x).to_degrees().rem_euclid(180.0);
        let needle = ((deg / 45.0).round() as usize) % 4;
        match &mut current {
            Some((n, run)) if !bridged && *n == needle && run.last() == Some(&a) => run.push(b),
            _ => {
                if let Some((n, run)) = current.take() {
                    out[n].push(run);
                }
                if !bridged {
                    current = Some((needle, vec![a, b]));
                }
            }
        }
    }
    if let Some((n, run)) = current {
        out[n].push(run);
    }
    out
}

/// Zigzag like [`crate::generate::zigzag`], with half stitch: on the inner side of a bend,
/// a point closer than one spacing (half the straight-line distance) to the previous point on
/// that side is pulled to the
/// centre line, so stitches do not pile up.
pub fn zigzag_half(path: &[Vec2], width: f64, spacing: f64) -> Run {
    if path.len() < 2 || spacing.is_nan() || spacing <= 0.0 {
        return path.to_vec();
    }
    let mut run: Run = Vec::new();
    let mut centres: Vec<Vec2> = Vec::new();
    let mut side_sign = 1.0;
    let mut carried = 0.0;
    for seg in path.windows(2) {
        let (a, b) = (seg[0], seg[1]);
        let len = a.distance(b);
        if len <= 0.0 {
            continue;
        }
        let dir = (b - a) * (1.0 / len);
        let nrm = v2(-dir.y, dir.x);
        let mut t = carried;
        while t <= len {
            let c = a + dir * t;
            run.push(c + nrm * (side_sign * width / 2.0));
            centres.push(c);
            side_sign = -side_sign;
            t += spacing;
        }
        carried = t - len;
    }
    let mut last_kept: [Option<Vec2>; 2] = [None, None];
    for i in 0..run.len() {
        let slot = i % 2;
        if let Some(prev) = last_kept[slot]
            && prev.distance(run[i]) < spacing
        {
            run[i] = centres[i];
            continue;
        }
        last_kept[slot] = Some(run[i]);
    }
    run
}

/// Zigzag between two rails, as PE-DESIGN 11.50 sews it (line-zigzag-default.pes and the
/// density minimum and maximum files): the rails are the line offset `width / 2` to each side,
/// mitred at corners. Along each straight leg a stitch joins the two rails at the same fraction
/// of the leg, so stitches lean more and more towards a mitre; consecutive stitches stay
/// `spacing` apart measured across the leg divided by the lean, which keeps the thread density
/// the same. It starts on the left rail at the line's start and ends with a stitch exactly at
/// the line's end.
pub fn zigzag_rails(path: &[Vec2], width: f64, spacing: f64) -> Run {
    zigzag_rails_sided(path, [width / 2.0; 2], spacing)
}

/// [`zigzag_rails`] with the left and right rail `half[0]` and `half[1]` from the line
/// (compensation per side, #522).
pub fn zigzag_rails_sided(path: &[Vec2], half: [f64; 2], spacing: f64) -> Run {
    zigzag_rails_cornered(path, half, spacing, None)
}

/// The longest stitch of the run PE-DESIGN sews out to a cut corner's point (#373,
/// miter-b-sc1-m1/m3/max50.pes: 2.1, 3.2 and 4.0 mm runs in 2, 2 and 3 stitches).
const CORNER_RUN: f64 = 20.0;

/// One piece of a zigzag column: rung `[left, right]` at its start and at its end, `len` along
/// `d`, the direction the stitches advance in. With `first`, the piece starts by sewing its
/// first rung across from that side, after a run out to it when `first.1` (the zigzag
/// otherwise carries on from the last piece).
#[derive(Clone, Copy)]
struct Quad {
    from: [Vec2; 2],
    to: [Vec2; 2],
    len: f64,
    d: Vec2,
    first: Option<(f64, bool)>,
}

/// How a zigzag column turns a vertex: the rung that ends the leg before it, the rung that
/// starts the leg after it (each with the point the leg's length is measured to), and the
/// pieces sewn between them.
struct Turn {
    end: [Vec2; 2],
    end_at: Vec2,
    start: [Vec2; 2],
    start_at: Vec2,
    pieces: Vec<Quad>,
}

/// Where the line through `p` along `d` meets the one through `q` along `e`.
fn meet(p: Vec2, d: Vec2, q: Vec2, e: Vec2) -> Option<Vec2> {
    let den = d.x * e.y - d.y * e.x;
    if den.abs() < 1e-9 {
        return None;
    }
    let w = q - p;
    Some(p + d * ((w.x * e.y - w.y * e.x) / den))
}

fn dot(a: Vec2, b: Vec2) -> f64 {
    a.x * b.x + a.y * b.y
}

/// A piece between two rungs whose length is measured between their midpoints.
fn piece(from: [Vec2; 2], to: [Vec2; 2], first: Option<(f64, bool)>) -> Option<Quad> {
    let (m0, m1) = (from[0].lerp(from[1], 0.5), to[0].lerp(to[1], 0.5));
    let len = m0.distance(m1);
    (len > 1e-6).then(|| Quad {
        from,
        to,
        len,
        d: (m1 - m0) * (1.0 / len),
        first,
    })
}

/// A rung with the point on outer side `sigma` (+1 right, −1 left) and the inner point.
fn rung(sigma: f64, outer: Vec2, inner: Vec2) -> [Vec2; 2] {
    if sigma > 0.0 {
        [inner, outer]
    } else {
        [outer, inner]
    }
}

/// Cosine of the corner angle below which PE-DESIGN cuts a zigzag corner's point (#373): its
/// two Sharp Corners buttons sew differently at 45° and 59.9° and identically, with the full
/// mitre, at 75.3° and 90° (line-zigzag-miter-v45/v60/v75-*). cos 67.5°, between the two.
const SHARP_CORNER_COS: f64 = 0.382_683_432_365_089_8;

/// PE-DESIGN's sharp corner (#373) at `v` between legs along `dp` and `dn`, sharper than
/// 67.5 degrees ([`SHARP_CORNER_COS`]), with rails `half` [left, right] from the line, the Miter Limit `limit` (0.1 mm)
/// and button 2 when `split`; `None` keeps the mitre. The point's tip is cut at `limit` from
/// `v` along the bisector, square to it. Button 1 (miter-b-sc1-*.pes) ends the first leg on
/// the rung where its rails cross the second leg's, runs out to the cut and fills the point
/// as a wedge back to that rung, where the second leg starts. Button 2 (miter-b-sc2-*.pes)
/// sews the first leg square to it out to the cut, its inner side reaching the second leg's
/// centre line where the rails meet, and the second leg from the cut with its inner side
/// along the bisector to where the rails meet.
fn sharp_turn(
    v: Vec2,
    dp: Vec2,
    dn: Vec2,
    half: [f64; 2],
    limit: f64,
    split: bool,
    legs: [f64; 2],
) -> Option<Turn> {
    // The legs' directions: the corner angle θ has cos θ = −dp·dn.
    if dot(dp, dn) > -SHARP_CORNER_COS {
        return None;
    }
    let bis = dp - dn;
    let u = bis * (1.0 / bis.length());
    let (np, nn) = (normal(dp), normal(dn));
    let sigma = if dot(np, u) > 0.0 { 1.0 } else { -1.0 };
    let h = |s: f64| if s < 0.0 { half[0] } else { half[1] };
    let rail_p = |s: f64| v + np * (s * h(s));
    let rail_n = |s: f64| v + nn * (s * h(s));
    // The rung where the legs' rails cross (on the cut through the corner when the sides are
    // even); legs reversing onto themselves end square.
    let xo = meet(rail_p(sigma), dp, rail_n(-sigma), dn).unwrap_or(rail_p(sigma));
    let xi = meet(rail_p(-sigma), dp, rail_n(sigma), dn).unwrap_or(rail_p(-sigma));
    let tip = meet(rail_p(sigma), dp, rail_n(sigma), dn);
    let reach = tip.map_or(f64::INFINITY, |t| dot(t - v, u));
    let c = limit.clamp(0.0, reach);
    let cut = v + u * c;
    let across = v2(-u.y, u.x);
    let bp = meet(rail_p(sigma), dp, cut, across)?;
    let bn = meet(rail_n(sigma), dn, cut, across)?;
    if split && let Some(q) = meet(rail_p(-sigma), dp, rail_n(-sigma), dn) {
        // Where the inner rails meet, measured back along each leg.
        let (ap, an) = (dot(q - v, dp), dot(q - v, dn));
        if -ap < legs[0] / 2.0 && -an < legs[1] / 2.0 {
            let foot = |p: Vec2, d: Vec2| p + d * dot(q - p, d);
            let po = foot(rail_p(sigma), dp);
            let no = foot(rail_n(sigma), dn);
            // The second leg's centre line across from `q`, no further from the corner than
            // the full point.
            let reach_c = tip.map_or(f64::INFINITY, |t| t.distance(v));
            let centre = meet(v, dn, q, np)
                .filter(|c| c.distance(v) <= reach_c)
                .unwrap_or_else(|| v + dn * reach_c.min(v.distance(q)));
            let pieces = [
                piece(rung(sigma, po, centre), rung(sigma, bp, cut), None),
                piece(
                    rung(sigma, bn, cut),
                    rung(sigma, no, q),
                    Some((-sigma, false)),
                ),
            ];
            return Some(Turn {
                end: rung(sigma, po, q),
                end_at: v + dp * ap,
                start: rung(sigma, no, q),
                start_at: v + dn * an,
                pieces: pieces.into_iter().flatten().collect(),
            });
        }
    }
    let wedge = if c > 1e-6 {
        piece(
            rung(sigma, bn, bp),
            rung(sigma, xi, xo),
            Some((-sigma, true)),
        )
    } else {
        None
    };
    Some(Turn {
        end: rung(sigma, xo, xi),
        end_at: v,
        start: rung(sigma, xi, xo),
        start_at: v,
        pieces: wedge.into_iter().collect(),
    })
}

/// [`zigzag_rails_sided`] with PE-DESIGN's Sharp Corners and Miter Limit (#373): `corners`
/// cuts the point of every corner sharper than 67.5 degrees (see [`sharp_turn`]); `None` mitres
/// every corner in full.
pub fn zigzag_rails_cornered(
    path: &[Vec2],
    half: [f64; 2],
    spacing: f64,
    corners: Option<&crate::model::SharpCorners>,
) -> Run {
    let pts: Vec<Vec2> = {
        let mut v: Vec<Vec2> = Vec::new();
        for &p in path {
            if v.last().is_none_or(|l| l.distance(p) > 1e-9) {
                v.push(p);
            }
        }
        v
    };
    if pts.len() < 2 || spacing.is_nan() || spacing <= 0.0 {
        return pts;
    }
    let side_h = |sign: f64| if sign < 0.0 { half[0] } else { half[1] };
    let dir = |i: usize| {
        let d = pts[i + 1] - pts[i];
        d * (1.0 / pts[i].distance(pts[i + 1]))
    };
    // Rail point at vertex `i` on side `sign` (+1 right, −1 left).
    let rail = |i: usize, sign: f64| -> Vec2 {
        let h = side_h(sign);
        let n = if i == 0 {
            normal(dir(0))
        } else if i == pts.len() - 1 {
            normal(dir(i - 1))
        } else {
            let (a, b) = (normal(dir(i - 1)), normal(dir(i)));
            let k = 1.0 + (a.x * b.x + a.y * b.y);
            if k < 1e-6 {
                return pts[i] + a * (sign * h);
            }
            (a + b) * (1.0 / k)
        };
        pts[i] + n * (sign * h)
    };
    let legs = pts.len() - 1;
    let turns: Vec<Option<Turn>> = (0..pts.len())
        .map(|i| {
            let c = corners?;
            if i == 0 || i == legs {
                return None;
            }
            sharp_turn(
                pts[i],
                dir(i - 1),
                dir(i),
                half,
                c.limit() * 10.0,
                c.split(),
                [pts[i - 1].distance(pts[i]), pts[i].distance(pts[i + 1])],
            )
        })
        .collect();
    let mut quads: Vec<Quad> = Vec::new();
    for i in 0..legs {
        let mitre = |j: usize| [rail(j, -1.0), rail(j, 1.0)];
        let (from, from_at) = match &turns[i] {
            Some(t) => {
                quads.extend(t.pieces.iter().copied());
                (t.start, Some(t.start_at))
            }
            None => (mitre(i), None),
        };
        let (to, to_at) = match &turns[i + 1] {
            Some(t) => (t.end, Some(t.end_at)),
            None => (mitre(i + 1), None),
        };
        let d = dir(i);
        // A leg between two mitres keeps its full length; a cut corner measures to its rung.
        let len = if from_at.is_none() && to_at.is_none() {
            pts[i].distance(pts[i + 1])
        } else {
            dot(to_at.unwrap_or(pts[i + 1]) - from_at.unwrap_or(pts[i]), d)
        };
        if len > 1e-6 {
            quads.push(Quad {
                from,
                to,
                len,
                d,
                first: None,
            });
        }
    }
    sew_quads(&quads, spacing)
}

/// Sews zigzag pieces one after another: along each piece a stitch joins its two rails at the
/// same fraction of the piece, consecutive stitches `spacing` apart across the piece (divided
/// by the stitch's lean), carried on from piece to piece; the last piece ends with a stitch
/// exactly across its end.
fn sew_quads(quads: &[Quad], spacing: f64) -> Run {
    let mut run: Run = Vec::new();
    let mut side = -1.0;
    let mut carry = 0.0;
    for (k, q) in quads.iter().enumerate() {
        let at = |f: f64, s: f64| {
            if s < 0.0 {
                q.from[0].lerp(q.to[0], f)
            } else {
                q.from[1].lerp(q.to[1], f)
            }
        };
        // The stitch across at `f` leans by the rails' offset along the piece.
        let step = |f: f64| {
            let r = at(f, 1.0) - at(f, -1.0);
            let span = r.length();
            let lean = if span < 1e-6 {
                1.0
            } else {
                span / (r.x * q.d.y - r.y * q.d.x).abs().max(1e-9)
            };
            spacing * lean / q.len
        };
        let mut f = carry / q.len;
        if let Some((s, travel)) = q.first {
            // The piece's first rung sewn across first, after a run out to it.
            let target = at(0.0, s);
            if let Some(&last) = run.last().filter(|_| travel) {
                run.extend(subdivide(&[last, target], CORNER_RUN).into_iter().skip(1));
            }
            if run.last().is_none_or(|l| l.distance(target) > 1e-9) {
                run.push(target);
            }
            run.push(at(0.0, -s));
            side = s;
            f = step(0.0);
        }
        while f <= 1.0 + 1e-9 {
            run.push(at(f.min(1.0), side));
            side = -side;
            f += step(f.min(1.0));
        }
        carry = (f - 1.0) * q.len;
        if k + 1 == quads.len() && carry > 1e-6 {
            // A last stitch across exactly at the line's end, after the last regular one.
            let last = run.last().copied();
            let end = at(1.0, side);
            if last.is_none_or(|p| p.distance(end) > 1e-9) {
                run.push(end);
            }
            run.push(at(1.0, -side));
        }
    }
    run
}

/// The rungs [left, right] of a zigzag line `width` wide at the vertices of `path`, its rails
/// mitred at the corners as in [`zigzag_rails`].
fn vertex_rungs(path: &[Vec2], width: f64) -> Vec<[Vec2; 2]> {
    let mut pts: Vec<Vec2> = Vec::new();
    for &p in path {
        if pts.last().is_none_or(|l| l.distance(p) > 1e-9) {
            pts.push(p);
        }
    }
    let h = width / 2.0;
    let dir = |i: usize| (pts[i + 1] - pts[i]) * (1.0 / pts[i].distance(pts[i + 1]));
    (0..pts.len())
        .map(|i| {
            let n = if pts.len() < 2 {
                v2(0.0, 0.0)
            } else if i == 0 {
                normal(dir(0))
            } else if i == pts.len() - 1 {
                normal(dir(i - 1))
            } else {
                let (a, b) = (normal(dir(i - 1)), normal(dir(i)));
                let k = 1.0 + (a.x * b.x + a.y * b.y);
                if k < 1e-6 { a } else { (a + b) * (1.0 / k) }
            };
            [pts[i] - n * h, pts[i] + n * h]
        })
        .collect()
}

/// A zigzag line with OpenStitch's satin shaping (#520-#522, see
/// [`crate::column::SatinShaping`]): with turned corners PE-DESIGN's [`zigzag_rails`] with each
/// rail moved out by its side's compensation; with corner styles, the line's mitred rungs sewn
/// through [`crate::column::zigzag_with`]; then short stitches. A curved line (`half_stitch`
/// is `Some`) keeps PE-DESIGN's curve zigzag ([`zigzag_half`] with Half Stitch) unless it is
/// compensated, and is folded back where it crosses itself ([`unfold_zigzag`]).
pub fn zigzag_shaped(
    path: &[Vec2],
    width: f64,
    spacing: f64,
    shaping: &crate::column::SatinShaping,
    half_stitch: Option<bool>,
) -> Run {
    let curved = half_stitch.is_some();
    if path.len() < 2 || spacing.is_nan() || spacing <= 0.0 {
        return path.to_vec();
    }
    // A closed line with corner styles starts halfway along its first side, so its first
    // vertex can be a corner too.
    // A line with no corner sharp enough for a mitre or cap keeps PE-DESIGN's zigzag.
    let rotated: Vec<Vec2>;
    let turned = shaping.corners.style == crate::column::CornerStyle::Turn
        || !crate::column::has_corners(&vertex_rungs(path, width), spacing, &shaping.corners);
    let path = if !turned && path.len() > 3 && path[0].distance(path[path.len() - 1]) < 1e-9 {
        let m = path[0].lerp(path[1], 0.5);
        rotated = std::iter::once(m)
            .chain(path[1..].iter().copied())
            .chain(std::iter::once(m))
            .collect();
        &rotated[..]
    } else {
        path
    };
    let mut run = if turned {
        let [l, r] = shaping.compensation.sides(0.0, width);
        let plain = l == 0.0 && r == 0.0;
        let open = if shaping.style == crate::column::SatinStyle::Open {
            3.0
        } else {
            1.0
        };
        let mut run = if half_stitch == Some(true) && plain {
            zigzag_half(path, width, spacing * open)
        } else {
            zigzag_rails_sided(path, [width / 2.0 + l, width / 2.0 + r], spacing * open)
        };
        // Folds undone before the styles add points off the edges.
        if curved {
            unfold_zigzag(path, &mut run, width);
        }
        crate::column::finish(vec![(run, true)], shaping)
    } else {
        let rungs = vertex_rungs(path, width);
        if rungs.len() < 2 {
            return path.to_vec();
        }
        crate::column::zigzag_with(&rungs, spacing, shaping)
    };
    // Raised satin (#531): the padding lanes first, ending where the zigzag starts.
    if let Some(raised) = &shaping.raised {
        let mut pad = crate::column::padding(&vertex_rungs(path, width), raised);
        if pad.len() > 1 {
            pad.extend(run);
            run = pad;
        }
    }
    run
}

/// A satin line (#524): a satin column along `path` (0.1 mm) whose width follows the line's
/// width keys, tapers and slant ([`crate::skeleton::SatinLine`]), with its satin shaping. With
/// one width all along, square to the line and no shaping, it is the skeleton satin of that
/// width ([`crate::skeleton::satin`]); if such a line is straight segments (`curved` false)
/// with a corner sharper than 67.5 degrees, it is sewn as a zigzag line of that width, so the
/// line's Sharp Corners button and Miter Limit cut the point as on a zigzag line (#373; a
/// corner style takes precedence, as there).
pub fn satin_line(path: &[Vec2], line: &crate::model::LineSewing, curved: bool) -> Run {
    let spacing = 10.0 / line.density.max(0.1);
    let sl = satin_line_shape(line);
    let shaping = &line.satin_shaping;
    if sl.is_uniform() && shaping.is_plain() {
        if !curved && has_sharp_corner(path) {
            // Its width (mm) as rails 0.1 mm from the line.
            let half = sl.uniform_width() * 5.0;
            return zigzag_rails_cornered(path, [half; 2], spacing, Some(&line.sharp_corners));
        }
        let skeleton = crate::skeleton::Skeleton {
            points: path.to_vec(),
            widths: vec![sl.uniform_width(); path.len()],
        };
        return crate::skeleton::satin(&skeleton, spacing, 0.0);
    }
    // Rungs every 1 mm carry the width; the stitches are placed between them.
    let rungs = crate::skeleton::satin_line_rungs(path, &sl, 10.0);
    if rungs.len() < 2 {
        return Vec::new();
    }
    let mut run = crate::column::zigzag_with(&rungs, spacing, shaping);
    if let Some(raised) = &shaping.raised {
        let mut pad = crate::column::padding(&rungs, raised);
        if pad.len() > 1 {
            pad.extend(run);
            run = pad;
        }
    }
    run
}

/// Whether `path` turns at a vertex by more than a corner of 67.5 degrees allows (see
/// [`SHARP_CORNER_COS`]): where PE-DESIGN's Sharp Corners cut a zigzag line's point (#373).
fn has_sharp_corner(path: &[Vec2]) -> bool {
    let pts: Vec<Vec2> = {
        let mut v: Vec<Vec2> = Vec::new();
        for &p in path {
            if v.last().is_none_or(|l| l.distance(p) > 1e-9) {
                v.push(p);
            }
        }
        v
    };
    pts.windows(3).any(|w| {
        let (a, b) = (w[1] - w[0], w[2] - w[1]);
        let (la, lb) = (a.length(), b.length());
        la > 1e-9 && lb > 1e-9 && dot(a * (1.0 / la), b * (1.0 / lb)) < -SHARP_CORNER_COS
    })
}

/// A satin line's widths, tapers and slant.
pub fn satin_line_shape(line: &crate::model::LineSewing) -> crate::skeleton::SatinLine {
    crate::skeleton::SatinLine {
        width: line.width,
        keys: line.width_keys.clone(),
        taper: [
            line.taper_start.unwrap_or(0.0),
            line.taper_end.unwrap_or(0.0),
        ],
        slant: line.slant.unwrap_or(0.0),
    }
}

/// The widest a satin line gets, mm: its width or its widest key.
pub fn satin_line_width(line: &crate::model::LineSewing) -> f64 {
    line.width_keys
        .iter()
        .map(|k| k.left + k.right)
        .fold(line.width, f64::max)
}

/// The under sewing of a zigzag line `width` wide (#523): `edge` is PE-DESIGN's
/// [`zigzag_underlay`]; `centreWalk` a running stitch along the line and back; `edgeZigzag`
/// the right-hand edge run out and a zigzag between the edge runs back; `auto` one of them by
/// the width (see [`crate::column::AutoUnderlay`]). Other styles sew as `edge`.
/// `zigzag`: a Zigzag Stitch line, whose `edge` style is PE-DESIGN's own
/// ([`pe_zigzag_underlay`]); a satin line (beyond PE-DESIGN) keeps runs a quarter of its width
/// to either side.
pub fn line_underlay(
    path: &[Vec2],
    width: f64,
    pitch: f64,
    style: crate::model::UnderlayStyle,
    auto: &crate::column::AutoUnderlay,
    zigzag: bool,
) -> Run {
    use crate::model::UnderlayStyle as U;
    let offset = width / 4.0;
    let style = if style == U::Auto {
        auto.style(width / 10.0)
    } else {
        style
    };
    match style {
        U::CentreWalk => {
            let mut run = subdivide(path, pitch);
            let mut back = run.clone();
            back.reverse();
            extend(&mut run, back);
            run
        }
        U::EdgeZigzag => {
            // The right-hand run out, then back in a zigzag 2 mm apart between the two runs.
            let Some(mut run) = underlay_side(path, offset, offset, pitch) else {
                return Vec::new();
            };
            let w = Walker::new(path);
            let mut rev = w.slice(offset, w.length() - offset);
            rev.reverse();
            extend(
                &mut run,
                subdivide(&zigzag_rails(&rev, 2.0 * offset, 20.0), pitch),
            );
            run
        }
        _ if zigzag => pe_zigzag_underlay(path, width, pitch),
        _ => zigzag_underlay(path, offset, pitch),
    }
}

/// A zigzag needle point closer to the line than this share of its half width has folded over
/// it (#466): see [`unfold_zigzag`].
const FOLDED: f64 = 0.5;

/// Zigzag needle points that folded over the line (#466) moved onto it. On a curve tighter than
/// the zigzag's half width, and at a cusp such as a heart's top, a point offset `width / 2` to
/// the inner side lands near or past the line's other side: the stitches cross each other and
/// knot there. Every needle point of a zigzag otherwise lies half the width from the line, so
/// one nearer than [`FOLDED`] of that lies in a fold, and is sewn on the line instead (a half
/// stitch, as PE-DESIGN's Half Stitch pulls crowded inner points in).
pub fn unfold_zigzag(path: &[Vec2], run: &mut Run, width: f64) {
    let h = width / 2.0;
    if path.len() < 2 || h <= 0.0 {
        return;
    }
    // Segments by grid cell (a cell as wide as the reach), so each point looks only at the
    // segments near it: Design Center lines hold thousands of segments.
    let cell = (FOLDED * h).max(1.0);
    let key = |x: f64, y: f64| ((x / cell).floor() as i64, (y / cell).floor() as i64);
    let mut grid: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
    for (i, w) in path.windows(2).enumerate() {
        // The segment's cells, sampled every half cell along it.
        let n = ((w[0].distance(w[1]) / (cell / 2.0)).ceil() as usize).clamp(1, 100_000);
        for k in 0..=n {
            let q = w[0].lerp(w[1], k as f64 / n as f64);
            let cells = grid.entry(key(q.x, q.y)).or_default();
            if cells.last() != Some(&i) {
                cells.push(i);
            }
        }
    }
    let nearest = |p: Vec2| -> (f64, Vec2) {
        let mut best = (f64::INFINITY, p);
        let (cx, cy) = key(p.x, p.y);
        let mut near: Vec<usize> = (cx - 2..=cx + 2)
            .flat_map(|gx| (cy - 2..=cy + 2).map(move |gy| (gx, gy)))
            .filter_map(|k| grid.get(&k))
            .flatten()
            .copied()
            .collect();
        near.sort_unstable();
        near.dedup();
        for i in near {
            let (a, b) = (path[i], path[i + 1]);
            let ab = b - a;
            let len2 = ab.x * ab.x + ab.y * ab.y;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = a.lerp(b, t);
            let d = q.distance(p);
            if d < best.0 {
                best = (d, q);
            }
        }
        best
    };
    for p in run.iter_mut() {
        let (d, q) = nearest(*p);
        if d > 1e-6 && d < FOLDED * h {
            *p = q;
        }
    }
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
}

#[cfg(test)]
mod shaping_tests {
    use super::*;
    use crate::column::{
        AutoUnderlay, Compensation, CornerStyle, Corners, SatinShaping, ShortStitches,
    };
    use crate::model::UnderlayStyle;

    /// A line with one corner of `deg` degrees (interior) at (0, 0), legs `leg` long.
    fn vee(deg: f64, leg: f64) -> Vec<Vec2> {
        let a = (180.0 - deg).to_radians();
        vec![
            v2(-leg, 0.0),
            v2(0.0, 0.0),
            v2(a.dcos() * leg, a.dsin() * leg),
        ]
    }

    fn corners(style: CornerStyle) -> SatinShaping {
        SatinShaping {
            corners: Corners {
                style,
                ..Corners::default()
            },
            ..SatinShaping::default()
        }
    }

    #[test]
    fn a_4mm_zigzag_line_with_a_30_degree_corner_is_mitred_without_spikes() {
        let path = vee(30.0, 200.0);
        let plain = zigzag_rails(&path, 40.0, 2.0);
        let shaped = zigzag_shaped(&path, 40.0, 2.0, &corners(CornerStyle::Auto), None);
        let longest = |r: &Run| {
            r.windows(2)
                .map(|w| w[0].distance(w[1]))
                .fold(0.0, f64::max)
        };
        // Turned, the stitches at the tip run out along the mitre (about 15.5 mm here).
        assert!(longest(&plain) > 100.0, "{}", longest(&plain));
        // Shaped: no stitch longer than 8 mm, the tip still covered.
        assert!(longest(&shaped) <= 80.0, "{}", longest(&shaped));
        let tip = vertex_rungs(&path, 40.0)[1]
            .into_iter()
            .max_by(|a, b| a.x.total_cmp(&b.x))
            .expect("tip");
        let reach = shaped
            .iter()
            .map(|q| q.distance(tip))
            .fold(f64::INFINITY, f64::min);
        assert!(reach < 10.0, "the outer tip is reached: {reach}");
        // Default: PE-DESIGN's zigzag, unchanged.
        let same = zigzag_shaped(&path, 40.0, 2.0, &SatinShaping::default(), None);
        assert_eq!(
            same,
            zigzag_rails_sided(&path, [20.0, 20.0], 2.0),
            "plain shaping with turned corners is zigzag_rails"
        );
        assert_eq!(same, plain);
    }

    #[test]
    fn zigzag_compensation_moves_each_rail_out_by_its_side() {
        let path = vec![v2(0.0, 0.0), v2(200.0, 0.0)];
        let s = SatinShaping {
            compensation: Compensation {
                left: Some(0.5),
                percent: 5.0,
                ..Compensation::default()
            },
            ..SatinShaping::default()
        };
        let run = zigzag_shaped(&path, 40.0, 2.0, &s, None);
        let (lo, hi) = run
            .iter()
            .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), q| {
                (a.min(q.y), b.max(q.y))
            });
        // Left rail (−y) at 2 + 0.5 + 0.2 mm, right at 2 + 0.2 mm.
        assert!((lo + 27.0).abs() < 1e-9, "{lo}");
        assert!((hi - 22.0).abs() < 1e-9, "{hi}");
    }

    #[test]
    fn short_stitches_on_a_tight_zigzag_curve() {
        // A half circle of 3 mm radius, 4 mm wide.
        let path: Vec<Vec2> = (0..=48)
            .map(|i| {
                let a = std::f64::consts::PI * i as f64 / 48.0;
                v2(a.dcos() * 30.0, a.dsin() * 30.0)
            })
            .collect();
        let s = SatinShaping {
            short_stitches: ShortStitches {
                distance: 0.25,
                inset: 15.0,
            },
            ..SatinShaping::default()
        };
        let plain = zigzag_rails(&path, 40.0, 10.0 / 4.5);
        let short = zigzag_shaped(&path, 40.0, 10.0 / 4.5, &s, None);
        let inner = |r: &Run| r.iter().filter(|q| q.length() < 11.0).count();
        assert!(
            inner(&short) * 10 <= inner(&plain) * 7,
            "{} vs {}",
            inner(&short),
            inner(&plain)
        );
    }

    #[test]
    fn line_underlay_styles() {
        let path = vec![v2(0.0, 0.0), v2(300.0, 0.0)];
        let auto = AutoUnderlay::default();
        let pe = line_underlay(&path, 30.0, 20.0, UnderlayStyle::Edge, &auto, false);
        assert_eq!(pe, zigzag_underlay(&path, 7.5, 20.0));
        // Narrow: a centre walk along the line and back, ending where it began.
        let walk = line_underlay(&path, 15.0, 20.0, UnderlayStyle::Auto, &auto, false);
        assert!(walk.iter().all(|q| q.y == 0.0));
        assert_eq!(walk.first(), walk.last());
        // Medium: PE-DESIGN's edge runs; wide: an edge run out and a zigzag back.
        assert_eq!(
            line_underlay(&path, 30.0, 20.0, UnderlayStyle::Auto, &auto, false),
            pe
        );
        let wide = line_underlay(&path, 50.0, 20.0, UnderlayStyle::Auto, &auto, false);
        assert!(wide.iter().any(|q| q.y > 0.0) && wide.iter().any(|q| q.y < 0.0));
        let last = *wide.last().expect("end");
        assert!(last.x < 20.0, "back at the start: {last:?}");
    }

    /// #978: a closed outline's zigzag is spaced by its outer edge: a 40 mm square at 2 mm wide
    /// by 40 / 42 (4.5 line/mm → 4.725 along a side), a circle by L / (L + π w); an open path
    /// keeps its spacing (the caller's choice: only closed outlines come here).
    #[test]
    fn closed_zigzag_is_spaced_by_its_outer_edge() {
        let sq = vec![
            v2(0.0, 0.0),
            v2(400.0, 0.0),
            v2(400.0, 400.0),
            v2(0.0, 400.0),
        ];
        let s = closed_zigzag_spacing(&sq, 20.0, 10.0 / 4.5);
        assert!((10.0 / s - 4.5 * 42.0 / 40.0).abs() < 1e-9, "{}", 10.0 / s);
        // Either way round.
        let mut rev = sq.clone();
        rev.reverse();
        assert!((closed_zigzag_spacing(&rev, 20.0, 10.0 / 4.5) - s).abs() < 1e-9);
        let circle: Vec<Vec2> = (0..360)
            .map(|i| {
                let a = std::f64::consts::TAU * f64::from(i) / 360.0;
                v2(100.0 * a.cos(), 100.0 * a.sin())
            })
            .collect();
        let len: f64 = (0..360)
            .map(|i| circle[i].distance(circle[(i + 1) % 360]))
            .sum();
        let c = closed_zigzag_spacing(&circle, 30.0, 10.0);
        let want = 10.0 * len / (len + std::f64::consts::PI * 30.0);
        assert!((c - want).abs() < 0.01, "{c} vs {want}");
    }

    /// #964: a Zigzag Stitch line's under sewing as PE-DESIGN 11.50 sews it: runs `width / 2 −
    /// 0.5 mm` to either side (on the line at 1 mm), and from 4.0 mm four runs 1 mm apart.
    #[test]
    fn zigzag_line_under_sewing_follows_pe_design() {
        let path = vec![v2(0.0, 0.0), v2(300.0, 0.0)];
        let auto = AutoUnderlay::default();
        let offsets = |run: &Run| {
            let mut ys: Vec<i64> = run.iter().map(|q| q.y.round() as i64).collect();
            ys.sort_unstable();
            ys.dedup();
            ys
        };
        let at = |w: f64| line_underlay(&path, w, 20.0, UnderlayStyle::Edge, &auto, true);
        assert_eq!(offsets(&at(10.0)), vec![0]);
        assert_eq!(offsets(&at(20.0)), vec![-5, 5]);
        assert_eq!(offsets(&at(30.0)), vec![-10, 10]);
        assert_eq!(offsets(&at(39.0)), vec![-15, 15]);
        assert_eq!(offsets(&at(40.0)), vec![-15, -5, 5, 15]);
        // Four runs, each the other way from the one before: back where it began.
        let four = at(40.0);
        assert!(four.first().expect("start").x < 20.0 && four.last().expect("end").x < 20.0);
        assert_eq!(at(20.0), zigzag_underlay(&path, 5.0, 20.0));
    }

    fn sharp(button: u8, limit: f64) -> crate::model::SharpCorners {
        let mut c = crate::model::SharpCorners {
            button,
            ..Default::default()
        };
        c.set_limit(limit);
        c
    }

    /// How far `run` reaches past the corner of [`vee`] along its outward bisector, 0.1 mm.
    fn reach(deg: f64, run: &Run) -> f64 {
        let p = vee(deg, 400.0);
        let dp = (p[1] - p[0]) * (1.0 / p[0].distance(p[1]));
        let dn = (p[2] - p[1]) * (1.0 / p[1].distance(p[2]));
        let u = dp - dn;
        let u = u * (1.0 / u.length());
        run.iter()
            .map(|q| q.x * u.x + q.y * u.y)
            .fold(f64::MIN, f64::max)
    }

    /// #373: a plain satin line round a corner sharper than 67.5 degrees is cut as a zigzag
    /// line of its width with the same Sharp Corners and Miter Limit; a wider corner, a curved
    /// line and a corner style keep the satin line as before.
    #[test]
    fn a_satin_lines_sharp_corner_is_cut_as_a_zigzag_lines() {
        let spacing = 10.0 / 4.5;
        let mut line = crate::model::LineSewing::new(
            crate::model::LineType::Satin,
            openstitch_core::Thread::rgb(0),
        );
        line.width = 2.0;
        line.density = 4.5;
        for button in [1, 2] {
            line.sharp_corners = sharp(button, 1.0);
            let path = vee(30.0, 400.0);
            let satin = satin_line(&path, &line, false);
            let zigzag =
                zigzag_rails_cornered(&path, [10.0; 2], spacing, Some(&line.sharp_corners));
            assert_eq!(satin, zigzag, "button {button}");
            assert!((reach(30.0, &satin) - 10.0).abs() < 0.5);
        }
        let skeleton = |path: &[Vec2]| {
            crate::skeleton::satin(
                &crate::skeleton::Skeleton {
                    points: path.to_vec(),
                    widths: vec![2.0; path.len()],
                },
                spacing,
                0.0,
            )
        };
        let wide = vee(90.0, 400.0);
        assert_eq!(
            satin_line(&wide, &line, false),
            skeleton(&wide),
            "90 degrees"
        );
        let sharp_path = vee(30.0, 400.0);
        assert_eq!(
            satin_line(&sharp_path, &line, true),
            skeleton(&sharp_path),
            "curved"
        );
        line.satin_shaping.corners.style = crate::column::CornerStyle::Mitre;
        assert_ne!(
            satin_line(&sharp_path, &line, false),
            zigzag_rails_cornered(&sharp_path, [10.0; 2], spacing, Some(&line.sharp_corners)),
            "a corner style wins"
        );
    }

    #[test]
    fn the_miter_limit_cuts_a_sharp_zigzag_corner_as_pe_design() {
        // A 2 mm zigzag round a 30-degree corner: the full point is 3.9 mm out.
        let path = vee(30.0, 400.0);
        let tip = 10.0 / (15.0f64).to_radians().dsin();
        let spacing = 10.0 / 4.5;
        for button in [1, 2] {
            for (limit, want) in [(0.0, 0.0), (1.0, 10.0), (3.0, 30.0), (50.0, tip)] {
                let c = sharp(button, limit);
                let run = zigzag_rails_cornered(&path, [10.0; 2], spacing, Some(&c));
                let r = reach(30.0, &run);
                assert!(
                    (r - want).abs() < 0.5,
                    "button {button}, limit {limit} mm: reaches {r}, want {want}"
                );
                assert!(run.iter().all(|q| q.x.is_finite() && q.y.is_finite()));
            }
        }
        // Without the attribute the point reaches out in full, as before.
        let full = zigzag_rails_cornered(&path, [10.0; 2], spacing, None);
        assert_eq!(full, zigzag_rails(&path, 20.0, spacing));
        // (its needle points step past the tip, so they need not land on it).
        assert!(reach(30.0, &full) > 30.0);
    }

    #[test]
    fn button_2_sews_the_corner_with_more_stitches() {
        // miter-b-sc1-m3 / sc2-m3: 465 against 481 stitches.
        let path = vee(30.0, 400.0);
        let n = |b| zigzag_rails_cornered(&path, [10.0; 2], 10.0 / 4.5, Some(&sharp(b, 3.0))).len();
        assert!(n(2) > n(1), "{} vs {}", n(2), n(1));
    }

    #[test]
    fn corners_of_75_degrees_and_wider_keep_their_mitre() {
        // miter-a-*: the L of line-zigzag-default sews the same 312 stitches either way;
        // line-zigzag-miter-v75-b1/b2-m0: PE-DESIGN's two buttons sew the 75.3° V alike.
        for deg in [70.0, 75.3, 90.0, 120.0, 170.0] {
            let path = vee(deg, 400.0);
            let plain = zigzag_rails(&path, 20.0, 10.0 / 4.5);
            for c in [sharp(1, 0.0), sharp(2, 7.0)] {
                assert_eq!(
                    zigzag_rails_cornered(&path, [10.0; 2], 10.0 / 4.5, Some(&c)),
                    plain,
                    "{deg} degrees"
                );
            }
        }
    }

    #[test]
    fn corners_sharper_than_67_5_degrees_are_cut() {
        // line-zigzag-miter-v45/v60-b1/b2-m0: PE-DESIGN's two buttons sew 45° and 59.9° apart.
        for deg in [45.0, 59.9, 65.0] {
            let path = vee(deg, 400.0);
            let plain = zigzag_rails(&path, 20.0, 10.0 / 4.5);
            let cut = |b| zigzag_rails_cornered(&path, [10.0; 2], 10.0 / 4.5, Some(&sharp(b, 0.0)));
            assert_ne!(cut(1), plain, "{deg} degrees");
            assert_ne!(cut(1), cut(2), "{deg} degrees");
        }
    }

    #[test]
    fn a_reversing_or_short_legged_corner_stays_bounded() {
        for deg in [0.0, 0.5, 5.0, 60.0] {
            for leg in [3.0, 400.0] {
                let mut path = vee(deg, 400.0);
                path[2] = path[1] + (path[2] - path[1]) * (leg / 400.0);
                for c in [sharp(1, 2.0), sharp(2, 2.0), sharp(1, 50.0)] {
                    let run = zigzag_rails_cornered(&path, [10.0; 2], 10.0 / 4.5, Some(&c));
                    assert!(run.len() > 2, "{deg} degrees, leg {leg}");
                    let r = reach(deg, &run);
                    assert!(r.is_finite() && r <= c.limit() * 10.0 + 0.5, "{deg}: {r}");
                }
            }
        }
    }

    #[test]
    fn each_sharp_corners_button_keeps_its_own_miter_limit() {
        let mut c = crate::model::SharpCorners::default();
        assert_eq!((c.button, c.limit()), (1, 0.0));
        c.set_limit(3.0);
        c.button = 2;
        assert_eq!(c.limit(), 7.0);
        c.set_limit(99.0);
        assert_eq!(c.limit(), 50.0, "PE-DESIGN clamps to 50 mm");
        c.button = 1;
        assert_eq!(c.limit(), 3.0);
    }
}
