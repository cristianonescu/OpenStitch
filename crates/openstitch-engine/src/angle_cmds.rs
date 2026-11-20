//! Stitch angle lines (#534, beyond PE-DESIGN): `object.setAngleLines` sets the lines across a
//! fill that its rows turn between (see `openstitch_design::angle_lines`).

use crate::commands::{CommandSpec, Effect, MM, req, spec};
use crate::params::{CommandError, Params};
use openstitch_design::{RegionType, Vec2, v2};
use serde_json::{Value, json};

/// Most angle lines on one fill.
pub const MAX_LINES: usize = 64;

pub fn commands() -> Vec<CommandSpec> {
    vec![
        spec(
            "object.setAngleLines",
            "Stitch angle lines (beyond PE-DESIGN): lines across a fill (2 or more, in order along the shape) whose directions its rows follow, turning smoothly from one line to the next, so a U, Z or ring sews in one object with no seam. An empty list removes them. Lines that cross inside the shape, or whose middle is outside it, are refused",
            Effect::Edit,
            vec![
                req("id", "int", "A shape whose region is a fill"),
                req(
                    "lines",
                    "[[[x, y], [x, y]], …] mm",
                    "The lines, each from one side of the shape to the other (they are extended to the outline), in order along it; [] removes them",
                ),
                crate::commands::opt(
                    "rule",
                    "sides | fan",
                    "How the rows turn between two lines, stored with the lines. sides (OpenStitch's, the default on a shape without lines): a row joins the points at the same fraction along the two outline sides. fan (PE-DESIGN's direction lines): the rows are a fan through the point where the two lines meet when extended, a row spacing apart at their far ends; parallel lines give parallel rows. Not given: the shape's rule is kept. A rule other than the one the shape's lines have is refused (lines are not converted from one rule to the other: remove them first with lines []). With fan the lines are PE-DESIGN's direction lines and what object.addDirectionLine refuses is refused here too: a shape with hole sewing, a curved direction or Manual Punch direction lines, two neighbouring lines on one straight line, lines that do not follow one another across the shape",
                ),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let lines = lines(p)?;
                let o = &s.design.objects[i];
                let Some(r) = o.region.as_ref().filter(|r| r.kind == RegionType::Fill) else {
                    return Err(p.failed(format!("object {id} is not a fill")));
                };
                if r.gradient.is_some() && !lines.is_empty() {
                    return Err(p.failed(format!(
                        "object {id} is part of a gradient; angle lines are for plain fills"
                    )));
                }
                // One shape has one rule: lines of the other rule are not converted.
                if let Some(asked) = p.get("rule").and_then(Value::as_str)
                    && r.angle_lines.len() >= 2
                    && !lines.is_empty()
                    && asked != r.angle_rule.key()
                    && matches!(asked, "sides" | "fan")
                {
                    return Err(p.invalid(
                    "rule",
                    format!(
                        "object {id} has lines with the rule {}; they are not converted: remove them first (lines [])",
                        r.angle_rule.key()
                    ),
                ));
                }
                // The rule the lines are sewn by: kept when not given, and OpenStitch's on a
                // shape without lines.
                let rule = match p.get("rule").and_then(Value::as_str) {
                    None if r.angle_lines.is_empty() => openstitch_design::AngleRule::Sides,
                    None => r.angle_rule,
                    Some("sides") => openstitch_design::AngleRule::Sides,
                    Some("fan") => openstitch_design::AngleRule::Fan,
                    Some(_) => return Err(p.invalid("rule", "must be sides or fan")),
                };
                if !lines.is_empty() {
                    let closed: Vec<_> = o.contours.iter().filter(|c| c.closed).cloned().collect();
                    if rule == openstitch_design::AngleRule::Fan {
                        // PE-DESIGN's direction lines: what Add Direction Line refuses.
                        fan_allowed(p, &s.design, i)?;
                        openstitch_design::angle_lines::check_fan(&lines, &closed, &[]).map_err(
                            |why| p.invalid("lines", why.replace("angle line", "direction line")),
                        )?;
                    } else {
                        openstitch_design::angle_lines::check(&lines, &closed, &[])
                            .map_err(|why| p.invalid("lines", why))?;
                    }
                }
                let n = lines.len();
                let mut auto_off = false;
                if let Some(r) = s.design.objects[i].region.as_mut() {
                    r.angle_rule = if lines.is_empty() {
                        openstitch_design::AngleRule::Sides
                    } else {
                        rule
                    };
                    // A shape with PE-DESIGN's direction lines has no Auto direction (as
                    // Add Direction Line leaves it).
                    if !lines.is_empty() && rule == openstitch_design::AngleRule::Fan {
                        auto_off = r.auto_direction;
                        r.auto_direction = false;
                    }
                    r.angle_lines = lines;
                }
                Ok(with_auto_note(
                    json!({ "id": id, "lines": n, "rule": rule.key() }),
                    auto_off,
                ))
            },
        ),
        spec(
            "object.addDirectionLine",
            "Add Direction Line, as PE-DESIGN's Select Point tool adds one with two clicks: a straight line across a filled shape, from a point outside it to a point outside it on another side. A shape's fill angle is its first direction line; with two or more lines the fill's rows follow them as PE-DESIGN sews them: the zone between two lines is cut at every point of the outline between the lines' crossings (a corner, a point on a side), by the straight line to the point at the same fraction along the outline on the other side of the zone; in each piece the rows are a fan through the point where the piece's two bounding lines meet when extended, a row spacing apart at their far ends (parallel lines give parallel rows; with no outline point between the crossings: one fan through the two lines' meeting point); before the first line and beyond the last the shape is cut the same way as far as its point farthest from the line, and a corner cut off by a line has rows parallel to it. So the outline's points matter: a rectangle imported from PE-DESIGN has a fifth point in the middle of its bottom side and can sew differently from OpenStitch's own with the same lines. The fill angle (object.setRegion angle) has no effect while the shape has two or more lines. The lines are stored as the shape's angle lines with the rule fan (object.setAngleLines sets them; object.get returns them as region.angleLines, each line as its two ends in 0.1 mm, with region.angleRule fan), in order across the shape: the new line is put where it lies among the others. Refused, changing nothing: when an end lies inside the shape; when the line does not cross the shape; when it would cross another direction line inside the shape; on a shape with hole sewing; on a shape whose angle lines have the rule sides (OpenStitch's own, from the Angle Lines tool: remove them first with object.setAngleLines lines [], they are not converted); on a shape with a curved direction or Manual Punch direction lines; on a region that is not a fill; when it lies on one straight line with the line next to it. A new line goes where it lies among the others going round the outline (a second line after the first). On a shape with Auto direction the first line is the angle the shape is sewn at now, and Auto direction is switched off (a shape with direction lines has no Auto direction; the answer's autoDirectionOff says when that happened). Where the rows of a fan would come closer than a quarter of their spacing (the lines meet on or just outside the outline), every other pair of rows ends early, and again at each halving; PE-DESIGN's reference design keeps every row down to 0.30 of the spacing, closer is not captured. PES 0110 files keep the lines. One undo step",
            Effect::Edit,
            vec![
                req("id", "int", "A shape whose region is a fill"),
                req(
                    "line",
                    "[[x, y], [x, y]] mm",
                    "The line's two ends, both outside the shape. The line is stored running the way the shape's lines already run (start, drawn with a square, to arrow head), whichever end is given first, as PE-DESIGN turns a new line; along the same line from 5 mm before its first crossing of the outline to 5 mm after its last one, wherever the two ends were put",
                ),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let o = &s.design.objects[i];
                let fan = fan_lines_of(p, &s.design, i)?;
                let closed: Vec<_> = o.contours.iter().filter(|c| c.closed).cloned().collect();
                let new = same_way(stored_line(p, &closed)?, fan.first());
                let mut lines = fan;
                if lines.len() >= MAX_LINES {
                    return Err(p.failed(format!("at most {MAX_LINES} direction lines")));
                }
                let at = openstitch_design::angle_lines::place_for(&lines, new, &closed);
                lines.insert(at, new);
                openstitch_design::angle_lines::check_fan(&lines, &closed, &[]).map_err(|why| {
                    p.invalid("line", why.replace("angle line", "direction line"))
                })?;
                let n = lines.len();
                let mut auto_off = false;
                if let Some(r) = s.design.objects[i].region.as_mut() {
                    r.angle_lines = lines;
                    r.angle_rule = openstitch_design::AngleRule::Fan;
                    auto_off = r.auto_direction;
                    r.auto_direction = false;
                }
                Ok(with_auto_note(
                    json!({ "id": id, "lines": n, "index": at }),
                    auto_off,
                ))
            },
        ),
        spec(
            "object.moveDirectionLine",
            "Move one direction line of a filled shape, as dragging one of its ends with the Select Point tool does: line 'index' (from 0: its place in region.angleLines of object.get) is replaced by 'line', under the rules of object.addDirectionLine: both ends outside the shape, stored from 5 mm before its first crossing of the outline to 5 mm after its last, and refused with the reason, changing nothing, when an end lies inside the shape, when it does not cross the shape, when it would cross another direction line inside the shape or lie on one straight line with a neighbour. The lines stay in order across the shape: the answer's index is the moved line's place afterwards (of two lines it keeps its place). For a shape with one direction, its fill angle, use object.setRegion {id, angle}. One undo step",
            Effect::Edit,
            vec![
                req("id", "int", "A shape with two or more direction lines"),
                req("index", "int", "The line to move (from 0)"),
                req(
                    "line",
                    "[[x, y], [x, y]] mm",
                    "The line's two new ends, both outside the shape. Stored running the way the shape's other lines run, whichever end is given first; along the same line from 5 mm before its first crossing of the outline to 5 mm after its last one, wherever the two ends were put",
                ),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let o = &s.design.objects[i];
                let Some(r) = o.region.as_ref().filter(|r| {
                    r.angle_rule == openstitch_design::AngleRule::Fan && r.angle_lines.len() >= 2
                }) else {
                    return Err(p.failed(format!(
                        "object {id} has one direction line, its fill angle: turn it with object.setRegion {{id: {id}, angle}}"
                    )));
                };
                fan_allowed(p, &s.design, i)?;
                let mut lines = r.angle_lines.clone();
                let k = p.int("index", None, 0, lines.len() as i64 - 1)? as usize;
                let closed: Vec<_> = o.contours.iter().filter(|c| c.closed).cloned().collect();
                let new = stored_line(p, &closed)?;
                lines.remove(k);
                let new = same_way(new, lines.first());
                // Of two lines the moved one keeps its place: there is no order to find.
                let at = if lines.len() == 1 {
                    k
                } else {
                    openstitch_design::angle_lines::place_for(&lines, new, &closed)
                };
                lines.insert(at, new);
                openstitch_design::angle_lines::check_fan(&lines, &closed, &[]).map_err(|why| {
                    p.invalid("line", why.replace("angle line", "direction line"))
                })?;
                let n = lines.len();
                if let Some(r) = s.design.objects[i].region.as_mut() {
                    r.angle_lines = lines;
                }
                Ok(json!({ "id": id, "lines": n, "index": at }))
            },
        ),
        spec(
            "object.deleteDirectionLine",
            "Delete one direction line of a filled shape (PE-DESIGN's Delete Selected Line, and the Delete key with a line selected), or with 'keep' every line but that one (Delete Lines Not Selected). The lines are the shape's angle lines with the rule fan, numbered from 0 in order across the shape (object.addDirectionLine): a line's index is its place in region.angleLines of object.get. When one line is left it becomes the shape's fill angle again (the direction it points in) and stays where it is, as PE-DESIGN keeps it: object.get then shows that one line in region.angleLines; a new angle (object.setRegion angle) or Auto direction puts the shape's line through its middle again. A shape's last line, its fill angle, cannot be deleted: refused. One undo step",
            Effect::Edit,
            vec![
                req("id", "int", "A shape with two or more direction lines"),
                crate::commands::opt("index", "int", "The line to delete (from 0)"),
                crate::commands::opt(
                    "keep",
                    "int",
                    "Instead of 'index': the one line to keep; all the others are deleted. With both given, 'keep' is used and 'index' is ignored",
                ),
            ],
            |s, p| {
                let id = p.int("id", None, 0, i64::MAX)? as u64;
                let i = s
                    .design
                    .index_of(id)
                    .ok_or_else(|| p.not_found(format!("object {id}")))?;
                let o = &s.design.objects[i];
                let fan = o.region.as_ref().is_some_and(|r| {
                    r.angle_rule == openstitch_design::AngleRule::Fan && r.angle_lines.len() >= 2
                });
                if !fan {
                    return Err(p.failed(format!(
                    "object {id} has one direction line, its fill angle: the last line cannot be deleted"
                )));
                }
                let mut lines = o
                    .region
                    .as_ref()
                    .map(|r| r.angle_lines.clone())
                    .unwrap_or_default();
                let last = lines.len() as i64 - 1;
                if p.has("keep") {
                    let k = p.int("keep", None, 0, last)? as usize;
                    lines = vec![lines[k]];
                } else {
                    let k = p.int("index", None, 0, last)? as usize;
                    lines.remove(k);
                }
                let n = lines.len();
                if let Some(r) = s.design.objects[i].region.as_mut() {
                    if let [only] = lines[..] {
                        // One line left: the fill is sewn at its angle again, and the line
                        // stays where it is (PE-DESIGN capture g: the kept line does not
                        // move). No Auto direction, as with the lines before.
                        r.angle = openstitch_design::angle_lines::line_angle(only);
                        r.angle_lines = vec![only];
                        r.angle_rule = openstitch_design::AngleRule::Fan;
                        r.auto_direction = false;
                    } else {
                        r.angle_lines = lines;
                    }
                }
                Ok(json!({ "id": id, "lines": n }))
            },
        ),
    ]
}

