//! The web editor's document: an engine [`Session`] plus the stitch simulator, driven by the
//! same commands as the desktop editor. Target-independent, so the native tests cover what the
//! browser runs.
//!
//! Files: the engine's commands read and write paths. In the browser those paths are the
//! engine's in-memory files ([`openstitch_engine::fsio::memfs`]); natively they are a real
//! folder (the tests use a temp folder). A picked file is stored, opened and removed again;
//! an exported or saved file is written, taken and handed to the browser as a download.

use base64::Engine as _;
use openstitch_core::{Command, StitchPattern};
use openstitch_design::compile::Compiler;
use openstitch_engine::Session;
use openstitch_formats::FileType;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// The largest file the editor opens: the engine's own limit for embroidery files.
pub const MAX_FILE_BYTES: usize = 64 << 20;

/// What the browser cannot do yet, shown in the editor and returned by `ui.state` (#556).
pub const DESKTOP_ONLY: &str = "Desktop only: the Assistant (it keeps its key in the system keychain), \
Quick Look, sending to a machine or USB stick, updates, the Design Database, image digitizing, \
lettering and drawing tools. Install the desktop app for these.";

/// One drawn piece of the stitch-out, in 0.1 mm page units.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Segment {
    pub from: [f32; 2],
    pub to: [f32; 2],
    pub rgb: [u8; 3],
}

/// One downloaded file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OutFile {
    pub name: String,
    pub bytes: Vec<u8>,
}

pub struct WebSession {
    session: Session,
    dir: PathBuf,
    /// The open file's name (as picked), for the window title and the default export name.
    pub file_name: Option<String>,
    /// The last message for the status line.
    pub status: String,
    /// Stitch records shown by the simulator (`pattern.stitches[..shown]`).
    shown: usize,
    pattern: StitchPattern,
    /// Bumped by every change of the design, so the editor knows to redraw.
    pub revision: u64,
    /// Engine undo steps per Undo, newest last: File ▸ Open is one Undo of its two commands,
    /// as in the desktop editor.
    undo_groups: Vec<usize>,
    redo_groups: Vec<usize>,
}

