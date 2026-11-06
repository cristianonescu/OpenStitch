//! The window chrome: Application button and menu, Quick Access Toolbar, ribbon tabs with
//! their groups (and the contextual tabs for the active tool or selection), the Option and
//! Help buttons. The layout is the model in [`openstitch_app::editor::ribbon`]; this module
//! draws it, decides which commands are enabled and runs them.

use crate::app::EditorApp;
use crate::{convert_ui, edit_ui, icons, panels};
use eframe::egui::{self, Color32, CornerRadius, RichText, Sense, Stroke, StrokeKind, vec2};
use openstitch_app::editor::clipboard::{self, Envelope};
use openstitch_app::editor::close_prompt::Pending;
use openstitch_app::editor::prefs::{EditingPrefs, Theme, UiPrefs};
use openstitch_app::editor::ribbon::{
    self, APPLICATION_MENU, Action, Facts, Info, Item, OPTION_MENU, TabId,
};
use openstitch_app::editor::{CommandRequest, Tool, objects};
use openstitch_design::ObjectKind;
use openstitch_formats::FileType;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::PathBuf;

pub const MAC: bool = cfg!(target_os = "macos");
/// Height of the button area of a ribbon group (the caption goes under it).
const BODY: f32 = 68.0;
const LARGE_ICON: f32 = 30.0;
const SMALL_ICON: f32 = 16.0;
/// Colour of contextual tabs.
const CONTEXT: Color32 = Color32::from_rgb(0xe0, 0x8a, 0x1e);
pub const APP_BLUE: Color32 = Color32::from_rgb(0x1f, 0x5f, 0xb4);

/// The right-hand pane's tabs.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PaneTab {
    Import,
    Color,
    #[default]
    Sewing,
    Text,
}

impl PaneTab {
    pub const ALL: [PaneTab; 4] = [
        PaneTab::Import,
        PaneTab::Color,
        PaneTab::Sewing,
        PaneTab::Text,
    ];

    pub fn label(self) -> &'static str {
        match self {
            PaneTab::Import => "Import",
            PaneTab::Color => "Color",
            PaneTab::Sewing => "Sewing Attributes",
            PaneTab::Text => "Text Attributes",
        }
    }

    /// `--pane` value.
    pub fn from_key(key: &str) -> Option<PaneTab> {
        Some(match key {
            "import" => PaneTab::Import,
            "color" => PaneTab::Color,
            "attributes" => PaneTab::Sewing,
            "text" => PaneTab::Text,
            _ => return None,
        })
    }
}

/// Chrome state: active tab, preferences, the internal clipboard and the small windows.
pub struct RibbonState {
    pub tab: TabId,
    pub prefs: UiPrefs,
    /// Where the preferences are saved (`None` = not saved, e.g. tests and screenshots).
    pub prefs_path: Option<PathBuf>,
    /// Opens the Application menu on the next frame (`--show appmenu`).
    pub open_app_menu: bool,
    /// Opens the Print dialog on the next frame (`--show print`).
    pub open_print: bool,
    last_tool: Tool,
    /// Objects Copy or Cut put on the clipboard (#500), kept here too: Paste falls back to
    /// them when the system clipboard holds no OpenStitch objects.
    pub copied: Option<Envelope>,
    /// Copy, Cut and Paste use the system clipboard (off in tests, screenshots and `ui.run`).
    pub system_clipboard: bool,
    /// The system clipboard held OpenStitch objects at the last check (another window's Copy).
    pub clipboard_objects: bool,
    /// When the system clipboard was last checked (seconds).
    pub clipboard_checked: f64,
    /// Pan: the primary button drags the view.
    pub pan: bool,
    pub property_open: bool,
    /// Design Property: the fields being edited (`None` = showing the design's own).
    pub property_draft: Option<openstitch_design::DesignProperties>,
    pub shortcuts_open: bool,
    /// Help ▸ Keyboard Shortcuts editing (#551).
    pub keys: crate::keymap_ui::KeysWindow,
    /// Getting started (#554): Open Sample and the Getting Started page.
    pub start: crate::start_ui::StartState,
    pub about_open: bool,
    pub pane_tab: PaneTab,
    /// Edit Text: the characters being typed.
    pub text_draft: String,
    /// Undo groups before the current font browse (arrow keys in the font list, #527): the
    /// browse is undone in one step.
    pub font_browse_undo: Option<usize>,
    /// Text Attributes ▸ Free envelope ▸ Edit handles (#535).
    pub envelope_edit: bool,
    /// The free-envelope handle being dragged on the canvas.
    pub envelope_drag: Option<crate::envelope_ui::EnvelopeDrag>,
    /// An arched text's centre handle being dragged (#456): the text and where the pointer is.
    pub arc_drag: Option<(u64, openstitch_design::Vec2)>,
    /// Text ▸ Ink/Stitch Fonts (#529).
    pub inkstitch: crate::inkstitch_ui::InkStitchFonts,
    /// ScreenTip of the command under the pointer (status bar).
    pub hover: Option<&'static str>,
    /// Sewing Attributes property search (#550).
    pub property_search: String,
    applied_theme: Option<(Theme, Option<[u8; 3]>)>,
}

impl Default for RibbonState {
    fn default() -> Self {
        RibbonState {
            tab: TabId::Home,
            prefs: UiPrefs::default(),
            prefs_path: None,
            open_app_menu: false,
            open_print: false,
            last_tool: Tool::Select,
            copied: None,
            system_clipboard: false,
            clipboard_objects: false,
            clipboard_checked: f64::NEG_INFINITY,
            pan: false,
            property_open: false,
            property_draft: None,
            shortcuts_open: false,
            keys: Default::default(),
            start: Default::default(),
            about_open: false,
            pane_tab: PaneTab::default(),
            text_draft: String::new(),
            font_browse_undo: None,
            envelope_edit: false,
            envelope_drag: None,
            arc_drag: None,
            inkstitch: Default::default(),
            hover: None,
            property_search: String::new(),
            applied_theme: None,
        }
    }
}

impl RibbonState {
    pub fn save_prefs(&self) {
        if let Some(p) = &self.prefs_path
            && let Err(e) = self.prefs.save(p)
        {
            eprintln!("cannot save {}: {e}", p.display());
        }
    }

    pub fn add_recent(&mut self, path: &std::path::Path) {
        self.prefs.add_recent(path);
        self.save_prefs();
    }

    /// Applies the colour theme when it changed.
    pub fn apply_theme(&mut self, ctx: &egui::Context) {
        let theme = self.prefs.theme;
        let accent = self.prefs.accent;
        if self.applied_theme == Some((theme, accent)) {
            return;
        }
        crate::theme::set_accent(ctx, accent);
        ctx.set_theme(match theme {
            Theme::Light => egui::ThemePreference::Light,
            Theme::Dark => egui::ThemePreference::Dark,
            Theme::System => egui::ThemePreference::System,
        });
        self.applied_theme = Some((theme, accent));
    }
}

// MARK: selection facts

pub fn facts(app: &EditorApp) -> Facts {
    let d = app.session.design();
    let has = |k: ObjectKind| {
        app.selection
            .iter()
            .any(|&id| d.index_of(id).is_some_and(|i| d.objects[i].kind == k))
    };
    Facts {
        tool: app.tools.tool(),
        text: has(ObjectKind::Text),
        shape: has(ObjectKind::Shape),
        stitches: has(ObjectKind::Stitches),
        sections: crate::sections_ui::has_sections(app),
    }
}

/// Text and shape of a two-object selection, for Fit Text to outline.
pub fn fit_pair(app: &EditorApp) -> Option<(u64, u64)> {
    if app.selection.len() != 2 {
        return None;
    }
    let d = app.session.design();
    let kind = |id: u64| d.index_of(id).map(|i| d.objects[i].kind);
    let mut it = app.selection.iter().copied();
    let (a, b) = (it.next()?, it.next()?);
    let (text, shape) = match (kind(a), kind(b)) {
        (Some(ObjectKind::Text), Some(ObjectKind::Shape)) => (a, b),
        (Some(ObjectKind::Shape), Some(ObjectKind::Text)) => (b, a),
        _ => return None,
    };
    // One text per outline: PE-DESIGN greys Fit Text when another text is fitted to it.
    let taken = d
        .objects
        .iter()
        .any(|o| o.id != text && o.text.as_ref().is_some_and(|t| t.path == Some(shape)));
    (!taken).then_some((text, shape))
}

/// Selected texts that are fitted to an outline.
pub fn fitted_texts(app: &EditorApp) -> Vec<u64> {
    let d = app.session.design();
    app.selection
        .iter()
        .copied()
        .filter(|&id| {
            d.index_of(id)
                .and_then(|i| d.objects[i].text.as_ref())
                .is_some_and(|t| t.path.is_some())
        })
        .collect()
}

/// Checks, at most once a second, whether the system clipboard holds OpenStitch objects, so
/// Paste is enabled after a Copy in another window.
/// The OS clipboard: the one way the editor reaches it (#705). Tests never do: on macOS it is
/// AppKit's pasteboard, which crashed the test binary when tests on several threads used it at
/// once, and one test's Copy reached another's Paste. A test that gets here fails at once;
/// `Launch` keeps `system_clipboard` off under `cfg(test)`.
pub(crate) fn os_clipboard() -> Result<arboard::Clipboard, arboard::Error> {
    #[cfg(test)]
    panic!("a test reached the OS clipboard (#705): keep ui.system_clipboard off in tests");
    #[cfg(not(test))]
    arboard::Clipboard::new()
}

pub fn poll_clipboard(app: &mut EditorApp, now: f64) {
    if !app.ui.system_clipboard || now - app.ui.clipboard_checked < 1.0 {
        return;
    }
    app.ui.clipboard_checked = now;
    app.ui.clipboard_objects = os_clipboard()
        .and_then(|mut c| c.get_text())
        .is_ok_and(|t| clipboard::looks_like(&t));
}

/// Writes `env` to the system clipboard (when used) and keeps it.
fn put_on_clipboard(app: &mut EditorApp, env: Envelope) {
    if app.ui.system_clipboard {
        let text = env.to_text();
        if let Err(e) = os_clipboard().and_then(|mut c| c.set_text(text)) {
            app.status =
                format!("Could not use the system clipboard ({e}); Paste works in this window");
        }
        app.ui.clipboard_objects = true;
    }
    app.ui.copied = Some(env);
}

