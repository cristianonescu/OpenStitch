//! Shape Builder tool (#497, Shapes ▸ Edit Outline, beyond PE-DESIGN): with two or more
//! overlapping shapes selected, drag across the pieces of their arrangement; on release the
//! pieces the drag touched are merged into one (`shape.build`), or with Alt held removed. One
//! undo step per drag; the pieces become the selection.

use super::{Phase, PointerEvent, ToolContext, ToolController, ToolEffect, ToolPreview};
use crate::editor::request::{CommandRequest, mm};
use openstitch_design::{Contour, ObjectKind};
use serde_json::json;
use std::collections::BTreeSet;

/// The selected closed shapes the builder works on (two or more), in id order.
pub fn builder_targets(ctx: &ToolContext) -> Option<BTreeSet<u64>> {
    let ids: BTreeSet<u64> = ctx
        .selection
        .iter()
        .copied()
        .filter(|&id| {
            ctx.design.index_of(id).is_some_and(|i| {
                let o = &ctx.design.objects[i];
                o.kind == ObjectKind::Shape
                    && !o.contours.is_empty()
                    && o.contours.iter().all(|c| c.closed)
            })
        })
        .collect();
    (ids.len() >= 2).then_some(ids)
}

impl ToolController {
    pub(super) fn builder_tool(&mut self, e: PointerEvent, ctx: &ToolContext) -> Vec<ToolEffect> {
        match e.phase {
            Phase::Down => {
                self.pencil = vec![e.location];
                self.preview = Some(ToolPreview::Outline(Contour::new(
                    self.pencil.clone(),
                    false,
                )));
                vec![]
            }
            Phase::Dragged if !self.pencil.is_empty() => {
                if self
                    .pencil
                    .last()
                    .is_some_and(|q| q.distance(e.location) >= ctx.units_per_point)
                    && self.pencil.len() < 10_000
                {
                    self.pencil.push(e.location);
                }
                self.preview = Some(ToolPreview::Outline(Contour::new(
                    self.pencil.clone(),
                    false,
                )));
                vec![]
            }
            Phase::Up if !self.pencil.is_empty() => {
                let path = std::mem::take(&mut self.pencil);
                self.preview = None;
                let Some(ids) = builder_targets(ctx) else {
                    return vec![];
                };
                let points: Vec<_> = path.iter().map(|p| json!([mm(p.x), mm(p.y)])).collect();
                let mode = if e.alt { "remove" } else { "merge" };
                vec![ToolEffect::run(
                    CommandRequest::new(
                        "shape.build",
                        json!({ "ids": CommandRequest::ids(&ids), "path": points, "mode": mode }),
                    ),
                    true,
                )]
            }
            _ => vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::Tool;
    use super::*;
    use openstitch_design::{Vec2, v2};
    use openstitch_engine::Session;

    fn drag(
        t: &mut ToolController,
        s: &mut Session,
        sel: &BTreeSet<u64>,
        path: &[Vec2],
        alt: bool,
    ) -> BTreeSet<u64> {
        let mut out = BTreeSet::new();
        for (k, &p) in path.iter().enumerate() {
            let phase = match k {
                0 => Phase::Down,
                _ if k + 1 == path.len() => Phase::Up,
                _ => Phase::Dragged,
            };
            let mut e = PointerEvent::new(phase, p);
            e.alt = alt;
            let design = s.design().clone();
            let ctx = ToolContext {
                design: &design,
                selection: sel,
                units_per_point: 1.0,
                scale_takes_origin: false,
            };
            for effect in t.handle(e, &ctx) {
                if let ToolEffect::Execute { requests, .. } = effect {
                    for r in requests {
                        let v = s.execute(r.command, Some(&r.params)).expect("build");
                        out = CommandRequest::created_ids(&v);
                    }
                }
            }
        }
        out
    }

    #[test]
    fn dragging_across_pieces_merges_them_and_alt_removes() {
        let mut s = Session::new(std::env::temp_dir());
        for x in [0, 5] {
            s.execute(
                "shape.rectangle",
                Some(&json!({ "x": x, "y": x, "width": 10, "height": 10 })),
            )
            .expect("rectangle");
        }
        let sel: BTreeSet<u64> = s.design().objects.iter().map(|o| o.id).collect();
        let mut t = ToolController::default();
        t.set_tool(Tool::ShapeBuilder);
        let pieces = drag(
            &mut t,
            &mut s,
            &sel,
            &[v2(20.0, 20.0), v2(50.0, 50.0), v2(70.0, 70.0)],
            false,
        );
        assert_eq!(pieces.len(), 2, "merged piece and the other square's part");
        assert_eq!(s.design().objects.len(), 2);
        // Alt-drag over the merged piece removes it.
        let removed = drag(
            &mut t,
            &mut s,
            &pieces,
            &[v2(20.0, 20.0), v2(25.0, 25.0)],
            true,
        );
        assert_eq!(removed.len(), 1);
        assert_eq!(s.design().objects.len(), 1);
        // Fewer than two shapes selected: nothing happens.
        let one = BTreeSet::from([s.design().objects[0].id]);
        assert!(
            drag(
                &mut t,
                &mut s,
                &one,
                &[v2(120.0, 120.0), v2(130.0, 130.0)],
                false
            )
            .is_empty()
        );
    }
}
