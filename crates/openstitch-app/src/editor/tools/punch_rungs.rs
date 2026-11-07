//! Point tool on a Manual Punch column (#525, beyond PE-DESIGN): the column's rungs (direction
//! lines) are edited instead of its outline, which follows them. Drag a rung's end to move it
//! (`object.moveRung`), double-click a side of the column to add a rung there
//! (`object.insertRung`), click a rung's end to select it and press Delete to remove it
//! (`object.deleteRung`). Each is one command, so one Undo; a move that would twist the column
//! is refused by the engine and leaves it as it was.

use super::{DRAG_THRESHOLD, Gesture, PointerEvent, ToolContext, ToolController, ToolEffect};
use crate::editor::hit::object_at;
use crate::editor::request::{CommandRequest, mm};
use crate::editor::tools::Phase;
use openstitch_design::{DesignObject, Vec2, column};
use serde_json::json;
use std::collections::BTreeSet;

/// What the canvas draws for the Point tool's column: its rungs (as being dragged) and the
/// selected one.
#[derive(Clone, Debug, PartialEq)]
pub struct RungOverlay {
    pub id: u64,
    pub rungs: Vec<[Vec2; 2]>,
    pub selected: Option<usize>,
}

/// The rungs of `o` when it is a Manual Punch column: a region with two or more direction
/// lines whose single outline is the one those lines make (a shape with direction lines added
/// to its own outline is not a column).
pub fn rungs(o: &DesignObject) -> Option<&[[Vec2; 2]]> {
    let r = o.region.as_ref()?;
    if r.direction_lines.len() < 2 || o.contours.len() != 1 {
        return None;
    }
    let made = if r.curved_lines {
        column::curved_outline(&r.direction_lines)
    } else {
        column::outline(&r.direction_lines)
    };
    (made.points == o.contours[0].points).then_some(&r.direction_lines)
}

/// The rung end nearest `p` within `radius`: (rung index, side 0 = left, 1 = right).
pub fn rung_end_at(rungs: &[[Vec2; 2]], p: Vec2, radius: f64) -> Option<(usize, usize)> {
    rungs
        .iter()
        .enumerate()
        .flat_map(|(i, r)| [(i, 0, r[0]), (i, 1, r[1])])
        .filter(|(.., q)| q.distance(p) <= radius)
        .min_by(|a, b| a.2.distance(p).total_cmp(&b.2.distance(p)))
        .map(|(i, side, _)| (i, side))
}

