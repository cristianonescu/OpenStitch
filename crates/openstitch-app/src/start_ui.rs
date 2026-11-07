//! Getting started (#554, beyond PE-DESIGN): start cards over an empty design, File ▸ Open
//! Sample and Help ▸ Getting Started. The model (cards, samples, steps) is
//! `openstitch_app::editor::getting_started`.

use crate::app::EditorApp;
use eframe::egui::{self, RichText, vec2};
use openstitch_app::editor::close_prompt::Pending;
use openstitch_app::editor::getting_started::{self as gs, CARDS, Card, SAMPLES, STEPS, Start};
use serde_json::{Value, json};

/// The windows' state.
#[derive(Default)]
pub struct StartState {
    pub samples_open: bool,
    pub page_open: bool,
    page_cache: egui_commonmark::CommonMarkCache,
}

/// Runs what a card or a step starts.
pub fn start(app: &mut EditorApp, s: Start, ctx: &egui::Context) {
    match s {
        Start::Run(a) => crate::ribbon::run(app, a, ctx),
        Start::Text => app.lettering.text.open = true,
        Start::Template => crate::lettering_ui::open_template_wizard(app),
        Start::Sample => app.ui.start.samples_open = true,
    }
}

/// Opens sample `key` from the kit as a new untitled design (asking first about unsaved
/// changes, as Open does).
pub fn open_sample(app: &mut EditorApp, key: &str) -> Result<(), String> {
    let exe = std::env::current_exe().ok();
    let kit =
        gs::kit(exe.as_deref()).ok_or("the ready-to-sew kit is not installed with this copy")?;
    let path = gs::sample_path(&kit, key).ok_or_else(|| format!("no sample '{key}'"))?;
    app.guarded(Pending::Open(Some(path)));
    Ok(())
}

/// The cards over the canvas while the design is empty (Options ▸ Show start cards).
pub fn cards(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(canvas) = app.workspace.canvas else {
        return;
    };
    // The Startup Wizard replaces the cards while it is open (#657).
    if !gs::cards_shown(app.ui.prefs.start_cards, app.session.design().objects.len())
        || app.tools.is_busy()
        || app.startup_wizard.open
    {
        return;
    }
    let mut picked: Option<Start> = None;
    egui::Area::new(egui::Id::unique("start-cards"))
        .order(egui::Order::Middle)
        .pivot(egui::Align2::CENTER_CENTER)
        .fixed_pos(canvas.center())
        .show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.label(RichText::new("Start your design").heading());
                ui.add_space(4.0);
                egui::Grid::new("start-cards")
                    .spacing(vec2(8.0, 8.0))
                    .show(ui, |ui| {
                        for (i, c) in CARDS.iter().enumerate() {
                            if card(ui, c).clicked() {
                                picked = Some(c.start);
                            }
                            if i % 3 == 2 {
                                ui.end_row();
                            }
                        }
                    });
                ui.add_space(2.0);
                ui.label(
                    RichText::new("Getting Started in the Help menu walks through a first design")
                        .weak()
                        .small(),
                );
            });
        });
    if let Some(s) = picked {
        start(app, s, ctx);
    }
}

fn card(ui: &mut egui::Ui, c: &Card) -> egui::Response {
    let size = vec2(170.0, 96.0);
    let (rect, resp) = ui.allocate_exact_size(size, egui::Sense::click());
    let v = ui.style().interact(&resp);
    let painter = ui.painter_at(rect);
    painter.rect(
        rect,
        egui::CornerRadius::same(6),
        v.weak_bg_fill,
        v.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let icon = egui::Rect::from_min_size(rect.min + vec2(10.0, 10.0), vec2(32.0, 32.0));
    crate::icons::paint(&painter, icon, c.icon, v.text_color(), true);
    painter.text(
        rect.min + vec2(50.0, 18.0),
        egui::Align2::LEFT_CENTER,
        c.title,
        egui::FontId::proportional(14.0),
        v.text_color(),
    );
    let galley = ui.painter().layout(
        c.text.to_string(),
        egui::FontId::proportional(11.5),
        ui.visuals().weak_text_color(),
        size.x - 20.0,
    );
    painter.galley(rect.min + vec2(10.0, 50.0), galley, v.text_color());
    resp.on_hover_text(c.text)
}

/// File ▸ Open Sample and Help ▸ Getting Started.
pub fn windows(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.ui.start.samples_open;
    let mut chosen: Option<&str> = None;
    egui::Window::new("Open Sample")
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .show(ctx, |ui| {
            ui.label("Opens a copy as a new untitled design; the kit itself never changes.");
            ui.add_space(4.0);
            for (key, title, file) in SAMPLES {
                if ui.button(*title).on_hover_text(*file).clicked() {
                    chosen = Some(key);
                }
            }
        });
    if let Some(key) = chosen {
        open = false;
        if let Err(e) = open_sample(app, key) {
            app.status = e;
        }
    }
    app.ui.start.samples_open = open;

    let mut open = app.ui.start.page_open;
    let mut step: Option<Start> = None;
    egui::Window::new("Getting Started")
        .open(&mut open)
        .default_size([520.0, 560.0])
        .show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                for (label, s) in STEPS {
                    if ui.button(*label).clicked() {
                        step = Some(*s);
                    }
                }
            });
            ui.separator();
            egui::ScrollArea::vertical().show(ui, |ui| {
                egui_commonmark::CommonMarkViewer::new().show(
                    ui,
                    &mut app.ui.start.page_cache,
                    gs::PAGE,
                );
            });
        });
    app.ui.start.page_open = open;
    if let Some(s) = step {
        start(app, s, ctx);
    }
}

/// `ui.gettingStarted`: the cards, samples and steps, or runs one.
pub fn command(
    app: &mut EditorApp,
    params: Option<&Value>,
    ctx: &egui::Context,
) -> Result<Value, String> {
    let get = |k: &str| params.and_then(|p| p.get(k)).and_then(Value::as_str);
    if let Some(key) = get("sample") {
        open_sample(app, key)?;
        return Ok(json!({ "opened": key }));
    }
    if let Some(key) = get("card") {
        let c = gs::card(key).ok_or_else(|| format!("no card '{key}'"))?;
        start(app, c.start, ctx);
        return Ok(json!({ "started": key }));
    }
    if let Some(open) = params.and_then(|p| p.get("page")).and_then(Value::as_bool) {
        app.ui.start.page_open = open;
        return Ok(json!({ "page": open }));
    }
    Ok(json!({
        "cardsShown": gs::cards_shown(app.ui.prefs.start_cards, app.session.design().objects.len()),
        "cards": CARDS.iter().map(|c| json!({ "card": c.key, "title": c.title, "text": c.text })).collect::<Vec<_>>(),
        "samples": SAMPLES.iter().map(|s| json!({ "sample": s.0, "title": s.1 })).collect::<Vec<_>>(),
    }))
}
