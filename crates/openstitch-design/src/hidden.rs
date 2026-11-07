//! Stitching hidden under later objects, beyond PE-DESIGN and off by default:
//! - leave out a region's stitches that later fills and satins cover, keeping an overlap under
//!   their edges, without cutting the shape (#511);
//! - travel between objects of one thread as running stitches hidden under fills sewn later,
//!   instead of a jump and trim (#545).
//!
//! Both only change what the compiler sews; the objects stay as drawn.

use crate::boolean::{self, BoolOp};
use crate::geometry::{Contour, Vec2};
use crate::model::{Design, DesignObject, ObjectKind, RegionType};
use crate::paths;

/// Model units (0.1 mm) per mm.
const MM: f64 = 10.0;
/// Travel stays this far inside the fills that hide it (0.1 mm).
pub const TRAVEL_INSET: f64 = 5.0;
/// Running stitch length of hidden travel and run connectors (0.1 mm).
pub const TRAVEL_STITCH: f64 = 20.0;
/// A run connector at least this long (0.1 mm) is reported: it shows on the fabric.
pub const LONG_CONNECTOR: f64 = 150.0;

/// Whether `o` covers what is sewn before it: a visible shape or text with an opaque region.
fn opaque(o: &DesignObject) -> bool {
    o.visible
        && o.is_outline()
        && o.has_closed_contour()
        && o.region.as_ref().is_some_and(|r| {
            matches!(
                r.kind,
                RegionType::Fill | RegionType::Satin | RegionType::ProgrammableFill
            )
        })
}

fn closed(o: &DesignObject) -> Vec<Contour> {
    o.contours.iter().filter(|c| c.closed).cloned().collect()
}

/// The union of the opaque objects among `objects`, shrunk by `inset` (0.1 mm). Empty when
/// none.
pub fn covered<'a>(
    objects: impl IntoIterator<Item = &'a DesignObject>,
    inset: f64,
) -> Vec<Contour> {
    let parts: Vec<Vec<Contour>> = objects
        .into_iter()
        .filter(|o| opaque(o))
        .map(closed)
        .collect();
    if parts.is_empty() {
        return Vec::new();
    }
    let union = boolean::contours(&boolean::union_all(parts.iter().map(|p| &p[..])));
    if inset > 0.0 {
        boolean::contours(&boolean::offset(&union, -inset, false))
    } else {
        union
    }
}

/// For each object, the opaque objects from it to the end of the sewing order, united, and the
/// same shrunk by [`TRAVEL_INSET`]: what hides the move into it (#545). Built from the end, one
/// union per object.
pub fn later_cover(objects: &[DesignObject]) -> Vec<Cover> {
    let mut acc: Vec<Contour> = Vec::new();
    let mut out = vec![Cover::default(); objects.len()];
    for (k, o) in objects.iter().enumerate().rev() {
        if opaque(o) {
            acc = boolean::contours(&boolean::combine(&acc, &closed(o), BoolOp::Union));
            out[k] = Cover {
                inset: boolean::contours(&boolean::offset(&acc, -TRAVEL_INSET, false)),
                area: acc.clone(),
            };
        } else if let Some(next) = out.get(k + 1).cloned() {
            out[k] = next;
        }
    }
    out
}

/// What hides travel: the fills' area, and the same 0.5 mm in from its edges.
#[derive(Clone, Debug, Default)]
pub struct Cover {
    pub area: Vec<Contour>,
    pub inset: Vec<Contour>,
}

/// The overlap (mm) object `index` keeps under later objects when its hidden stitching is left
/// out: its own setting, else the design's for shapes (text only by its own setting). Satin
/// regions are not clipped: their columns follow the whole shape.
pub fn hide_overlap(d: &Design, index: usize) -> Option<f64> {
    let o = d.objects.get(index)?;
    let r = o.region.as_ref()?;
    if r.kind == RegionType::Satin || !o.has_closed_contour() {
        return None;
    }
    o.hide_under.or(match o.kind {
        ObjectKind::Shape => d.production.remove_hidden,
        _ => None,
    })
}

