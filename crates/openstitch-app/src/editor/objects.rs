//! Object list (sewing order) model.

use super::request::CommandRequest;
use openstitch_core::Thread;
use openstitch_design::{Design, DesignObject, ObjectKind};
use serde_json::{Value, json};
use std::collections::BTreeSet;

#[derive(Clone, Debug, PartialEq)]
pub struct ObjectRow {
    pub id: u64,
    pub index: usize,
    pub label: String,
    pub detail: String,
    /// Thread colours in sewing order (region, line or the pattern's threads).
    /// The object's threads, each once (by colour), in the order it sews them.
    pub swatches: Vec<Thread>,
    pub visible: bool,
    pub selected: bool,
    /// Locked (#502): the canvas does not pick it.
    pub locked: bool,
    /// The name the user gave it, if any (`label` falls back to the stitch type).
    pub name: Option<String>,
}

/// Longest name the list takes (the engine's `object.setName` limit).
pub const MAX_NAME: usize = openstitch_engine::order::MAX_NAME;

pub fn label(o: &DesignObject) -> String {
    o.name.clone().unwrap_or_else(|| match o.kind {
        ObjectKind::Shape => format!("Shape {}", o.id),
        ObjectKind::Stitches => format!("Stitches {}", o.id),
        ObjectKind::Text => match &o.text {
            Some(t) => format!("“{}”", t.text.chars().take(24).collect::<String>()),
            None => format!("Text {}", o.id),
        },
    })
}

fn detail(o: &DesignObject) -> String {
    match o.kind {
        ObjectKind::Stitches => {
            let n = o.stitch_block.as_ref().map_or(0, |b| b.stitches.len());
            format!("Stitch pattern · {n} points")
        }
        ObjectKind::Shape | ObjectKind::Text => {
            let region = o
                .region
                .as_ref()
                .map(|r| super::inspector::region_name(Some(r.kind)));
            let line = o
                .line
                .as_ref()
                .map(|l| super::inspector::line_name(Some(l.kind)));
            match (region, line) {
                (Some(r), Some(l)) => format!("{r} + {l}"),
                (Some(r), None) => r.to_string(),
                (None, Some(l)) => l.to_string(),
                (None, None) => "not sewn".into(),
            }
        }
    }
}

fn swatches(o: &DesignObject) -> Vec<Thread> {
    let mut v: Vec<Thread> = Vec::new();
    let threads = o
        .region
        .iter()
        .map(|r| &r.thread)
        .chain(o.line.iter().map(|l| &l.thread));
    let threads: Vec<_> = threads
        .chain(o.stitch_block.iter().flat_map(|b| b.threads.iter()))
        .collect();
    for t in threads {
        if !v.iter().any(|x| x.rgb_value() == t.rgb_value()) {
            v.push(t.clone());
        }
    }
    v
}

/// Rows in sewing order (first sewn first).
pub fn rows(design: &Design, selection: &BTreeSet<u64>) -> Vec<ObjectRow> {
    design
        .objects
        .iter()
        .enumerate()
        .map(|(index, o)| ObjectRow {
            id: o.id,
            index,
            label: label(o),
            detail: detail(o),
            swatches: swatches(o),
            visible: o.visible,
            selected: selection.contains(&o.id),
            locked: o.locked,
            name: o.name.clone(),
        })
        .collect()
}

/// `object.reorder` moving `id` by `delta` places in the sewing order, if it can move.
pub fn shift(design: &Design, id: u64, delta: i64) -> Option<CommandRequest> {
    let from = design.index_of(id)? as i64;
    let to = from + delta;
    if delta == 0 || to < 0 || to >= design.objects.len() as i64 {
        return None;
    }
    Some(reorder(id, to as usize))
}

pub fn reorder(id: u64, index: usize) -> CommandRequest {
    CommandRequest::new("object.reorder", json!({ "id": id, "index": index }))
}

/// Target index when a row dragged from `from` is dropped above row `before` (`len` = at the end).
pub fn drop_index(from: usize, before: usize) -> usize {
    if before > from { before - 1 } else { before }
}

pub fn set_visible(id: u64, visible: bool) -> CommandRequest {
    CommandRequest::new("object.setVisible", json!({ "id": id, "visible": visible }))
}

/// `object.setName` (#502); an empty name goes back to the stitch-type label.
pub fn set_name(id: u64, name: &str) -> CommandRequest {
    CommandRequest::new("object.setName", json!({ "id": id, "name": name.trim() }))
}

pub fn set_locked(ids: &BTreeSet<u64>, locked: bool) -> CommandRequest {
    CommandRequest::new(
        "object.setLocked",
        json!({ "ids": CommandRequest::ids(ids), "locked": locked }),
    )
}

/// `object.setPartOrder` for `ids`: the outline first or PE-DESIGN's region first.
pub fn set_line_first(ids: &[u64], line_first: bool) -> CommandRequest {
    let order = if line_first {
        "lineFirst"
    } else {
        "regionFirst"
    };
    CommandRequest::new("object.setPartOrder", json!({ "ids": ids, "order": order }))
}

/// `object.setStopAfter` (#512): the machine stops after these objects.
/// `command` for the objects `ids` with `params` (an object) added.
pub fn set_flag(ids: &BTreeSet<u64>, command: &'static str, params: Value) -> CommandRequest {
    let mut p = params;
    p["ids"] = json!(CommandRequest::ids(ids));
    CommandRequest::new(command, p)
}

pub fn set_stop_after(ids: &BTreeSet<u64>, on: bool) -> CommandRequest {
    CommandRequest::new(
        "object.setStopAfter",
        json!({ "ids": CommandRequest::ids(ids), "on": on }),
    )
}

