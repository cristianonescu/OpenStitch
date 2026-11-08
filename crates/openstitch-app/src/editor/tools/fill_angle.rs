//! Fill Angle tool (#534): drag across the selected regions in the direction their rows should
//! run (PE-DESIGN's straight sewing direction set on the canvas); on release one
//! `object.setRegion` sets the angle of every selected region that has a direction (Auto
//! direction off), so one undo step. Clicking twice (or the arrow keys and Space) works as a
//! drag does. Shift turns the line onto 15° steps. The tool stays active; Esc drops the line in
//! progress (the angle is only set on release), and with none it goes back to Select.

use super::{Phase, PointerEvent, ToolContext, ToolController, ToolEffect, ToolPreview};
use crate::editor::request::CommandRequest;
use openstitch_design::{Contour, Design, RegionType, Vec2, v2};
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

/// Whether region type `k` has a sewing direction (the inspector's Direction): not the types
/// centred on a point or following the outline, and not cross stitch (on the page axes).
pub fn has_direction(k: RegionType) -> bool {
    !matches!(
        k,
        RegionType::ConcentricCircle
            | RegionType::Radial
            | RegionType::Spiral
            | RegionType::FlexibleSpiral
            | RegionType::Contour
            | RegionType::CrossStitch
    )
}

/// The selected objects whose region has a direction.
pub fn angle_targets(design: &Design, selection: &BTreeSet<u64>) -> Vec<u64> {
    design
        .objects
        .iter()
        .filter(|o| selection.contains(&o.id))
        .filter(|o| o.region.as_ref().is_some_and(|r| has_direction(r.kind)))
        .map(|o| o.id)
        .collect()
}

/// The stored direction (counter-clockwise on screen, 0 ≤ a < 180, as the rows run either way)
/// of a drag from `a` to `b`, rounded to 0.1°.
pub fn drag_angle(a: Vec2, b: Vec2) -> f64 {
    let d = b - a;
    let geometry = d.y.atan2(d.x).to_degrees();
    let stored = openstitch_design::region::stored_angle(geometry).rem_euclid(180.0);
    let stored = (stored * 10.0).round() / 10.0;
    if stored >= 180.0 { 0.0 } else { stored }
}

impl ToolController {
    fn angle_point(&self, e: &PointerEvent) -> Vec2 {
        match self.polygon.first() {
            Some(&start) if e.shift => constrain_15(start, e.location),
            _ => e.location,
        }
    }

    pub(super) fn fill_angle_tool(
        &mut self,
        e: PointerEvent,
        ctx: &ToolContext,
    ) -> Vec<ToolEffect> {
        match e.phase {
            Phase::Down => {
                if self.polygon.is_empty() {
                    self.polygon.push(e.location);
                    self.preview = None;
                    return vec![];
                }
                let end = self.angle_point(&e);
                self.finish_fill_angle(end, ctx)
            }
            Phase::Moved | Phase::Dragged => {
                if let Some(&start) = self.polygon.first() {
                    let end = self.angle_point(&e);
                    self.preview =
                        Some(ToolPreview::Outline(Contour::new(vec![start, end], false)));
                }
                vec![]
            }
            Phase::Up => match self.polygon.first() {
                Some(&start) if start.distance(e.location) > ctx.tolerance() => {
                    let end = self.angle_point(&e);
                    self.finish_fill_angle(end, ctx)
                }
                _ => vec![],
            },
        }
    }

