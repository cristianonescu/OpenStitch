//! Split designs, multi-position hoops and the jumbo frame in the editor: the hoopings drawn
//! on the canvas, their settings in the Design Settings window (`sections.set`) and File ▸
//! Export Sections (`sections.export`).

use crate::app::EditorApp;
use eframe::egui::{self, Color32, Pos2, Shape, Stroke};
use openstitch_app::editor::sections::Axis;
use openstitch_app::editor::{CommandRequest, Viewport};
use openstitch_design::sections::{self, SectionKind, sectionable};
use openstitch_design::{BROTHER_HOOPS, Vec2};
use serde_json::json;

const SECTION: Color32 = Color32::from_rgb(0xe5, 0x39, 0x35);
const ALIGN: Color32 = Color32::from_rgb(0x9e, 0x9e, 0x9e);

fn pos(p: Vec2) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

/// Section boundaries (solid red, labelled), multi-position areas (dotted) and jumbo sides,
/// with the alignment stitching lines; nothing for a design sewn in one hooping.
pub fn draw_sections(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let plan = sections::plan(app.session.design());
    let multi = plan.sections.len() > 1
        || plan
            .sections
            .first()
            .is_some_and(|s| s.kind != SectionKind::Split);
    if !multi {
        return;
    }
    // Move Sections: the line being dragged, solid and thicker.
    if let Some((axis, at)) = app.workspace.sections.dragged() {
        let page = app.session.design().page;
        let at = at * 10.0;
        let (a, b) = match axis {
            Axis::Vertical => (
                openstitch_design::v2(at, 0.0),
                openstitch_design::v2(at, page.height),
            ),
            Axis::Horizontal => (
                openstitch_design::v2(0.0, at),
                openstitch_design::v2(page.width, at),
            ),
        };
        painter.line_segment(
            [pos(vp.to_screen(a)), pos(vp.to_screen(b))],
            Stroke::new(3.0, SECTION),
        );
    }
    for (i, s) in plan.sections.iter().enumerate() {
        let r = egui::Rect::from_two_pos(
            pos(vp.to_screen(openstitch_design::v2(s.rect.min_x, s.rect.min_y))),
            pos(vp.to_screen(openstitch_design::v2(s.rect.max_x, s.rect.max_y))),
        );
        let stroke = Stroke::new(1.5, SECTION.gamma_multiply(0.85));
        match s.kind {
            SectionKind::Split => {
                painter.rect_stroke(r, 0.0, stroke, egui::StrokeKind::Middle);
            }
            SectionKind::Position | SectionKind::JumboSide | SectionKind::Hooping => {
                let c = [
                    r.left_top(),
                    r.right_top(),
                    r.right_bottom(),
                    r.left_bottom(),
                    r.left_top(),
                ];
                painter.extend(Shape::dashed_line(&c, stroke, 6.0, 4.0));
            }
        }
        for &(a, b) in s.marks_before.iter().chain(&s.marks_after) {
            painter.add(Shape::dashed_line(
                &[pos(vp.to_screen(a)), pos(vp.to_screen(b))],
                Stroke::new(1.0, ALIGN),
                3.0,
                3.0,
            ));
        }
        // Labels sit inside the top-left corner; positions that share an area offset theirs.
        let at = r.left_top()
            + egui::vec2(
                4.0,
                4.0 + 14.0
                    * (i % 3) as f32
                    * f32::from(u8::from(matches!(
                        s.kind,
                        SectionKind::Position | SectionKind::Hooping
                    ))),
            );
        painter.text(
            at,
            egui::Align2::LEFT_TOP,
            &s.label,
            egui::FontId::proportional(12.0),
            SECTION,
        );
    }
    // Free hoopings (#515): objects that fit no hooping are outlined in red.
    let free = app
        .session
        .design()
        .machine
        .sections
        .as_ref()
        .is_some_and(|s| s.free.is_some());
    if free {
        let placed: Vec<u64> = plan
            .sections
            .iter()
            .flat_map(|s| s.objects.clone().unwrap_or_default())
            .collect();
        for o in app.session.design().objects.iter().filter(|o| o.visible) {
            if let (false, Some(b)) = (placed.contains(&o.id), o.bounds()) {
                let r = egui::Rect::from_two_pos(
                    pos(vp.to_screen(openstitch_design::v2(b.min_x, b.min_y))),
                    pos(vp.to_screen(openstitch_design::v2(b.max_x, b.max_y))),
                );
                painter.rect_stroke(
                    r.expand(2.0),
                    0.0,
                    Stroke::new(2.0, egui::Color32::from_rgb(0xd3, 0x2f, 0x2f)),
                    egui::StrokeKind::Middle,
                );
            }
        }
    }
    for e in &plan.errors {
        painter.text(
            painter.clip_rect().left_bottom() + egui::vec2(28.0, -24.0),
            egui::Align2::LEFT_BOTTOM,
            e,
            egui::FontId::proportional(12.0),
            SECTION,
        );
    }
}

