//! Flow fill (#538, beyond PE-DESIGN): the rows of a fill follow a stored direction field
//! instead of one straight angle, as evenly spaced streamlines of the field.
//!
//! - The field is the region's own: rows radiating from a centre, rows circling one, or a grid
//!   of row angles. Nothing here reads a picture: Auto Punch's `flowFill` (the digitize
//!   crate's `flow`, OpenStitch mode) builds a grid from one, after the owner's decision of
//!   2026-11-08 (docs/research/beyond/patents.md).
//! - The rows are streamlines of the field, traced in short steps and placed a row spacing
//!   apart by the evenly-spaced-streamline method (Jobard and Lefer, 1997): each new row starts
//!   a spacing beside one already placed, and a row stops where it comes closer than
//!   [`SEPARATION`] spacings to another, leaves the region or closes on itself. Streamlines of
//!   one field never cross, and the stop keeps them apart, so no two rows cross.
//!
//! Lengths are 0.1 mm; angles are the stored region angles (degrees, counter-clockwise on
//! screen).

use crate::fmath::Det;
use crate::generate::Run;
use crate::geometry::{Contour, Rect, Vec2, contains, v2};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Rows stop where they come closer than this many row spacings to another row.
pub const SEPARATION: f64 = 0.8;
/// Rows shorter than this many spacings are left out (a sliver between longer rows).
const MIN_ROW: f64 = 3.0;
/// Most rows one region gets (a guard against a field that never settles).
const MAX_ROWS: usize = 20_000;

/// A region's direction field.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FlowField {
    /// Rows run away from `center` (a sunburst), 0.1 mm.
    Radial { center: Vec2 },
    /// Rows circle `center` (a swirl), 0.1 mm.
    Swirl { center: Vec2 },
    /// Row angles on a grid: `cols` cells across, as many rows down as `angles` fills, each
    /// `cell` (0.1 mm) square from `origin`; between cell centres the direction blends, and
    /// beyond the grid the nearest cells hold.
    Grid {
        origin: Vec2,
        cell: f64,
        cols: usize,
        angles: Vec<f64>,
    },
}

impl FlowField {
    /// Why this field cannot be used, if it cannot.
    pub fn check(&self) -> Result<(), String> {
        match self {
            FlowField::Radial { .. } | FlowField::Swirl { .. } => Ok(()),
            FlowField::Grid {
                cell, cols, angles, ..
            } => {
                if !(*cell >= 1.0 && cell.is_finite()) {
                    return Err("the grid's cell must be at least 0.1 mm".into());
                }
                if *cols == 0 || angles.is_empty() || angles.len() % cols != 0 {
                    return Err(
                        "the grid needs cols columns and a whole number of rows of angles".into(),
                    );
                }
                if angles.len() > 100_000 {
                    return Err("the grid has more than 100000 cells".into());
                }
                if angles.iter().any(|a| !a.is_finite()) {
                    return Err("every grid angle must be a number".into());
                }
                Ok(())
            }
        }
    }

    /// The row direction at `p` (a unit vector, either way along the row), or `None` at the
    /// centre of a radial or swirl field, where it has none.
    pub fn direction(&self, p: Vec2) -> Option<Vec2> {
        match self {
            FlowField::Radial { center } => unit(p - *center),
            FlowField::Swirl { center } => unit(p - *center).map(|d| v2(-d.y, d.x)),
            FlowField::Grid {
                origin,
                cell,
                cols,
                angles,
            } => {
                let rows = angles.len() / cols;
                // Cell centres at (i + 0.5) cells; blend the four around `p`.
                let fx = ((p.x - origin.x) / cell - 0.5).clamp(0.0, (*cols - 1) as f64);
                let fy = ((p.y - origin.y) / cell - 0.5).clamp(0.0, (rows - 1) as f64);
                let (i, j) = (fx.floor() as usize, fy.floor() as usize);
                let (i1, j1) = ((i + 1).min(cols - 1), (j + 1).min(rows - 1));
                let (tx, ty) = (fx - i as f64, fy - j as f64);
                // Directions blend as doubled angles: a row at 10° and one at 190° agree.
                let doubled = |a: f64| {
                    let r = crate::region::geometry_angle(a).to_radians() * 2.0;
                    v2(r.dcos(), r.dsin())
                };
                let at = |i: usize, j: usize| doubled(angles[j * cols + i]);
                let mix = at(i, j) * ((1.0 - tx) * (1.0 - ty))
                    + at(i1, j) * (tx * (1.0 - ty))
                    + at(i, j1) * ((1.0 - tx) * ty)
                    + at(i1, j1) * (tx * ty);
                if mix.x.dhypot(mix.y) < 1e-6 {
                    return None;
                }
                let half = mix.y.datan2(mix.x) / 2.0;
                Some(v2(half.dcos(), half.dsin()))
            }
        }
    }
}

