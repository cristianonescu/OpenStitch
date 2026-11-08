//! Lettering dialogs (Home ▸ Tools ▸ Text). The forms live in `openstitch_app::editor::lettering`; each
//! action is one engine command, so one Undo.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::lettering::{
    self, FontChoice, FontsForm, MonogramForm, NameDropForm, TemplateWizard, TextForm,
};
use openstitch_design::{PathFit, Spread, TextAlign, VAlign};
use serde_json::json;

/// Dialog state of the Lettering menu.
#[derive(Default)]
pub struct LetteringState {
    pub text: TextForm,
    pub fonts: FontsForm,
    pub monogram: MonogramForm,
    pub name_drop: NameDropForm,
    pub templates: TemplateWizard,
    pub fit: FitForm,
}

/// Fit Text to Outline Attribute Setting: the text and outline it applies to, and its options.
pub struct FitForm {
    /// (text, outline) while the dialog is open.
    pub open: Option<(u64, u64)>,
    pub align: TextAlign,
    /// Spaced evenly or stretched instead of `align`.
    pub spread: Spread,
    pub valign: VAlign,
    pub upright: bool,
    pub other_side: bool,
}

impl Default for FitForm {
    /// The dialog's defaults: centred, above the line, along it, this side.
    fn default() -> Self {
        let fit = PathFit::default();
        FitForm {
            open: None,
            align: TextAlign::Center,
            spread: fit.spread,
            valign: fit.valign,
            upright: fit.upright,
            other_side: fit.other_side,
        }
    }
}

/// Home ▸ Edit ▸ Fit Text ▸ Fit Text to Outline…: the dialog shows a fitted text's settings,
/// else the last choices (as PE-DESIGN's dialog remembers them).
pub fn open_fit(app: &mut EditorApp, text: u64, outline: u64) {
    let d = app.session.design();
    let Some(t) = d.index_of(text).and_then(|i| d.objects[i].text.as_ref()) else {
        return;
    };
    let (path, align, fit) = (t.path, t.align, t.fit);
    let f = &mut app.lettering.fit;
    if path.is_some() {
        (f.align, f.spread, f.valign) = (align, fit.spread, fit.valign);
        (f.upright, f.other_side) = (fit.upright, fit.other_side);
    }
    f.open = Some((text, outline));
}

/// The request the dialog's OK sends.
pub fn fit_request(f: &FitForm, text: u64, outline: u64) -> CommandRequest {
    let align = match (f.spread, f.align) {
        (Spread::Even, _) => "spaced",
        (Spread::Fill, _) => "stretch",
        (Spread::None, TextAlign::Left) => "left",
        (Spread::None, TextAlign::Center) => "center",
        (Spread::None, TextAlign::Right) => "right",
    };
    let valign = match f.valign {
        VAlign::Bottom => "bottom",
        VAlign::Middle => "middle",
        VAlign::Top => "top",
    };
    CommandRequest::new(
        "text.setPath",
        json!({
            "id": text, "path": outline, "align": align, "valign": valign,
            "upright": f.upright, "otherSide": f.other_side,
        }),
    )
}

fn fit_window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some((text, outline)) = app.lettering.fit.open else {
        return;
    };
    let mut open = true;
    let (mut ok, mut cancel) = (false, false);
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-fit-text-to-outline-attribute-setting")).id(egui::Id::unique("window-fit-text-to-outline-attribute-setting"))
        .open(&mut open)
        .resizable(false)
        .collapsible(false)
        .show(ctx, |ui| {
            let f = &mut app.lettering.fit;
            egui::Grid::new("fit-text").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-horizontal-alignment"));
                ui.horizontal(|ui| {
                    let mut h = (f.spread, f.align);
                    for (v, label, tip) in [
                        (
                            (Spread::None, TextAlign::Left),
                            "Left",
                            "From the outline's start",
                        ),
                        (
                            (Spread::None, TextAlign::Center),
                            "Center",
                            "Centred on the outline",
                        ),
                        (
                            (Spread::None, TextAlign::Right),
                            "Right",
                            "Ending at the outline's end",
                        ),
                        (
                            (Spread::Fill, f.align),
                            "Stretch",
                            "Letters scaled to fill the outline",
                        ),
                        (
                            (Spread::Even, f.align),
                            "Spaced",
                            "Spaced evenly round the outline",
                        ),
                    ] {
                        let on = h.0 == v.0 && (v.0 != Spread::None || h.1 == v.1);
                        if ui.radio(on, label).on_hover_text(tip).clicked() {
                            h = v;
                        }
                    }
                    (f.spread, f.align) = h;
                });
                ui.end_row();
                ui.label(t("ui-vertical-alignment"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.valign, VAlign::Bottom, t("ui-above"))
                        .on_hover_text(t("ui-the-text-stands-on-the-outline-tip"));
                    ui.radio_value(&mut f.valign, VAlign::Middle, t("ui-centered"))
                        .on_hover_text(t("ui-the-text-is-centred-on-the-tip"));
                    ui.radio_value(&mut f.valign, VAlign::Top, t("ui-below"))
                        .on_hover_text(t("ui-the-text-hangs-below-the-outline-tip"));
                });
                ui.end_row();
                ui.label(t("ui-text-orientation"));
                ui.horizontal(|ui| {
                    ui.radio_value(&mut f.upright, false, t("ui-along-the-outline"));
                    ui.radio_value(&mut f.upright, true, t("ui-upright"));
                });
                ui.end_row();
                ui.label("");
                ui.checkbox(&mut f.other_side, t("ui-the-other-side"))
                    .on_hover_text(t("ui-run-on-the-other-side-of-tip"));
                ui.end_row();
            });
            ui.separator();
            ui.horizontal(|ui| {
                ok = ui.button(t("ui-ok")).clicked();
                cancel = ui.button(t("ui-cancel")).clicked();
            });
        });
    if ok {
        let r = fit_request(&app.lettering.fit, text, outline);
        app.execute(&r);
    }
    if ok || cancel || !open {
        app.lettering.fit.open = None;
    }
}

/// The fonts `fonts.list` reports for the open design.
pub fn fonts(app: &mut EditorApp) -> Vec<FontChoice> {
    let all = app
        .session
        .execute("fonts.list", Some(&json!({})))
        .map(|v| lettering::font_choices(&v))
        .unwrap_or_default();
    // Text ▸ Font Filter.
    app.ui.prefs.font_filter.apply(all)
}

