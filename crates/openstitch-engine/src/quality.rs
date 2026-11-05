//! Quality checks, beyond PE-DESIGN 11 (which has none): Check Design (#503), the density map
//! (#504), Review Sewing Order (#505) and the read-back of exported files (#457). They live in
//! one module so the editor, `stitchtool exec`, the MCP server and the Assistant report the
//! same findings. Every check reads the design and its compiled stitches; only
//! `design.cleanup` changes anything.
//!
//! Errors use `tools/sew_check.py`'s limits, which PE-DESIGN 11's own files and the NV880E
//! sew-outs set (#295): 20 needle drops in a 0.5 mm spot, 80 mm of thread per mm² in a 2 mm
//! cell, 6 stitches under 0.3 mm in a row, nothing outside the hoop. The other limits are the
//! issues' provisional ones and are warnings: satins 1–10 mm, fills at least 1.5 mm wide,
//! objects at least 1.5 mm, text at least 4 mm (#465), same-thread moves under 15 mm, and the
//! thread layers of the density map (heavy from 2 layers, too dense from 3 on a standard woven).

use crate::commands::{CommandSpec, Effect, MM, opt, spec};
use crate::params::{CommandError, Params};
use openstitch_core::{Command, StitchPattern};
use openstitch_design::{
    Compiler, Contour, Design, DesignObject, LineType, ObjectKind, RegionType, Vec2, v2,
};
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::ops::Range;

/// The longest stitch a Brother home machine sews, mm.
pub const MAX_STITCH_MM: f64 = 12.1;
const STACK_CELL_MM: f64 = 0.5;
const MAX_PER_CELL: usize = 20;
const DENSE_CELL_MM: f64 = 2.0;
const MAX_THREAD_PER_MM2: f64 = 80.0;
const TINY_MM: f64 = 0.3;
const MAX_TINY_RUN: usize = 6;
const SATIN_WIDE_MM: f64 = 10.0;
const SATIN_NARROW_MM: f64 = 1.0;
const FILL_THIN_MM: f64 = 1.5;
const TINY_OBJECT_MM: f64 = 1.5;
const TEXT_MIN_MM: f64 = 4.0;
const SAME_THREAD_MOVE_MM: f64 = 15.0;
/// Thread one layer lays, mm per mm²: a default tatami fill (`shape.rectangle`, 4.5 rows per
/// mm, underlay off) measures 4.6 in this module's 1 mm cells.
pub const LAYER_MM_PER_MM2: f64 = 4.6;

/// The design compiled once, with the stitch range each visible object wrote.
pub struct Traced {
    pub pattern: StitchPattern,
    pub spans: Vec<(u64, Range<usize>)>,
}

impl Traced {
    pub fn new(d: &Design) -> Self {
        let (pattern, spans) = Compiler::for_design(d).compile_traced(d);
        Traced { pattern, spans }
    }

    /// The object whose stitches include index `i`.
    pub fn owner(&self, i: usize) -> Option<u64> {
        let k = self.spans.partition_point(|(_, r)| r.end <= i);
        self.spans
            .get(k)
            .filter(|(_, r)| r.contains(&i))
            .map(|(id, _)| *id)
    }

    /// Needle drops as (index, from, to) in 0.1 mm; `from` is the previous record's position,
    /// as `sew_check` measures.
    fn drops(&self) -> impl Iterator<Item = (usize, Vec2, Vec2)> + '_ {
        let st = &self.pattern.stitches;
        (0..st.len())
            .filter(move |&i| st[i].command == Command::Stitch)
            .map(move |i| {
                let to = v2(st[i].x, st[i].y);
                let from = i.checked_sub(1).map_or(to, |j| v2(st[j].x, st[j].y));
                (i, from, to)
            })
    }
}

fn cell(p: Vec2, size_mm: f64) -> (i64, i64) {
    let s = size_mm * MM;
    ((p.x / s).floor() as i64, (p.y / s).floor() as i64)
}

fn mm(p: Vec2) -> [f64; 2] {
    [round2(p.x / MM), round2(p.y / MM)]
}

fn round2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

// MARK: density map (#504)

/// Layer limits for a fabric (Fabric Selector ids): heavy and too dense. On a standard woven,
/// one layer is normal, two (two stacked fills) heavy and three too dense (#504); the limits
/// sit half a layer below, as k stacked fills measure k ± 10 %. Light and stretchy fabrics
/// take less, heavy ones more.
pub fn fabric_limits(fabric: &str) -> Option<(f64, f64)> {
    Some(match fabric {
        "thin" => (1.2, 2.2),
        "knit" => (1.3, 2.3),
        "heavy" | "towel" | "fleece" | "felt" => (2.0, 3.0),
        _ => {
            crate::fabric::find(fabric)?;
            (1.5, 2.5)
        }
    })
}

pub struct DensityMap {
    pub cell: f64,
    pub origin: Vec2,
    pub width: usize,
    pub height: usize,
    /// Thread layers per cell (1 = one default tatami fill), smoothed over 3 × 3 cells.
    pub layers: Vec<f64>,
    /// Needle drops per mm².
    pub holes: Vec<f64>,
    /// More than `sew_check`'s 20 drops in a 0.5 mm spot inside the cell.
    pub stacked: Vec<bool>,
    /// The objects that sew in each cell.
    owners: Vec<BTreeSet<u64>>,
}

