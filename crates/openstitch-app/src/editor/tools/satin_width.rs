//! Point tool on a satin line (#700, the canvas part of #524, beyond PE-DESIGN): each width key
//! shows a handle at both edges of the column. Drag a handle to set that side's reach
//! (`object.setWidthKey {left | right}`), double-click the line to add a key there at the
//! width the line has now, click a handle and press Delete to remove its key
//! (`object.deleteWidthKey`). Each is one command, so one Undo.

use super::{DRAG_THRESHOLD, Gesture, PointerEvent, ToolContext, ToolController, ToolEffect};
use crate::editor::hit::object_at;
use crate::editor::request::CommandRequest;
use crate::editor::tools::Phase;
use openstitch_design::fmath::Det;
use openstitch_design::skeleton::SatinLine;
use openstitch_design::{DesignObject, LineType, ObjectKind, Vec2, v2};
use serde_json::json;
use std::collections::BTreeSet;

/// Model units (0.1 mm) per mm.
const MM: f64 = 10.0;

/// One width key as the canvas shows it: where it sits on the line and its two edge handles.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct WidthHandle {
    /// Position of the key in the line's `width_keys`.
    pub index: usize,
    pub at: f64,
    pub centre: Vec2,
    /// Left and right edge.
    pub ends: [Vec2; 2],
}

/// What the canvas draws for the Point tool's satin line.
#[derive(Clone, Debug, PartialEq)]
pub struct WidthOverlay {
    pub id: u64,
    pub handles: Vec<WidthHandle>,
    pub selected: Option<usize>,
}

/// The line a satin line object sews along, when `o` is one: its first contour's path.
fn satin_path(o: &DesignObject) -> Option<Vec<Vec2>> {
    let l = o.line.as_ref()?;
    if o.kind != ObjectKind::Shape || l.kind != LineType::Satin {
        return None;
    }
    let p = o.contours.first()?.path();
    (p.len() >= 2).then_some(p)
}

/// The point and unit tangent at fraction `f` of the length of `path`.
fn station(path: &[Vec2], f: f64) -> (Vec2, Vec2) {
    let lens: Vec<f64> = path.windows(2).map(|w| w[0].distance(w[1])).collect();
    let total: f64 = lens.iter().sum();
    let mut left = (f.clamp(0.0, 1.0) * total).max(0.0);
    for (i, &l) in lens.iter().enumerate() {
        if left <= l || i + 1 == lens.len() {
            let (a, b) = (path[i], path[i + 1]);
            let t = if l > 0.0 {
                (b - a) * (1.0 / l)
            } else {
                v2(1.0, 0.0)
            };
            return (
                a.lerp(b, if l > 0.0 { (left / l).min(1.0) } else { 0.0 }),
                t,
            );
        }
        left -= l;
    }
    (path[0], v2(1.0, 0.0))
}

/// The fraction of the length of `path` at its point nearest `p`, and that distance.
fn nearest_fraction(path: &[Vec2], p: Vec2) -> (f64, f64) {
    let total: f64 = path.windows(2).map(|w| w[0].distance(w[1])).sum();
    let mut best = (0.0, f64::INFINITY);
    let mut run = 0.0;
    for w in path.windows(2) {
        let (a, b) = (w[0], w[1]);
        let d = b - a;
        let l2 = d.x * d.x + d.y * d.y;
        let t = if l2 > 0.0 {
            (((p - a).x * d.x + (p - a).y * d.y) / l2).clamp(0.0, 1.0)
        } else {
            0.0
        };
        let dist = (a + d * t).distance(p);
        if dist < best.1 {
            best = ((run + t * l2.sqrt()) / total.max(1e-9), dist);
        }
        run += l2.sqrt();
    }
    best
}

/// The direction a key's edges lie in from the line at fraction `f`, scaled for the slant
/// (as `skeleton::satin_line_rungs` places them): left edge = centre + dir × left.
fn edge_dir(t: Vec2, slant: f64) -> Vec2 {
    let a = slant.clamp(-60.0, 60.0).to_radians();
    let (sin, cos) = (a.dsin(), a.dcos());
    let left = v2(t.y, -t.x);
    (left * cos + t * sin) * (MM / cos)
}

