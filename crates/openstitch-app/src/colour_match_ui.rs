//! "Match to Thread Chart" (#484) and "Merge Similar Colours" (#485), opened from the User
//! Thread Charts window. Both show the engine's dry-run plan (`threads.remap` /
//! `threads.mergeSimilar` with `dryRun`) before anything changes; Apply is one undo step, and
//! "Restore original colours" undoes it while the window is open.

use crate::app::EditorApp;
use eframe::egui::{self, Color32};
use openstitch_app::editor::CommandRequest;
use serde_json::{Value, json};

/// Where a match goes: a built-in chart id or a user chart name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    Builtin(String),
    User(String),
}

pub struct MatchState {
    pub match_open: bool,
    pub merge_open: bool,
    pub target: Target,
    /// `rgb` (PE-DESIGN), `ciede2000` or `oklab`.
    pub metric: &'static str,
    pub keep_distinct: bool,
    /// The plan last asked for, and the parameters it was asked with.
    plan: Option<(Value, Value)>,
    pub max_delta_e: f64,
    merge_plan: Option<(Value, Value)>,
    /// Apply ran from one of these windows and has not been undone here.
    applied: bool,
}

impl Default for MatchState {
    fn default() -> Self {
        Self {
            match_open: false,
            merge_open: false,
            target: Target::Builtin("brother".into()),
            metric: "rgb",
            keep_distinct: false,
            plan: None,
            max_delta_e: 5.0,
            merge_plan: None,
            applied: false,
        }
    }
}

fn swatch(ui: &mut egui::Ui, hex: &str) {
    let c = u32::from_str_radix(hex.trim_start_matches('#'), 16).unwrap_or(0);
    let (r, _) = ui.allocate_exact_size(egui::vec2(14.0, 14.0), egui::Sense::hover());
    ui.painter().rect_filled(
        r,
        2.0,
        Color32::from_rgb((c >> 16) as u8, (c >> 8) as u8, c as u8),
    );
}

fn thread_label(t: &Value) -> String {
    [&t["code"], &t["brand"], &t["description"]]
        .iter()
        .filter_map(|v| v.as_str())
        .collect::<Vec<_>>()
        .join(" ")
}

/// The `threads.remap` parameters the window's choices give.
pub fn match_params(st: &MatchState) -> Value {
    let mut p = json!({ "metric": st.metric, "keepDistinct": st.keep_distinct });
    match &st.target {
        Target::Builtin(id) => p["builtin"] = json!(id),
        Target::User(name) => p["chart"] = json!(name),
    }
    p
}

/// The dry-run plan for the current choices, asked again only when they or the design change.
pub fn match_plan(app: &mut EditorApp) -> Option<Value> {
    let mut p = match_params(&app.colour_match);
    p["dryRun"] = json!(true);
    let key = json!([p, app.session.design().objects.len(), app.revision]);
    if let Some((k, v)) = &app.colour_match.plan
        && *k == key
    {
        return Some(v.clone());
    }
    let v = app.session.execute("threads.remap", Some(&p)).ok()?;
    app.colour_match.plan = Some((key, v.clone()));
    Some(v)
}

pub fn merge_plan(app: &mut EditorApp) -> Option<Value> {
    let p = json!({ "maxDeltaE": app.colour_match.max_delta_e, "dryRun": true });
    let key = json!([p, app.revision]);
    if let Some((k, v)) = &app.colour_match.merge_plan
        && *k == key
    {
        return Some(v.clone());
    }
    let v = app.session.execute("threads.mergeSimilar", Some(&p)).ok()?;
    app.colour_match.merge_plan = Some((key, v.clone()));
    Some(v)
}

/// Applies the current match as one undo step.
pub fn apply_match(app: &mut EditorApp) -> bool {
    let p = match_params(&app.colour_match);
    let done = app
        .execute(&CommandRequest::new("threads.remap", p))
        .is_some();
    app.colour_match.applied |= done;
    done
}

pub fn apply_merge(app: &mut EditorApp) -> bool {
    let p = json!({ "maxDeltaE": app.colour_match.max_delta_e });
    let done = app
        .execute(&CommandRequest::new("threads.mergeSimilar", p))
        .is_some();
    app.colour_match.applied |= done;
    done
}

