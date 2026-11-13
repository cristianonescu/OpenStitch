//! Strokes of a letter: the centre-line graph of a glyph's outline, cut into the columns
//! PE-DESIGN 11.50 sews TrueType text in (docs/research/stroke-lettering.md).
//!
//! PE-DESIGN sews a TrueType letter stroke by stroke, each stroke a column whose crossings stay
//! across it (owner references `stroke-satin-*`, `stroke-fill-*`, round 5). This module finds
//! the strokes: the region is sampled on a grid, thinned to a one-cell skeleton (Zhangâ€“Suen,
//! so the skeleton keeps the region's topology), and the skeleton is read as a graph whose
//! nodes are its free ends and junctions. Short spurs that thinning leaves at corners are pruned,
//! every branch between two nodes becomes a stroke, a branch is cut again where it turns sharply
//! (a V's foot, an A's apex; an L's right angle stays one stroke, as PE-DESIGN fans it), and a
//! closed loop with no junction (an O) is one closed stroke. Free ends reach out to the outline.
//!
//! [`GlyphStrokes::blocks`] orders them as PE-DESIGN's Convert to Blocks does: strokes that
//! continue each other straight through a junction are one column, a column another stroke
//! leaves from is sewn in two parts and its junction square, and the rest whole. Own algorithm;
//! lengths are 0.1 mm.

use crate::fmath::Det;
use crate::geometry::{Contour, Rect, Vec2, v2};
use crate::skeleton::segment_distance;

/// Grid cells along the longer side of the region.
const GRID_CELLS: f64 = 200.0;
/// Most grid cells (a guard on degenerate input).
const MAX_CELLS: usize = 250_000;
/// Most outline edges.
const MAX_EDGES: usize = 20_000;
/// A free branch shorter than this many times its junction's clearance is a thinning spur.
const SPUR: f64 = 1.6;
/// Two junctions joined by a branch shorter than this many times their clearance are one
/// junction (thinning splits an X's crossing in two).
const CONTRACT: f64 = 1.5;
/// A branch looping back to its own junction round no counter, shorter than this many times
/// the junction's clearance round, is a thinning knot in an acute notch (M's top corners, #446).
const NOTCH_LOOP: f64 = 4.0;
/// Most outside cells a thinning knot goes round: the tip of an acute notch narrower than a
/// cell, sealed off from the rest of the notch in the grid. A counter is larger.
const KNOT_CELLS: usize = 3;
/// How far from a knot's junction, in its clearances, [`Graph::straight_pair_at`] looks.
const KNOT_ARM: f64 = 6.0;
/// A branch turning by more than this (degrees) over about its width is cut there: an acute
/// V or A apex turns by 140Â° and more, an L's corner by 90Â°.
pub const SHARP_TURN: f64 = 115.0;

/// Where a stroke ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StrokeEnd {
    /// At the outline (the stroke's end reaches the edge of the letter).
    Free,
    /// At junction `n` of [`GlyphStrokes::junctions`].
    Junction(usize),
    /// At a sharp turn, where the next stroke starts.
    Corner,
    /// At a sharp turn where the other stroke, a stem, runs on to the outline past this end
    /// (N's and M's stem-to-diagonal corners, as PE-DESIGN sews them, #446): this stroke's
    /// satin stops against the stem, with no tip into the corner.
    Against,
}

/// One stroke: its centre line (start to end), the region's width across it at each centre-line
/// point (0.1 mm), and how it ends. A closed stroke's centre line is a loop (first point not
/// repeated) and its ends are [`StrokeEnd::Free`].
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub centre: Vec<Vec2>,
    pub widths: Vec<f64>,
    pub closed: bool,
    pub ends: [StrokeEnd; 2],
}

impl Stroke {
    /// Length of the centre line.
    pub fn length(&self) -> f64 {
        let open: f64 = self.centre.windows(2).map(|w| w[0].distance(w[1])).sum();
        match (self.closed, self.centre.first(), self.centre.last()) {
            (true, Some(a), Some(b)) => open + a.distance(*b),
            _ => open,
        }
    }

    /// The centre line's bounding box.
    pub fn bounds(&self) -> Option<Rect> {
        Rect::enclosing(self.centre.iter().copied())
    }
}

/// Where strokes meet: the centre of the junction and the region's clearance there (half the
/// widest stroke through it).
#[derive(Clone, Debug, PartialEq)]
pub struct Junction {
    pub at: Vec2,
    pub radius: f64,
}

/// The strokes of one region (a glyph's outline, even-odd, holes allowed).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GlyphStrokes {
    pub strokes: Vec<Stroke>,
    pub junctions: Vec<Junction>,
}

/// The strokes of the closed region `contours`; `None` for an empty or degenerate region.
pub fn strokes(contours: &[Contour]) -> Option<GlyphStrokes> {
    let edges: Vec<(Vec2, Vec2)> = contours
        .iter()
        .filter(|c| c.closed)
        .flat_map(Contour::edges)
        .filter(|(a, b)| a.x.is_finite() && a.y.is_finite() && b.x.is_finite() && b.y.is_finite())
        .collect();
    if edges.len() < 3 || edges.len() > MAX_EDGES {
        return None;
    }
    let b = Rect::enclosing(edges.iter().flat_map(|e| [e.0, e.1]))?;
    if b.width().max(b.height()) < 2.0 {
        return None;
    }
    let grid = Grid::new(&edges, &b)?;
    let skeleton = unstair(&grid, thin(&grid));
    let graph = Graph::trace(&grid, &skeleton);
    let graph = graph.pruned(&grid);
    Some(graph.into_strokes(&grid, &edges))
}

// ------------------------------------------------------------------ grid

struct Grid {
    origin: Vec2,
    step: f64,
    nx: usize,
    ny: usize,
    /// Distance to the outline; negative outside.
    clearance: Vec<f64>,
}

impl Grid {
    fn new(edges: &[(Vec2, Vec2)], b: &Rect) -> Option<Grid> {
        let step = (b.width().max(b.height()) / GRID_CELLS).max(0.05);
        // One cell of margin on every side, so the thinning never touches the border.
        let nx = (b.width() / step).ceil() as usize + 3;
        let ny = (b.height() / step).ceil() as usize + 3;
        if nx * ny > MAX_CELLS {
            return None;
        }
        let origin = v2(b.min_x - step, b.min_y - step);
        let mut clearance = vec![-1.0; nx * ny];
        let mut crossings = Vec::new();
        for j in 0..ny {
            let y = origin.y + (j as f64 + 0.5) * step;
            crossings.clear();
            for &(a, c) in edges {
                if (a.y > y) != (c.y > y) {
                    crossings.push(a.x + (y - a.y) * (c.x - a.x) / (c.y - a.y));
                }
            }
            crossings.sort_by(f64::total_cmp);
            for pair in crossings.as_chunks::<2>().0 {
                for i in 0..nx {
                    let x = origin.x + (i as f64 + 0.5) * step;
                    if x > pair[0] && x < pair[1] {
                        let p = v2(x, y);
                        clearance[j * nx + i] = edges
                            .iter()
                            .map(|&(a, c)| segment_distance(p, a, c))
                            .fold(f64::INFINITY, f64::min);
                    }
                }
            }
        }
        Some(Grid {
            origin,
            step,
            nx,
            ny,
            clearance,
        })
    }

    fn centre(&self, k: usize) -> Vec2 {
        v2(
            self.origin.x + ((k % self.nx) as f64 + 0.5) * self.step,
            self.origin.y + ((k / self.nx) as f64 + 0.5) * self.step,
        )
    }

    /// Cells outside the outline that no path of outside cells joins to the grid's border:
    /// the counters.
    fn counters(&self) -> Vec<bool> {
        let mut reached = vec![false; self.clearance.len()];
        let mut todo: Vec<usize> = (0..self.clearance.len())
            .filter(|&k| {
                let (i, j) = (k % self.nx, k / self.nx);
                i == 0 || j == 0 || i + 1 == self.nx || j + 1 == self.ny
            })
            .collect();
        while let Some(k) = todo.pop() {
            if reached[k] || self.clearance[k] >= 0.0 {
                continue;
            }
            reached[k] = true;
            todo.extend(self.ring(k).into_iter().flatten().filter(|&m| !reached[m]));
        }
        (0..self.clearance.len())
            .map(|k| self.clearance[k] < 0.0 && !reached[k])
            .collect()
    }

    /// Whether the loop through `cells` (in order) goes round a counter (see
    /// [`Self::counters`]), rather than being a knot of the thinning's own in solid ink.
    fn encloses_counter(&self, cells: &[usize], counters: &[bool]) -> bool {
        let poly: Vec<Vec2> = cells.iter().map(|&k| self.centre(k)).collect();
        let n = poly.len();
        if n < 3 {
            return false;
        }
        let (mut i0, mut i1, mut j0, mut j1) = (usize::MAX, 0, usize::MAX, 0);
        for &k in cells {
            let (i, j) = (k % self.nx, k / self.nx);
            (i0, i1, j0, j1) = (i0.min(i), i1.max(i), j0.min(j), j1.max(j));
        }
        let inside = |p: Vec2| {
            let mut odd = false;
            for e in 0..n {
                let (a, c) = (poly[e], poly[(e + 1) % n]);
                if (a.y > p.y) != (c.y > p.y) && p.x < a.x + (p.y - a.y) * (c.x - a.x) / (c.y - a.y)
                {
                    odd = !odd;
                }
            }
            odd
        };
        let enclosed = (j0..=j1)
            .flat_map(|j| (i0..=i1).map(move |i| j * self.nx + i))
            .filter(|&k| counters[k] && inside(self.centre(k)))
            .count();
        enclosed > KNOT_CELLS
    }

    /// The 8 neighbours of cell `k`, clockwise from the one above (P2â€¦P9 of Zhangâ€“Suen);
    /// `None` outside the grid.
    fn ring(&self, k: usize) -> [Option<usize>; 8] {
        let (i, j) = ((k % self.nx) as i64, (k / self.nx) as i64);
        [
            (0, -1),
            (1, -1),
            (1, 0),
            (1, 1),
            (0, 1),
            (-1, 1),
            (-1, 0),
            (-1, -1),
        ]
        .map(|(di, dj)| {
            let (a, c) = (i + di, j + dj);
            (a >= 0 && c >= 0 && a < self.nx as i64 && c < self.ny as i64)
                .then(|| c as usize * self.nx + a as usize)
        })
    }
}

/// Zhangâ€“Suen thinning of the inside cells: a one-cell-wide skeleton with the region's topology.
fn thin(grid: &Grid) -> Vec<bool> {
    let mut on: Vec<bool> = grid.clearance.iter().map(|&c| c >= 0.0).collect();
    let mut changed = true;
    let mut remove = Vec::new();
    while changed {
        changed = false;
        for pass in 0..2 {
            remove.clear();
            for k in 0..on.len() {
                if !on[k] {
                    continue;
                }
                let p = grid.ring(k).map(|n| n.is_some_and(|n| on[n]));
                let count = p.iter().filter(|&&x| x).count();
                if !(2..=6).contains(&count) {
                    continue;
                }
                let transitions = (0..8).filter(|&m| !p[m] && p[(m + 1) % 8]).count();
                if transitions != 1 {
                    continue;
                }
                // P2Â·P4Â·P6 and P4Â·P6Â·P8 (first pass); P2Â·P4Â·P8 and P2Â·P6Â·P8 (second pass).
                let (a, b) = if pass == 0 {
                    (p[0] && p[2] && p[4], p[2] && p[4] && p[6])
                } else {
                    (p[0] && p[2] && p[6], p[0] && p[4] && p[6])
                };
                if !a && !b {
                    remove.push(k);
                }
            }
            for &k in &remove {
                on[k] = false;
            }
            changed |= !remove.is_empty();
        }
    }
    on
}

/// Thinning leaves staircases: a cell whose two neighbours touch each other diagonally. Such a
/// cell carries nothing (its neighbours stay connected without it) and would read as a branch;
/// it is removed, so an ordinary skeleton cell has exactly two neighbours.
fn unstair(grid: &Grid, mut on: Vec<bool>) -> Vec<bool> {
    let mut changed = true;
    while changed {
        changed = false;
        for k in 0..on.len() {
            if !on[k] {
                continue;
            }
            let p = grid.ring(k).map(|n| n.is_some_and(|n| on[n]));
            // Orthogonal neighbours at 0 (up), 2 (right), 4 (down), 6 (left).
            let corner = [(0, 2, 1), (2, 4, 3), (4, 6, 5), (6, 0, 7)]
                .iter()
                .any(|&(a, b, between)| p[a] && p[b] && !p[between]);
            // The cell's neighbours, other than the corner pair, must keep the line connected:
            // only remove it when it has no third neighbour on the opposite side.
            let count = p.iter().filter(|&&x| x).count();
            if corner && count == 2 {
                on[k] = false;
                changed = true;
            }
        }
    }
    on
}

// ------------------------------------------------------------------ graph

/// A skeleton branch: its cells in order and the node at each end (`None` for a loop).
#[derive(Clone)]
struct Branch {
    cells: Vec<usize>,
    ends: [Option<usize>; 2],
}

#[derive(Clone)]
struct Node {
    cells: Vec<usize>,
    /// A free end (one neighbour) rather than a junction.
    free: bool,
    /// A skeleton cell with no neighbour at all: a blob thinned to one cell (the dot of an i).
    dot: bool,
}

struct Graph {
    nodes: Vec<Node>,
    branches: Vec<Branch>,
}

impl Graph {
    fn trace(grid: &Grid, on: &[bool]) -> Graph {
        let neighbours = |k: usize| -> Vec<usize> {
            grid.ring(k)
                .into_iter()
                .flatten()
                .filter(|&n| on[n])
                .collect()
        };
        // After `unstair`, an ordinary skeleton cell has exactly two neighbours: free ends have
        // one, junction cells three or more.
        let count: Vec<usize> = (0..on.len())
            .map(|k| if on[k] { neighbours(k).len() } else { 0 })
            .collect();
        let is_node = |k: usize| on[k] && count[k] != 2;
        let mut node_of = vec![usize::MAX; on.len()];
        let mut nodes: Vec<Node> = Vec::new();
        for k in 0..on.len() {
            if !is_node(k) || node_of[k] != usize::MAX {
                continue;
            }
            let id = nodes.len();
            let free = count[k] <= 1;
            let mut cells = vec![k];
            node_of[k] = id;
            if !free {
                let mut i = 0;
                while i < cells.len() {
                    for n in neighbours(cells[i]) {
                        if is_node(n) && count[n] >= 3 && node_of[n] == usize::MAX {
                            node_of[n] = id;
                            cells.push(n);
                        }
                    }
                    i += 1;
                }
            }
            nodes.push(Node {
                dot: count[k] == 0,
                cells,
                free,
            });
        }
        let mut used = vec![false; on.len()];
        let mut branches = Vec::new();
        let mut direct = std::collections::BTreeSet::new();
        for (id, node) in nodes.iter().enumerate() {
            for &c in &node.cells {
                for start in neighbours(c) {
                    let other = node_of[start];
                    if other == id {
                        continue;
                    }
                    if other != usize::MAX {
                        // Two nodes touching: a branch of no length.
                        if direct.insert((id.min(other), id.max(other))) {
                            branches.push(Branch {
                                cells: vec![c, start],
                                ends: [Some(id), Some(other)],
                            });
                        }
                        continue;
                    }
                    if used[start] {
                        continue;
                    }
                    used[start] = true;
                    let mut cells = vec![c, start];
                    let mut end = None;
                    loop {
                        let last = cells[cells.len() - 1];
                        let prev = cells[cells.len() - 2];
                        let next: Vec<usize> = neighbours(last)
                            .into_iter()
                            .filter(|&n| n != prev)
                            .collect();
                        if let Some(&n) = next.iter().find(|&&n| node_of[n] != usize::MAX) {
                            cells.push(n);
                            end = Some(node_of[n]);
                            break;
                        }
                        match next.into_iter().find(|&n| !used[n]) {
                            Some(n) => {
                                used[n] = true;
                                cells.push(n);
                            }
                            None => break,
                        }
                    }
                    branches.push(Branch {
                        cells,
                        ends: [Some(id), end],
                    });
                }
            }
        }
        // Loops with no node at all (an O).
        for k in 0..on.len() {
            if !on[k] || node_of[k] != usize::MAX || used[k] {
                continue;
            }
            let mut cells = vec![k];
            used[k] = true;
            while let Some(n) = neighbours(cells[cells.len() - 1])
                .into_iter()
                .find(|&n| !used[n] && node_of[n] == usize::MAX)
            {
                used[n] = true;
                cells.push(n);
            }
            if cells.len() > 2 {
                branches.push(Branch {
                    cells,
                    ends: [None, None],
                });
            }
        }
        Graph { nodes, branches }
    }

