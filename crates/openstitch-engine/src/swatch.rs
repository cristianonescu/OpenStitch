//! Test swatches to sew before the real thing (beyond PE-DESIGN, `batch:mac-sewout`):
//!
//! - `design.testSwatch` (#452): the chosen objects alone, centred in the smallest hoop they fit,
//!   optionally with a running-stitch label of their main settings;
//! - `design.testSwatches` (#507): one object copied in a grid with one setting (or two) stepped,
//!   each copy named and labelled with its value;
//! - `fabric.calibrationSwatch` (#453): a card of fill and satin pieces at graded densities and
//!   pull compensations for one fabric, labelled F1–F5, S1–S5 and P1–P5, with a 10 mm scale
//!   square; `fabric.calibrate` turns the rows the user picks into a custom fabric.
//!
//! Every swatch is a design of its own, written to `path` (any `design.export` format, or `.osd`)
//! through the same commands the user has: the open design is never changed. The commands build
//! it on the File effect's scratch state, which the session throws away afterwards.

use crate::commands::{CommandSpec, Effect, IDS, MM, indexes, opt, req, spec};
use crate::params::{CommandError, Params};
use crate::session::EngineState;
use openstitch_core::{Command, Thread};
use openstitch_design::{Compiler, Design, DesignObject, Rect};
use serde_json::{Value, json};

/// Most copies in a parameter swatch grid.
pub const MAX_COPIES: usize = 25;
/// Label height, mm (the single-line font, legible at 3–4 mm).
pub const LABEL_MM: f64 = 3.5;
/// The bundled single-line font the labels are sewn in.
const LABEL_FONT: &str = "openstitch-line";

/// Runs another command's handler on `s` (the scratch state), as the user would run it.
fn call(s: &mut EngineState, id: &str, params: Value) -> crate::CommandResult {
    let spec = crate::commands::all()
        .into_iter()
        .find(|c| c.id == id)
        .unwrap_or_else(|| panic!("swatch: unknown command {id}"));
    let p = Params::new(id, Some(&params))?;
    (spec.run)(s, &p)
}

/// The open design with no objects: same page, hoop, machine and fonts, a new name.
fn empty_like(d: &Design, name: &str) -> Design {
    let mut out = d.clone();
    out.objects.clear();
    out.background = None;
    out.source = None;
    out.name = name.to_string();
    out
}

/// Writes `s.design` to the `path` in `p`: `.osd` as the editable design, any other extension as
/// `design.export` writes it (the format follows the extension; PES by default when none).
fn write(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let path = p.string("path", None)?;
    let ext = std::path::Path::new(&path)
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase);
    match ext.as_deref() {
        Some(e) if e == crate::project::EXTENSION => {
            call(s, "project.save", json!({ "path": path }))
        }
        Some(_) => call(s, "design.export", json!({ "path": path })),
        None => call(s, "design.export", json!({ "path": path, "format": "pes" })),
    }
}

/// Every object's bounds, from the stitches they sew (labels included).
fn sewn_bounds(d: &Design) -> Option<Rect> {
    let pattern = Compiler::for_design(d).compile(d);
    if pattern.count(Command::Stitch) == 0 {
        return None;
    }
    let b = pattern.bounds();
    Some(Rect {
        min_x: b.min_x,
        min_y: b.min_y,
        max_x: b.max_x,
        max_y: b.max_y,
    })
}

/// Puts the design in the smallest hoop of its brand it fits (Brother when none is chosen) and
/// centres it there. Fails when it is larger than every single-position hoop.
fn fit_smallest_hoop(s: &mut EngineState, p: &Params) -> Result<String, CommandError> {
    let pattern = Compiler::for_design(&s.design).compile(&s.design);
    let check = crate::machine::export_check(&s.design, &pattern);
    let Some(hoop) = check["smallestHoop"].as_str().map(str::to_string) else {
        return Err(p.failed(format!(
            "the swatch ({:.1} × {:.1} mm) is larger than every single-position hoop",
            check["designMM"][0].as_f64().unwrap_or(0.0),
            check["designMM"][1].as_f64().unwrap_or(0.0)
        )));
    };
    call(s, "machine.set", json!({ "hoop": hoop }))?;
    let all: Vec<u64> = s.design.objects.iter().map(|o| o.id).collect();
    call(s, "object.center", json!({ "ids": all }))?;
    Ok(hoop)
}

