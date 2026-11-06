//! Window preferences kept between runs in `ui.json` next to the user settings file: colour
//! theme, Quick Access Toolbar, which panes show (docked or floating), the most recently used
//! files, the realistic view settings, the font filter and the update preferences. Unknown or malformed entries fall
//! back to the defaults.

use super::lettering::FontChoice;
use super::ribbon::{Action, QAT_DEFAULT};
use super::updates::UpdatePrefs;
use super::workspace::RealisticSettings;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

/// How many files the Application menu lists.
pub const RECENT_MAX: usize = 10;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Theme {
    #[default]
    Light,
    Dark,
    /// Follow the operating system.
    System,
}

impl Theme {
    pub const ALL: [Theme; 3] = [Theme::Light, Theme::Dark, Theme::System];

    pub fn key(self) -> &'static str {
        match self {
            Theme::Light => "light",
            Theme::Dark => "dark",
            Theme::System => "system",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Theme::Light => "Light",
            Theme::Dark => "Dark",
            Theme::System => "System",
        }
    }

    pub fn from_key(key: &str) -> Option<Theme> {
        Theme::ALL.into_iter().find(|t| t.key() == key)
    }
}

/// The window edge a docked pane sits on (#551).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    Left,
    Right,
    Top,
    Bottom,
}

impl Side {
    pub const ALL: [Side; 4] = [Side::Left, Side::Right, Side::Top, Side::Bottom];

    pub fn key(self) -> &'static str {
        match self {
            Side::Left => "left",
            Side::Right => "right",
            Side::Top => "top",
            Side::Bottom => "bottom",
        }
    }

    pub fn from_key(k: &str) -> Option<Side> {
        Side::ALL.into_iter().find(|s| s.key() == k)
    }

    /// Left/right panes stay left or right, top/bottom ones top or bottom.
    pub fn fits(self, default: Side) -> bool {
        matches!(self, Side::Left | Side::Right) == matches!(default, Side::Left | Side::Right)
    }

    /// The opposite edge.
    pub fn other(self) -> Side {
        match self {
            Side::Left => Side::Right,
            Side::Right => Side::Left,
            Side::Top => Side::Bottom,
            Side::Bottom => Side::Top,
        }
    }
}

/// A pane: shown or hidden, docked to the window (on one side) or floating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PaneState {
    pub visible: bool,
    pub floating: bool,
    pub side: Side,
}

impl PaneState {
    const fn docked(side: Side) -> Self {
        PaneState {
            visible: true,
            floating: false,
            side,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Panes {
    pub sewing_order: PaneState,
    pub simulator: PaneState,
    /// Import / Color / Sewing Attributes / Text Attributes.
    pub attributes: PaneState,
}

impl Default for Panes {
    /// PE-DESIGN's places: Sewing Order left, attributes right, simulator at the bottom.
    fn default() -> Self {
        Panes {
            sewing_order: PaneState::docked(Side::Left),
            simulator: PaneState::docked(Side::Bottom),
            attributes: PaneState::docked(Side::Right),
        }
    }
}

/// A saved window layout (Options ▸ Layouts, #551): where the panes are and the toolbar position.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Layout {
    pub panes: Panes,
    pub qat_below: bool,
}

impl Layout {
    /// Most layouts kept, and the longest name.
    pub const MAX: usize = 20;
    pub const NAME_MAX: usize = 40;
}

/// Preference areas that reset on their own (#551): key, label and what they hold.
pub const RESET_AREAS: &[(&str, &str, &str)] = &[
    (
        "display",
        "Display",
        "theme, accent colour, Quick Access Toolbar, splash, realistic view, View ▸ Show",
    ),
    (
        "editing",
        "Editing",
        "units, nudge step, snapping, click inside outlines",
    ),
    (
        "panes",
        "Panes",
        "which panes show, docked where or floating",
    ),
    ("shortcuts", "Keyboard shortcuts", "every changed shortcut"),
    ("layouts", "Saved layouts", "every saved layout"),
];

/// Text ▸ Font Filter: fonts hidden from the font lists while the filter is on.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct FontFilter {
    pub on: bool,
    /// Font ids cleared in Font Filter Settings.
    pub hidden: Vec<String>,
}

impl FontFilter {
    /// Most hidden font ids kept.
    pub const MAX_HIDDEN: usize = 1_000;

