//! Angle Line tool (#534, beyond PE-DESIGN): drag (or click twice, or the arrow keys and Space)
//! a line across the selected fill; it is added to the fill's stitch angle lines with one
//! `object.setAngleLines` (the fill's lines so far plus the new one), so one undo step per
//! line, and the rows turn from line to line once there are two. Draw the lines in order
//! along the shape. Shift: 15° steps. Lines the engine refuses (crossing ones) leave the fill
//! as it was and say why. The tool stays active; Esc drops the line in progress, and with none
//! it goes back to Select.

use super::fill_angle::constrain_15;
use super::{Phase, PointerEvent, ToolContext, ToolController, ToolEffect, ToolPreview};
use crate::editor::request::{CommandRequest, mm};
use openstitch_design::{Contour, Design, RegionType, Vec2};
use serde_json::json;
use std::collections::BTreeSet;

/// The fill the tool adds lines to: the selection when it is exactly one fill.
pub fn angle_line_target(design: &Design, selection: &BTreeSet<u64>) -> Option<u64> {
    let mut picked = design.objects.iter().filter(|o| selection.contains(&o.id));
    let o = picked.next()?;
    if picked.next().is_some() {
        return None;
    }
    o.region
        .as_ref()
        .filter(|r| r.kind == RegionType::Fill && r.gradient.is_none())
        .map(|_| o.id)
}

impl ToolController {
    fn angle_line_point(&self, e: &PointerEvent) -> Vec2 {
        match self.polygon.first() {
            Some(&start) if e.shift => constrain_15(start, e.location),
            _ => e.location,
        }
    }

    pub(super) fn angle_line_tool(
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
                let end = self.angle_line_point(&e);
                self.finish_angle_line(end, ctx)
            }
            Phase::Moved | Phase::Dragged => {
                if let Some(&start) = self.polygon.first() {
                    let end = self.angle_line_point(&e);
                    self.preview =
                        Some(ToolPreview::Outline(Contour::new(vec![start, end], false)));
                }
                vec![]
            }
            Phase::Up => match self.polygon.first() {
                Some(&start) if start.distance(e.location) > ctx.tolerance() => {
                    let end = self.angle_line_point(&e);
                    self.finish_angle_line(end, ctx)
                }
                _ => vec![],
            },
        }
    }

    /// Adds the line to the selected fill's angle lines; with one line so far nothing is sewn
    /// differently yet (the rows need two), so the first line is kept by the tool until the
    /// second is drawn.
    fn finish_angle_line(&mut self, end: Vec2, ctx: &ToolContext) -> Vec<ToolEffect> {
        let start = self.polygon.first().copied();
        self.polygon.clear();
        self.preview = None;
        let Some(start) = start.filter(|s| s.distance(end) > ctx.tolerance()) else {
            return vec![];
        };
        let Some(id) = angle_line_target(ctx.design, ctx.selection) else {
            return vec![];
        };
        let existing: Vec<[Vec2; 2]> = ctx
            .design
            .objects
            .iter()
            .find(|o| o.id == id)
            .and_then(|o| o.region.as_ref())
            .map(|r| r.angle_lines.clone())
            .unwrap_or_default();
        let mut lines: Vec<[Vec2; 2]> = if existing.is_empty() {
            self.angle_lines_pending.clone()
        } else {
            existing
        };
        lines.push([start, end]);
        if lines.len() < 2 {
            // The first line waits for the second (the engine needs two).
            self.angle_lines_pending = lines;
            return vec![];
        }
        self.angle_lines_pending.clear();
        let point = |p: Vec2| json!([mm(p.x), mm(p.y)]);
        let lines: Vec<_> = lines
            .iter()
            .map(|l| json!([point(l[0]), point(l[1])]))
            .collect();
        vec![ToolEffect::run(
            CommandRequest::new("object.setAngleLines", json!({ "id": id, "lines": lines })),
            false,
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::super::Tool;
    use super::*;
    use openstitch_design::v2;
    use openstitch_engine::Session;

    fn send(
        t: &mut ToolController,
        s: &mut Session,
        sel: &BTreeSet<u64>,
        e: PointerEvent,
    ) -> usize {
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
        let mut n = 0;
        for e in effects {
            if let ToolEffect::Execute { requests, .. } = e {
                for r in requests {
                    let _ = s.execute(r.command, Some(&r.params));
                    n += 1;
                }
            }
        }
        n
    }

    fn drag(
        t: &mut ToolController,
        s: &mut Session,
        sel: &BTreeSet<u64>,
        a: Vec2,
        b: Vec2,
    ) -> usize {
        send(t, s, sel, PointerEvent::new(Phase::Down, a));
        send(t, s, sel, PointerEvent::new(Phase::Dragged, a.lerp(b, 0.5)));
        send(t, s, sel, PointerEvent::new(Phase::Up, b))
    }

    fn lines(s: &Session) -> usize {
        s.design().objects[0]
            .region
            .as_ref()
            .map_or(0, |r| r.angle_lines.len())
    }

    #[test]
    fn each_drag_adds_an_angle_line_to_the_selected_fill() {
        let mut s = Session::new(std::env::temp_dir());
        // An L (mm).
        s.execute(
            "shape.polygon",
            Some(
                &json!({"points": [[0, 0], [60, 0], [60, 20], [20, 20], [20, 60], [0, 60]],
                "closed": true, "fill": "Red"}),
            ),
        )
        .expect("L");
        let sel = BTreeSet::from([1]);
        let mut t = ToolController::default();
        t.set_tool(Tool::AngleLine);
        // The first line waits for the second; the second sets both in one command.
        assert_eq!(
            drag(&mut t, &mut s, &sel, v2(400.0, -20.0), v2(400.0, 220.0)),
            0
        );
        assert_eq!(lines(&s), 0);
        assert_eq!(
            drag(&mut t, &mut s, &sel, v2(220.0, 220.0), v2(-20.0, -20.0)),
            1
        );
        assert_eq!(lines(&s), 2);
        // A third adds to them, one undo step each.
        assert_eq!(
            drag(&mut t, &mut s, &sel, v2(-20.0, 400.0), v2(220.0, 400.0)),
            1
        );
        assert_eq!(lines(&s), 3);
        s.execute("history.undo", None).expect("undo");
        assert_eq!(lines(&s), 2);
        // A line crossing another inside the shape is refused: nothing changes.
        drag(&mut t, &mut s, &sel, v2(300.0, -20.0), v2(500.0, 220.0));
        assert_eq!(lines(&s), 2);
        assert_eq!(t.tool(), Tool::AngleLine);
        // Nothing selected: nothing happens.
        let none = BTreeSet::new();
        assert_eq!(
            drag(&mut t, &mut s, &none, v2(-20.0, 500.0), v2(220.0, 500.0)),
            0
        );
    }
}
