//! Point tool on a stitch pattern, as PE-DESIGN's Select Point (#1272; its instruction manual,
//! captures k2 to k14 pending): needle points (the pattern's stitch and jump entries) are
//! selected by click, Ctrl+click (toggle), Shift+click (add), a drag box, a click on a jump
//! (both its ends), Tab and Shift+Tab (a colour's points), Home and End (a colour's entry and
//! exit point; with Ctrl the pattern's), Ctrl+arrow and Shift+arrow; dragged or moved with the
//! arrow keys as one `stitches.movePoints`; deleted with Delete (`stitches.deletePoints`);
//! one click on a stitch inserts a point there, Ctrl+Shift+click as a jump
//! (`stitches.insertPoint`); Alt+drag of the entry or exit point onto another pattern's exit
//! or entry point connects the two (`stitches.connect`). Indexes are positions in the
//! pattern's stitch list.

use super::{
    DRAG_THRESHOLD, Gesture, PointerEvent, ToolContext, ToolController, ToolEffect, ToolPreview,
    move_delta,
};
use crate::editor::hit::{object_at, rect_from};
use crate::editor::request::{CommandRequest, mm};
use crate::editor::tools::Phase;
use openstitch_design::stitch_edit::is_point;
use openstitch_design::{DesignObject, Vec2, v2};
use serde_json::json;
use std::collections::BTreeSet;

/// What the canvas draws for the needle points of the Point tool's stitch pattern.
#[derive(Clone, Debug, PartialEq)]
pub struct NeedleOverlay {
    pub id: u64,
    /// (index, position as shown, selected, jump end).
    pub points: Vec<(usize, Vec2, bool, bool)>,
}

/// Needle points of `o` as (index, position, jump end).
pub fn needle_points(o: &DesignObject) -> Vec<(usize, Vec2, bool)> {
    o.stitch_block
        .iter()
        .flat_map(|b| b.stitches.iter().enumerate())
        .filter(|(_, s)| is_point(s.command))
        .map(|(i, s)| (i, v2(s.x, s.y), s.command == openstitch_core::Command::Jump))
        .collect()
}

/// The needle point nearest `p` within `radius` (later points win ties, as they are drawn on
/// top).
pub fn needle_at(o: &DesignObject, p: Vec2, radius: f64) -> Option<usize> {
    needle_points(o)
        .into_iter()
        .filter(|(_, q, _)| q.distance(p) <= radius)
        .min_by(|a, b| {
            a.1.distance(p)
                .total_cmp(&b.1.distance(p))
                .then(b.0.cmp(&a.0))
        })
        .map(|(i, ..)| i)
}

/// The stitch line nearest `p` within `tolerance`: (index of the point it runs into, the
/// nearest position on it).
pub fn line_at(o: &DesignObject, p: Vec2, tolerance: f64) -> Option<(usize, Vec2)> {
    let pts = needle_points(o);
    pts.windows(2)
        .filter(|w| !w[1].2)
        .map(|w| {
            let (a, b) = (w[0].1, w[1].1);
            let d = b - a;
            let len2 = d.x * d.x + d.y * d.y;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (w[1].0, a + d * t)
        })
        .filter(|(_, q)| q.distance(p) <= tolerance)
        .min_by(|a, b| a.1.distance(p).total_cmp(&b.1.distance(p)))
}

/// The jump line nearest `p` within `tolerance`: the needle points at its two ends (the
/// point it leaves and the jump entry it lands on). A jump that a file stores as several
/// steps (DST's are at most 12.1 mm each) counts as one, from the point before the first
/// step to the last.
pub fn jump_at(o: &DesignObject, p: Vec2, tolerance: f64) -> Option<[usize; 2]> {
    let pts = needle_points(o);
    let (k, _) = pts
        .windows(2)
        .enumerate()
        .filter(|(_, w)| w[1].2)
        .map(|(k, w)| {
            let (a, b) = (w[0].1, w[1].1);
            let d = b - a;
            let len2 = d.x * d.x + d.y * d.y;
            let t = if len2 > 0.0 {
                (((p.x - a.x) * d.x + (p.y - a.y) * d.y) / len2).clamp(0.0, 1.0)
            } else {
                0.0
            };
            (k, (a + d * t).distance(p))
        })
        .filter(|(_, d)| *d <= tolerance)
        .min_by(|a, b| a.1.total_cmp(&b.1))?;
    // pts[k] is where this step leaves from, pts[k + 1] the jump it lands on.
    let mut start = k;
    while start > 0 && pts[start].2 {
        start -= 1;
    }
    let mut end = k + 1;
    while end + 1 < pts.len() && pts[end + 1].2 {
        end += 1;
    }
    Some([pts[start].0, pts[end].0])
}

/// The needle points of each colour of `o`, in sewing order.
pub fn color_groups(o: &DesignObject) -> Vec<Vec<usize>> {
    let Some(b) = o.stitch_block.as_ref() else {
        return vec![];
    };
    let colors = openstitch_design::stitch_edit::colors(b);
    let mut out: Vec<Vec<usize>> = Vec::new();
    for (i, ..) in needle_points(o) {
        let k = colors[i];
        while out.len() <= k {
            out.push(Vec::new());
        }
        out[k].push(i);
    }
    out.retain(|g| !g.is_empty());
    out
}

/// The pattern's entry point (its first needle point) or exit point (its last).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum End {
    Entry,
    Exit,
}

/// Whether needle point `i` is the entry or exit point of `o`.
pub fn end_of(o: &DesignObject, i: usize) -> Option<End> {
    let pts = needle_points(o);
    if pts.first().map(|p| p.0) == Some(i) {
        Some(End::Entry)
    } else if pts.last().map(|p| p.0) == Some(i) {
        Some(End::Exit)
    } else {
        None
    }
}

