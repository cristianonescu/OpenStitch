//! Gradient tool (#532, beyond PE-DESIGN): set where the selected gradient fills run. Click
//! (or press, with the keyboard cursor, Space) where the gradient starts, then where it ends;
//! or drag from start to end. Shift turns the line onto 15° steps. One `object.setRegion`
//! with `gradientFrom` and `gradientTo` changes every object of the selected gradients, so one
//! undo step. The tool stays active; Esc drops the start point, and with none it goes back to
//! Select.

use super::{Phase, PointerEvent, ToolContext, ToolController, ToolEffect, ToolPreview};
use crate::editor::request::{CommandRequest, mm};
use openstitch_design::{Contour, Design, Vec2, v2};
use serde_json::json;
use std::collections::BTreeSet;

/// `p` turned about `from` onto the nearest multiple of 15°.
pub fn constrain_15(from: Vec2, p: Vec2) -> Vec2 {
    let d = p - from;
    let len = d.length();
    if len == 0.0 {
        return p;
    }
    let step = std::f64::consts::PI / 12.0;
    let angle = (d.y.atan2(d.x) / step).round() * step;
    from + v2(angle.cos(), angle.sin()) * len
}

/// The objects of the gradients among `selection`: every object of the design whose fill is
/// part of a gradient and that is selected or grouped with a selected one.
pub fn gradient_targets(design: &Design, selection: &BTreeSet<u64>) -> Vec<u64> {
    let has = |o: &openstitch_design::DesignObject| {
        o.region.as_ref().is_some_and(|r| r.gradient.is_some())
    };
    let groups: BTreeSet<u64> = design
        .objects
        .iter()
        .filter(|o| selection.contains(&o.id) && has(o))
        .filter_map(|o| o.group)
        .collect();
    design
        .objects
        .iter()
        .filter(|o| {
            has(o) && (selection.contains(&o.id) || o.group.is_some_and(|g| groups.contains(&g)))
        })
        .map(|o| o.id)
        .collect()
}

impl ToolController {
    fn gradient_point(&self, e: &PointerEvent) -> Vec2 {
        match self.polygon.first() {
            Some(&start) if e.shift => constrain_15(start, e.location),
            _ => e.location,
        }
    }

