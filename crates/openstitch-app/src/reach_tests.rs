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
    /// The controls seen in full while scrolling, by role, name and layer (see `audit`).
    seen_named: HashSet<(String, String, egui::LayerId)>,
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
            seen_named: HashSet::new(),
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

    /// In view in full. The one point of slack is for rounding: a control's rectangle and
    /// the clip it is drawn in are placed at fractions of a point and snapped separately.
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
        (0..12).any(|from| self.reach_from(id, ok, from) == Some(true))
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
                    // (Its middle and its other end too: another window may lie over a part.)
                    let near_y = if far_y == r.min.y { r.max.y } else { r.min.y };
                    for p in [
                        egui::pos2(centre.x.clamp(r.min.x, r.max.x), far_y),
                        egui::pos2(far_x, centre.y.clamp(r.min.y, r.max.y)),
                        r.center(),
                        egui::pos2(r.center().x, near_y),
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
        // Moving the pointer to another place takes a moment to tell: a sub-menu that opened
        // under the resting pointer (and, in a narrow window, over its own menu, which then
        // does not get the wheel) closes a little after the pointer has left its entry.
        if from > 0 {
            let at = self
                .controls()
                .iter()
                .find(|c| c.id == id)
                .and_then(|c| self.other_wheel_points(c).get(from - 1).copied())?;
            self.frames(1, vec![egui::Event::PointerMoved(at)]);
            self.frames(30, Vec::new());
        }
        let (mut last, mut still) = (None, 0);
        // A control larger than the view it scrolls in (a button wider than a squeezed pane)
        // cannot be shown at once: it is reached when both of its ends have been in view.
        let mut ends = [false; 4];
        let trace = std::env::var("REACH_TRACE").is_ok();
        for _ in 0..if from == 0 { 300 } else { 50 } {
            let all = self.controls();
            for c in &all {
                if self.shown(c) {
                    ok.insert(c.id);
                    self.seen_named.insert((format!("{:?}", c.role), c.label.clone(), c.layer));
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
            // Nothing moves any more: neither the control nor any menu or dialog (a sub-menu
            // over the list may take the wheel to its own end first, then pass it on).
            let scrolled: Vec<i32> =
                self.app.drawn.list.iter().map(|s| s.offset.round() as i32).collect();
            let now = (c.rect, scrolled);
            if last.as_ref() == Some(&now) {
                still += 1;
                if still >= 2 {
                    return Some(false);
                }
            } else {
                still = 0;
            }
            last = Some(now);
            let at = match from {
                0 => self.wheel_point(c),
                n => *self.other_wheel_points(c).get(n - 1)?,
            };
            if trace {
                eprintln!("    from {from} at {at:?}: rect {:?} seen {:?} ends {ends:?}", c.rect, c.seen);
            }
            let v = self.visible(c);
            let part = v.width() > 1.0 && v.height() > 1.0;
            // How far to turn the wheel on one axis (points, at most 40 a step): just enough to
            // bring the cut end in; the far end first, then, once that has been seen, the near
            // one (a control larger than its view shows them one after the other).
            let axis = |lo: f32, hi: f32, vlo: f32, vhi: f32, p: f32, seen_hi: bool| -> f32 {
                let (over, under) = (hi - vhi, vlo - lo);
                if part {
                    if over > 0.5 && !(seen_hi && under > 0.5) {
                        -(over + 1.0).min(40.0)
                    } else if under > 0.5 {
                        (under + 1.0).min(40.0)
                    } else {
                        0.0
                    }
                } else if (lo + hi) / 2.0 > p + 1.0 {
                    -40.0
                } else if (lo + hi) / 2.0 < p - 1.0 {
                    40.0
                } else {
                    0.0
                }
            };
            let dx = axis(c.rect.min.x, c.rect.max.x, v.min.x, v.max.x, at.x, ends[1]);
            let dy = axis(c.rect.min.y, c.rect.max.y, v.min.y, v.max.y, at.y, ends[3]);
            self.frames(1, vec![egui::Event::PointerMoved(at)]);
            self.frames(
                1,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(dx, dy),
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
        // (By hand: REACH_ONLY=text audits only the surfaces whose name has it.)
        if std::env::var("REACH_ONLY").is_ok_and(|only| !surface.contains(&only)) {
            return Row {
                surface: surface.to_string(),
                ..Row::default()
            };
        }
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
        self.seen_named.clear();
        let hidden: Vec<Control> = all.iter().filter(|c| !self.shown(c)).cloned().collect();
        let mut lost = Vec::new();
        for c in &hidden {
            if ok.contains(&c.id) {
                continue;
            }
            // A menu gives its entries their identity by their order, and an entry that opens
            // a sub-menu under the resting pointer shifts that for the ones after it: such an
            // entry is known by its role and name instead, when the surface has only one of
            // that name and it was seen in full on the way.
            let named = (format!("{:?}", c.role), c.label.clone(), c.layer);
            let only = !c.label.is_empty()
                && all.iter().filter(|o| o.role == c.role && o.label == c.label && o.layer == c.layer).count() == 1;
            if !self.reach(c.id, &mut ok) && !(only && self.seen_named.contains(&named)) {
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
/// Walks every surface at a window of `px` pixels and a Text size of `scale` %. With `part`
/// `(n, of)` only every `of`-th surface, starting at the `n`-th: the parts together are the
/// whole walk, so they can run side by side.
fn survey(px: (f32, f32), scale: u32, part: Option<(usize, usize)>) -> Vec<Row> {
    let mut rows = Vec::new();
    let count = std::cell::Cell::new(0usize);
    let mine = || {
        let n = count.get();
        count.set(n + 1);
        part.is_none_or(|(at, of)| n % of == at)
    };
    let open = |launch: Launch| Win::new(launch, px, scale);
    // The main window with each ribbon tab: ribbon, panes, status bar.
    let base = open(demo()).layers();
    for tab in TabId::ALL {
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
        if !mine() {
            continue;
        }
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
    let rows = survey(px, scale, None);
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

/// `ui.state` says which menus and dialogs are on screen and how far each is scrolled (the
/// browser version's page test reads it), and the command's summary names every key.
#[test]
fn ui_state_reports_the_open_menu_and_how_far_it_is_scrolled() {
    let mut w = Win::new(demo(), (640.0, 400.0), 100);
    let surfaces = |w: &mut Win| w.app.ui_command("ui.state", None).expect("ui.state")["surfaces"].clone();
    let s = surfaces(&mut w);
    assert_eq!(s["windowPoints"], serde_json::json!([640.0, 400.0]));
    assert_eq!(s["textScale"], serde_json::json!(1.0));
    assert_eq!(s["open"], serde_json::json!([]), "nothing is open at the start");
    let b: Vec<f32> = s["optionsButton"]
        .as_array()
        .expect("the Options button is drawn")
        .iter()
        .map(|v| v.as_f64().expect("number") as f32)
        .collect();
    let at = egui::pos2(b[0] + b[2] / 2.0, b[1] + b[3] / 2.0);
    w.click(at);
    w.frames(6, Vec::new());
    let s = surfaces(&mut w);
    let open = s["open"].as_array().expect("open").clone();
    let focus = w.ctx.memory(|m| m.focused());
    let focused: Vec<String> = w.controls().iter().filter(|c| Some(c.id) == focus).map(|c| format!("{:?} {} {:?}", c.role, c.label, c.rect)).collect();
    assert_eq!(open.len(), 1, "{s} focus {focused:?}");
    let num = |v: &serde_json::Value| v.as_f64().expect("number");
    let rect: Vec<f64> = open[0]["rect"].as_array().expect("rect").iter().map(num).collect();
    assert_eq!(open[0]["kind"], "menu");
    assert!(rect[0] >= 0.0 && rect[1] >= 0.0, "{rect:?}");
    assert!(rect[0] + rect[2] <= 640.0 && rect[1] + rect[3] <= 400.0, "inside the window: {rect:?}");
    assert!(num(&open[0]["contentHeight"]) > rect[3] + 100.0, "the menu is taller than its view: {s}");
    assert_eq!(num(&open[0]["scrolled"]), 0.0);
    assert!(open[0]["buttons"].is_null());
    // The wheel over the menu scrolls it.
    let over = egui::pos2((rect[0] + rect[2] / 2.0) as f32, (rect[1] + rect[3] / 2.0) as f32);
    w.frames(1, vec![egui::Event::PointerMoved(over)]);
    w.frames(
        1,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -120.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    w.frames(8, Vec::new());
    // (The pointer now rests on an entry, which may have opened its sub-menu beside it.)
    let s = surfaces(&mut w);
    let options = s["open"]
        .as_array()
        .expect("open")
        .iter()
        .find(|o| num(&o["rect"][0]) == rect[0] && num(&o["rect"][2]) == rect[2])
        .unwrap_or_else(|| panic!("the Options menu is still open: {s}"));
    assert!(num(&options["scrolled"]) > 50.0, "{s}");
    let summary = openstitch_app::editor::assistant::ui_command("ui.state")
        .expect("ui.state")
        .summary;
    for key in ["surfaces", "windowPoints", "textScale", "optionsButton", "contentHeight", "scrolled", "buttons"] {
        assert!(summary.contains(key), "the summary of ui.state names {key}");
    }
}

/// What may be made outside `fit`, and why: (file, call, how many times).
const MADE_OUTSIDE_FIT: &[(&str, &str, usize, &str)] = &[
    ("app.rs", "egui::Window::new(", 1, "dialog_window builds the window its callers show through fit"),
    ("pickers_ui.rs", "Popup::from_response(", 1, "a picture picker scrolls its own grid, limited by fit::room"),
    ("splash.rs", "Area::new(", 1, "the splash picture has no controls"),
    ("start_ui.rs", "Area::new(", 1, "the start cards: the reach walk checks them (scene getting-started)"),
    ("point_menu_ui.rs", "Area::new(", 1, "the point menus put their body in fit::scroll"),
    ("view_ui.rs", "Area::new(", 1, "the guide menu puts its body in fit::scroll"),
];

/// Every menu, pop-up, dialog and prompt of the app is made through `fit`, so it stays inside
/// the window and scrolls: a new one made with the toolkit's own call fails here, with the
/// call to use instead.
#[test]
fn every_menu_and_dialog_is_made_through_fit() {
    // (the toolkit's call, the `fit` calls that may wrap it)
    let rules: &[(&str, &[&str])] = &[
        ("egui::Window::new(", &["crate::fit::show(", "crate::fit::show_filling("]),
        (".dialog_window(", &["crate::fit::show(", "crate::fit::show_filling("]),
        ("egui::Modal::new(", &["crate::fit::modal("]),
        ("Popup::menu(", &["crate::fit::popup("]),
        ("Popup::from_response(", &["crate::fit::popup("]),
        ("Popup::context_menu(", &["crate::fit::popup("]),
        (".menu_button(", &[]),
        (".context_menu(", &[]),
        ("Area::new(", &[]),
    ];
    fn files(dir: &std::path::Path, out: &mut Vec<std::path::PathBuf>) {
        for e in std::fs::read_dir(dir).expect("src").flatten() {
            let p = e.path();
            if p.is_dir() {
                files(&p, out);
            } else if p.extension().is_some_and(|x| x == "rs") {
                out.push(p);
            }
        }
    }
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut all = Vec::new();
    files(&src, &mut all);
    assert!(all.len() > 50, "the app's sources: {}", all.len());
    let mut found: HashMap<(String, &str), Vec<usize>> = HashMap::new();
    for path in &all {
        let name = path.strip_prefix(&src).expect("in src").to_string_lossy().replace('\\', "/");
        if name == "fit.rs" || name.ends_with("_tests.rs") {
            continue;
        }
        let text = std::fs::read_to_string(path).expect("source");
        for (call, wrappers) in rules {
            for (at, _) in text.match_indices(call) {
                let before = &text[..at];
                // Text in a string or a comment is not a call.
                let line = &before[before.rfind('\n').map_or(0, |i| i + 1)..];
                if before.ends_with('"') || line.trim_start().starts_with("//") {
                    continue;
                }
                // `crate::fit::show(app\n    .dialog_window(`, `crate::fit::popup(\n    egui::Popup::menu(`
                let head = before
                    .trim_end()
                    .trim_end_matches(|c: char| c.is_alphanumeric() || c == '_' || c == ':')
                    .trim_end();
                if wrappers.iter().any(|w| head.ends_with(w)) {
                    continue;
                }
                let line_nr = before.matches('\n').count() + 1;
                found.entry((name.clone(), call)).or_default().push(line_nr);
            }
        }
    }
    let mut wrong = Vec::new();
    for ((file, call), lines) in &found {
        let allowed = MADE_OUTSIDE_FIT
            .iter()
            .find(|a| a.0 == file && a.1 == *call)
            .map_or(0, |a| a.2);
        if lines.len() != allowed {
            wrong.push(format!("{file}: {call} at lines {lines:?} ({allowed} allowed)"));
        }
    }
    for (file, call, _, why) in MADE_OUTSIDE_FIT {
        assert!(
            found.contains_key(&(file.to_string(), *call)),
            "{file} no longer has {call} ({why}): take it off MADE_OUTSIDE_FIT"
        );
    }
    wrong.sort();
    assert!(
        wrong.is_empty(),
        "made without crate::fit (use fit::show, fit::modal, fit::menu, fit::popup, \
         fit::sub_menu or fit::context_menu, so it fits a small window, #1276):\n{}",
        wrong.join("\n")
    );
}

/// A tooltip cannot scroll, so the tallest one of the ribbon, menus and Quick Access Toolbar
/// has to fit the smallest window of the survey: 640 × 400 at Text size 200 %, 320 × 200
/// points.
#[test]
fn the_tallest_command_tooltip_fits_the_smallest_window() {
    let w = Win::new(demo(), (640.0, 400.0), 200);
    let screen = w.screen();
    let style = w.ctx.global_style();
    let font = egui::TextStyle::Body.resolve(&style);
    let margin = egui::Frame::popup(&style).total_margin().sum();
    let wrap = style.spacing.tooltip_width.min(screen.width() - margin.x - 2.0 * crate::fit::EDGE);
    let mut tallest = (0.0f32, String::new());
    for a in Action::ALL {
        let text = a.info().screen_tip(false);
        let galley = w
            .ctx
            .fonts_mut(|f| f.layout(text.clone(), font.clone(), egui::Color32::WHITE, wrap));
        let height = galley.size().y + margin.y;
        if height > tallest.0 {
            tallest = (height, text);
        }
    }
    eprintln!("the tallest tooltip: {:.0} of {:.0} points: {}", tallest.0, screen.height(), tallest.1);
    assert!(tallest.0 > 30.0, "tooltips were measured: {tallest:?}");
    assert!(
        tallest.0 <= screen.height() - 2.0 * crate::fit::EDGE,
        "a tooltip {:.0} points high does not fit a window {:.0} points high: {}",
        tallest.0,
        screen.height(),
        tallest.1
    );
}

/// The walk of the registry at the smallest browser window (640 × 400): every ribbon tab and
/// its lists, the top bar's menus, every `--show` scene, every dialog, wizard and pane, the
/// right-click menus, the other windows of the suite, and whatever each ribbon, menu and
/// Quick Access Toolbar command opens. It fails for a control that cannot be brought into
/// view with the wheel, and for a menu or dialog that reaches beyond the window. A new
/// command, scene or tab is walked without a change here (they come from the registries).
fn walk(part: usize) {
    let rows = survey((640.0, 400.0), 100, Some((part, WALK_PARTS)));
    assert!(rows.len() > 20, "part {part} walked {} surfaces", rows.len());
    let wrong: Vec<String> = rows
        .iter()
        .filter(|r| !r.lost.is_empty() || r.beyond.0 > 1.0 || r.beyond.1 > 1.0)
        .map(|r| format!("{}: out of reach {:?}, beyond the window by {:?}", r.surface, r.lost, r.beyond))
        .collect();
    assert!(wrong.is_empty(), "at 640 x 400 (#1276):
{}", wrong.join("
"));
}

/// The walk runs as this many tests, side by side.
const WALK_PARTS: usize = 6;

#[test]
fn every_control_can_be_reached_at_640x400_part_1() {
    walk(0);
}

#[test]
fn every_control_can_be_reached_at_640x400_part_2() {
    walk(1);
}

#[test]
fn every_control_can_be_reached_at_640x400_part_3() {
    walk(2);
}

#[test]
fn every_control_can_be_reached_at_640x400_part_4() {
    walk(3);
}

#[test]
fn every_control_can_be_reached_at_640x400_part_5() {
    walk(4);
}

#[test]
fn every_control_can_be_reached_at_640x400_part_6() {
    walk(5);
}

/// A window with room is laid out as it was before #1276: the Sewing Order pane is as wide as
/// its content (the monkey test's seed 881 caught it squeezed to its default width and
/// scrolling sideways), and nothing in it waits behind a scroll bar.
#[test]
fn a_pane_with_room_is_as_wide_as_its_content() {
    let w = Win::new(demo(), (1440.0, 900.0), 100);
    let canvas = w.app.workspace.canvas.expect("canvas");
    assert!(canvas.min.x > 280.0, "the Sewing Order pane is 286 points wide at this size: {canvas:?}");
    let all = w.controls();
    for label in ["Branch", "Then: run", "Sew in click order", "Optimise"] {
        let c = all.iter().find(|c| c.label == label).unwrap_or_else(|| panic!("{label}"));
        assert!(w.shown(c), "{label} is in view without scrolling: {c:?}");
    }
}

/// One turn of the wheel (30 points down) with the pointer at `at`.
fn wheel_down(w: &mut Win, at: egui::Pos2) {
    w.frames(1, vec![egui::Event::PointerMoved(at)]);
    w.frames(
        1,
        vec![egui::Event::MouseWheel {
            unit: egui::MouseWheelUnit::Point,
            delta: egui::vec2(0.0, -30.0),
            phase: egui::TouchPhase::Move,
            source: egui::MouseWheelSource::Wheel,
            modifiers: egui::Modifiers::NONE,
        }],
    );
    w.frames(8, Vec::new());
}

/// In a narrow window (640 × 400 at Text size 200 %: 320 points) a sub-menu opens over the
/// menu it belongs to, under the resting pointer. The wheel goes on scrolling the menu then:
/// it used to stop at the first entry with a sub-menu.
#[test]
fn the_wheel_scrolls_a_menu_on_past_an_entry_that_opens_a_sub_menu() {
    let mut w = Win::new(demo(), (640.0, 400.0), 200);
    assert!(w.click_label("Options"));
    w.frames(10, Vec::new());
    let menu = |w: &Win| {
        let list = &w.app.drawn.list;
        list.iter()
            .max_by(|a, b| a.content.total_cmp(&b.content))
            .cloned()
            .expect("the Options menu")
    };
    let end = menu(&w).content - menu(&w).rect.height();
    assert!(end > 500.0, "the menu is far taller than the window: {:?}", menu(&w));
    let mut sub_menus = 0;
    for _ in 0..60 {
        let m = menu(&w);
        sub_menus = sub_menus.max(w.app.drawn.list.len() - 1);
        if m.offset >= end - 1.0 {
            break;
        }
        // The pointer rests near the top of the menu, as a hand on a wheel does.
        wheel_down(&mut w, egui::pos2(m.rect.center().x, m.rect.top() + 12.0));
    }
    assert!(sub_menus > 0, "a sub-menu opened under the pointer on the way");
    let m = menu(&w);
    assert!(m.offset >= end - 1.0, "the wheel reached the end of the menu: {m:?}, end {end}");
}

/// By hand: one `--show` scene (REACH_SCENE) at REACH_PROBE's size, audited alone.
#[test]
#[ignore = "the survey: run by hand"]
fn probe_scene() {
    let px: Vec<f32> = std::env::var("REACH_PROBE")
        .unwrap_or_else(|_| "640,400,100".into())
        .split(',')
        .map(|v| v.parse().expect("number"))
        .collect();
    let scene = std::env::var("REACH_SCENE").unwrap_or_else(|_| "property".into());
    let base = Win::new(demo(), (px[0], px[1]), px[2] as u32).layers();
    let mut w = Win::new(
        Launch {
            show: vec![scene.clone()],
            ..demo()
        },
        (px[0], px[1]),
        px[2] as u32,
    );
    w.frames(6, Vec::new());
    let mut rows = Vec::new();
    if w.app.database.open || w.app.creator.open || w.app.sewing.open {
        rows.push(w.audit(&format!("Scene {scene} (whole window)"), None));
    } else {
        w.audit_new(&format!("Scene {scene}"), &base, &mut rows);
    }
    for r in rows {
        eprintln!("{r:?}");
    }
}
