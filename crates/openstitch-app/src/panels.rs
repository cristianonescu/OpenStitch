//! Sewing order list, sewing attribute sections and the stitch simulator, hosted by the panes
//! in `panes.rs`.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use crate::pickers_ui::{self, Sample, Tile};
use eframe::egui::{self, Color32, RichText, Sense};
use openstitch_app::editor::inspector::{self, Common, Control, InspectorModel};
use openstitch_app::editor::render::color_at;
use openstitch_app::editor::{CommandRequest, objects};
use openstitch_core::Thread;
use openstitch_design::{LineType, ObjectKind, RegionType};
use serde_json::{Value, json};
use std::collections::BTreeSet;

fn rgb(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

/// A plain colour square with no label: only for colours that are not threads (fabric,
/// accent) or for "Mixed". A thread colour goes through [`thread_swatch`] (#798; the guard
/// `no_thread_swatch_without_its_code` checks every call).
pub fn swatch(ui: &mut egui::Ui, c: [u8; 3], size: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(egui::vec2(size, size), Sense::click());
    paint_swatch(ui, r, c);
    resp
}

fn paint_swatch(ui: &egui::Ui, r: egui::Rect, c: [u8; 3]) {
    ui.painter().rect(
        r,
        egui::CornerRadius::same(2),
        rgb(c),
        egui::Stroke::new(1.0, Color32::from_gray(90)),
        egui::StrokeKind::Inside,
    );
}

/// Where a thread swatch's code goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Caption {
    /// Under the swatch, centred (grids).
    Under,
    /// To the right of the swatch (lists and rows).
    Beside,
}

/// A thread colour with its code always shown (#798): the colour, and under or beside it the
/// code of the thread's own chart ([`openstitch_core::thread_label`]: Brother 007, a maker's or
/// the user's code, `#rrggbb` for a custom colour). The code is in the text colour beside the
/// swatch, so it reads on any thread colour, at the small text size. The tooltip has the full
/// detail ([`thread_tip`]). Clicking it is the response's click.
pub fn thread_swatch(ui: &mut egui::Ui, t: &Thread, size: f32, caption: Caption) -> egui::Response {
    let label = openstitch_core::thread_label(t);
    let font = egui::TextStyle::Small.resolve(ui.style());
    let colour = ui.visuals().text_color();
    let galley = ui.painter().layout_no_wrap(label, font, colour);
    let g = galley.size();
    let size_v = match caption {
        Caption::Under => egui::vec2(size.max(g.x + 2.0), size + 1.0 + g.y),
        Caption::Beside => egui::vec2(size + 3.0 + g.x, size.max(g.y)),
    };
    let (rect, resp) = ui.allocate_exact_size(size_v, Sense::click());
    let (sw, text) = match caption {
        Caption::Under => (
            egui::Rect::from_min_size(
                egui::pos2(rect.center().x - size / 2.0, rect.top()),
                egui::vec2(size, size),
            ),
            egui::pos2(rect.center().x - g.x / 2.0, rect.top() + size + 1.0),
        ),
        Caption::Beside => (
            egui::Rect::from_min_size(
                egui::pos2(rect.left(), rect.center().y - size / 2.0),
                egui::vec2(size, size),
            ),
            egui::pos2(rect.left() + size + 3.0, rect.center().y - g.y / 2.0),
        ),
    };
    paint_swatch(ui, sw, [t.red, t.green, t.blue]);
    ui.painter().galley(text, galley, colour);
    resp.on_hover_text(thread_tip(t))
}

/// What a swatch of a colour list shows, as `ui.threadLabels` reports it (#798).
pub fn label_json(t: &Thread) -> Value {
    json!({
        "code": openstitch_core::thread_label(t),
        "name": t.name,
        "brand": openstitch_core::thread_brand(t),
        "color": t.hex(),
    })
}

/// The threads of one of the editor's colour lists, as its swatches show them (`ui.threadLabels`,
/// #798).
pub fn thread_labels(app: &EditorApp, list: &str) -> Result<Vec<Value>, String> {
    let threads: Vec<Thread> = match list {
        "picker" => {
            let user = app.session.user_settings();
            picker_chart(&user.default_chart(), user).1
        }
        "pecPicker" => pec_matches("").into_iter().map(|(_, t)| t.clone()).collect(),
        "sewingOrder" => objects::rows(app.session.design(), &app.selection)
            .into_iter()
            .flat_map(|r| r.swatches)
            .collect(),
        "colourPane" => app.pattern.threads.clone(),
        "changeColor" => {
            let id = crate::convert_ui::stitch_selection(app)
                .1
                .ok_or("select a stitch pattern first")?;
            let d = app.session.design();
            d.index_of(id)
                .and_then(|i| d.objects[i].stitch_block.as_ref())
                .map(|b| b.threads.clone())
                .unwrap_or_default()
        }
        other => {
            return Err(format!(
                "list '{other}': use picker, pecPicker, sewingOrder, colourPane or changeColor"
            ));
        }
    };
    Ok(threads.iter().map(label_json).collect())
}

/// A thread as a JSON object of the engine's (`color`, `code`, `brand`, `description`).
pub fn json_thread(v: &Value) -> Thread {
    let mut t = v["color"]
        .as_str()
        .and_then(Thread::from_hex)
        .unwrap_or(Thread::rgb(0));
    let text = |k: &str| {
        v[k].as_str()
            .filter(|s| !s.is_empty())
            .map(|s| std::borrow::Cow::Owned(s.to_string()))
    };
    t.catalog_number = text("code");
    t.brand = text("brand");
    t.name = text("description").or_else(|| text("name"));
    t
}

/// How a colour picker lays its threads out: a grid of swatches with their codes, or a list
/// (swatch, code, name) for when codes and names need the room.
fn list_view(ui: &egui::Ui) -> bool {
    ui.data(|d| d.get_temp::<bool>(egui::Id::unique("thread-picker-list")))
        .unwrap_or(false)
}

/// The Grid / List switch over a colour picker's threads.
fn layout_switch(ui: &mut egui::Ui) {
    let key = egui::Id::unique("thread-picker-list");
    let mut list = list_view(ui);
    ui.horizontal(|ui| {
        ui.selectable_value(&mut list, false, t("ui-grid"));
        ui.selectable_value(&mut list, true, t("ui-list"));
    });
    ui.data_mut(|d| d.insert_temp(key, list));
}

/// A picker's threads as a grid (8 to a row, code under each) or a list; returns the position
/// clicked. `marked` is the position of the current thread.
fn thread_cells(
    ui: &mut egui::Ui,
    id: (&str, &str),
    threads: &[&Thread],
    marked: Option<usize>,
    extra_tip: impl Fn(usize) -> Option<String>,
) -> Option<usize> {
    let mut chosen = None;
    let mut cell = |ui: &mut egui::Ui, n: usize, th: &Thread, caption: Caption| {
        let mut r = thread_swatch(ui, th, 20.0, caption);
        if let Some(more) = extra_tip(n) {
            r = r.on_hover_text(more);
        }
        if caption == Caption::Beside {
            r = r.union(ui.label(th.name.as_deref().unwrap_or_default()));
        }
        if marked == Some(n) {
            ui.painter().rect_stroke(
                r.rect.expand(1.5),
                egui::CornerRadius::same(3),
                egui::Stroke::new(2.0, ui.visuals().strong_text_color()),
                egui::StrokeKind::Outside,
            );
        }
        if r.clicked() {
            chosen = Some(n);
            ui.close();
        }
    };
    if list_view(ui) {
        egui::ScrollArea::vertical()
            .id_salt(("thread-list", id))
            .max_height(320.0)
            .show(ui, |ui| {
                for (n, th) in threads.iter().enumerate() {
                    ui.horizontal(|ui| cell(ui, n, th, Caption::Beside));
                }
            });
    } else {
        egui::Grid::new(id)
            .spacing(egui::vec2(3.0, 3.0))
            .show(ui, |ui| {
                for (n, th) in threads.iter().enumerate() {
                    cell(ui, n, th, Caption::Under);
                    if n % 8 == 7 {
                        ui.end_row();
                    }
                }
            });
    }
    chosen
}

pub fn zoom_center(app: &mut EditorApp, factor: f64) {
    if let Some(vp) = &mut app.viewport {
        // Anchor at the model point currently in the middle of the page.
        let page = app.session.design().page.center();
        let anchor = vp.to_screen(page);
        vp.zoom_about(anchor, factor);
    }
}

// MARK: object list

