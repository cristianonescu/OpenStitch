//! Region sew types beyond fill and satin: programmable fill, motif fill, piping, cross
//! stitch, concentric circle, radial, spiral, flexible spiral, stippling, net fill, zigzag net
//! fill and decorative fill. Independent algorithms specified from the public guide's
//! attribute lists. Lengths are 0.1 mm; output is deterministic (pseudo-random choices use a
//! fixed seed).

use crate::fmath::Det;
use crate::generate::{self, RegionParams, RowSpan, Run, subdivide};
use crate::geometry::{Contour, Rect, Span, Vec2, contains, v2};
use crate::model::{CrossKind, RegionSewing, RegionType};
use crate::paths::{Rng, TravelGraph, Walker, clip, join, order};
use crate::patterns::{self, NET_PATTERNS, Pattern, PatternKind, RegionEffect, RegionMask};
use crate::region::{reroute, rotated_bounds, row_positions, spans_at};
use crate::stamp;

/// Caps that keep generation bounded on huge regions or tiny spacings.
const MAX_ROWS: usize = 20_000;
const MAX_POINTS: usize = 2_000_000;

/// Longest programmable-fill stitch between two needle points, 0.1 mm. PE-DESIGN 11.50 sews
/// only where rows cross the pattern lines and leaves gaps of 8 mm unsplit
/// (compat/reference/stitches/prog-fill-square-4pt.pes); longer gaps are split evenly at
/// OpenStitch's general long-stitch bound, 12 mm.
pub const PROGRAMMABLE_MAX_STITCH: f64 = 120.0;

/// Longest travel stitch of a motif or decorative fill, 0.1 mm: between rows and from the
/// start point and back, PE-DESIGN 11.50 walks the outline in even steps of at most 2 mm
/// (compat/reference/stitches/motif-fill-2pt.pes, deco-fill-1line-100mm.pes).
pub const MOTIF_FILL_TRAVEL_PITCH: f64 = 20.0;

/// Layers (runs) of region type `r.kind` (not fill or satin), rows at `angle` degrees.
pub fn generate(
    closed: &[Contour],
    r: &RegionSewing,
    angle: f64,
    holes: &[Contour],
    user: &[Pattern],
) -> Vec<Vec<Run>> {
    let pitch = r.run_pitch_mm() * 10.0;
    let spacing = r.spacing_mm() * 10.0;
    let row_spacing = 10.0 / r.density.max(0.1);
    let (pw, ph) = r.pattern_size_mm();
    let (pw, ph) = (pw * 10.0, ph * 10.0);
    let region = Region {
        closed,
        holes,
        travel: r.travel_path().is_some(),
        pitch,
        join_pitch: pitch.max(10.0),
    };
    match r.kind {
        RegionType::Fill | RegionType::Satin => Vec::new(),
        RegionType::ProgrammableFill => {
            let pattern = patterns::resolve(user, PatternKind::Fill, r.pattern.as_deref());
            let p = RegionParams {
                angle,
                row_spacing,
                stitch_length: r.stitch_length * 10.0,
                pull_compensation: r.pull_compensation * 10.0,
            };
            let tile = Tile {
                pattern,
                width: pw,
                height: ph,
                row_offset: r.row_offset.unwrap_or(0.0) / 100.0,
                column_offset: r.column_offset.unwrap_or(0.0) / 100.0,
            };
            let mut layers = Vec::new();
            if r.base_sewing {
                let base = RegionParams {
                    angle: angle + 90.0,
                    row_spacing: row_spacing * 2.0,
                    ..p
                };
                // Frequency (or the random stagger) staggers the base sewing only, as in
                // PE-DESIGN (greyed there without Base Sewing): the pattern rows have no grid.
                layers.push(region.rows(
                    generate::fill_shifted(closed, &base, holes, r.fill_stagger()),
                    base.stitch_length,
                ));
            }
            // `programmable` places every needle point itself (crossings, area edges, splits
            // of long gaps); the rows are only rerouted, not split again.
            let split = r.split_length.map(|l| l * 10.0);
            let limit = match split {
                // The travel between rows keeps to the split length too.
                Some(l) if r.stamps.is_empty() => l,
                _ => PROGRAMMABLE_MAX_STITCH.max(stamp::EMBOSS_MAX),
            };
            layers.push(region.rows(programmable(closed, holes, &p, &tile, split), limit));
            layers
        }
        RegionType::Piping => {
            let p = RegionParams {
                angle,
                row_spacing,
                stitch_length: pitch,
                pull_compensation: 0.0,
            };
            vec![region.rows(piping(closed, holes, &p, r.frequency), pitch)]
        }
        RegionType::MotifFill => {
            let pattern = patterns::resolve(user, PatternKind::Motif, r.pattern.as_deref());
            let period = pw + spacing;
            // The motif's base line runs through the middle of its row band, as in PE-DESIGN's
            // motif cell (base line at 512 of 1024): rows start half a motif below the grid
            // origin, the region's top-left corner (compat/reference/stitches/motif-fill-2pt.pes).
            let unit: Vec<Vec2> = pattern.paths[0]
                .iter()
                .map(|q| v2(q.x * pw, q.y * ph + ph / 2.0))
                .chain(std::iter::once(v2(period, ph / 2.0)))
                .collect();
            let shift = r.row_offset.unwrap_or(50.0) / 100.0;
            let mut rows = tile_rows(closed, holes, angle, &unit, period, ph + spacing, shift);
            // PE-DESIGN starts at the row nearest the start point and walks the outline
            // between rows in even steps of at most 2 mm (motif-fill-2pt.pes: 5 mm in three).
            if let (Some(e), Some(first), Some(last)) = (
                r.entry,
                rows.first().and_then(|p| p.first()).copied(),
                rows.last().and_then(|p| p.last()).copied(),
            ) && last.distance(e) < first.distance(e)
            {
                rows.reverse();
                for row in &mut rows {
                    row.reverse();
                }
            }
            let region = Region {
                join_pitch: MOTIF_FILL_TRAVEL_PITCH,
                ..region
            };
            // Each row split to the pitch, without points under 0.3 mm apart (#617).
            let runs = rows
                .into_iter()
                .map(|row| {
                    generate::drop_close(&subdivide(&row, pitch), generate::MOTIF_MIN_STITCH, pitch)
                })
                .collect();
            vec![region.sequence_raw(runs)]
        }
        RegionType::DecorativeFill => {
            let pattern = patterns::resolve(user, PatternKind::Decorative, r.pattern.as_deref());
            if pattern.free_lines {
                let origin = match r.reference {
                    // `region::layers` moved the region so the reference point is (0, 0).
                    Some(_) => v2(0.0, 0.0),
                    None => match Rect::enclosing(closed.iter().flat_map(|c| c.outline_points())) {
                        Some(b) => v2(
                            (b.min_x + b.max_x - pw) / 2.0,
                            (b.min_y + b.max_y - ph) / 2.0,
                        ),
                        None => return Vec::new(),
                    },
                };
                let mut pieces = free_line_pieces(closed, holes, pattern, origin, pw, ph, angle);
                // Start with the piece nearest the start point, then the nearest next.
                if let Some(e) = r.entry
                    && let Some(i) = (0..pieces.len()).min_by(|&a, &b| {
                        let d = |k: usize| {
                            let p = &pieces[k];
                            p[0].distance(e).min(p[p.len() - 1].distance(e))
                        };
                        d(a).total_cmp(&d(b))
                    })
                {
                    pieces.swap(0, i);
                    let p = &mut pieces[0];
                    if p[p.len() - 1].distance(e) < p[0].distance(e) {
                        p.reverse();
                    }
                }
                return vec![region.sequence(nearest_next(pieces))];
            }
            let mut pieces = Vec::new();
            for path in &pattern.paths {
                let unit: Vec<Vec2> = path.iter().map(|q| v2(q.x * pw, q.y * ph)).collect();
                pieces.extend(tile_rows(
                    closed,
                    holes,
                    angle,
                    &unit,
                    pw,
                    ph,
                    pattern.row_shift,
                ));
            }
            let mut runs = region.sequence(pieces);
            if let Some(shift) = r.random_shift.filter(|&s| s > 0.0) {
                jitter(&mut runs, closed, holes, shift / 100.0 * pitch / 2.0);
            }
            vec![runs]
        }
        RegionType::CrossStitch => {
            // The grid lies on the page axes whatever the direction (see `cross_stitch`):
            // PE-DESIGN 11.50 has no Direction for cross stitch (#614).
            let times = r.cross_times().count();
            let runs = match r.cross_kind {
                CrossKind::Full => cross_stitch(closed, holes, spacing, times, r.entry, r.exit),
                CrossKind::Half | CrossKind::Upright => {
                    cross_lines(closed, holes, spacing, times, r.cross_kind)
                }
                CrossKind::Smyrna => {
                    let mut runs = cross_stitch(closed, holes, spacing, times, r.entry, None);
                    runs.extend(cross_lines(
                        closed,
                        holes,
                        spacing,
                        times,
                        CrossKind::Upright,
                    ));
                    runs
                }
            };
            vec![region.pieces_raw(runs)]
        }
        RegionType::ConcentricCircle => {
            // PE-DESIGN 11.50 (see crate::concentric): whole circles, a trim, the cut ones.
            let p = crate::concentric::ConcentricParams {
                center: r.center.unwrap_or_else(|| centre(closed)),
                spacing: row_spacing,
                pitch,
                start_inside: r.start_inside,
                entry: r.entry,
            };
            let cc = crate::concentric::concentric(closed, holes, &p);
            if cc.joined {
                // Outside in, one sequence joined by travel.
                let mut pieces = cc.arcs;
                if cc.full.len() >= 2 {
                    if cc.arcs_first {
                        pieces.push(cc.full);
                    } else {
                        pieces.insert(0, cc.full);
                    }
                }
                return vec![region.sequence_raw(pieces)];
            }
            let arcs = region.sequence_raw(cc.arcs);
            let full: Vec<Run> = if cc.full.len() >= 2 {
                vec![cc.full]
            } else {
                Vec::new()
            };
            vec![if cc.arcs_first {
                arcs.into_iter().chain(full).collect()
            } else {
                full.into_iter().chain(arcs).collect()
            }]
        }
        RegionType::Radial => {
            let c = r.center.unwrap_or_else(|| centre(closed));
            vec![region.sequence_raw(radial(closed, holes, c, row_spacing, pitch, r.entry))]
        }
        RegionType::Spiral => {
            // PE-DESIGN 11.50 (see crate::spiral): the outline offset a row per turn.
            let p = crate::spiral::SpiralParams {
                spacing: row_spacing,
                pitch,
                reverse_winding: r.counterclockwise,
                start_inside: r.start_inside,
                entry: r.entry,
            };
            let sewn = crate::spiral::spiral(closed, holes, &p);
            let mut runs = region.sequence_raw(sewn.paths);
            // The closing run is its own run: the compiler trims before (or after) it.
            if let Some(closing) = sewn.closing.filter(|c| c.len() >= 2) {
                if sewn.closing_first {
                    runs.insert(0, closing);
                } else {
                    runs.push(closing);
                }
            }
            vec![runs]
        }
        RegionType::FlexibleSpiral => {
            let spiral = flexible_spiral(closed, spacing, pitch, r.counterclockwise, r.entry);
            let mut pieces = clip(&spiral, closed, holes);
            if r.start_inside {
                pieces.reverse();
                pieces.iter_mut().for_each(|p| p.reverse());
            }
            vec![region.sequence(nearest_next(pieces))]
        }
        RegionType::Stippling => {
            // The meander is a smooth curve of many short segments: sew it at the Run Pitch,
            // at most half the spacing so its turns stay round (never at its 0.2 mm vertices).
            // At most 2 mm on wide spacings too (#462).
            let step = pitch.min(spacing / 2.0).clamp(5.0, 20.0);
            let mut loops: Vec<Vec<Vec2>> = stipple_fitted(closed, holes, angle, spacing, None)
                .iter()
                .map(|l| crate::ornament::resample_closed(l, step))
                .collect();
            // Start (and so end) at the meander's point nearest the start point: the travel
            // there and back is one short stitch in from the outline, not a walk along it
            // (the exposed L of #462).
            if let Some(e) = r.entry.or(r.exit)
                && let Some((li, pi)) = loops
                    .iter()
                    .enumerate()
                    .flat_map(|(li, l)| l.iter().enumerate().map(move |(pi, p)| (li, pi, *p)))
                    .min_by(|a, b| e.distance(a.2).total_cmp(&e.distance(b.2)))
                    .map(|(li, pi, _)| (li, pi))
            {
                let first = loops.remove(li);
                loops.insert(0, first[pi..].iter().chain(&first[..pi]).copied().collect());
            }
            vec![region.loops(loops)]
        }
        RegionType::Maze => {
            // Beyond PE-DESIGN (#533): the stippling walk with straight corridors and square
            // turns, each closed loop sewn at the run pitch from its point nearest the start.
            let maze = Corridors {
                straightness: r.maze_straightness(),
                seed: r.maze_seed.unwrap_or(RegionSewing::DEFAULT_MAZE_SEED),
            };
            let mut loops: Vec<Vec<Vec2>> =
                stipple_fitted(closed, holes, angle, spacing, Some(maze))
                    .iter()
                    .map(|l| {
                        let mut closed_l = l.clone();
                        closed_l.push(l[0]);
                        let mut out = subdivide(&closed_l, pitch);
                        out.pop();
                        out
                    })
                    .collect();
            if let Some(e) = r.entry.or(r.exit)
                && let Some((li, pi)) = loops
                    .iter()
                    .enumerate()
                    .flat_map(|(li, l)| l.iter().enumerate().map(move |(pi, p)| (li, pi, *p)))
                    .min_by(|a, b| e.distance(a.2).total_cmp(&e.distance(b.2)))
                    .map(|(li, pi, _)| (li, pi))
            {
                let first = loops.remove(li);
                loops.insert(0, first[pi..].iter().chain(&first[..pi]).copied().collect());
            }
            vec![region.loops(loops)]
        }
        RegionType::Meander => {
            // Beyond PE-DESIGN (#533): the tile chained into rows a tile height apart (shifted
            // by its row shift), clipped to the region, the pieces sewn nearest next and
            // joined along the outline.
            let pattern = patterns::resolve(user, PatternKind::Meander, r.pattern.as_deref());
            let unit: Vec<Vec2> = pattern.paths[0]
                .iter()
                .map(|q| v2(q.x * pw, q.y * ph))
                .collect();
            let mut pieces = tile_rows(closed, holes, angle, &unit, pw, ph, pattern.row_shift);
            if let Some(e) = r.entry
                && let Some(i) = (0..pieces.len()).min_by(|&a, &b| {
                    let d = |k: usize| {
                        let p = &pieces[k];
                        p[0].distance(e).min(p[p.len() - 1].distance(e))
                    };
                    d(a).total_cmp(&d(b))
                })
            {
                pieces.swap(0, i);
                let p = &mut pieces[0];
                if p[p.len() - 1].distance(e) < p[0].distance(e) {
                    p.reverse();
                }
            }
            // The tiles are drawn with points well under a stitch apart: split to the run
            // pitch, without points under 0.3 mm apart (as the motif fill, #617).
            let runs = nearest_next(pieces)
                .into_iter()
                .map(|p| {
                    generate::drop_close(&subdivide(&p, pitch), generate::MOTIF_MIN_STITCH, pitch)
                })
                .collect();
            vec![region.sequence_raw(runs)]
        }
        RegionType::Contour => {
            // Beyond PE-DESIGN (see crate::contour): rows that follow the outline.
            let p = crate::contour::ContourParams {
                spacing,
                pitch,
                strategy: r.contour_strategy,
                rounded: r.contour_join == crate::model::ContourJoin::Round,
                echo_lines: r.echo_lines,
                counterclockwise: r.counterclockwise,
                start_inside: r.start_inside,
                entry: r.entry,
            };
            vec![region.sequence_raw(crate::contour::contour(closed, holes, &p))]
        }
        RegionType::NetFill => {
            let id = r.pattern.as_deref().unwrap_or(NET_PATTERNS[0].id);
            let out = net_families(closed, holes, angle, spacing, id)
                .into_iter()
                .flatten()
                .map(|p| subdivide(&p, pitch))
                .collect();
            vec![region.sequence_raw(out)]
        }
        RegionType::ZigzagNetFill if angle.rem_euclid(360.0).abs() > 1e-9 => {
            // PE-DESIGN 11.50 turns the net by the Direction (round 13, zigzag-net-dir0/30/45/90,
            // #1043): the whole net (lines, bands, rungs, travel) is made on the shape turned
            // back by that angle, at Direction 0, and its stitches turned with it, so a turned
            // net is exactly an axis net turned. About the reference point (the origin here) or
            // the shape's centre.
            let about = match r.reference {
                Some(_) => v2(0.0, 0.0),
                None => Rect::enclosing(closed.iter().flat_map(|c| c.outline_points()))
                    .map_or(v2(0.0, 0.0), |b| b.center()),
            };
            let turn = angle.rem_euclid(360.0);
            let back = crate::geometry::Transform::rotation(-turn, about);
            let with = crate::geometry::Transform::rotation(turn, about);
            let turned = |cs: &[Contour]| -> Vec<Contour> {
                cs.iter().map(|c| c.transformed(&back)).collect()
            };
            generate(&turned(closed), r, 0.0, &turned(holes), user)
                .into_iter()
                .map(|layer| {
                    layer
                        .into_iter()
                        .map(|run| run.into_iter().map(|p| with.apply(p)).collect())
                        .collect()
                })
                .collect()
        }
        RegionType::ZigzagNetFill => {
            let id = r.pattern.as_deref().unwrap_or(NET_PATTERNS[0].id);
            // PE-DESIGN 11.50 (o-region-zigzag-net): a 3 mm satin band along every net line, a
            // rung straight across then a diagonal back, 0.44 mm apart, inside the outline.
            let width = pw.min(spacing * 0.8);
            let step = 10.0 / r.density.max(0.1);
            // PE-DESIGN 11.50 (o-region-zigzag-net) sews the net on the page axes at Direction 0,
            // its first lines half a band in from the shape's left and top edges; the lines start
            // from the shape, not from the page origin (#616). Another Direction is handled above.
            // With a reference point, `region::layers` put it at (0, 0): the net starts there.
            let lines: Vec<Vec<Vec2>> = match r.reference {
                Some(_) => net_families(closed, holes, 0.0, spacing, id)
                    .into_iter()
                    .flatten()
                    .collect(),
                None => net_from_shape(closed, holes, spacing, width / 2.0, id),
            };
            // A net edge two lines share (the honeycomb's rows meet along theirs) is sewn once:
            // a second band over it piled thread into knots on the machine (#293).
            let pieces = once_each(lines.clone());
            // The directions of the net edges leaving each vertex, where bands meet.
            let mut leaving: std::collections::HashMap<(i64, i64), Vec<Vec2>> =
                std::collections::HashMap::new();
            for l in &lines {
                for w in l.windows(2) {
                    let len = w[0].distance(w[1]);
                    if len > 1e-9 {
                        for (v, d) in [(w[0], w[1] - w[0]), (w[1], w[0] - w[1])] {
                            let d = d * (1.0 / len);
                            let list = leaving.entry(net_key(v)).or_default();
                            if list.iter().all(|e| e.distance(d) > 1e-6) {
                                list.push(d);
                            }
                        }
                    }
                }
            }
            let edges = |v: Vec2| leaving.get(&net_key(v)).cloned().unwrap_or_default();
            let out = along_net(&lines, pieces, pitch, |piece| {
                let band = zigzag_band(piece, width, step, closed, holes, &edges);
                if band.len() < 2 {
                    return None;
                }
                // Starting and ending on the line itself, so travel between lines runs along
                // the net, under the bands. A piece ending at the outline (nothing else leaves
                // its end) is sewn out along its line and back as a band, as with under sewing,
                // so the needle does not travel back over the band it has just sewn.
                let (first, last) = (piece[0], *piece.last().expect("2+ points"));
                let mut run = Vec::new();
                let end = if r.underlay || edges(last).len() <= 1 {
                    run.extend(subdivide(piece, pitch));
                    run.extend(band.into_iter().rev());
                    first
                } else {
                    run.push(first);
                    run.extend(band);
                    last
                };
                // No stitch shorter than 0.3 mm (a rung cut short at a corner or the outline
                // next to the line's end): the needle point is left out, the run still ends on
                // the line.
                let mut kept: Run = Vec::with_capacity(run.len() + 1);
                for p in run {
                    if kept
                        .last()
                        .is_none_or(|q: &Vec2| q.distance(p) >= ZIGZAG_NET_MIN_STITCH)
                    {
                        kept.push(p);
                    }
                }
                if kept
                    .last()
                    .is_some_and(|q| q.distance(end) < ZIGZAG_NET_MIN_STITCH)
                {
                    kept.pop();
                }
                kept.push(end);
                Some((kept, end))
            });
            vec![region.sequence_raw(out)]
        }
    }
}