impl DensityMap {
    pub fn new(t: &Traced, cell_mm: f64, only: Option<&BTreeSet<u64>>) -> Self {
        let b = t.pattern.bounds();
        let s = cell_mm * MM;
        let origin = v2((b.min_x / s).floor() * s, (b.min_y / s).floor() * s);
        let width = ((b.max_x - origin.x) / s).floor() as usize + 1;
        let height = ((b.max_y - origin.y) / s).floor() as usize + 1;
        let n = width * height;
        let mut map = DensityMap {
            cell: cell_mm,
            origin,
            width,
            height,
            layers: vec![0.0; n],
            holes: vec![0.0; n],
            stacked: vec![false; n],
            owners: vec![BTreeSet::new(); n],
        };
        let mut thread = vec![0.0; n];
        let mut spots: HashMap<(i64, i64), usize> = HashMap::new();
        for (i, from, to) in t.drops() {
            let owner = t.owner(i);
            if only.is_some_and(|o| !owner.is_some_and(|id| o.contains(&id))) {
                continue;
            }
            let Some(k) = map.index(to) else { continue };
            map.holes[k] += 1.0 / (cell_mm * cell_mm);
            owner.map(|id| map.owners[k].insert(id));
            *spots.entry(cell(to, STACK_CELL_MM)).or_default() += 1;
            // The stitch's length split over the cells it crosses, in pieces under a quarter
            // cell.
            let len = from.distance(to);
            let pieces = (len / (s / 4.0)).ceil().max(1.0) as usize;
            for j in 0..pieces {
                let f = (j as f64 + 0.5) / pieces as f64;
                let p = v2(from.x + (to.x - from.x) * f, from.y + (to.y - from.y) * f);
                if let Some(k) = map.index(p) {
                    thread[k] += len / pieces as f64 / MM;
                }
            }
        }
        for ((x, y), count) in spots {
            let centre = v2(
                (x as f64 + 0.5) * STACK_CELL_MM * MM,
                (y as f64 + 0.5) * STACK_CELL_MM * MM,
            );
            if count > MAX_PER_CELL
                && let Some(k) = map.index(centre)
            {
                map.stacked[k] = true;
            }
        }
        let (w, h) = (width as i64, height as i64);
        for y in 0..h {
            for x in 0..w {
                let (mut sum, mut cells) = (0.0, 0.0);
                for (dx, dy) in (-1..=1).flat_map(|dx| (-1..=1).map(move |dy| (dx, dy))) {
                    let (cx, cy) = (x + dx, y + dy);
                    if (0..w).contains(&cx) && (0..h).contains(&cy) {
                        sum += thread[(cy * w + cx) as usize];
                    }
                    cells += 1.0;
                }
                map.layers[(y * w + x) as usize] =
                    sum / cells / (cell_mm * cell_mm) / LAYER_MM_PER_MM2;
            }
        }
        map
    }

    fn index(&self, p: Vec2) -> Option<usize> {
        let s = self.cell * MM;
        let (x, y) = ((p.x - self.origin.x) / s, (p.y - self.origin.y) / s);
        (x >= 0.0 && y >= 0.0 && (x as usize) < self.width && (y as usize) < self.height)
            .then(|| y as usize * self.width + x as usize)
    }

    /// 0 thin, 1 ok, 2 heavy, 3 too dense; `None` where nothing sews.
    pub fn class(&self, k: usize, (heavy, too_dense): (f64, f64)) -> Option<u8> {
        let l = self.layers[k];
        if self.stacked[k] || l > too_dense {
            Some(3)
        } else if l > heavy {
            Some(2)
        } else if l >= 0.5 {
            Some(1)
        } else if self.holes[k] > 0.0 || l > 0.05 {
            Some(0)
        } else {
            None
        }
    }

    /// Connected too-dense cells: (cells, the objects sewing there).
    pub fn hotspots(&self, limits: (f64, f64)) -> Vec<(Vec<usize>, BTreeSet<u64>)> {
        let mut seen = vec![false; self.layers.len()];
        let mut out = Vec::new();
        for start in 0..self.layers.len() {
            if seen[start] || self.class(start, limits) != Some(3) {
                continue;
            }
            let (mut cells, mut ids, mut todo) = (Vec::new(), BTreeSet::new(), vec![start]);
            seen[start] = true;
            while let Some(k) = todo.pop() {
                cells.push(k);
                ids.extend(self.owners[k].iter().copied());
                let (x, y) = ((k % self.width) as i64, (k / self.width) as i64);
                for (nx, ny) in [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)] {
                    if nx < 0 || ny < 0 || nx >= self.width as i64 || ny >= self.height as i64 {
                        continue;
                    }
                    let j = ny as usize * self.width + nx as usize;
                    if !seen[j] && self.class(j, limits) == Some(3) {
                        seen[j] = true;
                        todo.push(j);
                    }
                }
            }
            out.push((cells, ids));
        }
        out
    }

    /// Bounds of `cells`, mm: [min x, min y, max x, max y].
    pub fn cells_bbox(&self, cells: &[usize]) -> [f64; 4] {
        let s = self.cell;
        let (ox, oy) = (self.origin.x / MM, self.origin.y / MM);
        let xs = cells.iter().map(|k| (k % self.width) as f64);
        let ys = cells.iter().map(|k| (k / self.width) as f64);
        [
            round2(ox + xs.clone().fold(f64::MAX, f64::min) * s),
            round2(oy + ys.clone().fold(f64::MAX, f64::min) * s),
            round2(ox + (xs.fold(0.0, f64::max) + 1.0) * s),
            round2(oy + (ys.fold(0.0, f64::max) + 1.0) * s),
        ]
    }

    pub fn to_json(&self, metric: &str, limits: (f64, f64)) -> Value {
        let values: &[f64] = if metric == "holes" {
            &self.holes
        } else {
            &self.layers
        };
        let mut counts = [0usize; 4];
        for k in 0..self.layers.len() {
            if let Some(c) = self.class(k, limits) {
                counts[c as usize] += 1;
            }
        }
        let total = counts.iter().sum::<usize>().max(1) as f64;
        let hotspots: Vec<Value> = self
            .hotspots(limits)
            .iter()
            .map(|(cells, ids)| {
                let max = |v: &[f64]| cells.iter().map(|&k| v[k]).fold(0.0, f64::max);
                json!({
                    "bbox": self.cells_bbox(cells),
                    "maxLayers": round2(max(&self.layers)),
                    "maxHolesPerMm2": round2(max(&self.holes)),
                    "stackedDrops": cells.iter().any(|&k| self.stacked[k]),
                    "ids": ids,
                })
            })
            .collect();
        json!({
            "cell": self.cell,
            "origin": mm(self.origin),
            "width": self.width,
            "height": self.height,
            "metric": metric,
            "values": values.iter().map(|&v| round2(v)).collect::<Vec<_>>(),
            "limits": { "heavy": limits.0, "tooDense": limits.1 },
            "classes": {
                "thin": round2(counts[0] as f64 / total),
                "ok": round2(counts[1] as f64 / total),
                "heavy": round2(counts[2] as f64 / total),
                "tooDense": round2(counts[3] as f64 / total),
            },
            "hotspots": hotspots,
        })
    }
}

