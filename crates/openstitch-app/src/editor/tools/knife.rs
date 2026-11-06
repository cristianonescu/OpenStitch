//! Knife tool (#477, Shapes ▸ Edit Outline): click the points of an open cut line (Shift: 45°
//! steps from the last point); a double-click or Return cuts every object the line crosses
//! with one `shape.knife`, so one undo step. A single click then Return, or a double-click
//! with no line, splits the line under the pointer there. The tool stays active for more
//! cuts; Esc drops the line in progress, and with none it goes back to Select.

use super::{Phase, PointerEvent, Tool, ToolContext, ToolController, ToolEffect, ToolPreview};
use crate::editor::request::{CommandRequest, mm};
use openstitch_design::{Contour, Vec2, v2};
use serde_json::json;

/// Screen points within which a click splits the line under it.
pub const SPLIT_REACH: f64 = 6.0;

/// `p` turned about `from` onto the nearest multiple of 45°.
pub fn constrain_45(from: Vec2, p: Vec2) -> Vec2 {
    let d = p - from;
    let len = d.length();
    if len == 0.0 {
        return p;
    }
    let step = std::f64::consts::FRAC_PI_4;
    let angle = (d.y.atan2(d.x) / step).round() * step;
    from + v2(angle.cos(), angle.sin()) * len
}

impl ToolController {
    fn knife_point(&self, e: &PointerEvent) -> Vec2 {
        match self.polygon.last() {
            Some(&last) if e.shift => constrain_45(last, e.location),
            _ => e.location,
        }
    }

    pub(super) fn knife_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        self.knife_reach = SPLIT_REACH * ctx.units_per_point;
        match e.phase {
            Phase::Down => {
                if e.clicks >= 2 {
                    if self.polygon.is_empty() {
                        self.polygon.push(e.location);
                    }
                    return self.finish_knife();
                }
                let p = self.knife_point(&e);
                if self
                    .polygon
                    .last()
                    .is_some_and(|last| last.distance(p) <= ctx.tolerance() / 2.0)
                {
                    return vec![];
                }
                self.polygon.push(p);
                self.preview = Some(ToolPreview::Outline(Contour::new(
                    self.polygon.clone(),
                    false,
                )));
                vec![]
            }
            Phase::Moved | Phase::Dragged => {
                if !self.polygon.is_empty() {
                    let mut pts = self.polygon.clone();
                    pts.push(self.knife_point(&e));
                    self.preview = Some(ToolPreview::Outline(Contour::new(pts, false)));
                }
                vec![]
            }
            Phase::Up => vec![],
        }
    }

    /// Cuts along the points clicked (`shape.knife` `line`), or with one point splits the
    /// line under it (`at`); the tool stays active.
    pub fn finish_knife(&mut self) -> Vec<ToolEffect> {
        let pts = std::mem::take(&mut self.polygon);
        self.preview = None;
        if self.tool() != Tool::Knife {
            return vec![];
        }
        let point = |p: &Vec2| json!([mm(p.x), mm(p.y)]);
        let params = match pts.as_slice() {
            [] => return vec![],
            [p] => {
                json!({ "at": point(p), "reach": (mm(self.knife_reach) * 100.0).round().max(1.0) / 100.0 })
            }
            _ => json!({ "line": pts.iter().map(point).collect::<Vec<_>>() }),
        };
        vec![ToolEffect::run(
            CommandRequest::new("shape.knife", params),
            true,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_engine::Session;
    use std::collections::BTreeSet;

    fn send(t: &mut ToolController, s: &mut Session, e: PointerEvent) -> usize {
        let design = s.design().clone();
        let sel = BTreeSet::new();
        let effects = t.handle(
            e,
            &ToolContext {
                design: &design,
                selection: &sel,
                units_per_point: 1.0,
                scale_takes_origin: false,
            },
        );
        run(s, effects)
    }

    fn run(s: &mut Session, effects: Vec<ToolEffect>) -> usize {
        let mut n = 0;
        for e in effects {
            if let ToolEffect::Execute { requests, .. } = e {
                for r in requests {
                    s.execute(r.command, Some(&r.params))
                        .unwrap_or_else(|e| panic!("{}: {e}", r.command));
                    n += 1;
                }
            }
        }
        n
    }

    fn click(t: &mut ToolController, s: &mut Session, p: Vec2, clicks: u32) -> usize {
        let mut e = PointerEvent::new(Phase::Down, p);
        e.clicks = clicks;
        let n = send(t, s, e);
        send(t, s, PointerEvent::new(Phase::Up, p));
        n
    }

    fn add(s: &mut Session, command: &str, params: serde_json::Value) {
        s.execute(command, Some(&params)).expect(command);
    }

    #[test]
    fn one_stroke_cuts_three_objects_in_one_command_and_the_tool_stays() {
        let mut s = Session::new(std::env::temp_dir());
        add(
            &mut s,
            "shape.rectangle",
            json!({ "x": 10, "y": 10, "width": 20, "height": 20 }),
        );
        add(
            &mut s,
            "shape.rectangle",
            json!({ "x": 40, "y": 10, "width": 20, "height": 20 }),
        );
        add(
            &mut s,
            "shape.polygon",
            json!({ "points": [[70, 10], [70, 30]], "closed": false }),
        );
        let mut t = ToolController::default();
        t.set_tool(Tool::Knife);
        click(&mut t, &mut s, v2(50.0, 150.0), 1);
        click(&mut t, &mut s, v2(800.0, 150.0), 1);
        // The double-click's second press finishes: one command for the three cuts.
        assert_eq!(click(&mut t, &mut s, v2(800.0, 150.0), 2), 1);
        assert_eq!(s.design().objects.len(), 6);
        assert_eq!(t.tool(), Tool::Knife);
        assert!(!t.is_busy());
        s.execute("history.undo", None).expect("undo");
        assert_eq!(s.design().objects.len(), 3);
    }

    #[test]
    fn shift_steps_by_45_degrees_and_a_single_click_splits_a_line() {
        assert_eq!(
            constrain_45(v2(0.0, 0.0), v2(10.0, 1.0)),
            v2(10.04987562112089, 0.0)
        );
        let d = constrain_45(v2(0.0, 0.0), v2(10.0, 9.0));
        assert!((d.x - d.y).abs() < 1e-9);
        let mut s = Session::new(std::env::temp_dir());
        add(
            &mut s,
            "shape.polygon",
            json!({ "points": [[10, 10], [30, 10]], "closed": false }),
        );
        let mut t = ToolController::default();
        t.set_tool(Tool::Knife);
        click(&mut t, &mut s, v2(200.0, 102.0), 1);
        let effects = t.finish();
        assert_eq!(run(&mut s, effects), 1);
        assert_eq!(s.design().objects.len(), 2);
        // A double-click with no line splits there too.
        click(&mut t, &mut s, v2(150.0, 100.0), 1);
        t.cancel();
        assert_eq!(click(&mut t, &mut s, v2(250.0, 100.0), 2), 1);
        assert_eq!(s.design().objects.len(), 3);
    }
}