/// The region being sewn and how pieces are joined.
struct Region<'a> {
    closed: &'a [Contour],
    holes: &'a [Contour],
    travel: bool,
    pitch: f64,
    /// Longest stitch of the travel that joins pieces.
    join_pitch: f64,
}

impl Region<'_> {
    fn graph(&self) -> Option<TravelGraph> {
        self.travel.then(|| {
            let mut g = TravelGraph::new(self.closed, self.holes);
            g.along_edges = true;
            g
        })
    }

    /// Row runs (fill-like): connections across gaps rerouted inside when travelling; no
    /// stitch longer than `limit`.
    fn rows(&self, runs: Vec<Run>, limit: f64) -> Vec<Run> {
        let runs = if self.travel {
            reroute(runs, self.closed, self.holes)
        } else {
            runs
        };
        runs.iter().map(|r| subdivide(r, limit)).collect()
    }

    /// Pieces already in sewing order: split to `pitch`, joined.
    fn sequence(&self, pieces: Vec<Vec<Vec2>>) -> Vec<Run> {
        let runs = pieces
            .into_iter()
            .map(|p| subdivide(&p, self.pitch))
            .collect();
        self.sequence_raw(runs)
    }

    /// Runs already split into stitches, in sewing order: joined.
    fn sequence_raw(&self, runs: Vec<Run>) -> Vec<Run> {
        let runs: Vec<Run> = runs.into_iter().filter(|r| r.len() >= 2).collect();
        let mut graph = self.graph();
        join(runs, graph.as_mut(), self.join_pitch)
    }

    /// Runs whose stitches are already final (cross stitch groups): ordered, joined.
    fn pieces_raw(&self, runs: Vec<Run>) -> Vec<Run> {
        let runs = order_runs(runs);
        self.sequence_raw(runs)
    }

    /// Closed loops: each starts at its point nearest the previous end.
    fn loops(&self, loops: Vec<Vec<Vec2>>) -> Vec<Run> {
        let mut pos: Option<Vec2> = None;
        let mut remaining = loops;
        let mut out = Vec::new();
        while !remaining.is_empty() {
            let here = pos.unwrap_or(remaining[0][0]);
            let (li, pi) = remaining
                .iter()
                .enumerate()
                .flat_map(|(li, l)| l.iter().enumerate().map(move |(pi, p)| (li, pi, *p)))
                .min_by(|a, b| here.distance(a.2).total_cmp(&here.distance(b.2)))
                .map(|(li, pi, _)| (li, pi))
                .expect("non-empty loops");
            let l = remaining.remove(li);
            let mut rotated: Vec<Vec2> = l[pi..].iter().chain(&l[..pi]).copied().collect();
            rotated.push(rotated[0]);
            pos = Some(rotated[0]);
            out.push(rotated);
        }
        self.sequence_raw(out)
    }
}

/// Orders runs greedily like [`order`], keeping their points.
fn order_runs(runs: Vec<Run>) -> Vec<Run> {
    order(runs.into_iter().filter(|r| !r.is_empty()).collect(), None)
}

/// Centre of the region's bounding box.
pub fn centre(closed: &[Contour]) -> Vec2 {
    Rect::enclosing(closed.iter().flat_map(Contour::outline_points))
        .map(|b| b.center())
        .unwrap_or_default()
}

/// Largest distance from `c` to the outline.
fn reach(closed: &[Contour], c: Vec2) -> f64 {
    closed
        .iter()
        .flat_map(Contour::outline_points)
        .map(|p| p.distance(c))
        .fold(0.0, f64::max)
}

// ------------------------------------------------------------------ row-based types

/// Spans of every row of the region at `p.angle`, `p.row_spacing` apart.
fn row_spans(closed: &[Contour], holes: &[Contour], p: &RegionParams) -> Vec<RowSpan> {
    let ys = row_positions(closed, p.angle, p.row_spacing, None);
    let mut out = Vec::new();
    for (row, &y) in ys.iter().enumerate().take(MAX_ROWS) {
        for s in spans_at(closed, holes, p.angle, y) {
            let (lo, hi) = (
                s.start() - p.pull_compensation,
                s.end() + p.pull_compensation,
            );
            if hi > lo {
                out.push(RowSpan {
                    row,
                    y,
                    range: lo..=hi,
                });
            }
        }
    }
    out
}

/// Needle positions on a span: every `l`, the grid shifted by `phase` (0…1 of `l`), with
/// points closer than a quarter stitch to an end dropped.
fn grid_points(span: &Span, l: f64, phase: f64) -> Vec<f64> {
    let (a, b) = (*span.start(), *span.end());
    let offset = phase * l;
    let mut xs = vec![a];
    let mut k = ((a - offset) / l).floor() + 1.0;
    while offset + k * l < b {
        let x = offset + k * l;
        if x - a > l * 0.25 && b - x > l * 0.25 {
            xs.push(x);
        }
        k += 1.0;
    }
    xs.push(b);
    xs
}

/// Piping: rows of running stitches at `p.stitch_length`, each row's needle points shifted
/// by `frequency` % of a stitch from the previous row.
fn piping(
    closed: &[Contour],
    holes: &[Contour],
    p: &RegionParams,
    frequency: Option<f64>,
) -> Vec<Run> {
    let f = frequency.unwrap_or(33.0) / 100.0;
    let spans = row_spans(closed, holes, p);
    generate::chain_spans(&spans, p, |span, row, forward| {
        let mut xs = grid_points(span, p.stitch_length, (row as f64 * f).fract());
        if !forward {
            xs.reverse();
        }
        xs
    })
}

/// A programmable-fill tile placement.
struct Tile<'a> {
    pattern: &'a Pattern,
    width: f64,
    height: f64,
    /// Shift of every other tile row, fraction of the width.
    row_offset: f64,
    /// Shift of every other tile column, fraction of the height.
    column_offset: f64,
}