/// Adds a running-stitch label, centred on `x` with its top at `top` (mm), sewn in `thread`
/// (the piece's own thread, so a label adds no colour change of its own).
fn label(
    s: &mut EngineState,
    text: &str,
    x: f64,
    top: f64,
    thread: &Thread,
) -> crate::CommandResult {
    let r = call(
        s,
        "text.add",
        json!({
            "text": text, "font": LABEL_FONT, "size": LABEL_MM,
            "x": x, "y": top + LABEL_MM, "outline": "#000000",
        }),
    )?;
    // The text's baseline was placed at `top + size`; move it so its sewn top sits at `top`.
    if let Some(id) = r["id"].as_u64() {
        let i = s.design.index_of(id).expect("the label just added");
        if let Some(l) = &mut s.design.objects[i].line {
            l.thread = thread.clone();
        }
        let one = Design {
            objects: vec![s.design.objects[i].clone()],
            ..s.design.clone()
        };
        if let Some(b) = sewn_bounds(&one) {
            call(
                s,
                "object.move",
                json!({ "ids": [id], "dy": top - b.min_y / MM, "dx": x - b.center().x / MM }),
            )?;
        }
    }
    Ok(r)
}

/// The object's main thread (the region's, else the line's).
fn main_thread(o: &DesignObject) -> Thread {
    o.region
        .as_ref()
        .map(|r| r.thread.clone())
        .or_else(|| o.line.as_ref().map(|l| l.thread.clone()))
        .unwrap_or_else(|| Thread::rgb(0))
}

/// A short description of an object's main settings, for the label of `design.testSwatch`.
fn settings_text(o: &DesignObject) -> String {
    if let Some(r) = &o.region {
        format!(
            "{} {:.1}/mm {:.0}° pc {:.1}",
            r.kind.id(),
            r.density,
            r.angle,
            r.pull_compensation
        )
    } else if let Some(l) = &o.line {
        format!("{} {:.1} mm", l.kind.id(), l.pitch / MM)
    } else {
        String::new()
    }
}

// MARK: #452, the selection alone

fn test_swatch(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let idx = indexes(p, &s.design)?;
    if idx.is_empty() {
        return Err(p.invalid("ids", "needs at least one object"));
    }
    let objects: Vec<Value> = idx
        .iter()
        .map(|&i| serde_json::to_value(&s.design.objects[i]).expect("an object serialises"))
        .collect();
    let first = s.design.objects[idx[0]].clone();
    let name = format!("{} test swatch", s.design.name);
    s.design = empty_like(&s.design, &name);
    call(s, "object.paste", json!({ "objects": objects }))?;
    let with_label = p.bool("label", Some(false))?;
    if with_label {
        let text = settings_text(&first);
        if !text.is_empty() {
            let b = sewn_bounds(&s.design).ok_or_else(|| p.failed("the objects sew nothing"))?;
            label(
                s,
                &text,
                b.center().x / MM,
                b.max_y / MM + 3.0,
                &main_thread(&first),
            )?;
        }
    }
    let hoop = fit_smallest_hoop(s, p)?;
    let mut out = write(s, p)?;
    out["hoop"] = json!(hoop);
    out["objects"] = json!(s.design.objects.len());
    Ok(out)
}

// MARK: #507, one setting stepped

/// A setting `design.testSwatches` can step: its name, the command that sets it, that command's
/// parameter and its unit.
struct Attribute {
    name: &'static str,
    command: &'static str,
    param: &'static str,
    unit: &'static str,
}

/// The settings `design.testSwatches` steps: name, setting command, its parameter, whether it is
/// a region (else a line) setting, and the unit its labels show.
pub const ATTRIBUTES: &[(&str, &str, &str, bool, &str)] = &[
    ("density", "object.setRegion", "density", true, "/mm"),
    (
        "stitchLength",
        "object.setRegion",
        "stitchLength",
        true,
        " mm",
    ),
    (
        "pullCompensation",
        "object.setRegion",
        "pullCompensation",
        true,
        " mm",
    ),
    ("angle", "object.setRegion", "angle", true, "°"),
    (
        "underlaySpacing",
        "object.setRegion",
        "underlaySpacing",
        true,
        " mm",
    ),
    ("linePitch", "object.setLine", "pitch", false, " mm"),
    ("lineWidth", "object.setLine", "width", false, " mm"),
    ("lineDensity", "object.setLine", "density", false, "/mm"),
];

fn attribute(name: &str) -> Option<Attribute> {
    ATTRIBUTES
        .iter()
        .find(|a| a.0 == name)
        .map(|&(name, command, param, _, unit)| Attribute {
            name,
            command,
            param,
            unit,
        })
}

