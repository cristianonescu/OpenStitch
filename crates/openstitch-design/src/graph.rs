//! Auto-routing of running lines (#546, roadmap B4, beyond PE-DESIGN): lines that touch or
//! cross are joined into a graph and each connected piece is sewn as one walk, doubling back
//! only where it must (route inspection, the "Chinese postman" rule: the doubled length is the
//! minimum matching of the odd vertices), or with every line sewn twice (Redwork).
//! See `docs/research/beyond/auto-route-and-branching.md` §2-4.

use crate::geometry::Vec2;
use std::collections::BinaryHeap;

/// An edge of the line graph: a piece of a line between two vertices.
#[derive(Clone, Debug)]
struct Edge {
    a: usize,
    b: usize,
    points: Vec<Vec2>,
    length: f64,
}

/// Most odd vertices matched exactly (bitmask dynamic programming); above, greedy nearest
/// pairs.
const EXACT_MATCH: usize = 16;

/// The routed walks over `lines` (polylines, 0.1 mm; a closed line repeats its first point):
/// one walk per connected piece, in nearest-next order from `start`. Ends within `join` of
/// another line, and crossings, are vertices. `redwork`: every line sewn twice, each walk
/// closed. Otherwise every line once plus the shortest doubling that makes one walk possible.
pub fn route(lines: &[Vec<Vec2>], join: f64, redwork: bool, start: Option<Vec2>) -> Vec<Vec<Vec2>> {
    let (vertices, mut edges) = build(lines, join);
    if edges.is_empty() {
        return Vec::new();
    }
    let n = vertices.len();
    // Connected pieces.
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(p: &mut [usize], x: usize) -> usize {
        let mut r = x;
        while p[r] != r {
            r = p[r];
        }
        let mut y = x;
        while p[y] != r {
            let next = p[y];
            p[y] = r;
            y = next;
        }
        r
    }
    for e in &edges {
        let (ra, rb) = (find(&mut parent, e.a), find(&mut parent, e.b));
        if ra != rb {
            parent[ra] = rb;
        }
    }
    let mut pieces: Vec<Vec<usize>> = Vec::new();
    let mut root_of: Vec<Option<usize>> = vec![None; n];
    for (i, e) in edges.iter().enumerate() {
        let r = find(&mut parent, e.a);
        let k = *root_of[r].get_or_insert_with(|| {
            pieces.push(Vec::new());
            pieces.len() - 1
        });
        pieces[k].push(i);
    }
    if redwork {
        let twins: Vec<Edge> = edges
            .iter()
            .map(|e| Edge {
                a: e.b,
                b: e.a,
                points: e.points.iter().rev().copied().collect(),
                length: e.length,
            })
            .collect();
        let base = edges.len();
        edges.extend(twins);
        for piece in &mut pieces {
            let extra: Vec<usize> = piece.iter().map(|&i| i + base).collect();
            piece.extend(extra);
        }
    } else {
        for piece in &mut pieces {
            let added = doubling(&vertices, &edges, piece);
            for e in added {
                piece.push(edges.len());
                edges.push(e);
            }
        }
    }
    // Walk the pieces nearest-next from the start.
    let mut out = Vec::new();
    let mut at = start;
    let mut left: Vec<Vec<usize>> = pieces;
    while !left.is_empty() {
        let (k, v0) = left
            .iter()
            .enumerate()
            .map(|(k, piece)| (k, start_vertex(&vertices, &edges, piece, at)))
            .min_by(|x, y| {
                let d = |v: usize| at.map_or(0.0, |p| vertices[v].distance(p));
                d(x.1).total_cmp(&d(y.1))
            })
            .expect("non-empty");
        let piece = left.remove(k);
        let walk = hierholzer(&vertices, &edges, &piece, v0);
        if let Some(&last) = walk.last() {
            at = Some(last);
        }
        out.push(walk);
    }
    out
}