// MARK: Check Design (#503)

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Severity {
    Error,
    Warning,
    Info,
}

#[derive(Debug)]
pub struct Finding {
    pub rule: &'static str,
    pub severity: Severity,
    pub ids: Vec<u64>,
    /// mm: [min x, min y, max x, max y].
    pub bbox: Option<[f64; 4]>,
    pub message: String,
    pub value: f64,
    pub limit: f64,
    pub fix: Option<Value>,
}

impl Finding {
    pub fn to_json(&self) -> Value {
        let severity = match self.severity {
            Severity::Error => "error",
            Severity::Warning => "warning",
            Severity::Info => "info",
        };
        let position = self
            .bbox
            .map(|b| [round2((b[0] + b[2]) / 2.0), round2((b[1] + b[3]) / 2.0)]);
        json!({
            "rule": self.rule, "severity": severity, "ids": self.ids, "bbox": self.bbox,
            "position": position, "message": self.message, "value": round2(self.value),
            "limit": self.limit, "fix": self.fix,
        })
    }
}

fn fix(command: &str, params: Value) -> Option<Value> {
    Some(json!({ "command": command, "params": params }))
}

fn bbox_of(points: impl IntoIterator<Item = Vec2>) -> Option<[f64; 4]> {
    let mut it = points.into_iter();
    let first = it.next()?;
    let b = it.fold([first.x, first.y, first.x, first.y], |b, p| {
        [b[0].min(p.x), b[1].min(p.y), b[2].max(p.x), b[3].max(p.y)]
    });
    Some(b.map(|v| round2(v / MM)))
}

fn is_satin(o: &DesignObject) -> bool {
    o.region
        .as_ref()
        .is_some_and(|r| r.kind == RegionType::Satin)
        || o.line.as_ref().is_some_and(|l| l.kind == LineType::Zigzag)
}

fn is_feed(o: &DesignObject) -> bool {
    o.stitch_block.as_ref().is_some_and(|b| {
        !b.stitches.is_empty() && b.stitches.iter().all(|s| s.command == Command::Jump)
    })
}

/// Whether the object is meant to sew: a region or line, or fixed stitches.
fn sews(o: &DesignObject) -> bool {
    o.region.is_some() || o.line.is_some() || o.stitch_block.is_some()
}

/// Average width of the closed outline, mm: area ÷ half the perimeter (contours inside
/// another's bounds count as holes).
fn average_width(o: &DesignObject) -> Option<f64> {
    let paths: Vec<Vec<Vec2>> = o
        .contours
        .iter()
        .filter(|c| c.closed)
        .map(Contour::path)
        .collect();
    let boxes: Vec<_> = paths.iter().map(|p| bbox_of(p.iter().copied())).collect();
    let (mut area, mut perimeter) = (0.0, 0.0);
    for (i, p) in paths.iter().enumerate() {
        let a = p
            .windows(2)
            .map(|w| w[0].x * w[1].y - w[1].x * w[0].y)
            .sum::<f64>()
            .abs()
            / 2.0;
        let inside = boxes.iter().enumerate().any(|(j, b)| {
            j != i && matches!((b, boxes[i]), (Some(b), Some(c)) if b[0] <= c[0] && b[1] <= c[1] && b[2] >= c[2] && b[3] >= c[3] && b != &c)
        });
        area += if inside { -a } else { a };
        perimeter += p.windows(2).map(|w| w[0].distance(w[1])).sum::<f64>();
    }
    (perimeter > 0.0).then(|| (2.0 * area / perimeter / MM).max(0.0))
}

/// First point where a closed outline crosses itself (non-adjacent segments only).
fn self_crossing(c: &Contour) -> Option<Vec2> {
    let p = c.path();
    let n = p.len().saturating_sub(1);
    if !c.closed || !(4..=4000).contains(&n) {
        return None;
    }
    for i in 0..n {
        for j in i + 2..n {
            if i == 0 && j == n - 1 {
                continue;
            }
            let (a, b, q, r) = (p[i], p[i + 1], p[j], p[j + 1]);
            let d = (b.x - a.x) * (r.y - q.y) - (b.y - a.y) * (r.x - q.x);
            if d.abs() < 1e-9 {
                continue;
            }
            let t = ((q.x - a.x) * (r.y - q.y) - (q.y - a.y) * (r.x - q.x)) / d;
            let u = ((q.x - a.x) * (b.y - a.y) - (q.y - a.y) * (b.x - a.x)) / d;
            if t > 1e-6 && t < 1.0 - 1e-6 && u > 1e-6 && u < 1.0 - 1e-6 {
                return Some(v2(a.x + (b.x - a.x) * t, a.y + (b.y - a.y) * t));
            }
        }
    }
    None
}

pub struct CheckOptions {
    pub fabric: (f64, f64),
}

