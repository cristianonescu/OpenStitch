//! The editor's `ui.*` commands for the AI assistant (B17): what only the window holds, run on
//! the UI thread beside the engine commands. Specs, settings and routes are in
//! [`openstitch_app::editor::assistant`]; this file runs them. Clients reach them through the
//! automation socket (`remote.rs`) and, from phase A1, the Assistant panel.

use crate::app::EditorApp;
use base64::Engine as _;
use openstitch_app::editor::assistant::{
    self, HELP_ITEMS, MAX_SCRATCH_BYTES, Route, SETTINGS, UI_COMMANDS,
};
use openstitch_app::editor::prefs::Theme;
use openstitch_app::editor::ribbon::Action;
use openstitch_engine::{Capabilities, Effect, Params};
use serde_json::{Value, json};
use std::path::PathBuf;

/// The assistant's side of the editor: its scratch folder and a link waiting to open.
#[derive(Default)]
pub struct AssistantState {
    /// `ui.scratchWrite` writes here; the socket's clients may read and write it.
    pub scratch: Option<PathBuf>,
    /// A Help link `ui.help` asked for, opened on the next frame (it needs the egui context).
    pub open_url: Option<String>,
}

/// Scratch folders are `openstitch-assistant-<pid>` in the OS temp folder.
const SCRATCH_PREFIX: &str = "openstitch-assistant-";
/// Held locked (an OS advisory lock) for as long as the editor that made the folder runs.
const SCRATCH_LOCK: &str = ".lock";

/// The scratch folders this process made, with the locks that mark them in use.
static SCRATCH: std::sync::Mutex<Vec<(PathBuf, std::fs::File)>> = std::sync::Mutex::new(Vec::new());

/// A private scratch folder for this launch, created before the socket starts so its file
/// roots can include it. On Unix only the user can open it. Folders left behind by editors
/// that have ended (a crash, or a forced quit) are removed first (#314).
pub fn create_scratch() -> std::io::Result<PathBuf> {
    let temp = std::env::temp_dir();
    remove_stale_scratch(&temp);
    let dir = temp.join(format!("{SCRATCH_PREFIX}{}", std::process::id()));
    std::fs::create_dir_all(&dir)?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
    }
    let mut held = SCRATCH.lock().unwrap_or_else(|e| e.into_inner());
    if !held.iter().any(|(d, _)| *d == dir) {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(dir.join(SCRATCH_LOCK))?;
        lock.try_lock().map_err(std::io::Error::other)?;
        held.push((dir.clone(), lock));
    }
    Ok(dir)
}

/// Removes `openstitch-assistant-*` folders in `temp` whose lock nobody holds: the editor that
/// made them has ended. Folders without a lock file are left alone (they may belong to an
/// editor from before the lock existed that is still running). Returns how many were removed.
pub fn remove_stale_scratch(temp: &std::path::Path) -> usize {
    let own = format!("{SCRATCH_PREFIX}{}", std::process::id());
    let Ok(entries) = std::fs::read_dir(temp) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with(SCRATCH_PREFIX) || name == own || !entry.path().is_dir() {
            continue;
        }
        let Ok(lock) = std::fs::File::open(entry.path().join(SCRATCH_LOCK)) else {
            continue;
        };
        if lock.try_lock().is_ok() {
            drop(lock);
            if std::fs::remove_dir_all(entry.path()).is_ok() {
                removed += 1;
            }
        }
    }
    removed
}

/// Removes the scratch folders this process made; called when the editor closes normally.
pub fn remove_own_scratch() {
    let mut held = SCRATCH.lock().unwrap_or_else(|e| e.into_inner());
    for (dir, lock) in held.drain(..) {
        drop(lock);
        let _ = std::fs::remove_dir_all(dir);
    }
}

