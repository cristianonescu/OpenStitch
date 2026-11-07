//! The window's texts outside the ribbon model, by message id (#555 part 2): the menus of the
//! Application button, Options, Help and File ▸ Export so far. [`t`] gives a text in this
//! thread's language through [`super::i18n`], English (this table) when the language has none;
//! `assets/i18n/en.ftl` lists them after the ribbon's messages. Ids never reach the screen: a
//! test checks that every `t("…")` in the app's source is in [`UI_TEXTS`].

/// Message id and English text.
pub const UI_TEXTS: &[(&str, &str)] = &[
    (
        "menu-optimize-entry-exit-points",
        "Optimize Entry/Exit Points",
    ),
    (
        "menu-new-shapes-are-entered-where-they-tip",
        "New shapes are entered where they come closest to the shape sewn before",
    ),
    ("menu-theme", "Theme"),
    ("menu-layouts", "Layouts"),
    ("menu-reset-preferences", "Reset Preferences"),
    ("menu-show-splash-at-startup", "Show splash at startup"),
    ("menu-text-size", "Text size"),
    (
        "menu-beyond-pe-design-everything-in-the-tip",
        "Beyond PE-DESIGN: everything in the window larger, for easier reading",
    ),
    ("menu-language", "Language"),
    (
        "menu-the-interface-language-english-is-the-tip",
        "The interface language; English is the source and fills in any text a translation lacks",
    ),
    ("menu-show-start-cards", "Show start cards"),
    (
        "menu-an-empty-design-offers-cards-to-tip",
        "An empty design offers cards to draw, letter, digitize a picture, import, use a template or open a sample",
    ),
    ("menu-new-objects", "New Objects"),
    ("menu-open-designs-in-tabs", "Open designs in tabs"),
    (
        "menu-beyond-pe-design-new-and-open-tip",
        "Beyond PE-DESIGN: New and Open add a tab and keep the current design open (Ctrl+Tab switches); off, they replace it as PE-DESIGN does",
    ),
    (
        "menu-picture-pickers-with-live-preview",
        "Picture pickers with live preview",
    ),
    (
        "menu-beyond-pe-design-sewing-types-fill-tip",
        "Beyond PE-DESIGN: sewing types, fill patterns and motifs as pictures of their stitches; hovering one shows it on the selection before you choose",
    ),
    (
        "menu-autosave-for-crash-recovery",
        "Autosave for crash recovery",
    ),
    (
        "menu-beyond-pe-design-keeps-a-copy-tip",
        "Beyond PE-DESIGN: keeps a copy of each open design in OpenStitch's settings folder (never the design's own file) and offers it at the next start after a crash or power cut",
    ),
    ("menu-autosave-at-most-every", "Autosave at most every"),
    ("menu-clear-recovery-files", "Clear Recovery Files"),
    (
        "menu-delete-the-copies-left-by-windows-tip",
        "Delete the copies left by windows that crashed",
    ),
    ("menu-show-ai-features", "Show AI features"),
    ("menu-units", "Units"),
    ("menu-millimetres", "Millimetres"),
    ("menu-inches", "Inches"),
    ("menu-editing", "Editing"),
    ("menu-nudge-step", "Nudge step"),
    (
        "menu-how-far-an-arrow-key-moves-tip",
        "How far an arrow key moves the selection; Shift moves ten times as far",
    ),
    (
        "menu-nudge-by-the-grid-step-while",
        "Nudge by the grid step while snapping",
    ),
    (
        "menu-with-snap-to-grid-on-an-tip",
        "With Snap to Grid on, an arrow key moves one grid step",
    ),
    (
        "menu-click-inside-outlines-selects-them",
        "Click inside outlines selects them",
    ),
    ("menu-updates", "Updates"),
    (
        "menu-asks-github-for-a-newer-openstitch-tip",
        "Asks GitHub for a newer OpenStitch release each time the editor starts",
    ),
    (
        "menu-also-offer-alpha-beta-and-release-tip",
        "Also offer alpha, beta and release-candidate versions",
    ),
    ("menu-check-for-updates-now", "Check for Updates Now"),
    ("menu-instruction-manual", "Instruction Manual"),
    ("menu-ready-to-sew-designs", "Ready-to-Sew Designs"),
    (
        "menu-calibration-designs-machine-checks-and-a-tip",
        "Calibration designs, machine checks and a design for each feature, ready for the machine",
    ),
    (
        "menu-ready-to-sew-designs-online",
        "Ready-to-Sew Designs Online",
    ),
    ("menu-getting-started", "Getting Started"),
    (
        "menu-your-first-design-step-by-step-tip",
        "Your first design, step by step, with a button for each step",
    ),
    ("menu-machine-setup", "Machine Setup"),
    (
        "menu-beyond-pe-design-your-machine-s-tip",
        "Beyond PE-DESIGN: your machine's brand, hoop and thread cutter, for new designs and export checks",
    ),
    ("menu-keyboard-shortcuts", "Keyboard Shortcuts"),
    ("menu-check-for-updates", "Check for Updates"),
    ("menu-about-openstitch", "About OpenStitch"),
    ("menu-pes", "PES"),
    ("menu-exp", "EXP"),
    ("menu-exp-2", "EXP…"),
    (
        "menu-exp-with-inf-colour-file",
        "EXP with .inf colour file…",
    ),
    ("menu-garment-mockup", "Garment Mockup"),
    (
        "menu-beyond-pe-design-the-design-at-tip",
        "Beyond PE-DESIGN: the design at true size on a T-shirt, cap or tote bag in a fabric colour, as a PNG to show a customer (design.mockup)",
    ),
    ("menu-thread-list", "Thread List"),
    (
        "menu-apply-thread-list-to-the-design",
        "Apply Thread List to the Design…",
    ),
    (
        "menu-give-the-colour-blocks-in-sewing-tip",
        "Give the colour blocks, in sewing order, the colours of a CSV, INF, EDR, COL or TXT list (one undo step)",
    ),
    ("menu-print", "Print…"),
    ("menu-startup-wizard", "Startup Wizard…"),
    (
        "menu-pe-design-s-start-up-dialog-tip",
        "PE-DESIGN's start-up dialog: templates, images, hoop and fabric, import, recent files",
    ),
    ("menu-template-wizard", "Template Wizard…"),
    ("menu-image-to-embroidery", "Image to Embroidery…"),
    ("menu-send-to-usb-media", "Send to USB Media…"),
    ("menu-export", "Export"),
    ("menu-insert-preset-shape", "Insert Preset Shape"),
    ("menu-fit-design-page", "Fit Design Page"),
    ("menu-zoom-to-selection", "Zoom to Selection"),
    ("menu-zoom-box", "Zoom Box"),
    (
        "menu-drag-a-rectangle-on-the-canvas-tip",
        "Drag a rectangle on the canvas to zoom to it; Esc cancels",
    ),
    ("menu-calibrate-real-size", "Calibrate Real Size…"),
    (
        "menu-make-100-real-size-on-this-tip",
        "Make 100 % real size on this screen with a ruler",
    ),
    ("menu-fit-text-to-outline", "Fit Text to Outline…"),
    (
        "menu-select-one-text-and-one-shape-tip",
        "Select one text and one shape",
    ),
    (
        "menu-release-text-from-outline",
        "Release Text from Outline",
    ),
    ("menu-center-on-design-page", "Center on Design Page"),
    ("menu-flip-horizontal", "Flip Horizontal"),
    ("menu-flip-vertical", "Flip Vertical"),
    ("menu-apply", "Apply"),
    ("menu-fabric-colour", "Fabric colour"),
    ("menu-accent-colour", "Accent colour"),
    ("menu-theme-default", "Theme default"),
    ("menu-custom", "Custom"),
    (
        "menu-put-this-layout-in-place-tip",
        "Put this layout in place",
    ),
    ("menu-save-layout", "Save Layout"),
    (
        "menu-keep-where-the-panes-are-and-tip",
        "Keep where the panes are and the toolbar position under this name",
    ),
    ("menu-new-shapes-start-with", "New shapes start with"),
    ("menu-tool-s-own", "tool's own"),
    ("menu-underlay", "Underlay"),
    ("menu-on", "on"),
    ("menu-clear", "Clear"),
    (
        "menu-new-shapes-start-as-the-tools-tip",
        "New shapes start as the tools make them (PE-DESIGN's defaults)",
    ),
    ("menu-recent-files", "Recent Files"),
    ("menu-options", "Options"),
    ("menu-exit", "Exit"),
    ("menu-no-files-opened-yet", "No files opened yet"),
];

