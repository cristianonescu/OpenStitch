//! Every control can be reached at a small window (#1276): the menus, drop-down lists, panes
//! and dialogs are opened in a small window, and each control that is not shown in full must
//! come into view under the mouse wheel. A child module of `app`, so it drives the whole frame
//! (`eframe::App::ui`) as the window does.

use super::*;
use eframe::egui::accesskit::{Node, NodeId, Role};
use openstitch_app::editor::ribbon::{APPLICATION_MENU, Action, OPTION_MENU, TabId};
use std::collections::{HashMap, HashSet};

/// What a user works with: a control of one of these roles has to be reachable.
const CONTROLS: &[Role] = &[
    Role::Button,
    Role::CheckBox,
    Role::ComboBox,
    Role::TextInput,
    Role::MultilineTextInput,
    Role::Slider,
    Role::SpinButton,
    Role::Link,
    Role::MenuItem,
    Role::MenuItemCheckBox,
    Role::MenuItemRadio,
    Role::RadioButton,
    Role::Tab,
    Role::ColorWell,
    Role::Switch,
];

#[derive(Clone, Debug)]
struct Control {
    id: egui::Id,
    parent: egui::Id,
    role: Role,
    label: String,
    /// Its description (a ribbon button's ScreenTip), as written.
    tip: String,
    /// Where the control is laid out.
    rect: egui::Rect,
    /// The part of it inside its clip rectangle.
    seen: egui::Rect,
    layer: egui::LayerId,
}

struct Win {
    app: EditorApp,
    ctx: egui::Context,
    frame: eframe::Frame,
    /// The window in points (its size in pixels over the UI scale).
    size: egui::Vec2,
    time: f64,
    nodes: HashMap<NodeId, Node>,
}

/// One row of the survey.
#[derive(Debug, Default)]
struct Row {
    surface: String,
    controls: usize,
    /// Controls not shown in full when the surface opens.
    hidden: usize,
    /// Of those, the ones the wheel does not bring into view.
    lost: Vec<String>,
    /// How far the surface's own area reaches past the window (points right, down).
    beyond: (f32, f32),
}

