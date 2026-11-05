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
        summary: "The editor's state: file, unsaved changes, selection, tool, view, zoom, panes, simulator and every setting",
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
        summary: "A PNG of the design as the editor holds it (Design Page and stitches in thread colours), base64, with the stitch statistics",
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
        summary: "Help menu: with no item, lists the items; with one, opens it (manual, ready-to-sew designs, keyboard shortcuts, About)",
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
    s("qatBelow", "Quick Access Toolbar below the Ribbon", "bool"),
    s("splash", "Show splash at startup", "bool"),
    s("aiFeatures", "Show AI features", "bool"),
    s("checkUpdates", "Check for updates at startup", "bool"),
    s("includePrereleases", "Include pre-releases", "bool"),
    s("optimizeEntryExit", "Optimize Entry/Exit Points", "bool"),
    s(
        "view",
        "Stitch View / Realistic View / Solid View",
        "stitch|realistic|solid",
    ),
    s("showTravel", "View Thread Trimming", "bool"),
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
    s("precisionBar", "Precision Bar", "bool"),
    s(
        "clickInsideOutlines",
        "Click inside outlines selects them",
        "bool",
    ),
    s("units", "Units: Millimetres / Inches", "mm|inch"),
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
];

pub fn setting(name: &str) -> Option<&'static Setting> {
    SETTINGS.iter().find(|s| s.name == name)
}

/// Help menu items, by `ui.help` item and by the label the menu shows.
pub const HELP_ITEMS: &[(&str, &str)] = &[
    ("manual", "Instruction Manual"),
    ("sewKit", "Ready-to-Sew Designs"),
    ("sewKitOnline", "Ready-to-Sew Designs Online"),
    ("shortcuts", "Keyboard Shortcuts"),
    ("checkUpdates", "Check for Updates"),
    ("about", "About OpenStitch"),
];

/// Options menu buttons that are commands rather than settings, with the `ui.*` command.
pub const OPTION_BUTTONS: &[(&str, &str)] = &[("Check for Updates Now", "ui.update")];

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