/// Home ▸ Tools ▸ Text: text, monogram, name drop, templates and fonts.
pub fn text_tools_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    if ui.button(t("ui-text-2")).clicked() {
        app.lettering.text.open = true;
    }
    if ui.button(t("ui-monogram")).clicked() {
        app.lettering.monogram.open = true;
    }
    if ui.button(t("ui-name-drop-list")).clicked() {
        app.lettering.name_drop.open = true;
    }
    if ui.button(t("menu-template-wizard")).clicked() {
        open_template_wizard(app);
    }
    if ui.button(t("ui-save-as-template")).clicked() {
        save_template(app);
    }
    ui.separator();
    if ui.button(t("ui-fonts")).clicked() {
        app.lettering.fonts.open = true;
    }
    if ui.button(t("ui-load-truetype-font")).clicked() {
        load_truetype(app);
    }
    crate::inkstitch_ui::menu(app, ui);
}

/// Loads a TrueType / OpenType font into the design.
pub fn load_truetype(_app: &mut EditorApp) {
    crate::files::pick_then(
        crate::files::FileDialog::new()
            .add_filter("TrueType / OpenType", &["ttf", "otf", "ttc", "otc"]),
        |app, p| load_truetype_from(app, &p),
    );
}

/// Loads the TrueType / OpenType font at `p` into the design.
fn load_truetype_from(app: &mut EditorApp, p: &std::path::Path) {
    if let Some(r) = app.execute(&lettering::load_font_request(p)) {
        app.status = openstitch_app::tf!("ui-status-loaded-font-glyphs-it-is-saved", id = r["id"].as_str().unwrap_or_default(), glyphs = r["glyphs"]);
    }
}

fn font_combo(
    ui: &mut egui::Ui,
    salt: &str,
    name: &str,
    value: &mut String,
    choices: &[FontChoice],
) {
    let label = choices
        .iter()
        .find(|c| &c.id == value)
        .map_or(value.clone(), |c| c.label.clone());
    let r = egui::ComboBox::from_id_salt(salt)
        .selected_text(label)
        .show_ui(ui, |ui| {
            for c in choices {
                ui.selectable_value(value, c.id.clone(), &c.label);
            }
        });
    crate::a11y::name(&r.response, name);
}

pub fn windows(app: &mut EditorApp, ctx: &egui::Context) {
    text_window(app, ctx);
    monogram_window(app, ctx);
    name_drop_window(app, ctx);
    template_window(app, ctx);
    fonts_window(app, ctx);
    fit_window(app, ctx);
    crate::inkstitch_ui::window(app, ctx);
}

/// What the Fonts window asked for this frame.
enum FontAction {
    Create,
    Convert,
    EditGlyph,
    SetGlyph,
    ChooseImage,
    TraceImage,
    FromImage,
    MapSelection(u64),
    MapFiles,
    Save,
    Open,
    Remove(String),
}