/// Whether `a` can run now (implemented, and the selection suits it).
pub fn enabled(app: &EditorApp, a: Action) -> bool {
    use Action::*;
    if !a.info().available {
        return false;
    }
    let any = !app.selection.is_empty();
    let many = app.selection.len() >= 2;
    let (outline, region, stitches) = convert_ui::kinds(app);
    let texts = !convert_ui::texts(app).is_empty();
    let drawn = app.stats.stitches > 0;
    let background = app.session.design().background.is_some();
    match a {
        Undo => app.session.can_undo(),
        Redo => app.session.can_redo(),
        Cut | Copy | Duplicate | Delete | ArrangeCopy | Arrange | RotateFlip | Size
        | DecorativeFrame | Align | SelectSame | SelectSameColor | SelectSameType
        | SelectSameBoth => any,
        AlignLeft | AlignCenter | AlignRight | AlignTop | AlignMiddle | AlignBottom
        | DistributeHorizontal | DistributeVertical => crate::arrange_ui::can_align(app, a),
        Paste => app.ui.copied.is_some() || app.ui.clipboard_objects,
        CopyAttributes => edit_ui::copy_attributes_source(app).is_some(),
        PasteAttributes => edit_ui::paste_attributes_source(app).is_some(),
        Group => many,
        Ungroup => edit_ui::can_ungroup(app),
        StampEmboss => edit_ui::single_shape(app).is_some() || app.selection.len() == 2,
        ModifyOverlap => many,
        FitTextToOutline => fit_pair(app).is_some() || !fitted_texts(app).is_empty(),
        Export | Print | SendEmbroidery => drawn,
        ExportSections => crate::sections_ui::has_sections(app),
        ImageDensity | DeleteImage => background,
        SaveImage | ModifyImage | ImageRotateRight | ImageRotateLeft => background,
        Font | TextSize | EditText | EditCharacter => texts,
        DeriveSkeleton => edit_ui::single_region(app).is_some_and(|id| {
            let d = app.session.design();
            d.index_of(id).is_some_and(|i| {
                let o = &d.objects[i];
                o.kind == openstitch_design::ObjectKind::Shape && !o.hole_sewing
            })
        }),
        LineColor | LineSewType | RegionColor | RegionSewType => outline,
        ConvertToBlocks | ConvertToOutlines => stitches,
        ConvertToOutline => stitches || texts,
        ConvertToStitches => outline,
        ConvertRegionToLine => region,
        SplitOutline | EntryExit => edit_ui::single_shape(app).is_some(),
        SmoothShape => crate::shape_tools_ui::can_smooth_shape(app),
        SmoothStitches => convert_ui::stitch_selection(app).1.is_some(),
        ShapeEffects => crate::shape_effects_ui::can_apply(app),
        LineGap => crate::line_gap_ui::target(app).is_some(),
        AddDirectionLine | DeleteDirectionLine | ReverseDirectionLine | SewingPoints => {
            edit_ui::single_region(app).is_some()
        }
        OffsetLines | FloralPattern => edit_ui::single_outline(app).is_some(),
        DeletePoint => app.tools.tool() == Tool::Point && any,
        SplitAtPoint => {
            let (_, one, needles) = convert_ui::stitch_selection(app);
            one.is_some() && needles.len() == 1
        }
        SplitStitchData => convert_ui::stitch_selection(app).1.is_some(),
        DivideByColor => stitches,
        ChangeColor => convert_ui::stitch_selection(app).1.is_some(),
        MoveSections => openstitch_app::editor::sections::can_move(app.session.design()),
        Connect => convert_ui::stitch_selection(app).0.len() == 2,
        PaneAssistant => app.ui.prefs.ai.enabled,
        _ => true,
    }
}

/// Whether an on/off command or a tool is on.
pub fn active(app: &EditorApp, a: Action) -> bool {
    use Action::*;
    let panes = &app.ui.prefs.panes;
    if let Some(t) = a.tool() {
        return app.tools.tool() == t && !app.workspace.measure.active && !app.ui.pan;
    }
    match a {
        StitchView => !app.workspace.realistic && !app.workspace.solid,
        RealisticView => app.workspace.realistic,
        SolidView => app.workspace.solid && !app.workspace.realistic,
        GridAxes => app.workspace.grid.axes,
        Guideline => app.workspace.guides,
        FontFilter => app.ui.prefs.font_filter.on,
        ViewThreadTrimming => app.show_travel,
        PaneSewingOrder => panes.sewing_order.visible,
        PaneSimulator => panes.simulator.visible,
        PaneAttributes => panes.attributes.visible,
        PaneImport => panes.attributes.visible && app.ui.pane_tab == PaneTab::Import,
        PaneAssistant => app.assistant.panel.open,
        ReferenceWindow => app.workspace.reference,
        ShowGrid => app.workspace.grid.visible,
        SnapToGrid => app.workspace.grid.snap,
        SnapToObjects => app.ui.prefs.editing.snap_objects,
        PrecisionBar => app.ui.prefs.editing.precision_bar,
        Ruler => app.workspace.rulers,
        Pan => app.ui.pan,
        Measure => app.workspace.measure.active,
        MoveSections => app.workspace.sections.active,
        _ => false,
    }
}

// MARK: running commands

fn set_tool(app: &mut EditorApp, t: Tool) {
    app.tools.set_tool(t);
    app.workspace.measure.active = false;
    app.ui.pan = false;
}

/// Zooms so the view shows `percent` % (100 % = real size: on this monitor once calibrated,
/// else on a 96 dpi screen).
pub fn zoom_to_percent(app: &mut EditorApp, percent: f64) {
    let ppm = app.points_per_mm();
    if let Some(vp) = &mut app.viewport {
        let now = vp.zoom_percent(ppm);
        if now > 0.0 && percent > 0.0 {
            let anchor = app.workspace.canvas.map_or_else(
                || vp.to_screen(app.session.design().page.center()),
                |r| openstitch_design::v2(f64::from(r.center().x), f64::from(r.center().y)),
            );
            vp.zoom_about(anchor, percent / now);
        }
    }
}

pub(crate) fn zoom_to_selection(app: &mut EditorApp) {
    let d = app.session.design();
    if let (Some(b), Some(r)) = (
        openstitch_app::editor::hit::bounds_of(d, &app.selection),
        app.workspace.canvas,
    ) {
        app.viewport = Some(openstitch_app::editor::Viewport::fit(
            b,
            crate::workspace_ui::screen_rect(r),
            48.0,
        ));
    }
}

/// Cut: the selection leaves the design (one Undo) and waits on the clipboard.
fn cut(app: &mut EditorApp) {
    if app.refuse_locked() {
        return;
    }
    let Some(env) = Envelope::copy(app.session.design(), &app.selection) else {
        return;
    };
    let n = env.objects.len();
    let request = CommandRequest::delete(&app.selection);
    if app.execute(&request).is_some() {
        app.selection.clear();
        app.status = format!("{n} objects cut");
        put_on_clipboard(app, env);
    }
}

/// Copy: the selection goes to the clipboard as object data (#500), so it can be pasted in
/// another window or after the originals are deleted.
fn copy(app: &mut EditorApp) {
    if let Some(env) = Envelope::copy(app.session.design(), &app.selection) {
        app.status = format!("{} objects copied", env.objects.len());
        put_on_clipboard(app, env);
    }
}

/// Paste: the system clipboard's OpenStitch objects, else the ones copied in this window,
/// added in one Undo step.
fn paste(app: &mut EditorApp) {
    let system = app
        .ui
        .system_clipboard
        .then(|| os_clipboard().and_then(|mut c| c.get_text()).ok())
        .flatten()
        .and_then(|t| Envelope::parse(&t));
    let Some(env) = system.or_else(|| app.ui.copied.clone()) else {
        return;
    };
    let r = env.paste_request(app.session.design());
    if let Some(result) = app.execute(&r) {
        app.selection = CommandRequest::created_ids(&result);
        app.status = "Pasted".into();
    }
}

fn delete(app: &mut EditorApp) {
    let mut point_deleted = false;
    app.with_tools(|t, c| {
        let effects = t.delete_point(c);
        point_deleted = effects.is_some();
        effects.unwrap_or_default()
    });
    if !point_deleted && !app.refuse_locked() {
        app.on_selection(CommandRequest::delete);
    }
}

fn open_database(app: &mut EditorApp) {
    let folder = app
        .path
        .as_deref()
        .and_then(std::path::Path::parent)
        .map(std::path::Path::to_path_buf);
    app.database.show(folder.as_deref());
}

fn toggle_pane(app: &mut EditorApp, f: impl FnOnce(&mut openstitch_app::editor::prefs::Panes)) {
    f(&mut app.ui.prefs.panes);
    app.ui.save_prefs();
}

fn image_wizard(app: &mut EditorApp, m: openstitch_app::editor::wizard::Method) {
    app.wizard.set_method(m);
    app.wizard.open = true;
}