/// One line, `[[x, y], [x, y]]` mm, in 0.1 mm.
fn one_line(p: &Params) -> Result<[Vec2; 2], CommandError> {
    let bad = || p.invalid("line", "must be [[x, y], [x, y]] in mm");
    let ends = p
        .get("line")
        .and_then(Value::as_array)
        .filter(|e| e.len() == 2)
        .ok_or_else(bad)?;
    let point = |v: &Value| -> Option<Vec2> {
        let a = v.as_array().filter(|a| a.len() == 2)?;
        let (x, y) = (a[0].as_f64()?, a[1].as_f64()?);
        (x.is_finite() && y.is_finite() && x.abs() <= 5000.0 && y.abs() <= 5000.0)
            .then(|| v2(x * MM, y * MM))
    };
    let line = [
        point(&ends[0]).ok_or_else(bad)?,
        point(&ends[1]).ok_or_else(bad)?,
    ];
    if line[0].distance(line[1]) < 1.0 {
        return Err(p.invalid("line", "its two ends are the same point"));
    }
    Ok(line)
}

/// The `line` parameter as a direction line is stored: both ends outside the shape, then
/// along the same line from 5 mm before its first crossing of the outline to 5 mm after its
/// last one, wherever the two ends were put. An error says what is wrong with it.
fn stored_line(
    p: &Params,
    closed: &[openstitch_design::Contour],
) -> Result<[Vec2; 2], CommandError> {
    let new = one_line(p)?;
    for (end, what) in [(new[0], "first"), (new[1], "second")] {
        if openstitch_design::geometry::contains(end, closed) {
            return Err(p.invalid(
                "line",
                format!("its {what} end lies inside the shape; both ends of a direction line are outside it"),
            ));
        }
    }
    openstitch_design::angle_lines::across(new, closed)
        .ok_or_else(|| p.invalid("line", "the line does not cross the shape"))
}