fn fonts_window(app: &mut EditorApp, ctx: &egui::Context) {
    let units = app.ui.prefs.units;
    if !app.lettering.fonts.open {
        return;
    }
    let rows = app
        .session
        .execute("fonts.list", Some(&json!({})))
        .map(|v| lettering::design_fonts(&v))
        .unwrap_or_default();
    if !rows
        .iter()
        .any(|r| r.embroidery && r.id == app.lettering.fonts.font)
        && let Some(r) = rows.iter().find(|r| r.embroidery)
    {
        app.lettering.fonts.font = r.id.clone();
    }
    let single = (app.selection.len() == 1)
        .then(|| app.selection.iter().next().copied())
        .flatten();
    let any_selected = !app.selection.is_empty();
    let has_background = app.session.design().background.is_some();
    let truetype: Vec<FontChoice> = {
        let embroidery: Vec<&str> = rows
            .iter()
            .filter(|r| r.embroidery)
            .map(|r| r.id.as_str())
            .collect();
        fonts(app)
            .into_iter()
            .filter(|c| !embroidery.contains(&c.id.as_str()))
            .collect()
    };
    let mut open = true;
    let mut action = None;
    let shown = app
        .dialog_window(ctx, "Fonts", None)
        .open(&mut open)
        .default_width(380.0)
        .show(ctx, |ui| {
            let f = &mut app.lettering.fonts;
            ui.label(egui::RichText::new(t("ui-fonts-in-this-design")).strong());
            if rows.is_empty() {
                ui.weak("None yet. Load a TrueType font or create an embroidery font.");
            }
            for r in &rows {
                ui.horizontal(|ui| {
                    if r.embroidery {
                        ui.radio_value(&mut f.font, r.id.clone(), &r.id);
                    } else {
                        ui.label(&r.id);
                    }
                    ui.weak(&r.label);
                    if ui.small_button(t("ui-remove")).clicked() {
                        action = Some(FontAction::Remove(r.id.clone()));
                    }
                });
            }
            if ui.button(t("ui-open-openstitch-font-osfont")).clicked() {
                action = Some(FontAction::Open);
            }
            ui.separator();
            ui.label(egui::RichText::new(t("ui-new-embroidery-font")).strong());
            ui.horizontal(|ui| {
                ui.label(t("ui-id"));
                crate::a11y::name(&ui.text_edit_singleline(&mut f.new_id), "Id");
            });
            ui.horizontal(|ui| {
                ui.label(t("ui-standard-character-height"));
                ui.add(
                    crate::units_ui::drag(&mut f.new_height, units)
                        .range(1.0..=500.0)
                        ,
                );
            });
            if ui.button(t("ui-create")).clicked() {
                action = Some(FontAction::Create);
            }
            ui.separator();
            let editing = rows.iter().any(|r| r.embroidery && r.id == f.font);
            ui.add_enabled_ui(editing, |ui| {
                ui.label(egui::RichText::new(t("ui-user-mapped-characters")).strong());
                ui.horizontal(|ui| {
                    ui.label(t("ui-character"));
                    let r = ui.add(egui::TextEdit::singleline(&mut f.character).desired_width(40.0));
                    crate::a11y::name(&r, "Character");
                    if ui
                        .add_enabled(single.is_some(), egui::Button::new(t("ui-register-selected-object")))
                        .on_hover_text(t("ui-the-selected-object-s-stitches-become-tip"))
                        .clicked()
                        && let Some(id) = single
                    {
                        action = Some(FontAction::MapSelection(id));
                    }
                });
                if ui
                    .button(t("ui-register-stitch-files"))
                    .on_hover_text(t("ui-file-names-give-the-characters-u-tip"))
                    .clicked()
                {
                    action = Some(FontAction::MapFiles);
                }
                ui.separator();
                ui.label(egui::RichText::new(t("ui-font-creator")).strong());
                ui.horizontal(|ui| {
                    ui.label(t("ui-from-truetype"));
                    font_combo(ui, "convert-from", "From TrueType", &mut f.convert_from, &truetype);
                });
                ui.add(egui::TextEdit::singleline(&mut f.convert_chars).hint_text("characters"));
                if ui.button(t("ui-convert-characters")).clicked() {
                    action = Some(FontAction::Convert);
                }
                ui.horizontal(|ui| {
                    ui.label(t("ui-draw-at-set-line"));
                    ui.add(crate::units_ui::drag(&mut f.glyph_x, units));
                    ui.label(t("ui-baseline"));
                    ui.add(crate::units_ui::drag(&mut f.glyph_y, units));
                });
                ui.horizontal(|ui| {
                    if ui
                        .button(t("ui-edit-character-as-shape"))
                        .on_hover_text(t("ui-puts-the-character-s-outline-on-tip"))
                        .clicked()
                    {
                        action = Some(FontAction::EditGlyph);
                    }
                    if ui
                        .add_enabled(any_selected, egui::Button::new(t("ui-store-selection-as-character")))
                        .on_hover_text(t("ui-the-selected-outlines-measured-from-the-tip"))
                        .clicked()
                    {
                        action = Some(FontAction::SetGlyph);
                    }
                });
                ui.separator();
                image_characters(ui, f, has_background, &mut action);
                ui.separator();
                if ui.button(t("ui-save-font-as")).clicked() {
                    action = Some(FontAction::Save);
                }
            });
        });
    if let Some(shown) = shown {
        app.dialog_rect_next = Some(shown.response.rect);
    }
    app.lettering.fonts.open = open;
    let f = app.lettering.fonts.clone();
    let ext = openstitch_engine::fonts::FONT_EXTENSION;
    match action {
        Some(FontAction::Create) => {
            if let Some(r) = app.execute(&f.create()) {
                app.lettering.fonts.font = r["id"].as_str().unwrap_or_default().into();
                app.status = openstitch_app::editor::ui_texts::t("ui-status-created-an-empty-embroidery-font").into();
            }
        }
        Some(FontAction::Convert) => {
            if let Some(r) = app.execute(&f.convert()) {
                app.status = openstitch_app::tf!("ui-status-converted-character-n-other-characters-n", converted = r["converted"].as_str().map_or(0, |c| c.chars().count()), missing = match r["missing"].as_str() {
                        Some(m) if !m.is_empty() => format!("; the font has none of {m}"),
                        _ => String::new(),
                    });
            }
        }
        Some(FontAction::EditGlyph) => {
            if let Some(r) = app.execute(&f.edit_glyph()) {
                app.selection = CommandRequest::created_ids(&r);
                app.status = openstitch_app::tf!("ui-status-editing-baseline-mm-capital-line-mm", character = f.character, baseline = r["baseline"], capline = r["capLine"], left = r["left"], right = r["right"]);
            }
        }
        Some(FontAction::SetGlyph) => {
            let ids = app.selection.clone();
            if let Some(r) = app.execute(&f.set_glyph(&ids)) {
                app.status = openstitch_app::tf!("ui-status-stored-mm-wide", character = f.character, advancemm = r["advanceMM"]);
            }
        }
        Some(FontAction::ChooseImage) => crate::files::pick_then(
            crate::files::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "bmp"]),
            |app, p| {
                app.lettering.fonts.image.path = Some(p);
                app.lettering.fonts.image.found.clear();
            },
        ),
        Some(FontAction::TraceImage) => {
            if let Some(r) = app.execute(&f.trace_image()) {
                app.lettering.fonts.set_found(&r);
                app.status = openstitch_app::tf!("ui-status-found-character-n-other-characters-n", count = app.lettering.fonts.image.found.len(), rows = r["rows"]);
            }
        }
        Some(FontAction::FromImage) => {
            if let Some(r) = f.from_image().and_then(|req| app.execute(&req)) {
                app.status = openstitch_app::tf!("ui-status-created-from-the-image-skipped", created = r["created"].as_str().unwrap_or_default(), skipped = r["skipped"]);
                app.lettering.fonts.image.found.clear();
            }
        }
        Some(FontAction::MapSelection(id)) => {
            if let Some(r) = app.execute(&f.map_object(id)) {
                app.status = openstitch_app::tf!("ui-status-registered-stitches", character = f.character, stitches = r["stitches"]);
            }
        }
        Some(FontAction::MapFiles) => {
            let exts: Vec<&str> = openstitch_formats::FileType::ALL
                .iter()
                .map(|t| t.extension())
                .collect();
            crate::files::pick_many_then(
                crate::files::FileDialog::new().add_filter("Embroidery files", &exts),
                move |app, paths| {
                    if let Some(r) = app.execute(&f.map_files(&paths)) {
                        app.status = lettering::map_summary(&r);
                    }
                },
            );
        }
        Some(FontAction::Save) => {
            if let Some(p) = crate::files::FileDialog::new()
                .add_filter("OpenStitch font", &[ext])
                .set_file_name(format!("{}.{ext}", f.font))
                .save_file()
                && app.execute(&f.save(&p)).is_some()
            {
                app.status = openstitch_app::tf!("ui-status-saved", path = p.display());
            }
        }
        Some(FontAction::Open) => crate::files::pick_then(
            crate::files::FileDialog::new().add_filter("OpenStitch font", &[ext]),
            |app, p| {
                if let Some(r) = app.execute(&FontsForm::open_file(&p)) {
                    app.lettering.fonts.font = r["id"].as_str().unwrap_or_default().into();
                }
            },
        ),
        Some(FontAction::Remove(id)) => {
            app.execute(&FontsForm::remove(&id));
        }
        None => {}
    }
}

