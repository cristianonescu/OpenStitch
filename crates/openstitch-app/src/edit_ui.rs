//! Shapes menu (presets, arcs, merge, remove overlap, combine, split, offset lines, entry/exit
//! points), the group / arrange / attribute items of the Edit menu, their dialogs and canvas
//! markers. Requests come from `openstitch_app::editor::edit_ops`; each action is one command,
//! so one Undo.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, Color32, Pos2, Stroke};
use openstitch_app::editor::edit_ops::{
    self, ArrangeForm, ArrangeMode, EntryExit, FloralForm, OffsetForm, PATH_SIDES, Pick,
};
use openstitch_app::editor::hit::bounds_of;
use openstitch_app::editor::objects;
use openstitch_app::editor::tools::snap_to_grid;
use openstitch_app::editor::{CommandRequest, Phase, SelectionHandles, Tool, ToolEffect, Viewport};
use openstitch_design::{Contour, MachineType, ObjectKind, OptimizeOrder, Vec2};
use std::collections::BTreeSet;

/// Editor state for these operations.
#[derive(Default)]
pub struct EditState {
    pub arrange_open: bool,
    pub arrange: ArrangeForm,
    pub offset_open: bool,
    pub offset: OffsetForm,
    pub floral_open: bool,
    pub floral: FloralForm,
    /// Decorative frames keep their own (square) proportions.
    pub frame_keep_aspect: bool,
    /// Source object of Copy Attributes.
    pub attributes_from: Option<u64>,
    /// The next canvas click sets this point of this object.
    pub pick: Option<(u64, Pick)>,
    /// The next open path drawn with the Pen tool becomes this shape's curved direction.
    pub curve_target: Option<u64>,
    /// The next canvas click places this stamp (object, pattern, effect) there.
    pub stamp_pick: Option<(u64, &'static str, &'static str)>,
    /// Stamp pattern, effect and size (mm) chosen in the Stamp menu.
    pub stamp_pattern: usize,
    pub stamp_effect: usize,
    /// Smooth Shape's tolerance, mm (`None`: the default, 0.2 mm).
    pub smooth_tolerance: Option<f64>,
    /// Shape Effects dropdown settings (#497).
    pub effect: openstitch_app::editor::shape_effects::EffectForm,
    /// Turning Gap dropdown: the last offset and length typed (mm).
    pub line_gap: Option<(f64, f64)>,
    /// The next polygon drawn is the cut outline splitting this shape.
    pub split_target: Option<u64>,
    /// Alt held: clicking a group member selects it alone.
    pub alt: bool,
    /// Sewing-order pane shows one frame per same-colour run.
    pub by_colour: bool,
    /// Sewing-order pane: the object being renamed and the name typed so far (#502).
    pub renaming: Option<(u64, String)>,
    /// The Arrange Copies handle is being dragged on the canvas.
    pub arrange_drag: bool,
    /// An entry/exit marker being dragged: object, which marker, where it is now.
    pub marker_drag: Option<(u64, EntryExit, Vec2)>,
}

impl EditState {
    /// Cancels a pending pick or split; whether there was one.
    pub fn cancel(&mut self) -> bool {
        self.pick.take().is_some()
            | self.split_target.take().is_some()
            | self.curve_target.take().is_some()
            | self.stamp_pick.take().is_some()
    }
}

fn single(app: &EditorApp) -> Option<u64> {
    (app.selection.len() == 1)
        .then(|| app.selection.iter().next().copied())
        .flatten()
}

fn kind_of(app: &EditorApp, id: u64) -> Option<ObjectKind> {
    let d = app.session.design();
    d.index_of(id).map(|i| d.objects[i].kind)
}

fn run_and_select(app: &mut EditorApp, r: CommandRequest) {
    if let Some(result) = app.execute(&r) {
        let created = CommandRequest::created_ids(&result);
        if !created.is_empty() {
            app.selection = created;
        }
        app.status = openstitch_app::tf!("ui-status-done", command = r.command);
    }
}

/// Copy Attributes: remembers the single selected shape or text.
pub fn copy_attributes_source(app: &EditorApp) -> Option<u64> {
    single(app).filter(|&id| kind_of(app, id) != Some(ObjectKind::Stitches))
}

pub fn copy_attributes(app: &mut EditorApp) {
    if let Some(source) = copy_attributes_source(app) {
        app.edit.attributes_from = Some(source);
        app.status = openstitch_app::editor::ui_texts::t("ui-status-sewing-attributes-copied-select-the-targets").into();
    }
}

/// The object whose attributes Paste Attributes would give the selection.
pub fn paste_attributes_source(app: &EditorApp) -> Option<u64> {
    app.edit
        .attributes_from
        .filter(|f| app.selection.iter().any(|id| id != f))
}

pub fn paste_attributes(app: &mut EditorApp) {
    if let Some(from) = paste_attributes_source(app) {
        let r = edit_ops::paste_attributes(from, &app.selection);
        app.execute(&r);
    }
}

pub fn group(app: &mut EditorApp) {
    if app.selection.len() >= 2 {
        let r = edit_ops::group(&app.selection);
        app.execute(&r);
    }
}

pub fn ungroup(app: &mut EditorApp) {
    if edit_ops::any_grouped(app.session.design(), &app.selection) {
        let r = edit_ops::ungroup(&app.selection);
        app.execute(&r);
    }
}

pub fn can_ungroup(app: &EditorApp) -> bool {
    edit_ops::any_grouped(app.session.design(), &app.selection)
}

/// The single selected shape object.
pub fn single_shape(app: &EditorApp) -> Option<u64> {
    single(app).filter(|&id| kind_of(app, id) == Some(ObjectKind::Shape))
}

/// The single selected shape or text (Offset Lines, Floral Pattern).
pub fn single_outline(app: &EditorApp) -> Option<u64> {
    single(app).filter(|&id| matches!(kind_of(app, id), Some(ObjectKind::Shape | ObjectKind::Text)))
}

/// The single selected shape with region sewing (sewing direction, centre point).
pub fn single_region(app: &EditorApp) -> Option<u64> {
    single_shape(app).filter(|&id| {
        let d = app.session.design();
        d.index_of(id)
            .is_some_and(|i| d.objects[i].region.is_some())
    })
}

/// Shapes ▸ Insert: preset shapes placed in the middle of the Design Page.
pub fn insert_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    for (label, kind) in edit_ops::PRESETS {
        if ui.button(openstitch_app::editor::ui_texts::tr(label)).clicked() {
            let center = app.session.design().page.center();
            run_and_select(app, edit_ops::insert_preset(kind, center, 30.0));
        }
    }
}

/// Modify Overlap: merge, remove overlap, combine.
pub fn overlap_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let many = app.selection.len() >= 2;
    if ui.add_enabled(many, egui::Button::new(t("ui-merge"))).clicked() {
        let r = edit_ops::merge(&app.selection);
        run_and_select(app, r);
    }
    if ui
        .add_enabled(many, egui::Button::new(t("ui-remove-overlap")))
        .clicked()
    {
        let r = edit_ops::remove_overlap(&app.selection);
        app.execute(&r);
    }
    ui.separator();
    for (label, op) in [
        ("Union", "union"),
        ("Subtract from first", "difference"),
        ("Intersect", "intersection"),
        ("Exclude overlap", "xor"),
        ("Divide", "divide"),
    ] {
        if ui
            .add_enabled(many, egui::Button::new(openstitch_app::editor::ui_texts::tr(label)))
            .clicked()
        {
            let r = edit_ops::combine(app.session.design(), &app.selection, op);
            run_and_select(app, r);
        }
    }
}

/// Split Outline: the next polygon drawn cuts the selected shape.
pub fn split_outline(app: &mut EditorApp) {
    if let Some(shape) = single_shape(app) {
        app.edit.split_target = Some(shape);
        app.tools.set_tool(Tool::Polygon);
        app.status = openstitch_app::editor::ui_texts::t("ui-status-draw-the-cut-outline-return-or").into();
    }
}