impl WebSession {
    /// A session whose files live in `dir`: `/web` in the browser, a real folder natively.
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        // Absolute, as the engine resolves relative paths against this same folder.
        let dir = dir.into();
        let dir = std::path::absolute(&dir).unwrap_or(dir);
        let mut s = Self {
            session: Session::new(&dir),
            dir,
            file_name: None,
            status: "Open a design (.osd) or an embroidery file to start.".into(),
            shown: 0,
            pattern: StitchPattern::new(),
            revision: 0,
            undo_groups: Vec::new(),
            redo_groups: Vec::new(),
        };
        s.recompile(true);
        s
    }

    pub fn session(&self) -> &Session {
        &self.session
    }

    pub fn pattern(&self) -> &StitchPattern {
        &self.pattern
    }

    pub fn page_mm(&self) -> (f64, f64) {
        let p = self.session.design().page;
        (p.width / 10.0, p.height / 10.0)
    }

    fn recompile(&mut self, show_all: bool) {
        let design = self.session.design();
        self.pattern = Compiler::for_design(design).compile(design);
        if show_all || self.shown > self.pattern.stitches.len() {
            self.shown = self.pattern.stitches.len();
        }
        self.revision += 1;
    }

    /// Opens a picked file: `.osd` as the design; an embroidery file as a new design holding it
    /// as an imported stitch pattern, on a page large enough for it, exactly as the desktop
    /// editor's File ▸ Open (`design.new` then `stitches.import {keepSource}`), undone as one
    /// step. Fails, leaving the open design as it was, on a file it cannot read.
    pub fn open(&mut self, name: &str, bytes: Vec<u8>) -> Result<String, String> {
        let name = plain_name(name)?;
        if bytes.len() > MAX_FILE_BYTES {
            return Err(format!("{name} is larger than 64 MB"));
        }
        let path = self.dir.join(&name);
        let path_str = path.display().to_string();
        let steps = if is_project(&path) {
            vec![("project.open", json!({ "path": path_str }))]
        } else {
            let ty = FileType::from_path(&path)
                .ok_or_else(|| format!("{name} is not an OpenStitch design or embroidery file"))?;
            let pattern = ty.read(&bytes).map_err(|e| format!("{name}: {e}"))?;
            let b = pattern.bounds();
            let page = |v: f64| ((v + 10.0) / 10.0).ceil().max(10.0) * 10.0;
            let (w, h) = (b.width() / 10.0, b.height() / 10.0);
            // A PE-DESIGN 11 file keeps its own Design Page size.
            let (page_w, page_h) = (ty == FileType::Pes)
                .then(|| openstitch_formats::pes::read_objects(&bytes).ok())
                .flatten()
                .and_then(|s| s.page_mm.filter(|_| s.version == "0110"))
                .map_or((page(w), page(h)), |(w, h)| (f64::from(w), f64::from(h)));
            let stem = Path::new(&name)
                .file_stem()
                .map_or_else(|| "Untitled".into(), |s| s.to_string_lossy().into_owned());
            vec![
                (
                    "design.new",
                    json!({ "name": stem, "width": page_w, "height": page_h }),
                ),
                (
                    "stitches.import",
                    json!({ "path": path_str, "keepSource": true }),
                ),
            ]
        };
        store::put(&path, bytes)?;
        // An edit command that changes the design is one engine undo step.
        let (mut result, mut steps_done) = (Ok(()), 0);
        for (id, p) in &steps {
            let before = self.session.design().clone();
            if let Err(e) = self.session.execute(id, Some(p)) {
                result = Err(e.to_string());
                break;
            }
            steps_done += usize::from(*self.session.design() != before);
        }
        let _ = store::take(&path);
        if let Err(e) = result {
            // Back to the design that was open; the steps undone are never redone.
            for _ in 0..steps_done {
                let _ = self.session.execute("history.undo", None);
            }
            self.redo_groups.clear();
            return Err(e);
        }
        self.push_group(steps_done);
        self.file_name = Some(name.clone());
        self.recompile(true);
        self.status = if is_project(&path) {
            format!("Opened {name}")
        } else {
            format!("Opened {name} as a new design (Save to keep it as .osd)")
        };
        Ok(self.status.clone())
    }

    /// The name downloads use: the design's name, made safe for a file name.
    fn stem(&self) -> String {
        let s: String = self
            .session
            .design()
            .name
            .chars()
            .map(|c| {
                if c.is_alphanumeric() || " -_".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let s = s.trim();
        if s.is_empty() {
            "design".into()
        } else {
            s.into()
        }
    }

    /// File ▸ Save: the editable design (`project.save`) as `NAME.osd`.
    pub fn save(&mut self) -> Result<OutFile, String> {
        let name = format!("{}.{}", self.stem(), openstitch_engine::project::EXTENSION);
        let path = self.dir.join(&name);
        self.session
            .execute(
                "project.save",
                Some(&json!({ "path": path.display().to_string() })),
            )
            .map_err(|e| e.to_string())?;
        let bytes = store::take(&path)?;
        self.status = format!("Saved {name}");
        Ok(OutFile { name, bytes })
    }

    /// File ▸ Export: `design.export` to `format` (any writable machine format, or csv, txt,
    /// gcode). Every file it writes (split parts, an EXP's .inf) is returned, in its order.
    pub fn export(&mut self, format: &str) -> Result<Vec<OutFile>, String> {
        let format = format.trim().to_ascii_lowercase();
        if !export_formats().contains(&format.as_str()) {
            return Err(format!(
                "cannot export {format}: use {}",
                export_formats().join(", ")
            ));
        }
        let path = self.dir.join(format!("{}.{format}", self.stem()));
        let r = self
            .session
            .execute(
                "design.export",
                Some(&json!({ "path": path.display().to_string(), "format": format })),
            )
            .map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        for p in r["paths"].as_array().into_iter().flatten() {
            let p = PathBuf::from(p.as_str().unwrap_or_default());
            let name = p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            out.push(OutFile {
                name,
                bytes: store::take(&p)?,
            });
        }
        self.status = format!(
            "Exported {} ({} stitches)",
            out.iter()
                .map(|f| f.name.as_str())
                .collect::<Vec<_>>()
                .join(", "),
            r["stats"]["stitches"]
        );
        Ok(out)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo_groups.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo_groups.is_empty()
    }

    /// Engine undo steps held: the groups' total (the engine keeps at most its history limit).
    fn undo_depth(&self) -> usize {
        self.undo_groups.iter().sum()
    }

    fn push_group(&mut self, steps: usize) {
        if steps > 0 {
            self.undo_groups.push(steps);
            self.redo_groups.clear();
        }
    }

    /// Edit ▸ Undo / Redo (`history.undo` / `history.redo`): one editor step, which may be
    /// several engine steps (File ▸ Open).
    pub fn history(&mut self, redo: bool) -> Result<Value, String> {
        let (id, from, to) = if redo {
            ("history.redo", &mut self.redo_groups, &mut self.undo_groups)
        } else {
            ("history.undo", &mut self.undo_groups, &mut self.redo_groups)
        };
        let steps = from
            .pop()
            .ok_or_else(|| format!("nothing to {}", if redo { "redo" } else { "undo" }))?;
        to.push(steps);
        for _ in 0..steps {
            self.session.execute(id, None).map_err(|e| e.to_string())?;
        }
        self.recompile(false);
        Ok(json!({ "canUndo": self.can_undo(), "canRedo": self.can_redo() }))
    }

    /// Runs one engine command; a change of the design is one Undo step and recompiles the
    /// stitches.
    pub fn command(&mut self, id: &str, params: Option<&Value>) -> Result<Value, String> {
        match id {
            "history.undo" => return self.history(false),
            "history.redo" => return self.history(true),
            _ => {}
        }
        let before = self.session.design().clone();
        let r = self
            .session
            .execute(id, params)
            .map_err(|e| e.to_string())?;
        if *self.session.design() != before {
            self.push_group(1);
            if self.undo_depth() > openstitch_engine::session::HISTORY_LIMIT {
                self.undo_groups.remove(0);
            }
            self.recompile(false);
        }
        Ok(r)
    }

    /// Edit ▸ the whole design: runs `id` (`object.rotate`, `object.flip`, `object.center`, …)
    /// with `params` on every object, as one Undo step.
    pub fn edit_all(&mut self, id: &str, params: Value) -> Result<Value, String> {
        let ids: Vec<u64> = self.session.design().objects.iter().map(|o| o.id).collect();
        if ids.is_empty() {
            return Err("the design is empty".into());
        }
        let mut params = params;
        params["ids"] = json!(ids);
        self.command(id, Some(&params))
    }

    /// Stitch records shown by the simulator, and the total.
    pub fn simulated(&self) -> (usize, usize) {
        (self.shown, self.pattern.stitches.len())
    }

    /// Moves the simulator to `stitch` (clamped to the design).
    pub fn simulate(&mut self, stitch: usize) {
        self.shown = stitch.min(self.pattern.stitches.len());
    }

    /// Moves the simulator to the end of the next colour block, or the start after the end.
    pub fn next_block(&mut self) {
        let s = &self.pattern.stitches;
        if self.shown >= s.len() {
            self.shown = 0;
            return;
        }
        self.shown = s[self.shown..]
            .iter()
            .position(|st| st.command == Command::ColorChange)
            .map_or(s.len(), |i| self.shown + i + 1);
    }

    /// The thread lines sewn by the first `limit` stitch records: a stitch draws from the
    /// needle's last position; jumps and trims move it without a line.
    pub fn segments(&self, limit: usize) -> Vec<Segment> {
        let threads = &self.pattern.threads;
        let rgb = |i: usize| {
            threads
                .get(i.min(threads.len().saturating_sub(1)))
                .map_or([40, 40, 40], |t| [t.red, t.green, t.blue])
        };
        let (mut out, mut colour, mut last) = (Vec::new(), 0, None::<[f32; 2]>);
        for st in self.pattern.stitches.iter().take(limit) {
            let at = [st.x as f32, st.y as f32];
            match st.command {
                Command::Stitch => {
                    if let Some(from) = last {
                        out.push(Segment {
                            from,
                            to: at,
                            rgb: rgb(colour),
                        });
                    }
                    last = Some(at);
                }
                Command::Jump | Command::Trim => last = Some(at),
                Command::ColorChange => {
                    colour += 1;
                    last = None;
                }
                Command::Stop | Command::End => {}
            }
        }
        out
    }

    /// The automation API (`openstitch.run` in the page, the browser's `--script`): an engine
    /// request `{"id"?, "command", "params"?}` or one of the web editor's own `ui.*` commands,
    /// answered as the desktop control channel answers (`{"ok", "result"}` or `{"ok", "error"}`).
    ///
    /// - `ui.state`: the open file, page size, stitches, simulator position, undo/redo, formats.
    /// - `ui.simulate {stitch}` or `{step: "block"}`: moves the stitch simulator.
    /// - `ui.openFile {name, base64}`: opens a file as File ▸ Open does.
    /// - `ui.saveFile`: File ▸ Save; the `.osd` comes back as `{name, base64}`.
    /// - `ui.exportFile {format}`: File ▸ Export; `{files: [{name, base64}]}`.
    pub fn handle(&mut self, request: &Value) -> Value {
        let command = request
            .get("command")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let params = request.get("params");
        let result = if command.starts_with("ui.") {
            self.ui_command(command, params.unwrap_or(&Value::Null))
        } else if command.is_empty() {
            Err("request needs a string 'command'".into())
        } else {
            self.command(command, params)
        };
        let mut reply = serde_json::Map::new();
        if let Some(id) = request.get("id") {
            reply.insert("id".into(), id.clone());
        }
        match result {
            Ok(r) => {
                reply.insert("ok".into(), json!(true));
                reply.insert("result".into(), r);
            }
            Err(e) => {
                reply.insert("ok".into(), json!(false));
                reply.insert("error".into(), json!(e));
            }
        }
        Value::Object(reply)
    }

    fn ui_command(&mut self, command: &str, p: &Value) -> Result<Value, String> {
        let b64 = base64::engine::general_purpose::STANDARD;
        let file = |f: &OutFile| json!({ "name": f.name, "base64": b64.encode(&f.bytes) });
        match command {
            "ui.state" => Ok(self.state()),
            "ui.simulate" => {
                match (p.get("stitch"), p.get("step").and_then(Value::as_str)) {
                    (Some(n), None) => {
                        let n = n.as_u64().ok_or("stitch must be a whole number")?;
                        self.simulate(usize::try_from(n).unwrap_or(usize::MAX));
                    }
                    (None, Some("block")) => self.next_block(),
                    _ => return Err("give stitch (a number) or step: \"block\"".into()),
                }
                let (shown, total) = self.simulated();
                Ok(json!({ "stitch": shown, "total": total }))
            }
            "ui.openFile" => {
                let name = p
                    .get("name")
                    .and_then(Value::as_str)
                    .ok_or("name is required")?;
                let data = p
                    .get("base64")
                    .and_then(Value::as_str)
                    .ok_or("base64 is required")?;
                let bytes = b64.decode(data).map_err(|e| format!("base64: {e}"))?;
                let status = self.open(name, bytes)?;
                Ok(json!({ "status": status, "state": self.state() }))
            }
            "ui.saveFile" => self.save().map(|f| file(&f)),
            "ui.exportFile" => {
                let format = p
                    .get("format")
                    .and_then(Value::as_str)
                    .ok_or("format is required")?;
                let files = self.export(format)?;
                Ok(json!({ "files": files.iter().map(file).collect::<Vec<_>>() }))
            }
            _ => Err(format!(
                "{command} is not in the web editor: use ui.state, ui.simulate, ui.openFile, \
                 ui.saveFile, ui.exportFile or an engine command"
            )),
        }
    }

    pub fn state(&self) -> Value {
        let (w, h) = self.page_mm();
        let (shown, total) = self.simulated();
        json!({
            "file": self.file_name,
            "design": self.session.design().name,
            "pageMm": [w, h],
            "objects": self.session.design().objects.len(),
            "stitches": self.pattern.count(Command::Stitch),
            "colours": self.pattern.color_block_count(),
            "simulator": { "stitch": shown, "total": total },
            "canUndo": self.can_undo(),
            "canRedo": self.can_redo(),
            "exportFormats": export_formats(),
            "status": self.status,
            "desktopOnly": DESKTOP_ONLY,
        })
    }
}

/// The formats File ▸ Export offers: every machine format the engine writes, then the stitch
/// lists and G-code.
pub fn export_formats() -> Vec<&'static str> {
    FileType::writable()
        .map(FileType::extension)
        .chain(openstitch_engine::extra_exports::EXTRA_FORMATS)
        .collect()
}

