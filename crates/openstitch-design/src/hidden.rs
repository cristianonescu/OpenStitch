//! Stitching hidden under later objects, beyond PE-DESIGN and off by default:
//! - leave out a region's stitches that later fills and satins cover, keeping an overlap under
//!   their edges, without cutting the shape (#511);
//! - travel between objects of one thread as running stitches hidden under fills sewn later,
//!   instead of a jump and trim (#545).
//!
//! Both only change what the compiler sews; the objects stay as drawn.

use crate::boolean::{self, BoolOp};
use crate::geometry::{Contour, Vec2};
use crate::model::{Design, DesignObject, ObjectKind, RegionType};
use crate::paths;

/// Model units (0.1 mm) per mm.
const MM: f64 = 10.0;
/// Travel stays this far inside the fills that hide it (0.1 mm).
pub const TRAVEL_INSET: f64 = 5.0;
/// Running stitch length of hidden travel and run connectors (0.1 mm).
pub const TRAVEL_STITCH: f64 = 20.0;
/// A run connector at least this long (0.1 mm) is reported: it shows on the fabric.
pub const LONG_CONNECTOR: f64 = 150.0;

/// Whether `o` covers what is sewn before it: a visible shape or text with an opaque region.
fn opaque(o: &DesignObject) -> bool {
    o.visible
        && o.is_outline()
        && o.has_closed_contour()
        && o.region.as_ref().is_some_and(|r| {
            matches!(
                r.kind,
                RegionType::Fill | RegionType::Satin | RegionType::ProgrammableFill
            )
        })
}

fn closed(o: &DesignObject) -> Vec<Contour> {
    o.contours.iter().filter(|c| c.closed).cloned().collect()
}

/// The union of the opaque objects among `objects`, shrunk by `inset` (0.1 mm). Empty when
/// none.
pub fn covered<'a>(
    objects: impl IntoIterator<Item = &'a DesignObject>,
    inset: f64,
) -> Vec<Contour> {
    let parts: Vec<Vec<Contour>> = objects
        .into_iter()
        .filter(|o| opaque(o))
        .map(closed)
        .collect();
    if parts.is_empty() {
        return Vec::new();
    }
    let union = boolean::contours(&boolean::union_all(parts.iter().map(|p| &p[..])));
    if inset > 0.0 {
        boolean::contours(&boolean::offset(&union, -inset, false))
    } else {
        union
    }
}

/// For each object, the opaque objects from it to the end of the sewing order, united, and the
/// same shrunk by [`TRAVEL_INSET`]: what hides the move into it (#545). Built from the end, one
/// union per object.
pub fn later_cover(objects: &[DesignObject]) -> Vec<Cover> {
    let mut acc: Vec<Contour> = Vec::new();
    let mut out = vec![Cover::default(); objects.len()];
    for (k, o) in objects.iter().enumerate().rev() {
        if opaque(o) {
            acc = boolean::contours(&boolean::combine(&acc, &closed(o), BoolOp::Union));
            out[k] = Cover {
                inset: boolean::contours(&boolean::offset(&acc, -TRAVEL_INSET, false)),
                area: acc.clone(),
            };
        } else if let Some(next) = out.get(k + 1).cloned() {
            out[k] = next;
        }
    }
    out
}

/// What hides travel: the fills' area, and the same 0.5 mm in from its edges.
#[derive(Clone, Debug, Default)]
pub struct Cover {
    pub area: Vec<Contour>,
    pub inset: Vec<Contour>,
}

/// The overlap (mm) object `index` keeps under later objects when its hidden stitching is left
/// out: its own setting, else the design's for shapes (text only by its own setting). Satin
/// regions are not clipped: their columns follow the whole shape.
pub fn hide_overlap(d: &Design, index: usize) -> Option<f64> {
    let o = d.objects.get(index)?;
    let r = o.region.as_ref()?;
    if r.kind == RegionType::Satin || !o.has_closed_contour() {
        return None;
    }
    o.hide_under.or(match o.kind {
        ObjectKind::Shape => d.production.remove_hidden,
        _ => None,
    })
}

