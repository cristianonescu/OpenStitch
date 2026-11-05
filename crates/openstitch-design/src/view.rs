//! View filters (#517, beyond PE-DESIGN): show one aspect of the stitching at a time without
//! changing the design or its export. They turn the compiled pattern into a pattern only for
//! drawing: stitches left out become moves, colours are replaced. The editor's canvas and the
//! MCP `preview` share them.

use crate::compile::Compiler;
use crate::geometry::{Vec2, v2};
use crate::model::Design;
use openstitch_core::{Command, Stitch, StitchPattern, Thread};
use std::collections::HashMap;
use std::ops::Range;

/// Underlay and top stitching.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Layers {
    #[default]
    Both,
    UnderlayOnly,
    TopOnly,
}

impl Layers {
    pub fn as_str(self) -> &'static str {
        match self {
            Layers::Both => "both",
            Layers::UnderlayOnly => "underlay",
            Layers::TopOnly => "top",
        }
    }

    pub fn parse(s: &str) -> Option<Layers> {
        [Layers::Both, Layers::UnderlayOnly, Layers::TopOnly]
            .into_iter()
            .find(|l| l.as_str() == s)
    }
}

/// What colours the stitches.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorBy {
    #[default]
    Thread,
    /// A fixed gradient from the first stitch to the last.
    SewingOrder,
    /// One colour, to judge texture and density.
    Monochrome,
}

impl ColorBy {
    pub fn as_str(self) -> &'static str {
        match self {
            ColorBy::Thread => "thread",
            ColorBy::SewingOrder => "order",
            ColorBy::Monochrome => "mono",
        }
    }

    pub fn parse(s: &str) -> Option<ColorBy> {
        [ColorBy::Thread, ColorBy::SewingOrder, ColorBy::Monochrome]
            .into_iter()
            .find(|c| c.as_str() == s)
    }
}

/// The view filters; the default changes nothing.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ViewFilter {
    pub layers: Layers,
    /// Hide unselected: only these objects' stitches.
    pub only: Option<Vec<u64>>,
    pub color_by: ColorBy,
    /// Solo colour: every colour block but this one (index in sewing order) faded.
    pub solo: Option<usize>,
}

impl ViewFilter {
    pub fn is_identity(&self) -> bool {
        *self == ViewFilter::default()
    }
}

/// Gradient steps for colour by sewing order.
pub const ORDER_STEPS: usize = 16;
/// Monochrome thread colour.
pub const MONO_RGB: u32 = 0x37474F;
/// Faded blocks keep this much of their colour over the page (≤ 20 % opacity).
pub const FADE: f64 = 0.2;

/// Per record of `pattern` (as compiled from `design`, `spans` its per-object record ranges):
/// whether it is underlay. A record is underlay when the same object compiled without any
/// underlay (every region's and line's underlay off) does not sew that needle point; matched
/// from the end, so a point both sew counts as top stitching.
pub fn underlay_flags(
    design: &Design,
    pattern: &StitchPattern,
    spans: &[(u64, Range<usize>)],
) -> Vec<bool> {
    let mut bare = design.clone();
    for o in &mut bare.objects {
        if let Some(r) = &mut o.region {
            r.underlay = false;
        }
        if let Some(l) = &mut o.line {
            l.underlay = false;
        }
    }
    let (top, top_spans) = Compiler::for_design(&bare).compile_traced(&bare);
    let key = |s: &Stitch| (s.x.round() as i64, s.y.round() as i64);
    let mut flags = vec![false; pattern.stitches.len()];
    for (id, range) in spans {
        let mut left: HashMap<(i64, i64), usize> = HashMap::new();
        if let Some((_, r)) = top_spans.iter().find(|(t, _)| t == id) {
            for s in &top.stitches[r.clone()] {
                if s.command == Command::Stitch {
                    *left.entry(key(s)).or_default() += 1;
                }
            }
        }
        for i in range.clone().rev() {
            let s = &pattern.stitches[i];
            if s.command != Command::Stitch {
                continue;
            }
            match left.get_mut(&key(s)) {
                Some(n) if *n > 0 => *n -= 1,
                _ => flags[i] = true,
            }
        }
    }
    flags
}

/// Where the machine trims: the needle position at each trim of `pattern`.
pub fn trim_points(pattern: &StitchPattern) -> Vec<Vec2> {
    let mut at: Option<Vec2> = None;
    let mut out = Vec::new();
    for s in &pattern.stitches {
        match s.command {
            Command::Stitch | Command::Jump => at = Some(v2(s.x, s.y)),
            Command::Trim => out.extend(at),
            _ => {}
        }
    }
    out
}

