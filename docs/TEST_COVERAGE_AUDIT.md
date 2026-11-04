# Test coverage audit

Which features [FEATURE_MATRIX.md](FEATURE_MATRIX.md) marks Implemented or Partially Compatible
without test evidence for what it claims, and the plan for each. Keep this file current: when a
gap's tests land, move its row to [Closed](#closed) with the PR.

## Method (2026-11-04, at `6386761`)

- Every Implemented or Partially Compatible row of FEATURE_MATRIX (154 rows) was read with its
  evidence column. Each backticked test name was looked up as `fn name` under `crates/`, each
  compat spec id under `compat/specs/`, and the test body or spec was read to see whether it
  asserts the claim.
- A row is a gap when a named test no longer exists (**stale**), when it names no test although
  the claim is testable (**none**), or when its tests leave out a concrete, testable part of the
  claim (**weak**). Claims about PE-DESIGN equivalence beyond the internal-regression and oracle
  tiers, and process evidence (CI packaging, screenshots), were not judged gaps.
- The whole workspace was run with every test, ignored ones included:
  `cargo nextest run --workspace --locked --no-fail-fast --run-ignored all` (macOS arm64,
  debug). Result: see [Run](#run). No test under `crates/` is `#[ignore]`d.
- README's feature list (README.md "Features") was compared with FEATURE_MATRIX: every README
  feature has a matrix row. `tools/readme_status.py --check --offline` differs only in the
  open-issue count, which it reads from GitHub.

Not verified: the audit read test bodies but did not mutate the product to prove each test fails
when the claim breaks; Windows and Linux runs (CI covers them).

## Run

At `6386761` on macOS arm64 (debug): 1160 tests run, 1160 passed, 0 skipped, 490 s; the slowest
was `openstitch-engine::digitize fidelity_high_raises_the_colour_counts` (over 7 min unoptimised,
see #307). Doctests are not run by nextest. Every test the matrix names therefore passes; the gaps
below are claims no test checks.

## Open gaps

| Area | Feature (matrix row) | Gap | Plan | Issue |
|---|---|---|---|---|
| Files | Import SVG: `<line>` | **fixed** (2026-11-04): every `<line>` element was dropped on import (SVG's default fill closed its two-point subpath, which was then discarded) | `svg_line_elements_import_as_running_lines` runs; `imports::tests::line_elements_import_as_open_strokes` | #341 |
| Editing | Centre point for spiral (`object.setCenterPoint`) | fixed: PE-DESIGN's spiral has no centre point, so the command refuses it on spiral | `a_spiral_refuses_a_centre_point` (`coverage_pattern_points.rs`) | #354 (closed) |

Smaller weak points still without a test, recorded in #356: the Optimise mode box and mixed-trim
checkbox (inline egui code); the MSI and macOS `.pes` file associations and
`tools/sew_kit.py --check` in CI (the CI workflow's owner). Each needs a seam in the product code
or a CI step, not only a test. (Solid View's drawing and Snap to Grid for handles, first listed
there, turned out reachable without a seam: #378, #379.)

## Closed

| Area | Feature | Gap | Fixed by |
|---|---|---|---|
| Docs | PES 0110 write, entry/exit points, Photo Stitch, PEC chart | stale test names in the evidence column | #326 (this file's PR) |
| Docs | PEC preview icons, app icon and logo, image similarity Phase 1, VIP colour table | tests existed but were not cited | #326 |
| Docs | OpenStitch-mode Fidelity row; Satin row; CSD | unescaped `\|` split the row on GitHub; a passage repeated; CSD read listed twice | #326 |
| Automation | CLI scripts (`stitchtool exec`), control channel (`stitchtool serve`) | none: only an example and a manual run | #318: `crates/stitchtool/tests/coverage_cli.rs` |
| Automation | Loopback socket (`openstitch --listen`) | weak: idle-connection timeout untested | #333: `crates/openstitch-remote/tests/coverage_socket.rs` |
| Editing | Move/scale/rotate/flip (`object.flip`) | weak: flip's geometry is never asserted | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Editing | Point editing (`object.setPoint`) | weak: "handles move with the vertex" not asserted | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Editing | Copy sewing attributes (`object.pasteAttributes`) | weak: `lineColor`, `lineSewing`, `holeSewing` selections and the refusal of stitch patterns untested | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Editing | Cut and Paste (`object.paste`) | weak: a pasted text on a path following the new path untested | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Editing | Reverse Direction Line | weak: reversing a curved direction untested | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Editing | Entry/exit points (`object.setEntryExit`, `startEnd`) | weak: bad `startEnd` refused and `startEnd`/`lineExit` surviving `.osd` untested | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Editing | Decorative frames (`shape.decorative`) | weak: `size` only tested as a refusal | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Model | Sewing area (page or design) | weak: the `sewingArea` returned by `design.export` is never asserted | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Simulation | Thread length per colour (`design.info`) | weak: `threadPerColorM` never asserted | #327: `crates/openstitch-engine/tests/coverage_editing.rs` |
| Line sew types | Zigzag line | weak: `underSewing` has no test or spec | #328: `crates/openstitch-engine/tests/coverage_stitch_types.rs` |
| Region sew types | Satin | weak: `object.setRegion` `underlayParts` untested (only the design-level parser) | #328: `crates/openstitch-engine/tests/coverage_stitch_types.rs` |
| Region sew types | Fill (tatami) | weak: pull compensation never asserted | #328: `crates/openstitch-engine/tests/coverage_stitch_types.rs` |
| Editing | Curved sewing direction (`object.setDirectionCurve`) | weak: piping and motif fill under a curve, and the curve in `.osd`, untested | #328: `crates/openstitch-engine/tests/coverage_stitch_types.rs` |
| Editing | Centre point / pattern reference point | weak: the reference point's effect is shown only for programmable fill; engine test asserts only that stitches exist | #328: `crates/openstitch-engine/tests/coverage_stitch_types.rs` |
| Beyond PE-DESIGN | Contour fill | weak: "no travel from R1 or to R2" untested | #328: `crates/openstitch-engine/tests/coverage_stitch_types.rs` |
| Files | PES write, rotated hoop (v6) | weak: only the header flag is checked, not the turned PEC stitches | #329: `crates/openstitch-formats/tests/coverage_formats.rs` |
| Files | Import SVG | weak: `<ellipse>`, `<line>`, `<polygon>`, `<polyline>`, CSS `style=` untested | #329: `crates/openstitch-engine/tests/coverage_formats.rs` (`<line>` found broken: #341, fixed) |
| Lettering | Text Transform written to PES 0110 | weak: OpenStitch's own envelope and level writer untested | #329: `crates/openstitch-engine/tests/coverage_formats.rs` |
| Files | Machine formats vs PE-DESIGN reference | weak: `formats.rs` comparisons return early, and pass, if a reference file is missing | #329: `crates/openstitch-formats/tests/coverage_formats.rs` |
| Lettering | TrueType / OpenType fonts | weak: 16 fonts and 40 MiB per design untested | #330: `crates/openstitch-engine/tests/coverage_lettering.rs` (the layout-complexity refusal is not reachable through `text.add` with the bundled fonts, 2000 characters staying far under 600 000 segments; not tested) |
| Design Database | Search | weak: date modified had no positive test; a property-search test could pass without asserting | #331: `crates/openstitch-engine/tests/coverage_library.rs` (through `library.search`; the library's own conditional test is left as it is) |
| Lettering | Per-character edits | weak: character moves on mirrored or on-path text untested | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Editor | Files: Import into design | weak: `EditorApp::import` (import, centre, select) untested | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Editor | Object list | weak: the visibility toggle's effect and undo untested | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Editor | Undo/redo | weak: ⌘Z, ⇧⌘Z, Ctrl+Y only covered by a manual scripted run | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Editor | Sewing simulator | weak: play at a speed, stop at the end untested | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Editor | Inspector | weak: line settings never set; region density/length not read back | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Editor | Select tool | weak: arrow-key nudge and Shift axis lock on move untested | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Editing | Fabric Selector | weak: claims eight fabrics, test asserts at least six | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Editor | Unsaved-changes prompt | weak: the window close request path untested | #332: `crates/openstitch-app/tests/coverage_editor.rs`, `crates/openstitch-app/src/app_coverage_tests.rs`, engine `coverage_fabric.rs` |
| Digitizing | Photo Stitch 1 fine rows (OpenStitch mode) | weak: `densityMean`/`densityWindow` never sent; `travel` only in the wizard's request | #334: `crates/openstitch-engine/tests/coverage_digitize.rs` |
| Digitizing | Photo Stitch 2 needle limits (OpenStitch mode) | weak: "≤ 97 % of PE mode's densest window" tested as ≤ 100 %; 0.1 mm units not asserted | #334: `crates/openstitch-digitize/tests/coverage_photo.rs` |
| Digitizing | Thread sources (T5) | weak: a single chart by id, and Photo Stitch 1 with `owned`, untested | #334: `crates/openstitch-engine/tests/coverage_digitize.rs` |
| Lettering | Fit text to outline | weak: the dialog remembering its last choices untested | #351: `crates/openstitch-app/src/app_coverage_tests.rs` |
| Editing | Boolean operations, offset line pattern (`shape.offsetLines`) | weak: "within 0.05 mm" tested at 0.15 mm; `roundedCorners`' effect not asserted | #351: `crates/openstitch-engine/tests/coverage_shape_ops.rs` |
| Editing | Split shape (`shape.split`) | weak: a curved shape keeping Bézier pieces not asserted | #351: `crates/openstitch-engine/tests/coverage_shape_ops.rs` |
| Editing | Centre point / pattern reference point | weak: cross, net, zigzag net and decorative fill, and spiral's centre point, untested | #351: `crates/openstitch-engine/tests/coverage_pattern_points.rs` (spiral's centre point found to have no effect: #354) |
| Automation | Ready-to-sew kit (`editor::sew_kit`) | weak: only the macOS layout tested | #351: `crates/openstitch-app/tests/coverage_editor_more.rs` (`tools/sew_kit.py --check` in CI not added: CI workflow) |
| Editor | Inspector | weak: choice and toggle controls not round-tripped | #351: `crates/openstitch-app/tests/coverage_editor_more.rs` |
| Lettering | Complex scripts, CJK | weak: `right_to_left_scripts_with_a_system_font` and `cjk_text_with_a_system_font` passed without checking when no suitable system font is installed; Indic reordering checked only as a script-run split | #345: Noto test subsets in `crates/openstitch-engine/tests/fixtures/fonts` (provenance and OFL there); `shaping.rs` `right_to_left_scripts_with_the_bundled_test_fonts`, `devanagari_vowel_sign_i_is_drawn_before_its_consonant`; `fonts.rs` `cjk_text_with_the_bundled_test_font` |
| Simulation | Solid View | weak: only the mode toggle tested, not the drawing (0.7 mm, unshaded) | #378: `crates/openstitch-app/src/app_coverage_tests.rs` (`draw_solid`'s shapes read back from egui's frame output) |
| Editor | Snap to Grid | weak: curve handles and the Arrange Copies handle untested | #379: `crates/openstitch-app/src/app_coverage_tests.rs` |
