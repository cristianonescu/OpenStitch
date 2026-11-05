//! Continue a finished line, path, satin skeleton or Manual Punch column (#479): with the Pen,
//! Open Straight Line, Running, Straight/Curved Block or Satin Skeleton tool active and exactly
//! one compatible object selected, a click within [`REACH`] screen points of one of its ends
//! starts drawing from that end; Return or a double-click adds the new points to the same
//! object (`object.extendPath` / `object.extendColumn`, one undo step), Esc leaves it as it
//! was. Closed outlines are not offered.

use super::{Tool, ToolContext, ToolController, ToolEffect};
use crate::editor::pen::{PenNode, pen_contour};
use crate::editor::request::{CommandRequest, mm};
use openstitch_design::extend::{End, same_outline};
use openstitch_design::{DesignObject, ObjectKind, Vec2, column, skeleton};
use serde_json::{Value, json};

/// Screen points from an end within which a click continues the object.
pub const REACH: f64 = 6.0;

/// The object being continued and from which end.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Continuation {
    pub id: u64,
    pub end: End,
    /// Where the new points join: the end point of a line or skeleton, the midpoint of the end
    /// direction line of a column.
    pub anchor: Vec2,
}

/// What can be continued: an open single-contour line, a satin skeleton or a column.
enum Kind {
    Line,
    Skeleton,
    Column,
}

fn kind(o: &DesignObject) -> Option<Kind> {
    if o.kind != ObjectKind::Shape || !o.visible {
        return None;
    }
    if let Some(r) = &o.region {
        if let Some(sk) = &r.skeleton
            && skeleton::outline(sk)
                .is_some_and(|c| o.contours.len() == 1 && same_outline(&o.contours[0], &c, 0.05))
        {
            return Some(Kind::Skeleton);
        }
        if r.direction_lines.len() >= 2 && o.contours.len() == 1 {
            let own = if r.curved_lines {
                column::curved_outline(&r.direction_lines)
            } else {
                column::outline(&r.direction_lines)
            };
            if same_outline(&o.contours[0], &own, 0.05) {
                return Some(Kind::Column);
            }
        }
    }
    (o.contours.len() == 1 && !o.contours[0].closed && o.contours[0].points.len() >= 2)
        .then_some(Kind::Line)
}

/// The two ends of `o` for its kind: (start anchor, end anchor).
fn ends(o: &DesignObject, k: &Kind) -> Option<(Vec2, Vec2)> {
    match k {
        Kind::Line => Some((
            *o.contours[0].points.first()?,
            *o.contours[0].points.last()?,
        )),
        Kind::Skeleton => {
            let sk = o.region.as_ref()?.skeleton.as_ref()?;
            Some((*sk.points.first()?, *sk.points.last()?))
        }
        Kind::Column => {
            let l = &o.region.as_ref()?.direction_lines;
            let mid = |r: &[Vec2; 2]| r[0].lerp(r[1], 0.5);
            Some((mid(l.first()?), mid(l.last()?)))
        }
    }
}

/// Distance from `p` to the end of `o` an anchor stands for (a column's end is its whole
/// direction line).
fn reach(o: &DesignObject, k: &Kind, end: End, anchor: Vec2, p: Vec2) -> f64 {
    match (k, o.region.as_ref()) {
        (Kind::Column, Some(r)) => {
            let l = match end {
                End::Start => r.direction_lines[0],
                End::End => r.direction_lines[r.direction_lines.len() - 1],
            };
            segment_distance(p, l[0], l[1])
        }
        _ => anchor.distance(p),
    }
}

/// Distance from `p` to segment a–b.
fn segment_distance(p: Vec2, a: Vec2, b: Vec2) -> f64 {
    let d = b - a;
    let len2 = d.x * d.x + d.y * d.y;
    if len2 == 0.0 {
        return p.distance(a);
    }
    let t = (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0);
    p.distance(a + d * t)
}

fn point(p: Vec2) -> Value {
    json!([mm(p.x), mm(p.y)])
}

impl ToolController {
    /// The object and end a click at `p` would continue, if the active tool can continue the
    /// single selected object and `p` is within reach of one of its ends.
    pub fn continue_target(&self, ctx: &ToolContext, p: Vec2) -> Option<Continuation> {
        if self.is_busy() || ctx.selection.len() != 1 {
            return None;
        }
        let id = *ctx.selection.first()?;
        let o = ctx.design.objects.iter().find(|o| o.id == id)?;
        let k = kind(o)?;
        let fits = match self.tool() {
            Tool::Pen | Tool::Line | Tool::Running => matches!(k, Kind::Line),
            Tool::Punch | Tool::CurvedBlock => matches!(k, Kind::Column),
            Tool::Skeleton => matches!(k, Kind::Skeleton),
            _ => false,
        };
        if !fits {
            return None;
        }
        let (a, b) = ends(o, &k)?;
        let limit = REACH * ctx.units_per_point;
        [(End::Start, a), (End::End, b)]
            .into_iter()
            .map(|(end, anchor)| (reach(o, &k, end, anchor, p), end, anchor))
            .filter(|(d, ..)| *d <= limit)
            .min_by(|x, y| x.0.total_cmp(&y.0))
            .map(|(_, end, anchor)| Continuation { id, end, anchor })
    }

