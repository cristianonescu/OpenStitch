//! The page's entry points: the desktop editor (`openstitch_app::EditorApp`, #693) on the
//! canvas, the browser's file picker for File ▸ Open and Import, and downloads for what the
//! editor saves or exports.
//!
//! A browser has no file system: a picked file's bytes go into the engine's in-memory files
//! (`openstitch_engine::fsio::memfs`) and the editor opens that path; the editor's Save and
//! Export dialogs answer paths under [`openstitch_app::SAVE_DIR`], and every file written there
//! is handed to the browser as a download after the frame, then dropped from memory.

use crate::session::WebSession;
use base64::Engine as _;
use openstitch_app::{EditorApp, Launch, Pick, PickRequest};
use openstitch_engine::fsio::memfs;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;

/// Where picked files are put in memory.
const OPEN_DIR: &str = "/web/open";
/// Where the automation API's `ui.saveFile` and `ui.exportFile` write before they hand the
/// bytes back (not downloaded).
const OUT_DIR: &str = "/web/out";

thread_local! {
    /// The document behind `run` before the editor has started (the Node smoke test).
    pub(crate) static DOC: RefCell<WebSession> = RefCell::new(WebSession::new("/web"));
    /// The editor once `start` has run: `run` drives it, as the desktop's socket does.
    static EDITOR: RefCell<Option<Rc<RefCell<EditorApp>>>> = const { RefCell::new(None) };
    /// The editor's egui context, so a picked file or an automation call can ask for a frame.
    static CTX: RefCell<Option<eframe::egui::Context>> = const { RefCell::new(None) };
    /// Files the picker has delivered, opened at the start of the next frame.
    static PICKED: RefCell<Vec<(Pick, Vec<PathBuf>)>> = const { RefCell::new(Vec::new()) };
}

fn repaint() {
    CTX.with(|c| {
        if let Some(c) = &*c.borrow() {
            c.request_repaint();
        }
    });
}

fn editor() -> Option<Rc<RefCell<EditorApp>>> {
    EDITOR.with(|e| e.borrow().clone())
}

/// The editor in the page: EditorApp's frame, plus the page's picker and downloads around it.
struct Page {
    editor: Rc<RefCell<EditorApp>>,
}

impl eframe::App for Page {
    fn raw_input_hook(&mut self, ctx: &eframe::egui::Context, raw: &mut eframe::egui::RawInput) {
        // Files dropped on the page (#693): read, put in memory, then opened.
        for f in std::mem::take(&mut raw.dropped_files) {
            dropped(f);
        }
        self.editor.borrow_mut().raw_input_hook(ctx, raw);
    }

    fn ui(&mut self, ui: &mut eframe::egui::Ui, frame: &mut eframe::Frame) {
        let picked = PICKED.with(|p| std::mem::take(&mut *p.borrow_mut()));
        let mut editor = self.editor.borrow_mut();
        for (kind, paths) in picked {
            editor.web_picked(kind, paths);
        }
        editor.ui(ui, frame);
        drop(editor);
        for request in openstitch_app::take_pick_requests() {
            pick(request);
        }
        flush_downloads();
    }
}

/// Starts the editor on the `<canvas id=canvas_id>`.
#[wasm_bindgen]
pub async fn start(canvas_id: String) -> Result<(), JsValue> {
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&canvas_id))
        .ok_or_else(|| JsValue::from_str(&format!("no element #{canvas_id}")))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    let options = eframe::WebOptions {
        // WebGL 2 everywhere the page runs (the desktop uses wgpu).
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::WebRunner::new()
        .start(
            canvas,
            options,
            Box::new(|cc| {
                CTX.with(|c| *c.borrow_mut() = Some(cc.egui_ctx.clone()));
                let launch = Launch {
                    no_splash: true,
                    ..Launch::default()
                };
                let editor = Rc::new(RefCell::new(EditorApp::new(launch)));
                EDITOR.with(|e| *e.borrow_mut() = Some(editor.clone()));
                Ok(Box::new(Page { editor }))
            }),
        )
        .await
}