    fn degree(&self, id: usize) -> usize {
        self.branches
            .iter()
            .map(|b| b.ends.iter().filter(|&&e| e == Some(id)).count())
            .sum()
    }

    /// Whether node `id` is a crossing of four arms that each end free (X), not a junction
    /// with more branches joined on (M's notch loops, g's and Pacifico's joins, #446).
    fn free_crossing(&self, id: usize) -> bool {
        self.degree(id) == 4
            && self
                .branches
                .iter()
                .filter(|b| b.ends.contains(&Some(id)))
                .all(|b| {
                    let other = if b.ends[0] == Some(id) {
                        b.ends[1]
                    } else {
                        b.ends[0]
                    };
                    b.ends[0] != b.ends[1] && other.is_some_and(|o| self.nodes[o].free)
                })
    }

    /// Whether junction `id` joins, besides loops back to itself, exactly two branches that both
    /// leave it straight: over their first [`KNOT_ARM`] clearances from the junction, no cell
    /// further than the junction's clearance from the chord of that stretch (M's stem and
    /// diagonal at its top corners). Only that stretch counts: further on, a branch may run on
    /// through a junction that pruning dissolves (M's diagonals join through the foot of its
    /// middle V). A script letter's curved strokes (Pacifico's a and m) are not straight there,
    /// and keep their knots (#446).
    fn straight_pair_at(&self, grid: &Grid, id: usize) -> bool {
        let r = self.radius(grid, id);
        let others: Vec<&Branch> = self
            .branches
            .iter()
            .filter(|b| b.ends.contains(&Some(id)) && b.ends[0] != b.ends[1])
            .collect();
        others.len() == 2
            && others.iter().all(|b| {
                let mut cells: Vec<Vec2> = b.cells.iter().map(|&k| grid.centre(k)).collect();
                if b.ends[0] != Some(id) {
                    cells.reverse();
                }
                let mut run = 0.0;
                let mut arm = Vec::new();
                for (i, &p) in cells.iter().enumerate() {
                    if i > 0 {
                        run += cells[i - 1].distance(p);
                    }
                    if run > KNOT_ARM * r {
                        break;
                    }
                    arm.push(p);
                }
                let (Some(&p), Some(&q)) = (arm.first(), arm.last()) else {
                    return false;
                };
                arm.iter().all(|&c| segment_distance(c, p, q) <= r)
            })
    }

    fn radius(&self, grid: &Grid, id: usize) -> f64 {
        self.nodes[id]
            .cells
            .iter()
            .map(|&k| grid.clearance[k])
            .fold(0.0, f64::max)
    }

    /// Spurs pruned (free branches shorter than [`SPUR`] times their junction's clearance) and
    /// the junctions they leave with two branches merged through.
    fn pruned(mut self, grid: &Grid) -> Graph {
        let counters = grid.counters();
        loop {
            let mut changed = false;
            let straight_pair: Vec<bool> = (0..self.nodes.len())
                .map(|id| self.straight_pair_at(grid, id))
                .collect();
            let mut keep = Vec::with_capacity(self.branches.len());
            for b in std::mem::take(&mut self.branches) {
                let len = b.cells.len() as f64 * grid.step;
                let spur = match b.ends {
                    // A small loop back to its own junction round no counter: the thinning's
                    // knot in an acute notch (M's top corners, #446).
                    [Some(a), Some(c)] if a == c => {
                        !self.nodes[a].free
                            && len < NOTCH_LOOP * self.radius(grid, a)
                            && straight_pair[a]
                            && !grid.encloses_counter(&b.cells, &counters)
                    }
                    [Some(a), Some(c)] => {
                        let (free, junction) = if self.nodes[a].free { (a, c) } else { (c, a) };
                        self.nodes[free].free
                            && !self.nodes[junction].free
                            && len < SPUR * self.radius(grid, junction)
                    }
                    _ => false,
                };
                if spur {
                    changed = true;
                } else {
                    keep.push(b);
                }
            }
            self.branches = keep;
            // Two junctions a stub apart are one junction.
            if let Some(i) = (0..self.branches.len()).find(|&i| match self.branches[i].ends {
                [Some(a), Some(c)] if a != c && !self.nodes[a].free && !self.nodes[c].free => {
                    let len = self.branches[i].cells.len() as f64 * grid.step;
                    len < CONTRACT * self.radius(grid, a).max(self.radius(grid, c))
                }
                _ => false,
            }) {
                let b = self.branches.remove(i);
                let [Some(a), Some(c)] = b.ends else {
                    unreachable!("matched above")
                };
                let moved = std::mem::take(&mut self.nodes[c].cells);
                self.nodes[a].cells.extend(moved);
                self.nodes[a].cells.extend(b.cells);
                for br in &mut self.branches {
                    for e in &mut br.ends {
                        if *e == Some(c) {
                            *e = Some(a);
                        }
                    }
                }
                self.nodes[c].free = true;
                changed = true;
            }
            // A junction left with two branches: join them.
            for id in 0..self.nodes.len() {
                if self.nodes[id].free || self.degree(id) != 2 {
                    continue;
                }
                let at: Vec<usize> = (0..self.branches.len())
                    .filter(|&i| self.branches[i].ends.contains(&Some(id)))
                    .collect();
                if at.len() != 2 {
                    // A loop back to the same junction.
                    if at.len() == 1 {
                        self.branches[at[0]].ends = [None, None];
                        changed = true;
                    }
                    continue;
                }
                let (mut a, mut c) = (self.branches[at[0]].clone(), self.branches[at[1]].clone());
                if a.ends[1] != Some(id) {
                    a.cells.reverse();
                    a.ends.reverse();
                }
                if c.ends[0] != Some(id) {
                    c.cells.reverse();
                    c.ends.reverse();
                }
                a.cells.extend(c.cells.into_iter().skip(1));
                a.ends[1] = c.ends[1];
                // Both ends back at one junction that has no other branch: a closed loop.
                if let [Some(x), Some(y)] = a.ends
                    && x == y
                    && self.degree_without(x, &at) == 0
                {
                    a.ends = [None, None];
                }
                let (lo, hi) = (at[0].min(at[1]), at[0].max(at[1]));
                self.branches.remove(hi);
                self.branches.remove(lo);
                self.branches.push(a);
                self.nodes[id].free = true;
                self.nodes[id].cells.clear();
                changed = true;
            }
            if !changed {
                return self;
            }
        }
    }

    fn degree_without(&self, id: usize, skip: &[usize]) -> usize {
        self.branches
            .iter()
            .enumerate()
            .filter(|(i, _)| !skip.contains(i))
            .map(|(_, b)| b.ends.iter().filter(|&&e| e == Some(id)).count())
            .sum()
    }

    fn into_strokes(self, grid: &Grid, edges: &[(Vec2, Vec2)]) -> GlyphStrokes {
        // Junctions that still join three or more branches, renumbered.
        let mut junction_of = vec![None; self.nodes.len()];
        let mut junctions = Vec::new();
        for (id, n) in self.nodes.iter().enumerate() {
            if n.free || n.cells.is_empty() || self.degree(id) < 3 {
                continue;
            }
            let at = n
                .cells
                .iter()
                .map(|&k| grid.centre(k))
                .fold(v2(0.0, 0.0), |s, p| s + p)
                * (1.0 / n.cells.len() as f64);
            junction_of[id] = Some(junctions.len());
            junctions.push(Junction {
                at,
                radius: self.radius(grid, id),
            });
        }
        let clearance = |p: Vec2| {
            edges
                .iter()
                .map(|&(a, c)| segment_distance(p, a, c))
                .fold(f64::INFINITY, f64::min)
        };
        let mut strokes = Vec::new();
        for b in &self.branches {
            let closed = b.ends == [None, None];
            // An arm of a crossing of four (X).
            let crossing_arm = b
                .ends
                .iter()
                .flatten()
                .any(|&id| junction_of[id].is_some() && self.free_crossing(id));
            let mut pts: Vec<Vec2> = b.cells.iter().map(|&k| grid.centre(k)).collect();
            // Branch ends at a junction meet at its centre.
            let end_kind = |e: Option<usize>| match e.and_then(|id| junction_of[id]) {
                Some(j) => StrokeEnd::Junction(j),
                None => StrokeEnd::Free,
            };
            let ends = if closed {
                [StrokeEnd::Free, StrokeEnd::Free]
            } else {
                [end_kind(b.ends[0]), end_kind(b.ends[1])]
            };
            if let StrokeEnd::Junction(j) = ends[0] {
                pts[0] = junctions[j].at;
            }
            if let StrokeEnd::Junction(j) = ends[1] {
                let n = pts.len() - 1;
                pts[n] = junctions[j].at;
            }
            let pts = smooth(&pts, closed);
            let tolerance = grid.step;
            let mut centre: Vec<Vec2> = simplify(&pts, tolerance, closed);
            if centre.len() < 2 {
                continue;
            }
            // Free ends reach out to the outline along the stroke.
            if !closed {
                for (end, kind) in ends.iter().enumerate() {
                    if *kind != StrokeEnd::Free {
                        continue;
                    }
                    // The thinned centre line hooks towards a corner where the end edge meets
                    // the stroke at a slant (an italic stem's foot): drop the last stretch, Â¾ of the
                    // stroke's width long, and reach out along the line before it.
                    if end == 0 {
                        centre.reverse();
                    }
                    let width = {
                        let mut w: Vec<f64> = centre.iter().map(|&p| 2.0 * clearance(p)).collect();
                        w.sort_by(f64::total_cmp);
                        0.75 * w[w.len() / 2]
                    };
                    let total = length(&centre);
                    let mut trend = None;
                    if width > 0.0 && total > 2.5 * width {
                        let tip = point_at(&centre, total - width);
                        let back = point_at(&centre, total - 2.0 * width);
                        let mut kept: Vec<Vec2> = Vec::new();
                        let mut at = 0.0;
                        for (i, &p) in centre.iter().enumerate() {
                            if i > 0 {
                                at += centre[i - 1].distance(p);
                            }
                            if at < total - width - 1e-6 {
                                kept.push(p);
                            }
                        }
                        kept.push(tip);
                        let d = tip - back;
                        // Only where the line before runs on straight to the outline (a hook);
                        // a curved stroke (S's ends) leaves through its side first.
                        let straight = d.length() > 1e-9
                            && ray_to(tip, d * (1.0 / d.length()), edges)
                                .is_some_and(|t| t >= 0.8 * width);
                        // A curved stroke turns steadily over its last stretch (S's ends: 17-22Â°)
                        // a straight stem's does not (the italic H: at most 6Â°).
                        let chord = centre[centre.len() - 1] - tip;
                        let cos = (chord.x * d.x + chord.y * d.y)
                            / (chord.length() * d.length()).max(1e-12);
                        let hook = cos.clamp(-1.0, 1.0).dacos().to_degrees();
                        // An arm of a four-arm crossing (X) dead straight before its last
                        // stretch is straight however far the thinning hooks it into a corner
                        // of its level cut foot (X's feet hook by 30Â°, #446). Kept to X: the
                        // same rule on every letter cut t's tail off in fill (#446 notes).
                        let before = point_at(&centre, total - 3.0 * width);
                        let e = back - before;
                        // A hook ends near where the straightened line reaches the outline; a
                        // tail turning off the stroke (t's foot) ends far from it.
                        let dn = d * (1.0 / d.length().max(1e-12));
                        let near = ray_to(tip, dn, edges).is_some_and(|t| {
                            (tip + dn * t).distance(centre[centre.len() - 1]) <= width
                        });
                        let straight_before = crossing_arm
                            && total > 3.5 * width
                            && near
                            && hook < END_HOOK
                            && ((e.x * d.x + e.y * d.y) / (e.length() * d.length()).max(1e-12))
                                .clamp(-1.0, 1.0)
                                .dacos()
                                .to_degrees()
                                < END_STRAIGHT;
                        let gentle = hook < END_TURN || straight_before;
                        if kept.len() >= 2 && straight && gentle {
                            centre = kept;
                            trend = Some(d);
                        }
                    }
                    let p = centre[centre.len() - 1];
                    let d = trend.unwrap_or(p - centre[centre.len() - 2]);
                    let len = d.length();
                    if len > 1e-9 {
                        let dir = d * (1.0 / len);
                        if let Some(t) = ray_to(p, dir, edges) {
                            let reach = p + dir * (t - grid.step * 0.5).max(0.0);
                            let n = centre.len() - 1;
                            centre[n] = reach;
                        }
                    }
                    if end == 0 {
                        centre.reverse();
                    }
                }
            }
            let mut pieces = cut_sharp(&centre, closed, ends);
            run_on_stems(&mut pieces, edges, grid.step);
            for piece in pieces {
                let centre = densify(&piece.0, piece.1);
                let widths = centre.iter().map(|&p| 2.0 * clearance(p)).collect();
                strokes.push(Stroke {
                    centre,
                    widths,
                    closed: piece.1,
                    ends: piece.2,
                });
            }
        }
        // A blob thinned to one cell (the dot of an i): a stroke across its longer side.
        for (id, n) in self.nodes.iter().enumerate() {
            if !n.dot || n.cells.len() != 1 || self.degree(id) != 0 {
                continue;
            }
            let k = n.cells[0];
            let mut blob = vec![k];
            let mut seen = std::collections::BTreeSet::from([k]);
            let mut i = 0;
            while i < blob.len() && blob.len() < MAX_CELLS {
                for m in grid.ring(blob[i]).into_iter().flatten() {
                    if grid.clearance[m] >= 0.0 && seen.insert(m) {
                        blob.push(m);
                    }
                }
                i += 1;
            }
            let Some(r) = Rect::enclosing(blob.iter().map(|&m| grid.centre(m))) else {
                continue;
            };
            let c = grid.centre(k);
            let half = grid.step * 0.5;
            let (a, b) = if r.width() >= r.height() {
                (v2(r.min_x - half, c.y), v2(r.max_x + half, c.y))
            } else {
                (v2(c.x, r.min_y - half), v2(c.x, r.max_y + half))
            };
            let centre = vec![a, b];
            let widths = centre.iter().map(|&p| 2.0 * clearance(p)).collect();
            strokes.push(Stroke {
                centre,
                widths,
                closed: false,
                ends: [StrokeEnd::Free, StrokeEnd::Free],
            });
        }
        // Deterministic order: by the top of each stroke, then its left.
        strokes.sort_by(|a, b| {
            let (ba, bb) = (a.bounds(), b.bounds());
            let key = |r: Option<Rect>| r.map_or((0.0, 0.0), |r| (r.min_y, r.min_x));
            let (ka, kb) = (key(ba), key(bb));
            ka.0.total_cmp(&kb.0).then(ka.1.total_cmp(&kb.1))
        });
        GlyphStrokes { strokes, junctions }
    }
}

/// A free end whose last Â¾ stroke width turns less than this (degrees) from the line before it is
/// a straight stroke, which the thinning bent towards a corner: its end is straightened.
const END_TURN: f64 = 12.0;
/// A free end whose line before its last stretch turns less than this (degrees) is straight
/// there, and its last stretch, turning by less than [`END_HOOK`], is a thinning hook.
const END_STRAIGHT: f64 = 3.0;
/// See [`END_STRAIGHT`].
const END_HOOK: f64 = 60.0;

/// Most distance between two centre-line points, 0.1 mm: widths are measured at every point.
const MAX_SEGMENT: f64 = 20.0;

/// `pts` with points added so no segment is longer than [`MAX_SEGMENT`] (vertices kept).
fn densify(pts: &[Vec2], closed: bool) -> Vec<Vec2> {
    let mut out = Vec::with_capacity(pts.len());
    let n = pts.len();
    let segments = if closed { n } else { n.saturating_sub(1) };
    for i in 0..n {
        out.push(pts[i]);
        if i < segments {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            let pieces = (a.distance(b) / MAX_SEGMENT).ceil().max(1.0) as usize;
            out.extend((1..pieces).map(|k| a.lerp(b, k as f64 / pieces as f64)));
        }
    }
    out
}

/// A moving average over five points (ends kept for an open line).
fn smooth(pts: &[Vec2], closed: bool) -> Vec<Vec2> {
    let n = pts.len();
    if n < 5 {
        return pts.to_vec();
    }
    (0..n)
        .map(|i| {
            if !closed && (i < 2 || i + 2 >= n) {
                return pts[i];
            }
            let at = |d: i64| pts[((i as i64 + d).rem_euclid(n as i64)) as usize];
            (at(-2) + at(-1) + at(0) + at(1) + at(2)) * 0.2
        })
        .collect()
}

