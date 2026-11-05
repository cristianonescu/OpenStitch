//! Align and distribute (#472), skew (#474) and select-same matching (#475). Align and
//! distribute only translate, so stitches move with their objects and are never regenerated;
//! a group, and a text with the outline it is fitted to, move as one unit.

use crate::commands::{CommandSpec, Effect, IDS, MM, indexes, opt, req, spec, thread};
use crate::params::{CommandError, Params};
use openstitch_core::Command;
use openstitch_design::{Design, DesignObject, Rect, Transform, Vec2, v2};
use serde_json::json;

pub(crate) fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "object.align",
            "Align objects' edges or centres with each other, with one key object or with the Design Page; groups move whole",
            Effect::Edit,
            vec![
                IDS,
                req(
                    "edge",
                    "left|right|top|bottom|centerX|centerY",
                    "What lines up: an edge, or the horizontal (centerX) or vertical (centerY) centres",
                ),
                opt(
                    "to",
                    "selection|key|page",
                    "Reference: the objects' common bounds (default), one key object that stays put, or the Design Page",
                ),
                opt(
                    "keyId",
                    "int",
                    "to=key: the object that stays put (default the last of ids)",
                ),
            ],
            |s, p| {
                let ids = expanded(&s.design, p)?;
                let units = units(&s.design, &ids);
                let to = p.string("to", Some("selection"))?;
                let edge = p.string("edge", None)?;
                let line = |b: Rect| -> Result<f64, CommandError> {
                    Ok(match edge.as_str() {
                        "left" => b.min_x,
                        "right" => b.max_x,
                        "top" => b.min_y,
                        "bottom" => b.max_y,
                        "centerX" => b.center().x,
                        "centerY" => b.center().y,
                        _ => {
                            return Err(p.invalid(
                                "edge",
                                "must be left, right, top, bottom, centerX or centerY",
                            ));
                        }
                    })
                };
                let horizontal = matches!(edge.as_str(), "left" | "right" | "centerX");
                let (target, fixed) = match to.as_str() {
                    "page" => {
                        let page = Rect {
                            min_x: 0.0,
                            min_y: 0.0,
                            max_x: s.design.page.width,
                            max_y: s.design.page.height,
                        };
                        (line(page)?, None)
                    }
                    "selection" | "key" => {
                        if units.len() < 2 {
                            return Err(p.failed(
                                "align needs at least two objects or groups (or to=page)",
                            ));
                        }
                        if to == "selection" {
                            let all = Rect::enclosing(units.iter().flat_map(|u| corners(u.1)));
                            (line(all.expect("units have bounds"))?, None)
                        } else {
                            let raw = p.ids()?;
                            let key = match p.get("keyId") {
                                Some(_) => p.int("keyId", None, 0, i64::MAX)? as u64,
                                None => *raw.last().expect("ids are not empty"),
                            };
                            let u = units
                                .iter()
                                .position(|u| u.0.contains(&key))
                                .ok_or_else(|| p.invalid("keyId", "must be one of ids"))?;
                            (line(units[u].1)?, Some(u))
                        }
                    }
                    _ => return Err(p.invalid("to", "must be selection, key or page")),
                };
                for (k, (members, b)) in units.iter().enumerate() {
                    if Some(k) == fixed {
                        continue;
                    }
                    let d = target - line(*b)?;
                    let t = if horizontal {
                        Transform::translation(d, 0.0)
                    } else {
                        Transform::translation(0.0, d)
                    };
                    apply(&mut s.design, members, &t);
                }
                Ok(json!({ "units": units.len() }))
            },
        ),
        spec(
            "object.distribute",
            "Space three or more objects evenly: equal gaps or equal centre spacing; the outermost two stay put",
            Effect::Edit,
            vec![
                IDS,
                req("axis", "x|y", "x: left to right, y: top to bottom"),
                opt(
                    "by",
                    "gaps|centers",
                    "gaps: equal edge-to-edge spaces (default); centers: equal centre spacing",
                ),
            ],
            |s, p| {
                let ids = expanded(&s.design, p)?;
                let mut units = units(&s.design, &ids);
                if units.len() < 3 {
                    return Err(p.failed("distribute needs at least three objects or groups"));
                }
                let x = match p.string("axis", None)?.as_str() {
                    "x" => true,
                    "y" => false,
                    _ => return Err(p.invalid("axis", "must be x or y")),
                };
                let span = |b: &Rect| {
                    if x {
                        (b.min_x, b.max_x)
                    } else {
                        (b.min_y, b.max_y)
                    }
                };
                let mid = |b: &Rect| (span(b).0 + span(b).1) / 2.0;
                units.sort_by(|a, b| mid(&a.1).total_cmp(&mid(&b.1)));
                let n = units.len();
                let (first, last) = (units[0].1, units[n - 1].1);
                let targets: Vec<f64> = match p.string("by", Some("gaps"))?.as_str() {
                    "centers" | "centres" => (0..n)
                        .map(|i| {
                            mid(&first) + (mid(&last) - mid(&first)) * i as f64 / (n - 1) as f64
                        })
                        .collect(),
                    "gaps" => {
                        let widths: f64 = units.iter().map(|u| span(&u.1).1 - span(&u.1).0).sum();
                        let gap = (span(&last).1 - span(&first).0 - widths) / (n - 1) as f64;
                        let mut at = span(&first).0;
                        units
                            .iter()
                            .map(|u| {
                                let (lo, hi) = span(&u.1);
                                let c = at + (hi - lo) / 2.0;
                                at += hi - lo + gap;
                                c
                            })
                            .collect()
                    }
                    _ => return Err(p.invalid("by", "must be gaps or centers")),
                };
                for ((members, b), c) in units.iter().zip(targets).take(n - 1).skip(1) {
                    let d = c - mid(b);
                    let t = if x {
                        Transform::translation(d, 0.0)
                    } else {
                        Transform::translation(0.0, d)
                    };
                    apply(&mut s.design, members, &t);
                }
                Ok(json!({ "units": n }))
            },
        ),
        spec(
            "object.skew",
            "Lean objects: angleX slants vertical lines sideways, angleY slants horizontal lines; fill and satin directions turn with the shape",
            Effect::Edit,
            vec![
                IDS,
                opt(
                    "angleX",
                    "degrees",
                    "Horizontal skew, -80..80 (positive leans the bottom to the right)",
                ),
                opt(
                    "angleY",
                    "degrees",
                    "Vertical skew, -80..80 (positive moves the right side down)",
                ),
                opt(
                    "originX",
                    "mm",
                    "Fixed point x (default: centre of the objects)",
                ),
                opt("originY", "mm", "Fixed point y"),
            ],
            |s, p| {
                let idx = indexes(p, &s.design)?;
                let ax = p.opt("angleX", 0.0, -80.0, 80.0)?.to_radians().tan();
                let ay = p.opt("angleY", 0.0, -80.0, 80.0)?.to_radians().tan();
                let Some(b) =
                    Rect::enclosing(idx.iter().flat_map(|&i| s.design.objects[i].points()))
                else {
                    return Ok(json!(null));
                };
                let o = if p.has("originX") || p.has("originY") {
                    v2(
                        p.req("originX", -5000.0, 5000.0)? * MM,
                        p.req("originY", -5000.0, 5000.0)? * MM,
                    )
                } else {
                    b.center()
                };
                let t = Transform {
                    a: 1.0,
                    b: ay,
                    c: ax,
                    d: 1.0,
                    tx: -ax * o.y,
                    ty: -ay * o.x,
                };
                for i in idx {
                    let obj = &mut s.design.objects[i];
                    obj.apply(&t);
                    if let Some(r) = &mut obj.region {
                        r.angle = turned(r.angle, &t);
                    }
                }
                Ok(json!(null))
            },
        ),
    ]
}