/// Runs a command button (menu commands open their list instead; see [`menu`]).
pub fn run(app: &mut EditorApp, a: Action, _ctx: &egui::Context) {
    use Action::*;
    use openstitch_app::editor::pattern_wizards::WizardKind;
    use openstitch_app::editor::wizard::Method;
    if !enabled(app, a) {
        return;
    }
    if let Some(t) = a.tool() {
        set_tool(app, t);
        return;
    }
    match a {
        New => app.guarded(Pending::New),
        Open => app.guarded(Pending::Open(None)),
        OpenSample => app.ui.start.samples_open = true,
        Save => app.save(false),
        SaveAs => app.save(true),
        DesignProperty => app.ui.property_open = true,
        DesignSettings => app.workspace.settings_open = true,
        SelectColorPalette | UserThreadCharts => app.settings.charts_open = true,
        Exit => app.guarded(Pending::Exit),
        Undo => app.undo(),
        Redo => app.redo(),
        DesignDatabase => open_database(app),
        StitchCreator => app.creator.open = true,
        StitchDesignFactory => app.sewing.open = true,
        FontCreator => app.lettering.fonts.open = true,
        AttributeSets => app.settings.sets_open = true,
        SelectAll => app.select_all(),
        Cut => cut(app),
        Copy => copy(app),
        Paste => paste(app),
        Duplicate => app.on_selection(CommandRequest::duplicate),
        Delete => delete(app),
        ArrangeCopy => {
            app.edit.arrange_open = true;
            app.edit.arrange.center = None;
        }
        CopyAttributes => edit_ui::copy_attributes(app),
        PasteAttributes => edit_ui::paste_attributes(app),
        ZoomIn => panels::zoom_center(app, 1.25),
        ZoomOut => panels::zoom_center(app, 1.0 / 1.25),
        Pan => {
            app.ui.pan = !app.ui.pan;
            app.workspace.measure.active = false;
        }
        Measure => {
            app.ui.pan = false;
            app.toggle_measure();
        }
        ImportPatterns => app.import_dialog(),
        ImportSvg => app.import_svg_dialog(),
        ImportPesObjects => app.import_pes_objects_dialog(),
        Group => edit_ui::group(app),
        Ungroup => edit_ui::ungroup(app),
        BackgroundFill => app.pattern_wizards.open = Some(WizardKind::Background),
        Applique => app.pattern_wizards.open = Some(WizardKind::Applique),
        Patch => app.pattern_wizards.open = Some(WizardKind::Patch),
        Cutwork => app.pattern_wizards.open = Some(WizardKind::Cutwork),
        OpenImage => app.background_dialog(),
        PasteImage => app.paste_background(),
        DeleteImage => {
            app.execute(&CommandRequest::new("background.clear", json!({})));
        }
        AutoPunch => image_wizard(app, Method::AutoPunch),
        CrossStitch => image_wizard(app, Method::CrossStitch),
        PhotoStitch1 => image_wizard(app, Method::PhotoFill),
        PhotoStitch2 => image_wizard(app, Method::PhotoLines),
        DesignCenter => image_wizard(app, Method::DesignCenter),
        StitchView => (app.workspace.realistic, app.workspace.solid) = (false, false),
        RealisticView => (app.workspace.realistic, app.workspace.solid) = (true, false),
        SolidView => (app.workspace.realistic, app.workspace.solid) = (false, true),
        RealisticSettings => app.workspace.view.realistic_open = true,
        GridAxes => app.workspace.grid.axes ^= true,
        Guideline => {
            app.workspace.guides ^= true;
            // Guidelines come out of the rulers.
            app.workspace.rulers |= app.workspace.guides;
        }
        ModifyImage => app.workspace.view.image_open = true,
        ImageRotateRight => crate::view_ui::rotate_image(app, "right"),
        ImageRotateLeft => crate::view_ui::rotate_image(app, "left"),
        FontFilter => {
            app.ui.prefs.font_filter.on ^= true;
            app.ui.save_prefs();
        }
        FontFilterSettings => app.workspace.view.fonts_open = true,
        FabricSelector => app.workspace.view.fabric_open = true,
        ViewThreadTrimming => app.show_travel = !app.show_travel,
        PaneSewingOrder => toggle_pane(app, |p| p.sewing_order.visible ^= true),
        PaneSimulator => toggle_pane(app, |p| p.simulator.visible ^= true),
        PaneAttributes => toggle_pane(app, |p| p.attributes.visible ^= true),
        PaneImport => {
            app.ui.pane_tab = PaneTab::Import;
            toggle_pane(app, |p| p.attributes.visible = true);
        }
        PaneAssistant => {
            let open = !app.assistant.panel.open;
            crate::assistant_panel::set_open(app, open);
        }
        ReferenceWindow => app.workspace.reference ^= true,
        ShowGrid => app.workspace.grid.visible ^= true,
        SnapToGrid => app.toggle_snap_to_grid(),
        SnapToObjects => {
            app.ui.prefs.editing.snap_objects ^= true;
            app.ui.save_prefs();
        }
        PrecisionBar => {
            app.ui.prefs.editing.precision_bar ^= true;
            app.ui.save_prefs();
        }
        AlignLeft | AlignCenter | AlignRight | AlignTop | AlignMiddle | AlignBottom
        | DistributeHorizontal | DistributeVertical => crate::arrange_ui::align(app, a),
        SelectSameColor | SelectSameType | SelectSameBoth => crate::arrange_ui::select_same(app, a),
        Ruler => app.workspace.rulers ^= true,
        TrueTypeAttributes => crate::lettering_ui::load_truetype(app),
        ConvertToBlocks => convert_ui::to_shapes(app, "blocks"),
        ConvertToOutlines => convert_ui::to_shapes(app, "parts"),
        ConvertToOutline => {
            if convert_ui::kinds(app).2 {
                convert_ui::to_shapes(app, "whole");
            } else {
                convert_ui::text_to_shapes(app);
            }
        }
        ConvertToStitches => convert_ui::to_stitches(app),
        ConvertRegionToLine => convert_ui::region_to_line(app),
        SplitOutline => edit_ui::split_outline(app),
        SmoothShape => crate::shape_tools_ui::smooth_shape(app),
        SmoothStitches => crate::shape_tools_ui::smooth_stitches(app),
        AddDirectionLine => edit_ui::add_direction(app),
        DeleteDirectionLine => edit_ui::delete_direction(app),
        DeletePoint => delete(app),
        SplitAtPoint => convert_ui::split_at_point(app),
        OffsetLines => app.edit.offset_open = true,
        FloralPattern => app.edit.floral_open = true,
        SplitStitchData => convert_ui::split_off_area(app),
        DivideByColor => convert_ui::divide_by_color(app),
        Connect => convert_ui::connect(app),
        ReverseDirectionLine => {
            if let Some(id) = edit_ui::single_region(app) {
                app.execute(&CommandRequest::new(
                    "object.reverseDirection",
                    json!({ "ids": [id] }),
                ));
            }
        }
        MoveSections => {
            app.workspace.sections.toggle();
            app.workspace.measure.active = false;
            app.ui.pan = false;
        }
        ExportSections => crate::sections_ui::export_sections(app),
        DeriveSkeleton => {
            if let Some(id) = edit_ui::single_region(app) {
                app.execute(&CommandRequest::new(
                    "object.deriveSkeleton",
                    json!({ "id": id }),
                ));
            }
        }
        // Tools are handled above; menus open their list; the rest are not available yet.
        SelectTool | PointTool | Rectangle | Ellipse | ClosedStraight | ClosedCurve | OpenCurve
        | StraightBlock | EditPoint | PencilClosed | OpenStraight | PencilOpen | CurvedBlock
        | Running | Feed | SatinSkeleton | Knife | ShapeBuilder => {}
        Print | Wizard | Export | Options | Help | TextTools | ShapesTools | OtherZoom
        | Arrange | Align | SelectSame | RotateFlip | Size | StampEmboss | ModifyOverlap
        | DecorativeFrame | FitTextToOutline | SendEmbroidery | SaveImage | ImageDensity
        | GridSpacing | ViewShow | Font | TextSize | EditText | LineColor | LineSewType
        | RegionColor | RegionSewType | EntryExit | SewingPoints | EdgeRadius | ChangeColor
        | SkeletonWidth | SmoothTolerance | ShapeEffects | LineGap | EditCharacter => {}
    }
}

/// A command inside a dropdown list: icon, label, shortcut and ScreenTip.
pub fn menu_item(app: &mut EditorApp, ui: &mut egui::Ui, a: Action) {
    let info = a.info();
    let mut b = egui::Button::new(info.label);
    if let Some(s) = info.shortcut_for(MAC) {
        b = b.shortcut_text(s);
    }
    let on = active(app, a);
    let r = ui
        .add_enabled(enabled(app, a), b.selected(on))
        .on_hover_text(info.screen_tip(MAC))
        .on_disabled_hover_text(info.screen_tip(MAC));
    if r.clicked() {
        run(app, a, ui.ctx());
        ui.close();
    }
}

fn export_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    for ty in FileType::writable() {
        if ty == FileType::Pes {
            ui.menu_button("PES", |ui| {
                for (v, label) in EditorApp::PES_VERSIONS {
                    if ui.button(format!("{label}…")).clicked() {
                        app.export_with(ty, Some(v));
                    }
                }
            });
            continue;
        }
        if ty == FileType::Exp {
            // Bernina machines read the colours from an INF beside the EXP (#482).
            ui.menu_button("EXP", |ui| {
                if ui.button("EXP…").clicked() {
                    app.export(ty);
                }
                if ui.button("EXP with .inf colour file…").clicked() {
                    app.export_exp_with_inf();
                }
            });
            continue;
        }
        if ui
            .button(format!("{}…", ty.extension().to_uppercase()))
            .clicked()
        {
            app.export(ty);
        }
    }
    ui.separator();
    // Not machine files (#488): every stitch record as a table, and G-code for DIY machines.
    for (ext, label) in [
        ("csv", "Stitch List (CSV)…"),
        ("txt", "Stitch List (TXT)…"),
        ("gcode", "G-code…"),
    ] {
        if ui.button(label).clicked() {
            app.export_extra(ext);
        }
    }
    // A picture, a stitch-out video (animated PNG) and a ZIP of PES, DST, JEF, the thread list
    // and the picture (#488).
    for (command, ext, label) in [
        ("design.exportPicture", "png", "Picture (PNG)…"),
        (
            "design.exportVideo",
            "png",
            "Stitch-out Video (animated PNG)…",
        ),
        (
            "design.exportBundle",
            "zip",
            "Bundle (ZIP: PES, DST, JEF, threads, picture)…",
        ),
    ] {
        if ui.button(label).clicked() {
            app.export_media(command, ext);
        }
    }
    ui.menu_button("Thread List", |ui| {
        for (ext, label) in EditorApp::THREAD_LISTS {
            if ui.button(format!("{label}…")).clicked() {
                app.export_thread_list(ext);
            }
        }
        ui.separator();
        if ui
            .button("Apply Thread List to the Design…")
            .on_hover_text("Give the colour blocks, in sewing order, the colours of a CSV, INF, EDR, COL or TXT list (one undo step)")
            .clicked()
        {
            app.apply_thread_list();
        }
    });
    menu_item(app, ui, Action::ExportSections);
}

