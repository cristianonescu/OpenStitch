//! File dialogs (#693). On the desktop this is `rfd::FileDialog`, unchanged. A browser has no
//! blocking file dialog (rfd offers only `AsyncFileDialog` there): the web build gets this
//! stand-in with the same builder methods, whose pickers return nothing, so every caller compiles
//! as it is; the web editor opens and saves through the page's own picker and downloads.

#[cfg(not(target_arch = "wasm32"))]
pub use rfd::FileDialog;

#[cfg(target_arch = "wasm32")]
pub use web::FileDialog;

#[cfg(target_arch = "wasm32")]
mod web {
    use std::path::{Path, PathBuf};

    /// The web build's stand-in for `rfd::FileDialog`: no blocking dialog exists in a browser.
    #[derive(Clone, Debug, Default)]
    pub struct FileDialog;

    impl FileDialog {
        pub fn new() -> Self {
            FileDialog
        }

        pub fn add_filter(self, _name: impl Into<String>, _extensions: &[impl ToString]) -> Self {
            self
        }

        pub fn set_file_name(self, _file_name: impl Into<String>) -> Self {
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

        pub fn save_file(self) -> Option<PathBuf> {
            None
        }
    }
}