/// The large-design part of the Design Settings window.
pub fn settings(app: &EditorApp, ui: &mut egui::Ui, requests: &mut Vec<CommandRequest>) {
    let units = app.ui.prefs.units;
    let d = app.session.design();
    let m = &d.machine;
    let set = |v: serde_json::Value| CommandRequest::new("sections.set", v);
    let hoop = m
        .hoop
        .as_deref()
        .and_then(openstitch_design::machine::brother_hoop);
    ui.separator();
    match hoop {
        Some(h) if h.multi_position => {
            ui.label(egui::RichText::new("Multi-position hoop").strong());
            let mut optimize = m.optimize_hoop_change;
            if ui
                .checkbox(&mut optimize, "Optimize hoop changes")
                .on_hover_text("Group patterns by installation position (a, b, c)")
                .changed()
            {
                requests.push(set(json!({ "optimizeHoopChange": optimize })));
            }
        }
        Some(h) if h.id == sections::JUMBO => {
            ui.label(egui::RichText::new("Jumbo frame").strong());
            ui.horizontal(|ui| {
                ui.label("Side overlap");
                let mut o = m.jumbo_overlap;
                let r = ui.add(crate::units_ui::drag(&mut o, units).range(0.0..=50.0));
                if r.drag_stopped() || r.lost_focus() {
                    requests.push(set(json!({ "jumboOverlap": o })));
                }
            });
        }
        Some(_) => {
            ui.label(
                egui::RichText::new("Split designs need Hoop: Custom size.")
                    .weak()
                    .small(),
            );
        }
        None => split_settings(app, ui, requests),
    }
    let n = sections::plan(d).sections.len();
    if n > 1 {
        ui.label(format!(
            "{n} hoopings · File ▸ Export Sections writes one file each"
        ));
    }
}

fn split_settings(app: &EditorApp, ui: &mut egui::Ui, requests: &mut Vec<CommandRequest>) {
    let units = app.ui.prefs.units;
    let d = app.session.design();
    let s = d.machine.sections.clone();
    let set = |v: serde_json::Value| CommandRequest::new("sections.set", v);
    ui.label(egui::RichText::new("Split design").strong());
    ui.horizontal(|ui| {
        ui.label("Section size");
        let current = s.as_ref().map(|s| s.hoop.clone()).unwrap_or_default();
        let text = s
            .as_ref()
            .and_then(|s| {
                s.section_size()
                    .map(|(w, h)| format!("{w} × {h} mm ({})", s.hoop))
            })
            .unwrap_or_else(|| "Not split".into());
        let r = egui::ComboBox::from_id_salt("section-hoop")
            .selected_text(text)
            .width(200.0)
            .show_ui(ui, |ui| {
                if ui.selectable_label(s.is_none(), "Not split").clicked() && s.is_some() {
                    requests.push(set(json!({ "hoop": "none" })));
                }
                for h in BROTHER_HOOPS.iter().filter(|h| sectionable(h)) {
                    let label = format!(
                        "{} ({} hoop)",
                        openstitch_app::editor::units::size(
                            units,
                            h.width - sections::SECTION_MARGIN_MM,
                            h.height - sections::SECTION_MARGIN_MM,
                        ),
                        openstitch_app::editor::units::hoop_size(units, h.width, h.height)
                    );
                    if ui.selectable_label(current == h.id, label).clicked() && current != h.id {
                        requests.push(set(json!({ "hoop": h.id })));
                    }
                }
            });
        crate::a11y::name(&r.response, "Section size");
    });
    let Some(s) = s else { return };
    free_settings(&s, units, ui, requests);
    if s.free.is_some() {
        return;
    }
    let mut smaller = s.smaller;
    if ui
        .checkbox(&mut smaller, "Use smaller section sizes")
        .changed()
    {
        requests.push(set(json!({ "smaller": smaller })));
    }
    // Split positions: the current ones (even split when none are set), editable in mm.
    let (xs, ys) = openstitch_app::editor::sections::current_splits(d);
    for (key, axis, values) in [
        ("splitsX", "Vertical splits", xs),
        ("splitsY", "Horizontal splits", ys),
    ] {
        if values.is_empty() {
            continue;
        }
        ui.horizontal_wrapped(|ui| {
            ui.label(axis);
            let mut edited = values.clone();
            let mut commit = false;
            for v in &mut edited {
                let r = ui.add(crate::units_ui::drag(v, units).speed(0.5));
                commit |= r.drag_stopped() || r.lost_focus();
            }
            if commit && edited != values {
                requests.push(set(json!({ key: edited })));
            }
        });
    }
    if (!s.splits_x.is_empty() || !s.splits_y.is_empty()) && ui.small_button("Even split").clicked()
    {
        requests.push(set(json!({ "splitsX": [], "splitsY": [] })));
    }
}