/// Every finding, errors first.
pub fn check(d: &Design, t: &Traced, opts: &CheckOptions) -> Vec<Finding> {
    let mut out = Vec::new();
    let st = &t.pattern.stitches;
    let objects: HashMap<u64, &DesignObject> = d.objects.iter().map(|o| (o.id, o)).collect();
    let all: Vec<u64> = t.spans.iter().map(|(id, _)| *id).collect();
    let drop_points = |r: &Range<usize>| {
        st[r.clone()]
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .map(|s| v2(s.x, s.y))
    };

    // Outside the hoop: stitches past the Design Page.
    let (pw, ph) = (d.page.width, d.page.height);
    let tol = 0.5;
    let mut outside = Vec::new();
    let mut overflow: f64 = 0.0;
    for (id, r) in &t.spans {
        for p in drop_points(r).chain(
            st[r.clone()]
                .iter()
                .filter(|s| s.command == Command::Jump)
                .map(|s| v2(s.x, s.y)),
        ) {
            let o = (-p.x).max(-p.y).max(p.x - pw).max(p.y - ph);
            if o > tol {
                overflow = overflow.max(o);
                if outside.last() != Some(id) {
                    outside.push(*id);
                }
            }
        }
    }
    if !outside.is_empty() {
        let b = t.pattern.bounds();
        let fits = b.width() <= pw && b.height() <= ph;
        let bbox = bbox_of(outside.iter().flat_map(|id| {
            t.spans
                .iter()
                .filter(move |(i, _)| i == id)
                .flat_map(|(_, r)| drop_points(r))
        }));
        out.push(Finding {
            rule: "outsideHoop",
            severity: Severity::Error,
            ids: outside,
            bbox,
            message: format!(
                "Stitches lie up to {:.1} mm outside the {:.0} × {:.0} mm hoop{}",
                overflow / MM,
                pw / MM,
                ph / MM,
                if fits {
                    ""
                } else {
                    ", and the design is larger than the hoop"
                }
            ),
            value: overflow / MM,
            limit: 0.0,
            fix: if fits {
                fix("object.center", json!({ "ids": all }))
            } else {
                None
            },
        });
    }

    // Per object: long stitches, satin width, thin fills, small text, tiny objects, crossings.
    for (id, r) in &t.spans {
        let Some(o) = objects.get(id) else { continue };
        let size = o.bounds().map(|b| b.width().max(b.height()) / MM);
        let empty = drop_points(r).next().is_none();
        if sews(o) && !is_feed(o) && (empty || size.is_some_and(|s| s < TINY_OBJECT_MM)) {
            out.push(Finding {
                rule: "tinyObject",
                severity: Severity::Warning,
                ids: vec![*id],
                bbox: o
                    .bounds()
                    .map(|b| [b.min_x, b.min_y, b.max_x, b.max_y].map(|v| round2(v / MM))),
                message: if empty {
                    "The object sews no stitches".into()
                } else {
                    format!(
                        "The object is {:.1} mm across: too small to sew",
                        size.unwrap_or(0.0)
                    )
                },
                value: size.unwrap_or(0.0),
                limit: TINY_OBJECT_MM,
                fix: fix("design.cleanup", json!({})),
            });
            // Its other measures mean nothing at this size.
            continue;
        }
        let mut longest = (0.0, Vec2::default());
        let mut prev: Option<(Command, Vec2)> = None;
        for s in &st[r.clone()] {
            let p = v2(s.x, s.y);
            if s.command == Command::Stitch
                && let Some((Command::Stitch, q)) = prev
                && q.distance(p) > longest.0
            {
                longest = (q.distance(p), p);
            }
            prev = Some((s.command, p));
        }
        let long_mm = longest.0 / MM;
        let at = bbox_of([longest.1]);
        let region = o.region.is_some();
        if is_satin(o) && long_mm > SATIN_WIDE_MM {
            out.push(Finding {
                rule: "satinTooWide",
                severity: if long_mm > MAX_STITCH_MM { Severity::Error } else { Severity::Warning },
                ids: vec![*id],
                bbox: at,
                message: format!("Satin stitches up to {long_mm:.1} mm long: loose and easily snagged above {SATIN_WIDE_MM} mm"),
                value: long_mm,
                limit: SATIN_WIDE_MM,
                fix: region.then(|| fix("object.setRegion", json!({ "ids": [id], "splitLength": 7 }))).flatten(),
            });
        } else if long_mm > MAX_STITCH_MM {
            out.push(Finding {
                rule: "longStitch",
                severity: Severity::Warning,
                ids: vec![*id],
                bbox: at,
                message: format!("A {long_mm:.1} mm stitch: loose and easily snagged; machine files split stitches over {MAX_STITCH_MM} mm into jumps"),
                value: long_mm,
                limit: MAX_STITCH_MM,
                fix: None,
            });
        }
        let text = o.kind == ObjectKind::Text;
        let narrow = match (&o.line, &o.region) {
            (Some(l), _) if l.kind == LineType::Zigzag => Some((
                l.width,
                fix("object.setLine", json!({ "ids": [id], "type": "running" })),
            )),
            (_, Some(g)) if g.kind == RegionType::Satin && !text => {
                average_width(o).map(|w| (w, fix("object.regionToLine", json!({ "ids": [id] }))))
            }
            _ => None,
        };
        if let Some((w, f)) = narrow.filter(|(w, _)| *w < SATIN_NARROW_MM) {
            out.push(Finding {
                rule: "satinTooNarrow",
                severity: Severity::Warning,
                ids: vec![*id],
                bbox: bbox_of(drop_points(r)),
                message: format!("A {w:.1} mm satin sews as a lump; under {SATIN_NARROW_MM} mm use a running or triple stitch"),
                value: w,
                limit: SATIN_NARROW_MM,
                fix: f,
            });
        }
        let fill = o.region.as_ref().is_some_and(|g| {
            matches!(
                g.kind,
                RegionType::Fill
                    | RegionType::ProgrammableFill
                    | RegionType::MotifFill
                    | RegionType::DecorativeFill
                    | RegionType::Stippling
            )
        });
        if fill
            && !text
            && let Some(w) = average_width(o).filter(|w| *w < FILL_THIN_MM)
        {
            out.push(Finding {
                rule: "fillTooThin",
                severity: Severity::Warning,
                ids: vec![*id],
                bbox: bbox_of(drop_points(r)),
                message: format!(
                    "The fill is {w:.1} mm wide on average: too thin to sew as a fill"
                ),
                value: w,
                limit: FILL_THIN_MM,
                fix: fix("object.setRegion", json!({ "ids": [id], "type": "satin" })),
            });
        }
        if let Some(spec) = o.text.as_ref().filter(|s| text && s.size < TEXT_MIN_MM) {
            out.push(Finding {
                rule: "textTooSmall",
                severity: Severity::Warning,
                ids: vec![*id],
                bbox: bbox_of(drop_points(r)),
                message: format!(
                    "{:.1} mm text: letters under {TEXT_MIN_MM} mm close up and lose detail",
                    spec.size
                ),
                value: spec.size,
                limit: TEXT_MIN_MM,
                fix: fix(
                    "object.setText",
                    json!({ "ids": [id], "size": TEXT_MIN_MM }),
                ),
            });
        }
        if o.kind == ObjectKind::Shape
            && sews(o)
            && let Some(p) = o.contours.iter().find_map(self_crossing)
        {
            out.push(Finding {
                rule: "selfCrossing",
                severity: Severity::Error,
                ids: vec![*id],
                bbox: bbox_of([p]),
                message: "The outline crosses itself, so the fill or satin cannot follow it".into(),
                value: 1.0,
                limit: 0.0,
                fix: None,
            });
        }
    }

    // Untrimmed jumps.
    let limit = (Compiler::for_design(d)
        .trim_distance
        .max(Compiler::default().sew_across_distance))
        / MM;
    for m in moves(t) {
        let feed = [m.from_id, m.to_id]
            .iter()
            .flatten()
            .any(|id| objects.get(id).is_some_and(|o| is_feed(o)));
        if m.jumped && !m.trimmed && !feed && m.length / MM > limit {
            out.push(Finding {
                rule: "untrimmedJump",
                severity: Severity::Warning,
                ids: [m.from_id, m.to_id]
                    .into_iter()
                    .flatten()
                    .collect::<BTreeSet<_>>()
                    .into_iter()
                    .collect(),
                bbox: bbox_of([m.from, m.to]),
                message: format!(
                    "A {:.1} mm jump is not trimmed: the thread lies across the design",
                    m.length / MM
                ),
                value: m.length / MM,
                limit,
                fix: m.to_id.and_then(|id| {
                    fix(
                        "object.setJumpTrimming",
                        json!({ "ids": [id], "enabled": true }),
                    )
                }),
            });
        }
    }

    // Stacked drops and dense thread (sew_check's limits), then thread layers per hotspot.
    let mut spots: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut dense: HashMap<(i64, i64), f64> = HashMap::new();
    let mut run = (0usize, 0usize, 0usize);
    for (i, s) in st.iter().enumerate() {
        if s.command != Command::Stitch {
            run.0 = 0;
            continue;
        }
        let to = v2(s.x, s.y);
        let from = i.checked_sub(1).map_or(to, |j| v2(st[j].x, st[j].y));
        spots.entry(cell(to, STACK_CELL_MM)).or_default().push(i);
        let mid = v2((from.x + to.x) / 2.0, (from.y + to.y) / 2.0);
        *dense.entry(cell(mid, DENSE_CELL_MM)).or_default() += from.distance(to) / MM;
        if i > 0 && from.distance(to) < TINY_MM * MM {
            run.0 += 1;
            if run.0 > run.1 {
                run = (run.0, run.0, i);
            }
        } else {
            run.0 = 0;
        }
    }
    let ids_at = |idx: &[usize]| {
        idx.iter()
            .filter_map(|&i| t.owner(i))
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>()
    };
    if let Some((c, idx)) = spots
        .iter()
        .max_by_key(|(c, v)| (v.len(), std::cmp::Reverse(**c)))
        .filter(|(_, v)| v.len() > MAX_PER_CELL)
    {
        out.push(Finding {
            rule: "stacked",
            severity: Severity::Error,
            ids: ids_at(idx),
            bbox: Some([
                c.0 as f64 * STACK_CELL_MM,
                c.1 as f64 * STACK_CELL_MM,
                (c.0 + 1) as f64 * STACK_CELL_MM,
                (c.1 + 1) as f64 * STACK_CELL_MM,
            ]),
            message: format!(
                "{} needle drops in one {STACK_CELL_MM} mm spot: the needle can break",
                idx.len()
            ),
            value: idx.len() as f64,
            limit: MAX_PER_CELL as f64,
            fix: None,
        });
    }
    if let Some((c, v)) = dense
        .iter()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .map(|(c, v)| (*c, *v / (DENSE_CELL_MM * DENSE_CELL_MM)))
        .filter(|(_, v)| *v > MAX_THREAD_PER_MM2)
    {
        let b =
            [c.0 as f64, c.1 as f64, (c.0 + 1) as f64, (c.1 + 1) as f64].map(|v| v * DENSE_CELL_MM);
        out.push(Finding {
            rule: "dense",
            severity: Severity::Error,
            ids: Vec::new(),
            bbox: Some(b),
            message: format!("{v:.0} mm of thread per mm² in a {DENSE_CELL_MM} mm cell: the fabric puckers or tears"),
            value: v,
            limit: MAX_THREAD_PER_MM2,
            fix: None,
        });
    }
    if run.1 > MAX_TINY_RUN {
        out.push(Finding {
            rule: "inPlace",
            severity: Severity::Error,
            ids: t.owner(run.2).into_iter().collect(),
            bbox: bbox_of([v2(st[run.2].x, st[run.2].y)]),
            message: format!(
                "{} stitches in a row shorter than {TINY_MM} mm: thread knots in one place",
                run.1
            ),
            value: run.1 as f64,
            limit: MAX_TINY_RUN as f64,
            fix: None,
        });
    }
    let map = DensityMap::new(t, 1.0, None);
    for (cells, ids) in map.hotspots(opts.fabric) {
        // An object's own dense corners are left out: only overlaps of two or more objects.
        if ids.len() < 2 || cells.iter().all(|&k| map.stacked[k]) {
            continue;
        }
        let peak = cells.iter().map(|&k| map.layers[k]).fold(0.0, f64::max);
        out.push(Finding {
            rule: "stacked",
            severity: Severity::Warning,
            ids: ids.iter().copied().collect(),
            bbox: Some(map.cells_bbox(&cells)),
            message: format!(
                "{peak:.1} layers of thread where {} objects overlap",
                ids.len()
            ),
            value: peak,
            limit: opts.fabric.1,
            fix: fix("shape.removeOverlap", json!({ "ids": ids })),
        });
    }
    out.sort_by_key(|f| f.severity);
    out
}