/// The list a menu command opens.
pub fn menu(app: &mut EditorApp, a: Action, ui: &mut egui::Ui) {
    use Action::*;
    ui.set_min_width(180.0);
    match a {
        Print => {
            if ui.button("Print…").clicked() {
                crate::print_ui::open(ui.ctx());
            }
        }
        Wizard => {
            if ui.button("Template Wizard…").clicked() {
                crate::lettering_ui::open_template_wizard(app);
            }
            if ui.button("Image to Embroidery…").clicked() {
                app.wizard.open = true;
            }
            ui.separator();
            for w in [BackgroundFill, Applique, Patch, Cutwork] {
                menu_item(app, ui, w);
            }
        }
        Export => export_menu(app, ui),
        SendEmbroidery => {
            if ui.button("Send to USB Media…").clicked() {
                crate::transfer_ui::open(app);
            }
            if ui.button("Test Swatches…").clicked() {
                crate::swatch_ui::open(ui.ctx());
            }
            menu_item(app, ui, ExportSections);
            ui.menu_button("Export", |ui| export_menu(app, ui));
        }
        Options => options_menu(app, ui),
        Help => help_menu(app, ui),
        TextTools => crate::lettering_ui::text_tools_menu(app, ui),
        ShapesTools => {
            for t in [
                Rectangle,
                Ellipse,
                ClosedStraight,
                ClosedCurve,
                StraightBlock,
            ] {
                menu_item(app, ui, t);
            }
            ui.separator();
            ui.menu_button("Insert Preset Shape", |ui| edit_ui::insert_menu(app, ui));
        }
        OtherZoom => {
            if ui
                .add(
                    egui::Button::new("Fit Design Page")
                        .shortcut_text(format!("{}0", crate::app::CMD)),
                )
                .clicked()
            {
                app.fit_requested = true;
            }
            if ui
                .add_enabled(
                    !app.selection.is_empty(),
                    egui::Button::new("Zoom to Selection"),
                )
                .clicked()
            {
                zoom_to_selection(app);
            }
            if ui
                .add(egui::Button::new("Zoom Box").shortcut_text("Z"))
                .on_hover_text("Drag a rectangle on the canvas to zoom to it; Esc cancels")
                .clicked()
            {
                app.zoom_box.toggle();
            }
            ui.separator();
            for p in [50.0, 100.0, 200.0, 400.0] {
                if ui.button(format!("{p:.0} %")).clicked() {
                    zoom_to_percent(app, p);
                }
            }
            ui.separator();
            if ui
                .button("Calibrate Real Size…")
                .on_hover_text("Make 100 % real size on this screen with a ruler")
                .clicked()
            {
                app.calibration.open = true;
            }
        }
        FitTextToOutline => {
            let pair = fit_pair(app);
            if ui
                .add_enabled(pair.is_some(), egui::Button::new("Fit Text to Outline…"))
                .on_hover_text("Select one text and one shape")
                .clicked()
                && let Some((text, shape)) = pair
            {
                crate::lettering_ui::open_fit(app, text, shape);
            }
            let fitted = fitted_texts(app);
            if ui
                .add_enabled(
                    !fitted.is_empty(),
                    egui::Button::new("Release Text from Outline"),
                )
                .clicked()
            {
                app.grouped(|a| {
                    for id in fitted {
                        a.execute(&CommandRequest::new(
                            "text.setPath",
                            json!({ "id": id, "path": null }),
                        ));
                    }
                });
            }
        }
        Align => crate::arrange_ui::align_menu(app, ui),
        SelectSame => crate::arrange_ui::select_same_menu(app, ui),
        Arrange => {
            if ui.button("Center on Design Page").clicked() {
                app.on_selection(|s| {
                    CommandRequest::new("object.center", json!({ "ids": CommandRequest::ids(s) }))
                });
            }
            ui.separator();
            for (label, to) in [
                ("Sew First", "first"),
                ("Sew Earlier", "earlier"),
                ("Sew Later", "later"),
                ("Sew Last", "last"),
            ] {
                if ui.button(label).clicked() {
                    let r = objects::move_to(&app.selection, to);
                    app.execute(&r);
                }
            }
        }
        RotateFlip => {
            for (label, deg) in [
                ("Rotate Right 90°", 90.0),
                ("Rotate Left 90°", -90.0),
                ("Rotate 180°", 180.0),
            ] {
                if ui.button(label).clicked() {
                    app.on_selection(|s| CommandRequest::rotate(s, deg));
                }
            }
            ui.separator();
            if ui.button("Flip Horizontal").clicked() {
                app.on_selection(|s| CommandRequest::flip(s, true));
            }
            if ui.button("Flip Vertical").clicked() {
                app.on_selection(|s| CommandRequest::flip(s, false));
            }
        }
        Size => {
            ui.horizontal(|ui| {
                ui.add(
                    egui::DragValue::new(&mut app.scale_percent)
                        .range(1.0..=1000.0)
                        .suffix(" %"),
                );
                if ui.button("Apply").clicked() {
                    scale(app, app.scale_percent / 100.0);
                    app.scale_percent = 100.0;
                }
            });
            ui.separator();
            for p in [50.0, 90.0, 110.0, 200.0] {
                if ui.button(format!("{p:.0} %")).clicked() {
                    scale(app, p / 100.0);
                }
            }
        }
        StampEmboss => edit_ui::stamp_menu(app, ui),
        ModifyOverlap => edit_ui::overlap_menu(app, ui),
        DecorativeFrame => edit_ui::decorative_menu(app, ui),
        ImageDensity => image_density_menu(app, ui),
        SaveImage => crate::view_ui::save_image_menu(app, ui),
        ViewShow => crate::view_ui::show_menu(app, ui),
        GridSpacing => {
            use openstitch_app::editor::units;
            for &s in units::grid_spacings(app.ui.prefs.units) {
                ui.radio_value(&mut app.workspace.grid.spacing_mm, s, units::grid_label(s));
            }
        }
        Font => {
            let fonts = crate::lettering_ui::fonts(app);
            egui::ScrollArea::vertical()
                .max_height(320.0)
                .show(ui, |ui| {
                    for f in fonts {
                        if ui.button(&f.label).clicked() {
                            set_text(app, json!({ "font": f.id }));
                        }
                    }
                });
        }
        TextSize => {
            let u = app.ui.prefs.units;
            for mm in [5.0, 8.0, 10.0, 15.0, 20.0, 30.0, 50.0] {
                let label = openstitch_app::editor::units::length(u, mm);
                if ui.button(label).clicked() {
                    set_text(app, json!({ "size": mm }));
                }
            }
        }
        EditText => {
            if app.ui.text_draft.is_empty()
                && let Some(t) = first_text(app)
            {
                app.ui.text_draft = t;
            }
            ui.text_edit_multiline(&mut app.ui.text_draft);
            if ui.button("Apply").clicked() {
                let text = app.ui.text_draft.clone();
                set_text(app, json!({ "text": text }));
                app.ui.text_draft.clear();
                ui.close();
            }
        }
        LineColor | RegionColor => {
            let region = a == RegionColor;
            let m = openstitch_app::editor::inspector::InspectorModel::new(
                app.session.design(),
                &app.selection,
            );
            let current = if region {
                m.region.as_ref().map(|r| r.thread.clone())
            } else {
                m.line.as_ref().map(|l| l.thread.clone())
            };
            if let Some(i) = panels::pec_grid(
                ui,
                if region {
                    "ribbon-region"
                } else {
                    "ribbon-line"
                },
                current.as_ref(),
            ) {
                let r = if region {
                    m.set_region(json!({ "color": i }))
                } else {
                    m.set_line(json!({ "color": i }))
                };
                app.execute(&r);
            }
        }
        LineSewType => {
            let m = openstitch_app::editor::inspector::InspectorModel::new(
                app.session.design(),
                &app.selection,
            );
            for (k, label) in openstitch_app::editor::inspector::line_types() {
                if ui
                    .selectable_label(m.line_kind.same() == Some(&k), label)
                    .clicked()
                {
                    let r = m.set_line_type(k);
                    app.execute(&r);
                }
            }
        }
        RegionSewType => {
            let m = openstitch_app::editor::inspector::InspectorModel::new(
                app.session.design(),
                &app.selection,
            );
            for (k, label) in openstitch_app::editor::inspector::region_types() {
                if ui
                    .selectable_label(m.region_kind.same() == Some(&k), label)
                    .clicked()
                {
                    let r = m.set_region_type(k);
                    app.execute(&r);
                }
            }
        }
        EntryExit => edit_ui::entry_exit_menu(app, ui),
        SewingPoints => edit_ui::sewing_points_menu(app, ui),
        EdgeRadius => edge_radius_menu(app, ui),
        ChangeColor => change_color_menu(app, ui),
        SkeletonWidth => skeleton_width_menu(app, ui),
        SmoothTolerance => crate::shape_tools_ui::smooth_tolerance_menu(app, ui),
        ShapeEffects => crate::shape_effects_ui::effects_menu(app, ui),
        LineGap => crate::line_gap_ui::gap_menu(app, ui),
        EditCharacter => crate::lettering_ui::character_menu(app, ui),
        _ => {}
    }
}

/// Shapes (Rectangle) ▸ Edge radius: the corner radius of the rectangles drawn next.
fn edge_radius_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let units = app.ui.prefs.units;
    let mut r = app.tools.corner_radius();
    ui.horizontal(|ui| {
        ui.label("Edge radius");
        ui.add(
            crate::units_ui::drag(&mut r, units)
                .range(0.0..=100.0)
                .speed(0.1),
        );
    });
    for mm in [0.0, 2.0, 5.0, 10.0] {
        if ui.selectable_label(r == mm, format!("{mm} mm")).clicked() {
            r = mm;
        }
    }
    app.tools.set_corner_radius(r);
    crate::shape_effects_ui::corner_radii_ui(app, ui);
}

/// Shapes (Manual Punch) ▸ Column width: the width of the next Satin Skeleton points.
fn skeleton_width_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let units = app.ui.prefs.units;
    let mut w = app.tools.skeleton_width();
    ui.horizontal(|ui| {
        ui.label("Column width");
        ui.add(
            crate::units_ui::drag(&mut w, units)
                .range(0.1..=openstitch_design::skeleton::MAX_WIDTH)
                .speed(0.1),
        );
    });
    for mm in [1.5, 3.0, 5.0, 8.0] {
        if ui.selectable_label(w == mm, format!("{mm} mm")).clicked() {
            w = mm;
        }
    }
    app.tools.set_skeleton_width(w);
}

/// Stitches ▸ Change Color: each thread of the selected stitch pattern, with the PEC chart to
/// pick its new colour.
fn change_color_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let Some(id) = convert_ui::stitch_selection(app).1 else {
        return;
    };
    let d = app.session.design();
    let threads = d
        .index_of(id)
        .and_then(|i| d.objects[i].stitch_block.as_ref())
        .map(|b| b.threads.clone())
        .unwrap_or_default();
    for (k, t) in threads.iter().enumerate() {
        let name = t.name.as_deref().unwrap_or("Thread").to_string();
        let (r, g, b) = (t.red, t.green, t.blue);
        // The thread colour as a swatch: a filled square (egui's white texel tinted), not a glyph.
        let side = ui.text_style_height(&egui::TextStyle::Button);
        let swatch = egui::Image::new(egui::load::SizedTexture::new(
            egui::TextureId::default(),
            egui::Vec2::splat(side),
        ))
        .uv(egui::Rect::from_min_max(
            egui::epaint::WHITE_UV,
            egui::epaint::WHITE_UV,
        ))
        .tint(Color32::from_rgb(r, g, b))
        .corner_radius(2.0);
        ui.menu_button((swatch, format!("{}. {name}", k + 1)), |ui| {
            if let Some(i) = panels::pec_grid(ui, &format!("change-color-{k}"), Some(t)) {
                app.execute(&CommandRequest::new(
                    "stitches.setThread",
                    json!({ "id": id, "index": k, "color": i }),
                ));
                ui.close();
            }
        });
    }
}

fn scale(app: &mut EditorApp, f: f64) {
    if (f - 1.0).abs() > 1e-9 {
        app.on_selection(|s| {
            CommandRequest::new(
                "object.scale",
                json!({ "ids": CommandRequest::ids(s), "sx": f }),
            )
        });
    }
}

fn first_text(app: &EditorApp) -> Option<String> {
    let d = app.session.design();
    convert_ui::texts(app)
        .first()
        .and_then(|&id| d.index_of(id))
        .and_then(|i| d.objects[i].text.as_ref().map(|t| t.text.clone()))
}

/// `object.setText` on the selected text objects.
pub fn set_text(app: &mut EditorApp, mut params: serde_json::Value) {
    let ids: BTreeSet<u64> = convert_ui::texts(app).into_iter().collect();
    if ids.is_empty() {
        return;
    }
    params["ids"] = CommandRequest::ids(&ids);
    app.execute(&CommandRequest::new("object.setText", params));
}

