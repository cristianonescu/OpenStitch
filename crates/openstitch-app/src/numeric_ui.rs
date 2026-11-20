//! The object menu's Numerical Setting ▸ Size... and Rotate... (and the ribbon's Rotate/Flip ▸
//! Rotate...), PE-DESIGN 11.50's two small dialogs (#1288, coordinator capture of 2026-11-20):
//!
//! - Size: a percentage row (width x height %) and a millimetre row (width x height mm), one of
//!   them chosen with its radio button; Maintain aspect ratio (on: the height follows the
//!   width and its field is grey); Keep density and fill pattern; OK, Preview and Cancel, OK and
//!   Preview grey until a value changes.
//! - Rotate: a dial whose red arrow points right at 0 degrees, and the angle in degrees; OK,
//!   Preview and Cancel.
//!
//! Preview applies the setting to the design; Cancel takes a preview back; OK applies it as
//! one undo step (a preview is taken back first). The selection turns and scales about its
//! centre, as `object.rotate` and `object.scale` do.

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::ui_texts::t;
use serde_json::{Value, json};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Size,
    Rotate,
}

/// An open Size or Rotate dialog.
pub struct Numeric {
    pub kind: Kind,
    /// Size: the percentage row is chosen (else millimetres).
    percent: bool,
    width_pct: f64,
    height_pct: f64,
    width_mm: f64,
    height_mm: f64,
    keep_aspect: bool,
    /// Rotate: the angle, degrees clockwise.
    degrees: f64,
    /// The selection's size when the dialog opened, mm.
    base: (f64, f64),
    /// The values a preview applied (one undo step), when one is applied.
    previewed: Option<[f64; 2]>,
}

/// The selection's width and height, mm.
fn selection_size(app: &EditorApp) -> Option<(f64, f64)> {
    let d = app.session.design();
    let b = openstitch_design::Rect::enclosing(
        d.objects
            .iter()
            .filter(|o| app.selection.contains(&o.id))
            .filter_map(|o| o.bounds())
            .flat_map(|b| {
                [
                    openstitch_design::v2(b.min_x, b.min_y),
                    openstitch_design::v2(b.max_x, b.max_y),
                ]
            }),
    )?;
    Some((b.width() / 10.0, b.height() / 10.0))
}

/// Opens the dialog for the selection; nothing without one.
pub fn open(app: &mut EditorApp, kind: Kind) {
    let Some(base) = selection_size(app) else {
        return;
    };
    app.workspace.view.numeric = Some(Numeric {
        kind,
        percent: true,
        width_pct: 100.0,
        height_pct: 100.0,
        width_mm: base.0,
        height_mm: base.1,
        keep_aspect: true,
        degrees: 0.0,
        base,
        previewed: None,
    });
}

impl Numeric {
    /// The scale factors (Size) or the angle and 0 (Rotate) the dialog holds.
    fn values(&self) -> [f64; 2] {
        match self.kind {
            Kind::Rotate => [self.degrees, 0.0],
            Kind::Size if self.percent => [self.width_pct / 100.0, self.height_pct / 100.0],
            Kind::Size => [
                self.width_mm / self.base.0.max(1e-9),
                self.height_mm / self.base.1.max(1e-9),
            ],
        }
    }

    /// Whether a value differs from the one the dialog opened with.
    fn changed(&self) -> bool {
        let v = self.values();
        match self.kind {
            Kind::Rotate => v[0].abs() > 1e-9,
            Kind::Size => (v[0] - 1.0).abs() > 1e-9 || (v[1] - 1.0).abs() > 1e-9,
        }
    }

    fn request(&self, app: &EditorApp) -> CommandRequest {
        let v = self.values();
        match self.kind {
            Kind::Rotate => CommandRequest::rotate(&app.selection, v[0]),
            Kind::Size => CommandRequest::new(
                "object.scale",
                json!({
                    "ids": CommandRequest::ids(&app.selection),
                    "sx": (v[0] * 10_000.0).round() / 10_000.0,
                    "sy": (v[1] * 10_000.0).round() / 10_000.0,
                }),
            ),
        }
    }
}

/// Takes back a preview that is applied.
fn unpreview(app: &mut EditorApp, n: &mut Numeric) {
    if n.previewed.take().is_some() {
        app.undo();
    }
}

