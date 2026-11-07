//! Editable embroidery designs: geometry, design objects with sewing attributes,
//! stitch generators and the compiler from [`Design`] to a machine `StitchPattern`.
//! Pure computation: no I/O, deterministic output.

pub mod boolean;
pub mod builder;
pub mod calibration;
pub mod column;
pub mod compile;
pub mod concentric;
pub mod contour;
pub mod decorative;
pub mod direction;
pub mod effects;
mod envelope_presets;
pub mod extend;
pub mod finish;
pub mod fmath;
pub mod fonts;
pub mod freehand;
pub mod generate;
pub mod geometry;
pub mod graph;
pub mod hidden;
pub mod holes;
pub mod knife;
pub mod letter_lines;
pub mod line_looks;
pub mod lines;
pub mod machine;
pub mod model;
pub mod monogram;
pub mod ornament;
pub mod paths;
pub mod pattern_text;
pub mod patterns;
pub mod production;
pub mod project;
mod provenance;
pub mod region;
pub mod regions;
pub mod route;
pub mod sections;
pub mod sewing;
pub mod shapes;
pub(crate) mod shaping;
pub mod simplify;
pub mod skeleton;
pub mod source;
pub mod spiral;
pub mod stamp;
pub mod stitch_edit;
pub mod stroke_fonts;
pub mod strokes;
pub mod text;
pub mod timeline;
pub mod view;

pub use compile::Compiler;
pub use fonts::{DesignFont, FontBytes, FontData};
pub use geometry::{Bezier, Contour, Rect, Transform, Vec2, v2};
pub use machine::{
    BRANDS, BROTHER_HOOPS, Estimate, Hoop, HoopShape, MachineSettings, MachineType, OTHER_HOOPS,
    SewingArea, UserHoop, estimate, find_hoop,
};
pub use model::{
    Arrange, BackgroundImage, BackgroundLayer, ChainShape, ContourJoin, ContourStrategy, CrossKind,
    CrossTimes, Design, DesignObject, DesignPage, DesignProperties, Feather, FeatherSide,
    FillStitchType, Gradation, Guides, ImageAdjust, JobInfo, LineGap, LineSewing, LineType,
    ObjectKind, OptimizeOrder, RegionSewing, RegionType, RunningStitchPath, StaggerKind, StartEnd,
    StitchBlock, UnderlayDensity, UnderlayParts, UnderlayStyle,
};
pub use text::{
    CharEdit, FONTS, FontInfo, FontKind, Layout, PathFit, PixelStyle, Spread, TextAlign,
    TextDirection, TextError, TextMetrics, TextSpec, VAlign,
};

#[cfg(test)]
mod arc_tests;
#[cfg(test)]
mod concentric_spiral_tests;
#[cfg(test)]
mod contour_tests;
#[cfg(test)]
mod finish_tests;
#[cfg(test)]
mod stitch_type_tests;
#[cfg(test)]
mod tests;