/// Font Creator ▸ Characters from image: read the image, check and assign the characters
/// found, then create them in one step.
fn image_characters(
    ui: &mut egui::Ui,
    f: &mut FontsForm,
    has_background: bool,
    action: &mut Option<FontAction>,
) {
    ui.label(egui::RichText::new(t("ui-characters-from-image")).strong());
    let i = &mut f.image;
    ui.horizontal(|ui| {
        let name = i.path.as_ref().map_or_else(
            || {
                if has_background {
                    "Background image".to_string()
                } else {
                    "No image".to_string()
                }
            },
            |p| {
                p.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            },
        );
        ui.label(name);
        if ui.button(t("ui-choose-image")).clicked() {
            *action = Some(FontAction::ChooseImage);
        }
        if i.path.is_some()
            && has_background
            && ui
                .button(t("ui-use-background"))
                .on_hover_text(t("ui-read-the-background-image-as-it-tip"))
                .clicked()
        {
            i.path = None;
            i.found.clear();
        }
    });
    ui.horizontal(|ui| {
        if ui
            .radio_value(&mut i.whole, false, t("ui-split-into-characters"))
            .changed()
            | ui.radio_value(&mut i.whole, true, t("ui-whole-image-is-the-character"))
                .changed()
        {
            i.found.clear();
        }
    });
    ui.horizontal(|ui| {
        ui.label(t("ui-ink-darker-than"));
        let r = ui.add(egui::DragValue::new(&mut i.threshold).range(1..=254));
        crate::a11y::name(&r, "Ink darker than");
        if i.whole {
            ui.add_enabled(
                i.path.is_none() && has_background,
                egui::Checkbox::new(&mut i.as_placed, "As placed, from the set line and baseline"),
            )
            .on_hover_text(t("ui-use-the-background-image-where-it-tip"));
        }
    });
    if !i.whole {
        ui.horizontal(|ui| {
            ui.label(t("ui-assign-in-order"));
            crate::a11y::name(&ui.text_edit_singleline(&mut i.order), "Assign in order");
        });
    }
    let ready = i.path.is_some() || has_background;
    if ui
        .add_enabled(ready, egui::Button::new(t("ui-find-characters")))
        .clicked()
    {
        *action = Some(FontAction::TraceImage);
    }
    if i.found.is_empty() {
        return;
    }
    ui.weak("Type the character each outline becomes; leave a box empty to skip it.");
    egui::ScrollArea::vertical()
        .max_height(220.0)
        .show(ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                for c in &mut i.found {
                    ui.vertical(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(egui::vec2(44.0, 44.0), egui::Sense::hover());
                        draw_outline(ui, rect, &c.outline);
                        ui.add(egui::TextEdit::singleline(&mut c.assigned).desired_width(44.0))
                            .on_hover_text(format!("Row {}", c.row + 1));
                    });
                }
            });
        });
    if ui.button(t("ui-create-characters")).clicked() {
        *action = Some(FontAction::FromImage);
    }
}

/// The first line of `text` (at most 24 characters) laid out in `font` at 10 mm, as polylines
/// (0.1 mm), for a font list sample; `None` when the font cannot set it.
fn sample_lines(
    text: &str,
    font: &str,
    fonts: &[openstitch_design::DesignFont],
) -> Option<Vec<Vec<[f64; 2]>>> {
    let line: String = text
        .lines()
        .next()
        .unwrap_or_default()
        .chars()
        .take(24)
        .collect();
    let line = if line.trim().is_empty() {
        "Abc".to_string()
    } else {
        line
    };
    let spec = openstitch_design::TextSpec::new(line, font, 10.0);
    let layout = openstitch_design::text::layout_in(&spec, None, fonts).ok()?;
    let mut out: Vec<Vec<[f64; 2]>> = layout
        .glyphs
        .iter()
        .flat_map(|g| &g.contours)
        .map(|c| {
            let mut pts: Vec<[f64; 2]> = c.path().iter().map(|p| [p.x, p.y]).collect();
            if c.closed
                && let Some(&f) = pts.first()
            {
                pts.push(f);
            }
            pts
        })
        .collect();
    out.extend(
        layout
            .glyphs
            .iter()
            .filter_map(|g| g.stitches.as_ref())
            .map(|b| b.stitches.iter().map(|s| [s.x, s.y]).collect()),
    );
    (!out.is_empty()).then_some(out)
}

/// Draws `lines` (0.1 mm) fitted into `rect`, left aligned.
fn draw_sample(ui: &egui::Ui, rect: egui::Rect, lines: &[Vec<[f64; 2]>], color: egui::Color32) {
    let painter = ui.painter_at(rect);
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in lines.iter().flatten() {
        (x0, y0, x1, y1) = (x0.min(q[0]), y0.min(q[1]), x1.max(q[0]), y1.max(q[1]));
    }
    if x0 > x1 {
        return;
    }
    let scale = (f64::from(rect.height() - 4.0) / (y1 - y0).max(1e-6))
        .min(f64::from(rect.width() - 4.0) / (x1 - x0).max(1e-6));
    let stroke = egui::Stroke::new(1.0, color);
    for line in lines {
        let points: Vec<egui::Pos2> = line
            .iter()
            .map(|q| {
                rect.min
                    + egui::vec2(
                        2.0 + ((q[0] - x0) * scale) as f32,
                        2.0 + ((q[1] - y0) * scale) as f32,
                    )
            })
            .collect();
        painter.add(egui::Shape::line(points, stroke));
    }
}

