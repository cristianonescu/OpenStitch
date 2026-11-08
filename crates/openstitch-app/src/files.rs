//! File dialogs (#693). On the desktop this is `rfd::FileDialog`, unchanged. A browser has no
//! blocking file dialog (rfd offers only `AsyncFileDialog` there): the web build gets this
//! stand-in with the same builder methods, so every caller compiles as it is. Its pickers return
//! nothing; File ▸ Open and Import ask the page instead (`request_pick`), which picks the file
//! asynchronously, puts its bytes in memory and hands the path back (`EditorApp::web_picked`).
//! Its Save dialog answers a path under `SAVE_DIR` at once: what the editor writes there the
//! page hands to the browser as a download.
//!
//! The other pickers go through [`pick_then`] / [`pick_many_then`]: the dialog and what to do
//! with the chosen file(s). On the desktop the dialog runs as before and the action runs at the
//! start of the next frame ([`run_ready`]); in a browser the page shows its picker and the
//! action runs once the file has arrived in memory (#693).

use std::cell::RefCell;
use std::path::PathBuf;

/// What to do with the file(s) a dialog gave.
type Then = Box<dyn FnOnce(&mut crate::app::EditorApp, Vec<PathBuf>)>;

thread_local! {
    /// Chosen files whose action has not run yet.
    static READY: RefCell<Vec<(Then, Vec<PathBuf>)>> = RefCell::new(Vec::new());
}

/// Shows `dialog` for one file; `then` runs with the chosen one (nothing when cancelled).
pub fn pick_then(
    dialog: FileDialog,
    then: impl FnOnce(&mut crate::app::EditorApp, PathBuf) + 'static,
) {
    pick(
        dialog,
        false,
        Box::new(move |app, v| {
            if let Some(p) = v.into_iter().next() {
                then(app, p);
            }
        }),
    );
}

/// Shows `dialog` for several files; `then` runs with the chosen ones.
pub fn pick_many_then(
    dialog: FileDialog,
    then: impl FnOnce(&mut crate::app::EditorApp, Vec<PathBuf>) + 'static,
) {
    pick(dialog, true, Box::new(then));
}

#[cfg(not(target_arch = "wasm32"))]
fn pick(dialog: FileDialog, many: bool, then: Then) {
    let chosen = if many {
        dialog.pick_files()
    } else {
        dialog.pick_file().map(|p| vec![p])
    };
    if let Some(v) = chosen.filter(|v| !v.is_empty()) {
        READY.with(|r| r.borrow_mut().push((then, v)));
    }
}

#[cfg(target_arch = "wasm32")]
fn pick(dialog: FileDialog, many: bool, then: Then) {
    web::request_then(dialog, many, then);
}