    pub fn shows(&self, id: &str) -> bool {
        !self.on || !self.hidden.iter().any(|h| h == id)
    }

    /// The fonts the lists show. With every font hidden the filter is ignored, so a list is
    /// never empty.
    pub fn apply(&self, fonts: Vec<FontChoice>) -> Vec<FontChoice> {
        if !self.on || fonts.iter().all(|f| !self.shows(&f.id)) {
            return fonts;
        }
        fonts.into_iter().filter(|f| self.shows(&f.id)).collect()
    }

    /// Shows or hides one font in the filter settings.
    pub fn set_shown(&mut self, id: &str, shown: bool) {
        self.hidden.retain(|h| h != id);
        if !shown && self.hidden.len() < Self::MAX_HIDDEN {
            self.hidden.push(id.to_string());
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct UiPrefs {
    pub theme: Theme,
    /// Accent colour (#559); `None` keeps each theme's own.
    pub accent: Option<[u8; 3]>,
    /// The user's keyboard shortcuts (#551).
    pub keymap: super::keymap::Keymap,
    /// Saved window layouts by name (#551).
    pub layouts: Vec<(String, Layout)>,
    pub qat: Vec<Action>,
    /// Quick Access Toolbar under the ribbon instead of above it.
    pub qat_below: bool,
    pub panes: Panes,
    /// Newest first.
    pub recent: Vec<PathBuf>,
    pub realistic: RealisticSettings,
    pub font_filter: FontFilter,
    /// Startup splash on an interactive start.
    pub splash: bool,
    /// The folder of Ink/Stitch-format embroidery fonts (Text ▸ Ink/Stitch Fonts, #529).
    pub inkstitch_folder: Option<PathBuf>,
    pub updates: UpdatePrefs,
    /// Preferences ▸ AI (the Assistant panel, B17); the API key is in the keychain.
    pub ai: super::assistant_chat::AiPrefs,
    /// Editing aids beyond PE-DESIGN (#473, #474, #475); all off by default.
    pub editing: EditingPrefs,
    /// Millimetres or inches for every length shown and typed (#519); the Design Database's
    /// System Unit is the same setting. Millimetres by default, as PE-DESIGN.
    pub units: super::units::Units,
    /// Real size (#518): points per mm measured with Options ▸ Calibrate Real Size, per monitor
    /// (keyed by its size in points). Without one, 100 % assumes a 96 dpi screen.
    pub screen_calibration: Vec<(String, f64)>,
    /// View ▸ Show (#517, beyond PE-DESIGN); all off by default.
    pub show: ShowPrefs,
    /// Autosave for crash recovery (#549): on by default, writes only to the recovery folder.
    pub autosave: bool,
    /// Longest time between snapshots while changes keep coming, seconds.
    pub autosave_interval: f64,
    /// Command palette (#550): the keys of the commands run from it, newest first.
    pub recent_commands: Vec<String>,
}

/// View ▸ Show: view-only toggles (#517). The design and its export never change.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ShowPrefs {
    pub needle_points: bool,
    pub layers: openstitch_design::view::Layers,
    pub trim_marks: bool,
    pub hide_unselected: bool,
    pub color_by: openstitch_design::view::ColorBy,
    /// Fade every colour block but the current one (the simulator's, else the selection's).
    pub solo: bool,
}

/// View ▸ Snap to Objects and Precision Bar, and the Options menu's nudge and click settings.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct EditingPrefs {
    pub snap_objects: bool,
    pub precision_bar: bool,
    /// A click inside a closed outline that sews no region selects it.
    pub click_inside: bool,
    /// Arrow-key step in mm (Shift: ten times); PE-DESIGN's 0.1 mm by default.
    pub nudge_mm: f64,
    /// While Snap to Grid is on, arrows move one grid step.
    pub nudge_grid: bool,
}

impl EditingPrefs {
    pub const NUDGE_MIN: f64 = 0.1;
    pub const NUDGE_MAX: f64 = 50.0;
}

impl Default for EditingPrefs {
    fn default() -> Self {
        EditingPrefs {
            snap_objects: false,
            precision_bar: false,
            click_inside: false,
            nudge_mm: 0.1,
            nudge_grid: false,
        }
    }
}

impl Default for UiPrefs {
    fn default() -> Self {
        UiPrefs {
            theme: Theme::default(),
            accent: None,
            keymap: Default::default(),
            layouts: Vec::new(),
            qat: QAT_DEFAULT.to_vec(),
            qat_below: false,
            panes: Panes::default(),
            recent: Vec::new(),
            realistic: RealisticSettings::default(),
            font_filter: FontFilter::default(),
            splash: true,
            inkstitch_folder: None,
            updates: UpdatePrefs::default(),
            ai: Default::default(),
            editing: EditingPrefs::default(),
            units: super::units::Units::Mm,
            screen_calibration: Vec::new(),
            show: ShowPrefs::default(),
            autosave: true,
            autosave_interval: super::recovery::DEFAULT_INTERVAL,
            recent_commands: Vec::new(),
        }
    }
}

fn pane_json(p: PaneState) -> Value {
    json!({ "visible": p.visible, "floating": p.floating, "side": p.side.key() })
}

fn pane_from(v: &Value, d: PaneState) -> PaneState {
    PaneState {
        visible: v["visible"].as_bool().unwrap_or(d.visible),
        floating: v["floating"].as_bool().unwrap_or(d.floating),
        side: v["side"]
            .as_str()
            .and_then(Side::from_key)
            .filter(|s| s.fits(d.side))
            .unwrap_or(d.side),
    }
}

fn panes_json(p: &Panes) -> Value {
    json!({
        "sewingOrder": pane_json(p.sewing_order),
        "simulator": pane_json(p.simulator),
        "attributes": pane_json(p.attributes),
    })
}

fn panes_from(v: &Value) -> Panes {
    let d = Panes::default();
    Panes {
        sewing_order: pane_from(&v["sewingOrder"], d.sewing_order),
        simulator: pane_from(&v["simulator"], d.simulator),
        attributes: pane_from(&v["attributes"], d.attributes),
    }
}

/// A layout name as stored: trimmed, 1-40 characters.
pub fn layout_name(name: &str) -> Result<String, String> {
    let n = name.trim();
    if n.is_empty() || n.chars().count() > Layout::NAME_MAX {
        return Err(format!(
            "a layout name has 1-{} characters",
            Layout::NAME_MAX
        ));
    }
    Ok(n.to_string())
}

impl UiPrefs {
    pub fn to_json(&self) -> Value {
        json!({
            "theme": self.theme.key(),
            "accent": self.accent.map(super::accent::to_hex),
            "shortcuts": self.keymap.to_json(),
            "layouts": self.layouts.iter()
                .map(|(n, l)| json!({ "name": n, "panes": panes_json(&l.panes), "quickAccessBelow": l.qat_below }))
                .collect::<Vec<_>>(),
            "quickAccess": self.qat.iter().map(|a| a.info().key).collect::<Vec<_>>(),
            "quickAccessBelow": self.qat_below,
            "panes": panes_json(&self.panes),
            "recent": self.recent.iter().map(|p| p.display().to_string()).collect::<Vec<_>>(),
            "realisticView": {
                "threadWidthMM": self.realistic.thread_width_mm,
                "contrast": self.realistic.contrast,
                "brightness": self.realistic.brightness,
            },
            "fontFilter": { "on": self.font_filter.on, "hidden": self.font_filter.hidden },
            "showSplash": self.splash,
            "inkStitchFolder": self.inkstitch_folder.as_ref().map(|p| p.display().to_string()),
            "updates": self.updates.to_json(),
            "ai": self.ai.to_json(),
            "editing": {
                "snapToObjects": self.editing.snap_objects,
                "precisionBar": self.editing.precision_bar,
                "clickInsideOutlines": self.editing.click_inside,
                "nudgeMM": self.editing.nudge_mm,
                "nudgeByGrid": self.editing.nudge_grid,
            },
            "units": super::units::key(self.units),
            "screenCalibration": self.screen_calibration.iter()
                .map(|(k, v)| (k.clone(), json!(v)))
                .collect::<serde_json::Map<_, _>>(),
            "show": {
                "needlePoints": self.show.needle_points,
                "layers": self.show.layers.as_str(),
                "trimMarks": self.show.trim_marks,
                "hideUnselected": self.show.hide_unselected,
                "colorBy": self.show.color_by.as_str(),
                "soloColor": self.show.solo,
            },
            "autosave": self.autosave,
            "autosaveInterval": self.autosave_interval,
            "recentCommands": self.recent_commands,
        })
    }

