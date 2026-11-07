//! The web build's stand-in for the updater (#693): the page is always the latest build, so
//! there is nothing to check, download or install.

use crate::app::EditorApp;
use eframe::egui;

/// How the editor would install an update: never, in a browser.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Install;

impl Install {
    pub fn checks(self) -> bool {
        false
    }
}

#[derive(Default)]
pub struct UpdateState {
    pub current: Option<()>,
    pub install: Install,
    pub check_at_start: bool,
    pub open: bool,
}

impl UpdateState {
    pub fn check(&mut self, _manual: bool) {}

    pub fn show_sample(&mut self) {}
}

pub fn poll(_app: &mut EditorApp, _ctx: &egui::Context) {}

pub fn window(_app: &mut EditorApp, _ctx: &egui::Context) {}

pub fn version_line() -> String {
    format!("OpenStitch {} (web)", env!("CARGO_PKG_VERSION"))
}
