//! The precision bar (#474): the selection's X, Y, width and height in mm, typed exactly,
//! plus a rotation and a skew applied about the reference point. Each value maps to one engine
//! command (`object.move`, `object.scale`, `object.rotate`, `object.skew`), so a commit is one
//! undo step and the Assistant reaches the same edits through the engine.

use super::request::{CommandRequest, mm};
use openstitch_design::{Rect, Vec2, v2};
use serde_json::json;
use std::collections::BTreeSet;

/// One field of the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Field {
    X,
    Y,
    Width,
    Height,
    /// Degrees clockwise, applied when committed (the field then shows 0 again).
    Rotate,
    /// Horizontal skew in degrees, applied when committed.
    Skew,
}

impl Field {
    pub const ALL: [Field; 6] = [
        Field::X,
        Field::Y,
        Field::Width,
        Field::Height,
        Field::Rotate,
        Field::Skew,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Field::X => "X",
            Field::Y => "Y",
            Field::Width => "W",
            Field::Height => "H",
            Field::Rotate => "Rotate",
            Field::Skew => "Skew",
        }
    }

    pub fn unit(self) -> &'static str {
        match self {
            Field::Rotate | Field::Skew => "°",
            _ => "mm",
        }
    }
}

/// How the bar measures: the reference point (top-left corner or centre, as PE-DESIGN's move
/// dialog) and whether width and height scale together.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Options {
    pub centre: bool,
    pub lock: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            centre: false,
            lock: true,
        }
    }
}

/// The reference point of `b`.
pub fn reference(b: Rect, o: Options) -> Vec2 {
    if o.centre {
        b.center()
    } else {
        v2(b.min_x, b.min_y)
    }
}

/// What a field shows for selection bounds `b` (mm or degrees).
pub fn value(f: Field, b: Rect, o: Options) -> f64 {
    let r = reference(b, o);
    match f {
        Field::X => mm(r.x),
        Field::Y => mm(r.y),
        Field::Width => mm(b.width()),
        Field::Height => mm(b.height()),
        Field::Rotate | Field::Skew => 0.0,
    }
}

/// The commands that give the selection `ids` (bounds `b`) the typed `v`; empty when nothing
/// changes or the value cannot apply (a zero size, a non-finite number).
pub fn requests(f: Field, v: f64, ids: &BTreeSet<u64>, b: Rect, o: Options) -> Vec<CommandRequest> {
    if ids.is_empty() || !v.is_finite() {
        return Vec::new();
    }
    let r = reference(b, o);
    let origin = json!({ "originX": mm(r.x), "originY": mm(r.y) });
    let with = |command: &'static str, mut params: serde_json::Value| {
        params["ids"] = CommandRequest::ids(ids);
        params["originX"] = origin["originX"].clone();
        params["originY"] = origin["originY"].clone();
        vec![CommandRequest::new(command, params)]
    };
    match f {
        Field::X | Field::Y => {
            let d = v * 10.0 - if f == Field::X { r.x } else { r.y };
            if mm(d) == 0.0 {
                return Vec::new();
            }
            let (dx, dy) = if f == Field::X { (d, 0.0) } else { (0.0, d) };
            vec![CommandRequest::move_by(ids, dx, dy)]
        }
        Field::Width | Field::Height => {
            let size = if f == Field::Width {
                b.width()
            } else {
                b.height()
            };
            if size <= 0.0 || v <= 0.0 {
                return Vec::new();
            }
            let k = v * 10.0 / size;
            if (k - 1.0).abs() < 1e-9 {
                return Vec::new();
            }
            let other = if o.lock { k } else { 1.0 };
            let (sx, sy) = if f == Field::Width {
                (k, other)
            } else {
                (other, k)
            };
            with("object.scale", json!({ "sx": sx, "sy": sy }))
        }
        Field::Rotate if v != 0.0 => with("object.rotate", json!({ "degrees": v })),
        Field::Skew if v != 0.0 => with("object.skew", json!({ "angleX": v })),
        Field::Rotate | Field::Skew => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_engine::Session;

    fn session() -> (Session, BTreeSet<u64>) {
        let mut s = Session::new(std::env::temp_dir());
        let r = s
            .execute(
                "shape.rectangle",
                Some(&json!({ "x": 10, "y": 20, "width": 40, "height": 20 })),
            )
            .expect("rectangle");
        (s, CommandRequest::created_ids(&r))
    }

    fn apply(s: &mut Session, f: Field, v: f64, ids: &BTreeSet<u64>, o: Options) {
        let b = s.design().objects[0].bounds().expect("bounds");
        for r in requests(f, v, ids, b, o) {
            s.execute(r.command, Some(&r.params)).expect("command");
        }
    }

    fn bounds(s: &Session) -> Rect {
        s.design().objects[0].bounds().expect("bounds")
    }

    #[test]
    fn width_with_lock_keeps_proportions_and_without_changes_only_width() {
        let (mut s, ids) = session();
        let o = Options::default();
        apply(&mut s, Field::Width, 80.0, &ids, o);
        let b = bounds(&s);
        assert!((mm(b.width()) - 80.0).abs() < 0.005, "{b:?}");
        assert!((mm(b.height()) - 40.0).abs() < 0.005, "height follows");
        assert_eq!((b.min_x, b.min_y), (100.0, 200.0), "top-left stays put");
        let unlocked = Options { lock: false, ..o };
        apply(&mut s, Field::Height, 10.0, &ids, unlocked);
        let b = bounds(&s);
        assert!((mm(b.width()) - 80.0).abs() < 0.005 && (mm(b.height()) - 10.0).abs() < 0.005);
    }

    #[test]
    fn x_and_y_move_the_reference_point_exactly() {
        let (mut s, ids) = session();
        let centre = Options {
            centre: true,
            lock: true,
        };
        apply(&mut s, Field::X, 50.0, &ids, centre);
        apply(&mut s, Field::Y, 50.0, &ids, centre);
        let c = bounds(&s).center();
        assert_eq!((mm(c.x), mm(c.y)), (50.0, 50.0));
        apply(&mut s, Field::X, 12.5, &ids, Options::default());
        assert_eq!(bounds(&s).min_x, 125.0);
        assert_eq!(value(Field::X, bounds(&s), Options::default()), 12.5);
        assert!(requests(Field::X, 12.5, &ids, bounds(&s), Options::default()).is_empty());
    }

    #[test]
    fn rotation_matches_object_rotate_and_skew_leans() {
        let (mut s, ids) = session();
        let mut direct = s.design().clone();
        let b = bounds(&s);
        let c = b.center();
        direct.objects[0].apply(&openstitch_design::Transform::rotation(12.0, c));
        apply(
            &mut s,
            Field::Rotate,
            12.0,
            &ids,
            Options {
                centre: true,
                lock: true,
            },
        );
        assert_eq!(s.design().objects[0].contours, direct.objects[0].contours);
        apply(&mut s, Field::Skew, 20.0, &ids, Options::default());
        assert!(bounds(&s).width() > b.width());
        assert!(requests(Field::Width, 0.0, &ids, b, Options::default()).is_empty());
        assert!(requests(Field::Rotate, f64::NAN, &ids, b, Options::default()).is_empty());
    }
}
