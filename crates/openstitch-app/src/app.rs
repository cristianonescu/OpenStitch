//! Application state, command dispatch, file actions, menus and shortcuts. Every change to the
//! design goes through [`Session::execute`]; this module only keeps view state (selection,
//! viewport, simulator position) and the compiled pattern the canvas draws.

use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use openstitch_app::editor::close_prompt::{Choice, ClosePrompt, Outcome, Pending};
use openstitch_app::editor::render::{PatternStats, RenderModel};
use openstitch_app::editor::{
    CommandRequest, Tool, ToolContext, ToolController, ToolEffect, Viewport,
};
use openstitch_core::StitchPattern;
use openstitch_engine::{Effect, Session};
use openstitch_formats::FileType;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Modifier name shown in hints.
pub const CMD: &str = if cfg!(target_os = "macos") {
    "Cmd+"
} else {
    "Ctrl+"
};

/// Options for scripted runs (`--screenshot`).
#[derive(Default, Clone, Debug)]
pub struct Launch {
    pub open: Option<PathBuf>,
    pub demo: bool,
    pub screenshot: Option<PathBuf>,
    pub select: Vec<u64>,
    /// Simulator position as a fraction of all stitches.
    pub simulate: Option<f64>,
    pub tool: Option<Tool>,
    /// Scripted input (see [`crate::script`]).
    pub script: Vec<crate::script::Step>,
    /// Zoom factor applied after fitting the page.
    pub zoom: Option<f64>,
    /// Design point (mm) the view is centred on after fitting and zooming (`--center`): a
    /// screenshot of part of a design at a high zoom, tiled into a sharper preview (#893).
    pub center: Option<(f64, f64)>,
    /// Workspace views to open (see [`SHOW`]).
    pub show: Vec<String>,
    /// Image to Embroidery: image to preselect (opens the dialog) and method to run.
    pub image: Option<PathBuf>,
    pub convert: Option<openstitch_app::editor::wizard::Method>,
    /// Pattern file to open in the Programmable Stitch Creator (opens the window).
    pub pattern: Option<PathBuf>,
    /// Pattern wizard dialog to open for the selection, and whether to apply it once.
    pub wizard: Option<openstitch_app::editor::pattern_wizards::WizardKind>,
    pub apply_wizard: bool,
    /// User settings file (attribute sets, thread charts); `None` keeps them in memory.
    pub user_settings: Option<PathBuf>,
    /// Design Database: folder (or file, which is selected) to open the mode at.
    pub database: Option<PathBuf>,
    pub database_view: Option<openstitch_app::editor::database::View>,
    /// Design Database dialog to open: search, convert, catalog, delete, newfolder.
    pub database_dialog: Option<String>,
    /// Automation socket (`--listen`); started by `main` before the window opens.
    pub listen: Option<crate::remote::ServerConfig>,
    /// Ribbon tab and right-hand pane tab to show (`--ribbon`, `--pane`).
    pub ribbon: Option<openstitch_app::editor::ribbon::TabId>,
    pub pane: Option<crate::ribbon::PaneTab>,
    /// Colour theme (`--theme`); otherwise the saved preference.
    pub theme: Option<openstitch_app::editor::prefs::Theme>,
    /// Accent colour (`--accent`, #559); otherwise the saved preference.
    pub accent: Option<[u8; 3]>,
    /// Interface language (`--language`, #555); otherwise the saved preference.
    pub language: Option<String>,
    /// `--no-splash`: no startup splash (also set by `main` for `--listen`).
    pub no_splash: bool,
}

impl Launch {
    /// Whether the startup splash may show: a plain interactive start, at most with a file to
    /// open. Screenshots, scripts, the automation socket and every option that sets up a
    /// particular view start straight away.
    pub fn splash_allowed(&self) -> bool {
        !self.no_splash && self.interactive()
    }

    /// A plain interactive start (at most a file to open), where the update check runs
    /// (`--no-splash` only hides the splash).
    pub fn interactive(&self) -> bool {
        self.screenshot.is_none()
            && self.script.is_empty()
            && self.listen.is_none()
            && !self.demo
            && self.select.is_empty()
            && self.simulate.is_none()
            && self.tool.is_none()
            && self.zoom.is_none()
            && self.show.is_empty()
            && self.image.is_none()
            && self.pattern.is_none()
            && self.wizard.is_none()
            && self.database.is_none()
            && self.ribbon.is_none()
            && self.pane.is_none()
    }
}

/// Values `--show` accepts.
pub const SHOW: [&str; 38] = [
    "update",
    "stitch-table",
    "large-text",
    "keyboard",
    "pickers",
    "tabs",
    "close-prompt",
    "realistic",
    "solid",
    "fabric",
    "reference",
    "settings",
    "measure",
    "norulers",
    "text",
    "creator",
    "sdf",
    "attributes",
    "charts",
    "matchThreads",
    "mergeColours",
    "usb",
    "fonts",
    "monogram",
    "namedrop",
    "templates",
    "arrange",
    "appmenu",
    "property",
    "shortcuts",
    "floating",
    "print",
    "assistant",
    "assistant-notices",
    "basic",
    "getting-started",
    "machine-setup",
    "startup-wizard",
];

pub struct EditorApp {
    pub session: Session,
    pub selection: BTreeSet<u64>,
    /// The selection in the order it was clicked (#502: Sew in Click Order).
    pub click_order: Vec<u64>,
    pub tools: ToolController,
    /// `None` until the canvas size is known; then fitted to the Design Page.
    pub viewport: Option<Viewport>,
    pub pattern: StitchPattern,
    pub stats: PatternStats,
    pub render: RenderModel,
    render_key: Option<(u64, usize, openstitch_design::view::ViewFilter, [u8; 3])>,
    /// The Design Page colour of the current theme (faded colours blend into it).
    pub page_rgb: [u8; 3],
    /// Underlay flags of the compiled pattern per revision (#517), computed when asked for.
    underlay: Option<(u64, Vec<bool>)>,
    /// Bumped after every change to the design.
    pub revision: u64,
    saved_revision: u64,
    /// Simulator: stitches shown (`None` = the whole design).
    pub simulate: Option<usize>,
    pub playing: bool,
    pub play_speed: f64,
    play_accumulator: f64,
    /// View ▸ Zoom ▸ Zoom Box (#518).
    pub zoom_box: crate::zoom_ui::ZoomBox,
    /// Options ▸ Calibrate Real Size (#518).
    pub calibration: crate::zoom_ui::Calibration,
    /// A background-image handle drag in progress (#518).
    pub image_drag: Option<crate::zoom_ui::ImageDrag>,
    /// The monitor the window is on (real-size calibrations are kept per monitor).
    pub monitor: Option<String>,
    /// The design drawn in two halves around a background image above an object (#518):
    /// (revision, simulator limit, split stitch) and the halves.
    pub background_split: Option<((u64, usize, usize), RenderModel, RenderModel)>,
    /// The compiled design as the machine sews it (#516): colour blocks, trims, objects, time.
    pub timeline: openstitch_design::timeline::Timeline,
    /// Stitch-out player toggles (#516), all off by default.
    pub stitch_player: PlayerToggles,
    pub show_travel: bool,
    pub path: Option<PathBuf>,
    pub status: String,
    effects: HashMap<&'static str, Effect>,
    pub scale_takes_origin: bool,
    /// Inspector fields being dragged or typed, committed as one command when released.
    pub drafts: HashMap<&'static str, f64>,
    /// Scale percentage typed in the inspector.
    pub scale_percent: f64,
    title: String,
    screenshot: Option<Screenshot>,
    pub canvas_pointer_captured: bool,
    pub clicks: openstitch_app::editor::ClickCounter,
    pub fit_requested: bool,
    /// Zoom applied once after the first fit (`--zoom`).
    pub initial_zoom: Option<f64>,
    /// Model point the automatic fit centres on (`--center`), until a fit is asked for.
    pub initial_center: Option<openstitch_design::Vec2>,
    /// The automatic fit last made: the canvas it was made for, the view it gave and the
    /// `--zoom` applied. While the view is still that one, a change of the canvas (the panes are
    /// laid out a frame after the canvas; the window resized) fits again (#450: kit previews and
    /// screenshots lost the page's bottom to the Stitch Simulator pane).
    pub auto_fit: Option<(openstitch_app::editor::ScreenRect, Viewport, Option<f64>)>,
    /// A wizard dialog's rectangle in the previous frame, and the one being drawn this frame
    /// (`dialog_window`). In a `--screenshot` run the automatic fit keeps the page clear of it
    /// (#565: the dialogs hid part of the design in the documentation images).
    pub dialog_rect: Option<egui::Rect>,
    pub dialog_rect_next: Option<egui::Rect>,
    player: Option<crate::script::Player>,
    /// The Stitch Table window (#501).
    pub stitch_table: crate::stitch_table_ui::StitchTable,
    /// The keyboard drawing cursor (#499).
    pub keyboard: crate::keyboard_ui::KeyboardCursor,
    /// Picture pickers and their hover preview (#656).
    pub pickers: crate::pickers_ui::Pickers,
    /// The other designs open as tabs (#552).
    pub tabs: documents::Tabs,
    /// A File ▸ Open or Import running on a worker thread (#558).
    pub open_job: Option<crate::open_job::OpenJob>,
    /// The object selected last (Align ▸ relative to Last Selected keeps it in place).
    pub key_object: Option<u64>,
    /// Align ▸ relative to: `"selection"`, `"key"` or `"page"` (#472).
    pub align_to: &'static str,
    /// View ▸ Precision Bar (#474).
    pub precision: crate::arrange_ui::PrecisionState,
    frames: u64,
    /// Engine history steps per user-level edit, so one gesture (e.g. scale + move, or
    /// new design + import + centre) is one Undo.
    pub(crate) undo_groups: Vec<usize>,
    pub(crate) redo_groups: Vec<usize>,
    /// Inside [`EditorApp::with_one_refresh`]: whether an edit is waiting for its stitch compile.
    refresh_deferred: Option<bool>,
    /// Rulers, grid, measure tool, Reference window, Design Settings, estimates.
    pub workspace: crate::workspace_ui::Workspace,
    /// Groups, arrange copies, shape operations, attributes, entry/exit points.
    pub edit: crate::edit_ui::EditState,
    /// Image to Embroidery dialog.
    pub wizard: openstitch_app::editor::wizard::ImageWizard,
    /// The Image to Embroidery dialog's marking preview: the image it shows, its size in
    /// pixels, its texture (#350).
    pub wizard_preview: Option<(std::path::PathBuf, (u32, u32), egui::TextureHandle)>,
    /// Lettering menu dialogs.
    pub lettering: crate::lettering_ui::LetteringState,
    /// Programmable Stitch Creator window.
    pub creator: crate::creator_ui::CreatorState,
    /// Stitch Design Factory window.
    pub sewing: crate::sewing_ui::SewingState,
    /// Background fill, appliqué, patch and cutwork dialogs.
    pub pattern_wizards: openstitch_app::editor::pattern_wizards::PatternWizards,
    /// Attribute sets and user thread charts windows.
    pub settings: crate::settings_ui::SettingsState,
    /// Match to Thread Chart and Merge Similar Colours (#484, #485).
    pub colour_match: crate::colour_match_ui::MatchState,
    /// The background image as shown, keyed by its path and adjustments (`None` when it
    /// cannot be read).
    pub background_texture: Option<(
        String,
        openstitch_design::ImageAdjust,
        Option<eframe::egui::TextureHandle>,
    )>,
    /// Design Database mode (File ▸ Design Database…).
    pub database: openstitch_app::editor::database::Database,
    pub db_textures: crate::database_ui::Textures,
    pub db_chrome: crate::database_ui::Chrome,
    /// A PDF being handed to the system's print dialog or viewer.
    pub print_job: Option<openstitch_app::os_print::Job>,
    /// The last PDF handed to [`EditorApp::print_pdf`]; in test builds nothing is printed and
    /// tests read this instead.
    pub printed: Option<PathBuf>,
    /// Loopback automation socket; its requests run in [`EditorApp::serve_remote`].
    pub remote: Option<crate::remote::Server>,
    /// The AI assistant's scratch folder and pending Help link (`assistant_ui.rs`).
    pub assistant: crate::assistant_ui::AssistantState,
    /// Autosave snapshots and the Recover Designs window (#549).
    pub recovery: crate::recovery_ui::RecoveryState,
    /// The command palette (#550).
    pub palette: crate::palette_ui::PaletteState,
    /// Startup splash while it shows (see [`crate::splash`]).
    pub splash: Option<crate::splash::Splash>,
    /// Ribbon, Quick Access Toolbar, panes and window preferences.
    pub ui: crate::ribbon::RibbonState,
    /// Check Design, the density map and the sewing-order review (#503-#505).
    pub quality: crate::quality_ui::QualityState,
    /// Self-update: startup check, dialog, download and restart.
    pub update: crate::update_ui::UpdateState,
    /// "Save changes to NAME?" when closing, File ▸ New or Open would lose unsaved changes.
    pub close_prompt: ClosePrompt,
    /// Help ▸ Machine Setup, also on the first interactive start (#508).
    pub machine_setup: crate::machine_setup_ui::MachineSetup,
    /// Application menu ▸ Wizard ▸ Startup Wizard, PE-DESIGN's start-up dialog (#657).
    pub startup_wizard: crate::startup_wizard_ui::StartupWizard,
    /// The next window close goes through: Don't Save, a Save that succeeded, a finished
    /// screenshot run or the updater's restart. Without it a close while the design has
    /// unsaved changes is cancelled and the prompt opens.
    exit_approved: bool,
}

