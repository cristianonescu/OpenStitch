//! What the AI assistant can do in the editor (B17, docs/research/beyond/ai-integration.md).
//! The engine's command registry covers every design change. This module adds the `ui.*`
//! commands for what only the window holds (selection, tools, view, panes, simulator,
//! preferences, Help and updates), and the [`route`] that says how the assistant reaches each
//! ribbon and menu [`Action`]. The binary's `assistant_ui.rs` runs the commands; its tests fail
//! when an action, an Options or Help menu item, or an engine command has no route, so a new
//! feature cannot be left out of the chat.
//!
//! Nothing here depends on egui.

use super::ribbon::Action;
use openstitch_engine::{Effect, ParamDoc};
use serde_json::{Value, json};

/// One editor command: listed by `commands.list` beside the engine's, in the same shape.
#[derive(Clone, Copy, Debug)]
pub struct UiCommand {
    pub id: &'static str,
    pub summary: &'static str,
    /// `Read` changes nothing the user keeps; `Edit` changes the window or the preferences
    /// (refused to read-only clients); `File` writes a file.
    pub effect: Effect,
    pub params: &'static [ParamDoc],
}

const fn p(
    name: &'static str,
    kind: &'static str,
    required: bool,
    summary: &'static str,
) -> ParamDoc {
    ParamDoc {
        name,
        kind,
        required,
        summary,
    }
}