/// The settings an object's sewing uses, so `design.testSwatches` can name them when refusing.
pub fn attributes_for(o: &DesignObject) -> Vec<&'static str> {
    ATTRIBUTES
        .iter()
        .filter(|a| {
            if a.3 {
                o.region.is_some()
            } else {
                o.line.is_some()
            }
        })
        .map(|a| a.0)
        .collect()
}

/// One stepped setting: which, and its values.
struct Steps {
    attribute: Attribute,
    values: Vec<f64>,
}

fn steps(p: &Params, o: &DesignObject, suffix: &str) -> Result<Option<Steps>, CommandError> {
    let key = format!("attribute{suffix}");
    if !p.has(&key) {
        return Ok(None);
    }
    let name = p.string(&key, None)?;
    let valid = attributes_for(o);
    let attribute = attribute(&name)
        .filter(|a| valid.contains(&a.name))
        .ok_or_else(|| {
            p.invalid(
                &key,
                format!(
                    "this object's sewing uses {}",
                    if valid.is_empty() {
                        "none of them".to_string()
                    } else {
                        valid.join(", ")
                    }
                ),
            )
        })?;
    let start = p.opt(&format!("start{suffix}"), 0.0, -360.0, 360.0)?;
    let step = p.opt(&format!("step{suffix}"), 0.0, -360.0, 360.0)?;
    if !p.has(&format!("start{suffix}")) || !p.has(&format!("step{suffix}")) {
        return Err(p.invalid(&key, format!("needs start{suffix} and step{suffix}")));
    }
    let count = p.int(&format!("count{suffix}"), None, 2, MAX_COPIES as i64)? as usize;
    // Rounded to 0.001 so the labels read as typed (0.30, 0.35, …).
    let values = (0..count)
        .map(|k| ((start + step * k as f64) * 1000.0).round() / 1000.0)
        .collect();
    Ok(Some(Steps { attribute, values }))
}

/// A copy's name: the setting and its value ("density 4.5/mm").
fn value_text(a: &Attribute, v: f64) -> String {
    format!("{} {v}{}", a.name, a.unit)
}

/// The value as a label sews it ("4.5/mm"): short, so it stays within its copy's column.
fn value_only(a: &Attribute, v: f64) -> String {
    format!("{v}{}", a.unit.trim_start())
}

/// The sewn width of a label, mm, measured on a copy of the state.
fn label_width(s: &EngineState, text: &str) -> Result<f64, CommandError> {
    let mut t = s.clone();
    t.design.objects.clear();
    label(&mut t, text, 0.0, 0.0, &Thread::rgb(0))?;
    Ok(sewn_bounds(&t.design).map_or(0.0, |b| b.width() / MM))
}