pub fn image_density_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let Some(b) = app.session.design().background.clone() else {
        crate::art::notice(ui, crate::art::Art::NoImage, 40.0, "No background image");
        return;
    };
    let label = if b.visible { "Hide" } else { "Show" };
    if ui.button(label).clicked() {
        app.execute(&CommandRequest::new(
            "background.show",
            json!({ "visible": !b.visible }),
        ));
    }
    for (name, o) in [
        ("Opacity 25 %", 0.25),
        ("Opacity 50 %", 0.5),
        ("Opacity 75 %", 0.75),
        ("Opacity 100 %", 1.0),
    ] {
        if ui.radio(b.opacity == o, name).clicked() {
            app.execute(&CommandRequest::new(
                "background.show",
                json!({ "opacity": o }),
            ));
        }
    }
}

/// Options ▸ Accent colour (#559): the theme's own, a preset or any colour.
fn accent_menu(app: &mut EditorApp, ui: &mut egui::Ui) -> bool {
    use openstitch_app::editor::accent::PRESETS;
    let mut changed = false;
    ui.menu_button("Accent colour", |ui| {
        let tip = "Selection outline, handles, selected buttons and links; text on it keeps a 4.5:1 contrast";
        let a = &mut app.ui.prefs.accent;
        changed |= ui.radio_value(a, None, "Theme default").on_hover_text(tip).changed();
        for (_, label, c) in PRESETS {
            changed |= ui.radio_value(a, Some(*c), *label).on_hover_text(tip).changed();
        }
        ui.horizontal(|ui| {
            let mut rgb = a.unwrap_or(PRESETS[0].2);
            if ui.color_edit_button_srgb(&mut rgb).on_hover_text(tip).changed() {
                *a = Some(rgb);
                changed = true;
            }
            ui.label("Custom");
        });
    });
    changed
}

/// Options ▸ Layouts (#551): save where the panes are under a name, put a saved one back.
fn layouts_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    let names: Vec<String> = app.ui.prefs.layouts.iter().map(|l| l.0.clone()).collect();
    for name in &names {
        ui.horizontal(|ui| {
            if ui
                .button(name.as_str())
                .on_hover_text("Put this layout in place")
                .clicked()
            {
                let _ = app.ui.prefs.apply_layout(name);
                app.ui.save_prefs();
                ui.close();
            }
            if ui.small_button("Delete").clicked() {
                let _ = app.ui.prefs.delete_layout(name);
                app.ui.save_prefs();
            }
        });
    }
    if !names.is_empty() {
        ui.separator();
    }
    // Where each docked pane sits (#551).
    {
        use openstitch_app::editor::prefs::Side;
        let panes = &mut app.ui.prefs.panes;
        let mut changed = false;
        for (label, state, sides) in [
            (
                "Sewing Order",
                &mut panes.sewing_order,
                [Side::Left, Side::Right],
            ),
            (
                "Attributes",
                &mut panes.attributes,
                [Side::Left, Side::Right],
            ),
            (
                "Stitch Simulator",
                &mut panes.simulator,
                [Side::Top, Side::Bottom],
            ),
        ] {
            ui.horizontal(|ui| {
                ui.label(label);
                for side in sides {
                    let text = match side {
                        Side::Left => "Left",
                        Side::Right => "Right",
                        Side::Top => "Top",
                        Side::Bottom => "Bottom",
                    };
                    changed |= ui.radio_value(&mut state.side, side, text).changed();
                }
            });
        }
        if changed {
            app.ui.save_prefs();
        }
        ui.separator();
    }
    let id = egui::Id::unique("layout_name");
    let mut name: String = ui.data_mut(|d| d.get_temp(id).unwrap_or_default());
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(&mut name)
                .hint_text("Lettering")
                .desired_width(110.0),
        );
        if ui
            .button("Save Layout")
            .on_hover_text("Keep where the panes are and the toolbar position under this name")
            .clicked()
        {
            match app.ui.prefs.save_layout(&name) {
                Ok(n) => {
                    app.status = format!("Layout '{n}' saved");
                    app.ui.save_prefs();
                    name.clear();
                }
                Err(e) => app.status = e,
            }
        }
    });
    ui.data_mut(|d| d.insert_temp(id, name));
}

/// Options ▸ Reset Preferences (#551): one area back to its defaults, the others kept.
fn reset_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    use openstitch_app::editor::prefs::RESET_AREAS;
    for (key, label, what) in RESET_AREAS {
        if ui
            .button(*label)
            .on_hover_text(format!("Reset {what}"))
            .clicked()
        {
            if app.ui.prefs.reset_area(key).is_ok() {
                app.ui.save_prefs();
                app.status = format!("{label} preferences reset");
            }
            ui.close();
        }
    }
}

/// Options ▸ New Objects (#553): what new shapes start with; an unset value keeps the tools' own.
fn new_objects_menu(app: &mut EditorApp, ui: &mut egui::Ui) -> bool {
    use openstitch_app::editor::new_objects::FIELDS;
    let mut changed = false;
    ui.label(RichText::new("New shapes start with").strong());
    let n = &mut app.ui.prefs.new_objects;
    egui::Grid::new("new-objects")
        .num_columns(2)
        .show(ui, |ui| {
            for f in FIELDS {
                let mut on = n.get(f.key).is_some();
                if ui.checkbox(&mut on, f.label).changed() {
                    let v = on.then(|| (f.range.0 + f.range.1) / 2.0);
                    let start = match f.key {
                        "angle" => 45.0,
                        "density" => 4.5,
                        "stitchLength" => 4.0,
                        "pullCompensation" => 0.2,
                        _ => 2.5,
                    };
                    let _ = n.set(f.key, v.map(|_| start));
                    changed = true;
                }
                if let Some(mut v) = n.get(f.key) {
                    if ui
                        .add(
                            egui::DragValue::new(&mut v)
                                .range(f.range.0..=f.range.1)
                                .speed(0.1)
                                .suffix(f.suffix),
                        )
                        .changed()
                    {
                        let _ = n.set(f.key, Some(v));
                        changed = true;
                    }
                } else {
                    ui.label(RichText::new("tool's own").weak());
                }
                ui.end_row();
            }
            let mut on = n.underlay.is_some();
            if ui.checkbox(&mut on, "Underlay").changed() {
                n.underlay = on.then_some(true);
                changed = true;
            }
            if let Some(u) = n.underlay.as_mut() {
                changed |= ui.checkbox(u, "on").changed();
            } else {
                ui.label(RichText::new("tool's own").weak());
            }
            ui.end_row();
        });
    if ui
        .add_enabled(!n.is_empty(), egui::Button::new("Clear"))
        .on_hover_text("New shapes start as the tools make them (PE-DESIGN's defaults)")
        .clicked()
    {
        *n = Default::default();
        changed = true;
    }
    changed
}

pub fn options_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    for a in OPTION_MENU {
        menu_item(app, ui, a);
    }
    ui.separator();
    let mut optimize = app.session.user_settings().optimize_entry_exit;
    if ui
        .checkbox(&mut optimize, "Optimize Entry/Exit Points")
        .on_hover_text("New shapes are entered where they come closest to the shape sewn before")
        .changed()
    {
        app.execute(&CommandRequest::new(
            "options.set",
            json!({ "optimizeEntryExit": optimize }),
        ));
    }
    ui.separator();
    ui.label(RichText::new("Theme").weak());
    let mut changed = false;
    for t in Theme::ALL {
        changed |= ui
            .radio_value(&mut app.ui.prefs.theme, t, t.label())
            .changed();
    }
    changed |= accent_menu(app, ui);
    ui.menu_button("Layouts", |ui| layouts_menu(app, ui));
    ui.menu_button("Reset Preferences", |ui| reset_menu(app, ui));
    ui.separator();
    changed |= ui
        .checkbox(
            &mut app.ui.prefs.qat_below,
            "Quick Access Toolbar below the Ribbon",
        )
        .changed();
    changed |= ui
        .checkbox(&mut app.ui.prefs.splash, "Show splash at startup")
        .changed();
    changed |= ui
        .checkbox(&mut app.ui.prefs.start_cards, "Show start cards")
        .on_hover_text("An empty design offers cards to draw, letter, digitize a picture, import, use a template or open a sample")
        .changed();
    ui.menu_button("New Objects", |ui| changed |= new_objects_menu(app, ui));
    changed |= ui
        .checkbox(&mut app.ui.prefs.open_in_tabs, "Open designs in tabs")
        .on_hover_text("Beyond PE-DESIGN: New and Open add a tab and keep the current design open (Ctrl+Tab switches); off, they replace it as PE-DESIGN does")
        .changed();
    changed |= ui
        .checkbox(&mut app.ui.prefs.autosave, "Autosave for crash recovery")
        .on_hover_text("Beyond PE-DESIGN: keeps a copy of each open design in OpenStitch's settings folder (never the design's own file) and offers it at the next start after a crash or power cut")
        .changed();
    ui.add_enabled_ui(app.ui.prefs.autosave, |ui| {
        ui.horizontal(|ui| {
            use openstitch_app::editor::recovery::{MAX_INTERVAL, MIN_INTERVAL};
            ui.label("Autosave at most every");
            changed |= ui
                .add(
                    egui::DragValue::new(&mut app.ui.prefs.autosave_interval)
                        .range(MIN_INTERVAL..=MAX_INTERVAL)
                        .suffix(" s"),
                )
                .on_hover_text(
                    "While you keep editing; after 3 s without a change a copy is kept at once",
                )
                .changed();
        });
    });
    if ui
        .button("Clear Recovery Files")
        .on_hover_text("Delete the copies left by windows that crashed")
        .clicked()
    {
        match crate::recovery_ui::discard(app, None) {
            Ok(v) => {
                let n = v["discarded"].as_array().map_or(0, Vec::len);
                app.status = format!("{n} recovery files deleted");
            }
            Err(e) => app.status = e,
        }
    }
    let ai_changed = ui
        .checkbox(&mut app.ui.prefs.ai.enabled, "Show AI features")
        .on_hover_text(
            "The Assistant panel, its button and Ctrl+Shift+A; nothing is sent until you use it",
        )
        .changed();
    changed |= ai_changed;
    if !app.ui.prefs.ai.enabled {
        crate::assistant_panel::ai_switched_off(app);
    } else if ai_changed && app.ui.prefs.ai.panel_open {
        // Back on: the panel returns as the user left it (#422).
        app.assistant.panel.open = true;
    }
    ui.separator();
    ui.label(RichText::new("Units").weak());
    ui.horizontal(|ui| {
        use openstitch_app::editor::units::Units;
        let tip =
            "Sizes, positions, rulers, grid and lettering heights; stitch settings stay in mm";
        let u = &mut app.ui.prefs.units;
        changed |= ui
            .radio_value(u, Units::Mm, "Millimetres")
            .on_hover_text(tip)
            .changed();
        changed |= ui
            .radio_value(u, Units::Inch, "Inches")
            .on_hover_text(tip)
            .changed();
    });
    let units = app.ui.prefs.units;
    app.database.units = units;
    ui.separator();
    ui.label(RichText::new("Editing").weak());
    let e = &mut app.ui.prefs.editing;
    ui.horizontal(|ui| {
        ui.label("Nudge step");
        changed |= ui
            .add(
                crate::units_ui::drag(&mut e.nudge_mm, units)
                    .range(EditingPrefs::NUDGE_MIN..=EditingPrefs::NUDGE_MAX)
                    .speed(0.05),
            )
            .on_hover_text("How far an arrow key moves the selection; Shift moves ten times as far")
            .changed();
    });
    changed |= ui
        .checkbox(&mut e.nudge_grid, "Nudge by the grid step while snapping")
        .on_hover_text("With Snap to Grid on, an arrow key moves one grid step")
        .changed();
    changed |= ui
        .checkbox(&mut e.click_inside, "Click inside outlines selects them")
        .on_hover_text(
            "A click inside a shape that sews only its outline selects it; open lines are easier to hit",
        )
        .changed();
    ui.separator();
    ui.label(RichText::new("Updates").weak());
    changed |= ui
        .checkbox(
            &mut app.ui.prefs.updates.check,
            "Check for updates at startup",
        )
        .on_hover_text("Asks GitHub for a newer OpenStitch release each time the editor starts")
        .changed();
    changed |= ui
        .checkbox(
            &mut app.ui.prefs.updates.include_prereleases,
            "Include pre-releases",
        )
        .on_hover_text("Also offer alpha, beta and release-candidate versions")
        .changed();
    if ui.button("Check for Updates Now").clicked() {
        app.update.check(true);
        ui.close();
    }
    if changed {
        app.ui.save_prefs();
    }
}