    pub fn from_json(v: &Value) -> UiPrefs {
        let d = UiPrefs::default();
        let qat = match v["quickAccess"].as_array() {
            Some(list) => {
                let mut out: Vec<Action> = Vec::new();
                for a in list
                    .iter()
                    .filter_map(|k| k.as_str().and_then(Action::from_key))
                {
                    if !out.contains(&a) {
                        out.push(a);
                    }
                }
                out
            }
            None => d.qat.clone(),
        };
        UiPrefs {
            theme: v["theme"]
                .as_str()
                .and_then(Theme::from_key)
                .unwrap_or(d.theme),
            accent: v["accent"].as_str().and_then(super::accent::parse),
            keymap: super::keymap::Keymap::load(&v["shortcuts"]),
            layouts: v["layouts"]
                .as_array()
                .map(|list| {
                    list.iter()
                        .filter_map(|l| {
                            let layout = Layout {
                                panes: panes_from(&l["panes"]),
                                qat_below: l["quickAccessBelow"].as_bool().unwrap_or(false),
                            };
                            Some((layout_name(l["name"].as_str()?).ok()?, layout))
                        })
                        .take(Layout::MAX)
                        .collect()
                })
                .unwrap_or_default(),
            qat,
            qat_below: v["quickAccessBelow"].as_bool().unwrap_or(d.qat_below),
            panes: panes_from(&v["panes"]),
            recent: v["recent"]
                .as_array()
                .map(|l| {
                    l.iter()
                        .filter_map(Value::as_str)
                        .map(PathBuf::from)
                        .take(RECENT_MAX)
                        .collect()
                })
                .unwrap_or_default(),
            realistic: {
                let r = &v["realisticView"];
                let n = |k: &str, d: f64| r[k].as_f64().unwrap_or(d);
                RealisticSettings {
                    thread_width_mm: n("threadWidthMM", d.realistic.thread_width_mm),
                    contrast: n("contrast", d.realistic.contrast),
                    brightness: n("brightness", d.realistic.brightness),
                }
                .clamped()
            },
            font_filter: FontFilter {
                on: v["fontFilter"]["on"].as_bool().unwrap_or(false),
                hidden: v["fontFilter"]["hidden"]
                    .as_array()
                    .map(|l| {
                        l.iter()
                            .filter_map(Value::as_str)
                            .map(str::to_string)
                            .take(FontFilter::MAX_HIDDEN)
                            .collect()
                    })
                    .unwrap_or_default(),
            },
            splash: v["showSplash"].as_bool().unwrap_or(d.splash),
            inkstitch_folder: v["inkStitchFolder"]
                .as_str()
                .filter(|s| !s.is_empty() && s.len() < 4096)
                .map(PathBuf::from),
            updates: UpdatePrefs::from_json(&v["updates"]),
            ai: super::assistant_chat::AiPrefs::from_json(&v["ai"]),
            editing: {
                let e = &v["editing"];
                let d = d.editing;
                let b = |k: &str, d: bool| e[k].as_bool().unwrap_or(d);
                EditingPrefs {
                    snap_objects: b("snapToObjects", d.snap_objects),
                    precision_bar: b("precisionBar", d.precision_bar),
                    click_inside: b("clickInsideOutlines", d.click_inside),
                    nudge_mm: e["nudgeMM"]
                        .as_f64()
                        .filter(|n| n.is_finite())
                        .map_or(d.nudge_mm, |n| {
                            n.clamp(EditingPrefs::NUDGE_MIN, EditingPrefs::NUDGE_MAX)
                        }),
                    nudge_grid: b("nudgeByGrid", d.nudge_grid),
                }
            },
            units: v["units"]
                .as_str()
                .and_then(super::units::from_key)
                .unwrap_or(d.units),
            screen_calibration: v["screenCalibration"]
                .as_object()
                .map(|m| {
                    m.iter()
                        .filter_map(|(k, v)| {
                            let k2 = v.as_f64()?;
                            super::viewport::CALIBRATION_RANGE
                                .contains(&k2)
                                .then(|| (k.clone(), k2))
                        })
                        .take(16)
                        .collect()
                })
                .unwrap_or_default(),
            show: {
                use openstitch_design::view::{ColorBy, Layers};
                let e = &v["show"];
                let b = |k: &str| e[k].as_bool().unwrap_or(false);
                ShowPrefs {
                    needle_points: b("needlePoints"),
                    layers: e["layers"]
                        .as_str()
                        .and_then(Layers::parse)
                        .unwrap_or_default(),
                    trim_marks: b("trimMarks"),
                    hide_unselected: b("hideUnselected"),
                    color_by: e["colorBy"]
                        .as_str()
                        .and_then(ColorBy::parse)
                        .unwrap_or_default(),
                    solo: b("soloColor"),
                }
            },
            autosave: v["autosave"].as_bool().unwrap_or(d.autosave),
            autosave_interval: v["autosaveInterval"]
                .as_f64()
                .filter(|n| n.is_finite())
                .map_or(d.autosave_interval, |n| {
                    n.clamp(super::recovery::MIN_INTERVAL, super::recovery::MAX_INTERVAL)
                }),
            recent_commands: v["recentCommands"]
                .as_array()
                .map(|l| {
                    l.iter()
                        .filter_map(Value::as_str)
                        .filter(|k| Action::from_key(k).is_some())
                        .map(str::to_string)
                        .take(super::palette::MAX_RECENT)
                        .collect()
                })
                .unwrap_or_default(),
        }
    }