/// Douglasâ€“Peucker; a closed line is split at its point farthest from the first.
fn simplify(pts: &[Vec2], tolerance: f64, closed: bool) -> Vec<Vec2> {
    fn walk(pts: &[Vec2], lo: usize, hi: usize, tolerance: f64, keep: &mut Vec<usize>) {
        let (far, d) = (lo + 1..hi)
            .map(|i| (i, segment_distance(pts[i], pts[lo], pts[hi])))
            .fold((lo, 0.0), |a, b| if b.1 > a.1 { b } else { a });
        if d > tolerance {
            walk(pts, lo, far, tolerance, keep);
            walk(pts, far, hi, tolerance, keep);
        } else {
            keep.push(hi);
        }
    }
    if pts.len() < 3 {
        return pts.to_vec();
    }
    if closed {
        let far = (1..pts.len())
            .max_by(|&a, &b| pts[a].distance(pts[0]).total_cmp(&pts[b].distance(pts[0])))
            .unwrap_or(1);
        let mut first: Vec<Vec2> = pts[..=far].to_vec();
        let mut second: Vec<Vec2> = pts[far..].to_vec();
        second.push(pts[0]);
        first = simplify(&first, tolerance, false);
        second = simplify(&second, tolerance, false);
        first.pop();
        second.pop();
        first.extend(second);
        return first;
    }
    let mut keep = vec![0];
    walk(pts, 0, pts.len() - 1, tolerance, &mut keep);
    keep.into_iter().map(|i| pts[i]).collect()
}

/// The line cut where it turns by more than [`SHARP_TURN`], measured between the directions
/// over a stretch of the line on either side of each point (so a smooth curve's many small
/// turns do not add up to a cut). Returns (centre, closed, ends) per piece.
fn cut_sharp(
    centre: &[Vec2],
    closed: bool,
    ends: [StrokeEnd; 2],
) -> Vec<(Vec<Vec2>, bool, [StrokeEnd; 2])> {
    let n = centre.len();
    if n < 3 {
        return vec![(centre.to_vec(), closed, ends)];
    }
    let total: f64 = centre.windows(2).map(|w| w[0].distance(w[1])).sum();
    let reach = (total * 0.08).max(1e-9);
    // Direction over `reach` before and after point i.
    let dir_at = |i: usize, forward: bool| -> Option<Vec2> {
        let mut acc = 0.0;
        let mut j = i;
        loop {
            let next = if forward {
                if closed { (j + 1) % n } else { j + 1 }
            } else if closed {
                (j + n - 1) % n
            } else {
                j.checked_sub(1)?
            };
            if next >= n || next == i {
                break;
            }
            acc += centre[j].distance(centre[next]);
            j = next;
            if acc >= reach {
                break;
            }
        }
        let d = if forward {
            centre[j] - centre[i]
        } else {
            centre[i] - centre[j]
        };
        (d.length() > 1e-9).then(|| d * (1.0 / d.length()))
    };
    let mut cuts = Vec::new();
    for i in 0..n {
        if !closed && (i == 0 || i == n - 1) {
            continue;
        }
        let (Some(a), Some(b)) = (dir_at(i, false), dir_at(i, true)) else {
            continue;
        };
        let turn = (a.x * b.x + a.y * b.y)
            .clamp(-1.0, 1.0)
            .dacos()
            .to_degrees();
        // A turn sharp at this point and sharper than at its neighbours (one cut per corner).
        if turn > SHARP_TURN {
            let sharper = |j: usize| -> f64 {
                match (dir_at(j, false), dir_at(j, true)) {
                    (Some(a), Some(b)) => (a.x * b.x + a.y * b.y)
                        .clamp(-1.0, 1.0)
                        .dacos()
                        .to_degrees(),
                    _ => 0.0,
                }
            };
            let prev = if i > 0 {
                sharper(i - 1)
            } else if closed {
                sharper(n - 1)
            } else {
                0.0
            };
            let next = if i + 1 < n {
                sharper(i + 1)
            } else if closed {
                sharper(0)
            } else {
                0.0
            };
            if turn >= prev && turn >= next {
                cuts.push(i);
            }
        }
    }
    // A corner with cuts on both sides of it along an open stroke (W's middle apex, M's middle
    // V) is not cut: PE-DESIGN sews the two diagonals there as one column through the corner,
    // its crossings fanned round it (stroke-satin-W: 155 crossings from the first foot over the
    // apex to the second, block running at 23 degrees; M-8: 65 crossings), as it fans L's corner.
    // A corner with a free end or a junction on one side (V's foot, A's apex, N's and W's outer
    // corners) is still cut (#1064).
    if !closed && cuts.len() >= 3 {
        cuts = vec![cuts[0], cuts[cuts.len() - 1]];
    }
    if cuts.is_empty() {
        return vec![(centre.to_vec(), closed, ends)];
    }
    let mut out = Vec::new();
    if closed {
        // A closed line with cuts becomes open pieces from cut to cut.
        for (k, &c) in cuts.iter().enumerate() {
            let next = cuts[(k + 1) % cuts.len()];
            let mut piece = vec![centre[c]];
            let mut j = c;
            loop {
                j = (j + 1) % n;
                piece.push(centre[j]);
                if j == next {
                    break;
                }
            }
            out.push((piece, false, [StrokeEnd::Corner, StrokeEnd::Corner]));
        }
        return out;
    }
    let mut start = 0;
    let mut start_end = ends[0];
    for &c in &cuts {
        out.push((
            centre[start..=c].to_vec(),
            false,
            [start_end, StrokeEnd::Corner],
        ));
        start = c;
        start_end = StrokeEnd::Corner;
    }
    out.push((centre[start..].to_vec(), false, [start_end, ends[1]]));
    out
}

/// Most a stem leans from upright (degrees) for [`run_on_stems`].
const STEM_LEAN: f64 = 12.0;
/// A stem point further than this many half widths off the stem's line is the thinning's
/// bend into the corner; [`run_on_stems`] drops it.
const STEM_BEND: f64 = 0.5;
/// How far from the corner (0.1 mm) [`run_on_stems`] measures a stroke's direction.
const STEM_REACH: f64 = 40.0;

/// Where a slanted run-on ([`slanted_run_on`]) stops the other arm: this much of the
/// run-on arm's half width from its centre line. The arms meet at a sharp angle, so stopping
/// at the full half width left the stopped arm's outer side short of the corner.
const SLANTED_AGAINST: f64 = 0.5;

/// Where two slanted arms leaning opposite ways meet at a cut (V's foot, M's middle V, W's
/// feet and middle apex, A's apex, #446), the arm that runs on, as [`run_on_stems`] takes it:
/// (its piece, the direction it runs on in). PE-DESIGN runs on the arm that falls to the right
/// (`\`, the thick stroke of a classic V or A) and stops the other against it
/// (stroke-satin-V: the left arm down to the baseline; stroke-satin-A: the right leg up to the
/// apex's flat top, its first crossing level). The run-on arm's end edge there is dropped from
/// its sides ([`GlyphStrokes::sides_along`]), so its crossings start along that edge rather
/// than fanning into the tip. `into` arrives at the cut along piece `k`, `out` leaves it along
/// piece `k + 1` (y down).
fn slanted_run_on(k: usize, into: Vec2, out: Vec2) -> Option<(usize, Vec2)> {
    // Leaning opposite ways.
    if into.x * into.y * out.x * out.y >= 0.0 {
        return None;
    }
    Some(if into.x * into.y > 0.0 {
        (k, into)
    } else {
        (k + 1, out * -1.0)
    })
}

/// How far along `line` [`run_on_stems`] looks: half of it, at most [`STEM_REACH`].
fn reach_of(line: &[Vec2]) -> f64 {
    (0.5 * length(line)).min(STEM_REACH)
}

/// At a cut where an upright stem meets a slanted stroke (N's top-left and bottom-right, M's
/// apexes), PE-DESIGN sews the stem the whole way to the outline and stops the slanted
/// stroke's satin against it (stroke-satin-N, -M, #446): the stem's centre line runs on past
/// the cut to the outline (its end becomes [`StrokeEnd::Free`]) and the slanted piece's end
/// becomes [`StrokeEnd::Against`]. Otherwise the slanted piece's crossings fan into the corner
/// and the stem stops at the notch's depth (the spike of #446).
fn run_on_stems(
    pieces: &mut [(Vec<Vec2>, bool, [StrokeEnd; 2])],
    edges: &[(Vec2, Vec2)],
    step: f64,
) {
    let unit = |d: Vec2| {
        let l = d.length();
        (l > 1e-9).then(|| d * (1.0 / l))
    };
    let upright = |d: Vec2| d.x.abs() <= STEM_LEAN.to_radians().dsin();
    for k in 0..pieces.len().saturating_sub(1) {
        let (a, b) = (&pieces[k].0, &pieces[k + 1].0);
        if pieces[k].1 || a.len() < 2 || b.len() < 2 {
            continue;
        }
        let back: Vec<Vec2> = a.iter().rev().copied().collect();
        // Past the thinning's bend into the corner (up to 3 mm), over the next 4 mm.
        let span = |line: &[Vec2]| {
            let near = (0.2 * length(line)).min(30.0);
            point_at(line, near) - point_at(line, (near + STEM_REACH).min(0.7 * length(line)))
        };
        let (Some(into), Some(out)) = (unit(span(&back)), unit(span(b) * -1.0)) else {
            continue;
        };
        // The stem runs on: on along `into` (piece k) or back along `out` (piece k + 1).
        let (stem, dir) = match (upright(into), upright(out)) {
            (true, false) => (k, into),
            (false, true) => (k + 1, out * -1.0),
            _ => match slanted_run_on(k, into, out) {
                Some(found) => found,
                None => continue,
            },
        };
        // The stem from the corner out to its far end.
        let stem_line: Vec<Vec2> = if stem == k {
            pieces[k].0.iter().rev().copied().collect()
        } else {
            pieces[k + 1].0.clone()
        };
        // A point on the stem's straight stretch, its half width there (its clearance), and
        // the distance of a point from the stem's centre line (through `anchor` along `dir`).
        let anchor = point_at(&stem_line, reach_of(&stem_line));
        let half = edges
            .iter()
            .map(|&(p, q)| segment_distance(anchor, p, q))
            .fold(f64::INFINITY, f64::min);
        let off = |p: Vec2| ((p.x - anchor.x) * dir.y - (p.y - anchor.y) * dir.x).abs();
        // Where the thinning bends the stem's end off its line into the corner (M's top
        // corners, whose notch knots are pruned, #446), the bend is dropped: the stem runs on
        // from its own line.
        // (Not for a slanted run-on, V's straight arms: #446.)
        let bent = if upright(dir) {
            stem_line
                .iter()
                .take_while(|&&p| off(p) > STEM_BEND * half)
                .count()
        } else {
            0
        };
        if bent + 1 >= stem_line.len() {
            continue;
        }
        let foot = stem_line[bent];
        let base = anchor + dir * ((foot.x - anchor.x) * dir.x + (foot.y - anchor.y) * dir.y);
        let Some(t) = ray_to(base, dir, edges) else {
            continue;
        };
        let reach = t - step * 0.5;
        if reach <= step {
            continue;
        }
        let tip = base + dir * reach;
        let mut run_on = vec![tip, base];
        run_on.extend(stem_line[bent + 1..].iter().copied());
        let slanted: Vec<Vec2> = if stem == k {
            pieces[k + 1].0.clone()
        } else {
            pieces[k].0.iter().rev().copied().collect()
        };
        let total = length(&slanted);
        // A slanted run-on stops the other arm nearer its centre line ([`SLANTED_AGAINST`]).
        let stop = half * if upright(dir) { 1.0 } else { SLANTED_AGAINST };
        let mut cut = None;
        let mut at = 0.0;
        for w in slanted.windows(2) {
            let (p, q) = (w[0], w[1]);
            let seg = p.distance(q);
            if off(q) >= stop && seg > 1e-9 {
                let (fp, fq) = (off(p), off(q));
                let t = if fq > fp {
                    ((stop - fp) / (fq - fp)).clamp(0.0, 1.0)
                } else {
                    1.0
                };
                cut = Some(at + seg * t);
                break;
            }
            at += seg;
        }
        let trimmed: Option<Vec<Vec2>> = cut.filter(|&d| d > 0.0 && d < 0.5 * total).map(|d| {
            let mut v = vec![point_at(&slanted, d)];
            let mut run = 0.0;
            for w in slanted.windows(2) {
                run += w[0].distance(w[1]);
                if run > d + 1e-6 {
                    v.push(w[1]);
                }
            }
            v
        });
        if stem == k {
            run_on.reverse();
            pieces[k].0 = run_on;
            pieces[k].2[1] = StrokeEnd::Free;
            pieces[k + 1].2[0] = StrokeEnd::Against;
            if let Some(v) = trimmed {
                pieces[k + 1].0 = v;
            }
        } else {
            pieces[k + 1].0 = run_on;
            pieces[k + 1].2[0] = StrokeEnd::Free;
            pieces[k].2[1] = StrokeEnd::Against;
            if let Some(mut v) = trimmed {
                v.reverse();
                pieces[k].0 = v;
            }
        }
    }
}

/// Distance from `p` along `dir` (unit) to the nearest edge, if one is hit.
fn ray_to(p: Vec2, dir: Vec2, edges: &[(Vec2, Vec2)]) -> Option<f64> {
    edges
        .iter()
        .filter_map(|&(a, b)| {
            let e = b - a;
            let den = dir.x * e.y - dir.y * e.x;
            if den.abs() < 1e-12 {
                return None;
            }
            let w = a - p;
            let t = (w.x * e.y - w.y * e.x) / den;
            let u = (w.x * dir.y - w.y * dir.x) / den;
            (t > 1e-9 && (-1e-9..=1.0 + 1e-9).contains(&u)).then_some(t)
        })
        .min_by(f64::total_cmp)
}

/// Where the ray from `p` along `dir` (any length) first meets the polyline `line`.
fn line_hits(line: &[Vec2], p: Vec2, dir: Vec2) -> Option<Vec2> {
    let unit = dir * (1.0 / dir.length().max(1e-12));
    let edges: Vec<(Vec2, Vec2)> = line.windows(2).map(|w| (w[0], w[1])).collect();
    ray_to(p, unit, &edges).map(|t| p + unit * t)
}

// ------------------------------------------------------------------ rungs

/// Outline points are taken at most this far apart (0.1 mm) when the sides are found.
const SIDE_STEP: f64 = 2.0;
/// A free end's centre point this near the outline (0.1 mm) ends on an end edge.
const EDGE_NEAR: f64 = 3.0;
/// Outline points this near a straight end edge (0.1 mm) lie on it; its vertices may stray this
/// far from the line between its corners.
const EDGE_ON: f64 = 0.3;

/// An end edge crosses the stroke at least this steeply (the sine of its angle to the stroke's
/// direction at the end): a stroke whose thinned end hooks into a corner (S's top) ends near
/// its side too.
const EDGE_ACROSS: f64 = 0.5;

/// The straight end edge a free end at `p`, arriving in direction `dir` (unit), reaches: the
/// run of outline vertices on one line through the outline edge near `p` most square to `dir`,
/// at most `limit` long; its two corners. `None` where `p` is not on the outline or no edge
/// near it crosses the stroke.
fn end_edge(outlines: &[Vec<Vec2>], p: Vec2, dir: Vec2, limit: f64) -> Option<(Vec2, Vec2)> {
    let across = |a: Vec2, b: Vec2| {
        let e = b - a;
        (dir.x * e.y - dir.y * e.x).abs() / e.length().max(1e-12)
    };
    let (c, i, _) = outlines
        .iter()
        .enumerate()
        .flat_map(|(c, pts)| {
            let n = pts.len();
            (0..n).filter_map(move |i| {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                (segment_distance(p, a, b) <= EDGE_NEAR).then(|| (c, i, across(a, b)))
            })
        })
        .max_by(|a, b| a.2.total_cmp(&b.2))
        .filter(|m| m.2 >= EDGE_ACROSS)?;
    let pts = &outlines[c];
    let n = pts.len();
    if n < 3 {
        return None;
    }
    let at = |k: isize| pts[k.rem_euclid(n as isize) as usize];
    let straight = |lo: isize, hi: isize| {
        let (a, b) = (at(lo), at(hi));
        hi - lo < n as isize
            && a.distance(b) <= limit
            && (lo + 1..hi).all(|k| segment_distance(at(k), a, b) <= EDGE_ON)
    };
    let (mut lo, mut hi) = (i as isize, i as isize + 1);
    if at(lo).distance(at(hi)) > limit {
        return None;
    }
    while straight(lo, hi + 1) {
        hi += 1;
    }
    while straight(lo - 1, hi) {
        lo -= 1;
    }
    Some((at(lo), at(hi)))
}

