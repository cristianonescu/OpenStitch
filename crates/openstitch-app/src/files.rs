//! File dialogs (#693). On the desktop this is `rfd::FileDialog`, unchanged. A browser has no
//! blocking file dialog (rfd offers only `AsyncFileDialog` there): the web build gets this
//! stand-in with the same builder methods, so every caller compiles as it is. Its pickers return
//! nothing; File ▸ Open and Import ask the page instead (`request_pick`), which picks the file
//! asynchronously, puts its bytes in memory and hands the path back (`EditorApp::web_picked`).
//! Its Save dialog answers a path under `SAVE_DIR` at once: what the editor writes there the
//! page hands to the browser as a download.

#[cfg(not(target_arch = "wasm32"))]
pub use rfd::FileDialog;

#[cfg(target_arch = "wasm32")]
pub use web::{FileDialog, Pick, PickRequest, SAVE_DIR, request_pick, take_pick_requests};

#[cfg(target_arch = "wasm32")]
mod web {
    use std::cell::RefCell;
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
    }

    /// A file picker the editor asked the page for.
    #[derive(Clone, Debug, PartialEq, Eq)]
    pub struct PickRequest {
        pub kind: Pick,
        /// The filter's name and extensions.
        pub filter: (String, Vec<String>),
    }

    thread_local! {
        static REQUESTS: RefCell<Vec<PickRequest>> = const { RefCell::new(Vec::new()) };
    }

    /// Asks the page for its file picker (the page runs it after this frame).
    pub fn request_pick(kind: Pick, name: &str, extensions: &[&str]) {
        let filter = (
            name.to_string(),
            extensions.iter().map(|e| e.to_string()).collect(),
        );
        REQUESTS.with(|r| r.borrow_mut().push(PickRequest { kind, filter }));
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
    }

    impl FileDialog {
        pub fn new() -> Self {
            FileDialog::default()
        }

        pub fn add_filter(mut self, _name: impl Into<String>, extensions: &[impl ToString]) -> Self {
            if self.extension.is_none() {
                self.extension = extensions.first().map(ToString::to_string);
            }
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
