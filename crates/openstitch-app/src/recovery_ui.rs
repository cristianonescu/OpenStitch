//! Autosave and recovery (#549): the window's snapshot slot and timer, the Recover Designs
//! window shown at start when a crashed window left snapshots, and the `ui.recovery*`
//! commands. The snapshot logic is in `openstitch_app::editor::recovery`. The desktop keeps
//! snapshots in the recovery folder; the browser, which has none, in the store the page
//! registers (#831, IndexedDB).

use crate::app::EditorApp;
use eframe::egui;
use openstitch_app::editor::CommandRequest;
use openstitch_app::editor::recovery::{self, Info, Orphan, Slot, Store, Timer};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

/// Windows started by this process, so each takes its own slot.
static WINDOWS: AtomicU32 = AtomicU32::new(0);

#[derive(Default)]
pub struct RecoveryState {
    /// The recovery folder (`None`: no settings folder, e.g. tests and screenshots).
    pub dir: Option<PathBuf>,
    /// The browser's snapshot store, used when there is no folder (#831).
    pub store: Option<Store>,
    slot: Option<Slot>,
    /// A snapshot of this window is in the store.
    stored: bool,
    timer: Timer,
    /// Snapshots left by windows that ended without closing.
    pub offer: Vec<Orphan>,
    /// The Recover Designs window is shown.
    pub open: bool,
    /// The design was restored from a snapshot of this design (shown in the title until saved).
    pub recovered_from: Option<String>,
    writing: Option<std::thread::JoinHandle<()>>,
}

impl RecoveryState {
    /// Uses `dir` and lists what crashed windows left; `ask` opens the window when there is any.
    pub fn start(&mut self, dir: PathBuf, ask: bool) {
        self.offer = recovery::orphans(&dir);
        self.open = ask && !self.offer.is_empty();
        self.dir = Some(dir);
    }

    /// Uses the browser's `store` (#831) and lists what closed or crashed tabs left; `ask`
    /// opens the window when there is any.
    pub fn start_store(&mut self, store: Store, ask: bool) {
        self.offer = store.orphans();
        self.open = ask && !self.offer.is_empty();
        self.store = Some(store);
    }

    /// Whether autosave has a place to write (a folder or a store).
    fn active(&self) -> bool {
        self.dir.is_some() || self.store.is_some()
    }

    fn refresh(&mut self) {
        if let Some(dir) = &self.dir {
            self.offer = recovery::orphans(dir);
        } else if let Some(store) = &self.store {
            self.offer = store.orphans();
        }
    }

    /// Whether this window has a snapshot on disk or in the store.
    #[cfg(test)]
    pub fn has_snapshot(&self) -> bool {
        self.stored || self.slot.as_ref().is_some_and(Slot::exists)
    }

    /// Waits for a snapshot being written.
    #[cfg(test)]
    pub fn wait(&mut self) {
        if let Some(h) = self.writing.take() {
            let _ = h.join();
        }
    }
}

