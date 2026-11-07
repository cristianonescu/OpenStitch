//! Check Design (#503), the density map (#504) and Review Sewing Order (#505) in the editor: a
//! window opened from the status bar's "Ready to sew / N problems" indicator or the Sewing
//! Order pane's Review button, and its marks on the canvas. The numbers come from
//! `openstitch_engine::quality`, the code `design.check`, `design.density` and `design.route`
//! run for stitchtool, the MCP server and the Assistant, on the stitches the editor compiled.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, Color32, CornerRadius, Pos2, RichText, Shape, Stroke};
use openstitch_app::editor::{CommandRequest, Viewport};
use openstitch_design::{Rect, v2};
use openstitch_engine::quality::{self, DensityMap, Traced};
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::ops::Range;

const ERROR: Color32 = Color32::from_rgb(0xd3, 0x2f, 0x2f);
const WARNING: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x00);
const READY: Color32 = Color32::from_rgb(0x2e, 0x7d, 0x32);
const DARK: Color32 = Color32::from_rgb(0x16, 0x16, 0x18);
const ROUTE: Color32 = Color32::from_rgb(0x00, 0x79, 0xc1);

#[derive(Clone, Copy, PartialEq, Eq, Default, Debug)]
pub enum Tab {
    #[default]
    Check,
    Density,
    Route,
}

impl Tab {
    pub const ALL: [Tab; 3] = [Tab::Check, Tab::Density, Tab::Route];

    /// The tab's name in the Assistant's `checkDesign` setting (#599).
    pub fn key(self) -> &'static str {
        match self {
            Tab::Check => "problems",
            Tab::Density => "density",
            Tab::Route => "route",
        }
    }

    pub fn from_key(key: &str) -> Option<Tab> {
        Tab::ALL.into_iter().find(|t| t.key() == key)
    }
}

/// Opens the window on `tab`, its results brought up to date: the indicator, the Sewing Order
/// pane's Review button, and the ribbon actions the Assistant runs (#599).
pub fn open(app: &mut EditorApp, tab: Tab) {
    app.quality.open = true;
    app.quality.tab = tab;
    update(app);
}

/// What the status bar's indicator and the window show, for `ui.state` (#599): whether the
/// design is ready to sew, its error and warning counts and first reason, which tab is open
/// (`closed` when none) and the Review Sewing Order step the window is on.
pub fn state_json(app: &EditorApp) -> Value {
    let c = &app.quality.check;
    let (errors, warnings) = (
        c["errors"].as_u64().unwrap_or(0),
        c["warnings"].as_u64().unwrap_or(0),
    );
    let route = app.quality.open && app.quality.tab == Tab::Route;
    json!({
        "ready": errors + warnings == 0,
        "errors": errors,
        "warnings": warnings,
        "summary": c["summary"],
        "window": if app.quality.open { app.quality.tab.key() } else { "closed" },
        "reviewSteps": if route { json!(steps(app).len()) } else { Value::Null },
    })
}

/// Moves Review Sewing Order to step `n` (from 1), on the window's Sewing order tab, and the
/// Stitch Simulator with it (the `reviewStep` setting, #599). A design with no steps takes 1.
pub fn review_step(app: &mut EditorApp, n: usize) -> Result<(), String> {
    open(app, Tab::Route);
    let len = steps(app).len();
    if n == 0 || n > len.max(1) {
        return Err(format!(
            "reviewStep takes 1 to {} (the design's steps)",
            len.max(1)
        ));
    }
    if len > 0 {
        go_to_step(app, n - 1);
    }
    Ok(())
}

#[derive(Default)]
pub struct QualityState {
    pub open: bool,
    pub tab: Tab,
    /// Each object's stitch range in the editor's compiled pattern.
    pub spans: Vec<(u64, Range<usize>)>,
    /// Fabric Selector id the limits follow (`None`: standard).
    pub fabric: Option<String>,
    /// The revision and fabric the results are for.
    key: Option<(u64, Option<String>)>,
    map_revision: Option<u64>,
    route_revision: Option<u64>,
    pub check: Value,
    map: Option<DensityMap>,
    pub density: Value,
    pub route: Value,
    pub step: usize,
    pub route_map: bool,
    pub density_overlay: bool,
    pub dark_background: bool,
    hotspot: usize,
}