impl ToolContext<'_> {
    /// The stitch pattern the Point tool edits: the selection when it is exactly one visible
    /// stitch pattern.
    pub fn needle_target(&self) -> Option<&DesignObject> {
        if self.selection.len() != 1 {
            return None;
        }
        let id = *self.selection.first()?;
        self.design.objects.iter().find(|o| {
            o.id == id
                && o.kind == openstitch_design::ObjectKind::Stitches
                && o.visible
                && o.stitch_block.is_some()
        })
    }
}

impl ToolController {
    /// The needle points of `owner` selected with the Point tool or the Stitch Table (#501).
    pub fn needle_selection(&self, owner: u64) -> BTreeSet<usize> {
        if self.needle_owner == Some(owner) {
            self.needles.clone()
        } else {
            BTreeSet::new()
        }
    }

    /// Selects needle points of `owner` (the Stitch Table's rows), as Point tool clicks would.
    pub fn set_needle_selection(&mut self, owner: u64, selected: BTreeSet<usize>) {
        self.needle_owner = Some(owner);
        self.needles = selected;
    }

    /// Selected needle points of the Point tool's stitch pattern.
    pub fn selected_needles(&self, ctx: &ToolContext) -> BTreeSet<usize> {
        match ctx.needle_target() {
            Some(o) if self.needle_owner == Some(o.id) => self.needles.clone(),
            _ => BTreeSet::new(),
        }
    }

    pub fn needle_overlay(&self, ctx: &ToolContext) -> Option<NeedleOverlay> {
        if self.tool() != super::Tool::Point {
            return None;
        }
        let o = ctx.needle_target()?;
        let selected = self.selected_needles(ctx);
        let delta = match &self.gesture {
            Some(Gesture::Needles { delta: Some(d), .. }) => *d,
            _ => v2(0.0, 0.0),
        };
        Some(NeedleOverlay {
            id: o.id,
            points: needle_points(o)
                .into_iter()
                .map(|(i, p, jump)| {
                    let s = selected.contains(&i);
                    (i, if s { p + delta } else { p }, s, jump)
                })
                .collect(),
        })
    }

    pub(super) fn needle_tool(
        &mut self,
        e: PointerEvent,
        ctx: &ToolContext,
        o: &DesignObject,
    ) -> Vec<ToolEffect> {
        let p = e.location;
        if self.needle_owner != Some(o.id) {
            self.needles.clear();
            self.needle_owner = Some(o.id);
            self.needle_anchor = None;
        }
        match (e.phase, self.gesture.clone()) {
            (Phase::Down, _) => {
                if let Some(i) = needle_at(o, p, ctx.point_radius()) {
                    // Alt on the selected entry or exit point: a drag connects it to an exit
                    // or entry point of another stitch pattern (PE-DESIGN's manual).
                    if e.alt && self.needles == BTreeSet::from([i]) && end_of(o, i).is_some() {
                        self.gesture = Some(Gesture::NeedleConnect {
                            index: i,
                            onto: None,
                        });
                        self.needle_alt_drag = true;
                        self.needle_connect_hover = None;
                        return vec![];
                    }
                    // Ctrl+click toggles a point, Shift+click adds it, a plain click selects it
                    // alone (one that is selected keeps the others, for a drag), as for the
                    // points of a shape (PE-DESIGN capture f).
                    if e.free {
                        if !self.needles.remove(&i) {
                            self.needles.insert(i);
                        }
                    } else if e.shift {
                        self.needles.insert(i);
                    } else if !self.needles.contains(&i) {
                        self.needles = BTreeSet::from([i]);
                    }
                    self.needle_anchor = None;
                    self.gesture = Some(Gesture::Needles {
                        start: p,
                        delta: None,
                        marquee: false,
                    });
                    return vec![];
                }
                // A click on a jump selects the needle points at both its ends.
                if let Some(ends) = jump_at(o, p, ctx.tolerance()) {
                    if e.free || e.shift {
                        self.needles.extend(ends);
                    } else {
                        self.needles = ends.into_iter().collect();
                    }
                    self.needle_anchor = None;
                    return vec![];
                }
                // On a stitch: one click inserts a needle point there (Ctrl+Shift: and the
                // stitch up to it becomes a jump); a drag from there is a drag box.
                if let Some((index, at)) = line_at(o, p, ctx.tolerance()) {
                    self.gesture = Some(Gesture::NeedleLine {
                        start: p,
                        index,
                        at,
                        jump: e.free && e.shift,
                    });
                    return vec![];
                }
                let inside = o.bounds().is_some_and(|b| {
                    p.x >= b.min_x && p.x <= b.max_x && p.y >= b.min_y && p.y <= b.max_y
                });
                if inside {
                    if !(e.shift || e.free) {
                        self.needles.clear();
                    }
                    self.gesture = Some(Gesture::Needles {
                        start: p,
                        delta: None,
                        marquee: true,
                    });
                    return vec![];
                }
                // Outside the pattern: the selected points are dropped; a click on another
                // object selects it, one on the empty page drops the pattern too (PE-DESIGN
                // capture k10).
                self.needles.clear();
                self.needle_anchor = None;
                match object_at(ctx.design, p, ctx.tolerance()) {
                    Some(id) if id == o.id => vec![],
                    Some(id) => vec![ToolEffect::Select(BTreeSet::from([id]))],
                    None => vec![ToolEffect::Select(BTreeSet::new())],
                }
            }
            (Phase::Dragged, Some(Gesture::NeedleLine { start, .. })) => {
                if p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point {
                    if !(e.shift || e.free) {
                        self.needles.clear();
                    }
                    self.gesture = Some(Gesture::Needles {
                        start,
                        delta: None,
                        marquee: true,
                    });
                    self.preview = Some(ToolPreview::Marquee(rect_from(start, p)));
                }
                vec![]
            }
            (
                Phase::Up,
                Some(Gesture::NeedleLine {
                    index, at, jump, ..
                }),
            ) => {
                self.gesture = None;
                // The new point is selected: it takes index `index` (with a jump, the
                // stitch after the jump entry, index + 1).
                self.needles = BTreeSet::from([if jump { index + 1 } else { index }]);
                self.needle_anchor = None;
                vec![ToolEffect::run(
                    CommandRequest::new(
                        "stitches.insertPoint",
                        json!({ "id": o.id, "index": index, "x": mm(at.x), "y": mm(at.y), "jump": jump }),
                    ),
                    false,
                )]
            }
            (Phase::Dragged, Some(Gesture::NeedleConnect { index, .. })) => {
                let onto = self
                    .needle_connect_ends(ctx)
                    .into_iter()
                    .find(|(_, q)| q.distance(p) <= 1.5 * ctx.point_radius());
                self.gesture = Some(Gesture::NeedleConnect { index, onto });
                self.needle_connect_hover = onto;
                vec![]
            }
            (Phase::Up, Some(Gesture::NeedleConnect { index, onto })) => {
                self.gesture = None;
                self.needle_alt_drag = false;
                self.needle_connect_hover = None;
                let Some((other, _)) = onto else {
                    return vec![];
                };
                // The exit point dropped on another pattern's entry: this one is sewn first;
                // the entry dropped on another's exit: the other one first.
                let first = if end_of(o, index) == Some(End::Exit) {
                    o.id
                } else {
                    other
                };
                self.needles.clear();
                vec![ToolEffect::run(
                    CommandRequest::new(
                        "stitches.connect",
                        json!({ "ids": [o.id, other], "first": first }),
                    ),
                    false,
                )]
            }
            (Phase::Dragged, Some(Gesture::Needles { start, marquee, .. })) => {
                let moved = p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point;
                if marquee {
                    self.preview = moved.then(|| ToolPreview::Marquee(rect_from(start, p)));
                    return vec![];
                }
                let delta = moved.then(|| move_delta(start, p, e.shift));
                self.gesture = Some(Gesture::Needles {
                    start,
                    delta,
                    marquee,
                });
                vec![]
            }
            (
                Phase::Up,
                Some(Gesture::Needles {
                    start,
                    delta,
                    marquee,
                }),
            ) => {
                self.gesture = None;
                self.preview = None;
                if marquee {
                    if p.distance(start) >= DRAG_THRESHOLD * ctx.units_per_point {
                        let r = rect_from(start, p);
                        self.needles.extend(
                            needle_points(o)
                                .into_iter()
                                .filter(|(_, q, _)| {
                                    q.x >= r.min_x
                                        && q.x <= r.max_x
                                        && q.y >= r.min_y
                                        && q.y <= r.max_y
                                })
                                .map(|(i, ..)| i),
                        );
                    }
                    return vec![];
                }
                match delta {
                    Some(d) if !self.needles.is_empty() => vec![ToolEffect::run(
                        CommandRequest::new(
                            "stitches.movePoints",
                            json!({ "id": o.id, "indexes": self.needles.iter().collect::<Vec<_>>(), "dx": mm(d.x), "dy": mm(d.y) }),
                        ),
                        false,
                    )],
                    _ => vec![],
                }
            }
            _ => vec![],
        }
    }

