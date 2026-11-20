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
    /// Greyed out: it cannot take the keyboard focus.
    disabled: bool,
}

struct Win {
    app: EditorApp,
    ctx: egui::Context,
    frame: eframe::Frame,
    /// The window in points (its size in pixels over the UI scale).
    size: egui::Vec2,
    time: f64,
    nodes: HashMap<NodeId, Node>,
    /// What the last frame painted.
    shapes: Vec<egui::epaint::ClippedShape>,
    /// Whether the last frame asked for another one at once.
    repaint: bool,
    /// False for a walk that only counts what opens (no wheel is turned).
    measure: bool,
    /// By hand (the survey): audit only the surfaces whose name has this text.
    only: Option<String>,
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
    /// What it is: main, list, menu, sub-menu, drop-down, scene, dialog, wizard, pane, suite
    /// or command.
    kind: &'static str,
    /// A dialog that keeps its button row in view below its scrolled content.
    pinned: bool,
    /// What else is wrong with it (the pinned row).
    problems: Vec<String>,
    /// Not measured, and why (an entry of `OPENS_NOTHING`).
    skipped: Option<&'static str>,
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
            shapes: Vec::new(),
            repaint: false,
            measure: true,
            only: None,
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
            self.repaint = out
                .viewport_output
                .get(&egui::ViewportId::ROOT)
                .is_some_and(|v| v.repaint_delay < std::time::Duration::from_millis(20));
            self.shapes = std::mem::take(&mut out.shapes);
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
                        disabled: n.is_disabled(),
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

    /// In view in full, and nothing lies over its middle (a window or a menu of another layer
    /// there would take the click). The one point of slack is for rounding: a control's
    /// rectangle and the clip it is drawn in are placed at fractions of a point and snapped
    /// separately.
    fn shown(&self, c: &Control) -> bool {
        let v = self.visible(c);
        v.width() >= c.rect.width() - 1.0 && v.height() >= c.rect.height() - 1.0 && self.on_top(c)
    }

    /// Whether `c`'s layer is the one a click at the middle of its visible part goes to.
    fn on_top(&self, c: &Control) -> bool {
        let v = self.visible(c);
        if !(v.width() > 0.0 && v.height() > 0.0) {
            return false;
        }
        self.ctx.layer_id_at(v.center()).is_none_or(|top| top == c.layer)
    }

    /// The size of the view `c` scrolls in: the smallest of what its containers show.
    fn view_of(&self, c: &Control) -> egui::Vec2 {
        let mut view = self.size;
        self.widgets(|all| {
            let mut id = c.parent;
            for _ in 0..40 {
                let Some(w) = all.get(id) else { break };
                let r = w.interact_rect;
                if r.width() > 1.0 && r.height() > 1.0 {
                    view = view.min(r.size());
                }
                if w.parent_id == id {
                    break;
                }
                id = w.parent_id;
            }
        });
        view
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
        self.reach_in(id, ok, 12)
    }