pub fn help_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    if ui.button("Instruction Manual").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(
            "https://github.com/cristianonescu/OpenStitch#readme",
        ));
    }
    if ui
        .button("Ready-to-Sew Designs")
        .on_hover_text("Calibration designs, machine checks and a design for each feature, ready for the machine")
        .clicked()
    {
        use openstitch_app::editor::sew_kit;
        // The packaged kit in the file manager; GitHub when there is none or it cannot open.
        let exe = std::env::current_exe().ok();
        if sew_kit::open_kit(exe.as_deref()) == sew_kit::Opened::Online {
            ui.ctx().open_url(egui::OpenUrl::new_tab(sew_kit::ONLINE));
        }
    }
    if ui.button("Ready-to-Sew Designs Online").clicked() {
        ui.ctx().open_url(egui::OpenUrl::new_tab(
            openstitch_app::editor::sew_kit::ONLINE,
        ));
    }
    if ui
        .button("Getting Started")
        .on_hover_text("Your first design, step by step, with a button for each step")
        .clicked()
    {
        app.ui.start.page_open = true;
    }
    if ui.button("Keyboard Shortcuts").clicked() {
        app.ui.shortcuts_open = true;
    }
    if ui.button("Check for Updates").clicked() {
        app.update.check(true);
        ui.close();
    }
    if ui.button("About OpenStitch").clicked() {
        app.ui.about_open = true;
    }
}

// MARK: drawing

/// A ribbon command button; `large` = icon over a two-line label.
fn command(app: &mut EditorApp, ui: &mut egui::Ui, a: Action, large: bool) {
    let info = a.info();
    let on = enabled(app, a);
    let menu = a.is_menu();
    let resp = button(ui, &info, a, large, on, active(app, a), menu);
    if resp.hovered() {
        app.ui.hover = Some(info.tip);
    }
    if menu {
        if on {
            egui::Popup::menu(&resp).show(|ui| self::menu(app, a, ui));
        }
    } else if resp.clicked() && on {
        run(app, a, ui.ctx());
    }
}

/// Draws a ribbon button of any window (`icon_of`: the icon it shows) and returns its response
/// with the ScreenTip attached.
pub fn button(
    ui: &mut egui::Ui,
    info: &Info,
    icon_of: impl Into<icons::Icon>,
    large: bool,
    on: bool,
    pressed: bool,
    menu: bool,
) -> egui::Response {
    let icon_of = icon_of.into();
    let visuals = ui.visuals().clone();
    let fg = if on {
        visuals.text_color()
    } else {
        visuals.weak_text_color()
    };
    let font = egui::FontId::proportional(11.0);
    let (size, galley) = if large {
        let g = ui.painter().layout(info.label.to_string(), font, fg, 62.0);
        (vec2(g.size().x.max(LARGE_ICON) + 8.0, BODY), g)
    } else {
        let g = ui
            .painter()
            .layout_no_wrap(info.label.to_string(), font, fg);
        let w = SMALL_ICON + 6.0 + g.size().x + if menu { 14.0 } else { 6.0 };
        (vec2(w, 21.0), g)
    };
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let hovered = resp.hovered() && on;
    if pressed || hovered {
        let fill = if pressed {
            visuals.selection.bg_fill.gamma_multiply(0.35)
        } else {
            visuals.widgets.hovered.weak_bg_fill
        };
        ui.painter().rect(
            rect,
            CornerRadius::same(3),
            fill,
            Stroke::new(1.0, visuals.selection.bg_fill.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
    }
    let icon_fg = visuals.strong_text_color();
    if large {
        let icon = egui::Rect::from_center_size(
            egui::pos2(rect.center().x, rect.min.y + 4.0 + LARGE_ICON / 2.0),
            vec2(LARGE_ICON, LARGE_ICON),
        );
        icons::paint(ui.painter(), icon, icon_of, icon_fg, on);
        let text_pos = egui::pos2(rect.center().x - galley.size().x / 2.0, icon.max.y + 2.0);
        let bottom = text_pos.y + galley.size().y;
        ui.painter().galley(text_pos, galley, fg);
        if menu {
            caret(ui, egui::pos2(rect.center().x, bottom + 5.0));
        }
    } else {
        let icon = egui::Rect::from_min_size(
            egui::pos2(rect.min.x + 3.0, rect.center().y - SMALL_ICON / 2.0),
            vec2(SMALL_ICON, SMALL_ICON),
        );
        icons::paint(ui.painter(), icon, icon_of, icon_fg, on);
        let y = rect.center().y - galley.size().y / 2.0;
        let x = icon.max.x + 4.0;
        let end = x + galley.size().x;
        ui.painter().galley(egui::pos2(x, y), galley, fg);
        if menu {
            caret(ui, egui::pos2(end + 7.0, rect.center().y));
        }
    }
    resp.on_hover_text(info.screen_tip(MAC))
}

/// The drop-down caret of a ribbon menu button, centred on `c`: the Higgsfield `menu_caret`
/// icon (#653), as the other menu buttons show it.
fn caret(ui: &egui::Ui, c: egui::Pos2) {
    let r = egui::Rect::from_center_size(c, vec2(9.0, 9.0));
    icons::paint_ui(ui.painter(), r, "menu_caret", ui.visuals().text_color());
}

/// A ribbon group of any window: its columns, drawn by `draw(ui, command, large)`, and the
/// caption under them.
pub fn group<A: Copy>(
    ui: &mut egui::Ui,
    g: &ribbon::Group<A>,
    mut draw: impl FnMut(&mut egui::Ui, A, bool),
) {
    ui.vertical(|ui| {
        ui.set_height(BODY + 16.0);
        ui.horizontal(|ui| {
            ui.set_height(BODY);
            ui.spacing_mut().item_spacing.x = 2.0;
            for item in g.items {
                match item {
                    Item::Large(a) => draw(ui, *a, true),
                    Item::Small(list) => {
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 1.0;
                            for &a in *list {
                                draw(ui, a, false);
                            }
                        });
                    }
                }
            }
        });
        let w = ui.min_rect().width();
        ui.allocate_ui(vec2(w, 14.0), |ui| {
            ui.centered_and_justified(|ui| {
                ui.label(RichText::new(g.name).size(11.0).weak());
            });
        });
    });
}

/// Tracks tool changes and the contextual tabs; picks the tab a new tool brings forward.
fn update_tab(app: &mut EditorApp) -> Vec<TabId> {
    let ctx_tabs = ribbon::contextual_tabs(&facts(app));
    let tool = app.tools.tool();
    if tool != app.ui.last_tool {
        app.ui.last_tool = tool;
        if let Some(t) = ribbon::tab_for_tool(tool) {
            app.ui.tab = t;
        }
    }
    if !TabId::FIXED.contains(&app.ui.tab) && !ctx_tabs.contains(&app.ui.tab) {
        app.ui.tab = TabId::Home;
    }
    ctx_tabs
}

/// Application button, ribbon tabs, Option and Help buttons.
fn tab_strip(app: &mut EditorApp, ui: &mut egui::Ui, ctx_tabs: &[TabId]) {
    ui.horizontal(|ui| {
        app_button(app, ui);
        ui.add_space(6.0);
        let mut tabs: Vec<TabId> = TabId::FIXED.to_vec();
        tabs.extend_from_slice(ctx_tabs);
        for t in tabs {
            let selected = app.ui.tab == t;
            let mut text = tab_text(t.label(), selected);
            if t.context().is_some() {
                text = text.color(CONTEXT);
            }
            let r = ui.add(egui::Button::selectable(selected, text));
            if let Some(c) = t.context() {
                ui.painter().line_segment(
                    [r.rect.left_top(), r.rect.right_top()],
                    Stroke::new(3.0, CONTEXT),
                );
                r.clone().on_hover_text(c);
            }
            if r.clicked() {
                app.ui.tab = t;
            }
        }
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            for a in [Action::Help, Action::Options] {
                let info = a.info();
                let r = ui
                    .add(
                        egui::Button::new((info.label, crate::icons::ui_image(ui, "menu_caret")))
                            .frame(false),
                    )
                    .on_hover_text(info.screen_tip(MAC));
                egui::Popup::menu(&r).show(|ui| menu(app, a, ui));
            }
        });
    });
}

/// Text of a ribbon tab.
pub fn tab_text(label: &str, selected: bool) -> RichText {
    let text = RichText::new(label).size(13.0);
    if selected { text.strong() } else { text }
}

fn app_button(app: &mut EditorApp, ui: &mut egui::Ui) {
    let resp = app_button_widget(ui)
        .on_hover_text("Application menu: new, open, save, print, export, recent files");
    let force = std::mem::take(&mut app.ui.open_app_menu);
    let cmd = if resp.clicked() {
        Some(egui::SetOpenCommand::Toggle)
    } else if force {
        Some(egui::SetOpenCommand::Bool(true))
    } else {
        None
    };
    egui::Popup::menu(&resp)
        .open_memory(cmd)
        .layout(egui::Layout::top_down(egui::Align::Min))
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show(|ui| application_menu(app, ui));
}

