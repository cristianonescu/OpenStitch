//! Keyboard drawing support in the tool controller (#499) and widths while drawing (#707).
//!
//! The window moves a keyboard cursor with the arrow keys and feeds the tools the same pointer
//! events a mouse would ([`ToolController::handle`]), so a shape drawn from the keyboard ends in
//! the same command. This module adds what the mouse path did not need: removing the last point
//! (Backspace), and the width of the next point for the Pen and Satin Skeleton tools (`[` / `]`
//! and the scroll wheel). A Pen line drawn with widths becomes a satin line whose width keys sit
//! at its anchors.

use super::{Tool, ToolController};
use crate::editor::pen::pen_contour;
use openstitch_design::Vec2;
use openstitch_design::skeleton::{DEFAULT_WIDTH, MAX_WIDTH};

/// One `[` / `]` press or scroll notch changes the width by this much (mm).
pub const WIDTH_STEP: f64 = 0.5;

fn clamp_width(mm: f64) -> f64 {
    if mm.is_finite() {
        mm.clamp(0.1, MAX_WIDTH)
    } else {
        DEFAULT_WIDTH
    }
}

impl ToolController {
    /// Backspace while drawing: drop the last point placed (polygon, line, pen, skeleton, punch,
    /// pencil). `false` when there was none.
    pub fn remove_last_point(&mut self) -> bool {
        let removed = match self.tool() {
            Tool::Polygon | Tool::Line | Tool::Knife | Tool::Gradient => {
                self.polygon.pop().is_some()
            }
            Tool::Pen | Tool::Skeleton => {
                let removed = self.pen.pop().is_some();
                self.widths.truncate(self.pen.len());
                removed
            }
            t if t.is_punch() => self.punch.pop().is_some(),
            t if t.is_pencil() => self.pencil.pop().is_some(),
            _ => false,
        };
        if removed {
            self.gesture = None;
            if self.polygon.is_empty()
                && self.pen.is_empty()
                && self.punch.is_empty()
                && self.pencil.is_empty()
            {
                self.preview = None;
            }
        }
        removed
    }

    /// Width (mm) the next Pen point gets; `None` while the Pen draws a plain line.
    pub fn pen_width(&self) -> Option<f64> {
        self.pen_width
    }

    /// The width of the next point of the tool drawing widths, if it is one (Pen, Satin
    /// Skeleton).
    pub fn next_width(&self) -> Option<f64> {
        match self.tool() {
            Tool::Skeleton => Some(self.skeleton_width()),
            Tool::Pen => Some(self.pen_width.unwrap_or(DEFAULT_WIDTH)),
            _ => None,
        }
    }

    /// Sets the next point's width: the Satin Skeleton's column width, or the Pen's (which
    /// makes the line a satin line). `None` for other tools.
    pub fn set_next_width(&mut self, mm: f64) -> Option<f64> {
        let w = clamp_width(mm);
        match self.tool() {
            Tool::Skeleton => self.set_skeleton_width(w),
            Tool::Pen => self.pen_width = Some(w),
            _ => return None,
        }
        Some(w)
    }

    /// `[` / `]`, or a scroll notch: change the next point's width by `delta` mm; with the
    /// pointer within `radius` of a point already placed, that point's width instead. Returns
    /// the width set.
    pub fn step_width(&mut self, delta: f64, at: Option<Vec2>, radius: f64) -> Option<f64> {
        if !matches!(self.tool(), Tool::Pen | Tool::Skeleton) {
            return None;
        }
        if let Some(p) = at
            && let Some(i) = self.pen.iter().rposition(|n| n.at.distance(p) <= radius)
        {
            self.fill_widths();
            let w = clamp_width(self.widths[i] + delta);
            self.widths[i] = w;
            if self.tool() == Tool::Pen && self.pen_width.is_none() {
                self.pen_width = Some(DEFAULT_WIDTH);
            }
            return Some(w);
        }
        let now = self.next_width()?;
        self.set_next_width(now + delta)
    }

    /// A width for every anchor placed so far (the Pen records none until a width is set).
    fn fill_widths(&mut self) {
        let fill = self.next_width().unwrap_or(DEFAULT_WIDTH);
        while self.widths.len() < self.pen.len() {
            self.widths.push(fill);
        }
    }

    /// Pen: a point was just placed; record its width when the line has widths.
    pub(super) fn pen_point_placed(&mut self) {
        if self.pen_width.is_some() {
            self.fill_widths();
        }
    }

    /// Pen: the width keys `[at, left, right]` of the line just finished, at its anchors (by
    /// length along the line), when it was drawn with widths. Taken once.
    pub fn take_satin_keys(&mut self) -> Option<Vec<[f64; 3]>> {
        self.satin_keys.take()
    }

