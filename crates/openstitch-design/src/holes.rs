//! Hole sewing (PE-DESIGN): a pair of regions where the outer one fully encloses the inner
//! one. The outer region is sewn around the inner shape instead of under it. The outer object
//! keeps the inner object's id ([`DesignObject::hole_inner`]); designs saved before the pair
//! was stored pair the outer object with the first region sewn after it that it encloses.

use crate::boolean::{self, BoolOp};
use crate::geometry::Contour;
use crate::model::{Design, DesignObject};

/// Relative area tolerance of the enclosure test (numerical slivers along shared edges).
const TOLERANCE: f64 = 0.01;

/// The closed contours of `o` when it is an outline object with region sewing.
pub fn region_contours(o: &DesignObject) -> Option<Vec<Contour>> {
    if !o.is_outline() || o.region.is_none() {
        return None;
    }
    let closed: Vec<Contour> = o.contours.iter().filter(|c| c.closed).cloned().collect();
    (!closed.is_empty()).then_some(closed)
}

fn area(contours: &[Contour]) -> f64 {
    boolean::simplify(contours)
        .iter()
        .map(|r| boolean::region_area(r))
        .sum()
}

/// Whether the region `outer` fully encloses the region `inner`: removing `inner` from
/// `outer` takes away exactly the area of `inner` (±1 %), and `inner` is the smaller one.
pub fn encloses(outer: &[Contour], inner: &[Contour]) -> bool {
    let (a_outer, a_inner) = (area(outer), area(inner));
    if a_inner <= 0.0 || a_inner >= a_outer {
        return false;
    }
    let rest: f64 = boolean::combine(outer, inner, BoolOp::Difference)
        .iter()
        .map(|r| boolean::region_area(r))
        .sum();
    ((a_outer - rest) - a_inner).abs() <= TOLERANCE * a_inner
}

/// Whether region object `outer` fully encloses region object `inner`.
pub fn object_encloses(outer: &DesignObject, inner: &DesignObject) -> bool {
    match (region_contours(outer), region_contours(inner)) {
        (Some(a), Some(b)) => encloses(&a, &b),
        _ => false,
    }
}

/// The object whose shape is cut out of `object` when it is sewn: its stored pair, or for
/// designs saved before the pair was stored, the first region sewn after it that it encloses.
pub fn paired_inner<'a>(object: &DesignObject, design: &'a Design) -> Option<&'a DesignObject> {
    if !object.hole_sewing {
        return None;
    }
    if let Some(id) = object.hole_inner {
        return design.index_of(id).map(|i| &design.objects[i]);
    }
    let i = design.index_of(object.id)?;
    design.objects[i + 1..]
        .iter()
        .find(|o| o.visible && object_encloses(object, o))
}

/// The other object of the hole-sewing pair object `id` belongs to (#1271): the inner object
/// when `id` is sewn around one, or the object that is sewn around `id`. `None` when `id` is
/// in no pair.
pub fn partner(design: &Design, id: u64) -> Option<&DesignObject> {
    let i = design.index_of(id)?;
    if let Some(inner) = paired_inner(&design.objects[i], design) {
        return Some(inner);
    }
    design
        .objects
        .iter()
        .find(|o| paired_inner(o, design).is_some_and(|inner| inner.id == id))
}

/// Whether segment `a`-`b` properly crosses segment `c`-`d` (touching ends do not count).
fn segments_cross(a: crate::Vec2, b: crate::Vec2, c: crate::Vec2, d: crate::Vec2) -> bool {
    let side = |p: crate::Vec2, q: crate::Vec2, r: crate::Vec2| {
        (q.x - p.x) * (r.y - p.y) - (q.y - p.y) * (r.x - p.x)
    };
    let (d1, d2) = (side(c, d, a), side(c, d, b));
    let (d3, d4) = (side(a, b, c), side(a, b, d));
    d1 * d2 < 0.0 && d3 * d4 < 0.0
}

/// Whether an outline of `a` crosses an outline of `b` (curves flattened): what a point of a
/// hole-sewn shape must not be moved into, since the outer region is sewn around the inner
/// shape (PE-DESIGN's manual: such a point cannot be dragged across an outline).
pub fn outlines_cross(a: &[Contour], b: &[Contour]) -> bool {
    let paths = |cs: &[Contour]| cs.iter().map(Contour::path).collect::<Vec<_>>();
    let (pa, pb) = (paths(a), paths(b));
    pa.iter().any(|p| {
        p.windows(2).any(|s| {
            pb.iter()
                .any(|q| q.windows(2).any(|t| segments_cross(s[0], s[1], t[0], t[1])))
        })
    })
}

/// Whether changing object `id`'s outlines from what they are in `design` to `edited` moves
/// one across an outline of its hole-sewing partner: they cross afterwards and did not
/// before. False for an object in no pair.
pub fn moves_across_partner(design: &Design, id: u64, edited: &[Contour]) -> bool {
    let (Some(i), Some(other)) = (design.index_of(id), partner(design, id)) else {
        return false;
    };
    outlines_cross(edited, &other.contours)
        && !outlines_cross(&design.objects[i].contours, &other.contours)
}
