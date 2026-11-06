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
use openstitch_assistant::notes::{self, Notes, WriteMode};
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
    /// The Assistant panel: conversation, worker and questions (A1).
    pub panel: crate::assistant_panel::Panel,
    /// The Assistant's memory (#491), read from the config folder on first use.
    pub notes: Option<Notes>,
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
                    (None, None) if params.and_then(|v| v.get("rect")).is_some() => {
                        // Zoom Box (#518): [x, y, width, height] in mm fills the view.
                        let r: Vec<f64> = params
                            .and_then(|v| v.get("rect"))
                            .and_then(|v| serde_json::from_value(v.clone()).ok())
                            .filter(|r: &Vec<f64>| r.len() == 4 && r.iter().all(|v| v.is_finite()))
                            .ok_or("rect takes [x, y, width, height] in mm")?;
                        let rect = openstitch_design::Rect {
                            min_x: r[0] * 10.0,
                            min_y: r[1] * 10.0,
                            max_x: (r[0] + r[2]) * 10.0,
                            max_y: (r[1] + r[3]) * 10.0,
                        };
                        let canvas = self.workspace.canvas.ok_or("the canvas is not shown yet")?;
                        self.viewport = Some(
                            openstitch_app::editor::Viewport::zoom_box(
                                rect,
                                crate::workspace_ui::screen_rect(canvas),
                            )
                            .ok_or("rect needs a positive width and height")?,
                        );
                    }
                    (None, Some(v)) => {
                        let percent = number_value("percent", v, 5.0, 3200.0)?;
                        crate::ribbon::zoom_to_percent(self, percent);
                    }
                    _ => {
                        return Err("give one of percent, fit (page or selection) or rect".into());
                    }
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
                // What View ▸ Show shows (#517), each setting overridable here.
                use openstitch_render::preview;
                let defaults = preview::ShowArgs {
                    filter: self.view_filter(),
                    needle_points: self.ui.prefs.show.needle_points,
                    trim_marks: self.ui.prefs.show.trim_marks,
                };
                let show = preview::show_args(params.unwrap_or(&Value::Null), defaults)?;
                let png = preview::png_shown(
                    self.session.design(),
                    &self.pattern,
                    &self.quality.spans,
                    size,
                    travel,
                    &show,
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
                if let Some(x) = v("stitch") {
                    let n = number_value("stitch", x, 0.0, self.timeline.total as f64)?;
                    if n.fract() != 0.0 {
                        return Err("stitch takes a whole number".into());
                    }
                    self.simulate =
                        openstitch_app::editor::player::position(&self.timeline, n as usize);
                    self.playing = false;
                }
                if let Some(x) = v("step") {
                    let s = x
                        .as_str()
                        .and_then(openstitch_app::editor::player::Step::from_key)
                        .ok_or("step takes start, end, stitch, stitchBack, block, blockBack, object, objectBack or trim")?;
                    self.simulate =
                        openstitch_app::editor::player::step(&self.timeline, self.simulate, s);
                    self.playing = false;
                }
                for (key, flag) in [
                    ("auto", &mut self.stitch_player.auto),
                    ("pauseAtTrims", &mut self.stitch_player.pause),
                    ("followNeedle", &mut self.stitch_player.follow),
                ] {
                    if let Some(x) = v(key) {
                        *flag = bool_value(key, x)?;
                    }
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
            "ui.notesRead" => {
                self.memory_on()?;
                Ok(self.notes_json())
            }
            "ui.notesWrite" => {
                self.memory_on()?;
                let section = p.string("section", None).map_err(|e| e.to_string())?;
                let text = p.string("text", None).map_err(|e| e.to_string())?;
                let mode = match opt_str(params, "mode") {
                    None => WriteMode::Append,
                    Some(m) => WriteMode::from_key(m).ok_or("mode takes append or replace")?,
                };
                let mut next = self.notes().clone();
                next.write(&section, &text, mode)?;
                self.set_notes(next)?;
                Ok(self.notes_json())
            }
            "ui.searchCommands" => {
                let query = opt_str(params, "query").unwrap_or_default();
                let limit = p.int("limit", Some(20), 1, 500).map_err(err)? as usize;
                let hits =
                    openstitch_app::editor::palette::search(query, &self.ui.prefs.recent_commands);
                Ok(Value::Array(
                    hits.into_iter()
                        .take(limit)
                        .map(|a| {
                            let mut e = assistant::action_entry(a, crate::ribbon::enabled(self, a));
                            e["path"] = json!(openstitch_app::editor::palette::path(a));
                            e
                        })
                        .collect(),
                ))
            }
            "ui.recoveryList" => Ok(crate::recovery_ui::list(self)),
            "ui.recoveryRestore" => {
                let id = p.string("id", None).map_err(err)?;
                crate::recovery_ui::restore(self, &id)
            }
            "ui.recoveryDiscard" => {
                let all = p.bool("all", Some(false)).map_err(err)?;
                match (opt_str(params, "id"), all) {
                    (Some(id), false) => crate::recovery_ui::discard(self, Some(id)),
                    (None, true) => crate::recovery_ui::discard(self, None),
                    _ => Err("give id or all: true".into()),
                }
            }
            "ui.notesClear" => {
                self.set_notes(Notes::default())?;
                Ok(self.notes_json())
            }
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
        // The Assistant never reads or writes the system clipboard (#500): its Copy, Cut and
        // Paste use the window's own copy only.
        let system = std::mem::replace(&mut self.ui.system_clipboard, false);
        crate::ribbon::run(self, a, &eframe::egui::Context::default());
        self.ui.system_clipboard = system;
        Ok(json!({ "ran": info.key, "state": self.ui_state() }))
    }

    fn zoom_percent(&self) -> Option<f64> {
        let ppm = self.points_per_mm();
        self.viewport.map(|vp| vp.zoom_percent(ppm))
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
        let t = &self.timeline;
        let n = openstitch_app::editor::player::at(t, self.simulate);
        json!({
            "stitch": self.simulate, "stitches": self.stats.stitches,
            "playing": self.playing, "speed": self.play_speed,
            "block": t.block_at(n),
            "seconds": (t.seconds_at(n) * 10.0).round() / 10.0,
            "totalSeconds": (t.total_seconds() * 10.0).round() / 10.0,
            "auto": self.stitch_player.auto, "pauseAtTrims": self.stitch_player.pause,
            "followNeedle": self.stitch_player.follow,
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
            // The background image and its layer (#518): pictures never draw it.
            "background": self.session.design().background.as_ref().map(|b| {
                let layer = match b.layer {
                    openstitch_design::BackgroundLayer::Bottom => json!({ "position": "bottom" }),
                    openstitch_design::BackgroundLayer::Top => json!({ "position": "top" }),
                    openstitch_design::BackgroundLayer::AfterObject { after } => {
                        json!({ "position": "afterObject", "afterId": after })
                    }
                };
                json!({
                    "visible": b.visible, "opacity": b.opacity, "layer": layer,
                    "xMM": b.x / 10.0, "yMM": b.y / 10.0,
                    "widthMM": b.width / 10.0, "heightMM": b.height / 10.0,
                })
            }),
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
                "aiFeatures" => json!(prefs.ai.enabled),
                "assistantMemory" => json!(prefs.ai.memory),
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
                "snapToObjects" => json!(prefs.editing.snap_objects),
                "precisionBar" => json!(prefs.editing.precision_bar),
                "clickInsideOutlines" => json!(prefs.editing.click_inside),
                "units" => json!(openstitch_app::editor::units::key(prefs.units)),
                "nudgeStep" => json!(prefs.editing.nudge_mm),
                "nudgeByGrid" => json!(prefs.editing.nudge_grid),
                "alignTo" => json!(self.align_to),
                "realSizePointsPerMM" => json!(self.points_per_mm()),
                "smoothTolerance" => json!(crate::shape_tools_ui::smooth_tolerance(self)),
                "rulers" => json!(w.rulers),
                "guides" => json!(w.guides),
                "referenceWindow" => json!(w.reference),
                "paneSewingOrder" => json!(prefs.panes.sewing_order.visible),
                "paneSimulator" => json!(prefs.panes.simulator.visible),
                "paneAttributes" => json!(prefs.panes.attributes.visible),
                "fontFilter" => json!(prefs.font_filter.on),
                "fontFilterHidden" => json!(prefs.font_filter.hidden),
                "needlePoints" => json!(prefs.show.needle_points),
                "showLayers" => json!(prefs.show.layers.as_str()),
                "trimMarks" => json!(prefs.show.trim_marks),
                "hideUnselected" => json!(prefs.show.hide_unselected),
                "colorBy" => json!(prefs.show.color_by.as_str()),
                "soloColor" => json!(prefs.show.solo),
                "simulatorSpeed" => json!(self.play_speed),
                "autosave" => json!(prefs.autosave),
                "autosaveInterval" => json!(prefs.autosave_interval),
                "propertySearch" => json!(self.ui.property_search),
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
            "aiFeatures" => self.ui.prefs.ai.enabled = bool_value(name, v)?,
            "assistantMemory" => self.ui.prefs.ai.memory = bool_value(name, v)?,
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
            "snapToObjects" => self.ui.prefs.editing.snap_objects = bool_value(name, v)?,
            "precisionBar" => self.ui.prefs.editing.precision_bar = bool_value(name, v)?,
            "clickInsideOutlines" => self.ui.prefs.editing.click_inside = bool_value(name, v)?,
            "units" => {
                self.ui.prefs.units = v
                    .as_str()
                    .and_then(openstitch_app::editor::units::from_key)
                    .ok_or("units takes mm or inch")?;
                self.database.units = self.ui.prefs.units;
            }
            "nudgeStep" => self.ui.prefs.editing.nudge_mm = number_value(name, v, 0.1, 50.0)?,
            "nudgeByGrid" => self.ui.prefs.editing.nudge_grid = bool_value(name, v)?,
            "smoothTolerance" => {
                self.edit.smooth_tolerance = Some(number_value(name, v, 0.01, 10.0)?);
                save = false;
            }
            "alignTo" => {
                self.align_to = crate::arrange_ui::ALIGN_TO
                    .iter()
                    .find(|(k, _)| Some(*k) == v.as_str())
                    .map(|(k, _)| *k)
                    .ok_or("alignTo takes selection, key or page")?;
                save = false;
            }
            "realSizePointsPerMM" => {
                // #518: a ruler reading turned into points per mm; 0 goes back to 96 dpi.
                let k = v.as_f64().ok_or("realSizePointsPerMM takes a number")?;
                let key = self.monitor_key();
                if k == 0.0 {
                    self.ui.prefs.calibrate(&key, None);
                } else {
                    let k = openstitch_app::editor::viewport::calibrated(k, 1.0)
                        .ok_or("realSizePointsPerMM takes 1.8-24 points per mm, or 0 for 96 dpi")?;
                    self.ui.prefs.calibrate(&key, Some(k));
                }
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
            "needlePoints" => self.ui.prefs.show.needle_points = bool_value(name, v)?,
            "showLayers" => {
                self.ui.prefs.show.layers = v
                    .as_str()
                    .and_then(openstitch_design::view::Layers::parse)
                    .ok_or("showLayers takes both, underlay or top")?;
            }
            "trimMarks" => self.ui.prefs.show.trim_marks = bool_value(name, v)?,
            "hideUnselected" => self.ui.prefs.show.hide_unselected = bool_value(name, v)?,
            "colorBy" => {
                self.ui.prefs.show.color_by = v
                    .as_str()
                    .and_then(openstitch_design::view::ColorBy::parse)
                    .ok_or("colorBy takes thread, order or mono")?;
            }
            "soloColor" => self.ui.prefs.show.solo = bool_value(name, v)?,
            "simulatorSpeed" => {
                (self.play_speed, save) = (number_value(name, v, 1.0, 5000.0)?, false)
            }
            "autosave" => self.ui.prefs.autosave = bool_value(name, v)?,
            "autosaveInterval" => {
                use openstitch_app::editor::recovery::{MAX_INTERVAL, MIN_INTERVAL};
                self.ui.prefs.autosave_interval = number_value(name, v, MIN_INTERVAL, MAX_INTERVAL)?
            }
            "propertySearch" => {
                let text = v.as_str().ok_or("propertySearch takes a string")?;
                if text.chars().count() > 100 {
                    return Err("propertySearch takes at most 100 characters".into());
                }
                (self.ui.property_search, save) = (text.to_string(), false);
            }
            other => unreachable!("setting {other} cannot be set"),
        }
        if save {
            self.ui.save_prefs();
        }
        Ok(())
    }

    /// The notes file: beside `ui.json`; None when there is none (the tests, `--demo` without a
    /// config folder), and then the notes live only as long as the editor.
    pub fn notes_path(&self) -> Option<PathBuf> {
        Some(
            self.ui
                .prefs_path
                .as_ref()?
                .parent()?
                .join(notes::FILE_NAME),
        )
    }

    /// The Assistant's notes, read from their file on first use.
    pub fn notes(&mut self) -> &Notes {
        if self.assistant.notes.is_none() {
            let loaded = match self.notes_path() {
                Some(path) => Notes::load(&path).unwrap_or_else(|e| {
                    eprintln!("cannot read {}: {e}", path.display());
                    Notes::default()
                }),
                None => Notes::default(),
            };
            self.assistant.notes = Some(loaded);
        }
        self.assistant.notes.get_or_insert_with(Notes::default)
    }

    /// Keeps `next` as the notes and saves them. On a save error nothing changes.
    pub fn set_notes(&mut self, next: Notes) -> Result<(), String> {
        if let Some(path) = self.notes_path() {
            next.save(&path)
                .map_err(|e| format!("cannot save {}: {e}", path.display()))?;
        }
        self.assistant.notes = Some(next);
        Ok(())
    }

    fn memory_on(&self) -> Result<(), String> {
        if self.ui.prefs.ai.memory {
            Ok(())
        } else {
            Err("the user turned the Assistant's memory off (Assistant settings, Memory)".into())
        }
    }

    fn notes_json(&mut self) -> Value {
        let enabled = self.ui.prefs.ai.memory;
        let n = self.notes();
        json!({
            "enabled": enabled, "text": n.text(), "bytes": n.bytes(),
            "limit": notes::MAX_NOTES_BYTES, "sections": notes::SECTIONS,
        })
    }

    /// What goes after the instructions in a conversation's system prompt: the notes, when
    /// memory is on and there are any.
    pub fn memory_prompt(&mut self) -> Option<String> {
        if !self.ui.prefs.ai.memory {
            return None;
        }
        self.notes().prompt_block()
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

    #[test]
    fn realistic_view_hides_unsewn_outlines_unless_selected() {
        // A text path's guide circle sews nothing: the Realistic view shows only what is sewn
        // (#441), the other views still outline it so it can be found.
        let mut app = demo();
        for (command, params) in [
            (
                "shape.ellipse",
                json!({ "cx": 50, "cy": 50, "rx": 10, "outline": "Red" }),
            ),
            ("object.setLine", json!({ "ids": [4], "type": "none" })),
        ] {
            let reply = app
                .session
                .handle(&json!({ "command": command, "params": params }));
            assert_eq!(reply["ok"], true, "{command}: {reply}");
        }
        assert_eq!(crate::canvas::unsewn_outlines_shown(&app), vec![4]);
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "view", "value": "realistic" })),
        )
        .expect("view");
        assert!(crate::canvas::unsewn_outlines_shown(&app).is_empty());
        app.selection.insert(4);
        assert_eq!(crate::canvas::unsewn_outlines_shown(&app), vec![4]);
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
        // #517: "show me only the underlay".
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "showLayers", "value": "underlay" })),
        )
        .expect("layers");
        assert_eq!(
            app.ui.prefs.show.layers,
            openstitch_design::view::Layers::UnderlayOnly
        );
        // #518: real size from a ruler reading, and back to 96 dpi.
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "realSizePointsPerMM", "value": 4.5 })),
        )
        .expect("calibrate");
        assert_eq!(app.points_per_mm(), 4.5);
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "realSizePointsPerMM", "value": 0 })),
        )
        .expect("reset");
        assert_eq!(
            app.points_per_mm(),
            openstitch_app::editor::viewport::DEFAULT_POINTS_PER_MM
        );
        assert!(
            app.ui_command(
                "ui.set",
                Some(&json!({ "name": "colorBy", "value": "plaid" }))
            )
            .is_err()
        );
        assert!(
            app.ui_command(
                "ui.set",
                Some(&json!({ "name": "realSizePointsPerMM", "value": 900 }))
            )
            .is_err()
        );
        // #519: the Assistant switches the unit; the Design Database follows.
        app.ui_command("ui.set", Some(&json!({ "name": "units", "value": "inch" })))
            .expect("units");
        assert_eq!(
            app.ui.prefs.units,
            openstitch_app::editor::units::Units::Inch
        );
        assert_eq!(app.database.units, app.ui.prefs.units);
        assert!(
            app.ui_command("ui.set", Some(&json!({ "name": "units", "value": "cm" })))
                .is_err()
        );
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
    fn ui_render_shows_what_view_show_sets() {
        let mut app = demo();
        let render = |app: &mut EditorApp, extra: Value| -> String {
            let mut params = json!({ "size": 300 });
            for (k, v) in extra.as_object().expect("object") {
                params[k] = v.clone();
            }
            app.ui_command("ui.render", Some(&params)).expect("render")["png"]
                .as_str()
                .expect("png")
                .to_string()
        };
        let plain = render(&mut app, json!({}));
        // Settings made with ui.set change view_render's picture ("show me only the underlay").
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "showLayers", "value": "top" })),
        )
        .expect("layers");
        let top = render(&mut app, json!({}));
        assert_ne!(top, plain);
        assert_eq!(
            render(&mut app, json!({ "layers": "both" })),
            plain,
            "override"
        );
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "showLayers", "value": "both" })),
        )
        .expect("back");
        // Hide unselected and solo colour, through the editor's own settings.
        app.selection = [app.session.design().objects[0].id].into_iter().collect();
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "hideUnselected", "value": true })),
        )
        .expect("hide");
        let only = render(&mut app, json!({}));
        assert_ne!(only, plain);
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "hideUnselected", "value": false })),
        )
        .expect("unhide");
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "soloColor", "value": true })),
        )
        .expect("solo");
        let solo = render(&mut app, json!({}));
        assert_ne!(solo, plain);
        app.ui_command(
            "ui.set",
            Some(&json!({ "name": "soloColor", "value": false })),
        )
        .expect("no solo");
        for extra in [
            json!({ "needlePoints": true }),
            json!({ "trimMarks": true }),
            json!({ "colorBy": "mono" }),
        ] {
            assert_ne!(render(&mut app, extra.clone()), plain, "{extra}");
        }
        assert_eq!(render(&mut app, json!({})), plain);
        assert!(
            app.ui_command("ui.render", Some(&json!({ "layers": "inside" })))
                .is_err()
        );
    }

    #[test]
    fn the_player_steps_through_ui_simulate() {
        use openstitch_app::editor::player;
        let mut app = demo();
        let t = app.timeline.clone();
        assert!(t.total > 0 && !t.blocks.is_empty());
        let r = app
            .ui_command("ui.simulate", Some(&json!({ "stitch": 0 })))
            .expect("seek");
        assert_eq!(r["stitch"], json!(0));
        assert_eq!(
            r["totalSeconds"],
            json!((t.total_seconds() * 10.0).round() / 10.0)
        );
        app.ui_command("ui.simulate", Some(&json!({ "step": "block" })))
            .expect("block");
        assert_eq!(app.simulate, player::position(&t, t.next_block(0)));
        app.ui_command(
            "ui.simulate",
            Some(&json!({ "auto": true, "pauseAtTrims": true, "followNeedle": true })),
        )
        .expect("toggles");
        assert!(app.stitch_player.auto && app.stitch_player.pause && app.stitch_player.follow);
        assert!(
            app.ui_command("ui.simulate", Some(&json!({ "step": "sideways" })))
                .is_err()
        );
        assert!(
            app.ui_command("ui.simulate", Some(&json!({ "stitch": t.total + 1 })))
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
        // #518: Zoom Box through ui.zoom: the rectangle fills the canvas.
        app.workspace.canvas = Some(eframe::egui::Rect::from_min_size(
            eframe::egui::pos2(0.0, 0.0),
            eframe::egui::vec2(800.0, 600.0),
        ));
        let state = app.ui_command("ui.state", None).expect("state");
        assert!(
            state["background"].is_null(),
            "no image: {}",
            state["background"]
        );
        let before = app.viewport;
        app.ui_command("ui.zoom", Some(&json!({ "rect": [10, 10, 40, 20] })))
            .expect("zoom box");
        let vp = app.viewport.expect("viewport");
        assert_ne!(Some(vp), before);
        let (a, b) = (
            vp.to_screen(openstitch_design::v2(100.0, 100.0)),
            vp.to_screen(openstitch_design::v2(500.0, 300.0)),
        );
        assert!(((b.x - a.x) / 800.0 - 1.0).abs() < 0.02, "{a:?} {b:?}");
        for bad in [json!([10, 10, 40]), json!([10, 10, 0, 20]), json!("box")] {
            assert!(
                app.ui_command("ui.zoom", Some(&json!({ "rect": bad.clone() })))
                    .is_err(),
                "{bad}"
            );
        }
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
        let dir = openstitch_testutil::TempDir::new("scratch-test");
        app.assistant.scratch = Some(dir.to_path_buf());
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
        let temp = openstitch_testutil::TempDir::new("stale-test");
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
