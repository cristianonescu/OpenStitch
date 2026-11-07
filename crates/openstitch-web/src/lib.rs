//! OpenStitch in a web browser (#556): the design editor built for WebAssembly.
//!
//! [`session::WebSession`] is the document and its commands (target-independent, tested
//! natively). In the browser, `ui` draws the editor with egui on a WebGL canvas and `web` is
//! the page's entry point: `start(canvas_id)` and the automation call `run(request_json)`.
//! See docs/DISTRIBUTION.md ("Web editor") for the build and hosting.

pub mod session;

#[cfg(target_arch = "wasm32")]
mod ui;
#[cfg(target_arch = "wasm32")]
mod web;

pub use session::{OutFile, Segment, WebSession};