fn test_swatches(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let id = p.int("id", None, 0, i64::MAX)? as u64;
    let i = s
        .design
        .index_of(id)
        .ok_or_else(|| p.not_found(format!("object {id}")))?;
    let original = s.design.objects[i].clone();
    let rows = steps(p, &original, "")?.ok_or_else(|| p.invalid("attribute", "is required"))?;
    let cols = steps(p, &original, "2")?;
    let n_rows = rows.values.len();
    let n_cols = cols.as_ref().map_or(1, |c| c.values.len());
    // One setting: the copies fill rows left to right; two: rows × columns.
    let (grid_rows, grid_cols) = if cols.is_some() {
        (n_rows, n_cols)
    } else {
        let c = (n_rows as f64).sqrt().ceil() as usize;
        (n_rows.div_ceil(c), c)
    };
    if n_rows * n_cols > MAX_COPIES {
        return Err(p.invalid("count2", format!("at most {MAX_COPIES} copies in all")));
    }
    let gap = p.opt("gap", 4.0, 1.0, 50.0)?;
    let with_label = p.bool("label", Some(true))?;
    let object = serde_json::to_value(&original).expect("an object serialises");
    let thread = main_thread(&original);
    let name = format!("{} test swatches", s.design.name);
    let page = s.design.page;
    s.design = empty_like(&s.design, &name);
    // The object's sewn size (its own stitches, outline and pull compensation included).
    call(s, "object.paste", json!({ "objects": [object.clone()] }))?;
    let b = sewn_bounds(&s.design).ok_or_else(|| p.failed("the object sews nothing"))?;
    s.design.objects.clear();
    let (w, h) = (b.width() / MM, b.height() / MM);
    let label_h = if with_label { LABEL_MM + 2.0 } else { 0.0 };
    // Each copy's label: its value(s), as wide as the widest decides the column width.
    let label_of = |r: usize, c: usize| -> String {
        let mut t = vec![value_only(&rows.attribute, rows.values[r])];
        if let Some(c2) = &cols {
            t.push(value_only(&c2.attribute, c2.values[c]));
        }
        t.join(" ")
    };
    let mut widest: f64 = 0.0;
    if with_label {
        for r in 0..n_rows {
            for c in 0..n_cols {
                widest = widest.max(label_width(s, &label_of(r, c))?);
            }
        }
    }
    let w_cell = w.max(widest);
    let cell_w = w_cell + gap;
    let cell_h = h + label_h + gap;
    let need = (
        cell_w * grid_cols as f64 - gap,
        cell_h * grid_rows as f64 - gap,
    );
    let (pw, ph) = (page.width / MM, page.height / MM);
    if need.0 > pw + 1e-6 || need.1 > ph + 1e-6 {
        let fit_c = ((pw + gap) / cell_w).floor().max(0.0) as usize;
        let fit_r = ((ph + gap) / cell_h).floor().max(0.0) as usize;
        return Err(p.failed(format!(
            "the {grid_rows} × {grid_cols} grid needs {:.1} × {:.1} mm and the Design Page is \
             {pw:.1} × {ph:.1} mm; at most {} copies fit ({fit_r} × {fit_c}): make the object \
             smaller, lower the count or choose a larger hoop",
            need.0,
            need.1,
            fit_r * fit_c
        )));
    }
    let (x0, y0) = ((pw - need.0) / 2.0, (ph - need.1) / 2.0);
    let mut names = Vec::new();
    let mut labels = Vec::new();
    for r in 0..n_rows {
        for c in 0..n_cols {
            let k = r * n_cols + c;
            let (row, col) = if cols.is_some() {
                (r, c)
            } else {
                (k / grid_cols, k % grid_cols)
            };
            let (cx, cy) = (x0 + col as f64 * cell_w, y0 + row as f64 * cell_h);
            let pasted = call(
                s,
                "object.paste",
                json!({
                    "objects": [object.clone()],
                    "dx": cx + (w_cell - w) / 2.0 - b.min_x / MM, "dy": cy - b.min_y / MM,
                }),
            )?;
            let new = pasted["ids"][0].as_u64().expect("the pasted copy");
            let mut text = Vec::new();
            for (st, v) in [
                Some((&rows, rows.values[r])),
                cols.as_ref().map(|c2| (c2, c2.values[c])),
            ]
            .into_iter()
            .flatten()
            {
                let a = &st.attribute;
                call(s, a.command, json!({ "ids": [new], a.param: v }))
                    .map_err(|e| p.failed(format!("{} {v}: {e}", a.name)))?;
                text.push(value_text(a, v));
            }
            let text = text.join(" ");
            let at = s.design.index_of(new).expect("the copy");
            s.design.objects[at].name = Some(text.clone());
            if with_label {
                labels.push((label_of(r, c), cx + w_cell / 2.0, cy + h + 1.5));
            }
            names.push(text);
        }
    }
    // The labels after every copy, top-left first, in the copies' thread.
    for (text, x, top) in labels {
        label(s, &text, x, top, &thread)?;
    }
    s.design.properties.comments = format!("Test swatches of object {id}: {}", names.join("; "));
    let mut out = write(s, p)?;
    out["copies"] = json!(names);
    out["grid"] = json!([grid_rows, grid_cols]);
    Ok(out)
}

// MARK: #453, the fabric calibration card

/// Fill and satin density factors of the card's F and S rows, against the fabric's own.
pub const DENSITY_STEPS: [f64; 5] = [0.8, 0.9, 1.0, 1.1, 1.2];
/// Pull compensation of the card's P row, mm.
pub const PULL_STEPS: [f64; 5] = [0.0, 0.15, 0.3, 0.45, 0.6];
/// Card size, mm (fits the 100 × 100 mm hoop).
const CARD_MM: f64 = 100.0;

/// The settings row `n` (1–5) of a card row gives, for `fabric`'s `fill`, `satin` density.
pub fn row_value(row: char, n: usize, fill: f64, satin: f64) -> Option<f64> {
    let k = n.checked_sub(1).filter(|&k| k < 5)?;
    let round = |v: f64| (v * 100.0).round() / 100.0;
    Some(match row {
        'F' => round(fill * DENSITY_STEPS[k]),
        'S' => round(satin * DENSITY_STEPS[k]),
        'P' => PULL_STEPS[k],
        _ => return None,
    })
}