/// Programmable fill, as PE-DESIGN 11.50 sews it (compat/reference/stitches/prog-fill-*.pes):
/// rows at the region's direction and density whose needle points are the row's two ends and
/// every point where the row crosses a line of the pattern tiled on the page axes (tile
/// origin at the region's top-left corner or the reference point; see `region::layers`).
/// A row that follows the previous one along the same edge leaves out its first end point, so
/// consecutive rows share their turning point and no stitch runs along the outline (PE-DESIGN
/// sews 3, 5, 7… points per row on the line tile, never a turn of two points). Nothing else
/// is added: gaps between crossings stay one stitch up to
/// [`PROGRAMMABLE_MAX_STITCH`] and are split evenly beyond it. Embossed and engraved tile
/// areas add needle points at their edges, drop those inside embossed areas and split
/// engraved stretches into short stitches.
///
/// `split` (beyond PE-DESIGN, which has no stitch length for this type; #459): gaps between
/// crossings longer than this are split evenly too, so a large or sparse pattern (12 mm
/// diamonds at 2.2 lines/mm sews 8-12 mm stitches) can be kept to a sewable length. The added
/// points divide each gap evenly, so they line up from row to row like the crossings and the
/// pattern's lines still read.
fn programmable(
    closed: &[Contour],
    holes: &[Contour],
    p: &RegionParams,
    tile: &Tile,
    split: Option<f64>,
) -> Vec<Run> {
    let plain = split.map_or(PROGRAMMABLE_MAX_STITCH, |l| l.min(PROGRAMMABLE_MAX_STITCH));
    let rad = p.angle.to_radians();
    let spans = row_spans(closed, holes, p);
    let mask = tile.pattern.region_mask();
    generate::chain_satin_spans(&spans, p, |span, row, forward| {
        let (a, b) = (*span.start(), *span.end());
        let y = row_y(&spans, row);
        let (from, to) = (v2(a, y).rotated(rad), v2(b, y).rotated(rad));
        let mut xs = vec![a, b];
        xs.extend(
            tile_crossings(tile, from, to, p.row_spacing, p.stitch_length / 3.0)
                .into_iter()
                .map(|t| a + (b - a) * t)
                .filter(|&m| m - a > 2.0 && b - m > 2.0),
        );
        xs.sort_by(f64::total_cmp);
        xs.dedup_by(|x, y| (*x - *y).abs() < 2.0);
        let effects = mask
            .as_ref()
            .map(|m| effect_spans(tile, m, a, b, y, rad))
            .unwrap_or_default();
        let effect = |x: f64| {
            effects
                .iter()
                .find(|(s, e, _)| x > *s && x < *e)
                .map(|&(_, _, f)| f)
        };
        if !effects.is_empty() {
            // Needle points at every area edge, except where a crossing already marks it (the
            // mask's edges lie within two of its cells of the line); none inside an embossed
            // area.
            let tol = 2.0 * tile.width.max(tile.height) / patterns::MASK_CELLS as f64;
            let edges: Vec<f64> = effects
                .iter()
                .flat_map(|&(s, e, _)| [s, e])
                .filter(|&x| x > a && x < b && xs.iter().all(|m| (m - x).abs() >= tol))
                .collect();
            xs.extend(edges);
            xs.retain(|&x| effect(x) != Some(RegionEffect::Emboss));
            xs.sort_by(f64::total_cmp);
            xs.dedup_by(|x, y| (*x - *y).abs() < 1.0);
        }
        // Long gaps split evenly (short in engraved areas, long in embossed ones).
        let mut out = vec![xs[0]];
        for w in xs.windows(2) {
            let max = match effect((w[0] + w[1]) / 2.0) {
                Some(RegionEffect::Engrave) => stamp::ENGRAVE_STEP,
                Some(RegionEffect::Emboss) => stamp::EMBOSS_MAX.max(PROGRAMMABLE_MAX_STITCH),
                None => plain,
            };
            let n = generate::parts(w[1] - w[0], max);
            out.extend((1..=n).map(|k| w[0] + (w[1] - w[0]) * k as f64 / n as f64));
        }
        if !forward {
            out.reverse();
        }
        out
    })
}

/// Programmable fill along a direction curve (#923): the rows are the curve's offsets
/// ([`crate::direction::guided_lines`], as plain fill along a curve), and the pattern is tiled in
/// the curve's own frame (along it, across it) from `origin`'s place in that frame. Each row is
/// a straight line in that frame, so its needle points are where it crosses the tiled pattern's
/// lines ([`tile_crossings`], as on a straight row), carried back onto the curved row. Long gaps
/// are split along the row, often enough that no stitch strays more than a third of a row
/// spacing from it. The pieces come back in sewing order, still to be joined. `None` where the
/// curve cannot guide, or the pattern has embossed or engraved areas or the region has stamps
/// (those keep the chord warp).
pub(crate) fn programmable_guided(
    closed: &[Contour],
    holes: &[Contour],
    r: &crate::model::RegionSewing,
    user: &[Pattern],
    curve: &Contour,
    origin: Vec2,
) -> Option<Vec<Run>> {
    let pattern = patterns::resolve(user, PatternKind::Fill, r.pattern.as_deref());
    if pattern.region_mask().is_some() || !r.stamps.is_empty() {
        return None;
    }
    let (pw, ph) = r.pattern_size_mm();
    let tile = Tile {
        pattern,
        width: pw * 10.0,
        height: ph * 10.0,
        row_offset: r.row_offset.unwrap_or(0.0) / 100.0,
        column_offset: r.column_offset.unwrap_or(0.0) / 100.0,
    };
    let spacing = 10.0 / r.density.max(0.1);
    let plain = r.split_length.map_or(PROGRAMMABLE_MAX_STITCH, |l| {
        (l * 10.0).min(PROGRAMMABLE_MAX_STITCH)
    });
    let (lines, guide) = crate::direction::guided_lines(closed, holes, curve, spacing)?;
    let (s0, d0) = guide.frame(origin);
    let mut rows: Vec<Vec<Vec<Vec2>>> = Vec::new();
    for (d, pieces) in lines {
        let mut row = Vec::new();
        for piece in pieces {
            if let Some(run) = guided_piece(&piece, &guide, &tile, (s0, d0), d, spacing, plain) {
                row.push(run);
            }
        }
        rows.push(row);
    }
    let path = curve.path();
    let (a, z) = (path[0], path[path.len() - 1]);
    Some(order_guided_rows(
        rows,
        (z.y - a.y).datan2(z.x - a.x).to_degrees(),
    ))
}

/// One row piece of [`programmable_guided`]: its ends and its crossings with the pattern tiled
/// in the guide's frame (offset by `origin`, the frame position of the tiling origin), as
/// needle points on the piece.
fn guided_piece(
    piece: &[Vec2],
    guide: &crate::direction::Guide,
    tile: &Tile,
    origin: (f64, f64),
    d: f64,
    spacing: f64,
    plain: f64,
) -> Option<Vec<Vec2>> {
    if piece.len() < 2 {
        return None;
    }
    // Arc length along the piece and the guide position of each of its points.
    let mut arc = vec![0.0];
    for w in piece.windows(2) {
        arc.push(arc[arc.len() - 1] + w[0].distance(w[1]));
    }
    let s: Vec<f64> = piece.iter().map(|&p| guide.frame(p).0).collect();
    let (sa, sb) = (s[0], s[s.len() - 1]);
    if (sb - sa).abs() < 1e-6 {
        return None;
    }
    // Arc length at guide position `t` (s is monotone along an offset row).
    let arc_at = |t: f64| -> f64 {
        for i in 0..s.len() - 1 {
            let (lo, hi) = (s[i].min(s[i + 1]), s[i].max(s[i + 1]));
            if t >= lo - 1e-9 && t <= hi + 1e-9 {
                let f = if (s[i + 1] - s[i]).abs() < 1e-12 {
                    0.0
                } else {
                    (t - s[i]) / (s[i + 1] - s[i])
                };
                return arc[i] + (arc[i + 1] - arc[i]) * f.clamp(0.0, 1.0);
            }
        }
        if (t - sa).abs() < (t - sb).abs() {
            0.0
        } else {
            arc[arc.len() - 1]
        }
    };
    let point_at = |a: f64| -> Vec2 {
        let i = arc.partition_point(|&x| x < a).clamp(1, arc.len() - 1);
        let f = (a - arc[i - 1]) / (arc[i] - arc[i - 1]).max(1e-12);
        piece[i - 1].lerp(piece[i], f.clamp(0.0, 1.0))
    };
    let total = arc[arc.len() - 1];
    let from = v2(sa - origin.0, d - origin.1);
    let to = v2(sb - origin.0, d - origin.1);
    let mut cuts: Vec<f64> = vec![0.0, total];
    cuts.extend(
        tile_crossings(tile, from, to, spacing, spacing)
            .into_iter()
            .map(|t| arc_at(sa + (sb - sa) * t))
            .filter(|&a| a > 2.0 && total - a > 2.0),
    );
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|x, y| (*x - *y).abs() < 2.0);
    // Long gaps split evenly along the row, and often enough to stay on it where it bends.
    let mut out = vec![point_at(cuts[0])];
    for w in cuts.windows(2) {
        let gap = w[1] - w[0];
        let mut n = generate::parts(gap, plain);
        loop {
            let ok = (1..=n).all(|k| {
                let (a0, a1) = (
                    w[0] + gap * (k - 1) as f64 / n as f64,
                    w[0] + gap * k as f64 / n as f64,
                );
                let (p, q) = (point_at(a0), point_at(a1));
                (0..=8).all(|m| {
                    let x = point_at(a0 + (a1 - a0) * m as f64 / 8.0);
                    crate::simplify::segment_distance(x, p, q) <= spacing / 3.0
                })
            });
            if ok || n > 64 {
                break;
            }
            n += 1;
        }
        out.extend((1..=n).map(|k| point_at(w[0] + gap * k as f64 / n as f64)));
    }
    (out.len() >= 2).then_some(out)
}

/// Origin of tile (`i`, `j`) on the page: tile columns `i` along x, rows `j` along y; odd
/// columns shift down by the column offset, odd rows right by the row offset.
fn tile_origin(tile: &Tile, i: i64, j: i64) -> Vec2 {
    let (w, h) = (tile.width, tile.height);
    let xo = if j.rem_euclid(2) == 1 {
        tile.row_offset * w
    } else {
        0.0
    };
    let yo = if i.rem_euclid(2) == 1 {
        tile.column_offset * h
    } else {
        0.0
    };
    v2(i as f64 * w + xo, j as f64 * h + yo)
}

/// Unit-tile coordinates of page point `q` (the inverse of [`tile_origin`]'s tiling).
fn tile_uv(tile: &Tile, q: Vec2) -> Vec2 {
    let (w, h) = (tile.width, tile.height);
    let j0 = (q.y / h).floor() as i64;
    let i0 = (q.x / w).floor() as i64;
    let mut best = v2(0.0, 0.0);
    for j in [j0, j0 - 1] {
        for i in [i0, i0 - 1] {
            let o = tile_origin(tile, i, j);
            let uv = v2((q.x - o.x) / w, (q.y - o.y) / h);
            if (0.0..1.0).contains(&uv.x) && (0.0..1.0).contains(&uv.y) {
                return uv;
            }
            best = uv;
        }
    }
    best
}

/// Pieces (start, end, effect) of the row a…b at rotated `y` (rows at `rad`) inside embossed
/// or engraved areas of the tile, found every 0.2 mm.
fn effect_spans(
    tile: &Tile,
    mask: &RegionMask,
    a: f64,
    b: f64,
    y: f64,
    rad: f64,
) -> Vec<(f64, f64, RegionEffect)> {
    const STEP: f64 = 2.0;
    let mut out: Vec<(f64, f64, RegionEffect)> = Vec::new();
    let mut open: Option<(f64, RegionEffect)> = None;
    let n = ((b - a) / STEP).ceil().max(1.0) as usize;
    for k in 0..=n {
        let x = a + (b - a) * k as f64 / n as f64;
        let e = mask.effect_at(tile_uv(tile, v2(x, y).rotated(rad)));
        if open.map(|o| o.1) != e {
            if let Some((s, f)) = open.take() {
                out.push((s, x, f));
            }
            open = e.map(|f| (x, f));
        }
    }
    if let Some((s, f)) = open {
        out.push((s, b, f));
    }
    out
}

fn row_y(spans: &[RowSpan], row: usize) -> f64 {
    spans
        .iter()
        .find(|s| s.row == row)
        .map(|s| s.y)
        .unwrap_or_default()
}

/// Where the page segment `from`…`to` (one row) crosses the lines of the pattern tiled on the
/// page axes, as fractions 0…1 of the segment. A pattern line nearly along the row (spanning
/// less than one row spacing across it) adds points every `along_step` along the part it
/// shares with the row, when it lies within half a row spacing of the row, so it still shows.
fn tile_crossings(
    tile: &Tile,
    from: Vec2,
    to: Vec2,
    row_spacing: f64,
    along_step: f64,
) -> Vec<f64> {
    let (w, h) = (tile.width, tile.height);
    let d = to - from;
    let len = d.length();
    if len <= 0.0 || w <= 0.0 || h <= 0.0 {
        return Vec::new();
    }
    let dir = d * (1.0 / len);
    let (lo, hi) = (
        v2(from.x.min(to.x), from.y.min(to.y)),
        v2(from.x.max(to.x), from.y.max(to.y)),
    );
    let mut out = Vec::new();
    // Only the tiles the row passes: per tile column (widened by one tile for the offsets),
    // the tile rows between the row's lowest and highest y over that column.
    let (i0, i1) = ((lo.x / w).floor() as i64 - 2, (hi.x / w).ceil() as i64 + 1);
    let y_at = |x: f64| {
        if d.x.abs() < 1e-9 {
            None
        } else {
            Some(from.y + (x.clamp(lo.x, hi.x) - from.x) * d.y / d.x)
        }
    };
    for i in i0..=i1 {
        let (xa, xb) = ((i - 1) as f64 * w, (i + 2) as f64 * w);
        let (ya, yb) = match (y_at(xa), y_at(xb)) {
            (Some(a), Some(b)) => (a.min(b), a.max(b)),
            _ => (lo.y, hi.y),
        };
        let (j0, j1) = ((ya / h).floor() as i64 - 2, (yb / h).ceil() as i64 + 1);
        for j in j0..=j1 {
            let o = tile_origin(tile, i, j);
            for path in &tile.pattern.paths {
                for seg in path.windows(2) {
                    let p = o + v2(seg[0].x * w, seg[0].y * h);
                    let q = o + v2(seg[1].x * w, seg[1].y * h);
                    let e = q - p;
                    let denom = d.x * e.y - d.y * e.x;
                    if e.length() <= 0.0 {
                        continue;
                    }
                    let across = (e.x * -dir.y + e.y * dir.x).abs();
                    if across < row_spacing {
                        // Along the row: points along the shared part if it is this row's.
                        let off = (p - from).x * -dir.y + (p - from).y * dir.x;
                        if off.abs() <= row_spacing / 2.0 {
                            let (s0, s1) = (
                                ((p - from).x * dir.x + (p - from).y * dir.y) / len,
                                ((q - from).x * dir.x + (q - from).y * dir.y) / len,
                            );
                            let (s0, s1) = (s0.min(s1).max(0.0), s0.max(s1).min(1.0));
                            let step = (along_step / len).max(1e-6);
                            let mut s = s0;
                            while s <= s1 {
                                out.push(s);
                                s += step;
                            }
                        }
                        continue;
                    }
                    let f = p - from;
                    let t = (f.x * e.y - f.y * e.x) / denom;
                    let u = (f.x * d.y - f.y * d.x) / denom;
                    if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u) {
                        out.push(t);
                    }
                }
            }
        }
    }
    out.retain(|&t| t > 0.0 && t < 1.0);
    out.sort_by(f64::total_cmp);
    out
}

// ------------------------------------------------------------------ tiled rows