/// The engine's `commands.list` plus the editor's own commands.
pub fn commands_list(engine: Value) -> Value {
    let mut list = match engine {
        Value::Array(v) => v,
        _ => Vec::new(),
    };
    list.extend(UI_COMMANDS.iter().map(assistant::describe));
    list.sort_by(|a, b| a["id"].as_str().cmp(&b["id"].as_str()));
    Value::Array(list)
}

/// Whether `caps` allows a `ui.*` command.
pub fn permitted(effect: Effect, caps: Capabilities) -> bool {
    match effect {
        Effect::Read => true,
        Effect::Edit | Effect::History | Effect::Settings => caps.edit,
        Effect::File => caps.write_files,
    }
}

fn png_size(png: &[u8]) -> (u32, u32) {
    let be = |i: usize| {
        png.get(i..i + 4)
            .map_or(0, |b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]))
    };
    (be(16), be(20))
}

fn opt_str<'a>(params: Option<&'a Value>, key: &str) -> Option<&'a str> {
    params.and_then(|v| v.get(key)).and_then(Value::as_str)
}

fn bool_value(name: &str, v: &Value) -> Result<bool, String> {
    v.as_bool()
        .ok_or_else(|| format!("{name} takes true or false"))
}

fn number_value(name: &str, v: &Value, lo: f64, hi: f64) -> Result<f64, String> {
    v.as_f64()
        .filter(|x| x.is_finite() && (lo..=hi).contains(x))
        .ok_or_else(|| format!("{name} takes a number from {lo} to {hi}"))
}