pub const UI_COMMANDS: &[UiCommand] = &[
    UiCommand {
        id: "ui.state",
        summary: "The editor's state: file, unsaved changes (unsaved: anything a save would write; unsavedDesign: the design itself changed since the last save; unsavedConversationOnly: only the Assistant's conversation did, as right after your own save while your reply goes on: the file has the design, and the next save adds the rest of the conversation), selection, tool, view, zoom (zoomPercent), where the canvas looks (view: centerMm, the design point in the middle of the canvas in mm; canvasPoints, the canvas's width and height in points; pagePointsInCanvas, the width and height in points of the Design Page's part inside the canvas, 0 when the page is out of sight; canvasAt, the canvas's top-left corner in the window in points, which moves when a pane or the ribbon changes size; null before the canvas is first drawn), the menus, pop-ups and dialogs on screen (surfaces: windowPoints, the window's width and height in points; textScale, Options ▸ Text size as a factor, points times it are the page's pixels in the browser version; optionsButton, the top bar's Options button as [x, y, width, height] in points, null when not drawn; open, one entry per menu, list or dialog drawn in the last frame with kind menu or dialog, rect, the part of the window its content is seen through, contentHeight, more than the rect's height when it scrolls, scrolled, how far down it is scrolled, and buttons, the rect of a dialog's button row when it is kept in view below the scrolled content, else null), panes, simulator, Check Design's indicator (check: level readyToSew, sewableWithSmallRisks or fixThisFirst, with the error, warning and note counts) and every setting",
        effect: Effect::Read,
        params: &[],
    },
    UiCommand {
        id: "ui.actions",
        summary: "Every ribbon, menu and Quick Access Toolbar command: key, label, what it does, shortcut, whether it can run now, and how the assistant reaches it (route)",
        effect: Effect::Read,
        params: &[p(
            "filter",
            "string",
            false,
            "Only commands whose key, label or text contains this (any case)",
        )],
    },
    UiCommand {
        id: "ui.run",
        summary: "Run a ribbon or menu command by key, as a click on it would (ui.actions lists the keys). Commands that need a file name or a choice from a list say which command to use instead",
        effect: Effect::Edit,
        params: &[p(
            "action",
            "string",
            true,
            "Command key from ui.actions, e.g. stitchView, paneSimulator, rectangle",
        )],
    },
    UiCommand {
        id: "ui.select",
        summary: "Set the selection the user sees (ids from object.list)",
        effect: Effect::Read,
        params: &[
            p("ids", "[int]", false, "Object ids"),
            p(
                "mode",
                "set|add|remove|all|none",
                false,
                "set (default), add, remove, all objects or none",
            ),
        ],
    },
    UiCommand {
        id: "ui.zoom",
        summary: "Zoom the view: a percentage (100 % = real size, calibrated per monitor), fit the Design Page or the selection, or zoom to a rectangle",
        effect: Effect::Read,
        params: &[
            p("percent", "percent", false, "Zoom, 5-3200 %"),
            p(
                "fit",
                "page|selection",
                false,
                "Fit the Design Page or the selection",
            ),
            p(
                "rect",
                "[x, y, width, height] mm",
                false,
                "Zoom Box: this rectangle fills the view",
            ),
        ],
    },
    UiCommand {
        id: "ui.threadLabels",
        summary: "What a colour list shows (#798): for each swatch the code drawn beside it (the thread's own chart code: Brother 007, a maker's or the user's code, #rrggbb for a custom colour), name, brand and colour. list: picker (the colour picker's default chart), pecPicker (its Brother PEC tab), sewingOrder (the Sewing Order pane's object rows), colourPane (thread colours in sewing order), changeColor (Change Color for the selected stitch pattern)",
        effect: Effect::Read,
        params: &[p(
            "list",
            "string",
            false,
            "picker (default), pecPicker, sewingOrder, colourPane or changeColor",
        )],
    },
    UiCommand {
        id: "ui.browserLimits",
        summary: "What the browser version cannot do (#830): inBrowser, the notice shown now (shown), the keys the browser keeps for itself and never passes to the editor (browserKeys: Ctrl+N, Ctrl+T, Ctrl+W; use the menus there, #834) and each limit (limits: print, transfer, updates, folders, clipboardImages, systemFonts: lettering in the computer's own fonts, #833) with what it is, what to do instead, the issue that tracks it and the message the browser shows when it is used. The Assistant itself works in the browser (#766)",
        effect: Effect::Read,
        params: &[],
    },
    UiCommand {
        id: "ui.settings",
        summary: "Every editor setting the user can change outside the design (view, grid, rulers, panes, theme, updates, realistic view, font filter, simulator speed), with its value and allowed values",
        effect: Effect::Read,
        params: &[],
    },
    UiCommand {
        id: "ui.set",
        summary: "Change one editor setting (names from ui.settings); saved like a change made in the window",
        effect: Effect::Edit,
        params: &[
            p("name", "string", true, "Setting name from ui.settings"),
            p(
                "value",
                "any",
                true,
                "New value: boolean, number, string or list, as ui.settings describes",
            ),
        ],
    },
    UiCommand {
        id: "ui.render",
        summary: "A PNG of the design as the editor holds it (Design Page and stitches in thread colours), base64, with the stitch statistics; it also shows the density overlay while it is on (ui.set densityOverlay) and Check Design's rings and out-of-hoop stitches while its Check tab is open, as the canvas does",
        effect: Effect::Read,
        params: &[
            p(
                "size",
                "int 64-2048",
                false,
                "Longest image side in pixels (default 800)",
            ),
            p(
                "showTravel",
                "bool",
                false,
                "Draw jumps as dashed lines (default false)",
            ),
            p(
                "layers",
                "both|underlay|top",
                false,
                "Underlay only or top stitching only (default: View ▸ Show's setting)",
            ),
            p(
                "colorBy",
                "thread|order|mono",
                false,
                "Colour by thread, sewing order or one colour (default: View ▸ Show's setting)",
            ),
            p("only", "[int]", false, "Only these objects' stitches"),
            p(
                "solo",
                "int",
                false,
                "Fade every colour block but this one (0-based)",
            ),
            p("needlePoints", "bool", false, "A dot at every needle drop"),
            p("trimMarks", "bool", false, "A red cut mark at every trim"),
        ],
    },
    UiCommand {
        id: "ui.simulate",
        summary: "Stitch Simulator and stitch-out player: show the design sewn up to a point (a fraction, a stitch, or a step by block, object or trim), play or stop, the speed and the player toggles",
        effect: Effect::Read,
        params: &[
            p(
                "position",
                "factor",
                false,
                "Fraction sewn, 0-1 (1 shows the whole design)",
            ),
            p(
                "play",
                "bool",
                false,
                "Start (true) or stop (false) playing",
            ),
            p(
                "speed",
                "factor",
                false,
                "Stitches per second while playing, 1-5000",
            ),
            p(
                "stitch",
                "int",
                false,
                "Go to this many stitches sewn (design.timeline gives block and object starts)",
            ),
            p(
                "step",
                "start|end|stitch|stitchBack|block|blockBack|object|objectBack|trim",
                false,
                "Step the player: a stitch, to the next or previous colour block or object, or to the next trim",
            ),
            p(
                "auto",
                "bool",
                false,
                "Auto speed: the whole design in about 12 s",
            ),
            p(
                "pauseAtTrims",
                "bool",
                false,
                "Stop playing at each trim, stop and colour change",
            ),
            p("followNeedle", "bool", false, "Keep the needle in view"),
        ],
    },
    UiCommand {
        id: "ui.help",
        summary: "Help menu: with no item, lists the items; with one, opens it (manual, ready-to-sew designs, keyboard shortcuts, About). sewKit replies with the packaged kit's folder it showed in the file manager, or the GitHub url it opened instead",
        effect: Effect::Read,
        params: &[p(
            "item",
            "manual|sewKit|sewKitOnline|shortcuts|checkUpdates|about",
            false,
            "Help item to open",
        )],
    },
    UiCommand {
        id: "ui.update",
        summary: "Updates: check GitHub for a newer release now; the update window then offers the download, which the user confirms",
        effect: Effect::Read,
        params: &[],
    },
    UiCommand {
        id: "ui.scratchWrite",
        summary: "Write an SVG (text) or a picture (base64 PNG/JPEG/BMP) into the assistant's scratch folder and return its path, for import.svg, background.set or digitize.*",
        effect: Effect::File,
        params: &[
            p(
                "name",
                "string",
                true,
                "File name with extension: .svg, .png, .jpg, .jpeg or .bmp; no folders",
            ),
            p("text", "string", false, "Text content (SVG)"),
            p("base64", "string", false, "Binary content, base64"),
        ],
    },
    UiCommand {
        id: "ui.scratchView",
        summary: "A picture in the assistant's scratch folder (an attached one, or one written there) as base64 for the model to look at (#1168): scaled to at most 2048 px on its longest side and 3.75 MB, as attached pictures are sent; the file itself is not changed",
        effect: Effect::Read,
        params: &[p(
            "name",
            "string",
            true,
            "The picture's file name in the scratch folder: .png, .jpg, .jpeg or .bmp; no folders",
        )],
    },
    UiCommand {
        id: "ui.askUser",
        summary: "The Assistant's clarifying questions (#1172), during its own turn only: shown on a card with answer chips (the first marked Recommended, #1205) and a field for a typed answer, plus Send answers and Use your judgment. It returns at once; end the turn, and the answers come as the user's next message: a skipped question, or Use your judgment, takes the recommended (first) answer. Texts are made plain and cut (question 200, choice 60 characters); choices that are not answers (\"I'll type it\") are dropped (#1198)",
        effect: Effect::Read,
        params: &[
            p(
                "questions",
                "[{question, choices}]",
                true,
                "1 to 10 questions, each with 2 to 4 different answer choices, the recommended one first; ask 5 to 10 only for a new design or a large change with open choices. A choice like \"I'll type it\" is dropped: the user types in the card's field",
            ),
            p(
                "intro",
                "string",
                false,
                "One short line before the questions",
            ),
        ],
    },
    UiCommand {
        id: "ui.suggestReplies",
        summary: "What the user may say next (#1172), during the Assistant's own turn only: one suggested message (Send or Edit) and 1 to 4 chips, shown when the turn ends; a click only sends that text as the user's message. Duplicates go; texts are made plain and cut (chip 80, message 300 characters)",
        effect: Effect::Read,
        params: &[
            p(
                "message",
                "string",
                true,
                "The one suggested reply, as the user would write it",
            ),
            p("chips", "[string]", true, "1 to 4 short replies"),
        ],
    },
    UiCommand {
        id: "ui.notesRead",
        summary: "The Assistant's memory (#491): the notes it keeps about this user between conversations (machine, threads, fabric, preferences, lessons), as Markdown, with its size and limit; an error when the user turned memory off",
        effect: Effect::Read,
        params: &[],
    },
    UiCommand {
        id: "ui.notesWrite",
        summary: "Remember a lasting fact the user stated or a correction they confirmed: append a line to, or replace, one section of the Assistant's notes. Always asks the user first; never store passwords, keys or personal data the user did not give for this",
        effect: Effect::Settings,
        params: &[
            p(
                "section",
                "Machine|Threads|Fabric|Preferences|Lessons",
                true,
                "The notes section",
            ),
            p(
                "text",
                "string",
                true,
                "The fact, short (at most 1000 characters); for replace, the section's whole new text (empty removes it)",
            ),
            p(
                "mode",
                "append|replace",
                false,
                "append (default) adds one line; replace rewrites the section, for example to summarise it",
            ),
        ],
    },
    UiCommand {
        id: "ui.searchCommands",
        summary: "Command palette search (#550): ribbon, menu and Quick Access commands matching words from their label, ScreenTip, key or ribbon path, best first (recently used first for an empty query), with where each lives, its shortcut and whether it can run now; run one with ui.run",
        effect: Effect::Read,
        params: &[
            p(
                "query",
                "string",
                false,
                "Words to find, e.g. \"arrange copy\", \"satin\", \"export\" (empty: recent commands first, then all)",
            ),
            p("limit", "int 1-500", false, "Most results (default 20)"),
        ],
    },
    UiCommand {
        id: "ui.recoveryList",
        summary: "Autosave and recovery (#549): the snapshots of designs whose window crashed or lost power (id, design name, original file, time), and whether autosave is on",
        effect: Effect::Read,
        params: &[],
    },
    UiCommand {
        id: "ui.recoveryRestore",
        summary: "Restore a crashed window's design as an untitled design in this window (the original file is never written; save it with project.save). Refused while the open design has unsaved changes",
        effect: Effect::Edit,
        params: &[p("id", "string", true, "Snapshot id from ui.recoveryList")],
    },
    UiCommand {
        id: "ui.recoveryDiscard",
        summary: "Delete recovery snapshots crashed windows left (Options ▸ Clear Recovery Files for all). Deletes files: ask the user first",
        effect: Effect::File,
        params: &[
            p("id", "string", false, "Snapshot id from ui.recoveryList"),
            p(
                "all",
                "bool",
                false,
                "true: every snapshot ui.recoveryList lists",
            ),
        ],
    },
    UiCommand {
        id: "ui.layout",
        summary: "Saved window layouts (#551, Options ▸ Layouts): list them, save where the panes are and the toolbar position under a name, put one in place, or delete one. Delete, and a save over an existing name, ask the user first",
        effect: Effect::Edit,
        params: &[
            p(
                "op",
                "list|save|apply|delete",
                false,
                "list (default), save, apply or delete",
            ),
            p(
                "name",
                "string",
                false,
                "Layout name, 1-40 characters (save, apply, delete)",
            ),
        ],
    },
    UiCommand {
        id: "ui.resetPrefs",
        summary: "Options ▸ Reset Preferences (#551): put one area of the preferences back to its defaults and keep the others. Always asks the user first (preferences have no undo)",
        effect: Effect::Edit,
        params: &[p(
            "area",
            "display|editing|panes|shortcuts|layouts|newObjects",
            true,
            "display (theme, accent, toolbar, splash, view), editing (units, nudge, snapping), panes, shortcuts, layouts or newObjects (new-shape defaults, Basic/Advanced, start cards)",
        )],
    },
    UiCommand {
        id: "ui.gettingStarted",
        summary: "Getting started (#554): without parameters, the start cards (shown while the design is empty), the kit's samples, whether the cards show (cardsShown), were closed this session (dismissed) and are turned on (enabled, ui.set showStartCards); card runs one card's action; sample opens a kit design as a new untitled design (asks about unsaved changes first, as Open does); page opens or closes Help ▸ Getting Started; cards dismiss closes the cards for this session (as their ×, Escape or a click on the canvas) and show brings them back, neither changing the preference (#797)",
        effect: Effect::Edit,
        params: &[
            p(
                "card",
                "draw|lettering|picture|import|template|sample",
                false,
                "Run this start card",
            ),
            p(
                "sample",
                "sampler|shapes|frames|lettering|monograms|applique",
                false,
                "Open this sample",
            ),
            p(
                "page",
                "bool",
                false,
                "Show or hide the Getting Started page",
            ),
            p(
                "cards",
                "dismiss|show",
                false,
                "Close the start cards for this session, or show them again",
            ),
        ],
    },
    UiCommand {
        id: "ui.startupWizard",
        summary: "The Startup Wizard (#657, PE-DESIGN's \"How do you want to produce embroidery?\"): without parameters it opens; choice closes it and runs that choice's action (manual = Getting Started, template, images = Image to Embroidery, hoopAndFabric = Design Settings and the Fabric Selector, importText, importShapes = the Import pane, importLibrary = Open Sample, open = File ▸ Open, database); close closes it",
        effect: Effect::Edit,
        params: &[
            p(
                "choice",
                "manual|template|images|hoopAndFabric|importText|importShapes|importLibrary|open|database",
                false,
                "Run this choice",
            ),
            p("close", "bool", false, "Close the wizard"),
        ],
    },
    UiCommand {
        id: "ui.documents",
        summary: "Designs open as tabs (#552): list them (index, name, unsaved, file, active), activate one, open an empty one, or close one. Commands act on the active tab. Closing a tab with unsaved changes is refused: save it first or ask the user",
        effect: Effect::Edit,
        params: &[
            p(
                "op",
                "list|activate|new|close",
                false,
                "list (default), activate, new or close",
            ),
            p(
                "index",
                "int",
                false,
                "Tab index from the list (activate; close: default the active tab)",
            ),
        ],
    },
    UiCommand {
        id: "ui.loading",
        summary: "A File ▸ Open or Import running in the background (#558): file, open or import, stage and seconds, or running: false. cancel: true stops it; the design stays as it was and no undo step is recorded",
        effect: Effect::Edit,
        params: &[p(
            "cancel",
            "bool",
            false,
            "true: stop the running open or import",
        )],
    },
    UiCommand {
        id: "ui.pointMenu",
        summary: "The Select Point tool's right-click menu on a point (#1258): its seven entries in order (toStraight, toCurve, toSmooth, toSymmetry, toCusp, splitAtPoint, deletePoint), each with its text, whether a separator sits above it and whether it is available for the selected points, and whether the menu is open. points (with contour) selects those vertices of the one selected shape with the Point tool, as clicks and Ctrl+clicks or Shift+clicks do; with several plain shapes selected (ui.select) the points of all of them show, and points is then a list of {id, contour, index}, on any of them and any of their outlines, as a drag box over them selects. directionLines lists the selected shape's direction lines as the Point tool draws them: the lines added with Add Direction Line (stored: true; two or more, in order across the shape; the fill's rows fan between them, object.addDirectionLine) or the one line that is its fill angle (stored: false); each with index, angle, from, to, screenFrom, screenTo and selected. Empty when the shape has none (no region, Auto direction, a curved direction, Manual Punch lines). addingDirectionLine is true while Add Direction Line (ui.run addDirection) waits for its two clicks on the canvas; from a script add the line with object.addDirectionLine instead. selectDirectionLine takes true (the first line), a line's index, or false. A click on one of a line's ends selects it the same way. directionLine is the selected line, else the first one (null when the shape has none): index, stored, angle in degrees, from and to in mm (from: the end with the square, to: the end with the arrow head), screenFrom and screenTo in the window, and selected. The one line that is the fill angle (stored: false) lies through the middle of the shape's bounds, its ends 5 mm outside them, and its angle is the region's direction (object.setRegion angle): dragging one of its ends turns the direction about the middle of the shape, the same as object.setRegion {id, angle}; it cannot be deleted (with it selected the Delete key does nothing and Delete Direction Line stays grey). A stored line lies where it was put, its ends 5 mm outside the outline: dragging one of its ends moves that end (object.moveDirectionLine; an end dropped inside the shape, or a line that would cross another inside it, is refused and the line stays where it was), and the selected one is deleted by the Delete key and by deleteSelectedLine. The selection is the line itself, not its index: after an undo, a redo or a command that deletes or moves lines the same line is still the selected one, and when that line is gone none is. connectable lists the ends the one selected end point of an open line can be connected to, as Alt held shows them with an x (id, index, x and y in mm, screen): the same line's other end and both ends of every other open line. connectTo {id, index} connects it to one of them as Alt + drag onto it does, in one undo step: its own other end closes the line as Close does; an end of another line joins the two lines into one object (object.setPoint, then shape.joinLines), which keeps its id, its sewing attributes and its place in the sewing order, the two ends one point. A refused join changes nothing (the end is not left moved, the point stays selected). connectTo cannot be combined with action or step in one call (it is refused then). connectOverlap is the end the Alt-dragged end point lies on now, where the red square is drawn (x, y in mm, screen), null otherwise. step moves the selection along the outline as Ctrl+arrow does (next: Right or Down, previous: Left or Up): only the neighbour of the point the selection started at is selected then, and it stays on the end point of an open line; with extend: true it is Shift+arrow, which grows the selection by one point from the point it started at and shrinks it again the other way. In both no point moves; a closed outline's order goes round (not captured on PE-DESIGN), and with points on several outlines the keys work on the outline of the point selected first. action runs an entry on the selected points as a click in the menu does and is refused when the entry is greyed out; with points on several outlines or shapes it sends one engine command per outline (object.setPointKind, object.deletePoint), all in one undo step, and the greying is over all the selected points: toStraight when one of them is a curve point, toCurve when one is straight, splitAtPoint only with exactly one point, deletePoint always (an outline that would keep fewer than 2 points is left as it is). menuRect is where the open menu (the point's or the object's) was painted, [x, y, width, height] in window points, and menuFill its panel colour (#rrggbb): both null until a frame has painted the menu (its first frame only measures it), so open: true with menuRect: null means the menu is still opening. hint is the hint line the status bar shows for what is selected (it changes with the selection: an end point of an open line tells of Alt and connecting the ends). selected lists the selected points (id of the shape, contour, index, x and y in mm, and screen: where the point is in the window, in points from its top-left corner) and pointCount the number of points that show (of every outline of every shape the Point tool edits; null when none does). objectMenu lists the entries of the menu a right-click on the shape itself opens (addDirection, deleteDirection, reverseDirection, then openOutline and closeOutline) with what is greyed out; run one with ui.run. deleteDirection's entry carries submenu, the list of its sub-menu's entries, each with key, label and enabled, as PE-DESIGN's: with one of two or more direction lines selected (selectDirectionLine) deleteSelectedLine removes the selected line and deleteLinesNotSelected removes every line but the selected one, which is not selected afterwards (object.deleteDirectionLine with index or keep; one undo step each); with no line selected, or with the shape's only line selected, both are grey: deleteSelectedLine (grey: a shape's only direction line cannot be deleted) and deleteLinesNotSelected (grey: there is no other line to delete). On a shape with a curved direction or Manual Punch lines, of which none can be selected, deleteLinesNotSelected removes them. Run either with action, as the point entries (refused while it is grey). The engine commands object.setPointKind, object.splitAtPoint, object.deletePoint, object.movePoints, object.openOutline and object.closeOutline do the same without the window",
        effect: Effect::Edit,
        params: &[
            p(
                "points",
                "[int] | [{id, contour, index}]",
                false,
                "Select these vertices (switches to the Point tool). Indexes are on the one selected shape (or the shape 'id' names); {id, contour, index} names a point of any selected plain shape, so one list can hold points of several shapes and outlines. Refused, selecting nothing, when a point does not exist on a shape whose points show",
            ),
            p(
                "contour",
                "int",
                false,
                "The contour of the points given as indexes, and of an {id, index} without one (default 0)",
            ),
            p(
                "id",
                "int",
                false,
                "The selected shape the points given as indexes are on (default: the one selected shape)",
            ),
            p(
                "selectDirectionLine",
                "bool | int",
                false,
                "Select one of the shape's direction lines (directionLines): its index, or true for the first; false deselects. Switches to the Point tool; the outline points are deselected. Refused when the selected shape shows no such line",
            ),
            p(
                "connectTo",
                "{id, index}",
                false,
                "Connect the one selected end point of an open line to this end (an entry of connectable), as Alt + drag onto it does. Refused, changing nothing, without exactly one end point selected or for an end not listed",
            ),
            p(
                "step",
                "next | previous",
                false,
                "Move the selection to the next or previous point of the outline (Ctrl+arrow); no point moves. Refused with no point selected",
            ),
            p(
                "extend",
                "bool",
                false,
                "With step: grow or shrink the selection by one point instead (Shift+arrow). Default false",
            ),
            p(
                "action",
                "toStraight|toCurve|toSmooth|toSymmetry|toCusp|splitAtPoint|deletePoint|deleteSelectedLine|deleteLinesNotSelected",
                false,
                "Run this entry: one of the seven on the selected points, or an entry of the Delete Direction Line sub-menu on the selected shape",
            ),
        ],
    },
    UiCommand {
        id: "ui.objectMenu",
        summary: "The Select tool's right-click menu on objects (#1288), PE-DESIGN 11.50's object menu: rows in order, each an entry (key, label, shortcut as this platform writes it, icon, enabled for the selection), a sub-menu (key, label, submenu: its entries) or a separator: for one selected object cut, copy, duplicate, paste (sub-menu: paste, pasteAttributes), delete; flip (sub-menu: flipVertical Ctrl+H, flipHorizontal Ctrl+J; Cmd+Shift+H for Flip Vertical on macOS), numericalSetting (sub-menu: sizeNumeric, rotateNumeric), center Ctrl+M (Cmd+Shift+M on macOS); openOutline, closeOutline; offsetLines, toStitches, floral; cutAndSew (sub-menu: applique, patch, cutwork); sewingAttributes (shows the Sewing Attributes pane). For two or more (a group too): group, ungroup; cut, copy, duplicate, paste, delete; flip, numericalSetting, center, align (sub-menu: alignLeft, alignCenter, alignRight, alignTop, alignMiddle, alignBottom); openOutline, closeOutline; modifyOverlap (sub-menu: removeOverlaps, merge, setHoleSewing, cancelHoleSewing), embossEngrave (sub-menu: engravedLine, engravedRegion, embossedRegion); toStitches; cutAndSew; sewingAttributes. PE-DESIGN's Paste Selected Attributes... and Export Cutting Data are not in it (OpenStitch has neither); a text is treated as a shape (PE-DESIGN's menu on text was not captured). sizeNumeric and rotateNumeric open PE-DESIGN's Size and Rotate dialogs (dialog reports the open one: kind, percent, widthPercent, heightPercent, widthMm, heightMm, maintainAspectRatio, degrees, previewed); numeric sets their fields (percent: true for the % row, width, height, maintainAspectRatio, degrees clockwise) and presses button ok, preview or cancel: Preview applies the setting, Cancel takes a preview back, OK applies it as one undo step. open tells whether it is open, at where it opened (window points), submenuOpen the key of the open sub-menu, menuRect [x, y, width, height] and menuFill (#rrggbb) where it was painted (null until a frame has painted it: its first frame only measures it), bar PE-DESIGN's small bar beside the menu (above it for one object, below for several): line and region (the selection's sew types, null when it has none or mixed ones), lineColor and regionColor (#rrggbb; null when the selected objects have several, then lineColorMixed or regionColorMixed is true and the swatch is the pane's Mixed one), below and rect; its buttons open the ribbon's Line Sew Type, Line Color, Region Sew Type and Region Color lists (object.setLine, object.setRegion do the same). selection the selected object ids and selectionRect where they are in the window ([x, y, width, height] in points; a right-click inside opens the menu for them); every entry and sub-menu row carries rect, where it was painted last (null until then; a sub-menu's entries only while it is open), so a script can point at it. at [x, y] opens it as a right-click there does: the object under the pointer is selected first unless it is already selected (several selected objects or a group stay selected), refused on empty canvas or with another tool than Select. action runs an entry (a row's key or a sub-menu entry's key) on the selection as a click on it does, each one undo step, refused while the entry is greyed out; close: true closes the menu. The same commands run from the ribbon (ui.run) and the shortcuts; the engine commands are object.flip, object.center, object.duplicate, object.paste, object.delete and the others the ribbon names",
        effect: Effect::Edit,
        params: &[
            p(
                "at",
                "[x, y]",
                false,
                "Open the menu as a right-click at this window point does (points from the window's top-left corner): selects the object there first when it is not selected. Refused on empty canvas or with another tool than Select",
            ),
            p(
                "action",
                "string",
                false,
                "Run this entry on the selection (a key from rows or a sub-menu), as a click on it does; refused while it is greyed out",
            ),
            p("close", "bool", false, "true: close the menu"),
            p(
                "numeric",
                "{percent, width, height, maintainAspectRatio, degrees, button}",
                false,
                "Fill in the open Size or Rotate dialog and press a button (ok, preview, cancel); width and height are % with percent: true, else mm; with maintainAspectRatio the height follows the width. Values outside the dialog's fields (1-1000 %, 0.1-2000 mm, -360 to 360 degrees) are kept to them",
            ),
        ],
    },
    UiCommand {
        id: "ui.guides",
        summary: "The guidelines as the canvas shows them (#799): each one's index, position (mm from the ruler origin, as its label reads) and label, whether they show, are locked and snap, the selected one and an open Set Position… dialog. orientation + index select one as a click does; then position (a length as typed in Set Position…: 12.5, \"12.5 mm\", \"1/2\\\"\"; from the ruler origin), turn (Alt-click) or delete (the Delete key) act on it, one undo step each; newGuide {orientation, position} is New Guide… (from the ruler origin). It also gives the guideline colour and an open right-click menu. Engine commands view.* do the same in page coordinates",
        effect: Effect::Edit,
        params: &[
            p(
                "orientation",
                "vertical|horizontal",
                false,
                "Select this guideline (with index)",
            ),
            p("index", "int", false, "Which one, from 0"),
            p(
                "position",
                "length",
                false,
                "Set Position… for the selected one, from the ruler origin",
            ),
            p(
                "turn",
                "bool",
                false,
                "Turn the selected one vertical <-> horizontal",
            ),
            p("delete", "bool", false, "Delete the selected one"),
            p(
                "newGuide",
                "{orientation, position}",
                false,
                "New Guide…: a guideline at a length from the ruler origin",
            ),
        ],
    },
    UiCommand {
        id: "ui.cursor",
        summary: "Keyboard drawing (#499) with the active drawing tool (pick one with ui.run, e.g. polygon, pen, punch): x, y (mm) or dx, dy move the drawing cursor; action place clicks there (Rectangle and Ellipse: first place starts, second ends), smooth places a smooth Pen or Satin Skeleton point, back removes the last point, finish ends the drawing (one undo step, the same command a mouse drawing makes), cancel drops it; width (mm) sets the next point's width for the Pen (making a satin line) and Satin Skeleton (#707)",
        effect: Effect::Edit,
        params: &[
            p("x", "mm", false, "Cursor x on the Design Page"),
            p("y", "mm", false, "Cursor y"),
            p("dx", "mm", false, "Move the cursor by this much"),
            p("dy", "mm", false, "Move the cursor by this much"),
            p(
                "action",
                "place|smooth|back|finish|cancel",
                false,
                "What to do at the cursor, after moving it",
            ),
            p(
                "width",
                "mm 0.1-30",
                false,
                "Next point's width (Pen, Satin Skeleton)",
            ),
        ],
    },
    UiCommand {
        id: "ui.preview",
        summary: "Preview a design change on the canvas without making it (#656, as hovering a picture picker's tile does): command and params of an editing command (e.g. object.setRegion with a type or pattern) show its stitches instead of the design's, with no undo step; clear: true ends it, any edit drops it; without parameters, whether a preview is on. Run the command itself to keep the change",
        effect: Effect::Edit,
        params: &[
            p(
                "command",
                "string",
                false,
                "An editing command id, e.g. object.setRegion",
            ),
            p("params", "object", false, "Its parameters"),
            p("clear", "bool", false, "true: end the preview"),
        ],
    },
    UiCommand {
        id: "ui.stitchTable",
        summary: "Stitch Table (#501): open or close the table of the selected stitch pattern's needle points (index, X and Y mm, command, colour block) and select rows (shown on the canvas with the Point tool). Edit the points with stitches.setPoint, stitches.insertPoint and stitches.deletePoints (stitches.points lists them); each is one undo step",
        effect: Effect::Edit,
        params: &[
            p("open", "bool", false, "Show or hide the table"),
            p(
                "select",
                "[int]",
                false,
                "Rows (needle point indexes) to select",
            ),
        ],
    },
    UiCommand {
        id: "ui.stitchPoints",
        summary: "The Select Point tool on the one selected stitch pattern (#1272), as PE-DESIGN's: needle points are the pattern's stitch and jump entries, by index (stitches.points lists them with their colour block). select sets the selected points; The keys stay inside the colour block of the selected point, as PE-DESIGN's, where each colour of a machine file is a stitch pattern of its own (#1289). tab: true is Tab (or Shift+Tab, the same): with points selected every point of the first selected point's colour block, with all of those selected none, with none selected nothing unless Tab has just dropped them (PE-DESIGN capture k6); end entry / exit selects that colour block's first or last point (Home, End; nothing with none selected), first / last the pattern's entry or exit point, with none selected too (Ctrl+Home, Ctrl+End); step next / previous selects the next or previous point alone (Ctrl+arrow, across jumps, stopping at the colour block's ends), with extend: true it grows the selection by one point from where it started and shrinks it the other way (Shift+arrow). connectTo {id} connects the pattern to another stitch pattern as Alt+drag of the selected entry or exit point onto that pattern's exit or entry point does (stitches.connect with first: the exit's pattern is sewn first), one undo step. Delete (the Delete key, and the menu's Delete) selects the deleted point's neighbour, as PE-DESIGN's. action splitAtPoint or deletePoint runs the needle point menu's Split at Point or Delete; Split at Point on the entry or exit point makes a group of that point alone and the pattern, which sews the same stitches (PE-DESIGN capture k12). Answers with menu (the needle point menu's entries: key, label, enabled), menuOpen, selected, selectedAt (each selected point's index, x and y in mm and screen: where it is in the window), colors (each colour's entry and exit index and point count), connectable (the patterns the one selected entry or exit point can connect to, with the end's x, y and screen), connectOverlap (the end the Alt-dragged point lies on now, where the red square is drawn; null otherwise) and the hint line. Insert a point with stitches.insertPoint (jump: true as Ctrl+Shift+click), move with stitches.movePoints, delete with stitches.deletePoints. Selecting runs the Point tool",
        effect: Effect::Edit,
        params: &[
            p(
                "select",
                "[int]",
                false,
                "Needle point indexes to select (replacing the selection)",
            ),
            p(
                "tab",
                "bool",
                false,
                "true: Tab (Shift+Tab is the same): with points selected, every point of the first selected point's colour block; with all of those selected, none; with none selected nothing, unless Tab has just dropped them",
            ),
            p(
                "end",
                "string",
                false,
                "entry (Home) or exit (End): the colour block's first or last point, nothing with none selected; first (Ctrl+Home) or last (Ctrl+End): the pattern's entry or exit point",
            ),
            p(
                "step",
                "string",
                false,
                "next or previous (Ctrl+arrow): the next or previous point alone, across jumps, stopping at the colour block's ends",
            ),
            p(
                "extend",
                "bool",
                false,
                "With step: grow or shrink the selection by one point from where it started (Shift+arrow)",
            ),
            p(
                "connectTo",
                "int",
                false,
                "Another stitch pattern's id (from connectable): connects as Alt+drag of the one selected entry or exit point does, one undo step",
            ),
            p(
                "action",
                "string",
                false,
                "splitAtPoint or deletePoint: runs that entry of the needle point menu on the selected points, refused when it is grey",
            ),
        ],
    },
    UiCommand {
        id: "ui.notesClear",
        summary: "Forget everything: clear the Assistant's notes (Assistant settings, Memory, Forget everything). Always asks the user first",
        effect: Effect::Settings,
        params: &[],
    },
    // #1180: the Assistant panel for tests and scripts, as the user drives it. The Assistant
    // itself is refused them (the gate's `user_only`): it must not send itself messages or answer
    // its own approval cards.
    UiCommand {
        id: "ui.assistantSend",
        summary: "Send a message in the Assistant panel as if the user typed it and pressed Send (for tests and scripts; the Assistant itself cannot). It starts a turn with the protocol and agent in the settings, or waits in the queue while a turn runs. Attached files are copied into the Assistant's scratch folder; the message and the replies become chat rows, which a save writes into the .osd while assistantSaveChat is on. Needs AI features on. Returns queued and the chat number; poll ui.assistantState until busy is false",
        effect: Effect::Edit,
        params: &[
            p(
                "text",
                "string",
                true,
                "The message, as typed into the composer",
            ),
            p(
                "attach",
                "[string]",
                false,
                "Files to attach, as dropped onto the panel (at most 8): each is copied into the scratch folder, nothing else is stored. A socket client's paths must be under its --listen-root or --listen-read-root folders",
            ),
            p(
                "view",
                "bool",
                false,
                "Send a picture of the view with it, as the View chip does (made for this message, not stored)",
            ),
            p(
                "selection",
                "bool",
                false,
                "Send the selected objects' ids with it, as the Selection chip does",
            ),
        ],
    },
    UiCommand {
        id: "ui.assistantState",
        summary: "The Assistant panel's state (#1180): enabled and open, busy (a turn runs) and queued messages, protocol and agent, the chat number, the rows as a save would keep them (role, text, failed), the approval cards waiting for an answer (index, command, what it asks), the tokens used, and agentSession: whether a coding agent continues its own earlier session for a restored chat (loaded, or failed with why; #1170). Changes nothing",
        effect: Effect::Read,
        params: &[],
    },
    UiCommand {
        id: "ui.assistantAnswer",
        summary: "Answer one of the Assistant's approval cards as the user would click it (#1180; the Assistant itself cannot). Allow once runs the command; Always allow also adds it to the always-allowed commands kept in ui.json (alwaysAllow), except commands that always ask; Deny tells the Assistant the user declined. The client needs the capability the waiting command needs (edit, or writeFiles for files). Returns the cards still waiting",
        effect: Effect::Edit,
        params: &[
            p(
                "index",
                "int",
                true,
                "The card, from 0, as ui.assistantState lists approvals",
            ),
            p(
                "answer",
                "once|always|deny",
                true,
                "Allow once, Always allow, or Deny",
            ),
        ],
    },
];