/// The area object `index`'s region is sewn over when its hidden stitching is left out: its
/// outline minus the later opaque objects shrunk by the overlap. `None` when nothing later
/// covers it (sewn as drawn).
pub fn visible_area(d: &Design, index: usize) -> Option<Vec<Contour>> {
    let overlap = hide_overlap(d, index)? * MM;
    let cover = covered(d.objects.iter().skip(index + 1), overlap);
    if cover.is_empty() {
        return None;
    }
    let own = closed(&d.objects[index]);
    let hidden = boolean::combine(&own, &cover, BoolOp::Intersection);
    if hidden.iter().map(|r| boolean::region_area(r)).sum::<f64>() < 1.0 {
        return None;
    }
    Some(boolean::contours(&boolean::combine(
        &own,
        &cover,
        BoolOp::Difference,
    )))
}

/// Whether the segment `a`–`b` lies inside `area` (even-odd contours).
fn inside(a: Vec2, b: Vec2, area: &[Contour]) -> bool {
    let pieces = paths::clip(&[a, b], area, &[]);
    pieces.len() == 1
        && pieces[0].len() >= 2
        && pieces[0][0].distance(a) < 0.5
        && pieces[0][pieces[0].len() - 1].distance(b) < 0.5
}

/// The leg `a`–`b` of hidden travel: inside the fills (`cover`), and inside `inset` (the fills
/// shrunk by [`TRAVEL_INSET`]) except within 1 mm of an end that is not a corner of the path,
/// where it leaves the previous object or meets the next one's first stitch on its edge.
fn leg(a: Vec2, b: Vec2, cover: &[Contour], inset: &[Contour], free_a: bool, free_b: bool) -> bool {
    let len = a.distance(b);
    let end = 10.0_f64.min(len / 2.0);
    let (ta, tb) = (
        if free_a { end / len } else { 0.0 },
        if free_b { end / len } else { 0.0 },
    );
    inside(a, b, cover) && inside(a.lerp(b, ta), a.lerp(b, 1.0 - tb), inset)
}

/// A path from `a` to `b` hidden under `cover` (the fills sewn later) and 0.5 mm in from its
/// edges (`inset`, [`later_cover`]): straight, or through one corner of `inset` (an L round a
/// bend); `None` when neither stays inside.
pub fn path_inside(a: Vec2, b: Vec2, cover: &[Contour], inset: &[Contour]) -> Option<Vec<Vec2>> {
    if inset.is_empty() || a.distance(b) < 1e-6 {
        return None;
    }
    if leg(a, b, cover, inset, true, true) {
        return Some(vec![a, b]);
    }
    inset
        .iter()
        .flat_map(Contour::outline_points)
        .filter(|&v| leg(a, v, cover, inset, true, false) && leg(v, b, cover, inset, false, true))
        .map(|v| (a.distance(v) + v.distance(b), v))
        .min_by(|x, y| x.0.total_cmp(&y.0))
        .map(|(_, v)| vec![a, v, b])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::v2;

    #[test]
    fn an_l_shaped_area_is_travelled_round_its_bend() {
        // An L: the left bar and the top bar.
        let l = Contour::new(
            vec![
                v2(0.0, 0.0),
                v2(100.0, 0.0),
                v2(100.0, 300.0),
                v2(300.0, 300.0),
                v2(300.0, 400.0),
                v2(0.0, 400.0),
            ],
            true,
        );
        let a = v2(50.0, 20.0);
        let b = v2(280.0, 350.0);
        let cover = std::slice::from_ref(&l);
        let inset = boolean::contours(&boolean::offset(cover, -TRAVEL_INSET, false));
        let p = path_inside(a, b, cover, &inset).expect("a path");
        assert_eq!(p.len(), 3);
        assert!(
            path_inside(a, v2(250.0, 100.0), cover, &inset).is_none(),
            "outside"
        );
    }
}