/// The answer of a command that gives a shape direction lines, saying whether it switched
/// the shape's Auto direction off (`autoDirectionOff`); when it did, `summary` says so in
/// words, which the editor shows in the status bar after the command's name.
fn with_auto_note(mut answer: Value, auto_off: bool) -> Value {
    answer["autoDirectionOff"] = json!(auto_off);
    if auto_off {
        answer["summary"] =
            json!("Auto direction is off now: a shape with direction lines has none");
    }
    answer
}

/// `line` turned, if need be, to run the way `other` does (start to arrow within a quarter
/// turn of it): PE-DESIGN draws a new line the way the shape's lines already run, whichever
/// of its two ends was clicked first (its capture k, `g5`).
fn same_way(line: [Vec2; 2], other: Option<&[Vec2; 2]>) -> [Vec2; 2] {
    let Some(other) = other else {
        return line;
    };
    let (d, e) = (line[1] - line[0], other[1] - other[0]);
    if d.x * e.x + d.y * e.y < 0.0 {
        [line[1], line[0]]
    } else {
        line
    }
}

/// Refuses `what` on an object that has PE-DESIGN's direction lines (two or more, the fan
/// rule): they do not go with hole sewing, a curved direction or Manual Punch lines.
pub(crate) fn refuse_fan(
    p: &Params,
    o: &openstitch_design::DesignObject,
    what: &str,
) -> Result<(), CommandError> {
    let fan = o.region.as_ref().is_some_and(|r| {
        r.angle_rule == openstitch_design::AngleRule::Fan && r.angle_lines.len() >= 2
    });
    if fan {
        let id = o.id;
        return Err(p.failed(format!(
            "object {id} has direction lines: {what}; go back to one direction first (object.deleteDirectionLine {{id: {id}, keep: 0}})"
        )));
    }
    Ok(())
}

