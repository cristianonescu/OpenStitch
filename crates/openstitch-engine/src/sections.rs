//! Split designs, multi-position hoops and the jumbo frame: `sections.set` (settings),
//! `sections.list` (the hoopings in sewing order) and `sections.export` (one machine file per
//! hooping). The planning itself is `openstitch_design::sections`.

use crate::commands::{CommandSpec, Effect, ParamDoc, RunFn};
use crate::params::{CommandError, CommandResult, Params};
use crate::session::EngineState;
use openstitch_design::machine::brother_hoop;
use openstitch_design::sections::{
    self, Hooping, MarkStyle, SectionSettings, check_splits, has_design_stitches, sectionable,
};
use openstitch_design::{Compiler, Design};
use serde_json::{Value, json};

const MM: f64 = 10.0;

fn spec(
    id: &'static str,
    summary: &'static str,
    effect: Effect,
    params: Vec<ParamDoc>,
    run: RunFn,
) -> CommandSpec {
    CommandSpec {
        id,
        summary,
        effect,
        params,
        run,
    }
}

const fn opt(name: &'static str, kind: &'static str, summary: &'static str) -> ParamDoc {
    ParamDoc {
        name,
        kind,
        required: false,
        summary,
    }
}

#[cfg(feature = "formats")]
const fn req(name: &'static str, kind: &'static str, summary: &'static str) -> ParamDoc {
    ParamDoc {
        name,
        kind,
        required: true,
        summary,
    }
}

pub fn commands() -> Vec<CommandSpec> {
    #[cfg_attr(not(feature = "formats"), allow(unused_mut))]
    let mut v = vec![
        spec(
            "sections.set",
            "Split-design and large-hoop settings: section hoop for a custom Design Page (sections 10 mm smaller than the hoop), smaller sections, split positions, hoop-change optimisation for multi-position hoops, jumbo overlap",
            Effect::Edit,
            vec![
                opt(
                    "hoop",
                    "string",
                    "Brother hoop id giving the section size, or none to stop splitting (custom Design Page only)",
                ),
                opt(
                    "smaller",
                    "bool",
                    "Use smaller section sizes (10 mm less on every side)",
                ),
                opt(
                    "splitsX",
                    "[mm]",
                    "Vertical split lines from the left page edge (empty: even split)",
                ),
                opt(
                    "splitsY",
                    "[mm]",
                    "Horizontal split lines from the top page edge (empty: even split)",
                ),
                opt(
                    "optimizeHoopChange",
                    "bool",
                    "Multi-position hoops: group patterns by position",
                ),
                opt(
                    "jumboOverlap",
                    "mm",
                    "Jumbo frame: overlap of the two sides (0-50)",
                ),
                opt(
                    "mode",
                    "grid|free",
                    "grid: PE-DESIGN's sections cut at the borders (default); free: hoopings placed anywhere, each object sewn whole in the first that holds it (beyond PE-DESIGN)",
                ),
                opt(
                    "hoopings",
                    "[{x, y, rotated}]",
                    "Free mode: each hooping's top-left corner on the page (mm) in sewing order; rotated turns the hoop 90°",
                ),
                opt(
                    "minOverlap",
                    "mm",
                    "Free mode: overlap needed between consecutive hoopings for the marks (default 16)",
                ),
                opt(
                    "marks",
                    "boxX|cross|none",
                    "Free mode: alignment mark sewn at the end of a hooping and the start of the next (default boxX)",
                ),
                opt(
                    "stopAfterMarks",
                    "bool",
                    "Free mode: stop after the marks at the start of a hooping (default true)",
                ),
            ],
            set,
        ),
        spec(
            "sections.autoLayout",
            "Free hoopings: the fewest hoopings of the section hoop (either orientation) that cover the design with the minimum overlap; switches to free mode",
            Effect::Edit,
            vec![opt(
                "hoop",
                "string",
                "Brother hoop id giving the section size (default: the one set)",
            )],
            auto_layout,
        ),
        spec(
            "sections.list",
            "The hoopings in sewing order (split sections, multi-position positions or jumbo sides) with their areas, alignment stitching and problems",
            Effect::Read,
            vec![],
            |s, _| Ok(list(&s.design)),
        ),
    ];
    #[cfg(feature = "formats")]
    v.push(export_command());
    v
}