struct Screenshot {
    path: PathBuf,
    frames: u32,
    requested: bool,
    done: Arc<AtomicBool>,
}

impl EditorApp {
    pub fn new(launch: Launch) -> Self {
        let base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
        let session = Session::new(base);
        let effects = session
            .commands()
            .iter()
            .map(|c| (c.id, c.effect))
            .collect();
        let scale_takes_origin = session
            .commands()
            .iter()
            .any(|c| c.id == "object.scale" && c.params.iter().any(|p| p.name == "originX"));
        let mut app = EditorApp {
            session,
            selection: BTreeSet::new(),
            click_order: Vec::new(),
            tools: ToolController::default(),
            viewport: None,
            pattern: StitchPattern::new(),
            stats: PatternStats::default(),
            render: RenderModel::default(),
            render_key: None,
            underlay: None,
            page_rgb: [255; 3],
            revision: 0,
            saved_revision: 0,
            simulate: None,
            playing: false,
            play_speed: 400.0,
            zoom_box: Default::default(),
            calibration: Default::default(),
            image_drag: None,
            monitor: None,
            background_split: None,
            timeline: Default::default(),
            stitch_player: PlayerToggles::default(),
            play_accumulator: 0.0,
            show_travel: true,
            path: None,
            status: "Ready".into(),
            effects,
            scale_takes_origin,
            drafts: HashMap::new(),
            scale_percent: 100.0,
            title: String::new(),
            screenshot: None,
            canvas_pointer_captured: false,
            clicks: Default::default(),
            fit_requested: false,
            initial_zoom: None,
            initial_center: None,
            auto_fit: None,
            dialog_rect: None,
            dialog_rect_next: None,
            player: None,
            stitch_table: Default::default(),
            keyboard: Default::default(),
            pickers: Default::default(),
            tabs: Default::default(),
            open_job: None,
            key_object: None,
            align_to: "selection",
            precision: Default::default(),
            frames: 0,
            undo_groups: Vec::new(),
            redo_groups: Vec::new(),
            refresh_deferred: None,
            workspace: Default::default(),
            edit: Default::default(),
            wizard: Default::default(),
            wizard_preview: None,
            lettering: Default::default(),
            creator: Default::default(),
            sewing: Default::default(),
            pattern_wizards: Default::default(),
            settings: Default::default(),
            colour_match: Default::default(),
            background_texture: None,
            database: Default::default(),
            db_textures: Default::default(),
            db_chrome: Default::default(),
            print_job: None,
            printed: None,
            remote: None,
            assistant: Default::default(),
            recovery: Default::default(),
            palette: Default::default(),
            splash: None,
            ui: Default::default(),
            quality: Default::default(),
            update: Default::default(),
            close_prompt: ClosePrompt::default(),
            machine_setup: Default::default(),
            startup_wizard: Default::default(),
            exit_approved: false,
        };
        app.refresh();
        app.saved_revision = app.revision;
        app.launch(launch);
        app
    }

    fn launch(&mut self, l: Launch) {
        let splash = l.splash_allowed();
        if let Some(path) = &l.user_settings
            && let Err(e) = self.session.with_user_settings(path)
        {
            self.status = openstitch_app::tf!("ui-status-could-not-read-the-user-settings", e = e);
        }
        // Screenshots use the default window layout, not the user's.
        if let Some(path) = &l.user_settings
            && l.screenshot.is_none()
        {
            let ui = openstitch_app::editor::prefs::path_beside(path);
            self.ui.prefs = openstitch_app::editor::prefs::UiPrefs::load(&ui);
            // The guidelines shown as the user left them (#799).
            self.workspace.guides = self.ui.prefs.editing.show_guides;
            // Autosave snapshots live beside ui.json (#549); screenshot runs have none.
            if let Some(dir) = ui.parent() {
                self.recovery.start(
                    dir.join(openstitch_app::editor::recovery::FOLDER),
                    l.script.is_empty(),
                );
            }
            self.ui.prefs_path = Some(ui);
        } else if l.screenshot.is_none()
            && let Some(store) = openstitch_app::platform::user_store()
        {
            // No user.json: the browser page's storage (#829), then its preferences below.
            self.session.with_user_store(store);
        }
        if l.user_settings.is_none()
            && l.screenshot.is_none()
            && let Some(v) = openstitch_app::platform::stored_prefs()
                .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
        {
            // No ui.json: the browser page's storage, when it registered one (#797).
            self.ui.prefs = openstitch_app::editor::prefs::UiPrefs::from_json(&v);
            self.workspace.guides = self.ui.prefs.editing.show_guides;
        }
        self.database.units = self.ui.prefs.units;
        // Copy and Paste use the system clipboard (#500), except in screenshot runs and tests
        // (#705: see `ribbon::os_clipboard`).
        self.ui.system_clipboard = l.screenshot.is_none() && !cfg!(test);
        if l.accent.is_some() {
            self.ui.prefs.accent = l.accent;
        }
        if let Some(t) = l.theme {
            self.ui.prefs.theme = t;
        }
        if let Some(code) = &l.language {
            self.ui.prefs.language = code.clone();
        }
        // The Assistant opens as the user left it, open on first start (#422); screenshots
        // keep the default layout without it.
        self.assistant.panel.open =
            l.screenshot.is_none() && self.ui.prefs.ai.enabled && self.ui.prefs.ai.panel_open;
        if splash && self.ui.prefs.splash {
            self.splash = Some(crate::splash::Splash::default());
        }
        // The first interactive start with the user's settings asks which machine (#508).
        if crate::machine_setup_ui::asks_at_start(&l, &self.ui.prefs) {
            self.machine_setup.open();
        }
        // The PE-compatible start-up (#657): the Startup Wizard instead of the start cards.
        if crate::startup_wizard_ui::opens_at_start(&l, &self.ui.prefs) {
            self.startup_wizard.open = true;
        }
        if l.interactive()
            && self.ui.prefs.updates.check
            && self.update.current.is_some()
            && self.update.install.checks()
        {
            self.update.check_at_start = true;
        }
        if l.demo {
            self.with_one_refresh(|app| {
                for r in demo_design() {
                    app.execute(&r);
                }
            });
        }
        if let Some(p) = l.open {
            self.open_path(&p);
        }
        if !l.select.is_empty() {
            self.selection = l
                .select
                .into_iter()
                .filter(|id| self.session.design().index_of(*id).is_some())
                .collect();
        }
        if let Some(f) = l.simulate {
            self.simulate =
                Some(((self.stats.stitches as f64) * f.clamp(0.0, 1.0)).round() as usize);
        }
        if let Some(t) = l.tool {
            self.tools.set_tool(t);
        }
        self.initial_zoom = l.zoom;
        self.initial_center = l
            .center
            .map(|(x, y)| openstitch_design::v2(x * 10.0, y * 10.0));
        for v in &l.show {
            match v.as_str() {
                "realistic" => self.workspace.realistic = true,
                "solid" => self.workspace.solid = true,
                "fabric" => self.workspace.view.fabric_open = true,
                "machine-setup" => self.machine_setup.open(),
                "startup-wizard" => self.startup_wizard.open = true,
                "reference" => self.workspace.reference = true,
                "settings" => self.workspace.settings_open = true,
                "attributes" => self.settings.sets_open = true,
                "charts" => self.settings.charts_open = true,
                "matchThreads" => self.colour_match.match_open = true,
                "mergeColours" => self.colour_match.merge_open = true,
                "text" => self.lettering.text.open = true,
                "fonts" => self.lettering.fonts.open = true,
                "monogram" => self.lettering.monogram.open = true,
                "namedrop" => self.lettering.name_drop.open = true,
                "templates" => crate::lettering_ui::open_template_wizard(self),
                "measure" => self.toggle_measure(),
                "norulers" => self.workspace.rulers = false,
                "creator" => self.creator.open = true,
                "sdf" => {
                    self.sewing.open = true;
                    if let Some(q) = self.session.design().sewing.first() {
                        self.sewing.editor =
                            openstitch_app::editor::sewing::SewingEditor::from_stitch(q);
                    }
                }
                "usb" => crate::transfer_ui::open(self),
                "arrange" => self.edit.arrange_open = true,
                // The Assistant panel with one attachment chip (a stand-in text file), when the AI
                // features are on in ui.json; for checking the panel's icons in screenshots.
                "assistant" => {
                    self.assistant.panel.open = true;
                    // Not in tests: the accessibility audit opens every scene, and tests leave
                    // nothing in the temp folder (#499).
                    let note = std::env::temp_dir().join("openstitch-notes.txt");
                    if !cfg!(test) && std::fs::write(&note, "Stitch the logo in blue.").is_ok() {
                        crate::assistant_panel::attach_files(self, vec![note]);
                    }
                }
                "assistant-notices" => crate::assistant_panel::show_sample_notices(self),
                // Sewing Attributes in its Basic view (#553), not saved.
                "basic" => self.ui.prefs.advanced_attributes = false,
                "getting-started" => self.ui.start.page_open = true,
                "appmenu" => {
                    // Whichever window shows: the editor's or a suite window's.
                    self.ui.open_app_menu = true;
                    self.creator.open_app_menu = true;
                    self.sewing.open_app_menu = true;
                }
                "property" => self.ui.property_open = true,
                "print" => self.ui.open_print = true,
                "shortcuts" => self.ui.shortcuts_open = true,
                "update" => self.update.show_sample(),
                "stitch-table" => {
                    // The Stitch Table of the selected pattern, a few rows selected (#501).
                    self.stitch_table.open = true;
                    let _ = self.stitch_table_command(Some(&json!({ "select": [40, 41, 42] })));
                }
                "large-text" => self.ui.prefs.text_size = 150,
                "close-prompt" => {
                    self.close_prompt.request(Pending::Exit, true);
                }
                "keyboard" => {
                    // A Satin Skeleton column drawn from the keyboard (#499, #707): three
                    // points of growing width, the cursor at the next one.
                    self.tools.set_tool(Tool::Skeleton);
                    for (x, y, w) in [(30.0, 70.0, 2.0), (50.0, 45.0, 4.0), (75.0, 60.0, 7.0)] {
                        let _ = self.cursor_command(Some(&json!({
                            "x": x, "y": y, "width": w, "action": "smooth",
                        })));
                    }
                    let _ =
                        self.cursor_command(Some(&json!({ "x": 85.0, "y": 35.0, "width": 9.0 })));
                }
                "pickers" => {
                    // The Region Type pictures open over a satin preview of the badge (#656).
                    self.ui.prefs.picture_pickers = true;
                    self.pickers.force_open = Some("region-type");
                    let _ = self.preview_command(Some(&json!({
                        "command": "object.setRegion",
                        "params": { "ids": [1], "type": "satin" },
                    })));
                }
                "tabs" => {
                    // Two designs as tabs (#552): the sample and an unsaved name tag.
                    self.ui.prefs.open_in_tabs = true;
                    self.new_tab();
                    let steps = [
                        CommandRequest::new("design.new", json!({ "name": "Name tag" })),
                        CommandRequest::new(
                            "shape.ellipse",
                            json!({ "cx": 50, "cy": 50, "rx": 35, "ry": 18 }),
                        ),
                    ];
                    self.execute_all(&steps);
                    self.activate_tab(0);
                }
                "floating" => {
                    let p = &mut self.ui.prefs.panes;
                    p.sewing_order.floating = true;
                    p.attributes.floating = true;
                }
                _ => {}
            }
        }
        if let Some(p) = &l.pattern {
            self.creator.open = true;
            match std::fs::read_to_string(p)
                .map_err(|e| e.to_string())
                .and_then(|t| openstitch_design::patterns::parse(&t))
            {
                Ok(pattern) => {
                    self.creator.doc =
                        openstitch_app::editor::creator::Creator::from_pattern(&pattern)
                }
                Err(e) => self.status = openstitch_app::tf!("ui-status-status", path = p.display(), e = e),
            }
        }
        if let Some(p) = l.image {
            self.wizard.path = Some(p);
            self.wizard.open = true;
            if let Some(m) = l.convert {
                self.wizard.set_method(m);
                crate::wizard_ui::run(self);
            }
        }
        if let Some(kind) = l.wizard {
            if l.apply_wizard {
                // Keep the patterns selected for the dialog, unless the wizard replaced them.
                let mut picked = self.selection.clone();
                crate::pattern_wizard_ui::run(self, kind);
                picked.retain(|id| self.session.design().index_of(*id).is_some());
                if !picked.is_empty() {
                    self.selection = picked;
                }
            }
            self.pattern_wizards.open = Some(kind);
        }
        if let Some(p) = &l.database {
            self.database.show(Some(p));
        }
        if let Some(v) = l.database_view {
            self.database.view = v;
        }
        if let Some(d) = &l.database_dialog {
            use openstitch_app::editor::database::Dialog;
            match d.as_str() {
                "search" => self.database.dialog = Dialog::Search,
                "convert" => self.database.dialog = Dialog::Convert,
                "catalog" => self.database.dialog = Dialog::Catalog,
                "delete" => self.database.request_delete(),
                "newfolder" => self.database.dialog = Dialog::NewFolder("New Folder".into()),
                _ => {}
            }
        }
        if let Some(t) = l.ribbon {
            self.ui.tab = t;
        }
        if let Some(p) = l.pane {
            self.ui.pane_tab = p;
        }
        if !l.script.is_empty() {
            self.player = Some(crate::script::Player::new(l.script));
        }
        if l.demo {
            self.saved_revision = self.revision;
        }
        self.screenshot = l.screenshot.map(|path| Screenshot {
            path,
            frames: 0,
            requested: false,
            done: Arc::new(AtomicBool::new(false)),
        });
    }

