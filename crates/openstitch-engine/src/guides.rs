//! Guidelines (View ▸ Guideline): lines dragged out of the rulers to line objects up. They are a
//! view aid saved with the design, never sewn; changing them is one undo step like any edit.

use crate::commands::{CommandSpec, Effect, IDS, MM, indexes, opt, req, spec};
use crate::params::{CommandError, Params};
use openstitch_design::{GuideLayout, Guides, Rect};
use serde_json::{Value, json};

fn positions(p: &Params, key: &str, now: &[f64]) -> Result<Vec<f64>, CommandError> {
    if !p.has(key) {
        return Ok(now.to_vec());
    }
    let list = p.array(key)?;
    if list.len() > Guides::MAX {
        return Err(p.invalid(key, format!("at most {} guidelines", Guides::MAX)));
    }
    list.iter()
        .map(|v| {
            v.as_f64()
                .filter(|n| (-5000.0..=5000.0).contains(n))
                .map(|n| n * MM)
                .ok_or_else(|| p.invalid(key, "positions must be numbers in -5000...5000 mm"))
        })
        .collect()
}

/// The guidelines in mm, and whether they are locked: what every guide command answers.
fn listing(g: &Guides) -> Value {
    let mm = |v: &[f64]| v.iter().map(|x| x / MM).collect::<Vec<_>>();
    json!({ "vertical": mm(&g.vertical), "horizontal": mm(&g.horizontal), "locked": g.locked })
}

/// `orientation`: true for a vertical guideline.
fn vertical(p: &Params, key: &str) -> Result<bool, CommandError> {
    match p.string(key, None)?.as_str() {
        "vertical" => Ok(true),
        "horizontal" => Ok(false),
        _ => Err(p.invalid(key, "must be vertical or horizontal")),
    }
}

/// A position in mm from the Design Page's left (vertical) or top (horizontal) edge.
fn position(p: &Params, key: &str) -> Result<f64, CommandError> {
    Ok(p.req(key, -5000.0, 5000.0)? * MM)
}

/// The list of `orientation` and the index of its guideline `index`, refused while locked.
fn existing<'a>(p: &Params, g: &'a mut Guides) -> Result<(&'a mut Vec<f64>, usize), CommandError> {
    if g.locked {
        return Err(p.failed("the guidelines are locked (view.lockGuides false to unlock)"));
    }
    let v = vertical(p, "orientation")?;
    let list = if v {
        &mut g.vertical
    } else {
        &mut g.horizontal
    };
    let n = list.len();
    if n == 0 {
        return Err(p.failed("no guideline of that orientation"));
    }
    let i = p.int("index", None, 0, n as i64 - 1)? as usize;
    Ok((list, i))
}

/// Adds `at` to `list` unless one is already there (within 0.05 mm); false when full.
fn add(list: &mut Vec<f64>, at: f64) -> bool {
    if list.iter().any(|v| (v - at).abs() < 0.5) {
        return true;
    }
    if list.len() >= Guides::MAX {
        return false;
    }
    list.push(at);
    true
}