    /// A right-click with the Point tool on a needle point of the stitch pattern: selects it
    /// (one that is already selected keeps the others) for the needle point menu. False when
    /// there is no needle point there.
    pub fn select_needle_at(&mut self, p: Vec2, ctx: &ToolContext) -> bool {
        if self.tool() != super::Tool::Point {
            return false;
        }
        let Some(o) = ctx.needle_target() else {
            return false;
        };
        let Some(i) = needle_at(o, p, ctx.point_radius()) else {
            return false;
        };
        if self.needle_owner != Some(o.id) || !self.needles.contains(&i) {
            self.needle_owner = Some(o.id);
            self.needles = BTreeSet::from([i]);
            self.needle_anchor = None;
        }
        true
    }

    /// The exit points (for a selected entry point) or entry points (for a selected exit
    /// point) of the other visible stitch patterns, where the selected point can be
    /// Alt-dragged to connect the two patterns: (pattern, position).
    pub fn needle_connect_ends(&self, ctx: &ToolContext) -> Vec<(u64, Vec2)> {
        let Some(o) = ctx.needle_target() else {
            return vec![];
        };
        let selected = self.selected_needles(ctx);
        let Some(&i) = selected.first().filter(|_| selected.len() == 1) else {
            return vec![];
        };
        let Some(end) = end_of(o, i) else {
            return vec![];
        };
        ctx.design
            .objects
            .iter()
            .filter(|q| {
                q.id != o.id
                    && q.visible
                    && q.kind == openstitch_design::ObjectKind::Stitches
                    && q.stitch_block.is_some()
            })
            .filter_map(|q| {
                let pts = needle_points(q);
                let want = match end {
                    End::Entry => pts.last(),
                    End::Exit => pts.first(),
                };
                want.map(|(_, p, _)| (q.id, *p))
            })
            .collect()
    }

    /// Where Alt shows an x with the Point tool on a stitch pattern: the ends the selected
    /// entry or exit point can be connected to (only while Alt is held or the drag is on).
    pub fn needle_connect_marks(&self, alt: bool, ctx: &ToolContext) -> Vec<Vec2> {
        if self.tool() != super::Tool::Point || !(alt || self.needle_alt_drag) {
            return vec![];
        }
        self.needle_connect_ends(ctx)
            .into_iter()
            .map(|(_, p)| p)
            .collect()
    }

    /// The end the Alt-dragged needle point lies on now (a red square is drawn there).
    pub fn needle_connect_overlap(&self) -> Option<Vec2> {
        self.needle_alt_drag
            .then_some(self.needle_connect_hover)
            .flatten()
            .map(|(_, p)| p)
    }