    // MARK: commands

    /// Runs one command; reports failures in the status bar. Returns the command's result.
    pub fn execute(&mut self, r: &CommandRequest) -> Option<Value> {
        let effect = self.effects.get(r.command).copied();
        let before = (effect == Some(Effect::Edit)).then(|| self.session.design().clone());
        match self.session.execute(r.command, Some(&r.params)) {
            Ok(v) => {
                if let Some(before) = before {
                    // An edit that changed nothing records no engine history step.
                    if *self.session.design() != before {
                        self.undo_groups.push(1);
                        self.redo_groups.clear();
                    }
                }
                // Hand edits on live objects (#501) last only while their settings do.
                let dropped = self.session.take_hand_edits_dropped();
                if !dropped.is_empty() {
                    self.status = openstitch_app::tf!("ui-status-hand-edited-stitches-discarded-on-object", count = dropped.len());
                }
                if matches!(effect, Some(Effect::Edit | Effect::History)) {
                    match &mut self.refresh_deferred {
                        Some(pending) => *pending = true,
                        None => self.refresh(),
                    }
                }
                Some(v)
            }
            Err(e) => {
                self.status = e.to_string();
                None
            }
        }
    }

    /// Runs requests in order, stopping at the first failure; they undo as one step.
    pub fn execute_all(&mut self, requests: &[CommandRequest]) -> Option<Value> {
        self.grouped(|app| {
            let mut last = None;
            for r in requests {
                last = Some(app.execute(r)?);
            }
            last
        })
    }

    /// Runs `f` with one stitch compile at the end instead of one per edit; for command
    /// batches that never look at the stitches between commands (the `--demo` sample).
    fn with_one_refresh(&mut self, f: impl FnOnce(&mut Self)) {
        self.refresh_deferred = Some(false);
        f(self);
        if self.refresh_deferred.take() == Some(true) {
            self.refresh();
        }
    }