fn calibration_swatch(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let id = p.string("fabric", None)?;
    let f = crate::fabric::resolve(s, &id).ok_or_else(|| p.not_found(format!("fabric '{id}'")))?;
    let name = format!("Calibration card {}", f.name);
    s.design = empty_like(&s.design, &name);
    s.design.page.width = CARD_MM * MM;
    s.design.page.height = CARD_MM * MM;
    let ink = "Black";
    let mut rows = Vec::new();
    let mut labels = Vec::new();
    // Column centres and the three rows' tops, mm.
    let col = |k: usize| 14.0 + k as f64 * 18.0;
    for (row, top, kind) in [
        ('F', 6.0, "fill"),
        ('S', 32.0, "satin"),
        ('P', 58.0, "satin"),
    ] {
        for k in 0..5 {
            let v = row_value(row, k + 1, f.fill_density, f.satin_density).expect("1-5");
            let (w, h) = if kind == "fill" {
                (12.0, 12.0)
            } else {
                (4.0, 14.0)
            };
            let r = call(
                s,
                "shape.rectangle",
                json!({ "x": col(k) - w / 2.0, "y": top, "width": w, "height": h, "fill": ink }),
            )?;
            let oid = r["id"].as_u64().expect("the rectangle");
            let (density, pull) = match row {
                'F' => (v, 0.0),
                'S' => (v, 0.0),
                _ => (f.satin_density, v),
            };
            let mut set = json!({
                "ids": [oid], "density": density, "pullCompensation": pull,
                "stitchLength": f.stitch_length, "underlay": f.underlay.is_some(),
            });
            if kind == "satin" {
                set["type"] = json!("satin");
                // Satin across the bar: stitches run along its short side.
                set["angle"] = json!(0);
            } else {
                set["angle"] = json!(45);
            }
            call(s, "object.setRegion", set)?;
            let tag = format!("{row}{}", k + 1);
            let at = s.design.index_of(oid).expect("the piece");
            s.design.objects[at].name = Some(tag.clone());
            labels.push((tag.clone(), col(k), top + h + 1.5));
            rows.push(json!({
                "row": tag,
                "fillDensity": if row == 'F' { json!(v) } else { Value::Null },
                "satinDensity": if row == 'F' { Value::Null } else { json!(density) },
                "pullCompensation": pull,
            }));
        }
    }
    // A 10 mm scale square in running stitch, to check the sewn size with a ruler.
    let r = call(
        s,
        "shape.rectangle",
        json!({ "x": 6.0, "y": 84.0, "width": 10.0, "height": 10.0, "outline": ink }),
    )?;
    if let Some(oid) = r["id"].as_u64() {
        let at = s.design.index_of(oid).expect("the square");
        s.design.objects[at].name = Some("10 mm".into());
    }
    labels.push(("10 mm".to_string(), 30.0, 87.0));
    // Every label after the pieces, in their black thread.
    let black = main_thread(s.design.objects.last().expect("the scale square"));
    for (text, x, top) in labels {
        label(s, &text, x, top, &black)?;
    }
    s.design.properties.comments = format!(
        "Calibration card for {} ({}): F1-F5 fill density x{:?}, S1-S5 satin density x{:?}, \
         P1-P5 satin pull compensation {:?} mm. Sew it on the fabric, pick the best piece of each \
         row, then fabric.calibrate.",
        f.name, f.id, DENSITY_STEPS, DENSITY_STEPS, PULL_STEPS
    );
    let hoop = fit_smallest_hoop(s, p)?;
    let mut out = write(s, p)?;
    out["fabric"] = json!(f.id);
    out["rows"] = Value::Array(rows);
    out["hoop"] = json!(hoop);
    Ok(out)
}

fn row_param(p: &Params, key: &str) -> Result<usize, CommandError> {
    Ok(p.int(key, Some(3), 1, 5)? as usize)
}