pub fn object_list(app: &mut EditorApp, ui: &mut egui::Ui) {
    if let Some(r) = crate::edit_ui::order_controls(app, ui)
        && let Some(result) = app.execute(&r)
        && let Some(summary) = result.get("summary").and_then(|v| v.as_str())
    {
        // Optimise reports what it changed (#547); Undo takes it back.
        app.status = openstitch_app::tf!("ui-status-optimise", summary = summary);
    }
    if ui
        .button(t("ui-review-sewing-order"))
        .on_hover_text(t("ui-step-through-every-move-between-objects-tip"))
        .clicked()
    {
        app.quality.open = true;
        app.quality.tab = crate::quality_ui::Tab::Route;
    }
    ui.checkbox(&mut app.edit.by_colour, t("ui-frames-by-thread-colour"))
        .on_hover_text(t("ui-one-frame-per-run-of-objects-tip"));
    ui.separator();
    if app.edit.by_colour {
        crate::edit_ui::colour_frames(app, ui);
        return;
    }
    let all = objects::rows(app.session.design(), &app.selection);
    if all.is_empty() {
        crate::art::notice(
            ui,
            crate::art::Art::ObjectsEmpty,
            64.0,
            "No objects. Draw with the shape tools or File > Import into design.",
        );
        return;
    }
    // Find and filter (#745): only the list narrows, never the design or its order.
    let rows: Vec<_> = all
        .iter()
        .filter(|r| objects::matches(&r.search, &app.edit.order_filter))
        .cloned()
        .collect();
    order_filter(app, ui, &rows, all.len());
    let mut pending: Option<CommandRequest> = None;
    let mut select: Option<(u64, bool)> = None;
    let (mut rename, mut rename_done) = (None, false);
    let modifiers = ui.input(|i| i.modifiers);
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for row in &rows {
                let drop = ui
                    .dnd_drop_zone::<usize, ()>(egui::Frame::NONE, |ui| {
                        ui.horizontal(|ui| {
                            let mut visible = row.visible;
                            if ui
                                .checkbox(&mut visible, "")
                                .on_hover_text(t("ui-visible-hidden-objects-are-not-sewn-tip"))
                                .changed()
                            {
                                pending = Some(objects::set_visible(row.id, visible));
                            }
                            let handle = ui.dnd_drag_source(
                                ui.scope_id().with(("row", row.id)),
                                row.index,
                                |ui| {
                                    // #653: the Higgsfield hand icon as the drag grip.
                                    ui.add(crate::icons::ui_image(ui, "hand"));
                                },
                            );
                            handle
                                .response
                                .on_hover_text(t("ui-drag-to-change-the-sewing-order-tip"));
                            let d = app.session.design();
                            if let Some(i) = d.index_of(row.id) {
                                crate::panes::thumbnail(ui, &d.objects[i], 30.0);
                            }
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 1.0;
                                for th in row.swatches.iter().take(3) {
                                    thread_swatch(ui, th, 9.0, Caption::Beside);
                                }
                            });
                            if let Some((id, name)) = &mut app.edit.renaming
                                && *id == row.id
                            {
                                let edit = ui.add(
                                    egui::TextEdit::singleline(name)
                                        .desired_width(140.0)
                                        .char_limit(objects::MAX_NAME),
                                );
                                edit.request_focus();
                                if edit.lost_focus() {
                                    let typed = name.clone();
                                    let escaped = ui.input(|i| i.key_pressed(egui::Key::Escape));
                                    if !escaped && Some(typed.trim()) != row.name.as_deref() {
                                        pending = Some(objects::set_name(row.id, &typed));
                                    }
                                    rename_done = true;
                                }
                                return;
                            }
                            let lock = if row.locked { " (locked)" } else { "" };
                            let text = format!("{}. {}{lock}", row.index + 1, row.label);
                            let resp = ui
                                .selectable_label(row.selected, text)
                                .on_hover_text(format!("{} · double-click to rename", row.detail));
                            if resp.double_clicked() {
                                rename = Some((row.id, row.name.clone().unwrap_or_default()));
                            } else if resp.clicked() {
                                select = Some((row.id, modifiers.shift || modifiers.command));
                            }
                        });
                    })
                    .1;
                if let Some(from) = drop {
                    pending = Some(objects::reorder(
                        rows[*from].id,
                        objects::drop_index(*from, row.index),
                    ));
                }
            }
        });
    if rename_done {
        app.edit.renaming = None;
    }
    if rename.is_some() {
        app.edit.renaming = rename;
    }
    if let Some((id, toggle)) = select {
        if toggle {
            if !app.selection.remove(&id) {
                app.selection.insert(id);
            }
        } else {
            app.selection = BTreeSet::from([id]);
        }
    }
    if let Some(r) = pending {
        app.execute(&r);
    }
}

/// The Sewing Order list's filter field (#745): Ctrl+F with the pointer over the list puts the
/// cursor in it, typing narrows the list, Esc clears it, and Enter or Select All Shown selects
/// the rows shown (through `ui.select`, as the Assistant selects).
fn order_filter(app: &mut EditorApp, ui: &mut egui::Ui, shown: &[objects::ObjectRow], total: usize) {
    let id = egui::Id::unique("sewing-order-filter");
    // The whole pane, not just the part laid out so far.
    let hovered = ui.rect_contains_pointer(ui.max_rect());
    let find = ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::F));
    let mut select = false;
    ui.horizontal(|ui| {
        let field = ui.add(
            egui::TextEdit::singleline(&mut app.edit.order_filter)
                .id(id)
                .hint_text(t("ui-find-objects"))
                .desired_width(150.0),
        );
        let field = field.on_hover_text(t("ui-find-objects-tip"));
        if find && hovered {
            field.request_focus();
        }
        if field.lost_focus() {
            let (enter, escape) =
                ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
            if escape {
                app.edit.order_filter.clear();
            } else if enter && !app.edit.order_filter.trim().is_empty() {
                select = true;
            }
        }
        let filtering = !app.edit.order_filter.trim().is_empty();
        if filtering {
            ui.label(openstitch_app::tf!("ui-objects-shown", shown = shown.len(), total = total));
        }
        if ui
            .add_enabled(filtering && !shown.is_empty(), egui::Button::new(t("ui-select-all-shown")))
            .on_hover_text(t("ui-select-all-shown-tip"))
            .clicked()
        {
            select = true;
        }
    });
    if select {
        let ids: Vec<u64> = shown.iter().map(|r| r.id).collect();
        let _ = app.ui_command("ui.select", Some(&serde_json::json!({ "ids": ids, "mode": "set" })));
    }
}

// MARK: inspector

/// A number field that issues one command when the drag ends or typing is confirmed.
fn number(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    key: &'static str,
    value: f64,
    range: (f64, f64),
    speed: f64,
    suffix: &str,
) -> Option<f64> {
    let mut v = app.drafts.get(key).copied().unwrap_or(value);
    let resp = ui.add(
        egui::DragValue::new(&mut v)
            .range(range.0..=range.1)
            .speed(speed)
            .max_decimals(2)
            .suffix(suffix),
    );
    if resp.changed() {
        app.drafts.insert(key, v);
    }
    let finished = resp.drag_stopped()
        || resp.lost_focus()
        || (resp.changed() && !resp.dragged() && !resp.has_focus());
    if finished
        && let Some(v) = app.drafts.remove(key)
        && (v - value).abs() > 1e-9
    {
        return Some(v);
    }
    None
}