    /// Runs `f` so that every edit it makes is undone by a single Undo.
    pub fn grouped<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let start = self.undo_groups.len();
        let out = f(self);
        let steps: usize = self.undo_groups.drain(start..).sum();
        if steps > 0 {
            self.undo_groups.push(steps);
        }
        out
    }

    /// What the tools see of the document; `design` is a snapshot of the session's design.
    pub fn tool_context<'a>(&'a self, design: &'a openstitch_design::Design) -> ToolContext<'a> {
        ToolContext {
            design,
            selection: &self.selection,
            units_per_point: self.viewport.unwrap_or_default().units_per_point(),
            scale_takes_origin: self.scale_takes_origin,
        }
    }

    /// Runs `f` on the tool controller with the current context and applies its effects.
    pub fn with_tools(
        &mut self,
        f: impl FnOnce(&mut ToolController, &ToolContext) -> Vec<ToolEffect>,
    ) {
        let design = self.session.design().clone();
        let ctx = ToolContext {
            design: &design,
            selection: &self.selection,
            units_per_point: self.viewport.unwrap_or_default().units_per_point(),
            scale_takes_origin: self.scale_takes_origin,
        };
        self.tools.set_snap(self.workspace.grid.snap_step());
        let editing = self.ui.prefs.editing;
        self.tools
            .set_object_snapping(editing.snap_objects, editing.click_inside);
        // Snap to guides while they show (#799).
        self.tools.set_guide_snapping(
            editing.snap_guides && self.workspace.guides && self.workspace.rulers,
        );
        let effects = f(&mut self.tools, &ctx);
        self.apply_effects(effects);
    }

    pub fn apply_effects(&mut self, effects: Vec<ToolEffect>) {
        for e in effects {
            match crate::edit_ui::filter_effect(self, e) {
                ToolEffect::Select(sel) => {
                    // The object added last is the key object Align ▸ Last Selected keeps.
                    let added: Vec<u64> = sel.difference(&self.selection).copied().collect();
                    if let [one] = added[..] {
                        self.key_object = Some(one);
                    }
                    self.selection = sel;
                }
                ToolEffect::Execute {
                    requests,
                    select_result,
                } => {
                    // New shapes take the user's defaults (#553) in the same undo step.
                    let defaults = self.ui.prefs.new_objects.clone();
                    let drawn = requests.first().is_some_and(|r| {
                        openstitch_app::editor::new_objects::DRAWN.contains(&r.command)
                    });
                    let result = self.grouped(|app| {
                        let result = app.execute_all(&requests)?;
                        if drawn && !defaults.is_empty() {
                            let ids = CommandRequest::created_ids(&result);
                            app.apply_new_object_defaults(&ids, &defaults);
                        }
                        // A Pen line drawn with widths is a satin line (#707), same undo step.
                        if requests[0].command == "shape.path"
                            && let Some(keys) = app.tools.take_satin_keys()
                        {
                            let ids = CommandRequest::created_ids(&result);
                            app.execute(&CommandRequest::new(
                                "object.setLine",
                                json!({
                                    "ids": CommandRequest::ids(&ids),
                                    "type": "satin",
                                    "widthKeys": keys,
                                }),
                            ))?;
                        }
                        Some(result)
                    });
                    if let Some(result) = result {
                        if select_result {
                            self.selection = CommandRequest::created_ids(&result);
                        }
                        // A command that reports what it changed (order.optimize, #547).
                        self.status = match result.get("summary").and_then(Value::as_str) {
                            Some(summary) => format!("{} done: {summary}", requests[0].command),
                            None => format!("{} done", requests[0].command),
                        };
                    }
                }
            }
        }
    }

    /// Sets the new-object defaults (#553) on the shapes just drawn: the region values on those
    /// with a region, the outline's on those with a line.
    pub(crate) fn apply_new_object_defaults(
        &mut self,
        ids: &BTreeSet<u64>,
        defaults: &openstitch_app::editor::new_objects::NewObjects,
    ) {
        let d = self.session.design();
        let with = |has: fn(&openstitch_design::DesignObject) -> bool| -> Vec<u64> {
            ids.iter()
                .copied()
                .filter(|id| d.index_of(*id).is_some_and(|i| has(&d.objects[i])))
                .collect()
        };
        let regions = with(|o| o.region.is_some());
        let lines = with(|o| o.line.is_some());
        for (command, ids, params) in [
            ("object.setRegion", regions, defaults.region_params()),
            ("object.setLine", lines, defaults.line_params()),
        ] {
            if let Some(mut p) = params
                && !ids.is_empty()
            {
                p.insert("ids".into(), json!(ids));
                self.execute(&CommandRequest::new(command, Value::Object(p)));
            }
        }
    }

    /// Recompiles after a change, keeping selection and simulator position valid.
    pub(crate) fn refresh(&mut self) {
        self.revision += 1;
        let d = self.session.design();
        self.selection.retain(|id| d.index_of(*id).is_some());
        let compiled = crate::open_job::Compiled::of(d);
        self.install_compiled(compiled);
    }

    /// Makes `c` the window's stitches: [`EditorApp::refresh`] without the compile, for a
    /// design compiled on a worker (#558).
    pub(crate) fn install(&mut self, c: crate::open_job::Compiled) {
        self.revision += 1;
        let d = self.session.design();
        self.selection.retain(|id| d.index_of(*id).is_some());
        self.install_compiled(c);
    }

    fn install_compiled(&mut self, c: crate::open_job::Compiled) {
        self.timeline = c.timeline;
        self.pattern = c.pattern;
        self.quality.spans = c.spans;
        self.stats = c.stats;
        self.workspace.estimate = Some(c.estimate);
        if self.simulate.is_some_and(|n| n >= self.stats.stitches) {
            self.simulate = None;
        }
        self.drafts.clear();
    }

    pub fn ensure_render_model(&mut self) {
        let limit = self.simulate.unwrap_or(usize::MAX);
        // A picture picker's hover preview (#656): its own stitches, never the design's cache.
        if let Some(p) = self.live_preview() {
            let key = (
                u64::MAX - p.generation,
                limit,
                openstitch_design::view::ViewFilter::default(),
                self.page_rgb,
            );
            if self.render_key.as_ref() != Some(&key) {
                self.render = RenderModel::build(&p.compiled.pattern, limit);
                self.render_key = Some(key);
            }
            return;
        }
        let filter = self.view_filter();
        let key = (self.revision, limit, filter.clone(), self.page_rgb);
        if self.render_key.as_ref() != Some(&key) {
            self.render = if filter.is_identity() {
                RenderModel::build(&self.pattern, limit)
            } else {
                if filter.layers != openstitch_design::view::Layers::Both
                    && self
                        .underlay
                        .as_ref()
                        .is_none_or(|(r, _)| *r != self.revision)
                {
                    let d = self.session.design();
                    let flags = openstitch_design::view::underlay_flags(
                        d,
                        &self.pattern,
                        &self.quality.spans,
                    );
                    self.underlay = Some((self.revision, flags));
                }
                let flags = self.underlay.as_ref().map_or(&[][..], |(_, f)| &f[..]);
                let page = self.page_rgb;
                let shown = openstitch_design::view::apply(
                    &self.pattern,
                    &filter,
                    flags,
                    &self.quality.spans,
                    page,
                );
                RenderModel::build(&shown, limit)
            };
            self.render_key = Some(key);
        }
    }

    /// View ▸ Show as a filter (#517): hide unselected only with a selection; solo colour on the
    /// simulator's block, else the selection's first block.
    pub fn view_filter(&self) -> openstitch_design::view::ViewFilter {
        let s = self.ui.prefs.show;
        let only = (s.hide_unselected && !self.selection.is_empty())
            .then(|| self.selection.iter().copied().collect());
        let solo = s.solo.then(|| self.current_block()).flatten();
        openstitch_design::view::ViewFilter {
            layers: s.layers,
            only,
            color_by: s.color_by,
            solo,
        }
    }

    /// The colour block of the simulator position, else of the first selected object's first
    /// stitch.
    fn current_block(&self) -> Option<usize> {
        let records = match self.simulate {
            Some(n) => {
                let mut sewn = 0;
                self.pattern.stitches.iter().position(|s| {
                    sewn += usize::from(s.command == openstitch_core::Command::Stitch);
                    sewn > n
                })?
            }
            None => {
                let (_, r) = self
                    .quality
                    .spans
                    .iter()
                    .find(|(id, _)| self.selection.contains(id))?;
                r.start
            }
        };
        Some(
            self.pattern.stitches[..records]
                .iter()
                .filter(|s| s.command == openstitch_core::Command::ColorChange)
                .count(),
        )
    }

    /// A `--script` or `--screenshot` run: background work is waited for, so each step sees
    /// its result.
    pub(crate) fn scripted(&self) -> bool {
        self.player.is_some() || self.screenshot.is_some()
    }

    pub fn is_dirty(&self) -> bool {
        self.revision != self.saved_revision
    }

    pub fn undo(&mut self) {
        let steps = self.undo_groups.pop().unwrap_or(1);
        let done = self.history("history.undo", steps);
        if done > 0 {
            self.redo_groups.push(done);
        }
    }

    pub fn redo(&mut self) {
        let steps = self.redo_groups.pop().unwrap_or(1);
        let done = self.history("history.redo", steps);
        if done > 0 {
            self.undo_groups.push(done);
        }
    }

    /// Runs `history.undo` / `history.redo` up to `steps` times; returns how many ran.
    fn history(&mut self, command: &'static str, steps: usize) -> usize {
        let mut done = 0;
        while done < steps
            && if command == "history.undo" {
                self.session.can_undo()
            } else {
                self.session.can_redo()
            }
        {
            if self
                .execute(&CommandRequest::new(command, json!({})))
                .is_none()
            {
                break;
            }
            done += 1;
        }
        done
    }

    pub fn select_all(&mut self) {
        self.selection = self
            .session
            .design()
            .objects
            .iter()
            .filter(|o| o.visible)
            .map(|o| o.id)
            .collect();
    }

    /// Whether the selection holds a locked object (#502); if so, says so in the status bar.
    /// Delete, Cut and the arrow keys leave locked objects alone.
    pub fn refuse_locked(&mut self) -> bool {
        let locked =
            openstitch_app::editor::objects::any_locked(self.session.design(), &self.selection);
        if locked {
            self.status = openstitch_app::editor::ui_texts::t("ui-status-locked-unlock-it-in-the-sewing").into();
        }
        locked
    }

    pub fn on_selection(&mut self, make: impl Fn(&BTreeSet<u64>) -> CommandRequest) {
        if self.selection.is_empty() {
            return;
        }
        let r = make(&self.selection);
        if let Some(result) = self.execute(&r) {
            let created = CommandRequest::created_ids(&result);
            if !created.is_empty() {
                self.selection = created;
            }
        }
    }

    // MARK: files

    /// The design as the title bar and the unsaved-changes prompt name it: its file name,
    /// or the design name while it has no file.
    pub fn document_name(&self) -> String {
        self.path.as_ref().and_then(|p| p.file_name()).map_or_else(
            || match &self.recovery.recovered_from {
                Some(from) => format!("{} (recovered from {from})", self.session.design().name),
                None => self.session.design().name.clone(),
            },
            |n| n.to_string_lossy().into_owned(),
        )
    }

    /// The design counts as unsaved until it is saved (a restored snapshot, #549).
    pub(crate) fn mark_unsaved(&mut self) {
        self.saved_revision = self.revision.wrapping_add(u64::MAX / 2);
    }

    /// Runs `what` now, or opens the unsaved-changes prompt for it when the design has
    /// unsaved changes (File ▸ New, Open, Exit and the Recent Files go through here).
    pub fn guarded(&mut self, what: Pending) {
        // Open designs in tabs (#552): New and Open keep this design in its tab.
        if self.ui.prefs.open_in_tabs && matches!(what, Pending::New | Pending::Open(_)) {
            self.perform_in_tab(what);
            return;
        }
        if self.close_prompt.request(what.clone(), self.is_dirty()) {
            self.perform(what);
        }
    }

    fn perform(&mut self, what: Pending) {
        match what {
            // The frame loop sends the close.
            Pending::Exit => match self.dirty_parked() {
                // Closing the window asks for each tab with unsaved changes (#552).
                Some(_) => {
                    self.close_active_tab();
                    if let Some(i) = self.dirty_parked() {
                        self.activate_tab(i);
                    }
                    self.close_prompt.request(Pending::Exit, self.is_dirty());
                    if !self.close_prompt.is_open() {
                        self.perform(Pending::Exit);
                    }
                }
                None => self.approve_exit(),
            },
            Pending::CloseTab => self.close_active_tab(),
            Pending::New => self.new_design(),
            Pending::Open(None) => self.open_dialog(),
            Pending::Open(Some(p)) => self.open_in_background(&p),
        }
    }

    /// New or Open with Open designs in tabs on (#552): an empty, untouched design is reused,
    /// otherwise a new tab is added; a cancelled or failed open closes the tab it added.
    fn perform_in_tab(&mut self, what: Pending) {
        let path = match what {
            Pending::Open(None) => {
                // The browser's picker answers later (#797): the open runs when it does.
                let mut exts = vec![openstitch_engine::project::EXTENSION];
                exts.extend(FileType::ALL.iter().map(|t| t.extension()));
                crate::files::pick_then(
                    crate::files::FileDialog::new()
                        .add_filter("Designs and embroidery files", &exts),
                    |app, p| app.perform_in_tab(Pending::Open(Some(p))),
                );
                return;
            }
            Pending::Open(Some(p)) => Some(p),
            _ => None,
        };
        let reuse = !self.is_dirty()
            && self.path.is_none()
            && self.session.design().objects.is_empty()
            && path.is_some();
        if !reuse {
            self.new_tab();
        }
        if let Some(p) = path {
            self.open_in_background(&p);
            match &mut self.open_job {
                Some(job) => job.added_tab = !reuse,
                // A scripted run applied it already; close the tab when nothing opened.
                None if !reuse
                    && self.session.design().objects.is_empty()
                    && self.path.is_none() =>
                {
                    let status = std::mem::take(&mut self.status);
                    self.close_active_tab();
                    self.status = status;
                }
                None => {}
            }
        }
    }

    /// The prompt's answer: Save goes on only when the save left the design clean (a
    /// cancelled Save As stays), Don't Save goes on, Cancel stays.
    pub fn answer_close_prompt(&mut self, choice: Choice) {
        match self.close_prompt.answer(choice) {
            Outcome::SaveThen(what) => {
                self.save(false);
                if !self.is_dirty() {
                    self.perform(what);
                }
            }
            Outcome::Proceed(what) => self.perform(what),
            Outcome::Stay => {}
        }
    }

    /// Lets the window close without the prompt: the frame loop closes it (File ▸ Exit, the
    /// prompt's Save and Don't Save, the updater's restart, a finished screenshot run).
    pub fn approve_exit(&mut self) {
        self.exit_approved = true;
    }

    /// Whether the window is about to close (see [`Self::approve_exit`]).
    pub fn exit_approved(&self) -> bool {
        self.exit_approved
    }

    /// The window was asked to close (X button, Alt+F4, the OS): with unsaved changes and no
    /// approval the close is cancelled and the prompt opens.
    fn close_request(&mut self, ctx: &egui::Context) {
        if !ctx.input(|i| i.viewport().close_requested()) || self.exit_approved || !self.any_dirty()
        {
            return;
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        if !self.is_dirty()
            && let Some(i) = self.dirty_parked()
        {
            self.activate_tab(i);
        }
        self.close_prompt.request(Pending::Exit, true);
    }

    pub fn new_design(&mut self) {
        if self
            .execute(&CommandRequest::new(
                "design.new",
                json!({ "name": "Untitled" }),
            ))
            .is_some()
        {
            self.after_load(None);
        }
    }

    pub(crate) fn after_load(&mut self, path: Option<PathBuf>) {
        self.recovery.recovered_from = None;
        self.path = path;
        self.selection.clear();
        self.simulate = None;
        self.tools.cancel();
        self.fit_requested = true;
        self.saved_revision = self.revision;
    }

    pub fn open_dialog(&mut self) {
        let mut exts = vec![openstitch_engine::project::EXTENSION];
        exts.extend(FileType::ALL.iter().map(|t| t.extension()));
        // A browser picks asynchronously: the page opens the file when it arrives (#693).
        #[cfg(target_arch = "wasm32")]
        crate::files::request_pick(
            crate::files::Pick::Open,
            "Designs and embroidery files",
            &exts,
        );
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = crate::files::FileDialog::new()
            .add_filter("Designs and embroidery files", &exts)
            .add_filter(
                "OpenStitch design",
                &[openstitch_engine::project::EXTENSION],
            )
            .add_filter("Embroidery files", &exts[1..])
            .pick_file()
        {
            self.open_in_background(&p);
        }
    }

    /// `.osd` opens as the design; a machine file opens as a new design holding it as an
    /// imported stitch pattern on a page large enough for it. Runs on the UI thread (launch
    /// `--open`, tests); the menus use [`EditorApp::open_in_background`].
    pub fn open_path(&mut self, p: &Path) {
        let steps = self.open_steps(p);
        if self.grouped(|app| app.execute_all(&steps)).is_some() {
            self.after_open(p);
        }
    }

    /// The commands that open `p`: `project.open`, or a new design sized for a machine file
    /// and its import.
    pub(crate) fn open_steps(&self, p: &Path) -> Vec<CommandRequest> {
        let path = p.display().to_string();
        if is_project(p) {
            return vec![CommandRequest::new("project.open", json!({ "path": path }))];
        }
        let (w, h) = machine_file_size_mm(p).unwrap_or((100.0, 100.0));
        let name = p
            .file_stem()
            .map_or_else(|| "Untitled".into(), |s| s.to_string_lossy().into_owned());
        let page = |v: f64| ((v + 10.0) / 10.0).ceil().max(10.0) * 10.0;
        // A PE-DESIGN 11 file keeps its own Design Page size.
        let (page_w, page_h) = pes_page_mm(p).unwrap_or((page(w), page(h)));
        vec![
            CommandRequest::new(
                "design.new",
                json!({ "name": name, "width": page_w, "height": page_h }),
            ),
            // The file stays with the design: exporting it unchanged as PES writes it back as
            // it was, PE-DESIGN's objects included (#375). The import centres it on the page.
            CommandRequest::new(
                "stitches.import",
                json!({ "path": path, "keepSource": true }),
            ),
        ]
    }

    /// After `p` opened: file name, recent files and status.
    pub(crate) fn after_open(&mut self, p: &Path) {
        self.ui.add_recent(p);
        if is_project(p) {
            self.after_load(Some(p.to_path_buf()));
            self.status = openstitch_app::tf!("ui-status-opened", path = p.display());
        } else {
            self.after_load(None);
            self.status = openstitch_app::tf!("ui-status-opened-as-a-new-design-save", path = p.display());
        }
    }

    pub fn import_dialog(&mut self) {
        let exts: Vec<_> = FileType::ALL.iter().map(|t| t.extension()).collect();
        #[cfg(target_arch = "wasm32")]
        crate::files::request_pick(crate::files::Pick::Import, "Embroidery files", &exts);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(p) = crate::files::FileDialog::new()
            .add_filter("Embroidery files", &exts)
            .pick_file()
        {
            self.import_in_background(&p);
        }
    }

    /// Home ▸ Import ▸ Import PE-DESIGN Objects: a PE-DESIGN 11 (`#PES0110`) file's shapes,
    /// texts and stitch groups as editable objects at their places, its groups kept (#397).
    pub fn import_pes_objects_dialog(&mut self) {
        crate::files::pick_then(
            crate::files::FileDialog::new().add_filter("PE-DESIGN 11 file", &["pes"]),
            |app, p| {
                app.import_pes_objects(&p);
            },
        );
    }

    /// Imports `p`'s PE-DESIGN objects in one undo step; the status says what was skipped.
    pub fn import_pes_objects(&mut self, p: &Path) -> bool {
        let request = CommandRequest::new("import.pes", json!({ "path": p.display().to_string() }));
        let Some(r) = self.grouped(|app| app.execute(&request)) else {
            return false;
        };
        let n = r["ids"].as_array().map_or(0, Vec::len);
        let skipped = r["skipped"].as_array().cloned().unwrap_or_default();
        self.status = match skipped.first() {
            None => format!("Imported {n} objects from {}", p.display()),
            Some(first) => format!(
                "Imported {n} objects from {}; {} not imported (first: {})",
                p.display(),
                skipped.len(),
                first["reason"].as_str().unwrap_or("not mapped")
            ),
        };
        true
    }

    /// File ▸ Import SVG: the artwork's paths (or a WMF/EMF drawing's shapes) become a group of
    /// editable shapes, centred.
    pub fn import_svg_dialog(&mut self) {
        crate::files::pick_then(
            crate::files::FileDialog::new()
                .add_filter("Vector images", &["svg", "wmf", "emf"])
                .add_filter("SVG vector image", &["svg"])
                .add_filter("Windows metafile", &["wmf", "emf"]),
            |app, p| app.import_svg_from(&p),
        );
    }

    /// Imports the SVG or WMF/EMF drawing at `p` as editable shapes, centred.
    pub fn import_svg_from(&mut self, p: &Path) {
        let metafile = p
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("wmf") || e.eq_ignore_ascii_case("emf"));
        let command = if metafile {
            "import.metafile"
        } else {
            "import.svg"
        };
        let request = CommandRequest::new(command, json!({ "path": p.display().to_string() }));
        if let Some(result) = self.grouped(|app| app.execute(&request)) {
            self.selection = CommandRequest::created_ids(&result);
            self.status = openstitch_app::tf!("ui-status-imported", path = p.display(), result = import_report(&result));
        }
    }

    /// File ▸ Background Image: a PNG, JPEG or BMP shown behind the design, fitted to the page.
    pub fn background_dialog(&mut self) {
        crate::files::pick_then(
            crate::files::FileDialog::new().add_filter("Images", &["png", "jpg", "jpeg", "bmp"]),
            |app, p| app.background_from(&p),
        );
    }

    /// Shows the image at `p` behind the design, fitted to the page.
    pub fn background_from(&mut self, p: &Path) {
        if self
                .execute(&CommandRequest::new(
                    "background.set",
                    json!({ "path": p.display().to_string() }),
                ))
                .is_some()
        {
            self.status = openstitch_app::tf!("ui-status-background", path = p.display());
        }
    }

    /// File ▸ Background Image ▸ Paste: the clipboard image becomes the background. It is kept
    /// as a PNG in the app's data folder (named by content), since the project only references it.
    pub fn paste_background(&mut self) {
        if crate::limits_ui::browser_limit(
            self,
            openstitch_app::editor::browser_limits::Limit::ClipboardImages,
        ) {
            return;
        }
        if !self.ui.system_clipboard {
            self.status = openstitch_app::editor::ui_texts::t("ui-status-paste-background-the-system-clipboard-is").into();
            return;
        }
        let image = crate::ribbon::os_clipboard()
            .and_then(|()| openstitch_app::platform::clipboard_image());
        let stored = match image {
            Ok(img) => pasted_images_dir()
                .ok_or_else(|| "no folder to keep pasted images in".to_string())
                .and_then(|dir| {
                    store_pasted_image(&dir, img.width as u32, img.height as u32, img.bytes)
                }),
            Err(e) => Err(format!("no image on the clipboard ({e})")),
        };
        match stored {
            Ok(path) => {
                if self
                    .execute(&CommandRequest::new(
                        "background.set",
                        json!({ "path": path.display().to_string() }),
                    ))
                    .is_some()
                {
                    self.status = openstitch_app::editor::ui_texts::t("ui-status-background-pasted-from-the-clipboard").into();
                }
            }
            Err(e) => self.status = openstitch_app::tf!("ui-status-paste-background", e = e),
        }
    }

    /// Import into design: adds the machine file as a stitch pattern, centred on the page.
    pub fn import(&mut self, p: &Path) -> bool {
        let Some(result) = self.execute(&CommandRequest::new(
            "stitches.import",
            json!({ "path": p.display().to_string() }),
        )) else {
            return false;
        };
        let ids = CommandRequest::created_ids(&result);
        self.execute(&CommandRequest::new(
            "object.center",
            json!({ "ids": CommandRequest::ids(&ids) }),
        ));
        self.selection = ids;
        true
    }

    pub fn save(&mut self, save_as: bool) {
        let path = match (&self.path, save_as) {
            (Some(p), false) => Some(p.clone()),
            _ => crate::files::FileDialog::new()
                .add_filter(
                    "OpenStitch design",
                    &[openstitch_engine::project::EXTENSION],
                )
                .set_file_name(format!(
                    "{}.{}",
                    self.session.design().name,
                    openstitch_engine::project::EXTENSION
                ))
                .save_file(),
        };
        let Some(path) = path else { return };
        if self
            .execute(&CommandRequest::new(
                "project.save",
                json!({ "path": path.display().to_string() }),
            ))
            .is_some()
        {
            self.status = openstitch_app::tf!("ui-status-saved", path = path.display());
            self.ui.add_recent(&path);
            self.path = Some(path);
            self.saved_revision = self.revision;
        }
    }

    pub fn export(&mut self, ty: FileType) {
        self.export_with(ty, None);
    }

    /// PES versions offered in File ▸ Export ▸ PES: `pesVersion` value and menu label.
    pub const PES_VERSIONS: [(&'static str, &'static str); 11] = [
        ("6", "Version 6 — RGB colours, names, stitch group"),
        (
            "11",
            "PE-DESIGN 11 (#PES0110) — editable shapes, texts and stitch groups",
        ),
        ("6t", "Version 6, stitches only"),
        ("10", "Version 10 — stitch data"),
        ("9", "Version 9 — stitch data"),
        ("8", "Version 8 — stitch data"),
        ("7", "Version 7 — stitch data"),
        ("5", "Version 5 — stitch data"),
        ("4", "Version 4 — stitch data, chart colours"),
        ("1", "Version 1 — stitch group, chart colours"),
        ("1t", "Version 1, stitches only (machine only, smallest)"),
    ];

    /// Thread list files in File ▸ Export ▸ Thread List (#482): extension and menu label.
    pub const THREAD_LISTS: [(&'static str, &'static str); 5] = [
        (
            "csv",
            "CSV — colours in sewing order with stitches and thread length",
        ),
        ("inf", "INF (Bernina)"),
        ("edr", "EDR"),
        ("col", "COL"),
        ("txt", "TXT"),
    ];

    /// File ▸ Export ▸ EXP with .inf colour file: `design.export {companionInf: true}`.
    pub fn export_exp_with_inf(&mut self) {
        self.export_with_params(FileType::Exp, None, true);
    }

    /// Writes a stitch list (`csv`, `txt`) or G-code (`gcode`) through `design.export` (#488).
    pub fn export_extra(&mut self, ext: &str) {
        let name = format!("{}.{ext}", self.session.design().name);
        let Some(path) = crate::files::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(name)
            .save_file()
        else {
            return;
        };
        let params = json!({ "path": path.display().to_string(), "format": ext });
        if let Some(r) = self.execute(&CommandRequest::new("design.export", params)) {
            self.status = openstitch_app::tf!("ui-status-exported-stitches", path = path.display(), stats = r["stats"]["stitches"]);
        }
    }

    /// Writes a picture, a stitch-out video or a ZIP bundle with `command` (#488).
    pub fn export_media(&mut self, command: &'static str, ext: &str) {
        let suffix = if command == "design.exportVideo" {
            " stitch-out"
        } else {
            ""
        };
        let name = format!("{}{suffix}.{ext}", self.session.design().name);
        let Some(path) = crate::files::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(name)
            .save_file()
        else {
            return;
        };
        let params = json!({ "path": path.display().to_string() });
        if let Some(r) = self.execute(&CommandRequest::new(command, params)) {
            self.status = match r["frames"].as_u64() {
                Some(frames) => format!("Wrote {} ({frames} frames)", path.display()),
                None => format!("Wrote {}", path.display()),
            };
        }
    }

    /// File ▸ Export ▸ Garment Mockup's entries: garment, placement, label (#454).
    pub const MOCKUPS: [(&'static str, &'static str, &'static str); 4] = [
        ("tshirt", "centre", "T-shirt, centre chest"),
        ("tshirt", "leftChest", "T-shirt, left chest"),
        ("cap", "centre", "Cap"),
        ("tote", "centre", "Tote bag"),
    ];

    /// The design on a garment as a PNG (`design.mockup`, #454); the status says when the
    /// design is larger than the placement suits.
    pub fn export_mockup(&mut self, garment: &str, placement: &str, fabric: [u8; 3]) {
        let name = format!("{} on {garment}.png", self.session.design().name);
        let Some(path) = crate::files::FileDialog::new()
            .add_filter("PNG", &["png"])
            .set_file_name(name)
            .save_file()
        else {
            return;
        };
        let [r, g, b] = fabric;
        let params = json!({
            "path": path.display().to_string(),
            "garment": garment,
            "placement": placement,
            "color": format!("#{r:02x}{g:02x}{b:02x}"),
        });
        if let Some(out) = self.execute(&CommandRequest::new("design.mockup", params)) {
            self.status = if out["fits"] == json!(false) {
                format!(
                    "Wrote {}; the design ({} × {} mm) is larger than this place suits ({} × {} mm)",
                    path.display(),
                    out["designMm"][0],
                    out["designMm"][1],
                    out["suitsMm"][0],
                    out["suitsMm"][1]
                )
            } else {
                format!("Wrote {}", path.display())
            };
        }
    }

    /// Writes the design's colours in sewing order (`design.exportThreads`).
    pub fn export_thread_list(&mut self, ext: &str) {
        let name = format!("{} threads.{ext}", self.session.design().name);
        let Some(path) = crate::files::FileDialog::new()
            .add_filter(ext.to_uppercase(), &[ext])
            .set_file_name(name)
            .save_file()
        else {
            return;
        };
        let params = json!({ "path": path.display().to_string(), "format": ext });
        if let Some(r) = self.execute(&CommandRequest::new("design.exportThreads", params)) {
            self.status = openstitch_app::tf!("ui-status-wrote-colours-to", colors = r["colors"], path = path.display());
        }
    }

    /// Applies a thread list file to the colour blocks (`design.applyThreads`).
    pub fn apply_thread_list(&mut self) {
        crate::files::pick_then(
            crate::files::FileDialog::new()
                .add_filter("Thread lists", &["csv", "inf", "edr", "col", "txt"]),
            |app, path| app.apply_thread_list_from(&path),
        );
    }

    /// Gives the colour blocks the colours of the thread list at `path` (`design.applyThreads`).
    pub fn apply_thread_list_from(&mut self, path: &Path) {
        let params = json!({ "path": path.display().to_string() });
        if let Some(r) = self.execute(&CommandRequest::new("design.applyThreads", params)) {
            self.status = openstitch_app::tf!("ui-status-applied-colours-from", colors = r["colors"], path = path.display());
        }
    }

    /// Exports `ty`; `pes_version` picks the PES variant (`None` = the engine default).
    pub fn export_with(&mut self, ty: FileType, pes_version: Option<&str>) {
        self.export_with_params(ty, pes_version, false);
    }

    fn export_with_params(&mut self, ty: FileType, pes_version: Option<&str>, companion_inf: bool) {
        let name = format!("{}.{}", self.session.design().name, ty.extension());
        let Some(path) = crate::files::FileDialog::new()
            .add_filter(ty.extension().to_uppercase(), &[ty.extension()])
            .set_file_name(name)
            .save_file()
        else {
            return;
        };
        // A design over the format's limits is written as NAME-1.ext, NAME-2.ext…
        let mut params = json!({
            "path": path.display().to_string(),
            "format": ty.extension(),
            "split": true,
        });
        if let Some(v) = pes_version {
            params["pesVersion"] = json!(v);
        }
        if companion_inf {
            params["companionInf"] = json!(true);
        }
        if let Some(r) = self.execute(&CommandRequest::new("design.export", params)) {
            let files = r["paths"].as_array().map_or(1, Vec::len);
            // A design outside the Design Page is written, as PE-DESIGN does; say so.
            if let Some(w) = r["warnings"]
                .as_array()
                .and_then(|w| w.first())
                .and_then(|w| w.as_str())
            {
                self.status = openstitch_app::tf!("ui-status-exported", path = path.display(), w = w);
                return;
            }
            self.status = if files > 1 {
                format!(
                    "Exported {} files ({}-1 …) — the design exceeds one {} file",
                    files,
                    path.with_extension("").display(),
                    ty.extension().to_uppercase()
                )
            } else {
                format!(
                    "Exported {} ({} stitches)",
                    path.display(),
                    r["stats"]["stitches"]
                )
            };
        }
    }

    // MARK: frame

    /// Hands a PDF OpenStitch wrote to the system's print dialog (or its PDF viewer).
    pub fn print_pdf(&mut self, pdf: &Path) {
        self.status = openstitch_app::tf!("ui-status-opening-the-print-dialog-for", path = pdf.display());
        self.database.status = self.status.clone();
        self.printed = Some(pdf.to_path_buf());
        // Tests run every command; they must never open a real print dialog.
        if !cfg!(test) {
            self.print_job = Some(openstitch_app::os_print::start(pdf));
        }
    }

    fn poll_print(&mut self, ctx: &egui::Context) {
        let Some(job) = &self.print_job else {
            return;
        };
        match job.poll() {
            Some(outcome) => {
                self.status = outcome.message();
                self.database.status = self.status.clone();
                self.print_job = None;
            }
            None => ctx.request_repaint_after(std::time::Duration::from_millis(200)),
        }
    }

    fn update_title(&mut self, ctx: &egui::Context) {
        let name = self.document_name();
        let program = if self.database.open {
            Some(openstitch_app::editor::suite::DATABASE.title)
        } else if self.creator.open {
            Some(openstitch_app::editor::suite::CREATOR.title)
        } else if self.sewing.open {
            Some(openstitch_app::editor::suite::FACTORY.title)
        } else {
            None
        };
        let title = match program {
            Some(p) => format!("{p} — OpenStitch"),
            None => format!(
                "{}{} — OpenStitch",
                name,
                if self.is_dirty() { " •" } else { "" }
            ),
        };
        if title != self.title {
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(title.clone()));
            self.title = title;
        }
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        use openstitch_app::editor::ribbon::Action as A;
        // The user's keymap (#551); the defaults are the keys handled here before.
        let hits = crate::keymap_ui::pressed(self, ctx);
        let hit = |a: A| hits.contains(&a);
        if self.ui.keys.recording.is_some() {
            return;
        }
        let cmd = |k| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let take = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if hit(A::Redo) {
            self.redo();
        } else if hit(A::Undo) {
            self.undo();
        }
        if hit(A::SaveAs) {
            self.save(true);
        } else if hit(A::Save) {
            self.save(false);
        }
        if hit(A::Open) {
            self.guarded(Pending::Open(None));
        }
        if hit(A::ImportPatterns) {
            self.import_dialog();
        }
        if hit(A::New) {
            self.guarded(Pending::New);
        }
        // Tabs (#552): Ctrl+Tab and Ctrl+Shift+Tab go to the next and previous design.
        let tab = |m: Modifiers| KeyboardShortcut::new(m, Key::Tab);
        if take(tab(Modifiers::COMMAND | Modifiers::SHIFT)) {
            self.cycle_tab(true);
        } else if take(tab(Modifiers::COMMAND)) {
            self.cycle_tab(false);
        }
        if hit(A::OtherZoom) {
            self.fit_requested = true;
        }
        // Command palette (#550, beyond PE-DESIGN).
        if take(cmd(Key::K)) {
            self.palette.show();
        }
        if hit(A::PaneAssistant) && self.ui.prefs.ai.enabled {
            let open = !self.assistant.panel.open;
            crate::assistant_panel::set_open(self, open);
        }
        if hit(A::ZoomIn) {
            crate::panels::zoom_center(self, 1.25);
        }
        if hit(A::ZoomOut) {
            crate::panels::zoom_center(self, 1.0 / 1.25);
        }
        // Commands the user gave a shortcut (#551) run as a click on them would.
        for &a in &hits {
            if !openstitch_app::editor::keymap::DEFAULTS
                .iter()
                .any(|(d, _)| *d == a)
                && crate::ribbon::enabled(self, a)
            {
                crate::ribbon::run(self, a, ctx);
            }
        }
        if ctx.egui_wants_keyboard_input() {
            return;
        }
        let take = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        // Cut, Copy and Paste arrive as clipboard events, not key presses.
        let (cut, copy, paste) = ctx.input(|i| {
            (
                i.events.iter().any(|e| matches!(e, egui::Event::Cut)),
                i.events.iter().any(|e| matches!(e, egui::Event::Copy)),
                i.events.iter().any(|e| matches!(e, egui::Event::Paste(_))),
            )
        });
        if cut {
            crate::ribbon::run(self, openstitch_app::editor::ribbon::Action::Cut, ctx);
        }
        if copy {
            crate::ribbon::run(self, openstitch_app::editor::ribbon::Action::Copy, ctx);
        }
        if paste {
            crate::ribbon::run(self, openstitch_app::editor::ribbon::Action::Paste, ctx);
        }
        if hit(A::SelectAll) {
            self.select_all();
        }
        if hit(A::Duplicate) {
            self.on_selection(CommandRequest::duplicate);
        }
        if hit(A::Ungroup) {
            crate::edit_ui::ungroup(self);
        } else if hit(A::Group) {
            crate::edit_ui::group(self);
        }
        if hit(A::Delete) && self.tools.is_busy() && crate::keyboard_ui::draws(self.tools.tool()) {
            // While drawing, Backspace / Delete take back the last point (#499).
            self.tools.remove_last_point();
        } else if hit(A::Delete) && crate::view_ui::delete_selected_guide(self) {
            // A selected guideline goes first (#799).
        } else if hit(A::Delete) {
            // The Point tool deletes the selected vertex; otherwise the selected objects.
            let mut point_deleted = false;
            self.with_tools(|t, c| {
                let effects = t.delete_point(c);
                point_deleted = effects.is_some();
                effects.unwrap_or_default()
            });
            if !point_deleted {
                self.on_selection(CommandRequest::delete);
            }
        }
        if take(KeyboardShortcut::new(Modifiers::NONE, Key::Escape)) {
            self.keyboard = Default::default();
            if self.zoom_box.active {
                self.zoom_box = Default::default();
            } else if self.edit.cancel() {
                self.tools.cancel();
                self.status = openstitch_app::editor::ui_texts::t("ui-status-cancelled").into();
            } else if self.workspace.measure.active {
                self.workspace.measure.active = false;
                self.workspace.measure.clear();
            } else if self.workspace.sections.active {
                self.workspace.sections.toggle();
            } else if self.tools.is_busy() {
                self.tools.cancel();
            } else if matches!(self.tools.tool(), Tool::Knife | Tool::Gradient | Tool::FillAngle | Tool::AngleLine) {
                // The Knife, Gradient and Fill Angle tools stay active between uses; Esc leaves them.
                self.tools.set_tool(Tool::Select);
            } else {
                self.selection.clear();
            }
        }
        if take(KeyboardShortcut::new(Modifiers::NONE, Key::Enter)) {
            let effects = self.tools.finish();
            self.apply_effects(effects);
        }
        let player_t = self.player_keys(&take);
        // Keyboard drawing (#499): with a drawing tool the arrows move the drawing cursor.
        crate::keyboard_ui::keys(self, ctx);
        for (key, dx, dy) in [
            (Key::ArrowLeft, -1.0, 0.0),
            (Key::ArrowRight, 1.0, 0.0),
            (Key::ArrowUp, 0.0, -1.0),
            (Key::ArrowDown, 0.0, 1.0),
        ] {
            // Arrow: the nudge step (0.1 mm by default), or one grid step while snapping with
            // Nudge by Grid on; Shift: ten times that.
            let e = self.ui.prefs.editing;
            let base = match self.workspace.grid.snap_step() {
                Some(grid) if e.nudge_grid => grid,
                _ => e.nudge_mm * 10.0,
            };
            for (mods, step) in [(Modifiers::SHIFT, base * 10.0), (Modifiers::NONE, base)] {
                if take(KeyboardShortcut::new(mods, key)) && !self.refuse_locked() {
                    self.on_selection(|s| CommandRequest::move_by(s, dx * step, dy * step));
                }
            }
        }
        let typed: Vec<char> = ctx.input(|i| {
            i.events
                .iter()
                .filter_map(|e| match e {
                    egui::Event::Text(t) if t.chars().count() == 1 => t.chars().next(),
                    _ => None,
                })
                .collect()
        });
        if !ctx.input(|i| i.modifiers.command) {
            for c in typed {
                if player_t && c.eq_ignore_ascii_case(&'t') {
                    continue;
                }
                // Point tool: C curves, S straightens the segment under the pointer.
                if c.eq_ignore_ascii_case(&'z') {
                    // Zoom Box (#518).
                    self.ui.pan = false;
                    self.zoom_box.toggle();
                } else if c.eq_ignore_ascii_case(&'m') {
                    self.ui.pan = false;
                    self.toggle_measure();
                } else if c.eq_ignore_ascii_case(&'h') {
                    crate::ribbon::run(self, openstitch_app::editor::ribbon::Action::Pan, ctx);
                } else if self.tools.tool() == Tool::Point && matches!(c, 'c' | 'C' | 's' | 'S') {
                    let curved = c.eq_ignore_ascii_case(&'c');
                    self.with_tools(|t, ctx| t.set_segment_under_pointer(curved, ctx));
                } else if let Some(t) = Tool::from_shortcut(c) {
                    self.tools.set_tool(t);
                    self.workspace.measure.active = false;
                    self.ui.pan = false;
                }
            }
        }
    }

    /// macOS: with no default application menu (see `main.rs`), Cmd+Q and Cmd+W close the
    /// window through the unsaved-changes prompt and Cmd+M minimises it. Taken before
    /// everything else, whatever has the keyboard and whichever suite window fills the editor,
    /// as the menu's key equivalents were. Platform-independent so it is tested everywhere;
    /// `update` calls it on macOS only.
    #[cfg_attr(not(target_os = "macos"), allow(dead_code))]
    fn macos_app_shortcuts(&mut self, ctx: &egui::Context) {
        let take = |k| {
            ctx.input_mut(|i| i.consume_shortcut(&KeyboardShortcut::new(Modifiers::COMMAND, k)))
        };
        if take(Key::Q) || take(Key::W) {
            self.guarded(Pending::Exit);
        } else if take(Key::M) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
    }

    /// Whether this is a `--screenshot` run.
    pub fn screenshot_run(&self) -> bool {
        self.screenshot.is_some()
    }

    /// A wizard dialog's window. In a `--screenshot` run it opens at the window's top right,
    /// over the right-hand pane, and the page is fitted beside it (#565); otherwise at
    /// `default_pos` (egui's own placement when `None`).
    pub fn dialog_window<'a>(
        &self,
        ctx: &egui::Context,
        title: &'static str,
        default_pos: Option<[f32; 2]>,
    ) -> egui::Window<'a> {
        let window = egui::Window::new(title);
        match (self.screenshot_run(), self.workspace.canvas) {
            (true, Some(canvas)) => window
                .pivot(egui::Align2::RIGHT_TOP)
                .default_pos([ctx.content_rect().max.x - 8.0, canvas.min.y]),
            _ => match default_pos {
                Some(pos) => window.default_pos(pos),
                None => window,
            },
        }
    }

    fn screenshot_step(&mut self, ctx: &egui::Context) {
        let Some(shot) = &mut self.screenshot else {
            return;
        };
        if shot.done.load(Ordering::SeqCst) {
            // A scripted run may have edited the design: the screenshot is the run's output,
            // so the window closes without asking.
            self.exit_approved = true;
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            return;
        }
        shot.frames += 1;
        ctx.request_repaint();
        let scripted = self.player.as_ref().is_some_and(|p| !p.is_done());
        if shot.frames < 8 || shot.requested || scripted {
            return;
        }
        shot.requested = true;
        let (path, done, repaint) = (shot.path.clone(), shot.done.clone(), ctx.clone());
        ctx.request_screenshot(move |image| {
            let rgba: Vec<u8> = image
                .pixels
                .iter()
                .flat_map(|c| c.to_srgba_unmultiplied())
                .collect();
            let [w, h] = image.size;
            match openstitch_app::png::encode_rgba(w as u32, h as u32, &rgba) {
                Some(bytes) => {
                    if let Err(e) = std::fs::write(&path, bytes) {
                        eprintln!("cannot write {}: {e}", path.display());
                    }
                }
                None => eprintln!("screenshot has no pixels"),
            }
            done.store(true, Ordering::SeqCst);
            repaint.request_repaint();
        });
    }

    /// View ▸ Snap to grid on or off.
    pub fn toggle_snap_to_grid(&mut self) {
        self.workspace.grid.snap = !self.workspace.grid.snap;
    }

    /// Measure tool on or off; it takes over the canvas pointer while on.
    pub fn toggle_measure(&mut self) {
        let m = &mut self.workspace.measure;
        m.active = !m.active;
        m.clear();
        self.status = if m.active {
            "Measure: drag on the canvas (Shift = 45° steps) · Esc or M ends".into()
        } else {
            "Ready".into()
        };
    }

    /// Stitch-out player keys while the simulator shows part of the design (#516): ←/→ a
    /// stitch, Shift a colour block, Ctrl/Cmd an object, T the next trim, Home/End. Returns
    /// whether T was taken, so it does not also pick a tool.
    fn player_keys(&mut self, take: &dyn Fn(KeyboardShortcut) -> bool) -> bool {
        use openstitch_app::editor::player::{Step, step};
        if self.simulate.is_none() {
            return false;
        }
        let keys = [
            (Modifiers::SHIFT, Key::ArrowLeft, Step::BlockBack),
            (Modifiers::SHIFT, Key::ArrowRight, Step::BlockForward),
            (Modifiers::COMMAND, Key::ArrowLeft, Step::ObjectBack),
            (Modifiers::COMMAND, Key::ArrowRight, Step::ObjectForward),
            (Modifiers::NONE, Key::ArrowLeft, Step::StitchBack),
            (Modifiers::NONE, Key::ArrowRight, Step::StitchForward),
            (Modifiers::NONE, Key::Home, Step::Start),
            (Modifiers::NONE, Key::End, Step::End),
            (Modifiers::NONE, Key::T, Step::NextTrim),
        ];
        let mut t = false;
        for (mods, key, s) in keys {
            if take(KeyboardShortcut::new(mods, key)) {
                self.simulate = step(&self.timeline, self.simulate, s);
                self.playing = false;
                t |= key == Key::T;
            }
        }
        t
    }

    /// Advances the simulator while playing.
    fn tick(&mut self, ctx: &egui::Context) {
        if !self.playing {
            return;
        }
        let dt = f64::from(ctx.input(|i| i.stable_dt)).min(0.1);
        let speed = if self.stitch_player.auto {
            openstitch_app::editor::player::auto_speed(&self.timeline)
        } else {
            self.play_speed
        };
        self.play_accumulator += dt * speed;
        let step = self.play_accumulator.floor() as usize;
        self.play_accumulator -= step as f64;
        // Pause at trims: playback stops at each trim, stop and colour change (#516).
        let (next, stopped) = openstitch_app::editor::player::advance(
            &self.timeline,
            Some(self.simulate.unwrap_or(0)),
            step,
            self.stitch_player.pause,
        );
        self.simulate = next;
        if stopped {
            self.playing = false;
            self.play_accumulator = 0.0;
        }
        ctx.request_repaint();
    }
}