    /// Reads `path`; a missing or unreadable file gives the defaults.
    pub fn load(path: &Path) -> UiPrefs {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok())
            .map_or_else(UiPrefs::default, |v| UiPrefs::from_json(&v))
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let text = serde_json::to_string_pretty(&self.to_json()).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Points per mm at 100 % on `monitor` (its calibration, else a 96 dpi screen).
    pub fn points_per_mm(&self, monitor: Option<&str>) -> f64 {
        monitor
            .and_then(|m| self.screen_calibration.iter().find(|(k, _)| k == m))
            .map_or(super::viewport::DEFAULT_POINTS_PER_MM, |(_, v)| *v)
    }

    /// Stores (or with `None` removes) `monitor`'s calibration.
    pub fn calibrate(&mut self, monitor: &str, points_per_mm: Option<f64>) {
        self.screen_calibration.retain(|(k, _)| k != monitor);
        if let Some(v) = points_per_mm {
            self.screen_calibration.push((monitor.to_string(), v));
            if self.screen_calibration.len() > 16 {
                self.screen_calibration.remove(0);
            }
        }
    }

    /// The current layout.
    pub fn layout(&self) -> Layout {
        Layout {
            panes: self.panes,
            qat_below: self.qat_below,
        }
    }

    /// Saves the current layout as `name` (replacing one of that name).
    pub fn save_layout(&mut self, name: &str) -> Result<String, String> {
        let name = layout_name(name)?;
        let layout = self.layout();
        let full = self.layouts.len() >= Layout::MAX;
        match self.layouts.iter_mut().find(|(n, _)| *n == name) {
            Some(slot) => slot.1 = layout,
            None if full => {
                return Err(format!("at most {} layouts; delete one first", Layout::MAX));
            }
            None => self.layouts.push((name.clone(), layout)),
        }
        Ok(name)
    }