/// A sewing direction (degrees counter-clockwise on screen, as `RegionSewing::angle`) after
/// the linear part of `t`, to 0.01°.
pub(crate) fn turned(angle: f64, t: &Transform) -> f64 {
    let (sn, cs) = angle.to_radians().sin_cos();
    let (x, y) = (cs, -sn);
    let (nx, ny) = (t.a * x + t.c * y, t.b * x + t.d * y);
    let deg = (-ny).atan2(nx).to_degrees().rem_euclid(360.0);
    ((deg * 100.0).round() / 100.0).rem_euclid(360.0)
}

fn expanded(d: &Design, p: &Params) -> Result<Vec<u64>, CommandError> {
    let ids: Vec<u64> = indexes(p, d)?.iter().map(|&i| d.objects[i].id).collect();
    Ok(d.with_groups(&ids))
}

/// What moves as one: each group, a text with the outline it is fitted to (when both are
/// among `ids`), and every other object; with the unit's bounds. Objects without geometry
/// are left out.
fn units(d: &Design, ids: &[u64]) -> Vec<(Vec<u64>, Rect)> {
    let key = |o: &DesignObject| -> (u8, u64) {
        if let Some(g) = o.group {
            return (0, g);
        }
        if let Some(path) = o.text.as_ref().and_then(|t| t.path)
            && ids.contains(&path)
            && let Some(i) = d.index_of(path)
        {
            let shape = &d.objects[i];
            return shape.group.map_or((1, path), |g| (0, g));
        }
        (1, o.id)
    };
    let mut out: Vec<((u8, u64), Vec<u64>)> = Vec::new();
    for o in d.objects.iter().filter(|o| ids.contains(&o.id)) {
        let k = key(o);
        match out.iter_mut().find(|u| u.0 == k) {
            Some(u) => u.1.push(o.id),
            None => out.push((k, vec![o.id])),
        }
    }
    out.into_iter()
        .filter_map(|(_, members)| {
            let b = Rect::enclosing(
                members
                    .iter()
                    .filter_map(|&id| d.index_of(id))
                    .flat_map(|i| d.objects[i].points()),
            )?;
            Some((members, b))
        })
        .collect()
}