fn now_seconds() -> u64 {
    openstitch_app::platform::SystemTime::now()
        .duration_since(openstitch_app::platform::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Called every frame with the time in seconds: writes a snapshot when one is due, removes
/// it when the design is saved or unchanged or autosave is off. Returns whether to come back
/// soon (a snapshot is pending).
pub fn tick(app: &mut EditorApp, now: f64) -> bool {
    if !app.recovery.active() {
        return false;
    }
    let busy = app
        .recovery
        .writing
        .as_ref()
        .is_some_and(|h| !h.is_finished());
    if !app.ui.prefs.autosave || !app.is_dirty() {
        if let Some(s) = &app.recovery.slot
            && !busy
            && s.exists()
        {
            s.clear();
        }
        if app.recovery.stored
            && let Some(store) = app.recovery.store
        {
            store.clear();
            app.recovery.stored = false;
        }
        app.recovery.timer.written(app.snapshot_key());
        return busy;
    }
    if busy
        || !app
            .recovery
            .timer
            .due(app.snapshot_key(), now, app.ui.prefs.autosave_interval)
    {
        return true;
    }
    if let Err(e) = write_now(app, false) {
        app.status = openstitch_app::tf!("ui-status-autosave-failed", e = e);
    }
    false
}

/// Writes this window's snapshot: on a background thread, or now when `wait`; to the browser's
/// store (#831) always now, as the page has one thread.
pub fn write_now(app: &mut EditorApp, wait: bool) -> std::io::Result<()> {
    if app.recovery.dir.is_none()
        && let Some(store) = app.recovery.store
    {
        app.recovery.timer.written(app.snapshot_key());
        crate::assistant_panel::chat_to_session(app);
        let bytes = openstitch_engine::project::encode_with_chat(
            app.session.design(),
            app.session.chat(),
        )
        .map_err(|e| std::io::Error::other(e.to_string()))?;
        store.write(&bytes, &snapshot_info(app));
        app.recovery.stored = true;
        return Ok(());
    }
    let Some(dir) = app.recovery.dir.clone() else {
        return Ok(());
    };
    // The timer counts from here, also when the write fails (no retry every frame).
    app.recovery.timer.written(app.snapshot_key());
    if app.recovery.slot.is_none() {
        let n = WINDOWS.fetch_add(1, Ordering::Relaxed);
        app.recovery.slot = Some(Slot::take(&dir, n)?);
    }
    let id = app
        .recovery
        .slot
        .as_ref()
        .map(|s| s.id().to_string())
        .unwrap_or_default();
    crate::assistant_panel::chat_to_session(app);
    let design = app.session.design().clone();
    let chat = app.session.chat().cloned();
    let info = snapshot_info(app);
    let job = move || match openstitch_engine::project::encode_with_chat(&design, chat.as_ref()) {
        Ok(bytes) => recovery::write_snapshot(&dir, &id, &bytes, &info),
        Err(e) => Err(std::io::Error::other(e.to_string())),
    };
    if wait {
        return job();
    }
    app.recovery.writing = Some(std::thread::spawn(move || {
        if let Err(e) = job() {
            eprintln!("autosave: {e}");
        }
    }));
    Ok(())
}

/// What this window's snapshot is of.
fn snapshot_info(app: &EditorApp) -> Info {
    Info {
        original: app.path.clone(),
        name: app.document_name(),
        time: now_seconds(),
        // A browser tab has no process id (std's panics there): 0, as it is only shown.
        process: if cfg!(target_arch = "wasm32") {
            0
        } else {
            std::process::id()
        },
    }
}

/// Where a snapshot from the browser's store is put to be opened (#831): the page's memory in
/// the browser, the system's temporary folder in a native test.
fn staged(id: &str) -> PathBuf {
    if cfg!(target_arch = "wasm32") {
        PathBuf::from(format!("/web/recovery/{id}.osd"))
    } else {
        std::env::temp_dir().join(format!("openstitch-recovery-{}-{id}.osd", std::process::id()))
    }
}

/// Drops a staged snapshot once it has opened.
fn unstage(path: &std::path::Path) {
    #[cfg(target_arch = "wasm32")]
    let _ = openstitch_engine::fsio::memfs::take(path);
    #[cfg(not(target_arch = "wasm32"))]
    let _ = std::fs::remove_file(path);
}

/// Restore: the snapshot opens as an untitled design (the original file is never written),
/// and its files go once this window has its own snapshot of it.
pub fn restore(app: &mut EditorApp, id: &str) -> Result<Value, String> {
    if !app.recovery.active() {
        return Err("recovery is off in this window".into());
    }
    app.recovery.refresh();
    let orphan = app
        .recovery
        .offer
        .iter()
        .find(|o| o.id == id)
        .cloned()
        .ok_or_else(|| format!("no recovery file '{id}'; ui.recoveryList lists them"))?;
    if app.is_dirty() {
        return Err("the open design has unsaved changes; save or close it first".into());
    }
    let (path, staged_copy) = match (&app.recovery.dir, app.recovery.store) {
        (Some(dir), _) => (dir.join(format!("{id}.osd")), false),
        (None, Some(store)) => {
            let bytes = store.read(id).map_err(|e| e.to_string())?;
            let path = staged(id);
            openstitch_app::platform::write_file(&path, bytes)?;
            (path, true)
        }
        (None, None) => return Err("recovery is off in this window".into()),
    };
    let r = CommandRequest::new(
        "project.open",
        json!({ "path": path.display().to_string() }),
    );
    let opened = app.execute(&r).is_some();
    if staged_copy {
        unstage(&path);
    }
    if !opened {
        return Err(app.status.clone());
    }
    app.after_load(None);
    app.mark_unsaved();
    app.recovery.recovered_from = Some(orphan.name.clone());
    if app.ui.prefs.autosave {
        write_now(app, true).map_err(|e| e.to_string())?;
    }
    discard_one(app, id)?;
    app.recovery.refresh();
    app.recovery.open &= !app.recovery.offer.is_empty();
    app.status = openstitch_app::tf!("ui-status-recovered-untitled-until-you-save-it", name = orphan.name);
    Ok(json!({ "restored": id, "name": orphan.name }))
}

/// Discard one snapshot left by a crashed window, or all of them (`None`).
pub fn discard(app: &mut EditorApp, id: Option<&str>) -> Result<Value, String> {
    if !app.recovery.active() {
        return Err("recovery is off in this window".into());
    }
    app.recovery.refresh();
    let ids: Vec<String> = match id {
        Some(id) => vec![id.to_string()],
        None => app.recovery.offer.iter().map(|o| o.id.clone()).collect(),
    };
    for id in &ids {
        discard_one(app, id).map_err(|e| format!("{id}: {e}"))?;
    }
    app.recovery.refresh();
    app.recovery.open &= !app.recovery.offer.is_empty();
    Ok(json!({ "discarded": ids }))
}

/// Removes one snapshot an ended window left, from the folder or the store.
fn discard_one(app: &EditorApp, id: &str) -> Result<(), String> {
    match (&app.recovery.dir, app.recovery.store) {
        (Some(dir), _) => recovery::discard(dir, id),
        (None, Some(store)) => store.discard(id),
        (None, None) => Err(std::io::Error::other("recovery is off in this window")),
    }
    .map_err(|e| e.to_string())
}

/// `ui.recoveryList`: the snapshots crashed windows left.
pub fn list(app: &mut EditorApp) -> Value {
    app.recovery.refresh();
    json!({
        "autosave": app.ui.prefs.autosave,
        "autosaveInterval": app.ui.prefs.autosave_interval,
        "snapshots": app.recovery.offer.iter().map(|o| json!({
            "id": o.id, "name": o.name, "time": o.time,
            "original": o.original.as_ref().map(|p| p.display().to_string()),
        })).collect::<Vec<_>>(),
    })
}

/// The Recover Designs window: Restore, Discard, or Later (the files stay for next time).
pub fn window(app: &mut EditorApp, ctx: &egui::Context) {
    if !app.recovery.open {
        return;
    }
    let mut action: Option<(String, bool)> = None;
    let mut later = false;
    crate::fit::show(egui::Window::new("Recover Designs")
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
        , ctx, |ui| {
            ui.label(
                "OpenStitch closed without saving these designs. Restore opens one as an untitled design; the original file is not changed.",
            );
            ui.separator();
            for o in &app.recovery.offer {
                ui.horizontal(|ui| {
                    ui.label(o.title());
                    if ui.button("Restore").clicked() {
                        action = Some((o.id.clone(), true));
                    }
                    if ui.button("Discard").clicked() {
                        action = Some((o.id.clone(), false));
                    }
                });
            }
            ui.separator();
            let asked = crate::fit::buttons(ui, |ui| {
                ui.button("Later")
                    .on_hover_text("Ask again at the next start")
                    .clicked()
            });
            if asked {
                later = true;
            }
        });
    if later {
        app.recovery.open = false;
    }
    if let Some((id, restore_it)) = action {
        let r = if restore_it {
            restore(app, &id)
        } else {
            discard(app, Some(&id))
        };
        if let Err(e) = r {
            app.status = e;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;

    fn window(dir: &std::path::Path) -> EditorApp {
        let mut app = EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        });
        app.recovery.start(dir.to_path_buf(), true);
        app
    }

    fn edit(app: &mut EditorApp) {
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 5, "y": 5, "width": 10, "height": 10 }),
        ))
        .expect("edit");
    }

    #[test]
    fn a_snapshot_after_idle_restores_untitled_and_goes_on_close() {
        let tmp = openstitch_testutil::TempDir::new("recovery-ui");
        let dir = tmp.path().join(recovery::FOLDER);
        let mut a = window(&dir);
        assert!(!a.recovery.open, "nothing to recover");
        edit(&mut a);
        assert!(tick(&mut a, 100.0), "waits for 3 s without a change");
        assert!(!a.recovery.has_snapshot());
        tick(&mut a, 103.5);
        a.recovery.wait();
        assert!(a.recovery.has_snapshot());
        // A second window of this process: A is running, so nothing is offered.
        let b = window(&dir);
        assert!(!b.recovery.open && b.recovery.offer.is_empty());
        drop(b);
        // A crash leaves the files and frees the lock: copy A's snapshot under a dead id.
        let id = a.recovery.slot.as_ref().expect("slot").id().to_string();
        for ext in ["osd", "json", "lock"] {
            std::fs::copy(
                dir.join(format!("{id}.{ext}")),
                dir.join(format!("4000000000-7.{ext}")),
            )
            .expect("copy");
        }
        let expected = a.session.design().clone();
        // A normal close removes A's own files.
        drop(a);
        assert!(!dir.join(format!("{id}.osd")).exists());
        assert!(!dir.join(format!("{id}.lock")).exists());
        // The next start offers the crashed window's design.
        let mut c = window(&dir);
        assert!(c.recovery.open);
        assert_eq!(list(&mut c)["snapshots"][0]["id"], "4000000000-7");
        restore(&mut c, "4000000000-7").expect("restore");
        assert_eq!(
            c.session.design().objects,
            expected.objects,
            "object for object"
        );
        assert!(c.path.is_none() && c.is_dirty(), "untitled until saved");
        assert!(c.document_name().contains("recovered from"));
        assert!(!c.recovery.open && c.recovery.offer.is_empty());
        assert!(!dir.join("4000000000-7.osd").exists());
        // Its own snapshot keeps the restored design safe meanwhile.
        assert!(c.recovery.has_snapshot());
        // With unsaved changes, a restore is refused.
        assert!(restore(&mut c, "4000000000-7").is_err());
    }

    /// #1169: a recovery snapshot keeps the conversation as a save does, and a restore shows it.
    #[test]
    fn a_recovered_design_brings_its_chat_back() {
        use crate::assistant_panel::Item;
        let tmp = openstitch_testutil::TempDir::new("recovery-chat");
        let dir = tmp.path().join(recovery::FOLDER);
        let mut a = window(&dir);
        edit(&mut a);
        let owner = a.chat_doc;
        a.assistant.panel.owner = Some(owner);
        a.assistant.panel.items = vec![
            Item::User("make a box".into()),
            Item::Assistant("A box.".into()),
        ];
        write_now(&mut a, true).expect("snapshot");
        let own = a.recovery.slot.as_ref().expect("slot").id().to_string();
        crashed(&dir, &own, "4000000000-3");
        drop(a);
        let mut c = window(&dir);
        restore(&mut c, "4000000000-3").expect("restore");
        let items = &c.assistant.panel.items;
        assert_eq!(items[0], Item::User("make a box".into()));
        assert_eq!(items[1], Item::Assistant("A box.".into()));
    }

    /// A crashed window's files under a dead process id, copied from `from`'s snapshot.
    fn crashed(dir: &std::path::Path, from: &str, id: &str) {
        for ext in ["osd", "json", "lock"] {
            std::fs::copy(
                dir.join(format!("{from}.{ext}")),
                dir.join(format!("{id}.{ext}")),
            )
            .expect("copy");
        }
    }

    #[test]
    fn discard_through_the_command_deletes_only_what_crashed() {
        let tmp = openstitch_testutil::TempDir::new("recovery-discard");
        let dir = tmp.path().join(recovery::FOLDER);
        let mut a = window(&dir);
        edit(&mut a);
        write_now(&mut a, true).expect("snapshot");
        let own = a.recovery.slot.as_ref().expect("slot").id().to_string();
        crashed(&dir, &own, "4000000000-1");
        crashed(&dir, &own, "4000000000-2");
        // One by id: its files go, the other's stay.
        let r = a
            .ui_command("ui.recoveryDiscard", Some(&json!({ "id": "4000000000-1" })))
            .expect("discard");
        assert_eq!(r["discarded"], json!(["4000000000-1"]));
        for ext in ["osd", "json", "lock"] {
            assert!(!dir.join(format!("4000000000-1.{ext}")).exists(), "{ext}");
        }
        assert!(dir.join("4000000000-2.osd").exists());
        // A running window's snapshot is refused and kept.
        let e = a
            .ui_command("ui.recoveryDiscard", Some(&json!({ "id": own })))
            .expect_err("running");
        assert!(e.contains("still open"), "{e}");
        assert!(dir.join(format!("{own}.osd")).exists());
        // Not a path.
        assert!(
            a.ui_command("ui.recoveryDiscard", Some(&json!({ "id": "../ui" })))
                .is_err()
        );
        // All: every crashed copy goes, this window's own stays.
        let r = a
            .ui_command("ui.recoveryDiscard", Some(&json!({ "all": true })))
            .expect("all");
        assert_eq!(r["discarded"], json!(["4000000000-2"]));
        assert!(!dir.join("4000000000-2.osd").exists());
        assert!(dir.join(format!("{own}.osd")).exists());
        assert!(
            list(&mut a)["snapshots"]
                .as_array()
                .is_some_and(Vec::is_empty)
        );
        // Restore through the command as well.
        crashed(&dir, &own, "4000000000-3");
        let mut b = window(&dir);
        b.ui_command("ui.recoveryRestore", Some(&json!({ "id": "4000000000-3" })))
            .expect("restore");
        assert_eq!(b.session.design().objects, a.session.design().objects);
        assert!(!dir.join("4000000000-3.osd").exists());
    }

    thread_local! {
        /// The browser's IndexedDB in these tests: id, info JSON, project bytes.
        static MEM: std::cell::RefCell<Vec<(String, String, Vec<u8>)>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    fn ids_in_store() -> Vec<String> {
        MEM.with(|m| m.borrow().iter().map(|e| e.0.clone()).collect())
    }

    /// A tab whose snapshot id is `own`, on the shared in-memory store.
    fn store(own: fn() -> String) -> Store {
        Store {
            own,
            list: || MEM.with(|m| m.borrow().iter().map(|e| (e.0.clone(), e.1.clone())).collect()),
            read: |id| MEM.with(|m| m.borrow().iter().find(|e| e.0 == id).map(|e| e.2.clone())),
            write: |id, osd, info| {
                MEM.with(|m| {
                    let mut m = m.borrow_mut();
                    m.retain(|e| e.0 != id);
                    m.push((id.into(), info.into(), osd.to_vec()));
                });
            },
            remove: |id| MEM.with(|m| m.borrow_mut().retain(|e| e.0 != id)),
        }
    }

    fn tab(own: fn() -> String) -> EditorApp {
        let mut app = EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        });
        app.recovery.start_store(store(own), true);
        app
    }

    /// #831: in the browser (no recovery folder) a tab's snapshot goes to the page's store
    /// after 3 s without a change, leaves it when autosave is turned off, and the next tab
    /// offers what a closed tab left and restores it untitled; the closed tab's copy then goes.
    #[test]
    fn the_browser_store_keeps_a_closed_tabs_design() {
        let mut a = tab(|| "1000-0".into());
        assert!(!a.recovery.open, "nothing to recover");
        edit(&mut a);
        assert!(tick(&mut a, 100.0));
        assert!(ids_in_store().is_empty(), "waits for 3 s without a change");
        tick(&mut a, 103.5);
        assert!(a.recovery.has_snapshot());
        assert_eq!(ids_in_store(), ["1000-0"]);
        // Autosave off: the tab's copy goes; back on, it is kept again.
        a.ui.prefs.autosave = false;
        tick(&mut a, 104.0);
        assert!(ids_in_store().is_empty());
        a.ui.prefs.autosave = true;
        edit(&mut a);
        tick(&mut a, 200.0);
        tick(&mut a, 204.0);
        assert_eq!(ids_in_store(), ["1000-0"]);
        // Tab A is closed (its lock is gone, so the page lists its record); tab B opens.
        let expected = a.session.design().clone();
        drop(a);
        let mut b = tab(|| "2000-0".into());
        assert!(b.recovery.open);
        assert_eq!(list(&mut b)["snapshots"][0]["id"], "1000-0");
        // Its own copy is never offered, nor deleted through the command.
        assert!(
            b.ui_command("ui.recoveryDiscard", Some(&json!({ "id": "2000-0" })))
                .is_err()
        );
        b.ui_command("ui.recoveryRestore", Some(&json!({ "id": "1000-0" })))
            .expect("restore");
        assert_eq!(b.session.design().objects, expected.objects);
        assert!(b.path.is_none() && b.is_dirty(), "untitled until saved");
        assert!(!b.recovery.open && b.recovery.offer.is_empty());
        assert_eq!(ids_in_store(), ["2000-0"], "A's copy gone, B's own kept");
        assert!(!staged("1000-0").exists(), "the staged copy is removed");
    }

    #[test]
    fn autosave_off_writes_nothing() {
        let tmp = openstitch_testutil::TempDir::new("recovery-off");
        let dir = tmp.path().join(recovery::FOLDER);
        let mut a = window(&dir);
        a.ui.prefs.autosave = false;
        edit(&mut a);
        for t in [0.0, 10.0, 100.0] {
            tick(&mut a, t);
        }
        a.recovery.wait();
        assert!(!a.recovery.has_snapshot());
        assert!(std::fs::read_dir(&dir).map_or(true, |mut d| d.next().is_none()));
    }
}