    /// The object being continued, if a continuation is in progress.
    pub fn continuation(&self) -> Option<Continuation> {
        self.continuing
    }

    /// Starts continuing at a click `p`, if it is on an end of the selected object. The line
    /// tools start their new points from the end itself.
    pub(super) fn start_continuation(&mut self, ctx: &ToolContext, p: Vec2) -> bool {
        let Some(c) = self.continue_target(ctx, p) else {
            return false;
        };
        self.continuing = Some(c);
        match self.tool() {
            Tool::Pen => self.pen.push(PenNode::corner(c.anchor)),
            Tool::Line | Tool::Running => self.polygon_or_punch_push(c.anchor),
            Tool::Skeleton => {
                let width = ctx
                    .design
                    .objects
                    .iter()
                    .find(|o| o.id == c.id)
                    .and_then(|o| o.region.as_ref()?.skeleton.as_ref())
                    .and_then(|sk| match c.end {
                        End::Start => sk.widths.first().copied(),
                        End::End => sk.widths.last().copied(),
                    })
                    .unwrap_or_else(|| self.skeleton_width());
                self.pen.push(PenNode::corner(c.anchor));
                self.widths.push(width);
            }
            _ => {}
        }
        true
    }

    fn polygon_or_punch_push(&mut self, p: Vec2) {
        if self.tool() == Tool::Running {
            self.punch.push(p);
        } else {
            self.polygon.push(p);
        }
    }

    fn end_id(end: End) -> &'static str {
        match end {
            End::Start => "start",
            End::End => "end",
        }
    }

    /// `object.extendPath` for the points after the anchor (`points[0]` is the anchor), or
    /// nothing when no point was added.
    pub(super) fn extend_line(&mut self, c: Continuation, points: &[Vec2]) -> Vec<ToolEffect> {
        self.continuing = None;
        if points.len() < 2 {
            return vec![];
        }
        let new: Vec<Value> = points[1..].iter().map(|&p| point(p)).collect();
        vec![ToolEffect::run(
            CommandRequest::new(
                "object.extendPath",
                json!({ "id": c.id, "end": Self::end_id(c.end), "points": new }),
            ),
            false,
        )]
    }

    /// Pen: the nodes from the anchor on, curves included.
    pub(super) fn extend_pen(&mut self, c: Continuation, nodes: &[PenNode]) -> Vec<ToolEffect> {
        self.continuing = None;
        if nodes.len() < 2 {
            return vec![];
        }
        let contour = pen_contour(nodes, false);
        let mut params = json!({
            "id": c.id,
            "end": Self::end_id(c.end),
            "points": contour.points[1..].iter().map(|&p| point(p)).collect::<Vec<_>>(),
        });
        if contour.has_curves() {
            params["curves"] = (0..contour.segment_count())
                .map(|i| {
                    contour
                        .curve(i)
                        .map_or(Value::Null, |z| json!([point(z.c1), point(z.c2)]))
                })
                .collect();
        }
        vec![ToolEffect::run(
            CommandRequest::new("object.extendPath", params),
            false,
        )]
    }

    /// Satin Skeleton: the flattened centre line after the anchor, with its widths.
    pub(super) fn extend_skeleton(
        &mut self,
        c: Continuation,
        nodes: &[PenNode],
        widths: &[f64],
    ) -> Vec<ToolEffect> {
        self.continuing = None;
        if nodes.len() < 2 {
            return vec![];
        }
        let flat = super::skeleton::flatten(nodes, widths);
        vec![ToolEffect::run(
            CommandRequest::new(
                "object.extendPath",
                json!({
                    "id": c.id,
                    "end": Self::end_id(c.end),
                    "points": flat.points[1..].iter().map(|&p| point(p)).collect::<Vec<_>>(),
                    "widths": flat.widths[1..].iter().map(|w| json!((w * 100.0).round() / 100.0)).collect::<Vec<_>>(),
                }),
            ),
            false,
        )]
    }

    /// Straight or Curved Block: the complete line pairs clicked become new direction lines.
    pub(super) fn extend_column(&mut self, c: Continuation, clicks: &[Vec2]) -> Vec<ToolEffect> {
        self.continuing = None;
        let lines: Vec<Value> = clicks
            .as_chunks::<2>()
            .0
            .iter()
            .map(|w| json!([point(w[0]), point(w[1])]))
            .collect();
        if lines.is_empty() {
            return vec![];
        }
        vec![ToolEffect::run(
            CommandRequest::new(
                "object.extendColumn",
                json!({ "id": c.id, "end": Self::end_id(c.end), "lines": lines }),
            ),
            false,
        )]
    }
}