/// Whether `p` lies within `tolerance` of a side of the column (between two rungs' ends).
fn on_side(rungs: &[[Vec2; 2]], p: Vec2, tolerance: f64) -> bool {
    (0..2).any(|side| {
        rungs.windows(2).any(|w| {
            let (a, b) = (w[0][side], w[1][side]);
            let d = b - a;
            let l2 = d.x * d.x + d.y * d.y;
            let t = if l2 > 0.0 {
                (((p - a).x * d.x + (p - a).y * d.y) / l2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (a + d * t).distance(p) <= tolerance
        })
    })
}

impl ToolContext<'_> {
    /// The Manual Punch column the Point tool edits: the selection when it is exactly one
    /// visible column.
    pub fn rung_target(&self) -> Option<&DesignObject> {
        if self.selection.len() != 1 {
            return None;
        }
        let id = *self.selection.first()?;
        self.design
            .objects
            .iter()
            .find(|o| o.id == id && o.visible && rungs(o).is_some())
    }
}

impl ToolController {
    /// The selected rung of the Point tool's column, if it still exists.
    pub fn selected_rung(&self, ctx: &ToolContext) -> Option<usize> {
        let (id, i) = self.rung?;
        let o = ctx.rung_target().filter(|o| o.id == id)?;
        (i < rungs(o)?.len()).then_some(i)
    }

    pub fn rung_overlay(&self, ctx: &ToolContext) -> Option<RungOverlay> {
        if self.tool() != super::Tool::Point {
            return None;
        }
        let o = ctx.rung_target()?;
        let mut lines = rungs(o)?.to_vec();
        if let Some(Gesture::Rung {
            id,
            index,
            side,
            to: Some(to),
            ..
        }) = &self.gesture
            && *id == o.id
            && let Some(r) = lines.get_mut(*index)
        {
            r[*side] = *to;
        }
        Some(RungOverlay {
            id: o.id,
            rungs: lines,
            selected: self.selected_rung(ctx),
        })
    }

    pub(super) fn rung_tool(
        &mut self,
        e: PointerEvent,
        ctx: &ToolContext,
        o: &DesignObject,
    ) -> Vec<ToolEffect> {
        let p = e.location;
        let Some(lines) = rungs(o) else {
            return vec![];
        };
        match (e.phase, self.gesture.clone()) {
            (Phase::Down, _) => {
                if let Some((index, side)) = rung_end_at(lines, p, ctx.point_radius()) {
                    self.rung = Some((o.id, index));
                    self.gesture = Some(Gesture::Rung {
                        id: o.id,
                        index,
                        side,
                        start: p,
                        to: None,
                    });
                    return vec![];
                }
                if e.clicks >= 2 && on_side(lines, p, ctx.tolerance()) {
                    self.rung = None;
                    return vec![ToolEffect::run(
                        CommandRequest::new(
                            "object.insertRung",
                            json!({ "id": o.id, "at": [mm(p.x), mm(p.y)] }),
                        ),
                        false,
                    )];
                }
                self.rung = None;
                match object_at(ctx.design, p, ctx.tolerance()) {
                    Some(id) if id == o.id => vec![],
                    Some(id) => vec![ToolEffect::Select(BTreeSet::from([id]))],
                    None => vec![ToolEffect::Select(BTreeSet::new())],
                }
            }
            (
                Phase::Dragged,
                Some(Gesture::Rung {
                    id,
                    index,
                    side,
                    start,
                    ..
                }),
            ) => {
                let moved = p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point;
                self.gesture = Some(Gesture::Rung {
                    id,
                    index,
                    side,
                    start,
                    to: moved.then_some(p),
                });
                vec![]
            }
            (
                Phase::Up,
                Some(Gesture::Rung {
                    id,
                    index,
                    side,
                    to,
                    ..
                }),
            ) => {
                self.gesture = None;
                match to {
                    Some(to) => vec![ToolEffect::run(
                        CommandRequest::new(
                            "object.moveRung",
                            json!({
                                "id": id,
                                "index": index,
                                "side": if side == 0 { "left" } else { "right" },
                                "point": [mm(to.x), mm(to.y)],
                            }),
                        ),
                        false,
                    )],
                    None => vec![],
                }
            }
            _ => vec![],
        }
    }

    /// Delete with the Point tool on a column: removes the selected rung.
    pub(super) fn delete_rung(&mut self, ctx: &ToolContext) -> Option<Vec<ToolEffect>> {
        let o = ctx.rung_target()?;
        let index = self.selected_rung(ctx)?;
        self.rung = None;
        Some(vec![ToolEffect::run(
            CommandRequest::new("object.deleteRung", json!({ "id": o.id, "index": index })),
            false,
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Tool;
    use openstitch_design::v2;
    use openstitch_engine::Session;
    use openstitch_testutil::TempDir;

    fn ev(phase: Phase, at: Vec2, clicks: u32) -> PointerEvent {
        let mut e = PointerEvent::new(phase, at);
        e.clicks = clicks;
        e
    }

    fn run_all(s: &mut Session, effects: Vec<ToolEffect>) {
        for e in effects {
            if let ToolEffect::Execute { requests, .. } = e {
                for r in requests {
                    s.execute(r.command, Some(&r.params)).expect("command");
                }
            }
        }
    }

    fn lines(s: &Session, id: u64) -> Vec<[Vec2; 2]> {
        let d = s.design();
        d.objects[d.index_of(id).expect("column")]
            .region
            .as_ref()
            .expect("region")
            .direction_lines
            .clone()
    }

    #[test]
    fn rungs_are_dragged_added_and_deleted() {
        let tmp = TempDir::new("punch-rungs");
        let mut s = Session::new(tmp.path());
        // A straight column of three rungs, 10 mm apart.
        let r = s
            .execute(
                "shape.column",
                Some(&json!({"lines": [[[10, 10], [10, 16]], [[20, 10], [20, 16]], [[30, 10], [30, 16]]]})),
            )
            .expect("column");
        let id = r["id"].as_u64().expect("id");
        let selection = BTreeSet::from([id]);
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        let step = |s: &Session, t: &mut ToolController, e: PointerEvent| {
            let design = s.design().clone();
            let ctx = ToolContext {
                design: &design,
                selection: &selection,
                units_per_point: 0.5,
                scale_takes_origin: true,
            };
            t.handle(e, &ctx)
        };

        // Drag the middle rung's second end 2 mm down: one moveRung.
        assert!(step(&s, &mut t, ev(Phase::Down, v2(200.0, 160.0), 1)).is_empty());
        assert!(step(&s, &mut t, ev(Phase::Dragged, v2(200.0, 180.0), 1)).is_empty());
        let e = step(&s, &mut t, ev(Phase::Up, v2(200.0, 180.0), 1));
        run_all(&mut s, e);
        assert_eq!(lines(&s, id)[1][1], v2(200.0, 180.0));
        assert_eq!(lines(&s, id).len(), 3);

        // Double-click on a side between the first two rungs adds a rung there.
        let e = step(&s, &mut t, ev(Phase::Down, v2(150.0, 100.0), 2));
        run_all(&mut s, e);
        assert_eq!(lines(&s, id).len(), 4);

        // Click the first rung's end to select it; Delete removes it.
        let _ = step(&s, &mut t, ev(Phase::Down, v2(100.0, 100.0), 1));
        let _ = step(&s, &mut t, ev(Phase::Up, v2(100.0, 100.0), 1));
        let design = s.design().clone();
        let ctx = ToolContext {
            design: &design,
            selection: &selection,
            units_per_point: 0.5,
            scale_takes_origin: true,
        };
        let e = t.delete_point(&ctx).expect("a rung is selected");
        run_all(&mut s, e);
        assert_eq!(lines(&s, id).len(), 3);

        // The overlay shows the rungs, not the outline's vertices.
        let design = s.design().clone();
        let ctx = ToolContext {
            design: &design,
            selection: &selection,
            units_per_point: 0.5,
            scale_takes_origin: true,
        };
        assert_eq!(t.rung_overlay(&ctx).expect("overlay").rungs.len(), 3);
        assert!(t.point_overlay(&ctx).is_none());
    }

    #[test]
    fn a_shape_with_direction_lines_is_not_a_column() {
        let tmp = TempDir::new("punch-rungs");
        let mut s = Session::new(tmp.path());
        s.execute(
            "shape.rectangle",
            Some(&json!({"x": 0, "y": 0, "width": 20, "height": 10, "fill": "#ff0000"})),
        )
        .expect("rectangle");
        let mut d = s.design().clone();
        d.objects[0].region.as_mut().expect("fill").direction_lines = vec![
            [v2(50.0, 0.0), v2(50.0, 100.0)],
            [v2(150.0, 0.0), v2(150.0, 100.0)],
        ];
        assert!(rungs(&d.objects[0]).is_none());
    }
}