/// Where `p` projects onto the polyline `line`: (arc length along it, distance, signed side:
/// positive on the left of its direction with +y down, i.e. counter-clockwise on screen).
fn project(line: &[Vec2], closed: bool, p: Vec2) -> (f64, f64, f64) {
    let n = line.len();
    let segments = if closed { n } else { n.saturating_sub(1) };
    let mut best = (0.0, f64::INFINITY, 0.0);
    let mut at = 0.0;
    for i in 0..segments {
        let (a, b) = (line[i], line[(i + 1) % n]);
        let d = b - a;
        let len2 = d.x * d.x + d.y * d.y;
        let t = if len2 > 0.0 {
            (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let q = a + d * t;
        let dist = p.distance(q);
        let len = len2.sqrt();
        if dist < best.1 {
            let cross = d.x * (p.y - a.y) - d.y * (p.x - a.x);
            best = (at + t * len, dist, cross.signum());
        }
        at += len;
    }
    best
}

/// A polyline's point at arc length `s` (clamped to its ends).
fn point_at(line: &[Vec2], s: f64) -> Vec2 {
    let mut rest = s.max(0.0);
    for w in line.windows(2) {
        let len = w[0].distance(w[1]);
        if rest <= len {
            return if len > 0.0 {
                w[0].lerp(w[1], rest / len)
            } else {
                w[0]
            };
        }
        rest -= len;
    }
    line.last().copied().unwrap_or(v2(0.0, 0.0))
}

/// The point of a side (points with their place along the stroke, non-decreasing) at fraction
/// `f` of the way along it: its first point at 0, its last at 1, and in between the point at
/// that fraction of the way from its first place to its last, not counting an end corner (the
/// corner of an end edge projects onto the centre line's very end, so a straight stroke cut
/// slanting, X's legs, would stand still there and fan). A side that does not progress is
/// taken by length instead.
fn progress_at(side: &[(f64, Vec2)], f: f64) -> Vec2 {
    let n = side.len();
    let pts = || side.iter().map(|q| q.1).collect::<Vec<_>>();
    let ends = (side[0].0, side[n - 1].0);
    let inner: Vec<(f64, Vec2)> = side
        .iter()
        .copied()
        .enumerate()
        .filter(|&(i, q)| !(i == 0 && q.0 <= ends.0 + 1e-6 || i == n - 1 && q.0 >= ends.1 - 1e-6))
        .map(|(_, q)| q)
        .collect();
    if f <= 0.0 {
        return side[0].1;
    }
    if f >= 1.0 {
        return side[n - 1].1;
    }
    let (Some(first), Some(last)) = (inner.first(), inner.last()) else {
        let p = pts();
        return point_at(&p, f * length(&p));
    };
    if last.0 - first.0 <= 1e-6 {
        let p = pts();
        return point_at(&p, f * length(&p));
    }
    // The end corners stand before and after the inner points.
    let mut walk: Vec<(f64, Vec2)> = Vec::with_capacity(n);
    let span = last.0 - first.0;
    let lead = side[0].1.distance(first.1);
    let tail = side[n - 1].1.distance(last.1);
    let total = span + lead + tail;
    if lead > 1e-9 {
        walk.push((0.0, side[0].1));
    }
    walk.extend(inner.iter().map(|&(a, p)| (lead + a - first.0, p)));
    if tail > 1e-9 {
        walk.push((total, side[n - 1].1));
    }
    let at = f * total;
    for w in walk.windows(2) {
        let ((a, p), (b, q)) = (w[0], w[1]);
        if b >= at {
            let t = if b - a > 1e-9 {
                (at - a) / (b - a)
            } else {
                1.0
            };
            return p.lerp(q, t.clamp(0.0, 1.0));
        }
    }
    walk.last().map_or(side[n - 1].1, |q| q.1)
}

fn length(line: &[Vec2]) -> f64 {
    line.windows(2).map(|w| w[0].distance(w[1])).sum()
}

impl GlyphStrokes {
    /// Each stroke's two side edges (the outline that is nearer to this stroke than to any
    /// other, on either side of its centre line), each ordered along the stroke and kept over
    /// the stretch both sides cover. `[left, right]` per stroke; empty sides where the stroke
    /// has no outline of its own.
    pub fn sides(&self, contours: &[Contour]) -> Vec<[Vec<Vec2>; 2]> {
        self.sides_along(contours, Pairing::Progress)
            .into_iter()
            .map(|sides| sides.map(|v| v.into_iter().map(|(_, p)| p).collect()))
            .collect()
    }

    /// Whether `st` runs through junction `j` (a column joined there) rather than ending there.
    fn through(&self, st: &Stroke, j: usize) -> bool {
        let at = self.junctions[j].at;
        !st.closed
            && !st.ends.contains(&StrokeEnd::Junction(j))
            && st.centre.first().is_some_and(|p| p.distance(at) > 1e-6)
            && project(&st.centre, false, at).1 < 0.5 * self.junctions[j].radius
    }

    /// [`Self::sides`] with each point's place along the stroke: the arc length of its
    /// projection onto the centre line (0.1 mm), non-decreasing along each side.
    /// Fill text (`Pairing::Length`) keeps the sides its rows were measured with (#376): the
    /// end-cut pairing of satin (#446) is not applied to it.
    fn sides_along(&self, contours: &[Contour], pairing: Pairing) -> Vec<[Vec<(f64, Vec2)>; 2]> {
        let satin = pairing == Pairing::Progress;
        let mut sides: Vec<[Vec<(f64, f64, Vec2)>; 2]> = self
            .strokes
            .iter()
            .map(|_| [Vec::new(), Vec::new()])
            .collect();
        for c in contours.iter().filter(|c| c.closed) {
            let pts = c.outline_points();
            let n = pts.len();
            for i in 0..n {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                let pieces = (a.distance(b) / SIDE_STEP).ceil().max(1.0) as usize;
                for k in 0..pieces {
                    let p = a.lerp(b, k as f64 / pieces as f64);
                    let Some((s, (at, dist, side))) = self
                        .strokes
                        .iter()
                        .enumerate()
                        .map(|(s, st)| (s, project(&st.centre, st.closed, p)))
                        .min_by(|x, y| x.1.1.total_cmp(&y.1.1))
                    else {
                        continue;
                    };
                    let which = usize::from(side < 0.0);
                    sides[s][which].push((at, dist, p));
                }
            }
        }
        // Junctions where four arms cross (X): four stroke ends, or two and a column through,
        // every arm ending free at its far end (not M's notch loops or joined junctions, #446).
        let crossings: Vec<usize> = (0..self.junctions.len())
            .filter(|&j| {
                let at_j = StrokeEnd::Junction(j);
                let ending: Vec<&Stroke> = self
                    .strokes
                    .iter()
                    .filter(|o| o.ends.contains(&at_j))
                    .collect();
                let through: Vec<&Stroke> =
                    self.strokes.iter().filter(|o| self.through(o, j)).collect();
                let free = ending
                    .iter()
                    .all(|o| o.ends[0] != o.ends[1] && o.ends.contains(&StrokeEnd::Free))
                    && through.iter().all(|o| o.ends == [StrokeEnd::Free; 2]);
                ending.len() + 2 * through.len() == 4 && free
            })
            .collect();
        let outlines: Vec<Vec<Vec2>> = contours
            .iter()
            .filter(|c| c.closed)
            .map(Contour::outline_points)
            .collect();
        // Where a stroke stops against another (#446).
        let stops: Vec<Vec2> = self
            .strokes
            .iter()
            .filter(|s| !s.centre.is_empty())
            .flat_map(|s| {
                [(0, s.centre[0]), (1, s.centre[s.centre.len() - 1])]
                    .into_iter()
                    .filter(|&(e, _)| s.ends[e] == StrokeEnd::Against)
                    .map(|(_, p)| p)
            })
            .collect();
        sides
            .into_iter()
            .zip(&self.strokes)
            .map(|([l, r], st)| {
                let total = st.length();
                // A free end's straight end edge, between its two corners: a level cut across
                // a slanted arm (X's, A's and V's feet) projects mostly inside the centre line's
                // end, and would start one side along the edge and fan the crossings there.
                let limit = 2.0 * st.widths.iter().copied().fold(0.0, f64::max);
                // Kept to the arms of a four-arm crossing (X) and to slanted arms another stops
                // against ([`slanted_run_on`]: V's left arm, A's right leg), whose crossings
                // PE-DESIGN starts along the level end (A's apex top, #446). On every letter
                // it moved S's, f's and 5's ends and took needle points off S, M and fill V
                // against PE-DESIGN (#446 notes).
                let crossing_arm = crossings
                    .iter()
                    .any(|&j| st.ends.contains(&StrokeEnd::Junction(j)) || self.through(st, j));
                let run_on_arm = st.centre.len() >= 2 && {
                    let d = st.centre[st.centre.len() - 1] - st.centre[0];
                    let slanted = d.x.abs() > STEM_LEAN.to_radians().dsin() * d.length();
                    satin
                        && slanted
                        && stops
                            .iter()
                            .any(|&s| project(&st.centre, false, s).1 <= 0.5 * limit)
                };
                let edges: Vec<(f64, Vec2, Vec2)> =
                    if st.closed || st.centre.len() < 2 || !(crossing_arm || run_on_arm) {
                        Vec::new()
                    } else {
                        [(0, 0.0), (1, total)]
                            .into_iter()
                            .filter(|&(e, _)| st.ends[e] == StrokeEnd::Free)
                            .filter_map(|(e, at)| {
                                let along: Vec<Vec2> = if e == 0 {
                                    st.centre.iter().rev().copied().collect()
                                } else {
                                    st.centre.clone()
                                };
                                let p = along[along.len() - 1];
                                let reach = (0.5 * limit).min(total);
                                let d = p - point_at(&along, total - reach);
                                let dir = d * (1.0 / d.length().max(1e-12));
                                end_edge(&outlines, p, dir, limit).map(|(a, b)| (at, a, b))
                            })
                            .collect()
                    };
                // An end edge projects onto the centre line's end point: keep only its corner
                // (the outermost point there) on each side, not the edge between the corners.
                let tidy = |v: Vec<(f64, f64, Vec2)>| -> Vec<(f64, f64, Vec2)> {
                    let mut v: Vec<(f64, f64, Vec2)> = v
                        .into_iter()
                        .filter_map(|(at, d, p)| {
                            for &(end, a, b) in &edges {
                                if p.distance(a) <= EDGE_ON || p.distance(b) <= EDGE_ON {
                                    return Some((end, d, p));
                                }
                                if segment_distance(p, a, b) <= EDGE_ON {
                                    return None;
                                }
                            }
                            Some((at, d, p))
                        })
                        .collect();
                    v.sort_by(|a, b| a.0.total_cmp(&b.0));
                    if st.closed {
                        return v;
                    }
                    let at_end = |at: f64| at <= 1e-6 || at >= total - 1e-6;
                    let corner = |pick_start: bool| {
                        v.iter()
                            .filter(|q| at_end(q.0) && (q.0 <= 1e-6) == pick_start)
                            .max_by(|a, b| a.1.total_cmp(&b.1))
                            .copied()
                    };
                    // An end against a stem (#446) keeps no corner: the satin starts where
                    // both sides run, not in a fan towards the stem's far corner.
                    let (first, last) = (
                        corner(true).filter(|_| st.ends[0] != StrokeEnd::Against),
                        corner(false).filter(|_| st.ends[1] != StrokeEnd::Against),
                    );
                    let mut out: Vec<(f64, f64, Vec2)> = first.into_iter().collect();
                    out.extend(v.iter().copied().filter(|q| !at_end(q.0)));
                    out.extend(last);
                    out
                };
                let (l, r) = (tidy(l), tidy(r));
                // The stretch both sides cover.
                let (lo, hi) = match (l.first(), l.last(), r.first(), r.last()) {
                    (Some(a), Some(b), Some(c), Some(d)) => (a.0.max(c.0), b.0.min(d.0)),
                    _ => return [Vec::new(), Vec::new()],
                };
                // A free end's corner stays even past the stretch both sides cover: on an end
                // edge the warp tilts (#428), the leading corner projects past the other side's
                // last point, and dropping it left the foot's lower corner unsewn. An end
                // stopped against a run-on arm keeps each side to where it meets that arm, so
                // the last crossing lies along the stop line, as PE-DESIGN's do (V's right arm
                // turns to 60 degrees there, along the left arm's inner edge, #446).
                // The same at an end that leaves a column there (A's bar at each leg, #938):
                // PE-DESIGN's bar crossings start and end along the legs' inner edges, from the
                // bar's top corner to its bottom one (110 and 70 degrees on stroke-satin-A).
                // Not at a four-arm crossing (X), whose arms end as their own branch sews them.
                let leaves_column = |e: usize| match st.ends[e] {
                    StrokeEnd::Junction(j) => {
                        !crossings.contains(&j)
                            && self
                                .strokes
                                .iter()
                                .any(|o| !std::ptr::eq(o, st) && self.through(o, j))
                    }
                    _ => false,
                };
                let against =
                    |e: usize| satin && (st.ends[e] == StrokeEnd::Against || leaves_column(e));
                let keep = |v: Vec<(f64, f64, Vec2)>| -> Vec<(f64, Vec2)> {
                    let n = v.len();
                    v.into_iter()
                        .enumerate()
                        .filter(|&(i, (at, _, _))| {
                            ((at >= lo - SIDE_STEP || against(0))
                                && (at <= hi + SIDE_STEP || against(1)))
                                || (i == 0 && st.ends[0] == StrokeEnd::Free)
                                || (i + 1 == n && st.ends[1] == StrokeEnd::Free)
                        })
                        .map(|(_, (at, _, p))| (at, p))
                        .collect()
                };
                [keep(l), keep(r)]
            })
            .collect()
    }

    /// Each stroke's rungs `spacing` apart (0.1 mm along its longer side): the two side edges
    /// joined at equal fractions of the centre line's progress over each side, from its first
    /// point to its last. On a straight stroke that is the fraction of the side's length, as
    /// PE-DESIGN 11.50 joins them (its Convert to Blocks lines on A's slanted right leg are
    /// about horizontal, not across the leg); where the stroke bends, the side on the outside
    /// of the bend is much longer than the one inside, and joining them by length would slant
    /// the rungs further and further along (S's spine, where PE-DESIGN's crossings stay across
    /// the stroke: stroke-satin-S, #444).
    pub fn rungs(&self, contours: &[Contour], spacing: f64) -> Vec<Vec<[Vec2; 2]>> {
        self.rungs_paired(contours, spacing, Pairing::Progress)
    }

    /// [`Self::rungs`] with the sides joined by `pairing`.
    pub fn rungs_paired(
        &self,
        contours: &[Contour],
        spacing: f64,
        pairing: Pairing,
    ) -> Vec<Vec<[Vec2; 2]>> {
        self.sides_along(contours, pairing)
            .into_iter()
            .zip(&self.strokes)
            .map(|([l, r], st)| {
                if l.len() < 2 || r.len() < 2 || spacing <= 0.0 {
                    return Vec::new();
                }
                let line = |v: &[(f64, Vec2)]| v.iter().map(|q| q.1).collect::<Vec<_>>();
                let (pl, pr) = (line(&l), line(&r));
                let (ll, lr) = (length(&pl), length(&pr));
                let n = (ll.max(lr) / spacing).ceil().clamp(1.0, 10_000.0) as usize;
                // Only a bending stroke needs the progress pairing: on a straight one the two
                // agree, except where a junction cuts one side short (A's legs at the bar),
                // where the progress pairing fans.
                let progress = pairing == Pairing::Progress && bends(st);
                (0..=n)
                    .map(|k| {
                        let f = k as f64 / n as f64;
                        if progress {
                            [progress_at(&l, f), progress_at(&r, f)]
                        } else {
                            [point_at(&pl, f * ll), point_at(&pr, f * lr)]
                        }
                    })
                    .collect()
            })
            .collect()
    }
}

/// A stroke bends when its centre line turns by more than this (degrees) at its median step,
/// measured between stretches [`BEND_STEP`] long: S's spine and bowls, O, C turn at nearly
/// every step; a straight stem or leg never, and a column through X's crossing only there.
const BEND: f64 = 4.0;
/// See [`BEND`] (0.1 mm).
const BEND_STEP: f64 = 10.0;

/// Whether `st` bends (see [`BEND`]).
fn bends(st: &Stroke) -> bool {
    if st.closed {
        return true;
    }
    let total = length(&st.centre);
    let steps = (total / BEND_STEP).floor() as usize;
    if steps < 2 {
        return false;
    }
    let pts: Vec<Vec2> = (0..=steps)
        .map(|k| point_at(&st.centre, total * k as f64 / steps as f64))
        .collect();
    let mut turns: Vec<f64> = pts
        .windows(3)
        .map(|w| {
            let (u, v) = (w[1] - w[0], w[2] - w[1]);
            let cos = (u.x * v.x + u.y * v.y) / (u.length() * v.length()).max(1e-12);
            cos.clamp(-1.0, 1.0).dacos().to_degrees()
        })
        .collect();
    // A hairpin (a corner left uncut, W's middle apex and M's middle V, #1064) is paired by the
    // centre line's progress too: PE-DESIGN's crossings stay across each diagonal and fan round
    // the apex, the notch side waiting at the notch's tip while the outer side runs over the top.
    let wide = (total / HAIRPIN_STEP).floor() as usize;
    if wide >= 2 {
        let at: Vec<Vec2> = (0..=wide)
            .map(|k| point_at(&st.centre, total * k as f64 / wide as f64))
            .collect();
        let hairpin = at.windows(3).any(|w| {
            let (u, v) = (w[1] - w[0], w[2] - w[1]);
            let cos = (u.x * v.x + u.y * v.y) / (u.length() * v.length()).max(1e-12);
            cos.clamp(-1.0, 1.0).dacos().to_degrees() > HAIRPIN
        });
        if hairpin {
            return true;
        }
    }
    turns.sort_by(f64::total_cmp);
    turns.get(turns.len() / 2).is_some_and(|&t| t > BEND)
}

/// A centre line turning by more than this (degrees) between two [`HAIRPIN_STEP`] stretches
/// doubles back: a hairpin ([`bends`]).
const HAIRPIN: f64 = 100.0;
/// See [`HAIRPIN`] (0.1 mm): the thinning rounds a V's tip over a few millimetres.
const HAIRPIN_STEP: f64 = 30.0;

// ------------------------------------------------------------------ order

/// How a stroke's two sides are joined into rungs (see [`GlyphStrokes::rungs`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pairing {
    /// At equal fractions of the centre line's progress: crossings stay across a bending
    /// stroke, as PE-DESIGN's satin S (the default).
    Progress,
    /// At equal fractions of each side's length: the rows of fill text, whose spacing was
    /// measured against PE-DESIGN's fill captures with them (#376).
    Length,
}

/// Two strokes through a junction whose directions out of it differ from straight by at most
/// this (degrees) are one column through it (H's stems, A's legs, T's bar).
const THROUGH: f64 = 40.0;

/// One column of a letter as PE-DESIGN 11.50 sews it (its Convert to Blocks blocks): rungs in
/// sewing order, each `[left, right]` across the column.
#[derive(Clone, Debug, PartialEq)]
pub struct Block {
    pub rungs: Vec<[Vec2; 2]>,
    /// The junction square: the piece of a column under the stroke that leaves it there.
    pub junction: bool,
    /// The first block of a piece not joined to the one before (an i's dot, X's second
    /// half): PE-DESIGN trims before it.
    pub new_run: bool,
    /// Where the needle runs straight from, under the block's satin, to its first rung instead
    /// of along the outline: X's arms are sewn from their free ends in to the crossing, and
    /// PE-DESIGN runs out along each arm first (stroke-satin-X). A new run starts there.
    pub lead: Option<Vec2>,
}

/// A stroke's end: `(stroke, 0 or 1)`.
type StrokeAt = (usize, usize);

/// Strokes sewn as one column: `(stroke, reversed)` along it, and the junction between each
/// two.
struct Column {
    parts: Vec<(usize, bool)>,
    through: Vec<usize>,
}

impl GlyphStrokes {
    /// The direction a stroke leaves junction end `end` in (towards its inside).
    fn out_of(&self, s: usize, end: usize) -> Vec2 {
        let c = &self.strokes[s].centre;
        let (from, along): (Vec2, Vec<Vec2>) = if end == 0 {
            (c[0], c.clone())
        } else {
            (c[c.len() - 1], c.iter().rev().copied().collect())
        };
        // A point about a stroke width in, so the junction's own bend does not count.
        let reach = self.strokes[s]
            .widths
            .iter()
            .copied()
            .fold(0.0, f64::max)
            .max(10.0);
        let to = point_at(&along, reach.min(length(&along)));
        let d = to - from;
        let len = d.length();
        if len > 1e-9 {
            d * (1.0 / len)
        } else {
            v2(0.0, 0.0)
        }
    }

    /// The columns: strokes through a junction that continue each other straight are joined.
    fn columns(&self) -> Vec<Column> {
        let n = self.strokes.len();
        // next[s][end] = (stroke, its end, junction) joined at that end.
        let mut next: Vec<[Option<(usize, usize, usize)>; 2]> = vec![[None, None]; n];
        for j in 0..self.junctions.len() {
            let at: Vec<(usize, usize)> = (0..n)
                .flat_map(|s| (0..2).map(move |e| (s, e)))
                .filter(|&(s, e)| {
                    !self.strokes[s].closed && self.strokes[s].ends[e] == StrokeEnd::Junction(j)
                })
                .collect();
            let mut best: Option<(f64, StrokeAt, StrokeAt)> = None;
            for (i, &a) in at.iter().enumerate() {
                for &b in &at[i + 1..] {
                    if a.0 == b.0 {
                        continue;
                    }
                    let (da, db) = (self.out_of(a.0, a.1), self.out_of(b.0, b.1));
                    let cos = (-(da.x * db.x + da.y * db.y)).clamp(-1.0, 1.0);
                    let off = cos.dacos().to_degrees();
                    if off <= THROUGH && best.is_none_or(|(o, _, _)| off < o) {
                        best = Some((off, a, b));
                    }
                }
            }
            // A crossing of four arms (X): the diagonal from the arm nearest the top-left is one
            // column through it, the other two arms end there (stroke-satin-X: PE-DESIGN sews
            // one diagonal through and the other in two halves meeting it, #446).
            if at.len() == 4
                && at
                    .iter()
                    .all(|&(s, e)| self.strokes[s].ends[1 - e] == StrokeEnd::Free)
            {
                let far = |(s, e): StrokeAt| {
                    let c = &self.strokes[s].centre;
                    let p = if e == 0 { c[c.len() - 1] } else { c[0] };
                    p.x + p.y
                };
                let first = *at
                    .iter()
                    .min_by(|a, b| far(**a).total_cmp(&far(**b)))
                    .expect("four arms");
                let d = self.out_of(first.0, first.1);
                let opposite = at
                    .iter()
                    .filter(|&&o| o != first)
                    .min_by(|a, b| {
                        let dot = |o: &StrokeAt| {
                            let v = self.out_of(o.0, o.1);
                            v.x * d.x + v.y * d.y
                        };
                        dot(a).total_cmp(&dot(b))
                    })
                    .copied();
                if let Some(o) = opposite.filter(|o| o.0 != first.0) {
                    next[first.0][first.1] = Some((o.0, o.1, j));
                    next[o.0][o.1] = Some((first.0, first.1, j));
                    continue;
                }
            }
            // Any other junction of four (M's notch loops, g's and Pacifico's joins): two
            // chevrons, the arms on each side one column turning through it, as before #446.
            if at.len() == 4 {
                let mut arms = at.clone();
                arms.sort_by(|a, b| self.out_of(a.0, a.1).x.total_cmp(&self.out_of(b.0, b.1).x));
                if arms[0].0 != arms[1].0 && arms[2].0 != arms[3].0 {
                    for (a, b) in [(arms[0], arms[1]), (arms[2], arms[3])] {
                        next[a.0][a.1] = Some((b.0, b.1, usize::MAX));
                        next[b.0][b.1] = Some((a.0, a.1, usize::MAX));
                    }
                    continue;
                }
            }
            if let Some((_, a, b)) = best {
                next[a.0][a.1] = Some((b.0, b.1, j));
                next[b.0][b.1] = Some((a.0, a.1, j));
            }
        }
        let mut used = vec![false; n];
        let mut out = Vec::new();
        // Chains from an unjoined end first, then what is left (a ring through junctions).
        let starts = (0..n)
            .flat_map(|s| (0..2).map(move |e| (s, e)))
            .filter(|&(s, e)| next[s][e].is_none())
            .chain((0..n).map(|s| (s, 0)));
        for (s, e) in starts {
            if used[s] {
                continue;
            }
            let mut col = Column {
                parts: Vec::new(),
                through: Vec::new(),
            };
            // Enter stroke s at end e.
            let (mut s, mut e) = (s, e);
            loop {
                used[s] = true;
                col.parts.push((s, e == 1));
                match next[s][1 - e] {
                    Some((t, te, j)) if !used[t] => {
                        // A chevron joins its arms without making them branch there.
                        if j != usize::MAX {
                            col.through.push(j);
                        }
                        (s, e) = (t, te);
                    }
                    _ => break,
                }
            }
            out.push(col);
        }
        out
    }

    /// A column's centre line and widths, start to end.
    fn column_stroke(&self, col: &Column) -> Stroke {
        let mut centre: Vec<Vec2> = Vec::new();
        let mut widths = Vec::new();
        for &(s, rev) in &col.parts {
            let st = &self.strokes[s];
            let mut c = st.centre.clone();
            let mut w = st.widths.clone();
            if rev {
                c.reverse();
                w.reverse();
            }
            // Joined at the junction's centre: one point there.
            let skip = usize::from(!centre.is_empty());
            centre.extend(c.into_iter().skip(skip));
            widths.extend(w.into_iter().skip(skip));
        }
        let first = col.parts[0];
        let last = col.parts[col.parts.len() - 1];
        let end =
            |(s, rev): (usize, bool), start: bool| self.strokes[s].ends[usize::from(rev == start)];
        Stroke {
            centre,
            widths,
            closed: col.parts.len() == 1 && self.strokes[first.0].closed,
            ends: [end(first, true), end(last, false)],
        }
    }

    /// The letter's blocks in PE-DESIGN 11.50's sewing order, rungs `spacing` apart (0.1 mm).
    ///
    /// PE-DESIGN's Convert to Blocks of Lato's H and A (stroke-blocks-H, -A) gives the rule:
    /// strokes through a junction that continue each other straight are one column; sewing
    /// starts at the column end nearest the letter's top-left; a column that a later stroke
    /// leaves from is sewn to that junction, then from its far end back to it, then the junction
    /// square, and the stroke leaving it follows; a column entered at a junction with nothing
    /// left to leave by is sewn whole from its nearer end. H: the left stem's top, its bottom
    /// (upwards), the junction square, the bar, the right stem whole; A: the left leg's upper
    /// and lower parts, the square, the bar, the right leg whole with the apex.
    pub fn blocks(&self, contours: &[Contour], spacing: f64) -> Vec<Block> {
        self.blocks_paired(contours, spacing, Pairing::Progress)
    }

    /// [`Self::blocks`] with each stroke's sides joined by `pairing`.
    pub fn blocks_paired(
        &self,
        contours: &[Contour],
        spacing: f64,
        pairing: Pairing,
    ) -> Vec<Block> {
        let cols = self.columns();
        if cols.is_empty() || spacing <= 0.0 {
            return Vec::new();
        }
        let col_strokes: Vec<Stroke> = cols.iter().map(|c| self.column_stroke(c)).collect();
        let merged = GlyphStrokes {
            strokes: col_strokes,
            junctions: self.junctions.clone(),
        };
        let Some(b) = Rect::enclosing(merged.strokes.iter().flat_map(|s| s.centre.iter().copied()))
        else {
            return Vec::new();
        };
        let mut rungs = merged.rungs_paired(contours, spacing, pairing);
        let sewn = |rungs: &[Vec<[Vec2; 2]>]| {
            let mut order = Order {
                g: self,
                cols: &cols,
                strokes: &merged.strokes,
                rungs,
                done: vec![false; cols.len()],
                out: Vec::new(),
                at: v2(b.min_x, b.min_y),
                seq: Vec::new(),
            };
            // Each piece of the letter from the column end nearest where the sewing is.
            while let Some((c, nearest)) = order.nearest_end() {
                let end = order.leading_end(c, nearest);
                let first = order.out.len();
                order.visit(c, Entry::End(end));
                if first > 0
                    && let Some(block) = order.out.get_mut(first)
                {
                    block.new_run = true;
                }
                // The letter's first piece sewn from its far end (N's and M's left stem, from
                // its foot up to the diagonal): PE-DESIGN enters it at the end nearest the
                // letter's start and runs down under it first (stroke-satin-M, -M-8, -N, -N-8:
                // 3 to 10 stitches, #940).
                let s = &order.strokes[c];
                if first == 0
                    && end != nearest
                    && !s.closed
                    && s.centre.len() >= 2
                    && let Some(block) = order.out.get_mut(first)
                {
                    let tip = if nearest == 0 {
                        s.centre[0]
                    } else {
                        s.centre[s.centre.len() - 1]
                    };
                    block.lead = Some(tip);
                }
            }
            (order.out, order.seq)
        };
        let (_, seq) = sewn(&rungs);
        // A sharp corner (an A's apex, a V's foot) belongs to the column sewn last there: its
        // satin runs on into the tip (stroke-blocks-A: the right leg with the apex).
        let outline: Vec<Vec2> = contours
            .iter()
            .filter(|c| c.closed)
            .flat_map(Contour::outline_points)
            .collect();
        let rank = |c: usize| seq.iter().position(|&k| k == c).unwrap_or(usize::MAX);
        for (c, st) in merged.strokes.iter().enumerate() {
            for end in 0..2 {
                if st.ends[end] != StrokeEnd::Corner || st.centre.len() < 2 {
                    continue;
                }
                let along: Vec<Vec2> = if end == 0 {
                    st.centre.clone()
                } else {
                    st.centre.iter().rev().copied().collect()
                };
                let p = along[0];
                let q = point_at(&along, 20.0_f64.min(length(&along)));
                let partner_later = merged.strokes.iter().enumerate().any(|(k, o)| {
                    k != c
                        && rank(k) > rank(c)
                        && (0..2).any(|e| {
                            o.ends[e] == StrokeEnd::Corner
                                && o.centre[if e == 0 { 0 } else { o.centre.len() - 1 }].distance(p)
                                    < 10.0
                        })
                });
                if partner_later {
                    continue;
                }
                let out = p - q;
                let len = out.length();
                if len <= 1e-9 {
                    continue;
                }
                let dir = out * (1.0 / len);
                let reach = st.widths.iter().copied().fold(0.0, f64::max) * 1.5;
                let tip = outline
                    .iter()
                    .filter(|o| o.distance(p) <= reach)
                    .max_by(|a, b| {
                        let along = |o: &Vec2| (o.x - p.x) * dir.x + (o.y - p.y) * dir.y;
                        along(a).total_cmp(&along(b))
                    })
                    .copied();
                if let Some(tip) = tip.filter(|t| (t.x - p.x) * dir.x + (t.y - p.y) * dir.y > 0.0)
                    && !rungs[c].is_empty()
                {
                    if end == 0 {
                        rungs[c].insert(0, [tip, tip]);
                    } else {
                        rungs[c].push([tip, tip]);
                    }
                }
            }
        }
        sewn(&rungs).0
    }
}

/// Where a column is entered: at one of its ends, or at a junction it runs through.
#[derive(Clone, Copy)]
enum Entry {
    End(usize),
    Through,
}

struct Order<'a> {
    g: &'a GlyphStrokes,
    cols: &'a [Column],
    strokes: &'a [Stroke],
    rungs: &'a [Vec<[Vec2; 2]>],
    done: Vec<bool>,
    out: Vec<Block>,
    /// Where the needle is.
    at: Vec2,
    /// The columns in the order they are sewn.
    seq: Vec<usize>,
}

