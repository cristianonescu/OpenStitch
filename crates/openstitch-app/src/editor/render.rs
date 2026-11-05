//! Render model: a compiled [`StitchPattern`] split into drawable strokes per colour block, cut
//! off at the sewing simulator's position.

use openstitch_core::{Command, StitchPattern};
use openstitch_design::{Vec2, v2};

/// Fallback when a pattern has fewer threads than colour blocks.
pub const UNKNOWN_THREAD: [u8; 3] = [0x80, 0x80, 0x80];

/// One colour block: polylines of consecutive needle penetrations.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorLayer {
    pub rgb: [u8; 3],
    pub strokes: Vec<Vec<Vec2>>,
    /// Per stroke, each stitch's drawn width as a share of the thread width: narrower where the
    /// thread's rows lie close, so the Realistic view hides as much fabric as sewn thread does
    /// (openstitch_design::calibration, the T1 sew-outs, #291).
    pub cover: Vec<Vec<f32>>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct RenderModel {
    /// In sewing order; later layers draw on top.
    pub layers: Vec<ColorLayer>,
    /// Jumps (travel without sewing), drawn dimmed.
    pub travel: Vec<(Vec2, Vec2)>,
    /// Per `travel` entry: the thread is cut on the way (a trim or a colour change between the
    /// stitches it joins), so the sewn piece has no thread there. The Realistic view leaves those
    /// out, as the fabric shows no thread there (#450: grey travel lines crossed every preview).
    pub travel_cut: Vec<bool>,
    /// Needle position after the last shown stitch.
    pub needle: Option<Vec2>,
    /// Stitches (penetrations) shown.
    pub shown: usize,
}

impl RenderModel {
    /// Builds strokes for the first `limit` stitches (`usize::MAX` for all). A stitch sews from
    /// the previous position; a jump moves without sewing; trims and colour changes break strokes.
    pub fn build(pattern: &StitchPattern, limit: usize) -> RenderModel {
        Self::build_range(pattern, 0, limit)
    }

    /// [`Self::build`] drawing only the stitches from the `from`-th on (the earlier ones only
    /// move the needle): the part sewn after an object a background image sits above (#518).
    pub fn build_range(pattern: &StitchPattern, from: usize, limit: usize) -> RenderModel {
        let rgb = |i: usize| {
            pattern
                .threads
                .get(i)
                .map_or(UNKNOWN_THREAD, |t| [t.red, t.green, t.blue])
        };
        let mut m = RenderModel::default();
        let mut color = 0;
        let mut layer = ColorLayer {
            rgb: rgb(0),
            strokes: Vec::new(),
            cover: Vec::new(),
        };
        let mut stroke: Vec<Vec2> = Vec::new();
        let mut pos: Option<Vec2> = None;
        // Travel entries since the last stitch, and whether the thread was cut since it.
        let mut pending = 0;
        let mut cut = false;
        for s in &pattern.stitches {
            if m.shown >= limit {
                break;
            }
            let p = v2(s.x, s.y);
            match s.command {
                Command::Stitch if m.shown < from => {
                    pos = Some(p);
                    m.shown += 1;
                }
                Command::Stitch => {
                    let n = m.travel_cut.len();
                    m.travel_cut[n - pending..].fill(cut);
                    (pending, cut) = (0, false);
                    if stroke.is_empty() {
                        stroke.push(pos.unwrap_or(p));
                    }
                    stroke.push(p);
                    pos = Some(p);
                    m.shown += 1;
                }
                Command::Jump => {
                    flush(&mut layer, &mut stroke);
                    if let Some(from) = pos.filter(|&q| q != p && m.shown >= from) {
                        m.travel.push((from, p));
                        m.travel_cut.push(true);
                        pending += 1;
                    }
                    pos = Some(p);
                }
                Command::Trim | Command::Stop => {
                    flush(&mut layer, &mut stroke);
                    cut = true;
                }
                Command::ColorChange => {
                    flush(&mut layer, &mut stroke);
                    cut = true;
                    color += 1;
                    let next = ColorLayer {
                        rgb: rgb(color),
                        strokes: Vec::new(),
                        cover: Vec::new(),
                    };
                    let done = std::mem::replace(&mut layer, next);
                    if !done.strokes.is_empty() {
                        m.layers.push(done);
                    }
                }
                Command::End => break,
            }
        }
        flush(&mut layer, &mut stroke);
        if !layer.strokes.is_empty() {
            m.layers.push(layer);
        }
        m.needle = pos;
        m.calibrate();
        m
    }

    /// Fills each layer's [`ColorLayer::cover`] from the local rows per mm of its thread colour
    /// (all layers of that colour together).
    fn calibrate(&mut self) {
        use openstitch_design::calibration::{RowDensity, THREAD_MM, covered_width_mm};
        let mut colours: Vec<[u8; 3]> = self.layers.iter().map(|l| l.rgb).collect();
        colours.sort_unstable();
        colours.dedup();
        for rgb in colours {
            let strokes: Vec<Vec<Vec2>> = self
                .layers
                .iter()
                .filter(|l| l.rgb == rgb)
                .flat_map(|l| l.strokes.iter().cloned())
                .collect();
            let density = RowDensity::of(&strokes);
            for l in self.layers.iter_mut().filter(|l| l.rgb == rgb) {
                l.cover = l
                    .strokes
                    .iter()
                    .map(|s| {
                        s.windows(2)
                            .map(|w| {
                                let r = density.at(w[0].lerp(w[1], 0.5));
                                (covered_width_mm(r) / THREAD_MM) as f32
                            })
                            .collect()
                    })
                    .collect();
            }
        }
    }
}

fn flush(layer: &mut ColorLayer, stroke: &mut Vec<Vec2>) {
    if stroke.len() >= 2 {
        layer.strokes.push(std::mem::take(stroke));
    } else {
        stroke.clear();
    }
}

/// Statistics shown under the simulator, matching the engine's `design.generate` result.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct PatternStats {
    pub stitches: usize,
    pub jumps: usize,
    pub trims: usize,
    pub color_changes: usize,
    pub width_mm: f64,
    pub height_mm: f64,
    pub colors: Vec<[u8; 3]>,
}

impl PatternStats {
    pub fn of(p: &StitchPattern) -> PatternStats {
        let b = p.bounds();
        PatternStats {
            stitches: p.count(Command::Stitch),
            jumps: p.count(Command::Jump),
            trims: p.count(Command::Trim),
            color_changes: p.count(Command::ColorChange),
            width_mm: (b.width()).round() / 10.0,
            height_mm: (b.height()).round() / 10.0,
            colors: p.threads.iter().map(|t| [t.red, t.green, t.blue]).collect(),
        }
    }
}

/// 0-based colour block that the `n`-th stitch (1-based) belongs to, and the block count.
pub fn color_at(p: &StitchPattern, n: usize) -> (usize, usize) {
    let blocks = p.color_block_count();
    let mut seen = 0;
    let mut color = 0;
    for s in &p.stitches {
        match s.command {
            Command::Stitch => {
                seen += 1;
                if seen >= n {
                    break;
                }
            }
            Command::ColorChange => color += 1,
            _ => {}
        }
    }
    (color.min(blocks.saturating_sub(1)), blocks)
}
