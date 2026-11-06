//! Sewing single-line (centre-line) letters and pixel letters (#530, #536).
//!
//! **Centre lines.** A letter's strokes meet where one starts or ends on another (the junctions
//! of an H's crossbar, the bowl of a b). [`route_strokes`] sews them one after another: from
//! where the needle is, it travels to the nearest unsewn stroke along strokes already sewn (so
//! every repeated stitch lies on a stroke and no travel shows), and only starts a new run (a
//! trim) for a piece no sewn stroke reaches, such as an i's dot.
//!
//! **Pixels.** [`pixel_runs`] walks a letter's lit pixels depth first, side neighbours before
//! diagonal ones, sewing each pixel once (a short run, a cross or a block) and travelling
//! centre to centre over lit pixels only; a letter's separate groups of pixels are separate runs.

use crate::fmath::Det;
use crate::generate::Run;
use crate::geometry::{Contour, Vec2};
use crate::text::PixelStyle;

/// Points closer than this (0.1 mm) are one point of the letter: where strokes meet.
const MEET: f64 = 0.5;

/// A piece of a routed letter.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// Travel along strokes already sewn.
    Travel(Vec<Vec2>),
    /// A stroke, in the direction it is sewn.
    Stroke(Vec<Vec2>),
}

fn dist_to_segment(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let ab = b - a;
    let len2 = ab.x * ab.x + ab.y * ab.y;
    if len2 <= 1e-18 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The routed runs of one letter whose strokes are `strokes` (open polylines, 0.1 mm), starting
/// at the stroke end nearest `from` (the needle's position after the letter before). Each run
/// is sewn after a trim; within a run, pieces join end to start.
pub fn route_strokes(strokes: &[Vec<Vec2>], from: Option<Vec2>) -> Vec<Vec<Piece>> {
    let mut strokes: Vec<Vec<Vec2>> = strokes.iter().filter(|s| s.len() >= 2).cloned().collect();
    if strokes.is_empty() {
        return Vec::new();
    }
    // A stroke end on another stroke's side becomes a point of that stroke: a junction.
    let ends: Vec<Vec2> = strokes
        .iter()
        .flat_map(|s| [s[0], s[s.len() - 1]])
        .collect();
    for (si, s) in strokes.iter_mut().enumerate() {
        for (ei, &e) in ends.iter().enumerate() {
            if ei / 2 == si {
                continue;
            }
            let mut k = 0;
            while k + 1 < s.len() {
                let (a, b) = (s[k], s[k + 1]);
                if e.distance(a) > MEET && e.distance(b) > MEET && dist_to_segment(e, a, b) <= MEET
                {
                    s.insert(k + 1, e);
                    break;
                }
                k += 1;
            }
        }
    }
    // Nodes: points within MEET merged.
    let mut nodes: Vec<Vec2> = Vec::new();
    let mut node_of = |p: Vec2| -> usize {
        if let Some(i) = nodes.iter().position(|q| q.distance(p) <= MEET) {
            i
        } else {
            nodes.push(p);
            nodes.len() - 1
        }
    };
    let paths: Vec<Vec<usize>> = strokes
        .iter()
        .map(|s| {
            let mut ids: Vec<usize> = s.iter().map(|&p| node_of(p)).collect();
            ids.dedup();
            ids
        })
        .collect();
    // Strokes split where they meet another stroke (or themselves), so the needle can enter a
    // stroke at a junction: from an H's crossbar into the right stem.
    let mut uses = vec![0usize; nodes.len()];
    for p in &paths {
        for &i in p {
            uses[i] += 1;
        }
        uses[p[0]] += 1;
        uses[p[p.len() - 1]] += 1;
    }
    let mut origin: Vec<usize> = Vec::new();
    let mut split: Vec<Vec<usize>> = Vec::new();
    for (si, p) in paths.iter().enumerate() {
        let mut piece = vec![p[0]];
        for (k, &i) in p.iter().enumerate().skip(1) {
            piece.push(i);
            if k + 1 < p.len() && uses[i] > 1 {
                split.push(std::mem::replace(&mut piece, vec![i]));
                origin.push(si);
            }
        }
        split.push(piece);
        origin.push(si);
    }
    let paths = split;
    let n = nodes.len();
    let mut sewn: Vec<Vec<(usize, f64)>> = vec![Vec::new(); n];
    let mut done: Vec<bool> = paths.iter().map(|p| p.len() < 2).collect();
    let mut runs: Vec<Vec<Piece>> = Vec::new();
    let mut at: Option<usize> = None;
    let mut sewing: Option<usize> = None;
    let mut last = from;
    let point_list = |ids: &[usize]| ids.iter().map(|&i| nodes[i]).collect::<Vec<_>>();
    while done.iter().any(|d| !d) {
        // Nearest unsewn stroke end reachable along sewn strokes.
        let mut next: Option<(usize, bool, Vec<usize>)> = None;
        if let Some(start) = at {
            let mut dist = vec![f64::INFINITY; n];
            let mut prev = vec![usize::MAX; n];
            let mut seen = vec![false; n];
            dist[start] = 0.0;
            loop {
                let Some(u) = (0..n)
                    .filter(|&i| !seen[i] && dist[i].is_finite())
                    .min_by(|&a, &b| dist[a].total_cmp(&dist[b]))
                else {
                    break;
                };
                seen[u] = true;
                for &(v, w) in &sewn[u] {
                    if dist[u] + w < dist[v] {
                        dist[v] = dist[u] + w;
                        prev[v] = u;
                    }
                }
            }
            // Nearest first; at the same distance, the stroke just sewn goes on.
            let mut best = (f64::INFINITY, true);
            for (si, p) in paths.iter().enumerate().filter(|(i, _)| !done[*i]) {
                for (rev, end) in [(false, p[0]), (true, p[p.len() - 1])] {
                    let key = (dist[end], Some(origin[si]) != sewing);
                    if dist[end].is_finite()
                        && (key.0 < best.0 - 1e-9 || (key.0 < best.0 + 1e-9 && key.1 < best.1))
                    {
                        best = key;
                        let mut way = vec![end];
                        while *way.last().expect("way") != start {
                            way.push(prev[*way.last().expect("way")]);
                        }
                        way.reverse();
                        next = Some((si, rev, way));
                    }
                }
            }
        }
        let (si, rev) = match next {
            Some((si, rev, way)) => {
                if way.len() > 1 {
                    runs.last_mut()
                        .expect("a run is open")
                        .push(Piece::Travel(point_list(&way)));
                }
                (si, rev)
            }
            None => {
                // A new run, from the stroke end nearest the needle (or the first stroke).
                let mut pick = (usize::MAX, false, f64::INFINITY);
                for (si, p) in paths.iter().enumerate().filter(|(i, _)| !done[*i]) {
                    for (rev, end) in [(false, p[0]), (true, p[p.len() - 1])] {
                        let d = last.map_or(0.0, |l| l.distance(nodes[end]));
                        if d < pick.2 - 1e-9 {
                            pick = (si, rev, d);
                        }
                    }
                }
                runs.push(Vec::new());
                (pick.0, pick.1)
            }
        };
        let mut ids = paths[si].clone();
        if rev {
            ids.reverse();
        }
        for w in ids.windows(2) {
            let d = nodes[w[0]].distance(nodes[w[1]]);
            sewn[w[0]].push((w[1], d));
            sewn[w[1]].push((w[0], d));
        }
        done[si] = true;
        sewing = Some(origin[si]);
        let end = *ids.last().expect("stroke");
        at = Some(end);
        last = Some(nodes[end]);
        runs.last_mut()
            .expect("a run is open")
            .push(Piece::Stroke(point_list(&ids)));
    }
    runs
}

/// `points` as running stitches at most `pitch` long: long segments split evenly, sharp turns
/// kept, and a curve's points skipped only while the stitch stays within 0.2 mm of it (curves
/// sew with shorter stitches, as they need to keep their shape).
pub fn resample(points: &[Vec2], pitch: f64) -> Vec<Vec2> {
    let mut dense: Vec<Vec2> = Vec::with_capacity(points.len() * 2);
    for w in points.windows(2) {
        let (a, b) = (w[0], w[1]);
        let parts = (a.distance(b) / pitch).ceil().max(1.0) as usize;
        if dense.is_empty() {
            dense.push(a);
        }
        for k in 1..=parts {
            dense.push(a + (b - a) * (k as f64 / parts as f64));
        }
    }
    if dense.len() < 2 {
        return points.to_vec();
    }
    let corner = |i: usize| {
        if i == 0 || i + 1 >= dense.len() {
            return true;
        }
        let (u, v) = (dense[i] - dense[i - 1], dense[i + 1] - dense[i]);
        let (lu, lv) = (u.x.dhypot(u.y), v.x.dhypot(v.y));
        lu > 1e-9 && lv > 1e-9 && (u.x * v.x + u.y * v.y) / (lu * lv) < 0.866
    };
    let mut out = vec![dense[0]];
    let mut i = 0;
    while i + 1 < dense.len() {
        let mut j = i + 1;
        while j + 1 < dense.len()
            && !corner(j)
            && dense[i].distance(dense[j + 1]) <= pitch + 1e-9
            && (i + 1..=j).all(|k| dist_to_segment(dense[k], dense[i], dense[j + 1]) <= 2.0)
        {
            j += 1;
        }
        out.push(dense[j]);
        i = j;
    }
    out
}

/// A pixel's corners: top left, top right, bottom right, bottom left (a Text Transform splits
/// each side into the same number of pieces).
fn corners(c: &Contour) -> Option<[Vec2; 4]> {
    let n = c.points.len();
    (c.closed && n >= 4 && n % 4 == 0).then(|| {
        [
            c.points[0],
            c.points[n / 4],
            c.points[n / 2],
            c.points[3 * n / 4],
        ]
    })
}

fn lerp(a: Vec2, b: Vec2, t: f64) -> Vec2 {
    a + (b - a) * t
}

/// The needle points that sew one pixel, from its centre back to its centre.
fn pixel_points(q: &[Vec2; 4], style: PixelStyle) -> Vec<Vec2> {
    let centre = (q[0] + q[1] + q[2] + q[3]) * 0.25;
    let inset = |p: Vec2| lerp(p, centre, 0.15);
    match style {
        PixelStyle::Run => vec![
            centre,
            inset(lerp(q[0], q[3], 0.5)),
            inset(lerp(q[1], q[2], 0.5)),
            centre,
        ],
        PixelStyle::Cross => vec![
            centre,
            inset(q[0]),
            inset(q[2]),
            centre,
            inset(q[1]),
            inset(q[3]),
            centre,
        ],
        PixelStyle::Block => {
            let height = q[0].distance(q[3]);
            // Rows 0.4 mm apart.
            let rows = ((height / 4.0).round() as usize).max(2);
            let mut out = vec![centre];
            for r in 0..rows {
                let t = (r as f64 + 0.5) / rows as f64;
                let (l, rt) = (inset(lerp(q[0], q[3], t)), inset(lerp(q[1], q[2], t)));
                if r % 2 == 0 {
                    out.extend([l, rt]);
                } else {
                    out.extend([rt, l]);
                }
            }
            out.push(centre);
            out
        }
    }
}

/// The runs that sew one pixel letter's `cells` (closed squares) in `style`, starting at the
/// pixel nearest `from`.
pub fn pixel_runs(cells: &[Contour], style: PixelStyle, from: Option<Vec2>) -> Vec<Run> {
    let quads: Vec<[Vec2; 4]> = cells.iter().filter_map(corners).collect();
    let n = quads.len();
    let centre = |i: usize| (quads[i][0] + quads[i][1] + quads[i][2] + quads[i][3]) * 0.25;
    let shared = |a: usize, b: usize| {
        quads[a]
            .iter()
            .filter(|p| quads[b].iter().any(|q| q.distance(**p) <= MEET))
            .count()
    };
    let neighbours: Vec<Vec<usize>> = (0..n)
        .map(|a| {
            let mut side: Vec<usize> = (0..n).filter(|&b| b != a && shared(a, b) >= 2).collect();
            side.extend((0..n).filter(|&b| b != a && shared(a, b) == 1));
            side
        })
        .collect();
    let mut seen = vec![false; n];
    let mut runs = Vec::new();
    let mut last = from;
    while let Some(start) = (0..n).filter(|&i| !seen[i]).min_by(|&a, &b| {
        let d = |i: usize| last.map_or(i as f64, |l| l.distance(centre(i)));
        d(a).total_cmp(&d(b))
    }) {
        let mut walk: Vec<Vec2> = Vec::new();
        let mut sewn_to = 0;
        // Depth first: (cell, next neighbour to try).
        let mut stack = vec![(start, 0usize)];
        seen[start] = true;
        walk.extend(pixel_points(&quads[start], style));
        sewn_to = sewn_to.max(walk.len());
        while let Some(top) = stack.last_mut() {
            let (cell, k) = *top;
            if let Some(&next) = neighbours[cell][k..].iter().find(|&&b| !seen[b]) {
                top.1 = neighbours[cell]
                    .iter()
                    .position(|&b| b == next)
                    .expect("nb")
                    + 1;
                seen[next] = true;
                walk.extend(pixel_points(&quads[next], style));
                sewn_to = walk.len();
                stack.push((next, 0));
            } else {
                stack.pop();
                if let Some(&(parent, _)) = stack.last() {
                    walk.push(centre(parent));
                }
            }
        }
        // The way back after the last pixel sewn is not needed.
        walk.truncate(sewn_to);
        walk.dedup_by(|a, b| a.distance(*b) < 1e-9);
        last = walk.last().copied();
        runs.push(walk);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    fn segment_on(p: Vec2, q: Vec2, strokes: &[Vec<Vec2>]) -> bool {
        let mid = lerp(p, q, 0.5);
        strokes.iter().any(|s| {
            s.windows(2)
                .any(|w| dist_to_segment(mid, w[0], w[1]) < 0.01)
        })
    }

    #[test]
    fn an_h_is_one_run_with_travel_on_its_strokes() {
        let h = vec![
            vec![v2(0.0, 0.0), v2(0.0, -100.0)],
            vec![v2(60.0, 0.0), v2(60.0, -100.0)],
            vec![v2(0.0, -50.0), v2(60.0, -50.0)],
        ];
        let runs = route_strokes(&h, None);
        assert_eq!(runs.len(), 1, "{runs:?}");
        for piece in &runs[0] {
            if let Piece::Travel(t) = piece {
                assert!(t.windows(2).all(|w| segment_on(w[0], w[1], &h)));
            }
        }
        // The stems are entered at the crossbar's ends: five pieces, every one sewn once.
        let sewn: f64 = runs[0]
            .iter()
            .filter_map(|p| match p {
                Piece::Stroke(s) => Some(s.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>()),
                Piece::Travel(_) => None,
            })
            .sum();
        assert!((sewn - 260.0).abs() < 1e-9, "{sewn}");
    }

    #[test]
    fn an_i_dot_is_its_own_run() {
        let i = vec![
            vec![v2(0.0, 0.0), v2(0.0, -70.0)],
            vec![v2(0.0, -88.0), v2(0.0, -96.0)],
        ];
        assert_eq!(route_strokes(&i, None).len(), 2);
    }

    #[test]
    fn resampling_keeps_the_pitch_and_the_corners() {
        let r = resample(&[v2(0.0, 0.0), v2(100.0, 0.0), v2(100.0, 35.0)], 15.0);
        assert!(r.windows(2).all(|w| w[0].distance(w[1]) <= 15.0 + 1e-9));
        assert!(r.contains(&v2(100.0, 0.0)));
        assert_eq!(*r.last().expect("end"), v2(100.0, 35.0));
    }

    #[test]
    fn pixels_are_walked_over_lit_pixels_only() {
        let sq = |x: f64, y: f64| {
            Contour::new(
                vec![
                    v2(x, y),
                    v2(x + 20.0, y),
                    v2(x + 20.0, y + 20.0),
                    v2(x, y + 20.0),
                ],
                true,
            )
        };
        // An L of three pixels and a lone pixel apart.
        let cells = vec![sq(0.0, 0.0), sq(0.0, 20.0), sq(20.0, 20.0), sq(80.0, 0.0)];
        for style in [PixelStyle::Run, PixelStyle::Cross, PixelStyle::Block] {
            let runs = pixel_runs(&cells, style, None);
            assert_eq!(runs.len(), 2, "{style:?}");
            for run in &runs {
                // Every needle point lies inside a lit pixel.
                assert!(run.iter().all(|p| cells.iter().any(|c| {
                    let q = corners(c).expect("square");
                    p.x >= q[0].x - 1e-9
                        && p.x <= q[1].x + 1e-9
                        && p.y >= q[0].y - 1e-9
                        && p.y <= q[3].y + 1e-9
                })));
            }
        }
    }
}