pub fn decorative_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.checkbox(&mut app.edit.frame_keep_aspect, t("ui-maintain-aspect-ratio"));
    ui.separator();
    for (label, kind) in edit_ops::FRAMES {
        if ui.button(openstitch_app::editor::ui_texts::tr(label)).clicked() {
            let r = edit_ops::decorative(kind, &app.selection, app.edit.frame_keep_aspect);
            run_and_select(app, r);
        }
    }
}

/// Stamp/Emboss: emboss/engrave with a second shape, or place stamp patterns.
pub fn stamp_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let shape = single_shape(app);
    let two = app.selection.len() == 2;
    for (label, effect) in edit_ops::EMBOSS {
        if ui
            .add_enabled(two, egui::Button::new(openstitch_app::editor::ui_texts::tr(label)))
            .on_hover_text(t("ui-select-the-shape-and-the-shape-tip"))
            .clicked()
            && let Some(r) = edit_ops::emboss(app.session.design(), &app.selection, effect)
        {
            run_and_select(app, r);
        }
    }
    ui.separator();
    ui.add_enabled_ui(shape.is_some(), |ui| {
        egui::ComboBox::from_label("Stamp")
            .selected_text(openstitch_app::editor::ui_texts::tr(openstitch_design::stamp::PATTERNS[app.edit.stamp_pattern]))
            .show_ui(ui, |ui| {
                for (k, p) in openstitch_design::stamp::PATTERNS.iter().enumerate() {
                    ui.selectable_value(&mut app.edit.stamp_pattern, k, openstitch_app::editor::ui_texts::tr(p));
                }
            });
        egui::ComboBox::from_label("Effect")
            .selected_text(openstitch_app::editor::ui_texts::tr(edit_ops::EMBOSS[app.edit.stamp_effect].0))
            .show_ui(ui, |ui| {
                for (k, (label, _)) in edit_ops::EMBOSS.iter().enumerate() {
                    ui.selectable_value(&mut app.edit.stamp_effect, k, openstitch_app::editor::ui_texts::tr(label));
                }
            });
        if ui.button(t("ui-place-stamp-click-in-the-shape")).clicked()
            && let Some(id) = shape
        {
            app.edit.stamp_pick = Some((
                id,
                openstitch_design::stamp::PATTERNS[app.edit.stamp_pattern],
                edit_ops::EMBOSS[app.edit.stamp_effect].1,
            ));
            app.status = openstitch_app::editor::ui_texts::t("ui-status-click-inside-the-shape-to-place").into();
        }
        if ui.button(t("ui-remove-stamps")).clicked()
            && let Some(id) = shape
        {
            app.execute(&edit_ops::clear_stamps(id));
        }
    });
}

pub fn entry_exit_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let Some(id) = single_shape(app) else {
        ui.label(t("ui-select-one-shape"));
        return;
    };
    for (label, which) in [
        ("Set region entry (R1)", EntryExit::RegionEntry),
        ("Set region exit (R2)", EntryExit::RegionExit),
        ("Set line entry (L1)", EntryExit::LineEntry),
        ("Set line exit (L2)", EntryExit::LineExit),
    ] {
        if ui.button(openstitch_app::editor::ui_texts::tr(label)).clicked() {
            app.edit.pick = Some((id, Pick::Point(which)));
            app.status = openstitch_app::tf!("ui-status-click-the-outline-where-should-be", name = which.label());
        }
    }
    if ui.button(t("ui-reset-to-automatic")).clicked() {
        app.execute(&edit_ops::clear_entry_exit(id));
    }
}

/// Add Direction Line: the next open Pen path becomes the region's curved direction.
pub fn add_direction(app: &mut EditorApp) {
    if let Some(id) = single_region(app) {
        app.edit.curve_target = Some(id);
        app.tools.set_tool(Tool::Pen);
        app.status = openstitch_app::editor::ui_texts::t("ui-status-draw-an-open-curve-across-the").into();
    }
}

pub fn delete_direction(app: &mut EditorApp) {
    if let Some(id) = single_region(app) {
        app.execute(&edit_ops::straight_direction(id));
    }
}

/// Centre point and reference point of the region's stitch pattern.
pub fn sewing_points_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let Some(id) = single_region(app) else {
        ui.label(t("ui-select-one-shape-with-region-sewing"));
        return;
    };
    for (label, which) in [
        ("Set centre point", Pick::Center),
        ("Set reference point", Pick::Reference),
    ] {
        if ui.button(openstitch_app::editor::ui_texts::tr(label)).clicked() {
            app.edit.pick = Some((id, which));
            app.status = openstitch_app::tf!("ui-status-click-where-should-be-esc-cancels", name = which.label());
        }
    }
}

/// A canvas click while a point is being picked; whether it was consumed.
pub fn pick(app: &mut EditorApp, at: Vec2) -> bool {
    if let Some((id, pattern, effect)) = app.edit.stamp_pick.take() {
        let r = edit_ops::add_stamp(id, pattern, effect, at, 10.0);
        if app.execute(&r).is_some() {
            app.status = openstitch_app::editor::ui_texts::t("ui-status-stamp-placed").into();
        }
        return true;
    }
    let Some((id, which)) = app.edit.pick.take() else {
        return false;
    };
    let r = which.request(id, at);
    if app.execute(&r).is_some() {
        app.status = openstitch_app::tf!("ui-status-set", name = which.label());
    }
    true
}

/// Canvas drags the editor handles before the tools: the Arrange Copies handle while the
/// dialog is open, and the entry/exit markers of the selected shape (Select tool). A marker
/// drag ends in one `object.setEntryExit`, so one Undo. Whether the event was consumed.
pub fn drag(app: &mut EditorApp, phase: Phase, at: Vec2) -> bool {
    let radius =
        SelectionHandles::RADIUS * 1.4 * app.viewport.unwrap_or_default().units_per_point();
    let snapped = app
        .workspace
        .grid
        .snap_step()
        .map_or(at, |s| snap_to_grid(at, s));
    let bounds = bounds_of(app.session.design(), &app.selection);
    if app.edit.arrange_drag {
        if let (Phase::Dragged | Phase::Up, Some(b)) = (phase, bounds) {
            app.edit.arrange.drag_handle(&b, snapped);
        }
        app.edit.arrange_drag = matches!(phase, Phase::Dragged | Phase::Moved);
        return true;
    }
    if let Some((id, which, _)) = app.edit.marker_drag {
        match phase {
            Phase::Dragged | Phase::Moved => app.edit.marker_drag = Some((id, which, at)),
            Phase::Up => {
                app.edit.marker_drag = None;
                let moved = edit_ops::entry_exit_markers(app.session.design(), id)
                    .into_iter()
                    .any(|(w, p)| w == which && p.distance(at) > radius / 2.0);
                if moved && app.execute(&which.set(id, at)).is_some() {
                    app.status = openstitch_app::tf!("ui-status-moved", name = which.label());
                }
            }
            Phase::Down => app.edit.marker_drag = None,
        }
        return true;
    }
    if phase != Phase::Down {
        return false;
    }
    if app.edit.arrange_open
        && let Some(b) = bounds
        && app
            .edit
            .arrange
            .handle(&b)
            .is_some_and(|h| h.distance(at) <= radius)
    {
        app.edit.arrange_drag = true;
        return true;
    }
    if app.tools.tool() == Tool::Select
        && let Some(id) = single(app)
        && let Some((which, p)) = edit_ops::entry_exit_at(app.session.design(), id, at, radius)
    {
        app.edit.marker_drag = Some((id, which, p));
        return true;
    }
    false
}

