//! The hints line (#550): what clicks and keys do now with the active tool, shown in the
//! status bar. Holding Shift, Ctrl (Cmd on macOS) or Alt shows what that key changes where
//! the tool uses it.
//!
//! Nothing here depends on egui.

use super::tools::Tool;

/// Modifier keys held now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Held {
    pub shift: bool,
    /// Ctrl, or Cmd on macOS.
    pub command: bool,
    pub alt: bool,
}

/// Every tool's hint: the text with no key held, then what Shift, Ctrl and Alt change
/// (`None` where the tool does not use the key).
pub fn table(tool: Tool) -> (&'static str, [Option<&'static str>; 3]) {
    const FREE: Option<&str> = Some("Ctrl: ignore Snap to Grid and Snap to Objects for this drag");
    match tool {
        Tool::Select => (
            "Click selects · drag moves · handles resize and turn · drag on empty canvas selects by box · arrows nudge · Ctrl+K finds any command",
            [
                Some(
                    "Shift: add or remove from the selection · move along one axis · resize in proportion · turn by 15°",
                ),
                FREE,
                Some(
                    "Alt: click selects a group member alone, or every stitch pattern of the colour under the pointer",
                ),
            ],
        ),
        Tool::Rectangle | Tool::Ellipse => (
            "Drag to draw · Esc cancels",
            [
                Some(if matches!(tool, Tool::Rectangle) {
                    "Shift: draw a square"
                } else {
                    "Shift: draw a circle"
                }),
                FREE,
                None,
            ],
        ),
        Tool::Polygon => (
            "Click to add points · click the first point, double-click or Return to close · Esc cancels",
            [None, FREE, None],
        ),
        Tool::Point => (
            "Drag points or handles · double-click an edge to add a point · Delete removes it · C curve / S straight under the pointer · on a text: click a character, drag it, its top handle turns it, the others resize it",
            [None, FREE, None],
        ),
        Tool::Pen => (
            "Click for a corner, drag for a smooth point · click the first point to close · Return finishes open · Esc cancels",
            [None, FREE, None],
        ),
        Tool::Punch | Tool::CurvedBlock => (
            "Click both ends of each sewing-direction line in turn (one side first) · double-click or Return finishes · Esc cancels",
            [None, FREE, None],
        ),
        Tool::Line | Tool::Running | Tool::Feed => (
            "Click the points of the line · double-click or Return finishes · Esc cancels",
            [None, FREE, None],
        ),
        Tool::Pencil | Tool::PencilOpen => (
            "Drag to draw freehand · let go to finish · Esc cancels",
            [None, None, None],
        ),
        Tool::Skeleton => (
            "Click for a corner, drag for a smooth point · drag the handles beside a point to set its width · double-click or Return finishes · Esc cancels",
            [None, FREE, None],
        ),
        Tool::Knife => (
            "Click the points of a cut line across the objects · double-click or Return cuts · Esc leaves the Knife",
            [None, FREE, None],
        ),
        Tool::Gradient => (
            "Drag (or click twice) from where the selected gradient starts to where it ends · Shift: 15° steps · Esc leaves the Gradient tool",
            [None, FREE, None],
        ),
        Tool::ShapeBuilder => (
            "Drag across the pieces of overlapping shapes to merge them · Esc cancels",
            [
                None,
                None,
                Some("Alt: the pieces dragged across are removed"),
            ],
        ),
    }
}

/// The hint for `tool` with the keys `held`: the text of the first held key the tool uses,
/// else its plain text.
pub fn hint(tool: Tool, held: Held) -> &'static str {
    let (plain, keys) = table(tool);
    [held.shift, held.command, held.alt]
        .into_iter()
        .zip(keys)
        .find_map(|(on, text)| text.filter(|_| on))
        .unwrap_or(plain)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tool_has_a_hint_and_held_keys_change_it_where_used() {
        let none = Held::default();
        let shift = Held {
            shift: true,
            ..none
        };
        let alt = Held { alt: true, ..none };
        for t in Tool::ALL {
            assert!(!hint(t, none).is_empty(), "{t:?}");
            let (_, keys) = table(t);
            assert_eq!(hint(t, shift) != hint(t, none), keys[0].is_some(), "{t:?}");
            assert_eq!(hint(t, alt) != hint(t, none), keys[2].is_some(), "{t:?}");
        }
        assert!(hint(Tool::Select, shift).starts_with("Shift"));
        assert!(hint(Tool::ShapeBuilder, alt).contains("removed"));
    }
}