/// Text Attributes ▸ Font list (#527): every font with the selected text set in it, grouped as
/// PE-DESIGN's Font List (Text, Small Text), with the sizes each sews well at. A click picks a
/// font; with the pointer over the list, Up and Down step through it with the canvas updating
/// live, and the whole browse is one Undo.
pub fn font_browser(app: &mut EditorApp, ui: &mut egui::Ui, spec: &openstitch_design::TextSpec) {
    let choices = fonts(app);
    let design_fonts = app.session.design().fonts.clone();
    let current = choices.iter().position(|c| c.id == spec.font);
    let mut pick: Option<usize> = None;
    let list = egui::Frame::group(ui.style()).show(ui, |ui| {
        egui::ScrollArea::vertical()
            .id_salt("font-browser-scroll")
            .max_height(220.0)
            .show(ui, |ui| {
                for small in [false, true] {
                    let members: Vec<usize> = (0..choices.len())
                        .filter(|&i| (choices[i].group == "smallText") == small)
                        .collect();
                    if members.is_empty() {
                        continue;
                    }
                    let title = if small { "Small Text" } else { "Text" };
                    ui.label(egui::RichText::new(title).small().strong());
                    for i in members {
                        let c = &choices[i];
                        let range = c
                            .size_range
                            .map(|(lo, hi)| format!("   {lo}-{hi} mm"))
                            .unwrap_or_default();
                        let r = ui.add_sized(
                            [ui.available_width(), 44.0],
                            egui::Button::selectable(Some(i) == current, ""),
                        );
                        let rect = r.rect.shrink(3.0);
                        // Readable on the selection colour too.
                        let (caption, ink) = if Some(i) == current {
                            let c = ui.visuals().selection.stroke.color;
                            (c, c)
                        } else {
                            (ui.visuals().weak_text_color(), ui.visuals().text_color())
                        };
                        ui.painter().text(
                            rect.left_top(),
                            egui::Align2::LEFT_TOP,
                            format!("{}{range}", c.label),
                            egui::FontId::proportional(11.0),
                            caption,
                        );
                        let body = egui::Rect::from_min_max(
                            egui::pos2(rect.min.x, rect.min.y + 14.0),
                            rect.max,
                        );
                        if ui.is_rect_visible(body)
                            && let Some(lines) = sample_lines(&spec.text, &c.id, &design_fonts)
                        {
                            draw_sample(ui, body, &lines, ink);
                        }
                        if r.clicked() {
                            pick = Some(i);
                        }
                    }
                }
            });
    });
    let over = list.response.contains_pointer();
    if over && let Some(cur) = current {
        let (up, down) = ui.input(|i| {
            (
                i.key_pressed(egui::Key::ArrowUp),
                i.key_pressed(egui::Key::ArrowDown),
            )
        });
        if up && cur > 0 {
            pick = Some(cur - 1);
        } else if down && cur + 1 < choices.len() {
            pick = Some(cur + 1);
        }
    }
    if !over {
        app.ui.font_browse_undo = None;
    }
    if let Some(i) = pick.filter(|&i| Some(i) != current) {
        browse_to(app, &choices[i].id);
    }
}

/// Sets the selected text's font as one step of a font browse: every step since the browse
/// began is one Undo (#527).
pub fn browse_to(app: &mut EditorApp, font: &str) {
    let start = *app.ui.font_browse_undo.get_or_insert(app.undo_groups.len());
    crate::ribbon::set_text(app, json!({ "font": font }));
    let start = start.min(app.undo_groups.len());
    let steps: usize = app.undo_groups.drain(start..).sum();
    if steps > 0 {
        app.undo_groups.push(steps);
    }
}

/// Why the selected text's size is shown amber: it lies outside the sizes its font sews well
/// at (#527). `None` inside the range or for fonts without one.
pub fn size_warning(app: &mut EditorApp, spec: &openstitch_design::TextSpec) -> Option<String> {
    let c = fonts(app).into_iter().find(|c| c.id == spec.font)?;
    let (lo, hi) = c.size_range?;
    c.out_of_range(spec.size).then(|| {
        format!(
            "{} sews best at {lo}-{hi} mm: smaller letters lose their detail, larger ones get loose stitches",
            c.label
        )
    })
}

/// A found character's outline, fitted into `rect`.
fn draw_outline(ui: &egui::Ui, rect: egui::Rect, outline: &[Vec<[f64; 2]>]) {
    let painter = ui.painter_at(rect);
    painter.rect_stroke(
        rect,
        2.0,
        ui.visuals().widgets.noninteractive.bg_stroke,
        egui::StrokeKind::Inside,
    );
    let pts = outline.iter().flatten();
    let (mut x0, mut y0, mut x1, mut y1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for q in pts {
        (x0, y0, x1, y1) = (x0.min(q[0]), y0.min(q[1]), x1.max(q[0]), y1.max(q[1]));
    }
    if x0 > x1 {
        return;
    }
    let side = (x1 - x0).max(y1 - y0).max(1e-6);
    let scale = f64::from(rect.width() - 8.0) / side;
    let offset = egui::vec2(
        ((f64::from(rect.width()) - (x1 - x0) * scale) / 2.0) as f32,
        ((f64::from(rect.height()) - (y1 - y0) * scale) / 2.0) as f32,
    );
    let stroke = egui::Stroke::new(1.0, ui.visuals().text_color());
    for line in outline {
        let mut points: Vec<egui::Pos2> = line
            .iter()
            .map(|q| {
                rect.min
                    + offset
                    + egui::vec2(((q[0] - x0) * scale) as f32, ((q[1] - y0) * scale) as f32)
            })
            .collect();
        if let Some(&first) = points.first() {
            points.push(first);
        }
        painter.add(egui::Shape::line(points, stroke));
    }
}

fn text_window(app: &mut EditorApp, ctx: &egui::Context) {
    let units = app.ui.prefs.units;
    if !app.lettering.text.open {
        return;
    }
    let choices = fonts(app);
    let mut open = true;
    let mut add = None;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-text")).id(egui::Id::unique("window-text"))
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let f = &mut app.lettering.text;
            ui.add(egui::TextEdit::multiline(&mut f.text).desired_rows(2));
            egui::Grid::new("text-form").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-font"));
                font_combo(ui, "text-font", "Font", &mut f.font, &choices);
                ui.end_row();
                ui.label(t("ui-size"));
                ui.add(
                    crate::units_ui::drag(&mut f.size, units)
                        .range(1.0..=300.0)
                        ,
                );
                ui.end_row();
                ui.label(t("ui-align"));
                ui.horizontal(|ui| {
                    for a in TextForm::ALIGNS {
                        ui.radio_value(&mut f.align, a, a);
                    }
                });
                ui.end_row();
                ui.label(t("menu-options"));
                ui.horizontal(|ui| {
                    ui.checkbox(&mut f.kerning, t("ui-kerning"))
                        .on_hover_text(t("ui-use-the-truetype-font-s-kerning-tip"));
                    ui.checkbox(&mut f.vertical, t("ui-vertical"))
                        .on_hover_text(t("ui-characters-top-to-bottom-lines-as-tip"));
                });
                ui.end_row();
            });
            ui.weak(
                "CJK, Arabic, Hebrew and other scripts need a TrueType font that has them (Load TrueType Font…); they are shaped and set right to left where needed.",
            );
            if ui.button(t("ui-add")).clicked() {
                add = f.request();
            }
        });
    app.lettering.text.open = open;
    if let Some(r) = add {
        run(app, &r, lettering::text_summary);
    }
}