    /// Puts the saved layout `name` in place.
    pub fn apply_layout(&mut self, name: &str) -> Result<(), String> {
        let l = self
            .layouts
            .iter()
            .find(|(n, _)| n == name.trim())
            .map(|(_, l)| *l)
            .ok_or_else(|| format!("no layout '{}'", name.trim()))?;
        self.panes = l.panes;
        self.qat_below = l.qat_below;
        Ok(())
    }

    pub fn delete_layout(&mut self, name: &str) -> Result<(), String> {
        let before = self.layouts.len();
        self.layouts.retain(|(n, _)| n != name.trim());
        if self.layouts.len() == before {
            return Err(format!("no layout '{}'", name.trim()));
        }
        Ok(())
    }

    /// Resets one area of [`RESET_AREAS`] and leaves the others as they are.
    pub fn reset_area(&mut self, area: &str) -> Result<(), String> {
        let d = UiPrefs::default();
        match area {
            "display" => {
                self.theme = d.theme;
                self.accent = d.accent;
                self.qat = d.qat;
                self.qat_below = d.qat_below;
                self.splash = d.splash;
                self.realistic = d.realistic;
                self.show = d.show;
            }
            "editing" => {
                self.editing = d.editing;
                self.units = d.units;
            }
            "panes" => self.panes = d.panes,
            "shortcuts" => self.keymap = d.keymap,
            "layouts" => self.layouts.clear(),
            _ => {
                let keys: Vec<&str> = RESET_AREAS.iter().map(|a| a.0).collect();
                return Err(format!("area takes {}", keys.join(", ")));
            }
        }
        Ok(())
    }

