//! Several designs open as tabs in one window (#552, beyond PE-DESIGN). The active design's
//! state stays in [`EditorApp`]'s own fields, so the rest of the editor is unchanged; the other
//! tabs are parked [`Document`]s and a switch swaps the fields. Each tab has its own session
//! (design, engine history), undo groups, selection, zoom, simulator and file.
//!
//! Revision-keyed caches (render, quality maps, colour matching, autosave) stay correct: a
//! design switched in takes a revision above every one seen, so no two tabs share one.
//!
//! Options ▸ Open designs in tabs (`openInTabs`, off by default) makes File ▸ New and Open add
//! a tab instead of replacing the design after the unsaved-changes prompt, as PE-DESIGN does.

use super::*;

/// A tab that is not the active one.
pub struct Document {
    session: Session,
    selection: BTreeSet<u64>,
    click_order: Vec<u64>,
    tools: ToolController,
    viewport: Option<Viewport>,
    pattern: StitchPattern,
    stats: PatternStats,
    render: RenderModel,
    render_key: Option<(u64, usize, openstitch_design::view::ViewFilter, [u8; 3])>,
    underlay: Option<(u64, Vec<bool>)>,
    revision: u64,
    saved_revision: u64,
    simulate: Option<usize>,
    playing: bool,
    play_accumulator: f64,
    background_split: Option<((u64, usize, usize), RenderModel, RenderModel)>,
    timeline: openstitch_design::timeline::Timeline,
    path: Option<PathBuf>,
    drafts: HashMap<&'static str, f64>,
    key_object: Option<u64>,
    undo_groups: Vec<usize>,
    redo_groups: Vec<usize>,
    spans: Vec<(u64, std::ops::Range<usize>)>,
    estimate: Option<openstitch_design::Estimate>,
    recovered_from: Option<String>,
}

impl Document {
    /// An empty, unsaved-free document beside `session`.
    fn blank(session: &Session) -> Document {
        Document {
            session: session.sibling(),
            selection: BTreeSet::new(),
            click_order: Vec::new(),
            tools: ToolController::default(),
            viewport: None,
            pattern: StitchPattern::new(),
            stats: PatternStats::default(),
            render: RenderModel::default(),
            render_key: None,
            underlay: None,
            revision: 0,
            saved_revision: 0,
            simulate: None,
            playing: false,
            play_accumulator: 0.0,
            background_split: None,
            timeline: Default::default(),
            path: None,
            drafts: HashMap::new(),
            key_object: None,
            undo_groups: Vec::new(),
            redo_groups: Vec::new(),
            spans: Vec::new(),
            estimate: None,
            recovered_from: None,
        }
    }

    fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    fn name(&self) -> String {
        tab_name(
            self.path.as_deref(),
            &self.session.design().name,
            self.recovered_from.as_deref(),
        )
    }
}

fn tab_name(path: Option<&Path>, design: &str, recovered: Option<&str>) -> String {
    path.and_then(|p| p.file_name()).map_or_else(
        || match recovered {
            Some(from) => format!("{design} (recovered from {from})"),
            None => design.to_string(),
        },
        |n| n.to_string_lossy().into_owned(),
    )
}

/// The open tabs: the parked documents, with `None` where the active one is.
#[derive(Default)]
pub struct Tabs {
    slots: Vec<Option<Document>>,
    active: usize,
    /// The highest revision any document reached.
    high: u64,
}

impl Tabs {
    pub fn count(&self) -> usize {
        self.slots.len().max(1)
    }

    pub fn active(&self) -> usize {
        self.active
    }
}

impl EditorApp {
    /// Swaps the active design's state with `doc`.
    fn swap_document(&mut self, doc: &mut Document) {
        use std::mem::swap;
        swap(&mut self.session, &mut doc.session);
        swap(&mut self.selection, &mut doc.selection);
        swap(&mut self.click_order, &mut doc.click_order);
        swap(&mut self.tools, &mut doc.tools);
        swap(&mut self.viewport, &mut doc.viewport);
        swap(&mut self.pattern, &mut doc.pattern);
        swap(&mut self.stats, &mut doc.stats);
        swap(&mut self.render, &mut doc.render);
        swap(&mut self.render_key, &mut doc.render_key);
        swap(&mut self.underlay, &mut doc.underlay);
        swap(&mut self.revision, &mut doc.revision);
        swap(&mut self.saved_revision, &mut doc.saved_revision);
        swap(&mut self.simulate, &mut doc.simulate);
        swap(&mut self.playing, &mut doc.playing);
        swap(&mut self.play_accumulator, &mut doc.play_accumulator);
        swap(&mut self.background_split, &mut doc.background_split);
        swap(&mut self.timeline, &mut doc.timeline);
        swap(&mut self.path, &mut doc.path);
        swap(&mut self.drafts, &mut doc.drafts);
        swap(&mut self.key_object, &mut doc.key_object);
        swap(&mut self.undo_groups, &mut doc.undo_groups);
        swap(&mut self.redo_groups, &mut doc.redo_groups);
        swap(&mut self.quality.spans, &mut doc.spans);
        swap(&mut self.workspace.estimate, &mut doc.estimate);
        swap(&mut self.recovery.recovered_from, &mut doc.recovered_from);
    }

