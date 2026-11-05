//! Length fields in the chosen unit (#519). The value stays in millimetres; the field shows it
//! in mm or inches and parses what is typed with [`units::parse`], so `2"` and `50.8 mm` give
//! the same value in either mode and a size typed in inches is stored as the exact mm value.

use eframe::egui;
use openstitch_app::editor::units::{self, Units};

/// A drag field for a length in mm, shown and typed in `u`. Callers chain `range` (in mm)
/// and `speed` as for any `DragValue`.
pub fn drag(mm: &mut f64, u: Units) -> egui::DragValue<'_> {
    egui::DragValue::new(mm)
        .custom_formatter(move |v, _| units::number(u, v))
        .custom_parser(move |s| units::parse(s, u))
        .suffix(units::suffix(u))
        .speed(match u {
            Units::Mm => 0.1,
            Units::Inch => 0.254,
        })
}