const ORIENTATION: &str =
    "vertical (an x, from the top ruler) | horizontal (a y, from the left ruler)";

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "view.newGuide",
            "Add a guideline (#799): orientation and position in mm from the Design Page's left or top edge (page coordinates, whatever the ruler origin shows); one undo step",
            Effect::Edit,
            vec![
                req("orientation", "vertical|horizontal", ORIENTATION),
                req(
                    "position",
                    "mm",
                    "x of a vertical or y of a horizontal guideline, -5000...5000",
                ),
            ],
            |s, p| {
                let v = vertical(p, "orientation")?;
                let at = position(p, "position")?;
                let g = &mut s.design.guides;
                let list = if v {
                    &mut g.vertical
                } else {
                    &mut g.horizontal
                };
                if !add(list, at) {
                    return Err(p.failed(format!("at most {} guidelines a ruler", Guides::MAX)));
                }
                Ok(listing(g))
            },
        ),
        spec(
            "view.moveGuide",
            "Move guideline `index` of `orientation` to `position` mm (page coordinates); refused while the guidelines are locked",
            Effect::Edit,
            vec![
                req("orientation", "vertical|horizontal", ORIENTATION),
                req(
                    "index",
                    "int",
                    "Which guideline, from 0, in the order guides.list gives",
                ),
                req(
                    "position",
                    "mm",
                    "New x (vertical) or y (horizontal), -5000...5000",
                ),
            ],
            |s, p| {
                let at = position(p, "position")?;
                let (list, i) = existing(p, &mut s.design.guides)?;
                list[i] = at;
                Ok(listing(&s.design.guides))
            },
        ),
        spec(
            "view.deleteGuide",
            "Delete guideline `index` of `orientation`; refused while the guidelines are locked",
            Effect::Edit,
            vec![
                req("orientation", "vertical|horizontal", ORIENTATION),
                req(
                    "index",
                    "int",
                    "Which guideline, from 0, in the order guides.list gives",
                ),
            ],
            |s, p| {
                let (list, i) = existing(p, &mut s.design.guides)?;
                list.remove(i);
                Ok(listing(&s.design.guides))
            },
        ),
        spec(
            "view.editGuide",
            "Edit guideline `index` of `orientation` (Set Position…, Alt-click to turn it): a new position in mm (page coordinates) and/or `turn` to the other orientation (then `position` is its new x or y; default: where it crosses the page centre); one undo step, refused while locked",
            Effect::Edit,
            vec![
                req("orientation", "vertical|horizontal", ORIENTATION),
                req(
                    "index",
                    "int",
                    "Which guideline, from 0, in the order guides.list gives",
                ),
                opt(
                    "position",
                    "mm",
                    "New x (vertical) or y (horizontal), -5000...5000",
                ),
                opt("turn", "bool", "Turn it vertical <-> horizontal"),
            ],
            |s, p| {
                let turn = p.bool("turn", Some(false))?;
                let at = if p.has("position") {
                    Some(position(p, "position")?)
                } else {
                    None
                };
                if at.is_none() && !turn {
                    return Err(p.invalid("position", "give position, turn or both"));
                }
                let page = (s.design.page.width, s.design.page.height);
                let was_vertical = vertical(p, "orientation")?;
                let (list, i) = existing(p, &mut s.design.guides)?;
                if !turn {
                    list[i] = at.expect("position");
                    return Ok(listing(&s.design.guides));
                }
                list.remove(i);
                let centre = if was_vertical {
                    page.1 / 2.0
                } else {
                    page.0 / 2.0
                };
                let g = &mut s.design.guides;
                let other = if was_vertical {
                    &mut g.horizontal
                } else {
                    &mut g.vertical
                };
                if !add(other, at.unwrap_or(centre)) {
                    return Err(p.failed(format!("at most {} guidelines a ruler", Guides::MAX)));
                }
                Ok(listing(g))
            },
        ),
        spec(
            "view.clearGuides",
            "Clear Guides: delete every guideline in one undo step (also while locked)",
            Effect::Edit,
            vec![],
            |s, _| {
                let g = &mut s.design.guides;
                g.vertical.clear();
                g.horizontal.clear();
                Ok(listing(g))
            },
        ),
        spec(
            "view.lockGuides",
            "Lock Guides: locked guidelines stay shown and snapped to but are not moved, edited or deleted one by one; saved with the design; one undo step",
            Effect::Edit,
            vec![req("locked", "bool", "Lock (true) or unlock (false)")],
            |s, p| {
                s.design.guides.locked = p.bool("locked", None)?;
                Ok(listing(&s.design.guides))
            },
        ),
        spec(
            "view.guideLayout",
            "New Guide Layout…: guidelines for columns and rows with gutters, inside margins, on the Design Page (each margin a line, each column or row its two edges); clear first drops the existing ones; one undo step. Lengths in mm",
            Effect::Edit,
            vec![
                opt(
                    "columns",
                    "int",
                    "0...50 columns (default 0: margin lines only)",
                ),
                opt("rows", "int", "0...50 rows (default 0)"),
                opt(
                    "gutter",
                    "mm",
                    "Space between columns and between rows, 0...1000",
                ),
                opt(
                    "columnGutter",
                    "mm",
                    "Space between columns (default gutter)",
                ),
                opt("rowGutter", "mm", "Space between rows (default gutter)"),
                opt("margin", "mm", "Every margin, 0...1000"),
                opt("marginTop", "mm", "Top margin (default margin)"),
                opt("marginLeft", "mm", "Left margin (default margin)"),
                opt("marginBottom", "mm", "Bottom margin (default margin)"),
                opt("marginRight", "mm", "Right margin (default margin)"),
                opt(
                    "clear",
                    "bool",
                    "Clear the existing guidelines first (default false)",
                ),
            ],
            |s, p| {
                let mm = |key: &str, default: f64| -> Result<f64, CommandError> {
                    Ok(p.opt(key, default, 0.0, 1000.0)? * MM)
                };
                let gutter = p.opt("gutter", 0.0, 0.0, 1000.0)?;
                let margin = p.opt("margin", 0.0, 0.0, 1000.0)?;
                let layout = GuideLayout {
                    columns: p.int("columns", Some(0), 0, 50)? as u32,
                    rows: p.int("rows", Some(0), 0, 50)? as u32,
                    column_gutter: mm("columnGutter", gutter)?,
                    row_gutter: mm("rowGutter", gutter)?,
                    margin_top: mm("marginTop", margin)?,
                    margin_left: mm("marginLeft", margin)?,
                    margin_bottom: mm("marginBottom", margin)?,
                    margin_right: mm("marginRight", margin)?,
                };
                let page = s.design.page;
                let (xs, ys) = layout
                    .lines(page.width, page.height)
                    .map_err(|e| p.failed(e))?;
                let g = &mut s.design.guides;
                if p.bool("clear", Some(false))? {
                    g.vertical.clear();
                    g.horizontal.clear();
                }
                let fits = xs.into_iter().all(|x| add(&mut g.vertical, x))
                    & ys.into_iter().all(|y| add(&mut g.horizontal, y));
                if !fits {
                    return Err(p.failed(format!("at most {} guidelines a ruler", Guides::MAX)));
                }
                Ok(listing(g))
            },
        ),
        spec(
            "view.newGuidesFromSelection",
            "New Guides From Selection: guidelines along the objects' bounding box edges and centre lines (three vertical, three horizontal; none doubled); one undo step",
            Effect::Edit,
            vec![IDS],
            |s, p| {
                let idx = indexes(p, &s.design)?;
                let b = Rect::enclosing(
                    idx.iter()
                        .filter_map(|&i| s.design.objects[i].bounds())
                        .flat_map(|r| {
                            [
                                openstitch_design::v2(r.min_x, r.min_y),
                                openstitch_design::v2(r.max_x, r.max_y),
                            ]
                        }),
                )
                .ok_or_else(|| p.failed("the objects have no extent"))?;
                let Rect {
                    min_x,
                    min_y,
                    max_x,
                    max_y,
                } = b;
                let g = &mut s.design.guides;
                let fits = [min_x, (min_x + max_x) / 2.0, max_x]
                    .into_iter()
                    .all(|x| add(&mut g.vertical, x))
                    & [min_y, (min_y + max_y) / 2.0, max_y]
                        .into_iter()
                        .all(|y| add(&mut g.horizontal, y));
                if !fits {
                    return Err(p.failed(format!("at most {} guidelines a ruler", Guides::MAX)));
                }
                Ok(listing(g))
            },
        ),
        spec(
            "guides.set",
            "Set the guidelines (lines dragged out of the rulers; saved with the design, never sewn)",
            Effect::Edit,
            vec![
                opt(
                    "vertical",
                    "[mm]",
                    "x of each vertical guideline, from the Design Page's left edge; at most 100 (default: unchanged)",
                ),
                opt(
                    "horizontal",
                    "[mm]",
                    "y of each horizontal guideline, from the Design Page's top edge; at most 100 (default: unchanged)",
                ),
            ],
            |s, p| {
                let g = &s.design.guides;
                let vertical = positions(p, "vertical", &g.vertical)?;
                let horizontal = positions(p, "horizontal", &g.horizontal)?;
                s.design.guides = Guides {
                    vertical,
                    horizontal,
                    locked: s.design.guides.locked,
                };
                Ok(listing(&s.design.guides))
            },
        ),
        spec(
            "guides.list",
            "The design's guidelines, mm from the Design Page's left and top edges, and whether they are locked",
            Effect::Read,
            vec![],
            |s, _| Ok(listing(&s.design.guides)),
        ),
    ]
}