/// Runs `r`, selects what it created and reports `summary` of the result.
fn run(app: &mut EditorApp, r: &CommandRequest, summary: fn(&serde_json::Value) -> String) {
    if let Some(result) = app.execute(r) {
        app.selection = CommandRequest::created_ids(&result);
        app.status = summary(&result);
    }
}

fn monogram_window(app: &mut EditorApp, ctx: &egui::Context) {
    let units = app.ui.prefs.units;
    if !app.lettering.monogram.open {
        return;
    }
    let choices = fonts(app);
    let frames = app
        .session
        .execute("monogram.list", Some(&json!({})))
        .map(|v| lettering::frame_choices(&v))
        .unwrap_or_default();
    // A monogram among the selection can be rebuilt with the form's settings.
    let selected_monogram = {
        let d = app.session.design();
        let ids: Vec<u64> = app.selection.iter().copied().collect();
        d.with_groups(&ids).into_iter().find(|&id| {
            d.index_of(id)
                .is_some_and(|i| d.objects[i].monogram.is_some())
        })
    };
    let mut open = true;
    let (mut add, mut edit, mut decorate) = (false, false, false);
    let shown = app
        .dialog_window(ctx, "Monogram", None)
        .open(&mut open)
        .resizable(false)
        .show(ctx, |ui| {
            let f = &mut app.lettering.monogram;
            egui::Grid::new("monogram-form")
                .num_columns(2)
                .show(ui, |ui| {
                    ui.label(t("ui-letters"));
                    ui.add(egui::TextEdit::singleline(&mut f.letters).desired_width(60.0))
                        .on_hover_text(t("ui-1-3-letters-or-digits-upper-tip"));
                    ui.end_row();
                    ui.label(t("ui-style"));
                    ui.horizontal(|ui| {
                        for (id, name) in MonogramForm::STYLES {
                            if ui.radio_value(&mut f.style, id, name).changed() {
                                f.font = None;
                            }
                        }
                    });
                    ui.end_row();
                    ui.label(t("ui-font"));
                    let mut font = f.font.clone().unwrap_or_else(|| {
                        if f.style == "script" {
                            "pacifico"
                        } else {
                            "lato-bold"
                        }
                        .into()
                    });
                    font_combo(ui, "monogram-font", "Font", &mut font, &choices);
                    f.font = Some(font);
                    ui.end_row();
                    ui.label(t("ui-height"));
                    ui.add(crate::units_ui::drag(&mut f.size, units).range(5.0..=300.0));
                    ui.end_row();
                    ui.label(t("ui-frame"));
                    let label = f
                        .frame
                        .and_then(|id| frames.iter().find(|(i, _)| i == id))
                        .map_or("None".to_string(), |(_, n)| n.clone());
                    let r = egui::ComboBox::from_id_salt("monogram-frame")
                        .selected_text(label)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut f.frame, None, "None");
                            for (id, name) in &frames {
                                let id: &'static str = openstitch_design::monogram::frame_info(id)
                                    .map_or("ring", |i| i.id);
                                ui.selectable_value(&mut f.frame, Some(id), name);
                            }
                        });
                    crate::a11y::name(&r.response, "Frame");
                    ui.end_row();
                    ui.label(t("ui-margin"));
                    ui.add(crate::units_ui::drag(&mut f.margin, units).range(0.0..=50.0));
                    ui.end_row();
                    ui.label("");
                    ui.checkbox(&mut f.keep_aspect, t("ui-maintain-aspect-ratio"));
                    ui.end_row();
                });
            ui.horizontal(|ui| {
                add = ui.button(t("ui-add")).clicked();
                edit = ui
                    .add_enabled(
                        selected_monogram.is_some(),
                        egui::Button::new(t("ui-apply-to-selected-monogram")),
                    )
                    .clicked();
                decorate = ui
                    .add_enabled(
                        !app.selection.is_empty() && f.frame.is_some(),
                        egui::Button::new(t("ui-frame-the-selection")),
                    )
                    .on_hover_text(t("ui-decorative-frame-around-the-selected-objects-tip"))
                    .clicked();
            });
            ui.weak("Letter styles and frames are OpenStitch's own designs.");
        });
    if let Some(shown) = shown {
        app.dialog_rect_next = Some(shown.response.rect);
    }
    app.lettering.monogram.open = open;
    let f = app.lettering.monogram.clone();
    let summary =
        |r: &serde_json::Value| format!("Monogram {}", r["letters"].as_str().unwrap_or_default());
    if add {
        run(app, &f.add(), summary);
    } else if edit && let Some(id) = selected_monogram {
        run(app, &f.edit(id), summary);
    } else if decorate && let Some(r) = f.decorate(&app.selection.clone()) {
        run(app, &r, |_| "Added a decorative frame".into());
    }
}

/// What the Name Drop window asked for this frame.
enum DropAction {
    Attribute(bool),
    Apply,
    ImportCsv,
    SaveCsv,
    ToFolder,
}

