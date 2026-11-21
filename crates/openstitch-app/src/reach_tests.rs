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
    /// The chosen entry of a list.
    selected: bool,
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
                        selected: n.is_selected().unwrap_or(false),
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
        // Every place [`Self::other_wheel_points`] gives (the page's own come last; a panel
        // that is a page of its own inside the page, the Assistant's, gives more than a dozen
        // before them).
        let places = 1 + self
            .controls()
            .iter()
            .find(|c| c.id == id)
            .map_or(0, |c| self.other_wheel_points(c).len());
        self.reach_in(id, ok, places.max(14))
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
            // Last, as a person scrolls a window that scrolls as a page: the pointer on the
            // top bar, and at the page's right edge (its scroll bar).
            for p in [
                egui::pos2(screen.center().x, screen.top() + 6.0),
                egui::pos2(screen.right() - 3.0, screen.center().y),
            ] {
                if out.iter().all(|q| q.distance(p) > 4.0) {
                    out.push(p);
                }
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
        let mut moved_to: Option<egui::Pos2> = None;
        let mut raised = false;
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
                eprintln!("    from {from} at {at:?}: rect {:?} seen {:?} ends {ends:?} top {:?} mine {:?} view {:?}", c.rect, c.seen, self.ctx.layer_id_at(self.visible(c).center()), c.layer, self.view_of(c));
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
            if covered {
                // Under a window that lies over it. A person moves the pointer onto a part of
                // the control that still shows, or onto another part of its own menu: that
                // closes a sub-menu which another entry under the resting pointer opened. The
                // page is not scrolled to get it out from under (a window lying over it, a
                // folded pane's title bar, stays where it is for the user too).
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
                    self.frames(1, vec![egui::Event::PointerMoved(p)]);
                    self.frames(30, Vec::new());
                    still = 0;
                    continue;
                }
                // A window under another window: a person clicks it (on an empty spot of it)
                // to bring it forward. Once.
                let cover = self.ctx.layer_id_at(v.center());
                if !raised
                    && c.layer.order == egui::Order::Middle
                    && cover.is_some_and(|t| t.order == egui::Order::Middle && t != c.layer)
                    && let Some(empty) = self.empty_spot(c.layer)
                {
                    raised = true;
                    self.click(empty);
                    still = 0;
                    continue;
                }
                // Still covered: reached only if what covers it is its own sub-menu (in a
                // window too narrow to open it beside, a sub-menu opens over its own entry).
                return Some(self.covered_by_its_own_sub_menu(id));
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

    /// A spot of `layer`'s area, in the window, on top, where no control is: a click there does
    /// nothing but close a pop-up opened from that window.
    fn empty_spot(&self, layer: egui::LayerId) -> Option<egui::Pos2> {
        let area = self.ctx.memory(|m| m.area_rect(layer.id))?.intersect(self.screen().shrink(2.0));
        let controls = self.controls();
        (1..12)
            .flat_map(|i| (1..12).map(move |j| (i, j)))
            .map(|(i, j)| {
                egui::pos2(
                    area.left() + area.width() * i as f32 / 12.0,
                    area.top() + area.height() * j as f32 / 12.0,
                )
            })
            .find(|p| {
                self.ctx.layer_id_at(*p) == Some(layer)
                    && !controls.iter().any(|c| c.layer == layer && c.rect.expand(4.0).contains(*p))
            })
    }

    /// Whether what lies over `id` is its own sub-menu (in a window too narrow to open it
    /// beside, a sub-menu opens over its own entry) or its own list (a combo box's list that
    /// opened over it). The pointer rests on the entry to open a sub-menu first.
    fn covered_by_its_own_sub_menu(&mut self, id: egui::Id) -> bool {
        let find = |w: &Win| w.controls().into_iter().find(|n| n.id == id);
        let Some(c) = find(self) else { return false };
        let at = self.visible(&c).center();
        self.frames(1, vec![egui::Event::PointerMoved(at)]);
        self.frames(30, Vec::new());
        // (The toolkit names a sub-menu after its entry, and a combo box's list after the box.)
        let own = [egui::containers::menu::SubMenu::id_from_widget_id(id), id.with("popup")];
        self.ctx.layer_id_at(at).is_some_and(|t| own.contains(&t.id))
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

    /// How far the window of `layer` lies outside the app's window, across and down (0 when
    /// inside).
    fn beyond(&self, layer: egui::LayerId) -> Option<(f32, f32)> {
        let screen = self.screen();
        let r = self.ctx.memory(|m| m.area_rect(layer.id))?;
        Some((
            (r.max.x - screen.max.x).max(screen.min.x - r.min.x).max(0.0),
            (r.max.y - screen.max.y).max(screen.min.y - r.min.y).max(0.0),
        ))
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
        // How far each window of the surface lies outside the app's window. A window that
        // moves with the page (the folded panes' title bars in a window that scrolls as a
        // page) is measured again each time the walk has scrolled a control into view: it is
        // beyond the window only if it stays so in every place the walk scrolled to. A menu
        // or dialog does not move when its own content scrolls, so this changes nothing for
        // them.
        let seen_layers: HashSet<egui::LayerId> = all.iter().map(|c| c.layer).collect();
        let mut beyond_of: HashMap<egui::LayerId, (f32, f32)> =
            seen_layers.iter().filter_map(|l| Some((*l, self.beyond(*l)?))).collect();
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
            } else {
                // (Other controls are reached on the way, by the same scrolling.)
                for (l, was) in beyond_of.iter_mut() {
                    if let Some(now) = self.beyond(*l) {
                        *was = (was.0.min(now.0), was.1.min(now.1));
                    }
                }
            }
        }
        let beyond = beyond_of
            .values()
            .fold((0.0f32, 0.0f32), |a, b| (a.0.max(b.0), a.1.max(b.1)));
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
            if !(menu_item || clicked) || !done.insert(c.id) {
                continue;
            }
            let name_of = |names: &mut HashMap<String, usize>| {
                // (The second of a name is told apart in the table.)
                let nth = names.entry(c.label.clone()).or_default();
                *nth += 1;
                match *nth {
                    1 => format!("{surface} > {}", c.label),
                    n => format!("{surface} > {} ({n})", c.label),
                }
            };
            let kind = if clicked { "drop-down" } else { "sub-menu" };
            if c.disabled {
                // A greyed-out combo box or colour button opens nothing: said so in the
                // table. (A greyed-out menu entry is a command that cannot run now.)
                if clicked {
                    let name = name_of(&mut names);
                    rows.push(Row {
                        surface: name,
                        kind,
                        skipped: Some(DISABLED),
                        ..Row::default()
                    });
                }
                continue;
            }
            // Brought under the pointer with the audit's whole patience. One that cannot be
            // is the audit's finding already (it is lost); a combo box or colour button that
            // cannot is a problem of its own here.
            if !self.reach(c.id, &mut HashSet::new()) {
                if clicked {
                    if std::env::var("REACH_TRACE").is_ok() {
                        let now = self.controls().into_iter().find(|x| x.id == c.id);
                        eprintln!("CHILD-LOST {:?} '{}' now {:?} top {:?} open {:?}", c.role, c.label, now.as_ref().map(|n| (n.rect, n.seen)), now.as_ref().and_then(|n| self.ctx.layer_id_at(self.visible(n).center())), self.app.drawn.list.iter().map(|s| s.rect).collect::<Vec<_>>());
                    }
                    let name = name_of(&mut names);
                    rows.push(Row {
                        surface: name,
                        kind,
                        problems: vec![format!("its {:?} '{}' cannot be brought under the pointer", c.role, c.label)],
                        ..Row::default()
                    });
                }
                continue;
            }
            let Some(c) = self.controls().into_iter().find(|x| x.id == c.id) else {
                continue;
            };
            let before = self.layers();
            let before_all = self.ctx.memory(|m| m.areas().visible_layer_ids());
            let at = self.visible(&c).center();
            if clicked {
                self.click(at);
            } else {
                self.frames(1, vec![egui::Event::PointerMoved(at)]);
                self.frames(6, Vec::new());
            }
            let opened: HashSet<egui::LayerId> =
                self.layers().difference(&before).copied().collect();
            // Every window or pop-up that appeared, named controls or not (a colour picker's
            // sliders and some lists' entries have no name a screen reader reads).
            let appeared: Vec<egui::LayerId> = self
                .ctx
                .memory(|m| m.areas().visible_layer_ids())
                .difference(&before_all)
                .copied()
                .collect();
            if !opened.is_empty() {
                let name = name_of(&mut names);
                rows.push(self.audit(&name, kind, Some(&opened)));
                if !clicked {
                    self.children(&name, &opened, rows, depth + 1);
                }
            } else if clicked && !appeared.is_empty() {
                // A list or picker without a named control: there is nothing to reach in it
                // one by one, but it has to open inside the window.
                let name = name_of(&mut names);
                let screen = self.screen().expand(0.5);
                let out: Vec<String> = appeared
                    .iter()
                    .filter_map(|l| self.ctx.memory(|m| m.area_rect(l.id)))
                    .filter(|r| !screen.contains_rect(*r))
                    .map(|r| format!("its list {r:?} is not inside the window"))
                    .collect();
                rows.push(Row {
                    surface: name,
                    kind,
                    skipped: Some(NO_NAMED_CONTROL),
                    problems: out,
                    ..Row::default()
                });
            } else if clicked {
                let name = name_of(&mut names);
                rows.push(Row {
                    surface: name,
                    kind,
                    problems: vec![format!("its {:?} '{}' opens nothing", c.role, c.label)],
                    ..Row::default()
                });
            }
            if clicked {
                // Closed as a person closes it: a click on the same control again; when its
                // list lies over it, a click on an empty spot of the window or menu it is in.
                // Escape only last: it also closes the menu it is in, and in the editor it
                // clears the selection (the Attributes pane then shows nothing).
                let still_open = |w: &Win| {
                    !w.ctx.memory(|m| m.areas().visible_layer_ids()).difference(&before_all).collect::<Vec<_>>().is_empty()
                };
                if let Some(now) = self.controls().into_iter().find(|x| x.id == c.id) {
                    // (Or, when the list covers all of a small menu, the entry already chosen:
                    // the list closes and nothing changes.)
                    // (The chosen entry: marked as chosen, or named as the box shows its value.)
                    let chosen = self.controls().into_iter().find(|x| {
                        opened.contains(&x.layer) && (x.selected || x.label == now.label) && self.shown(x)
                    });
                    if self.on_top(&now) {
                        self.click(self.visible(&now).center());
                    } else if let Some(empty) = self.empty_spot(now.layer) {
                        self.click(empty);
                    } else if let Some(chosen) = chosen {
                        self.click(self.visible(&chosen).center());
                    }
                }
                // (A list with a filter field of its own: the first Escape leaves the field.)
                for _ in 0..3 {
                    if !still_open(self) {
                        break;
                    }
                    self.key(egui::Key::Escape);
                }
                // The surface it is in is still open, or the rest of it cannot be walked.
                if !self.controls().iter().any(|x| of.contains(&x.layer)) {
                    rows.push(Row {
                        surface: format!("{surface} (closed by its {:?} '{}')", c.role, c.label),
                        kind,
                        problems: vec![format!("closing the list of its {:?} '{}' closed it", c.role, c.label)],
                        ..Row::default()
                    });
                    return;
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
    // The window without its bars: with the panes docked nothing is left but the canvas
    // (in the narrow layout the folded panes' title bars are).
    ("Scene bare (whole window)", "docked", NO_CONTROL),
    // In the narrow layout the panes are folded: the floating panes and the picture pickers
    // in them do not show.
    ("Scene floating", "narrow", NO_CONTROL),
    ("Scene pickers", "narrow", NO_CONTROL),
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
    "Scene templates",
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

/// The `--show` scenes that change the main window itself rather than open something over it,
/// with what they change: each is measured as the whole window. Every other scene has to
/// open a surface of its own, or it fails the walk.
const WHOLE_WINDOW_SCENES: &[(&str, &str)] = &[
    ("creator", "the Programmable Stitch Creator in place of the editor"),
    ("sdf", "the Stitch Design Factory in place of the editor"),
    ("bare", "the editor without its bars"),
    ("large-text", "Text size 150 %"),
    ("norulers", "the canvas without rulers"),
    ("tabs", "a second design as a tab: the tab strip"),
    ("realistic", "the Realistic view"),
    ("solid", "the Solid view"),
    ("measure", "the Measure tool"),
    ("keyboard", "a Satin Skeleton drawn from the keyboard"),
    ("basic", "the Attributes pane's basic properties"),
    ("assistant", "the Assistant's panel"),
    ("assistant-notices", "the Assistant's panel with its settings and notices"),
    ("assistant-questions", "the Assistant's panel with questions above the composer"),
    ("assistant-suggestions", "the Assistant's panel with suggested replies"),
];

/// How a case of the walk comes to give nothing (the third column of `OPENS_NOTHING`).
const DISABLED: &str = "disabled for the demo design with its first shape selected";
const NOT_SHOWN: &str = "a contextual tab the demo's selection does not bring forward";
const NO_CONTROL: &str = "opens nothing with a control";
const NO_NAMED_CONTROL: &str = "a list or picker whose entries have no name a screen reader reads: its place is checked";
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
        self.rows
            .extend(added.into_iter().filter(|r| r.controls > 0 || r.skipped.is_some()));
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
            // A scene measures the surface it opens over the main window; one that changes the
            // window itself is on WHOLE_WINDOW_SCENES and measured as the whole window. A
            // scene that is not on the list and opens nothing fails (through `case`).
            let mut added = Vec::new();
            let name = if WHOLE_WINDOW_SCENES.iter().any(|e| e.0 == show) {
                let name = format!("Scene {show} (whole window)");
                added.push(w.audit(&name, "scene", None));
                name
            } else {
                let name = format!("Scene {show}");
                w.audit_new(&name, "scene", &base, &mut added);
                name
            };
            self.case(&name, "scene", added, NO_CONTROL);
        }
        // Dialogs no scene or ribbon command opens directly.
        type Opener = fn(&mut EditorApp);
        let extras: [(&str, Opener); 32] = [
            // The object menu's Numerical Setting (#1288), for the demo's objects.
            ("Size (object menu)", |a| {
                a.select_all();
                crate::numeric_ui::open(a, crate::numeric_ui::Kind::Size);
            }),
            ("Rotate (object menu)", |a| {
                a.select_all();
                crate::numeric_ui::open(a, crate::numeric_ui::Kind::Rotate);
            }),
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
        // show, and on that shape beside its points. The Select tool has the object menu
        // (#1288).
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
        // The object menu's sub-menus (#1288), each opened beside the menu a right-click on the
        // shape opened.
        for (name, sub) in [
            ("Object menu, Paste", "paste"),
            ("Object menu, Flip", "flip"),
            ("Object menu, Numerical Setting", "numericalSetting"),
            ("Object menu, Cut and Sew", "cutAndSew"),
        ] {
            if !self.mine() {
                continue;
            }
            let mut w = self.open(Launch {
                tool: Some(Tool::Select),
                ..demo()
            });
            clear_canvas(&mut w);
            let at = w.in_view(|w| {
                let at = w.at_mm(74.0, 48.0)?;
                w.app.workspace.canvas.filter(|c| c.contains(at)).map(|_| at)
            });
            let before = w.layers();
            let mut added = Vec::new();
            let mut why = NO_CONTROL;
            match at {
                Some(at) => {
                    w.right_click(at);
                    crate::object_menu_ui::open_sub_for_test(&mut w.app, sub);
                    w.frames(3, Vec::new());
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
    // (The Options menu: the tallest content on screen; its width may have changed by the
    // scroll bar it shows once it scrolls.)
    let options = s["open"]
        .as_array()
        .expect("open")
        .iter()
        .max_by(|a, b| num(&a["contentHeight"]).total_cmp(&num(&b["contentHeight"])))
        .filter(|o| (num(&o["rect"][0]) - rect[0]).abs() < 1.0)
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
    ("panes.rs", "Window::new(", 1, "a folded pane's window, placed one of two ways, shown through fit::show_pane_within"),
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
        "crate::fit::show_pane_within(",
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
        ("Popup::menu(", &["crate::fit::popup(", "crate::fit::own_menu("]),
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
/// How many surfaces of each kind the walk measures at each size of the test run (ones with
/// at least one control), as counted on this code: the walk may find more, never fewer. With
/// the named lists this is what keeps it from passing on nothing.
/// A size of the test run (pixels and Text size) and its floors per kind of surface.
type Floors = ((f32, f32, u32), &'static [(&'static str, usize)]);
const MEASURED_AT_LEAST: &[Floors] = &[
    ((640.0, 400.0, 100), FLOORS_640X400),
    ((900.0, 600.0, 100), FLOORS_900X600),
    ((640.0, 400.0, 200), FLOORS_640X400_200),
];
const FLOORS_640X400: &[(&str, usize)] = &[
    ("main", 8),
    ("pane", 4),
    ("list", 49),
    ("menu", 13),
    ("sub-menu", 24),
    ("drop-down", 50),
    ("scene", 39),
    ("dialog", 29),
    ("wizard", 4),
    ("suite", 4),
    ("command", 25),
];
const FLOORS_900X600: &[(&str, usize)] = &[
    ("main", 8),
    ("pane", 4),
    ("list", 49),
    ("menu", 13),
    ("sub-menu", 28),
    ("drop-down", 63),
    ("scene", 40),
    ("dialog", 29),
    ("wizard", 4),
    ("suite", 4),
    ("command", 25),
];
const FLOORS_640X400_200: &[(&str, usize)] = &[
    ("main", 8),
    ("pane", 4),
    ("list", 49),
    ("menu", 13),
    ("sub-menu", 13),
    ("drop-down", 48),
    ("scene", 39),
    ("dialog", 29),
    ("wizard", 4),
    ("suite", 4),
    ("command", 25),
];

/// The whole walk once at one size without turning the wheel: everything opens as the named
/// lists say, and no fewer surfaces of each kind are measured than the floors.
fn opens_and_measures(px: (f32, f32), scale: u32) {
    assert!(
        std::env::var_os("REACH_ONLY").is_none() || std::env::var_os("CI").is_none(),
        "REACH_ONLY is for the survey by hand, not for a test run"
    );
    let walk = Walk::new(px, scale, None, false).run();
    let mut counts: std::collections::BTreeMap<&str, usize> = Default::default();
    for r in walk.rows.iter().filter(|r| r.controls > 0) {
        *counts.entry(r.kind).or_default() += 1;
    }
    let skipped = walk.rows.iter().filter(|r| r.skipped.is_some()).count();
    eprintln!("COUNTS {px:?} {scale}: {counts:?}, left out with a reason {skipped}");
    for p in &walk.problems {
        eprintln!("PROBLEM {p}");
    }
    assert!(
        walk.problems.is_empty(),
        "the walk and its named lists differ:\n{}",
        walk.problems.join("\n")
    );
    let floors = MEASURED_AT_LEAST
        .iter()
        .find(|m| m.0 == (px.0, px.1, scale))
        .map(|m| m.1)
        .expect("floors for this size");
    assert!(!floors.is_empty());
    for (kind, least) in floors {
        let n = counts.get(kind).copied().unwrap_or(0);
        assert!(n >= *least, "the walk measured {n} surfaces of kind {kind}, {least} before: {counts:?}");
    }
    for kind in counts.keys() {
        assert!(floors.iter().any(|m| m.0 == *kind), "a kind without a floor: {kind}");
    }
}

#[test]
fn the_walk_opens_and_measures_every_surface_at_640x400() {
    opens_and_measures((640.0, 400.0), 100);
}

#[test]
fn the_walk_opens_and_measures_every_surface_at_900x600() {
    opens_and_measures((900.0, 600.0), 100);
}

#[test]
fn the_walk_opens_and_measures_every_surface_at_640x400_text_200() {
    opens_and_measures((640.0, 400.0), 200);
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
            // (REACH_SHOT: as a `--screenshot` run lays its dialogs out, at the top right.)
            screenshot: std::env::var_os("REACH_SHOT").map(Into::into),
            ..demo()
        },
        (px[0], px[1]),
        px[2] as u32,
    );
    w.frames(6, Vec::new());
    // With REACH_SHOT, also where its windows lie and what `ui.state` reports of them, to
    // compare with the screenshot.
    if std::env::var_os("REACH_SHOT").is_some() {
        for l in w.layers().difference(&base) {
            eprintln!("layer {l:?} area {:?}", w.ctx.memory(|m| m.area_rect(l.id)));
        }
        eprintln!("surfaces {:?}", w.app.drawn.list);
    }
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
/// The surfaces `ui.state` reports are the last frame's in the other windows of the suite
/// too (their frame used to end before the surfaces were kept): the Design Database's menu bar,
/// and the File menus of the Programmable Stitch Creator and the Stitch Design Factory.
#[test]
fn ui_state_reports_the_open_menu_in_a_suite_window() {
    type Opener = fn(&mut EditorApp);
    let suites: [(&str, Opener); 3] = [
        ("Design Database", |a| a.database.show(None)),
        ("Programmable Stitch Creator", |a| a.creator.open = true),
        ("Stitch Design Factory", |a| a.sewing.open = true),
    ];
    for (name, opener) in suites {
        let mut w = Win::new(demo(), (900.0, 600.0), 100);
        opener(&mut w.app);
        w.frames(6, Vec::new());
        let open = |w: &mut Win| {
            w.app.ui_command("ui.state", None).expect("ui.state")["surfaces"]["open"]
                .as_array()
                .expect("open")
                .len()
        };
        assert_eq!(open(&mut w), 0, "{name}: no menu is open yet");
        // Its File menu (the Database's menu bar starts with File too).
        assert!(w.click_label("File"), "{name}: its File button");
        w.frames(6, Vec::new());
        assert_eq!(open(&mut w), 1, "{name}: its File menu is reported");
        // And gone from the report once it is closed.
        w.key(egui::Key::Escape);
        w.click(w.screen().shrink(4.0).right_bottom());
        w.frames(6, Vec::new());
        assert_eq!(open(&mut w), 0, "{name}: a closed menu is not reported");
    }
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

/// The Font menu's list of fonts scrolls: when it opens its first font is in view and its
/// last is not; with the pointer on the list the wheel brings the last font into view and
/// takes the first out, and nothing of the menu is left below the window.
#[test]
fn the_font_menu_scrolls_to_its_last_font() {
    // 320 × 80 points: the demo's fonts, about 100 points of list, are longer than that
    // window leaves for them (at 320 × 120 points they fit).
    let walk = Walk::new((640.0, 160.0), 200, None, true);
    let mut w = tab_window(&walk, TabId::Text);
    let before = w.layers();
    assert!(w.click_label("Font"), "the Font list's button");
    w.frames(6, Vec::new());
    let menu: HashSet<egui::LayerId> = w.layers().difference(&before).copied().collect();
    assert!(!menu.is_empty(), "the Font menu opens");
    let fonts: Vec<Control> = w.controls().into_iter().filter(|c| menu.contains(&c.layer)).collect();
    assert!(fonts.len() >= 4, "the Font list has its fonts: {}", fonts.len());
    let first = fonts
        .iter()
        .min_by(|a, b| a.rect.min.y.total_cmp(&b.rect.min.y))
        .expect("the fonts")
        .clone();
    let last = fonts
        .iter()
        .max_by(|a, b| a.rect.max.y.total_cmp(&b.rect.max.y))
        .expect("the fonts")
        .clone();
    assert!(w.shown(&first), "the first font is in view when the menu opens: {first:?}");
    assert!(!w.shown(&last), "the last font is not, the list is longer: {last:?}");
    let at = w.visible(&first).center();
    let end = wheel_to_the_end(&mut w, at);
    let now = |w: &Win, id| w.controls().into_iter().find(|c| c.id == id);
    let last_now = now(&w, last.id).expect("the last font");
    let first_now = now(&w, first.id).expect("the first font");
    assert!(w.shown(&last_now), "the wheel brings the last font into view: {last_now:?} ({end:?})");
    assert!(!w.shown(&first_now), "and takes the first out: {first_now:?}");
    let cut: Vec<String> = w
        .controls()
        .iter()
        .filter(|c| menu.contains(&c.layer) && c.rect.max.y > w.screen().bottom())
        .map(|c| c.label.clone())
        .collect();
    assert!(cut.is_empty(), "below the window after the wheel: {cut:?} ({end:?})");
    let row = w.audit("Font menu", "list", Some(&menu));
    assert!(row.lost.is_empty(), "{row:?}");
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
    let cases: [(&str, (f32, f32), u32, Setup); 6] = [
        ("the main window, panes docked", (900.0, 600.0), 100, |_| {}),
        // The Assistant's panel: its height follows what is open in it, and its settings
        // form scrolls sideways when the panel is narrower than it.
        ("the Assistant with its settings, panel squeezed", (900.0, 600.0), 100, |w| {
            w.app.assistant.panel.open = true;
            crate::assistant_panel::show_sample_notices(&mut w.app);
        }),
        ("the Assistant with its settings, as a page", (640.0, 400.0), 200, |w| {
            w.app.assistant.panel.open = true;
            crate::assistant_panel::show_sample_notices(&mut w.app);
        }),
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
        // (Closed unless the case opens it: nothing in it has the keyboard focus, so no
        // caret blinks either way.)
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

/// An open menu is opaque once it has faded in: its frame, the rectangle of the menu's own
/// area, is painted in the menu's fill at full strength, and that rectangle was not there
/// before the menu opened (a panel of the same colour behind it does not count). A
/// `--screenshot` taken six frames after the click had caught the fade and showed the pane
/// through the menu.
#[test]
fn an_open_menu_is_opaque_once_it_has_settled() {
    for (px, scale) in [((900.0, 600.0), 100), ((640.0, 400.0), 200)] {
        let mut w = Win::new(demo(), px, scale);
        let before_layers = w.layers();
        let before = filled(&w);
        assert!(w.click_label("Options"));
        w.frames(60, Vec::new());
        let layer = w
            .layers()
            .difference(&before_layers)
            .copied()
            .max_by_key(|l| w.controls().iter().filter(|c| c.layer == *l).count())
            .expect("the menu's layer");
        let area = w.ctx.memory(|m| m.area_rect(layer.id)).expect("the menu's area");
        let fill = egui::Frame::menu(&w.ctx.global_style()).fill;
        assert_eq!(fill.a(), 255, "the menu's fill is opaque");
        // Its frame: a rectangle in that fill as large as the menu's area (to within the
        // frame's stroke), which no shape of the frame before the click was.
        let frame = |r: &egui::Rect| {
            (r.min - area.min).length() <= 2.0 && (r.max - area.max).length() <= 2.0
        };
        let panel = filled(&w).into_iter().find(|(r, c)| *c == fill && frame(r));
        assert!(
            panel.is_some(),
            "at {px:?}, {scale} %: no rectangle of the menu's fill {fill:?} as large as its area {area:?}"
        );
        assert!(
            !before.iter().any(|(r, _)| frame(r)),
            "at {px:?}, {scale} %: a rectangle of that place was there before the menu"
        );
        // And the rows are painted over it: a row of the menu, in view, lies inside it.
        let row = w
            .controls()
            .into_iter()
            .find(|c| c.layer == layer && w.shown(c))
            .expect("a row of the menu in view");
        assert!(area.contains_rect(row.rect), "{row:?} in {area:?}");
    }
}

/// The window a scene opens, as the layers it adds over the main window.
fn scene(name: &str, px: (f32, f32), scale: u32) -> (Win, HashSet<egui::LayerId>) {
    let base = Win::new(demo(), px, scale).layers();
    let mut w = Win::new(
        Launch {
            show: vec![name.to_string()],
            ..demo()
        },
        px,
        scale,
    );
    w.frames(12, Vec::new());
    let new = w.layers().difference(&base).copied().collect();
    (w, new)
}

/// The Stitch Simulator docked below or above the work area gives way in a low window: the
/// work area keeps its 160 points of height (`WORK_AREA_LEAST`, to within half a point of
/// rounding), with or without the Assistant beside it, and the Simulator stays in view. At
/// 900 × 600 the Simulator's own height leaves more; at 900 × 400 (still docked, not yet a
/// page) the rule decides.
#[test]
fn the_simulator_docked_above_or_below_leaves_the_work_area_its_height() {
    use openstitch_app::editor::prefs::Side;
    let mut lowest = f32::INFINITY;
    for px in [(900.0, 600.0), (900.0, 400.0)] {
        for side in [Side::Bottom, Side::Top] {
            for assistant in [false, true] {
                let mut w = Win::new(demo(), px, 100);
                let sim = &mut w.app.ui.prefs.panes.simulator;
                sim.side = side;
                sim.visible = true;
                sim.floating = false;
                w.app.assistant.panel.open = assistant;
                w.frames(12, Vec::new());
                let canvas = w.app.workspace.canvas.expect("canvas");
                lowest = lowest.min(canvas.height());
                assert!(
                    canvas.height() >= 160.0 - 0.5,
                    "{px:?}, Simulator {side:?}, Assistant {assistant}: the work area is {:.1} points high",
                    canvas.height()
                );
                // And the Simulator is there, not pushed out: at 900 × 600 its Play button is in
                // view as it is; at 900 × 400 the Simulator is squeezed to the room left over
                // and scrolls, so Play is reached by scrolling the pane.
                let play = w.controls().into_iter().find(|c| c.label == "Play").expect("the Simulator's Play");
                if px.1 >= 600.0 {
                    assert!(w.shown(&play), "{px:?}, Simulator {side:?}, Assistant {assistant}: {play:?}");
                } else {
                    assert!(
                        w.reach(play.id, &mut HashSet::new()),
                        "{px:?}, Simulator {side:?}, Assistant {assistant}: {play:?}"
                    );
                }
            }
        }
    }
    // The rule is what holds the work area at 900 × 400: it is 160 points there, not more.
    assert!(lowest < 160.0 + 0.5, "the Simulator gave way at neither size: {lowest}");
}

/// A dialog that opens with the app opens over the folded panes' title bars of the narrow
/// layout (they used to lie over Machine Setup's Save and Not now).
#[test]
fn a_dialog_that_opens_with_the_app_opens_over_the_folded_panes() {
    for name in ["machine-setup", "startup-wizard", "update"] {
        let (w, dialog) = scene(name, (640.0, 400.0), 100);
        assert!(!dialog.is_empty(), "{name} opens");
        let covered: Vec<String> = w
            .controls()
            .iter()
            .filter(|c| dialog.contains(&c.layer))
            .filter(|c| w.visible(c).width() > 1.0 && w.visible(c).height() > 1.0)
            .filter(|c| !w.on_top(c))
            .map(|c| format!("{:?} '{}' under {:?}", c.role, c.label, w.ctx.layer_id_at(w.visible(c).center())))
            .collect();
        assert!(covered.is_empty(), "{name}: {covered:?}");
    }
}

/// In a window that scrolls as a page (640 × 400 at Text size 200 %: 320 × 200 points) the
/// folded panes' title bars lie over the work area, never over the top bar or the ribbon:
/// they follow the work area as the page scrolls.
#[test]
fn in_a_window_that_scrolls_as_a_page_the_folded_panes_leave_the_ribbon_free() {
    let mut w = Win::new(demo(), (640.0, 400.0), 200);
    w.frames(10, Vec::new());
    let canvas = w.app.workspace.canvas.expect("canvas");
    let bars: Vec<egui::Rect> = ["Sewing Order", "Attributes", "Stitch Simulator"]
        .iter()
        .filter_map(|t| w.ctx.memory(|m| m.area_rect(egui::Id::unique(("narrow-pane", *t)))))
        .collect();
    assert_eq!(bars.len(), 3, "the three folded panes");
    for b in &bars {
        assert!(b.top() >= canvas.top() - 0.5, "a title bar {b:?} above the work area {canvas:?}");
    }
    // Every control of the top bar and the ribbon that is in view is on top.
    let above: Vec<Control> = w
        .controls()
        .into_iter()
        .filter(|c| {
            let v = w.visible(c);
            c.rect.max.y <= canvas.top() && v.width() > 1.0 && v.height() > 1.0
        })
        .collect();
    assert!(above.len() >= 10, "the top bar and the ribbon: {}", above.len());
    let covered: Vec<String> =
        above.iter().filter(|c| !w.on_top(c)).map(|c| format!("{:?} '{}'", c.role, c.label)).collect();
    assert!(covered.is_empty(), "under a title bar: {covered:?}");
    // The title bars are reached by scrolling the page.
    let base = demo_base_layers(&w);
    let row = w.audit("folded panes", "pane", Some(&base));
    assert!(row.controls >= 6 && row.lost.is_empty(), "{row:?}");
}

/// Beside the Assistant's panel (640 × 400 at Text size 150 %: 427 × 267 points, the work
/// area 142 points wide) the folded panes' title bars that follow the work area keep to its
/// width: they used to reach over the Assistant's composer and its View button.
#[test]
fn folded_panes_beside_the_assistant_keep_to_the_work_area() {
    let mut w = Win::new(demo(), (640.0, 400.0), 150);
    w.app.assistant.panel.open = true;
    w.frames(12, Vec::new());
    let canvas = w.app.workspace.canvas.expect("canvas");
    assert!(canvas.width() < 200.0, "the Assistant takes most of the width: {canvas:?}");
    let bars: Vec<egui::Rect> = ["Sewing Order", "Attributes", "Stitch Simulator"]
        .iter()
        .filter_map(|t| w.ctx.memory(|m| m.area_rect(egui::Id::unique(("narrow-pane", *t)))))
        .collect();
    assert_eq!(bars.len(), 3, "the three folded panes");
    for b in &bars {
        assert!(b.right() <= canvas.right() + 0.5, "a title bar {b:?} beyond the work area {canvas:?}");
    }
    let view = w
        .controls()
        .into_iter()
        .find(|c| c.label == "View" && c.tip.contains("picture"))
        .expect("the composer's View");
    assert!(w.reach(view.id, &mut HashSet::new()), "the composer's View is reached: {view:?}");
}

/// The layers of the folded panes' windows.
fn demo_base_layers(w: &Win) -> HashSet<egui::LayerId> {
    ["Sewing Order", "Attributes", "Stitch Simulator"]
        .iter()
        .map(|t| egui::LayerId::new(egui::Order::Middle, egui::Id::unique(("narrow-pane", *t))))
        .filter(|l| w.controls().iter().any(|c| c.layer == *l))
        .collect()
}

/// The Assistant's panel with its settings open: in the desktop's smallest window, where the
/// panel gives way to the docked panes, its settings form scrolls sideways; in a window that
/// scrolls as a page its settings, notices and composer scroll with the panel. Every control
/// of it is reached.
#[test]
fn the_assistants_settings_are_reached_in_a_squeezed_or_low_panel() {
    for (px, scale) in [((900.0, 600.0), 100), ((640.0, 400.0), 200)] {
        let mut w = Win::new(
            Launch {
                show: vec!["assistant-notices".into()],
                ..demo()
            },
            px,
            scale,
        );
        w.frames(12, Vec::new());
        let all = w.controls();
        let key = all.iter().find(|c| c.label == "Remove key").expect("the settings' Remove key");
        let panel = key.layer;
        let row = w.audit("Assistant", "pane", Some(&HashSet::from([panel])));
        assert!(row.lost.is_empty(), "{px:?} at {scale} %: {row:?}");
        assert!(row.hidden > 0, "{px:?} at {scale} %: something of it is out of view at first: {row:?}");
    }
    // At 900 × 600 the form is wider than the panel and scrolls sideways: Remove key starts
    // cut at the panel's right edge.
    let mut w = Win::new(
        Launch {
            show: vec!["assistant-notices".into()],
            ..demo()
        },
        (900.0, 600.0),
        100,
    );
    w.frames(12, Vec::new());
    let key = w.controls().into_iter().find(|c| c.label == "Remove key").expect("Remove key");
    assert!(!w.shown(&key), "the form is wider than the squeezed panel: {key:?}");
}

/// The Template Wizard in a low window: its list of templates takes part of the window, so
/// Next (kept in view below it) is reached.
#[test]
fn the_template_wizards_next_is_reached_in_a_low_window() {
    let (mut w, dialog) = scene("templates", (640.0, 400.0), 200);
    assert!(!dialog.is_empty(), "the Template Wizard opens");
    let next = w.controls().into_iter().find(|c| dialog.contains(&c.layer) && c.label == "Next");
    let next = next.expect("its Next");
    let row = w.audit("Template Wizard", "dialog", Some(&dialog));
    assert!(row.lost.is_empty(), "{row:?}");
    assert!(w.reach(next.id, &mut HashSet::new()), "Next is reached");
}

/// Check Design and Getting Started are windows of their own size around a list: the list
/// scrolls inside them and the row above it (the tabs; the steps) stays in view.
#[test]
fn a_window_around_a_list_keeps_its_header_in_view() {
    type Opener = fn(&mut EditorApp);
    let cases: [(&str, &str, Opener); 2] = [
        ("Check Design", "Problems", |a| {
            crate::quality_ui::open(a, crate::quality_ui::Tab::Route)
        }),
        ("Getting Started", "", |a| a.ui.start.page_open = true),
    ];
    for (name, header, opener) in cases {
        let base = Win::new(demo(), (1440.0, 900.0), 100).layers();
        let mut w = Win::new(demo(), (1440.0, 900.0), 100);
        opener(&mut w.app);
        w.frames(12, Vec::new());
        let layers: HashSet<egui::LayerId> = w.layers().difference(&base).copied().collect();
        assert!(!layers.is_empty(), "{name} opens");
        let area = layers
            .iter()
            .filter_map(|l| w.ctx.memory(|m| m.area_rect(l.id)))
            .next()
            .expect("its window");
        assert!(area.height() < 900.0 - 10.0, "{name} is a window of its own size: {area:?}");
        // The header: the topmost control of the window.
        let top = |w: &Win| {
            w.controls()
                .into_iter()
                .filter(|c| layers.contains(&c.layer))
                .filter(|c| header.is_empty() || c.label == header)
                .min_by(|a, b| a.rect.min.y.total_cmp(&b.rect.min.y))
                .expect("a header control")
        };
        let before = top(&w);
        assert!(w.shown(&before), "{name}: {before:?}");
        wheel_to_the_end(&mut w, area.center());
        let after = w.controls().into_iter().find(|c| c.id == before.id).expect("still there");
        assert!(w.shown(&after), "{name}: the header scrolled away: {after:?}");
        assert!((after.rect.min.y - before.rect.min.y).abs() < 1.0, "{name}: {before:?} to {after:?}");
    }
}

/// A combo box or a colour button inside a menu opens its list or picker and the menu stays
/// open (a click anywhere in a menu used to close it, so these lists never opened): Stamp /
/// Emboss's shape and Shape Effects' Warp in their ribbon lists, the guide colour in Options.
#[test]
fn a_combo_box_or_colour_button_in_a_menu_opens_and_keeps_the_menu() {
    let walk = Walk::new((900.0, 600.0), 100, None, true);
    let list = |tab: TabId, label: &str| {
        tab.groups()
            .iter()
            .flat_map(|g| g.actions().collect::<Vec<_>>())
            .find(|a| a.is_menu() && a.info().label == label)
            .unwrap_or_else(|| panic!("the {label} list"))
    };
    let cases: [(&str, TabId, Option<Action>, Role); 3] = [
        ("Stamp/Emboss", TabId::Home, Some(list(TabId::Home, "Stamp/Emboss")), Role::ComboBox),
        ("Shape Effects", TabId::Shapes, Some(list(TabId::Shapes, "Shape Effects")), Role::ComboBox),
        ("Options", TabId::Home, None, Role::ColorWell),
    ];
    for (name, tab, action, role) in cases {
        let mut w = tab_window(&walk, tab);
        let before = w.layers();
        let opened = match action {
            Some(a) => w.click_action(a),
            None => w.click_label("Options"),
        };
        assert!(opened, "{name} opens");
        w.frames(6, Vec::new());
        let menu: HashSet<egui::LayerId> = w.layers().difference(&before).copied().collect();
        let control = w
            .controls()
            .into_iter()
            .find(|c| menu.contains(&c.layer) && c.role == role && !c.disabled)
            .unwrap_or_else(|| panic!("{name}: a {role:?} in it"));
        assert!(w.reach(control.id, &mut HashSet::new()), "{name}: its {role:?} is reached");
        let control = w.controls().into_iter().find(|c| c.id == control.id).expect("still there");
        let layers_before = w.ctx.memory(|m| m.areas().visible_layer_ids());
        w.click(w.visible(&control).center());
        w.frames(6, Vec::new());
        let appeared = w.ctx.memory(|m| m.areas().visible_layer_ids()).difference(&layers_before).count();
        assert!(appeared > 0, "{name}: its {role:?} '{}' opens its list", control.label);
        assert!(
            w.controls().iter().any(|c| menu.contains(&c.layer)),
            "{name}: the menu stays open"
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

/// Escape closes an open combo box list in a dialog (the editor's own Escape used to take the
/// key first, so the list stayed open over the dialog) and the dialog stays open.
#[test]
fn escape_closes_a_combo_box_list_in_a_dialog() {
    let (mut w, dialog) = scene("settings", (640.0, 400.0), 200);
    let hoop = w
        .controls()
        .into_iter()
        .find(|c| dialog.contains(&c.layer) && c.role == Role::ComboBox)
        .expect("a combo box in Design Settings");
    assert!(w.reach(hoop.id, &mut HashSet::new()));
    let hoop = w.controls().into_iter().find(|c| c.id == hoop.id).expect("still there");
    let before = w.ctx.memory(|m| m.areas().visible_layer_ids());
    w.click(w.visible(&hoop).center());
    w.frames(6, Vec::new());
    let open = |w: &Win| w.ctx.memory(|m| m.areas().visible_layer_ids()).difference(&before).count();
    assert!(open(&w) > 0, "its list opens");
    let selection = w.app.selection.clone();
    w.key(egui::Key::Escape);
    w.frames(6, Vec::new());
    assert_eq!(open(&w), 0, "Escape closes the list");
    assert!(w.controls().iter().any(|c| dialog.contains(&c.layer)), "the dialog stays open");
    assert_eq!(w.app.selection, selection, "and the selection is kept");
}

/// The start cards over an empty design are a list that scrolls in place, not a pop-up:
/// Escape under them still does the editor's Escape (here: leaves the Measure tool), as on
/// main. (Round 3 had let every list drawn as a menu take Escape.)
#[test]
fn escape_leaves_the_measure_tool_under_the_start_cards() {
    let mut w = Win::new(Launch::default(), (1440.0, 900.0), 100);
    w.app.ui.prefs.start_cards = true;
    w.app.ui.start.cards_dismissed = false;
    w.frames(6, Vec::new());
    let cards = egui::LayerId::new(egui::Order::Middle, egui::Id::unique("start-cards"));
    assert!(
        w.ctx.memory(|m| m.areas().visible_layer_ids()).contains(&cards),
        "the start cards show over the empty design"
    );
    w.app.workspace.measure.active = true;
    w.frames(2, Vec::new());
    w.key(egui::Key::Escape);
    w.frames(3, Vec::new());
    assert!(!w.app.workspace.measure.active, "Escape leaves the Measure tool");
}

/// With a menu open, Escape closes the menu and nothing else (the selection stays); the next
/// Escape is the editor's (it clears the selection). For a menu of the toolkit's (Options) and
/// one that keeps its own open state (the ribbon's File menu).
#[test]
fn escape_closes_an_open_menu_first_then_does_the_editors_escape() {
    for label in ["Options", "File"] {
        let mut w = Win::new(demo(), (1440.0, 900.0), 100);
        w.frames(4, Vec::new());
        assert!(!w.app.selection.is_empty(), "the demo starts with a selection");
        let before = w.layers();
        assert!(w.click_label(label), "{label}");
        w.frames(6, Vec::new());
        let open = |w: &Win| w.layers().difference(&before).count();
        assert!(open(&w) > 0, "{label}: the menu opens");
        let selection = w.app.selection.clone();
        w.key(egui::Key::Escape);
        w.frames(6, Vec::new());
        assert_eq!(open(&w), 0, "{label}: the first Escape closes the menu");
        assert_eq!(w.app.selection, selection, "{label}: and keeps the selection");
        w.key(egui::Key::Escape);
        w.frames(3, Vec::new());
        assert!(w.app.selection.is_empty(), "{label}: the second Escape clears the selection");
    }
}

/// A dialog's scroll bar lies beside its content, not over it: a click in the last 10 points
/// of the content's width reaches the content. In the dialogs that scroll at 320 × 200 points
/// (their bar shows), a combo box or text field whose right end is in those 10 points opens or
/// takes the keyboard there; a floating bar used to take that click (the Catalogue's format
/// box, round 3). At least one such control is found.
#[test]
fn a_click_at_the_right_edge_of_a_scrolling_dialog_reaches_its_content() {
    let mut tried = Vec::new();
    for name in ["settings", "property", "text", "print", "fabric", "usb", "monogram", "arrange", "matchThreads"] {
        let (mut w, dialog) = scene(name, (640.0, 400.0), 200);
        let Some(view) = w.app.drawn.list.iter().find(|s| s.kind == "dialog").cloned() else {
            continue;
        };
        if view.content <= view.rect.height() + 1.0 {
            continue;
        }
        let edge = view.rect.right();
        let near: Vec<Control> = w
            .controls()
            .into_iter()
            .filter(|c| dialog.contains(&c.layer) && matches!(c.role, Role::ComboBox | Role::TextInput))
            .filter(|c| c.rect.right() > edge - 10.0 && c.rect.left() < edge - 10.0)
            .collect();
        for c in near {
            if !w.reach(c.id, &mut HashSet::new()) {
                continue;
            }
            let Some(c) = w.controls().into_iter().find(|n| n.id == c.id) else { continue };
            let view = w.app.drawn.list.iter().find(|s| s.kind == "dialog").expect("open").rect;
            let at = egui::pos2(view.right() - 3.0, w.visible(&c).center().y);
            if !w.visible(&c).contains(at) {
                continue;
            }
            let before = w.layers();
            w.click(at);
            w.frames(6, Vec::new());
            let reached = match c.role {
                Role::ComboBox => w.layers().difference(&before).count() > 0,
                _ => w.ctx.memory(|m| m.focused()) == Some(c.id),
            };
            assert!(reached, "{name}: a click at {at:?}, 3 points inside the content's right edge {view:?}, missed {c:?}");
            tried.push(format!("{name}: {:?} '{}'", c.role, c.label));
            w.key(egui::Key::Escape);
        }
    }
    eprintln!("clicked at the right edge: {tried:?}");
    assert!(!tried.is_empty(), "no control reaches the last 10 points of a scrolling dialog");
}

/// A dialog that scrolls shows its scroll bar at rest, beside its content and inside the
/// window: Design Settings at 640 × 400, whose content is cut at its foot (a review of a
/// screenshot asked whether the bar is drawn there; it is, a light handle by the edge).
#[test]
fn a_scrolling_dialog_shows_its_scroll_bar_at_rest() {
    let (mut w, _) = scene("settings", (640.0, 400.0), 100);
    // The pointer away from it, over the status bar's left end.
    w.frames(1, vec![egui::Event::PointerMoved(egui::pos2(4.0, w.screen().bottom() - 4.0))]);
    w.frames(30, Vec::new());
    let view = w.app.drawn.list.iter().find(|s| s.kind == "dialog").cloned().expect("Design Settings");
    assert!(view.content > view.rect.height() + 1.0, "it scrolls: {view:?}");
    let bar_room = egui::style::ScrollStyle::solid().allocated_width() + 1.0;
    let beside: Vec<(egui::Rect, egui::Color32)> = filled(&w)
        .into_iter()
        .filter(|(r, c)| {
            c.a() > 0
                && r.left() >= view.rect.right() - 0.5
                && r.right() <= view.rect.right() + bar_room
                && r.height() >= 12.0
                && r.top() >= view.rect.top() - 0.5
                && r.bottom() <= view.rect.bottom() + 0.5
        })
        .collect();
    assert!(!beside.is_empty(), "a scroll bar painted beside the content {view:?} at rest");
    let screen = w.screen();
    assert!(
        beside.iter().all(|(r, _)| screen.contains_rect(*r)),
        "the bar is inside the window {screen:?}: {beside:?}"
    );
}

/// A dialog a person made larger in a large app window fits inside the app window once that is
/// made smaller (the toolkit holds a resized window to the app window, and `fit::show` caps
/// the content's height; nothing of the dialog is left beyond the window).
/// The resizable dialogs Keyboard Shortcuts, Match to Thread Chart and the update, dragged by
/// their corner to 300 points more at 1440 × 900, then the app window made 640 × 400.
#[test]
fn a_dialog_sized_in_a_larger_window_is_held_to_a_smaller_one() {
    for name in ["shortcuts", "matchThreads", "update"] {
        let (mut w, dialog) = scene(name, (1440.0, 900.0), 100);
        let area = |w: &Win| {
            dialog
                .iter()
                .filter_map(|l| w.ctx.memory(|m| m.area_rect(l.id)))
                .fold(egui::Rect::NOTHING, |a, r| a.union(r))
        };
        let before = area(&w);
        assert!(before.is_positive(), "{name} opens");
        // Its bottom-right corner dragged 300 points down and right.
        let from = before.max - egui::vec2(3.0, 3.0);
        let press = |at, pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        w.frames(2, vec![egui::Event::PointerMoved(from)]);
        w.frames(1, vec![press(from, true)]);
        for i in 1..=10 {
            let at = from + egui::vec2(30.0, 30.0) * i as f32;
            w.frames(1, vec![egui::Event::PointerMoved(at)]);
        }
        let to = from + egui::vec2(300.0, 300.0);
        w.frames(1, vec![press(to, false)]);
        w.frames(6, Vec::new());
        let large = area(&w);
        // (Its height is the dialog's own, held by the room the content may take; its width
        // is the person's.)
        assert!(
            large.width() > before.width() + 100.0 || large.height() > before.height() + 100.0,
            "{name} was made larger: {before:?} to {large:?}"
        );
        assert!(
            large.width() > 640.0 || large.height() > 400.0,
            "{name} is larger than the small window will be: {large:?}"
        );
        w.size = egui::vec2(640.0, 400.0);
        w.frames(15, Vec::new());
        let small = area(&w);
        let screen = w.screen();
        assert!(
            small.min.x >= screen.min.x - 0.5
                && small.min.y >= screen.min.y - 0.5
                && small.max.x <= screen.max.x + 0.5
                && small.max.y <= screen.max.y + 0.5,
            "{name}: {large:?} at 1440 x 900, {small:?} in the 640 x 400 window {screen:?}"
        );
    }
}

/// The Template Wizard keeps Next (Back, the output buttons) in view once a step is taller
/// than the window, at 320 × 200 points (on `PINNED_AT_320X200`) and 320 × 150; Floppy Image,
/// 83 points high with its buttons, fits every window in which a row can be kept in view
/// (below that, about 130 points, rows scroll with the content), so it has no pinned row.
#[test]
fn the_template_wizard_keeps_its_buttons_in_view_and_floppy_image_fits() {
    type Opener = fn(&mut EditorApp);
    let steps: [(&str, Opener); 2] = [
        ("step 1", |a| crate::lettering_ui::open_template_wizard(a)),
        ("step 2", |a| {
            crate::lettering_ui::open_template_wizard(a);
            a.lettering.templates.step = 2;
        }),
    ];
    // 320 × 150 points: both steps; 320 × 200: step 1, the one taller than the window.
    let sizes: [(&str, Opener, (f32, f32)); 3] = [
        (steps[0].0, steps[0].1, (640.0, 300.0)),
        (steps[1].0, steps[1].1, (640.0, 300.0)),
        (steps[0].0, steps[0].1, (640.0, 400.0)),
    ];
    for (name, opener, px) in sizes {
        let mut w = Win::new(demo(), px, 200);
        opener(&mut w.app);
        w.frames(15, Vec::new());
        let d = w.app.drawn.list.iter().find(|s| s.kind == "dialog").cloned().expect("the wizard");
        assert!(d.buttons.is_some(), "Template Wizard {name} at {px:?}: its buttons are kept in view: {d:?}");
        // And it rests there: the row does not come and go from frame to frame.
        w.frames(5, Vec::new());
        let again = w.app.drawn.list.iter().find(|s| s.kind == "dialog").cloned().expect("the wizard");
        assert_eq!(again.buttons, d.buttons, "Template Wizard {name} at {px:?} settles");
    }
    for px in [(640.0, 300.0), (640.0, 400.0)] {
        let mut w = Win::new(demo(), px, 200);
        crate::floppy_ui::open(&mut w.app);
        w.frames(15, Vec::new());
        let d = w.app.drawn.list.iter().find(|s| s.kind == "dialog").cloned().expect("Floppy Image");
        assert!(d.content <= d.rect.height() + 0.5, "Floppy Image fits at {px:?}: {d:?}");
    }
}