/// Rows of a periodic polyline (`unit`: one period from x = 0 to x = `period`, in rotated
/// space units) every `row_step`, each row shifted by `shift` periods more than the last;
/// mapped back to design space and clipped to the region minus `holes`. Returns the clipped
/// pieces in sewing order (see [`order_rows`]).
fn tile_rows(
    closed: &[Contour],
    holes: &[Contour],
    angle: f64,
    unit: &[Vec2],
    period: f64,
    row_step: f64,
    shift: f64,
) -> Vec<Vec<Vec2>> {
    let Some(b) = rotated_bounds(closed, angle) else {
        return Vec::new();
    };
    if period <= 0.0 || row_step <= 0.0 || unit.len() < 2 {
        return Vec::new();
    }
    let (ymin, ymax) = unit
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
            (lo.min(p.y), hi.max(p.y))
        });
    let first = ((b.min_y - ymax) / row_step).floor() as i64;
    let last = ((b.max_y - ymin) / row_step).ceil() as i64;
    let rows = (last - first).clamp(0, MAX_ROWS as i64);
    let periods = ((b.width() / period).ceil() as usize + 3).min(MAX_POINTS / unit.len().max(1));
    let mut out: Vec<Vec<Vec<Vec2>>> = Vec::new();
    let mut budget = MAX_POINTS;
    for k in first..first + rows + 1 {
        let y0 = k as f64 * row_step;
        let sx = (k as f64 * shift).rem_euclid(1.0) * period;
        let start = ((b.min_x - sx) / period).floor() - 1.0;
        let mut row: Vec<Vec2> = Vec::with_capacity(periods * unit.len());
        for n in 0..periods {
            let x0 = sx + (start + n as f64) * period;
            for (i, q) in unit.iter().enumerate() {
                if n > 0 && i == 0 {
                    continue;
                }
                row.push(v2(x0 + q.x, y0 + q.y).rotated(angle.to_radians()));
            }
        }
        budget = budget.saturating_sub(row.len());
        if budget == 0 {
            break;
        }
        out.push(clip(&row, closed, holes));
    }
    order_rows(out, angle)
}

/// Orders row pieces like a digitizer sews a fill: up or down through pieces of neighbouring
/// rows that overlap along the row (so each lobe of a shape is finished in one sweep), and
/// only when a sweep ends, on to the nearest unsewn piece. Each piece is entered from the end
/// nearer the needle.
pub(crate) fn order_rows(rows: Vec<Vec<Vec<Vec2>>>, angle: f64) -> Vec<Vec<Vec2>> {
    sweep_rows(rows, angle, false)
}

/// [`order_rows`] for rows along a direction curve (#947): before a sweep steps into a piece
/// that also lies beside another unsewn piece of the row it leaves (two lobes of the region
/// merging, as below a U-shaped curve, where the rows split round the region's corners), it
/// sews that other lobe first, from its far end back to the merge. Otherwise that lobe was
/// left to the end and reached by travel sewn over every row in between.
pub(crate) fn order_guided_rows(rows: Vec<Vec<Vec<Vec2>>>, angle: f64) -> Vec<Vec<Vec2>> {
    sweep_rows(rows, angle, true)
}

fn sweep_rows(rows: Vec<Vec<Vec<Vec2>>>, angle: f64, lobes_first: bool) -> Vec<Vec<Vec2>> {
    let u = v2(1.0, 0.0).rotated(angle.to_radians());
    struct Piece {
        row: usize,
        lo: f64,
        hi: f64,
        pts: Vec<Vec2>,
    }
    let mut pieces: Vec<Piece> = Vec::new();
    for (row, list) in rows.into_iter().enumerate() {
        for pts in list {
            let (lo, hi) = pts
                .iter()
                .fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), p| {
                    let x = p.x * u.x + p.y * u.y;
                    (lo.min(x), hi.max(x))
                });
            pieces.push(Piece { row, lo, hi, pts });
        }
    }
    let n = pieces.len();
    let mut sewn = vec![false; n];
    let mut out = Vec::with_capacity(n);
    if n == 0 {
        return out;
    }
    let end_distance = |p: &Piece, pos: Vec2| {
        pos.distance(p.pts[0])
            .min(pos.distance(*p.pts.last().expect("non-empty")))
    };
    let mut cur = 0;
    let mut pos: Option<Vec2> = None;
    let mut dir: i64 = 1;
    loop {
        sewn[cur] = true;
        let mut pts = std::mem::take(&mut pieces[cur].pts);
        if let Some(at) = pos
            && at.distance(*pts.last().expect("non-empty")) < at.distance(pts[0])
        {
            pts.reverse();
        }
        let here = *pts.last().expect("non-empty");
        pieces[cur].pts = vec![pts[0], here];
        out.push(pts);
        pos = Some(here);
        let (row, lo, hi) = (pieces[cur].row as i64, pieces[cur].lo, pieces[cur].hi);
        let neighbour = |d: i64| {
            (0..n)
                .filter(|&i| {
                    !sewn[i]
                        && pieces[i].row as i64 == row + d
                        && pieces[i].lo < hi
                        && pieces[i].hi > lo
                })
                .min_by(|&a, &b| {
                    end_distance(&pieces[a], here).total_cmp(&end_distance(&pieces[b], here))
                })
        };
        if let Some(next) = neighbour(dir) {
            cur = next;
            // A lobe still to sew beside this piece that merges into `next`: sew it first, from
            // the piece farthest back along it.
            let (nlo, nhi) = (pieces[next].lo, pieces[next].hi);
            let sibling = (0..n).find(|&i| {
                lobes_first
                    && !sewn[i]
                    && pieces[i].row as i64 == row
                    && pieces[i].lo < nhi
                    && pieces[i].hi > nlo
            });
            if let Some(mut far) = sibling {
                while let Some(back) = (0..n).find(|&i| {
                    !sewn[i]
                        && pieces[i].row as i64 == pieces[far].row as i64 - dir
                        && pieces[i].lo < pieces[far].hi
                        && pieces[i].hi > pieces[far].lo
                }) {
                    far = back;
                }
                cur = far;
            }
        } else if let Some(next) = neighbour(-dir) {
            dir = -dir;
            cur = next;
        } else if let Some(next) = (0..n).filter(|&i| !sewn[i]).min_by(|&a, &b| {
            end_distance(&pieces[a], here).total_cmp(&end_distance(&pieces[b], here))
        }) {
            dir = if pieces[next].row as i64 >= row {
                1
            } else {
                -1
            };
            cur = next;
        } else {
            break;
        }
    }
    out
}

/// Pieces in nearest-next order starting with the first piece (as given).
fn nearest_next(pieces: Vec<Vec<Vec2>>) -> Vec<Vec<Vec2>> {
    let start = pieces.first().map(|p| p[0]);
    order(pieces, start)
}

// ------------------------------------------------------------------ free decorative lines

/// Decorative fill with free lines: the tile (`pw` × `ph`, origin `origin`, turned by `angle`
/// about it) repeated as a grid over the region, every line of every copy clipped to the region
/// minus `holes`. PE-DESIGN 11.50 centres its 100 mm tile on the region
/// (compat/reference/stitches/deco-fill-1line-100mm.pes: the line (128, 128)–(896, 896) of the
/// cell crosses the 40 mm square from corner to corner).
fn free_line_pieces(
    closed: &[Contour],
    holes: &[Contour],
    pattern: &Pattern,
    origin: Vec2,
    pw: f64,
    ph: f64,
    angle: f64,
) -> Vec<Vec<Vec2>> {
    if pw <= 0.0 || ph <= 0.0 {
        return Vec::new();
    }
    let rad = angle.to_radians();
    // The region in tile coordinates, to know which copies can reach it.
    let Some(b) = Rect::enclosing(
        closed
            .iter()
            .flat_map(|c| c.outline_points())
            .map(|p| (p - origin).rotated(-rad)),
    ) else {
        return Vec::new();
    };
    let (i0, i1) = ((b.min_x / pw).floor() as i64, (b.max_x / pw).ceil() as i64);
    let (j0, j1) = ((b.min_y / ph).floor() as i64, (b.max_y / ph).ceil() as i64);
    let copies = (i1 - i0 + 1).max(0) as usize * (j1 - j0 + 1).max(0) as usize;
    if copies.saturating_mul(pattern.paths.len()) > MAX_ROWS {
        return Vec::new();
    }
    let mut pieces = Vec::new();
    for j in j0..=j1 {
        for i in i0..=i1 {
            for path in &pattern.paths {
                let poly: Vec<Vec2> = path
                    .iter()
                    .map(|q| origin + v2((i as f64 + q.x) * pw, (j as f64 + q.y) * ph).rotated(rad))
                    .collect();
                pieces.extend(
                    clip(&poly, closed, holes)
                        .into_iter()
                        .filter(|p| p.len() >= 2),
                );
            }
        }
    }
    pieces
}

// ------------------------------------------------------------------ cross stitch

/// Cross stitch: an X in every cell of a `size` grid that lies wholly inside the region.
///
/// The grid lies on the page axes, its lines on multiples of `size` from the design origin, so
/// every cross reads as an X and two regions of the same size share one grid. PE-DESIGN 11.50
/// sews its cross stitch region this way at its default 45° direction
/// (crates/openstitch-formats/tests/fixtures/pe11/o-region-cross.pes); a grid turned with the
/// direction read as "+" at 45° (#463), so the direction does not turn it.
///
/// Each row of neighbouring cells is sewn out and back as Cross Stitch from an image sews it
/// (`openstitch-digitize`'s `cross::sew_run`): out along the row each cell top-left → centre →
/// bottom-left, then "/" in full; back each cell top-right → centre → bottom-right, then "\"
/// in full, ending where the row began. Every diagonal is covered whole and in halves and no
/// stitch runs along a cell side. From one row to the next the needle travels along the half
/// diagonals of the region's crosses only (the nearest row end first), so travel lies under
/// the crosses; only cells no diagonal path reaches start a new run. The sewing starts at the
/// cell corner nearest the start point and goes back along the crosses to the corner nearest
/// the end point, so the travel along the outline from and to them stays short (it ran along
/// two sides of the region, #463).
///
/// `times` is PE-DESIGN's Times (#614): every full diagonal is sewn whole 2·times − 1 times, so
/// with its halves each is covered 2, 4 or 6 times, as PE-DESIGN covers it (Single, Double,
/// Triple; 256 more stitches per step on a 20 mm square, as its references).
fn cross_stitch(
    closed: &[Contour],
    holes: &[Contour],
    size: f64,
    times: usize,
    entry: Option<Vec2>,
    exit: Option<Vec2>,
) -> Vec<Run> {
    let Some(CrossGrid {
        x0,
        y0,
        cols,
        rows,
        sewn,
    }) = cross_grid(closed, holes, size)
    else {
        return Vec::new();
    };
    let extra = 2 * times.max(1) - 2;
    // Runs of neighbouring cells per row: (row, first column, end column).
    let mut segments: Vec<(usize, usize, usize)> = Vec::new();
    for cy in 0..rows {
        let mut cx = 0;
        while cx < cols {
            if !sewn[cy * cols + cx] {
                cx += 1;
                continue;
            }
            let a = cx;
            while cx < cols && sewn[cy * cols + cx] {
                cx += 1;
            }
            segments.push((cy, a, cx));
        }
    }
    // Needle points on a half-cell lattice: node (u, v) is (x0 + u·size/2, y0 + v·size/2);
    // cell corners have even u and v, centres odd ones.
    let w = 2 * cols + 1;
    let nodes = w * (2 * rows + 1);
    let node = |u: usize, v: usize| v * w + u;
    let at = |n: usize| {
        v2(
            x0 + (n % w) as f64 * size / 2.0,
            y0 + (n / w) as f64 * size / 2.0,
        )
    };
    let cell_sewn = |cx: usize, cy: usize| cx < cols && cy < rows && sewn[cy * cols + cx];
    // Half diagonals of sewn cells: a centre to its four corners, a corner to the centres of
    // the sewn cells round it.
    let neighbours = |n: usize, out: &mut Vec<usize>| {
        out.clear();
        let (u, v) = (n % w, n / w);
        if u % 2 == 1 {
            if cell_sewn(u / 2, v / 2) {
                out.extend([
                    node(u - 1, v - 1),
                    node(u + 1, v - 1),
                    node(u - 1, v + 1),
                    node(u + 1, v + 1),
                ]);
            }
            return;
        }
        for (du, dv) in [(-1i64, -1i64), (1, -1), (-1, 1), (1, 1)] {
            let (cu, cv) = (u as i64 + du, v as i64 + dv);
            if cu >= 0 && cv >= 0 && cell_sewn(cu as usize / 2, cv as usize / 2) {
                out.push(node(cu as usize, cv as usize));
            }
        }
    };
    // Breadth-first search from `from` along the half diagonals: steps to every node (MAX where
    // unreachable) and the node each was reached from.
    let search = |from: usize| -> (Vec<usize>, Vec<usize>) {
        let mut dist = vec![usize::MAX; nodes];
        let mut prev = vec![usize::MAX; nodes];
        let mut queue = std::collections::VecDeque::from([from]);
        let mut adjacent = Vec::with_capacity(4);
        dist[from] = 0;
        while let Some(n) = queue.pop_front() {
            neighbours(n, &mut adjacent);
            for &m in &adjacent {
                if dist[m] == usize::MAX {
                    dist[m] = dist[n] + 1;
                    prev[m] = n;
                    queue.push_back(m);
                }
            }
        }
        (dist, prev)
    };
    // A full diagonal from `from` to `to`, sewn whole 2·times − 1 times.
    let diagonal = |run: &mut Run, from: usize, to: usize| {
        run.push(at(to));
        for _ in 0..extra / 2 {
            run.extend([at(from), at(to)]);
        }
    };
    // Out along the row and back (from its left end), or back and out (from its right end);
    // either way the tour ends where it began.
    let tour = |run: &mut Run, (r, a, e): (usize, usize, usize), from_left: bool| {
        let (top, mid, bottom) = (2 * r, 2 * r + 1, 2 * r + 2);
        let out = |run: &mut Run| {
            for i in a..e {
                run.extend([at(node(2 * i + 1, mid)), at(node(2 * i, bottom))]);
                diagonal(run, node(2 * i, bottom), node(2 * i + 2, top));
            }
        };
        let back = |run: &mut Run| {
            for i in (a..e).rev() {
                run.extend([at(node(2 * i + 1, mid)), at(node(2 * i + 2, bottom))]);
                diagonal(run, node(2 * i + 2, bottom), node(2 * i, top));
            }
        };
        if from_left {
            out(run);
            back(run);
        } else {
            back(run);
            out(run);
        }
    };
    let start = |(r, a, e): (usize, usize, usize), left: bool| {
        node(if left { 2 * a } else { 2 * e }, 2 * r)
    };
    // The cell corner nearest `p` (snapped to the outline).
    let corner_near = |p: Vec2| {
        let p = crate::route::snap(closed, p);
        segments
            .iter()
            .flat_map(|&(r, a, e)| (a..=e).flat_map(move |i| [(2 * i, 2 * r), (2 * i, 2 * r + 2)]))
            .map(|(u, v)| node(u, v))
            .min_by(|&m, &n| {
                at(m)
                    .distance(p)
                    .total_cmp(&at(n).distance(p))
                    .then(m.cmp(&n))
            })
    };
    let mut done = vec![false; segments.len()];
    let mut out = Vec::new();
    let mut here = entry.and_then(corner_near);
    let mut run: Run = here.map(|n| vec![at(n)]).unwrap_or_default();
    for _ in 0..segments.len() {
        // The nearest row end the needle can reach along the crosses (ties: the upper row, then
        // the left one, then its left end) …
        let reached = here.and_then(|h| {
            let (dist, prev) = search(h);
            segments
                .iter()
                .enumerate()
                .filter(|&(k, _)| !done[k])
                .flat_map(|(k, &s)| [(k, s, true), (k, s, false)])
                .filter_map(|(k, s, left)| {
                    let d = dist[start(s, left)];
                    (d != usize::MAX).then_some((d, s.0, s.1, !left, k))
                })
                .min()
                .map(|(_, _, _, right, k)| {
                    let mut path = vec![start(segments[k], !right)];
                    while path[path.len() - 1] != h {
                        path.push(prev[path[path.len() - 1]]);
                    }
                    (k, !right, path)
                })
        });
        let (k, left) = match reached {
            Some((k, left, path)) => {
                run.extend(path.iter().rev().skip(1).map(|&n| at(n)));
                (k, left)
            }
            // … else the first row left starts a new run.
            None => {
                let k = done.iter().position(|d| !d).expect("a row is left");
                if run.len() >= 2 {
                    out.push(std::mem::take(&mut run));
                }
                run = vec![at(start(segments[k], true))];
                (k, true)
            }
        };
        tour(&mut run, segments[k], left);
        done[k] = true;
        here = Some(start(segments[k], left));
    }
    if let (Some(h), Some(to)) = (here, exit.and_then(corner_near)) {
        let (dist, prev) = search(h);
        if dist[to] != usize::MAX {
            let mut path = vec![to];
            while path[path.len() - 1] != h {
                path.push(prev[path[path.len() - 1]]);
            }
            run.extend(path.iter().rev().skip(1).map(|&n| at(n)));
        }
    }
    if run.len() >= 2 {
        out.push(run);
    }
    out
}