/// Tool effects with the editor's own rules: selections take whole groups (unless Alt), and a
/// polygon drawn for Split with Outline becomes the split.
pub fn filter_effect(app: &mut EditorApp, e: ToolEffect) -> ToolEffect {
    match e {
        ToolEffect::Select(sel) if app.tools.tool() == Tool::Select => {
            let d = app.session.design();
            ToolEffect::Select(edit_ops::group_selection(
                d,
                &app.selection,
                sel,
                app.edit.alt,
            ))
        }
        ToolEffect::Execute {
            requests,
            select_result,
        } => match (app.edit.split_target, app.edit.curve_target) {
            (_, Some(id)) if requests.len() == 1 && requests[0].command == "shape.path" => {
                app.edit.curve_target = None;
                app.tools.set_tool(Tool::Select);
                match edit_ops::direction_curve(id, &requests[0]) {
                    Some(r) => ToolEffect::Execute {
                        requests: vec![r],
                        select_result: false,
                    },
                    None => {
                        app.status = openstitch_app::editor::ui_texts::t("ui-status-the-direction-must-be-an-open").into();
                        ToolEffect::Select(BTreeSet::from([id]))
                    }
                }
            }
            (Some(id), _) if requests.len() == 1 && requests[0].command == "shape.polygon" => {
                app.edit.split_target = None;
                app.tools.set_tool(Tool::Select);
                let points = requests[0].params["points"].clone();
                // A stitch pattern splits off the needle points inside the outline.
                let request = if kind_of(app, id) == Some(ObjectKind::Stitches) {
                    CommandRequest::new(
                        "stitches.splitArea",
                        serde_json::json!({ "id": id, "points": points }),
                    )
                } else {
                    edit_ops::split(id, points)
                };
                ToolEffect::Execute {
                    requests: vec![request],
                    select_result: true,
                }
            }
            _ => ToolEffect::Execute {
                requests,
                select_result,
            },
        },
        other => other,
    }
}

/// Arrange Copies and Offset Lines dialogs.
pub fn windows(app: &mut EditorApp, ctx: &egui::Context) {
    app.edit.alt = ctx.input(|i| i.modifiers.alt);
    arrange_window(app, ctx);
    offset_window(app, ctx);
    floral_window(app, ctx);
}

fn arrange_window(app: &mut EditorApp, ctx: &egui::Context) {
    let units = app.ui.prefs.units;
    let mut open = app.edit.arrange_open;
    let mut apply = false;
    let bounds = bounds_of(app.session.design(), &app.selection);
    // Path and scatter: the objects that can guide the copies (lines and outlines).
    let guides: Vec<(u64, String)> = app
        .session
        .design()
        .objects
        .iter()
        .filter(|o| !o.contours.is_empty())
        .map(|o| {
            (
                o.id,
                format!("#{} {}", o.id, o.name.clone().unwrap_or_default()),
            )
        })
        .collect();
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-arrange-copies")).id(egui::Id::unique("window-arrange-copies"))
        .open(&mut open)
        .resizable(false)
        .default_width(300.0)
        .show(ctx, |ui| {
            let f = &mut app.edit.arrange;
            let r = egui::ComboBox::from_label("Arrangement")
                .selected_text(f.mode.label())
                .show_ui(ui, |ui| {
                    for m in ArrangeMode::ALL {
                        ui.selectable_value(&mut f.mode, m, m.label());
                    }
                });
            crate::a11y::name(&r.response, "Arrangement");
            egui::Grid::new("arrange")
                .num_columns(2)
                .show(ui, |ui| match f.mode {
                    ArrangeMode::Circle => {
                        ui.label(t("ui-patterns"));
                        ui.add(egui::DragValue::new(&mut f.count).range(2..=360));
                        ui.end_row();
                        ui.label(t("ui-radius"));
                        if ui
                            .add(
                                crate::units_ui::drag(&mut f.radius, units)
                                    .range(0.0..=1000.0)
                                    ,
                            )
                            .changed()
                            && let Some(b) = bounds
                        {
                            f.radius_changed(&b);
                        }
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut f.rotate, t("ui-turn-copies"));
                        ui.end_row();
                        ui.label("");
                        ui.checkbox(&mut f.mirror_alternate, t("ui-mirror-every-other-kaleidoscope"))
                            .on_hover_text(t("ui-beyond-pe-design-every-second-copy-tip"));
                        ui.end_row();
                    }
                    ArrangeMode::Path | ArrangeMode::Scatter => {
                        let path = f.mode == ArrangeMode::Path;
                        ui.label(if path { "Line" } else { "Shape" });
                        let shown = guides
                            .iter()
                            .find(|(id, _)| Some(*id) == f.guide)
                            .map_or("Choose…".to_string(), |(_, l)| l.clone());
                        egui::ComboBox::from_id_salt("arrange-guide")
                            .selected_text(shown)
                            .show_ui(ui, |ui| {
                                for (id, label) in &guides {
                                    ui.selectable_value(&mut f.guide, Some(*id), label);
                                }
                            });
                        ui.end_row();
                        ui.label(if path { "Copies" } else { "At most" });
                        ui.add(egui::DragValue::new(&mut f.count).range(1..=200));
                        ui.end_row();
                        if path {
                            ui.label(t("ui-side"));
                            egui::ComboBox::from_id_salt("arrange-side")
                                .selected_text(
                                    PATH_SIDES
                                        .iter()
                                        .find(|(k, _)| *k == f.side)
                                        .map_or("", |(_, l)| l),
                                )
                                .show_ui(ui, |ui| {
                                    for (k, l) in PATH_SIDES {
                                        ui.selectable_value(&mut f.side, k, l);
                                    }
                                });
                            ui.end_row();
                            ui.label("");
                            ui.checkbox(&mut f.follow, t("ui-turn-with-the-line"));
                            ui.end_row();
                            ui.label("");
                            ui.checkbox(&mut f.mirror_alternate, t("ui-mirror-every-other"));
                            ui.end_row();
                        } else {
                            ui.label(t("ui-size"));
                            ui.horizontal(|ui| {
                                ui.add(
                                    egui::DragValue::new(&mut f.size_min)
                                        .range(0.05..=20.0)
                                        .speed(0.01),
                                );
                                ui.add(
                                    egui::DragValue::new(&mut f.size_max)
                                        .range(0.05..=20.0)
                                        .speed(0.01),
                                );
                            });
                            ui.end_row();
                            ui.label(t("ui-gap"));
                            ui.add(
                                crate::units_ui::drag(&mut f.gap, units)
                                    .range(0.0..=100.0)
                                    ,
                            );
                            ui.end_row();
                            ui.label("");
                            ui.checkbox(&mut f.random_rotate, t("ui-turn-at-random"));
                            ui.end_row();
                            ui.label(t("ui-seed"));
                            ui.horizontal(|ui| {
                                ui.add(egui::DragValue::new(&mut f.seed));
                                if ui.button(t("ui-shuffle")).clicked() {
                                    f.seed = f.seed.wrapping_mul(1_103_515_245).wrapping_add(12_345) % 1_000_000;
                                }
                            });
                            ui.end_row();
                        }
                    }
                    ArrangeMode::Matrix => {
                        ui.label(t("ui-rows-columns"));
                        ui.horizontal(|ui| {
                            ui.add(egui::DragValue::new(&mut f.rows).range(1..=100));
                            ui.add(egui::DragValue::new(&mut f.columns).range(1..=100));
                        });
                        ui.end_row();
                        ui.label(t("ui-spacing-x-y"));
                        ui.horizontal(|ui| {
                            ui.add(
                                crate::units_ui::drag(&mut f.spacing_x, units)
                                    .range(-100.0..=1000.0)
                                    ,
                            );
                            ui.add(
                                crate::units_ui::drag(&mut f.spacing_y, units)
                                    .range(-100.0..=1000.0)
                                    ,
                            );
                        });
                        ui.end_row();
                    }
                    _ => {
                        ui.label(t("ui-spacing"));
                        ui.add(
                            crate::units_ui::drag(&mut f.spacing, units)
                                .range(-100.0..=1000.0)
                                ,
                        );
                        ui.end_row();
                    }
                });
            ui.weak("Drag the handle on the canvas to set the spacing or circle centre.");
            ui.checkbox(&mut f.merge, t("ui-merge-overlapping-shapes"));
            ui.checkbox(&mut f.optimize, t("ui-optimize-sewing-order"));
            let ready = !app.selection.is_empty() && (!f.mode.needs_guide() || f.guide.is_some());
            apply = ui
                .add_enabled(ready, egui::Button::new(t("ui-arrange")))
                .clicked();
        });
    if apply {
        let r = app.edit.arrange.request(&app.selection);
        if let Some(result) = app.execute(&r) {
            if app.edit.arrange.mode == ArrangeMode::Scatter {
                app.status = openstitch_app::tf!("ui-status-copies-fitted", placed = result["placed"]);
            }
            let mut sel: BTreeSet<u64> = CommandRequest::created_ids(&result);
            if let Some(all) = result.get("objects").and_then(|v| v.as_array()) {
                sel = all.iter().filter_map(|v| v.as_u64()).collect();
            }
            app.selection = sel;
        }
    }
    app.edit.arrange_open = open;
}

