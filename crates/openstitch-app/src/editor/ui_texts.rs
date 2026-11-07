//! The window's texts outside the ribbon model, by message id (#555): the menus of the
//! Application button, Options, Help and File ▸ Export (`menu-…`, part 2), and the panes and
//! dialogs (`ui-…`, part 3). [`t`] gives a text in this
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
    (
        "ui-hide-view-show-hide-shows-it-tip",
        "Hide (View ▸ Show/Hide shows it again)",
    ),
    ("ui-dock-to-the-window-tip", "Dock to the window"),
    ("ui-hide-tip", "Hide"),
    ("ui-float-as-a-window-tip", "Float as a window"),
    (
        "ui-add-designs-artwork-or-a-lettering",
        "Add designs, artwork or a lettering template to the Design Page.",
    ),
    ("ui-template-wizard", "Template Wizard"),
    ("ui-import-a-recent-file", "Import a recent file"),
    ("ui-selection", "Selection"),
    (
        "ui-thread-colours-in-sewing-order",
        "Thread colours in sewing order",
    ),
    ("ui-sort-by-colour", "Sort by colour"),
    (
        "ui-same-colour-objects-sew-together-tip",
        "Same-colour objects sew together",
    ),
    (
        "ui-select-a-text-object-to-change",
        "Select a text object to change its attributes.",
    ),
    ("ui-text", "Text"),
    ("ui-apply-text", "Apply text"),
    ("ui-font", "Font"),
    ("ui-size", "Size"),
    ("ui-character-spacing", "Character Spacing"),
    ("ui-line-spacing", "Line Spacing"),
    ("ui-pixels", "Pixels"),
    ("ui-design-size-tip", "Design size"),
    ("ui-zoom-tip", "Zoom"),
    (
        "ui-magnification-type-a-value-or-drag-tip",
        "Magnification: type a value or drag",
    ),
    ("ui-revert", "Revert"),
    ("ui-openstitch", "OpenStitch"),
    (
        "ui-an-open-source-embroidery-design-suite",
        "An open-source embroidery design suite for macOS, Windows and Linux.",
    ),
    (
        "ui-reads-and-writes-pes-pec-dst",
        "Reads and writes PES, PEC, DST, EXP, JEF, VP3 and more.",
    ),
    (
        "ui-which-machine-do-you-sew-on",
        "Which machine do you sew on? OpenStitch starts new designs on its hoop and warns when an export has stitches it cannot sew or trims it cannot cut.",
    ),
    ("ui-brand", "Brand"),
    ("ui-hoop", "Hoop"),
    (
        "ui-the-machine-cuts-threads-thread-cutter",
        "The machine cuts threads (thread cutter)",
    ),
    ("ui-longest-stitch", "Longest stitch"),
    ("ui-from-the-manual", "From the manual:"),
    (
        "ui-leave-it-off-when-the-manual-tip",
        "Leave it off when the manual does not say; OpenStitch then checks only the file format's own limit",
    ),
    ("ui-save", "Save"),
    ("ui-not-now", "Not now"),
    (
        "ui-help-machine-setup-asks-again-tip",
        "Help ▸ Machine Setup asks again",
    ),
    ("ui-most-recently-used-files", "Most Recently Used Files"),
    ("ui-no-recent-files", "No recent files"),
    ("ui-open", "Open"),
    ("ui-close-this-wizard", "Close This Wizard"),
    ("ui-to-file", "to File…"),
    ("ui-to-clipboard", "to Clipboard"),
    (
        "ui-no-background-image-use-image-open",
        "No background image. Use Image ▸ Open Image first.",
    ),
    ("ui-position", "Position"),
    ("ui-width", "Width"),
    ("ui-reset", "Reset"),
    (
        "ui-no-crop-original-colours-tip",
        "No crop, original colours",
    ),
    ("ui-thread-width", "Thread width"),
    ("ui-contrast", "Contrast"),
    ("ui-brightness", "Brightness"),
    ("ui-default", "Default"),
    ("ui-show-realistic-view", "Show Realistic View"),
    (
        "ui-tick-the-fonts-to-show-clear",
        "Tick the fonts to show; clear the ones to hide.",
    ),
    ("ui-show-every-font-tip", "Show every font"),
    ("ui-ok", "OK"),
    (
        "ui-beyond-pe-design-a-thinner-thread-tip",
        "Beyond PE-DESIGN: a thinner thread (higher number) sews rows closer, density times the square root of (weight / 40); No. 40 keeps the fabric's values",
    ),
    (
        "ui-apply-to-all-shape-and-text",
        "Apply to all shape and text patterns on the Design Page",
    ),
    (
        "ui-new-shapes-and-texts-start-from",
        "New shapes and texts start from this fabric",
    ),
    (
        "ui-beyond-pe-design-shapes-and-texts-tip",
        "Beyond PE-DESIGN: shapes and texts drawn afterwards get these settings (fabric.setDefault); existing ones change only through OK above",
    ),
    ("ui-needle-points", "Needle Points"),
    (
        "ui-a-dot-at-every-needle-drop-tip",
        "A dot at every needle drop",
    ),
    ("ui-trim-marks", "Trim Marks"),
    ("ui-hide-unselected", "Hide Unselected"),
    (
        "ui-only-the-selected-objects-stitches-tip",
        "Only the selected objects' stitches",
    ),
    ("ui-colour-by", "Colour by"),
    ("ui-solo-colour", "Solo Colour"),
    ("ui-paper", "Paper"),
    (
        "ui-sewing-area-box-and-centre-axes",
        "Sewing area box and centre axes",
    ),
    (
        "ui-embroidery-positioning-mark",
        "Embroidery positioning mark",
    ),
    ("ui-template-grid-10-mm", "Template grid (10 mm)"),
    (
        "ui-mirror-the-image-iron-on-transfer",
        "Mirror the image (iron-on transfer sheets)",
    ),
    (
        "ui-open-the-system-print-dialog-or-tip",
        "Open the system print dialog (or the PDF viewer)",
    ),
    ("ui-save-pdf", "Save PDF…"),
    ("ui-no-saved-sets-yet", "No saved sets yet."),
    (
        "ui-apply-to-the-selected-objects-their-tip",
        "Apply to the selected objects (their colours stay)",
    ),
    ("ui-delete", "Delete"),
    ("ui-save-selected", "Save selected"),
    (
        "ui-save-the-selected-object-s-sewing-tip",
        "Save the selected object's sewing attributes under this name",
    ),
    ("ui-match-to-thread-chart", "Match to thread chart…"),
    ("ui-merge-similar-colours", "Merge similar colours…"),
    (
        "ui-join-near-identical-thread-colours-to-tip",
        "Join near-identical thread colours to save thread changes",
    ),
    ("ui-new", "New"),
    ("ui-import-palette", "Import palette…"),
    (
        "ui-a-gimp-or-inkscape-palette-gpl-tip",
        "A GIMP or Inkscape palette (.gpl) as a new chart",
    ),
    ("ui-rename", "Rename"),
    ("ui-move-up-tip", "Move up"),
    ("ui-move-down-tip", "Move down"),
    ("ui-remove", "Remove"),
    ("ui-colour", "Colour"),
    ("ui-code", "Code"),
    ("ui-description", "Description"),
    ("ui-add-colour", "Add colour"),
    ("ui-import-csv", "Import CSV…"),
    ("ui-export-palette", "Export palette…"),
    ("ui-export-csv", "Export CSV…"),
    ("ui-remap-design", "Remap design"),
    (
        "ui-change-every-thread-to-the-nearest-tip",
        "Change every thread to the nearest colour of this chart",
    ),
    ("ui-match", "Match…"),
    (
        "ui-preview-the-match-to-this-chart-tip",
        "Preview the match to this chart before it applies",
    ),
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
            assert!(
                (id.starts_with("menu-") || id.starts_with("ui-")) && !text.is_empty(),
                "{id}"
            );
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
                    let calls_at = src
                        .match_indices("t(\"menu-")
                        .chain(src.match_indices("t(\"ui-"))
                        .map(|(i, _)| i)
                        .collect::<Vec<_>>();
                    for i in calls_at {
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