/// The colour picker (#796): the user's default thread chart (`threads.defaultChart`, New
/// Brothread until they choose another) as a grid of swatches, with Brother's PEC chart one
/// click away. Returns the `color` parameter of the thread clicked: a PEC chart number, a
/// built-in chart's `{builtin, code}` or a user chart's whole thread.
pub fn thread_grid(
    ui: &mut egui::Ui,
    id: &str,
    current: Option<&Thread>,
    user: &openstitch_engine::user::UserSettings,
) -> Option<Value> {
    use openstitch_engine::user::ChartRef;
    let default = user.default_chart();
    let pec_key = egui::Id::unique("thread-picker-pec");
    let mut pec = default == ChartRef::Builtin("brother".into())
        || ui.data(|d| d.get_temp::<bool>(pec_key)).unwrap_or(false);
    if default != ChartRef::Builtin("brother".into()) {
        let (name, _) = picker_chart(&default, user);
        ui.horizontal(|ui| {
            if ui.selectable_label(!pec, name).clicked() {
                pec = false;
            }
            if ui
                .selectable_label(pec, t("ui-brother-pec-thread-chart"))
                .clicked()
            {
                pec = true;
            }
        });
        ui.data_mut(|d| d.insert_temp(pec_key, pec));
    }
    if pec {
        return pec_grid(ui, id, current).map(|i| json!(i));
    }
    let (name, threads) = picker_chart(&default, user);
    ui.label(RichText::new(name).strong());
    let search_id = ui.make_persistent_id(("chart-search", id));
    let mut query: String = ui.data_mut(|d| d.get_temp(search_id)).unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text(t("ui-search-code-name-or-madeira"))
            .desired_width(8.0 * 23.0),
    );
    ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
    let shown = chart_matches(&threads, &query);
    if shown.is_empty() {
        ui.label(RichText::new(t("ui-no-thread-in-this-chart-matches")).weak());
    }
    layout_switch(ui);
    let cells: Vec<&Thread> = shown.iter().map(|&i| &threads[i]).collect();
    let marked = cells
        .iter()
        .position(|th| current.is_some_and(|c| same_thread(c, th)));
    thread_cells(ui, ("chart", id), &cells, marked, |_| None)
        .map(|n| colour_param(&default, shown[n], cells[n]))
}

/// The picker's chart: its name and threads (a missing chart gives none).
pub fn picker_chart(
    c: &openstitch_engine::user::ChartRef,
    user: &openstitch_engine::user::UserSettings,
) -> (String, Vec<Thread>) {
    use openstitch_engine::user::ChartRef;
    match c {
        ChartRef::Builtin(id) => openstitch_engine::thread_match::builtins()
            .into_iter()
            .find(|b| b.id == id)
            .map(|b| {
                (
                    b.name.to_string(),
                    b.entries.into_iter().map(|(_, t)| t).collect(),
                )
            })
            .unwrap_or_default(),
        ChartRef::Chart(n) => user
            .thread_charts
            .iter()
            .find(|ch| ch.name == *n)
            .map(|ch| (ch.name.clone(), ch.threads.clone()))
            .unwrap_or_default(),
    }
}

/// The positions of a chart's threads matching a search (all for an empty one), as
/// `threads.find` ranks them: code, name, brand or another maker's code (#796: "1801" finds
/// New Brothread 001 White).
pub fn chart_matches(threads: &[Thread], query: &str) -> Vec<usize> {
    let q = query.trim();
    if q.is_empty() {
        return (0..threads.len()).collect();
    }
    let mut v: Vec<(u8, usize)> = threads
        .iter()
        .enumerate()
        .filter_map(|(i, th)| openstitch_engine::thread_match::find_rank(th, q).map(|r| (r, i)))
        .collect();
    v.sort();
    v.into_iter().map(|(_, i)| i).collect()
}

/// A thread's hover text: brand, code, name, other makers' codes and colour (#798).
pub fn thread_tip(th: &Thread) -> String {
    let code = openstitch_core::thread_code(th);
    let mut s = [
        openstitch_core::thread_brand(th),
        code.as_deref(),
        th.name.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    for (brand, code) in openstitch_engine::user::cross_references(th) {
        s.push_str(&format!(" · {brand} {code}"));
    }
    format!("{s} ({})", th.hex())
}

/// Whether `a` is chart thread `b`: the same code and brand, or (no code) the same colour and
/// name.
fn same_thread(a: &Thread, b: &Thread) -> bool {
    match (&b.catalog_number, &a.catalog_number) {
        (Some(x), Some(y)) => x == y && a.brand == b.brand,
        _ => a.rgb_value() == b.rgb_value() && a.name == b.name,
    }
}

/// The `color` parameter for thread `i` of the picker's chart.
fn colour_param(c: &openstitch_engine::user::ChartRef, i: usize, th: &Thread) -> Value {
    use openstitch_engine::user::ChartRef;
    match (c, &th.catalog_number) {
        (ChartRef::Builtin(id), Some(code)) => json!({ "builtin": id, "code": code }),
        (ChartRef::Builtin(id), None) => json!({ "builtin": id, "index": i }),
        (ChartRef::Chart(_), _) => json!({
            "color": th.hex(),
            "code": th.catalog_number,
            "brand": th.brand,
            "description": th.name,
        }),
    }
}

/// The PEC thread chart as a grid of swatches; returns the chart number clicked.
pub fn pec_grid(ui: &mut egui::Ui, id: &str, current: Option<&Thread>) -> Option<usize> {
    let index = current.and_then(inspector::pec_index);
    ui.label(RichText::new(t("ui-brother-pec-thread-chart")).strong());
    // Search by chart number, code or name (#486), matched as `threads.find` matches.
    let search_id = ui.make_persistent_id(("pec-search", id));
    let mut query: String = ui.data_mut(|d| d.get_temp(search_id)).unwrap_or_default();
    ui.add(
        egui::TextEdit::singleline(&mut query)
            .hint_text(t("ui-search-code-name-or-pec-number"))
            .desired_width(8.0 * 23.0),
    );
    ui.data_mut(|d| d.insert_temp(search_id, query.clone()));
    let shown = pec_matches(&query);
    if shown.is_empty() {
        ui.label(RichText::new(t("ui-no-thread-in-this-chart-matches")).weak());
    }
    layout_switch(ui);
    // Brother's code under each swatch; the PEC number, which PE-DESIGN's own chart counts by,
    // in the tooltip, labelled.
    let cells: Vec<&Thread> = shown.iter().map(|(_, t)| *t).collect();
    let marked = shown
        .iter()
        .position(|(i, _)| Some(*i) == index && current.is_some_and(|c| c.name.is_some()));
    let numbers: Vec<usize> = shown.iter().map(|(i, _)| *i).collect();
    thread_cells(ui, ("pec", id), &cells, marked, |n| {
        Some(format!("PEC # {}", numbers[n]))
    })
    .map(|n| numbers[n])
}

/// The PEC chart's threads matching a search: all of them for an empty query, otherwise those
/// whose chart number starts with it and those `threads.find {charts: ["brother"]}` finds (by
/// Brother Embroidery code or name, the engine's own entries for the chart).
pub fn pec_matches(query: &str) -> Vec<(usize, &'static Thread)> {
    let q = query.trim();
    if q.is_empty() {
        return inspector::pec_palette();
    }
    let found: Vec<usize> = openstitch_engine::thread_match::builtins()
        .into_iter()
        .filter(|b| b.id == "brother")
        .flat_map(|b| b.entries)
        .filter(|(_, t)| openstitch_engine::thread_match::find_rank(t, q).is_some())
        .map(|(i, _)| i)
        .collect();
    inspector::pec_palette()
        .into_iter()
        .filter(|(i, _)| i.to_string().starts_with(q) || found.contains(i))
        .collect()
}

/// Colour picker button ([`thread_grid`]): returns the `color` parameter chosen.
fn thread_picker(
    ui: &mut egui::Ui,
    id: &str,
    current: Option<&Thread>,
    user: &openstitch_engine::user::UserSettings,
) -> Option<Value> {
    let mut chosen = None;
    // The swatch carries the thread's code (#798: Brother 007, not PEC 1); the button its name.
    let label = match current {
        Some(t) => t.name.as_deref().unwrap_or("Custom").to_string(),
        None => "Mixed".into(),
    };
    ui.horizontal(|ui| {
        let resp = match current {
            Some(t) => thread_swatch(ui, t, 18.0, Caption::Beside),
            // bare-swatch: "Mixed" is several threads, named on the button.
            None => swatch(ui, [128, 128, 128], 18.0),
        };
        let button = ui.button(label);
        let popup_id = ui.make_persistent_id(id);
        let open = resp.clicked() || button.clicked();
        egui::Popup::from_response(&button)
            .id(popup_id)
            .open_memory(open.then_some(egui::SetOpenCommand::Toggle))
            .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
            .show(|ui| {
                chosen = thread_grid(ui, id, current, user);
            });
    });
    chosen
}

/// The Sewing Attributes pane: transform, region and line sewing of the selection.
pub fn sewing_attributes(app: &mut EditorApp, ui: &mut egui::Ui) {
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            if app.selection.is_empty() {
                design_info(app, ui);
                return;
            }
            let m = InspectorModel::new(app.session.design(), &app.selection);
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("{} selected", app.selection.len()));
                for a in [
                    openstitch_app::editor::ribbon::Action::CopyAttributes,
                    openstitch_app::editor::ribbon::Action::PasteAttributes,
                ] {
                    let info = a.info();
                    if ui
                        .add_enabled(
                            crate::ribbon::enabled(app, a),
                            egui::Button::new(info.label),
                        )
                        .on_hover_text(info.tip)
                        .clicked()
                    {
                        crate::ribbon::run(app, a, ui.ctx());
                    }
                }
                basic_advanced_and_defaults(app, ui);
            });
            // Property search (#550): only the rows whose label or parameter match.
            ui.add(
                egui::TextEdit::singleline(&mut app.ui.property_search)
                    .hint_text("Search properties: pull, underlay…")
                    .desired_width(f32::INFINITY),
            );
            let filter = app.ui.property_search.clone();
            if inspector::matches_filter(
                &filter,
                &["Transform", "rotate flip centre scale duplicate delete"],
            ) {
                transform_section(app, ui);
            }
            if m.shapes.is_empty() {
                ui.separator();
                ui.label(
                    RichText::new(
                        "Imported stitch patterns keep their stitches; only transforms apply.",
                    )
                    .weak(),
                );
                if crate::stitch_table_ui::target(app).is_some()
                    && ui
                        .button(t("ui-stitch-table"))
                        .on_hover_text(
                            "Every needle point with its exact position, to edit, insert or delete",
                        )
                        .clicked()
                {
                    app.stitch_table.open = true;
                }
                return;
            }
            let mut requests: Vec<CommandRequest> = Vec::new();
            hand_edits_row(app, ui, &mut requests);
            region_section(app, ui, &m, &mut requests);
            line_section(app, ui, &m, &mut requests);
            if !filter.trim().is_empty() && ui.button(t("ui-clear-search")).clicked() {
                app.ui.property_search.clear();
            }
            for r in requests {
                app.execute(&r);
            }
        });
}