impl EditorApp {
    /// Runs one `ui.*` command; `Err` is the message for the client.
    pub(crate) fn ui_command(&mut self, id: &str, params: Option<&Value>) -> Result<Value, String> {
        let spec = assistant::ui_command(id).ok_or_else(|| format!("unknown command '{id}'"))?;
        if !permitted(spec.effect, self.session.capabilities()) {
            return Err(format!("'{id}' is not permitted for this client"));
        }
        let p = Params::new(id, params).map_err(|e| e.to_string())?;
        let err = |e: openstitch_engine::CommandError| e.to_string();
        match id {
            "ui.state" => Ok(self.ui_state()),
            "ui.actions" => {
                let filter = opt_str(params, "filter").map(str::to_lowercase);
                let list: Vec<Value> = Action::ALL
                    .iter()
                    .filter(|a| {
                        let i = a.info();
                        filter.as_ref().is_none_or(|f| {
                            [i.key, i.label, i.tip]
                                .iter()
                                .any(|t| t.to_lowercase().contains(f))
                        })
                    })
                    .map(|&a| assistant::action_entry(a, crate::ribbon::enabled(self, a)))
                    .collect();
                Ok(Value::Array(list))
            }
            "ui.run" => {
                let key = p.string("action", None).map_err(err)?;
                self.ui_run(&key)
            }
            "ui.select" => {
                let mode = opt_str(params, "mode").unwrap_or("set");
                let ids: Vec<u64> = match params.and_then(|v| v.get("ids")) {
                    None => Vec::new(),
                    Some(v) => serde_json::from_value(v.clone())
                        .map_err(|_| "ids must be a list of object ids".to_string())?,
                };
                let known: std::collections::BTreeSet<u64> =
                    self.session.design().objects.iter().map(|o| o.id).collect();
                if let Some(bad) = ids.iter().find(|i| !known.contains(i)) {
                    return Err(format!("no object {bad}"));
                }
                match mode {
                    "set" => self.selection = ids.into_iter().collect(),
                    "add" => self.selection.extend(ids),
                    "remove" => self.selection.retain(|i| !ids.contains(i)),
                    "all" => self.select_all(),
                    "none" => self.selection.clear(),
                    _ => return Err("mode must be set, add, remove, all or none".into()),
                }
                Ok(json!({ "selection": self.selection }))
            }
            "ui.zoom" => {
                match (
                    opt_str(params, "fit"),
                    params.and_then(|v| v.get("percent")),
                ) {
                    (Some("page"), None) => self.fit_requested = true,
                    (Some("selection"), None) => crate::ribbon::zoom_to_selection(self),
                    (None, Some(v)) => {
                        let percent = number_value("percent", v, 5.0, 3200.0)?;
                        crate::ribbon::zoom_to_percent(self, percent);
                    }
                    _ => return Err("give either percent or fit (page or selection)".into()),
                }
                Ok(json!({ "zoomPercent": self.zoom_percent() }))
            }
            "ui.settings" => Ok(self.ui_settings()),
            "ui.set" => {
                let name = p.string("name", None).map_err(err)?;
                let name = name.as_str();
                let value = params
                    .and_then(|v| v.get("value"))
                    .ok_or("ui.set needs a value")?;
                self.ui_set(name, value)?;
                Ok(json!({ name: self.ui_settings()[name].clone() }))
            }
            "ui.render" => {
                let size = p.int("size", Some(800), 64, 2048).map_err(err)? as u32;
                let travel = p.bool("showTravel", Some(false)).map_err(err)?;
                let png = openstitch_render::preview::png(
                    self.session.design(),
                    &self.pattern,
                    size,
                    travel,
                )?;
                let (width, height) = png_size(&png);
                Ok(json!({
                    "png": base64::engine::general_purpose::STANDARD.encode(&png),
                    "width": width, "height": height, "stats": self.stats_json(),
                }))
            }
            "ui.simulate" => {
                let v = |k: &str| params.and_then(|p| p.get(k));
                if let Some(x) = v("speed") {
                    self.play_speed = number_value("speed", x, 1.0, 5000.0)?;
                }
                if let Some(x) = v("position") {
                    let f = number_value("position", x, 0.0, 1.0)?;
                    self.simulate =
                        (f < 1.0).then(|| (f * self.stats.stitches as f64).round() as usize);
                }
                if let Some(x) = v("play") {
                    self.playing = bool_value("play", x)?;
                    if self.playing && self.simulate.is_none() {
                        self.simulate = Some(0);
                    }
                }
                Ok(self.simulator_json())
            }
            "ui.help" => {
                let Some(item) = opt_str(params, "item") else {
                    return Ok(HELP_ITEMS
                        .iter()
                        .map(|(k, l)| json!({ "item": k, "label": l }))
                        .collect());
                };
                use openstitch_app::editor::sew_kit;
                match item {
                    "manual" => {
                        self.assistant.open_url =
                            Some("https://github.com/cristianonescu/OpenStitch#readme".into())
                    }
                    "sewKit" => {
                        let kit = std::env::current_exe()
                            .ok()
                            .and_then(|e| sew_kit::bundled(&e));
                        if kit.is_none_or(|k| sew_kit::open_folder(&k).is_err()) {
                            self.assistant.open_url = Some(sew_kit::ONLINE.into());
                        }
                    }
                    "sewKitOnline" => self.assistant.open_url = Some(sew_kit::ONLINE.into()),
                    "shortcuts" => self.ui.shortcuts_open = true,
                    "checkUpdates" => self.update.check(true),
                    "about" => self.ui.about_open = true,
                    _ => return Err(format!("unknown Help item '{item}'")),
                }
                Ok(json!({ "opened": item }))
            }
            "ui.update" => {
                self.update.check(true);
                Ok(json!({ "checking": true }))
            }
            "ui.scratchWrite" => self.scratch_write(&p, params),
            _ => Err(format!("unknown command '{id}'")),
        }
    }