fn traced(app: &EditorApp) -> Traced {
    Traced {
        pattern: app.pattern.clone(),
        spans: app.quality.spans.clone(),
    }
}

fn limits(app: &EditorApp) -> (f64, f64) {
    quality::fabric_limits(app.quality.fabric.as_deref().unwrap_or("standard"))
        .unwrap_or((1.5, 2.5))
}

/// Brings the results up to date: the check after every change (the status bar shows it), the
/// density map and the route while they are shown. A new fabric judges the same stitches again.
pub fn update(app: &mut EditorApp) {
    let key = Some((app.revision, app.quality.fabric.clone()));
    if app.quality.key != key {
        app.quality.check =
            quality::check_json(app.session.design(), &traced(app), limits(app), None, None);
        app.quality.key = key;
        app.quality.map_revision = None;
    }
    let q = &app.quality;
    if (q.open && q.tab == Tab::Density || q.density_overlay)
        && q.map_revision != Some(app.revision)
    {
        let map = DensityMap::new(&traced(app), 1.0, None);
        app.quality.density = map.to_json("layers", limits(app));
        app.quality.map = Some(map);
        app.quality.map_revision = Some(app.revision);
        app.quality.hotspot = 0;
    }
    if app.quality.open
        && app.quality.tab == Tab::Route
        && app.quality.route_revision != Some(app.revision)
    {
        app.quality.route = quality::route(app.session.design(), &traced(app));
        app.quality.route_revision = Some(app.revision);
        app.quality.step = app.quality.step.min(steps(app).len().saturating_sub(1));
    }
}

fn findings(app: &EditorApp) -> Vec<Value> {
    app.quality.check["findings"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn steps(app: &EditorApp) -> Vec<Value> {
    app.quality.route["steps"]
        .as_array()
        .cloned()
        .unwrap_or_default()
}

fn ids(v: &Value) -> BTreeSet<u64> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_u64)
        .collect()
}

/// Selects `ids` and zooms to them.
pub fn show(app: &mut EditorApp, ids: BTreeSet<u64>) {
    if !ids.is_empty() {
        app.selection = ids;
        crate::ribbon::zoom_to_selection(app);
    }
}

/// Runs a finding's fix (`{command, params}`) as one undo step.
pub fn run_fix(app: &mut EditorApp, fix: &Value) -> bool {
    let name = fix["command"].as_str().unwrap_or_default();
    let Some(id) = openstitch_engine::commands::all()
        .into_iter()
        .map(|c| c.id)
        .find(|c| *c == name)
    else {
        return false;
    };
    app.execute(&CommandRequest::new(id, fix["params"].clone()))
        .is_some()
}

/// The status bar's indicator: "Ready to sew", or the problem count and the first reason.
pub fn indicator(app: &mut EditorApp, ui: &mut egui::Ui) {
    update(app);
    let c = &app.quality.check;
    let (errors, warnings) = (
        c["errors"].as_u64().unwrap_or(0),
        c["warnings"].as_u64().unwrap_or(0),
    );
    let text = if errors + warnings == 0 {
        RichText::new(t("ui-ready-to-sew")).color(READY)
    } else {
        let n = errors + warnings;
        let reason = c["summary"].as_str().unwrap_or_default();
        let short: String = reason.chars().take(48).collect();
        let more = if reason.chars().count() > 48 {
            "…"
        } else {
            ""
        };
        RichText::new(format!(
            "{n} problem{}: {short}{more}",
            if n == 1 { "" } else { "s" }
        ))
        .color(if errors > 0 { ERROR } else { WARNING })
    };
    if ui
        .button(text)
        .on_hover_text(t("ui-check-design-open-the-list-of-tip"))
        .clicked()
    {
        open(app, Tab::Check);
    }
}

pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.quality.open {
        return;
    }
    update(app);
    let mut open = true;
    egui::Window::new("Check Design")
        .open(&mut open)
        .default_size([380.0, 420.0])
        .show(ctx, |ui| {
            ui.horizontal(|ui| {
                for (t, label) in [
                    (Tab::Check, "Problems"),
                    (Tab::Density, "Density map"),
                    (Tab::Route, "Sewing order"),
                ] {
                    ui.selectable_value(&mut app.quality.tab, t, label);
                }
            });
            ui.horizontal(|ui| {
                ui.label(t("ui-fabric"));
                let current = app
                    .quality
                    .fabric
                    .clone()
                    .unwrap_or_else(|| "standard".into());
                egui::ComboBox::from_id_salt("quality-fabric")
                    .selected_text(&current)
                    .show_ui(ui, |ui| {
                        for f in openstitch_engine::fabric::FABRICS {
                            if ui.selectable_label(current == f.id, f.name).clicked() {
                                app.quality.fabric = Some(f.id.to_string());
                            }
                        }
                    });
            });
            ui.separator();
            update(app);
            match app.quality.tab {
                Tab::Check => check_tab(app, ui),
                Tab::Density => density_tab(app, ui),
                Tab::Route => route_tab(app, ui),
            }
        });
    app.quality.open = open;
}

fn check_tab(app: &mut EditorApp, ui: &mut egui::Ui) {
    let list = findings(app);
    if list.is_empty() {
        ui.label(RichText::new(t("ui-ready-to-sew-no-problems-found")).color(READY));
        return;
    }
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, f) in list.iter().enumerate() {
            let error = f["severity"] == "error";
            ui.horizontal_wrapped(|ui| {
                let tag = if error { "Error" } else { "Warning" };
                ui.label(
                    RichText::new(tag)
                        .strong()
                        .color(if error { ERROR } else { WARNING }),
                );
                let text = format!(
                    "{} ({} / {})",
                    f["message"].as_str().unwrap_or_default(),
                    f["value"],
                    f["limit"]
                );
                if ui
                    .selectable_label(false, text)
                    .on_hover_text(t("ui-select-and-show-tip"))
                    .clicked()
                {
                    show(app, ids(&f["ids"]));
                }
                if f["fix"].is_object()
                    && ui
                        .button(t("ui-fix"))
                        .on_hover_text(f["fix"]["command"].as_str().unwrap_or_default())
                        .clicked()
                {
                    run_fix(app, &f["fix"]);
                }
            });
            if i + 1 < list.len() {
                ui.separator();
            }
        }
    });
}

fn density_tab(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.checkbox(
        &mut app.quality.density_overlay,
        "Show the density map on the design",
    );
    ui.checkbox(
        &mut app.quality.dark_background,
        "Dark background (review light threads)",
    );
    let d = app.quality.density.clone();
    let (heavy, dense) = (
        d["limits"]["heavy"].as_f64().unwrap_or(0.0),
        d["limits"]["tooDense"].as_f64().unwrap_or(0.0),
    );
    ui.horizontal_wrapped(|ui| {
        ui.label(t("ui-thread-layers-1-one-fill"));
        ui.label(RichText::new(format!("heavy above {heavy}")).color(WARNING));
        ui.label(RichText::new(format!("too dense above {dense}")).color(ERROR));
    });
    let share = |k: &str| d["classes"][k].as_f64().unwrap_or(0.0) * 100.0;
    ui.label(format!(
        "Sewn area: {:.0} % thin, {:.0} % ok, {:.0} % heavy, {:.0} % too dense",
        share("thin"),
        share("ok"),
        share("heavy"),
        share("tooDense")
    ));
    let hot = d["hotspots"].as_array().cloned().unwrap_or_default();
    if hot.is_empty() {
        ui.label(t("ui-no-hotspots"));
        return;
    }
    let k = app.quality.hotspot.min(hot.len() - 1);
    let h = &hot[k];
    ui.label(format!(
        "Hotspot {} of {}: {} layers, {} drops per mm², objects {}",
        k + 1,
        hot.len(),
        h["maxLayers"],
        h["maxHolesPerMm2"],
        h["ids"]
    ));
    ui.horizontal(|ui| {
        if ui.button(t("ui-show")).clicked() {
            show(app, ids(&h["ids"]));
        }
        if ui.button(t("ui-next")).clicked() {
            app.quality.hotspot = (k + 1) % hot.len();
            show(app, ids(&hot[app.quality.hotspot]["ids"]));
        }
    });
}

/// Moves the review to `step` and the Stitch Simulator to its end.
pub fn go_to_step(app: &mut EditorApp, step: usize) {
    let all = steps(app);
    if let Some(s) = all.get(step) {
        app.quality.step = step;
        app.simulate = s["drops"].as_u64().map(|n| n as usize);
        app.selection = ids(&json!([s["id"]]));
    }
}

