//! Embossing, engraving and stamps (PE-DESIGN's "Embossing/Engraving" and "Applying and
//! editing stamps"): effects that change where the needle goes down inside a fill, satin or
//! programmable-fill region, so part of it looks raised or sunken. Independent algorithms on
//! the region's stitches:
//!
//! - engraved line: a penetration wherever a stitch crosses the effect outline, so the outline
//!   shows as a line of needle holes;
//! - engraved region: stitches inside the effect area are cut into short ones (sunken look);
//! - embossed region: needle points inside the effect area are dropped, so stitches there are
//!   long (raised look), up to [`STAMP_EMBOSS_MAX`].
//!
//! Lengths are 0.1 mm.

use crate::generate::Run;
use crate::geometry::{Contour, Rect, Transform, Vec2, contains, v2};
use crate::model::RegionType;
use crate::shapes;
use serde::{Deserialize, Serialize};

/// Stitch length inside an engraved area of a programmable-fill pattern.
pub const ENGRAVE_STEP: f64 = 10.0;
/// Longest stitch inside an embossed area of a programmable-fill pattern.
pub const EMBOSS_MAX: f64 = 70.0;
/// Stitch length inside a stamp's engraved region. PE-DESIGN 11.50 cuts a Fill's 4 mm
/// stitches to 1.9 mm on average there (round-10 capture
/// `471-engraved-region-circle.pes`, spec `stamp-engraved-region-circle`: 262 needle points per cm²
/// inside against 128 outside, #471).
pub const STAMP_ENGRAVE_STEP: f64 = 20.0;
/// Longest stitch inside a stamp's embossed region. PE-DESIGN 11.50 drops every needle point
/// inside and leaves the rows crossing a 14.5 × 14 mm circle whole, up to 14.3 mm (round-10
/// capture `471-embossed-region-circle.pes`: within 6 mm of the centre 13 needle points per cm², the
/// under sewing's, against 126 without the stamp,
/// #471); longer crossings are split.
pub const STAMP_EMBOSS_MAX: f64 = 145.0;

/// Region types that take stamps and embossing.
pub const STAMP_TYPES: [RegionType; 3] = [
    RegionType::Fill,
    RegionType::Satin,
    RegionType::ProgrammableFill,
];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum StampEffect {
    EngravedLine,
    EngravedRegion,
    EmbossedRegion,
}

impl StampEffect {
    pub fn id(self) -> &'static str {
        match self {
            StampEffect::EngravedLine => "engravedLine",
            StampEffect::EngravedRegion => "engravedRegion",
            StampEffect::EmbossedRegion => "embossedRegion",
        }
    }

    pub fn from_id(s: &str) -> Option<StampEffect> {
        [
            StampEffect::EngravedLine,
            StampEffect::EngravedRegion,
            StampEffect::EmbossedRegion,
        ]
        .into_iter()
        .find(|e| e.id() == s)
    }
}

/// One effect area on a region: closed outlines in model coordinates. `pattern` names the
/// stamp it came from (`None`: converted from a shape).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stamp {
    pub effect: StampEffect,
    pub contours: Vec<Contour>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pattern: Option<String>,
}

impl Stamp {
    pub fn bounds(&self) -> Option<Rect> {
        Rect::enclosing(self.contours.iter().flat_map(Contour::outline_points))
    }

    pub fn transformed(&self, t: &Transform) -> Stamp {
        Stamp {
            contours: self.contours.iter().map(|c| c.transformed(t)).collect(),
            ..self.clone()
        }
    }
}

/// Stamp patterns: OpenStitch's own designs, built from the preset shapes.
pub const PATTERNS: [&str; 7] = [
    "circle", "heart", "star", "diamond", "teardrop", "crescent", "cross",
];

/// Stamp `pattern` of `size` (0.1 mm, its larger side) centred at `at`, turned `rotation`
/// degrees.
pub fn stamp(
    pattern: &str,
    effect: StampEffect,
    at: Vec2,
    size: f64,
    rotation: f64,
) -> Option<Stamp> {
    let unit = match pattern {
        "circle" => shapes::arc(v2(0.0, 0.0), 1.0, 1.0, 0.0, 360.0, shapes::ArcMode::Arc),
        "heart" => shapes::heart(),
        "star" => shapes::star(5, 0.45),
        "diamond" => shapes::regular_polygon(4),
        "teardrop" => shapes::teardrop(),
        "crescent" => shapes::crescent(0.9),
        "cross" => shapes::cross(0.35),
        _ => return None,
    };
    let c = shapes::fit(&unit, at, size, size, rotation);
    Some(Stamp {
        effect,
        contours: vec![c],
        pattern: Some(pattern.to_string()),
    })
}