thread_local! {
    /// A folder chooser the browser could not answer (#830), shown as a notice next frame.
    static FOLDER_REFUSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Notes that a folder was asked for where none can be chosen (the browser's dialog stub; the
/// desktop's dialog always answers, so only the browser build and tests call it).
#[cfg_attr(not(any(test, target_arch = "wasm32")), allow(dead_code))]
pub fn note_folder_refused() {
    FOLDER_REFUSED.with(|f| f.set(true));
}

/// Whether a folder chooser was refused since the last call.
pub fn take_folder_refused() -> bool {
    FOLDER_REFUSED.with(|f| f.replace(false))
}

/// Runs the actions of the files chosen since the last frame (`EditorApp::ui` calls it first).
pub fn run_ready(app: &mut crate::app::EditorApp) {
    let ready = READY.with(|r| std::mem::take(&mut *r.borrow_mut()));
    for (then, files) in ready {
        then(app, files);
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub use rfd::FileDialog;

#[cfg(target_arch = "wasm32")]
pub use web::{FileDialog, Pick, PickRequest, SAVE_DIR, request_pick, take_pick_requests};

#[cfg(target_arch = "wasm32")]
pub use web::deliver;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
    use std::collections::HashMap;
    use std::path::{Path, PathBuf};

    /// Where the web build's Save dialog puts files: the page downloads and removes them.
    pub const SAVE_DIR: &str = "/web/save";

    /// What a file picked by the page is for.
    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    pub enum Pick {
        /// File ▸ Open.
        Open,
        /// Home ▸ Import.
        Import,
        /// File ▸ Background Image (a picture dropped on the page).
        Background,
        /// A [`super::pick_then`] dialog, by its number.
        Then(u64),
    }

    /// A file picker the editor asked the page for.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PickRequest {
        pub kind: Pick,
        /// The filters' names and extensions.
        pub filters: Vec<(String, Vec<String>)>,
        /// Several files at once.
        pub many: bool,
    }

    thread_local! {
        static REQUESTS: RefCell<Vec<PickRequest>> = const { RefCell::new(Vec::new()) };
        /// The actions of the pickers shown, by number.
        static WAITING: RefCell<HashMap<u64, super::Then>> = RefCell::new(HashMap::new());
        static NEXT: std::cell::Cell<u64> = const { std::cell::Cell::new(1) };
    }

    /// Asks the page for its file picker (the page runs it after this frame).
    pub fn request_pick(kind: Pick, name: &str, extensions: &[&str]) {
        let filters = vec![(
            name.to_string(),
            extensions.iter().map(|e| e.to_string()).collect(),
        )];
        REQUESTS.with(|r| {
            r.borrow_mut().push(PickRequest {
                kind,
                filters,
                many: false,
            })
        });
    }

    /// A [`super::pick_then`] dialog: the page's picker, with the action kept until the file
    /// arrives.
    pub(super) fn request_then(dialog: FileDialog, many: bool, then: super::Then) {
        let id = NEXT.with(|n| n.replace(n.get() + 1));
        WAITING.with(|w| w.borrow_mut().insert(id, then));
        REQUESTS.with(|r| {
            r.borrow_mut().push(PickRequest {
                kind: Pick::Then(id),
                filters: dialog.filters,
                many,
            })
        });
    }

    /// The page has the files of picker `id` in memory: its action runs at the next frame.
    pub fn deliver(id: u64, files: Vec<PathBuf>) {
        if let Some(then) = WAITING.with(|w| w.borrow_mut().remove(&id))
            && !files.is_empty()
        {
            super::READY.with(|r| r.borrow_mut().push((then, files)));
        }
    }

    /// The pickers asked for since the last call.
    pub fn take_pick_requests() -> Vec<PickRequest> {
        REQUESTS.with(|r| std::mem::take(&mut *r.borrow_mut()))
    }

    /// The web build's stand-in for `rfd::FileDialog`: no blocking dialog exists in a browser.
    #[derive(Clone, Debug, Default)]
    pub struct FileDialog {
        file_name: Option<String>,
        extension: Option<String>,
        filters: Vec<(String, Vec<String>)>,
    }

    impl FileDialog {
        pub fn new() -> Self {
            FileDialog::default()
        }

        pub fn add_filter(mut self, name: impl Into<String>, extensions: &[impl ToString]) -> Self {
            if self.extension.is_none() {
                self.extension = extensions.first().map(ToString::to_string);
            }
            let extensions = extensions.iter().map(ToString::to_string).collect();
            self.filters.push((name.into(), extensions));
            self
        }

        pub fn set_file_name(mut self, file_name: impl Into<String>) -> Self {
            self.file_name = Some(file_name.into());
            self
        }

        pub fn set_directory<P: AsRef<Path>>(self, _path: P) -> Self {
            self
        }

        pub fn pick_file(self) -> Option<PathBuf> {
            None
        }

        pub fn pick_files(self) -> Option<Vec<PathBuf>> {
            None
        }

        pub fn pick_folder(self) -> Option<PathBuf> {
            // A page cannot browse folders: the editor says so (#830).
            super::note_folder_refused();
            None
        }

        /// A path under [`SAVE_DIR`]: the given file name, or "design" with the first filter's
        /// extension. Only the name is kept, so the download is named after it.
        pub fn save_file(self) -> Option<PathBuf> {
            let name = self
                .file_name
                .as_deref()
                .and_then(|n| Path::new(n).file_name())
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| match &self.extension {
                    Some(e) => format!("design.{e}"),
                    None => "design".into(),
                });
            Some(Path::new(SAVE_DIR).join(name))
        }
    }
}
