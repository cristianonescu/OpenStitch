//! OpenStitch engine: a [`Session`] owns the open design and its undo history, and every
//! operation is a registered command taking JSON parameters. The desktop app, CLI, JSON
//! control channel, MCP server and tests all go through [`Session::execute`].

mod arrange_modes;
mod builder;
pub mod capabilities;
pub mod commands;
mod continuing;
mod convert;
#[cfg(feature = "digitize")]
mod digitize;
mod drawing;
mod envelopes;
#[cfg(feature = "formats")]
pub mod extra_exports;
pub mod fabric;
mod finish;
#[cfg(feature = "digitize")]
mod font_images;
pub mod fonts;
mod guides;
#[cfg(feature = "formats")]
pub mod imports;
#[cfg(feature = "formats")]
pub mod inkstitch_fonts;
mod knife;
pub mod layout;
mod lettering;
#[cfg(feature = "library")]
mod library;
pub mod machine;
#[cfg(feature = "formats")]
pub mod media_exports;
#[cfg(feature = "formats")]
pub mod metafile;
pub mod order;
mod ornament;
pub mod palette;
pub mod params;
mod pattern_cmds;
#[cfg(feature = "formats")]
pub mod pes11_export;
#[cfg(feature = "formats")]
pub mod pes11_import;
pub mod preview;
pub mod print;
pub mod production;
pub mod project;
mod punch;
pub mod quality;
pub mod sandbox;
pub mod sections;
pub mod session;
mod sewing;
mod sewing_cmds;
mod shape_ops;
mod shape_tools;
mod smoothing;
mod stamps;
mod stitch_edit;
#[cfg(feature = "formats")]
mod svg_settings;
pub mod templates;
pub mod thread_match;
#[cfg(feature = "formats")]
pub mod threadlists;
mod timeline;
#[cfg(feature = "formats")]
pub mod transfer;
pub mod user;
pub mod validate;
pub(crate) mod wizards;

pub use capabilities::{Capabilities, Capability};
pub use commands::{CommandSpec, Effect, ParamDoc};
pub use params::{CommandError, CommandResult, Params};
pub use sandbox::{Access, Sandbox, SandboxError};
pub use session::{EngineState, Session};
pub use validate::{ParamProblem, check_docs, check_listed, check_params};
