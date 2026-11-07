//! `design.print`: printable PDF output (Actual Size, Reduced Size, positioning sheets, Print
//! and Stitch). Layout and the PDF writer are `openstitch-print`.

use crate::commands::{CommandSpec, Effect, ParamDoc};
use crate::params::{CommandResult, Params};
use crate::sandbox::Access;
use crate::session::EngineState;
use openstitch_print::{Options, PAPERS};
use serde_json::json;

const fn opt(name: &'static str, kind: &'static str, summary: &'static str) -> ParamDoc {
    ParamDoc {
        name,
        kind,
        required: false,
        summary,
    }
}

/// Most characters in one job field (notes: [`MAX_NOTES`]).
pub const MAX_FIELD: usize = 200;
/// Most characters in the job notes.
pub const MAX_NOTES: usize = 2000;

fn job_sheet(
    d: &openstitch_design::Design,
    p: &Params,
    mode: &str,
    opts: &Options,
) -> Result<Vec<openstitch_print::Page>, crate::CommandError> {
    let detailed = match p.string("detail", Some("overview"))?.as_str() {
        "overview" => false,
        "detailed" => true,
        _ => return Err(p.invalid("detail", "use overview or detailed")),
    };
    let shop = p.string("shop", Some(""))?;
    if shop.chars().count() > 80 || shop.chars().any(char::is_control) {
        return Err(p.invalid("shop", "up to 80 printable characters"));
    }
    let j = d.job.clone().unwrap_or_default();
    let job = openstitch_print::Job {
        customer: j.customer,
        order: j.order,
        due: j.due,
        notes: j.notes,
        fabric: j.fabric,
        stabiliser: j.stabiliser,
        shop,
    };
    let sheet = if mode == "client" {
        openstitch_print::Sheet::Client
    } else {
        openstitch_print::Sheet::Operator
    };
    Ok(openstitch_print::job_sheet(d, &job, sheet, detailed, opts))
}

fn set_job(s: &mut EngineState, p: &Params) -> CommandResult {
    let mut job = s.design.job.clone().unwrap_or_default();
    if p.bool("clear", Some(false))? {
        job = openstitch_design::JobInfo::default();
    }
    for key in openstitch_design::JobInfo::KEYS {
        if !p.has(key) {
            continue;
        }
        let v = p.string(key, None)?;
        let max = if key == "notes" { MAX_NOTES } else { MAX_FIELD };
        let bad = v
            .chars()
            .any(|c| c.is_control() && !(key == "notes" && c == '\n'));
        if v.chars().count() > max || bad {
            return Err(p.invalid(key, format!("up to {max} printable characters")));
        }
        *job.field_mut(key).expect("a job key") = v.trim().to_string();
    }
    s.design.job = (!job.is_empty()).then_some(job);
    Ok(serde_json::to_value(s.design.job.clone().unwrap_or_default()).unwrap_or_default())
}

pub fn commands() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: "design.setJob",
            summary: "Set the job the design is sewn for, printed on the job sheets (design.print mode client or operator; #513, beyond PE-DESIGN): customer, order, due date, notes, fabric, stabiliser. Saved in the project only, never in machine files; an empty string clears a field, clear: true all of them. Returns the job",
            effect: Effect::Edit,
            params: vec![
                opt("customer", "string", "Customer (up to 200 characters)"),
                opt("order", "string", "Order or job number"),
                opt("due", "string", "Due date, as the user writes it"),
                opt(
                    "notes",
                    "string",
                    "Notes for the sheets (up to 2000 characters, line breaks kept)",
                ),
                opt("fabric", "string", "Fabric (operator sheet)"),
                opt(
                    "stabiliser",
                    "string",
                    "Stabiliser or backing (operator sheet)",
                ),
                opt("clear", "bool", "Clear every field first (default false)"),
            ],
            run: set_job,
        },
        CommandSpec {
            id: "design.print",
            summary: "Write a printable PDF: actual size (1:1, tiled over several sheets if needed, with a sewing information page), reduced (one page), positioning sheets (one per hooping) or Print and Stitch (background image and alignment sheet)",
            effect: Effect::File,
            params: vec![
                ParamDoc {
                    name: "path",
                    kind: "string",
                    required: true,
                    summary: "Output .pdf file",
                },
                opt(
                    "mode",
                    "actual|reduced|positioning|image|client|operator",
                    "What to print (default actual); client and operator are job sheets (#513, beyond PE-DESIGN): the client approval sheet (picture at size or fitted, size in mm and inches, thread colours, job fields, approval line) or the operator worksheet (job fields, hoop, size, stitches, stops, trims, sewing time, fabric, stabiliser, notes, colour sequence table); the job fields come from design.setJob",
                ),
                opt(
                    "detail",
                    "overview|detailed",
                    "Job sheets: overview (one page, default) or detailed (client: plus the actual-size pages; operator: plus one thumbnail per colour step)",
                ),
                opt(
                    "shop",
                    "string",
                    "Job sheets: the shop's name printed at the top (up to 80 characters)",
                ),
                opt(
                    "paper",
                    "a4|letter|a3|legal",
                    "Paper size (default a4); orientation is chosen to fit",
                ),
                opt(
                    "sewingAreaBox",
                    "bool",
                    "Sewing area box and centre axes (default true)",
                ),
                opt(
                    "centerMark",
                    "bool",
                    "Embroidery positioning mark at the centre (default true)",
                ),
                opt(
                    "templateGrid",
                    "bool",
                    "10 mm template grid in green (default false)",
                ),
                opt(
                    "units",
                    "mm|inch",
                    "Scale bars on actual-size pages: mm draws the 50 mm bar (default), inch adds a 2-inch bar",
                ),
                opt(
                    "mirror",
                    "bool",
                    "Print and Stitch: mirror the image for iron-on transfer sheets",
                ),
            ],
            run,
        },
    ]
}