/// The Basic / Advanced switch and Save as Default (#553, beyond PE-DESIGN).
fn basic_advanced_and_defaults(app: &mut EditorApp, ui: &mut egui::Ui) {
    let adv = app.ui.prefs.advanced_attributes;
    let mut pick = None;
    if ui
        .selectable_label(!adv, t("ui-basic"))
        .on_hover_text(t("ui-only-the-settings-most-designs-change-tip"))
        .clicked()
    {
        pick = Some(false);
    }
    if ui
        .selectable_label(adv, t("ui-advanced"))
        .on_hover_text(t("ui-every-sewing-setting-as-pe-design-tip"))
        .clicked()
    {
        pick = Some(true);
    }
    if let Some(a) = pick {
        app.ui.prefs.advanced_attributes = a;
        app.ui.save_prefs();
    }
    let single = (app.selection.len() == 1)
        .then(|| app.selection.iter().next().copied())
        .flatten()
        .and_then(|id| {
            let d = app.session.design();
            d.index_of(id).map(|i| d.objects[i].clone())
        })
        .filter(|o| o.region.is_some() || o.line.is_some());
    if ui
        .add_enabled(single.is_some(), egui::Button::new(t("ui-save-as-default")))
        .on_hover_text(t("ui-new-shapes-start-with-this-shape-tip"))
        .on_disabled_hover_text("Select one shape")
        .clicked()
        && let Some(o) = single
    {
        app.ui.prefs.new_objects = openstitch_app::editor::new_objects::NewObjects::from_object(
            o.region.as_ref(),
            o.line.as_ref(),
        );
        app.ui.save_prefs();
        app.status = openstitch_app::editor::ui_texts::t("ui-status-new-shapes-start-with-these-settings").into();
    }
}

pub fn design_info(app: &mut EditorApp, ui: &mut egui::Ui) {
    let d = app.session.design();
    let (name, w, h, n) = (
        d.name.clone(),
        d.page.width / 10.0,
        d.page.height / 10.0,
        d.objects.len(),
    );
    egui::Grid::new("design").num_columns(2).show(ui, |ui| {
        ui.label(t("ui-design"));
        ui.label(name);
        ui.end_row();
        ui.label(t("ui-design-page"));
        ui.label(openstitch_app::editor::units::size(
            app.ui.prefs.units,
            w,
            h,
        ));
        ui.end_row();
        ui.label(t("ui-objects"));
        ui.label(n.to_string());
        ui.end_row();
        let m = &app.session.design().machine;
        ui.label(t("ui-machine"));
        ui.label(match m.machine {
            openstitch_design::MachineType::SingleNeedle => "Single needle",
            openstitch_design::MachineType::MultiNeedle => "Multi-needle",
        });
        ui.end_row();
        ui.label(t("ui-hoop"));
        ui.label(m.hoop.as_deref().unwrap_or("custom"));
        ui.end_row();
        if let Some(e) = &app.workspace.estimate {
            let per: Vec<String> = e
                .needle_thread
                .iter()
                .map(|mm| format!("{:.1}", mm / 1000.0))
                .collect();
            ui.label(t("ui-estimate"));
            ui.label(crate::workspace_ui::estimate_text(e))
                .on_hover_text(format!("Needle thread per colour (m): {}", per.join(", ")));
            ui.end_row();
        }
    });
    if ui.button(t("ui-design-settings")).clicked() {
        app.workspace.settings_open = true;
    }
    ui.add_space(8.0);
    ui.label(RichText::new(t("ui-select-an-object-to-edit-its")).weak());
}

pub fn transform_section(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.separator();
    ui.label(RichText::new(t("ui-transform")).strong());
    ui.horizontal_wrapped(|ui| {
        if ui.button(t("ui-rotate-90")).clicked() {
            app.on_selection(|s| CommandRequest::rotate(s, -90.0));
        }
        if ui.button(t("ui-rotate-90-2")).clicked() {
            app.on_selection(|s| CommandRequest::rotate(s, 90.0));
        }
        if ui.button(t("ui-flip-h")).clicked() {
            app.on_selection(|s| CommandRequest::flip(s, true));
        }
        if ui.button(t("ui-flip-v")).clicked() {
            app.on_selection(|s| CommandRequest::flip(s, false));
        }
        if ui.button(t("ui-centre")).clicked() {
            app.on_selection(|s| {
                CommandRequest::new("object.center", json!({ "ids": CommandRequest::ids(s) }))
            });
        }
    });
    ui.horizontal(|ui| {
        ui.label(t("ui-scale"));
        ui.add(
            egui::DragValue::new(&mut app.scale_percent)
                .range(1.0..=1000.0)
                .speed(1.0)
                .suffix(" %"),
        );
        if ui.button(t("menu-apply")).clicked() && (app.scale_percent - 100.0).abs() > 1e-9 {
            let f = app.scale_percent / 100.0;
            app.on_selection(|s| {
                CommandRequest::new(
                    "object.scale",
                    json!({ "ids": CommandRequest::ids(s), "sx": f }),
                )
            });
            app.scale_percent = 100.0;
        }
    });
    ui.horizontal(|ui| {
        if ui
            .button(format!("Duplicate ({}D)", crate::app::CMD))
            .clicked()
        {
            app.on_selection(CommandRequest::duplicate);
        }
        if ui.button(t("ui-delete")).clicked() {
            app.on_selection(CommandRequest::delete);
        }
    });
}

fn kind_combo<K: Copy + PartialEq>(
    ui: &mut egui::Ui,
    id: &str,
    current: &Common<K>,
    options: &[(K, &str)],
    enabled: bool,
) -> Option<K> {
    let text = match current {
        Common::Same(k) => options.iter().find(|o| o.0 == *k).map_or("?", |o| o.1),
        Common::Mixed => "Mixed",
        Common::Empty => "—",
    };
    let mut chosen = None;
    ui.add_enabled_ui(enabled, |ui| {
        let r = egui::ComboBox::from_id_salt(id)
            .selected_text(text)
            .show_ui(ui, |ui| {
                for &(k, label) in options {
                    if ui
                        .selectable_label(current.same() == Some(&k), label)
                        .clicked()
                    {
                        chosen = Some(k);
                    }
                }
            });
        let name = if id.starts_with("line") {
            "Line sewing type"
        } else {
            "Region sewing type"
        };
        crate::a11y::name(&r.response, name);
    });
    chosen.filter(|k| current.same() != Some(k))
}