/// The cross stitch grid: cells `size` square on the page axes from the design origin, cell
/// (cx, cy) at (x0 + cx·size, y0 + cy·size), `sewn` row by row.
struct CrossGrid {
    x0: f64,
    y0: f64,
    cols: usize,
    rows: usize,
    sewn: Vec<bool>,
}

/// The cells of a cross stitch region: those wholly inside it. `None` for an empty region, a
/// size of 0 or a grid too large to sew.
fn cross_grid(closed: &[Contour], holes: &[Contour], size: f64) -> Option<CrossGrid> {
    let b = Rect::enclosing(closed.iter().flat_map(Contour::outline_points))?;
    if size <= 0.0 {
        return None;
    }
    let inside = |p: Vec2| contains(p, closed) && !contains(p, holes);
    let x0 = (b.min_x / size).floor() * size;
    let y0 = (b.min_y / size).floor() * size;
    let cols = ((b.max_x - x0) / size).ceil().max(0.0) as usize;
    let rows = ((b.max_y - y0) / size).ceil().max(0.0) as usize;
    if cols == 0 || rows == 0 || cols.saturating_mul(rows) > MAX_POINTS / 4 {
        return None;
    }
    // A cell is sewn when its corners (drawn in a little), centre, side midpoints and the
    // quarter points of its diagonals are all inside: no cross is cut by the outline.
    const PROBES: [(f64, f64); 13] = [
        (0.02, 0.02),
        (0.98, 0.02),
        (0.02, 0.98),
        (0.98, 0.98),
        (0.5, 0.5),
        (0.5, 0.02),
        (0.5, 0.98),
        (0.02, 0.5),
        (0.98, 0.5),
        (0.25, 0.25),
        (0.75, 0.25),
        (0.25, 0.75),
        (0.75, 0.75),
    ];
    let sewn = (0..rows * cols)
        .map(|k| {
            let (x, y) = (x0 + (k % cols) as f64 * size, y0 + (k / cols) as f64 * size);
            PROBES
                .iter()
                .all(|&(fx, fy)| inside(v2(x + fx * size, y + fy * size)))
        })
        .collect();
    Some(CrossGrid {
        x0,
        y0,
        cols,
        rows,
        sewn,
    })
}

/// Half (`/` only) or upright (`+`) crosses, beyond PE-DESIGN (#614), on [`cross_grid`]'s cells.
/// The strokes of neighbouring cells line up: a half cross's `/` continues into the cell up
/// and to the right, an upright cross's arms into the cells beside and below it. Each line of
/// neighbouring cells is one run, sewn out and back `times` times, so every stroke is covered
/// 2·times times, as [`cross_stitch`] covers a diagonal, and the run ends where it began; the
/// region's travel joins the runs. Any other kind sews nothing.
fn cross_lines(
    closed: &[Contour],
    holes: &[Contour],
    size: f64,
    times: usize,
    kind: CrossKind,
) -> Vec<Run> {
    let Some(CrossGrid {
        x0,
        y0,
        cols,
        rows,
        sewn,
    }) = cross_grid(closed, holes, size)
    else {
        return Vec::new();
    };
    // Point (u, v) of the half-cell lattice.
    let at = |u: usize, v: usize| v2(x0 + u as f64 * size / 2.0, y0 + v as f64 * size / 2.0);
    // Every line as its cells in order: each a list of (cx, cy).
    let mut lines: Vec<Vec<(usize, usize)>> = Vec::new();
    // The sewn cells of `cells`, split where a cell is not sewn, onto `lines`.
    let push_runs = |lines: &mut Vec<Vec<(usize, usize)>>,
                     cells: &mut dyn Iterator<Item = (usize, usize)>| {
        let mut current: Vec<(usize, usize)> = Vec::new();
        for (cx, cy) in cells {
            if sewn[cy * cols + cx] {
                current.push((cx, cy));
            } else if !current.is_empty() {
                lines.push(std::mem::take(&mut current));
            }
        }
        if !current.is_empty() {
            lines.push(current);
        }
    };
    let mut strokes: Vec<Vec<Vec2>> = Vec::new();
    match kind {
        CrossKind::Half => {
            // "/" from a cell's bottom-left corner to its top-right one, the next cell up and
            // to the right: the cells with cx + cy = d, from the bottom row up.
            for d in 0..cols + rows - 1 {
                let mut cells = (0..rows)
                    .rev()
                    .filter(|&cy| d >= cy && d - cy < cols)
                    .map(|cy| (d - cy, cy));
                push_runs(&mut lines, &mut cells);
            }
            for l in &lines {
                let (cx, cy) = l[0];
                let mut s = vec![at(2 * cx, 2 * cy + 2)];
                s.extend(l.iter().map(|&(cx, cy)| at(2 * cx + 2, 2 * cy)));
                strokes.push(s);
            }
        }
        CrossKind::Upright => {
            // Across each row through the cells' middles, then down each column.
            for cy in 0..rows {
                push_runs(&mut lines, &mut (0..cols).map(|cx| (cx, cy)));
            }
            let across = lines.len();
            for cx in 0..cols {
                push_runs(&mut lines, &mut (0..rows).map(|cy| (cx, cy)));
            }
            for (k, l) in lines.iter().enumerate() {
                let (cx, cy) = l[0];
                let s = if k < across {
                    let mut s = vec![at(2 * cx, 2 * cy + 1)];
                    s.extend(l.iter().map(|&(cx, cy)| at(2 * cx + 2, 2 * cy + 1)));
                    s
                } else {
                    let mut s = vec![at(2 * cx + 1, 2 * cy)];
                    s.extend(l.iter().map(|&(cx, cy)| at(2 * cx + 1, 2 * cy + 2)));
                    s
                };
                strokes.push(s);
            }
        }
        CrossKind::Full | CrossKind::Smyrna => {}
    }
    strokes
        .into_iter()
        .map(|s| {
            let mut run = s.clone();
            for k in 0..2 * times.max(1) - 1 {
                if k % 2 == 0 {
                    run.extend(s.iter().rev().skip(1));
                } else {
                    run.extend(s.iter().skip(1));
                }
            }
            run
        })
        .collect()
}

// ------------------------------------------------------------------ circles and spirals

/// Radial lines from the centre, `spacing` apart at the farthest outline point. Lines are
/// thinned towards the centre: a line of level L (the number of trailing zero bits of its
/// index) starts where the lines of level L and higher are half a spacing apart, and never
/// closer than [`RADIAL_MIN_GAP`] to them, so only line 0 reaches the centre and needle points
/// there do not pile up (#461). The lines are sewn in angular order from the one nearest the
/// start point `entry`, alternately in from the outline and back out, the last one out, so the
/// fill starts and ends on the outline. Between two lines at the inner end the needle runs along
/// one of them over its own needle points and steps across, rather than out between the lines.
fn radial(
    closed: &[Contour],
    holes: &[Contour],
    c: Vec2,
    spacing: f64,
    pitch: f64,
    entry: Option<Vec2>,
) -> Vec<Vec<Vec2>> {
    use std::f64::consts::{FRAC_PI_2, TAU};
    let r_max = reach(closed, c) + 1.0;
    let n_min = (TAU * r_max / spacing).ceil().max(8.0) as usize;
    let n = n_min.next_power_of_two().min(1 << 16);
    let gap_min = (spacing / 2.0).max(RADIAL_MIN_GAP);
    // Each line: its pieces inside the region from the centre out, in needle points.
    let mut lines: Vec<Vec<Vec<Vec2>>> = Vec::new();
    for i in 0..n {
        let start = if i == 0 {
            0.0
        } else {
            // The nearest line of this level or higher is `gap` round: at radius `start` it is
            // `start · sin(gap)` away (the opposite line, past a right angle, `start`).
            let gap = TAU * f64::from(1u32 << i.trailing_zeros()) / n as f64;
            gap_min / if gap < FRAC_PI_2 { gap.dsin() } else { 1.0 }
        };
        if start >= r_max {
            continue;
        }
        let dir = v2(1.0, 0.0).rotated(TAU * i as f64 / n as f64);
        // Clipped first, then split: splitting first left slivers of stitches at the outline.
        let pieces: Vec<Vec<Vec2>> = clip(&[c + dir * start, c + dir * r_max], closed, holes)
            .iter()
            // A ray through an outline corner clips to slivers: none under a needle gap.
            .filter(|p| p.len() >= 2 && p[0].distance(p[p.len() - 1]) >= RADIAL_MIN_GAP)
            .map(|p| subdivide(p, pitch))
            .collect();
        if !pieces.is_empty() {
            lines.push(pieces);
        }
    }
    if lines.is_empty() {
        return Vec::new();
    }
    let outer_end = |l: &Vec<Vec<Vec2>>| *l.last().and_then(|p| p.last()).expect("non-empty");
    if let Some(e) = entry
        && let Some(first) = (0..lines.len()).min_by(|&a, &b| {
            outer_end(&lines[a])
                .distance(e)
                .total_cmp(&outer_end(&lines[b]).distance(e))
        })
    {
        lines.rotate_left(first);
    }
    // In, out, in, …, out; with an odd count the first line is sewn in and straight back out.
    let m = lines.len();
    let mut out: Vec<Vec<Vec2>> = Vec::new();
    let mut inward = true;
    for (j, line) in lines.iter().enumerate() {
        let mut pieces: Vec<Vec<Vec2>> = Vec::new();
        if j == 0 && m % 2 == 1 {
            let mut back: Vec<Vec<Vec2>> = line
                .iter()
                .rev()
                .map(|p| p.iter().rev().copied().collect())
                .collect();
            back.extend(line.iter().cloned());
            pieces = back;
        } else if inward {
            pieces.extend(
                line.iter()
                    .rev()
                    .map(|p| p.iter().rev().copied().collect::<Vec<_>>()),
            );
            // From the last line's outer end straight on when that stays inside: travel round
            // by the outline's corners left slivers of stitches there (24-51 under 0.3 mm on a
            // 40 mm circle).
            if let Some(prev) = out.last_mut()
                && let (Some(&a), Some(b)) = (prev.last(), pieces.first().map(|p| p[0]))
            {
                let chord = subdivide(&[a, b], pitch);
                let inside = chord.windows(2).all(|w| {
                    let m = w[0].lerp(w[1], 0.5);
                    contains(m, closed) && !contains(m, holes)
                });
                if a.distance(b) <= pitch || (inside && a.distance(b) <= 3.0 * pitch) {
                    prev.extend(chord.into_iter().skip(1));
                    prev.extend(pieces.remove(0).into_iter().skip(1));
                }
            }
        } else {
            pieces.extend(line.iter().cloned());
            // From the last line's inner end: along the longer of the two lines to the other's
            // inner end, on its own needle points, then one step across.
            if let Some(prev) = out.last_mut()
                && let (Some(&a), Some(b)) = (prev.last(), pieces.first().map(|p| p[0]))
            {
                let (ra, rb) = (a.distance(c), b.distance(c));
                let near = |pts: &[Vec2], r: f64| {
                    (0..pts.len())
                        .min_by(|&x, &y| {
                            (pts[x].distance(c) - r)
                                .abs()
                                .total_cmp(&(pts[y].distance(c) - r).abs())
                        })
                        .unwrap_or(0)
                };
                if ra > rb {
                    let own = &pieces[0];
                    let k = near(own, ra);
                    let mut lead: Vec<Vec2> = own[..=k].iter().rev().copied().collect();
                    lead.extend(own.iter().skip(1).copied());
                    pieces[0] = lead;
                } else {
                    let prev_line = &lines[j - 1][0];
                    let k = near(prev_line, rb);
                    prev.extend(prev_line[1..=k].iter().copied());
                }
            }
        }
        out.extend(pieces);
        inward = !inward;
        if j == 0 && m % 2 == 1 {
            inward = true;
        }
    }
    out
}