/// Largest file `ui.scratchWrite` writes.
pub const MAX_SCRATCH_BYTES: usize = 16 << 20;

pub fn ui_command(id: &str) -> Option<&'static UiCommand> {
    UI_COMMANDS.iter().find(|c| c.id == id)
}

/// The `commands.list` entry of an editor command, in the engine's shape.
pub fn describe(c: &UiCommand) -> Value {
    let requires: &[&str] = match c.effect {
        Effect::Read => &[],
        Effect::Edit | Effect::History | Effect::Settings => &["edit"],
        Effect::File => &["writeFiles"],
    };
    json!({
        "id": c.id, "summary": c.summary, "effect": c.effect.as_str(), "requires": requires,
        "params": c.params.iter().map(|p| json!({
            "name": p.name, "type": p.kind, "required": p.required, "summary": p.summary
        })).collect::<Vec<_>>(),
    })
}

/// A setting of the window, read by `ui.settings` and changed by `ui.set`. `label` is the text
/// the window shows for it, so the menu scan in the binary's tests can match them up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Setting {
    pub name: &'static str,
    pub label: &'static str,
    /// `bool`, `mm`, `int a-b`, `number a..b`, `a|b|c` or `[string]`.
    pub kind: &'static str,
}

const fn s(name: &'static str, label: &'static str, kind: &'static str) -> Setting {
    Setting { name, label, kind }
}