// MARK: Review Sewing Order (#505)

/// The move from one object's last needle drop to the next object's first.
pub struct Move {
    pub from_id: Option<u64>,
    pub to_id: Option<u64>,
    pub from: Vec2,
    pub to: Vec2,
    pub length: f64,
    pub jumped: bool,
    pub trimmed: bool,
    pub colour_change: bool,
    pub stop: bool,
}

/// Every move between needle drops that is not a stitch, in sewing order.
pub fn moves(t: &Traced) -> Vec<Move> {
    let st = &t.pattern.stitches;
    let mut out = Vec::new();
    let mut last: Option<usize> = None;
    let (mut jumped, mut trimmed, mut colour, mut stop) = (false, false, false, false);
    for (i, s) in st.iter().enumerate() {
        match s.command {
            Command::Stitch => {
                if let Some(j) = last
                    && (jumped || trimmed || colour || stop)
                {
                    let (from, to) = (v2(st[j].x, st[j].y), v2(s.x, s.y));
                    out.push(Move {
                        from_id: t.owner(j),
                        to_id: t.owner(i),
                        from,
                        to,
                        length: from.distance(to),
                        jumped,
                        trimmed,
                        colour_change: colour,
                        stop,
                    });
                }
                last = Some(i);
                (jumped, trimmed, colour, stop) = (false, false, false, false);
            }
            Command::Jump => jumped = true,
            Command::Trim => trimmed = true,
            Command::ColorChange => colour = true,
            Command::Stop => stop = true,
            Command::End => {}
        }
    }
    out
}