fn squash(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

impl Win {
    /// The window `px` pixels large at a UI scale of `scale` percent.
    fn new(launch: Launch, px: (f32, f32), scale: u32) -> Win {
        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let mut app = EditorApp::new(launch);
        app.splash = None;
        app.ui.prefs.text_size = scale as _;
        let k = scale as f32 / 100.0;
        let mut w = Win {
            app,
            ctx,
            frame: eframe::Frame::_new_kittest(),
            size: egui::vec2(px.0 / k, px.1 / k),
            time: 0.0,
            nodes: HashMap::new(),
        };
        w.frames(5, Vec::new());
        w
    }

    fn screen(&self) -> egui::Rect {
        egui::Rect::from_min_size(egui::Pos2::ZERO, self.size)
    }

    fn frames(&mut self, n: usize, mut events: Vec<egui::Event>) {
        for _ in 0..n {
            self.time += 1.0 / 60.0;
            let raw = egui::RawInput {
                screen_rect: Some(self.screen()),
                time: Some(self.time),
                events: std::mem::take(&mut events),
                ..Default::default()
            };
            let (app, frame) = (&mut self.app, &mut self.frame);
            let mut out = self.ctx.run_ui(raw, |ui| eframe::App::ui(app, ui, frame));
            out.textures_delta.clear();
            if let Some(update) = out.platform_output.accesskit_update {
                self.nodes = update.nodes.into_iter().collect();
            }
        }
    }

    /// The widgets of the last frame.
    fn widgets<R>(&self, read: impl FnOnce(&egui::WidgetRects) -> R) -> R {
        self.ctx.viewport(|v| {
            let (a, b) = (&v.prev_pass.widgets, &v.this_pass.widgets);
            read(if a.layer_ids().len() >= b.layer_ids().len() { a } else { b })
        })
    }

    fn controls(&self) -> Vec<Control> {
        let nodes = &self.nodes;
        self.widgets(|all| {
            let mut out = Vec::new();
            for (layer, rects) in all.layers() {
                for w in rects {
                    let Some(n) = nodes.get(&w.id.accesskit_id()) else {
                        continue;
                    };
                    if !CONTROLS.contains(&n.role())
                        || !(w.rect.width() > 0.5 && w.rect.height() > 0.5)
                    {
                        continue;
                    }
                    out.push(Control {
                        id: w.id,
                        parent: w.parent_id,
                        role: n.role(),
                        label: {
                            let tip = n.description().unwrap_or_default();
                            let l = squash(n.label().or(n.value()).unwrap_or_default());
                            if l.is_empty() {
                                squash(tip.lines().next().unwrap_or_default())
                            } else {
                                l
                            }
                        },
                        tip: n.description().unwrap_or_default().to_string(),
                        rect: w.rect,
                        seen: w.interact_rect,
                        layer: *layer,
                    });
                }
            }
            out
        })
    }

    /// The part of `c` a user sees.
    fn visible(&self, c: &Control) -> egui::Rect {
        c.seen.intersect(c.rect).intersect(self.screen())
    }

    fn shown(&self, c: &Control) -> bool {
        let v = self.visible(c);
        v.width() >= c.rect.width() - 1.0 && v.height() >= c.rect.height() - 1.0
    }

    /// Where the pointer rests to scroll `c` into view: on its visible part, else on the
    /// visible part of the nearest container around it (a scroll area's view).
    fn wheel_point(&self, c: &Control) -> egui::Pos2 {
        let screen = self.screen().shrink(2.0);
        let v = self.visible(c);
        if v.width() > 1.0 && v.height() > 1.0 {
            return v.center();
        }
        let centre = c.rect.center();
        let within = |r: egui::Rect| {
            let r = r.intersect(screen).shrink(3.0);
            (r.width() > 1.0 && r.height() > 1.0).then(|| {
                egui::pos2(centre.x.clamp(r.min.x, r.max.x), centre.y.clamp(r.min.y, r.max.y))
            })
        };
        let up = self.widgets(|all| {
            let mut id = c.parent;
            for _ in 0..40 {
                let Some(w) = all.get(id) else { break };
                if let Some(p) = within(w.interact_rect) {
                    return Some(p);
                }
                if w.parent_id == id {
                    break;
                }
                id = w.parent_id;
            }
            None
        });
        up.or_else(|| within(self.screen())).unwrap_or(centre)
    }

    /// Scrolls with the wheel until `id` is shown in full; `ok` collects every control seen in
    /// full on the way.
    fn reach(&mut self, id: egui::Id, ok: &mut HashSet<egui::Id>) -> bool {
        // First with the pointer on the control or the nearest view of it; then, as a person
        // does when a list inside a scrolling pane takes the wheel, from other places in the
        // containers around it.
        (0..6).any(|from| self.reach_from(id, ok, from) == Some(true))
    }

    /// Other places to rest the pointer for [`Self::reach`]: in each container around `c`,
    /// the end away from it.
    fn other_wheel_points(&self, c: &Control) -> Vec<egui::Pos2> {
        let screen = self.screen().shrink(2.0);
        let centre = c.rect.center();
        self.widgets(|all| {
            let mut out: Vec<egui::Pos2> = Vec::new();
            let mut id = c.parent;
            for _ in 0..40 {
                let Some(w) = all.get(id) else { break };
                let r = w.interact_rect.intersect(screen).shrink(3.0);
                if r.width() > 1.0 && r.height() > 1.0 {
                    let far_y = if centre.y > r.center().y { r.min.y } else { r.max.y };
                    let far_x = if centre.x > r.center().x { r.min.x } else { r.max.x };
                    for p in [
                        egui::pos2(centre.x.clamp(r.min.x, r.max.x), far_y),
                        egui::pos2(far_x, centre.y.clamp(r.min.y, r.max.y)),
                    ] {
                        if out.iter().all(|q| q.distance(p) > 4.0) {
                            out.push(p);
                        }
                    }
                }
                if w.parent_id == id {
                    break;
                }
                id = w.parent_id;
            }
            out
        })
    }

    /// `None` when there is no place number `from` to scroll from.
    fn reach_from(&mut self, id: egui::Id, ok: &mut HashSet<egui::Id>, from: usize) -> Option<bool> {
        let (mut last, mut still) = (None, 0);
        // A control larger than the view it scrolls in (a button wider than a squeezed pane)
        // cannot be shown at once: it is reached when both of its ends have been in view.
        let mut ends = [false; 4];
        let trace = std::env::var("REACH_TRACE").is_ok();
        for _ in 0..if from == 0 { 400 } else { 120 } {
            let all = self.controls();
            for c in &all {
                if self.shown(c) {
                    ok.insert(c.id);
                }
            }
            let Some(c) = all.iter().find(|c| c.id == id) else {
                return Some(false);
            };
            if self.shown(c) {
                return Some(true);
            }
            let v = self.visible(c);
            let (wide, tall) = (v.width() >= c.rect.width() - 1.0, v.height() >= c.rect.height() - 1.0);
            if v.width() >= c.rect.width().min(16.0) && v.height() >= c.rect.height().min(12.0) {
                ends[0] |= v.min.x <= c.rect.min.x + 0.5;
                ends[1] |= v.max.x >= c.rect.max.x - 0.5;
                ends[2] |= v.min.y <= c.rect.min.y + 0.5;
                ends[3] |= v.max.y >= c.rect.max.y - 0.5;
            }
            if (wide || (ends[0] && ends[1])) && (tall || (ends[2] && ends[3])) {
                return Some(true);
            }
            if last == Some(c.rect) {
                still += 1;
                if still >= 2 {
                    return Some(false);
                }
            } else {
                still = 0;
            }
            last = Some(c.rect);
            let at = match from {
                0 => self.wheel_point(c),
                n => *self.other_wheel_points(c).get(n - 1)?,
            };
            if trace {
                eprintln!("    from {from} at {at:?}: rect {:?} seen {:?} ends {ends:?}", c.rect, c.seen);
            }
            let v = self.visible(c);
            let part = v.width() > 1.0 && v.height() > 1.0;
            let axis = |lo: f32, hi: f32, vlo: f32, vhi: f32, p: f32| -> f32 {
                if part {
                    if hi > vhi + 0.5 {
                        -1.0
                    } else if lo < vlo - 0.5 {
                        1.0
                    } else {
                        0.0
                    }
                } else if (lo + hi) / 2.0 > p + 1.0 {
                    -1.0
                } else if (lo + hi) / 2.0 < p - 1.0 {
                    1.0
                } else {
                    0.0
                }
            };
            let dx = axis(c.rect.min.x, c.rect.max.x, v.min.x, v.max.x, at.x);
            let dy = axis(c.rect.min.y, c.rect.max.y, v.min.y, v.max.y, at.y);
            self.frames(1, vec![egui::Event::PointerMoved(at)]);
            self.frames(
                1,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(dx * 40.0, dy * 40.0),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            self.frames(8, Vec::new());
        }
        Some(false)
    }

    fn click(&mut self, at: egui::Pos2) {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frames(2, vec![egui::Event::PointerMoved(at)]);
        self.frames(1, vec![press(true)]);
        self.frames(1, vec![press(false)]);
        self.frames(4, Vec::new());
    }

    /// A click with the right button at a window position.
    fn right_click(&mut self, at: egui::Pos2) {
        let press = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Secondary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        self.frames(2, vec![egui::Event::PointerMoved(at)]);
        self.frames(1, vec![press(true)]);
        self.frames(1, vec![press(false)]);
        self.frames(5, Vec::new());
    }

    /// Where a design point (mm) is in the window.
    fn at_mm(&self, x: f64, y: f64) -> Option<egui::Pos2> {
        let p = self.app.viewport?.to_screen(openstitch_design::v2(x * 10.0, y * 10.0));
        Some(egui::pos2(p.x as f32, p.y as f32))
    }

    fn key(&mut self, key: egui::Key) {
        let ev = |pressed| egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat: false,
            modifiers: egui::Modifiers::NONE,
        };
        self.frames(1, vec![ev(true)]);
        self.frames(3, vec![ev(false)]);
    }

    fn layers(&self) -> HashSet<egui::LayerId> {
        self.controls().iter().map(|c| c.layer).collect()
    }

    /// Brings the control labelled `label` into view and clicks it.
    fn click_label(&mut self, label: &str) -> bool {
        let want = squash(label);
        let named = |c: &Control| {
            c.label == want
                || c.tip.starts_with(&format!("{label}
"))
                || c.tip.starts_with(&format!("{label} ("))
        };
        let Some(c) = self.controls().into_iter().find(named) else {
            return false;
        };
        let id = c.id;
        if !self.reach(id, &mut HashSet::new()) {
            return false;
        }
        let Some(c) = self.controls().into_iter().find(|c| c.id == id) else {
            return false;
        };
        self.click(self.visible(&c).center());
        true
    }

    /// Checks the controls of `layers` (all layers when `None`).
    fn audit(&mut self, surface: &str, layers: Option<&HashSet<egui::LayerId>>) -> Row {
        let mine = |c: &Control| layers.is_none_or(|l| l.contains(&c.layer));
        let all: Vec<Control> = self.controls().into_iter().filter(mine).collect();
        let screen = self.screen();
        let mut beyond = (0.0f32, 0.0f32);
        let seen_layers: HashSet<egui::LayerId> = all.iter().map(|c| c.layer).collect();
        for l in &seen_layers {
            if let Some(r) = self.ctx.memory(|m| m.area_rect(l.id)) {
                beyond.0 = beyond.0.max(r.max.x - screen.max.x).max(screen.min.x - r.min.x);
                beyond.1 = beyond.1.max(r.max.y - screen.max.y).max(screen.min.y - r.min.y);
            }
        }
        let t0 = std::time::Instant::now();
        let mut ok = HashSet::new();
        let hidden: Vec<Control> = all.iter().filter(|c| !self.shown(c)).cloned().collect();
        let mut lost = Vec::new();
        for c in &hidden {
            if ok.contains(&c.id) {
                continue;
            }
            if !self.reach(c.id, &mut ok) {
                if let Some(now) = self.controls().into_iter().find(|n| n.id == c.id) {
                    eprintln!(
                        "  LOST in {surface}: {:?} '{}' rect {:?} seen {:?}",
                        now.role, now.label, now.rect, now.seen
                    );
                }
                lost.push(format!("{:?} '{}'", c.role, c.label));
            }
        }
        eprintln!(
            "{surface}: {} controls, {} hidden, {} lost, {:.1}s, t={:.0} frames",
            all.len(),
            hidden.len(),
            lost.len(),
            t0.elapsed().as_secs_f32(),
            self.time * 60.0
        );
        Row {
            surface: surface.to_string(),
            controls: all.len(),
            hidden: hidden.len(),
            lost,
            beyond: (beyond.0.max(0.0), beyond.1.max(0.0)),
        }
    }

    /// Audits the layers that are not in `base`; then opens each of their sub-menus (a hover)
    /// and each combo box in turn and audits those too.
    fn audit_new(&mut self, surface: &str, base: &HashSet<egui::LayerId>, rows: &mut Vec<Row>) {
        let new: HashSet<egui::LayerId> =
            self.layers().difference(base).copied().collect();
        if new.is_empty() {
            return;
        }
        rows.push(self.audit(surface, Some(&new)));
        self.children(surface, &new, rows, 0);
    }

    fn children(
        &mut self,
        surface: &str,
        of: &HashSet<egui::LayerId>,
        rows: &mut Vec<Row>,
        depth: usize,
    ) {
        if depth > 2 {
            return;
        }
        let items: Vec<Control> =
            self.controls().into_iter().filter(|c| of.contains(&c.layer)).collect();
        let mut done = HashSet::new();
        for c in items {
            let menu_item = matches!(c.role, Role::Button | Role::MenuItem);
            let combo = c.role == Role::ComboBox;
            if !(menu_item || combo) || !done.insert(c.label.clone()) {
                continue;
            }
            if !self.reach(c.id, &mut HashSet::new()) {
                continue;
            }
            let Some(c) = self.controls().into_iter().find(|x| x.id == c.id) else {
                continue;
            };
            let before = self.layers();
            let at = self.visible(&c).center();
            if combo {
                self.click(at);
            } else {
                self.frames(1, vec![egui::Event::PointerMoved(at)]);
                self.frames(6, Vec::new());
            }
            let opened: HashSet<egui::LayerId> =
                self.layers().difference(&before).copied().collect();
            if !opened.is_empty() {
                let name = format!("{surface} > {}", c.label);
                rows.push(self.audit(&name, Some(&opened)));
                if !combo {
                    self.children(&name, &opened, rows, depth + 1);
                }
            }
            if combo {
                self.key(egui::Key::Escape);
            }
        }
    }
}

fn demo() -> Launch {
    Launch {
        demo: true,
        select: vec![1],
        ..Launch::default()
    }
}

/// Actions that open a native file dialog, a URL or close the window.
fn interactive(a: Action) -> bool {
    use Action::*;
    matches!(
        a,
        Open | Save
            | SaveAs
            | Exit
            | ImportPatterns
            | ImportSvg
            | ImportPesObjects
            | OpenImage
            | PasteImage
            | TrueTypeAttributes
            | ExportSections
    )
}

/// The main window showing ribbon tab `tab`: the tool or the selection that brings a
/// contextual tab forward is set first.
fn tab_window(tab: TabId, px: (f32, f32), scale: u32) -> Win {
    let tool = match tab {
        TabId::Outline => Some(Tool::Polygon),
        TabId::OutlineRect => Some(Tool::Rectangle),
        TabId::ManualPunch => Some(Tool::Punch),
        _ => None,
    };
    let mut w = Win::new(Launch { tool, ..demo() }, px, scale);
    if tab == TabId::Text {
        let id = w
            .app
            .execute(&CommandRequest::new("text.add", json!({ "text": "Abc" })))
            .and_then(|r| r["id"].as_u64());
        w.app.selection = id.into_iter().collect();
        w.frames(2, Vec::new());
    }
    w.app.ui.tab = tab;
    w.frames(4, Vec::new());
    w
}

/// Every surface at one window size and UI scale.
fn survey(px: (f32, f32), scale: u32) -> Vec<Row> {
    let mut rows = Vec::new();
    let open = |launch: Launch| Win::new(launch, px, scale);
    // The main window with each ribbon tab: ribbon, panes, status bar.
    let base = open(demo()).layers();
    for tab in TabId::ALL {
        let mut w = tab_window(tab, px, scale);
        let name = format!("Main window, ribbon tab {}", tab.english_label());
        if w.app.ui.tab != tab {
            rows.push(Row {
                surface: format!("{name} (not shown for this selection)"),
                ..Row::default()
            });
            continue;
        }
        let mut row = w.audit(&name, None);
        row.surface = name.clone();
        rows.push(row);
        // The tab's drop-down lists.
        for g in tab.groups() {
            for a in g.actions() {
                let info = a.info();
                if !a.is_menu() || !info.available || !crate::ribbon::enabled(&w.app, a) {
                    continue;
                }
                let mut w = tab_window(tab, px, scale);
                let before = w.layers();
                let menu = format!("Ribbon {} > {} (list)", tab.english_label(), squash(info.label));
                if !w.click_label(info.label) {
                    rows.push(Row {
                        surface: format!("{menu}: its button cannot be reached"),
                        lost: vec![squash(info.label)],
                        ..Row::default()
                    });
                    continue;
                }
                w.audit_new(&menu, &before, &mut rows);
            }
        }
    }
    // The menus of the top bar.
    for label in ["Options", "Help"] {
        let mut w = open(demo());
        let before = w.layers();
        if w.click_label(label) {
            w.audit_new(&format!("Menu {label}"), &before, &mut rows);
        } else {
            rows.push(Row {
                surface: format!("Menu {label}: its button cannot be reached"),
                lost: vec![label.to_string()],
                ..Row::default()
            });
        }
    }
    // The --show scenes: the application menu, dialogs, panes.
    for show in SHOW {
        if ["bare", "large-text", "norulers"].contains(&show) {
            continue;
        }
        let mut w = open(Launch {
            show: vec![show.to_string()],
            ..demo()
        });
        w.frames(4, Vec::new());
        let full = ["creator", "sdf"].contains(&show);
        if full {
            rows.push(w.audit(&format!("Scene {show} (whole window)"), None));
        } else {
            let n = rows.len();
            w.audit_new(&format!("Scene {show}"), &base, &mut rows);
            if rows.len() == n {
                rows.push(Row {
                    surface: format!("Scene {show} (adds no surface of its own)"),
                    ..Row::default()
                });
            }
        }
    }
    // Dialogs no scene or ribbon command opens directly.
    type Opener = fn(&mut EditorApp);
    let extras: [(&str, Opener); 30] = [
        ("Assistant closed hint", |a| a.assistant.panel.close_hint_open = true),
        ("Assistant still working", |a| a.turn_close = Some((Pending::Exit, false))),
        ("Offset Lines", |a| a.edit.offset_open = true),
        ("Floral Pattern", |a| a.edit.floral_open = true),
        ("Floppy Image", crate::floppy_ui::open),
        ("Ink/Stitch Fonts", |a| a.ui.inkstitch.open = true),
        ("Job Sheet", |a| a.ui.open_job_sheet = true),
        ("Fit Text to Outline", |a| a.lettering.fit.open = Some((1, 2))),
        ("Machine Setup", |a| a.machine_setup.open()),
        ("Command Palette", |a| a.palette.show()),
        ("Recover Designs", |a| a.recovery.open = true),
        ("Check Design", |a| crate::quality_ui::open(a, crate::quality_ui::Tab::Check)),
        ("Check Design, Density", |a| crate::quality_ui::open(a, crate::quality_ui::Tab::Density)),
        ("Check Design, Route", |a| crate::quality_ui::open(a, crate::quality_ui::Tab::Route)),
        ("Test Swatches", |a| a.ui.open_swatches = true),
        ("Calibrate Real Size", |a| a.calibration.open = true),
        ("Open Sample", |a| a.ui.start.samples_open = true),
        ("Startup Wizard", |a| a.startup_wizard.open = true),
        ("Not in the browser version", |a| {
            a.ui.limit_notice = Some(openstitch_app::editor::browser_limits::Limit::Transfer)
        }),
        ("Image to Embroidery", |a| a.wizard.open = true),
        ("Modify Image", |a| a.workspace.view.image_open = true),
        ("Realistic View Settings", |a| a.workspace.view.realistic_open = true),
        ("Font Filter Settings", |a| a.workspace.view.fonts_open = true),
        ("About", |a| a.ui.about_open = true),
        ("Report a Problem", |a| a.ui.problem.open = true),
        ("Template Wizard, step 2", |a| {
            crate::lettering_ui::open_template_wizard(a);
            a.lettering.templates.step = 2;
        }),
        ("Template Wizard, step 3", |a| {
            crate::lettering_ui::open_template_wizard(a);
            a.lettering.templates.step = 3;
        }),
        ("Design Database (whole window)", |a| a.database.show(None)),
        ("Design Database, Search", |a| {
            a.database.show(None);
            a.database.dialog = openstitch_app::editor::database::Dialog::Search;
        }),
        ("Design Database, Catalogue", |a| {
            a.database.show(None);
            a.database.dialog = openstitch_app::editor::database::Dialog::Catalog;
        }),
    ];
    for (name, opener) in extras {
        let mut w = open(demo());
        opener(&mut w.app);
        w.frames(5, Vec::new());
        if name.ends_with("(whole window)") {
            rows.push(w.audit(name, None));
            continue;
        }
        let n = rows.len();
        w.audit_new(&format!("Dialog {name}"), &base, &mut rows);
        if rows.len() == n {
            rows.push(Row {
                surface: format!("Dialog {name} (did not open)"),
                ..Row::default()
            });
        }
    }
    for kind in ["background", "applique", "patch", "cutwork"] {
        let Some(k) = openstitch_app::editor::pattern_wizards::WizardKind::from_key(kind) else {
            continue;
        };
        let mut w = open(Launch {
            wizard: Some(k),
            ..demo()
        });
        w.audit_new(&format!("Wizard {kind}"), &base, &mut rows);
    }
    for pane in ["import", "color", "attributes", "text"] {
        let mut w = open(Launch {
            pane: crate::ribbon::PaneTab::from_key(pane),
            ..demo()
        });
        rows.push(w.audit(&format!("Main window, pane tab {pane}"), None));
    }
    // Right clicks: on a shape and on its points with the Point tool, on the rulers.
    for (name, tool, points) in [
        ("Right click on a shape, Select tool", Tool::Select, false),
        ("Right click on a shape, Point tool", Tool::Point, false),
        ("Right click on a point, Point tool", Tool::Point, true),
    ] {
        let mut w = open(Launch {
            tool: Some(tool),
            ..demo()
        });
        if points {
            let _ = w.app.ui_command("ui.pointMenu", Some(&json!({ "points": [0, 1] })));
            w.frames(3, Vec::new());
        }
        let before = w.layers();
        let Some(at) = w.at_mm(50.0, 50.0).filter(|p| w.screen().contains(*p)) else {
            rows.push(Row {
                surface: format!("{name} (the shape is not in view)"),
                ..Row::default()
            });
            continue;
        };
        w.right_click(at);
        let n = rows.len();
        w.audit_new(name, &before, &mut rows);
        if rows.len() == n {
            rows.push(Row {
                surface: format!("{name} (opens no menu)"),
                ..Row::default()
            });
        }
    }
    for (name, dx, dy) in [("Right click on the top ruler", 80.0, 6.0), ("Right click on the left ruler", 6.0, 80.0)] {
        let mut w = open(demo());
        let before = w.layers();
        let Some(canvas) = w.app.workspace.canvas else {
            continue;
        };
        w.right_click(canvas.min + egui::vec2(dx, dy));
        let n = rows.len();
        w.audit_new(name, &before, &mut rows);
        if rows.len() == n {
            rows.push(Row {
                surface: format!("{name} (opens no menu)"),
                ..Row::default()
            });
        }
    }
    // The other windows of the suite: their File menu and, in the Design Database, the menu
    // bar.
    type Suite = fn(&mut EditorApp);
    let suites: [(&str, Suite); 3] = [
        ("Programmable Stitch Creator", |a| {
            a.creator.open = true;
            a.creator.open_app_menu = true;
        }),
        ("Stitch Design Factory", |a| {
            a.sewing.open = true;
            a.sewing.open_app_menu = true;
        }),
        ("Design Database", |a| a.database.show(None)),
    ];
    for (name, opener) in suites {
        let mut plain = open(demo());
        opener(&mut plain.app);
        plain.app.creator.open_app_menu = false;
        plain.app.sewing.open_app_menu = false;
        plain.frames(5, Vec::new());
        let before = plain.layers();
        let mut w = open(demo());
        opener(&mut w.app);
        w.frames(5, Vec::new());
        w.audit_new(&format!("{name}, File menu"), &before, &mut rows);
        if name == "Design Database" {
            let bar: Vec<Control> = w
                .controls()
                .into_iter()
                .filter(|c| c.role == Role::Button && c.rect.max.y < 30.0)
                .collect();
            for c in bar {
                let mut w = open(demo());
                opener(&mut w.app);
                w.frames(5, Vec::new());
                let before = w.layers();
                if w.click_label(&c.label) {
                    w.audit_new(&format!("{name}, menu {}", c.label), &before, &mut rows);
                }
            }
        }
    }
    // What each command opens.
    let actions: Vec<Action> = openstitch_app::editor::ribbon::ribbon_actions()
        .into_iter()
        .chain(APPLICATION_MENU)
        .chain(OPTION_MENU)
        .collect();
    let mut done = HashSet::new();
    for a in actions {
        let info = a.info();
        if !info.available || interactive(a) || a.is_menu() || !done.insert(info.key) {
            continue;
        }
        let mut w = open(demo());
        if !crate::ribbon::enabled(&w.app, a) {
            continue;
        }
        let before = w.layers();
        let ctx = w.ctx.clone();
        crate::ribbon::run(&mut w.app, a, &ctx);
        w.frames(5, Vec::new());
        if w.app.database.open || w.app.creator.open || w.app.sewing.open {
            rows.push(w.audit(&format!("Command {} (whole window)", squash(info.label)), None));
        } else {
            w.audit_new(&format!("Command {}", squash(info.label)), &before, &mut rows);
        }
    }
    rows
}

fn write(px: (f32, f32), scale: u32) {
    let rows = survey(px, scale);
    eprintln!("done {px:?} {scale}");
    let mut out = String::from("surface\tcontrols\tnot in view\tunreachable\tbeyond x\tbeyond y\twhich\n");
    for r in &rows {
        out += &format!(
            "{}\t{}\t{}\t{}\t{:.0}\t{:.0}\t{}\n",
            r.surface,
            r.controls,
            r.hidden,
            r.lost.len(),
            r.beyond.0,
            r.beyond.1,
            r.lost.join("; ")
        );
    }
    let name = format!("survey-{}x{}-{}.tsv", px.0, px.1, scale);
    match std::env::var_os("OPENSTITCH_REACH_SURVEY") {
        Some(dir) => std::fs::write(std::path::Path::new(&dir).join(name), out).expect("write"),
        None => println!("{out}"),
    }
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_800x500_100() {
    write((800.0, 500.0), 100);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_640x400_100() {
    write((640.0, 400.0), 100);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_800x500_150() {
    write((800.0, 500.0), 150);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_800x500_200() {
    write((800.0, 500.0), 200);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_640x400_150() {
    write((640.0, 400.0), 150);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_640x400_200() {
    write((640.0, 400.0), 200);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_1440x900_100() {
    write((1440.0, 900.0), 100);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_900x600_100() {
    write((900.0, 600.0), 100);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_900x600_150() {
    write((900.0, 600.0), 150);
}

#[test]
#[ignore = "the survey: run by hand"]
fn survey_900x600_200() {
    write((900.0, 600.0), 200);
}

#[test]
#[ignore = "the survey: run by hand"]
fn probe() {
    let px: Vec<f32> = std::env::var("REACH_PROBE")
        .unwrap_or_else(|_| "640,400,100".into())
        .split(',')
        .map(|v| v.parse().expect("number"))
        .collect();
    let mut w = Win::new(demo(), (px[0], px[1]), px[2] as u32);
    eprintln!("screen {:?} canvas {:?}", w.screen(), w.app.workspace.canvas);
    if let Ok(find) = std::env::var("REACH_FIND") {
        for c in w.controls() {
            if c.label.contains(&find) || c.tip.contains(&find) {
                eprintln!("found {:?} label '{}' tip '{}' rect {:?}", c.role, c.label, c.tip, c.rect);
            }
        }
        for n in w.nodes.values() {
            let d = n.description().unwrap_or_default();
            let l = n.label().unwrap_or_default();
            if d.contains(&find) || l.contains(&find) {
                eprintln!("node {:?} label '{l}' description '{d}'", n.role());
            }
        }
        return;
    }
    let all = w.controls();
    let every = std::env::var("REACH_ALL").is_ok();
    for c in &all {
        if w.shown(c) {
            if every {
                eprintln!("seen {:?} '{}' rect {:?}", c.role, c.label, c.rect);
            }
            continue;
        }
        let at = w.wheel_point(c);
        let ok = w.reach(c.id, &mut HashSet::new());
        eprintln!(
            "{} {:?} '{}' rect {:?} seen {:?} wheel at {:?} layer {:?}",
            if ok { "ok  " } else { "LOST" },
            c.role,
            c.label,
            c.rect,
            c.seen,
            at,
            c.layer.order
        );
    }
}

/// The owner's report: Options at the top right is taller than a small window. It stays inside
/// the window and the wheel brings every entry into view.
#[test]
fn the_options_menu_scrolls_to_every_entry_in_a_small_window() {
    for (px, scale) in [((640.0, 400.0), 100), ((900.0, 600.0), 200)] {
        let mut w = Win::new(demo(), px, scale);
        let before = w.layers();
        assert!(w.click_label("Options"), "the Options button at {px:?}, {scale} %");
        let menu: HashSet<egui::LayerId> = w.layers().difference(&before).copied().collect();
        let row = w.audit("Options", Some(&menu));
        assert!(row.controls >= 30, "{row:?}");
        assert!(row.hidden > 0, "the menu is taller than the window: {row:?}");
        assert!(row.lost.is_empty(), "{px:?} at {scale} %: {row:?}");
        assert_eq!(row.beyond, (0.0, 0.0), "{px:?} at {scale} %: inside the window");
    }
}

/// A dialog taller than the window: its content scrolls and its buttons stay in view. Design
/// Property at 640 x 400 had Apply and Revert below the window's edge.
#[test]
fn a_dialog_scrolls_its_content_and_keeps_its_buttons_in_view() {
    let base = Win::new(demo(), (640.0, 400.0), 100).layers();
    let mut w = Win::new(
        Launch {
            show: vec!["property".into()],
            ..demo()
        },
        (640.0, 400.0),
        100,
    );
    w.frames(6, Vec::new());
    let dialog: HashSet<egui::LayerId> = w.layers().difference(&base).copied().collect();
    let controls: Vec<Control> =
        w.controls().into_iter().filter(|c| dialog.contains(&c.layer)).collect();
    // The buttons are in view before anything is scrolled.
    for label in ["Apply", "Revert"] {
        let b = controls.iter().find(|c| c.label == label).expect(label);
        assert!(w.shown(b), "{label} is in view: {b:?}");
    }
    let fields = controls.iter().filter(|c| !w.shown(c)).count();
    assert!(fields > 0, "the fields do not all fit at this size");
    let row = w.audit("Design Property", Some(&dialog));
    assert!(row.lost.is_empty(), "{row:?}");
    assert_eq!(row.beyond, (0.0, 0.0), "inside the window");
    // Still in view with the content scrolled to its end.
    for label in ["Apply", "Revert"] {
        let all = w.controls();
        let b = all.iter().find(|c| c.label == label).expect(label);
        assert!(w.shown(b), "{label} stays in view: {b:?}");
    }
    // A window with room for all of it lays the dialog out as before: nothing scrolls.
    let mut big = Win::new(
        Launch {
            show: vec!["property".into()],
            ..demo()
        },
        (1440.0, 900.0),
        100,
    );
    big.frames(6, Vec::new());
    let cut: Vec<Control> = big
        .controls()
        .into_iter()
        .filter(|c| dialog.contains(&c.layer) && !big.shown(c))
        .collect();
    assert!(cut.is_empty(), "{cut:?}");
}

/// The arrow keys walk the Options menu to its last entry, and the highlighted entry is in
/// view at every step (the list scrolls with it).
#[test]
fn the_arrow_keys_keep_the_highlighted_options_entry_in_view() {
    let mut w = Win::new(demo(), (640.0, 400.0), 100);
    let before = w.layers();
    assert!(w.click_label("Options"));
    let menu: HashSet<egui::LayerId> = w.layers().difference(&before).copied().collect();
    let entries: Vec<Control> =
        w.controls().into_iter().filter(|c| menu.contains(&c.layer)).collect();
    let last = entries
        .iter()
        .max_by(|a, b| a.rect.max.y.total_cmp(&b.rect.max.y))
        .expect("entries")
        .clone();
    assert!(!w.shown(&last), "the last entry starts below the window: {last:?}");
    let mut walked = HashSet::new();
    let mut at = None;
    for step in 0..80 {
        w.key(egui::Key::ArrowDown);
        w.frames(6, Vec::new());
        if w.ctx.memory(|m| m.focused()) == at {
            // A number field takes the arrows for its value: Tab goes on from it.
            w.key(egui::Key::Tab);
            w.frames(6, Vec::new());
        }
        at = w.ctx.memory(|m| m.focused());
        let Some(id) = at else {
            continue;
        };
        let all = w.controls();
        let Some(c) = all.iter().find(|c| c.id == id) else {
            continue;
        };
        assert!(w.shown(c), "step {step}: the highlighted entry is cut off: {c:?}");
        walked.insert(id);
        if id == last.id {
            break;
        }
    }
    let missed: Vec<String> = entries
        .iter()
        .filter(|c| !walked.contains(&c.id))
        .map(|c| format!("{:?} '{}'", c.role, c.label))
        .collect();
    assert!(walked.contains(&last.id), "the keys reach the last entry; missed {missed:?}");
    assert!(walked.len() >= 20, "walked {} entries", walked.len());
}