    /// Arrow keys with needle points selected: moves them by (`dx`, `dy`) (0.1 mm units) in
    /// one `stitches.movePoints`. `None` when no needle point is selected.
    pub fn nudge_needles(
        &mut self,
        dx: f64,
        dy: f64,
        ctx: &ToolContext,
    ) -> Option<Vec<ToolEffect>> {
        let o = ctx.needle_target()?;
        let selected = self.selected_needles(ctx);
        if self.tool() != super::Tool::Point || selected.is_empty() {
            return None;
        }
        Some(vec![ToolEffect::run(
            CommandRequest::new(
                "stitches.movePoints",
                json!({ "id": o.id, "indexes": selected.iter().collect::<Vec<_>>(), "dx": mm(dx), "dy": mm(dy) }),
            ),
            false,
        )])
    }

    /// Ctrl+arrow (`extend` false) and Shift+arrow (`extend` true) with needle points
    /// selected, as for the points of a shape: Ctrl+arrow selects the next (`forward`) or
    /// previous needle point alone; Shift+arrow grows the selection by one point from where
    /// it started and shrinks it again the other way. The order is the sewing order, across
    /// jumps and colour changes (not captured: PE-DESIGN capture k4). False, changing
    /// nothing, when no needle point is selected.
    pub fn step_needles(&mut self, forward: bool, extend: bool, ctx: &ToolContext) -> bool {
        let Some(o) = ctx.needle_target() else {
            return false;
        };
        let selected = self.selected_needles(ctx);
        if self.tool() != super::Tool::Point || selected.is_empty() {
            return false;
        }
        let order: Vec<usize> = needle_points(o).into_iter().map(|(i, ..)| i).collect();
        let place = |i: usize| order.iter().position(|&k| k == i);
        let step: i64 = if forward { 1 } else { -1 };
        if !extend {
            let from = if forward {
                selected.last()
            } else {
                selected.first()
            };
            let Some(at) = from.and_then(|&i| place(i)) else {
                return false;
            };
            let to = (at as i64 + step).clamp(0, order.len() as i64 - 1) as usize;
            self.needles = BTreeSet::from([order[to]]);
            self.needle_anchor = None;
            return true;
        }
        let (anchor, span) = match self.needle_anchor {
            Some((a, s)) if selected.contains(&a) => (a, s),
            _ => (*selected.first().expect("not empty"), 0),
        };
        let Some(base) = place(anchor) else {
            return false;
        };
        let reach = (base as i64 + span + step).clamp(0, order.len() as i64 - 1);
        let span = reach - base as i64;
        let (lo, hi) = if span >= 0 {
            (base as i64, reach)
        } else {
            (reach, base as i64)
        };
        self.needles = (lo..=hi).map(|k| order[k as usize]).collect();
        self.needle_anchor = Some((anchor, span));
        true
    }

    /// Tab and Shift+Tab with the Point tool on a stitch pattern, as PE-DESIGN 11.50's (capture
    /// k6): with points selected, every needle point of the pattern; with all of them
    /// selected, none; with none selected nothing happens, unless Tab has just dropped them
    /// (then all again). Both keys do the same. True when the selection changed.
    pub fn tab_needles(&mut self, ctx: &ToolContext) -> bool {
        let Some(o) = ctx.needle_target() else {
            return false;
        };
        if self.tool() != super::Tool::Point {
            return false;
        }
        let all: BTreeSet<usize> = needle_points(o).into_iter().map(|(i, ..)| i).collect();
        let selected = self.selected_needles(ctx);
        let toggled_off = self.needle_tab_off == Some(o.id);
        self.needle_owner = Some(o.id);
        self.needle_anchor = None;
        if selected.is_empty() && !toggled_off {
            return false;
        }
        if selected == all {
            self.needles.clear();
            self.needle_tab_off = Some(o.id);
        } else {
            self.needles = all;
            self.needle_tab_off = None;
        }
        true
    }

    /// Home and End (`exit` false / true) with the Point tool on a stitch pattern, as
    /// PE-DESIGN 11.50's (capture k7): the pattern's entry or exit point, with a needle point
    /// selected; with none selected nothing happens. `whole` (Ctrl+Home, Ctrl+End): the same
    /// with none selected too (Ctrl+End's target in PE-DESIGN, perhaps the design's last
    /// point, is not confirmed). True when the selection changed.
    pub fn select_needle_end(&mut self, exit: bool, whole: bool, ctx: &ToolContext) -> bool {
        let Some(o) = ctx.needle_target() else {
            return false;
        };
        if self.tool() != super::Tool::Point {
            return false;
        }
        if !whole && self.selected_needles(ctx).is_empty() {
            return false;
        }
        let pts = needle_points(o);
        let Some(point) = (if exit { pts.last() } else { pts.first() }).map(|p| p.0) else {
            return false;
        };
        self.needle_owner = Some(o.id);
        self.needles = BTreeSet::from([point]);
        self.needle_anchor = None;
        true
    }