pub fn route(d: &Design, t: &Traced) -> Value {
    let st = &t.pattern.stitches;
    let threads = &t.pattern.threads;
    let trim_limit = Compiler::for_design(d)
        .trim_distance
        .max(Compiler::default().sew_across_distance)
        / MM;
    let sewn: Vec<_> = t
        .spans
        .iter()
        .filter_map(|(id, r)| {
            let first = r.clone().find(|&i| st[i].command == Command::Stitch)?;
            let last = r
                .clone()
                .rev()
                .find(|&i| st[i].command == Command::Stitch)?;
            let colour = st[..=first]
                .iter()
                .filter(|s| s.command == Command::ColorChange)
                .count();
            Some((*id, first, last, colour))
        })
        .collect();
    let mut steps = Vec::new();
    for (k, &(id, first, last, colour)) in sewn.iter().enumerate() {
        let o = d.objects.iter().find(|o| o.id == id);
        let (end, start) = (v2(st[last].x, st[last].y), v2(st[first].x, st[first].y));
        let mut warnings = Vec::new();
        let mv = if let Some(&(next, nf, _, _)) = sewn.get(k + 1) {
            let between = &st[last + 1..nf];
            let has = |c| between.iter().any(|s| s.command == c);
            let to = v2(st[nf].x, st[nf].y);
            let len = end.distance(to) / MM;
            let (colour, trim, jump, stop) = (
                has(Command::ColorChange),
                has(Command::Trim),
                has(Command::Jump),
                has(Command::Stop),
            );
            let kind = if colour {
                "colorChange"
            } else if stop {
                "stop"
            } else if trim {
                "trim"
            } else if jump {
                "jump"
            } else {
                "stitch"
            };
            let feed = [id, next]
                .iter()
                .any(|i| d.objects.iter().any(|o| o.id == *i && is_feed(o)));
            if jump && !trim && !colour && !feed && len > trim_limit {
                warnings.push(json!({ "rule": "untrimmedJump", "valueMm": round2(len), "limitMm": trim_limit,
                    "fix": fix("object.setJumpTrimming", json!({ "ids": [next], "enabled": true })) }));
            } else if !colour && trim && len >= SAME_THREAD_MOVE_MM {
                warnings.push(json!({ "rule": "longSameThreadMove", "valueMm": round2(len), "limitMm": SAME_THREAD_MOVE_MM,
                    "fix": fix("order.optimize", json!({ "mode": "minJump" })) }));
            }
            let label = match kind {
                "colorChange" => format!("Trim and thread change, then {len:.1} mm"),
                "stop" => format!("Stop, then {len:.1} mm"),
                "trim" => format!("Trim, then {len:.1} mm"),
                "jump" => format!("Jump of {len:.1} mm, not trimmed"),
                _ => format!("Sewn across, {len:.1} mm"),
            };
            json!({ "kind": kind, "lengthMm": round2(len), "trimmed": trim || colour, "to": next, "label": label })
        } else {
            json!({ "kind": "none", "lengthMm": 0.0, "trimmed": true, "label": "End" })
        };
        steps.push(json!({
            "step": k + 1, "id": id, "name": o.and_then(|o| o.name.clone()),
            "color": threads.get(colour).map(|c| format!("#{:02x}{:02x}{:02x}", c.red, c.green, c.blue)),
            "start": mm(start), "end": mm(end), "move": mv, "warnings": warnings,
            // Needle drops sewn when the step ends (the Stitch Simulator's position).
            "drops": st[..=last].iter().filter(|s| s.command == Command::Stitch).count(),
        }));
    }
    let drops = t.pattern.count(Command::Stitch);
    let trims = t.pattern.count(Command::Trim);
    let travel: f64 = moves(t).iter().map(|m| m.length).sum::<f64>() / MM;
    json!({
        "steps": steps,
        "totals": {
            "trims": trims,
            "colorChanges": t.pattern.count(Command::ColorChange),
            "stops": t.pattern.count(Command::Stop),
            "jumps": t.pattern.count(Command::Jump),
            "travelMm": round2(travel),
            "trimsPer1000": if drops == 0 { 0.0 } else { round2(trims as f64 * 1000.0 / drops as f64) },
            "warnings": steps.iter().map(|s| s["warnings"].as_array().map_or(0, Vec::len)).sum::<usize>(),
        },
    })
}