/// Vertices (clustered within `join`) and the line pieces between them.
fn build(lines: &[Vec<Vec2>], join: f64) -> (Vec<Vec2>, Vec<Edge>) {
    // Split parameters per line: (segment index, t) where a vertex lies.
    let mut cuts: Vec<Vec<(usize, f64, Vec2)>> = lines
        .iter()
        .map(|l| {
            let mut c = Vec::new();
            if let (Some(&f), Some(&e)) = (l.first(), l.last()) {
                c.push((0, 0.0, f));
                c.push((l.len().saturating_sub(2), 1.0, e));
            }
            c
        })
        .collect();
    for i in 0..lines.len() {
        for j in i..lines.len() {
            for (si, s) in lines[i].windows(2).enumerate() {
                for (sj, t) in lines[j].windows(2).enumerate() {
                    if i == j && sj <= si + 1 {
                        continue;
                    }
                    if let Some((u, v, p)) = crossing(s[0], s[1], t[0], t[1]) {
                        cuts[i].push((si, u, p));
                        cuts[j].push((sj, v, p));
                    }
                }
            }
        }
    }
    // An end within `join` of another line (not at its own end) meets it there.
    for i in 0..lines.len() {
        for end in [lines[i].first(), lines[i].last()].into_iter().flatten() {
            for (j, l) in lines.iter().enumerate() {
                for (sj, t) in l.windows(2).enumerate() {
                    let (u, q) = project(*end, t[0], t[1]);
                    if q.distance(*end) <= join && (i != j || (u > 0.0 && u < 1.0)) {
                        cuts[j].push((sj, u, q));
                    }
                }
            }
        }
    }
    // Cluster the cut points into vertices.
    let mut vertices: Vec<Vec2> = Vec::new();
    let mut vertex_of = |p: Vec2| -> usize {
        if let Some(k) = vertices.iter().position(|v| v.distance(p) <= join) {
            return k;
        }
        vertices.push(p);
        vertices.len() - 1
    };
    let mut edges = Vec::new();
    for (l, c) in lines.iter().zip(&mut cuts) {
        if l.len() < 2 {
            continue;
        }
        c.sort_by(|x, y| (x.0, x.1).partial_cmp(&(y.0, y.1)).expect("finite"));
        let mut piece = vec![c[0].2];
        let mut from = vertex_of(c[0].2);
        let mut seg = c[0].0;
        for &(s, _, p) in &c[1..] {
            while seg < s {
                seg += 1;
                piece.push(l[seg]);
            }
            piece.push(p);
            let to = vertex_of(p);
            let length: f64 = piece.windows(2).map(|w| w[0].distance(w[1])).sum();
            if to != from || length > join {
                edges.push(Edge {
                    a: from,
                    b: to,
                    points: std::mem::replace(&mut piece, vec![p]),
                    length,
                });
                from = to;
            } else {
                piece = vec![p];
            }
        }
    }
    (vertices, edges)
}