/// A section title that folds the section (#553; kept in ui.json). A search shows every
/// section. Returns whether the section is folded.
fn folded_header(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    key: &str,
    title: &str,
    filter: &str,
) -> bool {
    let folded = app.ui.prefs.folded(key) && filter.trim().is_empty();
    let mut text = egui::text::LayoutJob::default();
    RichText::new(title).strong().append_to(
        &mut text,
        ui.style(),
        egui::FontSelection::Default,
        egui::Align::Center,
    );
    if folded {
        RichText::new(t("ui-folded-click-to-show")).weak().append_to(
            &mut text,
            ui.style(),
            egui::FontSelection::Default,
            egui::Align::Center,
        );
    }
    if ui
        .add(egui::Button::new(text).frame(false))
        .on_hover_text(t("ui-fold-or-unfold-this-section-tip"))
        .clicked()
    {
        let now = !app.ui.prefs.folded(key);
        app.ui.prefs.set_folded(key, now);
        app.ui.save_prefs();
    }
    folded
}

pub fn region_section(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    m: &InspectorModel,
    out: &mut Vec<CommandRequest>,
) {
    let filter = app.ui.property_search.clone();
    let show = |texts: &[&str]| inspector::matches_filter(&filter, texts);
    let any = filter.trim().is_empty()
        || m.region.as_ref().is_some_and(|r| {
            inspector::region_controls_with(r, &app.session.design().patterns)
                .iter()
                .any(|c| c.shown(&filter))
        })
        || show(&["Region sewing type", "regionType"])
        || show(&["Colour", "color thread"])
        || show(&["Hole sewing", "holeSewing"]);
    if !any {
        return;
    }
    ui.separator();
    if folded_header(app, ui, "region", "Region sewing", &filter) {
        return;
    }
    if !m.closed {
        ui.label(RichText::new(t("ui-needs-a-closed-outline")).weak());
        return;
    }
    egui::Grid::new("region")
        .num_columns(2)
        .spacing(egui::vec2(8.0, 6.0))
        .show(ui, |ui| {
            if show(&["Region sewing type", "regionType"]) {
                ui.label(t("ui-type"));
                let picked = if app.ui.prefs.picture_pickers {
                    region_type_picker(app, ui, m)
                } else {
                    kind_combo::<Option<RegionType>>(
                        ui,
                        "region-type",
                        &m.region_kind,
                        &inspector::region_types(),
                        true,
                    )
                };
                if let Some(k) = picked {
                    out.push(m.set_region_type(k));
                }
                ui.end_row();
            }
            let Some(r) = m.region.clone() else { return };
            if show(&["Colour", "color thread"]) {
                ui.label(t("ui-colour"));
                let colour = if all_same_region_thread(app, m) {
                    Some(&r.thread)
                } else {
                    None
                };
                if let Some(i) =
                    thread_picker(ui, "region-colour", colour, app.session.user_settings())
                {
                    out.push(m.set_region(json!({ "color": i })));
                }
                ui.end_row();
            }
            let user = app.session.design().patterns.clone();
            // Text has no Running Stitch Path (guide).
            let text = app
                .session
                .design()
                .objects
                .iter()
                .any(|o| m.shapes.contains(&o.id) && o.kind == ObjectKind::Text);
            for c in inspector::region_controls_with(&r, &user) {
                if (text && c.param() == "runningStitchPath")
                    || !c.shown(&filter)
                    || !(app.ui.prefs.advanced_attributes || c.basic())
                {
                    continue;
                }
                if app.ui.prefs.picture_pickers
                    && let Some(v) = pattern_picker(app, ui, &c, r.kind, m)
                {
                    out.push(m.set_region(c.params(json!(v))));
                    continue;
                }
                if app.ui.prefs.picture_pickers && c.param() == "pattern" {
                    continue;
                }
                if let Some(v) = control(app, ui, &c) {
                    out.push(m.set_region(c.params(v)));
                }
            }
            if !show(&["Hole sewing", "holeSewing"]) {
                return;
            }
            ui.label(t("ui-hole-sewing"));
            let mut hole = matches!(m.hole_sewing, Common::Same(true));
            let text = if m.hole_sewing == Common::Mixed {
                "Mixed"
            } else {
                "Skip shapes sewn on top"
            };
            if ui.checkbox(&mut hole, text).changed()
                && let Some(r) = m.set_hole_sewing(app.session.design(), hole)
            {
                out.push(r);
            }
            ui.end_row();
        });
}

pub fn all_same_region_thread(app: &EditorApp, m: &InspectorModel) -> bool {
    let d = app.session.design();
    let threads = d
        .objects
        .iter()
        .filter(|o| m.shapes.contains(&o.id))
        .filter_map(|o| o.region.as_ref().map(|r| r.thread.clone()));
    matches!(Common::of(threads), Common::Same(_))
}

pub fn all_same_line_thread(app: &EditorApp, m: &InspectorModel) -> bool {
    let d = app.session.design();
    let threads = d
        .objects
        .iter()
        .filter(|o| m.shapes.contains(&o.id))
        .filter_map(|o| o.line.as_ref().map(|l| l.thread.clone()));
    matches!(Common::of(threads), Common::Same(_))
}

pub fn line_section(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    m: &InspectorModel,
    out: &mut Vec<CommandRequest>,
) {
    let filter = app.ui.property_search.clone();
    let show = |texts: &[&str]| inspector::matches_filter(&filter, texts);
    let any = filter.trim().is_empty()
        || m.line.as_ref().is_some_and(|l| {
            inspector::line_controls_with(l, &app.session.design().patterns)
                .iter()
                .any(|c| c.shown(&filter))
        })
        || show(&["Line sewing type", "lineType"])
        || show(&["Colour", "color thread"]);
    if !any {
        return;
    }
    ui.separator();
    if folded_header(app, ui, "line", "Line sewing", &filter) {
        return;
    }
    egui::Grid::new("line")
        .num_columns(2)
        .spacing(egui::vec2(8.0, 6.0))
        .show(ui, |ui| {
            if show(&["Line sewing type", "lineType"]) {
                ui.label(t("ui-type"));
                let picked = if app.ui.prefs.picture_pickers {
                    line_type_picker(app, ui, m)
                } else {
                    kind_combo::<Option<LineType>>(
                        ui,
                        "line-type",
                        &m.line_kind,
                        &inspector::line_types(),
                        true,
                    )
                };
                if let Some(k) = picked {
                    out.push(m.set_line_type(k));
                }
                ui.end_row();
            }
            let Some(l) = m.line.clone() else { return };
            if show(&["Colour", "color thread"]) {
                ui.label(t("ui-colour"));
                let colour = if all_same_line_thread(app, m) {
                    Some(&l.thread)
                } else {
                    None
                };
                if let Some(i) =
                    thread_picker(ui, "line-colour", colour, app.session.user_settings())
                {
                    out.push(m.set_line(json!({ "color": i })));
                }
                ui.end_row();
            }
            let user = app.session.design().patterns.clone();
            for c in inspector::line_controls_with(&l, &user) {
                if !c.shown(&filter) || !(app.ui.prefs.advanced_attributes || c.basic()) {
                    continue;
                }
                if app.ui.prefs.picture_pickers && c.param() == "motif" {
                    if let Some(v) = motif_picker(app, ui, &c, m) {
                        out.push(m.set_line(c.params(json!(v))));
                    }
                    continue;
                }
                if let Some(v) = control(app, ui, &c) {
                    out.push(m.set_line(c.params(v)));
                }
            }
        });
}

/// Selected live objects whose needle points were edited by hand (#501): say so and offer
/// Regenerate, which sews their generated stitches again.
fn hand_edits_row(app: &mut EditorApp, ui: &mut egui::Ui, out: &mut Vec<CommandRequest>) {
    let d = app.session.design();
    let ids: Vec<u64> = d
        .objects
        .iter()
        .filter(|o| app.selection.contains(&o.id) && o.hand_edits.is_some())
        .map(|o| o.id)
        .collect();
    let table = crate::stitch_table_ui::target(app).is_some();
    if ids.is_empty() && !table {
        return;
    }
    ui.separator();
    ui.horizontal_wrapped(|ui| {
        if table
            && ui
                .button(t("ui-stitch-table"))
                .on_hover_text(t("ui-every-needle-point-with-its-exact-tip"))
                .clicked()
        {
            app.stitch_table.open = true;
        }
        if ids.is_empty() {
            return;
        }
        ui.label(RichText::new(t("ui-stitches-edited-by-hand")).strong());
        if ui
            .button(t("ui-regenerate"))
            .on_hover_text(t("ui-drop-the-hand-edits-and-sew-tip"))
            .clicked()
        {
            out.push(CommandRequest::new(
                "object.regenerate",
                json!({ "ids": ids }),
            ));
        }
    });
}