/// The width handles of `o` when it is a satin line.
pub fn width_handles(o: &DesignObject) -> Option<Vec<WidthHandle>> {
    let path = satin_path(o)?;
    let l = o.line.as_ref()?;
    let slant = l.slant.unwrap_or(0.0);
    Some(
        l.width_keys
            .iter()
            .enumerate()
            .map(|(index, k)| {
                let (c, t) = station(&path, k.at);
                let dir = edge_dir(t, slant);
                WidthHandle {
                    index,
                    at: k.at,
                    centre: c,
                    ends: [c + dir * k.left, c - dir * k.right],
                }
            })
            .collect(),
    )
}

impl ToolContext<'_> {
    /// The satin line the Point tool edits: the selection when it is exactly one visible
    /// shape sewn as a satin line.
    pub fn satin_target(&self) -> Option<&DesignObject> {
        if self.selection.len() != 1 {
            return None;
        }
        let id = *self.selection.first()?;
        self.design
            .objects
            .iter()
            .find(|o| o.id == id && o.visible && satin_path(o).is_some())
    }
}

impl ToolController {
    /// The selected width key of the Point tool's satin line, if it still exists.
    pub fn selected_width_key(&self, ctx: &ToolContext) -> Option<usize> {
        let (id, i) = self.width_key?;
        let o = ctx.satin_target().filter(|o| o.id == id)?;
        (i < o.line.as_ref()?.width_keys.len()).then_some(i)
    }

    pub fn width_overlay(&self, ctx: &ToolContext) -> Option<WidthOverlay> {
        if self.tool() != super::Tool::Point {
            return None;
        }
        let o = ctx.satin_target()?;
        let mut handles = width_handles(o)?;
        if let Some(Gesture::Width {
            id,
            index,
            side,
            to: Some(to),
            ..
        }) = &self.gesture
            && *id == o.id
            && let Some(h) = handles.get_mut(*index)
        {
            h.ends[*side] = *to;
        }
        Some(WidthOverlay {
            id: o.id,
            handles,
            selected: self.selected_width_key(ctx),
        })
    }

