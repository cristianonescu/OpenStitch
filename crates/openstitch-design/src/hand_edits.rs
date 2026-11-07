//! Hand edits kept on a live object (#501, beyond PE-DESIGN): needle points moved, added or
//! deleted on a fill, satin, line or text without converting it to a stitch pattern. The object
//! stays editable; its edited stitches are sewn instead of the generated ones while nothing
//! that generates them has changed. Moving the object keeps the edits (stitches are generated
//! relative to the object's bounds, see `compile::sew_object`); a sewing setting, a reshape, a
//! rotation or a resize makes them stale, and [`crate::Design::drop_stale_hand_edits`] drops
//! them. `object.regenerate` drops them on purpose.

use crate::model::{Design, DesignObject, StitchBlock};
use crate::{Transform, Vec2, v2};
use serde::{Deserialize, Serialize};

/// Most needle points kept on one object (as many as a large stitch pattern).
pub const MAX_STITCHES: usize = 1_000_000;

/// An object's edited stitches.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HandEdits {
    /// [`key`] of the object the edits were made on.
    pub key: u64,
    /// Where the object's bounds started when the stitches were stored (model units).
    pub anchor: Vec2,
    /// The stitches, at `anchor`, and their threads.
    pub block: StitchBlock,
}

/// Where `o`'s stitches are generated from: its bounds' top-left corner.
pub fn anchor(o: &DesignObject) -> Vec2 {
    o.bounds().map_or(v2(0.0, 0.0), |b| v2(b.min_x, b.min_y))
}

/// FNV-1a: stable across builds and Rust versions (the key is saved in `.osd` files).
fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    h
}

/// A key of everything that generates `o`'s stitches, independent of where `o` is: its shape
/// moved to the origin, its sewing, text, hole sewing and finish. Not its id, name, group,
/// visibility, lock or stop.
pub fn key(o: &DesignObject) -> u64 {
    let mut c = o.clone();
    let a = anchor(o);
    if a != v2(0.0, 0.0) {
        c.apply(&Transform::translation(-a.x, -a.y));
    }
    c.id = 0;
    c.name = None;
    c.group = None;
    c.visible = true;
    c.locked = false;
    c.stop_after = false;
    c.connector_run = false;
    c.hand_edits = None;
    fnv(serde_json::to_string(&c).unwrap_or_default().as_bytes())
}

impl HandEdits {
    /// Edits of `o`'s stitches `block` (absolute positions, as `o` sews now).
    pub fn new(o: &DesignObject, block: StitchBlock) -> Self {
        HandEdits {
            key: key(o),
            anchor: anchor(o),
            block,
        }
    }

    /// Moves the stored stitches to `to`, the object's current anchor.
    pub fn rebase(&mut self, to: Vec2) {
        let d = to - self.anchor;
        if d != v2(0.0, 0.0) {
            for s in &mut self.block.stitches {
                s.x += d.x;
                s.y += d.y;
            }
            self.anchor = to;
        }
    }

    /// Whether the stored stitches are finite and within [`MAX_STITCHES`] (files are not
    /// trusted).
    pub fn is_valid(&self) -> bool {
        self.block.stitches.len() <= MAX_STITCHES
            && self.anchor.x.is_finite()
            && self.anchor.y.is_finite()
            && self
                .block
                .stitches
                .iter()
                .all(|s| s.x.is_finite() && s.y.is_finite())
    }
}

/// `o`'s hand edits when they still apply, moved to where `o` is now.
pub fn current(o: &DesignObject) -> Option<StitchBlock> {
    let h = o.hand_edits.as_ref()?;
    if h.key != key(o) {
        return None;
    }
    let mut h = h.clone();
    h.rebase(anchor(o));
    Some(h.block)
}

impl Design {
    /// Drops the hand edits that no longer apply (a sewing setting or the shape changed);
    /// returns the objects that lost them.
    pub fn drop_stale_hand_edits(&mut self) -> Vec<u64> {
        let mut dropped = Vec::new();
        for o in &mut self.objects {
            if o.hand_edits.as_ref().is_some_and(|h| h.key != key(o)) {
                o.hand_edits = None;
                dropped.push(o.id);
            }
        }
        dropped
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RegionSewing, RegionType};
    use crate::{Compiler, Contour};
    use openstitch_core::{Command, Stitch, Thread};

    fn square(x: f64) -> DesignObject {
        let c = Contour::new(
            vec![
                v2(x, 0.0),
                v2(x + 200.0, 0.0),
                v2(x + 200.0, 200.0),
                v2(x, 200.0),
            ],
            true,
        );
        DesignObject::shape(
            vec![c],
            Some(RegionSewing::new(RegionType::Fill, Thread::rgb(0x0000ff))),
            None,
        )
    }

    #[test]
    fn the_key_ignores_where_the_object_is_but_not_its_settings() {
        let a = square(0.0);
        let mut b = square(500.0);
        b.name = Some("moved".into());
        assert_eq!(key(&a), key(&b), "moved and renamed: same key");
        let mut c = square(0.0);
        c.region.as_mut().expect("region").density = 2.0;
        assert_ne!(key(&a), key(&c), "another density: another key");
    }

    #[test]
    fn edits_are_sewn_follow_a_move_and_go_stale_on_a_setting() {
        let mut d = Design::default();
        let id = d.add(square(0.0));
        let generated = Compiler::for_design(&d).compile(&d);
        let block = StitchBlock {
            stitches: vec![
                Stitch::new(10.0, 10.0, Command::Stitch),
                Stitch::new(150.0, 30.0, Command::Stitch),
            ],
            threads: vec![Thread::rgb(0x0000ff)],
        };
        d.objects[0].hand_edits = Some(Box::new(HandEdits::new(&d.objects[0], block)));
        let edited = Compiler::for_design(&d).compile(&d);
        assert_ne!(edited, generated);
        assert!(
            edited
                .stitches
                .iter()
                .any(|s| s.x == 150.0 && s.y == 30.0 && s.command == Command::Stitch),
            "the edited point is sewn"
        );
        // Moved 50 mm right: the edits move along.
        d.objects[0].apply(&Transform::translation(500.0, 0.0));
        let moved = Compiler::for_design(&d).compile(&d);
        assert!(moved.stitches.iter().any(|s| s.x == 650.0 && s.y == 30.0));
        assert!(d.drop_stale_hand_edits().is_empty());
        // Another density: the edits no longer apply and are dropped.
        d.objects[0].region.as_mut().expect("r").density = 2.0;
        assert_eq!(d.drop_stale_hand_edits(), vec![id]);
        assert!(d.objects[0].hand_edits.is_none());
    }
}