/// Stitch-out player toggles (#516), beyond PE-DESIGN and off by default.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PlayerToggles {
    /// Play the whole design in about 12 seconds.
    pub auto: bool,
    /// Keep the needle in view.
    pub follow: bool,
    /// Stop at each trim, stop and colour change.
    pub pause: bool,
}

impl eframe::App for EditorApp {
    fn raw_input_hook(&mut self, ctx: &egui::Context, raw: &mut egui::RawInput) {
        self.frames += 1;
        // Start once the canvas has laid out and fitted the page.
        let (Some(player), Some(vp)) = (&mut self.player, self.viewport) else {
            return;
        };
        if self.frames < 4 {
            return;
        }
        if let Some((events, modifiers)) = player.next_frame(&vp) {
            raw.events.push(egui::Event::ModifiersChanged(modifiers));
            raw.events.extend(events);
            if let Some(path) = player.take_dropped() {
                raw.dropped_files
                    .push(crate::script::DroppedPath::handle(path));
            }
            if player.take_close_request() {
                raw.viewports
                    .entry(raw.viewport_id)
                    .or_default()
                    .events
                    .push(egui::ViewportEvent::Close);
            }
            ctx.request_repaint();
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        // Files chosen in a dialog (or picked by the page) since the last frame (#693).
        crate::files::run_ready(self);
        // Options ▸ Language (#555): the window's texts for this frame.
        openstitch_app::editor::i18n::set_language(&self.ui.prefs.language);
        // A hover preview lasts while a picture picker's tile is hovered (#656).
        self.pickers_frame_end();
        self.pickers_frame_start();
        if self.preview_pending() {
            ctx.request_repaint_after(std::time::Duration::from_millis(50));
        }
        self.dialog_rect = self.dialog_rect_next.take();
        openstitch_app::editor::objects::track_click_order(&mut self.click_order, &self.selection);
        crate::ribbon::poll_clipboard(self, ctx.input(|i| i.time));
        self.ui.apply_theme(&ctx);
        // Options ▸ Text size (#499), on top of the system's display scale.
        let zoom = self.ui.prefs.text_size as f32 / 100.0;
        if (ctx.zoom_factor() - zoom).abs() > f32::EPSILON {
            ctx.set_zoom_factor(zoom);
        }
        if let Some(s) = &mut self.splash
            && !s.show(&ctx)
        {
            self.splash = None;
        }
        self.serve_remote();
        if let Some(url) = self.assistant.open_url.take() {
            ctx.open_url(egui::OpenUrl::new_tab(url));
        }
        self.poll_print(&ctx);
        crate::update_ui::poll(self, &ctx);
        crate::update_ui::window(self, &ctx);
        crate::machine_setup_ui::window(self, &ctx);
        crate::startup_wizard_ui::window(self, &ctx);
        self.close_request(&ctx);
        if self.exit_approved() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
        // Before the shortcuts, so the prompt takes Enter and Escape.
        crate::close_ui::window(self, &ctx);
        #[cfg(target_os = "macos")]
        self.macos_app_shortcuts(&ctx);
        if self.database.open || self.creator.open || self.sewing.open {
            // The other suite windows take the whole window, as separate programs do.
            self.update_title(&ctx);
            if self.database.open {
                crate::database_ui::database(self, ui);
            } else if self.creator.open {
                crate::creator_ui::creator(self, ui);
            } else {
                crate::sewing_ui::factory(self, ui);
            }
            self.screenshot_step(&ctx);
            return;
        }
        self.poll_job(&ctx);
        // The progress window is modal: no shortcut edits the design under a running open.
        if self.open_job.is_none() {
            self.shortcuts(&ctx);
        }
        self.tick(&ctx);
        if crate::recovery_ui::tick(self, ctx.input(|i| i.time)) {
            ctx.request_repaint_after(std::time::Duration::from_millis(500));
        }
        self.update_title(&ctx);
        egui::Panel::top("ribbon").show(ui, |ui| crate::ribbon::ribbon(self, ui));
        if self.tab_count() > 1 || self.ui.prefs.open_in_tabs {
            egui::Panel::top("documents").show(ui, |ui| documents::strip(self, ui));
        }
        if self.ui.prefs.editing.precision_bar {
            egui::Panel::top("precision").show(ui, |ui| crate::arrange_ui::precision_bar(self, ui));
        }
        egui::Panel::bottom("status").show(ui, |ui| crate::panes::status_bar(self, ui));
        crate::assistant_panel::panel(self, ui);
        crate::panes::docked(self, ui);
        egui::CentralPanel::default()
            .frame(egui::Frame::NONE)
            .show(ui, |ui| crate::canvas::canvas(self, ui));
        crate::start_ui::cards(self, &ctx);
        crate::start_ui::windows(self, &ctx);
        crate::limits_ui::poll(self);
        crate::limits_ui::window(self, &ctx);
        crate::panes::floating_panes(self, &ctx);
        crate::panes::windows(self, &ctx);
        crate::recovery_ui::window(self, &ctx);
        crate::palette_ui::window(self, &ctx);
        crate::workspace_ui::design_settings_window(self, &ctx);
        crate::print_ui::window(self, &ctx);
        crate::swatch_ui::window(self, &ctx);
        crate::jobsheet_ui::window(self, &ctx);
        crate::transfer_ui::window(self, &ctx);
        crate::edit_ui::windows(self, &ctx);
        crate::settings_ui::windows(self, &ctx);
        crate::colour_match_ui::windows(self, &ctx);
        crate::wizard_ui::image_wizard_window(self, &ctx);
        crate::lettering_ui::windows(self, &ctx);
        crate::pattern_wizard_ui::windows(self, &ctx);
        crate::view_ui::windows(self, &ctx);
        crate::stitch_table_ui::window(self, &ctx);
        crate::zoom_ui::calibration_window(self, &ctx);
        self.monitor = crate::zoom_ui::monitor(&ctx);
        crate::quality_ui::window(self, &ctx);
        let canvas = self.workspace.canvas;
        crate::workspace_ui::reference_window(self, &ctx, canvas);
        self.screenshot_step(&ctx);
    }
}

fn is_project(p: &Path) -> bool {
    p.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case(openstitch_engine::project::EXTENSION))
}