fn route_tab(app: &mut EditorApp, ui: &mut egui::Ui) {
    let all = steps(app);
    let t = &app.quality.route["totals"];
    ui.label(format!(
        "{} steps, {} trims ({} per 1,000 stitches), {} thread changes, {} mm of travel, {} warnings",
        all.len(),
        t["trims"],
        t["trimsPer1000"],
        t["colorChanges"],
        t["travelMm"],
        t["warnings"]
    ));
    ui.horizontal(|ui| {
        ui.selectable_value(&mut app.quality.route_map, false, "Step by step");
        ui.selectable_value(&mut app.quality.route_map, true, "Route map");
    });
    if all.is_empty() {
        return;
    }
    let k = app.quality.step.min(all.len() - 1);
    ui.horizontal(|ui| {
        if ui
            .add_enabled(k > 0, egui::Button::new(openstitch_app::editor::ui_texts::t("ui-previous")))
            .clicked()
        {
            go_to_step(app, k - 1);
        }
        if ui
            .add_enabled(k + 1 < all.len(), egui::Button::new(openstitch_app::editor::ui_texts::t("ui-next")))
            .clicked()
        {
            go_to_step(app, k + 1);
        }
        if ui.button(openstitch_app::editor::ui_texts::t("ui-whole-design")).clicked() {
            app.simulate = None;
        }
    });
    egui::ScrollArea::vertical().show(ui, |ui| {
        for (i, s) in all.iter().enumerate() {
            let name = s["name"]
                .as_str()
                .map_or_else(|| format!("object {}", s["id"]), str::to_string);
            let text = format!(
                "{}. {name}: {}",
                i + 1,
                s["move"]["label"].as_str().unwrap_or_default()
            );
            if ui.selectable_label(i == k, text).clicked() {
                go_to_step(app, i);
            }
            for w in s["warnings"].as_array().into_iter().flatten() {
                ui.horizontal(|ui| {
                    let what = if w["rule"] == "untrimmedJump" {
                        "Untrimmed jump"
                    } else {
                        "Long move in the same thread"
                    };
                    if ui
                        .selectable_label(
                            false,
                            RichText::new(format!("    {what}: {} mm", w["valueMm"]))
                                .color(WARNING),
                        )
                        .clicked()
                    {
                        show(app, ids(&json!([s["id"], s["move"]["to"]])));
                    }
                    if w["fix"].is_object() && ui.button(openstitch_app::editor::ui_texts::t("ui-fix")).clicked() {
                        run_fix(app, &w["fix"]);
                    }
                });
            }
        }
    });
}

fn pos(vp: &Viewport, mm: &Value) -> Pos2 {
    let p = vp.to_screen(v2(
        mm[0].as_f64().unwrap_or(0.0) * 10.0,
        mm[1].as_f64().unwrap_or(0.0) * 10.0,
    ));
    Pos2::new(p.x as f32, p.y as f32)
}

/// The marks [`draw`] puts on the canvas, for a rendered picture (`ui.render`, #769): the
/// density overlay's cells while it is on, and Check Design's rings and out-of-hoop stitches
/// while its window shows the Check tab. Brings the results up to date first.
pub fn render_marks(app: &mut EditorApp) -> openstitch_engine::preview::Marks {
    update(app);
    let mut marks = openstitch_engine::preview::Marks::default();
    let q = &app.quality;
    if q.density_overlay
        && let Some(map) = &q.map
    {
        let s = map.cell * 10.0;
        for k in 0..map.layers.len() {
            let rgba = match map.class(k, limits(app)) {
                Some(3) => [0xd3, 0x2f, 0x2f, 120],
                Some(2) => [0xe0, 0x8a, 0x00, 90],
                _ => continue,
            };
            let x = (k % map.width) as f64 * s + map.origin.x;
            let y = (k / map.width) as f64 * s + map.origin.y;
            marks.cells.push(([x, y, s, s], rgba));
        }
    }
    if q.open && q.tab == Tab::Check {
        let rgb = |c: Color32| [c.r(), c.g(), c.b()];
        let page = app.session.design().page;
        let out =
            |x: f64, y: f64| x < -0.5 || y < -0.5 || x > page.width + 0.5 || y > page.height + 0.5;
        for w in app.pattern.stitches.windows(2) {
            if w[1].command == openstitch_core::Command::Stitch
                && (out(w[0].x, w[0].y) || out(w[1].x, w[1].y))
            {
                marks.lines.push(([w[0].x, w[0].y, w[1].x, w[1].y], rgb(ERROR)));
            }
        }
        for f in findings(app) {
            let b = &f["bbox"];
            let Some(v) = b.as_array().filter(|v| v.len() == 4) else {
                continue;
            };
            let n: Vec<f64> = v.iter().map(|x| x.as_f64().unwrap_or(0.0) * 10.0).collect();
            let (cx, cy) = ((n[0] + n[2]) / 2.0, (n[1] + n[3]) / 2.0);
            // Half the box's diagonal and 1.5 mm, as the canvas ring sits round the problem.
            let r = ((n[2] - n[0]).hypot(n[3] - n[1]) / 2.0).max(8.0) + 15.0;
            let colour = if f["severity"] == "error" { ERROR } else { WARNING };
            marks.rings.push(([cx, cy, r], rgb(colour)));
        }
    }
    marks
}