    /// Delete with the Point tool on a stitch pattern: removes the selected needle points.
    pub(super) fn delete_needles(&mut self, ctx: &ToolContext) -> Option<Vec<ToolEffect>> {
        let o = ctx.needle_target()?;
        let selected = self.selected_needles(ctx);
        if selected.is_empty() {
            return None;
        }
        // One point deleted: its neighbour is selected afterwards, the next point, or the one
        // before when the deleted point was the last of its run (a jump or the exit follows),
        // as PE-DESIGN 11.50 (captures k11, k14). Indexes after it move down by one.
        self.needles.clear();
        self.needle_anchor = None;
        if let [deleted] = selected.iter().copied().collect::<Vec<_>>()[..] {
            let pts = needle_points(o);
            if let Some(k) = pts.iter().position(|q| q.0 == deleted) {
                let next = pts.get(k + 1).filter(|q| !q.2).map(|q| q.0 - 1);
                let before = k.checked_sub(1).map(|b| pts[b].0);
                if let Some(n) = next.or(before) {
                    self.needles.insert(n);
                }
            }
        }
        Some(vec![ToolEffect::run(
            CommandRequest::new(
                "stitches.deletePoints",
                json!({ "id": o.id, "indexes": selected.into_iter().collect::<Vec<_>>() }),
            ),
            false,
        )])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::editor::Tool;
    use openstitch_engine::Session;

    fn pattern() -> Session {
        let mut s = Session::new(std::env::temp_dir());
        s.execute(
            "shape.rectangle",
            Some(&json!({"x": 10, "y": 10, "width": 10, "height": 5})),
        )
        .expect("rect");
        s.execute("object.toStitches", Some(&json!({"id": 1})))
            .expect("convert");
        s
    }

    fn ev(phase: Phase, at: Vec2) -> PointerEvent {
        PointerEvent::new(phase, at)
    }

    fn requests(effects: &[ToolEffect]) -> Vec<CommandRequest> {
        effects
            .iter()
            .flat_map(|e| match e {
                ToolEffect::Execute { requests, .. } => requests.clone(),
                ToolEffect::Select(_) => vec![],
            })
            .collect()
    }

    #[test]
    fn select_drag_delete_insert_and_marquee() {
        let mut s = pattern();
        let selection = BTreeSet::from([1]);
        let design = s.design().clone();
        let ctx = ToolContext {
            design: &design,
            selection: &selection,
            units_per_point: 0.5,
            scale_takes_origin: true,
        };
        let o = ctx.needle_target().expect("stitch pattern");
        let pts = needle_points(o);
        let (i, p, _) = pts[pts.len() / 2];
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);

        // Click selects; drag moves the selection by one command.
        assert!(t.handle(ev(Phase::Down, p), &ctx).is_empty());
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([i]));
        t.handle(ev(Phase::Dragged, p + v2(10.0, 0.0)), &ctx);
        let overlay = t.needle_overlay(&ctx).expect("overlay");
        assert!(
            overlay
                .points
                .iter()
                .any(|q| q.0 == i && q.2 && q.1 == p + v2(10.0, 0.0))
        );
        let r = requests(&t.handle(ev(Phase::Up, p + v2(10.0, 0.0)), &ctx));
        assert_eq!(r.len(), 1);
        assert_eq!(r[0].command, "stitches.movePoints");
        assert_eq!(r[0].params["indexes"], json!([i]));
        assert_eq!(r[0].params["dx"], json!(1.0));
        s.execute(r[0].command, Some(&r[0].params)).expect("moves");

        // Delete removes the selected point.
        let r = requests(&t.delete_point(&ctx).expect("needles selected"));
        assert_eq!(r[0].command, "stitches.deletePoints");
        assert_eq!(
            t.selected_needles(&ctx).len(),
            1,
            "the neighbour is selected (PE-DESIGN capture k14)"
        );

        // A click on a stitch line inserts a point there: the longest stitch before the
        // deleted point (the fill starts with short tie-in steps).
        let k = (0..i.saturating_sub(1))
            .max_by(|&x, &y| {
                let len = |k: usize| pts[k].1.distance(pts[k + 1].1);
                len(x).total_cmp(&len(y))
            })
            .expect("stitches before the deleted point");
        let (_, a, _) = pts[k];
        let (j, b, _) = pts[k + 1];
        let mid = a.lerp(b, 0.5);
        assert!(a.distance(b) > 4.0, "a line long enough to click");
        // One click (press and release in place) on a stitch inserts a point there (#1272,
        // PE-DESIGN's manual; it was a double-click).
        assert!(t.handle(ev(Phase::Down, mid), &ctx).is_empty());
        let r = requests(&t.handle(ev(Phase::Up, mid), &ctx));
        assert_eq!(r[0].command, "stitches.insertPoint");
        assert_eq!(r[0].params["index"], json!(j));
        assert_eq!(r[0].params["jump"], json!(false));
        assert_eq!(
            t.selected_needles(&ctx),
            BTreeSet::from([j]),
            "the new point"
        );