    fn finish_fill_angle(&mut self, end: Vec2, ctx: &ToolContext) -> Vec<ToolEffect> {
        let start = self.polygon.first().copied();
        self.polygon.clear();
        self.preview = None;
        let Some(start) = start.filter(|s| s.distance(end) > ctx.tolerance()) else {
            return vec![];
        };
        let ids = angle_targets(ctx.design, ctx.selection);
        if ids.is_empty() {
            return vec![];
        }
        vec![ToolEffect::run(
            CommandRequest::new(
                "object.setRegion",
                json!({ "ids": ids, "angle": drag_angle(start, end), "autoDirection": false }),
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

    fn angles(s: &Session) -> Vec<(f64, bool)> {
        s.design()
            .objects
            .iter()
            .filter_map(|o| o.region.as_ref().map(|r| (r.angle, r.auto_direction)))
            .collect()
    }

    #[test]
    fn dragging_turns_the_rows_of_the_selected_regions() {
        let mut s = Session::new(std::env::temp_dir());
        for (x, kind) in [(10, "fill"), (40, "satin"), (70, "concentricCircle")] {
            s.execute(
                "shape.rectangle",
                Some(&json!({"x": x, "y": 10, "width": 20, "height": 20, "fill": "Blue"})),
            )
            .expect("rect");
            let id = s.design().objects.last().expect("o").id;
            s.execute(
                "object.setRegion",
                Some(&json!({"id": id, "type": kind, "autoDirection": kind == "fill"})),
            )
            .expect("type");
        }
        let sel: BTreeSet<u64> = s.design().objects.iter().map(|o| o.id).collect();
        let before = angles(&s);
        let mut t = ToolController::default();
        t.set_tool(Tool::FillAngle);
        // A drag up and to the right at 30° on screen (y down): rows at 30°.
        let a = v2(100.0, 300.0);
        let b = a + v2(30f64.to_radians().cos(), -30f64.to_radians().sin()) * 200.0;
        send(&mut t, &mut s, &sel, PointerEvent::new(Phase::Down, a));
        send(
            &mut t,
            &mut s,
            &sel,
            PointerEvent::new(Phase::Dragged, a.lerp(b, 0.5)),
        );
        send(&mut t, &mut s, &sel, PointerEvent::new(Phase::Up, b));
        let after = angles(&s);
        assert!(
            (after[0].0 - 30.0).abs() < 1.0 && !after[0].1,
            "fill {:?}",
            after[0]
        );
        assert!((after[1].0 - 30.0).abs() < 1.0, "satin {:?}", after[1]);
        assert_eq!(after[2], before[2], "a concentric circle has no direction");
        assert_eq!(t.tool(), Tool::FillAngle);
        // Two clicks with Shift: 15° steps (a 50° drag becomes 45°).
        let c = a + v2(50f64.to_radians().cos(), -50f64.to_radians().sin()) * 200.0;
        send(&mut t, &mut s, &sel, PointerEvent::new(Phase::Down, a));
        send(&mut t, &mut s, &sel, PointerEvent::new(Phase::Up, a));
        send(
            &mut t,
            &mut s,
            &sel,
            PointerEvent::new(Phase::Down, c).shift(),
        );
        assert!((angles(&s)[0].0 - 45.0).abs() < 1e-6, "{:?}", angles(&s));
        // One undo step per gesture.
        s.execute("history.undo", None).expect("undo");
        assert!((angles(&s)[0].0 - 30.0).abs() < 1.0);
        s.execute("history.undo", None).expect("undo");
        assert_eq!(angles(&s), before);
        // Nothing selected: no change.
        let none = BTreeSet::new();
        send(&mut t, &mut s, &none, PointerEvent::new(Phase::Down, a));
        send(&mut t, &mut s, &none, PointerEvent::new(Phase::Up, b));
        assert_eq!(angles(&s), before);
    }

    #[test]
    fn the_drag_gives_the_stored_angle() {
        assert_eq!(drag_angle(v2(0.0, 0.0), v2(10.0, 0.0)), 0.0);
        assert_eq!(drag_angle(v2(0.0, 0.0), v2(-10.0, 0.0)), 0.0);
        assert_eq!(drag_angle(v2(0.0, 0.0), v2(0.0, -10.0)), 90.0);
        assert_eq!(drag_angle(v2(0.0, 0.0), v2(0.0, 10.0)), 90.0);
        assert_eq!(drag_angle(v2(0.0, 0.0), v2(10.0, 10.0)), 135.0);
        let p = constrain_15(v2(0.0, 0.0), v2(10.0, 5.6));
        assert!((p.y.atan2(p.x).to_degrees() - 30.0).abs() < 1e-9);
    }
}