/// Width and height (mm) of a machine file, used only to size the page before importing.
/// The Design Page size in mm a PE-DESIGN 11 `#PES0110` file was saved with.
fn pes_page_mm(p: &Path) -> Option<(f64, f64)> {
    if FileType::from_path(p)? != FileType::Pes {
        return None;
    }
    let bytes = openstitch_app::platform::read_small(p, 64 << 20)?;
    let section = openstitch_formats::pes::read_objects(&bytes).ok()?;
    let (w, h) = section.page_mm.filter(|_| section.version == "0110")?;
    Some((f64::from(w), f64::from(h)))
}

fn machine_file_size_mm(p: &Path) -> Option<(f64, f64)> {
    let ty = FileType::from_path(p)?;
    let bytes = openstitch_app::platform::read_small(p, 64 << 20)?;
    let pattern = ty.read(&bytes).ok()?;
    let b = pattern.bounds();
    Some((b.width() / 10.0, b.height() / 10.0))
}

/// Sample design for `--demo` (documentation screenshots), built with ordinary commands. Its
/// threads are New Brothread's (#796, the default chart), so the colour pane and the Sewing
/// Order swatches in the screenshots show their codes (#828).
pub fn demo_design() -> Vec<CommandRequest> {
    let c = |id: &'static str, p: Value| CommandRequest::new(id, p);
    vec![
        c(
            "design.new",
            json!({ "name": "Badge", "width": 100, "height": 100 }),
        ),
        c(
            "shape.ellipse",
            json!({ "cx": 50, "cy": 50, "rx": 40, "ry": 34, "fill": { "builtin": "newBrothread", "code": "405" }, "outline": { "builtin": "newBrothread", "code": "900" }, "name": "Badge" }),
        ),
        c(
            "object.setRegion",
            json!({ "id": 1, "angle": 30, "density": 3 }),
        ),
        c(
            "shape.polygon",
            json!({ "name": "Star", "fill": { "builtin": "newBrothread", "code": "206" }, "points": star(50.0, 47.0, 22.0, 9.5) }),
        ),
        c(
            "object.setHoleSewing",
            json!({ "ids": [1, 2], "enabled": true }),
        ),
        c(
            "object.setRegion",
            // Split at 7 mm: the 44 mm star sewed 31.8 mm satin stitches, which Check Design
            // reported as an error in every screenshot's status bar (#441).
            json!({ "id": 2, "type": "satin", "angle": 90, "splitLength": 7 }),
        ),
        c(
            "shape.rectangle",
            json!({ "x": 24, "y": 74, "width": 52, "height": 8, "fill": { "builtin": "newBrothread", "code": "800" }, "name": "Ribbon" }),
        ),
        c(
            "object.setLine",
            json!({ "id": 3, "type": "zigzag", "color": { "builtin": "newBrothread", "code": "001" }, "width": 1.2 }),
        ),
    ]
}