fn offset_window(app: &mut EditorApp, ctx: &egui::Context) {
    let units = app.ui.prefs.units;
    let mut open = app.edit.offset_open;
    let mut apply = false;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-offset-lines")).id(egui::Id::unique("window-offset-lines"))
        .open(&mut open)
        .resizable(false)
        .default_width(260.0)
        .show(ctx, |ui| {
            let f = &mut app.edit.offset;
            ui.horizontal(|ui| {
                ui.selectable_value(&mut f.inward, false, "Outward");
                ui.selectable_value(&mut f.inward, true, "Inward");
            });
            egui::Grid::new("offset").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-spacing"));
                ui.add(crate::units_ui::drag(&mut f.spacing, units).range(0.1..=100.0));
                ui.end_row();
                ui.label(t("ui-lines"));
                ui.add(egui::DragValue::new(&mut f.count).range(1..=20));
                ui.end_row();
            });
            ui.checkbox(&mut f.rounded, t("ui-rounded-corners"));
            apply = ui
                .add_enabled(single(app).is_some(), egui::Button::new(t("ui-create")))
                .clicked();
        });
    if apply && let Some(id) = single(app) {
        let r = app.edit.offset.request(id);
        run_and_select(app, r);
    }
    app.edit.offset_open = open;
}

fn floral_window(app: &mut EditorApp, ctx: &egui::Context) {
    let mut open = app.edit.floral_open;
    let mut apply = false;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-floral-pattern")).id(egui::Id::unique("window-floral-pattern"))
        .open(&mut open)
        .resizable(false)
        .default_width(260.0)
        .show(ctx, |ui| {
            let f = &mut app.edit.floral;
            ui.add(egui::Slider::new(&mut f.density, 1..=10).text("Density"));
            ui.add(egui::Slider::new(&mut f.pattern, 0..=251).text("Pattern"));
            apply = ui
                .add_enabled(single(app).is_some(), egui::Button::new(t("ui-create")))
                .clicked();
        });
    if apply && let Some(id) = single(app) {
        let r = app.edit.floral.request(id);
        run_and_select(app, r);
    }
    app.edit.floral_open = open;
}

/// Buttons at the top of the Sewing Order pane: move the selection, sort, optimise, and jump-stitch
/// trimming for the selection (multi-needle machines).
pub fn order_controls(app: &mut EditorApp, ui: &mut egui::Ui) -> Option<CommandRequest> {
    let mut out = None;
    let any = !app.selection.is_empty();
    ui.horizontal_wrapped(|ui| {
        for (label, icon, to, hint) in [
            ("First", None, "first", "Sew first"),
            ("", Some("move_up"), "earlier", "Sew earlier"),
            ("", Some("move_down"), "later", "Sew later"),
            ("Last", None, "last", "Sew last"),
        ] {
            let button = match icon {
                Some(icon) => egui::Button::new(crate::icons::ui_image(ui, icon)),
                None => egui::Button::new(label),
            };
            if ui
                .add_enabled(any, button)
                .on_hover_text(hint)
                .clicked()
            {
                out = Some(objects::move_to(&app.selection, to));
            }
        }
        if ui
            .button(t("ui-sort-by-colour"))
            .on_hover_text(t("ui-same-colour-objects-sew-together-tip"))
            .clicked()
        {
            out = Some(CommandRequest::new("order.byColor", serde_json::json!({})));
        }
        let mode = app.session.design().optimize_order;
        let hint = match mode {
            OptimizeOrder::PeDesign => "As PE-DESIGN 11.50: group colours (drawing order within a colour) and link the entry and exit points of same-colour neighbours; overlapping objects keep their order",
            OptimizeOrder::MinJump => "Beyond PE-DESIGN: the same colour groups, each arranged for the shortest total jump (greedy nearest, then 2-opt), every neighbouring pair linked; overlapping objects keep their order",
        };
        if ui.button(t("ui-optimise")).on_hover_text(hint).clicked() {
            out = Some(CommandRequest::new("order.optimize", serde_json::json!({})));
        }
        let mut chosen = mode;
        egui::ComboBox::from_id_salt("order-optimize-mode")
            .selected_text(mode.label())
            .width(130.0)
            .show_ui(ui, |ui| {
                for m in OptimizeOrder::ALL {
                    ui.selectable_value(&mut chosen, m, m.label());
                }
            })
            .response
            .on_hover_text(t("ui-how-optimise-arranges-the-objects-saved-tip"));
        if let Some(r) = optimize_mode_request(mode, chosen) {
            out = Some(r);
        }
    });
    if let Some(r) = list_controls(app, ui) {
        out = Some(r);
    }
    if let Some(r) = crate::finish_ui::finish_controls(app, ui) {
        out = Some(r);
    }
    let d = app.session.design();
    if d.machine.machine == MachineType::MultiNeedle && any {
        let selected: Vec<_> = d
            .objects
            .iter()
            .filter(|o| app.selection.contains(&o.id))
            .collect();
        let (mut on, label) = jump_trimming_state(selected.iter().map(|o| o.trim_jumps));
        if ui.checkbox(&mut on, label).changed() {
            out = Some(jump_trimming_request(&app.selection, on));
        }
    }
    out
}