fn numbers(p: &Params, key: &str) -> Result<Vec<f64>, CommandError> {
    let a = p.array(key)?;
    if a.len() > sections::MAX_SPLITS {
        return Err(p.invalid(key, format!("at most {} positions", sections::MAX_SPLITS)));
    }
    a.iter()
        .map(|v| {
            v.as_f64()
                .filter(|f| f.is_finite())
                .ok_or_else(|| p.invalid(key, "must be numbers (mm)"))
        })
        .collect()
}

fn set(s: &mut EngineState, p: &Params) -> CommandResult {
    let m = &mut s.design.machine;
    let (w, h) = (s.design.page.width / MM, s.design.page.height / MM);
    if p.has("optimizeHoopChange") {
        m.optimize_hoop_change = p.bool("optimizeHoopChange", None)?;
    }
    if p.has("jumboOverlap") {
        m.jumbo_overlap = p.req("jumboOverlap", 0.0, 50.0)?;
    }
    if p.has("hoop") {
        let key = p.string("hoop", None)?;
        if key == "none" {
            m.sections = None;
        } else {
            if m.hoop.is_some() {
                return Err(
                    p.failed("split designs need a custom Design Page (machine.set hoop: custom)")
                );
            }
            match brother_hoop(&key) {
                Some(h) if sectionable(h) => {}
                Some(_) => {
                    return Err(p.invalid(
                        "hoop",
                        "multi-position hoops and the jumbo frame cannot give a section size",
                    ));
                }
                None => return Err(p.not_found(format!("hoop {key}"))),
            }
            let keep = m.sections.as_ref().is_some_and(|x| x.hoop == key);
            let mut next = m.sections.clone().filter(|_| keep).unwrap_or_default();
            // Changing the section size resets the split positions, as in PE-DESIGN.
            next.hoop = key;
            m.sections = Some(next);
        }
    }
    let wants_free = ["mode", "hoopings", "minOverlap", "marks", "stopAfterMarks"]
        .iter()
        .any(|k| p.has(k));
    if wants_free {
        let Some(sec) = m.sections.as_mut() else {
            return Err(p.failed("set a section hoop first (sections.set hoop)"));
        };
        match p
            .string(
                "mode",
                Some(if sec.free.is_some() { "free" } else { "grid" }),
            )?
            .as_str()
        {
            "grid" => {
                if ["hoopings", "minOverlap", "marks", "stopAfterMarks"]
                    .iter()
                    .any(|k| p.has(k))
                {
                    return Err(p.invalid("mode", "hoopings and marks need mode: free"));
                }
                sec.free = None;
            }
            "free" => {
                let mut f = sec.free.clone().unwrap_or_default();
                if p.has("hoopings") {
                    f.hoopings = hoopings(p)?;
                }
                if p.has("minOverlap") {
                    f.min_overlap = p.req("minOverlap", 0.0, 100.0)?;
                }
                if p.has("marks") {
                    f.marks = MarkStyle::parse(&p.string("marks", None)?)
                        .ok_or_else(|| p.invalid("marks", "use boxX, cross or none"))?;
                }
                if p.has("stopAfterMarks") {
                    f.stop_after_marks = p.bool("stopAfterMarks", None)?;
                }
                sec.free = Some(f);
            }
            _ => return Err(p.invalid("mode", "use grid or free")),
        }
    }
    let wants_split = ["smaller", "splitsX", "splitsY"].iter().any(|k| p.has(k));
    if wants_split {
        let Some(sec) = m.sections.as_mut() else {
            return Err(p.failed("set a section hoop first (sections.set hoop)"));
        };
        if p.has("smaller") {
            sec.smaller = p.bool("smaller", None)?;
            if !(p.has("splitsX") || p.has("splitsY")) {
                sec.splits_x.clear();
                sec.splits_y.clear();
            }
        }
        if p.has("splitsX") {
            sec.splits_x = numbers(p, "splitsX")?;
        }
        if p.has("splitsY") {
            sec.splits_y = numbers(p, "splitsY")?;
        }
        let (sw, sh) = sec
            .section_size()
            .ok_or_else(|| p.failed("the section hoop has no section size"))?;
        for (key, v, len, max) in [
            ("splitsX", &sec.splits_x, w, sw),
            ("splitsY", &sec.splits_y, h, sh),
        ] {
            if !v.is_empty() {
                check_splits(v, len, max).map_err(|e| p.invalid(key, e))?;
            }
        }
    }
    Ok(list(&s.design))
}