fn star(cx: f64, cy: f64, r: f64, inner: f64) -> Value {
    let pts: Vec<Value> = (0..10)
        .map(|i| {
            let a = std::f64::consts::PI * (i as f64 / 5.0) - std::f64::consts::FRAC_PI_2;
            let rr = if i % 2 == 0 { r } else { inner };
            json!([
                ((cx + rr * a.cos()) * 100.0).round() / 100.0,
                ((cy + rr * a.sin()) * 100.0).round() / 100.0
            ])
        })
        .collect();
    Value::Array(pts)
}

/// What an SVG import left out or approximated (#487), for the status line: "; left out: 2
/// text, 1 image; approximated: 1 gradient", or nothing.
pub(crate) fn import_report(result: &Value) -> String {
    let list = |key: &str| -> Vec<String> {
        result[key]
            .as_array()
            .into_iter()
            .flatten()
            .map(|k| {
                format!(
                    "{} {}",
                    k["count"].as_u64().unwrap_or(0),
                    k["kind"].as_str().unwrap_or("")
                )
            })
            .collect()
    };
    let mut out = String::new();
    for (key, label) in [("skipped", "left out"), ("approximated", "approximated")] {
        let l = list(key);
        if !l.is_empty() {
            out.push_str(&format!("; {label}: {}", l.join(", ")));
        }
    }
    out
}

/// Where pasted background images are kept: the per-user application data folder.
fn pasted_images_dir() -> Option<PathBuf> {
    let env = |k: &str| {
        std::env::var_os(k)
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
    };
    if cfg!(target_os = "macos") {
        env("HOME").map(|h| h.join("Library/Application Support/OpenStitch/backgrounds"))
    } else if cfg!(windows) {
        env("APPDATA").map(|a| a.join("OpenStitch").join("backgrounds"))
    } else {
        env("XDG_DATA_HOME")
            .or_else(|| env("HOME").map(|h| h.join(".local/share")))
            .map(|d| d.join("openstitch").join("backgrounds"))
    }
}