/// The direction lines object `i` has for Add Direction Line to add to: its stored fan lines,
/// or the one line its fill angle is (with Auto direction: the angle it is sewn at now). An
/// error says why the shape cannot take a line.
fn fan_lines_of(
    p: &Params,
    d: &openstitch_design::Design,
    i: usize,
) -> Result<Vec<[Vec2; 2]>, CommandError> {
    fan_allowed(p, d, i)?;
    let o = &d.objects[i];
    let id = o.id;
    let Some(r) = o.region.as_ref() else {
        return Err(p.failed(format!("object {id} is not a fill")));
    };
    // The shape's one direction kept at a place: the first line is that line.
    if r.angle_lines.len() == 1 && r.angle_rule == openstitch_design::AngleRule::Fan {
        return Ok(r.angle_lines.clone());
    }
    if r.angle_lines.len() >= 2 {
        if r.angle_rule != openstitch_design::AngleRule::Fan {
            return Err(p.failed(format!(
                "object {id} has OpenStitch angle lines (rule sides); they are not converted: remove them first (object.setAngleLines with lines [])"
            )));
        }
        return Ok(r.angle_lines.clone());
    }
    let closed: Vec<_> = o.contours.iter().filter(|c| c.closed).cloned().collect();
    let angle = if r.auto_direction {
        openstitch_design::region::stored_angle(openstitch_design::region::auto_angle(&closed))
    } else {
        r.angle
    };
    openstitch_design::angle_lines::angle_line(&closed, angle)
        .map(|l| vec![l])
        .ok_or_else(|| p.failed(format!("object {id} has no closed outline")))
}