/// Least distance (0.1 mm) at which a radial line starts from the lines beside it.
const RADIAL_MIN_GAP: f64 = 4.0;

/// Flexible spiral: the region's outer outline shrunk towards its centre, turn by turn, so the
/// spiral follows the shape. Each turn shrinks by `spacing` relative to the mean distance of
/// the outline from the centre. Starts outside, on the outline at the start point (`entry`)
/// when set, and ends in the middle.
fn flexible_spiral(
    closed: &[Contour],
    spacing: f64,
    pitch: f64,
    ccw: bool,
    entry: Option<Vec2>,
) -> Vec<Vec2> {
    let Some(outer) = closed
        .iter()
        .max_by(|a, b| area(a).abs().total_cmp(&area(b).abs()))
    else {
        return Vec::new();
    };
    let mut ring = match entry {
        Some(e) => outer.starting_at(e).path(),
        None => outer.path(),
    };
    // Clockwise on screen (y down) is positive signed area in this convention.
    if (area(outer) < 0.0) != ccw {
        ring.reverse();
    }
    let c = centroid(outer);
    let w = Walker::new(&ring);
    let len = w.length();
    if len <= 0.0 || spacing <= 0.0 {
        return Vec::new();
    }
    let mean = ring.iter().map(|p| p.distance(c)).sum::<f64>() / ring.len() as f64;
    let shrink = spacing / mean.max(spacing);
    let m = ((len / pitch.max(1.0)).ceil() as usize).max(8);
    let mut out = Vec::new();
    let mut k = 0usize;
    loop {
        let s = (k % m) as f64 / m as f64;
        let scale = 1.0 - (k as f64 / m as f64) * shrink;
        if scale <= 0.0 || out.len() >= MAX_POINTS {
            break;
        }
        let p = c + (w.at(s * len) - c) * scale;
        // Every turn has the same number of points, so they crowd together towards the centre;
        // a needle point under 0.5 mm from the last stitches in place (the ready-to-sew kit's
        // spiral had 52 in a row), so such points are left out.
        if out
            .last()
            .is_none_or(|q: &Vec2| q.distance(p) >= MIN_SPIRAL_STEP)
        {
            out.push(p);
        }
        k += 1;
    }
    if out.last().is_none_or(|q: &Vec2| q.distance(c) > 0.0) {
        out.push(c);
    }
    out
}

/// Shortest step between the flexible spiral's needle points, 0.1 mm.
const MIN_SPIRAL_STEP: f64 = 5.0;

/// Signed area (shoelace) of a closed contour's outline.
fn area(c: &Contour) -> f64 {
    let p = c.outline_points();
    let n = p.len();
    (0..n)
        .map(|i| {
            let (a, b) = (p[i], p[(i + 1) % n]);
            a.x * b.y - b.x * a.y
        })
        .sum::<f64>()
        / 2.0
}

/// Area centroid of a closed contour (bounding-box centre for degenerate ones).
fn centroid(c: &Contour) -> Vec2 {
    let p = c.outline_points();
    let n = p.len();
    let a = area(c);
    if a.abs() < 1e-9 {
        return centre(std::slice::from_ref(c));
    }
    let (mut cx, mut cy) = (0.0, 0.0);
    for i in 0..n {
        let (u, v) = (p[i], p[(i + 1) % n]);
        let f = u.x * v.y - v.x * u.y;
        cx += (u.x + v.x) * f;
        cy += (u.y + v.y) * f;
    }
    v2(cx / (6.0 * a), cy / (6.0 * a))
}

// ------------------------------------------------------------------ stippling

/// [`stipple`] at `spacing`, or, where the region is too narrow for the meander's cells at that
/// spacing (a 7.5 mm ring band at 3 mm), at the largest tighter spacing that fits, in 10 % steps
/// down to half the setting (#708). A region too small even then stays empty.
fn stipple_fitted(
    closed: &[Contour],
    holes: &[Contour],
    angle: f64,
    spacing: f64,
    maze: Option<Corridors>,
) -> Vec<Vec<Vec2>> {
    let mut s = spacing;
    loop {
        let loops = stipple(closed, holes, angle, s, maze);
        if !loops.is_empty() || s * 0.9 < spacing * 0.5 {
            return loops;
        }
        s *= 0.9;
    }
}

/// Maze fill (#533, beyond PE-DESIGN): the spanning tree of [`stipple`] grown towards long
/// straight corridors, and its walk sewn with square turns.
#[derive(Clone, Copy, Debug)]
struct Corridors {
    /// Chance that a branch runs straight on where it can, 0–1.
    straightness: f64,
    seed: u64,
}

/// A spanning-tree edge of the stippling grid: `Right(i)` joins cell i to i + 1, `Down(i)`
/// joins cell i to i + cols.
#[derive(Clone, Copy)]
enum Edge {
    Right(usize),
    Down(usize),
}

/// Stippling: a meandering closed path that never crosses itself, its neighbouring passes
/// `spacing` apart and reaching to about half a spacing from the outline and the holes (#462).
/// 1. A lattice of points about `spacing` apart, fitted to the region's (rotated) bounds so a
///    rectangle's outer points lie half a spacing inside; points 0.35 spacing or more inside
///    are usable.
/// 2. Cells of 2 × 2 usable points get a random spanning tree (fixed seed); the path walks
///    around the tree through every point of its cells (a Hamiltonian cycle of the doubled
///    grid). One loop per connected part.
/// 3. Usable points of the partial cells along the outline are taken in by detours: a step
///    u → v becomes u → u' → v' → v through two free neighbours; loops whose steps face each
///    other one point apart are joined into one.
/// 4. Rounded by corner cutting, then relaxed: each point moves square to the path towards
///    the middle between the passes (or the outline, counted at half a spacing) on its two
///    sides, never so that it crosses or comes near another pass.
///
/// With `maze` (#533) the tree runs straight on with the given chance and its own seed, and
/// the loops keep the lattice's square turns (step 4 is left out), each straight stretch one
/// segment.
fn stipple(
    closed: &[Contour],
    holes: &[Contour],
    angle: f64,
    spacing: f64,
    maze: Option<Corridors>,
) -> Vec<Vec<Vec2>> {
    let Some(b) = rotated_bounds(closed, angle) else {
        return Vec::new();
    };
    let cell = spacing * 2.0;
    if cell <= 0.0 || !cell.is_finite() {
        return Vec::new();
    }
    // Cells fitted to the bounds when that changes the spacing by 15 % or less.
    let fit = |len: f64| {
        let n = (len / cell).round().max(1.0);
        if (len / n - cell).abs() <= 0.15 * cell {
            (n as usize, len / n / 2.0, 0.0)
        } else {
            let n = (len / cell).floor().max(1.0);
            (n as usize, spacing, (len - n * cell) / 2.0)
        }
    };
    let ((cols, sx, mx), (rows, sy, my)) = (fit(b.width()), fit(b.height()));
    if cols.saturating_mul(rows) > MAX_POINTS / 8 {
        return Vec::new();
    }
    let rad = angle.to_radians();
    let edges = EdgeIndex::new(closed.iter().chain(holes), 2.5 * spacing);
    let inside = |p: Vec2, margin: f64| {
        contains(p, closed) && !contains(p, holes) && edges.distance(p, margin).is_none()
    };
    // Fine lattice: w × h points, id = y * w + x.
    let (w, h) = (2 * cols, 2 * rows);
    let pos = |x: usize, y: usize| {
        v2(
            b.min_x + mx + (x as f64 + 0.5) * sx,
            b.min_y + my + (y as f64 + 0.5) * sy,
        )
        .rotated(rad)
    };
    let usable: Vec<bool> = (0..w * h)
        .map(|i| inside(pos(i % w, i / w), 0.35 * spacing))
        .collect();
    let step_ok = |a: (usize, usize), b: (usize, usize)| {
        usable[a.1 * w + a.0]
            && usable[b.1 * w + b.0]
            && inside(pos(a.0, a.1).lerp(pos(b.0, b.1), 0.5), 0.2 * spacing)
    };
    let on: Vec<bool> = (0..rows * cols)
        .map(|i| {
            let (x, y) = (2 * (i % cols), 2 * (i / cols));
            step_ok((x, y), (x + 1, y))
                && step_ok((x, y + 1), (x + 1, y + 1))
                && step_ok((x, y), (x, y + 1))
                && step_ok((x + 1, y), (x + 1, y + 1))
        })
        .collect();
    let mut rng = Rng::new(maze.map_or(0x57_1991E, |m| m.seed));
    let mut visited = vec![false; rows * cols];
    // Maze: the direction each cell was entered from its parent (0 right, 1 left, 2 down,
    // 3 up; 4 for a root).
    let mut came = vec![4u8; rows * cols];
    // Tree edges: right[i] links cell i to i + 1, down[i] links i to i + cols.
    let mut right = vec![false; rows * cols];
    let mut down = vec![false; rows * cols];
    let mut cycles = Vec::new();
    for root in 0..rows * cols {
        if !on[root] || visited[root] {
            continue;
        }
        let mut stack = vec![root];
        visited[root] = true;
        let mut members = vec![root];
        while let Some(&cur) = stack.last() {
            let (cx, cy) = (cur % cols, cur / cols);
            let (x, y) = (2 * cx, 2 * cy);
            // Each option with the tree edge it adds: `(cell, Some(edge))` sets `right[edge]`,
            // `(cell, None)` is resolved below. The move's kind is recorded when it is offered,
            // not read back from the index: with one column `cur + cols` is `cur + 1` (#638).
            let mut options: Vec<(usize, Edge, u8)> = Vec::with_capacity(4);
            if cx + 1 < cols
                && step_ok((x + 1, y), (x + 2, y))
                && step_ok((x + 1, y + 1), (x + 2, y + 1))
            {
                options.push((cur + 1, Edge::Right(cur), 0));
            }
            if cx > 0 && step_ok((x - 1, y), (x, y)) && step_ok((x - 1, y + 1), (x, y + 1)) {
                options.push((cur - 1, Edge::Right(cur - 1), 1));
            }
            if cy + 1 < rows
                && step_ok((x, y + 1), (x, y + 2))
                && step_ok((x + 1, y + 1), (x + 1, y + 2))
            {
                options.push((cur + cols, Edge::Down(cur), 2));
            }
            if cy > 0 && step_ok((x, y - 1), (x, y)) && step_ok((x + 1, y - 1), (x + 1, y)) {
                options.push((cur - cols, Edge::Down(cur - cols), 3));
            }
            options.retain(|&(n, _, _)| on[n] && !visited[n]);
            if options.is_empty() {
                stack.pop();
                continue;
            }
            // Stippling draws once per step, uniformly; the maze first tries to run straight on.
            let straight = maze.and_then(|m| {
                let i = options.iter().position(|o| o.2 == came[cur])?;
                (rng.next_f64() < m.straightness).then_some(i)
            });
            let pick = straight.unwrap_or_else(|| (rng.next_u64() % options.len() as u64) as usize);
            let (next, edge, dir) = options[pick];
            came[next] = dir;
            visited[next] = true;
            members.push(next);
            match edge {
                Edge::Right(i) => right[i] = true,
                Edge::Down(i) => down[i] = true,
            }
            stack.push(next);
        }
        cycles.push(walk_tree(&members, cols, &right, &down));
    }
    // Detours through the free usable points next to each step (step 3).
    let mut next = vec![usize::MAX; w * h];
    for c in &cycles {
        for (k, &(x, y)) in c.iter().enumerate() {
            let (nx, ny) = c[(k + 1) % c.len()];
            next[y * w + x] = ny * w + nx;
        }
    }
    let free = |id: usize, next: &[usize]| usable[id] && next[id] == usize::MAX;
    // The points one aside of the step u → v (on `side`), if on the lattice.
    let aside = |u: usize, v: usize, side: isize| {
        let (dx, dy) = if u / w == v / w { (0, side) } else { (side, 0) };
        let shift = |i: usize| {
            let (x, y) = ((i % w) as isize + dx, (i / w) as isize + dy);
            (x >= 0 && y >= 0 && (x as usize) < w && (y as usize) < h)
                .then(|| y as usize * w + x as usize)
        };
        Some((shift(u)?, shift(v)?))
    };
    let at = |i: usize| (i % w, i / w);
    for c in &cycles {
        let Some(&(x0, y0)) = c.first() else {
            continue;
        };
        let start = y0 * w + x0;
        let mut u = start;
        // Each pass either inserts a detour (two free points used up) or moves on, so a closed
        // loop ends within 3 × the lattice's points; the bound stops links that do not close.
        let mut steps = 0;
        loop {
            steps += 1;
            if steps > 3 * w * h || next[u] >= next.len() {
                break;
            }
            let v = next[u];
            let detour = [1, -1].into_iter().find_map(|side| {
                aside(u, v, side).filter(|&(a, b)| {
                    free(a, &next)
                        && free(b, &next)
                        && step_ok(at(u), at(a))
                        && step_ok(at(a), at(b))
                        && step_ok(at(b), at(v))
                })
            });
            if let Some((a, b)) = detour {
                next[u] = a;
                next[a] = b;
                next[b] = v;
                continue;
            }
            u = v;
            if u == start {
                break;
            }
        }
    }
    // Neighbouring loops (parts joined only corner to corner) become one where two of their
    // steps face each other: u → v and y → x one point aside become u → x and y → v.
    // A loop of the `next` links from `start`; it never holds more than the lattice's points,
    // so links that do not close (a bug) end the walk instead of growing it without end (#638).
    let walk = |start: usize, next: &[usize]| {
        let mut l = vec![start];
        let mut u = next[start];
        while u != start && u < next.len() && l.len() <= next.len() {
            l.push(u);
            u = next[u];
        }
        l
    };
    let mut part = vec![usize::MAX; w * h];
    for u in 0..w * h {
        if next[u] != usize::MAX && part[u] == usize::MAX {
            for i in walk(u, &next) {
                part[i] = u;
            }
        }
    }
    for u in 0..w * h {
        if next[u] == usize::MAX {
            continue;
        }
        for side in [1, -1] {
            let v = next[u];
            let Some((x, y)) = aside(u, v, side) else {
                continue;
            };
            if part[x] == usize::MAX
                || part[x] == part[u]
                || part[y] != part[x]
                || !step_ok(at(u), at(x))
                || !step_ok(at(y), at(v))
            {
                continue;
            }
            if next[x] == y {
                let l = walk(x, &next);
                for k in 0..l.len() {
                    next[l[(k + 1) % l.len()]] = l[k];
                }
            }
            if next[y] == x {
                next[u] = x;
                next[y] = v;
                for i in walk(u, &next) {
                    part[i] = part[u];
                }
                break;
            }
        }
    }
    let mut loops = Vec::new();
    let mut seen = vec![false; w * h];
    for u in 0..w * h {
        if next[u] != usize::MAX && !seen[u] {
            let l = walk(u, &next);
            for &i in &l {
                seen[i] = true;
            }
            loops.push(l.into_iter().map(|i| pos(i % w, i / w)).collect::<Vec<_>>());
        }
    }
    if maze.is_some() {
        return loops.into_iter().map(|l| corners(&l)).collect();
    }
    let pitch = spacing / 4.0;
    let mut loops: Vec<Vec<Vec2>> = loops
        .into_iter()
        .map(|l| crate::ornament::resample_closed(&chaikin(&chaikin(&l)), pitch))
        .collect();
    relax(&mut loops, &edges, spacing);
    loops
}