    pub(super) fn width_tool(
        &mut self,
        e: PointerEvent,
        ctx: &ToolContext,
        o: &DesignObject,
    ) -> Vec<ToolEffect> {
        let p = e.location;
        let (Some(handles), Some(path), Some(line)) =
            (width_handles(o), satin_path(o), o.line.as_ref())
        else {
            return vec![];
        };
        match (e.phase, self.gesture.clone()) {
            (Phase::Down, _) => {
                let hit = handles
                    .iter()
                    .flat_map(|h| [(h.index, 0, h.ends[0]), (h.index, 1, h.ends[1])])
                    .filter(|(.., q)| q.distance(p) <= ctx.point_radius())
                    .min_by(|a, b| a.2.distance(p).total_cmp(&b.2.distance(p)));
                if let Some((index, side, _)) = hit {
                    self.width_key = Some((o.id, index));
                    self.gesture = Some(Gesture::Width {
                        id: o.id,
                        index,
                        side,
                        start: p,
                        to: None,
                    });
                    return vec![];
                }
                let (f, dist) = nearest_fraction(&path, p);
                if e.clicks >= 2 && dist <= ctx.tolerance().max(line.width * MM / 2.0) {
                    let sides = SatinLine {
                        width: line.width,
                        keys: line.width_keys.clone(),
                        ..Default::default()
                    }
                    .sides(f, 1.0);
                    self.width_key = None;
                    return vec![ToolEffect::run(
                        CommandRequest::new(
                            "object.setWidthKey",
                            json!({ "id": o.id, "at": round(f), "left": round(sides[0]), "right": round(sides[1]) }),
                        ),
                        false,
                    )];
                }
                self.width_key = None;
                match object_at(ctx.design, p, ctx.tolerance()) {
                    Some(id) if id == o.id => vec![],
                    Some(id) => vec![ToolEffect::Select(BTreeSet::from([id]))],
                    None => vec![ToolEffect::Select(BTreeSet::new())],
                }
            }
            (
                Phase::Dragged,
                Some(Gesture::Width {
                    id,
                    index,
                    side,
                    start,
                    ..
                }),
            ) => {
                let moved = p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point;
                self.gesture = Some(Gesture::Width {
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
                Some(Gesture::Width {
                    id,
                    index,
                    side,
                    to,
                    ..
                }),
            ) => {
                self.gesture = None;
                let (Some(to), Some(h)) = (to, handles.get(index)) else {
                    return vec![];
                };
                // The reach along the edge direction, in mm (0 at the line, never negative).
                let (_, t) = station(&path, h.at);
                let dir = edge_dir(t, line.slant.unwrap_or(0.0));
                let along = if side == 0 { dir } else { dir * -1.0 };
                let l2 = along.x * along.x + along.y * along.y;
                let reach = (((to - h.centre).x * along.x + (to - h.centre).y * along.y) / l2)
                    .clamp(0.0, openstitch_design::skeleton::MAX_WIDTH / 2.0);
                let key = if side == 0 { "left" } else { "right" };
                vec![ToolEffect::run(
                    CommandRequest::new(
                        "object.setWidthKey",
                        json!({ "id": id, "at": h.at, key: round(reach) }),
                    ),
                    false,
                )]
            }
            _ => vec![],
        }
    }

    /// Delete with the Point tool on a satin line: removes the selected width key.
    pub(super) fn delete_width_key(&mut self, ctx: &ToolContext) -> Option<Vec<ToolEffect>> {
        let o = ctx.satin_target()?;
        let index = self.selected_width_key(ctx)?;
        self.width_key = None;
        Some(vec![ToolEffect::run(
            CommandRequest::new(
                "object.deleteWidthKey",
                json!({ "id": o.id, "index": index }),
            ),
            false,
        )])
    }
}

/// Three decimals: commands carry tidy numbers.
fn round(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Tool;
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

    #[test]
    fn width_keys_are_added_dragged_and_deleted() {
        let tmp = TempDir::new("satin-width");
        let mut s = Session::new(tmp.path());
        // A straight 40 mm line along y = 20 mm, sewn as a 4 mm satin line.
        let id = s
            .execute(
                "shape.path",
                Some(
                    &json!({"points": [[10, 20], [50, 20]], "closed": false, "outline": "#ff0000"}),
                ),
            )
            .expect("line")["id"]
            .as_u64()
            .expect("id");
        s.execute(
            "object.setLine",
            Some(&json!({"ids": [id], "type": "satin", "width": 4})),
        )
        .expect("satin line");
        let selection = BTreeSet::from([id]);
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        let keys = |s: &Session| {
            let d = s.design();
            d.objects[d.index_of(id).expect("line")]
                .line
                .as_ref()
                .expect("line")
                .width_keys
                .clone()
        };
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

        // Double-click the line halfway: a key there at the line's width now (2 mm a side).
        let e = step(&s, &mut t, ev(Phase::Down, v2(300.0, 200.0), 2));
        run_all(&mut s, e);
        let k = keys(&s);
        assert_eq!(k.len(), 1);
        assert!((k[0].at - 0.5).abs() < 1e-3 && (k[0].left - 2.0).abs() < 1e-6);

        // Drag one edge handle 3 mm further out: that side becomes 5 mm, the other stays.
        let design = s.design().clone();
        let o = &design.objects[design.index_of(id).expect("line")];
        let h = width_handles(o).expect("satin line")[0];
        let out = h.ends[0] + (h.ends[0] - h.centre) * (3.0 / 2.0);
        assert!(step(&s, &mut t, ev(Phase::Down, h.ends[0], 1)).is_empty());
        assert!(step(&s, &mut t, ev(Phase::Dragged, out, 1)).is_empty());
        let e = step(&s, &mut t, ev(Phase::Up, out, 1));
        run_all(&mut s, e);
        let k = keys(&s);
        assert!((k[0].left - 5.0).abs() < 1e-3, "{k:?}");
        assert!((k[0].right - 2.0).abs() < 1e-6);

        // The handle stays selected; Delete removes the key.
        let design = s.design().clone();
        let ctx = ToolContext {
            design: &design,
            selection: &selection,
            units_per_point: 0.5,
            scale_takes_origin: true,
        };
        assert_eq!(t.width_overlay(&ctx).expect("overlay").handles.len(), 1);
        let e = t.delete_point(&ctx).expect("a key is selected");
        run_all(&mut s, e);
        assert!(keys(&s).is_empty());
    }
}