/// The area object `index`'s region is sewn over when its hidden stitching is left out: its
/// outline minus the later opaque objects shrunk by the overlap. `None` when nothing later
/// covers it (sewn as drawn).
pub fn visible_area(d: &Design, index: usize) -> Option<Vec<Contour>> {
    let overlap = hide_overlap(d, index)? * MM;
    let cover = covered(d.objects.iter().skip(index + 1), overlap);
    if cover.is_empty() {
        return None;
    }
    let own = closed(&d.objects[index]);
    let hidden = boolean::combine(&own, &cover, BoolOp::Intersection);
    if hidden.iter().map(|r| boolean::region_area(r)).sum::<f64>() < 1.0 {
        return None;
    }
    Some(boolean::contours(&boolean::combine(
        &own,
        &cover,
        BoolOp::Difference,
    )))
}

/// Whether the segment `a`–`b` lies inside `area` (even-odd contours).
fn inside(a: Vec2, b: Vec2, area: &[Contour]) -> bool {
    let pieces = paths::clip(&[a, b], area, &[]);
    pieces.len() == 1
        && pieces[0].len() >= 2
        && pieces[0][0].distance(a) < 0.5
        && pieces[0][pieces[0].len() - 1].distance(b) < 0.5
}

/// The leg `a`–`b` of hidden travel: inside the fills (`cover`), and inside `inset` (the fills
/// shrunk by [`TRAVEL_INSET`]) except within 1 mm of an end that is not a corner of the path,
/// where it leaves the previous object or meets the next one's first stitch on its edge.
fn leg(a: Vec2, b: Vec2, cover: &[Contour], inset: &[Contour], free_a: bool, free_b: bool) -> bool {
    let len = a.distance(b);
    let end = 10.0_f64.min(len / 2.0);
    let (ta, tb) = (
        if free_a { end / len } else { 0.0 },
        if free_b { end / len } else { 0.0 },
    );
    inside(a, b, cover) && inside(a.lerp(b, ta), a.lerp(b, 1.0 - tb), inset)
}

/// Corners of `inset` the visibility graph routes through at most (a curved fill's flattened
/// outline is thinned to about this many): the graph checks every pair, so it stays small.
const GRAPH_CORNERS: usize = 160;

/// A path from `a` to `b` hidden under `cover` (the fills sewn later) and 0.5 mm in from its
/// edges (`inset`, [`later_cover`]): straight, else the shortest way through corners of
/// `inset` (a visibility graph, #545: an L round one bend, a U or a zigzag round several);
/// `None` when no way stays inside.
pub fn path_inside(a: Vec2, b: Vec2, cover: &[Contour], inset: &[Contour]) -> Option<Vec<Vec2>> {
    if inset.is_empty() || a.distance(b) < 1e-6 {
        return None;
    }
    if leg(a, b, cover, inset, true, true) {
        return Some(vec![a, b]);
    }
    let mut corners: Vec<Vec2> = inset.iter().flat_map(Contour::outline_points).collect();
    if corners.len() > GRAPH_CORNERS {
        let step = corners.len().div_ceil(GRAPH_CORNERS);
        corners = corners.into_iter().step_by(step).collect();
    }
    // Dijkstra from `a` over the corners; an edge is a hidden leg. Node 0 is `a`, 1 is `b`.
    let nodes: Vec<Vec2> = [a, b].into_iter().chain(corners).collect();
    let n = nodes.len();
    let mut dist = vec![f64::INFINITY; n];
    let mut prev = vec![usize::MAX; n];
    let mut done = vec![false; n];
    dist[0] = 0.0;
    loop {
        let u = (0..n)
            .filter(|&k| !done[k] && dist[k].is_finite())
            .min_by(|&x, &y| dist[x].total_cmp(&dist[y]))?;
        if u == 1 {
            break;
        }
        done[u] = true;
        for v in 1..n {
            if done[v] || v == u {
                continue;
            }
            let d = dist[u] + nodes[u].distance(nodes[v]);
            if d < dist[v] && leg(nodes[u], nodes[v], cover, inset, u == 0, v == 1) {
                dist[v] = d;
                prev[v] = u;
            }
        }
    }
    let mut path = vec![b];
    let mut k = 1;
    while prev[k] != usize::MAX {
        k = prev[k];
        path.push(nodes[k]);
    }
    path.reverse();
    (path[0] == a).then_some(path)
}