/// Applies the dialog's setting: Preview leaves the dialog open, OK closes it. One undo step
/// either way (an earlier preview is taken back first).
fn apply(app: &mut EditorApp, n: &mut Numeric) {
    unpreview(app, n);
    if n.changed() {
        let r = n.request(app);
        if app.execute(&r).is_some() {
            n.previewed = Some(n.values());
        }
    }
}

/// The Rotate dialog's dial: a circle with a red arrow at the angle (pointing right at 0,
/// turning clockwise as the angle grows); a click or a drag on it sets the angle in whole
/// degrees.
fn dial(ui: &mut egui::Ui, degrees: &mut f64) -> egui::Response {
    let size = egui::vec2(96.0, 96.0);
    let (rect, response) = ui.allocate_exact_size(size, egui::Sense::click_and_drag());
    let c = rect.center();
    let r = rect.width() / 2.0 - 4.0;
    if let Some(p) = response.interact_pointer_pos()
        && (response.dragged() || response.clicked())
    {
        let d = p - c;
        *degrees = f64::from(d.y.atan2(d.x).to_degrees()).round();
    }
    let v = ui.visuals();
    ui.painter().circle(
        c,
        r,
        v.extreme_bg_color,
        egui::Stroke::new(1.0, v.text_color()),
    );
    let a = (*degrees as f32).to_radians();
    let tip = c + egui::vec2(a.cos(), a.sin()) * (r - 4.0);
    let red = egui::Color32::from_rgb(0xd0, 0x20, 0x20);
    ui.painter().arrow(c, tip - c, egui::Stroke::new(2.5, red));
    response
}

/// The dialog, until OK or Cancel.
pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    let Some(mut n) = app.workspace.view.numeric.take() else {
        return;
    };
    let title = match n.kind {
        Kind::Size => t("numeric-size-title"),
        Kind::Rotate => t("numeric-rotate-title"),
    };
    let (mut ok, mut preview, mut cancel) = (false, false, false);
    let mut open = true;
    app.dialog_window(ctx, title, Some([16.0, 140.0]))
        .collapsible(false)
        .resizable(false)
        .open(&mut open)
        .show(ctx, |ui| {
            match n.kind {
                Kind::Size => size_body(ui, &mut n),
                Kind::Rotate => {
                    ui.horizontal(|ui| {
                        dial(ui, &mut n.degrees);
                        ui.add(
                            egui::DragValue::new(&mut n.degrees)
                                .range(-360.0..=360.0)
                                .speed(1.0),
                        );
                        ui.label(t("numeric-degree"));
                    });
                }
            }
            ui.separator();
            let fresh = n.changed() && n.previewed != Some(n.values());
            ui.horizontal(|ui| {
                ok = ui.add_enabled(n.changed(), egui::Button::new(t("ui-ok"))).clicked();
                preview = ui
                    .add_enabled(fresh, egui::Button::new(t("numeric-preview")))
                    .clicked();
                cancel = ui.button(t("ui-cancel")).clicked();
            });
        });
    if ok {
        if n.previewed != Some(n.values()) {
            apply(app, &mut n);
        }
        return;
    }
    if preview {
        apply(app, &mut n);
    }
    if cancel || !open {
        unpreview(app, &mut n);
        return;
    }
    app.workspace.view.numeric = Some(n);
}