fn hoopings(p: &Params) -> Result<Vec<Hooping>, CommandError> {
    let a = p.array("hoopings")?;
    if a.len() > sections::MAX_HOOPINGS {
        return Err(p.invalid(
            "hoopings",
            format!("at most {} hoopings", sections::MAX_HOOPINGS),
        ));
    }
    a.iter()
        .map(|v| {
            let n = |k: &str| v.get(k).and_then(Value::as_f64).filter(|f| f.is_finite());
            match (n("x"), n("y")) {
                (Some(x), Some(y)) if x.abs() <= 5000.0 && y.abs() <= 5000.0 => Ok(Hooping {
                    x,
                    y,
                    rotated: v.get("rotated").and_then(Value::as_bool).unwrap_or(false),
                }),
                _ => Err(p.invalid("hoopings", "each needs x and y in mm (-5000-5000)")),
            }
        })
        .collect()
}

fn auto_layout(s: &mut EngineState, p: &Params) -> CommandResult {
    if p.has("hoop") {
        set(s, p)?;
    }
    let design = s.design.clone();
    let Some(sec) = s.design.machine.sections.as_mut() else {
        return Err(p.failed("set a section hoop first (sections.set hoop)"));
    };
    let size = sec
        .section_size()
        .ok_or_else(|| p.failed("the section hoop has no section size"))?;
    let mut f = sec.free.clone().unwrap_or_default();
    f.hoopings = sections::auto_layout(&design, size, f.min_overlap);
    if f.hoopings.is_empty() {
        return Err(
            p.failed("the design has nothing to lay out, or the hoop is smaller than the overlap")
        );
    }
    sec.free = Some(f);
    Ok(list(&s.design))
}

fn mm_rect(r: &openstitch_design::Rect) -> Value {
    json!({
        "x": r.min_x / MM, "y": r.min_y / MM,
        "width": r.width() / MM, "height": r.height() / MM,
    })
}

fn settings_json(s: &Option<SectionSettings>) -> Value {
    match s {
        None => Value::Null,
        Some(s) => {
            let (w, h) = s.section_size().unwrap_or_default();
            let mut v = json!({
                "hoop": s.hoop, "smaller": s.smaller,
                "sectionWidthMM": w, "sectionHeightMM": h,
                "splitsX": s.splits_x, "splitsY": s.splits_y,
                "mode": if s.free.is_some() { "free" } else { "grid" },
            });
            if let Some(f) = &s.free {
                v["hoopings"] = json!(
                    f.hoopings
                        .iter()
                        .map(|h| json!({
                            "x": h.x, "y": h.y, "rotated": h.rotated,
                        }))
                        .collect::<Vec<_>>()
                );
                v["minOverlapMM"] = json!(f.min_overlap);
                v["marks"] = json!(f.marks.as_str());
                v["stopAfterMarks"] = json!(f.stop_after_marks);
            }
            v
        }
    }
}