// MARK: export read-back (#457)

/// Reads `bytes` back with OpenStitch's reader for `ty` and compares it with `written`, the
/// pattern given to the writer: needle drops in order (after the writer's shift of the whole
/// design, to within 0.15 mm), colour changes, trims and stops. Points the writer inserts to
/// keep long moves inside the format's limit, and commands a format cannot store, are
/// expected differences.
#[cfg(feature = "formats")]
pub fn verify(ty: openstitch_formats::FileType, written: &StitchPattern, bytes: &[u8]) -> Value {
    let read = match ty.read(bytes) {
        Ok(p) => p,
        Err(e) => {
            return json!({ "verified": false, "problems": [format!("the file cannot be read back: {e}")], "expected": [] });
        }
    };
    let drops = |p: &StitchPattern| -> Vec<Vec2> {
        p.stitches
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .map(|s| v2(s.x, s.y))
            .collect()
    };
    let (a, b) = (drops(written), drops(&read));
    let mut problems = Vec::new();
    let mut expected = Vec::new();
    let mut deviation: f64 = 0.0;
    let (mut matched, mut added) = (0, 0);
    if let Some(&a0) = a.first() {
        // The writer may move the design and put needle points before its first: the shift
        // is the one that lines up most of the first drops.
        let shift = b
            .iter()
            .take(8)
            .enumerate()
            // Whole 0.1 mm units: the file's coordinates are rounded, the shift is not.
            .map(|(k, q)| (k, v2((q.x - a0.x).round(), (q.y - a0.y).round())))
            .max_by_key(|&(k, sh)| (align(&a[..a.len().min(60)], &b, sh).0, std::cmp::Reverse(k)))
            .map_or_else(Vec2::default, |(_, sh)| sh);
        let j;
        (matched, j, deviation) = align(&a, &b, shift);
        added = b.len() - matched.min(b.len());
        let _ = j;
    }
    if matched != a.len() {
        problems.push(format!("{} of {} needle drops were not read back where they were written (largest deviation {:.1} mm)", a.len() - matched, a.len(), deviation / MM));
    }
    if added > 0 {
        expected.push(format!(
            "{added} needle drops added to keep long stitches inside the format's limit"
        ));
    }
    let count = |p: &StitchPattern, c| p.count(c);
    let colours = (written.color_block_count(), read.color_block_count());
    if colours.0 != colours.1 {
        if ty == openstitch_formats::FileType::Exp {
            expected.push(format!(
                "{} colours written, {} read: the format stores no colour changes",
                colours.0, colours.1
            ));
        } else {
            problems.push(format!(
                "{} colours written, {} read back",
                colours.0, colours.1
            ));
        }
    }
    for (c, name) in [(Command::Trim, "trims"), (Command::Stop, "stops")] {
        let (w, r) = (count(written, c), count(&read, c));
        if w != r {
            expected.push(format!(
                "{w} {name} written, {r} read back: the format stores them differently"
            ));
        }
    }
    json!({
        "verified": problems.is_empty(),
        "format": ty.extension(),
        "stitches": [a.len(), b.len()],
        "colors": [colours.0, colours.1],
        "maxDeviationMm": round2(deviation / MM),
        "problems": problems,
        "expected": expected,
    })
}

/// Matches `a`'s drops, moved by `shift`, in order against `b`'s, skipping points the writer
/// inserted: (drops matched, where the match stopped in `b`, largest deviation in 0.1 mm).
#[cfg(feature = "formats")]
fn align(a: &[Vec2], b: &[Vec2], shift: Vec2) -> (usize, usize, f64) {
    let (mut matched, mut j, mut deviation) = (0, 0, 0.0f64);
    for p in a {
        let want = v2(p.x + shift.x, p.y + shift.y);
        // Inserted points are skipped; a drop must turn up within the next few.
        match (j..b.len().min(j + 64)).find(|&k| b[k].distance(want) <= 1.0) {
            Some(k) => {
                deviation = deviation.max(b[k].distance(want));
                matched += 1;
                j = k + 1;
            }
            None => deviation = deviation.max(b.get(j).map_or(f64::INFINITY, |q| q.distance(want))),
        }
    }
    (matched, j, deviation)
}

/// [`verify`] on the file as it is on disk: a short write (a full USB stick) or a file that
/// differs from the bytes encoded is a problem before any stitch is compared.
#[cfg(feature = "formats")]
pub fn verify_file(
    ty: openstitch_formats::FileType,
    written: &StitchPattern,
    path: &std::path::Path,
    bytes: &[u8],
) -> Value {
    let mut r = match std::fs::read(path) {
        Ok(disk) if disk == bytes => verify(ty, written, &disk),
        Ok(disk) => {
            json!({ "verified": false, "problems": [format!("{} bytes on disk, {} written", disk.len(), bytes.len())], "expected": [] })
        }
        Err(e) => {
            json!({ "verified": false, "problems": [format!("the file cannot be opened again: {e}")], "expected": [] })
        }
    };
    r["path"] = json!(path.display().to_string());
    r
}