/// The English text of message `id`.
pub fn english(id: &str) -> Option<&'static str> {
    UI_TEXTS.iter().find(|(i, _)| *i == id).map(|(_, e)| *e)
}

/// Message `id` in this thread's language. An unknown id (a bug the tests catch) shows as
/// itself rather than nothing.
pub fn t(id: &'static str) -> &'static str {
    match english(id) {
        Some(e) => super::i18n::text(id, e),
        None => id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_are_unique_message_ids_with_text() {
        let mut ids: Vec<_> = UI_TEXTS.iter().map(|(i, _)| *i).collect();
        let n = ids.len();
        ids.sort_unstable();
        ids.dedup();
        assert_eq!(ids.len(), n, "duplicate id");
        for (id, text) in UI_TEXTS {
            assert!(id.starts_with("menu-") && !text.is_empty(), "{id}");
            let line = format!("{id} = {}", super::super::i18n::escape(text));
            assert_eq!(super::super::i18n::parse(&line).expect(id)[0].1, *text);
        }
    }

    /// Every `t("…")` in the app's source names a text of this table.
    #[test]
    fn every_t_call_names_a_known_text() {
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![dir];
        let mut calls = 0;
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).expect("src").flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                } else if p.extension().is_some_and(|x| x == "rs") {
                    let src = std::fs::read_to_string(&p).expect("read");
                    for (i, _) in src.match_indices("t(\"menu-") {
                        // A call, not part of another name (`set_t(`, `split(`…).
                        let before = src[..i].chars().last();
                        if before.is_some_and(|c| c.is_alphanumeric() || c == '_') {
                            continue;
                        }
                        let rest = &src[i + 3..];
                        let id = &rest[..rest.find('"').expect("quote")];
                        assert!(
                            english(id).is_some(),
                            "{}: unknown text id {id}",
                            p.display()
                        );
                        calls += 1;
                    }
                }
            }
        }
        assert!(calls >= UI_TEXTS.len(), "{calls} calls");
    }
}