/// The automation API: one JSON request in, one JSON reply out. Once the editor has started
/// it takes the desktop socket's requests (engine commands and `ui.*`, on the editor's own
/// history) plus the page's file commands ([`page_command`]); before that,
/// [`WebSession::handle`]. The page exposes it as `openstitch.run`.
#[wasm_bindgen]
pub fn run(request_json: &str) -> String {
    let reply = match serde_json::from_str::<Value>(request_json) {
        Ok(request) => match editor() {
            Some(e) => match e.try_borrow_mut() {
                Ok(mut e) => {
                    let command = request["command"].as_str().unwrap_or_default();
                    let params = request.get("params").cloned().unwrap_or(Value::Null);
                    match page_command(&mut e, command, &params) {
                        Some(r) => reply_of(&request, r),
                        None => e.web_request(&request),
                    }
                }
                Err(_) => json!({ "ok": false, "error": "the editor is busy" }),
            },
            None => DOC.with(|d| d.borrow_mut().handle(&request)),
        },
        Err(e) => json!({ "ok": false, "error": format!("request is not JSON: {e}") }),
    };
    repaint();
    reply.to_string()
}

fn reply_of(request: &Value, result: Result<Value, String>) -> Value {
    let mut reply = match result {
        Ok(r) => json!({ "ok": true, "result": r }),
        Err(e) => json!({ "ok": false, "error": e }),
    };
    if let Some(id) = request.get("id") {
        reply["id"] = id.clone();
    }
    reply
}

/// The page's file commands, files passed as base64 (as the editor without a window takes them):
/// - `ui.openFile {name, base64}`: File ▸ Open with those bytes (it opens on the next frames;
///   `ui.state`'s `loading` is null once it has);
/// - `ui.saveFile`: the design as `.osd`, `{name, base64}`;
/// - `ui.exportFile {format}`: `{files: [{name, base64}]}`.
///
/// `None` for every other command.
fn page_command(
    editor: &mut EditorApp,
    command: &str,
    params: &Value,
) -> Option<Result<Value, String>> {
    let b64 = base64::engine::general_purpose::STANDARD;
    Some(match command {
        "ui.openFile" => (|| {
            let name = plain_name(params["name"].as_str().ok_or("name is required")?);
            let bytes = b64
                .decode(params["base64"].as_str().ok_or("base64 is required")?)
                .map_err(|e| format!("base64: {e}"))?;
            let path = Path::new(OPEN_DIR).join(&name);
            memfs::put(path.clone(), bytes);
            editor.web_picked(Pick::Open, vec![path]);
            Ok(json!({ "opening": name }))
        })(),
        "ui.saveFile" => {
            written(editor, "project.save", json!({}), "osd").map(|mut files| files.swap_remove(0))
        }
        "ui.exportFile" => match params["format"].as_str() {
            Some(format) => written(editor, "design.export", json!({ "format": format }), format)
                .map(|files| json!({ "files": files })),
            None => Err("format is required".into()),
        },
        _ => return None,
    })
}

/// Runs `command` writing to `OUT_DIR/<design>.<extension>` and hands back every file it wrote,
/// base64, dropping them from memory.
fn written(
    editor: &mut EditorApp,
    command: &str,
    mut params: Value,
    extension: &str,
) -> Result<Vec<Value>, String> {
    let b64 = base64::engine::general_purpose::STANDARD;
    let name = editor.document_name();
    let stem = Path::new(&name)
        .file_stem()
        .map_or_else(|| "design".into(), |s| s.to_string_lossy().into_owned());
    let path = Path::new(OUT_DIR).join(format!("{stem}.{extension}"));
    params["path"] = json!(path.display().to_string());
    let reply = editor.web_request(&json!({ "command": command, "params": params }));
    if reply["ok"] != json!(true) {
        return Err(reply["error"].as_str().unwrap_or("failed").to_string());
    }
    let mut paths: Vec<PathBuf> = reply["result"]["paths"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|p| p.as_str().map(PathBuf::from))
        .collect();
    if paths.is_empty() {
        paths.push(path);
    }
    paths
        .iter()
        .map(|p| {
            let bytes = memfs::take(p).ok_or_else(|| format!("{} was not written", p.display()))?;
            let name = p
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            Ok(json!({ "name": name, "base64": b64.encode(bytes) }))
        })
        .collect()
}