    /// Pen finishing an open line: keep the width keys for [`Self::take_satin_keys`].
    pub(super) fn record_satin_keys(&mut self, nodes: &[crate::editor::pen::PenNode]) {
        self.satin_keys = None;
        if self.pen_width.is_none() || nodes.len() < 2 {
            return;
        }
        let c = pen_contour(nodes, false);
        let lengths: Vec<f64> = (0..c.segment_count())
            .map(|i| {
                c.segment_polyline(i)
                    .windows(2)
                    .map(|w| w[0].distance(w[1]))
                    .sum()
            })
            .collect();
        let total: f64 = lengths.iter().sum();
        if total <= 0.0 {
            return;
        }
        let fill = self.pen_width.unwrap_or(DEFAULT_WIDTH);
        let mut at = 0.0;
        let mut keys = Vec::with_capacity(nodes.len());
        for i in 0..nodes.len() {
            let w = self.widths.get(i).copied().unwrap_or(fill);
            keys.push([(at / total).clamp(0.0, 1.0), w / 2.0, w / 2.0]);
            if let Some(l) = lengths.get(i) {
                at += l;
            }
        }
        self.satin_keys = Some(keys);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::tools::{Phase, PointerEvent, ToolContext};
    use openstitch_design::{Design, v2};
    use std::collections::BTreeSet;

    fn ctx<'a>(d: &'a Design, sel: &'a BTreeSet<u64>) -> ToolContext<'a> {
        ToolContext {
            design: d,
            selection: sel,
            units_per_point: 1.0,
            scale_takes_origin: true,
        }
    }

    fn click(t: &mut ToolController, c: &ToolContext, x: f64, y: f64) {
        t.handle(PointerEvent::new(Phase::Down, v2(x, y)), c);
        t.handle(PointerEvent::new(Phase::Up, v2(x, y)), c);
    }

    #[test]
    fn backspace_drops_the_last_point_of_each_drawing() {
        let (d, sel) = (Design::default(), BTreeSet::new());
        let c = ctx(&d, &sel);
        for tool in [Tool::Polygon, Tool::Pen, Tool::Skeleton, Tool::Punch] {
            let mut t = ToolController::default();
            t.set_tool(tool);
            click(&mut t, &c, 0.0, 0.0);
            click(&mut t, &c, 100.0, 0.0);
            assert!(t.remove_last_point(), "{tool:?}");
            assert!(t.remove_last_point(), "{tool:?}");
            assert!(!t.remove_last_point(), "{tool:?}: nothing left");
            assert!(!t.is_busy(), "{tool:?}");
        }
    }

    #[test]
    fn brackets_set_the_next_width_and_a_point_under_the_pointer() {
        let (d, sel) = (Design::default(), BTreeSet::new());
        let c = ctx(&d, &sel);
        let mut t = ToolController::default();
        t.set_tool(Tool::Skeleton);
        assert_eq!(
            t.step_width(WIDTH_STEP, None, 1.0),
            Some(DEFAULT_WIDTH + WIDTH_STEP)
        );
        click(&mut t, &c, 0.0, 0.0);
        assert_eq!(t.step_width(-1.0, None, 1.0), Some(DEFAULT_WIDTH - 0.5));
        click(&mut t, &c, 200.0, 0.0);
        assert_eq!(t.widths, vec![3.5, 2.5], "each point keeps its width");
        // Scrolling over the first point changes that one.
        assert_eq!(t.step_width(WIDTH_STEP, Some(v2(1.0, 0.0)), 5.0), Some(4.0));
        assert_eq!(t.widths, vec![4.0, 2.5]);
        t.set_tool(Tool::Select);
        assert_eq!(t.step_width(1.0, None, 1.0), None, "only Pen and Skeleton");
    }

    #[test]
    fn a_pen_line_with_widths_gets_keys_at_its_anchors() {
        let (d, sel) = (Design::default(), BTreeSet::new());
        let c = ctx(&d, &sel);
        let mut t = ToolController::default();
        t.set_tool(Tool::Pen);
        click(&mut t, &c, 0.0, 0.0);
        click(&mut t, &c, 100.0, 0.0);
        assert!(t.pen_width().is_none(), "a plain line so far");
        t.set_next_width(4.0);
        click(&mut t, &c, 300.0, 0.0);
        let effects = t.finish();
        assert_eq!(effects.len(), 1, "the same shape.path as without widths");
        let keys = t.take_satin_keys().expect("keys");
        assert_eq!(keys.len(), 3);
        assert_eq!(keys[0][0], 0.0);
        assert!((keys[1][0] - 1.0 / 3.0).abs() < 1e-9, "{keys:?}");
        assert_eq!(keys[2], [1.0, 2.0, 2.0]);
        assert!(t.take_satin_keys().is_none(), "taken once");
        // Without widths, no keys.
        t.set_tool(Tool::Pen);
        click(&mut t, &c, 0.0, 0.0);
        click(&mut t, &c, 100.0, 0.0);
        t.finish();
        assert!(t.take_satin_keys().is_none());
    }
}