/// Region sewing type as pictures (#656): each type's stitches on a sample shape.
fn region_type_picker(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    m: &InspectorModel,
) -> Option<Option<RegionType>> {
    let types = inspector::region_types();
    let current = match &m.region_kind {
        Common::Same(k) => inspector::region_name(*k).to_string(),
        _ => String::new(),
    };
    let tiles: Vec<Tile> = types
        .iter()
        .map(|&(k, label)| Tile {
            value: inspector::region_name(k).to_string(),
            label: label.to_string(),
            sample: Sample::Region(json!({ "type": inspector::region_name(k) })),
        })
        .collect();
    let v = pickers_ui::picker(app, ui, "region-type", &current, &tiles, &|v| {
        m.set_region(json!({ "type": v }))
    })?;
    types
        .into_iter()
        .find(|&(k, _)| inspector::region_name(k) == v)
        .map(|(k, _)| k)
}

/// Line sewing type as pictures (#656).
fn line_type_picker(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    m: &InspectorModel,
) -> Option<Option<LineType>> {
    let types = inspector::line_types();
    let current = match &m.line_kind {
        Common::Same(k) => inspector::line_name(*k).to_string(),
        _ => String::new(),
    };
    let tiles: Vec<Tile> = types
        .iter()
        .map(|&(k, label)| Tile {
            value: inspector::line_name(k).to_string(),
            label: label.to_string(),
            sample: Sample::Line(json!({ "type": inspector::line_name(k) })),
        })
        .collect();
    let v = pickers_ui::picker(app, ui, "line-type", &current, &tiles, &|v| {
        m.set_line(json!({ "type": v }))
    })?;
    types
        .into_iter()
        .find(|&(k, _)| inspector::line_name(k) == v)
        .map(|(k, _)| k)
}

/// A programmable fill or net pattern as pictures (#656): one grid row, the new pattern id.
fn pattern_picker(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    c: &Control,
    kind: RegionType,
    m: &InspectorModel,
) -> Option<String> {
    let Control::Choice {
        label,
        param: "pattern",
        value,
        options,
    } = c
    else {
        return None;
    };
    ui.label(*label);
    let tiles: Vec<Tile> = options
        .iter()
        .map(|(v, l)| Tile {
            value: v.clone(),
            label: l.clone(),
            sample: Sample::Region(json!({ "type": kind.id(), "pattern": v })),
        })
        .collect();
    let picked = pickers_ui::picker(app, ui, "region-pattern", value, &tiles, &|v| {
        m.set_region(json!({ "pattern": v }))
    });
    ui.end_row();
    picked
}

/// A motif as pictures (#656).
fn motif_picker(
    app: &mut EditorApp,
    ui: &mut egui::Ui,
    c: &Control,
    m: &InspectorModel,
) -> Option<String> {
    let Control::Choice {
        label,
        value,
        options,
        ..
    } = c
    else {
        return None;
    };
    ui.label(*label);
    let tiles: Vec<Tile> = options
        .iter()
        .map(|(v, l)| Tile {
            value: v.clone(),
            label: l.clone(),
            sample: Sample::Line(json!({ "type": LineType::Motif.id(), "motif": v })),
        })
        .collect();
    let picked = pickers_ui::picker(app, ui, "line-motif", value, &tiles, &|v| {
        m.set_line(json!({ "motif": v }))
    });
    ui.end_row();
    picked
}

/// Shows one attribute control (label and widget, one grid row); returns a new value.
fn control(app: &mut EditorApp, ui: &mut egui::Ui, c: &Control) -> Option<Value> {
    let mut changed = None;
    match c {
        Control::Number {
            label,
            key,
            value,
            range,
            speed,
            suffix,
            ..
        } => {
            ui.label(*label);
            changed = number(app, ui, key, *value, *range, *speed, suffix).map(|v| json!(v));
        }
        Control::Choice {
            label,
            param,
            value,
            options,
        } => {
            ui.label(*label);
            let text = options
                .iter()
                .find(|o| o.0 == *value)
                .map_or(value.as_str(), |o| o.1.as_str());
            let r = egui::ComboBox::from_id_salt(*param)
                .selected_text(text)
                .show_ui(ui, |ui| {
                    for (v, l) in options {
                        if ui.selectable_label(v == value, l).clicked() && v != value {
                            changed = Some(json!(v));
                        }
                    }
                });
            crate::a11y::name(&r.response, label);
        }
        Control::Toggle { label, value, .. } => {
            ui.label("");
            let mut on = *value;
            if ui.checkbox(&mut on, *label).changed() {
                changed = Some(json!(on));
            }
        }
    }
    ui.end_row();
    changed
}

// MARK: simulator

/// The simulator's transport: Play/Pause, start, previous and next stitch, end, a slider, the
/// position and the speed. `pos` is the stitch reached (`None`: all sewn). Shared by the
/// editor's Stitch Simulator pane and the Stitch Design Factory's simulator.
pub fn sim_controls(
    ui: &mut egui::Ui,
    playing: &mut bool,
    pos: &mut Option<usize>,
    speed: &mut f64,
    total: usize,
) {
    ui.horizontal(|ui| {
        let play = if *playing { "Pause" } else { "Play" };
        if ui.add_enabled(total > 0, egui::Button::new(play)).clicked() {
            *playing = !*playing;
            if *playing && pos.is_none() {
                *pos = Some(0);
            }
        }
        // #653: the player's buttons are Higgsfield icons, not ASCII marks.
        if ui
            .add_enabled(
                total > 0,
                egui::Button::new(crate::icons::ui_image(ui, "player_start")),
            )
            .on_hover_text(t("ui-start-tip"))
            .clicked()
        {
            *pos = Some(0);
        }
        if ui
            .add_enabled(
                total > 0,
                egui::Button::new(crate::icons::ui_image(ui, "player_back")),
            )
            .on_hover_text(t("ui-previous-stitch-tip"))
            .clicked()
        {
            *pos = Some(pos.unwrap_or(total).saturating_sub(1));
        }
        if ui
            .add_enabled(
                total > 0,
                egui::Button::new(crate::icons::ui_image(ui, "player_forward")),
            )
            .on_hover_text(t("ui-next-stitch-tip"))
            .clicked()
        {
            let n = pos.map_or(total, |n| n + 1);
            *pos = (n < total).then_some(n);
        }
        if ui
            .add_enabled(
                total > 0,
                egui::Button::new(crate::icons::ui_image(ui, "player_end")),
            )
            .on_hover_text(t("ui-end-tip"))
            .clicked()
        {
            *pos = None;
            *playing = false;
        }
        let mut n = pos.unwrap_or(total);
        let slider = egui::Slider::new(&mut n, 0..=total.max(1)).show_value(false);
        let width = (ui.available_width() - 260.0).max(120.0);
        ui.spacing_mut().slider_width = width;
        let r = ui.add_enabled(total > 0, slider);
        crate::a11y::name(&r, "Stitch Simulator position");
        if r.changed() {
            *pos = (n < total).then_some(n);
            *playing = false;
        }
        ui.label(format!("{} / {}", pos.unwrap_or(total), total));
        let r = egui::ComboBox::from_id_salt("speed")
            .selected_text(format!("{:.0}/s", *speed))
            .width(70.0)
            .show_ui(ui, |ui| {
                for s in SIM_SPEEDS {
                    ui.selectable_value(speed, s, format!("{s:.0}/s"));
                }
            });
        crate::a11y::name(&r.response, "Simulator speed, stitches per second");
    });
}

/// Simulator speeds (stitches per second): the player's presets, 12 to 5,000 (#516).
pub const SIM_SPEEDS: [f64; 7] = openstitch_app::editor::player::SPEEDS;