fn unit(d: Vec2) -> Option<Vec2> {
    let l = d.x.dhypot(d.y);
    (l > 1e-6).then(|| d * (1.0 / l))
}

/// A grid cell's key: its column and row.
type Key = (i64, i64);

/// Points of the rows placed so far, by grid cell, for the distance tests: each point with its
/// row and its index along the row.
struct Placed {
    cell: f64,
    cells: HashMap<Key, Vec<(Vec2, usize, usize)>>,
}

impl Placed {
    fn key(&self, p: Vec2) -> Key {
        (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        )
    }

    fn add(&mut self, p: Vec2, row: usize, index: usize) {
        let k = self.key(p);
        self.cells.entry(k).or_default().push((p, row, index));
    }

    /// Whether a point of another row (or of `row` itself, more than `skip` points from
    /// `index`) lies within `d` of `p`.
    fn near(&self, p: Vec2, d: f64, row: usize, index: usize, skip: usize) -> bool {
        let (kx, ky) = self.key(p);
        let reach = (d / self.cell).ceil() as i64;
        for x in kx - reach..=kx + reach {
            for y in ky - reach..=ky + reach {
                if let Some(list) = self.cells.get(&(x, y)) {
                    for &(q, r, i) in list {
                        if (r != row || i.abs_diff(index) > skip) && q.distance(p) < d {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }
}

/// The flow fill's rows over the region (`closed` minus `holes`): evenly spaced streamlines of
/// `field`, `spacing` apart, each a polyline in steps of under half the separation.
pub fn rows(field: &FlowField, closed: &[Contour], holes: &[Contour], spacing: f64) -> Vec<Run> {
    if spacing <= 0.0 || field.check().is_err() {
        return Vec::new();
    }
    let Some(bounds) = Rect::enclosing(closed.iter().flat_map(|c| c.outline_points())) else {
        return Vec::new();
    };
    let inside = |p: Vec2| contains(p, closed) && !contains(p, holes);
    let test = SEPARATION * spacing;
    let step = test * 0.45;
    // A row may come back near its own points only after this many steps (a closing circle).
    let skip = (3.0 * spacing / step).ceil() as usize + 2;
    let max_steps = ((bounds.width() + bounds.height()) * 8.0 / step) as usize + 10;
    let mut placed = Placed {
        cell: spacing,
        cells: HashMap::new(),
    };
    let mut out: Vec<Run> = Vec::new();

    // One row through `seed`: traced both ways from it.
    let trace = |seed: Vec2, placed: &Placed, row: usize| -> Option<Run> {
        let first = field.direction(seed)?;
        let mut halves: Vec<Vec<Vec2>> = Vec::new();
        for sign in [1.0, -1.0] {
            let mut pts = vec![seed];
            let mut heading = first * sign;
            for n in 1..max_steps {
                let p = *pts.last().expect("a point");
                // Midpoint step, the direction turned to keep going the way it was going.
                let along = |q: Vec2, h: Vec2| {
                    field.direction(q).map(|d| {
                        if d.x * h.x + d.y * h.y < 0.0 {
                            d * -1.0
                        } else {
                            d
                        }
                    })
                };
                let Some(d1) = along(p, heading) else { break };
                let Some(d2) = along(p + d1 * (step / 2.0), d1) else {
                    break;
                };
                let next = p + d2 * step;
                if !inside(next) {
                    break;
                }
                // Index along the whole row: the backward half counts down from the seed.
                let index = if sign > 0.0 {
                    max_steps + n
                } else {
                    max_steps - n
                };
                // Another row too near (this row's own points are not placed yet).
                if placed.near(next, test, row, index, usize::MAX) {
                    break;
                }
                // Closing on itself (a circle round a swirl): stop before the start.
                let own = &pts[..pts.len().saturating_sub(skip)];
                if own.iter().any(|q| q.distance(next) < test) {
                    break;
                }
                if let Some(other) = halves.first()
                    && other.iter().any(|q| q.distance(next) < test)
                    && n > skip
                {
                    break;
                }
                heading = d2;
                pts.push(next);
            }
            halves.push(pts);
        }
        let mut row_pts: Vec<Vec2> = halves[1].iter().rev().copied().collect();
        row_pts.extend(halves[0].iter().skip(1));
        let length: f64 = row_pts.windows(2).map(|w| w[0].distance(w[1])).sum();
        (length >= MIN_ROW * spacing).then_some(row_pts)
    };

    let accept = |row_pts: Run, placed: &mut Placed, out: &mut Vec<Run>| {
        let id = out.len();
        for (i, p) in row_pts.iter().enumerate() {
            placed.add(*p, id, i);
        }
        out.push(row_pts);
    };

    // Seeds: a coarse grid over the region (so every part of it is reached), and beside every
    // row placed, a spacing to each side.
    let coarse = (spacing * 6.0).max(1.0);
    let mut grid_seeds = Vec::new();
    let mut y = bounds.min_y + coarse / 2.0;
    while y < bounds.max_y {
        let mut x = bounds.min_x + coarse / 2.0;
        while x < bounds.max_x {
            let p = v2(x, y);
            if inside(p) {
                grid_seeds.push(p);
            }
            x += coarse;
        }
        y += coarse;
    }
    let mut next_row = 0usize;
    for seed in grid_seeds {
        if out.len() >= MAX_ROWS {
            break;
        }
        if placed.near(seed, spacing * 0.99, usize::MAX, 0, 0) {
            continue;
        }
        let Some(first) = trace(seed, &placed, out.len()) else {
            continue;
        };
        accept(first, &mut placed, &mut out);
        // Grow from every row placed: new rows a spacing to each side.
        while next_row < out.len() && out.len() < MAX_ROWS {
            let row = out[next_row].clone();
            next_row += 1;
            for w in row.windows(2) {
                let Some(d) = unit(w[1] - w[0]) else { continue };
                let n = v2(-d.y, d.x);
                for side in [1.0, -1.0] {
                    let s = w[0] + n * (spacing * side);
                    if !inside(s) || placed.near(s, spacing * 0.99, usize::MAX, 0, 0) {
                        continue;
                    }
                    if let Some(new) = trace(s, &placed, out.len()) {
                        accept(new, &mut placed, &mut out);
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Contour;

    fn circle(c: Vec2, r: f64) -> Contour {
        let pts: Vec<Vec2> = (0..96)
            .map(|i| {
                let a = std::f64::consts::TAU * i as f64 / 96.0;
                v2(c.x + r * a.dcos(), c.y + r * a.dsin())
            })
            .collect();
        Contour::new(pts, true)
    }

    fn crosses(a: Vec2, b: Vec2, c: Vec2, d: Vec2) -> bool {
        let cross =
            |o: Vec2, p: Vec2, q: Vec2| (p.x - o.x) * (q.y - o.y) - (p.y - o.y) * (q.x - o.x);
        let (d1, d2) = (cross(c, d, a), cross(c, d, b));
        let (d3, d4) = (cross(a, b, c), cross(a, b, d));
        d1 * d2 < 0.0 && d3 * d4 < 0.0
    }

    /// Mean angle (degrees) between each row segment and the field at its middle; no two rows
    /// cross; every point of the region is near a row.
    fn check(field: &FlowField, closed: &[Contour], holes: &[Contour], spacing: f64) -> f64 {
        let rows = rows(field, closed, holes, spacing);
        assert!(rows.len() > 10, "{} rows", rows.len());
        let (mut sum, mut n) = (0.0, 0.0);
        for r in &rows {
            for w in r.windows(2) {
                let Some(d) = unit(w[1] - w[0]) else { continue };
                let Some(f) = field.direction(w[0].lerp(w[1], 0.5)) else {
                    continue;
                };
                let cos = (d.x * f.x + d.y * f.y).abs().min(1.0);
                sum += cos.dacos().to_degrees();
                n += 1.0;
            }
        }
        // No two rows cross: every segment against every other row's segments (by cell).
        type Segments = HashMap<Key, Vec<(usize, Vec2, Vec2)>>;
        let mut cells: Segments = HashMap::new();
        let key = |p: Vec2| ((p.x / 20.0).floor() as i64, (p.y / 20.0).floor() as i64);
        for (k, r) in rows.iter().enumerate() {
            for w in r.windows(2) {
                cells.entry(key(w[0])).or_default().push((k, w[0], w[1]));
            }
        }
        for (k, r) in rows.iter().enumerate() {
            for w in r.windows(2) {
                let (kx, ky) = key(w[0]);
                for x in kx - 1..=kx + 1 {
                    for y in ky - 1..=ky + 1 {
                        for &(j, c, d) in cells.get(&(x, y)).into_iter().flatten() {
                            assert!(
                                j == k || !crosses(w[0], w[1], c, d),
                                "rows {k} and {j} cross near {:?}",
                                w[0]
                            );
                        }
                    }
                }
            }
        }
        // The rows cover the region: no point of a coarse grid is more than 2 spacings from one.
        let bounds = Rect::enclosing(closed.iter().flat_map(|c| c.outline_points())).expect("b");
        let mut y = bounds.min_y + 7.0;
        let mut far = 0;
        let mut total = 0;
        while y < bounds.max_y {
            let mut x = bounds.min_x + 7.0;
            while x < bounds.max_x {
                let p = v2(x, y);
                if contains(p, closed) && !contains(p, holes) {
                    total += 1;
                    let near = rows.iter().flatten().any(|q| q.distance(p) < 2.0 * spacing);
                    if !near {
                        far += 1;
                    }
                }
                x += 13.0;
            }
            y += 13.0;
        }
        assert!(
            far * 50 <= total,
            "{far} of {total} points far from any row"
        );
        sum / n
    }

    #[test]
    fn radial_rows_follow_the_field_and_never_cross() {
        // A ring 6 to 30 mm round the centre, rows 0.25 mm apart radiating out.
        let c = v2(400.0, 400.0);
        let field = FlowField::Radial { center: c };
        let error = check(&field, &[circle(c, 300.0)], &[circle(c, 60.0)], 2.5);
        assert!(error < 15.0, "mean error {error:.2}°");
        assert!(error < 5.0, "mean error {error:.2}°");
    }

    #[test]
    fn swirl_rows_follow_the_field_and_never_cross() {
        // A disc 30 mm round with a 6 mm hole, rows circling the centre.
        let c = v2(400.0, 400.0);
        let field = FlowField::Swirl { center: c };
        let error = check(&field, &[circle(c, 300.0)], &[circle(c, 60.0)], 2.5);
        assert!(error < 15.0, "mean error {error:.2}°");
        assert!(error < 5.0, "mean error {error:.2}°");
    }

    #[test]
    fn a_grid_field_turns_the_rows_and_a_uniform_one_is_straight() {
        // 4 x 4 cells of 30°: straight rows at 30°.
        let square = Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(400.0, 0.0),
                v2(400.0, 400.0),
                v2(0.0, 400.0),
            ],
            true,
        );
        let uniform = FlowField::Grid {
            origin: v2(0.0, 0.0),
            cell: 100.0,
            cols: 4,
            angles: vec![30.0; 16],
        };
        assert!(check(&uniform, std::slice::from_ref(&square), &[], 2.5) < 1.0);
        // Turning from 0° on the left to 90° on the right.
        let turning = FlowField::Grid {
            origin: v2(0.0, 0.0),
            cell: 100.0,
            cols: 4,
            angles: (0..16).map(|i| 30.0 * (i % 4) as f64).collect(),
        };
        assert!(check(&turning, &[square], &[], 2.5) < 15.0);
        // A bad grid is refused.
        let bad = FlowField::Grid {
            origin: v2(0.0, 0.0),
            cell: 100.0,
            cols: 3,
            angles: vec![0.0; 4],
        };
        assert!(bad.check().is_err());
    }
}
