//! UI-independent editor logic. Model coordinates are 0.1 mm, +y down (see
//! [`openstitch_design::Design`]); screen coordinates are egui points.

pub mod assistant;
pub mod assistant_attach;
pub mod assistant_chat;
pub mod background;
pub mod clipboard;
pub mod close_prompt;
pub mod conversions;
pub mod creator;
pub mod database;
pub mod edit_ops;
pub mod handles;
pub mod hints;
pub mod hit;
pub mod inspector;
pub mod lettering;
pub mod objects;
pub mod palette;
pub mod pattern_wizards;
pub mod pen;
pub mod player;
pub mod points;
pub mod precision;
pub mod prefs;
pub mod recovery;
pub mod render;
pub mod request;
pub mod ribbon;
pub mod sections;
pub mod sew_kit;
pub mod sewing;
pub mod shape_effects;
pub mod shape_tools;
pub mod snap;
pub mod suite;
pub mod tools;
pub mod units;
pub mod updates;
pub mod viewport;
pub mod wizard;
pub mod workspace;

pub use handles::{Handle, SelectionHandles};
pub use request::CommandRequest;
pub use tools::{
    ClickCounter, NeedleOverlay, Phase, PointOverlay, PointerEvent, Tool, ToolContext,
    ToolController, ToolEffect, ToolPreview,
};
pub use viewport::{ScreenRect, Viewport};

#[cfg(test)]
mod drawing_tests;
#[cfg(test)]
mod shape_tools_tests;
#[cfg(test)]
mod skeleton_char_tests;
#[cfg(test)]
mod tests;