    /// Parks the active design in its slot and makes `doc` active.
    fn park_and_activate(&mut self, mut doc: Document) -> Document {
        self.tools.cancel();
        self.playing = false;
        self.tabs.high = self.tabs.high.max(self.revision);
        self.swap_document(&mut doc);
        // `doc` now holds the parked design. Shared settings follow the user across tabs.
        self.session.copy_user_settings(&doc.session);
        let clean = self.revision == self.saved_revision;
        self.tabs.high += 1;
        self.revision = self.tabs.high;
        self.saved_revision = if clean {
            self.revision
        } else {
            self.revision.wrapping_add(u64::MAX / 2)
        };
        doc
    }

    pub fn tab_count(&self) -> usize {
        self.tabs.count()
    }

    /// Opens an empty tab and makes it active.
    pub fn new_tab(&mut self) {
        if self.tabs.slots.is_empty() {
            self.tabs.slots.push(None);
        }
        let parked = self.park_and_activate(Document::blank(&self.session));
        self.tabs.slots[self.tabs.active] = Some(parked);
        self.tabs.slots.push(None);
        self.tabs.active = self.tabs.slots.len() - 1;
        self.refresh();
        self.saved_revision = self.revision;
        self.fit_requested = true;
    }

    /// Makes tab `i` active; `false` when there is no such tab.
    pub fn activate_tab(&mut self, i: usize) -> bool {
        if i >= self.tabs.slots.len() {
            return false;
        }
        if i == self.tabs.active {
            return true;
        }
        let doc = self.tabs.slots[i].take().expect("parked");
        let parked = self.park_and_activate(doc);
        self.tabs.slots[self.tabs.active] = Some(parked);
        self.tabs.active = i;
        true
    }

    /// Ctrl+Tab / Ctrl+Shift+Tab.
    pub fn cycle_tab(&mut self, back: bool) {
        let n = self.tab_count();
        if n > 1 {
            let i = (self.tabs.active + if back { n - 1 } else { 1 }) % n;
            self.activate_tab(i);
        }
    }

    /// Closes the active tab without asking (the caller asked about unsaved changes). The last
    /// tab is never closed: it becomes an empty design.
    pub fn close_active_tab(&mut self) {
        if self.tab_count() <= 1 {
            self.tabs.slots.clear();
            self.tabs.active = 0;
            self.new_design();
            return;
        }
        let closing = self.tabs.active;
        let next = if closing + 1 < self.tabs.slots.len() {
            closing + 1
        } else {
            closing - 1
        };
        self.activate_tab(next);
        self.tabs.slots.remove(closing);
        if self.tabs.active > closing {
            self.tabs.active -= 1;
        }
        if self.tabs.slots.len() == 1 {
            self.tabs.slots.clear();
            self.tabs.active = 0;
        }
    }

    /// File ▸ Close Tab, the tab's close button: asks first when the tab has unsaved changes.
    pub fn request_close_tab(&mut self, i: usize) {
        if self.activate_tab(i) || self.tab_count() == 1 {
            self.guarded(Pending::CloseTab);
        }
    }

    /// Any tab with unsaved changes.
    pub fn any_dirty(&self) -> bool {
        self.is_dirty() || self.dirty_parked().is_some()
    }

    /// The first parked tab with unsaved changes.
    pub(crate) fn dirty_parked(&self) -> Option<usize> {
        self.tabs
            .slots
            .iter()
            .position(|d| d.as_ref().is_some_and(Document::is_dirty))
    }

    /// Each tab: name, unsaved, file, active.
    pub fn tab_list(&self) -> Vec<(String, bool, Option<PathBuf>, bool)> {
        if self.tabs.slots.is_empty() {
            return vec![(
                self.document_name(),
                self.is_dirty(),
                self.path.clone(),
                true,
            )];
        }
        self.tabs
            .slots
            .iter()
            .map(|d| match d {
                Some(d) => (d.name(), d.is_dirty(), d.path.clone(), false),
                None => (
                    self.document_name(),
                    self.is_dirty(),
                    self.path.clone(),
                    true,
                ),
            })
            .collect()
    }