/// The blue Application button at the left of the tab row (every ribbon window).
pub fn app_button_widget(ui: &mut egui::Ui) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(64.0, 24.0), Sense::click());
    let bg = if resp.hovered() {
        APP_BLUE.gamma_multiply(1.2)
    } else {
        APP_BLUE
    };
    ui.painter().rect_filled(rect, CornerRadius::same(3), bg);
    icons::paint_logo(
        ui.painter(),
        egui::Rect::from_min_size(rect.min + vec2(4.0, 2.0), vec2(20.0, 20.0)),
        bg,
    );
    ui.painter().text(
        egui::pos2(rect.min.x + 28.0, rect.center().y),
        egui::Align2::LEFT_CENTER,
        "File",
        egui::FontId::proportional(13.0),
        Color32::WHITE,
    );
    resp
}

fn application_menu(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.set_width(470.0);
    let line = ui.visuals().widgets.noninteractive.bg_stroke;
    ui.horizontal_top(|ui| {
        let left = ui
            .vertical(|ui| {
                ui.set_width(200.0);
                for a in APPLICATION_MENU {
                    app_menu_row(app, ui, a);
                }
            })
            .response
            .rect;
        ui.painter().vline(left.max.x + 6.0, left.y_range(), line);
        ui.add_space(10.0);
        ui.vertical(|ui| {
            ui.set_width(250.0);
            ui.label(RichText::new("Recent Files").strong());
            let recent = app.ui.prefs.recent.clone();
            if recent.is_empty() {
                crate::art::notice(ui, crate::art::Art::DbEmpty, 64.0, "No files opened yet");
            }
            for (k, p) in recent.iter().enumerate() {
                let name = p.file_name().map_or_else(
                    || p.display().to_string(),
                    |n| n.to_string_lossy().into_owned(),
                );
                if ui
                    .add(egui::Button::new(format!("{} {name}", k + 1)).frame(false))
                    .on_hover_text(p.display().to_string())
                    .clicked()
                {
                    if p.exists() {
                        app.guarded(Pending::Open(Some(p.clone())));
                    } else {
                        app.status = format!("{} no longer exists", p.display());
                    }
                    ui.close();
                }
            }
        });
    });
    let r = ui.available_rect_before_wrap();
    ui.painter().hline(r.x_range(), r.min.y + 3.0, line);
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(250.0);
        ui.menu_button("Options", |ui| options_menu(app, ui));
        if ui.button("Exit").clicked() {
            app.guarded(Pending::Exit);
        }
    });
}

fn app_menu_row(app: &mut EditorApp, ui: &mut egui::Ui, a: Action) {
    let info = a.info();
    let on = enabled(app, a);
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(24.0, 24.0), Sense::hover());
        icons::paint(ui.painter(), r, a, ui.visuals().strong_text_color(), on);
        let text = RichText::new(info.label).size(14.0);
        if a.is_menu() {
            ui.add_enabled_ui(on, |ui| {
                ui.menu_button(text, |ui| menu(app, a, ui))
                    .response
                    .on_hover_text(info.screen_tip(MAC));
            });
        } else {
            let mut b = egui::Button::new(text).frame(false);
            if let Some(s) = info.shortcut_for(MAC) {
                b = b.shortcut_text(s);
            }
            let r = ui
                .add_enabled(on, b)
                .on_hover_text(info.screen_tip(MAC))
                .on_disabled_hover_text(info.screen_tip(MAC));
            if r.clicked() {
                run(app, a, ui.ctx());
                ui.close();
            }
        }
    });
}

/// A small icon button of a Quick Access Toolbar (or the Design Database toolbar), drawn
/// pressed while `pressed`.
pub fn qat_button(
    ui: &mut egui::Ui,
    info: &Info,
    icon_of: impl Into<icons::Icon>,
    on: bool,
    pressed: bool,
) -> egui::Response {
    let icon_of = icon_of.into();
    let (r, resp) = ui.allocate_exact_size(vec2(22.0, 20.0), Sense::click());
    let visuals = ui.visuals();
    if pressed {
        ui.painter().rect(
            r,
            CornerRadius::same(3),
            visuals.selection.bg_fill.gamma_multiply(0.35),
            Stroke::new(1.0, visuals.selection.bg_fill.gamma_multiply(0.6)),
            StrokeKind::Inside,
        );
    } else if resp.hovered() && on {
        ui.painter().rect_filled(
            r,
            CornerRadius::same(3),
            visuals.widgets.hovered.weak_bg_fill,
        );
    }
    icons::paint(
        ui.painter(),
        egui::Rect::from_center_size(r.center(), vec2(16.0, 16.0)),
        icon_of,
        ui.visuals().strong_text_color(),
        on,
    );
    resp.on_hover_text(info.screen_tip(MAC))
}

/// Quick Access Toolbar: small icon buttons and the customise list.
pub fn quick_access(app: &mut EditorApp, ui: &mut egui::Ui) {
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for a in app.ui.prefs.qat.clone() {
            let on = enabled(app, a);
            let resp = qat_button(ui, &a.info(), a, on, false);
            if a.is_menu() {
                if on {
                    egui::Popup::menu(&resp).show(|ui| menu(app, a, ui));
                }
            } else if resp.clicked() {
                run(app, a, ui.ctx());
            }
        }
        let r = ui
            .add(egui::Button::new(crate::icons::ui_image(ui, "menu_caret")).frame(false))
            .on_hover_text("Customize Quick Access Toolbar");
        egui::Popup::menu(&r).show(|ui| {
            ui.label(RichText::new("Customize Quick Access Toolbar").strong());
            for a in ribbon::qat_candidates() {
                let mut on = app.ui.prefs.qat.contains(&a);
                if ui.checkbox(&mut on, a.info().label).changed() {
                    app.ui.prefs.toggle_qat(a);
                    app.ui.save_prefs();
                }
            }
            ui.separator();
            if ui.button("Reset to default").clicked() {
                app.ui.prefs.qat = ribbon::QAT_DEFAULT.to_vec();
                app.ui.save_prefs();
            }
            if ui
                .checkbox(&mut app.ui.prefs.qat_below, "Show below the Ribbon")
                .changed()
            {
                app.ui.save_prefs();
            }
        });
    });
}

/// The top of the window: Quick Access Toolbar, tabs and the active tab's groups.
pub fn ribbon(app: &mut EditorApp, ui: &mut egui::Ui) {
    app.ui.hover = None;
    let ctx_tabs = update_tab(app);
    if !app.ui.prefs.qat_below {
        quick_access(app, ui);
    }
    tab_strip(app, ui, &ctx_tabs);
    body(ui, app.ui.tab.groups(), |ui, a, large| {
        command(app, ui, a, large)
    });
    if app.ui.prefs.qat_below {
        quick_access(app, ui);
    }
}