pub const SETTINGS: &[Setting] = &[
    s("theme", "Theme", "light|dark|system"),
    s(
        "accent",
        "Accent colour: selection outline, handles, selected buttons (default = the theme's own)",
        "default|blue|orange|green|magenta|purple|teal|#rrggbb",
    ),
    s(
        "shortcuts",
        "Keyboard Shortcuts: the changed ones as { commandKey: [\"Ctrl+Alt+R\", …] } (keys from ui.actions; {} = all default; a clash is refused)",
        "object",
    ),
    s(
        "paneSewingOrderSide",
        "Sewing Order: docked on the left or right",
        "left|right",
    ),
    s(
        "paneAttributesSide",
        "Attributes Setting: docked on the left or right",
        "left|right",
    ),
    s(
        "paneSimulatorSide",
        "Stitch Simulator: docked at the top or bottom",
        "top|bottom",
    ),
    s("qatBelow", "Quick Access Toolbar below the Ribbon", "bool"),
    s("splash", "Show splash at startup", "bool"),
    s(
        "textSize",
        "Options ▸ Text size: the whole window at this percentage (on top of the system display scale)",
        "number 100..200",
    ),
    s(
        "language",
        "Options ▸ Language: the interface language (ribbon and menus so far; pseudo = a test language that brackets every translated text)",
        "en|pseudo",
    ),
    s("showStartCards", "Show start cards", "bool"),
    s(
        "startupWizard",
        "Always show Wizard at Startup: the PE-compatible start-up, PE-DESIGN's Startup Wizard instead of the start cards",
        "bool",
    ),
    s(
        "newObjectDefaults",
        "New Objects: what new shapes start with, { angle 0-359, density 0.5-10, stitchLength 1-10, pullCompensation 0-2, pitch 1-10 (outline), underlay bool }; {} = the tools' own",
        "object",
    ),
    s(
        "advancedAttributes",
        "Sewing Attributes: Advanced (every row) or Basic (the common ones)",
        "bool",
    ),
    s("aiFeatures", "Show AI features", "bool"),
    s(
        "assistantMemory",
        "Let the Assistant remember things",
        "bool",
    ),
    // #1180: the panel and the Assistant's protocol and agent, for tests and scripts; the
    // Assistant itself cannot change them.
    s(
        "assistantPanel",
        "Assistant panel open (kept in ui.json as panelOpen; View ▸ Show/Hide ▸ Assistant)",
        "bool",
    ),
    s(
        "assistantProtocol",
        "The Assistant's protocol, kept in ui.json: anthropic (Claude API), openai (OpenAI-compatible) or agent (a coding agent); a change starts a new conversation with the next message",
        "anthropic|openai|agent",
    ),
    s(
        "assistantAgent",
        "The coding agent for the agent protocol, kept in ui.json (claude, codex, opencode, …: an id the Assistant's settings list); a change starts a new conversation with the next message",
        "string",
    ),
    s(
        "assistantSaveChat",
        "Save the chat with the project: a save writes the conversation into the .osd; off, nothing is written and a chat the file kept is removed on its next save",
        "bool",
    ),
    s("checkUpdates", "Check for updates at startup", "bool"),
    s("includePrereleases", "Include pre-releases", "bool"),
    s("optimizeEntryExit", "Optimize Entry/Exit Points", "bool"),
    s(
        "view",
        "Stitch View / Realistic View / Solid View",
        "stitch|realistic|solid",
    ),
    s("showTravel", "View Thread Trimming", "bool"),
    // The Check Design window (#503-#505, #599): which tab is open, the fabric it judges
    // density by, its marks on the design and the Review Sewing Order step.
    s(
        "checkDesign",
        "Check Design window",
        "closed|problems|density|route",
    ),
    s(
        "checkFabric",
        "Check Design: fabric",
        "standard|thin|heavy|knit|fleece|towel|felt|leather|polyester|pique|organza|canvas|cap",
    ),
    s(
        "densityOverlay",
        "Check Design: show the density map on the design",
        "bool",
    ),
    s(
        "darkBackground",
        "Check Design: dark background (review light threads)",
        "bool",
    ),
    s("routeMap", "Review Sewing Order: route map", "bool"),
    s("reviewStep", "Review Sewing Order: step", "int 1-100000"),
    s(
        "realisticThreadWidth",
        "Realistic View Settings: thread width",
        "number 0.1..1.5",
    ),
    s(
        "realisticContrast",
        "Realistic View Settings: contrast",
        "number -100..100",
    ),
    s(
        "realisticBrightness",
        "Realistic View Settings: brightness",
        "number -100..100",
    ),
    s("grid", "Show Grid", "bool"),
    s("gridAxes", "Show Grid with Axes", "bool"),
    s("gridSpacing", "Grid Spacing", "number 1..100"),
    s(
        "gridMajorEvery",
        "Grid Spacing: every n-th line darker",
        "int 1-50",
    ),
    s("snapToGrid", "Snap to Grid", "bool"),
    s("snapToObjects", "Snap to Objects", "bool"),
    s("snapToGuides", "Snap to guides", "bool"),
    s("precisionBar", "Precision Bar", "bool"),
    s(
        "clickInsideOutlines",
        "Click inside outlines selects them",
        "bool",
    ),
    s("units", "Units: Millimetres / Inches", "mm|inch"),
    s(
        "rulerOrigin",
        "Ruler origin: the Design Page's centre (0, 0 in the middle, negative left and up) or its top-left corner; the rulers, the cursor position and the precision bar's X and Y follow it",
        "centre|topLeft",
    ),
    s(
        "guideColor",
        "Guide colour: the guidelines' colour",
        "#rrggbb",
    ),
    s("nudgeStep", "Nudge step (mm; Shift ×10)", "number 0.1..50"),
    s(
        "nudgeByGrid",
        "Nudge by the grid step while snapping",
        "bool",
    ),
    s("alignTo", "Align relative to", "selection|key|page"),
    s(
        "realSizePointsPerMM",
        "Calibrate Real Size: points per mm at 100 % on this monitor",
        "number 1.8..24 (0 = 96 dpi)",
    ),
    s(
        "smoothTolerance",
        "Smooth Shape tolerance (mm; what ui.run smoothShape uses)",
        "number 0.01..10",
    ),
    s("rulers", "Ruler", "bool"),
    s("guides", "Guideline", "bool"),
    s("referenceWindow", "Reference Window", "bool"),
    s("paneSewingOrder", "Sewing Order", "bool"),
    s("paneSimulator", "Stitch Simulator", "bool"),
    s("paneAttributes", "Attributes Setting", "bool"),
    s("fontFilter", "Font Filter", "bool"),
    s("fontFilterHidden", "Font Filter Settings", "[string]"),
    s(
        "inkStitchFolder",
        "Ink/Stitch Fonts: the folder holding the user's Ink/Stitch font folders (\"\" for none; fonts.scanFolder lists it)",
        "string",
    ),
    s("needlePoints", "View ▸ Show ▸ Needle Points", "bool"),
    s(
        "showLayers",
        "View ▸ Show ▸ Underlay (both, underlay only, top only)",
        "both|underlay|top",
    ),
    s("trimMarks", "View ▸ Show ▸ Trim Marks", "bool"),
    s("hideUnselected", "View ▸ Show ▸ Hide Unselected", "bool"),
    s("colorBy", "View ▸ Show ▸ Colour by", "thread|order|mono"),
    s("soloColor", "View ▸ Show ▸ Solo Colour", "bool"),
    s(
        "simulatorSpeed",
        "Stitch Simulator: speed",
        "number 1..5000",
    ),
    s("autosave", "Autosave for crash recovery", "bool"),
    s(
        "picturePickers",
        "Picture pickers with live preview",
        "bool",
    ),
    s("openInTabs", "Open designs in tabs", "bool"),
    s(
        "autosaveInterval",
        "Autosave: at most this many seconds between snapshots while editing",
        "number 5..600",
    ),
    s(
        "propertySearch",
        "Sewing Attributes: search properties",
        "string",
    ),
    // The Sewing Order pane's Find box (#745, #1031): it narrows the list, never the design.
    s(
        "orderFilter",
        "Sewing Order: find objects (\"\" shows them all)",
        "string",
    ),
];