impl Order<'_> {
    /// The column end nearest the needle; of ends within 1 mm of the nearest (an A's two legs
    /// at its apex), the leftmost column's (PE-DESIGN starts the A on its left leg).
    fn nearest_end(&self) -> Option<(usize, usize)> {
        let ends: Vec<(f64, f64, usize, usize)> = self
            .strokes
            .iter()
            .enumerate()
            .filter(|&(c, s)| !self.done[c] && !s.centre.is_empty())
            .flat_map(|(c, s)| {
                let mid = s.centre.iter().map(|p| p.x).sum::<f64>() / s.centre.len() as f64;
                [(0, s.centre[0]), (1, s.centre[s.centre.len() - 1])]
                    .map(|(end, p)| (p.distance(self.at), mid, c, end))
            })
            .collect();
        let nearest = ends.iter().map(|e| e.0).fold(f64::INFINITY, f64::min);
        ends.into_iter()
            .filter(|e| e.0 <= nearest + 10.0)
            .min_by(|a, b| a.1.total_cmp(&b.1).then(a.0.total_cmp(&b.0)))
            .map(|e| (e.2, e.3))
    }

    /// Which end to sew a stand-alone column `c` (a piece of one stroke, no junction: N's
    /// stems and diagonal, cut apart at its acute corners) from: the one whose far end lands
    /// nearest another piece still to sew, so the needle runs on into it instead of trimming
    /// (PE-DESIGN sews satin N, W, K and Z in one run: stroke-satin-n, round 8, #446). The
    /// travel to the start counts too, except for the letter's first piece, which is jumped
    /// to anyway. Other columns keep `end`.
    fn leading_end(&self, c: usize, end: usize) -> usize {
        let s = &self.strokes[c];
        let alone = self.cols[c].parts.len() == 1
            && self.cols[c].through.is_empty()
            && !s.closed
            && s.centre.len() >= 2
            && !s.ends.iter().any(|e| matches!(e, StrokeEnd::Junction(_)));
        if !alone {
            return end;
        }
        let tip = |e: usize| {
            if e == 0 {
                s.centre[0]
            } else {
                s.centre[s.centre.len() - 1]
            }
        };
        let next = |p: Vec2| {
            self.strokes
                .iter()
                .enumerate()
                .filter(|&(k, o)| k != c && !self.done[k] && !o.centre.is_empty())
                .flat_map(|(_, o)| [o.centre[0], o.centre[o.centre.len() - 1]])
                .map(|q| q.distance(p))
                .fold(f64::INFINITY, f64::min)
        };
        let first = self.out.is_empty();
        let cost = |e: usize| {
            let lead = if first { 0.0 } else { tip(e).distance(self.at) };
            let after = next(tip(1 - e));
            lead + if after.is_finite() { after } else { 0.0 }
        };
        // Only a clear gain moves away from the nearest end (1 mm).
        if cost(1 - end) + 10.0 < cost(end) {
            1 - end
        } else {
            end
        }
    }

    /// Arc length along column `c` of each through junction.
    fn through_at(&self, c: usize) -> Vec<(usize, f64)> {
        let s = &self.strokes[c];
        self.cols[c]
            .through
            .iter()
            .map(|&j| (j, project(&s.centre, false, self.g.junctions[j].at).0))
            .collect()
    }

    /// The other columns at junction `j`, not sewn yet.
    fn waiting_at(&self, j: usize, except: usize) -> Vec<usize> {
        (0..self.cols.len())
            .filter(|&k| k != except && !self.done[k])
            .filter(|&k| {
                let s = &self.strokes[k];
                s.ends.contains(&StrokeEnd::Junction(j)) || self.cols[k].through.contains(&j)
            })
            .collect()
    }

    /// Half the width of the strokes leaving column `c` at junction `j`: the square's reach.
    fn square_half(&self, c: usize, j: usize) -> f64 {
        self.waiting_at(j, c)
            .iter()
            .map(|&k| {
                let s = &self.strokes[k];
                let near = if s.centre[0].distance(self.g.junctions[j].at)
                    <= s.centre[s.centre.len() - 1].distance(self.g.junctions[j].at)
                {
                    0
                } else {
                    s.centre.len() - 1
                };
                // The width a little way out of the junction, where the stroke is itself.
                let k_in = if near == 0 {
                    (s.widths.len() / 2).min(3)
                } else {
                    s.widths
                        .len()
                        .saturating_sub(1 + (s.widths.len() / 2).min(3))
                };
                s.widths.get(k_in).copied().unwrap_or(0.0) * 0.5
            })
            .fold(0.0, f64::max)
    }

    /// Where column `c`'s junction square at `j` starts and ends (arc lengths along it, lower
    /// first): at the corners where the straight strokes leaving it there meet its side, their
    /// end rungs' two points (#938). PE-DESIGN cuts A's left leg there, each cut a rung through
    /// one corner of the bar (stroke-satin-A: the upper part ends on (6.8, 12.8), the foot on
    /// (5.8, 15.5), the square between them, 18 crossings). `None` where a stroke leaving it
    /// bends (R's and P's bowls) or has no rungs: the square is then the leaving strokes' width
    /// about the junction ([`Self::square_half`]).
    fn corner_cut(&self, c: usize, j: usize) -> Option<(f64, f64)> {
        let at = self.g.junctions[j].at;
        let centre = &self.strokes[c].centre;
        let own = &self.rungs[c];
        if own.len() < 2 {
            return None;
        }
        // Column `c`'s two sides, as its rungs' ends.
        let sides: [Vec<Vec2>; 2] = [
            own.iter().map(|r| r[0]).collect(),
            own.iter().map(|r| r[1]).collect(),
        ];
        let mut arcs = Vec::new();
        for k in self.waiting_at(j, c) {
            let s = &self.strokes[k];
            let rs = &self.rungs[k];
            if bends(s) || rs.len() < 4 || s.centre.len() < 2 {
                return None;
            }
            let first = s.centre[0].distance(at) <= s.centre[s.centre.len() - 1].distance(at);
            for i in 0..2 {
                // The side's straight line through its middle rungs (a rounded inner corner
                // bends its last points down the column's edge: A's bar bottom), carried to
                // where it meets the column's side facing it: the corner.
                let n = rs.len();
                let (p, q) = (rs[n / 4][i], rs[3 * n / 4][i]);
                let (from, to) = if first { (q, p) } else { (p, q) };
                let d = to - from;
                if d.length() < 1e-9 {
                    return None;
                }
                let hit = sides
                    .iter()
                    .filter_map(|side| line_hits(side, from, d))
                    .min_by(|a, b| a.distance(to).total_cmp(&b.distance(to)))?;
                arcs.push(project(centre, false, hit).0);
            }
        }
        let lo = arcs.iter().copied().fold(f64::INFINITY, f64::min);
        let hi = arcs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        (lo.is_finite() && hi - lo > 1e-6).then_some((lo, hi))
    }

    /// Rungs of column `c` between arc lengths `from` and `to` (either order), in that order,
    /// with a rung at each end.
    fn piece(&self, c: usize, from: f64, to: f64) -> Vec<[Vec2; 2]> {
        let s = &self.strokes[c];
        let rs = &self.rungs[c];
        if rs.len() < 2 {
            return Vec::new();
        }
        let at: Vec<f64> = rs
            .iter()
            .map(|r| project(&s.centre, false, r[0].lerp(r[1], 0.5)).0)
            .collect();
        let rung_at = |x: f64| -> [Vec2; 2] {
            let k = at
                .iter()
                .position(|&a| a >= x)
                .unwrap_or(rs.len() - 1)
                .max(1);
            let (a0, a1) = (at[k - 1], at[k]);
            let t = if a1 - a0 > 1e-9 {
                ((x - a0) / (a1 - a0)).clamp(0.0, 1.0)
            } else {
                0.0
            };
            [
                rs[k - 1][0].lerp(rs[k][0], t),
                rs[k - 1][1].lerp(rs[k][1], t),
            ]
        };
        let (lo, hi) = (from.min(to), from.max(to));
        let lo = lo.max(at[0]);
        let hi = hi.min(at[at.len() - 1]);
        if hi - lo < 1e-6 {
            return Vec::new();
        }
        let mut v = vec![rung_at(lo)];
        v.extend(
            rs.iter()
                .zip(&at)
                .filter(|&(_, &a)| a > lo + 1e-6 && a < hi - 1e-6)
                .map(|(r, _)| *r),
        );
        v.push(rung_at(hi));
        if from > to {
            v.reverse();
        }
        v
    }

    /// [`Self::piece`] with its two sides joined again at equal fractions of their own lengths
    /// between the piece's end rungs, on a straight column (#938). The column's rungs join its
    /// sides over its whole length, so where a junction cuts one side short (A's left leg at the
    /// bar: its inner side stops 1.5 mm lower than the outer) the piece below inherits a tilt
    /// that grows towards the junction from the foot's middle on; PE-DESIGN's foot keeps its
    /// crossings nearly level up to the junction (stroke-satin-A: 6 degrees at the middle).
    fn piece_own(&self, c: usize, from: f64, to: f64) -> Vec<[Vec2; 2]> {
        let v = self.piece(c, from, to);
        if v.len() < 3 || bends(&self.strokes[c]) {
            return v;
        }
        let side = |i: usize| v.iter().map(|r| r[i]).collect::<Vec<Vec2>>();
        let (l, r) = (side(0), side(1));
        let (ll, lr) = (length(&l), length(&r));
        let n = v.len() - 1;
        (0..=n)
            .map(|k| {
                let f = k as f64 / n as f64;
                [point_at(&l, f * ll), point_at(&r, f * lr)]
            })
            .collect()
    }

    fn emit(&mut self, rungs: Vec<[Vec2; 2]>, junction: bool) {
        if let Some(last) = rungs.last() {
            self.at = last[0].lerp(last[1], 0.5);
            self.out.push(Block {
                rungs,
                junction,
                new_run: false,
                lead: None,
            });
        }
    }

    /// [`Self::emit`] reached by running straight from `lead` (see [`Block::lead`]).
    fn emit_led(&mut self, rungs: Vec<[Vec2; 2]>, lead: Vec2) {
        let first = self.out.len();
        self.emit(rungs, false);
        if let Some(block) = self.out.get_mut(first) {
            block.lead = Some(lead);
        }
    }

    fn visit(&mut self, c: usize, entry: Entry) {
        if self.done[c] {
            return;
        }
        self.done[c] = true;
        self.seq.push(c);
        // A ring (an O): one block round it.
        if self.strokes[c].closed {
            let ring = self.rungs[c].clone();
            self.emit(ring, false);
            return;
        }
        let len = self.strokes[c].length();
        let through = self.through_at(c);
        // The first junction along the column, from where it is entered, that another stroke
        // leaves from.
        let leave = match entry {
            Entry::End(e) => {
                let mut t: Vec<&(usize, f64)> = through
                    .iter()
                    .filter(|(j, _)| !self.waiting_at(*j, c).is_empty())
                    .collect();
                t.sort_by(|a, b| {
                    let key = |s: f64| if e == 0 { s } else { len - s };
                    key(a.1).total_cmp(&key(b.1))
                });
                t.first().map(|&&(j, s)| (j, s))
            }
            Entry::Through => None,
        };
        let mut sewn_through = Vec::new();
        let crossing = leave.and_then(|(j, _)| self.crossing_arms(c, j));
        match (entry, leave) {
            (Entry::End(e), Some((j, s))) if crossing.is_some() => {
                // X (stroke-satin-X): the column to the crossing, each arm from its free end in
                // to the crossing, the second after a trim, then the crossing square and the
                // rest of the column.
                let h = self.square_half(c, j);
                let (start, far) = if e == 0 { (0.0, len) } else { (len, 0.0) };
                let toward = if e == 0 { -1.0 } else { 1.0 };
                let near_edge = s + toward * h;
                let a = self.piece(c, start, near_edge);
                self.emit(a, false);
                for (i, (k, free)) in crossing.into_iter().flatten().enumerate() {
                    self.done[k] = true;
                    self.seq.push(k);
                    let lk = self.strokes[k].length();
                    let (from, to) = if free == 0 { (0.0, lk) } else { (lk, 0.0) };
                    let first = self.out.len();
                    let mut arm = self.piece(k, from, to);
                    // An arm one of whose sides reaches the crossing's corner while the other
                    // stops short (the corner's outline is the column's) ends in a rung of almost
                    // no width there (X's top-right arm), which would fan the crossings: its
                    // last rung joins the side that reaches the corner to where the other side
                    // stops, as PE-DESIGN's arm ends from corner to corner of the crossing.
                    let mut spans: Vec<f64> = arm.iter().map(|r| r[0].distance(r[1])).collect();
                    spans.sort_by(f64::total_cmp);
                    let full = spans.get(spans.len() / 2).copied().unwrap_or(0.0) * 0.25;
                    if let [.., prev, last] = arm[..]
                        && last[0].distance(last[1]) < full
                    {
                        let a = [last[0], prev[1]];
                        let b = [prev[0], last[1]];
                        let best = if a[0].distance(a[1]) >= b[0].distance(b[1]) {
                            a
                        } else {
                            b
                        };
                        let n = arm.len();
                        if best[0].distance(best[1]) >= full {
                            arm[n - 1] = best;
                        } else {
                            arm.pop();
                        }
                    }
                    // Out along the arm from its crossing end, under the satin to come.
                    let Some(inner) = arm.last().map(|r| r[0].lerp(r[1], 0.5)) else {
                        continue;
                    };
                    self.emit_led(arm, inner);
                    if i > 0
                        && let Some(block) = self.out.get_mut(first)
                    {
                        block.new_run = true;
                    }
                }
                // Across the crossing to its near edge, not round the outline.
                let rest = self.piece(c, near_edge, far);
                if let Some(start) = rest.first().map(|r| r[0].lerp(r[1], 0.5)) {
                    self.emit_led(rest, start);
                }
                sewn_through.push(j);
            }
            (Entry::End(e), Some((j, s))) => {
                let h = self.square_half(c, j);
                let (start, far) = if e == 0 { (0.0, len) } else { (len, 0.0) };
                let toward = if e == 0 { -1.0 } else { 1.0 };
                let (near_edge, far_edge) = match self.corner_cut(c, j) {
                    Some((lo, hi)) if e == 0 => (lo, hi),
                    Some((lo, hi)) => (hi, lo),
                    None => (s + toward * h, s - toward * h),
                };
                let a = self.piece_own(c, start, near_edge);
                self.emit(a, false);
                let b = self.piece_own(c, far, far_edge);
                self.emit(b, false);
                let sq = self.piece(c, near_edge, far_edge);
                self.emit(sq, true);
                sewn_through.push(j);
                for k in self.waiting_at(j, c) {
                    self.visit(k, self.entry_at(k, j));
                }
            }
            (Entry::End(e), None) => {
                let (a, b) = if e == 0 { (0.0, len) } else { (len, 0.0) };
                let whole = self.piece(c, a, b);
                self.emit(whole, false);
            }
            (Entry::Through, _) => {
                // Sewn whole from its upper end, however near the junction is to the other
                // (H's right stem, A's right leg from the apex: stroke-blocks-H, -A).
                let s = &self.strokes[c];
                let top_first = s.centre[0].y <= s.centre[s.centre.len() - 1].y;
                let (a, b) = if top_first { (0.0, len) } else { (len, 0.0) };
                let whole = self.piece(c, a, b);
                self.emit(whole, false);
            }
        }
        // Strokes still waiting at this column's other junctions, nearest the needle first.
        let mut js: Vec<usize> = self.cols[c]
            .through
            .iter()
            .copied()
            .chain(self.strokes[c].ends.iter().filter_map(|e| match e {
                StrokeEnd::Junction(j) => Some(*j),
                _ => None,
            }))
            .filter(|j| !sewn_through.contains(j))
            .collect();
        let here = self.at;
        js.sort_by(|a, b| {
            let d = |j: usize| self.g.junctions[j].at.distance(here);
            d(*a).total_cmp(&d(*b))
        });
        for j in js {
            for k in self.waiting_at(j, c) {
                self.visit(k, self.entry_at(k, j));
            }
        }
    }

    /// Where column `c` runs through junction `j` and two arms end there from either side of
    /// it, each with a free far end (X's crossing): those arms with their free end, the one
    /// whose free end is further left first. `None` elsewhere (H's bar leads on to the other
    /// stem, T's stem and K's arms leave one side only).
    fn crossing_arms(&self, c: usize, j: usize) -> Option<Vec<(usize, usize)>> {
        let waiting = self.waiting_at(j, c);
        if waiting.len() != 2 {
            return None;
        }
        let at = self.g.junctions[j].at;
        let s = &self.strokes[c];
        let (k_at, _, _) = project(&s.centre, false, at);
        let ahead = point_at(&s.centre, k_at + 10.0) - point_at(&s.centre, (k_at - 10.0).max(0.0));
        let mut arms = Vec::new();
        let mut sides = Vec::new();
        for &k in &waiting {
            let st = &self.strokes[k];
            if self.cols[k].parts.len() != 1 || st.closed || st.centre.len() < 2 {
                return None;
            }
            let free = match st.ends {
                [StrokeEnd::Junction(x), StrokeEnd::Free] if x == j => 1,
                [StrokeEnd::Free, StrokeEnd::Junction(x)] if x == j => 0,
                _ => return None,
            };
            let p = if free == 0 {
                st.centre[0]
            } else {
                st.centre[st.centre.len() - 1]
            };
            let v = p - at;
            sides.push((ahead.x * v.y - ahead.y * v.x).signum());
            arms.push((k, free, p.x));
        }
        if sides[0] == sides[1] {
            return None;
        }
        arms.sort_by(|a, b| a.2.total_cmp(&b.2));
        Some(arms.into_iter().map(|(k, f, _)| (k, f)).collect())
    }

    /// How column `k` is entered from junction `j`: at its end there, or through it.
    fn entry_at(&self, k: usize, j: usize) -> Entry {
        let s = &self.strokes[k];
        if s.ends[0] == StrokeEnd::Junction(j) {
            Entry::End(0)
        } else if s.ends[1] == StrokeEnd::Junction(j) {
            Entry::End(1)
        } else {
            Entry::Through
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::{DEFAULT_FONT, TextSpec, layout};

    /// #446: N's and M's stems run the whole height to the outline, straight, as PE-DESIGN
    /// sews them (stroke-satin-N, -M, and the 8 mm captures), and the diagonals stop against
    /// them: no stroke ends in a corner fan at a stem. M's notch knots are pruned, so its top
    /// corners are cuts, not junctions.
    #[test]
    fn n_and_m_stems_run_on_and_the_diagonals_stop_against_them() {
        for (ch, size) in [("N", 20.0), ("N", 8.0), ("M", 20.0), ("M", 8.0)] {
            let spec = TextSpec::new(ch, DEFAULT_FONT, size);
            let l = layout(&spec, None).expect("layout");
            let contours = l.contours();
            let b =
                Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
            let g = strokes(&contours).expect("strokes");
            let stems: Vec<&Stroke> = g
                .strokes
                .iter()
                .filter(|s| s.bounds().is_some_and(|r| r.height() > 0.95 * b.height()))
                .collect();
            assert_eq!(stems.len(), 2, "{ch} {size} mm: two full-height stems");
            for s in &stems {
                assert_eq!(s.ends, [StrokeEnd::Free; 2], "{ch} {size} mm");
                // Straight up: the thinning's bend into the corner is dropped.
                let x0 = s.centre[0].x;
                let stray = s
                    .centre
                    .iter()
                    .map(|p| (p.x - x0).abs())
                    .fold(0.0, f64::max);
                assert!(
                    stray < 0.5 * median(&s.widths),
                    "{ch} {size} mm: stem strays {stray}"
                );
            }
            assert!(g.junctions.is_empty(), "{ch} {size} mm: {:?}", g.junctions);
            let against = g
                .strokes
                .iter()
                .flat_map(|s| s.ends)
                .filter(|&e| e == StrokeEnd::Against)
                .count();
            // M's middle V too: one diagonal runs on to the baseline, the other stops
            // against it (`slanted_run_on`).
            let want_against = if ch == "M" { 3 } else { 2 };
            assert_eq!(
                against, want_against,
                "{ch} {size} mm: the diagonals stop against both stems"
            );
            let corners = g
                .strokes
                .iter()
                .flat_map(|s| s.ends)
                .filter(|&e| e == StrokeEnd::Corner)
                .count();
            // No cut is left as two corner ends: M's middle V is a run-on too.
            assert_eq!(corners, 0, "{ch} {size} mm");
        }
    }

    /// #446: at V's foot the left arm (falling to the right) runs on to the baseline and the
    /// right arm stops against it, as PE-DESIGN sews it (stroke-satin-V); at A's apex the right
    /// leg runs on to the apex's top and the left leg stops against it (stroke-satin-A).
    #[test]
    fn a_slanted_arm_runs_on_at_v_and_the_other_stops_against_it() {
        let (v, vb) = glyph("V");
        let foot = v
            .strokes
            .iter()
            .filter(|s| s.bounds().is_some_and(|r| r.max_y > vb.max_y - 5.0))
            .collect::<Vec<_>>();
        assert_eq!(foot.len(), 1, "one arm reaches the baseline");
        let reach = foot[0].bounds().expect("bounds");
        assert!(
            reach.min_x < (vb.min_x + vb.max_x) / 2.0,
            "the left arm: {reach:?}"
        );
        assert!(
            v.strokes
                .iter()
                .any(|s| s.ends.contains(&StrokeEnd::Against))
        );
        let (a, ab) = glyph("A");
        let mid_x = |s: &Stroke| s.centre.iter().map(|p| p.x).sum::<f64>() / s.centre.len() as f64;
        let centre_x = (ab.min_x + ab.max_x) / 2.0;
        let stopped: Vec<&Stroke> = a
            .strokes
            .iter()
            .filter(|s| s.ends.contains(&StrokeEnd::Against))
            .collect();
        assert_eq!(stopped.len(), 1, "one leg stops at A's apex");
        assert!(mid_x(stopped[0]) < centre_x, "the left leg stops");
        let top = a
            .strokes
            .iter()
            .filter(|s| mid_x(s) > centre_x)
            .flat_map(|s| s.centre.iter().map(|p| p.y))
            .fold(f64::INFINITY, f64::min);
        assert!(
            top - ab.min_y < 5.0,
            "the right leg runs on to the top: {top} vs {ab:?}"
        );
        assert!(
            a.strokes
                .iter()
                .all(|s| !s.ends.contains(&StrokeEnd::Corner))
        );
    }

    fn glyph(ch: &str) -> (GlyphStrokes, Rect) {
        let spec = TextSpec::new(ch, DEFAULT_FONT, 20.0);
        let l = layout(&spec, None).expect("layout");
        let contours = l.contours();
        let b = Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
        (strokes(&contours).expect("strokes"), b)
    }

    fn count(g: &GlyphStrokes) -> (usize, usize, usize) {
        let closed = g.strokes.iter().filter(|s| s.closed).count();
        (g.strokes.len(), g.junctions.len(), closed)
    }

    /// Cuts at sharp corners: a pair of [`StrokeEnd::Corner`] ends, or one arm run on and the
    /// other stopped [`StrokeEnd::Against`] it (#446).
    fn corners(g: &GlyphStrokes) -> usize {
        let ends = || g.strokes.iter().flat_map(|s| s.ends);
        ends().filter(|&e| e == StrokeEnd::Corner).count() / 2
            + ends().filter(|&e| e == StrokeEnd::Against).count()
    }

    fn median(v: &[f64]) -> f64 {
        let mut v = v.to_vec();
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    }

    /// The stroke structure PE-DESIGN 11.50 sews these letters with (round-5 captures,
    /// docs/research/stroke-lettering.md): strokes, junctions, closed loops and sharp corners.
    #[test]
    fn letters_have_pe_designs_stroke_structure() {
        // (letter, strokes, junctions, closed, corners)
        for (ch, want, corner) in [
            ("I", (1, 0, 0), 0),
            ("L", (1, 0, 0), 0), // one column round its corner, as PE-DESIGN fans it
            ("T", (3, 1, 0), 0), // the bar cut at the stem
            ("H", (5, 2, 0), 0), // the stems cut at the bar
            ("X", (4, 1, 0), 0), // four arms at one crossing
            ("V", (2, 0, 0), 1), // cut at its foot
            ("O", (1, 0, 1), 0), // one ring
            ("A", (5, 2, 0), 1), // legs cut at the apex and at the bar
            ("S", (1, 0, 0), 0),
            ("e", (2, 1, 0), 0), // the bowl round its counter and the tail
            ("i", (2, 0, 0), 0), // the stem and the dot
        ] {
            let (g, _) = glyph(ch);
            assert_eq!(count(&g), want, "{ch}: {g:?}");
            assert_eq!(corners(&g), corner, "{ch}");
        }
    }

    #[test]
    fn the_h_is_cut_at_its_bar_with_pe_designs_widths() {
        let (g, b) = glyph("H");
        // PE-DESIGN's satin H (stroke-satin-H-20): stems 3.8 mm wide, the bar 2.8 mm thick
        // between 8.8 and 11.6 mm below the top.
        let (bar, stems): (Vec<&Stroke>, Vec<&Stroke>) = g.strokes.iter().partition(|s| {
            let r = s.bounds().expect("bounds");
            r.width() > r.height()
        });
        assert_eq!((bar.len(), stems.len()), (1, 4));
        for j in &g.junctions {
            let below_top = j.at.y - b.min_y;
            assert!((below_top - 102.0).abs() < 8.0, "{j:?}");
        }
        for s in &stems {
            let w = median(&s.widths);
            assert!((w - 38.0).abs() < 4.0, "stem width {w}");
            let x = s.centre[0].x - b.min_x;
            assert!(
                (x - 19.0).abs() < 4.0 || (x - 155.0).abs() < 4.0,
                "stem at {x}"
            );
        }
        let w = median(&bar[0].widths);
        assert!((w - 28.0).abs() < 4.0, "bar width {w}");
    }

    #[test]
    fn sharp_feet_and_apexes_are_corners_right_angles_are_not() {
        // V: the two arms meet at its foot (PE-DESIGN sews them as two columns).
        let (g, b) = glyph("V");
        let foot = g.strokes[0]
            .centre
            .iter()
            .chain(&g.strokes[1].centre)
            .map(|p| p.y)
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(
            b.max_y - foot < 40.0,
            "the cut is at the foot: {foot} vs {}",
            b.max_y
        );
        // A: the apex at the top (the left leg stops against the right there, #446).
        let (g, b) = glyph("A");
        let apex = g
            .strokes
            .iter()
            .filter(|s| s.ends.contains(&StrokeEnd::Against))
            .flat_map(|s| s.centre.iter().map(|p| p.y))
            .fold(f64::INFINITY, f64::min);
        assert!(
            apex - b.min_y < 30.0,
            "the cut is at the apex: {apex} vs {}",
            b.min_y
        );
        // L keeps its corner (PE-DESIGN fans it in one column).
        let (g, _) = glyph("L");
        assert!(g.strokes[0].ends == [StrokeEnd::Free, StrokeEnd::Free]);
        assert!(g.strokes[0].centre.len() >= 3);
    }

    #[test]
    fn strokes_are_deterministic_and_degenerate_input_is_refused() {
        let (a, _) = glyph("A");
        let (b, _) = glyph("A");
        assert_eq!(a, b);
        assert_eq!(strokes(&[]), None);
        let tiny = Contour::rectangle(0.0, 0.0, 1.0, 1.0);
        assert_eq!(strokes(&[tiny]), None);
        let open = Contour::new(vec![v2(0.0, 0.0), v2(100.0, 0.0), v2(100.0, 50.0)], false);
        assert_eq!(strokes(&[open]), None);
        let nan = Contour::new(vec![v2(0.0, 0.0), v2(f64::NAN, 0.0), v2(100.0, 50.0)], true);
        assert!(strokes(&[nan]).is_none_or(|g| g.strokes.len() <= 1));
        // A huge region is refused rather than sampled at a huge size.
        let huge = Contour::rectangle(0.0, 0.0, 1.0e9, 1.0e9);
        assert!(strokes(&[huge]).is_some_and(|g| g.strokes.len() <= 4));
    }

    /// Angle of a rung, degrees in 0..180 (y down).
    fn angle(r: &[Vec2; 2]) -> f64 {
        let d = r[1] - r[0];
        d.y.datan2(d.x).to_degrees().rem_euclid(180.0)
    }

    /// Distance between two angles on the half circle.
    fn off(a: f64, b: f64) -> f64 {
        let d = (a - b).rem_euclid(180.0);
        d.min(180.0 - d)
    }

    #[test]
    fn rungs_stay_across_a_bending_stroke() {
        // PE-DESIGN's S (stroke-satin-S) crosses its spine across the stroke all along. Joined
        // at equal fractions of the sides' lengths, the rungs slanted further and further along
        // the spine, where the side outside each bend is much longer than the one inside
        // (#444). Each rung's middle sits on the centre line, so measure how far a rung is from
        // square to the centre line there.
        for size in [40.0, 80.0, 200.0] {
            let spec = TextSpec::new("S", DEFAULT_FONT, size / 10.0);
            let contours = layout(&spec, None).expect("layout").contours();
            let g = strokes(&contours).expect("strokes");
            let rungs = g.rungs(&contours, 30.0);
            let mut worst: f64 = 0.0;
            for (st, rs) in g.strokes.iter().zip(&rungs) {
                for r in rs.iter().filter(|r| r[0].distance(r[1]) > 1.0) {
                    let mid = r[0].lerp(r[1], 0.5);
                    let (at, _, _) = project(&st.centre, st.closed, mid);
                    let (a, b) = (
                        point_at(&st.centre, at - 5.0),
                        point_at(&st.centre, at + 5.0),
                    );
                    let along = angle(&[a, b]);
                    worst = worst.max(off(off(angle(r), along), 90.0).abs());
                }
            }
            assert!(
                worst < 40.0,
                "S at {size}: a rung {worst} degrees off square"
            );
        }
    }

    #[test]
    fn rungs_join_the_sides_as_pe_designs_blocks_do() {
        // PE-DESIGN's Convert to Blocks of the H (stroke-blocks-H, read with the PES 0110
        // reader): horizontal lines across the stems from the top and the foot to the bar
        // (8.8 and 11.6 mm below the top), vertical lines across the bar from x 3.8 to 13.6.
        let spec = TextSpec::new("H", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let b = Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
        let g = strokes(&contours).expect("strokes");
        let rungs = g.rungs(&contours, 30.0);
        assert_eq!(rungs.len(), g.strokes.len());
        for (st, rs) in g.strokes.iter().zip(&rungs) {
            assert!(rs.len() >= 3, "{st:?}");
            let r = st.bounds().expect("bounds");
            let bar = r.width() > r.height();
            for rung in rs {
                let want = if bar { 90.0 } else { 0.0 };
                assert!(off(angle(rung), want) < 3.0, "{rung:?}");
                if bar {
                    let x = (rung[0].x - b.min_x) / 10.0;
                    assert!((3.4..=14.0).contains(&x), "bar rung at x {x}");
                } else {
                    // Every stem rung spans the stem: 3.8 mm.
                    assert!((rung[0].distance(rung[1]) - 38.0).abs() < 3.0, "{rung:?}");
                }
            }
            if !bar {
                // The stem's rungs stop near the bar's edges, where PE-DESIGN's do (8.8 and
                // 11.6 mm): at the junction's clearance, 8.3 and 12.2 mm (the junction square
                // is the order's to give to a stroke, step 4).
                let ys: Vec<f64> = rs.iter().map(|r| (r[0].y - b.min_y) / 10.0).collect();
                let near_bar = ys
                    .iter()
                    .map(|&y| (y - 8.8).abs().min((y - 11.6).abs()))
                    .fold(f64::INFINITY, f64::min);
                assert!(near_bar < 0.7, "{ys:?}");
            }
        }
        // A: the crossbar's lines are vertical, as PE-DESIGN's; the slanted right leg's lines
        // run near horizontal (PE-DESIGN: 161-180 degrees), not across the 60-degree leg
        // (150 degrees would be across it).
        let spec = TextSpec::new("A", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let g = strokes(&contours).expect("strokes");
        let rungs = g.rungs(&contours, 30.0);
        let bar = g
            .strokes
            .iter()
            .position(|s| s.ends.iter().all(|e| matches!(e, StrokeEnd::Junction(_))))
            .expect("bar");
        assert!(
            rungs[bar].iter().all(|r| off(angle(r), 90.0) < 6.0),
            "{:?}",
            rungs[bar]
        );
        // The right leg's upper part: right of the middle, the highest (it runs on to the apex's
        // top, #446).
        let right_upper = g
            .strokes
            .iter()
            .enumerate()
            .filter(|(_, s)| {
                s.centre.iter().map(|p| p.x).sum::<f64>() / s.centre.len() as f64 > 100.0
            })
            .min_by(|(_, s), (_, o)| {
                let top = |s: &Stroke| s.centre.iter().map(|p| p.y).fold(f64::INFINITY, f64::min);
                top(s).total_cmp(&top(o))
            })
            .map(|(i, _)| i)
            .expect("right leg");
        for r in &rungs[right_upper] {
            assert!(off(angle(r), 170.0) < 15.0, "{r:?} {}", angle(r));
        }
    }

    #[test]
    fn rungs_of_degenerate_strokes_are_empty() {
        let g = GlyphStrokes::default();
        assert!(g.rungs(&[], 30.0).is_empty());
        let spec = TextSpec::new("I", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let g = strokes(&contours).expect("strokes");
        assert!(g.rungs(&contours, 0.0).iter().all(Vec::is_empty));
        assert!(g.rungs(&[], 30.0).iter().all(Vec::is_empty));
    }

    /// Each block's first and last rung midpoints in mm from the letter box's top-left.
    fn ends_mm(bl: &Block, b: &Rect) -> ((f64, f64), (f64, f64)) {
        let m = |r: &[Vec2; 2]| {
            (
                ((r[0].x + r[1].x) / 2.0 - b.min_x) / 10.0,
                ((r[0].y + r[1].y) / 2.0 - b.min_y) / 10.0,
            )
        };
        (m(&bl.rungs[0]), m(&bl.rungs[bl.rungs.len() - 1]))
    }

    fn near(a: (f64, f64), b: (f64, f64), tol: f64) -> bool {
        (a.0 - b.0).abs() <= tol && (a.1 - b.1).abs() <= tol
    }

    #[test]
    fn blocks_follow_pe_designs_order_for_h() {
        // PE-DESIGN 11.50's Convert to Blocks of Lato Bold H at 20 mm (stroke-blocks-H, first
        // and last direction lines' midpoints, mm from the left stem's left edge and the top):
        // the left stem from the top down to the bar (1.9, 0 to 8.8), its bottom from the foot
        // up (20.6 to 11.6), the junction square downwards (8.8 to 11.6), the bar left to
        // right (3.8 to 13.6 at 10.2) and the right stem whole from the top (15.5, 0 to 20.6).
        let spec = TextSpec::new("H", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let b = Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
        let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
        let want = [
            ((1.9, 0.0), (1.9, 8.8), false),
            ((1.9, 20.6), (1.9, 11.6), false),
            ((1.9, 8.8), (1.9, 11.6), true),
            ((3.8, 10.2), (13.6, 10.2), false),
            ((15.5, 0.0), (15.5, 20.6), false),
        ];
        assert_eq!(blocks.len(), want.len(), "{blocks:?}");
        for (bl, (from, to, junction)) in blocks.iter().zip(want) {
            let (f, t) = ends_mm(bl, &b);
            assert!(
                near(f, from, 0.5) && near(t, to, 0.5),
                "{f:?} {t:?} want {from:?} {to:?}"
            );
            assert_eq!(bl.junction, junction);
            assert!(bl.rungs.len() >= 2);
        }
    }

    #[test]
    fn blocks_follow_pe_designs_order_for_a_and_t() {
        // stroke-blocks-A: the left leg from below the apex down to the bar, its foot upwards,
        // the junction square downwards, the bar left to right, then the right leg whole from
        // the apex to its foot. Positions from the left foot's line (PE: the upper part starts
        // 7.4 mm right of it and 18.8 mm up, the bar 6.4 mm up from 4.6 mm).
        let spec = TextSpec::new("A", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let b = Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
        let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
        assert_eq!(blocks.len(), 5, "{blocks:?}");
        let e: Vec<_> = blocks.iter().map(|bl| ends_mm(bl, &b)).collect();
        let junctions: Vec<bool> = blocks.iter().map(|bl| bl.junction).collect();
        assert_eq!(junctions, [false, false, true, false, false]);
        let foot = e[1].0;
        let from_foot = |p: (f64, f64)| (p.0 - foot.0, p.1 - foot.1);
        // The upper part goes down and left; the foot part up; the bar left to right; the
        // right leg down and right.
        assert!(e[0].1.1 > e[0].0.1 && e[0].1.0 < e[0].0.0, "{:?}", e[0]);
        assert!(e[1].1.1 < e[1].0.1, "{:?}", e[1]);
        assert!(e[2].1.1 > e[2].0.1, "{:?}", e[2]);
        assert!(
            e[3].1.0 > e[3].0.0 && (e[3].0.1 - e[3].1.1).abs() < 0.5,
            "{:?}",
            e[3]
        );
        assert!(e[4].1.1 > e[4].0.1 && e[4].1.0 > e[4].0.0, "{:?}", e[4]);
        assert!(
            near(from_foot(e[0].0), (7.4, -18.8), 1.0),
            "{:?}",
            from_foot(e[0].0)
        );
        assert!(
            (from_foot(e[3].0).1 + 6.4).abs() < 0.5,
            "{:?}",
            from_foot(e[3].0)
        );
        // The right leg's foot is the letter's bottom.
        assert!((e[4].1.1 - b.height() / 10.0).abs() < 0.5, "{:?}", e[4]);
        // T: the bar's left half, its right half from the right end, the square, the stem down.
        let spec = TextSpec::new("T", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let b = Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
        let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
        let e: Vec<_> = blocks.iter().map(|bl| ends_mm(bl, &b)).collect();
        assert_eq!(blocks.len(), 4, "{blocks:?}");
        assert!(blocks[2].junction);
        assert!(e[0].0.0 < 0.5 && e[0].1.0 > e[0].0.0, "{:?}", e[0]);
        assert!(
            e[1].0.0 > b.width() / 10.0 - 0.5 && e[1].1.0 < e[1].0.0,
            "{:?}",
            e[1]
        );
        assert!(e[3].1.1 > b.height() / 10.0 - 0.5, "{:?}", e[3]);
    }

    #[test]
    fn a_s_bar_ends_along_the_legs_and_its_square_is_cut_at_the_bar_s_corners() {
        // stroke-satin-A (#938): PE-DESIGN's bar starts and ends along the legs' inner edges,
        // its end crossings at 110 and 70 degrees from level (y down), from the bar's top corner
        // to its bottom one; its left leg is cut through those corners, the square between them.
        let spec = TextSpec::new("A", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
        assert_eq!(blocks.len(), 5, "{blocks:?}");
        let (square, bar) = (&blocks[2], &blocks[3]);
        assert!(square.junction && !bar.junction);
        // A rung's angle from level, its top point to its bottom one.
        let angle = |r: &[Vec2; 2]| {
            let (top, bottom) = if r[0].y <= r[1].y {
                (r[0], r[1])
            } else {
                (r[1], r[0])
            };
            (bottom.y - top.y).datan2(bottom.x - top.x).to_degrees()
        };
        let (first, last) = (bar.rungs[0], bar.rungs[bar.rungs.len() - 1]);
        assert!((angle(&first) - 110.0).abs() <= 10.0, "{}", angle(&first));
        assert!((angle(&last) - 70.0).abs() <= 10.0, "{}", angle(&last));
        // Each of the square's end rungs passes within 0.5 mm of one of the bar's left corners.
        let corners = [first[0], first[1]];
        for end in [square.rungs[0], square.rungs[square.rungs.len() - 1]] {
            let near = corners.iter().any(|&c| {
                let d = end[1] - end[0];
                let t = ((c - end[0]).x * d.x + (c - end[0]).y * d.y) / (d.x * d.x + d.y * d.y);
                (end[0] + d * t.clamp(0.0, 1.0)).distance(c) <= 5.0
            });
            assert!(
                near,
                "square end {end:?} misses the bar's corners {corners:?}"
            );
        }
    }

    #[test]
    fn m_and_n_start_at_the_top_and_run_down_under_the_first_stem() {
        // stroke-satin-M, -M-8, -N, -N-8: PE-DESIGN's first stitches are at the top of the
        // left stem; it runs down under the stem and sews the satin from the foot up (#940).
        for (ch, size) in [("M", 20.0), ("M", 8.0), ("N", 20.0), ("N", 8.0)] {
            let spec = TextSpec::new(ch, DEFAULT_FONT, size);
            let contours = layout(&spec, None).expect("layout").contours();
            let b =
                Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
            let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
            let first = &blocks[0];
            let lead = first.lead.expect("the first stem is led into");
            let (from, to) = ends_mm(first, &b);
            let (h, lead_y) = (b.height() / 10.0, (lead.y - b.min_y) / 10.0);
            assert!(lead_y < 0.5, "{ch} {size}: lead at {lead_y}");
            assert!(
                from.1 > h - 0.5 && to.1 < from.1,
                "{ch} {size}: {from:?} {to:?}"
            );
            // The lead starts within the first stem's width, above its satin.
            let xs = first.rungs.iter().flat_map(|r| [r[0].x, r[1].x]);
            let (lo, hi) = xs.fold((f64::MAX, f64::MIN), |(a, b), x| (a.min(x), b.max(x)));
            assert!(
                (lo - 1.0..=hi + 1.0).contains(&lead.x) && from.0 < b.width() / 30.0,
                "{ch} {size}: lead x {} stem {lo}..{hi}",
                lead.x
            );
            // Only the first piece is led into.
            assert!(
                blocks[1..].iter().all(|bl| bl.lead.is_none()),
                "{ch} {size}"
            );
        }
        // A letter whose first piece is sewn from the end nearest its start keeps no lead.
        for ch in ["I", "L", "O"] {
            let spec = TextSpec::new(ch, DEFAULT_FONT, 20.0);
            let contours = layout(&spec, None).expect("layout").contours();
            let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
            assert!(blocks.iter().all(|bl| bl.lead.is_none()), "{ch}");
        }
    }

    #[test]
    fn letters_without_junctions_are_one_block_per_stroke_and_blocks_are_deterministic() {
        for (ch, n) in [("I", 1), ("L", 1), ("O", 1), ("i", 2), ("S", 1)] {
            let spec = TextSpec::new(ch, DEFAULT_FONT, 20.0);
            let contours = layout(&spec, None).expect("layout").contours();
            let g = strokes(&contours).expect("strokes");
            let blocks = g.blocks(&contours, 30.0);
            assert_eq!(blocks.len(), n, "{ch}");
            assert!(
                blocks.iter().all(|bl| !bl.junction && bl.rungs.len() >= 2),
                "{ch}"
            );
            assert_eq!(blocks, g.blocks(&contours, 30.0), "{ch}");
        }
        assert!(GlyphStrokes::default().blocks(&[], 30.0).is_empty());
    }

    #[test]
    fn crossings_and_separate_pieces_start_new_runs() {
        // stroke-satin-X: one diagonal through and the other in two halves meeting it, a trim
        // between the halves; stroke-satin-lower-i: the dot and the stem apart.
        for (ch, blocks_n, runs) in [("X", 4, 2), ("i", 2, 2), ("H", 5, 1), ("A", 5, 1)] {
            let spec = TextSpec::new(ch, DEFAULT_FONT, 20.0);
            let contours = layout(&spec, None).expect("layout").contours();
            let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
            assert_eq!(blocks.len(), blocks_n, "{ch}");
            assert!(!blocks[0].new_run, "{ch}");
            let n = 1 + blocks.iter().filter(|b| b.new_run).count();
            assert_eq!(n, runs, "{ch}");
            assert!(blocks.iter().all(|b| !b.junction) || ch != "X", "{ch}");
        }
    }

    #[test]
    fn x_is_one_diagonal_through_and_two_halves_meeting_it() {
        // stroke-satin-X (PE-DESIGN 11.50): the top-left arm down to the crossing, the
        // bottom-left arm from its foot up to it, a trim, the top-right arm from its top down to
        // it, then the crossing and the bottom-right arm down to its foot (#446). Each arm's
        // level cut end is sewn with a level crossing from corner to corner, as PE-DESIGN's
        // (0.2-4.6 mm at the top, 0.0-4.1 mm at the foot), not fanned from the cut's middle.
        let spec = TextSpec::new("X", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let b = Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
        let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
        assert_eq!(blocks.len(), 4, "{blocks:?}");
        let new_runs: Vec<bool> = blocks.iter().map(|bl| bl.new_run).collect();
        assert_eq!(new_runs, [false, false, true, false]);
        let e: Vec<_> = blocks.iter().map(|bl| ends_mm(bl, &b)).collect();
        let (w, h) = (b.width() / 10.0, b.height() / 10.0);
        // (start, end) of each block: top-left, foot-left, top-right, foot-right.
        assert!(
            e[0].0.0 < 4.0 && e[0].0.1 < 0.3 && e[0].1.1 > 8.0,
            "{:?}",
            e[0]
        );
        assert!(
            e[1].0.0 < 4.0 && e[1].0.1 > h - 0.3 && e[1].1.1 < 13.0,
            "{:?}",
            e[1]
        );
        assert!(
            e[2].0.0 > w - 4.0 && e[2].0.1 < 0.3 && e[2].1.1 > 7.0,
            "{:?}",
            e[2]
        );
        assert!(
            e[3].0.1 < 11.0 && e[3].1.0 > w - 4.0 && e[3].1.1 > h - 0.3,
            "{:?}",
            e[3]
        );
        // The feet's rungs: level, across the whole cut.
        let feet = [
            blocks[0].rungs[0],
            blocks[1].rungs[0],
            blocks[2].rungs[0],
            blocks[3].rungs[blocks[3].rungs.len() - 1],
        ];
        for r in feet {
            assert!((r[0].y - r[1].y).abs() < 3.0, "{r:?}");
            assert!(r[0].distance(r[1]) > 34.0, "{r:?}");
        }
    }

    #[test]
    fn the_column_sewn_last_at_a_sharp_corner_runs_into_its_tip() {
        // stroke-blocks-A: the right leg, sewn after the left, takes the apex in; at V's foot
        // the left arm runs on (#446). The letters keep their full height.
        for ch in ["A", "V"] {
            let spec = TextSpec::new(ch, DEFAULT_FONT, 20.0);
            let contours = layout(&spec, None).expect("layout").contours();
            let b =
                Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
            let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
            let ys = blocks
                .iter()
                .flat_map(|bl| bl.rungs.iter().flat_map(|r| [r[0].y, r[1].y]));
            let (top, bottom) = ys.fold((f64::INFINITY, f64::NEG_INFINITY), |(t, m), y| {
                (t.min(y), m.max(y))
            });
            assert!(
                top - b.min_y < 3.0 && b.max_y - bottom < 3.0,
                "{ch}: {top} {bottom} {b:?}"
            );
        }
        let spec = TextSpec::new("A", DEFAULT_FONT, 20.0);
        let contours = layout(&spec, None).expect("layout").contours();
        let b = Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
        let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
        let first = |bl: &Block| bl.rungs[0][0].y - b.min_y;
        assert!(first(&blocks[4]) < 3.0, "the right leg starts at the apex");
        // The left leg stops against the right one: its first crossing lies along the right
        // leg's inner edge, as PE-DESIGN's (stroke-satin-A: 56-65 degrees, #446).
        let a0 = angle(&blocks[0].rungs[0]);
        assert!(off(a0, 60.0) < 15.0, "the left leg's first crossing: {a0}");
    }

    #[test]
    fn a_slanted_stems_foot_is_cut_across_the_whole_stem() {
        // The 10 mm H under the slant transform (text-transform-9): the thinned centre line of
        // its left stem hooked towards one corner of the foot, and the foot's rung covered
        // 0.4 mm of the 1.1 mm stem. Its free end is straightened: the rung on the baseline
        // spans the stem.
        let mut spec = TextSpec::new("H", DEFAULT_FONT, 10.0);
        spec.envelope = Some(9);
        let contours = layout(&spec, None).expect("layout").contours();
        let b = Rect::enclosing(contours.iter().flat_map(Contour::outline_points)).expect("box");
        let blocks = strokes(&contours).expect("strokes").blocks(&contours, 30.0);
        let feet: Vec<f64> = blocks
            .iter()
            .flat_map(|bl| [bl.rungs[0], bl.rungs[bl.rungs.len() - 1]])
            .filter(|r| (r[0].y - b.max_y).abs() < 1.0 && (r[1].y - b.max_y).abs() < 1.0)
            .map(|r| r[0].distance(r[1]))
            .collect();
        assert_eq!(feet.len(), 2, "{feet:?}");
        assert!(feet.iter().all(|&w| w > 9.0), "{feet:?}");
        // Curved ends stay as they are: S keeps its stroke structure and full height.
        let (g, sb) = glyph("S");
        let blocks = g.blocks(
            &layout(&TextSpec::new("S", DEFAULT_FONT, 20.0), None)
                .expect("layout")
                .contours(),
            30.0,
        );
        let lowest = blocks
            .iter()
            .flat_map(|bl| bl.rungs.iter().flat_map(|r| [r[0].y, r[1].y]))
            .fold(f64::NEG_INFINITY, f64::max);
        assert!(sb.max_y - lowest < 5.0, "{lowest} {sb:?}");
    }
}