    /// As [`Self::reach`], from at most `places` places: first with the pointer on the control
    /// or the nearest view of it; then, as a person does when a list inside a scrolling pane
    /// takes the wheel, from other places in the containers around it.
    fn reach_in(&mut self, id: egui::Id, ok: &mut HashSet<egui::Id>, places: usize) -> bool {
        (0..places).any(|from| self.reach_from(id, ok, from) == Some(true))
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
        let mut nudged = 0;
        let mut moved_to: Option<egui::Pos2> = None;
        // A control larger than the view it scrolls in (a button wider than a squeezed pane)
        // cannot be shown at once: it is reached when both of its ends have been in view.
        let mut ends = [false; 4];
        let trace = std::env::var("REACH_TRACE").is_ok();
        for _ in 0..if from == 0 { 300 } else { 50 } {
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
            // Only for a control really larger than its view on that axis; any other has to
            // be in view whole.
            let view = self.view_of(c);
            let larger = [c.rect.width() > view.x + 0.5, c.rect.height() > view.y + 0.5];
            if self.on_top(c) && v.width() >= c.rect.width().min(16.0) && v.height() >= c.rect.height().min(12.0) {
                ends[0] |= v.min.x <= c.rect.min.x + 0.5;
                ends[1] |= v.max.x >= c.rect.max.x - 0.5;
                ends[2] |= v.min.y <= c.rect.min.y + 0.5;
                ends[3] |= v.max.y >= c.rect.max.y - 0.5;
            }
            let covered = wide && tall && !self.on_top(c);
            if !covered
                && (wide || (larger[0] && ends[0] && ends[1]))
                && (tall || (larger[1] && ends[2] && ends[3]))
            {
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
            let mut dy = axis(c.rect.min.y, c.rect.max.y, v.min.y, v.max.y, at.y, ends[3]);
            if covered {
                // Under a window that lies over it (a sub-menu that opened over its own menu
                // in a narrow window, under the resting pointer): first, as a person does,
                // the pointer goes to a part of the control that still shows, which closes
                // a sub-menu of another entry.
                // (Or, when all of it is covered, to a part of its menu or window that shows:
                // the sub-menu of the entry under the pointer closes when the pointer leaves.)
                let on_mine = |p: &egui::Pos2| self.ctx.layer_id_at(*p).is_none_or(|top| top == c.layer);
                let area = self.ctx.memory(|m| m.area_rect(c.layer.id)).unwrap_or(v);
                let free = (1..10)
                    .map(|k| egui::pos2(v.left() + v.width() * k as f32 / 10.0, v.center().y))
                    .find(on_mine)
                    .or_else(|| {
                        let area = area.intersect(self.screen().shrink(2.0));
                        (1..8)
                            .flat_map(|i| (1..8).map(move |j| (i, j)))
                            .map(|(i, j)| {
                                egui::pos2(
                                    area.left() + area.width() * i as f32 / 8.0,
                                    area.top() + area.height() * j as f32 / 8.0,
                                )
                            })
                            .find(on_mine)
                    });
                if let Some(p) = free.filter(|p| moved_to.is_none_or(|q| q.distance(*p) > 1.0)) {
                    moved_to = Some(p);
                    let before = self.layers_on_screen();
                    self.frames(1, vec![egui::Event::PointerMoved(p)]);
                    self.frames(30, Vec::new());
                    // An entry that opens a sub-menu under the pointer, and the sub-menu lies
                    // over the entry itself (a window too narrow to open it beside): the
                    // entry has done what it is for.
                    let now = self.controls().into_iter().find(|n| n.id == c.id);
                    if let Some(now) = now.filter(|n| self.visible(n).width() > 1.0) {
                        let top = self.ctx.layer_id_at(self.visible(&now).center());
                        if top.is_some_and(|t| t != now.layer && !before.contains(&t)) {
                            return Some(true);
                        }
                    }
                    still = 0;
                    continue;
                }
                if nudged == 0 && self.covered_by_its_own_sub_menu(id) {
                    return Some(true);
                }
                // (A few steps a place: what lies over it may move along, as a sub-menu
                // does with its menu, and then no step frees it.)
                nudged += 1;
                if nudged > 8 {
                    return Some(false);
                }
                // In view, but under a window that lies over it (a folded pane's title bar
                // in a window that scrolls as a page): the page is moved, one way from the
                // first places and the other way from the next.
                dy = if from.is_multiple_of(2) { -30.0 } else { 30.0 };
            }
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

    /// The layers that have a control on screen.
    fn layers_on_screen(&self) -> HashSet<egui::LayerId> {
        self.layers()
    }

    /// Whether what lies over `id` is its own sub-menu (in a window too narrow to open it
    /// beside, a sub-menu opens over its own entry): the toolkit names a sub-menu's layer
    /// after its entry. The pointer rests on the entry to open it first.
    fn covered_by_its_own_sub_menu(&mut self, id: egui::Id) -> bool {
        let find = |w: &Win| w.controls().into_iter().find(|n| n.id == id);
        let Some(c) = find(self) else { return false };
        let at = self.visible(&c).center();
        self.frames(1, vec![egui::Event::PointerMoved(at)]);
        self.frames(30, Vec::new());
        let own = egui::containers::menu::SubMenu::id_from_widget_id(id);
        self.ctx.layer_id_at(at).is_some_and(|t| t.id == own)
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
        // (The widest of that name: the Quick Access Toolbar has small buttons called as a
        // menu or a list of the ribbon is, Help and Zoom.)
        let Some(c) = self
            .controls()
            .into_iter()
            .filter(named)
            .max_by(|a, b| a.rect.width().total_cmp(&b.rect.width()))
        else {
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

    /// The pinned button rows of the dialogs on screen, each from its dialog's view's corner,
    /// with what is wrong with each (a row outside the window or over the content, a control
    /// of it cut, a row that shows none).
    fn pinned_rows(&self, mine: &dyn Fn(&Control) -> bool) -> (Vec<egui::Rect>, Vec<String>) {
        let screen = self.screen();
        let mut wrong = Vec::new();
        let rows: Vec<(egui::Rect, egui::Rect)> =
            self.app.drawn.list.iter().filter_map(|s| s.buttons.map(|b| (s.rect, b))).map(|(view, row)| {
                if !screen.expand(0.5).contains_rect(row) {
                    wrong.push(format!("the pinned button row {row:?} is not inside the window"));
                }
                if row.top() < view.bottom() - 0.5 {
                    wrong.push(format!("the pinned button row {row:?} lies over the content {view:?}"));
                }
                (view, row)
            }).collect();
        // The row's controls: the ones whose visible part is there (content scrolled under the
        // row has the same place, but is clipped away at the view's edge above it). Each is in view from top to bottom (a row
        // wider than the window scrolls sideways), and there is at least one.
        let mut in_row = 0;
        for c in self.controls().iter().filter(|c| mine(c)) {
            let v = self.visible(c);
            if !(v.width() > 1.0 && v.height() > 1.0)
                || !rows.iter().any(|(_, r)| r.expand(1.0).contains(v.center()))
            {
                continue;
            }
            in_row += 1;
            if v.height() < c.rect.height() - 1.0 {
                wrong.push(format!("{:?} '{}' of the pinned row is cut", c.role, c.label));
            }
        }
        if !rows.is_empty() && in_row == 0 {
            wrong.push("the pinned button row shows no control".to_string());
        }
        // Where each row is in its dialog (the dialog may move as a whole, as a window does).
        let places = rows
            .iter()
            .map(|(view, row)| row.translate(-view.min.to_vec2()))
            .collect();
        (places, wrong)
    }

    /// Brings the ribbon button of `a` into view and clicks it. The button is known by its
    /// ScreenTip: a tab (Shapes, Text) or a small button of the Quick Access Toolbar (Zoom)
    /// can have its name.
    fn click_action(&mut self, a: Action) -> bool {
        let tip = a.info().screen_tip(crate::ribbon::MAC);
        let Some(c) = self
            .controls()
            .into_iter()
            .filter(|c| c.tip == tip)
            .max_by(|x, y| x.rect.width().total_cmp(&y.rect.width()))
        else {
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

    /// Scrolls the window's page (a window lower than its layout) until `at` gives a place
    /// inside the window, and returns it.
    fn in_view(&mut self, at: impl Fn(&mut Win) -> Option<egui::Pos2>) -> Option<egui::Pos2> {
        let inside = self.screen().shrink(4.0);
        for _ in 0..30 {
            let p = at(self)?;
            if inside.contains(p) {
                return Some(p);
            }
            // (The wheel over the top bar, which belongs to the page.)
            let down = if p.y > inside.max.y { -30.0 } else { 30.0 };
            self.frames(1, vec![egui::Event::PointerMoved(egui::pos2(inside.center().x, 10.0))]);
            self.frames(
                1,
                vec![egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, down),
                    phase: egui::TouchPhase::Move,
                    source: egui::MouseWheelSource::Wheel,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
            self.frames(8, Vec::new());
        }
        None
    }

    /// Checks the controls of `layers` (all layers when `None`).
    fn audit(&mut self, surface: &str, kind: &'static str, layers: Option<&HashSet<egui::LayerId>>) -> Row {
        if self.only.as_ref().is_some_and(|only| !surface.contains(only.as_str())) {
            return Row {
                surface: surface.to_string(),
                kind,
                ..Row::default()
            };
        }
        // (A window that has just opened sizes itself over its first frames.)
        self.frames(10, Vec::new());
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
        // A dialog's pinned row: in view before anything is scrolled, and in the same place
        // with all its controls in view after.
        let own_row = !matches!(kind, "sub-menu" | "drop-down" | "list" | "menu");
        let (pinned, mut problems) = if own_row {
            self.pinned_rows(&mine)
        } else {
            (Vec::new(), Vec::new())
        };
        let mut lost = Vec::new();
        let measure = self.measure;
        for c in hidden.iter().filter(|_| measure) {
            if ok.contains(&c.id) {
                continue;
            }
            if !self.reach(c.id, &mut ok) {
                if let Some(now) = self.controls().into_iter().find(|n| n.id == c.id) {
                    eprintln!(
                        "  LOST in {surface}: {:?} '{}' rect {:?} seen {:?} layer {:?} top {:?}",
                        now.role,
                        now.label,
                        now.rect,
                        now.seen,
                        now.layer,
                        self.ctx.layer_id_at(self.visible(&now).center())
                    );
                }
                lost.push(format!("{:?} '{}'", c.role, c.label));
            }
        }
        if self.measure && !pinned.is_empty() {
            let (after, wrong) = self.pinned_rows(&mine);
            problems.extend(wrong.into_iter().map(|w| format!("{w} (after scrolling)")));
            let moved = after.len() != pinned.len()
                || after.iter().zip(&pinned).any(|(a, b)| {
                    (a.min - b.min).length() > 1.0 || (a.max - b.max).length() > 1.0
                });
            if moved {
                problems.push(format!(
                    "the pinned button row moved in its dialog: {pinned:?} to {after:?}"
                ));
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
            kind,
            controls: all.len(),
            hidden: hidden.len(),
            lost,
            beyond: (beyond.0.max(0.0), beyond.1.max(0.0)),
            pinned: !pinned.is_empty(),
            problems,
            skipped: None,
        }
    }

    /// Audits the layers that are not in `base`; then opens each of their sub-menus (a hover),
    /// each combo box and each colour button in turn and audits those too.
    fn audit_new(
        &mut self,
        surface: &str,
        kind: &'static str,
        base: &HashSet<egui::LayerId>,
        rows: &mut Vec<Row>,
    ) {
        let new: HashSet<egui::LayerId> =
            self.layers().difference(base).copied().collect();
        if new.is_empty() {
            return;
        }
        rows.push(self.audit(surface, kind, Some(&new)));
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
        // (Each control once, by its identity: two of the same name are two.)
        let mut done = HashSet::new();
        let mut names: HashMap<String, usize> = HashMap::new();
        for c in items {
            let menu_item = matches!(c.role, Role::Button | Role::MenuItem);
            let clicked = matches!(c.role, Role::ComboBox | Role::ColorWell);
            if !(menu_item || clicked) || c.disabled || !done.insert(c.id) {
                continue;
            }
            // (Whether it can be reached at all is the audit's question, asked with patience;
            // this only brings it under the pointer to see what it opens.)
            if !self.reach_in(c.id, &mut HashSet::new(), 3) {
                continue;
            }
            let Some(c) = self.controls().into_iter().find(|x| x.id == c.id) else {
                continue;
            };
            let before = self.layers();
            let at = self.visible(&c).center();
            if clicked {
                self.click(at);
            } else {
                self.frames(1, vec![egui::Event::PointerMoved(at)]);
                self.frames(6, Vec::new());
            }
            let opened: HashSet<egui::LayerId> =
                self.layers().difference(&before).copied().collect();
            if !opened.is_empty() {
                // (The second of a name is told apart in the table.)
                let nth = names.entry(c.label.clone()).or_default();
                *nth += 1;
                let name = match *nth {
                    1 => format!("{surface} > {}", c.label),
                    n => format!("{surface} > {} ({n})", c.label),
                };
                let kind = if clicked { "drop-down" } else { "sub-menu" };
                rows.push(self.audit(&name, kind, Some(&opened)));
                if !clicked {
                    self.children(&name, &opened, rows, depth + 1);
                }
            }
            if clicked {
                self.key(egui::Key::Escape);
                // (A colour picker closes on a click beside it.)
                if !self.layers().difference(&before).collect::<Vec<_>>().is_empty() {
                    let beside = self.screen().shrink(2.0).left_bottom();
                    self.click(beside);
                }
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

/// Cases of the walk that give nothing to measure: (the case, the layout it holds for, how it
/// comes to nothing). A case that gives nothing and is not here, or gives nothing for another
/// reason than the one named, fails the walk (a menu, scene or dialog that stopped opening),
/// and so does an entry here that opens something. The layout is "" for every size, "narrow"
/// for a window under 780 points wide (the panes folded) or "docked" for a wider one.
const OPENS_NOTHING: &[(&str, &str, &str)] = &[
    // Lists of the ribbon that need another selection than the demo's one shape.
    ("Ribbon Home > Fit Text (list)", "", DISABLED),
    ("Ribbon Home > Modify Overlap (list)", "", DISABLED),
    ("Ribbon Image > Save Image (list)", "", DISABLED),
    ("Ribbon Image > Background Image Density (list)", "", DISABLED),
    ("Ribbon Text > Fit Text (list)", "", DISABLED),
    ("Ribbon Shapes > Edit Point (list)", "", DISABLED),
    // Contextual tabs: Stitches needs a stitch pattern selected, Hoop Settings a design
    // larger than its hoop.
    ("Main window, ribbon tab Stitches", "", NOT_SHOWN),
    ("Main window, ribbon tab Hoop Settings", "", NOT_SHOWN),
    // The Select tool has no right-click menu (the Point tool has two).
    ("Right click on a shape, Select tool", "", NO_CONTROL),
    // The window without its bars: with the panes docked nothing is left but the canvas
    // (in the narrow layout the folded panes' title bars are).
    ("Scene bare", "docked", NO_CONTROL),
    // Commands that need another selection, an image, a clipboard or a stitch pattern.
    ("Command Paste", "", DISABLED),
    ("Command Group", "", DISABLED),
    ("Command Ungroup", "", DISABLED),
    ("Command Join Lines", "", DISABLED),
    ("Command Break Apart", "", DISABLED),
    ("Command Modify Image", "", DISABLED),
    ("Command Rotate Right", "", DISABLED),
    ("Command Rotate Left", "", DISABLED),
    ("Command Delete Image", "", DISABLED),
    ("Command Convert to Blocks", "", DISABLED),
    ("Command Convert to Outline", "", DISABLED),
    ("Command Delete Direction Line", "", DISABLED),
    ("Command Satin Skeleton from Shape", "", DISABLED),
    ("Command Delete Point", "", DISABLED),
    ("Command Split at Point", "", DISABLED),
    ("Command Satin Columns per Branch", "", DISABLED),
    ("Command Split Stitch Data", "", DISABLED),
    ("Command Divide by Color", "", DISABLED),
    ("Command Connect", "", DISABLED),
    ("Command Smooth Stitches", "", DISABLED),
    ("Command Convert to Outlines", "", DISABLED),
    ("Command Move Sections", "", DISABLED),
];

/// The commands that open a menu, pane or dialog of their own (their keys). Every other
/// command acts on the design or the view and opens nothing: one that starts to open
/// something, or one of these that stops, fails the walk.
const COMMANDS_THAT_OPEN: &[&str] = &[
    "applique", "arrangeCopy", "attributeSets", "autoPunch", "backgroundFill", "colorPalette",
    "crossStitch", "cutwork", "designCenter", "designDatabase", "designProperty",
    "designSettings", "fabricSelector", "floral", "fontCreator", "fontFilterSettings", "new",
    "offsetLines", "openSample", "paneAssistant", "patch", "photoStitch1", "photoStitch2",
    "realisticSettings", "reference", "stitchCreator", "stitchFactory", "threadCharts",
];

/// The surfaces that keep a button row in view below their scrolled content at the smallest
/// window of the survey (640 × 400 at Text size 200 %, where every dialog with a button row
/// is taller than the window). The walk checks each row's place before and after scrolling.
const PINNED_AT_320X200: &[&str] = &[
    "Scene update",
    "Scene fabric",
    "Scene text",
    "Scene matchThreads",
    "Scene usb",
    "Scene monogram",
    "Scene arrange",
    "Scene property",
    "Scene print",
    "Scene machine-setup",
    "Scene startup-wizard",
    "Dialog Job Sheet",
    "Dialog Machine Setup",
    "Dialog Startup Wizard",
    "Dialog Image to Embroidery",
    "Dialog Font Filter Settings",
    "Dialog Design Database, Search",
    "Dialog Design Database, Catalogue",
    "Wizard background",
    "Wizard applique",
    "Wizard patch",
    "Wizard cutwork",
    "Command Arrange Copy",
    "Command Background fill",
    "Command Applique Wizard",
    "Command Embroidered Patch Wizard",
    "Command Cutwork Wizard",
    "Command Auto Punch Wizard",
    "Command Cross Stitch Wizard",
    "Command Photo Stitch 1 Wizard",
    "Command Photo Stitch 2 Wizard",
    "Command Import to Design Center",
    "Command Font Filter Settings",
    "Command Design Property",
    "Command Fabric Selector",
];

/// How a case of the walk comes to give nothing (the third column of `OPENS_NOTHING`).
const DISABLED: &str = "disabled for the demo design with its first shape selected";
const NOT_SHOWN: &str = "a contextual tab the demo's selection does not bring forward";
const NO_CONTROL: &str = "opens nothing with a control";
const OUT_OF_VIEW: &str = "the place to click is outside the window";

/// Actions the walk does not run, each with the reason: they leave the app's window.
const NOT_RUN: &[(Action, &str)] = &[
    (Action::Open, "opens the system's file dialog"),
    (Action::Save, "opens the system's file dialog for a design without a file"),
    (Action::SaveAs, "opens the system's file dialog"),
    (Action::Exit, "closes the window"),
    (Action::ImportPatterns, "opens the system's file dialog"),
    (Action::ImportSvg, "opens the system's file dialog"),
    (Action::ImportPesObjects, "opens the system's file dialog"),
    (Action::OpenImage, "opens the system's file dialog"),
    (Action::PasteImage, "reads the system's clipboard"),
    (Action::TrueTypeAttributes, "opens the system's file dialog"),
    (Action::ExportSections, "opens the system's folder dialog"),
];

/// The main window showing ribbon tab `tab`: the tool or the selection that brings a
/// contextual tab forward is set first.
fn tab_window(walk: &Walk, tab: TabId) -> Win {
    let tool = match tab {
        TabId::Outline => Some(Tool::Polygon),
        TabId::OutlineRect => Some(Tool::Rectangle),
        TabId::ManualPunch => Some(Tool::Punch),
        _ => None,
    };
    let mut w = walk.open(Launch { tool, ..demo() });
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

/// Closes the panes and the Assistant, as a user does to work on the canvas of a small
/// window: docked they leave it 160 points, and folded their title bars lie over its foot.
fn clear_canvas(w: &mut Win) {
    let panes = &mut w.app.ui.prefs.panes;
    panes.sewing_order.visible = false;
    panes.attributes.visible = false;
    panes.simulator.visible = false;
    w.app.assistant.panel.open = false;
    w.frames(6, Vec::new());
}

/// One walk: every surface at a window of `px` pixels and a Text size of `scale` %.
struct Walk {
    px: (f32, f32),
    scale: u32,
    /// `(n, of)`: only every `of`-th case, starting at the `n`-th. The parts together are the
    /// whole walk, so they can run side by side.
    part: Option<(usize, usize)>,
    /// False to open and count only (no wheel is turned).
    measure: bool,
    /// By hand: audit only the surfaces whose name has this text (the expectations are not
    /// checked then).
    only: Option<String>,
    count: usize,
    rows: Vec<Row>,
    /// What does not match the named expectations (`OPENS_NOTHING`, `COMMANDS_THAT_OPEN`,
    /// `PINNED_AT_320X200`) or a pinned button row.
    problems: Vec<String>,
}

impl Walk {
    fn new(px: (f32, f32), scale: u32, part: Option<(usize, usize)>, measure: bool) -> Walk {
        Walk {
            px,
            scale,
            part,
            measure,
            only: None,
            count: 0,
            rows: Vec::new(),
            problems: Vec::new(),
        }
    }

    /// Whether the next case belongs to this part.
    fn mine(&mut self) -> bool {
        let n = self.count;
        self.count += 1;
        self.part.is_none_or(|(at, of)| n % of == at)
    }

    fn open(&self, launch: Launch) -> Win {
        let mut w = Win::new(launch, self.px, self.scale);
        w.measure = self.measure;
        w.only = self.only.clone();
        w
    }

    /// The layout the window's width gives: the panes folded or docked.
    fn layout(&self) -> &'static str {
        let points = self.px.0 * 100.0 / self.scale as f32;
        if openstitch_app::editor::workspace::narrow(points) {
            "narrow"
        } else {
            "docked"
        }
    }

    /// The rows one case of the walk gave; `why` says how it came to nothing when it gave
    /// none with a control. Such a case has to be on `OPENS_NOTHING` for this layout with
    /// that reason, and one on that list must give none: so a surface that stops opening
    /// fails the walk, and so does a stale entry.
    fn case(&mut self, name: &str, kind: &'static str, added: Vec<Row>, why: &str) {
        let measured = added.iter().any(|r| r.controls > 0);
        let layout = self.layout();
        let listed = OPENS_NOTHING
            .iter()
            .find(|e| e.0 == name && (e.1.is_empty() || e.1 == layout));
        if self.only.is_none() {
            match (measured, listed) {
                (false, Some(e)) if e.2 == why => self.rows.push(Row {
                    surface: name.to_string(),
                    kind,
                    skipped: Some(e.2),
                    ..Row::default()
                }),
                (false, Some(e)) => self
                    .problems
                    .push(format!("NOTHING\t{name}\t{layout}\t{why}\t(listed as: {})", e.2)),
                (false, None) => self.problems.push(format!("NOTHING\t{name}\t{layout}\t{why}")),
                (true, Some(_)) => self.problems.push(format!("LISTED-BUT-OPENS\t{name}\t{layout}")),
                (true, None) => {}
            }
            self.pinned_as_listed(&added);
        }
        for r in &added {
            for p in &r.problems {
                self.problems.push(format!("ROW\t{}: {p}", r.surface));
            }
        }
        self.rows.extend(added.into_iter().filter(|r| r.controls > 0));
    }

    /// At the smallest window every dialog with a button row is taller than the window: which
    /// ones keep the row in view is a named list too.
    fn pinned_as_listed(&mut self, added: &[Row]) {
        if !(self.px == (640.0, 400.0) && self.scale == 200) {
            return;
        }
        for r in added.iter().filter(|r| r.controls > 0) {
            let expected = PINNED_AT_320X200.contains(&r.surface.as_str());
            if r.pinned != expected {
                self.problems.push(format!(
                    "{}\t{}",
                    if r.pinned { "PINNED" } else { "NOT-PINNED" },
                    r.surface
                ));
            }
        }
    }

    /// A case the walk leaves out for a reason it knows by itself (the registry says the
    /// command is not available yet; the action leaves the window).
    fn left_out(&mut self, name: &str, kind: &'static str, reason: &'static str) {
        self.rows.push(Row {
            surface: name.to_string(),
            kind,
            skipped: Some(reason),
            ..Row::default()
        });
    }

    fn run(mut self) -> Walk {
        // The main window with each ribbon tab: ribbon, panes, status bar.
        let base = self.open(demo()).layers();
        for tab in TabId::ALL {
            if !self.mine() {
                continue;
            }
            let mut w = tab_window(&self, tab);
            let name = format!("Main window, ribbon tab {}", tab.english_label());
            if w.app.ui.tab != tab {
                self.case(&name, "main", Vec::new(), NOT_SHOWN);
                continue;
            }
            let row = w.audit(&name, "main", None);
            self.case(&name, "main", vec![row], NO_CONTROL);
            // The tab's drop-down lists.
            for g in tab.groups() {
                for a in g.actions() {
                    let info = a.info();
                    if !a.is_menu() {
                        continue;
                    }
                    let menu = format!("Ribbon {} > {} (list)", tab.english_label(), squash(info.label));
                    if !info.available {
                        self.left_out(&menu, "list", "not available yet in OpenStitch (the registry)");
                        continue;
                    }
                    if !crate::ribbon::enabled(&w.app, a) {
                        self.case(&menu, "list", Vec::new(), DISABLED);
                        continue;
                    }
                    let mut w = tab_window(&self, tab);
                    let before = w.layers();
                    if !w.click_action(a) {
                        self.problems.push(format!("BUTTON\t{menu}: its button cannot be reached"));
                        continue;
                    }
                    let mut added = Vec::new();
                    w.audit_new(&menu, "list", &before, &mut added);
                    self.case(&menu, "list", added, NO_CONTROL);
                }
            }
        }
        // The menus of the top bar.
        for label in ["Options", "Help"] {
            if !self.mine() {
                continue;
            }
            let mut w = self.open(demo());
            let before = w.layers();
            let name = format!("Menu {label}");
            if !w.click_label(label) {
                self.problems.push(format!("BUTTON\t{name}: its button cannot be reached"));
                continue;
            }
            let mut added = Vec::new();
            w.audit_new(&name, "menu", &before, &mut added);
            self.case(&name, "menu", added, NO_CONTROL);
        }
        // The --show scenes: the application menu, dialogs, panes, the other windows.
        for show in SHOW {
            if !self.mine() {
                continue;
            }
            let mut w = self.open(Launch {
                show: vec![show.to_string()],
                ..demo()
            });
            w.frames(4, Vec::new());
            // A scene that opens something over the main window is measured as that; one that
            // changes the window itself (another suite window, a view, a tool, the Assistant's
            // panel) as the whole window.
            let name = format!("Scene {show}");
            let mut added = Vec::new();
            w.audit_new(&name, "scene", &base, &mut added);
            if added.is_empty() {
                added.push(w.audit(&name, "scene", None));
            }
            self.case(&name, "scene", added, NO_CONTROL);
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
            if !self.mine() {
                continue;
            }
            let mut w = self.open(demo());
            opener(&mut w.app);
            w.frames(5, Vec::new());
            let mut added = Vec::new();
            if name.ends_with("(whole window)") {
                added.push(w.audit(name, "suite", None));
                self.case(name, "suite", added, NO_CONTROL);
            } else {
                let name = format!("Dialog {name}");
                w.audit_new(&name, "dialog", &base, &mut added);
                self.case(&name, "dialog", added, NO_CONTROL);
            }
        }
        for kind in ["background", "applique", "patch", "cutwork"] {
            if !self.mine() {
                continue;
            }
            let name = format!("Wizard {kind}");
            let Some(k) = openstitch_app::editor::pattern_wizards::WizardKind::from_key(kind) else {
                self.problems.push(format!("NOTHING\t{name}: no wizard of that key"));
                continue;
            };
            let mut w = self.open(Launch {
                wizard: Some(k),
                ..demo()
            });
            let mut added = Vec::new();
            w.audit_new(&name, "wizard", &base, &mut added);
            self.case(&name, "wizard", added, NO_CONTROL);
        }
        for pane in ["import", "color", "attributes", "text"] {
            if !self.mine() {
                continue;
            }
            let mut w = self.open(Launch {
                pane: crate::ribbon::PaneTab::from_key(pane),
                ..demo()
            });
            let name = format!("Main window, pane tab {pane}");
            let row = w.audit(&name, "pane", None);
            self.case(&name, "pane", vec![row], NO_CONTROL);
        }
        // Right clicks. The Point tool has two menus: on a point of the shape whose points
        // show, and on that shape beside its points. The Select tool has none.
        for (name, tool, on_point) in [
            ("Right click on a shape, Select tool", Tool::Select, false),
            ("Right click on a shape, Point tool", Tool::Point, false),
            ("Right click on a point, Point tool", Tool::Point, true),
        ] {
            if !self.mine() {
                continue;
            }
            let mut w = self.open(Launch {
                tool: Some(tool),
                ..demo()
            });
            // The first point of the selected shape, where the editor says it is on screen;
            // and a place on the shape (the demo's badge) clear of its points and of the star
            // that lies on it.
            clear_canvas(&mut w);
            if tool == Tool::Point {
                // (Selects the first point; the command is the Point tool's own.)
                let _ = w.app.ui_command("ui.pointMenu", Some(&json!({ "points": [0] })));
                w.frames(3, Vec::new());
            }
            let at = w.in_view(|w| {
                let at = if on_point {
                    let s = w.app.ui_command("ui.pointMenu", None).ok()?;
                    let p = &s["selected"][0]["screen"];
                    egui::pos2(p[0].as_f64()? as f32, p[1].as_f64()? as f32)
                } else {
                    w.at_mm(74.0, 48.0)?
                };
                // (On the canvas: a place beside it belongs to a pane.)
                w.app.workspace.canvas.filter(|c| c.contains(at)).map(|_| at)
            });
            let before = w.layers();
            let mut added = Vec::new();
            let mut why = NO_CONTROL;
            match at {
                Some(at) => {
                    w.right_click(at);
                    w.audit_new(name, "menu", &before, &mut added);
                }
                None => why = OUT_OF_VIEW,
            }
            self.case(name, "menu", added, why);
        }
        for (name, dx, dy) in [
            ("Right click on the top ruler", 80.0, 6.0),
            ("Right click on the left ruler", 6.0, 24.0),
        ] {
            if !self.mine() {
                continue;
            }
            let mut w = self.open(demo());
            clear_canvas(&mut w);
            let before = w.layers();
            let mut added = Vec::new();
            let mut why = NO_CONTROL;
            let at = w.in_view(|w| w.app.workspace.canvas.map(|c| c.min + egui::vec2(dx, dy)));
            match at {
                Some(at) => {
                    w.right_click(at);
                    w.audit_new(name, "menu", &before, &mut added);
                }
                None => why = OUT_OF_VIEW,
            }
            self.case(name, "menu", added, why);
        }
        // The other windows of the suite: their File menu and, in the Design Database, the
        // menu bar.
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
            if !self.mine() {
                continue;
            }
            let mut plain = self.open(demo());
            opener(&mut plain.app);
            plain.app.creator.open_app_menu = false;
            plain.app.sewing.open_app_menu = false;
            plain.frames(5, Vec::new());
            let before = plain.layers();
            let mut w = self.open(demo());
            opener(&mut w.app);
            w.frames(5, Vec::new());
            if name != "Design Database" {
                let file = format!("{name}, File menu");
                let mut added = Vec::new();
                w.audit_new(&file, "menu", &before, &mut added);
                self.case(&file, "menu", added, NO_CONTROL);
            }
            // (The Design Database has a menu bar where the other two have a File button.)
            if name == "Design Database" {
                let bar: Vec<Control> = w
                    .controls()
                    .into_iter()
                    .filter(|c| c.role == Role::Button && c.rect.max.y < 30.0)
                    .collect();
                if bar.len() < 4 {
                    self.problems.push(format!("NOTHING\t{name}: its menu bar has {} buttons", bar.len()));
                }
                for c in bar {
                    let mut w = self.open(demo());
                    opener(&mut w.app);
                    w.frames(5, Vec::new());
                    let before = w.layers();
                    let menu = format!("{name}, menu {}", c.label);
                    if !w.click_label(&c.label) {
                        self.problems.push(format!("BUTTON\t{menu}: its button cannot be reached"));
                        continue;
                    }
                    let mut added = Vec::new();
                    w.audit_new(&menu, "menu", &before, &mut added);
                    self.case(&menu, "menu", added, NO_CONTROL);
                }
            }
        }
        // What each command opens. (A command that is a list is walked as a list of its tab
        // above, or as a sub-menu of the application or Options menu.)
        let actions: Vec<Action> = openstitch_app::editor::ribbon::ribbon_actions()
            .into_iter()
            .chain(APPLICATION_MENU)
            .chain(OPTION_MENU)
            .collect();
        let mut done = HashSet::new();
        for a in actions {
            if !self.mine() {
                continue;
            }
            let info = a.info();
            if a.is_menu() || !done.insert(info.key) {
                continue;
            }
            let name = format!("Command {}", squash(info.label));
            if !info.available {
                self.left_out(&name, "command", "not available yet in OpenStitch (the registry)");
                continue;
            }
            if let Some((_, why)) = NOT_RUN.iter().find(|n| n.0 == a) {
                self.left_out(&name, "command", why);
                continue;
            }
            let mut w = self.open(demo());
            let opens = COMMANDS_THAT_OPEN.contains(&info.key);
            if !crate::ribbon::enabled(&w.app, a) {
                self.case(&name, "command", Vec::new(), DISABLED);
                continue;
            }
            let before = w.layers();
            let ctx = w.ctx.clone();
            crate::ribbon::run(&mut w.app, a, &ctx);
            w.frames(5, Vec::new());
            let mut added = Vec::new();
            if w.app.database.open || w.app.creator.open || w.app.sewing.open {
                let name = format!("{name} (whole window)");
                added.push(w.audit(&name, "suite", None));
            } else {
                w.audit_new(&name, "command", &before, &mut added);
            }
            // Most commands act on the design or the view and open nothing: the ones that
            // open a menu, pane or dialog are the named list.
            let opened = added.iter().any(|r| r.controls > 0);
            if self.only.is_none() && opened != opens {
                self.problems.push(format!(
                    "{}\t{}",
                    if opened { "OPENS" } else { "DOES-NOT-OPEN" },
                    info.key
                ));
            }
            for r in &added {
                for p in &r.problems {
                    self.problems.push(format!("ROW\t{}: {p}", r.surface));
                }
            }
            if self.only.is_none() {
                self.pinned_as_listed(&added);
            }
            self.rows.extend(added.into_iter().filter(|r| r.controls > 0));
        }
        self
    }
}

fn write(px: (f32, f32), scale: u32) {
    let mut walk = Walk::new(px, scale, None, true);
    walk.only = std::env::var("REACH_ONLY").ok();
    let walk = walk.run();
    eprintln!("done {px:?} {scale}");
    for p in &walk.problems {
        eprintln!("PROBLEM {p}");
    }
    let mut out = String::from("surface\tcontrols\tnot in view\tunreachable\tbeyond x\tbeyond y\twhich\n");
    for r in walk.rows.iter().filter(|r| r.skipped.is_none()) {
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
        let row = w.audit("Options", "menu", Some(&menu));
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
    let row = w.audit("Design Property", "dialog", Some(&dialog));
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
    // Every entry that can take the focus was highlighted on the way (a greyed-out one
    // cannot).
    let missed: Vec<String> = entries
        .iter()
        .filter(|c| !c.disabled && !walked.contains(&c.id))
        .map(|c| format!("{:?} '{}'", c.role, c.label))
        .collect();
    assert!(missed.is_empty(), "entries the keys did not reach: {missed:?}");
    assert!(walked.contains(&last.id), "the keys reach the last entry");
    assert!(entries.len() >= 30 && walked.len() >= 30, "walked {} of {} entries", walked.len(), entries.len());
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
    ("app.rs", "Window::new(", 1, "dialog_window builds the window its callers show through fit"),
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
    const WINDOWS: &[&str] = &[
        "crate::fit::show(",
        "crate::fit::show_filling(",
        "crate::fit::show_filling_at_least(",
        "crate::fit::show_pane(",
    ];
    // (the toolkit's call, the `fit` calls that may wrap it)
    let rules: &[(&str, &[&str])] = &[
        // (With or without `egui::` before it: a `use` does not hide a window from this.)
        ("Window::new(", WINDOWS),
        (".dialog_window(", WINDOWS),
        ("Modal::new(", &["crate::fit::modal("]),
        // A combo box's list and a colour button's picker are pop-ups too.
        ("ComboBox::", &["crate::fit::combo("]),
        ("color_edit_button", &[]),
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
    // (Two slashes, written so that the icon check does not read a label made of a mark.)
    let comment = "/".repeat(2);
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
                if before.ends_with('"') || line.trim_start().starts_with(&comment) {
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
         fit::sub_menu, fit::context_menu, fit::combo or fit::color_button, so it fits a small \
         window, #1276):\n{}",
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

/// The walk of the registries: every ribbon tab and its lists, the top bar's menus, every
/// `--show` scene, every dialog, wizard and pane tab, the right-click menus, the other windows
/// of the suite, and whatever each ribbon, menu and Quick Access Toolbar command opens, with
/// the sub-menus, combo boxes and colour buttons of each. It fails for a control that cannot
/// be brought into view with the wheel, for a menu or dialog that reaches beyond the window,
/// for a pinned button row out of place, and for a case that does not match the named lists
/// (`OPENS_NOTHING`, `COMMANDS_THAT_OPEN`, `PINNED_AT_320X200`). A new command, scene or tab
/// is walked without a change here (they come from the registries).
fn walk(px: (f32, f32), scale: u32, part: usize) {
    let walk = Walk::new(px, scale, Some((part, WALK_PARTS)), true).run();
    let measured = walk.rows.iter().filter(|r| r.controls > 0).count();
    assert!(measured >= 20, "part {part} measured {measured} surfaces");
    let mut wrong: Vec<String> = walk
        .rows
        .iter()
        .filter(|r| !r.lost.is_empty() || r.beyond.0 > 1.0 || r.beyond.1 > 1.0)
        .map(|r| format!("{}: out of reach {:?}, beyond the window by {:?}", r.surface, r.lost, r.beyond))
        .collect();
    wrong.extend(walk.problems.iter().cloned());
    assert!(
        wrong.is_empty(),
        "at {} x {}, Text size {scale} % (#1276):\n{}",
        px.0,
        px.1,
        wrong.join("\n")
    );
}

/// Each size's walk runs as this many tests, side by side.
const WALK_PARTS: usize = 6;

/// The sizes walked with every test run, chosen for what they reach: 640 × 400 is the narrow
/// layout (panes folded, the smallest browser window); 900 × 600 the desktop's smallest
/// window, with the panes docked, the Assistant sharing the width with them, panes that
/// scroll sideways and the Simulator giving way; 640 × 400 at Text size 200 % (320 × 200
/// points) the window that scrolls as a page, the dialogs' pinned rows and the menus that
/// cannot open beside their button. The other seven sizes are the survey, by hand.
macro_rules! walks {
    ($($name:ident: $w:expr, $h:expr, $scale:expr, $part:expr;)*) => {
        $(
            #[test]
            fn $name() {
                walk(($w, $h), $scale, $part);
            }
        )*
    };
}

walks! {
    every_control_can_be_reached_at_640x400_part_1: 640.0, 400.0, 100, 0;
    every_control_can_be_reached_at_640x400_part_2: 640.0, 400.0, 100, 1;
    every_control_can_be_reached_at_640x400_part_3: 640.0, 400.0, 100, 2;
    every_control_can_be_reached_at_640x400_part_4: 640.0, 400.0, 100, 3;
    every_control_can_be_reached_at_640x400_part_5: 640.0, 400.0, 100, 4;
    every_control_can_be_reached_at_640x400_part_6: 640.0, 400.0, 100, 5;
    every_control_can_be_reached_at_900x600_part_1: 900.0, 600.0, 100, 0;
    every_control_can_be_reached_at_900x600_part_2: 900.0, 600.0, 100, 1;
    every_control_can_be_reached_at_900x600_part_3: 900.0, 600.0, 100, 2;
    every_control_can_be_reached_at_900x600_part_4: 900.0, 600.0, 100, 3;
    every_control_can_be_reached_at_900x600_part_5: 900.0, 600.0, 100, 4;
    every_control_can_be_reached_at_900x600_part_6: 900.0, 600.0, 100, 5;
    every_control_can_be_reached_at_640x400_text_200_part_1: 640.0, 400.0, 200, 0;
    every_control_can_be_reached_at_640x400_text_200_part_2: 640.0, 400.0, 200, 1;
    every_control_can_be_reached_at_640x400_text_200_part_3: 640.0, 400.0, 200, 2;
    every_control_can_be_reached_at_640x400_text_200_part_4: 640.0, 400.0, 200, 3;
    every_control_can_be_reached_at_640x400_text_200_part_5: 640.0, 400.0, 200, 4;
    every_control_can_be_reached_at_640x400_text_200_part_6: 640.0, 400.0, 200, 5;
}

/// How many surfaces of each kind the walk measures (ones with at least one control), as
/// counted today at 640 × 400: the walk may find more, never fewer. With the named lists this
/// is what keeps it from passing on nothing.
const MEASURED_AT_LEAST: &[(&str, usize)] = &[
    ("main", 8),
    ("pane", 4),
    ("list", 49),
    ("menu", 13),
    ("sub-menu", 16),
    ("drop-down", 43),
    ("scene", 41),
    ("dialog", 29),
    ("wizard", 4),
    ("suite", 4),
    ("command", 25),
];

/// The whole walk once without turning the wheel: everything opens as the named lists say,
/// and no fewer surfaces of each kind are measured than today.
#[test]
fn the_walk_opens_and_measures_every_surface() {
    assert!(
        std::env::var_os("REACH_ONLY").is_none() || std::env::var_os("CI").is_none(),
        "REACH_ONLY is for the survey by hand, not for a test run"
    );
    let walk = Walk::new((640.0, 400.0), 100, None, false).run();
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for r in walk.rows.iter().filter(|r| r.controls > 0) {
        *counts.entry(r.kind).or_default() += 1;
    }
    let skipped = walk.rows.iter().filter(|r| r.skipped.is_some()).count();
    eprintln!("measured {counts:?}, left out with a reason {skipped}");
    for p in &walk.problems {
        eprintln!("PROBLEM {p}");
    }
    assert!(walk.problems.is_empty(), "the walk and its named lists differ:\n{}", walk.problems.join("\n"));
    assert!(!MEASURED_AT_LEAST.is_empty());
    for (kind, least) in MEASURED_AT_LEAST {
        let n = counts.get(kind).copied().unwrap_or(0);
        assert!(n >= *least, "the walk measured {n} surfaces of kind {kind}, {least} before: {counts:?}");
    }
    for kind in counts.keys() {
        assert!(MEASURED_AT_LEAST.iter().any(|m| m.0 == *kind), "a kind without a floor: {kind}");
    }
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
        rows.push(w.audit(&format!("Scene {scene} (whole window)"), "scene", None));
    } else {
        w.audit_new(&format!("Scene {scene}"), "scene", &base, &mut rows);
        if rows.is_empty() {
            rows.push(w.audit(&format!("Scene {scene} (whole window)"), "scene", None));
        }
    }
    for r in rows {
        eprintln!("{r:?}");
    }
}

/// The surfaces `ui.state` reports are the last frame's in the other windows of the suite
/// too (their frame used to end before the surfaces were kept).
#[test]
fn ui_state_reports_the_open_menu_in_a_suite_window() {
    let mut w = Win::new(demo(), (900.0, 600.0), 100);
    w.app.database.show(None);
    w.frames(6, Vec::new());
    let open = |w: &mut Win| {
        w.app.ui_command("ui.state", None).expect("ui.state")["surfaces"]["open"]
            .as_array()
            .expect("open")
            .len()
    };
    assert_eq!(open(&mut w), 0, "no menu is open in the Design Database yet");
    let bar: Vec<Control> = w
        .controls()
        .into_iter()
        .filter(|c| c.role == Role::Button && c.rect.max.y < 30.0)
        .collect();
    let first = bar.first().expect("the Design Database's menu bar");
    w.click(first.rect.center());
    w.frames(6, Vec::new());
    assert_eq!(open(&mut w), 1, "the menu {:?} is reported", first.label);
    // And gone from the report once it is closed.
    w.click(w.screen().shrink(4.0).right_bottom());
    w.frames(6, Vec::new());
    assert_eq!(open(&mut w), 0, "a closed menu is not reported");
}

/// The wheel turned with the pointer resting on one control, until nothing moves any more:
/// the offsets of the menus and dialogs on screen at the end.
fn wheel_to_the_end(w: &mut Win, at: egui::Pos2) -> Vec<crate::fit::Surface> {
    let mut last: Vec<i32> = Vec::new();
    for _ in 0..200 {
        wheel_down(w, at);
        let now: Vec<i32> = w.app.drawn.list.iter().map(|s| s.offset.round() as i32).collect();
        let rects: Vec<i32> = w.controls().iter().map(|c| c.rect.min.y.round() as i32).collect();
        let now: Vec<i32> = now.into_iter().chain(rects).collect();
        if now == last {
            break;
        }
        last = now;
    }
    w.app.drawn.list.clone()
}

/// A list inside a dialog or a menu that scrolls as well (the shortcuts of Keyboard
/// Shortcuts, the fonts of Font Filter Settings, the Font menu's fonts): with the pointer
/// resting on the inner list the wheel scrolls it to its end and then goes on to what it is
/// in, so the controls below the list are reached without moving the pointer.
#[test]
fn the_wheel_goes_on_from_a_list_inside_a_dialog_to_the_dialog() {
    type Opener = fn(&mut EditorApp);
    // (With a Text size at which the dialog scrolls although its list is short already.)
    let cases: [(&str, u32, Opener); 2] = [
        ("Keyboard Shortcuts", 150, |a| a.ui.shortcuts_open = true),
        ("Font Filter Settings", 200, |a| a.workspace.view.fonts_open = true),
    ];
    for (name, scale, opener) in cases {
        let px = (640.0, 400.0);
        let base = Win::new(demo(), px, scale).layers();
        let mut w = Win::new(demo(), px, scale);
        opener(&mut w.app);
        w.frames(8, Vec::new());
        let layers: HashSet<egui::LayerId> = w.layers().difference(&base).copied().collect();
        assert!(!layers.is_empty(), "{name} opens");
        let dialog = w.app.drawn.list.iter().find(|s| s.kind == "dialog").cloned();
        let dialog = dialog.unwrap_or_else(|| panic!("{name} is a dialog: {:?}", w.app.drawn.list));
        assert!(
            dialog.content > dialog.rect.height() + 5.0,
            "{name} is taller than its view at this size: {dialog:?}"
        );
        // The pointer rests in the middle of the dialog's view: on its inner list.
        let end = wheel_to_the_end(&mut w, dialog.rect.center());
        let d = end.iter().find(|s| s.kind == "dialog").expect("still open");
        assert!(
            d.offset >= d.content - d.rect.height() - 1.0,
            "{name}: the wheel over the inner list reached the dialog's end: {d:?}"
        );
        let cut: Vec<String> = w
            .controls()
            .iter()
            .filter(|c| layers.contains(&c.layer))
            .filter(|c| c.rect.max.y > d.rect.bottom() + 1.0 && !d.buttons.is_some_and(|b| b.contains(c.rect.center())))
            .map(|c| format!("{:?} '{}'", c.role, c.label))
            .collect();
        assert!(cut.is_empty(), "{name}: still below the view at the end: {cut:?}");
    }
}

/// The Font menu's list of fonts scrolls inside the menu: with the pointer on it the wheel
/// brings the last font into view.
#[test]
fn the_font_menu_scrolls_to_its_last_font() {
    let walk = Walk::new((640.0, 400.0), 100, None, true);
    let mut w = tab_window(&walk, TabId::Text);
    let before = w.layers();
    assert!(w.click_label("Font"), "the Font list's button");
    w.frames(6, Vec::new());
    let menu: HashSet<egui::LayerId> = w.layers().difference(&before).copied().collect();
    assert!(!menu.is_empty(), "the Font menu opens");
    let row = w.audit("Font menu", "list", Some(&menu));
    assert!(row.controls >= 4 && row.lost.is_empty(), "{row:?}");
    let rect = w
        .controls()
        .iter()
        .filter(|c| menu.contains(&c.layer))
        .fold(egui::Rect::NOTHING, |r, c| r.union(w.visible(c)));
    let end = wheel_to_the_end(&mut w, rect.center());
    let cut: Vec<String> = w
        .controls()
        .iter()
        .filter(|c| menu.contains(&c.layer) && c.rect.max.y > w.screen().bottom())
        .map(|c| c.label.clone())
        .collect();
    assert!(cut.is_empty(), "below the window after the wheel: {cut:?} ({end:?})");
}

/// A menu opens at its top every time, also after it was scrolled and closed, and the wheel a
/// sub-menu passed on belongs to the menu it was passed to: it is not found by the next menu.
#[test]
fn a_menu_opens_at_its_top_again() {
    let mut w = Win::new(demo(), (640.0, 400.0), 200);
    let menu = |w: &Win| {
        let list = &w.app.drawn.list;
        list.iter().max_by(|a, b| a.content.total_cmp(&b.content)).cloned()
    };
    assert!(w.click_label("Options"));
    w.frames(10, Vec::new());
    let m = menu(&w).expect("the Options menu");
    // Scrolled through its sub-menu entries (the wheel is passed on from them).
    for _ in 0..14 {
        wheel_down(&mut w, egui::pos2(m.rect.center().x, m.rect.top() + 12.0));
    }
    assert!(menu(&w).expect("open").offset > 100.0, "{:?}", menu(&w));
    // Closed with the wheel still turning over a sub-menu, then another menu and this one.
    w.click(egui::pos2(4.0, w.screen().bottom() - 4.0));
    w.frames(10, Vec::new());
    assert!(w.app.drawn.list.is_empty(), "the menu is closed: {:?}", w.app.drawn.list);
    for label in ["Help", "Options"] {
        assert!(w.click_label(label), "{label}");
        w.frames(10, Vec::new());
        let m = menu(&w).unwrap_or_else(|| panic!("the {label} menu"));
        assert_eq!(m.offset, 0.0, "the {label} menu opens at its top: {m:?}");
        w.click(egui::pos2(4.0, w.screen().bottom() - 4.0));
        w.frames(10, Vec::new());
    }
}

/// A layout at rest asks for no further frame: the sizes a dialog, a pane or a row measured
/// are compared within half a point, so rounding cannot keep the window repainting.
#[test]
fn a_settled_layout_stops_asking_for_frames() {
    type Setup = fn(&mut Win);
    let cases: [(&str, (f32, f32), u32, Setup); 4] = [
        ("the main window, panes docked", (900.0, 600.0), 100, |_| {}),
        ("the main window as a page", (640.0, 400.0), 200, |_| {}),
        ("a dialog that scrolls with a pinned row", (640.0, 400.0), 100, |w| {
            w.app.ui.property_open = true;
        }),
        ("an open menu that scrolls", (640.0, 400.0), 100, |w| {
            assert!(w.click_label("Options"));
        }),
    ];
    for (name, px, scale, setup) in cases {
        let mut w = Win::new(demo(), px, scale);
        // (The Assistant's panel blinks a caret and the simulator may animate: away.)
        w.app.assistant.panel.open = false;
        setup(&mut w);
        w.frames(40, Vec::new());
        let mut asked = 0;
        for _ in 0..20 {
            w.frames(1, Vec::new());
            asked += usize::from(w.repaint);
        }
        assert_eq!(asked, 0, "{name}: {asked} of 20 frames at rest asked for another at once");
    }
}

/// In a window too low to keep a button row in view and still show `crate::fit::PIN_LEAST` points of
/// the content, the row scrolls with the content instead of lying over it: it is reached by
/// the wheel, and the dialog stays inside the window.
#[test]
fn in_a_very_low_window_the_button_row_scrolls_with_the_content() {
    // 200 × 120 points.
    let px = (400.0, 240.0);
    let base = Win::new(demo(), px, 200).layers();
    let mut w = Win::new(
        Launch {
            show: vec!["property".into()],
            ..demo()
        },
        px,
        200,
    );
    w.frames(8, Vec::new());
    let dialog: HashSet<egui::LayerId> = w.layers().difference(&base).copied().collect();
    let d = w.app.drawn.list.iter().find(|s| s.kind == "dialog").cloned().expect("Design Property");
    assert!(d.buttons.is_none(), "no row is pinned in {:?} points: {d:?}", w.screen().size());
    assert!(d.rect.height() < crate::fit::PIN_LEAST + 40.0, "{d:?}");
    assert!(w.screen().expand(0.5).contains_rect(d.rect), "inside the window: {d:?}");
    let row = w.audit("Design Property", "dialog", Some(&dialog));
    assert!(row.lost.is_empty(), "Apply and Revert are reached by scrolling: {row:?}");
    assert_eq!(row.beyond, (0.0, 0.0));
    // With a little more room (640 × 400 at 200 %) the row is kept in view.
    let mut w = Win::new(
        Launch {
            show: vec!["property".into()],
            ..demo()
        },
        (640.0, 400.0),
        200,
    );
    w.frames(8, Vec::new());
    let d = w.app.drawn.list.iter().find(|s| s.kind == "dialog").cloned().expect("Design Property");
    let b = d.buttons.expect("pinned at 320 x 200 points");
    assert!(b.top() >= d.rect.bottom() - 0.5 && d.rect.height() >= crate::fit::PIN_LEAST - 0.5, "{d:?}");
}

/// A pane keeps the width it had when a smaller window squeezed it: the toolkit remembers the
/// width a panel was last drawn at, so without care a pane limited at 900 points would stay
/// narrow when the window is 1440 wide again.
#[test]
fn a_pane_squeezed_by_a_small_window_gets_its_width_back() {
    let mut w = Win::new(demo(), (1440.0, 900.0), 100);
    w.frames(10, Vec::new());
    let wide = w.app.workspace.canvas.expect("canvas");
    assert!(wide.min.x > 280.0, "the Sewing Order pane at its content's width: {wide:?}");
    w.size = egui::vec2(900.0, 600.0);
    w.frames(20, Vec::new());
    let small = w.app.workspace.canvas.expect("canvas");
    assert!(
        small.min.x < wide.min.x - 20.0,
        "with the Assistant open the pane gives way at 900 points: {small:?}"
    );
    assert!(small.width() >= 159.0, "the work area keeps 160 points: {small:?}");
    w.size = egui::vec2(1440.0, 900.0);
    w.frames(20, Vec::new());
    let back = w.app.workspace.canvas.expect("canvas");
    assert!(
        (back.min.x - wide.min.x).abs() < 1.0 && (back.max.x - wide.max.x).abs() < 1.0,
        "the panes are as wide as before: {wide:?}, then {back:?}"
    );
}

/// Every filled rectangle the last frame painted, with its clip.
fn filled(w: &Win) -> Vec<(egui::Rect, egui::Color32)> {
    fn collect(shape: &egui::Shape, clip: egui::Rect, out: &mut Vec<(egui::Rect, egui::Color32)>) {
        match shape {
            egui::Shape::Rect(r) => out.push((r.rect.intersect(clip), r.fill)),
            egui::Shape::Vec(v) => v.iter().for_each(|s| collect(s, clip, out)),
            _ => {}
        }
    }
    let mut out = Vec::new();
    for s in &w.shapes {
        collect(&s.shape, s.clip_rect, &mut out);
    }
    out
}

/// An open menu is opaque once it has faded in: its panel is painted in the menu's own fill
/// at full strength behind every row (a `--screenshot` taken six frames after the click had
/// caught the fade and showed the pane through the menu).
#[test]
fn an_open_menu_is_opaque_once_it_has_settled() {
    for (px, scale) in [((900.0, 600.0), 100), ((640.0, 400.0), 200)] {
        let mut w = Win::new(demo(), px, scale);
        assert!(w.click_label("Options"));
        w.frames(60, Vec::new());
        let m = w.app.drawn.list.iter().max_by(|a, b| a.content.total_cmp(&b.content)).cloned().expect("menu");
        let fill = egui::Frame::menu(&w.ctx.global_style()).fill;
        assert_eq!(fill.a(), 255, "the menu's fill is opaque");
        let panel = filled(&w)
            .into_iter()
            .find(|(r, c)| *c == fill && r.expand(1.0).contains_rect(m.rect));
        assert!(
            panel.is_some(),
            "at {px:?}, {scale} %: no rectangle of the menu's fill {fill:?} behind its rows {:?}",
            m.rect
        );
    }
}

/// By hand: what the walk opens at REACH_PROBE's size, without turning the wheel, as lines for
/// the named lists.
#[test]
#[ignore = "the survey: run by hand"]
fn discover() {
    let px: Vec<f32> = std::env::var("REACH_PROBE")
        .unwrap_or_else(|_| "640,400,100".into())
        .split(',')
        .map(|v| v.parse().expect("number"))
        .collect();
    let walk = Walk::new((px[0], px[1]), px[2] as u32, None, false).run();
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for r in walk.rows.iter().filter(|r| r.controls > 0) {
        *counts.entry(r.kind).or_default() += 1;
        if r.pinned {
            println!("IS-PINNED\t{}", r.surface);
        }
    }
    println!("COUNTS\t{counts:?}");
    for p in &walk.problems {
        println!("PROBLEM\t{p}");
    }
}

/// By hand: how wide each ribbon tab's groups are at Text size 100 % (the ribbon's scroll bar
/// shows in a window narrower than that).
#[test]
#[ignore = "the survey: run by hand"]
fn ribbon_widths() {
    let walk = Walk::new((2600.0, 900.0), 100, None, false);
    for tab in TabId::ALL {
        let w = tab_window(&walk, tab);
        if w.app.ui.tab != tab {
            continue;
        }
        let right = w
            .controls()
            .iter()
            .filter(|c| c.rect.min.y > 50.0 && c.rect.max.y < 140.0)
            .map(|c| c.rect.max.x)
            .fold(0.0f32, f32::max);
        println!("RIBBON {}: {right:.0}", tab.english_label());
    }
}