pub fn setting(name: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.name == name)
}

/// Help menu items, by `ui.help` item and by the label the menu shows.
pub const HELP_ITEMS: &[(&str, &str)] = &[
    ("manual", "Instruction Manual"),
    ("sewKit", "Ready-to-Sew Designs"),
    ("sewKitOnline", "Ready-to-Sew Designs Online"),
    ("gettingStarted", "Getting Started"),
    ("machineSetup", "Machine Setup"),
    ("shortcuts", "Keyboard Shortcuts"),
    ("checkUpdates", "Check for Updates"),
    ("reportProblem", "Report a Problem"),
    ("about", "About OpenStitch"),
];

/// File ▸ Export entries other than the machine formats (which are `design.export` by their
/// extension), with the engine command each runs (#597, #598): the menu guard test checks
/// that every entry is listed here and that every command exists.
pub const EXPORT_MENU: &[(&str, &str)] = &[
    ("PES", "design.export"),
    ("EXP", "design.export"),
    ("EXP…", "design.export"),
    ("EXP with .inf colour file…", "design.export"),
    ("Stitch List (CSV)…", "design.export"),
    ("Stitch List (TXT)…", "design.export"),
    ("G-code…", "design.export"),
    ("Picture (PNG)…", "design.exportPicture"),
    ("Stitch-out Video (animated PNG)…", "design.exportVideo"),
    (
        "Bundle (ZIP: PES, DST, JEF, threads, picture)…",
        "design.exportBundle",
    ),
    ("Garment Mockup", "design.mockup"),
    ("Thread List", "design.exportThreads"),
    ("Apply Thread List to the Design…", "design.applyThreads"),
];