/// Under the stitches: the dark background for light threads.
pub fn draw_background(app: &EditorApp, painter: &egui::Painter, vp: &Viewport, page: &Rect) {
    if app.quality.dark_background {
        let a = vp.to_screen(v2(page.min_x, page.min_y));
        let b = vp.to_screen(v2(page.max_x, page.max_y));
        let r = egui::Rect::from_two_pos(
            Pos2::new(a.x as f32, a.y as f32),
            Pos2::new(b.x as f32, b.y as f32),
        );
        painter.rect_filled(r, CornerRadius::ZERO, DARK);
    }
}

/// Over the stitches: the density map, rings round each problem and stitches past the hoop in
/// red while the Check Design window is open, and the sewing-order review.
pub fn draw(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let q = &app.quality;
    if q.density_overlay
        && let Some(map) = &q.map
    {
        let (heavy, dense) = limits(app);
        let s = map.cell * 10.0;
        for k in 0..map.layers.len() {
            let fill = match map.class(k, (heavy, dense)) {
                Some(3) => Color32::from_rgba_unmultiplied(0xd3, 0x2f, 0x2f, 120),
                Some(2) => Color32::from_rgba_unmultiplied(0xe0, 0x8a, 0x00, 90),
                _ => continue,
            };
            let (x, y) = (
                (k % map.width) as f64 * s + map.origin.x,
                (k / map.width) as f64 * s + map.origin.y,
            );
            let (a, b) = (vp.to_screen(v2(x, y)), vp.to_screen(v2(x + s, y + s)));
            painter.rect_filled(
                egui::Rect::from_two_pos(
                    Pos2::new(a.x as f32, a.y as f32),
                    Pos2::new(b.x as f32, b.y as f32),
                ),
                CornerRadius::ZERO,
                fill,
            );
        }
    }
    if !q.open {
        return;
    }
    if q.tab == Tab::Check {
        let page = app.session.design().page;
        let out =
            |x: f64, y: f64| x < -0.5 || y < -0.5 || x > page.width + 0.5 || y > page.height + 0.5;
        for w in app.pattern.stitches.windows(2) {
            if w[1].command == openstitch_core::Command::Stitch
                && (out(w[0].x, w[0].y) || out(w[1].x, w[1].y))
            {
                let (a, b) = (
                    vp.to_screen(v2(w[0].x, w[0].y)),
                    vp.to_screen(v2(w[1].x, w[1].y)),
                );
                painter.line_segment(
                    [
                        Pos2::new(a.x as f32, a.y as f32),
                        Pos2::new(b.x as f32, b.y as f32),
                    ],
                    Stroke::new(2.0, ERROR),
                );
            }
        }
        for f in findings(app) {
            let b = &f["bbox"];
            if b.is_array() {
                let (a, c) = (pos(vp, &json!([b[0], b[1]])), pos(vp, &json!([b[2], b[3]])));
                let colour = if f["severity"] == "error" {
                    ERROR
                } else {
                    WARNING
                };
                painter.circle_stroke(
                    a.lerp(c, 0.5),
                    (a.distance(c) / 2.0).max(8.0) + 6.0,
                    Stroke::new(2.0, colour),
                );
            }
        }
    }
    if q.tab == Tab::Route {
        draw_route(app, painter, vp);
    }
}

