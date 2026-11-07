//! OpenStitch in a web browser (#556): the design editor built for WebAssembly.
//!
//! In the browser the page runs the desktop editor itself (`openstitch_app::EditorApp`, #693) on
//! a WebGL canvas; `web` is the page's entry point: `start(canvas_id)` and the automation call
//! `run(request_json)`. [`session::WebSession`] is a document and its commands without the
//! window (target-independent, tested natively): `run` uses it until the editor has started,
//! as the Node smoke test does.
//! See docs/DISTRIBUTION.md ("Web editor") for the build and hosting.

pub mod session;

#[cfg(target_arch = "wasm32")]
mod web;

pub use session::{OutFile, Segment, WebSession};