fn corners(b: Rect) -> [Vec2; 2] {
    [v2(b.min_x, b.min_y), v2(b.max_x, b.max_y)]
}

fn apply(d: &mut Design, ids: &[u64], t: &Transform) {
    for o in d.objects.iter_mut().filter(|o| ids.contains(&o.id)) {
        o.apply(t);
    }
}

// MARK: select same

/// Every thread colour an object sews (`#rrggbb`).
pub fn colours(o: &DesignObject) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    v.extend(o.region.as_ref().map(|r| r.thread.hex()));
    v.extend(o.line.as_ref().map(|l| l.thread.hex()));
    if let Some(b) = &o.stitch_block {
        v.extend(b.threads.iter().map(|t| t.hex()));
    }
    v.dedup();
    v
}

/// The colour a stitch pattern sews nearest to `p` (model units), for Alt-click.
pub fn colour_at(o: &DesignObject, p: Vec2) -> Option<String> {
    let b = o.stitch_block.as_ref()?;
    let mut block = 0usize;
    let mut best: Option<(f64, usize)> = None;
    for s in &b.stitches {
        if s.command == Command::ColorChange {
            block += 1;
            continue;
        }
        let d = v2(s.x, s.y).distance(p);
        if best.is_none_or(|(bd, _)| d < bd) {
            best = Some((d, block));
        }
    }
    let i = best?.1.min(b.threads.len().checked_sub(1)?);
    Some(b.threads[i].hex())
}

/// What makes two objects "the same" for Select Same.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Same {
    Colour,
    /// The region type for objects with a region (fills, satins…), else the line type.
    Type,
    Both,
}