/// The groups of the active tab, divided by separators (every ribbon window).
pub fn body<A: Copy>(
    ui: &mut egui::Ui,
    groups: &[ribbon::Group<A>],
    mut draw: impl FnMut(&mut egui::Ui, A, bool),
) {
    let fill = ui.visuals().extreme_bg_color;
    egui::Frame::new()
        .fill(fill)
        .inner_margin(egui::Margin::symmetric(6, 4))
        .stroke(ui.visuals().widgets.noninteractive.bg_stroke)
        .show(ui, |ui| {
            egui::ScrollArea::horizontal()
                .id_salt("ribbon-body")
                .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        for (k, g) in groups.iter().enumerate() {
                            group(ui, g, &mut draw);
                            if k + 1 < groups.len() {
                                ui.add(egui::Separator::default().spacing(4.0));
                            }
                        }
                    });
                });
        });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Launch;
    use openstitch_app::editor::ribbon::{TabId, ribbon_actions};

    /// Actions that open a native file dialog, a URL or close the window.
    fn interactive(a: Action) -> bool {
        use Action::*;
        matches!(
            a,
            Open | Save
                | SaveAs
                | Exit
                | ImportPatterns
                | ImportSvg
                | ImportPesObjects
                | OpenImage
                | PasteImage
                | TrueTypeAttributes
                | ExportSections
        )
    }

    fn demo() -> EditorApp {
        EditorApp::new(Launch {
            demo: true,
            ..Launch::default()
        })
    }

    /// Runs every available, non-interactive button with each of `selections` (or, for a menu,
    /// draws its list); `unavailable` also checks the missing ones are disabled.
    fn every_ribbon_button_runs(selections: &[&[u64]], unavailable: bool) {
        let ctx = egui::Context::default();
        for a in ribbon_actions()
            .into_iter()
            .chain(APPLICATION_MENU)
            .chain(OPTION_MENU)
        {
            let info = a.info();
            if !info.available {
                if unavailable {
                    let app = demo();
                    assert!(!enabled(&app, a), "{a:?} is unavailable but enabled");
                    assert!(info.screen_tip(false).contains("Not yet available"));
                }
                continue;
            }
            if interactive(a) {
                continue;
            }
            for &sel in selections {
                let mut app = demo();
                app.selection = sel.iter().copied().collect();
                // Runs the command (or, for a menu, draws its list) without panicking.
                if a.is_menu() {
                    let mut out = ctx.run_ui(egui::RawInput::default(), |ui| menu(&mut app, a, ui));
                    out.textures_delta.clear();
                } else {
                    run(&mut app, a, &ctx);
                }
            }
        }
    }

    // The sweep builds a fresh demo app per button and selection, so it is split in two
    // tests that run in parallel.
    #[test]
    fn every_ribbon_button_runs_or_is_marked_unavailable() {
        every_ribbon_button_runs(&[&[], &[1]], true);
    }

    #[test]
    fn every_ribbon_button_runs_with_the_other_selections() {
        every_ribbon_button_runs(&[&[2], &[1, 2]], false);
    }

    #[test]
    fn ribbon_commands_change_the_design_and_view() {
        let ctx = egui::Context::default();
        let mut app = demo();
        let n = app.session.design().objects.len();
        app.selection = BTreeSet::from([1]);
        run(&mut app, Action::Copy, &ctx);
        assert!(enabled(&app, Action::Paste));
        run(&mut app, Action::Paste, &ctx);
        assert_eq!(app.session.design().objects.len(), n + 1);
        assert!(!app.selection.contains(&1), "the copy is selected");
        run(&mut app, Action::Delete, &ctx);
        assert_eq!(app.session.design().objects.len(), n);
        run(&mut app, Action::Undo, &ctx);
        assert_eq!(app.session.design().objects.len(), n + 1);
        run(&mut app, Action::ClosedStraight, &ctx);
        assert_eq!(app.tools.tool(), Tool::Polygon);
        assert!(active(&app, Action::ClosedStraight));
        run(&mut app, Action::RealisticView, &ctx);
        assert!(app.workspace.realistic && !active(&app, Action::StitchView));
        run(&mut app, Action::PaneSimulator, &ctx);
        assert!(!app.ui.prefs.panes.simulator.visible);
        run(&mut app, Action::PaneImport, &ctx);
        assert_eq!(app.ui.pane_tab, PaneTab::Import);
        run(&mut app, Action::Applique, &ctx);
        assert!(app.pattern_wizards.open.is_some());
        run(&mut app, Action::CrossStitch, &ctx);
        assert!(app.wizard.open);
    }

    #[test]
    fn view_image_text_and_fabric_commands() {
        let ctx = egui::Context::default();
        let mut app = demo();
        // View modes are exclusive.
        run(&mut app, Action::SolidView, &ctx);
        assert!(active(&app, Action::SolidView) && !active(&app, Action::StitchView));
        run(&mut app, Action::RealisticView, &ctx);
        assert!(!active(&app, Action::SolidView) && active(&app, Action::RealisticView));
        run(&mut app, Action::StitchView, &ctx);
        assert!(active(&app, Action::StitchView));
        run(&mut app, Action::GridAxes, &ctx);
        assert!(!app.workspace.grid.axes);
        app.workspace.rulers = false;
        run(&mut app, Action::Guideline, &ctx);
        assert!(active(&app, Action::Guideline) && app.workspace.rulers);
        run(&mut app, Action::FontFilter, &ctx);
        assert!(active(&app, Action::FontFilter));

        // Image commands need a background image.
        assert!(!enabled(&app, Action::ImageRotateRight));
        let dir = openstitch_testutil::TempDir::new("view-ui");
        let png = dir.join("bg.png");
        let raster =
            openstitch_digitize::Raster::from_fn(40, 20, |x, _| [x as u8 * 6, 90, 30, 255])
                .expect("raster");
        std::fs::write(&png, raster.encode_png().expect("png")).expect("write");
        app.execute(&CommandRequest::new(
            "background.set",
            json!({ "path": png.display().to_string(), "width": 40 }),
        ));
        for a in [
            Action::SaveImage,
            Action::ModifyImage,
            Action::ImageRotateLeft,
        ] {
            assert!(enabled(&app, a), "{a:?}");
        }
        run(&mut app, Action::ImageRotateRight, &ctx);
        let b = app.session.design().background.clone().expect("background");
        assert_eq!((b.adjust.rotation, b.width, b.height), (1, 200.0, 400.0));
        let shown = crate::view_ui::shown_image(&app).expect("image");
        assert_eq!((shown.width(), shown.height()), (20, 40));

        // Every new window draws without panicking.
        for a in [
            Action::RealisticSettings,
            Action::ModifyImage,
            Action::FontFilterSettings,
            Action::FabricSelector,
        ] {
            run(&mut app, a, &ctx);
        }
        let v = &app.workspace.view;
        assert!(v.realistic_open && v.image_open && v.fonts_open && v.fabric_open);
        app.workspace.solid = true;
        for _ in 0..2 {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| {
                crate::view_ui::windows(&mut app, ui.ctx());
                crate::canvas::canvas(&mut app, ui);
            });
            out.textures_delta.clear();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn cut_and_paste_put_the_objects_back() {
        let ctx = egui::Context::default();
        let mut app = demo();
        let n = app.session.design().objects.len();
        assert!(!enabled(&app, Action::Cut));
        app.selection = BTreeSet::from([1]);
        let before = app.session.design().objects[0].clone();
        run(&mut app, Action::Cut, &ctx);
        assert_eq!(app.session.design().objects.len(), n - 1);
        assert!(app.selection.is_empty() && enabled(&app, Action::Paste));
        run(&mut app, Action::Paste, &ctx);
        let d = app.session.design();
        assert_eq!(d.objects.len(), n);
        let pasted = d.objects.last().expect("pasted");
        assert_eq!(app.selection, BTreeSet::from([pasted.id]));
        assert_eq!(pasted.contours, before.contours, "back where it was");
        // Cut is one Undo; Copy forgets the cut objects.
        run(&mut app, Action::Undo, &ctx);
        run(&mut app, Action::Undo, &ctx);
        assert_eq!(app.session.design().objects.len(), n);
        app.selection = BTreeSet::from([2]);
        // Copy keeps the objects, not their ids: deleting the originals keeps Paste working.
        run(&mut app, Action::Copy, &ctx);
        let d = app.session.design();
        let copied = d.objects[d.index_of(2).expect("object 2")].clone();
        run(&mut app, Action::Delete, &ctx);
        assert!(enabled(&app, Action::Paste));
        run(&mut app, Action::Paste, &ctx);
        let pasted = app.session.design().objects.last().expect("pasted");
        assert_eq!(pasted.contours, copied.contours);
        assert_ne!(pasted.id, copied.id);
    }

    #[test]
    fn drawing_and_editing_commands() {
        let ctx = egui::Context::default();
        let mut app = demo();
        for (a, t) in [
            (Action::OpenStraight, Tool::Line),
            (Action::PencilClosed, Tool::Pencil),
            (Action::PencilOpen, Tool::PencilOpen),
            (Action::CurvedBlock, Tool::CurvedBlock),
            (Action::Running, Tool::Running),
            (Action::Feed, Tool::Feed),
        ] {
            run(&mut app, a, &ctx);
            assert_eq!(app.tools.tool(), t);
            assert!(active(&app, a) && !active(&app, Action::StraightBlock));
        }
        assert_eq!(update_tab(&mut app), [TabId::ManualPunch]);
        // Reverse Direction Line on a region.
        app.tools.set_tool(Tool::Select);
        let id = app
            .session
            .design()
            .objects
            .iter()
            .find(|o| o.region.is_some())
            .expect("region")
            .id;
        app.selection = BTreeSet::from([id]);
        let angle = |app: &EditorApp| {
            let d = app.session.design();
            d.objects[d.index_of(id).expect("o")]
                .region
                .as_ref()
                .expect("r")
                .angle
        };
        let a0 = angle(&app);
        assert!(enabled(&app, Action::ReverseDirectionLine));
        run(&mut app, Action::ReverseDirectionLine, &ctx);
        assert_eq!(angle(&app), (a0 + 180.0).rem_euclid(360.0));
        // Satin Skeleton from Shape: the region becomes satin along a derived centre line.
        let bar = app
            .execute(&CommandRequest::new(
                "shape.rectangle",
                json!({ "x": 10, "y": 10, "width": 60, "height": 8 }),
            ))
            .and_then(|r| r["id"].as_u64())
            .expect("bar");
        app.selection = BTreeSet::from([bar]);
        assert!(enabled(&app, Action::DeriveSkeleton));
        run(&mut app, Action::DeriveSkeleton, &ctx);
        let d = app.session.design();
        let r = d.objects[d.index_of(bar).expect("o")]
            .region
            .as_ref()
            .expect("r");
        assert!(r.skeleton.is_some(), "{}", app.status);
        run(&mut app, Action::Undo, &ctx);
        run(&mut app, Action::Undo, &ctx);
        app.selection = BTreeSet::from([id]);
        // Change Color needs one stitch pattern.
        assert!(!enabled(&app, Action::ChangeColor));
        run(&mut app, Action::ConvertToStitches, &ctx);
        let st = *app.selection.first().expect("converted");
        app.selection = BTreeSet::from([st]);
        assert!(enabled(&app, Action::ChangeColor));
        app.execute(&CommandRequest::new(
            "stitches.setThread",
            json!({ "id": st, "index": 0, "color": "#123456" }),
        ));
        let d = app.session.design();
        let b = d.objects[d.index_of(st).expect("st")]
            .stitch_block
            .as_ref()
            .expect("b");
        assert_eq!(b.threads[0].hex(), "#123456");
        // Move Sections needs a split design.
        assert!(!enabled(&app, Action::MoveSections));
        app.execute(&CommandRequest::new(
            "design.new",
            json!({ "name": "Banner", "width": 230, "height": 160 }),
        ));
        app.execute(&CommandRequest::new(
            "sections.set",
            json!({ "hoop": "130x180" }),
        ));
        assert!(enabled(&app, Action::MoveSections));
        run(&mut app, Action::MoveSections, &ctx);
        assert!(active(&app, Action::MoveSections));
        run(&mut app, Action::MoveSections, &ctx);
        assert!(!active(&app, Action::MoveSections));
    }

    #[test]
    fn contextual_tabs_appear_for_tool_and_selection() {
        let mut app = demo();
        assert!(update_tab(&mut app).is_empty());
        assert_eq!(app.ui.tab, TabId::Home);
        app.tools.set_tool(Tool::Rectangle);
        assert_eq!(update_tab(&mut app), [TabId::OutlineRect]);
        assert_eq!(
            app.ui.tab,
            TabId::OutlineRect,
            "the tool brings its tab forward"
        );
        app.tools.set_tool(Tool::Select);
        app.selection = BTreeSet::from([2]);
        assert_eq!(update_tab(&mut app), [TabId::Shapes]);
        assert_eq!(
            app.ui.tab,
            TabId::Home,
            "a tab that went away falls back to Home"
        );
        app.ui.tab = TabId::Shapes;
        app.execute(&CommandRequest::new(
            "text.add",
            json!({ "text": "Hi", "x": 50, "y": 20 }),
        ));
        let text = app.session.design().objects.last().expect("text").id;
        app.selection = BTreeSet::from([text, 2]);
        assert_eq!(update_tab(&mut app), [TabId::Text, TabId::Shapes]);
        assert!(enabled(&app, Action::FitTextToOutline));
        // Fit Text ▸ Fit Text to Outline…: the dialog's OK fits it with its options.
        crate::lettering_ui::open_fit(&mut app, text, 2);
        app.lettering.fit.valign = openstitch_design::VAlign::Middle;
        app.lettering.fit.other_side = true;
        let r = crate::lettering_ui::fit_request(&app.lettering.fit, text, 2);
        app.execute(&r);
        let d = app.session.design();
        let t = d.objects[d.index_of(text).expect("text")]
            .text
            .clone()
            .expect("spec");
        assert_eq!(t.path, Some(2));
        assert!(t.fit.other_side && t.fit.valign == openstitch_design::VAlign::Middle);
        assert_eq!(fitted_texts(&app), vec![text]);
        app.execute(&CommandRequest::new(
            "text.setPath",
            json!({ "id": text, "path": null }),
        ));
        assert!(enabled(&app, Action::Font) && enabled(&app, Action::RegionColor));
        app.selection = BTreeSet::from([text]);
        let ctx = egui::Context::default();
        run(&mut app, Action::ConvertToOutline, &ctx);
        assert!(
            app.session
                .design()
                .objects
                .iter()
                .all(|o| o.kind != ObjectKind::Text),
            "text converted to shapes"
        );
        app.open_path(
            &std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../openstitch-engine/tests/fixtures/badge-golden.pes"),
        );
        app.select_all();
        assert_eq!(update_tab(&mut app), [TabId::Stitches]);
        assert!(enabled(&app, Action::DivideByColor));
    }
}