fn calibrate(s: &mut EngineState, p: &Params) -> crate::CommandResult {
    let id = p.string("fabric", None)?;
    let f = crate::fabric::resolve(s, &id).ok_or_else(|| p.not_found(format!("fabric '{id}'")))?;
    let name = p.string("name", None)?;
    let name = name.trim();
    if name.is_empty() || name.chars().count() > 60 || name.chars().any(char::is_control) {
        return Err(p.invalid("name", "must be 1-60 printable characters"));
    }
    let pick = |row: char, key: &str| -> Result<f64, CommandError> {
        Ok(row_value(row, row_param(p, key)?, f.fill_density, f.satin_density).expect("1-5"))
    };
    let custom = crate::fabric::CustomFabric {
        id: crate::fabric::custom_id(name),
        name: name.to_string(),
        base: f.base_id.clone(),
        fill_density: pick('F', "fill")?,
        satin_density: pick('S', "satin")?,
        pull_compensation: pick('P', "pull")?,
        stitch_length: f.stitch_length,
    };
    let list = &mut s.user.custom_fabrics;
    let replaced = list.iter().any(|c| c.id == custom.id);
    list.retain(|c| c.id != custom.id);
    if list.len() >= crate::fabric::MAX_CUSTOM {
        return Err(p.failed(format!(
            "at most {} custom fabrics; delete one first (fabric.deleteCustom)",
            crate::fabric::MAX_CUSTOM
        )));
    }
    list.push(custom.clone());
    Ok(json!({ "fabric": crate::fabric::describe_custom(&custom), "replaced": replaced }))
}

pub fn commands() -> Vec<CommandSpec> {
    let path = || {
        req(
            "path",
            "string",
            "Where to write the swatch: a machine file (format from the extension, PES when none) or .osd; the open design is not changed",
        )
    };
    vec![
        spec(
            "design.testSwatch",
            "Write the chosen objects alone as a test swatch, centred in the smallest hoop they fit (beyond PE-DESIGN, #452); the open design is not changed",
            Effect::File,
            vec![
                IDS,
                path(),
                opt(
                    "label",
                    "bool",
                    "Sew the first object's main settings as a 3.5 mm running-stitch label below it (default false)",
                ),
            ],
            test_swatch,
        ),
        spec(
            "design.testSwatches",
            "Copy one object in a grid with one setting stepped (or two: rows × columns), each copy named and labelled with its value, as a new design file (beyond PE-DESIGN, #507); the open design is not changed",
            Effect::File,
            vec![
                req("id", "int", "The object to vary"),
                path(),
                req(
                    "attribute",
                    "density|stitchLength|pullCompensation|angle|underlaySpacing|linePitch|lineWidth|lineDensity",
                    "Setting to step; only those the object's sewing uses (region settings need a region, line settings a line)",
                ),
                req("start", "number", "First value (mm, rows/mm or degrees)"),
                req("step", "number", "Added for each next copy"),
                req("count", "2-25", "Copies"),
                opt(
                    "attribute2",
                    "string",
                    "A second setting, stepped across the columns (rows × columns ≤ 25)",
                ),
                opt("start2", "number", "attribute2's first value"),
                opt("step2", "number", "attribute2's step"),
                opt("count2", "2-25", "attribute2's copies"),
                opt("gap", "mm", "Space between copies (1-50, default 4)"),
                opt(
                    "label",
                    "bool",
                    "Sew each copy's value below it in 3.5 mm running stitch (default true)",
                ),
            ],
            test_swatches,
        ),
        spec(
            "fabric.calibrationSwatch",
            "Write a 100 × 100 mm calibration card for a fabric (beyond PE-DESIGN, #453): fill squares F1-F5 at 0.8-1.2 × its fill density, satin bars S1-S5 at 0.8-1.2 × its satin density, satin bars P1-P5 with 0-0.6 mm pull compensation, and a 10 mm scale square; returns each piece's settings",
            Effect::File,
            vec![
                req(
                    "fabric",
                    "string",
                    "Fabric id from fabric.list (a custom one too)",
                ),
                path(),
            ],
            calibration_swatch,
        ),
        spec(
            "fabric.calibrate",
            "Save the calibration card's best pieces as a custom fabric (beyond PE-DESIGN, #453): fill density from row F, satin density from S, pull compensation from P; fabric.apply then gives it to the design's objects",
            Effect::Settings,
            vec![
                req("fabric", "string", "The fabric the card was sewn for"),
                req(
                    "name",
                    "string",
                    "Name of the custom fabric (1-60 characters; the same name replaces it)",
                ),
                opt(
                    "fill",
                    "1-5",
                    "Best fill square F1-F5 (default 3, the fabric's own)",
                ),
                opt("satin", "1-5", "Best satin bar S1-S5 (default 3)"),
                opt(
                    "pull",
                    "1-5",
                    "Best pull compensation bar P1-P5 (default 3)",
                ),
            ],
            calibrate,
        ),
    ]
}