// MARK: commands

fn only(p: &Params) -> Result<Option<BTreeSet<u64>>, CommandError> {
    Ok(if p.has("ids") || p.has("id") {
        Some(p.ids()?.into_iter().collect())
    } else {
        None
    })
}

fn limits(p: &Params) -> Result<(f64, f64), CommandError> {
    let id = p.string("fabric", Some("standard"))?;
    fabric_limits(&id)
        .ok_or_else(|| p.invalid("fabric", "not a Fabric Selector id (see fabric.list)"))
}

/// `design.check`'s result for the session's design.
pub fn check_json(
    d: &Design,
    t: &Traced,
    fabric: (f64, f64),
    only: Option<&BTreeSet<u64>>,
    rules: Option<&[String]>,
) -> Value {
    let findings: Vec<Value> = check(d, t, &CheckOptions { fabric })
        .iter()
        .filter(|f| only.is_none_or(|o| f.ids.iter().any(|id| o.contains(id))))
        .filter(|f| rules.is_none_or(|r| r.iter().any(|x| x == f.rule)))
        .map(Finding::to_json)
        .collect();
    let errors = findings.iter().filter(|f| f["severity"] == "error").count();
    let first = findings.first().map(|f| f["message"].clone());
    json!({
        "ready": errors == 0,
        "errors": errors,
        "warnings": findings.len() - errors,
        "summary": if findings.is_empty() { json!("Ready to sew") } else { first.unwrap_or_default() },
        "findings": findings,
    })
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "design.check",
            "Check Design (beyond PE-DESIGN): every likely sewing problem, errors first, with the objects, where and an optional fix command. Rules: outsideHoop, longStitch, satinTooWide, satinTooNarrow, fillTooThin, textTooSmall, tinyObject, selfCrossing, untrimmedJump, stacked, dense, inPlace. `ready` is false while there are errors. Run it before reporting a design done",
            Effect::Read,
            vec![
                opt(
                    "ids",
                    "[int]",
                    "Only findings about these objects (default all)",
                ),
                opt("rules", "[string]", "Only these rules"),
                opt(
                    "fabric",
                    "string",
                    "Fabric Selector id the density limits follow (default standard)",
                ),
            ],
            |s, p| {
                let rules: Option<Vec<String>> = if p.has("rules") {
                    Some(
                        p.array("rules")?
                            .iter()
                            .filter_map(|v| v.as_str().map(String::from))
                            .collect(),
                    )
                } else {
                    None
                };
                Ok(check_json(
                    &s.design,
                    &Traced::new(&s.design),
                    limits(p)?,
                    only(p)?.as_ref(),
                    rules.as_deref(),
                ))
            },
        ),
        spec(
            "design.density",
            "Density map (beyond PE-DESIGN): thread layers (1 = one default tatami fill) or needle holes per mm² on a grid, the share of thin, ok, heavy and too-dense cells for the fabric, and the hotspots with the objects that cause them",
            Effect::Read,
            vec![
                opt("cell", "mm", "Grid cell (0.5–5, default 1)"),
                opt("ids", "[int]", "Only these objects' stitches"),
                opt("metric", "layers|holes", "Values returned (default layers)"),
                opt(
                    "fabric",
                    "string",
                    "Fabric Selector id the limits follow (default standard)",
                ),
            ],
            |s, p| {
                let cell = p.opt("cell", 1.0, 0.5, 5.0)?;
                let metric = p.string("metric", Some("layers"))?;
                if metric != "layers" && metric != "holes" {
                    return Err(p.invalid("metric", "must be layers or holes"));
                }
                let t = Traced::new(&s.design);
                Ok(DensityMap::new(&t, cell, only(p)?.as_ref()).to_json(&metric, limits(p)?))
            },
        ),
        spec(
            "design.route",
            "Review Sewing Order (beyond PE-DESIGN): each sewn object in order with its start and end and the move to the next (trim, jump, colour change, stop or sewn across, with its length), warnings for untrimmed jumps and long same-thread moves, and totals (trims, colour changes, travel, trims per 1,000 stitches)",
            Effect::Read,
            vec![],
            |s, _| Ok(route(&s.design, &Traced::new(&s.design))),
        ),
        spec(
            "design.cleanup",
            "Delete every object that is meant to sew but sews nothing, or is smaller than minSize; one undo step. Returns the ids removed",
            Effect::Edit,
            vec![opt("minSize", "mm", "Smallest object kept (default 1.5)")],
            |s, p| {
                let min = p.opt("minSize", TINY_OBJECT_MM, 0.0, 50.0)? * MM;
                let t = Traced::new(&s.design);
                let sewing: BTreeSet<u64> = t.drops().filter_map(|(i, _, _)| t.owner(i)).collect();
                let removed: Vec<u64> = s
                    .design
                    .objects
                    .iter()
                    .filter(|o| o.visible && sews(o) && !is_feed(o))
                    .filter(|o| {
                        !sewing.contains(&o.id)
                            || o.bounds().is_some_and(|b| b.width().max(b.height()) < min)
                    })
                    .map(|o| o.id)
                    .collect();
                s.design.objects.retain(|o| !removed.contains(&o.id));
                Ok(json!({ "removed": removed }))
            },
        ),
    ]
}

/// Check Design's errors, for the export's `problems`.
pub(crate) fn errors(d: &Design) -> Vec<String> {
    let t = Traced::new(d);
    check(d, &t, &CheckOptions { fabric: (1.5, 2.5) })
        .into_iter()
        .filter(|f| f.severity == Severity::Error)
        .map(|f| f.message)
        .collect()
}