fn run(s: &mut EngineState, p: &Params) -> CommandResult {
    let path = s.resolve(&p.string("path", None)?, Access::Write, &p.command)?;
    if !path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
    {
        return Err(p.invalid("path", "must end in .pdf"));
    }
    let paper_name = p.string("paper", Some("a4"))?;
    let (_, pw, ph) = PAPERS
        .iter()
        .find(|(n, _, _)| *n == paper_name)
        .copied()
        .ok_or_else(|| p.invalid("paper", "use a4, letter, a3 or legal"))?;
    let opts = Options {
        paper: (pw, ph),
        sewing_area_box: p.bool("sewingAreaBox", Some(true))?,
        center_mark: p.bool("centerMark", Some(true))?,
        template_grid: p.bool("templateGrid", Some(false))?,
        inches: match p.string("units", Some("mm"))?.as_str() {
            "mm" => false,
            "inch" => true,
            _ => return Err(p.invalid("units", "use mm or inch")),
        },
    };
    let mode = p.string("mode", Some("actual"))?;
    let d = &s.design;
    let pages = match mode.as_str() {
        "actual" => openstitch_print::actual_size(d, &opts),
        "reduced" => openstitch_print::reduced(d, &opts),
        "positioning" => {
            let plan = openstitch_design::sections::plan(d);
            if !plan.errors.is_empty() {
                return Err(p.failed(plan.errors.join("; ")));
            }
            openstitch_print::positioning_sheets(d, &opts)
        }
        "image" => {
            let mirror = p.bool("mirror", Some(false))?;
            let image = background_image(s, p)?;
            openstitch_print::print_and_stitch(d, image, mirror, &opts).map_err(|e| p.failed(e))?
        }
        "client" | "operator" => job_sheet(d, p, &mode, &opts)?,
        _ => {
            return Err(p.invalid(
                "mode",
                "use actual, reduced, positioning, image, client or operator",
            ));
        }
    };
    let bytes = openstitch_print::write(&pages, &d.name);
    std::fs::write(&path, &bytes).map_err(|e| p.failed(e.to_string()))?;
    Ok(json!({ "path": path.display().to_string(), "pages": pages.len(), "bytes": bytes.len() }))
}

#[cfg(feature = "digitize")]
fn background_image(
    s: &EngineState,
    p: &Params,
) -> Result<openstitch_print::Image, crate::CommandError> {
    let bg = s
        .design
        .background
        .as_ref()
        .ok_or_else(|| p.failed("the design has no background image (background.set)"))?;
    let path = s.resolve(&bg.path, Access::Read, &p.command)?;
    let meta = std::fs::metadata(&path)
        .map_err(|_| p.failed(format!("cannot read {}", path.display())))?;
    if meta.len() > 64 << 20 {
        return Err(p.failed("background image too large"));
    }
    let data =
        std::fs::read(&path).map_err(|_| p.failed(format!("cannot read {}", path.display())))?;
    let r = openstitch_digitize::Raster::decode(&data)
        .map_err(|e| p.failed(e.to_string()))?
        .with_adjust(&bg.adjust);
    let (w, h) = (r.width(), r.height());
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for y in 0..h {
        for x in 0..w {
            // Transparent pixels print as paper white.
            let [cr, cg, cb, a] = r.pixel(x, y);
            let mix =
                |c: u8| ((u32::from(c) * u32::from(a) + 255 * (255 - u32::from(a))) / 255) as u8;
            rgb.extend([mix(cr), mix(cg), mix(cb)]);
        }
    }
    Ok(openstitch_print::Image {
        width: w,
        height: h,
        rgb,
    })
}

#[cfg(not(feature = "digitize"))]
fn background_image(
    _: &EngineState,
    p: &Params,
) -> Result<openstitch_print::Image, crate::CommandError> {
    Err(p.failed("this build cannot decode images (digitize feature off)"))
}