/// Undoes the last Apply made from these windows.
pub fn restore(app: &mut EditorApp) {
    if std::mem::take(&mut app.colour_match.applied) {
        app.undo();
    }
}

pub fn windows(app: &mut EditorApp, ctx: &egui::Context) {
    match_window(app, ctx);
    merge_window(app, ctx);
}

fn match_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.colour_match.match_open;
    if !open {
        return;
    }
    let builtins: Vec<(String, String)> = app
        .session
        .execute("threads.builtinCharts", None)
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|c| {
            (
                c["id"].as_str().unwrap_or("").to_string(),
                c["name"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect();
    let user: Vec<String> = app
        .session
        .user_settings()
        .thread_charts
        .iter()
        .map(|c| c.name.clone())
        .collect();
    let plan = match_plan(app);
    let (mut apply, mut undo) = (false, false);
    egui::Window::new("Match to Thread Chart")
        .open(&mut open)
        .default_width(460.0)
        .show(ctx, |ui| {
            let st = &mut app.colour_match;
            let shown = match &st.target {
                Target::Builtin(id) => builtins
                    .iter()
                    .find(|b| &b.0 == id)
                    .map_or(id.clone(), |b| b.1.clone()),
                Target::User(n) => n.clone(),
            };
            egui::ComboBox::from_label("Chart")
                .selected_text(shown)
                .show_ui(ui, |ui| {
                    for (id, name) in &builtins {
                        ui.selectable_value(&mut st.target, Target::Builtin(id.clone()), name);
                    }
                    for n in &user {
                        ui.selectable_value(&mut st.target, Target::User(n.clone()), n);
                    }
                });
            ui.horizontal(|ui| {
                ui.label("Metric");
                ui.selectable_value(&mut st.metric, "rgb", "PE-DESIGN (RGB)")
                    .on_hover_text("PE-DESIGN's rule: the nearest colour by RGB distance");
                ui.selectable_value(&mut st.metric, "ciede2000", "Perceptual (CIEDE2000)")
                    .on_hover_text("The thread that looks closest");
                ui.selectable_value(&mut st.metric, "oklab", "Perceptual (Oklab)");
            });
            ui.checkbox(&mut st.keep_distinct, "Keep distinct colours apart")
                .on_hover_text(
                    "Two design colours more than ΔE 10 apart never share a thread unless the chart has none left",
                );
            ui.separator();
            match &plan {
                None => {
                    ui.label(egui::RichText::new("No plan: the chart is empty or missing.").weak());
                }
                Some(v) => {
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        egui::Grid::new("match-plan").striped(true).show(ui, |ui| {
                            for h in ["Design colour", "", "Thread", "ΔE", "Objects"] {
                                ui.strong(h);
                            }
                            ui.end_row();
                            for row in v["plan"].as_array().into_iter().flatten() {
                                ui.horizontal(|ui| {
                                    swatch(ui, row["from"]["color"].as_str().unwrap_or(""));
                                    ui.label(row["from"]["color"].as_str().unwrap_or(""));
                                });
                                ui.label("→");
                                ui.horizontal(|ui| {
                                    swatch(ui, row["to"]["color"].as_str().unwrap_or(""));
                                    ui.label(thread_label(&row["to"]));
                                });
                                ui.label(format!("{:.1}", row["deltaE"].as_f64().unwrap_or(0.0)));
                                ui.label(row["objects"].to_string());
                                ui.end_row();
                            }
                        });
                    });
                    ui.label(format!(
                        "Mean ΔE {:.2}",
                        v["meanDeltaE"].as_f64().unwrap_or(0.0)
                    ));
                    for m in v["merged"].as_array().into_iter().flatten() {
                        ui.label(
                            egui::RichText::new(format!(
                                "{} share {}",
                                m["from"]
                                    .as_array()
                                    .into_iter()
                                    .flatten()
                                    .filter_map(Value::as_str)
                                    .collect::<Vec<_>>()
                                    .join(", "),
                                thread_label(&m["to"])
                            ))
                            .weak(),
                        );
                    }
                }
            }
            ui.horizontal(|ui| {
                apply = ui
                    .add_enabled(plan.is_some(), egui::Button::new("Apply"))
                    .clicked();
                undo = ui
                    .add_enabled(st.applied, egui::Button::new("Restore original colours"))
                    .clicked();
            });
        });
    if apply && apply_match(app) {
        app.status = "Threads matched to the chart".into();
    }
    if undo {
        restore(app);
    }
    if !open {
        app.colour_match.applied = false;
    }
    app.colour_match.match_open = open;
}

