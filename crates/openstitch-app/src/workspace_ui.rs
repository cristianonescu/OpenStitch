//! Workspace: Design Settings window (machine type, hoops, user hoops, sewing area, jump-stitch
//! trimming), rulers, grid, measure tool, Reference window and realistic thread drawing. Only
//! the Design Settings window changes the design, through `machine.*` commands.

use openstitch_app::editor::ui_texts::t;
use crate::app::EditorApp;
use eframe::egui::{self, Color32, Pos2, Sense, Shape, Stroke};
use openstitch_app::editor::units::{self, Units, ruler_ticks};
use openstitch_app::editor::workspace::{GridSettings, MeasureTool, ReferenceMap};
use openstitch_app::editor::{CommandRequest, ScreenRect, Viewport};
use openstitch_design::{BRANDS, Estimate, MachineType, Rect, SewingArea, Vec2, v2};
use serde_json::json;

pub const RULER: f32 = 20.0;

/// View state for the workspace aids.
pub struct Workspace {
    pub grid: GridSettings,
    pub rulers: bool,
    pub reference: bool,
    pub realistic: bool,
    /// Solid View (never on together with `realistic`).
    pub solid: bool,
    /// View ▸ Guideline: guidelines shown and added from the rulers.
    pub guides: bool,
    /// View, Image and Text dialogs and the guideline being dragged.
    pub view: crate::view_ui::ViewState,
    pub measure: MeasureTool,
    /// Hoop Settings ▸ Move Sections.
    pub sections: openstitch_app::editor::sections::SectionMover,
    pub settings_open: bool,
    user_hoop: (f64, f64, String),
    /// Thread length and sewing time of the compiled design.
    pub estimate: Option<Estimate>,
    /// Where the canvas was drawn last frame (for the Reference window).
    pub canvas: Option<egui::Rect>,
    /// File ▸ Send to USB Media.
    pub usb: crate::transfer_ui::UsbDialog,
}

impl Default for Workspace {
    fn default() -> Self {
        Workspace {
            grid: GridSettings::default(),
            rulers: true,
            reference: false,
            realistic: false,
            solid: false,
            // The owner's default (#799): guidelines shown.
            guides: true,
            view: Default::default(),
            measure: MeasureTool::default(),
            sections: Default::default(),
            settings_open: false,
            user_hoop: (100.0, 100.0, String::new()),
            estimate: None,
            canvas: None,
            usb: Default::default(),
        }
    }
}

fn pos(p: Vec2) -> Pos2 {
    Pos2::new(p.x as f32, p.y as f32)
}

pub fn screen_rect(r: egui::Rect) -> ScreenRect {
    ScreenRect {
        x: f64::from(r.min.x),
        y: f64::from(r.min.y),
        width: f64::from(r.width()),
        height: f64::from(r.height()),
    }
}

// MARK: canvas overlays

/// Rulers along the top and left edge of the canvas, in the chosen units from `zero` (0.1 mm,
/// page coordinates: the Design Page's centre or its top-left corner, #799), with the pointer
/// position marked.
pub fn draw_rulers(
    painter: &egui::Painter,
    rect: egui::Rect,
    vp: &Viewport,
    hover: Option<Pos2>,
    units: Units,
    zero: openstitch_design::Vec2,
) {
    let colors = crate::theme::canvas(painter.ctx());
    let (bg, fg) = (colors.ruler_bg, colors.ruler_fg);
    let top = egui::Rect::from_min_max(rect.min, Pos2::new(rect.max.x, rect.min.y + RULER));
    let left = egui::Rect::from_min_max(rect.min, Pos2::new(rect.min.x + RULER, rect.max.y));
    painter.rect_filled(top, 0.0, bg);
    painter.rect_filled(left, 0.0, bg);
    let font = egui::FontId::monospace(9.0);
    let a = vp.to_model(v2(f64::from(rect.min.x), f64::from(rect.min.y)));
    let b = vp.to_model(v2(f64::from(rect.max.x), f64::from(rect.max.y)));
    for t in ruler_ticks(units, a.x - zero.x, b.x - zero.x, vp.scale, 6.0) {
        let x = vp.to_screen(v2(t.at + zero.x, 0.0)).x as f32;
        if x < left.max.x {
            continue;
        }
        let len = if t.major { RULER * 0.6 } else { RULER * 0.25 };
        painter.line_segment(
            [Pos2::new(x, top.max.y - len), Pos2::new(x, top.max.y)],
            Stroke::new(1.0, fg),
        );
        if let Some(l) = t.label.as_deref().map(ruler_label) {
            painter.text(
                Pos2::new(x + 2.0, top.min.y + 1.0),
                egui::Align2::LEFT_TOP,
                l,
                font.clone(),
                fg,
            );
        }
    }
    for t in ruler_ticks(units, a.y - zero.y, b.y - zero.y, vp.scale, 6.0) {
        let y = vp.to_screen(v2(0.0, t.at + zero.y)).y as f32;
        if y < top.max.y {
            continue;
        }
        let len = if t.major { RULER * 0.6 } else { RULER * 0.25 };
        painter.line_segment(
            [Pos2::new(left.max.x - len, y), Pos2::new(left.max.x, y)],
            Stroke::new(1.0, fg),
        );
        if let Some(l) = t.label.as_deref().map(ruler_label) {
            painter.text(
                Pos2::new(left.min.x + 1.0, y + 2.0),
                egui::Align2::LEFT_TOP,
                l,
                font.clone(),
                fg,
            );
        }
    }
    painter.rect_filled(
        egui::Rect::from_min_size(rect.min, egui::vec2(RULER, RULER)),
        0.0,
        bg,
    );
    if let Some(h) = hover {
        let mark = Stroke::new(1.0, Color32::from_rgb(0xff, 0x8a, 0x3d));
        painter.line_segment([Pos2::new(h.x, top.min.y), Pos2::new(h.x, top.max.y)], mark);
        painter.line_segment(
            [Pos2::new(left.min.x, h.y), Pos2::new(left.max.x, h.y)],
            mark,
        );
    }
}