/// Straight edges of the closed contours.
fn edges(contours: &[Contour]) -> Vec<(Vec2, Vec2)> {
    contours
        .iter()
        .filter(|c| c.closed)
        .flat_map(Contour::edges)
        .collect()
}

/// Parameters (0–1, exclusive) where segment a→b crosses `edges`, sorted.
fn crossings(a: Vec2, b: Vec2, edges: &[(Vec2, Vec2)]) -> Vec<f64> {
    let d = b - a;
    let mut ts: Vec<f64> = edges
        .iter()
        .filter_map(|&(p, q)| {
            let e = q - p;
            let den = d.x * e.y - d.y * e.x;
            if den.abs() < 1e-12 {
                return None;
            }
            let w = p - a;
            let t = (w.x * e.y - w.y * e.x) / den;
            let u = (w.x * d.y - w.y * d.x) / den;
            (t > 1e-9 && t < 1.0 - 1e-9 && (0.0..=1.0).contains(&u)).then_some(t)
        })
        .collect();
    ts.sort_by(f64::total_cmp);
    ts.dedup_by(|x, y| (*x - *y).abs() < 1e-9);
    ts
}

/// Points along a→b (excluding a, including b) with crossings of `edges` inserted, each piece
/// tagged with whether its midpoint is inside `area`.
fn pieces(a: Vec2, b: Vec2, edges: &[(Vec2, Vec2)], area: &[Contour]) -> Vec<(Vec2, bool)> {
    let mut ts = crossings(a, b, edges);
    ts.push(1.0);
    let mut out = Vec::new();
    let mut from = 0.0;
    for t in ts {
        let mid = a.lerp(b, (from + t) / 2.0);
        out.push((a.lerp(b, t), contains(mid, area)));
        from = t;
    }
    out
}

fn apply_one(runs: Vec<Run>, s: &Stamp) -> Vec<Run> {
    let e = edges(&s.contours);
    if e.is_empty() {
        return runs;
    }
    let area = &s.contours;
    runs.into_iter()
        .map(|run| {
            let Some(&first) = run.first() else {
                return run;
            };
            let mut out = vec![first];
            // Whether the piece ending at each output point lies inside the area.
            let mut inside_before = vec![false];
            for w in run.windows(2) {
                let (a, b) = (w[0], w[1]);
                let mut from = a;
                for (p, inside) in pieces(a, b, &e, area) {
                    match s.effect {
                        StampEffect::EngravedRegion if inside => {
                            let n =
                                (from.distance(p) / STAMP_ENGRAVE_STEP).ceil().max(1.0) as usize;
                            out.extend((1..=n).map(|i| from.lerp(p, i as f64 / n as f64)));
                        }
                        _ => {
                            out.push(p);
                            inside_before.push(inside);
                        }
                    }
                    from = p;
                }
            }
            if s.effect == StampEffect::EmbossedRegion {
                out = emboss(&out, &inside_before);
            }
            out
        })
        .collect()
}

/// Drops needle points with the area on both sides (run ends and outline crossings stay), then
/// splits stitches longer than [`STAMP_EMBOSS_MAX`]: whole stitches of that length from where the
/// row enters, the rest last, as PE-DESIGN 11.50 sews them (round-10 capture: across a 14.5 mm
/// circle its embossed stitches are 12–14.3 mm with a few short ends, none near half the chord).
fn emboss(run: &[Vec2], inside_before: &[bool]) -> Run {
    let n = run.len();
    let kept: Vec<Vec2> = (0..n)
        .filter(|&i| i == 0 || i + 1 == n || !(inside_before[i] && inside_before[i + 1]))
        .map(|i| run[i])
        .collect();
    let mut out = vec![kept[0]];
    for w in kept.windows(2) {
        let (a, b) = (w[0], w[1]);
        let len = a.distance(b);
        if len > STAMP_EMBOSS_MAX {
            // Whole stitches of STAMP_EMBOSS_MAX, then the rest; a rest under 1 mm joins the last
            // whole stitch instead (at most 1 mm longer) so no needle point lands next to the
            // outline crossing.
            let k = (len / STAMP_EMBOSS_MAX).floor() as usize;
            let rest = len - k as f64 * STAMP_EMBOSS_MAX;
            let last = if rest < 10.0 { k - 1 } else { k };
            out.extend((1..=last).map(|i| a.lerp(b, i as f64 * STAMP_EMBOSS_MAX / len)));
        }
        out.push(b);
    }
    out
}