/// The Size dialog's rows: % or mm, Maintain aspect ratio, Keep density and fill pattern.
fn size_body(ui: &mut egui::Ui, n: &mut Numeric) {
    let ratio = n.base.1 / n.base.0.max(1e-9);
    egui::Grid::new("numeric-size").num_columns(5).show(ui, |ui| {
        ui.radio_value(&mut n.percent, true, "");
        let w = ui.add_enabled(
            n.percent,
            egui::DragValue::new(&mut n.width_pct)
                .range(1.0..=1000.0)
                .speed(1.0),
        );
        ui.label("x");
        ui.add_enabled(
            n.percent && !n.keep_aspect,
            egui::DragValue::new(&mut n.height_pct)
                .range(1.0..=1000.0)
                .speed(1.0),
        );
        ui.label("%");
        ui.end_row();
        if w.changed() && n.keep_aspect {
            n.height_pct = n.width_pct;
        }
        ui.radio_value(&mut n.percent, false, "");
        let w = ui.add_enabled(
            !n.percent,
            egui::DragValue::new(&mut n.width_mm)
                .range(0.1..=2000.0)
                .speed(0.1)
                .fixed_decimals(1),
        );
        ui.label("x");
        ui.add_enabled(
            !n.percent && !n.keep_aspect,
            egui::DragValue::new(&mut n.height_mm)
                .range(0.1..=2000.0)
                .speed(0.1)
                .fixed_decimals(1),
        );
        ui.label("mm");
        ui.end_row();
        if w.changed() && n.keep_aspect {
            n.height_mm = n.width_mm * ratio;
        }
    });
    if ui
        .checkbox(&mut n.keep_aspect, t("ui-maintain-aspect-ratio"))
        .changed()
        && n.keep_aspect
    {
        n.height_pct = n.width_pct;
        n.height_mm = n.width_mm * ratio;
    }
    // OpenStitch sews a shape again after every resize, so its density and fill pattern are
    // always kept: shown ticked and grey.
    let mut kept = true;
    ui.add_enabled(
        false,
        egui::Checkbox::new(&mut kept, t("numeric-keep-density")),
    )
    .on_disabled_hover_text(t("numeric-keep-density-tip"));
}

/// The open dialog as `ui.objectMenu` reports it: kind, the values it holds and whether a
/// preview is applied; null when none is open.
pub fn state(app: &EditorApp) -> Value {
    let Some(n) = app.workspace.view.numeric.as_ref() else {
        return Value::Null;
    };
    json!({
        "kind": match n.kind { Kind::Size => "size", Kind::Rotate => "rotate" },
        "percent": n.percent,
        "widthPercent": n.width_pct,
        "heightPercent": n.height_pct,
        "widthMm": n.width_mm,
        "heightMm": n.height_mm,
        "maintainAspectRatio": n.keep_aspect,
        "degrees": n.degrees,
        "previewed": n.previewed.is_some(),
    })
}

/// `ui.objectMenu {numeric}`: sets the open dialog's fields (`percent`, `width`, `height`,
/// `maintainAspectRatio`, `degrees`) and presses `button` (ok, preview, cancel), as the user
/// does in it.
pub fn command(app: &mut EditorApp, params: &Value) -> Result<(), String> {
    let Some(mut n) = app.workspace.view.numeric.take() else {
        return Err("numeric: no Size or Rotate dialog is open (open it with action sizeNumeric or rotateNumeric)".into());
    };
    let num = |k: &str| params.get(k).and_then(Value::as_f64);
    if let Some(on) = params.get("maintainAspectRatio").and_then(Value::as_bool) {
        n.keep_aspect = on;
    }
    if let Some(p) = params.get("percent").and_then(Value::as_bool) {
        n.percent = p;
    }
    let ratio = n.base.1 / n.base.0.max(1e-9);
    if let Some(w) = num("width") {
        if n.percent {
            n.width_pct = w.clamp(1.0, 1000.0);
            if n.keep_aspect {
                n.height_pct = n.width_pct;
            }
        } else {
            n.width_mm = w.clamp(0.1, 2000.0);
            if n.keep_aspect {
                n.height_mm = n.width_mm * ratio;
            }
        }
    }
    if let Some(h) = num("height")
        && !n.keep_aspect
    {
        if n.percent {
            n.height_pct = h.clamp(1.0, 1000.0);
        } else {
            n.height_mm = h.clamp(0.1, 2000.0);
        }
    }
    if let Some(d) = num("degrees") {
        n.degrees = d.clamp(-360.0, 360.0);
    }
    match params.get("button").and_then(Value::as_str) {
        None => {}
        Some("ok") => {
            if !n.changed() {
                app.workspace.view.numeric = Some(n);
                return Err("ok: no value differs from what the dialog opened with".into());
            }
            if n.previewed != Some(n.values()) {
                apply(app, &mut n);
            }
            return Ok(());
        }
        Some("preview") => apply(app, &mut n),
        Some("cancel") => {
            unpreview(app, &mut n);
            return Ok(());
        }
        Some(_) => {
            app.workspace.view.numeric = Some(n);
            return Err("button: must be ok, preview or cancel".into());
        }
    }
    app.workspace.view.numeric = Some(n);
    Ok(())
}