/// Options menu buttons that are commands rather than settings, with the `ui.*` command.
pub const OPTION_BUTTONS: &[(&str, &str)] = &[
    ("Check for Updates Now", "ui.update"),
    ("Clear Recovery Files", "ui.recoveryDiscard"),
    ("Save Layout", "ui.layout"),
    ("Reset Preferences", "ui.resetPrefs"),
];

/// How the assistant reaches a ribbon or menu command.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// `ui.run` runs it as a click would.
    Run,
    /// `ui.run` picks the drawing tool; the assistant draws with the engine command instead.
    Tool(&'static str),
    /// A native file dialog or the clipboard: the assistant runs the engine command with a
    /// path (inside its file roots) instead.
    File(&'static str),
    /// A list of choices: the assistant runs the engine or `ui.*` command behind the choices.
    Choices(&'static str),
}

impl Route {
    pub fn describe(self) -> String {
        match self {
            Route::Run => "ui.run".into(),
            Route::Tool(c) => format!("ui.run picks the tool; draw with {c}"),
            Route::File(c) => format!("{c} with a file path"),
            Route::Choices(c) => format!("{c} (a list of choices)"),
        }
    }

    /// The command the route names besides `ui.run`.
    pub fn command(self) -> Option<&'static str> {
        match self {
            Route::Run => None,
            Route::Tool(c) | Route::File(c) | Route::Choices(c) => Some(c),
        }
    }
}

/// Actions that open a native file dialog or read the clipboard.
pub const FILE_ACTIONS: &[Action] = &[
    Action::Open,
    Action::Save,
    Action::SaveAs,
    Action::ImportPatterns,
    Action::ImportSvg,
    Action::ImportPesObjects,
    Action::OpenImage,
    Action::PasteImage,
    Action::TrueTypeAttributes,
    Action::ExportSections,
];

/// The command behind a list of choices that has no engine command of its own.
fn choices_without_engine(a: Action) -> Option<&'static str> {
    Some(match a {
        Action::Wizard => "digitize.autoPunch",
        Action::Options => "ui.set",
        Action::Help => "ui.help",
        Action::OtherZoom => "ui.zoom",
        Action::GridSpacing => "ui.set",
        _ => return None,
    })
}