/// The region's top-layer `runs` with every stamp applied in order.
pub fn apply(mut runs: Vec<Run>, stamps: &[Stamp]) -> Vec<Run> {
    for s in stamps {
        runs = apply_one(runs, s);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows() -> Vec<Run> {
        // Three horizontal rows across 0..400 with a needle point every 40.
        (0..3)
            .map(|r| {
                let y = 150.0 + 17.0 * f64::from(r);
                (0..=10).map(|k| v2(40.0 * f64::from(k), y)).collect()
            })
            .collect()
    }

    fn square_stamp(effect: StampEffect) -> Stamp {
        Stamp {
            effect,
            contours: vec![Contour::rectangle(105.0, 100.0, 190.0, 200.0)],
            pattern: None,
        }
    }

    fn inside(p: &Vec2) -> bool {
        p.x > 105.0 && p.x < 295.0
    }

    #[test]
    fn engraved_line_adds_penetrations_on_the_outline() {
        let out = apply(rows(), &[square_stamp(StampEffect::EngravedLine)]);
        for run in &out {
            assert!(run.contains(&v2(105.0, run[0].y)));
            assert!(run.contains(&v2(295.0, run[0].y)));
            assert_eq!(run.len(), 13);
        }
    }

    #[test]
    fn engraved_region_shortens_stitches_inside() {
        let out = apply(rows(), &[square_stamp(StampEffect::EngravedRegion)]);
        for run in &out {
            for w in run.windows(2) {
                let mid = w[0].lerp(w[1], 0.5);
                if inside(&mid) {
                    assert!(w[0].distance(w[1]) <= STAMP_ENGRAVE_STEP + 1e-9);
                } else {
                    assert!(w[0].distance(w[1]) <= 40.0 + 1e-9);
                }
            }
            // 19 mm inside in 2 mm stitches: 10 where the plain row has 5.
            assert!(run.len() > 15, "{}", run.len());
        }
    }

    #[test]
    fn embossed_region_drops_inside_points() {
        let out = apply(rows(), &[square_stamp(StampEffect::EmbossedRegion)]);
        for run in &out {
            assert!(
                run.windows(2)
                    .all(|w| w[0].distance(w[1]) <= STAMP_EMBOSS_MAX + 10.0 + 1e-9)
            );
            // The 19 mm inside is one whole 14.5 mm stitch from where the row enters and the
            // 4.5 mm rest, as PE-DESIGN sews it: one needle point where the plain row has five.
            let inner: Vec<f64> = run.iter().filter(|p| inside(p)).map(|p| p.x).collect();
            assert_eq!(inner.len(), 1, "{inner:?}");
            assert!(
                (inner[0] - (105.0 + STAMP_EMBOSS_MAX)).abs() < 1e-9,
                "{inner:?}"
            );
            assert!(run.contains(&v2(105.0, run[0].y)) && run.contains(&v2(295.0, run[0].y)));
            assert_eq!(run.first(), Some(&v2(0.0, run[0].y)));
            assert_eq!(run.last(), Some(&v2(400.0, run[0].y)));
        }
    }

    #[test]
    fn stamps_from_patterns() {
        for p in PATTERNS {
            let s = stamp(
                p,
                StampEffect::EngravedRegion,
                v2(500.0, 500.0),
                100.0,
                30.0,
            )
            .expect("pattern");
            let b = s.bounds().expect("b");
            assert!(b.center().distance(v2(500.0, 500.0)) < 40.0, "{p}");
            assert!(b.width() <= 150.0 && b.height() <= 150.0, "{p}");
        }
        assert!(stamp("dragon", StampEffect::EngravedLine, v2(0.0, 0.0), 10.0, 0.0).is_none());
        assert_eq!(
            StampEffect::from_id("embossedRegion"),
            Some(StampEffect::EmbossedRegion)
        );
        assert!(StampEffect::from_id("x").is_none());
        // No stamps: unchanged.
        assert_eq!(apply(rows(), &[]), rows());
    }
}