fn name_drop_window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.lettering.name_drop.open {
        return;
    }
    let columns: Vec<String> = app
        .session
        .execute("nameDrop.list", Some(&json!({})))
        .ok()
        .and_then(|v| v.as_array().cloned())
        .unwrap_or_default()
        .iter()
        .map(|c| {
            c["label"]
                .as_str()
                .map(str::to_string)
                .unwrap_or_else(|| format!("Text {}", c["id"]))
        })
        .collect();
    let texts_selected = {
        let d = app.session.design();
        !app.selection.is_empty()
            && app.selection.iter().all(|&id| {
                d.index_of(id)
                    .is_some_and(|i| d.objects[i].kind == openstitch_design::ObjectKind::Text)
            })
    };
    let n = columns.len();
    if app.lettering.name_drop.rows.is_empty() && n > 0 {
        // The list starts with what the design says now.
        let current: Vec<String> = app
            .session
            .execute("nameDrop.list", Some(&json!({})))
            .ok()
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .iter()
            .map(|c| c["text"].as_str().unwrap_or_default().to_string())
            .collect();
        app.lettering.name_drop.rows.push(current);
        app.lettering.name_drop.selected = Some(0);
    }
    app.lettering.name_drop.fit_columns(n);
    let mut open = true;
    let mut action = None;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-name-drop-list")).id(egui::Id::unique("window-name-drop-list"))
        .open(&mut open)
        .default_width(420.0)
        .show(ctx, |ui| {
            let f = &mut app.lettering.name_drop;
            ui.label(egui::RichText::new(t("ui-name-drop-attribute")).strong());
            ui.horizontal(|ui| {
                ui.checkbox(&mut f.maintain_aspect, t("ui-maintain-aspect-ratio"));
                for a in ["left", "center", "right"] {
                    ui.radio_value(&mut f.align, a, a);
                }
                for a in ["top", "middle", "bottom"] {
                    ui.radio_value(&mut f.valign, a, a);
                }
            });
            ui.horizontal(|ui| {
                ui.add_enabled_ui(texts_selected, |ui| {
                    if ui.button(t("ui-apply-to-selected-text")).clicked() {
                        action = Some(DropAction::Attribute(true));
                    }
                    if ui.button(t("ui-remove-from-selected-text")).clicked() {
                        action = Some(DropAction::Attribute(false));
                    }
                });
            });
            ui.separator();
            if n == 0 {
                crate::art::header(ui, crate::art::Art::NameDrop, |ui| {
                    ui.add_space(8.0);
                    ui.weak("No text has the Name Drop attribute yet.");
                    ui.weak("Select text and apply it.");
                });
                return;
            }
            egui::ScrollArea::vertical()
                .max_height(260.0)
                .show(ui, |ui| {
                    egui::Grid::new("name-drop-grid")
                        .striped(true)
                        .show(ui, |ui| {
                            ui.label("");
                            for c in &columns {
                                ui.strong(c);
                            }
                            ui.end_row();
                            for (i, row) in f.rows.iter_mut().enumerate() {
                                if ui
                                    .selectable_label(
                                        f.selected == Some(i),
                                        format!("{:03}", i + 1),
                                    )
                                    .clicked()
                                {
                                    f.selected = Some(i);
                                }
                                for cell in row.iter_mut() {
                                    if ui
                                        .add_sized([150.0, 20.0], egui::TextEdit::singleline(cell))
                                        .gained_focus()
                                    {
                                        f.selected = Some(i);
                                    }
                                }
                                ui.end_row();
                            }
                        });
                });
            ui.horizontal(|ui| {
                if ui
                    .button("+")
                    .on_hover_text(t("ui-add-a-row-500-at-most-tip"))
                    .clicked()
                {
                    f.add_row(n);
                }
                if ui.button(t("ui-insert")).clicked() {
                    f.insert_row(n);
                }
                if ui.button(t("ui-delete")).clicked() {
                    f.delete_row();
                }
                if ui.button(t("ui-import-csv")).clicked() {
                    action = Some(DropAction::ImportCsv);
                }
                if ui
                    .add_enabled(!f.rows.is_empty(), egui::Button::new(t("ui-save-as-csv")))
                    .clicked()
                {
                    action = Some(DropAction::SaveCsv);
                }
            });
            ui.separator();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(f.selected.is_some(), egui::Button::new(t("ui-use-selected-row")))
                    .on_hover_text(t("ui-put-the-row-s-text-into-tip"))
                    .clicked()
                {
                    action = Some(DropAction::Apply);
                }
                ui.label(t("ui-file-name"));
                ui.add(egui::TextEdit::singleline(&mut f.name).desired_width(90.0));
                egui::ComboBox::from_id_salt("name-drop-format")
                    .selected_text(f.format)
                    .show_ui(ui, |ui| {
                        for t in openstitch_formats::FileType::writable() {
                            ui.selectable_value(&mut f.format, t.extension(), t.extension());
                        }
                    });
                if ui
                    .add_enabled(!f.rows.is_empty(), egui::Button::new(t("ui-to-folder")))
                    .on_hover_text(t("ui-one-file-per-row-name001-name002-tip"))
                    .clicked()
                {
                    action = Some(DropAction::ToFolder);
                }
            });
        });
    app.lettering.name_drop.open = open;
    let f = app.lettering.name_drop.clone();
    match action {
        Some(DropAction::Attribute(on)) => {
            let ids = app.selection.clone();
            app.execute(&f.set_attribute(&ids, on));
        }
        Some(DropAction::Apply) => {
            if let Some(r) = f.apply() {
                app.execute(&r);
            }
        }
        Some(DropAction::ImportCsv) => crate::files::pick_then(
            crate::files::FileDialog::new().add_filter("CSV", &["csv"]),
            move |app, p| {
                if let Some(r) = app.execute(&NameDropForm::read_csv(&p)) {
                    app.lettering.name_drop.set_rows(&r, n);
                }
            },
        ),
        Some(DropAction::SaveCsv) => {
            if let Some(p) = crate::files::FileDialog::new()
                .add_filter("CSV", &["csv"])
                .set_file_name("name-drop.csv")
                .save_file()
            {
                app.execute(&f.write_csv(&p));
            }
        }
        Some(DropAction::ToFolder) => {
            if let Some(dir) = crate::files::FileDialog::new().pick_folder()
                && let Some(r) = app.execute(&f.export(&dir))
            {
                app.status = openstitch_app::tf!("ui-status-wrote-file-n-other-files-n", count = r["count"], path = dir.display());
            }
        }
        None => {}
    }
}

pub fn open_template_wizard(app: &mut EditorApp) {
    let w = &mut app.lettering.templates;
    w.open = true;
    w.step = 1;
    let r = w.list();
    if let Some(v) = app.execute(&r) {
        app.lettering.templates.set_templates(&v);
    }
}

pub fn save_template(app: &mut EditorApp) {
    let ext = openstitch_engine::templates::EXTENSION;
    if let Some(p) = crate::files::FileDialog::new()
        .add_filter("OpenStitch template", &[ext])
        .set_file_name(format!("{}.{ext}", app.session.design().name))
        .save_file()
        && let Some(r) = app.execute(&CommandRequest::new(
            "template.save",
            json!({ "path": p.display().to_string() }),
        ))
    {
        app.status = openstitch_app::tf!("ui-status-saved-template-with-editable-text-s", slots = r["slots"].as_array().map_or(0, Vec::len));
    }
}

