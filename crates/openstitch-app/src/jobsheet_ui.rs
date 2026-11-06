//! File ▸ Print ▸ Job Sheet… (beyond PE-DESIGN, #513): the job fields (`design.setJob`, one
//! undo step, saved with the project only) and the client approval sheet or operator worksheet
//! (`design.print` with mode `client` or `operator`), printed through the system or saved as a
//! PDF. The dialog keeps its state in egui's memory; the job lives in the design.

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_design::JobInfo;
use serde_json::{Value, json};

#[derive(Clone, Debug)]
struct JobDialog {
    open: bool,
    /// The fields being edited; loaded from the design when the dialog opens.
    job: JobInfo,
    loaded: bool,
    shop: String,
    operator: bool,
    detailed: bool,
    paper: &'static str,
}

impl Default for JobDialog {
    fn default() -> Self {
        JobDialog {
            open: false,
            job: JobInfo::default(),
            loaded: false,
            shop: String::new(),
            operator: false,
            detailed: false,
            paper: "a4",
        }
    }
}

const PAPERS: [(&str, &str); 2] = [("a4", "A4"), ("letter", "Letter")];

fn id() -> egui::Id {
    egui::Id::unique("job-sheet-dialog")
}

pub fn open(ctx: &egui::Context) {
    ctx.data_mut(|d| {
        let s = d.get_temp_mut_or_default::<JobDialog>(id());
        s.open = true;
        s.loaded = false;
    });
}

/// `design.setJob` parameters for the edited fields (every field, so emptied ones clear).
fn set_job_params(job: &JobInfo) -> Value {
    json!({
        "customer": job.customer, "order": job.order, "due": job.due,
        "notes": job.notes, "fabric": job.fabric, "stabiliser": job.stabiliser,
    })
}