fn list(d: &Design) -> Value {
    let plan = sections::plan(d);
    let full = Compiler::for_design(d).compile(d);
    let items: Vec<Value> = plan
        .sections
        .iter()
        .map(|sec| {
            let pattern = sections::section_pattern(d, &full, sec);
            let mut v = json!({
                "label": sec.label,
                "kind": sec.kind,
                "area": mm_rect(&sec.rect),
                "rotate180": sec.rotate180,
                "alignmentBefore": sec.marks_before.len(),
                "alignmentAfter": sec.marks_after.len(),
                "stopAfterMarks": sec.stop_after_marks && !sec.marks_before.is_empty(),
                "stitches": pattern.count(openstitch_core::Command::Stitch),
                "empty": !has_design_stitches(&pattern),
            });
            if let Some(ids) = &sec.objects {
                v["objects"] = json!(ids);
            }
            v
        })
        .collect();
    json!({
        "sections": items,
        "errors": plan.errors,
        "settings": settings_json(&d.machine.sections),
        "optimizeHoopChange": d.machine.optimize_hoop_change,
        "jumboOverlapMM": d.machine.jumbo_overlap,
    })
}

#[cfg(feature = "formats")]
fn export_command() -> CommandSpec {
    use crate::sandbox::Access;
    use openstitch_formats::FileType;
    spec(
        "sections.export",
        "Write one machine file per hooping (split section, multi-position position, jumbo side: <name>_<label>.<ext>; free hooping: <name> - hooping k of n.<ext>), each centred on its hoop",
        Effect::File,
        vec![
            req(
                "path",
                "string",
                "Base file path; the format comes from its extension (design.pes → design_Aa.pes, …)",
            ),
            opt(
                "alignment",
                "bool",
                "Include alignment stitching for split designs (default true)",
            ),
            opt(
                "includeEmpty",
                "bool",
                "Also write sections without design stitches",
            ),
        ],
        |s, p| {
            let base = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
            let ext = base
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or_default()
                .to_string();
            let ty = FileType::from_extension(&ext)
                .filter(|t| t.can_write())
                .ok_or_else(|| p.invalid("path", "extension must be a writable machine format"))?;
            let stem = base
                .file_stem()
                .and_then(|e| e.to_str())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| p.invalid("path", "needs a file name"))?
                .to_string();
            let alignment = p.bool("alignment", Some(true))?;
            let include_empty = p.bool("includeEmpty", Some(false))?;
            let mut plan = sections::plan(&s.design);
            if !plan.errors.is_empty() {
                return Err(p.failed(plan.errors.join("; ")));
            }
            if !alignment {
                for sec in &mut plan.sections {
                    sec.marks_before.clear();
                    sec.marks_after.clear();
                }
            }
            let full = Compiler::for_design(&s.design).compile(&s.design);
            let mut files = Vec::new();
            let mut skipped = Vec::new();
            // Check every section before writing any file.
            let mut out = Vec::new();
            for sec in &plan.sections {
                let pattern = sections::section_pattern(&s.design, &full, sec);
                if !include_empty && !has_design_stitches(&pattern) {
                    skipped.push(sec.label.clone());
                    continue;
                }
                let bytes = ty
                    .write(&pattern)
                    .map_err(|e| p.failed(format!("{}: {e}", sec.label)))?;
                let name = if sec.kind == sections::SectionKind::Hooping {
                    // "Name - hooping k of n" (#515).
                    format!("{stem} - {}.{}", sec.label, ty.extension())
                } else {
                    format!("{stem}_{}.{}", sec.label, ty.extension())
                };
                out.push((base.with_file_name(name), bytes, sec.label.clone()));
            }
            if out.is_empty() {
                return Err(p.failed("no section has stitches"));
            }
            for (path, bytes, label) in out {
                std::fs::write(&path, bytes).map_err(|e| p.failed(e.to_string()))?;
                files.push(json!({ "label": label, "path": path.display().to_string() }));
            }
            Ok(json!({ "files": files, "skipped": skipped }))
        },
    )
}
