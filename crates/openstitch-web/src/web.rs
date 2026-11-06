//! The page's entry points and the browser's file picker and downloads.

use crate::session::{OutFile, WebSession};
use std::cell::RefCell;
use wasm_bindgen::JsCast as _;
use wasm_bindgen::prelude::*;

thread_local! {
    /// The one open document, shared by the editor and the automation API.
    pub(crate) static DOC: RefCell<WebSession> = RefCell::new(WebSession::new("/web"));
    /// The editor's egui context, so an automation call can ask for a repaint.
    static CTX: RefCell<Option<eframe::egui::Context>> = const { RefCell::new(None) };
}

pub(crate) fn with_doc<R>(f: impl FnOnce(&mut WebSession) -> R) -> R {
    DOC.with(|d| f(&mut d.borrow_mut()))
}

fn repaint() {
    CTX.with(|c| {
        if let Some(c) = &*c.borrow() {
            c.request_repaint();
        }
    });
}

/// Starts the editor on the `<canvas id=canvas_id>`.
#[wasm_bindgen]
pub async fn start(canvas_id: String) -> Result<(), JsValue> {
    let canvas = web_sys::window()
        .and_then(|w| w.document())
        .and_then(|d| d.get_element_by_id(&canvas_id))
        .ok_or_else(|| JsValue::from_str(&format!("no element #{canvas_id}")))?
        .dyn_into::<web_sys::HtmlCanvasElement>()?;
    eframe::WebRunner::new()
        .start(
            canvas,
            eframe::WebOptions::default(),
            Box::new(|cc| {
                CTX.with(|c| *c.borrow_mut() = Some(cc.egui_ctx.clone()));
                Ok(Box::new(crate::ui::Editor::default()))
            }),
        )
        .await
}

/// The automation API: one JSON request in, one JSON reply out (see
/// [`WebSession::handle`]). The page exposes it as `openstitch.run`.
#[wasm_bindgen]
pub fn run(request_json: &str) -> String {
    let reply = match serde_json::from_str(request_json) {
        Ok(request) => with_doc(|d| d.handle(&request)),
        Err(e) => serde_json::json!({ "ok": false, "error": format!("request is not JSON: {e}") }),
    };
    repaint();
    reply.to_string()
}

/// File ▸ Open: the browser's file picker; the picked file opens when its bytes have arrived.
pub(crate) fn pick_and_open() {
    let exts = crate::session::open_extensions();
    wasm_bindgen_futures::spawn_local(async move {
        let Some(file) = rfd::AsyncFileDialog::new()
            .add_filter("Designs and embroidery files", &exts)
            .pick_file()
            .await
        else {
            return;
        };
        let (name, bytes) = (file.file_name(), file.read().await);
        with_doc(|d| {
            if let Err(e) = d.open(&name, bytes) {
                d.status = e;
            }
        });
        repaint();
    });
}

/// Hands files to the browser as downloads (the browser's Save dialog or Downloads folder).
pub(crate) fn download(files: &[OutFile]) -> Result<(), String> {
    let js = |e: JsValue| format!("{e:?}");
    let document = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    for f in files {
        let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(f.bytes.as_slice()));
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
        a.set_download(&f.name);
        a.click();
        web_sys::Url::revoke_object_url(&url).map_err(js)?;
    }
    Ok(())
}