/// How the assistant reaches `a`. Every action has a route; the binary's tests check that each
/// named command exists and that every `Run` action runs.
pub fn route(a: Action) -> Route {
    let info = a.info();
    if a.tool().is_some() {
        return Route::Tool(info.engine.unwrap_or("object.list"));
    }
    if FILE_ACTIONS.contains(&a) {
        return Route::File(info.engine.unwrap_or("project.open"));
    }
    if a.is_menu() {
        return Route::Choices(
            info.engine
                .or_else(|| choices_without_engine(a))
                .unwrap_or("ui.actions"),
        );
    }
    Route::Run
}

/// `ui.actions` entry.
pub fn action_entry(a: Action, enabled: bool) -> Value {
    let info = a.info();
    json!({
        "key": info.key, "label": info.label, "tip": info.tip, "shortcut": info.shortcut,
        "available": info.available, "enabled": enabled, "engine": info.engine,
        "toggle": a.is_toggle(), "route": route(a).describe(),
    })
}

/// File names `ui.scratchWrite` accepts: a plain name with a picture or SVG extension.
pub fn scratch_name_ok(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | ' '))
        && [".svg", ".png", ".jpg", ".jpeg", ".bmp"]
            .iter()
            .any(|e| lower.ends_with(e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// #1205, #1198: ui.askUser's description says the first choice is the recommended one, what
    /// a skipped question takes, and that "I'll type it" choices are dropped.
    #[test]
    fn ask_user_describes_the_recommended_answer() {
        let c = ui_command("ui.askUser").expect("ui.askUser");
        assert!(
            c.summary.contains("first marked Recommended"),
            "{}",
            c.summary
        );
        assert!(
            c.summary.contains("takes the recommended (first) answer"),
            "{}",
            c.summary
        );
        let questions = c
            .params
            .iter()
            .find(|p| p.name == "questions")
            .expect("questions");
        assert!(
            questions.summary.contains("the recommended one first"),
            "{}",
            questions.summary
        );
        assert!(
            questions.summary.contains("I'll type it"),
            "{}",
            questions.summary
        );
        let entry = describe(c);
        assert!(
            entry["params"]
                .to_string()
                .contains("the recommended one first")
        );
    }

    #[test]
    fn every_action_has_a_route_and_ids_are_unique() {
        for &a in Action::ALL {
            let r = route(a);
            if let Some(c) = r.command() {
                assert!(!c.is_empty(), "{a:?}");
            }
        }
        let mut ids: Vec<_> = UI_COMMANDS.iter().map(|c| c.id).collect();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), UI_COMMANDS.len());
        let mut names: Vec<_> = SETTINGS.iter().map(|s| s.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), SETTINGS.len());
    }

    #[test]
    fn scratch_names_are_plain_picture_or_svg_files() {
        for ok in ["logo.svg", "Photo 1.PNG", "a_b-c.jpeg", "x.bmp"] {
            assert!(scratch_name_ok(ok), "{ok}");
        }
        for bad in [
            "",
            "../x.svg",
            "a/b.svg",
            "a\\b.png",
            ".hidden.svg",
            "x.exe",
            "x.svg.sh",
            "c:x.png",
        ] {
            assert!(!scratch_name_ok(bad), "{bad}");
        }
    }
}
