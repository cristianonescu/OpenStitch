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
| Files | Import SVG: `<line>` | **bug**: every `<line>` element is dropped on import (an equivalent `<path>` imports) | fix in `crates/openstitch-engine/src/imports.rs`; then un-ignore `svg_line_elements_import_as_running_lines` | #341 |
| Lettering | Per-character edits | weak: character moves on mirrored or on-path text untested | editor tests (`tools::characters`), with the other editor gaps | #332 |
| Lettering | Complex scripts, CJK | weak: `right_to_left_scripts_with_a_system_font` and `cjk_text_with_a_system_font` pass without checking when no suitable system font is installed | recorded here; needs a bundled test font, not only a test | #345 |
| Editor | Files: Import into design | weak: `EditorApp::import` (import, centre, select) untested | app tests or headless `--script` | #332 |
| Editor | Object list | weak: the visibility toggle's effect and undo untested | same | #332 |
| Editor | Undo/redo | weak: ⌘Z, ⇧⌘Z, Ctrl+Y only covered by a manual scripted run | same | #332 |
| Editor | Sewing simulator | weak: play at a speed, stop at the end untested | same | #332 |
| Editor | Inspector | weak: line settings never set; region density/length not read back | same | #332 |
| Editor | Select tool | weak: arrow-key nudge and Shift axis lock on move untested | same | #332 |
| Editing | Fabric Selector | weak: claims eight fabrics, test asserts at least six | same | #332 |
| Editor | Unsaved-changes prompt | weak: the window close request path untested | same | #332 |
| Digitizing | Photo Stitch 1 fine rows (OpenStitch mode) | weak: `densityMean`/`densityWindow` never sent; `travel` only in the wizard's request | engine `coverage_digitize.rs` on small images | #334 |
| Digitizing | Photo Stitch 2 needle limits (OpenStitch mode) | weak: "≤ 97 % of PE mode's densest window" tested as ≤ 100 %; 0.1 mm units not asserted | same file | #334 |
| Digitizing | Thread sources (T5) | weak: a single chart by id, and Photo Stitch 1 with `owned`, untested | same file | #334 |

Smaller weak points recorded but not planned yet: the Fit Text dialog remembering its last
choices; Solid View's drawing (only its toggle is tested); Snap to Grid for curve handles and the
Arrange handle; the editor's Optimise mode box and mixed-trim checkbox (inline egui code);
offset lines' "within 0.05 mm" (tested at 0.15 mm); `roundedCorners`' effect; the ready-to-sew
kit lookup on Linux and Windows layouts (`sew_kit` is being edited elsewhere); Windows MSI file
associations and the macOS `.pes` document type (CI workflow, owned by the CI session).

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
| Files | Import SVG | weak: `<ellipse>`, `<line>`, `<polygon>`, `<polyline>`, CSS `style=` untested | #329: `crates/openstitch-engine/tests/coverage_formats.rs` (`<line>` found broken: #341) |
| Lettering | Text Transform written to PES 0110 | weak: OpenStitch's own envelope and level writer untested | #329: `crates/openstitch-engine/tests/coverage_formats.rs` |
| Files | Machine formats vs PE-DESIGN reference | weak: `formats.rs` comparisons return early, and pass, if a reference file is missing | #329: `crates/openstitch-formats/tests/coverage_formats.rs` |
| Lettering | TrueType / OpenType fonts | weak: 16 fonts and 40 MiB per design untested | #330: `crates/openstitch-engine/tests/coverage_lettering.rs` (the layout-complexity refusal is not reachable through `text.add` with the bundled fonts, 2000 characters staying far under 600 000 segments; not tested) |
| Design Database | Search | weak: date modified had no positive test; a property-search test could pass without asserting | #331: `crates/openstitch-engine/tests/coverage_library.rs` (through `library.search`; the library's own conditional test is left as it is) |