/// `design.print` parameters for the chosen sheet (all but `path`).
fn print_params(d: &JobDialog) -> serde_json::Map<String, Value> {
    let mut p = serde_json::Map::new();
    p.insert(
        "mode".into(),
        json!(if d.operator { "operator" } else { "client" }),
    );
    p.insert(
        "detail".into(),
        json!(if d.detailed { "detailed" } else { "overview" }),
    );
    p.insert("paper".into(), json!(d.paper));
    p.insert("shop".into(), json!(d.shop.trim()));
    p
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if std::mem::take(&mut app.ui.open_job_sheet) {
        open(ctx);
    }
    let mut d: JobDialog = ctx.data_mut(|m| m.get_temp_mut_or_default::<JobDialog>(id()).clone());
    if !d.open {
        return;
    }
    if !d.loaded {
        d.job = app.session.design().job.clone().unwrap_or_default();
        d.loaded = true;
    }
    let saved = app.session.design().job.clone().unwrap_or_default();
    // Some(true): print through the system; Some(false): save the PDF.
    let mut print: Option<bool> = None;
    let mut apply = false;
    let mut open = true;
    egui::Window::new("Job Sheet")
        .open(&mut open)
        .resizable(false)
        .default_width(380.0)
        .show(ctx, |ui| {
            egui::Grid::new("job-fields").num_columns(2).show(ui, |ui| {
                for (label, value) in [
                    ("Customer", &mut d.job.customer),
                    ("Order", &mut d.job.order),
                    ("Due", &mut d.job.due),
                    ("Fabric", &mut d.job.fabric),
                    ("Stabiliser", &mut d.job.stabiliser),
                ] {
                    ui.label(label);
                    ui.text_edit_singleline(value);
                    ui.end_row();
                }
                ui.label("Notes");
                ui.text_edit_multiline(&mut d.job.notes);
                ui.end_row();
                ui.label("Shop name");
                ui.text_edit_singleline(&mut d.shop)
                    .on_hover_text("Printed at the top of the sheet; not saved with the design");
                ui.end_row();
            });
            ui.label(
                egui::RichText::new(
                    "The job is saved with the design (.osd), never in machine files.",
                )
                .weak()
                .small(),
            );
            if ui
                .add_enabled(d.job != saved, egui::Button::new("Save Job Fields"))
                .clicked()
            {
                apply = true;
            }
            ui.separator();
            ui.horizontal(|ui| {
                ui.radio_value(&mut d.operator, false, "Client approval");
                ui.radio_value(&mut d.operator, true, "Operator worksheet");
            });
            ui.horizontal(|ui| {
                ui.radio_value(&mut d.detailed, false, "Overview");
                ui.radio_value(&mut d.detailed, true, "Detailed");
            });
            ui.horizontal(|ui| {
                ui.label("Paper");
                for (p, label) in PAPERS {
                    ui.selectable_value(&mut d.paper, p, label);
                }
            });
            ui.horizontal(|ui| {
                if ui
                    .button("Print…")
                    .on_hover_text("Saves the job fields, then opens the system print dialog")
                    .clicked()
                {
                    print = Some(true);
                }
                if ui.button("Save PDF…").clicked() {
                    print = Some(false);
                }
            });
        });
    d.open = open && print.is_none();
    // Printing prints the fields as edited: they are saved first (one undo step).
    if (apply || print.is_some()) && d.job != saved {
        app.execute(&CommandRequest::new(
            "design.setJob",
            set_job_params(&d.job),
        ));
    }
    let params = print_params(&d);
    ctx.data_mut(|m| m.insert_temp(id(), d));
    let Some(system) = print else {
        return;
    };
    let name = app.session.design().name.clone();
    let path = if system {
        let dir = openstitch_app::os_print::print_dir();
        if let Err(e) = openstitch_app::os_print::prepare_dir(&dir) {
            app.status = format!("{}: {e}", dir.display());
            return;
        }
        openstitch_app::os_print::temp_pdf_in(&dir, &name)
    } else {
        let Some(path) = rfd::FileDialog::new()
            .add_filter("PDF", &["pdf"])
            .set_file_name(format!("{name} job sheet.pdf"))
            .save_file()
        else {
            return;
        };
        path
    };
    let mut params = params;
    params.insert("path".into(), json!(path.display().to_string()));
    if let Some(r) = app.execute(&CommandRequest::new("design.print", Value::Object(params))) {
        if system {
            app.print_pdf(&path);
        } else {
            app.status = format!("Saved {} ({} pages)", path.display(), r["pages"]);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_dialog_asks_for_the_sheet_and_the_job_it_shows() {
        let mut d = JobDialog::default();
        d.job.customer = "Example Shop".into();
        d.shop = " Example Embroidery ".into();
        let p = print_params(&d);
        assert_eq!(
            (p["mode"].clone(), p["detail"].clone(), p["shop"].clone()),
            (
                json!("client"),
                json!("overview"),
                json!("Example Embroidery")
            )
        );
        d.operator = true;
        d.detailed = true;
        let p = print_params(&d);
        assert_eq!(
            (p["mode"].clone(), p["detail"].clone()),
            (json!("operator"), json!("detailed"))
        );
        // Every field is sent, so a field emptied in the dialog is cleared in the design.
        let j = set_job_params(&d.job);
        assert_eq!(j["customer"], json!("Example Shop"));
        assert_eq!(j["order"], json!(""));
    }

    /// The Assistant opens the dialog with `ui.run {action: "jobSheet"}`; it shows on the next
    /// frame.
    #[test]
    fn ui_run_opens_the_job_sheet_dialog() {
        let mut app = EditorApp::new(crate::app::Launch {
            demo: true,
            ..crate::app::Launch::default()
        });
        assert!(!app.ui.open_job_sheet);
        app.ui_command("ui.run", Some(&json!({ "action": "jobSheet" })))
            .expect("runs");
        assert!(app.ui.open_job_sheet);
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| window(&mut app, ui.ctx()));
        out.textures_delta.clear();
        assert!(!app.ui.open_job_sheet, "taken by the window");
    }
}