/// A ruler label without its sign: PE-DESIGN 11's rulers count up both ways from 0 at the page
/// centre (… 20 10 0 10 20 …; #799). The cursor position and the guide labels keep theirs.
fn ruler_label(l: &str) -> &str {
    l.trim_start_matches(['-', '\u{2212}'])
}

/// Grid lines on the Design Page from the grid settings: solid with axes, else dotted.
pub fn draw_grid(painter: &egui::Painter, vp: &Viewport, page: &Rect, grid: &GridSettings) {
    let colors = crate::theme::canvas(painter.ctx());
    let (minor, major) = (colors.grid_minor, colors.grid_major);
    let line = |a: Pos2, b: Pos2, m: bool| {
        let c = if m { major } else { minor };
        if grid.axes {
            painter.line_segment([a, b], Stroke::new(1.0, c));
        } else {
            painter.extend(Shape::dotted_line(&[a, b], c, 4.0, 0.8));
        }
    };
    for (x, m) in grid.lines(page.max_x, vp.scale, 6.0) {
        line(
            pos(vp.to_screen(v2(x, 0.0))),
            pos(vp.to_screen(v2(x, page.max_y))),
            m,
        );
    }
    for (y, m) in grid.lines(page.max_y, vp.scale, 6.0) {
        line(
            pos(vp.to_screen(v2(0.0, y))),
            pos(vp.to_screen(v2(page.max_x, y))),
            m,
        );
    }
}