    /// `ui.documents`: list, activate, new or close a tab.
    pub(crate) fn documents_command(
        &mut self,
        op: &str,
        index: Option<usize>,
    ) -> Result<Value, String> {
        let list = |app: &EditorApp| {
            json!({
                "active": app.tabs.active(),
                "openInTabs": app.ui.prefs.open_in_tabs,
                "tabs": app.tab_list().into_iter().enumerate().map(|(i, (name, unsaved, file, active))| json!({
                    "index": i,
                    "name": name,
                    "unsaved": unsaved,
                    "file": file.map(|p| p.display().to_string()),
                    "active": active,
                })).collect::<Vec<_>>(),
            })
        };
        let need = || index.ok_or_else(|| "give index (from ui.documents)".to_string());
        match op {
            "list" => {}
            "activate" => {
                let i = need()?;
                if !self.activate_tab(i) && i != 0 {
                    return Err(format!("no tab {i}; there are {}", self.tab_count()));
                }
            }
            "new" => self.new_tab(),
            "close" => {
                let i = index.unwrap_or(self.tabs.active);
                if i >= self.tab_count() {
                    return Err(format!("no tab {i}; there are {}", self.tab_count()));
                }
                self.activate_tab(i);
                if self.is_dirty() {
                    return Err(format!(
                        "'{}' has unsaved changes: save it with project.save first, or ask the user to close the tab",
                        self.document_name()
                    ));
                }
                self.close_active_tab();
            }
            other => {
                return Err(format!(
                    "op takes list, activate, new or close, not '{other}'"
                ));
            }
        }
        Ok(list(self))
    }
}