/// Sewing Order list controls (#502): Sew in Click Order, Lock, Sew outline first.
fn list_controls(app: &mut EditorApp, ui: &mut egui::Ui) -> Option<CommandRequest> {
    objects::track_click_order(&mut app.click_order, &app.selection);
    let mut out = None;
    let d = app.session.design();
    let shapes = objects::two_part_shapes(d, &app.selection);
    let all_locked = !app.selection.is_empty()
        && d.objects
            .iter()
            .filter(|o| app.selection.contains(&o.id))
            .all(|o| o.locked);
    let line_first = !shapes.is_empty()
        && shapes
            .iter()
            .all(|&id| d.index_of(id).is_some_and(|i| d.objects[i].line_first));
    ui.horizontal_wrapped(|ui| {
        if ui
            .add_enabled(
                app.click_order.len() >= 2,
                egui::Button::new(t("ui-sew-in-click-order")),
            )
            .on_hover_text(t("ui-sew-the-selected-objects-in-the-tip"))
            .on_disabled_hover_text("Shift- or Ctrl+click two or more objects in the order they should sew")
            .clicked()
        {
            out = objects::click_order(&app.click_order);
        }
        let mut locked = all_locked;
        if ui
            .add_enabled(!app.selection.is_empty(), egui::Checkbox::new(&mut locked, "Lock"))
            .on_hover_text(t("ui-locked-objects-cannot-be-picked-moved-tip"))
            .changed()
        {
            out = Some(objects::set_locked(&app.selection, locked));
        }
        let mut first = line_first;
        if ui
            .add_enabled(!shapes.is_empty(), egui::Checkbox::new(&mut first, "Sew outline first"))
            .on_hover_text(t("ui-beyond-pe-design-sew-the-outline-tip"))
            .on_disabled_hover_text("Select a shape with both a region and an outline")
            .changed()
        {
            out = Some(objects::set_line_first(&shapes, first));
        }
        // Beyond PE-DESIGN (#512): a machine stop after the object.
        let stops = !app.selection.is_empty()
            && d.objects
                .iter()
                .filter(|o| app.selection.contains(&o.id))
                .all(|o| o.stop_after);
        let mut stop = stops;
        if ui
            .add_enabled(!app.selection.is_empty(), egui::Checkbox::new(&mut stop, "Stop after"))
            .on_hover_text(t("ui-beyond-pe-design-the-machine-pauses-tip"))
            .changed()
        {
            out = Some(objects::set_stop_after(&app.selection, stop));
        }
        // Beyond PE-DESIGN (#525): a selected Manual Punch column.
        let column = match app.selection.iter().collect::<Vec<_>>()[..] {
            [&id] => d
                .objects
                .iter()
                .find(|o| o.id == id && openstitch_app::editor::tools::punch_rungs::rungs(o).is_some())
                .map(|o| o.id),
            _ => None,
        };
        if let Some(id) = column {
            for (label, command, tip) in [
                ("Swap sides", "object.swapSides", "Beyond PE-DESIGN: every crossing starts from the other side of the column"),
                ("Untangle", "object.untangle", "Beyond PE-DESIGN: turn back rungs whose ends were clicked the other way round"),
            ] {
                if ui.button(openstitch_app::editor::ui_texts::tr(label)).on_hover_text(tip).clicked() {
                    out = Some(CommandRequest::new(command, serde_json::json!({ "id": id })));
                }
            }
        }
        // Beyond PE-DESIGN (#546).
        let ids = CommandRequest::ids(&app.selection);
        let branched = d
            .objects
            .iter()
            .any(|o| app.selection.contains(&o.id) && o.branch);
        if branched {
            if ui
                .button(t("ui-unbranch"))
                .on_hover_text(t("ui-take-the-branch-apart-the-objects-tip"))
                .clicked()
            {
                out = Some(CommandRequest::new("object.unbranch", serde_json::json!({ "ids": ids })));
            }
        } else if ui
            .add_enabled(app.selection.len() >= 2, egui::Button::new(t("ui-branch")))
            .on_hover_text(t("ui-beyond-pe-design-sew-the-selected-tip"))
            .on_disabled_hover_text("Select two or more touching shapes")
            .clicked()
        {
            out = Some(CommandRequest::new("object.branch", serde_json::json!({ "ids": ids })));
        }
        let lines = !app.selection.is_empty()
            && d.objects
                .iter()
                .filter(|o| app.selection.contains(&o.id))
                .all(|o| o.region.is_none() && o.line.is_some());
        for (label, redwork, tip) in [
            ("Auto-route lines", false, "Beyond PE-DESIGN: join touching running lines into one walk each, doubling back only where needed"),
            ("Redwork", true, "Beyond PE-DESIGN: sew the touching lines twice, out and back, in one walk"),
        ] {
            if ui
                .add_enabled(lines, egui::Button::new(label))
                .on_hover_text(tip)
                .on_disabled_hover_text("Select line objects (no region)")
                .clicked()
            {
                out = Some(CommandRequest::new(
                    "shape.autoRoute",
                    serde_json::json!({ "ids": ids, "redwork": redwork }),
                ));
            }
        }
        // Beyond PE-DESIGN (#511, #545).
        let chosen: Vec<_> = d
            .objects
            .iter()
            .filter(|o| app.selection.contains(&o.id))
            .collect();
        let any = !chosen.is_empty();
        let mut hide = any && chosen.iter().all(|o| o.hide_under.is_some());
        if ui
            .add_enabled(any, egui::Checkbox::new(&mut hide, "Leave out hidden stitching"))
            .on_hover_text(t("ui-beyond-pe-design-skip-the-stitching-tip"))
            .changed()
        {
            out = Some(objects::set_flag(&app.selection, "object.setHideUnder", serde_json::json!({ "on": hide })));
        }
        let mut run = any && chosen.iter().all(|o| o.connector_run);
        if ui
            .add_enabled(any, egui::Checkbox::new(&mut run, "Then: run"))
            .on_hover_text(t("ui-beyond-pe-design-reach-the-next-tip"))
            .changed()
        {
            let connector = if run { "run" } else { "jump" };
            out = Some(objects::set_flag(&app.selection, "object.setConnector", serde_json::json!({ "connector": connector })));
        }
        // Beyond PE-DESIGN (#510).
        let update = d
            .objects
            .iter()
            .any(|o| app.selection.contains(&o.id) && o.knockdown.is_some());
        let label = if update { "Update knockdown" } else { "Knockdown" };
        if ui
            .button(openstitch_app::editor::ui_texts::tr(label))
            .on_hover_text(t("ui-beyond-pe-design-a-light-fill-tip"))
            .clicked()
        {
            out = Some(objects::knockdown(d, &app.selection));
        }
    });
    out
}

/// The command the Optimise mode box sends when `chosen` is picked while the design has
/// `current`; `None` when the mode is unchanged.
fn optimize_mode_request(current: OptimizeOrder, chosen: OptimizeOrder) -> Option<CommandRequest> {
    (chosen != current).then(|| {
        CommandRequest::new(
            "design.setProperties",
            serde_json::json!({ "optimizeOrder": chosen.id() }),
        )
    })
}

/// The jump-trimming checkbox for the selected objects' `trim_jumps`: ticked unless every object
/// has it off, and labelled "(mixed)" when ticked but some object has it off.
fn jump_trimming_state(trims: impl Iterator<Item = Option<bool>>) -> (bool, &'static str) {
    let (mut on, mut off) = (false, false);
    for t in trims {
        if t == Some(false) {
            off = true;
        } else {
            on = true;
        }
    }
    let label = if on && off {
        "Jump stitch trimming (mixed)"
    } else {
        "Jump stitch trimming"
    };
    (on, label)
}

/// The command the jump-trimming checkbox sends for the selection when it becomes `enabled`.
fn jump_trimming_request(ids: &BTreeSet<u64>, enabled: bool) -> CommandRequest {
    CommandRequest::new(
        "object.setJumpTrimming",
        serde_json::json!({ "ids": CommandRequest::ids(ids), "enabled": enabled }),
    )
}

/// The sewing order as colour frames: click selects a frame's objects; the arrows move it past
/// the neighbouring frame.
pub fn colour_frames(app: &mut EditorApp, ui: &mut egui::Ui) {
    let d = app.session.design();
    let frames = objects::colour_frames(d);
    let rows = objects::rows(d, &app.selection);
    let mut pending = None;
    let mut select = None;
    egui::ScrollArea::vertical()
        .auto_shrink([false, false])
        .show(ui, |ui| {
            for (k, frame) in frames.iter().enumerate() {
                let first = rows.iter().find(|r| r.id == frame[0]);
                let selected = frame.iter().all(|id| app.selection.contains(id));
                ui.horizontal(|ui| {
                    if let Some(th) = first.and_then(|r| r.swatches.first()) {
                        crate::panels::thread_swatch(ui, th, 12.0, crate::panels::Caption::Beside);
                    }
                    let text = match frame.len() {
                        1 => format!("{}. {}", k + 1, first.map_or("", |r| r.label.as_str())),
                        n => format!("{}. {n} objects", k + 1),
                    };
                    if ui.selectable_label(selected, text).clicked() {
                        select = Some(frame.clone());
                    }
                    if selected {
                        let up = egui::Button::new(crate::icons::ui_image(ui, "move_up")).small();
                        if ui.add(up).on_hover_text(t("ui-sew-earlier-tip")).clicked() {
                            pending = objects::shift_frame(&frames, k, -1);
                        }
                        let down =
                            egui::Button::new(crate::icons::ui_image(ui, "move_down")).small();
                        if ui.add(down).on_hover_text(t("ui-sew-later-tip")).clicked() {
                            pending = objects::shift_frame(&frames, k, 1);
                        }
                    }
                });
            }
        });
    if let Some(ids) = select {
        app.selection = ids.into_iter().collect();
    }
    if let Some(r) = pending {
        app.execute(&r);
    }
}