pub fn simulator(app: &mut EditorApp, ui: &mut egui::Ui) {
    let total = app.stats.stitches;
    ui.add_space(4.0);
    sim_controls(
        ui,
        &mut app.playing,
        &mut app.simulate,
        &mut app.play_speed,
        total,
    );
    player_strip(app, ui);
    ui.horizontal(|ui| {
        let s = &app.stats;
        ui.label(format!(
            "Stitches {}   Jumps {}   Trims {}   Colour changes {}   Size {}",
            s.stitches,
            s.jumps,
            s.trims,
            s.color_changes,
            openstitch_app::editor::units::size(app.ui.prefs.units, s.width_mm, s.height_mm)
        ));
        if let Some(e) = &app.workspace.estimate {
            ui.separator();
            ui.label(crate::workspace_ui::estimate_text(e));
        }
        ui.separator();
        let current = app.simulate.map(|n| color_at(&app.pattern, n.max(1)).0);
        for (i, &c) in s.colors.iter().enumerate() {
            let th = app
                .pattern
                .threads
                .get(i)
                .cloned()
                .unwrap_or_else(|| Thread::rgb(u32::from_be_bytes([0, c[0], c[1], c[2]])));
            let r = thread_swatch(ui, &th, 14.0, Caption::Under);
            if current == Some(i) {
                ui.painter().rect_stroke(
                    r.rect.expand(2.0),
                    egui::CornerRadius::same(3),
                    egui::Stroke::new(2.0, Color32::from_rgb(0xff, 0x3d, 0x00)),
                    egui::StrokeKind::Outside,
                );
            }
        }
        if let Some(n) = app.simulate {
            let (c, blocks) = color_at(&app.pattern, n.max(1));
            ui.label(format!("colour {} of {}", c + 1, blocks));
        }
        ui.separator();
        ui.checkbox(&mut app.show_travel, t("ui-show-travel"));
    });
    ui.add_space(4.0);
}

/// The stitch-out player (#516, beyond PE-DESIGN): counters, step buttons, the Auto, Pause at
/// trims and Follow needle toggles, one lane per colour block with trim ticks and an end marker
/// at each colour change or stop (click or drag to seek), and the threads by name to jump to.
fn player_strip(app: &mut EditorApp, ui: &mut egui::Ui) {
    use openstitch_app::editor::player::{self, Step};
    use openstitch_design::timeline::EndKind;
    if app.timeline.total == 0 {
        return;
    }
    // Wraps onto a second row when the window is narrow or the text large (#499).
    ui.horizontal_wrapped(|ui| {
        ui.monospace(player::counter(&app.timeline, app.simulate))
            .on_hover_text(t("ui-stitches-and-machine-time-at-the-tip"));
        ui.separator();
        for (label, s, tip) in [
            ("Prev block", Step::BlockBack, "Shift+←"),
            ("Next block", Step::BlockForward, "Shift+→"),
            ("Prev object", Step::ObjectBack, "Ctrl+←"),
            ("Next object", Step::ObjectForward, "Ctrl+→"),
            ("Next trim", Step::NextTrim, "T"),
        ] {
            if ui.small_button(label).on_hover_text(tip).clicked() {
                // From the whole design shown, forward steps start at the first stitch.
                let forward =
                    matches!(s, Step::BlockForward | Step::ObjectForward | Step::NextTrim);
                let from = if forward {
                    app.simulate.or(Some(0))
                } else {
                    app.simulate
                };
                app.simulate = player::step(&app.timeline, from, s);
                app.playing = false;
            }
        }
        ui.separator();
        ui.toggle_value(&mut app.stitch_player.auto, "Auto")
            .on_hover_text(t("ui-play-the-whole-design-in-about-tip"));
        ui.toggle_value(&mut app.stitch_player.pause, "Pause at trims")
            .on_hover_text(
                "Stop playing at each trim, stop and colour change, as the machine does",
            );
        ui.toggle_value(&mut app.stitch_player.follow, "Follow needle")
            .on_hover_text(t("ui-keep-the-needle-in-view-while-tip"));
    });
    let t = &app.timeline;
    let n = player::at(t, app.simulate);
    let (rect, resp) = ui.allocate_exact_size(
        egui::vec2(ui.available_width(), 22.0),
        Sense::click_and_drag(),
    );
    let painter = ui.painter_at(rect);
    let x = |k: usize| rect.left() + rect.width() * k as f32 / t.total as f32;
    let colour = |i: usize| {
        app.pattern
            .threads
            .get(i)
            .map_or(Color32::GRAY, |c| Color32::from_rgb(c.red, c.green, c.blue))
    };
    for b in &t.blocks {
        let c = colour(b.thread);
        let lane = egui::Rect::from_x_y_ranges(
            x(b.start)..=x(b.end),
            rect.top() + 4.0..=rect.bottom() - 4.0,
        );
        painter.rect_filled(lane, 0.0, c.gamma_multiply(0.3));
        let sewn = n.clamp(b.start, b.end);
        if sewn > b.start {
            let done = egui::Rect::from_x_y_ranges(x(b.start)..=x(sewn), lane.y_range());
            painter.rect_filled(done, 0.0, c);
        }
        for &tr in &b.trims {
            painter.line_segment(
                [
                    egui::pos2(x(tr), rect.top()),
                    egui::pos2(x(tr), rect.top() + 7.0),
                ],
                egui::Stroke::new(1.0, ui.visuals().text_color()),
            );
        }
        if b.end_kind != EndKind::End {
            let mark = match b.end_kind {
                EndKind::Stop => Color32::from_rgb(0xd3, 0x2f, 0x2f),
                _ => ui.visuals().strong_text_color(),
            };
            painter.line_segment(
                [
                    egui::pos2(x(b.end), rect.top()),
                    egui::pos2(x(b.end), rect.bottom()),
                ],
                egui::Stroke::new(2.0, mark),
            );
        }
    }
    painter.line_segment(
        [
            egui::pos2(x(n), rect.top()),
            egui::pos2(x(n), rect.bottom()),
        ],
        egui::Stroke::new(2.0, Color32::from_rgb(0xff, 0x3d, 0x00)),
    );
    if (resp.clicked() || resp.dragged())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let f = f64::from((p.x - rect.left()) / rect.width().max(1.0));
        app.simulate = player::seek(&app.timeline, f);
        app.playing = false;
    }
    resp.on_hover_text(openstitch_app::editor::ui_texts::t("ui-colour-blocks-in-sewing-order-ticks-tip"));
    let jumps: Vec<(String, Color32, usize)> = app
        .timeline
        .blocks
        .iter()
        .enumerate()
        .map(|(i, b)| {
            let name = app
                .pattern
                .threads
                .get(b.thread)
                .and_then(|c| c.name.as_deref())
                .unwrap_or("Thread");
            (format!("{}. {name}", i + 1), colour(b.thread), b.start)
        })
        .collect();
    ui.horizontal_wrapped(|ui| {
        for (label, c, start) in jumps {
            let r = ui.add(
                egui::Button::new(RichText::new(label).small()).stroke(egui::Stroke::new(2.0, c)),
            );
            if r.on_hover_text(openstitch_app::editor::ui_texts::t("ui-go-to-the-first-stitch-of-tip"))
                .clicked()
            {
                app.simulate = player::position(&app.timeline, start);
                app.playing = false;
            }
        }
    });
}

#[cfg(test)]
mod pec_search_tests {
    use super::*;

    #[test]
    fn pec_picker_search_finds_threads_by_number_or_name() {
        assert_eq!(pec_matches("").len(), inspector::pec_palette().len());
        let rose = pec_matches("deep rose");
        assert!(!rose.is_empty());
        assert!(rose.iter().all(|(_, t)| {
            t.name
                .as_deref()
                .unwrap_or("")
                .to_lowercase()
                .contains("deep rose")
        }));
        // A chart number finds its own thread, and so does its Brother Embroidery code, as
        // `threads.find {charts: ["brother"]}` does.
        let (i, _) = rose[0];
        assert!(pec_matches(&i.to_string()).iter().any(|(j, _)| *j == i));
        let brother = openstitch_engine::thread_match::builtins()
            .into_iter()
            .find(|b| b.id == "brother")
            .expect("the Brother chart");
        let (_, entry) = brother
            .entries
            .iter()
            .find(|(j, _)| *j == i)
            .expect("entry");
        let code = entry.catalog_number.as_deref().expect("a code");
        assert!(pec_matches(code).iter().any(|(j, _)| *j == i));
        assert!(pec_matches("no such thread").is_empty());
    }