/// Encodes clipboard RGBA pixels as PNG in `dir`, named by a hash of the content, within the
/// image decoder's size limits. Returns the file's path.
pub fn store_pasted_image(
    dir: &Path,
    width: u32,
    height: u32,
    rgba: Vec<u8>,
) -> Result<PathBuf, String> {
    let raster =
        openstitch_digitize::Raster::from_rgba(width, height, rgba).map_err(|e| e.to_string())?;
    let png = raster.encode_png().map_err(|e| e.to_string())?;
    // FNV-1a: a stable name, so pasting the same image twice reuses the file.
    let hash = png.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, &b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("pasted-{hash:016x}.png"));
    std::fs::write(&path, png).map_err(|e| e.to_string())?;
    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use openstitch_design::Compiler;

    #[test]
    fn every_offered_pes_version_is_one_the_writer_knows() {
        for (v, _) in EditorApp::PES_VERSIONS {
            assert!(
                openstitch_formats::pes::WriteOptions::parse(v).is_some(),
                "{v}"
            );
        }
    }

    #[test]
    fn pasted_pixels_are_stored_as_png_and_become_the_background() {
        let dir = openstitch_testutil::TempDir::new("paste");
        let rgba: Vec<u8> = (0..4 * 3 * 2).map(|i| (i * 10) as u8).collect();
        let path = store_pasted_image(&dir, 3, 2, rgba.clone()).expect("store");
        assert_eq!(store_pasted_image(&dir, 3, 2, rgba).expect("again"), path);
        assert!(store_pasted_image(&dir, 3, 3, vec![0; 4]).is_err()); // size mismatch
        assert!(store_pasted_image(&dir, 100_000, 1, vec![0; 400_000]).is_err()); // too wide
        let mut app = EditorApp::new(Launch::default());
        app.execute(&CommandRequest::new(
            "background.set",
            json!({ "path": path.display().to_string() }),
        ))
        .expect("background");
        let b = app.session.design().background.clone().expect("set");
        assert_eq!((b.pixel_width, b.pixel_height), (3, 2));
        assert!(pasted_images_dir().is_some());
    }

    #[test]
    fn demo_design_runs_through_the_engine() {
        let app = EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        });
        assert_eq!(app.session.design().objects.len(), 3);
        assert!(app.stats.stitches > 1000, "{:?}", app.stats);
        assert_eq!(app.status, "Ready");
        assert!(!app.is_dirty());
        // The sample compiles once at the end: the stitches are those of the final design,
        // and each command is still its own Undo.
        let d = app.session.design();
        assert_eq!(app.pattern, Compiler::for_design(d).compile(d));
        assert_eq!(app.undo_groups, vec![1; demo_design().len()]);
        assert_eq!(app.refresh_deferred, None);
        // Every documentation screenshot shows it: Check Design finds no error in it (#441:
        // its unsplit satin star sewed 31.8 mm stitches).
        let mut app = app;
        let check = app
            .session
            .handle(&serde_json::json!({ "command": "design.check" }));
        assert_eq!(check["result"]["errors"], 0, "{check}");
    }

    #[test]
    fn opening_a_pe_design_file_keeps_its_page_and_the_file() {
        // #375: PE-DESIGN's 200 x 150 mm Design Page, and the file itself, so exporting it
        // unchanged writes it back with its objects.
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../openstitch-formats/tests/fixtures/pe11/group-two.pes");
        let mut app = EditorApp::new(Launch::default());
        app.open_path(&fixture);
        let d = app.session.design();
        assert_eq!(
            (d.page.width, d.page.height),
            (2000.0, 1500.0),
            "{}",
            app.status
        );
        let source = d.source.as_ref().expect("the file is kept");
        assert!(source.unchanged(d));
        assert_eq!(source.version.as_deref(), Some("0110"));
    }

    #[test]
    fn importing_pe_design_objects_adds_texts_and_shapes_in_one_undo_step() {
        // #397: Home ▸ Import ▸ Import PE-DESIGN Objects: a fitted text and its circle
        // (text-fit-default) become a text object and a shape; one undo removes both.
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../compat/reference/text/text-fit-default.pes");
        let mut app = EditorApp::new(Launch::default());
        let before = app.session.design().objects.len();
        assert!(app.import_pes_objects(&fixture), "{}", app.status);
        let d = app.session.design();
        let kinds: Vec<_> = d.objects[before..].iter().map(|o| o.kind).collect();
        assert!(
            kinds.contains(&openstitch_design::ObjectKind::Text),
            "{kinds:?}"
        );
        assert!(
            kinds.contains(&openstitch_design::ObjectKind::Shape),
            "{kinds:?}"
        );
        assert!(
            app.status.starts_with("Imported 2 objects"),
            "{}",
            app.status
        );
        app.undo();
        assert_eq!(app.session.design().objects.len(), before);
    }

    #[test]
    fn opening_a_machine_file_imports_it_into_a_new_design() {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../openstitch-engine/tests/fixtures/badge-golden.pes");
        let mut app = EditorApp::new(Launch::default());
        app.open_path(&fixture);
        let d = app.session.design();
        assert_eq!(d.objects.len(), 1, "{}", app.status);
        assert_eq!(d.objects[0].kind, openstitch_design::ObjectKind::Stitches);
        assert!(d.page.width >= 800.0, "page fits the 80 mm design");
        let b = d.objects[0].bounds().expect("bounds");
        assert!(
            (b.center().x - d.page.center().x).abs() < 1.0,
            "centred on the page"
        );
        assert!(
            app.path.is_none(),
            "a machine file is never overwritten by Save"
        );
        app.undo();
        assert!(
            app.session.design().objects.is_empty(),
            "new design + import + centre is one undo step"
        );
        assert!(!app.session.can_undo());
        app.redo();
        assert_eq!(app.session.design().objects.len(), 1, "and one redo step");
    }

    #[test]
    fn corner_scale_fallback_undoes_in_one_step() {
        use openstitch_app::editor::hit::rect_from;
        use openstitch_design::v2;
        let mut app = EditorApp::new(Launch::default());
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 10, "y": 10, "width": 20, "height": 10 }),
        ));
        app.execute(&CommandRequest::new(
            "object.setVisible",
            json!({ "id": 1, "visible": true }),
        )); // no-op: no history
        let ids = BTreeSet::from([1]);
        let reqs = CommandRequest::scale_about(
            &ids,
            2.0,
            2.0,
            v2(100.0, 100.0),
            rect_from(v2(100.0, 100.0), v2(300.0, 200.0)),
            false,
        );
        assert_eq!(reqs.len(), 2);
        app.apply_effects(vec![ToolEffect::Execute {
            requests: reqs,
            select_result: false,
        }]);
        assert_eq!(
            app.session.design().objects[0]
                .bounds()
                .map(|b| (b.min_x, b.max_x)),
            Some((100.0, 500.0))
        );
        app.undo();
        let b = app.session.design().objects[0].bounds().expect("bounds");
        assert_eq!(
            (b.min_x, b.max_x),
            (100.0, 300.0),
            "scale and move undone together"
        );
        app.undo();
        assert!(
            app.session.design().objects.is_empty(),
            "the no-op edit took no undo step"
        );
        app.redo();
        app.redo();
        assert_eq!(
            app.session.design().objects[0].bounds().map(|b| b.max_x),
            Some(500.0)
        );
    }

    #[test]
    fn edits_bump_revision_and_failed_commands_report() {
        let mut app = EditorApp::new(Launch::default());
        let r0 = app.revision;
        assert!(
            app.execute(&CommandRequest::new("design.info", json!({})))
                .is_some()
        );
        assert_eq!(app.revision, r0, "reads do not recompile");
        app.execute(&CommandRequest::new(
            "shape.rectangle",
            json!({ "x": 0, "y": 0, "width": 10, "height": 10 }),
        ));
        assert_eq!(app.revision, r0 + 1);
        assert!(app.is_dirty());
        assert!(
            app.execute(&CommandRequest::new("object.delete", json!({ "id": 99 })))
                .is_none()
        );
        assert!(app.status.contains("not found"), "{}", app.status);
        app.simulate = Some(5);
        app.ensure_render_model();
        assert_eq!(app.render.shown, 5);
    }

    #[test]
    fn hoop_change_resizes_page_updates_estimate_and_undoes() {
        let mut app = EditorApp::new(Launch {
            demo: true,
            show: vec!["measure".into(), "norulers".into()],
            ..Default::default()
        });
        assert!(app.workspace.measure.active && !app.workspace.rulers);
        let before = app.workspace.estimate.clone().expect("estimate");
        assert!(before.needle_total() > 1000.0 && before.seconds > 60.0);
        app.execute(&CommandRequest::new(
            "machine.set",
            json!({ "hoop": "130x180", "speed": 400 }),
        ));
        assert_eq!(app.session.design().page.width, 1300.0);
        let slow = app.workspace.estimate.clone().expect("estimate");
        assert!(slow.seconds > before.seconds * 1.5);
        app.undo();
        assert_eq!(app.session.design().page.width, 1000.0);
        assert_eq!(app.workspace.estimate, Some(before));
        app.toggle_measure();
        assert!(!app.workspace.measure.active);
    }

    fn ellipse() -> CommandRequest {
        CommandRequest::new("shape.ellipse", json!({ "cx": 20, "cy": 15, "rx": 3 }))
    }

    #[test]
    fn dirty_flag_follows_edits_new_open_and_save() {
        let mut app = EditorApp::new(Launch::default());
        assert!(!app.is_dirty(), "a new empty design is clean");
        app.execute(&ellipse()).expect("edit");
        assert!(app.is_dirty(), "an edit makes it dirty");
        app.new_design();
        assert!(!app.is_dirty(), "File ▸ New starts clean");
        app.execute(&ellipse()).expect("edit");
        app.undo();
        // Undo back to the saved state may stay dirty; it never makes a changed design clean.
        app.execute(&ellipse()).expect("edit");
        assert!(app.is_dirty());

        let dir = openstitch_testutil::TempDir::new("dirty");
        let file = dir.join("design.osd");
        app.path = Some(file.clone());
        app.save(false);
        assert!(!app.is_dirty(), "Save makes it clean: {}", app.status);
        assert!(file.exists());
        assert_eq!(app.document_name(), "design.osd");
        app.execute(&ellipse()).expect("edit");
        assert!(app.is_dirty());
        app.open_path(&file);
        assert!(!app.is_dirty(), "a freshly opened design is clean");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn cmd_q_quits_through_the_prompt() {
        let ctx = egui::Context::default();
        let press = |app: &mut EditorApp, key: Key| {
            let mut raw = egui::RawInput::default();
            raw.events
                .push(egui::Event::ModifiersChanged(Modifiers::COMMAND));
            raw.events.push(egui::Event::Key {
                key,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: Modifiers::COMMAND,
            });
            let mut consumed = false;
            let mut out = ctx.run_ui(raw, |ui| {
                app.macos_app_shortcuts(ui.ctx());
                consumed = ui.ctx().input(|i| {
                    !i.events
                        .iter()
                        .any(|e| matches!(e, egui::Event::Key { .. }))
                });
            });
            out.textures_delta.clear();
            consumed
        };
        let mut app = EditorApp::new(Launch::default());
        press(&mut app, Key::Q);
        assert!(app.exit_approved(), "clean: quits at once");

        let mut app = EditorApp::new(Launch::default());
        app.execute(&ellipse()).expect("edit");
        let consumed = press(&mut app, Key::Q);
        assert!(app.close_prompt.is_open(), "dirty: the prompt");
        assert!(!app.exit_approved());
        assert!(consumed, "the key is consumed, so nothing else sees it");

        press(&mut app, Key::M);
        assert!(!app.exit_approved(), "Cmd+M only minimises");

        // Cmd+W is the same request as Cmd+Q.
        let mut app = EditorApp::new(Launch::default());
        app.execute(&ellipse()).expect("edit");
        press(&mut app, Key::W);
        assert!(
            app.close_prompt.is_open() && !app.exit_approved(),
            "Cmd+W asks too"
        );
    }

    #[test]
    fn exit_new_and_open_ask_only_with_unsaved_changes() {
        use openstitch_app::editor::close_prompt::{Choice, Pending};
        let mut app = EditorApp::new(Launch::default());
        app.guarded(Pending::New);
        assert!(!app.close_prompt.is_open(), "clean: no prompt");
        app.guarded(Pending::Exit);
        assert!(app.exit_approved(), "clean: the window closes at once");

        let mut app = EditorApp::new(Launch::default());
        app.execute(&ellipse()).expect("edit");
        app.guarded(Pending::Exit);
        assert!(app.close_prompt.is_open());
        assert!(
            !app.exit_approved(),
            "dirty: the close waits for the answer"
        );
        app.answer_close_prompt(Choice::Cancel);
        assert!(!app.close_prompt.is_open());
        assert!(
            !app.exit_approved() && app.is_dirty(),
            "Cancel keeps everything"
        );

        app.guarded(Pending::New);
        assert!(app.close_prompt.is_open());
        assert_eq!(
            app.session.design().objects.len(),
            1,
            "nothing happened yet"
        );
        app.answer_close_prompt(Choice::DontSave);
        assert_eq!(
            app.session.design().objects.len(),
            0,
            "Don't Save: the new design"
        );
        assert!(!app.is_dirty() && !app.exit_approved());

        // Save with a file: saves, then goes on.
        let dir = openstitch_testutil::TempDir::new("prompt");
        let file = dir.join("design.osd");
        app.execute(&ellipse()).expect("edit");
        app.path = Some(file.clone());
        app.guarded(Pending::Exit);
        app.answer_close_prompt(Choice::Save);
        assert!(file.exists() && !app.is_dirty());
        assert!(app.exit_approved(), "Save then close");

        // Open a given file (Recent Files, Design Database) after Don't Save.
        let mut app = EditorApp::new(Launch::default());
        app.execute(&ellipse()).expect("edit");
        app.guarded(Pending::Open(Some(file.clone())));
        assert!(app.close_prompt.is_open());
        app.answer_close_prompt(Choice::DontSave);
        assert!(app.open_job.is_some(), "opens in the background (#558)");
        app.finish_job_blocking();
        assert_eq!(app.path.as_deref(), Some(file.as_path()));
        assert!(!app.is_dirty());
        let _ = std::fs::remove_dir_all(&dir);
    }
}

#[path = "documents.rs"]
pub(crate) mod documents;

#[cfg(test)]
#[path = "app_coverage_tests.rs"]
mod coverage_tests;

#[cfg(test)]
#[path = "editor_tools_tests.rs"]
mod editor_tools_tests;

#[cfg(test)]
#[path = "open_bench_tests.rs"]
mod open_bench_tests;

#[cfg(test)]
#[path = "a11y_tests.rs"]
mod a11y_tests;
#[cfg(test)]
#[path = "menu_i18n_tests.rs"]
mod menu_i18n_tests;
