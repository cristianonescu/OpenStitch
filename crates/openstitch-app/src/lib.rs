//! OpenStitch desktop editor. [`editor`] holds the UI-independent logic (viewport maths,
//! hit-testing, selection handles, tool state machines, render-model building, inspector and
//! object-list models) so it can be unit-tested without a window; `main.rs` and `ui` are the
//! thin egui shell that feeds it pointer events and runs the commands it returns through
//! [`openstitch_engine::Session::execute`].
// On wasm32 the editor window's modules (the binary's, `bin_modules.rs`) are part of this
// library, so the web page runs the same EditorApp (#693). Some of them serve only the desktop
// binary's command line, hence the allowances there.
#![cfg_attr(
    target_arch = "wasm32",
    allow(dead_code, unused_imports, clippy::drop_non_drop)
)]

pub mod editor;
#[cfg(not(target_arch = "wasm32"))]
pub mod keys;
pub mod os_print;
pub mod platform;
pub mod png;

#[cfg(target_arch = "wasm32")]
extern crate self as openstitch_app;
#[cfg(target_arch = "wasm32")]
include!("bin_modules.rs");
#[cfg(target_arch = "wasm32")]
pub use app::{EditorApp, Launch};