fn template_window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.lettering.templates.open {
        return;
    }
    let mut open = true;
    let (mut next, mut back, mut folder, mut export, mut done, mut again) =
        (false, false, false, false, false, false);
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-template-wizard")).id(egui::Id::unique("window-template-wizard"))
        .open(&mut open)
        .default_width(360.0)
        .show(ctx, |ui| {
            let w = &mut app.lettering.templates;
            match w.step {
                2 => {
                    ui.label(egui::RichText::new(t("ui-step-2-edit-the-text")).strong());
                    ui.weak("The Design Page shows the template; it updates when you apply.");
                    egui::Grid::new("template-slots")
                        .num_columns(2)
                        .show(ui, |ui| {
                            for slot in &mut w.slots {
                                ui.label(&slot.label);
                                ui.add_sized(
                                    [220.0, 20.0],
                                    egui::TextEdit::singleline(&mut slot.text),
                                );
                                ui.end_row();
                            }
                        });
                    ui.horizontal(|ui| {
                        back = ui.button(t("ui-back")).clicked();
                        next = ui.button(t("ui-apply-and-continue")).clicked();
                    });
                }
                3 => {
                    ui.label(egui::RichText::new(t("ui-step-3-output")).strong());
                    done = ui.button(t("ui-edit-in-design-page")).clicked();
                    export = ui.button(t("ui-export-pes")).clicked();
                    again = ui.button(t("ui-create-new-embroidery-design")).clicked();
                }
                _ => {
                    ui.label(egui::RichText::new(t("ui-step-1-choose-a-template")).strong());
                    ui.horizontal(|ui| {
                        folder = ui.button(t("ui-template-folder")).clicked();
                        ui.weak(
                            w.dir
                                .as_ref()
                                .map_or("OpenStitch templates".into(), |d| d.display().to_string()),
                        );
                    });
                    let mut picked = None;
                    egui::ScrollArea::vertical()
                        .max_height(260.0)
                        .show(ui, |ui| {
                            for (category, rows) in w.categories() {
                                ui.label(egui::RichText::new(category).strong());
                                for t in rows {
                                    let chosen = w.chosen.as_deref() == Some(t.id.as_str());
                                    let r = ui.selectable_label(chosen, format!("  {}", t.name));
                                    let r = if t.description.is_empty() {
                                        r
                                    } else {
                                        r.on_hover_text(&t.description)
                                    };
                                    if r.clicked() {
                                        picked = Some(t.id.clone());
                                    }
                                }
                            }
                        });
                    if picked.is_some() {
                        w.chosen = picked;
                    }
                    ui.add_enabled_ui(w.chosen.is_some(), |ui| {
                        next = ui.button(t("ui-next")).clicked();
                    });
                }
            }
        });
    app.lettering.templates.open = open;
    let step = app.lettering.templates.step.max(1);
    if folder && let Some(d) = crate::files::FileDialog::new().pick_folder() {
        app.lettering.templates.dir = Some(d);
        open_template_wizard(app);
    } else if back {
        app.lettering.templates.step = 1;
    } else if next && step == 1 {
        if let Some(r) = app.lettering.templates.open_chosen()
            && let Some(v) = app.execute(&r)
        {
            app.lettering.templates.set_slots(&v);
            app.lettering.templates.step = 2;
            app.selection.clear();
            app.fit_requested = true;
        }
    } else if next && step == 2 {
        let r = app.lettering.templates.fill();
        if let Some(v) = app.execute(&r) {
            app.lettering.templates.set_slots(&v);
            app.lettering.templates.step = 3;
        }
    } else if export {
        app.export(openstitch_formats::FileType::Pes);
    } else if done {
        app.lettering.templates.open = false;
    } else if again {
        app.lettering.templates.step = 1;
    }
}

/// Text ▸ Edit Character: offset, rotation and size of the character picked with Point Edit.
/// A value is applied when its drag ends or it is typed, so each change is one Undo.
pub fn character_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    use openstitch_app::editor::Tool;
    use openstitch_app::editor::tools::characters::{char_edit, set_character};
    let design = app.session.design().clone();
    let picked = app.tools.selected_character(&app.tool_context(&design));
    let Some((id, index)) = picked else {
        ui.label(t("ui-pick-point-edit-then-click-a"));
        if app.tools.tool() != Tool::Point && ui.button(t("ui-point-edit")).clicked() {
            app.tools.set_tool(Tool::Point);
            app.workspace.measure.active = false;
            app.ui.pan = false;
        }
        return;
    };
    let Some(spec) = design
        .index_of(id)
        .and_then(|i| design.objects[i].text.as_ref())
    else {
        return;
    };
    let ch = spec.text.chars().nth(index).unwrap_or(' ');
    ui.label(format!(
        "Character “{ch}” ({} of {})",
        index + 1,
        spec.text.chars().count()
    ));
    let before = char_edit(spec, index);
    let mut e = before.clone();
    let mut size = e.size.unwrap_or(spec.size);
    let mut commit = false;
    egui::Grid::new("character-edit").show(ui, |ui| {
        let mut row = |ui: &mut egui::Ui, label: &str, v: &mut f64, range, suffix: &str| {
            ui.label(label);
            let r = ui.add(
                egui::DragValue::new(v)
                    .range(range)
                    .speed(0.1)
                    .max_decimals(2)
                    .suffix(suffix),
            );
            commit |= r.drag_stopped() || (r.changed() && !r.dragged());
            ui.end_row();
        };
        row(ui, "Right", &mut e.x, -500.0..=500.0, " mm");
        row(ui, "Down", &mut e.y, -500.0..=500.0, " mm");
        row(ui, "Rotation", &mut e.rotation, -360.0..=360.0, "°");
        row(ui, "Size", &mut size, 1.0..=300.0, " mm");
    });
    if (size - e.size.unwrap_or(spec.size)).abs() > 1e-9 {
        e.size = ((size - spec.size).abs() > 1e-9).then_some(size);
    }
    if commit && e != before {
        app.execute(&set_character(id, &e));
    }
    if ui
        .add_enabled(!before.is_noop(), egui::Button::new(t("ui-reset-character")))
        .clicked()
    {
        app.execute(&CommandRequest::new(
            "text.setCharacter",
            json!({ "id": id, "index": index, "reset": true }),
        ));
    }
}