/// `wizard.knockdown` (#510): rebuild the selected knockdown, else one under the selection
/// (the whole design when nothing is selected), with the default settings.
pub fn knockdown(d: &openstitch_design::Design, selection: &BTreeSet<u64>) -> CommandRequest {
    let existing = d
        .objects
        .iter()
        .find(|o| selection.contains(&o.id) && o.knockdown.is_some());
    let params = match existing {
        Some(o) => json!({ "update": o.id }),
        None if selection.is_empty() => json!({}),
        None => json!({ "ids": CommandRequest::ids(selection) }),
    };
    CommandRequest::new("wizard.knockdown", params)
}

/// `order.byClickOrder` of the selection in the order it was clicked; `None` below 2 objects.
pub fn click_order(order: &[u64]) -> Option<CommandRequest> {
    (order.len() >= 2).then(|| CommandRequest::new("order.byClickOrder", json!({ "ids": order })))
}

/// Keeps `order` as the selection in the order its members were added: drops what left the
/// selection and appends what joined it (several at once, by id).
pub fn track_click_order(order: &mut Vec<u64>, selection: &BTreeSet<u64>) {
    order.retain(|id| selection.contains(id));
    for id in selection {
        if !order.contains(id) {
            order.push(*id);
        }
    }
}

/// Shapes among `ids` that have both a region and an outline, so their part order matters.
pub fn two_part_shapes(design: &Design, ids: &BTreeSet<u64>) -> Vec<u64> {
    design
        .objects
        .iter()
        .filter(|o| ids.contains(&o.id) && o.kind == ObjectKind::Shape)
        .filter(|o| o.region.is_some() && o.line.is_some())
        .map(|o| o.id)
        .collect()
}

/// Whether any of `ids` is locked (#502): the canvas and Delete leave them alone.
pub fn any_locked(design: &Design, ids: &BTreeSet<u64>) -> bool {
    design
        .objects
        .iter()
        .any(|o| o.locked && ids.contains(&o.id))
}

/// `order.move` of `ids` to `to` (first, last, earlier, later).
pub fn move_to(ids: &BTreeSet<u64>, to: &str) -> CommandRequest {
    CommandRequest::new(
        "order.move",
        json!({ "ids": CommandRequest::ids(ids), "to": to }),
    )
}

/// `order.move` of `ids` as a block before (or after) `target`.
pub fn move_next_to(ids: &[u64], target: u64, after: bool) -> CommandRequest {
    CommandRequest::new(
        "order.move",
        json!({ "ids": ids, "to": if after { "after" } else { "before" }, "target": target }),
    )
}

/// PE-DESIGN's "Show by color order": consecutive objects starting with the same thread form
/// one frame.
pub fn colour_frames(design: &Design) -> Vec<Vec<u64>> {
    let mut frames: Vec<(Option<[u8; 3]>, Vec<u64>)> = Vec::new();
    for o in &design.objects {
        let c = swatches(o).first().map(|t| [t.red, t.green, t.blue]);
        match frames.last_mut() {
            Some((last, ids)) if *last == c => ids.push(o.id),
            _ => frames.push((c, vec![o.id])),
        }
    }
    frames.into_iter().map(|(_, ids)| ids).collect()
}

/// Moving the frame at `index` one frame earlier (`delta` −1) or later (+1).
pub fn shift_frame(frames: &[Vec<u64>], index: usize, delta: i64) -> Option<CommandRequest> {
    let other = frames.get(usize::try_from(index as i64 + delta).ok()?)?;
    let frame = frames.get(index)?;
    Some(if delta < 0 {
        move_next_to(frame, other[0], false)
    } else {
        move_next_to(frame, *other.last()?, true)
    })
}

#[cfg(test)]
mod order_tests {
    use super::*;
    use openstitch_engine::Session;

    fn run(s: &mut Session, r: &CommandRequest) {
        s.execute(r.command, Some(&r.params))
            .unwrap_or_else(|e| panic!("{}: {e}", r.command));
    }

    fn ids(s: &Session) -> Vec<u64> {
        s.design().objects.iter().map(|o| o.id).collect()
    }

    #[test]
    fn colour_frames_move_as_blocks() {
        let mut s = Session::new(std::env::temp_dir());
        for (k, c) in ["Red", "Red", "Blue", "Red", "Blue", "Blue"]
            .iter()
            .enumerate()
        {
            let r = CommandRequest::new(
                "shape.rectangle",
                json!({ "x": k * 12, "y": 0, "width": 10, "height": 10, "fill": c }),
            );
            run(&mut s, &r);
        }
        let frames = colour_frames(s.design());
        assert_eq!(frames, vec![vec![1, 2], vec![3], vec![4], vec![5, 6]]);
        assert!(shift_frame(&frames, 0, -1).is_none());
        assert!(shift_frame(&frames, 3, 1).is_none());
        run(&mut s, &shift_frame(&frames, 3, -1).expect("earlier"));
        assert_eq!(ids(&s), [1, 2, 3, 5, 6, 4]);
        let frames = colour_frames(s.design());
        assert_eq!(frames, vec![vec![1, 2], vec![3, 5, 6], vec![4]]);
        run(&mut s, &shift_frame(&frames, 0, 1).expect("later"));
        assert_eq!(ids(&s), [3, 5, 6, 1, 2, 4]);
        run(&mut s, &move_to(&BTreeSet::from([4]), "first"));
        assert_eq!(ids(&s), [4, 3, 5, 6, 1, 2]);
    }
}
