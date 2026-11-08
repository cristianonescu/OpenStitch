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
    ("menu-ruler-origin", "Ruler origin"),
    ("menu-snap-to-guides", "Snap to guides"),
    (
        "menu-snap-to-guides-tip",
        "A dragged selection snaps to the guidelines while they show; hold Ctrl (Cmd on macOS) to drag freely",
    ),
    ("menu-ruler-origin-centre", "Page centre"),
    ("menu-ruler-origin-top-left", "Top-left corner"),
    (
        "menu-ruler-origin-tip",
        "Where the rulers, the cursor position and the precision bar's X and Y count from",
    ),
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
    ("ui-horizontal-alignment", "Horizontal Alignment"),
    ("ui-vertical-alignment", "Vertical Alignment"),
    ("ui-above", "Above"),
    (
        "ui-the-text-stands-on-the-outline-tip",
        "The text stands on the outline",
    ),
    ("ui-centered", "Centered"),
    (
        "ui-the-text-is-centred-on-the-tip",
        "The text is centred on the outline",
    ),
    ("ui-below", "Below"),
    (
        "ui-the-text-hangs-below-the-outline-tip",
        "The text hangs below the outline",
    ),
    ("ui-text-orientation", "Text Orientation"),
    ("ui-along-the-outline", "Along the outline"),
    ("ui-upright", "Upright"),
    ("ui-the-other-side", "The other side"),
    (
        "ui-run-on-the-other-side-of-tip",
        "Run on the other side of the outline, reading the other way",
    ),
    ("ui-cancel", "Cancel"),
    ("ui-text-2", "Text…"),
    ("ui-monogram", "Monogram…"),
    ("ui-name-drop-list", "Name Drop List…"),
    ("ui-save-as-template", "Save as Template…"),
    ("ui-fonts", "Fonts…"),
    ("ui-load-truetype-font", "Load TrueType Font…"),
    ("ui-fonts-in-this-design", "Fonts in this design"),
    (
        "ui-open-openstitch-font-osfont",
        "Open OpenStitch font (.osfont)…",
    ),
    ("ui-new-embroidery-font", "New embroidery font"),
    ("ui-id", "Id"),
    ("ui-standard-character-height", "Standard character height"),
    ("ui-create", "Create"),
    ("ui-user-mapped-characters", "User-mapped characters"),
    ("ui-character", "Character"),
    ("ui-register-selected-object", "Register selected object"),
    (
        "ui-the-selected-object-s-stitches-become-tip",
        "The selected object's stitches become this character",
    ),
    ("ui-register-stitch-files", "Register stitch files…"),
    (
        "ui-file-names-give-the-characters-u-tip",
        "File names give the characters: u_A, l_a, s_qm, a single character or U+XXXX",
    ),
    ("ui-font-creator", "Font Creator"),
    ("ui-from-truetype", "From TrueType"),
    ("ui-convert-characters", "Convert characters"),
    ("ui-draw-at-set-line", "Draw at set line"),
    ("ui-baseline", "baseline"),
    ("ui-edit-character-as-shape", "Edit character as shape"),
    (
        "ui-puts-the-character-s-outline-on-tip",
        "Puts the character's outline on the Design Page",
    ),
    (
        "ui-store-selection-as-character",
        "Store selection as character",
    ),
    (
        "ui-the-selected-outlines-measured-from-the-tip",
        "The selected outlines, measured from the set line and baseline above, become the character",
    ),
    ("ui-save-font-as", "Save font as…"),
    ("ui-characters-from-image", "Characters from image"),
    ("ui-choose-image", "Choose image…"),
    ("ui-use-background", "Use background"),
    (
        "ui-read-the-background-image-as-it-tip",
        "Read the background image as it is shown on the Design Page",
    ),
    ("ui-split-into-characters", "Split into characters"),
    (
        "ui-whole-image-is-the-character",
        "Whole image is the character",
    ),
    ("ui-ink-darker-than", "Ink darker than"),
    (
        "ui-use-the-background-image-where-it-tip",
        "Use the background image where it lies instead of fitting it to the standard height",
    ),
    ("ui-assign-in-order", "Assign in order"),
    ("ui-find-characters", "Find characters"),
    ("ui-create-characters", "Create characters"),
    ("ui-align", "Align"),
    ("ui-kerning", "Kerning"),
    (
        "ui-use-the-truetype-font-s-kerning-tip",
        "Use the TrueType font's kerning pairs",
    ),
    ("ui-vertical", "Vertical"),
    (
        "ui-characters-top-to-bottom-lines-as-tip",
        "Characters top to bottom, lines as columns right to left",
    ),
    ("ui-add", "Add"),
    ("ui-letters", "Letters"),
    (
        "ui-1-3-letters-or-digits-upper-tip",
        "1-3 letters or digits; upper-cased",
    ),
    ("ui-style", "Style"),
    ("ui-height", "Height"),
    ("ui-frame", "Frame"),
    ("ui-margin", "Margin"),
    ("ui-maintain-aspect-ratio", "Maintain aspect ratio"),
    (
        "ui-apply-to-selected-monogram",
        "Apply to selected monogram",
    ),
    ("ui-frame-the-selection", "Frame the selection"),
    (
        "ui-decorative-frame-around-the-selected-objects-tip",
        "Decorative frame around the selected objects",
    ),
    ("ui-name-drop-attribute", "Name Drop attribute"),
    ("ui-apply-to-selected-text", "Apply to selected text"),
    ("ui-remove-from-selected-text", "Remove from selected text"),
    ("ui-add-a-row-500-at-most-tip", "Add a row (500 at most)"),
    ("ui-insert", "Insert"),
    ("ui-save-as-csv", "Save as CSV…"),
    ("ui-use-selected-row", "Use selected row"),
    (
        "ui-put-the-row-s-text-into-tip",
        "Put the row's text into the design",
    ),
    ("ui-file-name", "File name"),
    ("ui-to-folder", "To Folder…"),
    (
        "ui-one-file-per-row-name001-name002-tip",
        "One file per row: name001, name002, …",
    ),
    ("ui-step-2-edit-the-text", "Step 2 · Edit the text"),
    ("ui-back", "Back"),
    ("ui-apply-and-continue", "Apply and continue"),
    ("ui-step-3-output", "Step 3 · Output"),
    ("ui-edit-in-design-page", "Edit in Design Page"),
    ("ui-export-pes", "Export PES…"),
    (
        "ui-create-new-embroidery-design",
        "Create new embroidery design",
    ),
    ("ui-step-1-choose-a-template", "Step 1 · Choose a template"),
    ("ui-template-folder", "Template folder…"),
    ("ui-next", "Next"),
    (
        "ui-pick-point-edit-then-click-a",
        "Pick Point Edit, then click a character of the selected text.",
    ),
    ("ui-point-edit", "Point Edit"),
    ("ui-reset-character", "Reset character"),
    ("ui-fit-page", "Fit page"),
    ("ui-mode", "Mode"),
    ("ui-mono", "Mono"),
    ("ui-colours", "Colours"),
    ("ui-smallest-region", "Smallest region"),
    ("ui-satin-up-to", "Satin up to"),
    ("ui-cell-size", "Cell size"),
    ("ui-stitch", "Stitch"),
    ("ui-full-cross", "Full cross"),
    ("ui-half-cross", "Half cross"),
    ("ui-row-spacing", "Row spacing"),
    ("ui-line-threshold", "Line threshold"),
    ("ui-lines", "Lines"),
    ("ui-regions", "Regions"),
    ("ui-isolate-the-subject", "Isolate the subject"),
    ("ui-mark-subject", "Mark subject…"),
    ("ui-sew-the-background", "Sew the background"),
    (
        "ui-leave-out-the-background-colour",
        "Leave out the background colour",
    ),
    ("ui-method", "Method"),
    ("ui-pe-design-compatible", "PE-DESIGN-compatible"),
    (
        "ui-converts-as-pe-design-11-50-tip",
        "Converts as PE-DESIGN 11.50 does (RGB colour reduction and thread matching).",
    ),
    ("ui-fidelity", "Fidelity"),
    ("ui-standard", "Standard"),
    (
        "ui-today-s-colour-counts-and-speed-tip",
        "Today's colour counts and speed.",
    ),
    ("ui-high", "High"),
    ("ui-fit-curves", "Fit curves"),
    ("ui-convert", "Convert"),
    ("ui-fix-on-the-picture", "Fix on the picture…"),
    (
        "ui-click-a-part-of-the-picture-tip",
        "Click a part of the picture to leave it out or put it back.",
    ),
    ("ui-earlier", "Earlier"),
    ("ui-later", "Later"),
    ("ui-pe-design-s-order", "PE-DESIGN's order"),
    (
        "ui-the-image-could-not-be-read",
        "The image could not be read.",
    ),
    ("ui-subject", "Subject"),
    ("ui-background", "Background"),
    ("ui-brush", "Brush"),
    ("ui-undo-stroke", "Undo stroke"),
    ("ui-preset", "Preset"),
    (
        "ui-vary-angles-of-touching-fills",
        "Vary angles of touching fills",
    ),
    ("ui-fill-each-cell", "Fill each cell"),
    (
        "ui-parallel-diagonals-about-0-3-mm-tip",
        "Parallel diagonals about 0.3 mm apart instead of one cross.",
    ),
    ("ui-part-crosses-on-edges", "Part crosses on edges"),
    (
        "ui-automatic-threshold-and-lines",
        "Automatic threshold and lines",
    ),
    ("ui-trims", "Trims"),
    ("ui-fewer", "Fewer"),
    ("ui-all", "All"),
    (
        "ui-every-jump-trimmed-the-best-likeness-tip",
        "Every jump trimmed: the best likeness.",
    ),
    ("ui-threads", "Threads"),
    ("ui-i-own", "I own"),
    (
        "ui-one-of-your-thread-charts-threads-tip",
        "One of your thread charts: threads chosen outside it are listed to buy.",
    ),
    ("ui-review-sewing-order", "Review sewing order"),
    (
        "ui-step-through-every-move-between-objects-tip",
        "Step through every move between objects, with trims, jumps and warnings",
    ),
    ("ui-frames-by-thread-colour", "Frames by thread colour"),
    (
        "ui-one-frame-per-run-of-objects-tip",
        "One frame per run of objects in the same colour",
    ),
    (
        "ui-visible-hidden-objects-are-not-sewn-tip",
        "Visible (hidden objects are not sewn)",
    ),
    (
        "ui-drag-to-change-the-sewing-order-tip",
        "Drag to change the sewing order",
    ),
    ("ui-brother-pec-thread-chart", "Brother PEC thread chart"),
    (
        "ui-no-thread-in-this-chart-matches",
        "No thread in this chart matches.",
    ),
    ("ui-stitch-table", "Stitch Table"),
    ("ui-clear-search", "Clear search"),
    ("ui-basic", "Basic"),
    (
        "ui-only-the-settings-most-designs-change-tip",
        "Only the settings most designs change",
    ),
    ("ui-advanced", "Advanced"),
    (
        "ui-every-sewing-setting-as-pe-design-tip",
        "Every sewing setting (as PE-DESIGN shows them)",
    ),
    ("ui-save-as-default", "Save as Default"),
    (
        "ui-new-shapes-start-with-this-shape-tip",
        "New shapes start with this shape's fill angle, density, stitch length, pull compensation, underlay and outline stitch length (Options ▸ New Objects)",
    ),
    ("ui-design", "Design"),
    ("ui-design-page", "Design Page"),
    ("ui-objects", "Objects"),
    ("ui-machine", "Machine"),
    ("ui-estimate", "Estimate"),
    ("ui-design-settings", "Design Settings…"),
    (
        "ui-select-an-object-to-edit-its",
        "Select an object to edit its sewing attributes.",
    ),
    ("ui-transform", "Transform"),
    ("ui-rotate-90", "Rotate −90°"),
    ("ui-rotate-90-2", "Rotate +90°"),
    ("ui-flip-h", "Flip H"),
    ("ui-flip-v", "Flip V"),
    ("ui-centre", "Centre"),
    ("ui-scale", "Scale"),
    ("ui-folded-click-to-show", "  (folded: click to show)"),
    (
        "ui-fold-or-unfold-this-section-tip",
        "Fold or unfold this section",
    ),
    ("ui-needs-a-closed-outline", "Needs a closed outline."),
    ("ui-type", "Type"),
    ("ui-hole-sewing", "Hole sewing"),
    (
        "ui-every-needle-point-with-its-exact-tip",
        "Every needle point with its exact position: move, insert or delete points; the object stays editable",
    ),
    ("ui-stitches-edited-by-hand", "Stitches edited by hand"),
    ("ui-regenerate", "Regenerate"),
    (
        "ui-drop-the-hand-edits-and-sew-tip",
        "Drop the hand edits and sew the generated stitches again (a sewing setting change does it too)",
    ),
    ("ui-start-tip", "Start"),
    ("ui-previous-stitch-tip", "Previous stitch"),
    ("ui-next-stitch-tip", "Next stitch"),
    ("ui-end-tip", "End"),
    ("ui-show-travel", "Show travel"),
    (
        "ui-stitches-and-machine-time-at-the-tip",
        "Stitches and machine time at the needle, out of the totals",
    ),
    (
        "ui-play-the-whole-design-in-about-tip",
        "Play the whole design in about 12 seconds",
    ),
    (
        "ui-keep-the-needle-in-view-while-tip",
        "Keep the needle in view while zoomed in",
    ),
    (
        "ui-colour-blocks-in-sewing-order-ticks-tip",
        "Colour blocks in sewing order: ticks are trims, bars colour changes (red: a stop). Click or drag to go there",
    ),
    (
        "ui-go-to-the-first-stitch-of-tip",
        "Go to the first stitch of this colour block",
    ),
    ("ui-mm", "mm"),
    ("ui-inch", "inch"),
    ("ui-jumps-that-make-a-trim", "Jumps that make a trim"),
    ("ui-folders", "Folders"),
    ("ui-no-folders", "(no folders)"),
    ("ui-back-to-folder", "Back to folder"),
    ("ui-open-in-layout-editing", "Open in Layout & Editing"),
    (
        "ui-import-into-layout-editing",
        "Import Into Layout & Editing",
    ),
    ("ui-cut", "Cut"),
    ("ui-copy", "Copy"),
    ("ui-rename-2", "Rename…"),
    ("ui-delete-2", "Delete…"),
    ("ui-convert-format", "Convert Format…"),
    ("ui-add-to-writing-list", "Add to Writing List"),
    ("ui-double-click-to-open-tip", "Double-click to open"),
    ("ui-preview", "Preview"),
    (
        "ui-select-a-design-to-see-it",
        "Select a design to see it and its information.",
    ),
    ("ui-property", "Property"),
    ("ui-import", "Import"),
    ("ui-move-to-trash", "Move to Trash"),
    ("ui-to", "to"),
    ("ui-format", "Format"),
    ("ui-date-modified", "Date modified"),
    ("ui-search-subfolders", "Search subfolders"),
    ("ui-search", "Search"),
    ("ui-format-type", "Format type"),
    ("ui-output-to", "Output to"),
    ("ui-same-folder", "Same folder"),
    ("ui-following-folder", "Following folder"),
    ("ui-browse", "Browse…"),
    ("ui-dst-jumps-per-trim", "DST jumps per trim"),
    (
        "ui-existing-files-are-never-replaced-the",
        "Existing files are never replaced: the new file gets a number.",
    ),
    ("ui-output", "Output"),
    ("ui-designs", "Designs"),
    ("ui-title", "Title"),
    ("ui-layout", "Layout"),
    ("ui-a4", "A4"),
    ("ui-letter", "Letter"),
    ("ui-portrait", "Portrait"),
    ("ui-landscape", "Landscape"),
    ("ui-color-list", "Color list"),
    ("ui-total-colors", "Total colors"),
    ("ui-color-description", "Color description"),
    ("ui-colour-name", "Colour name"),
    ("ui-brand-and-number", "Brand and number"),
    (
        "ui-the-images-go-to-a-folder",
        "The images go to a folder named after the HTML file.",
    ),
    ("ui-save-2", "Save…"),
    ("ui-merge", "Merge"),
    ("ui-remove-overlap", "Remove Overlap"),
    (
        "ui-select-the-shape-and-the-shape-tip",
        "Select the shape and the shape on top that gives the outline",
    ),
    (
        "ui-place-stamp-click-in-the-shape",
        "Place stamp (click in the shape)",
    ),
    ("ui-remove-stamps", "Remove stamps"),
    ("ui-select-one-shape", "Select one shape"),
    ("ui-reset-to-automatic", "Reset to automatic"),
    (
        "ui-select-one-shape-with-region-sewing",
        "Select one shape with region sewing",
    ),
    ("ui-patterns", "Patterns"),
    ("ui-radius", "Radius"),
    ("ui-turn-copies", "Turn copies"),
    (
        "ui-mirror-every-other-kaleidoscope",
        "Mirror every other (kaleidoscope)",
    ),
    (
        "ui-beyond-pe-design-every-second-copy-tip",
        "Beyond PE-DESIGN: every second copy mirrored about its radial axis",
    ),
    ("ui-side", "Side"),
    ("ui-turn-with-the-line", "Turn with the line"),
    ("ui-mirror-every-other", "Mirror every other"),
    ("ui-gap", "Gap"),
    ("ui-turn-at-random", "Turn at random"),
    ("ui-seed", "Seed"),
    ("ui-shuffle", "Shuffle"),
    ("ui-rows-columns", "Rows × columns"),
    ("ui-spacing-x-y", "Spacing x / y"),
    ("ui-spacing", "Spacing"),
    ("ui-merge-overlapping-shapes", "Merge overlapping shapes"),
    ("ui-optimize-sewing-order", "Optimize sewing order"),
    ("ui-arrange", "Arrange"),
    ("ui-rounded-corners", "Rounded corners"),
    ("ui-optimise", "Optimise"),
    (
        "ui-how-optimise-arranges-the-objects-saved-tip",
        "How Optimise arranges the objects; saved with the design",
    ),
    ("ui-sew-in-click-order", "Sew in click order"),
    (
        "ui-sew-the-selected-objects-in-the-tip",
        "Sew the selected objects in the order you clicked them (Shift or Ctrl+click), starting where the first of them sews now",
    ),
    (
        "ui-locked-objects-cannot-be-picked-moved-tip",
        "Locked objects cannot be picked, moved, resized, point-edited or deleted on the canvas; select them here",
    ),
    (
        "ui-beyond-pe-design-sew-the-outline-tip",
        "Beyond PE-DESIGN: sew the outline before the region (PE-DESIGN sews the region first)",
    ),
    (
        "ui-beyond-pe-design-the-machine-pauses-tip",
        "Beyond PE-DESIGN: the machine pauses after these objects (to place appliqué fabric or a patch) without a colour change",
    ),
    ("ui-unbranch", "Unbranch"),
    (
        "ui-take-the-branch-apart-the-objects-tip",
        "Take the branch apart: the objects stay but are no longer linked",
    ),
    ("ui-branch", "Branch"),
    (
        "ui-beyond-pe-design-sew-the-selected-tip",
        "Beyond PE-DESIGN: sew the selected touching shapes in one go in the first one's thread, each starting where the one before touches it",
    ),
    (
        "ui-beyond-pe-design-skip-the-stitching-tip",
        "Beyond PE-DESIGN: skip the stitching later fills and satins cover, keeping 1 mm under their edges; the shape is not cut",
    ),
    (
        "ui-beyond-pe-design-reach-the-next-tip",
        "Beyond PE-DESIGN: reach the next object of the same thread with a running-stitch connector instead of a jump and trim",
    ),
    (
        "ui-beyond-pe-design-a-light-fill-tip",
        "Beyond PE-DESIGN: a light fill sewn first under the selection (or the whole design) that flattens towel and fleece pile; on a selected knockdown, rebuild it to cover what changed",
    ),
    ("ui-sew-earlier-tip", "Sew earlier"),
    ("ui-sew-later-tip", "Sew later"),
    ("ui-area-to-fill", "Area to fill"),
    ("ui-fill-type", "Fill type"),
    ("ui-echo-type", "Echo type"),
    ("ui-echo-clipped", "Echo clipped"),
    ("ui-echo", "Echo"),
    ("ui-link-echo-ends", "Link echo ends"),
    ("ui-connect-object-edges", "Connect object edges"),
    ("ui-angle", "Angle"),
    ("ui-link-line-ends", "Link line ends"),
    ("ui-sew-type", "Sew type"),
    ("ui-stitch-type", "Stitch type"),
    ("ui-triangle", "Triangle"),
    ("ui-diamond", "Diamond"),
    ("ui-times", "Times"),
    (
        "ui-skip-overlapped-background",
        "Skip overlapped background",
    ),
    ("ui-material", "Material"),
    ("ui-density", "Density"),
    ("ui-replace", "Replace"),
    (
        "ui-create-an-appliqu-with-hole-sewing",
        "Create an appliqué with hole sewing",
    ),
    ("ui-border", "Border"),
    ("ui-yes", "Yes"),
    ("ui-no", "No"),
    ("ui-cut-by", "Cut by"),
    ("ui-2-cutting-line", "2 Cutting line"),
    ("ui-cutting", "Cutting"),
    ("ui-running-stitch", "Running stitch"),
    ("ui-pattern", "Pattern"),
    ("ui-direction", "Direction"),
    ("ui-the-assistant-is-closed", "The Assistant is closed"),
    ("ui-to-open-it-again", "To open it again:"),
    ("ui-endpoint", "Endpoint"),
    ("ui-protocol", "Protocol"),
    ("ui-address", "Address"),
    ("ui-model", "Model"),
    (
        "ui-the-models-this-address-lists-tip",
        "The models this address lists",
    ),
    ("ui-list", "List"),
    (
        "ui-ask-the-endpoint-for-its-models-tip",
        "Ask the endpoint for its models",
    ),
    ("ui-effort", "Effort"),
    ("ui-reasoning-effort", "Reasoning effort"),
    (
        "ui-sent-as-reasoning-effort-when-set-tip",
        "Sent as reasoning_effort when set (for example low, medium, high); values differ per provider",
    ),
    ("ui-request-timeout-s", "Request timeout (s)"),
    (
        "ui-how-long-the-server-may-go-tip",
        "How long the server may go without sending any of the reply (text, thinking or a tool call) before the turn ends; a slow local model may need more",
    ),
    ("ui-agent", "Agent"),
    ("ui-install", "Install"),
    ("ui-permissions", "Permissions"),
    ("ui-changes-per-message", "Changes per message"),
    ("ui-0-no-limit-tip", "0 = no limit"),
    (
        "ui-looking-for-a-model-that-can",
        "Looking for a model that can use tools on this endpoint…",
    ),
    ("ui-save-notes", "Save notes"),
    ("ui-undo-edits", "Undo edits"),
    (
        "ui-clear-the-notes-the-assistant-keeps-tip",
        "Clear the notes the Assistant keeps between conversations",
    ),
    (
        "ui-image-generator-optional-an-openai-compatible",
        "Image generator (optional): an OpenAI-compatible /images/generations address ending in /v1. Without one, the assistant draws pictures as vector shapes.",
    ),
    ("ui-clear-image-generator", "Clear image generator"),
    (
        "ui-back-to-the-defaults-and-remove-tip",
        "Back to the defaults, and remove the saved API keys",
    ),
    ("ui-paste-a-key", "Paste a key"),
    ("ui-save-key", "Save key"),
    ("ui-thinking", "Thinking"),
    ("ui-assistant", "Assistant"),
    ("ui-new-chat", "New chat"),
    (
        "ui-start-a-new-conversation-the-design-tip",
        "Start a new conversation; the design stays",
    ),
    ("ui-save-as-script", "Save as script"),
    ("ui-allow-once", "Allow once"),
    ("ui-always-allow", "Always allow"),
    ("ui-deny", "Deny"),
    ("ui-view", "View"),
    (
        "ui-send-a-picture-of-the-design-tip",
        "Send a picture of the design with the message",
    ),
    ("ui-stop", "Stop"),
    ("ui-undo-these-changes", "Undo these changes"),
    (
        "ui-production-beyond-pe-design",
        "Production (beyond PE-DESIGN)",
    ),
    ("ui-basting-box", "Basting box"),
    (
        "ui-long-running-stitches-sewn-first-round-tip",
        "Long running stitches sewn first round the design to tack floated fabric to the stabiliser; pull them out after sewing",
    ),
    (
        "ui-distance-from-the-design-shrunk-to-tip",
        "Distance from the design (shrunk to fit an oval or round hoop)",
    ),
    ("ui-basting-stitch-length-tip", "Basting stitch length"),
    ("ui-own-thread", "Own thread"),
    (
        "ui-sew-the-basting-in-its-own-tip",
        "Sew the basting in its own colour (an extra colour change); off sews it in the first thread",
    ),
    (
        "ui-stop-at-jumps-machine-without-thread",
        "Stop at jumps (machine without thread cutter)",
    ),
    (
        "ui-the-machine-stops-before-every-jump-tip",
        "The machine stops before every jump so you can snip the thread; stops set with Stop after stay",
    ),
    (
        "ui-start-each-object-nearest-the-last",
        "Start each object nearest the last",
    ),
    (
        "ui-shapes-without-their-own-entry-point-tip",
        "Shapes without their own entry point start where the needle stopped: regions and closed lines at the nearest point, open lines at the nearer end",
    ),
    ("ui-travel-under-later-fills", "Travel under later fills"),
    (
        "ui-a-move-to-an-object-of-tip",
        "A move to an object of the same thread that a later fill will cover is sewn as short running stitches under it, or along running lines of that thread already sewn, instead of a jump and trim",
    ),
    (
        "ui-leave-out-hidden-stitching-1-mm",
        "Leave out hidden stitching (1 mm overlap)",
    ),
    (
        "ui-shapes-skip-the-stitching-later-fills-tip",
        "Shapes skip the stitching later fills and satins cover, keeping 1 mm under their edges; the shapes are not cut",
    ),
    ("ui-machine-type", "Machine type"),
    ("ui-sewing-area", "Sewing area"),
    ("ui-jump-stitch-trimming", "Jump stitch trimming"),
    ("ui-speed-estimate", "Speed (estimate)"),
    ("ui-user-hoops", "User hoops"),
    ("ui-add-hoop", "Add Hoop"),
    ("ui-multi-position-hoop", "Multi-position hoop"),
    ("ui-optimize-hoop-changes", "Optimize hoop changes"),
    (
        "ui-group-patterns-by-installation-position-a-tip",
        "Group patterns by installation position (a, b, c)",
    ),
    ("ui-jumbo-frame", "Jumbo frame"),
    ("ui-side-overlap", "Side overlap"),
    (
        "ui-split-designs-need-hoop-custom-size",
        "Split designs need Hoop: Custom size.",
    ),
    ("ui-split-design", "Split design"),
    ("ui-section-size", "Section size"),
    ("ui-not-split", "Not split"),
    ("ui-use-smaller-section-sizes", "Use smaller section sizes"),
    ("ui-even-split", "Even split"),
    ("ui-hoopings", "Hoopings"),
    ("ui-grid", "Grid"),
    (
        "ui-pe-design-s-sections-cut-at-tip",
        "PE-DESIGN's sections, cut at the borders",
    ),
    ("ui-free", "Free"),
    (
        "ui-hoopings-placed-anywhere-overlapping-each-object-tip",
        "Hoopings placed anywhere, overlapping; each object sewn whole in the first hooping that holds it",
    ),
    ("ui-turned-90", "Turned 90°"),
    ("ui-auto-layout", "Auto layout"),
    (
        "ui-the-fewest-hoopings-covering-the-design-tip",
        "The fewest hoopings covering the design",
    ),
    ("ui-marks", "Marks"),
    ("ui-overlap", "overlap"),
    (
        "ui-stop-after-the-marks-to-line",
        "Stop after the marks to line the fabric up",
    ),
    ("ui-step", "Step"),
    ("ui-copies", "Copies"),
    ("ui-fabric-calibration-card", "Fabric calibration card"),
    (
        "ui-sew-its-settings-as-a-label",
        "Sew its settings as a label below it",
    ),
    (
        "ui-select-one-shape-or-text-object",
        "Select one shape or text object.",
    ),
    ("ui-setting", "Setting"),
    (
        "ui-and-a-second-setting-across-the",
        "And a second setting across the columns",
    ),
    ("ui-sew-each-value-as-a-label", "Sew each value as a label"),
    ("ui-save-swatch", "Save Swatch…"),
    (
        "ui-after-sewing-it-the-best-piece",
        "After sewing it: the best piece of each row",
    ),
    ("ui-name", "Name"),
    ("ui-save-as-fabric", "Save as Fabric"),
    (
        "ui-a-custom-fabric-in-the-fabric-tip",
        "A custom fabric in the Fabric Selector, for Fabric ▸ Apply",
    ),
    ("ui-open-it", "Open It"),
    ("ui-no-release-notes", "No release notes."),
    (
        "ui-don-t-remind-me-about-this",
        "Don't remind me about this version",
    ),
    (
        "ui-stays-quiet-about-this-version-a-tip",
        "Stays quiet about this version; a newer one is offered again",
    ),
    ("ui-update", "Update"),
    (
        "ui-downloads-the-release-checks-it-installs-tip",
        "Downloads the release, checks it, installs it and restarts OpenStitch",
    ),
    ("ui-dismiss", "Dismiss"),
    (
        "ui-save-your-design-before-updating",
        "Save your design before updating.",
    ),
    (
        "ui-the-download-is-checked-against-the",
        "The download is checked against the release's SHA-256 sums before it is installed.",
    ),
    (
        "ui-update-installed-restarting-openstitch",
        "Update installed. Restarting OpenStitch…",
    ),
    ("ui-close", "Close"),
    (
        "ui-updates-are-not-available-for-this",
        "Updates are not available for this build.",
    ),
    (
        "ui-the-update-did-not-go-through",
        "The update did not go through.",
    ),
    ("ui-try-again", "Try again"),
    ("ui-checking-for-updates", "Checking for updates…"),
    ("ui-lighter", "Lighter"),
    ("ui-darker", "Darker"),
    ("ui-edit", "Edit"),
    ("ui-kind", "Kind"),
    ("ui-cell", "Cell"),
    ("ui-rows", "Rows"),
    ("ui-triple", "Triple"),
    ("ui-ready-to-sew", "Ready to sew"),
    (
        "ui-check-design-open-the-list-of-tip",
        "Check Design: open the list of sewing problems",
    ),
    ("ui-fabric", "Fabric"),
    (
        "ui-ready-to-sew-no-problems-found",
        "Ready to sew: no problems found.",
    ),
    ("ui-select-and-show-tip", "Select and show"),
    ("ui-fix", "Fix"),
    (
        "ui-thread-layers-1-one-fill",
        "Thread layers (1 = one fill):",
    ),
    ("ui-no-hotspots", "No hotspots."),
    ("ui-show", "Show"),
    ("ui-previous", "Previous"),
    ("ui-whole-design", "Whole design"),
    ("ui-find", "Find"),
    ("ui-all-commands", "All commands"),
    (
        "ui-list-commands-without-a-shortcut-too-tip",
        "List commands without a shortcut too, to give them one",
    ),
    ("ui-reset-all", "Reset All"),
    (
        "ui-back-to-the-pe-style-default-tip",
        "Back to the PE-style default shortcuts",
    ),
    ("ui-export", "Export…"),
    (
        "ui-save-the-shortcuts-as-a-json-tip",
        "Save the shortcuts as a JSON file",
    ),
    ("ui-import-2", "Import…"),
    (
        "ui-load-shortcuts-saved-with-export-replaces-tip",
        "Load shortcuts saved with Export (replaces the changed ones)",
    ),
    (
        "ui-click-to-remove-this-shortcut-tip",
        "Click to remove this shortcut",
    ),
    (
        "ui-press-the-new-shortcut-esc-cancels-tip",
        "Press the new shortcut; Esc cancels",
    ),
    ("ui-notes", "Notes"),
    ("ui-shop-name", "Shop name"),
    (
        "ui-printed-at-the-top-of-the-tip",
        "Printed at the top of the sheet; not saved with the design",
    ),
    ("ui-save-job-fields", "Save Job Fields"),
    ("ui-client-approval", "Client approval"),
    ("ui-operator-worksheet", "Operator worksheet"),
    ("ui-overview", "Overview"),
    ("ui-detailed", "Detailed"),
    (
        "ui-saves-the-job-fields-then-opens-tip",
        "Saves the job fields, then opens the system print dialog",
    ),
    ("ui-metric", "Metric"),
    (
        "ui-pe-design-s-rule-the-nearest-tip",
        "PE-DESIGN's rule: the nearest colour by RGB distance",
    ),
    (
        "ui-the-thread-that-looks-closest-tip",
        "The thread that looks closest",
    ),
    (
        "ui-keep-distinct-colours-apart",
        "Keep distinct colours apart",
    ),
    (
        "ui-no-plan-the-chart-is-empty",
        "No plan: the chart is empty or missing.",
    ),
    ("ui-becomes", "becomes"),
    ("ui-restore-original-colours", "Restore original colours"),
    ("ui-merge-into", "merge into"),
    (
        "ui-beyond-pe-design-when-far-is",
        "Beyond PE-DESIGN; When far is PE-DESIGN's rule",
    ),
    ("ui-trim-distance", "Trim distance"),
    (
        "ui-off-the-design-s-trim-distance-tip",
        "Off: the design's trim distance",
    ),
    ("ui-tie-stitches", "Tie stitches"),
    (
        "ui-out-and-back-steps-of-each-tip",
        "Out-and-back steps of each tie-in and lock",
    ),
    (
        "ui-length-of-each-tie-stitch-tip",
        "Length of each tie stitch",
    ),
    ("ui-pe", "PE"),
    ("ui-pe-design-s-own-tie-tip", "PE-DESIGN's own tie"),
    ("ui-between-letters", "Between letters"),
    (
        "ui-where-a-text-trims-between-its-tip",
        "Where a text trims between its letters; letters not trimmed apart are joined by a stitch or a jump",
    ),
    ("ui-pe-design-s-rules", "PE-DESIGN's rules"),
    ("ui-refresh", "Refresh"),
    (
        "ui-look-for-usb-media-again-tip",
        "Look for USB media again",
    ),
    (
        "ui-add-the-selected-designs-to-the-tip",
        "Add the selected designs to the Writing List",
    ),
    (
        "ui-remove-the-picked-designs-from-the-tip",
        "Remove the picked designs from the Writing List",
    ),
    ("ui-write", "Write"),
    (
        "ui-copy-the-designs-in-the-writing-tip",
        "Copy the designs in the Writing List to the USB media",
    ),
    (
        "ui-add-designs-or-drag-them-here",
        "Add designs, or drag them here.",
    ),
    ("ui-remove-from-writing-list", "Remove from Writing List"),
    ("ui-overwrite", "Overwrite"),
    (
        "ui-copy-with-a-different-name",
        "Copy with a different name",
    ),
    ("ui-snap-to-1-16-of-the", "Snap to 1/16 of the tile"),
    ("ui-row-shift", "Row shift"),
    ("ui-add-to-design", "Add to Design"),
    ("ui-delete-file", "Delete file"),
    (
        "ui-no-usb-stick-or-sd-card",
        "No USB stick or SD card is mounted.",
    ),
    ("ui-send", "Send"),
    (
        "ui-select-one-stitch-pattern-to-list",
        "Select one stitch pattern to list its needle points.",
    ),
    ("ui-edited-by-hand", "edited by hand"),
    (
        "ui-drop-the-hand-edits-the-object-tip",
        "Drop the hand edits: the object sews its generated stitches again",
    ),
    (
        "ui-add-a-needle-point-halfway-along-tip",
        "Add a needle point halfway along the line into the first selected row",
    ),
    (
        "ui-delete-the-selected-rows-a-jump-tip",
        "Delete the selected rows (a jump's end joins the stitches either side)",
    ),
    ("ui-free-envelope", "Free envelope"),
    (
        "ui-bend-the-text-with-handles-beyond-tip",
        "Bend the text with handles (beyond PE-DESIGN; not written to PES 0110)",
    ),
    ("ui-from-preset", "From preset"),
    (
        "ui-the-text-transform-preset-as-a-tip",
        "The Text Transform preset as a free envelope: the same stitches until a handle moves",
    ),
    ("ui-edit-handles", "Edit handles"),
    (
        "ui-drag-a-handle-on-the-canvas-tip",
        "Drag a handle on the canvas; Shift bends both sides alike",
    ),
    (
        "ui-select-one-closed-running-triple-zigzag",
        "Select one closed running, triple, zigzag, E or V line.",
    ),
    ("ui-starts-at", "Starts at"),
    ("ui-length", "Length"),
    ("ui-remove-gap", "Remove gap"),
    ("ui-don-t-save", "Don't Save"),
    (
        "ui-goes-on-and-discards-the-changes-tip",
        "Goes on and discards the changes",
    ),
    ("ui-back-to-the-editor-esc-tip", "Back to the editor · Esc"),
    ("ui-amount", "Amount"),
    (
        "ui-different-radius-per-corner",
        "Different radius per corner",
    ),
    ("ui-ink-stitch-fonts", "Ink/Stitch Fonts…"),
    (
        "ui-import-ink-stitch-font-folder",
        "Import Ink/Stitch Font Folder…",
    ),
    ("ui-choose", "Choose…"),
    ("ui-rescan", "Rescan"),
    (
        "ui-close-this-design-asks-first-when-tip",
        "Close this design (asks first when it has unsaved changes)",
    ),
    ("ui-new-tab", "New Tab"),
    (
        "ui-an-empty-design-in-a-new-tip",
        "An empty design in a new tab (Ctrl+T)",
    ),
    ("ui-relative-to", "Relative to"),
    (
        "ui-the-point-x-and-y-give-tip",
        "The point X and Y give, and that stays put when sizing or turning",
    ),
    (
        "ui-lock-aspect-width-and-height-scale-tip",
        "Lock aspect: width and height scale together",
    ),
    ("ui-use-96-dpi", "Use 96 dpi"),
    (
        "ui-forget-this-monitor-s-calibration-tip",
        "Forget this monitor's calibration",
    ),
    ("ui-start-your-design", "Start your design"),
    (
        "ui-getting-started-in-the-help-menu",
        "Getting Started in the Help menu walks through a first design",
    ),
    (
        "ui-opens-a-copy-as-a-new",
        "Opens a copy as a new untitled design; the kit itself never changes.",
    ),
    (
        "ui-stop-opening-the-current-design-stays-tip",
        "Stop opening; the current design stays as it is",
    ),
    ("ui-tolerance", "Tolerance"),
    ("ui-no-command-matches", "No command matches"),
    ("ui-arc", "Arc"),
    ("ui-window-match-to-thread-chart", "Match to Thread Chart"),
    ("ui-window-merge-similar-colours", "Merge Similar Colours"),
    ("ui-window-delete", "Delete"),
    ("ui-window-search", "Search"),
    ("ui-window-convert-format", "Convert Format"),
    ("ui-window-catalogue", "Catalogue"),
    ("ui-window-arrange-copies", "Arrange Copies"),
    ("ui-window-offset-lines", "Offset Lines"),
    ("ui-window-floral-pattern", "Floral Pattern"),
    ("ui-window-job-sheet", "Job Sheet"),
    ("ui-window-keyboard-shortcuts", "Keyboard Shortcuts"),
    (
        "ui-window-fit-text-to-outline-attribute-setting",
        "Fit Text to Outline Attribute Setting",
    ),
    ("ui-window-text", "Text"),
    ("ui-window-name-drop-list", "Name Drop List"),
    ("ui-window-template-wizard", "Template Wizard"),
    ("ui-window-machine-setup", "Machine Setup"),
    ("ui-window-command-palette", "Command Palette"),
    ("ui-window-design-property", "Design Property"),
    ("ui-window-about-openstitch", "About OpenStitch"),
    ("ui-window-print", "Print"),
    ("ui-window-check-design", "Check Design"),
    ("ui-window-sewing-attribute-sets", "Sewing Attribute Sets"),
    ("ui-window-user-thread-charts", "User Thread Charts"),
    ("ui-window-stitch-simulator", "Stitch Simulator"),
    ("ui-window-open-sample", "Open Sample"),
    ("ui-window-getting-started", "Getting Started"),
    (
        "ui-window-how-do-you-want-to-produce",
        "How do you want to produce embroidery?",
    ),
    ("ui-window-stitch-table", "Stitch Table"),
    ("ui-window-test-swatches", "Test Swatches"),
    ("ui-window-send-to-usb-media", "Send to USB Media"),
    ("ui-window-openstitch-update", "OpenStitch Update"),
    ("ui-window-modify-image", "Modify Image"),
    (
        "ui-window-realistic-view-settings",
        "Realistic View Settings",
    ),
    ("ui-window-font-filter-settings", "Font Filter Settings"),
    ("ui-window-fabric-selector", "Fabric Selector"),
    ("ui-window-image-to-embroidery", "Image to Embroidery"),
    ("ui-window-fix-on-the-picture", "Fix on the picture"),
    ("ui-window-mark-the-subject", "Mark the subject"),
    ("ui-window-reference", "Reference"),
    ("ui-window-calibrate-real-size", "Calibrate Real Size"),
    (
        "ui-status-could-not-read-the-user-settings",
        "Could not read the user settings ({ $e }); they are not saved",
    ),
    ("ui-status-status", "{ $path }: { $e }"),
    ("ui-status-opened", "Opened { $path }"),
    ("ui-status-imported", "Imported { $path }{ $result }"),
    ("ui-status-background", "Background { $path }"),
    ("ui-status-paste-background", "Paste background: { $e }"),
    ("ui-status-saved", "Saved { $path }"),
    (
        "ui-status-wrote-colours-to",
        "Wrote { $colors } colours to { $path }",
    ),
    (
        "ui-status-applied-colours-from",
        "Applied { $colors } colours from { $path }",
    ),
    ("ui-status-exported", "Exported { $path }. { $w }"),
    (
        "ui-status-opening-the-print-dialog-for",
        "Opening the print dialog for { $path }…",
    ),
    (
        "ui-status-locked-unlock-it-in-the-sewing",
        "Locked: unlock it in the Sewing Order list first",
    ),
    (
        "ui-status-paste-background-the-system-clipboard-is",
        "Paste background: the system clipboard is not used here",
    ),
    (
        "ui-status-background-pasted-from-the-clipboard",
        "Background pasted from the clipboard",
    ),
    ("ui-status-cancelled", "Cancelled"),
    ("ui-status-objects-selected", "{ $count } objects selected"),
    (
        "ui-status-threads-matched-to-the-chart",
        "Threads matched to the chart",
    ),
    ("ui-status-similar-colours-merged", "Similar colours merged"),
    (
        "ui-status-imported-design-n-other-designs-n",
        "Imported { $n ->\n[one] { $n } design\n*[other] { $n } designs\n} from the Design Database",
    ),
    ("ui-status-done", "{ $command } done"),
    (
        "ui-status-click-where-should-be-esc-cancels",
        "Click where { $name } should be; Esc cancels",
    ),
    ("ui-status-set", "{ $name } set"),
    ("ui-status-moved", "{ $name } moved"),
    ("ui-status-copies-fitted", "{ $placed } copies fitted"),
    (
        "ui-status-sewing-attributes-copied-select-the-targets",
        "Sewing attributes copied; select the targets and Paste Attributes",
    ),
    (
        "ui-status-draw-the-cut-outline-return-or",
        "Draw the cut outline; Return or double-click splits, Esc cancels",
    ),
    (
        "ui-status-click-inside-the-shape-to-place",
        "Click inside the shape to place the stamp; Esc cancels",
    ),
    (
        "ui-status-draw-an-open-curve-across-the",
        "Draw an open curve across the shape; Return finishes, Esc cancels",
    ),
    ("ui-status-stamp-placed", "Stamp placed"),
    (
        "ui-status-the-direction-must-be-an-open",
        "The direction must be an open curve",
    ),
    (
        "ui-status-saved-pages",
        "Saved { $path } ({ $pages } pages)",
    ),
    (
        "ui-status-stored-mm-wide",
        "Stored '{ $character }' ({ $advancemm } mm wide)",
    ),
    (
        "ui-status-registered-stitches",
        "Registered '{ $character }' ({ $stitches } stitches)",
    ),
    (
        "ui-status-wrote-file-n-other-files-n",
        "Wrote { $count ->\n[one] { $count } file\n*[other] { $count } files\n} to { $path }",
    ),
    (
        "ui-status-created-an-empty-embroidery-font",
        "Created an empty embroidery font",
    ),
    ("ui-status-opening", "Opening { $path }…"),
    (
        "ui-status-could-not-open",
        "Could not open { $path }: { $e }",
    ),
    ("ui-status-imported-2", "Imported { $path }"),
    ("ui-status-optimise", "Optimise: { $summary }"),
    (
        "ui-status-new-shapes-start-with-these-settings",
        "New shapes start with these settings",
    ),
    (
        "ui-status-select-the-patterns-first",
        "{ $title }: select the patterns first",
    ),
    ("ui-status-applied", "{ $title } applied"),
    (
        "ui-status-exported-section-files-next-to",
        "Exported { $n } section files next to { $path }",
    ),
    ("ui-status-applied-2", "{ $name } applied"),
    (
        "ui-status-smooth-stitches-points-moved",
        "Smooth Stitches: { $moved } points moved",
    ),
    (
        "ui-status-stitch-table-select-the-row-to",
        "Stitch Table: select the row to insert before",
    ),
    (
        "ui-status-stitch-table-select-the-rows-to",
        "Stitch Table: select the rows to delete",
    ),
    (
        "ui-status-saved-the-test-swatch",
        "Saved the test swatch { $path }",
    ),
    ("ui-status-sent-to-usb-media", "Sent to USB media"),
    (
        "ui-status-updated-from-to",
        "Updated from { $from } to { $to }",
    ),
    (
        "ui-status-at-most-guidelines-per-ruler",
        "At most { $uides } guidelines per ruler",
    ),
    (
        "ui-status-background-image-turned",
        "Background image turned { $direction }",
    ),
    ("ui-status-image-saved-to", "Image saved to { $path }"),
    (
        "ui-status-fabric-settings-applied-to-pattern-n",
        "Fabric settings applied to { $n ->\n[one] { $n } pattern\n*[other] { $n } patterns\n}",
    ),
    (
        "ui-status-background-image-changed",
        "Background image changed",
    ),
    ("ui-status-font-filter-on", "Font filter on"),
    (
        "ui-status-auto-punch-preview-regions",
        "Auto Punch preview: { $n } regions",
    ),
    (
        "ui-status-real-size-calibrated-for",
        "Real size calibrated for { $monitor }",
    ),
    (
        "ui-status-hand-edited-stitches-discarded-on-object",
        "Hand-edited stitches discarded on { $count ->\n[one] { $count } object\n*[other] { $count } objects\n}: a sewing setting or the shape changed",
    ),
    (
        "ui-status-opened-as-a-new-design-save",
        "Opened { $path } as a new design (Save to keep it as .osd)",
    ),
    (
        "ui-status-exported-stitches",
        "Exported { $path } ({ $stats } stitches)",
    ),
    (
        "ui-status-click-the-outline-where-should-be",
        "Click the outline where { $name } should be; Esc cancels",
    ),
    (
        "ui-status-loaded-font-glyphs-it-is-saved",
        "Loaded font { $id } ({ $glyphs } glyphs); it is saved with the design",
    ),
    (
        "ui-status-converted-character-n-other-characters-n",
        "Converted { $converted ->\n[one] { $converted } character\n*[other] { $converted } characters\n}{ $missing }",
    ),
    (
        "ui-status-editing-baseline-mm-capital-line-mm",
        "Editing '{ $character }': baseline { $baseline } mm, capital line { $capline } mm, set lines { $left }–{ $right } mm",
    ),
    (
        "ui-status-found-character-n-other-characters-n",
        "Found { $count ->\n[one] { $count } character\n*[other] { $count } characters\n} in { $rows ->\n[one] { $rows } row\n*[other] { $rows } rows\n}; check the characters, then Create characters",
    ),
    (
        "ui-status-created-from-the-image-skipped",
        "Created { $created } from the image ({ $skipped } skipped)",
    ),
    (
        "ui-status-saved-template-with-editable-text-s",
        "Saved template with { $slots } editable text(s)",
    ),
    (
        "ui-status-machine-set-new-designs-start-on",
        "Machine set: { $brand }; new designs start on its hoop",
    ),
    (
        "ui-status-cancelled-opening-the-design-is-unchanged",
        "Cancelled opening { $path }; the design is unchanged",
    ),
    (
        "ui-status-the-design-changed-while-was-opening",
        "The design changed while { $path } was opening; nothing was replaced. Open it again",
    ),
    (
        "ui-status-is-a-list-choose-on-the",
        "{ $label } is a list: choose on the ribbon, { $a }",
    ),
    ("ui-status-saved-the-fabric", "Saved the fabric { $fabric }"),
    ("ui-status-satin-width-mm", "Satin width { $width } mm"),
    (
        "ui-status-turning-gap-of-mm",
        "Turning gap of { $length } mm",
    ),
    ("ui-status-guideline-mm", "Guideline { $position }"),
    ("ui-status-created", "Created { $path }"),
    ("ui-status-renamed-to", "Renamed to { $path }"),
    (
        "ui-status-moved-item-n-other-items-n",
        "Moved { $n ->\n[one] { $n } item\n*[other] { $n } items\n} to the trash",
    ),
    ("ui-status-preview", "Preview: { $e }"),
    ("ui-status-autosave-failed", "Autosave failed: { $e }"),
    (
        "ui-status-recovered-untitled-until-you-save-it",
        "Recovered { $name }: untitled until you save it",
    ),
    ("ui-status-objects-cut", "{ $n } objects cut"),
    ("ui-status-objects-copied", "{ $count } objects copied"),
    ("ui-status-layout-saved", "Layout '{ $n }' saved"),
    (
        "ui-status-preferences-reset",
        "{ $label } preferences reset",
    ),
    (
        "ui-status-recovery-files-deleted",
        "{ $n } recovery files deleted",
    ),
    ("ui-status-no-longer-exists", "{ $path } no longer exists"),
    ("ui-status-pasted", "Pasted"),
    (
        "ui-status-designs-folders",
        "{ $count } designs, { $count2 } folders{ $else }",
    ),
    (
        "ui-status-design-n-other-designs-n-choose",
        "{ $opied } { $count ->\n[one] { $count } design\n*[other] { $count } designs\n}: choose a folder and Paste",
    ),
    (
        "ui-status-wrote-catalogue-of-designs",
        "Wrote catalogue of { $count } designs: { $to_string }",
    ),
];