    /// #798's guard: no thread colour is drawn as a bare swatch outside [`thread_swatch`]. Each
    /// bare swatch (`swatch(`, a thread's own `from_rgb(t.red…`, a tinted swatch image) needs a
    /// `bare-swatch:` reason on its line or the two lines above: a colour that is not a thread,
    /// stitches drawn in their colour, or a label beside it that carries the code.
    #[test]
    fn no_thread_swatch_without_its_code() {
        // Built from parts so this test's own lines do not match.
        let bare_call = concat!("sw", "atch(");
        let rgb = [concat!("from_rgb(t", ".red"), concat!("from_rgb(th", ".red"), concat!("from_rgb(thread", ".red")];
        let tint = concat!("tint(Color32::", "from_rgb(");
        let matches = |code: &str| {
            let call = code.match_indices(bare_call).any(|(i, _)| {
                !code[..i]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c == '_' || c.is_ascii_alphanumeric())
            });
            (call && !code.contains(concat!("fn sw", "atch(")))
                || rgb.iter().any(|r| code.contains(r))
                || code.contains(tint)
        };
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        let mut files = Vec::new();
        let mut dirs = vec![root.join("src"), root.join("../openstitch-web/src")];
        while let Some(d) = dirs.pop() {
            for e in std::fs::read_dir(&d).into_iter().flatten().flatten() {
                let p = e.path();
                if p.is_dir() {
                    dirs.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    files.push(p);
                }
            }
        }
        assert!(files.len() > 50, "the sources were found");
        let mut bare = Vec::new();
        for f in files {
            let text = std::fs::read_to_string(&f).expect("source");
            let lines: Vec<&str> = text.lines().collect();
            for (n, l) in lines.iter().enumerate() {
                let code = l.split("//").next().unwrap_or("");
                let reason = (n.saturating_sub(2)..=n).any(|k| lines[k].contains("bare-swatch:"));
                if matches(code) && !reason {
                    bare.push(format!("{}:{}: {}", f.display(), n + 1, l.trim()));
                }
            }
        }
        assert!(
            bare.is_empty(),
            "thread swatches without their code (use thread_swatch, or give a bare-swatch: reason):\n{}",
            bare.join("\n")
        );
    }

    /// The shared widget's label (#798): Brother 007 for PEC 1, a maker's and a user chart's own
    /// code, the hex for a custom colour.
    #[test]
    fn thread_swatch_labels_are_the_threads_own_codes() {
        let prussian = openstitch_core::PEC_CHART[1].clone().expect("entry 1");
        assert_eq!(label_json(&prussian)["code"], "007");
        assert_eq!(label_json(&prussian)["brand"], "Brother");
        let isacord = openstitch_core::brands::brand_charts()
            .into_iter()
            .find(|c| c.0 == "isacord40")
            .expect("Isacord")
            .2[3]
            .clone();
        assert_eq!(label_json(&isacord)["code"], json!(isacord.catalog_number.as_deref()));
        let mine = json_thread(&json!({"color": "#123456", "code": "A7", "brand": "Mine"}));
        assert_eq!(label_json(&mine)["code"], "A7");
        assert_eq!(label_json(&Thread::rgb(0x123456))["code"], "#123456");
        assert!(thread_tip(&prussian).starts_with("Brother 007 Prussian Blue"), "{}", thread_tip(&prussian));
        // Searching the PEC tab by the code it shows finds the thread.
        assert!(pec_matches("007").iter().any(|(i, _)| *i == 1));
    }

    /// #796: the colour picker starts from New Brothread on a first run, and its search finds a
    /// thread by its code, its name or a Madeira code; a chosen thread is given as
    /// `{builtin, code}`.
    #[test]
    fn chart_picker_search_finds_new_brothread_by_code_name_or_madeira() {
        let user = openstitch_engine::user::UserSettings::first_run();
        let default = user.default_chart();
        let (name, threads) = picker_chart(&default, &user);
        assert!(name.starts_with("New Brothread"), "{name}");
        assert_eq!(threads.len(), 63);
        let code = |i: usize| threads[i].catalog_number.as_deref().unwrap_or_default();
        assert_eq!(chart_matches(&threads, "").len(), 63);
        assert_eq!(code(chart_matches(&threads, "001")[0]), "001");
        assert_eq!(code(chart_matches(&threads, "901")[0]), "901");
        // A name finds every thread with it, in card order: 124 Flesh Pink, then 901 Flesh.
        let flesh: Vec<&str> = chart_matches(&threads, "flesh")
            .into_iter()
            .map(code)
            .collect();
        assert_eq!(flesh, ["124", "901"]);
        assert_eq!(code(chart_matches(&threads, "1801")[0]), "001", "Madeira Polyneon");
        assert_eq!(code(chart_matches(&threads, "1002")[0]), "001", "Madeira Rayon");
        assert!(chart_matches(&threads, "no such thread").is_empty());
        let white = &threads[0];
        assert!(thread_tip(white).contains("Madeira Polyneon 1801"), "{}", thread_tip(white));
        assert_eq!(
            colour_param(&default, 0, white),
            json!({ "builtin": "newBrothread", "code": "001" })
        );
        let mine = openstitch_engine::user::ChartRef::Chart("My Threads".into());
        assert_eq!(colour_param(&mine, 0, white)["code"], "001");
        assert_eq!(colour_param(&mine, 0, white)["color"], "#f0f0f0");
    }
}

/// #745: the Sewing Order list's filter, driven by a `--script` as the window plays one.
#[cfg(test)]
mod order_filter_tests {
    use super::*;
    use crate::app::Launch;
    use openstitch_app::editor::Viewport;
    use openstitch_design::v2;

    /// Plays `script` on the Sewing Order list alone (a 320 x 600 point pane).
    fn play(app: &mut EditorApp, script: &str) {
        let ctx = egui::Context::default();
        let frame = |app: &mut EditorApp, events: Vec<egui::Event>, m: egui::Modifiers| {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(320.0, 600.0),
                )),
                events: std::iter::once(egui::Event::ModifiersChanged(m))
                    .chain(events)
                    .collect(),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| object_list(app, ui));
            out.textures_delta.clear();
        };
        for _ in 0..2 {
            frame(app, Vec::new(), egui::Modifiers::NONE);
        }
        let vp = Viewport { scale: 1.0, offset: v2(0.0, 0.0) };
        let mut player = crate::script::Player::new(crate::script::parse(script).expect("script"));
        while let Some((events, m)) = player.next_frame(&vp) {
            frame(app, events, m);
        }
        frame(app, Vec::new(), egui::Modifiers::NONE);
    }

    fn add(app: &mut EditorApp, command: &'static str, params: Value) -> u64 {
        let r = app.execute(&CommandRequest::new(command, params));
        r.unwrap_or_else(|| panic!("{command}: {}", app.status))["id"]
            .as_u64()
            .expect("id")
    }

    /// Typing narrows the list, Enter selects exactly the rows shown, Esc brings every row
    /// back; the design, its order and its undo history never change.
    #[test]
    fn find_narrows_the_list_and_selects_the_rows_shown() {
        let mut app = EditorApp::new(Launch::default());
        add(&mut app, "shape.rectangle", json!({"x": 10, "y": 10, "width": 20, "height": 10, "fill": "Red"}));
        let rim = add(&mut app, "shape.ellipse", json!({"cx": 60, "cy": 20, "rx": 8, "ry": 8, "outline": "Blue"}));
        app.execute(&CommandRequest::new("object.setLine", json!({"id": rim, "type": "satin", "width": 2})))
            .expect("satin");
        let bar = add(&mut app, "shape.rectangle", json!({"x": 10, "y": 40, "width": 30, "height": 4, "outline": "Emerald Green"}));
        app.execute(&CommandRequest::new("object.setLine", json!({"id": bar, "type": "satin", "width": 3})))
            .expect("satin");
        add(&mut app, "text.add", json!({"text": "Ana", "size": 10, "x": 20, "y": 60, "fill": "Black"}));
        let design = app.session.design().clone();
        let undo = app.session.can_undo();
        let undo_groups = app.undo_groups.len();

        // Ctrl+F over the list, then the query: Enter selects the satin outlines and the
        // lettering (sewn as satin), not the filled rectangle.
        play(&mut app, "sclick:160,590;cmd+key:F;type:satin;key:Enter");
        assert_eq!(app.edit.order_filter, "satin");
        let text = app.session.design().objects[3].id;
        assert_eq!(app.selection, BTreeSet::from([rim, bar, text]));
        // Another query: selecting what it shows replaces the selection.
        app.edit.order_filter = "text".into();
        play(&mut app, "sclick:160,590;cmd+key:F;key:Enter");
        assert_eq!(app.selection, BTreeSet::from([text]));
        // Esc in the field clears it: every row in sewing order again.
        play(&mut app, "sclick:160,590;cmd+key:F;key:Escape");
        assert!(app.edit.order_filter.is_empty());
        assert_eq!(*app.session.design(), design, "filtering never edits the design");
        assert_eq!(app.session.can_undo(), undo);
        assert_eq!(app.undo_groups.len(), undo_groups, "nor its undo history");
    }
}