    fn ui_run(&mut self, key: &str) -> Result<Value, String> {
        let a = Action::from_key(key)
            .ok_or_else(|| format!("no command '{key}'; ui.actions lists them"))?;
        let info = a.info();
        if !info.available {
            return Err(format!("{} is not available in OpenStitch yet", info.label));
        }
        match assistant::route(a) {
            Route::File(c) => {
                return Err(format!(
                    "{} opens a file dialog; run {c} with a path instead",
                    info.label
                ));
            }
            Route::Choices(c) => {
                return Err(format!(
                    "{} is a list of choices; run {c} instead",
                    info.label
                ));
            }
            Route::Run | Route::Tool(_) => {}
        }
        if !crate::ribbon::enabled(self, a) {
            return Err(format!(
                "{} cannot run now (check the selection with ui.state)",
                info.label
            ));
        }
        crate::ribbon::run(self, a, &eframe::egui::Context::default());
        Ok(json!({ "ran": info.key, "state": self.ui_state() }))
    }

    fn zoom_percent(&self) -> Option<f64> {
        self.viewport.map(|vp| vp.zoom_percent(96.0 / 25.4))
    }

    fn stats_json(&self) -> Value {
        let s = &self.stats;
        json!({
            "stitches": s.stitches, "jumps": s.jumps, "trims": s.trims,
            "colorChanges": s.color_changes, "widthMm": s.width_mm, "heightMm": s.height_mm,
            "colors": s.colors.iter().map(|c| format!("#{:02x}{:02x}{:02x}", c[0], c[1], c[2])).collect::<Vec<_>>(),
        })
    }

    fn simulator_json(&self) -> Value {
        json!({
            "stitch": self.simulate, "stitches": self.stats.stitches,
            "playing": self.playing, "speed": self.play_speed,
        })
    }

    pub(crate) fn ui_state(&self) -> Value {
        json!({
            "file": self.path.as_ref().map(|p| p.display().to_string()),
            "unsaved": self.is_dirty(),
            "selection": self.selection,
            "tool": format!("{:?}", self.tools.tool()),
            "zoomPercent": self.zoom_percent(),
            "stats": self.stats_json(),
            "simulator": self.simulator_json(),
            "canUndo": self.session.can_undo(), "canRedo": self.session.can_redo(),
            "settings": self.ui_settings(),
        })
    }

    fn ui_settings(&self) -> Value {
        let w = &self.workspace;
        let prefs = &self.ui.prefs;
        let view = if w.realistic {
            "realistic"
        } else if w.solid {
            "solid"
        } else {
            "stitch"
        };
        let mut out = serde_json::Map::new();
        for s in SETTINGS {
            let v = match s.name {
                "theme" => json!(prefs.theme.key()),
                "qatBelow" => json!(prefs.qat_below),
                "splash" => json!(prefs.splash),
                "checkUpdates" => json!(prefs.updates.check),
                "includePrereleases" => json!(prefs.updates.include_prereleases),
                "optimizeEntryExit" => json!(self.session.user_settings().optimize_entry_exit),
                "view" => json!(view),
                "showTravel" => json!(self.show_travel),
                "realisticThreadWidth" => json!(prefs.realistic.thread_width_mm),
                "realisticContrast" => json!(prefs.realistic.contrast),
                "realisticBrightness" => json!(prefs.realistic.brightness),
                "grid" => json!(w.grid.visible),
                "gridAxes" => json!(w.grid.axes),
                "gridSpacing" => json!(w.grid.spacing_mm),
                "gridMajorEvery" => json!(w.grid.major_every),
                "snapToGrid" => json!(w.grid.snap),
                "rulers" => json!(w.rulers),
                "guides" => json!(w.guides),
                "referenceWindow" => json!(w.reference),
                "paneSewingOrder" => json!(prefs.panes.sewing_order.visible),
                "paneSimulator" => json!(prefs.panes.simulator.visible),
                "paneAttributes" => json!(prefs.panes.attributes.visible),
                "fontFilter" => json!(prefs.font_filter.on),
                "fontFilterHidden" => json!(prefs.font_filter.hidden),
                "simulatorSpeed" => json!(self.play_speed),
                other => unreachable!("setting {other} has no value"),
            };
            out.insert(
                s.name.into(),
                json!({ "value": v, "type": s.kind, "label": s.label }),
            );
        }
        Value::Object(out)
    }