/// The English text of message `id`.
pub fn english(id: &str) -> Option<&'static str> {
    UI_TEXTS.iter().find(|(i, _)| *i == id).map(|(_, e)| *e)
}

/// Message `id` with its variables (`tf!`): see [`super::i18n::message`].
pub fn message(id: &'static str, args: &[(&str, &dyn std::fmt::Display)]) -> String {
    match english(id) {
        Some(e) => super::i18n::message(id, e, args),
        None => id.to_string(),
    }
}

/// A message with variables in this thread's language:
/// `tf!("ui-status-opened", path = p.display())`. The names are the message's `$` variables.
#[macro_export]
macro_rules! tf {
    ($id:literal $(, $name:ident = $value:expr)* $(,)?) => {
        $crate::editor::ui_texts::message(
            $id,
            &[$((stringify!($name), &$value as &dyn ::std::fmt::Display)),*],
        )
    };
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

    /// Every `tf!("…", name = …)` call names a message of this table and passes exactly the
    /// variables its English uses (#555 part 5).
    #[test]
    fn every_tf_call_passes_its_messages_variables() {
        use super::super::i18n::{is_message, variables};
        let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut stack = vec![dir];
        let mut calls = 0;
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d).expect("src").flatten() {
                let p = e.path();
                if p.is_dir() {
                    stack.push(p);
                    continue;
                }
                if p.extension().is_none_or(|x| x != "rs") || p.ends_with("ui_texts.rs") {
                    continue;
                }
                let src = std::fs::read_to_string(&p).expect("read");
                for (i, _) in src.match_indices("tf!(\"") {
                    let rest = &src[i + 5..];
                    let id = &rest[..rest.find('"').expect("quote")];
                    let english = english(id)
                        .unwrap_or_else(|| panic!("{}: unknown message {id}", p.display()));
                    assert!(is_message(english), "{id} has no variables: use t()");
                    // The call's names, up to its closing parenthesis.
                    let call = &rest[id.len() + 1..];
                    let mut depth = 1;
                    let end = call
                        .char_indices()
                        .find(|&(_, c)| {
                            match c {
                                '(' | '[' | '{' => depth += 1,
                                ')' | ']' | '}' => depth -= 1,
                                _ => {}
                            }
                            depth == 0
                        })
                        .map_or(call.len(), |(k, _)| k);
                    let mut names: Vec<String> = Vec::new();
                    let mut d = 0;
                    for part in call[..end].split(|c: char| {
                        match c {
                            '(' | '[' | '{' => d += 1,
                            ')' | ']' | '}' => d -= 1,
                            _ => {}
                        }
                        c == ',' && d == 0
                    }) {
                        if let Some((n, _)) = part.split_once('=') {
                            names.push(n.trim().to_string());
                        }
                    }
                    let mut want = variables(english);
                    want.sort();
                    names.sort();
                    assert_eq!(names, want, "{}: tf!(\"{id}\")", p.display());
                    calls += 1;
                }
            }
        }
        assert!(calls > 0);
    }

    /// A shipped translation may use only its English message's variables (#555 part 5).
    #[test]
    fn translations_use_only_their_messages_variables() {
        use super::super::i18n::{parse, shipped, variables};
        for (code, source) in shipped() {
            for (id, text) in parse(source).expect("parses") {
                if let Some(e) = english(id) {
                    let allowed = variables(e);
                    for v in variables(&text) {
                        assert!(allowed.contains(&v), "{code} {id}: unknown variable ${v}");
                    }
                }
            }
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