fn draw_route(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let all = steps(app);
    let font = egui::FontId::proportional(12.0);
    let style = |kind: &Value| match kind.as_str() {
        Some("jump") => Stroke::new(2.0, ERROR),
        Some("colorChange") => Stroke::new(1.5, ROUTE),
        _ => Stroke::new(1.0, Color32::from_gray(110)),
    };
    let arrow = |from: Pos2, to: Pos2, stroke: Stroke| {
        painter.add(Shape::dashed_line(&[from, to], stroke, 5.0, 3.0));
        let d = (to - from).normalized() * 8.0;
        let n = egui::vec2(-d.y, d.x) * 0.5;
        painter.add(Shape::convex_polygon(
            vec![to, to - d + n, to - d - n],
            stroke.color,
            Stroke::NONE,
        ));
    };
    if app.quality.route_map {
        // Numbered starts, labels moved down until they clear the ones already placed.
        let mut placed: Vec<Pos2> = Vec::new();
        for (i, s) in all.iter().enumerate() {
            let mut at = pos(vp, &s["start"]);
            while placed.iter().any(|p| p.distance(at) < 14.0) {
                at.y += 14.0;
            }
            placed.push(at);
            painter.text(
                at,
                egui::Align2::CENTER_CENTER,
                format!("{}", i + 1),
                font.clone(),
                ROUTE,
            );
            if let Some(next) = all.get(i + 1) {
                arrow(
                    pos(vp, &s["end"]),
                    pos(vp, &next["start"]),
                    style(&s["move"]["kind"]),
                );
            }
        }
        return;
    }
    let k = app.quality.step.min(all.len().saturating_sub(1));
    let Some(s) = all.get(k) else { return };
    let (start, end) = (pos(vp, &s["start"]), pos(vp, &s["end"]));
    let mark = Stroke::new(2.0, ROUTE);
    painter.line_segment(
        [start - egui::vec2(7.0, 0.0), start + egui::vec2(7.0, 0.0)],
        mark,
    );
    painter.line_segment(
        [start - egui::vec2(0.0, 7.0), start + egui::vec2(0.0, 7.0)],
        mark,
    );
    painter.line_segment(
        [end - egui::vec2(6.0, 6.0), end + egui::vec2(6.0, 6.0)],
        mark,
    );
    painter.line_segment(
        [end - egui::vec2(6.0, -6.0), end + egui::vec2(6.0, -6.0)],
        mark,
    );
    if let Some(prev) = k.checked_sub(1).and_then(|j| all.get(j)) {
        let from = pos(vp, &prev["end"]);
        arrow(from, start, style(&prev["move"]["kind"]));
        let label = prev["move"]["label"].as_str().unwrap_or_default();
        painter.text(
            from.lerp(start, 0.5) + egui::vec2(0.0, -10.0),
            egui::Align2::CENTER_BOTTOM,
            label,
            font,
            ROUTE,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    fn app_with(shapes: &[Value]) -> EditorApp {
        let mut app = EditorApp::new(Launch::default());
        app.execute(&CommandRequest::new(
            "machine.set",
            json!({"hoop": "100x100"}),
        ));
        for s in shapes {
            app.execute(&CommandRequest::new("shape.rectangle", s.clone()))
                .expect("shape");
        }
        update(&mut app);
        app
    }

    #[test]
    fn status_rows_and_fixes_follow_the_engine() {
        let mut app =
            app_with(&[json!({"x": 10, "y": 10, "width": 20, "height": 20, "fill": "Red"})]);
        assert_eq!(app.quality.check["summary"], json!("Ready to sew"));
        // A 13 mm satin (error) and a thin fill (warning): errors first.
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({"x": 40, "y": 20, "width": 2, "height": 1, "fill": "Red"}),
        ));
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({"x": 50, "y": 20, "width": 13, "height": 40, "fill": "Red"}),
        ));
        app.execute(&CommandRequest::new(
            "object.setRegion",
            json!({"ids": [3], "type": "satin"}),
        ));
        update(&mut app);
        let list = findings(&app);
        assert_eq!(list.len(), 2, "{list:?}");
        assert_eq!(list[0]["severity"], json!("error"));
        assert_eq!(list[1]["rule"], json!("fillTooThin"));
        // The same as design.check.
        let engine = app
            .session
            .execute("design.check", Some(&json!({})))
            .expect("value");
        assert_eq!(engine, app.quality.check);
        // A row selects its object; its fix clears it in one undo step.
        show(&mut app, ids(&list[0]["ids"]));
        assert_eq!(app.selection, BTreeSet::from([3]));
        let before = app.session.design().clone();
        assert!(run_fix(&mut app, &list[0]["fix"]));
        update(&mut app);
        assert!(!findings(&app).iter().any(|f| f["rule"] == "satinTooWide"));
        app.undo();
        assert_eq!(app.session.design(), &before);
    }

    #[test]
    fn density_overlay_uses_the_engine_grid_and_the_fabric() {
        let shape = json!({"x": 10, "y": 10, "width": 20, "height": 20, "fill": "Red"});
        let mut app = app_with(&[shape.clone(), shape.clone(), shape]);
        app.quality.density_overlay = true;
        update(&mut app);
        let engine = app
            .session
            .execute("design.density", Some(&json!({})))
            .expect("value");
        assert_eq!(engine, app.quality.density);
        let revision = app.revision;
        let standard = app.quality.density["classes"].clone();
        app.quality.fabric = Some("heavy".into());
        update(&mut app);
        app.quality.dark_background = true;
        assert_eq!(app.revision, revision, "only the view changes");
        assert_ne!(app.quality.density["classes"], standard, "{standard}");
    }

    /// #769, #786: `ui.render` shows the density overlay while it is on, and Check Design's marks
    /// while its Check tab is open, as the canvas does; off, the picture is as before.
    #[test]
    fn ui_render_shows_the_density_overlay_and_check_marks() {
        use base64::Engine as _;
        let shape = json!({"x": 10, "y": 10, "width": 20, "height": 20, "fill": "Red"});
        let mut app = app_with(&[shape.clone(), shape.clone(), shape]);
        let render = |app: &mut EditorApp| {
            let r = app
                .ui_command("ui.render", Some(&json!({ "size": 400 })))
                .expect("render");
            base64::engine::general_purpose::STANDARD
                .decode(r["png"].as_str().expect("png"))
                .expect("base64")
        };
        let plain = render(&mut app);
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "densityOverlay", "value": true })),
        )
        .expect("overlay");
        assert!(!render_marks(&mut app).cells.is_empty(), "three fills: dense cells");
        let overlay = render(&mut app);
        assert_ne!(plain, overlay, "the overlay shows in the picture");
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "densityOverlay", "value": false })),
        )
        .expect("overlay off");
        assert_eq!(render(&mut app), plain, "off: as before");

        // Check Design (#786): a fill that runs off the 100 × 100 mm page is sure to give marks.
        // Opened the way a model opens it, through ui.run.
        let off_page = json!({"x": 85, "y": 40, "width": 30, "height": 20, "fill": "Blue"});
        let mut app = app_with(&[off_page]);
        let closed = render(&mut app);
        assert!(render_marks(&mut app).is_empty(), "nothing marked while the window is closed");
        app.ui_command("ui.run", Some(&json!({ "action": "checkDesign" })))
            .expect("checkDesign");
        assert!(app.quality.open && app.quality.tab == Tab::Check);
        let marks = render_marks(&mut app);
        assert!(!marks.lines.is_empty(), "stitches past the page are drawn in red");
        assert_eq!(
            marks.rings.len(),
            findings(&app).iter().filter(|f| f["bbox"].is_array()).count(),
            "one ring per finding with a place"
        );
        assert_ne!(render(&mut app), closed, "the marks show in the picture");
    }

    #[test]
    fn review_steps_move_the_simulator() {
        let mut app = app_with(&[
            json!({"x": 10, "y": 10, "width": 10, "height": 10, "fill": "Red"}),
            json!({"x": 40, "y": 10, "width": 10, "height": 10, "fill": "Blue"}),
        ]);
        app.quality.open = true;
        app.quality.tab = Tab::Route;
        update(&mut app);
        assert_eq!(steps(&app).len(), 2);
        go_to_step(&mut app, 0);
        let first = app.simulate.expect("simulate");
        assert!(first > 0 && first < app.stats.stitches);
        assert_eq!(app.selection, BTreeSet::from([1]));
        go_to_step(&mut app, 1);
        assert_eq!(app.simulate, Some(app.stats.stitches));
        assert_eq!(steps(&app)[0]["move"]["kind"], json!("colorChange"));
    }
}