fn merge_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.colour_match.merge_open;
    if !open {
        return;
    }
    let plan = merge_plan(app);
    let (mut apply, mut undo) = (false, false);
    egui::Window::new("Merge Similar Colours")
        .open(&mut open)
        .default_width(380.0)
        .show(ctx, |ui| {
            let st = &mut app.colour_match;
            ui.add(egui::Slider::new(&mut st.max_delta_e, 0.5..=20.0).text("Max ΔE"));
            if let Some(v) = &plan {
                ui.label(format!(
                    "{} colours → {}; colour changes {} → {} ({} after Sort by Colour)",
                    v["colors"]["before"],
                    v["colors"]["after"],
                    v["colorChanges"]["before"],
                    v["colorChanges"]["after"],
                    v["colorChanges"]["afterSortByColor"],
                ));
                egui::ScrollArea::vertical()
                    .max_height(220.0)
                    .show(ui, |ui| {
                        for g in v["groups"].as_array().into_iter().flatten() {
                            ui.horizontal_wrapped(|ui| {
                                for m in g["merge"].as_array().into_iter().flatten() {
                                    swatch(ui, m["thread"]["color"].as_str().unwrap_or(""));
                                }
                                ui.label("→");
                                swatch(ui, g["keep"]["color"].as_str().unwrap_or(""));
                                ui.label(g["keep"]["color"].as_str().unwrap_or(""));
                            });
                        }
                    });
            }
            ui.horizontal(|ui| {
                apply = ui
                    .add_enabled(
                        plan.as_ref()
                            .is_some_and(|v| !v["groups"].as_array().is_none_or(Vec::is_empty)),
                        egui::Button::new("Apply"),
                    )
                    .clicked();
                undo = ui
                    .add_enabled(st.applied, egui::Button::new("Restore original colours"))
                    .clicked();
            });
        });
    if apply && apply_merge(app) {
        app.status = "Similar colours merged".into();
    }
    if undo {
        restore(app);
    }
    if !open {
        app.colour_match.applied = false;
    }
    app.colour_match.merge_open = open;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    fn render(app: &mut EditorApp) {
        let ctx = egui::Context::default();
        let mut out = ctx.run_ui(egui::RawInput::default(), |ui| windows(app, ui.ctx()));
        out.textures_delta.clear();
    }

    #[test]
    fn match_window_previews_applies_and_restores() {
        let mut app = EditorApp::new(Launch::default());
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({"x": 0, "y": 0, "width": 10, "height": 10, "fill": "#000060"}),
        ));
        let before = app.session.design().clone();
        app.colour_match.match_open = true;
        app.colour_match.target = Target::Builtin("isacord40".into());
        app.colour_match.metric = "ciede2000";
        render(&mut app);
        let plan = match_plan(&mut app).expect("plan");
        assert_eq!(plan["plan"].as_array().map(Vec::len), Some(1));
        assert_eq!(app.session.design(), &before, "the preview changes nothing");
        assert!(apply_match(&mut app));
        let to = plan["plan"][0]["to"]["color"].clone();
        let fill = app.session.design().objects[0]
            .region
            .as_ref()
            .map(|r| r.thread.hex());
        assert_eq!(json!(fill), to);
        render(&mut app);
        restore(&mut app);
        assert_eq!(app.session.design(), &before);
    }

    #[test]
    fn merge_window_previews_and_applies() {
        let mut app = EditorApp::new(Launch::default());
        for (x, c) in [(0, "#2050c0"), (20, "#2252c2"), (40, "#e01010")] {
            app.execute(&CommandRequest::new(
                "shape.rectangle",
                json!({"x": x, "y": 0, "width": 10, "height": 10, "fill": c}),
            ));
        }
        app.colour_match.merge_open = true;
        render(&mut app);
        let plan = merge_plan(&mut app).expect("plan");
        assert_eq!(plan["colors"], json!({"before": 3, "after": 2}));
        let before = app.session.design().clone();
        assert!(apply_merge(&mut app));
        assert_ne!(app.session.design(), &before);
        restore(&mut app);
        assert_eq!(app.session.design(), &before);
    }
}