/// The extensions File ▸ Open accepts: `.osd` and every embroidery format the engine reads.
pub fn open_extensions() -> Vec<&'static str> {
    std::iter::once(openstitch_engine::project::EXTENSION)
        .chain(FileType::ALL.iter().map(|t| t.extension()))
        .collect()
}

fn is_project(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(openstitch_engine::project::EXTENSION))
}

/// The picked file's own name, without any folder (a browser gives only the name; the
/// automation API might pass anything).
fn plain_name(name: &str) -> Result<String, String> {
    let n = name.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    if n.is_empty() || n == "." || n == ".." {
        return Err("the file has no name".into());
    }
    Ok(n.to_string())
}

/// Where the engine's file commands find their files: memory in the browser, the folder
/// natively.
mod store {
    use std::path::Path;

    #[cfg(target_arch = "wasm32")]
    pub fn put(path: &Path, bytes: Vec<u8>) -> Result<(), String> {
        openstitch_engine::fsio::memfs::put(path, bytes);
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn take(path: &Path) -> Result<Vec<u8>, String> {
        openstitch_engine::fsio::memfs::take(path)
            .ok_or_else(|| format!("{} was not written", path.display()))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn put(path: &Path, bytes: Vec<u8>) -> Result<(), String> {
        std::fs::write(path, bytes).map_err(|e| e.to_string())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn take(path: &Path) -> Result<Vec<u8>, String> {
        let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
        let _ = std::fs::remove_file(path);
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_testutil::TempDir;

    fn reference(path: &str) -> Vec<u8> {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../compat/reference");
        std::fs::read(root.join(path)).expect("reference file")
    }

    /// The desktop editor's File ▸ Open and File ▸ Export, as app.rs runs them, on real files.
    fn desktop_export(dir: &Path, name: &str, bytes: &[u8], format: &str) -> Vec<u8> {
        let input = dir.join(name);
        std::fs::write(&input, bytes).expect("write");
        let pattern = FileType::from_path(&input)
            .expect("type")
            .read(bytes)
            .expect("read");
        let b = pattern.bounds();
        let page = |v: f64| ((v + 10.0) / 10.0).ceil().max(10.0) * 10.0;
        let (w, h) = openstitch_formats::pes::read_objects(bytes)
            .ok()
            .and_then(|s| s.page_mm.filter(|_| s.version == "0110"))
            .map_or(
                (page(b.width() / 10.0), page(b.height() / 10.0)),
                |(w, h)| (f64::from(w), f64::from(h)),
            );
        let stem = Path::new(name).file_stem().expect("stem").to_string_lossy();
        let mut s = Session::new(dir);
        s.execute(
            "design.new",
            Some(&json!({ "name": stem, "width": w, "height": h })),
        )
        .expect("new");
        s.execute(
            "stitches.import",
            Some(&json!({ "path": input.display().to_string(), "keepSource": true })),
        )
        .expect("import");
        let out_dir = dir.join("out");
        std::fs::create_dir_all(&out_dir).expect("out dir");
        let out = out_dir.join(format!("{stem}.{format}"));
        s.execute(
            "design.export",
            Some(&json!({ "path": out.display().to_string() })),
        )
        .expect("export");
        std::fs::read(out).expect("exported")
    }

    #[test]
    fn web_exports_are_byte_identical_to_the_desktop_editors() {
        let desk = TempDir::new("web-desk");
        let web_dir = TempDir::new("web-doc");
        for (file, name) in [
            ("pes/pes-default-v6.pes", "default.pes"),
            ("pes/pes-0110-stitches.pes", "pe11.pes"),
            ("stitches/import-rect20x10.dst", "rect.dst"),
        ] {
            let bytes = reference(file);
            let mut web = WebSession::new(web_dir.path());
            web.open(name, bytes.clone()).expect("open");
            for format in ["pes", "dst", "exp", "vp3", "csv"] {
                let files = web.export(format).expect("export");
                assert_eq!(files.len(), 1, "{name} {format}");
                assert!(files[0].name.ends_with(&format!(".{format}")));
                assert_eq!(
                    files[0].bytes,
                    desktop_export(desk.path(), name, &bytes, format),
                    "{name} as {format}"
                );
            }
            // Nothing is left behind: the picked file and the downloads were taken.
            assert_eq!(std::fs::read_dir(web_dir.path()).expect("dir").count(), 0);
        }
    }

    #[test]
    fn a_relative_folder_works() {
        let dir = TempDir::new("web-rel");
        let rel = PathBuf::from(format!(
            "{}/../{}",
            dir.path().display(),
            dir.path()
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or_default()
        ));
        let cwd = std::env::current_dir().expect("cwd");
        let rel = pathdiff(&rel, &cwd);
        let mut web = WebSession::new(&rel);
        web.open("rect.dst", reference("stitches/import-rect20x10.dst"))
            .expect("open");
        assert_eq!(web.export("dst").expect("export").len(), 1);
    }

    /// `to` relative to `from` (both absolute), with `..` steps.
    fn pathdiff(to: &Path, from: &Path) -> PathBuf {
        let (to, from): (Vec<_>, Vec<_>) = (to.components().collect(), from.components().collect());
        let same = to.iter().zip(&from).take_while(|(a, b)| a == b).count();
        let mut out = PathBuf::new();
        for _ in same..from.len() {
            out.push("..");
        }
        to[same..].iter().for_each(|c| out.push(c));
        out
    }

    #[test]
    fn an_unchanged_pes_is_written_back_as_the_file_itself() {
        let dir = TempDir::new("web-same");
        let bytes = reference("pes/pes-0110-stitches.pes");
        let mut web = WebSession::new(dir.path());
        web.open("pe11.pes", bytes.clone()).expect("open");
        assert_eq!(web.export("pes").expect("export")[0].bytes, bytes);
    }

    #[test]
    fn save_and_reopen_keep_the_design_and_start_a_new_history() {
        let dir = TempDir::new("web-osd");
        let mut web = WebSession::new(dir.path());
        web.open("rect.dst", reference("stitches/import-rect20x10.dst"))
            .expect("open");
        assert!(
            web.can_undo(),
            "opening is one undo step, as on the desktop"
        );
        web.history(false).expect("undo the open");
        assert!(web.session().design().objects.is_empty() && !web.can_undo());
        web.history(true).expect("redo the open");
        let saved = web.save().expect("save");
        assert_eq!(saved.name, "rect.osd");
        let design = web.session().design().clone();
        let mut again = WebSession::new(dir.path());
        again.open(&saved.name, saved.bytes).expect("reopen");
        assert_eq!(*again.session().design(), design);
        assert_eq!(again.file_name.as_deref(), Some("rect.osd"));
    }

    #[test]
    fn a_bad_file_leaves_the_open_design() {
        let dir = TempDir::new("web-bad");
        let mut web = WebSession::new(dir.path());
        web.open("rect.dst", reference("stitches/import-rect20x10.dst"))
            .expect("open");
        let before = web.session().design().clone();
        assert!(web.open("notes.txt", b"hello".to_vec()).is_err());
        assert!(web.open("broken.pes", b"#PES0001".to_vec()).is_err());
        assert!(web.open("", vec![1]).is_err());
        assert_eq!(*web.session().design(), before);
        assert!(
            web.export("png").is_err(),
            "only machine formats and stitch lists"
        );
    }

    #[test]
    fn edits_undo_and_the_simulator_follow_the_design() {
        let dir = TempDir::new("web-edit");
        let mut web = WebSession::new(dir.path());
        web.open("default.pes", reference("pes/pes-default-v6.pes"))
            .expect("open");
        let (shown, total) = web.simulated();
        assert!(
            total > 0 && shown == total,
            "opens with the whole stitch-out shown"
        );
        assert!(!web.segments(total).is_empty());
        web.simulate(0);
        assert!(web.segments(web.simulated().0).is_empty());
        web.next_block();
        assert!(web.simulated().0 > 0);
        web.simulate(usize::MAX);
        assert_eq!(web.simulated().0, total);

        let rev = web.revision;
        let id = web.session().design().objects[0].id;
        web.command(
            "object.move",
            Some(&json!({ "ids": [id], "dx": 5, "dy": 0 })),
        )
        .expect("move");
        assert!(web.revision > rev);
        let moved = web.session().design().clone();
        web.history(false).expect("undo the move");
        assert!(
            web.can_undo() && web.can_redo(),
            "the open is still undoable"
        );
        web.history(true).expect("redo");
        assert_eq!(*web.session().design(), moved);
        web.history(false).expect("undo the move");
        web.history(false).expect("undo the open");
        assert!(!web.can_undo() && web.history(false).is_err());
    }

    #[test]
    fn whole_design_edits_change_the_export_and_undo_as_one_step() {
        let dir = TempDir::new("web-whole");
        let mut web = WebSession::new(dir.path());
        assert!(
            web.edit_all("object.center", json!({})).is_err(),
            "nothing to edit"
        );
        web.open("rect.dst", reference("stitches/import-rect20x10.dst"))
            .expect("open");
        let before = web.export("dst").expect("export")[0].bytes.clone();
        web.edit_all("object.rotate", json!({ "degrees": 90 }))
            .expect("rotate");
        web.edit_all("object.flip", json!({ "axis": "horizontal" }))
            .expect("flip");
        web.edit_all("object.move", json!({ "dx": 5, "dy": 0 }))
            .expect("move");
        let undo_steps = web.undo_groups.len();
        web.edit_all("object.center", json!({})).expect("centre");
        assert_eq!(
            web.undo_groups.len(),
            undo_steps + 1,
            "centring after a move is a step"
        );
        web.edit_all("object.center", json!({}))
            .expect("centre again");
        assert_eq!(
            web.undo_groups.len(),
            undo_steps + 1,
            "a no-op is not an undo step"
        );
        assert_ne!(web.export("dst").expect("export")[0].bytes, before);
        for _ in 0..4 {
            web.history(false).expect("undo");
        }
        assert_eq!(web.export("dst").expect("export")[0].bytes, before);
    }

    #[test]
    fn the_automation_api_runs_ui_and_engine_commands() {
        let dir = TempDir::new("web-api");
        let b64 = base64::engine::general_purpose::STANDARD;
        let mut web = WebSession::new(dir.path());
        let bytes = reference("stitches/import-rect20x10.dst");
        let r = web.handle(&json!({ "id": 1, "command": "ui.openFile",
            "params": { "name": "rect.dst", "base64": b64.encode(&bytes) } }));
        assert_eq!(r["ok"], true, "{r}");
        assert_eq!(r["id"], 1);
        let state = web.handle(&json!({ "command": "ui.state" }));
        assert_eq!(state["result"]["file"], "rect.dst");
        assert!(
            state["result"]["exportFormats"]
                .as_array()
                .is_some_and(|f| f.len() > 5)
        );
        let r = web.handle(&json!({ "command": "ui.simulate", "params": { "stitch": 3 } }));
        assert_eq!(r["result"]["stitch"], 3);
        let r = web.handle(&json!({ "command": "ui.simulate", "params": { "step": "block" } }));
        assert_eq!(r["ok"], true);
        let r = web.handle(&json!({ "command": "ui.exportFile", "params": { "format": "dst" } }));
        let file = &r["result"]["files"][0];
        assert_eq!(file["name"], "rect.dst");
        assert!(
            !b64.decode(file["base64"].as_str().expect("b64"))
                .expect("decode")
                .is_empty()
        );
        let r = web.handle(&json!({ "command": "ui.saveFile" }));
        assert_eq!(r["result"]["name"], "rect.osd");
        let r = web.handle(&json!({ "command": "design.info" }));
        assert_eq!(r["ok"], true, "{r}");
        for bad in [
            json!({ "command": "ui.run", "params": { "action": "open" } }),
            json!({ "command": "ui.simulate", "params": {} }),
            json!({ "params": {} }),
            json!({ "command": "no.such" }),
        ] {
            assert_eq!(web.handle(&bad)["ok"], false, "{bad}");
        }
    }

    #[test]
    fn open_accepts_every_format_the_engine_reads() {
        let exts = open_extensions();
        assert_eq!(exts[0], "osd");
        assert_eq!(exts.len(), 1 + FileType::ALL.len());
        assert!(export_formats().contains(&"pes") && export_formats().contains(&"gcode"));
        assert_eq!(plain_name("C:\\x\\a.pes").ok().as_deref(), Some("a.pes"));
        assert_eq!(plain_name("/tmp/../b.dst").ok().as_deref(), Some("b.dst"));
        assert!(plain_name("dir/").is_err() && plain_name("..").is_err());
    }
}