    pub(super) fn gradient_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        match e.phase {
            Phase::Down => {
                if self.polygon.is_empty() {
                    self.polygon.push(e.location);
                    self.preview = None;
                    return vec![];
                }
                let end = self.gradient_point(&e);
                self.finish_gradient(end, ctx)
            }
            Phase::Moved | Phase::Dragged => {
                if let Some(&start) = self.polygon.first() {
                    let end = self.gradient_point(&e);
                    self.preview =
                        Some(ToolPreview::Outline(Contour::new(vec![start, end], false)));
                }
                vec![]
            }
            // A drag: released away from where it started ends the line there.
            Phase::Up => match self.polygon.first() {
                Some(&start) if start.distance(e.location) > ctx.tolerance() => {
                    let end = self.gradient_point(&e);
                    self.finish_gradient(end, ctx)
                }
                _ => vec![],
            },
        }
    }

    /// Sets the selected gradients' start and end (`object.setRegion` `gradientFrom` /
    /// `gradientTo`); the tool stays active.
    fn finish_gradient(&mut self, end: Vec2, ctx: &ToolContext) -> Vec<ToolEffect> {
        let start = self.polygon.first().copied();
        self.polygon.clear();
        self.preview = None;
        let Some(start) = start.filter(|s| s.distance(end) > ctx.tolerance()) else {
            return vec![];
        };
        let ids = gradient_targets(ctx.design, ctx.selection);
        if ids.is_empty() {
            return vec![];
        }
        let point = |p: Vec2| json!([mm(p.x), mm(p.y)]);
        vec![ToolEffect::run(
            CommandRequest::new(
                "object.setRegion",
                json!({ "ids": ids, "gradientFrom": point(start), "gradientTo": point(end) }),
            ),
            false,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::super::Tool;
    use super::*;
    use openstitch_engine::Session;

    fn send(t: &mut ToolController, s: &mut Session, sel: &BTreeSet<u64>, e: PointerEvent) {
        let design = s.design().clone();
        let effects = t.handle(
            e,
            &ToolContext {
                design: &design,
                selection: sel,
                units_per_point: 1.0,
                scale_takes_origin: false,
            },
        );
        for e in effects {
            if let ToolEffect::Execute { requests, .. } = e {
                for r in requests {
                    s.execute(r.command, Some(&r.params))
                        .unwrap_or_else(|e| panic!("{}: {e}", r.command));
                }
            }
        }
    }

    fn gradients(s: &Session) -> Vec<(Vec2, Vec2)> {
        s.design()
            .objects
            .iter()
            .filter_map(|o| o.region.as_ref()?.gradient.as_ref().map(|g| (g.from, g.to)))
            .collect()
    }

    #[test]
    fn a_drag_or_two_clicks_set_where_every_colour_of_the_gradient_runs() {
        let mut s = Session::new(std::env::temp_dir());
        s.execute(
            "shape.rectangle",
            Some(&json!({"x": 10, "y": 10, "width": 40, "height": 20, "fill": "Blue"})),
        )
        .expect("rect");
        s.execute(
            "shape.gradient",
            Some(&json!({"id": 1, "threads": ["Blue", "White", "Red"]})),
        )
        .expect("gradient");
        let sel = BTreeSet::from([1]);
        let mut t = ToolController::default();
        t.set_tool(Tool::Gradient);
        // A drag from (100, 100) to (100, 300): top to bottom, on all three colours.
        send(
            &mut t,
            &mut s,
            &sel,
            PointerEvent::new(Phase::Down, v2(100.0, 100.0)),
        );
        send(
            &mut t,
            &mut s,
            &sel,
            PointerEvent::new(Phase::Dragged, v2(100.0, 200.0)),
        );
        send(
            &mut t,
            &mut s,
            &sel,
            PointerEvent::new(Phase::Up, v2(100.0, 300.0)),
        );
        let g = gradients(&s);
        assert_eq!(g.len(), 3);
        assert!(
            g.iter()
                .all(|&(a, b)| a == v2(100.0, 100.0) && b == v2(100.0, 300.0))
        );
        assert_eq!(t.tool(), Tool::Gradient);
        // Two clicks with Shift: the second point snaps onto 15° steps (here 45°).
        send(
            &mut t,
            &mut s,
            &sel,
            PointerEvent::new(Phase::Down, v2(100.0, 100.0)),
        );
        send(
            &mut t,
            &mut s,
            &sel,
            PointerEvent::new(Phase::Up, v2(100.0, 100.0)),
        );
        send(
            &mut t,
            &mut s,
            &sel,
            PointerEvent::new(Phase::Down, v2(300.0, 290.0)).shift(),
        );
        let (a, b) = gradients(&s)[2];
        assert_eq!(a, v2(100.0, 100.0));
        assert!(((b.y - a.y) - (b.x - a.x)).abs() < 1e-9, "{b:?}");
        // One undo step per gesture.
        s.execute("history.undo", None).expect("undo");
        assert!(gradients(&s).iter().all(|&(_, b)| b == v2(100.0, 300.0)));
        // Nothing selected, or a click without a drag: no change.
        let none = BTreeSet::new();
        send(
            &mut t,
            &mut s,
            &none,
            PointerEvent::new(Phase::Down, v2(0.0, 0.0)),
        );
        send(
            &mut t,
            &mut s,
            &none,
            PointerEvent::new(Phase::Up, v2(0.0, 400.0)),
        );
        assert!(gradients(&s).iter().all(|&(_, b)| b == v2(100.0, 300.0)));
    }

    #[test]
    fn steps_of_15_degrees() {
        let p = constrain_15(v2(0.0, 0.0), v2(10.0, 1.0));
        assert!(p.y.abs() < 1e-9 && (p.x - 10.0_f64.hypot(1.0)).abs() < 1e-9);
        let q = constrain_15(v2(0.0, 0.0), v2(10.0, 5.6));
        let angle = q.y.atan2(q.x).to_degrees();
        assert!((angle - 30.0).abs() < 1e-9, "{angle}");
    }
}