/// The window's MSAA sample count (desktop; the browser page draws without it).
pub const MSAA_SAMPLES: u16 = 4;
/// Set when the window draws with [`MSAA_SAMPLES`]: the Realistic view then draws its threads
/// as one mesh without feathered edges (#907).
pub static MSAA: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Thread segments as quads without feathering, for a window drawn with MSAA (#907). Feathered
/// edges are partly transparent: where two rows' edges meet, each lets some fabric through,
/// and rows 1–2 px apart turn that into light stripes. Under MSAA every sample is covered by a
/// row or not, so the rows' seams close and the average colour follows the covered area.
#[derive(Default)]
pub struct ThreadMesh(pub egui::Mesh);

impl ThreadMesh {
    /// A segment from `a` to `b`, `w` points wide, with square ends at `a` and `b`.
    pub fn segment(&mut self, a: Pos2, b: Pos2, w: f32, color: Color32) {
        let d = b - a;
        let len = d.length();
        if len < 1e-4 {
            return;
        }
        let n = egui::vec2(-d.y, d.x) * (w * 0.5 / len);
        let i = self.0.vertices.len() as u32;
        for p in [a + n, a - n, b - n, b + n] {
            self.0.colored_vertex(p, color);
        }
        self.0.add_triangle(i, i + 1, i + 2);
        self.0.add_triangle(i, i + 2, i + 3);
    }
}

/// Threads narrower than this many pixels are drawn in one colour under MSAA (#907).
pub const FINE_PX: f32 = 3.0;

/// Colours mixed by the share of a pixel each covers, averaged as the view's own pixels average
/// when the same stitches are drawn large enough to show their shading (sRGB values, as egui
/// blends them).
pub fn mix_by_area(parts: &[(Color32, f32)]) -> Color32 {
    let total: f32 = parts.iter().map(|p| p.1).sum();
    let m = |f: fn(&Color32) -> u8| {
        (parts.iter().map(|(c, w)| f32::from(f(c)) * w).sum::<f32>() / total).round() as u8
    };
    Color32::from_rgb(m(Color32::r), m(Color32::g), m(Color32::b))
}

/// Stitches drawn as thread: true width, a dark edge, the colour and a highlight, so rows read
/// like sewn thread rather than lines (width, contrast and brightness from Realistic View
/// Settings).
pub fn draw_realistic(app: &EditorApp, painter: &egui::Painter, vp: &Viewport) {
    let msaa = MSAA.load(std::sync::atomic::Ordering::Relaxed);
    let mut mesh = ThreadMesh::default();
    let line = |mesh: &mut ThreadMesh, a: Pos2, b: Pos2, w: f32, c: Color32| {
        if msaa {
            mesh.segment(a, b, w, c);
        } else {
            painter.line_segment([a, b], Stroke::new(w, c));
        }
    };
    let mut dots = Vec::new();
    let settings = app.ui.prefs.realistic;
    let w = (settings.thread_width_mm * 10.0 * vp.scale).max(0.8) as f32;
    for layer in &app.render.layers {
        let [edge, body, light] = settings
            .shades(layer.rgb)
            .map(|c| Color32::from_rgb(c[0], c[1], c[2]));
        // As the shading shows at a few pixels wide: the edge on 0.3 of the width, the body
        // on 0.7, the highlight on a quarter of the width along the middle 60 % (0.15).
        let fine = mix_by_area(&[(edge, 0.3), (body, 0.55), (light, 0.15)]);
        // Thread drawn wider or narrower than the default reaches as much further off its line.
        let scale = settings.thread_width_mm / openstitch_design::calibration::THREAD_MM;
        for (n, s) in layer.strokes.iter().enumerate() {
            let cover = layer.cover.get(n);
            let shift = layer.shift.get(n);
            for (k, pair) in s.windows(2).enumerate() {
                // Off centre towards the wider gap beside it (#893), so rows on the machine's
                // 0.1 mm grid leave no bare stripe.
                let off = f64::from(shift.and_then(|c| c.get(k)).copied().unwrap_or(0.0)) * scale;
                let d = pair[1] - pair[0];
                let len = d.length();
                let o = if len > 1e-9 && off != 0.0 {
                    v2(-d.y, d.x) * (off / len)
                } else {
                    v2(0.0, 0.0)
                };
                let (a, b) = (
                    pos(vp.to_screen(pair[0] + o)),
                    pos(vp.to_screen(pair[1] + o)),
                );
                // As wide as sewn thread covers at this stitch's row density (#291).
                let w = (w * cover.and_then(|c| c.get(k)).copied().unwrap_or(1.0)).max(0.8);
                if msaa && w < FINE_PX {
                    // Under a few pixels the edge and body shading is finer than a pixel and
                    // would alias into bands: one colour, their mix by area (#907).
                    line(&mut mesh, a, b, w, fine);
                } else {
                    line(&mut mesh, a, b, w, edge);
                    line(&mut mesh, a, b, w * 0.7, body);
                    if w >= 2.5 {
                        // The highlight sits towards the middle of each stitch, as light
                        // catches the top of the thread.
                        let d = b - a;
                        line(&mut mesh, a + d * 0.2, b - d * 0.2, w * 0.25, light);
                    }
                }
            }
            // Needle penetrations show as small dark dots between stitches.
            if w >= 3.0 {
                dots.extend(s.iter().map(|&p| (pos(vp.to_screen(p)), w * 0.18, edge)));
            }
        }
        if msaa {
            // One layer's mesh, then its needle dots, keeps the colours' stacking order.
            painter.add(Shape::mesh(std::mem::take(&mut mesh.0)));
        }
        for (p, r, c) in dots.drain(..) {
            painter.circle_filled(p, r, c);
        }
    }
}

/// The measure line and its readout.
pub fn draw_measure(painter: &egui::Painter, vp: &Viewport, tool: &MeasureTool, units: Units) {
    let Some(m) = tool.current else { return };
    let (a, b) = (pos(vp.to_screen(m.from)), pos(vp.to_screen(m.to)));
    let c = Color32::from_rgb(0xe6, 0x3b, 0x7a);
    painter.add(Shape::dashed_line(&[a, b], Stroke::new(1.5, c), 6.0, 3.0));
    painter.circle_stroke(a, 3.0, Stroke::new(1.5, c));
    painter.circle_stroke(b, 3.0, Stroke::new(1.5, c));
    let text = m.label_in(units);
    let galley = painter.layout_no_wrap(text, egui::FontId::monospace(12.0), Color32::WHITE);
    let size = galley.size();
    // Beside the end point, flipped to the other side where it would leave the canvas.
    let clip = painter.clip_rect();
    let x = if b.x + 14.0 + size.x > clip.max.x {
        b.x - 14.0 - size.x
    } else {
        b.x + 14.0
    };
    let y = (b.y - 14.0 - size.y).max(clip.min.y + RULER + 6.0);
    let r = egui::Rect::from_min_size(Pos2::new(x, y), size).expand(4.0);
    painter.rect_filled(r, 3.0, c.gamma_multiply(0.9));
    painter.galley(r.min + egui::vec2(4.0, 4.0), galley, Color32::WHITE);
}

// MARK: windows

/// The Reference window: whole Design Page with the stitches and the canvas's visible area;
/// click or drag to move the canvas there.
pub fn reference_window(app: &mut EditorApp, ctx: &egui::Context, canvas: Option<egui::Rect>) {
    let mut open = app.workspace.reference;
    egui::Window::new(openstitch_app::editor::ui_texts::t("ui-window-reference")).id(egui::Id::unique("window-reference"))
        .open(&mut open)
        .default_size([220.0, 220.0])
        .resizable(true)
        .show(ctx, |ui| {
            let size = ui.available_size().max(egui::vec2(120.0, 120.0));
            let (rect, response) = ui.allocate_exact_size(size, Sense::click_and_drag());
            let page = app.session.design().page;
            let page_rect = Rect {
                min_x: 0.0,
                min_y: 0.0,
                max_x: page.width,
                max_y: page.height,
            };
            let map = ReferenceMap::new(page_rect, screen_rect(rect));
            let painter = ui.painter_at(rect);
            let colors = crate::theme::canvas(ui.ctx());
            painter.rect_filled(rect, 0.0, colors.work_area);
            let p = &map.thumb;
            painter.rect_filled(
                egui::Rect::from_two_pos(
                    pos(p.to_screen(v2(0.0, 0.0))),
                    pos(p.to_screen(v2(page.width, page.height))),
                ),
                0.0,
                colors.page,
            );
            app.ensure_render_model();
            for layer in &app.render.layers {
                let [r, g, b] = layer.rgb;
                let stroke = Stroke::new(1.0, Color32::from_rgb(r, g, b));
                for s in &layer.strokes {
                    let pts: Vec<Pos2> = s.iter().map(|&q| pos(p.to_screen(q))).collect();
                    painter.add(Shape::line(pts, stroke));
                }
            }
            let (Some(canvas), Some(mut vp)) = (canvas, app.viewport) else {
                return;
            };
            let screen = screen_rect(canvas);
            let v = map.visible(&vp, screen);
            painter.rect_stroke(
                egui::Rect::from_min_size(
                    Pos2::new(v.x as f32, v.y as f32),
                    egui::vec2(v.width as f32, v.height as f32),
                ),
                0.0,
                Stroke::new(1.5, Color32::from_rgb(0xe5, 0x39, 0x35)),
                egui::StrokeKind::Middle,
            );
            if (response.clicked() || response.dragged())
                && let Some(at) = response.interact_pointer_pos()
            {
                map.center_on(&mut vp, screen, v2(f64::from(at.x), f64::from(at.y)));
                app.viewport = Some(vp);
            }
        });
    app.workspace.reference = open;
}

/// Design Settings ▸ Production (beyond PE-DESIGN, all off by default): basting box (#509),
/// Stop at jumps (#512) and nearest start (#547). Each change is one command.
fn production_settings(app: &EditorApp, ui: &mut egui::Ui, requests: &mut Vec<CommandRequest>) {
    use openstitch_design::production::{AutoStart, BastingShape};
    let p = app.session.design().production.clone();
    ui.separator();
    ui.label(egui::RichText::new(t("ui-production-beyond-pe-design")).strong());
    let basting = |v: serde_json::Value| CommandRequest::new("design.setBasting", v);
    let mut on = p.basting.is_some();
    if ui
        .checkbox(&mut on, t("ui-basting-box"))
        .on_hover_text(t("ui-long-running-stitches-sewn-first-round-tip"))
        .changed()
    {
        requests.push(basting(json!({ "on": on })));
    }
    if let Some(b) = &p.basting {
        ui.horizontal(|ui| {
            for shape in [BastingShape::Box, BastingShape::Outline] {
                let label = match shape {
                    BastingShape::Box => "Box",
                    BastingShape::Outline => "Follow outline",
                };
                if ui.radio(b.shape == shape, label).clicked() && b.shape != shape {
                    requests.push(basting(json!({ "on": true, "shape": shape.id() })));
                }
            }
        });
        ui.horizontal(|ui| {
            let mut margin = b.margin;
            let r = ui
                .add(
                    egui::DragValue::new(&mut margin)
                        .range(0.0..=30.0)
                        .speed(0.1)
                        .suffix(" mm margin"),
                )
                .on_hover_text(t("ui-distance-from-the-design-shrunk-to-tip"));
            if r.drag_stopped() || r.lost_focus() {
                requests.push(basting(json!({ "on": true, "margin": margin })));
            }
            let mut stitch = b.stitch;
            let r = ui
                .add(
                    egui::DragValue::new(&mut stitch)
                        .range(2.0..=12.0)
                        .speed(0.1)
                        .suffix(" mm stitch"),
                )
                .on_hover_text(t("ui-basting-stitch-length-tip"));
            if r.drag_stopped() || r.lost_focus() {
                requests.push(basting(json!({ "on": true, "stitch": stitch })));
            }
        });
        let mut own = b.thread.is_some();
        if ui
            .checkbox(&mut own, t("ui-own-thread"))
            .on_hover_text(t("ui-sew-the-basting-in-its-own-tip"))
            .changed()
        {
            let thread = if own { "own" } else { "first" };
            requests.push(basting(json!({ "on": true, "thread": thread })));
        }
    }
    let mut stops = p.stop_at_jumps;
    if ui
        .checkbox(&mut stops, t("ui-stop-at-jumps-machine-without-thread"))
        .on_hover_text(t("ui-the-machine-stops-before-every-jump-tip"))
        .changed()
    {
        requests.push(CommandRequest::new("design.setStopAtJumps", json!({ "on": stops })));
    }
    let mut nearest = p.auto_start == AutoStart::Nearest;
    if ui
        .checkbox(&mut nearest, t("ui-start-each-object-nearest-the-last"))
        .on_hover_text(t("ui-shapes-without-their-own-entry-point-tip"))
        .changed()
    {
        let rule = if nearest { AutoStart::Nearest } else { AutoStart::Off };
        requests.push(CommandRequest::new(
            "design.setProperties",
            json!({ "autoStart": rule.id() }),
        ));
    }
    let mut under = p.travel_under;
    if ui
        .checkbox(&mut under, t("ui-travel-under-later-fills"))
        .on_hover_text(t("ui-a-move-to-an-object-of-tip"))
        .changed()
    {
        requests.push(CommandRequest::new("design.setTravelUnder", json!({ "on": under })));
    }
    let mut hidden = p.remove_hidden.is_some();
    if ui
        .checkbox(&mut hidden, t("ui-leave-out-hidden-stitching-1-mm"))
        .on_hover_text(t("ui-shapes-skip-the-stitching-later-fills-tip"))
        .changed()
    {
        requests.push(CommandRequest::new("design.setRemoveHidden", json!({ "on": hidden })));
    }
}

/// PE-DESIGN's Design Settings: machine type, hoop, rotation, custom size, user hoops, sewing
/// area and jump-stitch trimming. Every change is one `machine.*` command (one Undo).
pub fn design_settings_window(app: &mut EditorApp, ctx: &egui::Context) {
    let units = app.ui.prefs.units;
    let mut open = app.workspace.settings_open;
    let mut requests: Vec<CommandRequest> = Vec::new();
    let shown = app
        .dialog_window(ctx, "Design Settings", None)
        .open(&mut open)
        .default_width(340.0)
        .resizable(false)
        .show(ctx, |ui| {
            let d = app.session.design();
            let m = d.machine.clone();
            let (pw, ph) = (d.page.width / 10.0, d.page.height / 10.0);
            let set = |v: serde_json::Value| CommandRequest::new("machine.set", v);
            egui::Grid::new("machine").num_columns(2).show(ui, |ui| {
                ui.label(t("ui-machine-type"));
                ui.horizontal(|ui| {
                    for (t, label) in [
                        (MachineType::SingleNeedle, "Single needle"),
                        (MachineType::MultiNeedle, "Multi-needle"),
                    ] {
                        if ui.selectable_label(m.machine == t, label).clicked() && m.machine != t {
                            requests.push(set(json!({ "machine": t.as_str() })));
                        }
                    }
                });
                ui.end_row();

                ui.label(t("ui-hoop"));
                let current = m.hoop.clone().unwrap_or_else(|| "custom".into());
                let text = openstitch_design::find_hoop(&current)
                    .map(|h| match h.brand {
                        "Brother" => h.label.to_string(),
                        b => format!("{b} {}", h.label),
                    })
                    .or_else(|| {
                        m.user_hoop(&current)
                            .map(|u| format!("{} (user)", units::hoop_size(units, u.width, u.height)))
                    })
                    .unwrap_or_else(|| "Custom size".into());
                let r = egui::ComboBox::from_id_salt("hoop")
                    .selected_text(text)
                    .width(230.0)
                    .show_ui(ui, |ui| {
                        // Grouped by brand, Brother first (#514); the filter matches brand or
                        // name and is kept for the session.
                        let fid = egui::Id::unique("hoop-filter");
                        let mut filter: String = ui.data_mut(|d| d.get_temp(fid).unwrap_or_default());
                        ui.add(egui::TextEdit::singleline(&mut filter).hint_text("Filter hoops"));
                        let needle = filter.to_lowercase();
                        egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                            for brand in BRANDS {
                                let shown: Vec<_> = openstitch_design::machine::brand_hoops(brand)
                                    .filter(|h| {
                                        needle.is_empty()
                                            || h.label.to_lowercase().contains(&needle)
                                            || h.brand.to_lowercase().contains(&needle)
                                    })
                                    .collect();
                                if shown.is_empty() {
                                    continue;
                                }
                                ui.label(egui::RichText::new(brand).strong());
                                for h in shown {
                                    if ui
                                        .selectable_label(current == h.id, h.label)
                                        .on_hover_text(h.source)
                                        .clicked()
                                    {
                                        requests.push(set(json!({ "hoop": h.id })));
                                    }
                                }
                            }
                        });
                        ui.data_mut(|d| d.insert_temp(fid, filter));
                        if !m.user_hoops.is_empty() {
                            ui.separator();
                        }
                        for u in &m.user_hoops {
                            let key = openstitch_design::machine::user_hoop_key(u.id);
                            let label = if u.comment.is_empty() {
                                units::hoop_size(units, u.width, u.height)
                            } else {
                                format!("{}  {}", units::hoop_size(units, u.width, u.height), u.comment)
                            };
                            if ui.selectable_label(current == key, label).clicked() {
                                requests.push(set(json!({ "hoop": key })));
                            }
                        }
                        ui.separator();
                        if ui.selectable_label(current == "custom", "Custom size").clicked() {
                            requests.push(set(json!({ "hoop": "custom" })));
                        }
                    });
                crate::a11y::name(&r.response, "Hoop");
                ui.end_row();

                // PE-DESIGN's page mode (#1056): Hoop Size (the chosen hoop, or 100 x 100 mm)
                // or Custom Size. On a Hoop Size page each one-step tie-in starts on a stitch in
                // place, as PE-DESIGN sews it; a PE-DESIGN 11 file opened keeps its own mode.
                ui.label(t("ui-page-mode"));
                ui.horizontal(|ui| {
                    let hoop_page = d.hoop_size_page();
                    let can_hoop =
                        m.hoop.is_some() || ((pw.round(), ph.round()) == (100.0, 100.0));
                    let r = ui
                        .add_enabled(
                            can_hoop,
                            egui::Button::selectable(hoop_page, t("ui-page-mode-hoop")),
                        )
                        .on_hover_text(t("ui-page-mode-hoop-tip"))
                        .on_disabled_hover_text(t("ui-page-mode-hoop-needs"));
                    if r.clicked() && !hoop_page {
                        requests.push(set(json!({ "pageMode": "hoop" })));
                    }
                    let r = ui
                        .selectable_label(!hoop_page, t("ui-page-mode-custom"))
                        .on_hover_text(t("ui-page-mode-custom-tip"));
                    if r.clicked() && hoop_page {
                        requests.push(set(json!({ "pageMode": "custom" })));
                    }
                });
                ui.end_row();

                let user = m.hoop.as_deref().is_some_and(|k| m.user_hoop(k).is_some());
                let turns = m.listed_hoop().is_none_or(|h| h.can_rotate());
                ui.label("");
                let mut rotate = m.rotate90;
                if ui
                    .add_enabled(
                        m.hoop.is_some() && !user && turns,
                        egui::Checkbox::new(&mut rotate, "Rotate 90°"),
                    )
                    .changed()
                {
                    requests.push(set(json!({ "rotate90": rotate })));
                }
                ui.end_row();

                // One-click hoop commands (#879, beyond PE-DESIGN), one undo step each.
                ui.label("");
                ui.horizontal(|ui| {
                    let any = !d.objects.is_empty();
                    if ui
                        .add_enabled(any, egui::Button::new(t("ui-use-smallest-hoop")))
                        .on_hover_text(t("ui-use-smallest-hoop-tip"))
                        .clicked()
                    {
                        requests.push(CommandRequest::new("machine.useSmallestHoop", json!({})));
                    }
                    if ui
                        .add_enabled(any, egui::Button::new(t("ui-fit-to-hoop")))
                        .on_hover_text(t("ui-fit-to-hoop-tip"))
                        .clicked()
                    {
                        requests.push(CommandRequest::new("design.fitToHoop", json!({})));
                    }
                });
                ui.end_row();

                ui.label(t("ui-design-page"));
                ui.horizontal(|ui| {
                    let custom = m.hoop.is_none();
                    let (mut w, mut h) = (pw, ph);
                    let rw = ui.add_enabled(
                        custom,
                        crate::units_ui::drag(&mut w, units).range(10.0..=2000.0),
                    );
                    ui.label("×");
                    let rh = ui.add_enabled(
                        custom,
                        crate::units_ui::drag(&mut h, units).range(10.0..=2000.0),
                    );
                    let done = |r: &egui::Response| r.drag_stopped() || r.lost_focus();
                    if (done(&rw) || done(&rh)) && (w != pw || h != ph) {
                        requests.push(set(json!({ "width": w, "height": h })));
                    }
                });
                ui.end_row();

                ui.label(t("ui-sewing-area"));
                ui.vertical(|ui| {
                    for (a, label, tip) in [
                        (
                            SewingArea::DesignPage,
                            "Design Page area",
                            "The needle starts at the centre of the Design Page",
                        ),
                        (
                            SewingArea::DesignArea,
                            "Use existing design area",
                            "The needle starts at the centre of the design",
                        ),
                    ] {
                        if ui
                            .radio(m.sewing_area == a, label)
                            .on_hover_text(tip)
                            .clicked()
                            && m.sewing_area != a
                        {
                            requests.push(set(json!({ "sewingArea": a.as_str() })));
                        }
                    }
                });
                ui.end_row();

                ui.label(t("ui-jump-stitch-trimming"));
                let multi = m.machine == MachineType::MultiNeedle;
                let mut trim = m.trim_distance(openstitch_design::Compiler::default().trim_distance)
                    / 10.0;
                let r = ui
                    .add_enabled(
                        multi,
                        egui::DragValue::new(&mut trim)
                            .range(0.5..=50.0)
                            .speed(0.1)
                            .suffix(" mm minimum"),
                    )
                    .on_disabled_hover_text("Multi-needle machines only");
                if (r.drag_stopped() || r.lost_focus()) && multi {
                    requests.push(set(json!({ "minJumpTrim": (trim * 10.0).round() / 10.0 })));
                }
                ui.end_row();

                ui.label(t("ui-speed-estimate"));
                let mut spm = m.speed();
                let r = ui.add(
                    egui::DragValue::new(&mut spm)
                        .range(100.0..=2000.0)
                        .speed(10.0)
                        .suffix(" st/min"),
                );
                if r.drag_stopped() || r.lost_focus() {
                    requests.push(set(json!({ "speed": spm.round() as i64 })));
                }
                ui.end_row();
            });
            crate::sections_ui::settings(app, ui, &mut requests);
            production_settings(app, ui, &mut requests);

            ui.separator();
            ui.label(egui::RichText::new(t("ui-user-hoops")).strong());
            for (i, u) in m.user_hoops.iter().enumerate() {
                let key = openstitch_design::machine::user_hoop_key(u.id);
                ui.horizontal(|ui| {
                    ui.label(units::hoop_size(units, u.width, u.height));
                    if !u.comment.is_empty() {
                        ui.label(egui::RichText::new(&u.comment).weak());
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button(t("ui-delete")).clicked() {
                            requests.push(CommandRequest::new(
                                "machine.deleteUserHoop",
                                json!({ "hoop": key }),
                            ));
                        }
                        let down =
                            egui::Button::new(crate::icons::ui_image(ui, "move_down")).small();
                        if ui
                            .add_enabled(i + 1 < m.user_hoops.len(), down)
                            .on_hover_text(t("ui-move-down-tip"))
                            .clicked()
                        {
                            requests.push(CommandRequest::new(
                                "machine.moveUserHoop",
                                json!({ "hoop": key, "direction": "down" }),
                            ));
                        }
                        let up = egui::Button::new(crate::icons::ui_image(ui, "move_up")).small();
                        if ui.add_enabled(i > 0, up).on_hover_text(t("ui-move-up-tip")).clicked() {
                            requests.push(CommandRequest::new(
                                "machine.moveUserHoop",
                                json!({ "hoop": key, "direction": "up" }),
                            ));
                        }
                    });
                });
            }
            ui.horizontal(|ui| {
                let (w, h, c) = &mut app.workspace.user_hoop;
                ui.add(crate::units_ui::drag(w, units).range(10.0..=2000.0));
                ui.label("×");
                ui.add(crate::units_ui::drag(h, units).range(10.0..=2000.0));
                ui.add(egui::TextEdit::singleline(c).hint_text("comment").desired_width(80.0));
                if ui.button(t("ui-add-hoop")).clicked() {
                    requests.push(CommandRequest::new(
                        "machine.addUserHoop",
                        json!({ "width": *w, "height": *h, "comment": c.clone() }),
                    ));
                    c.clear();
                }
            });
            ui.add_space(4.0);
            ui.label(
                egui::RichText::new(
                    "Do not choose a hoop larger than your machine can use. Hoop sizes come from Brother's public pages (hover for the source).",
                )
                .weak()
                .small(),
            );
        });
    if let Some(shown) = shown {
        app.dialog_rect_next = Some(shown.response.rect);
    }
    app.workspace.settings_open = open;
    for r in requests {
        if let Some(v) = app.execute(&r) {
            app.status = openstitch_app::tf!("ui-status-done", command = r.command);
            // Fit to hoop says when stitch-only objects change density (#879).
            if let Some(w) = v["warnings"].get(0).and_then(|w| w.as_str()) {
                app.status = w.to_string();
            }
            if matches!(
                r.command,
                "machine.set" | "machine.useSmallestHoop" | "design.fitToHoop"
            ) {
                app.fit_requested = true;
            }
        }
    }
}