/// Grid (PE-DESIGN) or free hoopings (#515, beyond PE-DESIGN): the hoopings with their
/// positions, Add left / right / up / down, Auto layout, the marks and the stop.
fn free_settings(
    s: &sections::SectionSettings,
    units: openstitch_app::editor::units::Units,
    ui: &mut egui::Ui,
    requests: &mut Vec<CommandRequest>,
) {
    let set = |v: serde_json::Value| CommandRequest::new("sections.set", v);
    ui.horizontal(|ui| {
        ui.label("Hoopings");
        if ui.selectable_label(s.free.is_none(), "Grid").on_hover_text("PE-DESIGN's sections, cut at the borders").clicked() && s.free.is_some() {
            requests.push(set(json!({ "mode": "grid" })));
        }
        if ui
            .selectable_label(s.free.is_some(), "Free")
            .on_hover_text("Hoopings placed anywhere, overlapping; each object sewn whole in the first hooping that holds it")
            .clicked()
            && s.free.is_none()
        {
            requests.push(CommandRequest::new("sections.autoLayout", json!({})));
        }
    });
    let Some(f) = &s.free else { return };
    let Some((sw, sh)) = s.section_size() else {
        return;
    };
    let mut list = f.hoopings.clone();
    let mut changed = false;
    let mut remove = None;
    for (i, h) in list.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.label(format!("{}.", i + 1));
            for v in [&mut h.x, &mut h.y] {
                let r = ui.add(crate::units_ui::drag(v, units));
                changed |= r.drag_stopped() || r.lost_focus();
            }
            changed |= ui.checkbox(&mut h.rotated, "Turned 90°").changed();
            if ui.small_button("Remove").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        list.remove(i);
        changed = true;
    }
    ui.horizontal(|ui| {
        let last = list.last().copied().unwrap_or_default();
        let (w, h) = if last.rotated { (sh, sw) } else { (sw, sh) };
        let o = f.min_overlap;
        for (label, dx, dy) in [
            ("Add left", -(w - o), 0.0),
            ("Add right", w - o, 0.0),
            ("Add up", 0.0, -(h - o)),
            ("Add down", 0.0, h - o),
        ] {
            if ui.small_button(label).clicked() && list.len() < sections::MAX_HOOPINGS {
                list.push(sections::Hooping {
                    x: last.x + dx,
                    y: last.y + dy,
                    rotated: last.rotated,
                });
                changed = true;
            }
        }
        if ui
            .small_button("Auto layout")
            .on_hover_text("The fewest hoopings covering the design")
            .clicked()
        {
            requests.push(CommandRequest::new("sections.autoLayout", json!({})));
        }
    });
    if changed {
        let v: Vec<_> = list
            .iter()
            .map(|h| json!({ "x": h.x, "y": h.y, "rotated": h.rotated }))
            .collect();
        requests.push(set(json!({ "hoopings": v })));
    }
    ui.horizontal(|ui| {
        ui.label("Marks");
        for m in [
            sections::MarkStyle::BoxX,
            sections::MarkStyle::Cross,
            sections::MarkStyle::None,
        ] {
            let label = match m {
                sections::MarkStyle::BoxX => "Boxed X",
                sections::MarkStyle::Cross => "Cross",
                sections::MarkStyle::None => "None",
            };
            if ui.selectable_label(f.marks == m, label).clicked() && f.marks != m {
                requests.push(set(json!({ "marks": m.as_str() })));
            }
        }
        let mut o = f.min_overlap;
        ui.label("overlap");
        let r = ui.add(crate::units_ui::drag(&mut o, units).range(0.0..=100.0));
        if (r.drag_stopped() || r.lost_focus()) && o != f.min_overlap {
            requests.push(set(json!({ "minOverlap": o })));
        }
    });
    let mut stop = f.stop_after_marks;
    if ui
        .checkbox(&mut stop, "Stop after the marks to line the fabric up")
        .changed()
    {
        requests.push(set(json!({ "stopAfterMarks": stop })));
    }
}

/// File ▸ Export Sections: one machine file per hooping, named from the chosen base name.
pub fn export_sections(app: &mut EditorApp) {
    let name = format!("{}.pes", app.session.design().name);
    let Some(path) = rfd::FileDialog::new()
        .add_filter("PES", &["pes"])
        .add_filter("DST", &["dst"])
        .add_filter("JEF", &["jef"])
        .add_filter("EXP", &["exp"])
        .set_file_name(name)
        .save_file()
    else {
        return;
    };
    let params = json!({ "path": path.display().to_string() });
    if let Some(r) = app.execute(&CommandRequest::new("sections.export", params)) {
        let n = r["files"].as_array().map_or(0, Vec::len);
        app.status = format!("Exported {n} section files next to {}", path.display());
    }
}

/// Whether the design is sewn in more than one hooping.
pub fn has_sections(app: &EditorApp) -> bool {
    sections::plan(app.session.design()).sections.len() > 1
}