/// The browser's file picker for an editor dialog (File ▸ Open, Import, a picture, a thread
/// chart, a font…); the dialog's action runs when the bytes have arrived.
fn pick(request: PickRequest) {
    let mut dialog = rfd::AsyncFileDialog::new();
    for (name, extensions) in &request.filters {
        dialog = dialog.add_filter(name.as_str(), extensions);
    }
    wasm_bindgen_futures::spawn_local(async move {
        let files = if request.many {
            dialog.pick_files().await.unwrap_or_default()
        } else {
            dialog.pick_file().await.into_iter().collect()
        };
        if files.is_empty() {
            return;
        }
        let mut paths = Vec::new();
        for file in files {
            let path = Path::new(OPEN_DIR).join(plain_name(&file.file_name()));
            memfs::put(path.clone(), file.read().await);
            paths.push(path);
        }
        PICKED.with(|p| p.borrow_mut().push((request.kind, paths)));
        repaint();
    });
}

/// A file dropped on the page: a design or embroidery file opens (as File ▸ Open), a picture
/// becomes the background image; anything else is left alone.
fn dropped(file: eframe::egui::DroppedFileHandle) {
    let name = plain_name(&file.path().to_string_lossy());
    let ext = Path::new(&name)
        .extension()
        .map(|e| e.to_string_lossy().to_ascii_lowercase())
        .unwrap_or_default();
    let kind = if ext == openstitch_engine::project::EXTENSION
        || openstitch_formats::FileType::from_extension(&ext).is_some()
    {
        Pick::Open
    } else if ["png", "jpg", "jpeg", "bmp"].contains(&ext.as_str()) {
        Pick::Background
    } else {
        web_sys::console::warn_1(&JsValue::from_str(&format!(
            "{name}: not a design, embroidery file or picture"
        )));
        return;
    };
    wasm_bindgen_futures::spawn_local(async move {
        match file.bytes_async().await {
            Ok(bytes) => {
                let path = Path::new(OPEN_DIR).join(&name);
                memfs::put(path.clone(), bytes);
                PICKED.with(|p| p.borrow_mut().push((kind, vec![path])));
                repaint();
            }
            Err(e) => web_sys::console::error_1(&JsValue::from_str(&format!("{name}: {e}"))),
        }
    });
}

/// A picked file's name without folders (browsers give only the name; this keeps it so).
fn plain_name(name: &str) -> String {
    Path::new(name)
        .file_name()
        .map_or_else(|| "design".into(), |n| n.to_string_lossy().into_owned())
}

/// Hands every file the editor wrote under [`openstitch_app::SAVE_DIR`] to the browser as a
/// download and drops it from memory.
fn flush_downloads() {
    for path in memfs::paths() {
        if !path.starts_with(openstitch_app::SAVE_DIR) {
            continue;
        }
        let Some(bytes) = memfs::take(&path) else {
            continue;
        };
        let name = path
            .file_name()
            .map_or_else(|| "design".into(), |n| n.to_string_lossy().into_owned());
        if let Err(e) = download(&name, &bytes) {
            web_sys::console::error_1(&JsValue::from_str(&format!("download {name}: {e}")));
        }
    }
}

/// One download (the browser's Save dialog or Downloads folder).
fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let js = |e: JsValue| format!("{e:?}");
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let options = web_sys::BlobPropertyBag::new();
    options.set_type("application/octet-stream");
    let blob =
        web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &options).map_err(js)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
    let a = document
        .create_element("a")
        .map_err(js)?
        .dyn_into::<web_sys::HtmlAnchorElement>()
        .map_err(|_| "no anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.click();
    web_sys::Url::revoke_object_url(&url).map_err(js)?;
    Ok(())
}
