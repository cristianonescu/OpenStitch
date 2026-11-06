//! Shapes ▸ Edit Outline ▸ Shape Effects (#497, beyond PE-DESIGN): the dropdown's settings and
//! the `shape.effect` request. One command, so one Undo.

use super::request::CommandRequest;
use openstitch_design::effects::Effect;
use openstitch_design::{Contour, Design, ObjectKind};
use serde_json::json;
use std::collections::BTreeSet;

/// Settings of the Shape Effects dropdown.
#[derive(Clone, Debug, PartialEq)]
pub struct EffectForm {
    pub effect: Effect,
    /// In the effect's unit (see [`Effect::amount`]).
    pub amount: f64,
    /// Roughen and Zigzag: mm between points.
    pub size: f64,
    pub seed: u64,
}

impl Default for EffectForm {
    fn default() -> Self {
        EffectForm {
            effect: Effect::Warp,
            amount: Effect::Warp.amount().1,
            size: 2.0,
            seed: 1,
        }
    }
}

impl EffectForm {
    /// Picks `effect` with its default amount.
    pub fn set_effect(&mut self, effect: Effect) {
        if effect != self.effect {
            self.effect = effect;
            self.amount = effect.amount().1;
        }
    }

    pub fn request(&self, ids: &BTreeSet<u64>) -> CommandRequest {
        let round = |v: f64| (v * 100.0).round() / 100.0;
        CommandRequest::new(
            "shape.effect",
            json!({
                "ids": CommandRequest::ids(ids),
                "kind": self.effect.id(),
                "amount": round(self.amount),
                "size": round(self.size.clamp(0.2, 50.0)),
                "seed": self.seed,
            }),
        )
    }

    /// The outlines of `contours` as the effect would leave them (the dropdown's preview).
    pub fn preview(&self, contours: &[Contour]) -> Vec<Contour> {
        openstitch_design::effects::apply(contours, self.effect, self.amount, self.size, self.seed)
    }
}

/// The selected shapes an effect can change: free outlines (not columns or skeletons).
pub fn targets(design: &Design, selection: &BTreeSet<u64>) -> BTreeSet<u64> {
    selection
        .iter()
        .copied()
        .filter(|&id| {
            design.index_of(id).is_some_and(|i| {
                let o = &design.objects[i];
                o.kind == ObjectKind::Shape
                    && !o
                        .region
                        .as_ref()
                        .is_some_and(|r| r.direction_lines.len() >= 2 || r.skeleton.is_some())
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_engine::Session;

    #[test]
    fn the_form_runs_one_effect_and_previews_what_it_does() {
        let mut s = Session::new(std::env::temp_dir());
        let r = s
            .execute(
                "shape.rectangle",
                Some(&json!({ "x": 10, "y": 10, "width": 30, "height": 20 })),
            )
            .expect("rectangle")["id"]
            .as_u64()
            .expect("id");
        let mut form = EffectForm::default();
        form.set_effect(Effect::Twist);
        assert_eq!(form.amount, 45.0);
        let ids = targets(s.design(), &BTreeSet::from([r]));
        assert_eq!(ids, BTreeSet::from([r]));
        let preview = form.preview(&s.design().objects[0].contours);
        let req = form.request(&ids);
        s.execute(req.command, Some(&req.params)).expect("effect");
        assert_eq!(s.design().objects[0].contours, preview);
    }

    #[test]
    fn the_rectangle_tool_draws_a_radius_per_corner() {
        use crate::editor::{Phase, PointerEvent, Tool, ToolContext, ToolController, ToolEffect};
        use openstitch_design::v2;
        let mut s = Session::new(std::env::temp_dir());
        let mut t = ToolController::default();
        t.set_tool(Tool::Rectangle);
        t.set_corner_radii(Some([0.0, 2.0, 4.0, 6.0]));
        let design = s.design().clone();
        let sel = BTreeSet::new();
        let ctx = ToolContext {
            design: &design,
            selection: &sel,
            units_per_point: 1.0,
            scale_takes_origin: false,
        };
        t.handle(PointerEvent::new(Phase::Down, v2(100.0, 100.0)), &ctx);
        t.handle(PointerEvent::new(Phase::Dragged, v2(500.0, 300.0)), &ctx);
        let effects = t.handle(PointerEvent::new(Phase::Up, v2(500.0, 300.0)), &ctx);
        let Some(ToolEffect::Execute { requests, .. }) = effects.into_iter().next() else {
            panic!("a rectangle");
        };
        assert_eq!(
            requests[0].params["cornerRadii"],
            json!([0.0, 2.0, 4.0, 6.0])
        );
        s.execute(requests[0].command, Some(&requests[0].params))
            .expect("rectangle");
        let c = &s.design().objects[0].contours[0];
        assert!(c.points.contains(&v2(100.0, 100.0)) && c.points.contains(&v2(460.0, 300.0)));
        t.set_corner_radii(None);
        assert_eq!(t.corner_radii(), None);
    }
}