/// R1 / R2 / L1 markers of the selected shape.
pub fn draw_markers(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let Some(id) = single(app) else { return };
    let d = app.session.design();
    let screen = |p: Vec2| {
        let s = vp.to_screen(p);
        Pos2::new(s.x as f32, s.y as f32)
    };
    let purple = Color32::from_rgb(0x8e, 0x24, 0xaa);
    let (center, reference, curve) = edit_ops::direction_markers(d, id);
    if let Some(path) = curve {
        let pts: Vec<Pos2> = path.into_iter().map(screen).collect();
        painter.add(egui::Shape::line(
            pts,
            Stroke::new(2.0, Color32::from_rgb(0xd3, 0x2f, 0x2f)),
        ));
    }
    if let Some(c) = center.map(screen) {
        for (a, b) in [
            (egui::vec2(-7.0, 0.0), egui::vec2(7.0, 0.0)),
            (egui::vec2(0.0, -7.0), egui::vec2(0.0, 7.0)),
        ] {
            painter.line_segment([c + a, c + b], Stroke::new(2.0, purple));
        }
    }
    if let Some(c) = reference.map(screen) {
        painter.rect_stroke(
            egui::Rect::from_center_size(c, egui::vec2(9.0, 9.0)),
            0.0,
            Stroke::new(2.0, purple),
            egui::StrokeKind::Middle,
        );
    }
    // Stamps: engraved red, embossed blue (PE-DESIGN's colours).
    for (embossed, paths) in edit_ops::stamp_outlines(d, id) {
        let colour = if embossed {
            Color32::from_rgb(0x1e, 0x63, 0xd6)
        } else {
            Color32::from_rgb(0xd3, 0x2f, 0x2f)
        };
        for path in paths {
            let pts: Vec<Pos2> = path
                .into_iter()
                .map(|p| {
                    let s = vp.to_screen(p);
                    Pos2::new(s.x as f32, s.y as f32)
                })
                .collect();
            painter.add(egui::Shape::line(pts, Stroke::new(1.5, colour)));
        }
    }
    for (which, p) in edit_ops::entry_exit_markers(d, id) {
        let p = match app.edit.marker_drag {
            Some((i, w, at)) if i == id && w == which => at,
            _ => p,
        };
        let s = vp.to_screen(p);
        let c = Pos2::new(s.x as f32, s.y as f32);
        let colour = match which {
            EntryExit::RegionEntry | EntryExit::LineEntry => Color32::from_rgb(0x2e, 0x7d, 0x32),
            EntryExit::RegionExit | EntryExit::LineExit => Color32::from_rgb(0xc6, 0x28, 0x28),
        };
        painter.circle_filled(c, 4.5, colour);
        painter.circle_stroke(c, 4.5, Stroke::new(1.0, Color32::WHITE));
        painter.text(
            c + egui::vec2(6.0, -6.0),
            egui::Align2::LEFT_BOTTOM,
            which.label(),
            egui::FontId::proportional(11.0),
            colour,
        );
    }
}