    fn ui_set(&mut self, name: &str, v: &Value) -> Result<(), String> {
        assistant::setting(name)
            .ok_or_else(|| format!("no setting '{name}'; ui.settings lists them"))?;
        let mut save = true;
        match name {
            "theme" => {
                let key = v.as_str().unwrap_or_default();
                self.ui.prefs.theme = Theme::ALL
                    .into_iter()
                    .find(|t| t.key() == key)
                    .ok_or("theme takes light, dark or system")?;
            }
            "qatBelow" => self.ui.prefs.qat_below = bool_value(name, v)?,
            "splash" => self.ui.prefs.splash = bool_value(name, v)?,
            "checkUpdates" => self.ui.prefs.updates.check = bool_value(name, v)?,
            "includePrereleases" => {
                self.ui.prefs.updates.include_prereleases = bool_value(name, v)?
            }
            "optimizeEntryExit" => {
                let on = bool_value(name, v)?;
                self.session
                    .execute("options.set", Some(&json!({ "optimizeEntryExit": on })))
                    .map_err(|e| e.to_string())?;
                save = false;
            }
            "view" => {
                let (r, s) = match v.as_str() {
                    Some("stitch") => (false, false),
                    Some("realistic") => (true, false),
                    Some("solid") => (false, true),
                    _ => return Err("view takes stitch, realistic or solid".into()),
                };
                (self.workspace.realistic, self.workspace.solid) = (r, s);
                save = false;
            }
            "showTravel" => (self.show_travel, save) = (bool_value(name, v)?, false),
            "realisticThreadWidth" => {
                self.ui.prefs.realistic.thread_width_mm = number_value(name, v, 0.1, 1.5)?
            }
            "realisticContrast" => {
                self.ui.prefs.realistic.contrast = number_value(name, v, -100.0, 100.0)?
            }
            "realisticBrightness" => {
                self.ui.prefs.realistic.brightness = number_value(name, v, -100.0, 100.0)?
            }
            "grid" => (self.workspace.grid.visible, save) = (bool_value(name, v)?, false),
            "gridAxes" => (self.workspace.grid.axes, save) = (bool_value(name, v)?, false),
            "gridSpacing" => {
                (self.workspace.grid.spacing_mm, save) = (number_value(name, v, 1.0, 100.0)?, false)
            }
            "gridMajorEvery" => {
                let n = number_value(name, v, 1.0, 50.0)?;
                if n.fract() != 0.0 {
                    return Err("gridMajorEvery takes a whole number".into());
                }
                (self.workspace.grid.major_every, save) = (n as u32, false);
            }
            "snapToGrid" => {
                if bool_value(name, v)? != self.workspace.grid.snap {
                    self.toggle_snap_to_grid();
                }
                save = false;
            }
            "rulers" => (self.workspace.rulers, save) = (bool_value(name, v)?, false),
            "guides" => {
                self.workspace.guides = bool_value(name, v)?;
                self.workspace.rulers |= self.workspace.guides;
                save = false;
            }
            "referenceWindow" => (self.workspace.reference, save) = (bool_value(name, v)?, false),
            "paneSewingOrder" => self.ui.prefs.panes.sewing_order.visible = bool_value(name, v)?,
            "paneSimulator" => self.ui.prefs.panes.simulator.visible = bool_value(name, v)?,
            "paneAttributes" => {
                self.ui.prefs.panes.attributes.visible = bool_value(name, v)?;
            }
            "fontFilter" => self.ui.prefs.font_filter.on = bool_value(name, v)?,
            "fontFilterHidden" => {
                self.ui.prefs.font_filter.hidden = serde_json::from_value(v.clone())
                    .map_err(|_| "fontFilterHidden takes a list of font ids".to_string())?
            }
            "simulatorSpeed" => {
                (self.play_speed, save) = (number_value(name, v, 1.0, 5000.0)?, false)
            }
            other => unreachable!("setting {other} cannot be set"),
        }
        if save {
            self.ui.save_prefs();
        }
        Ok(())
    }