/// The tab strip under the ribbon: one tab per design, `•` when unsaved, a close button each,
/// and a New Tab button. Shown when more than one design is open or Open designs in tabs is on.
pub fn strip(app: &mut EditorApp, ui: &mut egui::Ui) {
    if app.tab_count() < 2 && !app.ui.prefs.open_in_tabs {
        return;
    }
    let mut activate = None;
    let mut close = None;
    let mut add = false;
    ui.horizontal_wrapped(|ui| {
        for (i, (name, unsaved, file, active)) in app.tab_list().into_iter().enumerate() {
            let text = if unsaved { format!("{name} •") } else { name };
            let r = ui
                .add(egui::Button::selectable(active, text))
                .on_hover_text(file.map_or_else(
                    || "Not saved to a file yet".to_string(),
                    |p| p.display().to_string(),
                ));
            if r.clicked() {
                activate = Some(i);
            }
            if ui
                .small_button("Close")
                .on_hover_text("Close this design (asks first when it has unsaved changes)")
                .clicked()
            {
                close = Some(i);
            }
            ui.separator();
        }
        add = ui
            .button("New Tab")
            .on_hover_text("An empty design in a new tab (Ctrl+T)")
            .clicked();
    });
    if let Some(i) = activate {
        app.activate_tab(i);
    }
    if let Some(i) = close {
        app.request_close_tab(i);
    }
    if add {
        app.new_tab();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rectangle(app: &mut EditorApp, x: f64) {
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": x, "y": 5, "width": 20, "height": 20 }),
        ))
        .expect("rectangle");
    }

    #[test]
    fn two_tabs_keep_their_own_undo_selection_and_zoom() {
        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app, 5.0);
        app.select_all();
        let zoom_a = Viewport::default();
        app.viewport = Some(zoom_a);
        app.new_tab();
        assert_eq!(app.tab_count(), 2);
        assert!(
            app.session.design().objects.is_empty(),
            "a new tab is empty"
        );
        assert!(app.selection.is_empty() && app.viewport.is_none());
        rectangle(&mut app, 30.0);
        rectangle(&mut app, 60.0);
        app.undo();
        assert_eq!(app.session.design().objects.len(), 1, "B's own undo");

        app.activate_tab(0);
        assert_eq!(app.session.design().objects.len(), 1);
        assert_eq!(app.selection.len(), 1, "A's selection came back");
        assert!(app.viewport.is_some(), "A's zoom came back");
        app.undo();
        assert!(app.session.design().objects.is_empty(), "A's own undo");
        app.cycle_tab(false);
        assert_eq!(
            app.session.design().objects.len(),
            1,
            "B untouched by A's undo"
        );
        assert_eq!(app.pattern, {
            let d = app.session.design();
            openstitch_design::Compiler::for_design(d).compile(d)
        });
    }

    #[test]
    fn revisions_never_repeat_across_tabs_and_dirty_survives_a_switch() {
        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app, 5.0);
        assert!(app.is_dirty());
        let a = app.revision;
        app.new_tab();
        let b = app.revision;
        assert!(b > a);
        assert!(!app.is_dirty(), "the new tab is clean");
        app.activate_tab(0);
        assert!(
            app.revision > b,
            "a switched-in design takes a new revision"
        );
        assert!(app.is_dirty(), "and stays unsaved");
        app.activate_tab(1);
        assert!(!app.is_dirty());
        assert!(app.any_dirty(), "tab 0 still has changes");
        assert_eq!(app.dirty_parked(), Some(0));
    }

    #[test]
    fn closing_a_tab_asks_only_for_that_tab() {
        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app, 5.0);
        app.new_tab();
        app.request_close_tab(1);
        assert!(!app.close_prompt.is_open(), "a clean tab closes at once");
        assert_eq!(app.tab_count(), 1);
        assert_eq!(
            app.session.design().objects.len(),
            1,
            "back on the first design"
        );

        app.new_tab();
        rectangle(&mut app, 30.0);
        app.request_close_tab(0);
        assert!(app.close_prompt.is_open(), "the dirty first tab asks");
        assert_eq!(app.tabs.active(), 0);
        app.answer_close_prompt(Choice::Cancel);
        assert_eq!(app.tab_count(), 2, "Cancel keeps it");
        app.request_close_tab(0);
        app.answer_close_prompt(Choice::DontSave);
        assert_eq!(app.tab_count(), 1);
        assert_eq!(
            app.session.design().objects[0]
                .bounds()
                .expect("b")
                .min_x
                .round(),
            300.0,
            "the other design is left (bounds in 0.1 mm)"
        );
    }

    #[test]
    fn closing_the_window_asks_for_each_dirty_tab() {
        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app, 5.0);
        app.new_tab();
        rectangle(&mut app, 30.0);
        app.new_tab();
        assert!(!app.is_dirty() && app.any_dirty());
        app.guarded(Pending::Exit);
        assert!(app.close_prompt.is_open(), "a dirty tab asks");
        app.answer_close_prompt(Choice::DontSave);
        assert!(!app.exit_approved(), "another dirty tab is left");
        assert!(app.close_prompt.is_open(), "and asks next");
        app.answer_close_prompt(Choice::DontSave);
        assert!(app.exit_approved(), "then the window closes");
    }

    #[test]
    fn open_replaces_by_default_and_opens_a_tab_when_chosen() {
        assert!(!openstitch_app::editor::prefs::UiPrefs::default().open_in_tabs);
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../openstitch-engine/tests/fixtures/badge-golden.pes");
        let mut app = EditorApp::new(Launch::default());
        app.guarded(Pending::Open(Some(fixture.clone())));
        assert_eq!(app.tab_count(), 1, "PE-DESIGN's behaviour: replace");

        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "openInTabs", "value": true })),
        )
        .expect("set");
        rectangle(&mut app, 5.0);
        app.guarded(Pending::Open(Some(fixture)));
        assert!(
            !app.close_prompt.is_open(),
            "the edited design stays in its tab"
        );
        assert_eq!(app.tab_count(), 2);
        assert_eq!(app.tabs.active(), 1);
        app.guarded(Pending::New);
        assert_eq!(app.tab_count(), 3, "New adds a tab too");
    }

    #[test]
    fn the_assistant_lists_switches_and_closes_tabs() {
        let mut app = EditorApp::new(Launch::default());
        rectangle(&mut app, 5.0);
        let r = app
            .ui_command("ui.documents", Some(&json!({ "op": "new" })))
            .expect("new");
        assert_eq!(r["tabs"].as_array().map(Vec::len), Some(2));
        assert_eq!(r["active"], json!(1));
        let r = app
            .ui_command(
                "ui.documents",
                Some(&json!({ "op": "activate", "index": 0 })),
            )
            .expect("activate");
        assert_eq!(r["tabs"][0]["unsaved"], json!(true));
        assert_eq!(app.session.design().objects.len(), 1);
        let e = app
            .ui_command("ui.documents", Some(&json!({ "op": "close", "index": 0 })))
            .expect_err("unsaved");
        assert!(e.contains("unsaved"), "{e}");
        assert_eq!(app.tab_count(), 2, "nothing was discarded");
        app.ui_command("ui.documents", Some(&json!({ "op": "close", "index": 1 })))
            .expect("close a clean tab");
        assert_eq!(app.tab_count(), 1);
        let list = app.ui_command("ui.documents", None).expect("list");
        assert_eq!(list["tabs"].as_array().map(Vec::len), Some(1));
        let state = app.ui_command("ui.state", None).expect("state");
        assert_eq!(state["documents"], json!(1));
    }
}
