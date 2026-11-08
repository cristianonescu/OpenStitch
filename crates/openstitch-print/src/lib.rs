//! Printable output as PDF: actual-size and reduced prints with sewing information,
//! positioning sheets for split, multi-position and jumbo designs, Print and Stitch, and job
//! sheets for the client and the operator (#513).
//! Pure Rust and deterministic; see [`pdf`] for why the PDF writer is our own.

pub mod jobsheet;
pub mod pdf;
pub mod sheets;
pub mod texts;

pub use jobsheet::{Job, Sheet, job_sheet};
pub use pdf::{Image, Page, write};
pub use sheets::{Options, PAPERS, actual_size, positioning_sheets, print_and_stitch, reduced};