/// A point this close (0.1 mm) to a sewn line is on it: where travel joins and leaves the line,
/// and where two lines meet.
const ON_LINE: f64 = 5.0;
/// Travel along sewn lines is at most this many times the straight way.
const ALONG_DETOUR: f64 = 3.0;

/// A path from `a` to `b` along running lines already sewn in the same thread (`lines`, their
/// needle points), as filum's Route joins running stitches (#545): the travel lies on the line
/// and reads as part of it. `a` and `b` must each lie on a line (within 0.5 mm); lines that
/// meet (within 0.5 mm) join. `None` when no such way is at most three times the straight one.
pub fn path_along(a: Vec2, b: Vec2, lines: &[Vec<Vec2>]) -> Option<Vec<Vec2>> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashMap};
    let straight = a.distance(b);
    if straight < 1e-6 || lines.is_empty() {
        return None;
    }
    let mut nodes: Vec<Vec2> = Vec::new();
    let mut adj: Vec<Vec<(usize, f64)>> = Vec::new();
    let edge = |adj: &mut Vec<Vec<(usize, f64)>>, nodes: &[Vec2], i: usize, j: usize| {
        let d = nodes[i].distance(nodes[j]);
        adj[i].push((j, d));
        adj[j].push((i, d));
    };
    let mut segments: Vec<(usize, usize)> = Vec::new();
    for line in lines.iter().filter(|l| l.len() >= 2) {
        let first = nodes.len();
        for &p in line {
            nodes.push(p);
            adj.push(Vec::new());
        }
        for k in first..nodes.len() - 1 {
            edge(&mut adj, &nodes, k, k + 1);
            segments.push((k, k + 1));
        }
    }
    // Lines meet where needle points of different lines lie within ON_LINE: a grid of cells
    // ON_LINE wide finds them.
    let cell = |p: Vec2| {
        (
            (p.x / ON_LINE).floor() as i64,
            (p.y / ON_LINE).floor() as i64,
        )
    };
    let mut grid: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (k, &p) in nodes.iter().enumerate() {
        grid.entry(cell(p)).or_default().push(k);
    }
    for k in 0..nodes.len() {
        let (cx, cy) = cell(nodes[k]);
        for dx in -1..=1 {
            for dy in -1..=1 {
                for &j in grid.get(&(cx + dx, cy + dy)).into_iter().flatten() {
                    if j > k + 1 && nodes[j].distance(nodes[k]) <= ON_LINE {
                        edge(&mut adj, &nodes, k, j);
                    }
                }
            }
        }
    }
    // `a` and `b` join the graph at the nearest point of a segment within ON_LINE.
    let attach = |adj: &mut Vec<Vec<(usize, f64)>>, nodes: &mut Vec<Vec2>, p: Vec2| {
        let (k, d, at) = segments
            .iter()
            .map(|&(i, j)| {
                let (s, t) = (nodes[i], nodes[j]);
                let dot = |u: Vec2, v: Vec2| u.x * v.x + u.y * v.y;
                let len2 = dot(t - s, t - s);
                let f = if len2 > 0.0 {
                    (dot(p - s, t - s) / len2).clamp(0.0, 1.0)
                } else {
                    0.0
                };
                let q = s.lerp(t, f);
                ((i, j), q.distance(p), q)
            })
            .min_by(|x, y| x.1.total_cmp(&y.1))?;
        if d > ON_LINE {
            return None;
        }
        let n = nodes.len();
        nodes.push(at);
        adj.push(Vec::new());
        edge(adj, nodes, n, k.0);
        edge(adj, nodes, n, k.1);
        Some(n)
    };
    let start = attach(&mut adj, &mut nodes, a)?;
    let end = attach(&mut adj, &mut nodes, b)?;
    let limit = ALONG_DETOUR * straight + 2.0 * ON_LINE;
    let key = |d: f64| (d * 1000.0) as u64;
    let mut dist = vec![f64::INFINITY; nodes.len()];
    let mut prev = vec![usize::MAX; nodes.len()];
    let mut heap = BinaryHeap::new();
    dist[start] = 0.0;
    heap.push(Reverse((0u64, start)));
    while let Some(Reverse((_, u))) = heap.pop() {
        if u == end {
            break;
        }
        for &(v, w) in &adj[u] {
            let d = dist[u] + w;
            if d < dist[v] && d <= limit {
                dist[v] = d;
                prev[v] = u;
                heap.push(Reverse((key(d), v)));
            }
        }
    }
    if !dist[end].is_finite() {
        return None;
    }
    let mut path = vec![b];
    let mut k = end;
    while prev[k] != usize::MAX {
        k = prev[k];
        path.push(nodes[k]);
    }
    path.push(a);
    path.reverse();
    path.dedup_by(|x, y| x.distance(*y) < 1e-6);
    Some(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    #[test]
    fn an_l_shaped_area_is_travelled_round_its_bend() {
        // An L: the left bar and the top bar.
        let l = Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(100.0, 0.0),
                v2(100.0, 300.0),
                v2(300.0, 300.0),
                v2(300.0, 400.0),
                v2(0.0, 400.0),
            ],
            true,
        );
        let a = v2(50.0, 20.0);
        let b = v2(280.0, 350.0);
        let cover = std::slice::from_ref(&l);
        let inset = boolean::contours(&boolean::offset(cover, -TRAVEL_INSET, false));
        let p = path_inside(a, b, cover, &inset).expect("a path");
        assert_eq!(p.len(), 3);
        assert!(
            path_inside(a, v2(250.0, 100.0), cover, &inset).is_none(),
            "outside"
        );
    }

    #[test]
    fn travel_runs_along_sewn_lines_that_meet() {
        // #545: a T of running lines, the bar from (0, 0) to (400, 0) and the stem down from
        // (200, 0) to (200, 300). From the bar's right end to the stem's foot the way runs back
        // along the bar to the junction, then down the stem; the bar's left end off any line
        // is not reached.
        let bar: Vec<Vec2> = (0..=20).map(|k| v2(20.0 * k as f64, 0.0)).collect();
        let stem: Vec<Vec2> = (0..=15).map(|k| v2(200.0, 2.0 + 20.0 * k as f64)).collect();
        let lines = [bar, stem];
        let p = path_along(v2(400.0, 0.0), v2(200.0, 302.0), &lines).expect("along the T");
        let length: f64 = p.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!((length - 502.0).abs() < 10.0, "{length}: {p:?}");
        assert!(
            p.iter()
                .all(|q| q.y.abs() < 1.0 || (q.x - 200.0).abs() < 1.0),
            "{p:?}"
        );
        assert!(
            path_along(v2(400.0, 0.0), v2(300.0, 200.0), &lines).is_none(),
            "off the lines"
        );
        // A way much longer than the straight one is not taken: from one end of the bar to a
        // point just below it, round a long loop, would be.
        let hook: Vec<Vec2> = vec![
            v2(0.0, 0.0),
            v2(0.0, 400.0),
            v2(30.0, 400.0),
            v2(30.0, 10.0),
        ];
        assert!(path_along(v2(0.0, 0.0), v2(30.0, 10.0), &[hook]).is_none());
    }

    #[test]
    fn a_u_shaped_area_is_travelled_round_both_bends() {
        // #545: a U, 10 mm bars, open at the top: from the top of the left bar to the top of
        // the right one takes two bends, which one corner could not.
        let u = Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(100.0, 0.0),
                v2(100.0, 300.0),
                v2(300.0, 300.0),
                v2(300.0, 0.0),
                v2(400.0, 0.0),
                v2(400.0, 400.0),
                v2(0.0, 400.0),
            ],
            true,
        );
        let cover = std::slice::from_ref(&u);
        let inset = boolean::contours(&boolean::offset(cover, -TRAVEL_INSET, false));
        let (a, b) = (v2(50.0, 20.0), v2(350.0, 20.0));
        let p = path_inside(a, b, cover, &inset).expect("round the U");
        assert!(p.len() >= 4, "two bends at least: {p:?}");
        assert_eq!((p[0], p[p.len() - 1]), (a, b));
        // Every leg stays hidden, and the way is not much longer than down, across and up.
        let length: f64 = p.windows(2).map(|w| w[0].distance(w[1])).sum();
        assert!(length < 1.1 * (330.0 + 300.0 + 330.0), "{length}");
        for w in p.windows(2) {
            assert!(inside(w[0], w[1], cover), "{w:?}");
        }
    }
}