/// The corners of a closed lattice loop: the points where it turns (straight stretches become
/// one segment each). A loop that never turns keeps its points.
fn corners(l: &[Vec2]) -> Vec<Vec2> {
    let n = l.len();
    if n < 3 {
        return l.to_vec();
    }
    let out: Vec<Vec2> = (0..n)
        .filter(|&i| {
            let (a, b, c) = (l[(i + n - 1) % n], l[i], l[(i + 1) % n]);
            let (u, v) = (b - a, c - b);
            (u.x * v.y - u.y * v.x).abs() > 1e-6 * u.length() * v.length()
        })
        .map(|i| l[i])
        .collect();
    if out.len() < 3 { l.to_vec() } else { out }
}

/// The edges of some contours, bucketed for nearest-edge queries.
struct EdgeIndex {
    edges: Vec<(Vec2, Vec2)>,
    cell: f64,
    buckets: std::collections::HashMap<(i64, i64), Vec<usize>>,
}

impl EdgeIndex {
    fn new<'a>(contours: impl Iterator<Item = &'a Contour>, cell: f64) -> Self {
        let edges: Vec<(Vec2, Vec2)> = contours.flat_map(Contour::edges).collect();
        let mut buckets: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
        let key = |v: f64| (v / cell).floor() as i64;
        for (i, &(a, b)) in edges.iter().enumerate() {
            // Sampled every half cell: an edge closer than half a cell to a point is in the
            // point's bucket.
            let n = ((2.0 * a.distance(b) / cell).ceil() as usize).clamp(1, 1 << 16);
            for k in 0..=n {
                let p = a.lerp(b, k as f64 / n as f64);
                let at = (key(p.x), key(p.y));
                for dx in -1..=1 {
                    for dy in -1..=1 {
                        buckets.entry((at.0 + dx, at.1 + dy)).or_default().push(i);
                    }
                }
            }
        }
        for list in buckets.values_mut() {
            list.sort_unstable();
            list.dedup();
        }
        EdgeIndex {
            edges,
            cell,
            buckets,
        }
    }

    /// The nearest outline point to `p` closer than `within` (at most half a cell), and its
    /// distance.
    fn distance(&self, p: Vec2, within: f64) -> Option<(f64, Vec2)> {
        let key = (
            (p.x / self.cell).floor() as i64,
            (p.y / self.cell).floor() as i64,
        );
        let mut best: Option<(f64, Vec2)> = None;
        for &i in self.buckets.get(&key).into_iter().flatten() {
            let (a, b) = self.edges[i];
            let d = b - a;
            let len2 = d.x * d.x + d.y * d.y;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            let q = a + d * t;
            let dist = p.distance(q);
            if dist < within && best.is_none_or(|(bd, _)| dist < bd) {
                best = Some((dist, q));
            }
        }
        best
    }
}

/// Evens out stippling (step 4 of [`stipple`]): points about a quarter spacing apart along
/// closed loops move square to their loop, a little per round, to the middle between the
/// nearest passes on their two sides; the outline counts as a pass half a spacing beyond.
/// Smoothing keeps the turns round. A move is kept only if the point stays a quarter spacing
/// inside the outline and 0.4 spacing from every other pass, and neither of its two stitches
/// crosses another.
fn relax(loops: &mut [Vec<Vec2>], edges: &EdgeIndex, spacing: f64) {
    use std::collections::HashMap;
    let pitch = spacing / 4.0;
    let reach = 2.5 * spacing;
    let total: usize = loops.iter().map(Vec::len).sum();
    let rounds = 40.min(2_000_000 / total.max(1));
    // Points closer than this many steps along a loop are neighbours on the same pass; all
    // others (the other leg of a turn too) must keep apart.
    let local = (0.75 * spacing / pitch).ceil() as usize;
    let key = |p: Vec2| {
        (
            (p.x / spacing).floor() as i64,
            (p.y / spacing).floor() as i64,
        )
    };
    let o = |p: Vec2, q: Vec2, r: Vec2| (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x);
    let cross = |a: Vec2, b: Vec2, c: Vec2, d: Vec2| {
        o(a, b, c) * o(a, b, d) < 0.0 && o(c, d, a) * o(c, d, b) < 0.0
    };
    for round in 0..rounds {
        if round > 0 && round % 4 == 0 {
            for l in loops.iter_mut() {
                *l = crate::ornament::resample_closed(l, pitch);
            }
        }
        let mut grid: HashMap<(i64, i64), Vec<(usize, usize)>> = HashMap::new();
        for (li, l) in loops.iter().enumerate() {
            for (pi, &p) in l.iter().enumerate() {
                grid.entry(key(p)).or_default().push((li, pi));
            }
        }
        for li in 0..loops.len() {
            let n = loops[li].len();
            if n < 8 {
                continue;
            }
            for pi in 0..n {
                let l = &loops[li];
                let (a, p, c) = (l[(pi + n - 1) % n], l[pi], l[(pi + 1) % n]);
                let t = c - a;
                let tl = t.length();
                if tl <= 0.0 {
                    continue;
                }
                let normal = v2(-t.y / tl, t.x / tl);
                let near = |lj: usize, pj: usize| {
                    lj == li && {
                        let d = pj.abs_diff(pi);
                        d.min(n - d) <= local
                    }
                };
                // Nearest pass on each side, within a 60 degree cone round the normal.
                let mut side = [reach, reach];
                let (kx, ky) = key(p);
                for gx in kx - 3..=kx + 3 {
                    for gy in ky - 3..=ky + 3 {
                        for &(lj, pj) in grid.get(&(gx, gy)).into_iter().flatten() {
                            if near(lj, pj) {
                                continue;
                            }
                            let d = loops[lj][pj] - p;
                            let dist = d.length();
                            let along = d.x * normal.x + d.y * normal.y;
                            if along.abs() > 0.5 * dist {
                                let s = usize::from(along < 0.0);
                                side[s] = side[s].min(dist);
                            }
                        }
                    }
                }
                if let Some((dist, q)) = edges.distance(p, reach / 2.0) {
                    let d = q - p;
                    let along = d.x * normal.x + d.y * normal.y;
                    if along.abs() > 0.5 * dist {
                        let s = usize::from(along < 0.0);
                        side[s] = side[s].min(2.0 * dist);
                    }
                }
                let shift = ((side[0] - side[1]) / 4.0).clamp(-0.1 * spacing, 0.1 * spacing);
                let smooth = (a.lerp(c, 0.5) - p) * 0.1;
                let to = p + normal * shift + smooth;
                if to.distance(p) < 1e-3 {
                    continue;
                }
                // Keep the move only if it is safe. A move shorter than a quarter spacing to
                // a point a quarter spacing from the outline cannot have crossed it.
                if to.distance(p) >= 0.25 * spacing || edges.distance(to, 0.25 * spacing).is_some()
                {
                    continue;
                }
                let mut safe = true;
                let (kx, ky) = key(to);
                'check: for gx in kx - 2..=kx + 2 {
                    for gy in ky - 2..=ky + 2 {
                        for &(lj, pj) in grid.get(&(gx, gy)).into_iter().flatten() {
                            let m = loops[lj].len();
                            let q = loops[lj][pj];
                            if !near(lj, pj) && q.distance(to) < 0.4 * spacing {
                                safe = false;
                                break 'check;
                            }
                            // The stitch pj → pj + 1 against the two moved ones.
                            let pk = (pj + 1) % m;
                            if lj == li && (pj == pi || pk == pi) {
                                continue;
                            }
                            let r = loops[lj][pk];
                            if (!(lj == li && pk == (pi + n - 1) % n) && cross(a, to, q, r))
                                || (!(lj == li && pj == (pi + 1) % n) && cross(to, c, q, r))
                            {
                                safe = false;
                                break 'check;
                            }
                        }
                    }
                }
                if safe {
                    loops[li][pi] = to;
                }
            }
        }
    }
}

/// The Hamiltonian cycle of the doubled grid around a spanning tree: each tree cell is a ring
/// of four quarter cells; a tree edge swaps the two ring edges facing it for two edges joining
/// the rings. Returns the cycle (without the closing repeat) as (fine x, fine y) points.
fn walk_tree(members: &[usize], cols: usize, right: &[bool], down: &[bool]) -> Vec<(usize, usize)> {
    use std::collections::HashMap;
    type Node = (usize, usize);
    let mut adj: HashMap<Node, Vec<Node>> = HashMap::new();
    let link = |a: Node, b: Node, adj: &mut HashMap<Node, Vec<Node>>| {
        adj.entry(a).or_default().push(b);
        adj.entry(b).or_default().push(a);
    };
    for &i in members {
        let (x, y) = (2 * (i % cols), 2 * (i / cols));
        let (tl, tr, bl, br) = ((x, y), (x + 1, y), (x, y + 1), (x + 1, y + 1));
        // Top and left ring edges always exist unless a tree edge from the neighbour above /
        // to the left replaces them; right and bottom unless this cell's own edge does.
        let from_left = i % cols > 0 && right[i - 1];
        let from_above = i >= cols && down[i - cols];
        if !from_above {
            link(tl, tr, &mut adj);
        }
        if !from_left {
            link(tl, bl, &mut adj);
        }
        if right[i] {
            link(tr, (x + 2, y), &mut adj);
            link(br, (x + 2, y + 1), &mut adj);
        } else {
            link(tr, br, &mut adj);
        }
        if down[i] {
            link(bl, (x, y + 2), &mut adj);
            link(br, (x + 1, y + 2), &mut adj);
        } else {
            link(bl, br, &mut adj);
        }
    }
    let Some(&start) = members.first() else {
        return Vec::new();
    };
    let start = (2 * (start % cols), 2 * (start / cols));
    let mut path = vec![start];
    let mut prev = start;
    let mut cur = adj
        .get(&start)
        .and_then(|n| n.first())
        .copied()
        .unwrap_or(start);
    while cur != start && path.len() <= adj.len() {
        path.push(cur);
        let next = adj[&cur]
            .iter()
            .copied()
            .find(|&n| n != prev)
            .unwrap_or(prev);
        prev = cur;
        cur = next;
    }
    path
}

/// One pass of Chaikin corner cutting on a closed loop (no closing repeat).
fn chaikin(l: &[Vec2]) -> Vec<Vec2> {
    let n = l.len();
    if n < 3 {
        return l.to_vec();
    }
    let mut out = Vec::with_capacity(2 * n);
    for i in 0..n {
        let (a, b) = (l[i], l[(i + 1) % n]);
        out.push(a.lerp(b, 0.25));
        out.push(a.lerp(b, 0.75));
    }
    out
}

// ------------------------------------------------------------------ nets

/// The line families of net pattern `id`: per family, clipped pieces in row order.
/// The net lines of pattern `id` on the page axes, lined up so that a line runs `inset` in from
/// the left and the top of the shape's bounds (PE-DESIGN's zigzag net, #616).
fn net_from_shape(
    closed: &[Contour],
    holes: &[Contour],
    spacing: f64,
    inset: f64,
    id: &str,
) -> Vec<Vec<Vec2>> {
    let Some(b) = Rect::enclosing(closed.iter().flat_map(|c| c.outline_points())) else {
        return Vec::new();
    };
    let o = v2(b.min_x + inset, b.min_y + inset);
    let to_origin = crate::geometry::Transform::translation(-o.x, -o.y);
    let shift =
        |cs: &[Contour]| -> Vec<Contour> { cs.iter().map(|c| c.transformed(&to_origin)).collect() };
    net_families(&shift(closed), &shift(holes), 0.0, spacing, id)
        .into_iter()
        .flatten()
        .map(|line| line.into_iter().map(|p| p + o).collect())
        .collect()
}

fn net_families(
    closed: &[Contour],
    holes: &[Contour],
    angle: f64,
    spacing: f64,
    id: &str,
) -> Vec<Vec<Vec<Vec2>>> {
    let line = |a: f64, step: f64| {
        let unit = [v2(0.0, 0.0), v2(step, 0.0)];
        tile_rows(closed, holes, angle + a, &unit, step, step, 0.0)
    };
    let s = spacing;
    match id {
        "diamond" => vec![line(45.0, s), line(-45.0, s)],
        "triangle" => {
            let h = s * 3f64.sqrt() / 2.0;
            vec![line(0.0, h), line(60.0, h), line(120.0, h)]
        }
        "honeycomb" => {
            let a = s / 3f64.sqrt();
            let h = a * 3f64.sqrt() / 2.0;
            let unit = [
                v2(0.0, 0.0),
                v2(a, 0.0),
                v2(1.5 * a, h),
                v2(2.5 * a, h),
                v2(3.0 * a, 0.0),
            ];
            vec![tile_rows(closed, holes, angle, &unit, 3.0 * a, h, 0.5)]
        }
        "star" => vec![
            line(0.0, s),
            line(90.0, s),
            line(45.0, s * std::f64::consts::SQRT_2),
            line(-45.0, s * std::f64::consts::SQRT_2),
        ],
        "waves" => {
            let unit: Vec<Vec2> = (0..=16)
                .map(|k| {
                    let t = k as f64 / 16.0;
                    v2(t * 2.0 * s, (t * std::f64::consts::TAU).dsin() * s / 4.0)
                })
                .collect();
            vec![
                tile_rows(closed, holes, angle, &unit, 2.0 * s, s, 0.0),
                tile_rows(closed, holes, angle + 90.0, &unit, 2.0 * s, s, 0.0),
            ]
        }
        _ => vec![line(0.0, s), line(90.0, s)],
    }
}