/// "12.3 m · 1 h 05 min" for the status line and design info.
pub fn estimate_text(e: &Estimate) -> String {
    let minutes = (e.seconds / 60.0).round() as i64;
    let time = if minutes >= 60 {
        format!("{} h {:02} min", minutes / 60, minutes % 60)
    } else {
        format!("{} min", minutes.max(if e.seconds > 0.0 { 1 } else { 0 }))
    };
    format!(
        "Thread {:.1} m (bobbin {:.1} m) · about {time}",
        e.needle_total() / 1000.0,
        e.bobbin_thread / 1000.0
    )
}

#[cfg(test)]
mod ruler_tests {
    #[test]
    fn ruler_labels_count_up_both_ways_from_the_origin() {
        assert_eq!(super::ruler_label("-20"), "20");
        assert_eq!(super::ruler_label("\u{2212}1.5"), "1.5");
        assert_eq!(super::ruler_label("10"), "10");
    }
}

/// The Realistic view under MSAA (#907).
#[cfg(test)]
mod realistic_tests {
    use super::*;

    /// A thread segment is one quad, `w` wide across its direction, without feathering (no
    /// extra transparent vertices), so rows that meet close the seam between them.
    #[test]
    fn a_thread_segment_is_one_unfeathered_quad() {
        let mut m = ThreadMesh::default();
        let c = Color32::from_rgb(10, 85, 163);
        m.segment(Pos2::new(10.0, 20.0), Pos2::new(30.0, 20.0), 1.5, c);
        m.segment(Pos2::new(10.0, 21.5), Pos2::new(30.0, 21.5), 1.5, c);
        m.segment(Pos2::new(5.0, 5.0), Pos2::new(5.0, 5.0), 1.5, c);
        assert_eq!((m.0.vertices.len(), m.0.indices.len()), (8, 12));
        assert!(m.0.vertices.iter().all(|v| v.color == c));
        let ys = |r: std::ops::Range<usize>| {
            let y: Vec<f32> = m.0.vertices[r].iter().map(|v| v.pos.y).collect();
            (y.iter().copied().fold(f32::MAX, f32::min), y.iter().copied().fold(f32::MIN, f32::max))
        };
        assert_eq!(ys(0..4), (19.25, 20.75));
        // The next row 1.5 px below starts exactly where the first ends: no gap, no overlap.
        assert_eq!(ys(4..8).0, ys(0..4).1);
    }

    /// The one colour of a thread under 3 px is its shading mixed by area: the T1 blue's
    /// edge, body and highlight average to the colour the same stitches show zoomed in.
    #[test]
    fn a_fine_thread_is_its_shading_mixed_by_area() {
        let [edge, body, light] = openstitch_app::editor::workspace::thread_shades([10, 85, 163])
            .map(|c| Color32::from_rgb(c[0], c[1], c[2]));
        let fine = mix_by_area(&[(edge, 0.3), (body, 0.55), (light, 0.15)]);
        assert_eq!(fine, Color32::from_rgb(25, 85, 147));
        let grey = Color32::from_gray(100);
        assert_eq!(mix_by_area(&[(grey, 1.0)]), grey);
        assert_eq!(
            mix_by_area(&[(Color32::BLACK, 1.0), (Color32::WHITE, 3.0)]),
            Color32::from_gray(191)
        );
    }
}