/// Where segments `a`-`b` and `c`-`d` cross: their parameters and the point.
fn crossing(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> Option<(f64, f64, Vec2)> {
    let (r, s) = (b - a, d - c);
    let den = r.x * s.y - r.y * s.x;
    if den.abs() < 1e-12 {
        return None;
    }
    let (qx, qy) = (c.x - a.x, c.y - a.y);
    let t = (qx * s.y - qy * s.x) / den;
    let u = (qx * r.y - qy * r.x) / den;
    let inner = |x: f64| x > 1e-9 && x < 1.0 - 1e-9;
    (inner(t) && inner(u)).then(|| (t, u, a.lerp(b, t)))
}

/// The parameter and point of segment `a`-`b` nearest to `p`.
fn project(p: Vec2, a: Vec2, b: Vec2) -> (f64, Vec2) {
    let ab = b - a;
    let l2 = ab.x * ab.x + ab.y * ab.y;
    if l2 == 0.0 {
        return (0.0, a);
    }
    let t = (((p.x - a.x) * ab.x + (p.y - a.y) * ab.y) / l2).clamp(0.0, 1.0);
    (t, a.lerp(b, t))
}

/// Shortest paths from `from` over `piece`: distance and the edge into each vertex.
fn dijkstra(
    n: usize,
    edges: &[Edge],
    piece: &[usize],
    from: usize,
) -> (Vec<f64>, Vec<Option<usize>>) {
    #[derive(PartialEq)]
    struct Item(f64, usize);
    impl Eq for Item {}
    impl PartialOrd for Item {
        fn partial_cmp(&self, o: &Self) -> Option<std::cmp::Ordering> {
            Some(self.cmp(o))
        }
    }
    impl Ord for Item {
        fn cmp(&self, o: &Self) -> std::cmp::Ordering {
            o.0.total_cmp(&self.0).then(o.1.cmp(&self.1))
        }
    }
    let mut dist = vec![f64::INFINITY; n];
    let mut via = vec![None; n];
    dist[from] = 0.0;
    let mut heap = BinaryHeap::from([Item(0.0, from)]);
    while let Some(Item(d, v)) = heap.pop() {
        if d > dist[v] {
            continue;
        }
        for &i in piece {
            let e = &edges[i];
            let w = if e.a == v {
                e.b
            } else if e.b == v {
                e.a
            } else {
                continue;
            };
            if d + e.length < dist[w] {
                dist[w] = d + e.length;
                via[w] = Some(i);
                heap.push(Item(dist[w], w));
            }
        }
    }
    (dist, via)
}

/// The edges to sew twice so `piece` has one walk: the shortest paths between the odd
/// vertices, paired by a minimum matching.
fn doubling(vertices: &[Vec2], edges: &[Edge], piece: &[usize]) -> Vec<Edge> {
    let n = vertices.len();
    let mut degree = vec![0usize; n];
    for &i in piece {
        degree[edges[i].a] += 1;
        degree[edges[i].b] += 1;
    }
    let odd: Vec<usize> = (0..n).filter(|&v| degree[v] % 2 == 1).collect();
    // An Euler path needs no doubling with two odd vertices.
    if odd.len() <= 2 {
        return Vec::new();
    }
    let paths: Vec<(Vec<f64>, Vec<Option<usize>>)> =
        odd.iter().map(|&v| dijkstra(n, edges, piece, v)).collect();
    let cost = |i: usize, j: usize| paths[i].0[odd[j]];
    // Leave the two odd vertices whose matching would cost most unmatched: they are the walk's
    // ends. Exactly, by trying every pair to leave out when small, else greedily.
    let pairs = if odd.len() <= EXACT_MATCH {
        let mut best: Option<(f64, Vec<(usize, usize)>)> = None;
        for x in 0..odd.len() {
            for y in x + 1..odd.len() {
                let rest: Vec<usize> = (0..odd.len()).filter(|&k| k != x && k != y).collect();
                let (c, m) = exact_matching(&rest, &cost);
                if best.as_ref().is_none_or(|b| c < b.0) {
                    best = Some((c, m));
                }
            }
        }
        best.map(|b| b.1).unwrap_or_default()
    } else {
        greedy_matching(odd.len(), &cost)
    };
    let mut out = Vec::new();
    for (i, j) in pairs {
        // Walk back from odd[j] to odd[i] along the shortest-path tree of odd[i].
        let mut v = odd[j];
        while v != odd[i] {
            let Some(e) = paths[i].1[v] else { break };
            let edge = &edges[e];
            out.push(edge.clone());
            v = if edge.a == v { edge.b } else { edge.a };
        }
    }
    out
}

/// Minimum perfect matching of `items` (an even count) under `cost`, by bitmask dynamic
/// programming.
fn exact_matching(
    items: &[usize],
    cost: &dyn Fn(usize, usize) -> f64,
) -> (f64, Vec<(usize, usize)>) {
    let m = items.len();
    let full = (1usize << m) - 1;
    let mut best = vec![f64::INFINITY; full + 1];
    let mut choice = vec![(0usize, 0usize); full + 1];
    best[0] = 0.0;
    for mask in 0..=full {
        if best[mask].is_infinite() || mask == full {
            continue;
        }
        let i = (0..m).find(|&k| mask & (1 << k) == 0).expect("free item");
        for j in i + 1..m {
            if mask & (1 << j) != 0 {
                continue;
            }
            let next = mask | (1 << i) | (1 << j);
            let c = best[mask] + cost(items[i], items[j]);
            if c < best[next] {
                best[next] = c;
                choice[next] = (i, j);
            }
        }
    }
    let mut pairs = Vec::new();
    let mut mask = full;
    while mask != 0 {
        let (i, j) = choice[mask];
        pairs.push((items[i], items[j]));
        mask &= !((1 << i) | (1 << j));
    }
    (best[full], pairs)
}

/// Nearest pairs first, leaving the last two unmatched (the walk's ends).
fn greedy_matching(k: usize, cost: &dyn Fn(usize, usize) -> f64) -> Vec<(usize, usize)> {
    let mut all: Vec<(f64, usize, usize)> = (0..k)
        .flat_map(|i| (i + 1..k).map(move |j| (i, j)))
        .map(|(i, j)| (cost(i, j), i, j))
        .collect();
    all.sort_by(|x, y| x.0.total_cmp(&y.0));
    let mut used = vec![false; k];
    let mut pairs = Vec::new();
    for (_, i, j) in all {
        if pairs.len() + 1 >= k / 2 {
            break;
        }
        if !used[i] && !used[j] {
            used[i] = true;
            used[j] = true;
            pairs.push((i, j));
        }
    }
    pairs
}

/// The vertex to start `piece` from: an odd one when the walk is a path, the nearest to `at`.
fn start_vertex(vertices: &[Vec2], edges: &[Edge], piece: &[usize], at: Option<Vec2>) -> usize {
    let mut degree = std::collections::BTreeMap::<usize, usize>::new();
    for &i in piece {
        *degree.entry(edges[i].a).or_default() += 1;
        *degree.entry(edges[i].b).or_default() += 1;
    }
    let odd: Vec<usize> = degree
        .iter()
        .filter(|(_, d)| *d % 2 == 1)
        .map(|(&v, _)| v)
        .collect();
    let candidates: Vec<usize> = if odd.is_empty() {
        degree.keys().copied().collect()
    } else {
        odd
    };
    *candidates
        .iter()
        .min_by(|&&x, &&y| {
            let d = |v: usize| at.map_or(0.0, |p| vertices[v].distance(p));
            d(x).total_cmp(&d(y)).then(x.cmp(&y))
        })
        .expect("a vertex")
}

/// Hierholzer's walk over `piece` from `v0`, taking at each vertex the unused edge that turns
/// least: the needle points in walk order.
fn hierholzer(vertices: &[Vec2], edges: &[Edge], piece: &[usize], v0: usize) -> Vec<Vec2> {
    let mut used = vec![false; edges.len()];
    // (vertex, edge taken into it, reversed)
    let mut stack: Vec<(usize, Option<(usize, bool)>)> = vec![(v0, None)];
    let mut circuit: Vec<(usize, bool)> = Vec::new();
    let heading = |e: usize, rev: bool| {
        let p = &edges[e].points;
        let (a, b) = if rev {
            (p[1.min(p.len() - 1)], p[0])
        } else {
            (p[p.len().saturating_sub(2)], p[p.len() - 1])
        };
        b - a
    };
    while let Some(&(v, into)) = stack.last() {
        let incoming = into.map(|(e, rev)| heading(e, rev));
        let next = piece
            .iter()
            .copied()
            .filter(|&i| !used[i] && (edges[i].a == v || edges[i].b == v))
            .map(|i| {
                let rev = edges[i].a != v;
                let p = &edges[i].points;
                let out = if rev {
                    p[p.len().saturating_sub(2)] - p[p.len() - 1]
                } else {
                    p[1.min(p.len() - 1)] - p[0]
                };
                let turn = incoming.map_or(0.0, |d| {
                    let (l1, l2) = (d.length(), out.length());
                    if l1 == 0.0 || l2 == 0.0 {
                        0.0
                    } else {
                        -(d.x * out.x + d.y * out.y) / (l1 * l2)
                    }
                });
                (turn, i, rev)
            })
            .min_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        match next {
            Some((_, i, rev)) => {
                used[i] = true;
                let w = if rev { edges[i].a } else { edges[i].b };
                stack.push((w, Some((i, rev))));
            }
            None => {
                let (_, into) = stack.pop().expect("non-empty");
                if let Some(step) = into {
                    circuit.push(step);
                }
            }
        }
    }
    circuit.reverse();
    let mut out = vec![vertices[v0]];
    for (e, rev) in circuit {
        let p = &edges[e].points;
        let pts: Vec<Vec2> = if rev {
            p.iter().rev().copied().collect()
        } else {
            p.clone()
        };
        out.extend(pts.into_iter().skip(1));
    }
    out
}

/// Total length of a polyline (0.1 mm).
pub fn length(p: &[Vec2]) -> f64 {
    p.windows(2).map(|w| w[0].distance(w[1])).sum()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    #[test]
    fn a_tree_of_lines_is_one_walk() {
        // A "Y": three lines meeting at the middle.
        let lines = vec![
            vec![v2(0.0, 0.0), v2(100.0, 100.0)],
            vec![v2(200.0, 0.0), v2(100.0, 100.0)],
            vec![v2(100.0, 100.0), v2(100.0, 250.0)],
        ];
        let walks = route(&lines, 5.0, false, None);
        assert_eq!(walks.len(), 1);
        let sum: f64 = lines.iter().map(|l| length(l)).sum();
        let shortest_leg = 100.0 * std::f64::consts::SQRT_2;
        // Four odd vertices: the cheapest doubling is the shortest leg.
        assert!((length(&walks[0]) - (sum + shortest_leg)).abs() < 1e-6);
    }

    #[test]
    fn a_window_grid_doubles_the_minimum() {
        // A 2 x 2 window: 3 horizontal and 3 vertical lines of 200, cell 100.
        let mut lines = Vec::new();
        for k in 0..3 {
            let c = 100.0 * k as f64;
            lines.push(vec![v2(0.0, c), v2(200.0, c)]);
            lines.push(vec![v2(c, 0.0), v2(c, 200.0)]);
        }
        let walks = route(&lines, 5.0, false, Some(v2(0.0, 0.0)));
        assert_eq!(walks.len(), 1);
        // Odd vertices: the four edge middles. Leave two as the ends; the other two are 200
        // apart however they pair (round a corner or through the centre): 200 doubled.
        assert!(
            (length(&walks[0]) - (1200.0 + 200.0)).abs() < 1e-6,
            "{}",
            length(&walks[0])
        );
    }

    #[test]
    fn redwork_sews_every_line_twice_and_returns() {
        let lines = vec![
            vec![v2(0.0, 0.0), v2(100.0, 0.0)],
            vec![v2(100.0, 0.0), v2(100.0, 80.0)],
            vec![v2(100.0, 0.0), v2(200.0, 0.0)],
        ];
        let walks = route(&lines, 5.0, true, Some(v2(0.0, 0.0)));
        assert_eq!(walks.len(), 1);
        assert!((length(&walks[0]) - 2.0 * 280.0).abs() < 1e-6);
        assert_eq!(walks[0].first(), walks[0].last());
    }

    #[test]
    fn separate_lines_stay_separate_walks() {
        let lines = vec![
            vec![v2(0.0, 0.0), v2(100.0, 0.0)],
            vec![v2(0.0, 100.0), v2(100.0, 100.0)],
        ];
        assert_eq!(route(&lines, 5.0, false, None).len(), 2);
    }
}