    /// Puts `path` first in the recent files list.
    pub fn add_recent(&mut self, path: &Path) {
        self.recent.retain(|p| p != path);
        self.recent.insert(0, path.to_path_buf());
        self.recent.truncate(RECENT_MAX);
    }

    /// Adds `a` to the Quick Access Toolbar, or removes it.
    pub fn toggle_qat(&mut self, a: Action) {
        if let Some(i) = self.qat.iter().position(|x| *x == a) {
            self.qat.remove(i);
        } else {
            self.qat.push(a);
        }
    }
}

/// `ui.json` beside the user settings file.
pub fn path_beside(user_settings: &Path) -> PathBuf {
    user_settings.with_file_name("ui.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_junk() {
        let mut p = UiPrefs {
            theme: Theme::System,
            qat_below: true,
            ..UiPrefs::default()
        };
        p.toggle_qat(Action::Help);
        p.toggle_qat(Action::Print);
        p.panes.simulator.visible = false;
        p.panes.attributes.floating = true;
        p.realistic.contrast = 40.0;
        p.realistic.thread_width_mm = 0.6;
        p.font_filter.on = true;
        p.font_filter.set_shown("lato-bold", false);
        p.splash = false;
        p.updates.check = false;
        p.updates.skip_version = Some("v0.1.0-alpha.4".into());
        p.units = super::super::units::Units::Inch;
        p.editing = EditingPrefs {
            snap_objects: true,
            precision_bar: true,
            click_inside: true,
            nudge_mm: 0.5,
            nudge_grid: true,
        };
        p.calibrate("1512x982", Some(4.25));
        p.calibrate("1920x1080", Some(3.5));
        p.calibrate("1920x1080", None);
        assert_eq!(p.points_per_mm(Some("1512x982")), 4.25);
        assert_eq!(
            p.points_per_mm(Some("1920x1080")),
            crate::editor::viewport::DEFAULT_POINTS_PER_MM
        );
        for i in 0..12 {
            p.add_recent(Path::new(&format!("/d/{i}.osd")));
        }
        p.add_recent(Path::new("/d/5.osd"));
        assert_eq!(p.recent.len(), RECENT_MAX);
        assert_eq!(p.recent[0], Path::new("/d/5.osd"));
        assert_eq!(p.recent.iter().filter(|r| r.ends_with("5.osd")).count(), 1);
        assert!(!p.qat.contains(&Action::Help) && p.qat.last() == Some(&Action::Print));
        assert_eq!(UiPrefs::from_json(&p.to_json()), p);
        let junk = json!({ "theme": "neon", "quickAccess": ["save", "nope", "save"], "panes": 3 });
        let q = UiPrefs::from_json(&junk);
        assert_eq!(q.theme, Theme::Light);
        assert_eq!(q.qat, [Action::Save]);
        assert_eq!(q.panes, Panes::default());
        assert_eq!(UiPrefs::from_json(&json!(null)), UiPrefs::default());
        let wild = json!({ "realisticView": { "threadWidthMM": 50, "contrast": "x" }, "fontFilter": { "on": 1, "hidden": [3, "a"] } });
        let w = UiPrefs::from_json(&wild);
        assert_eq!(w.editing, EditingPrefs::default(), "off unless chosen");
        assert_eq!(
            w.units,
            super::super::units::Units::Mm,
            "millimetres by default"
        );
        assert!(w.screen_calibration.is_empty(), "96 dpi unless calibrated");
        assert_eq!(
            w.show,
            ShowPrefs::default(),
            "view toggles off unless chosen"
        );
        let shown = ShowPrefs {
            needle_points: true,
            layers: openstitch_design::view::Layers::UnderlayOnly,
            trim_marks: true,
            hide_unselected: true,
            color_by: openstitch_design::view::ColorBy::SewingOrder,
            solo: true,
        };
        let q = UiPrefs {
            show: shown,
            ..UiPrefs::default()
        };
        assert_eq!(UiPrefs::from_json(&q.to_json()).show, shown);
        let e = UiPrefs::from_json(&json!({ "editing": { "nudgeMM": 900, "snapToObjects": "y" } }));
        assert_eq!((e.editing.nudge_mm, e.editing.snap_objects), (50.0, false));
        assert_eq!(w.realistic.thread_width_mm, 1.0);
        assert_eq!(w.realistic.contrast, 0.0);
        assert_eq!(
            w.font_filter,
            FontFilter {
                on: false,
                hidden: vec!["a".into()]
            }
        );
    }

    #[test]
    fn layouts_survive_a_restart_and_areas_reset_alone() {
        let dir = openstitch_testutil::TempDir::new("layouts");
        let path = dir.join("ui.json");
        let mut p = UiPrefs::default();
        p.panes.sewing_order.side = Side::Right;
        p.panes.simulator.floating = true;
        p.qat_below = true;
        assert_eq!(p.save_layout("  Lettering ").as_deref(), Ok("Lettering"));
        p.panes = Panes::default();
        p.save_layout("Digitizing").expect("save");
        assert!(p.save_layout("").is_err() && p.save_layout(&"x".repeat(41)).is_err());
        p.accent = Some([0xef, 0x6c, 0x00]);
        p.keymap.add(Action::Rectangle, "Alt+R").expect("free");
        p.save(&path).expect("save");
        let mut q = UiPrefs::load(&path);
        assert_eq!(
            q, p,
            "layouts, accent and shortcuts come back after a restart"
        );
        q.apply_layout("Lettering").expect("apply");
        assert_eq!(q.panes.sewing_order.side, Side::Right);
        assert!(q.panes.simulator.floating && q.qat_below);
        assert!(q.apply_layout("nope").is_err());
        q.delete_layout("Digitizing").expect("delete");
        assert_eq!(q.layouts.len(), 1);
        // A side that does not fit the pane falls back to its own.
        let odd = json!({ "panes": { "simulator": { "side": "left" } } });
        assert_eq!(UiPrefs::from_json(&odd).panes, Panes::default());
        // Resetting one area leaves the others.
        let before = q.clone();
        q.reset_area("display").expect("display");
        assert_eq!((q.accent, q.qat_below), (None, false));
        assert_eq!(
            (&q.keymap, &q.layouts, q.panes),
            (&before.keymap, &before.layouts, before.panes)
        );
        q.reset_area("shortcuts").expect("shortcuts");
        assert!(q.keymap.is_default() && !q.layouts.is_empty());
        q.reset_area("panes").expect("panes");
        assert_eq!(q.panes, Panes::default());
        q.reset_area("layouts").expect("layouts");
        assert!(q.layouts.is_empty());
        assert!(q.reset_area("everything").is_err());
        for (area, _, _) in RESET_AREAS {
            q.reset_area(area).expect(area);
        }
    }

    #[test]
    fn font_filter_hides_fonts_but_never_all() {
        let font = |id: &str| FontChoice {
            id: id.into(),
            label: id.into(),
            size_range: None,
            group: "text".into(),
        };
        let fonts = vec![font("a"), font("b")];
        let mut f = FontFilter::default();
        f.set_shown("a", false);
        assert_eq!(f.apply(fonts.clone()), fonts, "off: everything shows");
        f.on = true;
        assert_eq!(f.apply(fonts.clone()), [font("b")]);
        assert!(f.shows("b") && !f.shows("a"));
        f.set_shown("b", false);
        assert_eq!(f.apply(fonts.clone()), fonts, "never an empty list");
        f.set_shown("a", true);
        f.set_shown("a", true);
        assert_eq!(f.hidden, ["b"]);
    }

    #[test]
    fn saves_beside_the_user_settings() {
        let dir = openstitch_testutil::TempDir::new("ui");
        let path = path_beside(&dir.join("user.json"));
        assert!(path.ends_with("ui.json"));
        assert_eq!(UiPrefs::load(&path), UiPrefs::default());
        let p = UiPrefs {
            theme: Theme::Dark,
            ..UiPrefs::default()
        };
        p.save(&path).expect("save");
        assert_eq!(UiPrefs::load(&path), p);
        let _ = std::fs::remove_dir_all(dir);
    }
}