impl Same {
    pub fn from_key(k: &str) -> Option<Same> {
        Some(match k {
            "color" | "colour" => Same::Colour,
            "type" => Same::Type,
            "both" => Same::Both,
            _ => return None,
        })
    }
}

/// The sew type that identifies an object for Select Same ▸ Stitch Type: its region type, or
/// its line type when it has no region; stitch patterns have none.
pub fn sew_type(o: &DesignObject) -> Option<String> {
    o.region
        .as_ref()
        .map(|r| format!("region:{}", r.kind.id()))
        .or_else(|| o.line.as_ref().map(|l| format!("line:{}", l.kind.id())))
}

/// Visible objects that share a colour (any of the reference objects' threads) and/or a
/// sew type with `refs`, in sewing order. The reference objects themselves are included.
pub fn select_same(d: &Design, refs: &[u64], how: Same) -> Vec<u64> {
    let refs: Vec<&DesignObject> = refs
        .iter()
        .filter_map(|&id| d.index_of(id).map(|i| &d.objects[i]))
        .collect();
    let cols: Vec<String> = refs.iter().flat_map(|o| colours(o)).collect();
    let types: Vec<String> = refs.iter().filter_map(|o| sew_type(o)).collect();
    d.objects
        .iter()
        .filter(|o| o.visible)
        .filter(|o| {
            let c = colours(o).iter().any(|c| cols.contains(c));
            let t = sew_type(o).is_some_and(|t| types.contains(&t));
            match how {
                Same::Colour => c,
                Same::Type => t,
                Same::Both => c && t,
            }
        })
        .map(|o| o.id)
        .collect()
}

/// `object.list`'s filters: true when `o` passes every one given.
pub(crate) fn list_filter(
    d: &Design,
    p: &Params,
) -> Result<Option<impl Fn(&DesignObject) -> bool>, CommandError> {
    let keys = [
        "color",
        "regionType",
        "lineType",
        "visible",
        "sameAs",
        "match",
    ];
    if !keys.iter().any(|k| p.has(k)) {
        return Ok(None);
    }
    let colour = thread(p, "color")?.map(|t| t.hex());
    let region = p
        .has("regionType")
        .then(|| p.string("regionType", None))
        .transpose()?;
    let line = p
        .has("lineType")
        .then(|| p.string("lineType", None))
        .transpose()?;
    let visible = p
        .has("visible")
        .then(|| p.bool("visible", None))
        .transpose()?;
    let same = if p.has("sameAs") {
        let refs = match p.get("sameAs") {
            Some(v) if v.is_array() => p
                .array("sameAs")?
                .iter()
                .map(|v| {
                    v.as_u64()
                        .ok_or_else(|| p.invalid("sameAs", "must be object ids"))
                })
                .collect::<Result<Vec<u64>, _>>()?,
            _ => vec![p.int("sameAs", None, 0, i64::MAX)? as u64],
        };
        if let Some(bad) = refs.iter().find(|&&id| d.index_of(id).is_none()) {
            return Err(p.not_found(format!("object {bad}")));
        }
        let how = Same::from_key(&p.string("match", Some("color"))?)
            .ok_or_else(|| p.invalid("match", "must be color, type or both"))?;
        Some(select_same(d, &refs, how))
    } else {
        None
    };
    Ok(Some(move |o: &DesignObject| {
        colour.as_ref().is_none_or(|c| colours(o).contains(c))
            && region
                .as_ref()
                .is_none_or(|k| o.region.as_ref().is_some_and(|r| r.kind.id() == k))
            && line
                .as_ref()
                .is_none_or(|k| o.line.as_ref().is_some_and(|l| l.kind.id() == k))
            && visible.is_none_or(|v| o.visible == v)
            && same.as_ref().is_none_or(|ids| ids.contains(&o.id))
    }))
}