fn rgb(c: u32) -> [u8; 3] {
    [(c >> 16) as u8, (c >> 8) as u8, c as u8]
}

/// The gradient colour at `t` (0..1): blue to green to orange to red.
pub fn order_rgb(t: f64) -> [u8; 3] {
    const STOPS: [[f64; 3]; 4] = [
        [0x1e as f64, 0x88 as f64, 0xe5 as f64],
        [0x43 as f64, 0xa0 as f64, 0x47 as f64],
        [0xfb as f64, 0x8c as f64, 0x00 as f64],
        [0xd3 as f64, 0x2f as f64, 0x2f as f64],
    ];
    let x = t.clamp(0.0, 1.0) * 3.0;
    let i = (x.floor() as usize).min(2);
    let f = x - i as f64;
    let mut out = [0u8; 3];
    for c in 0..3 {
        out[c] = (STOPS[i][c] + (STOPS[i + 1][c] - STOPS[i][c]) * f).round() as u8;
    }
    out
}

/// `pattern` as `filter` shows it, over a page of colour `page`. `underlay` is
/// [`underlay_flags`] (needed only for [`Layers`] other than both), `spans` the per-object
/// record ranges (needed for `only`). Stitches left out become moves; a colour change starts
/// each new colour.
pub fn apply(
    pattern: &StitchPattern,
    filter: &ViewFilter,
    underlay: &[bool],
    spans: &[(u64, Range<usize>)],
    page: [u8; 3],
) -> StitchPattern {
    if filter.is_identity() {
        return pattern.clone();
    }
    let mut keep = vec![true; pattern.stitches.len()];
    if let Some(ids) = &filter.only {
        keep.fill(false);
        for (id, r) in spans.iter().filter(|(id, _)| ids.contains(id)) {
            let _ = id;
            for k in &mut keep[r.clone()] {
                *k = true;
            }
        }
    }
    for (i, k) in keep.iter_mut().enumerate() {
        let u = underlay.get(i).copied().unwrap_or(false);
        *k &= match filter.layers {
            Layers::Both => true,
            Layers::UnderlayOnly => u,
            Layers::TopOnly => !u,
        };
    }
    let total = pattern.count(Command::Stitch).max(1);
    let fade = |c: [u8; 3]| -> [u8; 3] {
        let mut o = [0u8; 3];
        for i in 0..3 {
            o[i] =
                (f64::from(page[i]) + (f64::from(c[i]) - f64::from(page[i])) * FADE).round() as u8;
        }
        o
    };
    let colour = |block: usize, n: usize| -> [u8; 3] {
        let base = match filter.color_by {
            ColorBy::Thread => pattern
                .threads
                .get(block)
                .map_or([0x80; 3], |t| [t.red, t.green, t.blue]),
            ColorBy::SewingOrder => {
                let step = (n * ORDER_STEPS / total).min(ORDER_STEPS - 1);
                order_rgb(step as f64 / (ORDER_STEPS - 1) as f64)
            }
            ColorBy::Monochrome => rgb(MONO_RGB),
        };
        match filter.solo {
            Some(b) if b != block => fade(base),
            _ => base,
        }
    };
    let mut out = StitchPattern::new();
    out.metadata = pattern.metadata.clone();
    let (mut block, mut n) = (0, 0);
    let mut current: Option<[u8; 3]> = None;
    for (i, s) in pattern.stitches.iter().enumerate() {
        match s.command {
            Command::ColorChange => {
                block += 1;
                continue;
            }
            Command::Stitch if keep[i] => {
                let c = colour(block, n);
                if current != Some(c) {
                    if current.is_some() {
                        // Start the new colour where the needle is, without a gap.
                        let at = out.stitches.last().map_or(v2(s.x, s.y), |l| v2(l.x, l.y));
                        out.stitches
                            .push(Stitch::new(at.x, at.y, Command::ColorChange));
                    }
                    out.threads.push(Thread::rgb(
                        (u32::from(c[0]) << 16) | (u32::from(c[1]) << 8) | u32::from(c[2]),
                    ));
                    current = Some(c);
                }
                out.stitches.push(*s);
                n += 1;
            }
            Command::Stitch => {
                out.stitches.push(Stitch::new(s.x, s.y, Command::Jump));
                n += 1;
            }
            _ => out.stitches.push(*s),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Contour;
    use crate::model::{DesignObject, DesignPage, RegionSewing, RegionType};

    fn design() -> Design {
        let mut d = Design::new(
            "View",
            DesignPage {
                width: 1000.0,
                height: 1000.0,
            },
        );
        for (x, c) in [(100.0, 0xFF0000), (500.0, 0x0000FF)] {
            let sq = Contour::new(
                vec![
                    v2(x, 100.0),
                    v2(x + 300.0, 100.0),
                    v2(x + 300.0, 400.0),
                    v2(x, 400.0),
                ],
                true,
            );
            let mut r = RegionSewing::new(RegionType::Fill, Thread::rgb(c));
            r.underlay = true;
            d.add(DesignObject::shape(vec![sq], Some(r), None));
        }
        d
    }

    fn stitches(p: &StitchPattern) -> usize {
        p.count(Command::Stitch)
    }

    #[test]
    fn underlay_only_shows_exactly_the_underlay_and_top_only_the_rest() {
        let d = design();
        let (p, spans) = Compiler::for_design(&d).compile_traced(&d);
        let flags = underlay_flags(&d, &p, &spans);
        let tagged = flags.iter().filter(|f| **f).count();
        assert!(tagged > 0, "fills with underlay have underlay stitches");
        let mut bare = d.clone();
        for o in &mut bare.objects {
            o.region.as_mut().expect("region").underlay = false;
        }
        let top = Compiler::for_design(&bare).compile(&bare);
        let page = [255; 3];
        let f = |layers| ViewFilter {
            layers,
            ..ViewFilter::default()
        };
        let under = apply(&p, &f(Layers::UnderlayOnly), &flags, &spans, page);
        assert_eq!(stitches(&under), tagged);
        let tops = apply(&p, &f(Layers::TopOnly), &flags, &spans, page);
        assert_eq!(stitches(&tops), stitches(&p) - tagged);
        assert!(stitches(&tops) <= stitches(&top));
        // Nothing moves: every record keeps its place; left-out stitches are moves.
        assert_eq!(under.stitches.len(), p.stitches.len());
        assert_eq!(apply(&p, &ViewFilter::default(), &flags, &spans, page), p);
    }

    #[test]
    fn hide_unselected_colour_by_order_monochrome_and_solo() {
        let d = design();
        let (p, spans) = Compiler::for_design(&d).compile_traced(&d);
        let page = [255; 3];
        let first = d.objects[0].id;
        let only = apply(
            &p,
            &ViewFilter {
                only: Some(vec![first]),
                ..ViewFilter::default()
            },
            &[],
            &spans,
            page,
        );
        let in_first = p.stitches[spans[0].1.clone()]
            .iter()
            .filter(|s| s.command == Command::Stitch)
            .count();
        assert_eq!(stitches(&only), in_first);
        let order = apply(
            &p,
            &ViewFilter {
                color_by: ColorBy::SewingOrder,
                ..ViewFilter::default()
            },
            &[],
            &spans,
            page,
        );
        let first_rgb = &order.threads[0];
        let last_rgb = order.threads.last().expect("threads");
        assert_eq!(
            [first_rgb.red, first_rgb.green, first_rgb.blue],
            order_rgb(0.0)
        );
        assert_eq!(
            [last_rgb.red, last_rgb.green, last_rgb.blue],
            order_rgb(1.0)
        );
        assert_eq!(order.threads.len(), ORDER_STEPS);
        let mono = apply(
            &p,
            &ViewFilter {
                color_by: ColorBy::Monochrome,
                ..ViewFilter::default()
            },
            &[],
            &spans,
            page,
        );
        assert_eq!(mono.threads.len(), 1, "one colour");
        let solo = apply(
            &p,
            &ViewFilter {
                solo: Some(1),
                ..ViewFilter::default()
            },
            &[],
            &spans,
            page,
        );
        // Block 0 (red) faded to 20 % over white; block 1 (blue) as it is.
        let red = &solo.threads[0];
        assert_eq!([red.red, red.green, red.blue], [255, 204, 204]);
        let blue = &solo.threads[1];
        assert_eq!([blue.red, blue.green, blue.blue], [0, 0, 255]);
        assert_eq!(stitches(&solo), stitches(&p));
    }

    #[test]
    fn trim_marks_are_at_every_trim_and_nowhere_else() {
        let d = design();
        let p = Compiler::for_design(&d).compile(&d);
        let marks = trim_points(&p);
        assert_eq!(marks.len(), p.count(Command::Trim));
        assert!(!marks.is_empty());
    }
}