/// Whether object `i` can have PE-DESIGN's direction lines at all: a plain fill without hole
/// sewing, a curved direction or Manual Punch direction lines. An error says why not.
fn fan_allowed(p: &Params, d: &openstitch_design::Design, i: usize) -> Result<(), CommandError> {
    let o = &d.objects[i];
    let id = o.id;
    let Some(r) = o.region.as_ref().filter(|r| r.kind == RegionType::Fill) else {
        return Err(p.failed(format!(
            "object {id} is not a fill: direction lines are added to a fill"
        )));
    };
    if o.hole_sewing || openstitch_design::holes::partner(d, id).is_some() {
        return Err(p.failed(format!(
            "object {id} has hole sewing: a direction line cannot be added; cancel hole sewing first (object.setHoleSewing {{ids: [{id}], enabled: false}})"
        )));
    }
    if r.gradient.is_some() {
        return Err(p.failed(format!(
            "object {id} is part of a gradient; direction lines are for plain fills"
        )));
    }
    if r.direction_curve.is_some() || r.direction_lines.len() >= 2 {
        return Err(p.failed(format!(
            "object {id} has a curved direction or Manual Punch direction lines; remove them first (object.setDirectionCurve points null, object.setDirectionLines lines null)"
        )));
    }
    Ok(())
}

/// The `lines` list, in 0.1 mm.
fn lines(p: &Params) -> Result<Vec<[Vec2; 2]>, CommandError> {
    let bad = || p.invalid("lines", "must be a list of [[x, y], [x, y]] in mm");
    let list = p.get("lines").and_then(Value::as_array).ok_or_else(bad)?;
    if list.len() > MAX_LINES {
        return Err(p.invalid("lines", format!("at most {MAX_LINES} lines")));
    }
    let point = |v: &Value| -> Option<Vec2> {
        let a = v.as_array().filter(|a| a.len() == 2)?;
        let (x, y) = (a[0].as_f64()?, a[1].as_f64()?);
        (x.is_finite() && y.is_finite() && x.abs() <= 5000.0 && y.abs() <= 5000.0)
            .then(|| v2(x * MM, y * MM))
    };
    list.iter()
        .map(|l| {
            let ends = l.as_array().filter(|e| e.len() == 2).ok_or_else(bad)?;
            Ok([
                point(&ends[0]).ok_or_else(bad)?,
                point(&ends[1]).ok_or_else(bad)?,
            ])
        })
        .collect()
}