/// The net lines with every stretch an earlier line already covers taken out (each line split
/// where it was), so no edge is sewn twice.
fn once_each(lines: Vec<Vec<Vec2>>) -> Vec<Vec<Vec2>> {
    const NEAR: f64 = 0.5;
    let covered = |a: Vec2, b: Vec2, kept: &[(Vec2, Vec2)]| {
        let m = a.lerp(b, 0.5);
        kept.iter().any(|&(c, d)| {
            let cd = d - c;
            let len2 = cd.x * cd.x + cd.y * cd.y;
            if len2 <= 1e-12 {
                return false;
            }
            let on = |p: Vec2| {
                let t = ((p - c).x * cd.x + (p - c).y * cd.y) / len2;
                (-1e-6..=1.0 + 1e-6).contains(&t) && (c + cd * t).distance(p) <= NEAR
            };
            on(a) && on(b) && on(m)
        })
    };
    let mut kept: Vec<(Vec2, Vec2)> = Vec::new();
    let mut out = Vec::new();
    for line in lines {
        let mut cur: Vec<Vec2> = Vec::new();
        for w in line.windows(2) {
            let (a, b) = (w[0], w[1]);
            if a.distance(b) <= 1e-9 {
                continue;
            }
            if covered(a, b, &kept) {
                if cur.len() >= 2 {
                    out.push(std::mem::take(&mut cur));
                }
                cur.clear();
                continue;
            }
            if cur.is_empty() {
                cur.push(a);
            }
            cur.push(b);
            kept.push((a, b));
        }
        if cur.len() >= 2 {
            out.push(cur);
        }
    }
    out
}

/// A satin band `width` wide along `line`, as PE-DESIGN's zigzag net sews it: every `step` along
/// the line a rung straight across, then on to the next rung's first side. Along each edge the
/// rungs stay square to it. Where the line meets other net edges (`edges` gives the directions
/// of every net edge leaving a point, the line's own included), each rung is cut back at the
/// line halving the angle to each of them, so bands meeting at a corner or a junction share
/// it wedge by wedge instead of fanning or piling over each other (#463); at the line's own
/// corners one rung runs from the corner out along that halving line on the outer side (at
/// most √2 × the half width). Every rung is cut to the region: no needle point lies outside
/// it.
fn zigzag_band(
    line: &[Vec2],
    width: f64,
    step: f64,
    closed: &[Contour],
    holes: &[Contour],
    edges: &dyn Fn(Vec2) -> Vec<Vec2>,
) -> Run {
    let n = line.len();
    if n < 2 || step <= 0.0 || width <= 0.0 {
        return line.to_vec();
    }
    let unit = |d: Vec2| d * (1.0 / d.x.dhypot(d.y).max(1e-12));
    let normal = |a: Vec2, b: Vec2| {
        let d = unit(b - a);
        v2(-d.y, d.x)
    };
    let dot = |a: Vec2, b: Vec2| a.x * b.x + a.y * b.y;
    // Turns of less than a degree count as running straight on.
    const STRAIGHT: f64 = 0.999_85;
    // The halving lines at vertex `v` between the edge leaving it along `own` and every other
    // net edge there.
    let halving = |v: Vec2, own: Vec2| -> Vec<Vec2> {
        edges(v)
            .into_iter()
            .filter(|&e| dot(e, own) < 1.0 - 1e-9)
            .map(|e| {
                let m = own + e;
                if m.x.dhypot(m.y) < 1e-9 {
                    v2(-own.y, own.x)
                } else {
                    unit(m)
                }
            })
            .collect()
    };
    // `p` drawn back towards `c` onto the line through `v` along `m` when it lies past it.
    let before = |c: Vec2, p: Vec2, v: Vec2, m: Vec2| {
        let side = |q: Vec2| m.x * (q.y - v.y) - m.y * (q.x - v.x);
        let (sc, sp) = (side(c), side(p));
        if sc.abs() < 1e-9 || sc * sp >= 0.0 {
            p
        } else {
            c.lerp(p, sc / (sc - sp))
        }
    };
    // The mitre at each inner vertex of the line (None at its ends and where it turns back).
    let mitres: Vec<Option<Vec2>> = (0..n)
        .map(|i| {
            if i == 0 || i + 1 == n {
                return None;
            }
            let (u, v) = (normal(line[i - 1], line[i]), normal(line[i], line[i + 1]));
            let m = u + v;
            if m.x.dhypot(m.y) < 1e-9 || dot(u, v) > STRAIGHT {
                return None;
            }
            let m = unit(m);
            // At most √2 out (a right-angle corner), so no rung grows past √2 × the width.
            let cos = dot(m, v).max(std::f64::consts::FRAC_1_SQRT_2);
            Some(m * (1.0 / cos))
        })
        .collect();
    let inside = |p: Vec2| contains(p, closed) && !contains(p, holes);
    // A rung from `a` to `b` about `c`, cut to the region (the part nearest `c`), or none.
    let cut = |c: Vec2, a: Vec2, b: Vec2| -> Option<(Vec2, Vec2)> {
        if a.distance(b) < 1e-6 {
            return None;
        }
        if inside(a) && inside(b) && inside(c) {
            return Some((a, b));
        }
        clip(&[a, b], closed, holes)
            .into_iter()
            .filter(|p| p.len() >= 2)
            .map(|p| (p[0], p[p.len() - 1]))
            .min_by(|x, y| {
                let d = |(p, q): (Vec2, Vec2)| {
                    let (pq, pc) = (q - p, c - p);
                    let t = (dot(pc, pq) / dot(pq, pq).max(1e-12)).clamp(0.0, 1.0);
                    (p + pq * t).distance(c)
                };
                d(*x).total_cmp(&d(*y))
            })
    };
    let half = width / 2.0;
    let mut run = Vec::new();
    let mut rung = |c: Vec2, a: Vec2, b: Vec2| {
        if let Some((a, b)) = cut(c, a, b) {
            run.push(a);
            run.push(b);
        }
    };
    let mut t = 0.0;
    for i in 0..n - 1 {
        let (a, b) = (line[i], line[i + 1]);
        let len = a.distance(b);
        if len <= 1e-9 {
            continue;
        }
        let d = unit(b - a);
        let off = normal(a, b) * half;
        // Bands are cut only where the line ends or turns: where it runs straight on through a
        // crossing, its band crosses the other band whole, as PE-DESIGN's square net does
        // (o-region-zigzag-net.pes).
        let ends_or_turns = |k: usize| k == 0 || k + 1 == n || mitres[k].is_some();
        let cuts: Vec<(Vec2, Vec2)> = (if ends_or_turns(i) {
            halving(a, d)
        } else {
            Vec::new()
        })
        .into_iter()
        .map(|m| (a, m))
        .chain(
            (if ends_or_turns(i + 1) {
                halving(b, d * -1.0)
            } else {
                Vec::new()
            })
            .into_iter()
            .map(|m| (b, m)),
        )
        .collect();
        while t <= len + 1e-9 {
            let c = a.lerp(b, t / len);
            // No rung stands on a vertex where other edges meet: the wedges pinch to it (at the
            // line's own corners the mitre rung below stands there).
            let at_vertex = (t < 1e-6 && !cuts.iter().all(|&(v, _)| v != a))
                || (t > len - 1e-6 && !cuts.iter().all(|&(v, _)| v != b));
            if !at_vertex {
                let mut ends = [c - off, c + off];
                for &(v, m) in &cuts {
                    for e in &mut ends {
                        *e = before(c, *e, v, m);
                    }
                }
                rung(c, ends[0], ends[1]);
            }
            t += step;
        }
        t -= len;
        if let Some(m) = mitres[i + 1] {
            // The mitre rung on the outer side only, from the corner out (the rungs either side
            // already meet on the inner side), and none where another edge leaves there.
            let next = unit(line[i + 2] - b);
            let others: Vec<Vec2> = edges(b)
                .into_iter()
                .filter(|&e| dot(e, d * -1.0) < 1.0 - 1e-9 && dot(e, next) < 1.0 - 1e-9)
                .collect();
            let end = |p: Vec2| {
                if dot(p - b, d) <= 0.0 || others.iter().any(|&e| dot(e, p - b) > 0.0) {
                    b
                } else {
                    p
                }
            };
            rung(b, end(b - m * half), end(b + m * half));
        }
    }
    run.dedup_by(|a, b| a.distance(*b) < 1e-9);
    run
}

/// Shortest zigzag net stitch, 0.1 mm: a needle point nearer the previous one is left out.
const ZIGZAG_NET_MIN_STITCH: f64 = 3.0;

/// A net vertex as a map key (to 0.001 mm: the lines share their vertices exactly).
fn net_key(p: Vec2) -> (i64, i64) {
    ((p.x * 100.0).round() as i64, (p.y * 100.0).round() as i64)
}

/// The zigzag net's lines `pieces` (each sewn once, see [`once_each`]) as runs: each piece's
/// band from `sew`, the needle travelling from one piece to the nearest unsewn piece end along
/// the net's own lines `net`, which the bands cover, so no travel crosses an open cell (#463).
/// Pieces the net does not connect start a new run.
fn along_net(
    net: &[Vec<Vec2>],
    pieces: Vec<Vec<Vec2>>,
    pitch: f64,
    mut sew: impl FnMut(&[Vec2]) -> Option<(Run, Vec2)>,
) -> Vec<Run> {
    use std::cmp::Reverse;
    use std::collections::{BinaryHeap, HashMap};
    let key = net_key;
    let mut ids: HashMap<(i64, i64), usize> = HashMap::new();
    let mut points: Vec<Vec2> = Vec::new();
    let mut edges: Vec<Vec<(usize, f64)>> = Vec::new();
    for l in net.iter().chain(&pieces) {
        for w in l.windows(2) {
            let [a, b] = [w[0], w[1]].map(|p| {
                *ids.entry(key(p)).or_insert_with(|| {
                    points.push(p);
                    edges.push(Vec::new());
                    points.len() - 1
                })
            });
            if a != b {
                let d = w[0].distance(w[1]);
                edges[a].push((b, d));
                edges[b].push((a, d));
            }
        }
    }
    let node = |p: Vec2| ids.get(&key(p)).copied();
    let ends: Vec<(usize, usize)> = pieces
        .iter()
        .map(|p| {
            let at = |q: Vec2| node(q).expect("a piece end is a net node");
            (at(p[0]), at(p[p.len() - 1]))
        })
        .collect();
    // Shortest paths from `from` along the net: (distance, previous node) per node.
    let paths = |from: usize| {
        let mut best = vec![(f64::INFINITY, usize::MAX); points.len()];
        best[from].0 = 0.0;
        let mut heap = BinaryHeap::from([(Reverse(0u64), from)]);
        while let Some((Reverse(bits), n)) = heap.pop() {
            let d = f64::from_bits(bits);
            if d > best[n].0 {
                continue;
            }
            for &(m, w) in &edges[n] {
                let e = d + w;
                if e < best[m].0 {
                    best[m] = (e, n);
                    heap.push((Reverse(e.to_bits()), m));
                }
            }
        }
        best
    };
    let mut done = vec![false; pieces.len()];
    let mut out = Vec::new();
    let mut run: Run = Vec::new();
    let mut here: Option<usize> = None;
    for _ in 0..pieces.len() {
        // The nearest piece end along the net (ties: earlier piece, its start) …
        let next = here.and_then(|h| {
            let best = paths(h);
            (0..pieces.len())
                .filter(|&k| !done[k])
                .flat_map(|k| [(k, false), (k, true)])
                .filter_map(|(k, rev)| {
                    let e = if rev { ends[k].1 } else { ends[k].0 };
                    best[e].0.is_finite().then_some((best[e].0, k, rev, e))
                })
                .min_by(|x, y| x.0.total_cmp(&y.0).then((x.1, x.2).cmp(&(y.1, y.2))))
                .map(|(_, k, rev, e)| {
                    let mut path = vec![points[e]];
                    let mut n = e;
                    while n != h {
                        n = best[n].1;
                        path.push(points[n]);
                    }
                    path.reverse();
                    (k, rev, path)
                })
        });
        let (k, rev) = match next {
            Some((k, rev, path)) => {
                run.extend(subdivide(&path, pitch).into_iter().skip(1));
                here = Some(if rev { ends[k].1 } else { ends[k].0 });
                (k, rev)
            }
            // … else the first piece left starts a new run.
            None => {
                if run.len() >= 2 {
                    out.push(std::mem::take(&mut run));
                }
                let k = done.iter().position(|d| !d).expect("a piece is left");
                here = Some(ends[k].0);
                (k, false)
            }
        };
        done[k] = true;
        let mut piece = pieces[k].clone();
        if rev {
            piece.reverse();
        }
        if let Some((sewn, end)) = sew(&piece) {
            let skip = usize::from(run.last() == sewn.first());
            run.extend(sewn.into_iter().skip(skip));
            here = node(end).or(here);
        }
    }
    if run.len() >= 2 {
        out.push(run);
    }
    out
}

/// Moves needle points by up to `amount` (deterministic), keeping each inside the region.
fn jitter(runs: &mut [Run], closed: &[Contour], holes: &[Contour], amount: f64) {
    let mut rng = Rng::new(0xDEC0_5EED);
    for run in runs.iter_mut() {
        for p in run.iter_mut() {
            let d = v2(rng.next_f64() * 2.0 - 1.0, rng.next_f64() * 2.0 - 1.0) * amount;
            let q = *p + d;
            if contains(q, closed) && !contains(q, holes) {
                *p = q;
            }
        }
    }
}