/// While Arrange Copies is open: the copies it would make, faded, and its drag handle.
pub fn draw_arrange_preview(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    if !app.edit.arrange_open {
        return;
    }
    let d = app.session.design();
    let Some(b) = bounds_of(d, &app.selection) else {
        return;
    };
    let screen = |p: Vec2| {
        let s = vp.to_screen(p);
        Pos2::new(s.x as f32, s.y as f32)
    };
    let accent = Color32::from_rgb(0x1e, 0x88, 0xe5);
    let stroke = Stroke::new(1.2, accent.gamma_multiply(0.75));
    let selected: Vec<_> = d
        .objects
        .iter()
        .filter(|o| app.selection.contains(&o.id))
        .collect();
    for t in app.edit.arrange.transforms(&b) {
        for o in &selected {
            let paths: Vec<Vec<Vec2>> = match o.kind {
                ObjectKind::Stitches => o
                    .bounds()
                    .map(|r| Contour::rectangle(r.min_x, r.min_y, r.width(), r.height()).path())
                    .into_iter()
                    .collect(),
                _ => o.contours.iter().map(|c| c.path()).collect(),
            };
            for path in paths {
                let pts: Vec<Pos2> = path.into_iter().map(|p| screen(t.apply(p))).collect();
                painter.add(egui::Shape::dashed_line(&pts, stroke, 5.0, 3.0));
            }
        }
    }
    if let Some(h) = app.edit.arrange.handle(&b).map(screen) {
        let r = SelectionHandles::RADIUS as f32;
        painter.circle(h, r, Color32::WHITE, Stroke::new(1.5, accent));
        painter.line_segment([h - egui::vec2(r, 0.0), h + egui::vec2(r, 0.0)], stroke);
        painter.line_segment([h - egui::vec2(0.0, r), h + egui::vec2(0.0, r)], stroke);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use openstitch_app::editor::PointerEvent;
    use openstitch_design::v2;
    use serde_json::json;

    fn rect(app: &mut EditorApp, x: f64) {
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": x, "y": 10, "width": 20, "height": 20 }),
        ));
    }

    #[test]
    fn clicking_a_group_member_selects_the_group() {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        rect(&mut app, 50.0);
        app.selection = BTreeSet::from([1, 2]);
        group(&mut app);
        app.selection.clear();
        let click = |app: &mut EditorApp, at: Vec2| {
            for phase in [Phase::Down, Phase::Up] {
                app.with_tools(|t, c| t.handle(PointerEvent::new(phase, at), c));
            }
        };
        click(&mut app, v2(200.0, 200.0));
        assert_eq!(app.selection, BTreeSet::from([1, 2]));
        app.selection.clear();
        app.edit.alt = true;
        click(&mut app, v2(200.0, 200.0));
        assert_eq!(app.selection, BTreeSet::from([1]));
        app.edit.alt = false;
        app.selection = BTreeSet::from([1, 2]);
        ungroup(&mut app);
        app.selection.clear();
        click(&mut app, v2(200.0, 200.0));
        assert_eq!(app.selection, BTreeSet::from([1]));
    }

    #[test]
    fn split_with_outline_uses_the_polygon_tool() {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        app.edit.split_target = Some(1);
        app.tools.set_tool(Tool::Polygon);
        for p in [
            v2(50.0, 50.0),
            v2(200.0, 50.0),
            v2(200.0, 400.0),
            v2(50.0, 400.0),
        ] {
            for phase in [Phase::Down, Phase::Up] {
                app.with_tools(|t, c| t.handle(PointerEvent::new(phase, p), c));
            }
        }
        let effects = app.tools.finish();
        app.apply_effects(effects);
        assert_eq!(app.session.design().objects.len(), 2, "{}", app.status);
        assert_eq!(app.tools.tool(), Tool::Select);
        assert!(app.edit.split_target.is_none());
        app.undo();
        assert_eq!(app.session.design().objects.len(), 1);
    }

    #[test]
    fn picking_sets_entry_points_and_escape_cancels() {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        app.edit.pick = Some((1, Pick::Point(EntryExit::RegionEntry)));
        assert!(pick(&mut app, v2(90.0, 200.0)));
        let r = app.session.design().objects[0]
            .region
            .clone()
            .expect("region");
        assert_eq!(r.entry, Some(v2(100.0, 200.0)));
        assert!(!pick(&mut app, v2(0.0, 0.0)));
        app.edit.pick = Some((1, Pick::Point(EntryExit::RegionExit)));
        assert!(app.edit.cancel());
        assert!(!app.edit.cancel());
    }

    #[test]
    fn pen_draws_the_curved_direction() {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        app.edit.curve_target = Some(1);
        app.tools.set_tool(Tool::Pen);
        for p in [v2(50.0, 200.0), v2(350.0, 150.0)] {
            for phase in [Phase::Down, Phase::Up] {
                app.with_tools(|t, c| t.handle(PointerEvent::new(phase, p), c));
            }
        }
        let effects = app.tools.finish();
        app.apply_effects(effects);
        let r = app.session.design().objects[0]
            .region
            .clone()
            .expect("region");
        assert!(r.direction_curve.is_some(), "{}", app.status);
        assert_eq!(app.session.design().objects.len(), 1, "no new shape");
        assert_eq!(app.tools.tool(), Tool::Select);
        app.edit.pick = Some((1, Pick::Center));
        assert!(pick(&mut app, v2(150.0, 150.0)));
        assert!(
            app.status.contains("failed") || app.status.contains("no such"),
            "fill has no centre: {}",
            app.status
        );
    }

    #[test]
    fn stamp_pick_places_a_stamp() {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        app.edit.stamp_pick = Some((1, "star", "engravedRegion"));
        assert!(pick(&mut app, v2(200.0, 200.0)));
        let r = app.session.design().objects[0]
            .region
            .clone()
            .expect("region");
        assert_eq!(r.stamps.len(), 1);
        assert!(app.edit.stamp_pick.is_none());
    }

    fn gesture(app: &mut EditorApp, from: Vec2, to: Vec2) {
        for (phase, at) in [(Phase::Down, from), (Phase::Dragged, to), (Phase::Up, to)] {
            app.with_tools(|t, c| t.handle(PointerEvent::new(phase, at), c));
        }
    }

    fn bounds(app: &EditorApp, id: u64) -> openstitch_design::Rect {
        let d = app.session.design();
        d.objects[d.index_of(id).expect("object")]
            .bounds()
            .expect("bounds")
    }

    #[test]
    fn snap_to_grid_places_drawing_moves_and_points_on_the_grid() {
        let mut app = EditorApp::new(Launch::default());
        app.toggle_snap_to_grid();
        app.workspace.grid.spacing_mm = 10.0;
        app.tools.set_tool(Tool::Rectangle);
        gesture(&mut app, v2(93.0, 104.0), v2(296.0, 212.0));
        let b = bounds(&app, 1);
        assert_eq!(
            (b.min_x, b.min_y, b.max_x, b.max_y),
            (100.0, 100.0, 300.0, 200.0)
        );
        // A move the grid rounds away does nothing; a longer one lands on the next line.
        app.tools.set_tool(Tool::Select);
        gesture(&mut app, v2(150.0, 150.0), v2(183.0, 160.0));
        assert_eq!(bounds(&app, 1).min_x, 100.0);
        gesture(&mut app, v2(150.0, 150.0), v2(214.0, 163.0));
        let b = bounds(&app, 1);
        assert_eq!((b.min_x, b.min_y), (200.0, 100.0));
        app.tools.set_tool(Tool::Point);
        gesture(&mut app, v2(200.0, 100.0), v2(288.0, 113.0));
        let c = &app.session.design().objects[0].contours[0];
        assert!(c.points.contains(&v2(300.0, 100.0)), "{:?}", c.points);
        // Off: positions are taken as they are.
        app.toggle_snap_to_grid();
        app.tools.set_tool(Tool::Rectangle);
        gesture(&mut app, v2(503.0, 507.0), v2(611.0, 655.0));
        let b = bounds(&app, 2);
        assert_eq!((b.min_x, b.min_y), (503.0, 507.0));
    }

    #[test]
    fn dragging_an_entry_marker_is_one_undo() {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        app.selection = BTreeSet::from([1]);
        app.edit.pick = Some((1, Pick::Point(EntryExit::RegionEntry)));
        assert!(pick(&mut app, v2(90.0, 200.0)));
        let entry = |app: &EditorApp| {
            app.session.design().objects[0]
                .region
                .as_ref()
                .and_then(|r| r.entry)
        };
        assert_eq!(entry(&app), Some(v2(100.0, 200.0)));
        assert!(
            !drag(&mut app, Phase::Down, v2(200.0, 200.0)),
            "not on a marker"
        );
        assert!(drag(&mut app, Phase::Down, v2(102.0, 203.0)));
        assert!(drag(&mut app, Phase::Dragged, v2(210.0, 320.0)));
        assert_eq!(
            entry(&app),
            Some(v2(100.0, 200.0)),
            "unchanged until released"
        );
        assert!(drag(&mut app, Phase::Up, v2(210.0, 320.0)));
        assert!(app.edit.marker_drag.is_none());
        let moved = entry(&app).expect("entry");
        assert_eq!(moved.y, 300.0, "on the outline: {moved:?}");
        app.undo();
        assert_eq!(entry(&app), Some(v2(100.0, 200.0)));
    }

    #[test]
    fn the_line_exit_marker_is_set_dragged_and_cleared() {
        // #547: L2, the line exit point, has its marker as R1, R2 and L1 do: picked on the
        // outline, dragged in one undo step, cleared with the others (object.setEntryExit
        // lineExit, the path the Assistant takes).
        let mut app = EditorApp::new(Launch::default());
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 10, "y": 10, "width": 20, "height": 20, "outline": "Black" }),
        ));
        app.selection = BTreeSet::from([1]);
        let exit = |app: &EditorApp| {
            app.session.design().objects[0]
                .line
                .as_ref()
                .and_then(|l| l.exit)
        };
        app.edit.pick = Some((1, Pick::Point(EntryExit::LineExit)));
        assert!(pick(&mut app, v2(310.0, 200.0)));
        assert_eq!(exit(&app), Some(v2(300.0, 200.0)));
        let markers = edit_ops::entry_exit_markers(app.session.design(), 1);
        assert!(markers.contains(&(EntryExit::LineExit, v2(300.0, 200.0))), "{markers:?}");
        assert!(drag(&mut app, Phase::Down, v2(298.0, 202.0)));
        assert!(drag(&mut app, Phase::Dragged, v2(200.0, 320.0)));
        assert!(drag(&mut app, Phase::Up, v2(200.0, 320.0)));
        assert_eq!(exit(&app).map(|p| p.y), Some(300.0), "on the outline");
        app.undo();
        assert_eq!(exit(&app), Some(v2(300.0, 200.0)));
        app.execute(&edit_ops::clear_entry_exit(1));
        assert_eq!(exit(&app), None);
    }

    fn preview_bounds(app: &EditorApp) -> Vec<[i64; 4]> {
        let b = bounds(app, 1);
        let mut out: Vec<[i64; 4]> = app
            .edit
            .arrange
            .transforms(&b)
            .iter()
            .map(|t| {
                let corners = [
                    v2(b.min_x, b.min_y),
                    v2(b.max_x, b.min_y),
                    v2(b.max_x, b.max_y),
                    v2(b.min_x, b.max_y),
                ]
                .map(|p| t.apply(p));
                let r = openstitch_design::Rect::enclosing(corners).expect("rect");
                [r.min_x, r.min_y, r.max_x, r.max_y].map(|v| v.round() as i64)
            })
            .collect();
        out.sort();
        out
    }

    /// Runs the arrangement and returns the copies' bounds; undoes it again.
    fn applied_bounds(app: &mut EditorApp) -> Vec<[i64; 4]> {
        let result = app
            .execute(&app.edit.arrange.request(&app.selection))
            .expect("arranged");
        let mut out: Vec<[i64; 4]> = CommandRequest::created_ids(&result)
            .into_iter()
            .map(|id| {
                let r = bounds(app, id);
                [r.min_x, r.min_y, r.max_x, r.max_y].map(|v| v.round() as i64)
            })
            .collect();
        out.sort();
        app.undo();
        out
    }

    #[test]
    fn arrange_dialog_kaleidoscope_path_and_scatter() {
        let mut app = EditorApp::new(Launch::default());
        // A rectangle: the preview's boxes match the engine's copies exactly (the mirroring
        // itself is checked in the engine's arrange_modes tests).
        rect(&mut app, 10.0);
        app.selection = BTreeSet::from([1]);
        app.edit.arrange.mode = ArrangeMode::Circle;
        app.edit.arrange.count = 6;
        app.edit.arrange.mirror_alternate = true;
        // The dashed preview shows the kaleidoscope exactly as the engine places it.
        assert_eq!(preview_bounds(&app).len(), 5);
        assert_eq!(preview_bounds(&app), applied_bounds(&mut app));
        // Along a line: needs a guide; the preview is left to the engine.
        let line = app
            .execute(&CommandRequest::new(
                "shape.polygon",
                json!({ "points": [[10, 60], [90, 60]], "closed": false }),
            ))
            .expect("line")["id"]
            .as_u64()
            .expect("id");
        let f = &mut app.edit.arrange;
        f.mode = ArrangeMode::Path;
        f.count = 5;
        let b = bounds(&app, 1);
        let f = &app.edit.arrange;
        assert!(f.mode.needs_guide() && f.transforms(&b).is_empty());
        app.edit.arrange.guide = Some(line);
        app.selection = BTreeSet::from([1]);
        let r = app
            .execute(&app.edit.arrange.request(&app.selection))
            .unwrap_or_else(|| panic!("path: {}", app.status));
        assert_eq!(r["ids"].as_array().map(Vec::len), Some(5));
        app.undo();
        // Scatter inside a circle, reported back.
        let circle = app
            .execute(&CommandRequest::new(
                "shape.ellipse",
                json!({ "cx": 60, "cy": 60, "rx": 30, "outline": "Red" }),
            ))
            .expect("circle")["id"]
            .as_u64()
            .expect("id");
        let f = &mut app.edit.arrange;
        f.mode = ArrangeMode::Scatter;
        f.guide = Some(circle);
        f.count = 8;
        f.size_min = 0.3;
        app.selection = BTreeSet::from([1]);
        let r = app
            .execute(&app.edit.arrange.request(&app.selection))
            .expect("scatter");
        assert!(r["placed"].as_u64().is_some_and(|n| n >= 1));
    }

    #[test]
    fn arrange_preview_matches_the_copies_and_its_handle_drags() {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        app.selection = BTreeSet::from([1]);
        app.edit.arrange_open = true;
        // Mirror: the handle is the mirror axis; dragging it 5 mm right makes a 10 mm gap.
        app.edit.arrange.mode = ArrangeMode::MirrorHorizontal;
        let b = bounds(&app, 1);
        assert_eq!(app.edit.arrange.handle(&b), Some(v2(300.0, 200.0)));
        assert!(drag(&mut app, Phase::Down, v2(302.0, 199.0)));
        assert!(drag(&mut app, Phase::Dragged, v2(350.0, 230.0)));
        assert!(drag(&mut app, Phase::Up, v2(350.0, 230.0)));
        assert!(!app.edit.arrange_drag);
        assert_eq!(app.edit.arrange.spacing, 10.0);
        assert_eq!(preview_bounds(&app), vec![[400, 100, 600, 300]]);
        assert_eq!(preview_bounds(&app), applied_bounds(&mut app));
        assert_eq!(
            app.session.design().objects.len(),
            1,
            "dragging adds nothing"
        );
        // Circle: the handle is the centre; the radius follows it.
        app.edit.arrange.mode = ArrangeMode::Circle;
        app.edit.arrange.count = 4;
        assert!(drag(&mut app, Phase::Down, v2(200.0, 500.0)));
        assert!(drag(&mut app, Phase::Up, v2(200.0, 600.0)));
        assert_eq!(app.edit.arrange.radius, 40.0);
        assert_eq!(app.edit.arrange.center, Some(v2(20.0, 60.0)));
        assert_eq!(preview_bounds(&app).len(), 3);
        assert_eq!(preview_bounds(&app), applied_bounds(&mut app));
        app.edit.arrange.radius = 20.0;
        app.edit.arrange.radius_changed(&bounds(&app, 1));
        assert_eq!(app.edit.arrange.center, Some(v2(20.0, 40.0)));
        // Matrix: the handle is the last copy's corner; dragging sets the gap, never a stack.
        app.edit.arrange.mode = ArrangeMode::Matrix;
        app.edit.arrange.rows = 1;
        app.edit.arrange.columns = 3;
        assert_eq!(
            app.edit.arrange.handle(&bounds(&app, 1)),
            Some(v2(540.0, 100.0))
        );
        assert!(drag(&mut app, Phase::Down, v2(540.0, 100.0)));
        assert!(drag(&mut app, Phase::Up, v2(600.0, 100.0)));
        assert_eq!(app.edit.arrange.spacing_x, 5.0);
        assert_eq!(preview_bounds(&app), applied_bounds(&mut app));
        app.edit.arrange.drag_handle(&bounds(&app, 1), v2(0.0, 0.0));
        assert!(app.edit.arrange.spacing_x > -20.0);
        assert!(
            app.execute(&app.edit.arrange.request(&app.selection))
                .is_some()
        );
        // Closed: no handle to grab.
        app.undo();
        app.edit.arrange_open = false;
        assert!(!drag(&mut app, Phase::Down, v2(110.0, 100.0)));
    }

    #[test]
    fn optimise_mode_box_sets_the_design_mode() {
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        let mode = app.session.design().optimize_order;
        assert_eq!(mode, OptimizeOrder::PeDesign);
        assert!(
            optimize_mode_request(mode, mode).is_none(),
            "an unchanged box sends nothing"
        );
        let r = optimize_mode_request(mode, OptimizeOrder::MinJump).expect("a change");
        assert_eq!(r.command, "design.setProperties");
        assert_eq!(r.params, json!({ "optimizeOrder": "minJump" }));
        assert!(app.execute(&r).is_some(), "{}", app.status);
        assert_eq!(app.session.design().optimize_order, OptimizeOrder::MinJump);
        let back = optimize_mode_request(OptimizeOrder::MinJump, OptimizeOrder::PeDesign)
            .expect("a change");
        assert!(app.execute(&back).is_some(), "{}", app.status);
        assert_eq!(app.session.design().optimize_order, OptimizeOrder::PeDesign);
    }

    #[test]
    fn jump_trimming_checkbox_shows_mixed_and_sets_the_selection() {
        fn state(t: &[Option<bool>]) -> (bool, &'static str) {
            jump_trimming_state(t.iter().copied())
        }
        assert_eq!(state(&[None, Some(true)]), (true, "Jump stitch trimming"));
        assert_eq!(
            state(&[Some(false), Some(false)]),
            (false, "Jump stitch trimming")
        );
        assert_eq!(
            state(&[None, Some(false)]),
            (true, "Jump stitch trimming (mixed)")
        );
        let mut app = EditorApp::new(Launch::default());
        rect(&mut app, 10.0);
        rect(&mut app, 50.0);
        app.selection = BTreeSet::from([2]);
        let off = jump_trimming_request(&app.selection, false);
        assert_eq!(off.command, "object.setJumpTrimming");
        assert_eq!(off.params, json!({ "ids": [2], "enabled": false }));
        assert!(app.execute(&off).is_some(), "{}", app.status);
        let trims = |app: &EditorApp| -> Vec<Option<bool>> {
            app.session
                .design()
                .objects
                .iter()
                .map(|o| o.trim_jumps)
                .collect()
        };
        assert_eq!(trims(&app), [None, Some(false)]);
        assert_eq!(
            jump_trimming_state(trims(&app).into_iter()),
            (true, "Jump stitch trimming (mixed)")
        );
        app.selection = BTreeSet::from([1, 2]);
        let on = jump_trimming_request(&app.selection, true);
        assert!(app.execute(&on).is_some(), "{}", app.status);
        assert_eq!(trims(&app), [None, None]);
        assert_eq!(
            jump_trimming_state(trims(&app).into_iter()),
            (true, "Jump stitch trimming")
        );
    }
}