        // A marquee inside the pattern selects every point in it.
        // Start inside the bounds away from needle points (a single press on a line starts a
        // marquee too).
        let b = o.bounds().expect("bounds");
        let start = (1..20)
            .flat_map(|i| (1..20).map(move |j| (i, j)))
            .map(|(i, j)| {
                v2(
                    b.min_x + b.width() * f64::from(i) / 20.0,
                    b.min_y + b.height() * f64::from(j) / 20.0,
                )
            })
            .find(|&q| needle_at(o, q, ctx.point_radius()).is_none())
            .expect("an empty spot");
        t.handle(ev(Phase::Down, start), &ctx);
        t.handle(ev(Phase::Dragged, v2(b.max_x + 5.0, b.max_y + 5.0)), &ctx);
        t.handle(ev(Phase::Up, v2(b.max_x + 5.0, b.max_y + 5.0)), &ctx);
        let inside = pts
            .iter()
            .filter(|q| q.1.x >= start.x && q.1.y >= start.y)
            .count();
        assert!(inside > 0);
        assert_eq!(t.selected_needles(&ctx).len(), inside);
    }

    /// One stitch pattern in two colours, each two straight running lines with a jump
    /// between them: red at y = 20 and 30 mm, blue at y = 50 and 60, each from x = 20 to 40
    /// (PE-DESIGN capture k0's design). Read from a DST file written of four lines, as a
    /// machine file is imported. Returns the session and the pattern's id.
    pub(crate) fn two_colours() -> (Session, u64) {
        let dir = std::env::temp_dir().join(format!(
            "openstitch-two-colours-{}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("scratch folder");
        let mut s = Session::new(dir.clone());
        for (k, (y, colour)) in [
            (20, "#d32f2f"),
            (30, "#d32f2f"),
            (50, "#1f3a93"),
            (60, "#1f3a93"),
        ]
        .into_iter()
        .enumerate()
        {
            s.execute(
                "shape.path",
                Some(&json!({ "points": [[20, y], [40, y]], "closed": false, "outline": colour })),
            )
            .expect("line");
            s.execute(
                "object.setLine",
                Some(&json!({ "id": k + 1, "type": "running", "pitch": 2.0 })),
            )
            .expect("running");
        }
        s.execute("design.export", Some(&json!({ "path": "k0.dst" })))
            .expect("export");
        let mut t = Session::new(dir.clone());
        let r = t
            .execute("stitches.import", Some(&json!({ "path": "k0.dst" })))
            .expect("import");
        let _ = std::fs::remove_dir_all(&dir);
        let id = r["id"]
            .as_u64()
            .or_else(|| r["ids"][0].as_u64())
            .unwrap_or(1);
        // The import is centred on the page: put it back where it was drawn.
        to_corner(&mut t, id, v2(20.0, 20.0));
        (t, id)
    }

    /// Moves object `id` so that its bounds start at `corner` (mm).
    fn to_corner(s: &mut Session, id: u64, corner: Vec2) {
        let d = s.design();
        let b = d.objects[d.index_of(id).expect("object")]
            .bounds()
            .expect("bounds");
        let (dx, dy) = (corner.x - b.min_x / 10.0, corner.y - b.min_y / 10.0);
        s.execute(
            "object.move",
            Some(&json!({ "ids": [id], "dx": dx, "dy": dy })),
        )
        .expect("move");
    }

    struct Fixture {
        design: openstitch_design::Design,
        selection: BTreeSet<u64>,
    }

    impl Fixture {
        fn new(s: &Session, id: u64) -> Self {
            Fixture {
                design: s.design().clone(),
                selection: BTreeSet::from([id]),
            }
        }
        fn ctx(&self) -> ToolContext<'_> {
            ToolContext {
                design: &self.design,
                selection: &self.selection,
                units_per_point: 0.5,
                scale_takes_origin: true,
            }
        }
    }

    /// The needle point at (x, y) mm, the first one there.
    fn point_at(o: &DesignObject, x: f64, y: f64) -> usize {
        needle_points(o)
            .into_iter()
            .find(|(_, p, _)| p.distance(v2(x * 10.0, y * 10.0)) < 0.5)
            .map(|(i, ..)| i)
            .unwrap_or_else(|| panic!("no needle point at ({x}, {y})"))
    }

    #[test]
    fn the_two_colour_pattern_is_what_the_captures_use() {
        let (s, id) = two_colours();
        let f = Fixture::new(&s, id);
        let ctx = f.ctx();
        let o = ctx.needle_target().expect("one stitch pattern");
        let groups = color_groups(o);
        assert_eq!(groups.len(), 2, "two colours");
        let jumps = needle_points(o).iter().filter(|p| p.2).count();
        assert!(jumps >= 3, "jumps between the lines: {jumps}");
    }

    /// Ctrl+click toggles a needle point, Shift+click adds one, a plain click on a selected
    /// point keeps the selection (for a drag), on another selects it alone (PE-DESIGN's
    /// manual: as for a shape's points, capture f).
    #[test]
    fn ctrl_click_toggles_and_shift_click_adds() {
        let (s, id) = two_colours();
        let f = Fixture::new(&s, id);
        let ctx = f.ctx();
        let o = ctx.needle_target().expect("pattern");
        let (a, b, c) = (
            point_at(o, 24.0, 20.0),
            point_at(o, 28.0, 20.0),
            point_at(o, 32.0, 20.0),
        );
        let at = |i: usize| {
            needle_points(o)
                .into_iter()
                .find(|q| q.0 == i)
                .expect("point")
                .1
        };
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        let click = |t: &mut ToolController, i: usize, ctrl: bool, shift: bool| {
            let mut e = ev(Phase::Down, at(i));
            e.free = ctrl;
            e.shift = shift;
            t.handle(e, &ctx);
            t.handle(ev(Phase::Up, at(i)), &ctx);
        };
        click(&mut t, a, false, false);
        click(&mut t, b, true, false);
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([a, b]));
        click(&mut t, c, false, true);
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([a, b, c]));
        // Shift+click on a selected point keeps it (adds, does not toggle).
        click(&mut t, c, false, true);
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([a, b, c]));
        // Ctrl+click on a selected point drops it.
        click(&mut t, b, true, false);
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([a, c]));
        // A plain click on another point selects it alone.
        click(&mut t, b, false, false);
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([b]));
    }

    /// Ctrl+Shift+click on a stitch inserts a point there and turns the stitch up to it into a
    /// jump; a drag that starts on a stitch is a drag box, not an insert.
    #[test]
    fn ctrl_shift_click_inserts_as_a_jump_and_a_drag_from_a_stitch_is_a_box() {
        let (mut s, id) = two_colours();
        let f = Fixture::new(&s, id);
        let ctx = f.ctx();
        let o = ctx.needle_target().expect("pattern");
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        let mid = v2(250.0, 500.0);
        let into = point_at(o, 26.0, 50.0);
        let mut e = ev(Phase::Down, mid);
        e.free = true;
        e.shift = true;
        assert!(t.handle(e, &ctx).is_empty());
        let r = requests(&t.handle(ev(Phase::Up, mid), &ctx));
        assert_eq!(r[0].command, "stitches.insertPoint");
        assert_eq!(
            (r[0].params["index"].clone(), r[0].params["jump"].clone()),
            (json!(into), json!(true))
        );
        assert_eq!(r[0].params["x"], json!(25.0));
        let before = needle_points(o).iter().filter(|p| p.2).count();
        s.execute(r[0].command, Some(&r[0].params)).expect("insert");
        let after = Fixture::new(&s, id);
        let o2 = after.ctx().needle_target().expect("pattern").clone();
        assert_eq!(
            needle_points(&o2).iter().filter(|p| p.2).count(),
            before + 1
        );
        // A drag from a stitch: a drag box, nothing inserted.
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        t.handle(ev(Phase::Down, mid), &ctx);
        t.handle(ev(Phase::Dragged, v2(450.0, 650.0)), &ctx);
        let r = requests(&t.handle(ev(Phase::Up, v2(450.0, 650.0)), &ctx));
        assert!(r.is_empty(), "{r:?}");
        assert!(t.selected_needles(&ctx).contains(&point_at(o, 30.0, 60.0)));
    }

    /// A click on a jump selects the needle points at both its ends (PE-DESIGN capture k9); a
    /// click on the empty page drops the selected points and the pattern (k10).
    #[test]
    fn a_click_on_a_jump_selects_both_ends_and_one_on_the_page_drops_them() {
        let (s, id) = two_colours();
        let f = Fixture::new(&s, id);
        let ctx = f.ctx();
        let o = ctx.needle_target().expect("pattern");
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        // The jump from the end of the first red line (40, 20) to the start of the second
        // (20, 30): its middle.
        let on_jump = v2(300.0, 250.0);
        t.handle(ev(Phase::Down, on_jump), &ctx);
        t.handle(ev(Phase::Up, on_jump), &ctx);
        let selected = t.selected_needles(&ctx);
        let pts = needle_points(o);
        let at = |i: usize| pts.iter().find(|q| q.0 == i).expect("point").1;
        assert_eq!(selected.len(), 2, "{selected:?}");
        let ends: Vec<Vec2> = selected.iter().map(|&i| at(i)).collect();
        assert!(
            ends.iter().any(|p| p.distance(v2(400.0, 200.0)) < 5.0),
            "{ends:?}"
        );
        assert!(
            ends.iter().any(|p| p.distance(v2(200.0, 300.0)) < 5.0),
            "{ends:?}"
        );
        // The empty page: the points and the pattern are dropped (PE-DESIGN capture k10).
        let page = v2(800.0, 900.0);
        let effects = t.handle(ev(Phase::Down, page), &ctx);
        t.handle(ev(Phase::Up, page), &ctx);
        assert!(
            matches!(effects.as_slice(), [ToolEffect::Select(s)] if s.is_empty()),
            "the pattern is dropped: {effects:?}"
        );
        assert!(t.selected_needles(&ctx).is_empty());
    }

    /// PE-DESIGN captures k6 and k7: Tab and Shift+Tab toggle all / none of the pattern's
    /// needle points (nothing selected: nothing, unless Tab has just dropped them); Home and
    /// End select the pattern's entry and exit point (nothing selected: nothing); Ctrl+Home and
    /// Ctrl+End do so with nothing selected too.
    #[test]
    fn tab_toggles_all_points_and_home_and_end_select_the_patterns_ends() {
        let (s, id) = two_colours();
        let f = Fixture::new(&s, id);
        let ctx = f.ctx();
        let o = ctx.needle_target().expect("pattern");
        let all: BTreeSet<usize> = needle_points(o).into_iter().map(|(i, ..)| i).collect();
        let (first, last) = (*all.first().expect("entry"), *all.last().expect("exit"));
        let middle = *all.iter().nth(4).expect("a point");
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        assert!(!t.tab_needles(&ctx), "nothing selected: Tab does nothing");
        assert!(t.selected_needles(&ctx).is_empty());
        assert!(
            !t.select_needle_end(false, false, &ctx),
            "nothing selected: Home does nothing"
        );
        assert!(t.selected_needles(&ctx).is_empty());
        t.set_needle_selection(id, BTreeSet::from([middle]));
        assert!(t.tab_needles(&ctx), "one selected: all");
        assert_eq!(t.selected_needles(&ctx), all);
        assert!(t.tab_needles(&ctx), "all selected: none");
        assert!(t.selected_needles(&ctx).is_empty());
        assert!(t.tab_needles(&ctx), "Shift+Tab right after: all again");
        assert_eq!(t.selected_needles(&ctx), all);
        t.set_needle_selection(id, BTreeSet::from([middle]));
        assert!(t.select_needle_end(true, false, &ctx));
        assert_eq!(
            t.selected_needles(&ctx),
            BTreeSet::from([last]),
            "End: the exit point"
        );
        assert!(t.select_needle_end(false, false, &ctx));
        assert_eq!(
            t.selected_needles(&ctx),
            BTreeSet::from([first]),
            "Home: the entry point"
        );
        t.set_needle_selection(id, BTreeSet::new());
        assert!(
            t.select_needle_end(true, true, &ctx),
            "Ctrl+End with nothing selected"
        );
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([last]));
        assert!(t.select_needle_end(false, true, &ctx));
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([first]));
        assert_eq!(end_of(o, first), Some(End::Entry));
        assert_eq!(end_of(o, last), Some(End::Exit));
    }

    /// Delete with one needle point selected selects its neighbour afterwards (PE-DESIGN
    /// captures k11 and k14): the next point, or the one before when the deleted point is the
    /// last of its run (a jump follows, or it is the exit).
    #[test]
    fn delete_selects_the_neighbour() {
        let (s, id) = two_colours();
        let f = Fixture::new(&s, id);
        let ctx = f.ctx();
        let o = ctx.needle_target().expect("pattern");
        let pts = needle_points(o);
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        // A point inside a line: the next one, whose index moves down by one.
        let k = 6;
        t.set_needle_selection(id, BTreeSet::from([pts[k].0]));
        let r = requests(&t.delete_point(&ctx).expect("deleted"));
        assert_eq!(r[0].command, "stitches.deletePoints");
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([pts[k + 1].0 - 1]));
        // The last point before a jump: the point before it.
        let before_jump = (1..pts.len() - 1)
            .find(|&k| pts[k + 1].2 && !pts[k].2)
            .expect("a point before a jump");
        t.set_needle_selection(id, BTreeSet::from([pts[before_jump].0]));
        t.delete_point(&ctx).expect("deleted");
        assert_eq!(
            t.selected_needles(&ctx),
            BTreeSet::from([pts[before_jump - 1].0])
        );
        // The exit point: the point before it.
        let last = pts.len() - 1;
        t.set_needle_selection(id, BTreeSet::from([pts[last].0]));
        t.delete_point(&ctx).expect("deleted");
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([pts[last - 1].0]));
    }

    /// Ctrl+arrow selects the next or previous needle point alone, Shift+arrow grows and
    /// shrinks the selection from where it started; the arrows move the selected points.
    #[test]
    fn ctrl_and_shift_arrows_step_and_the_arrows_move_the_points() {
        let (s, id) = two_colours();
        let f = Fixture::new(&s, id);
        let ctx = f.ctx();
        let o = ctx.needle_target().expect("pattern");
        let order: Vec<usize> = needle_points(o).into_iter().map(|(i, ..)| i).collect();
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        assert!(
            !t.step_needles(true, false, &ctx),
            "nothing selected: nothing to step"
        );
        t.set_needle_selection(id, BTreeSet::from([order[4]]));
        assert!(t.step_needles(true, false, &ctx));
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([order[5]]));
        assert!(t.step_needles(false, false, &ctx));
        assert_eq!(t.selected_needles(&ctx), BTreeSet::from([order[4]]));
        assert!(t.step_needles(true, true, &ctx));
        assert!(t.step_needles(true, true, &ctx));
        assert_eq!(
            t.selected_needles(&ctx),
            BTreeSet::from([order[4], order[5], order[6]])
        );
        assert!(t.step_needles(false, true, &ctx));
        assert_eq!(
            t.selected_needles(&ctx),
            BTreeSet::from([order[4], order[5]])
        );
        // The arrows: one stitches.movePoints of the selected points.
        let r = requests(&t.nudge_needles(5.0, 0.0, &ctx).expect("points selected"));
        assert_eq!(r[0].command, "stitches.movePoints");
        assert_eq!(r[0].params["indexes"], json!([order[4], order[5]]));
        assert_eq!(r[0].params["dx"], json!(0.5));
    }

    /// With the pattern's entry or exit point selected and Alt held, the other patterns'
    /// exit or entry points are marked; Alt+drag onto one connects the two, the exit's
    /// pattern first.
    #[test]
    fn alt_drag_of_an_end_connects_to_another_pattern() {
        let (mut s, id) = two_colours();
        // A second pattern: one green line from (60, 40) to (80, 40) mm.
        let dir = std::env::temp_dir().join(format!("openstitch-k1-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("scratch");
        let mut k1 = Session::new(dir.clone());
        k1.execute(
            "shape.path",
            Some(&json!({ "points": [[60, 40], [80, 40]], "closed": false, "outline": "#2e7d32" })),
        )
        .expect("line");
        k1.execute(
            "object.setLine",
            Some(&json!({ "id": 1, "type": "running", "pitch": 2.0 })),
        )
        .expect("running");
        k1.execute("design.export", Some(&json!({ "path": "k1.dst" })))
            .expect("export");
        let r = s
            .execute(
                "stitches.import",
                Some(&json!({ "path": dir.join("k1.dst").display().to_string() })),
            )
            .expect("import");
        let _ = std::fs::remove_dir_all(&dir);
        let other = r["id"]
            .as_u64()
            .or_else(|| r["ids"][0].as_u64())
            .expect("id");
        to_corner(&mut s, other, v2(60.0, 40.0));
        let f = Fixture::new(&s, id);
        let ctx = f.ctx();
        let o = ctx.needle_target().expect("pattern");
        let pts = needle_points(o);
        let (exit, exit_at) = (pts.last().expect("exit").0, pts.last().expect("exit").1);
        let mut t = ToolController::default();
        t.set_tool(Tool::Point);
        // Not an end: no marks.
        t.set_needle_selection(id, BTreeSet::from([pts[5].0]));
        assert!(t.needle_connect_marks(true, &ctx).is_empty());
        // The exit point: the other pattern's entry is marked, only with Alt.
        t.set_needle_selection(id, BTreeSet::from([exit]));
        assert!(t.needle_connect_marks(false, &ctx).is_empty());
        let marks = t.needle_connect_marks(true, &ctx);
        assert_eq!(marks.len(), 1);
        assert!(marks[0].distance(v2(600.0, 400.0)) < 5.0, "{marks:?}");
        // Alt+drag onto it: stitches.connect, this pattern first.
        let mut e = ev(Phase::Down, exit_at);
        e.alt = true;
        t.handle(e, &ctx);
        t.handle(ev(Phase::Dragged, marks[0]), &ctx);
        assert_eq!(t.needle_connect_overlap(), Some(marks[0]));
        let r = requests(&t.handle(ev(Phase::Up, marks[0]), &ctx));
        assert_eq!(r[0].command, "stitches.connect");
        assert_eq!(r[0].params["first"], json!(id));
        let mut ids: Vec<u64> = serde_json::from_value(r[0].params["ids"].clone()).expect("ids");
        ids.sort_unstable();
        assert_eq!(ids, {
            let mut v = vec![id, other];
            v.sort_unstable();
            v
        });
        s.execute(r[0].command, Some(&r[0].params))
            .expect("connect");
        assert_eq!(
            s.design()
                .objects
                .iter()
                .filter(|q| q.kind == openstitch_design::ObjectKind::Stitches)
                .count(),
            1,
            "one pattern"
        );
    }
}
