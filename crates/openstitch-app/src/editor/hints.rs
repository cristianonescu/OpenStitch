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
            "Click the outline to add a point · drag points or handles; Ctrl+click or a drag box selects several, which move together · arrow keys move them 0.5 mm · right-click a point for its menu, the shape for Open and Close · Delete removes the selected points · C curve / S straight under the pointer · on a text: click a character, drag it, its top handle turns it, the others resize it",
            [
                Some(
                    "Shift: drag a point only horizontally or vertically · Shift+click adds a point to the selection · Shift+arrow grows the selection to the next or previous point, and shrinks it again · a handle moves as without Shift",
                ),
                Some(
                    "Ctrl: Ctrl+click adds a point to the selection or takes it out · Ctrl+arrow selects the next or previous point · a drag moves a point or a handle as without Ctrl",
                ),
                None,
            ],
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
        Tool::AngleLine => (
            "Drag a line across the selected fill, then the next one along it: its rows turn from line to line · Shift: 15° steps · Esc leaves the Angle Line tool",
            [None, FREE, None],
        ),
        Tool::FillAngle => (
            "Drag (or click twice) across the selected regions in the direction their rows should run · Shift: 15° steps · Esc leaves the Fill Angle tool",
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

/// What the Point tool has selected on an outline, for its hint line.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PointSelection {
    /// The points show and none is selected.
    None,
    /// One straight point (not an end of an open line).
    Straight,
    /// One curve point (not an end of an open line).
    Curve,
    /// One point, the first or last of an open line.
    LineEnd,
    /// Two or more points.
    Several,
    /// The shape's direction line (a click on one of its ends).
    DirectionLine,
    /// Add Direction Line is waiting for its two clicks.
    AddingDirectionLine,
}

/// The Point tool's hint on a stitch pattern (#1272), by what is selected: nothing, the
/// pattern's entry or exit point alone, or other points. Our own words.
pub fn needle_hint(selected: usize, end: bool) -> &'static str {
    match (selected, end) {
        (0, _) => {
            "Click a needle point to select it, or a stitch to add one there (Ctrl+Shift+click: as a jump) · a drag box selects the points inside it · right-click a point for Split at Point and Delete"
        }
        (1, true) => {
            "Drag the point or move it with the arrow keys · hold Alt to see the other patterns' ends it connects to, Alt+drag onto one joins the two · Delete removes it"
        }
        _ => {
            "Drag the points (Shift: only horizontally or vertically) or move them 0.1 mm with the arrow keys · Ctrl+arrow and Shift+arrow step along the stitches · Tab selects all the points of their colour, Home and End its first and last · Delete removes them"
        }
    }
}

/// The Point tool's hint for what is selected (#1260): PE-DESIGN changes its status-bar hint
/// with the selected point (with an end point of an open line it tells of Alt and connecting);
/// these are our own words for what each selection can do.
pub fn point_hint(selection: PointSelection) -> &'static str {
    match selection {
        PointSelection::None => {
            "Click a point to select it, or the outline to add one there · a drag box selects the points inside it, on every selected shape · right-click the shape for Open and Close"
        }
        PointSelection::Straight => {
            "Drag the point (Shift: only horizontally or vertically), or move it 0.5 mm with the arrow keys · right-click it for To Curve, Split at Point and Delete · Ctrl+click or Shift+click adds another point"
        }
        PointSelection::Curve => {
            "Drag the point, or the ends of its two handle lines to shape the curve · right-click it for To Straight, To Smooth, To Symmetry and To Cusp · arrow keys move it 0.5 mm"
        }
        PointSelection::LineEnd => {
            "Hold Alt and drag this end point onto the line's other end to connect the two ends into a closed shape, or onto an end of another open line to join the two lines into one (Alt marks each such end with an x) · arrow keys move it 0.5 mm · right-click it for To Curve and Delete"
        }
        PointSelection::DirectionLine => {
            "Drag an end of the direction line to turn the sewing direction · Delete removes the selected line, but a shape's last direction line cannot be deleted · Reverse Direction Line turns every line the other way"
        }
        PointSelection::AddingDirectionLine => {
            "Add Direction Line: click outside the shape for the line's start, then outside it on another side for its end · a line that ends inside the shape or crosses another line inside it is not added · Esc cancels"
        }
        PointSelection::Several => {
            "Drag one of the selected points to move them all (Shift: only horizontally or vertically), or use the arrow keys (0.5 mm) · Delete removes them · Ctrl+click or Shift+click adds a point, Ctrl+click on a selected one takes it out · right-click one for what applies to all"
        }
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
        // #1260: with the Point tool Shift locks a point's drag to an axis and Ctrl changes
        // no drag (PE-DESIGN's, the owner's hand check of 2026-11-19); both add by a click.
        let ctrl = Held {
            command: true,
            ..none
        };
        assert!(hint(Tool::Point, shift).contains("only horizontally or vertically"));
        assert!(hint(Tool::Point, shift).contains("Shift+click adds"));
        assert!(hint(Tool::Point, ctrl).contains("as without Ctrl"));
        assert!(!hint(Tool::Point, ctrl).contains("Snap"));
        assert!(hint(Tool::ShapeBuilder, alt).contains("removed"));
    }
}