    fn scratch_write(&mut self, p: &Params, params: Option<&Value>) -> Result<Value, String> {
        let dir = self
            .assistant
            .scratch
            .clone()
            .ok_or("this editor has no scratch folder (start it with --listen)")?;
        let name = p.string("name", None).map_err(|e| e.to_string())?;
        let name = name.as_str();
        if !assistant::scratch_name_ok(name) {
            return Err(
                "name must be a plain file name ending in .svg, .png, .jpg, .jpeg or .bmp".into(),
            );
        }
        let get = |k: &str| params.and_then(|v| v.get(k)).and_then(Value::as_str);
        let bytes = match (get("text"), get("base64")) {
            (Some(t), None) => t.as_bytes().to_vec(),
            (None, Some(b)) => {
                if b.len() > MAX_SCRATCH_BYTES / 3 * 4 + 4 {
                    return Err(format!("larger than {} bytes", MAX_SCRATCH_BYTES));
                }
                base64::engine::general_purpose::STANDARD
                    .decode(b.trim())
                    .map_err(|_| "base64 is not valid".to_string())?
            }
            _ => return Err("give either text or base64".into()),
        };
        if bytes.len() > MAX_SCRATCH_BYTES {
            return Err(format!("larger than {MAX_SCRATCH_BYTES} bytes"));
        }
        let path = dir.join(name);
        std::fs::write(&path, &bytes).map_err(|e| format!("cannot write {name}: {e}"))?;
        Ok(json!({ "path": path.display().to_string(), "bytes": bytes.len() }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use openstitch_app::editor::assistant::{FILE_ACTIONS, OPTION_BUTTONS};

    fn demo() -> EditorApp {
        EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        })
    }

    fn known_ids(app: &mut EditorApp) -> std::collections::BTreeSet<String> {
        let list = commands_list(
            app.session.handle(&json!({ "command": "commands.list" }))["result"].clone(),
        );
        list.as_array()
            .expect("list")
            .iter()
            .map(|c| c["id"].as_str().expect("id").to_string())
            .collect()
    }

    /// The coverage rule the owner set for B17: everything the user can do by hand has a way
    /// in from the chat. A new ribbon or menu command, Options or Help item, or engine command
    /// fails here until it has one.
    #[test]
    fn every_editor_feature_is_reachable_from_the_assistant() {
        let mut app = demo();
        let ids = known_ids(&mut app);
        // Every engine command and every ui.* command is listed.
        for c in app.session.commands() {
            assert!(ids.contains(c.id), "engine command {} is not listed", c.id);
        }
        for c in UI_COMMANDS {
            assert!(ids.contains(c.id), "{} is not listed", c.id);
        }
        // Every action's engine command and route command exist.
        for &a in Action::ALL {
            let info = a.info();
            if let Some(e) = info.engine {
                assert!(ids.contains(e), "{a:?} names {e}, which is not a command");
            }
            if let Some(c) = assistant::route(a).command() {
                assert!(
                    ids.contains(c),
                    "{a:?} routes to {c}, which is not a command"
                );
            }
        }
        // Every Options and Help menu entry the window draws has a setting or a command.
        let src = include_str!("ribbon.rs");
        for (func, labels) in [
            (
                "pub fn options_menu",
                SETTINGS
                    .iter()
                    .map(|s| s.label)
                    .chain(OPTION_BUTTONS.iter().map(|b| b.0))
                    .collect::<Vec<_>>(),
            ),
            ("pub fn help_menu", HELP_ITEMS.iter().map(|h| h.1).collect()),
        ] {
            let body = &src[src.find(func).expect(func)..];
            let body = &body[..body[1..].find("\npub fn ").map_or(body.len(), |e| e + 1)];
            for call in [".checkbox(", ".button("] {
                for (i, _) in body.match_indices(call) {
                    let rest = &body[i + call.len()..];
                    let Some(q) = rest.find('"') else { continue };
                    if rest[..q].contains(')') {
                        continue;
                    }
                    let label =
                        &rest[q + 1..q + 1 + rest[q + 1..].find('"').expect("closing quote")];
                    assert!(
                        labels.contains(&label),
                        "{func}: '{label}' has no assistant setting or command"
                    );
                }
            }
        }
    }

    #[test]
    fn every_run_action_runs_through_ui_run() {
        for &a in Action::ALL {
            let info = a.info();
            if !info.available || a == Action::Exit || a.is_menu() || FILE_ACTIONS.contains(&a) {
                continue;
            }
            for sel in [&[][..], &[1][..], &[1, 2][..]] {
                let mut app = demo();
                app.selection = sel.iter().copied().collect();
                let enabled = crate::ribbon::enabled(&app, a);
                let reply = app.ui_command("ui.run", Some(&json!({ "action": info.key })));
                assert_eq!(reply.is_ok(), enabled, "{a:?} with {sel:?}: {reply:?}");
            }
        }
    }

    #[test]
    fn file_dialogs_and_lists_name_the_command_to_use() {
        let mut app = demo();
        let e = app
            .ui_command("ui.run", Some(&json!({ "action": "open" })))
            .expect_err("open needs a path");
        assert!(e.contains("project.open"), "{e}");
        let e = app
            .ui_command("ui.run", Some(&json!({ "action": "help" })))
            .expect_err("help is a list");
        assert!(e.contains("ui.help"), "{e}");
        assert!(
            app.ui_command("ui.run", Some(&json!({ "action": "nope" })))
                .is_err()
        );
    }

    #[test]
    fn settings_read_and_write_every_entry() {
        let mut app = demo();
        let all = app.ui_command("ui.settings", None).expect("settings");
        for s in SETTINGS {
            let current = all[s.name]["value"].clone();
            assert!(!current.is_null(), "{}", s.name);
            // Writing the current value back is always accepted.
            app.ui_command("ui.set", Some(&json!({ "name": s.name, "value": current })))
                .unwrap_or_else(|e| panic!("{}: {e}", s.name));
        }
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "view", "value": "realistic" })),
        )
        .expect("view");
        assert!(app.workspace.realistic);
        assert!(
            app.ui_command(
                "ui.set",
                Some(&json!({ "name": "gridSpacing", "value": 0.0 }))
            )
            .is_err()
        );
        assert!(
            app.ui_command("ui.set", Some(&json!({ "name": "nope", "value": 1 })))
                .is_err()
        );
    }

    #[test]
    fn select_zoom_render_and_simulate() {
        let mut app = demo();
        let r = app
            .ui_command("ui.select", Some(&json!({ "ids": [1, 2] })))
            .expect("select");
        assert_eq!(r["selection"], json!([1, 2]));
        assert!(
            app.ui_command("ui.select", Some(&json!({ "ids": [999] })))
                .is_err()
        );
        app.ui_command("ui.select", Some(&json!({ "mode": "none" })))
            .expect("none");
        assert!(app.selection.is_empty());
        app.ui_command("ui.zoom", Some(&json!({ "fit": "page" })))
            .expect("fit");
        assert!(app.fit_requested);
        assert!(app.ui_command("ui.zoom", Some(&json!({}))).is_err());
        let png = app
            .ui_command("ui.render", Some(&json!({ "size": 200 })))
            .expect("render");
        assert!(png["width"].as_u64().is_some_and(|w| w > 0 && w <= 200));
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(png["png"].as_str().expect("png"))
            .expect("base64");
        assert!(bytes.starts_with(b"\x89PNG"));
        let s = app
            .ui_command("ui.simulate", Some(&json!({ "position": 0.5 })))
            .expect("simulate");
        assert!(s["stitch"].as_u64().is_some());
        app.ui_command("ui.simulate", Some(&json!({ "position": 1.0 })))
            .expect("end");
        assert!(app.simulate.is_none());
    }

    #[test]
    fn help_items_and_scratch_files() {
        let mut app = demo();
        let items = app.ui_command("ui.help", None).expect("list");
        assert_eq!(items.as_array().map(Vec::len), Some(HELP_ITEMS.len()));
        app.ui_command("ui.help", Some(&json!({ "item": "about" })))
            .expect("about");
        assert!(app.ui.about_open);
        app.ui_command("ui.help", Some(&json!({ "item": "manual" })))
            .expect("manual");
        assert!(app.assistant.open_url.is_some());

        assert!(
            app.ui_command(
                "ui.scratchWrite",
                Some(&json!({ "name": "a.svg", "text": "<svg/>" }))
            )
            .is_err(),
            "no scratch folder without --listen"
        );
        let dir =
            std::env::temp_dir().join(format!("openstitch-scratch-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("dir");
        app.assistant.scratch = Some(dir.clone());
        let r = app
            .ui_command(
                "ui.scratchWrite",
                Some(&json!({ "name": "a.svg", "text": "<svg/>" })),
            )
            .expect("write");
        assert_eq!(std::fs::read(dir.join("a.svg")).expect("read"), b"<svg/>");
        assert_eq!(r["bytes"], json!(6));
        let r = app
            .ui_command(
                "ui.scratchWrite",
                Some(&json!({ "name": "b.png", "base64": "iVBORw==" })),
            )
            .expect("write b64");
        assert_eq!(r["bytes"], json!(4));
        for bad in [
            json!({ "name": "../a.svg", "text": "x" }),
            json!({ "name": "a.exe", "text": "x" }),
            json!({ "name": "a.svg" }),
            json!({ "name": "a.svg", "text": "x", "base64": "eA==" }),
            json!({ "name": "a.png", "base64": "not base64!" }),
        ] {
            assert!(
                app.ui_command("ui.scratchWrite", Some(&bad)).is_err(),
                "{bad}"
            );
        }
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn scratch_folders_of_ended_editors_are_removed() {
        let temp =
            std::env::temp_dir().join(format!("openstitch-stale-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temp);
        let make = |name: &str, lock: bool| {
            let d = temp.join(name);
            std::fs::create_dir_all(&d).expect("dir");
            std::fs::write(d.join("a.svg"), "<svg/>").expect("file");
            if lock {
                std::fs::write(d.join(SCRATCH_LOCK), "").expect("lock");
            }
            d
        };
        let ended = make("openstitch-assistant-1", true);
        let running = make("openstitch-assistant-2", true);
        let no_lock = make("openstitch-assistant-3", false);
        let other = make("someone-else", true);
        // A live editor holds its lock.
        let held = std::fs::File::open(running.join(SCRATCH_LOCK)).expect("open");
        held.try_lock().expect("lock");
        assert_eq!(remove_stale_scratch(&temp), 1);
        assert!(!ended.exists(), "the ended editor's folder is removed");
        assert!(running.exists() && no_lock.exists() && other.exists());
        drop(held);
        assert_eq!(
            remove_stale_scratch(&temp),
            1,
            "once its editor ends, it goes too"
        );
        std::fs::remove_dir_all(&temp).expect("clean up");
    }

    #[test]
    fn read_only_clients_cannot_change_the_window_or_write() {
        let mut app = demo();
        app.session.set_capabilities(Capabilities::FULL.read_only());
        assert!(app.ui_command("ui.state", None).is_ok());
        assert!(
            app.ui_command("ui.run", Some(&json!({ "action": "showGrid" })))
                .is_err()
        );
        assert!(
            app.ui_command("ui.set", Some(&json!({ "name": "grid", "value": true })))
                .is_err()
        );
        assert!(
            app.ui_command(
                "ui.scratchWrite",
                Some(&json!({ "name": "a.svg", "text": "x" }))
            )
            .is_err()
        );
    }
}
