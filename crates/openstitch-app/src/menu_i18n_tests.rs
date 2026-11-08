//! The interface language reaches every menu entry (#555): with the pseudo-locale (every
//! translated text in brackets), each entry of every ribbon menu shows bracketed text, so none is
//! hard-coded English. A child module of `app`, as the screen-reader audit is.

use super::*;
use eframe::egui::accesskit::Role;

/// Entry roles the menus draw.
const ENTRIES: &[Role] = &[
    Role::Button,
    Role::MenuItem,
    Role::MenuItemCheckBox,
    Role::MenuItemRadio,
    Role::CheckBox,
    Role::RadioButton,
    Role::ComboBox,
];

/// A label that is not words to translate: a file format, a number with its unit, a code.
fn untranslatable(label: &str) -> bool {
    let words: String = label
        .chars()
        .filter(|c| c.is_alphabetic())
        .collect();
    words.is_empty()
        || words.chars().all(|c| c.is_uppercase())
        || ["mm", "px", "in"].iter().any(|u| label.trim_end().ends_with(u) && label.chars().any(|c| c.is_ascii_digit()))
}

#[test]
fn the_pseudo_locale_reaches_every_menu_entry() {
    use openstitch_app::editor::i18n;
    use openstitch_app::editor::ribbon::Action;
    let mut app = EditorApp::new(Launch {
        demo: true,
        ..Launch::default()
    });
    assert!(i18n::set_language(i18n::PSEUDO));
    let ctx = egui::Context::default();
    ctx.enable_accesskit();
    let mut nodes = Vec::new();
    for _ in 0..2 {
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1440.0, 20_000.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for &a in Action::ALL {
                    ui.push_id(format!("{a:?}"), |ui| crate::ribbon::menu(&mut app, a, ui));
                }
            });
        });
        out.textures_delta.clear();
        if let Some(update) = out.platform_output.accesskit_update {
            nodes = update.nodes;
        }
    }
    assert!(i18n::set_language(i18n::ENGLISH));
    // Names, not words: fonts and thread charts keep their own names in every language.
    let mut names: Vec<String> = openstitch_engine::thread_match::builtins()
        .iter()
        .map(|b| b.name.to_string())
        .collect();
    if let Some(list) = app.execute(&CommandRequest::new("fonts.list", serde_json::json!({}))) {
        names.extend(
            openstitch_app::editor::lettering::font_choices(&list)
                .into_iter()
                .map(|c| c.label),
        );
    }
    let mut english: Vec<String> = nodes
        .iter()
        .filter(|(_, n)| ENTRIES.contains(&n.role()))
        .filter_map(|(_, n)| n.label().map(str::to_string))
        .filter(|l| !l.trim().is_empty() && !l.contains('[') && !untranslatable(l))
        .filter(|l| !names.iter().any(|n| n == l))
        .collect();
    english.sort();
    english.dedup();
    assert!(
        english.is_empty(),
        "{} menu entries are not translated:\n{}",
        english.len(),
        english.join("\n")
    );
}

/// Menu entries kept in tables, submenus included (the frame above cannot open a submenu): each
/// label has a catalogue entry, so `ui_texts::tr` translates it where it is drawn.
#[test]
fn every_menu_table_label_has_a_catalogue_entry() {
    use openstitch_app::editor::ui_texts::has_entry;
    let mut labels: Vec<&str> = Vec::new();
    labels.extend(EditorApp::PES_VERSIONS.iter().map(|(_, l)| *l));
    labels.extend(EditorApp::THREAD_LISTS.iter().map(|(_, l)| *l));
    labels.extend(EditorApp::MOCKUPS.iter().map(|(_, _, l)| *l));
    labels.extend(openstitch_app::editor::prefs::RESET_AREAS.iter().map(|(_, l, _)| *l));
    labels.extend(openstitch_app::editor::new_objects::FIELDS.iter().map(|f| f.label));
    labels.extend(openstitch_app::editor::edit_ops::PRESETS.iter().map(|(l, _)| *l));
    labels.extend(openstitch_app::editor::edit_ops::FRAMES.iter().map(|(l, _)| *l));
    labels.extend(openstitch_app::editor::edit_ops::EMBOSS.iter().map(|(l, _)| *l));
    labels.extend(openstitch_design::stamp::PATTERNS);
    labels.extend(openstitch_design::effects::Effect::ALL.iter().map(|e| e.label()));
    labels.extend(crate::arrange_ui::ALIGN_TO.iter().map(|(_, l)| *l));
    labels.extend(openstitch_app::editor::prefs::Theme::ALL.iter().map(|t| t.label()));
    labels.extend(openstitch_app::editor::inspector::line_types().into_iter().map(|(_, l)| l));
    labels.extend(openstitch_app::editor::inspector::region_types().into_iter().map(|(_, l)| l));
    let grid: Vec<String> = [3.175, 6.35, 12.7, 25.4]
        .into_iter()
        .map(openstitch_app::editor::units::grid_label)
        .collect();
    labels.extend(grid.iter().map(String::as_str));
    let mut missing: Vec<&str> = labels
        .into_iter()
        .filter(|l| !has_entry(l) && !untranslatable(l))
        .collect();
    missing.sort_unstable();
    missing.dedup();
    assert!(missing.is_empty(), "no catalogue entry:
{}", missing.join("
"));
}
