# Backlog: batches

OpenStitch's open work, grouped into batches. A batch is 3–6 related issues (or FEATURE_MATRIX
planned/partial rows), grouped by **code area first** (the same crates or files), then by
feature, so batches running in parallel do not conflict. One session takes one batch and ships
it as **one PR**: one commit per issue, "Closes #a #b #c", pushed when the whole batch is done
(under about 1,500 changed lines, no stacked PRs). The session watches the PR until it merges,
merging main in only on a conflict or a failure caused by main, and checks after every push that
CI started. It then reports what closed, what moved and any new issues, cleans up and stops; a
fresh session takes the next batch.

Issues in a batch carry the label `batch:<name>`. Merge order: P1 and release blockers, CI, the
Assistant, bugs, planned features, ideas.
The coordinators (Mac: image digitizing, the AI assistant, the kit and calibration, CI and
tooling, tests; Windows: the stitch engine, formats, lettering and the release gate) refresh
this file at each day's close-out.

Updated 2026-11-05: the alpha.13 plan below is the current list. The sections after it are the
2026-11-04 round, kept for history; where they name an issue, the plan's batch is the one that counts.

## alpha.13 plan (scope frozen 2026-11-05)

The owner froze the scope of `v0.1.0-alpha.13`: every open issue in the milestone (151 on
2026-11-05, main at 5374f5a) is implemented or fixed, and tested, before the release. Each issue
sits in exactly one batch below. Batches are grouped by code area so that two batches running at
the same time do not edit the same files; where they would, the later one names the earlier one
under **Depends on**. The rows are in the owner's priority order: P1 and release blockers, then
CI, then the Assistant, then bugs, then planned features, then ideas; blocked batches close the
table.

**Where a batch runs.** *Cloud* means a cloud session can do the whole batch: no Windows-only
check and no PE-DESIGN reference file that lives only on the Windows machine. *Windows* means it
needs one of these: PE-DESIGN reference files kept outside the repo, Windows-specific UI or
Assistant tests, the owner's sewing setup (kit regeneration that goes to the NV880E), or the
release gate. Sew-outs that an issue asks for "when the owner can" are not run by the session and
do not make a batch Windows-only.

**Status.** *running*: a session has the batch. *ready*: it can start (after its Depends on).
*blocked*: it waits for the owner or outside input, named in the notes.

This plan replaces the "Proposed batches from the open-source survey" table: its batches are
here, with the same names where they fit (`batch:ai-assistant` is now `batch:assistant-skill`,
`batch:lettering` (survey) is `batch:lettering-library`, `batch:formats` (survey) is the running
`batch:formats`, `batch:image-digitizing` (survey) is in `batch:auto-punch`), and their "next
round" and "ideas" issues have batches of their own.

| # | Batch | Issues | Crates / files | Depends on | Status | Runs on |
|---|---|---|---|---|---|---|
| 1 | `batch:photo-perf` | #433 (digitizing 8x slower since #299) | `openstitch-digitize` (photo.rs, coverage.rs), `openstitch-engine/tests/digitize.rs` | — | running (local) | Windows |
| 2 | `batch:kit-images` | #563 (fabric_cotton.png job id), #564 (kit preview colours), #565 (wizard screenshots, engine-templates generator) | `tools/sew_kit.py` (preview), `tools/screenshots.py`, `assets/textures` | — | running (local) | Windows |
| 3 | `batch:kit-calibration` | #441 (P1: sloppy kit designs and screenshots), #295 (P1: review every kit design), #311 (T2 calibration card), #312 (swatch density check in the kit build) | `tools/sew_kit.py`, `tools/sew_check.py`, `examples/sew`, README screenshots | `batch:kit-images` (same files) | ready | Windows (kit goes to the owner's NV880E; screenshots) |
| 4 | `batch:photo-stitch` | #298 (P1: Photo Stitch 1 trims), #291 (P1: Realistic view and scorer fitted to T1), #468 (Photo Stitch colours and needle drops), #129 (rotating-line coverage model) | `openstitch-digitize` (photo.rs, coverage.rs, perceptual.rs), `tools/image_similarity.py` | `batch:photo-perf` (photo.rs) | ready | Windows (PE-DESIGN Photo Stitch references, T1 scans) |
| 5 | `batch:pe-formats` | #120 (P1: `#PES0110` writer, remaining items), #397 (`import.pes` texts and stitch groups), #398 (CEmbLine variant in tatami-40), #349 (SHV and CSD open points) | `openstitch-formats` (pes.rs, pes11.rs, shv.rs, csd.rs), `openstitch-engine` (pes11_export.rs, imports) | `batch:formats` (both in `openstitch-formats`; different files, merge it first) | ready | Windows (PE-DESIGN reference files) |
| 6 | `batch:ci` | — (#403, #439, #356 closed by #566) | `.github/workflows`, Cargo profiles, `tools/check_icons.py` | — | running (merged #566) | — |
| 7 | `batch:ci-tooling` | #426 (flaky acp start test), #406 (`readme_status.py` refuses conflict markers), #496 (`stitchtool` one-line commands) | `openstitch-assistant/tests/acp.rs`, `tools/readme_status.py`, `stitchtool` | `batch:assistant-platforms` (acp.rs) | ready | Windows (#426 fails only on a loaded Windows machine) |
| 8 | `batch:assistant-platforms` | #401 (Assistant on Windows and Linux), #420 (Coding agent mode API key), #429 (gpt-oss streams stop), #425 (mistral-small3.2 empty answers) | `openstitch-assistant`, the Assistant panel | — | running | Windows |
| 9 | `batch:assistant-settings` | #431 (scratch_write file name asks permission), #432 (Ollama / LM Studio preset keeps the Claude model), #491 (Assistant memory between conversations) | `openstitch-assistant` (gate, settings, memory), `openstitch-app` (assistant_panel.rs) | `batch:assistant-platforms` (same crate) | ready | Windows (Assistant UI tests) |
| 10 | `batch:assistant-skill` | #492 (example designs, composition guide), #493 (illustrated bench tasks), #494 (every parameter error at once), #495 (only the user's threads) | `skills/openstitch-embroidery`, `openstitch-mcp`, `stitchtool` bench, `run_batch` | `batch:ci-tooling` (`stitchtool`) | ready | Cloud |
| 11 | `batch:stitch-engine` | #460 (region satin length), #461 (concentric/spiral/contour/radial seams), #462 (stippling), #463 (cross stitch, zigzag net), #464 (line sew types) | `openstitch-design` (region.rs, regions.rs, concentric.rs, contour.rs, spiral.rs, lines.rs) | — | running (local) | Windows |
| 12 | `batch:lettering` | #446 (satin N, X), #458 (Diamond and Circle monograms), #465 (small lettering) | `openstitch-design` (text.rs, strokes.rs, monogram.rs, compile.rs) | — | running (local) | Windows |
| 13 | `batch:stitch-engine-2` | #567 (move changes the stitch count), #459 (programmable fill), #467 (laurel and stars frames trims), #470 (appliqué wizard keeps the fill), #471 (direction-curve fill, piping), #368 (Storybook hairline satin) | `openstitch-design` (compile.rs, patterns.rs, ornament.rs, direction.rs, column.rs), `openstitch-engine` (appliqué wizard) | `batch:stitch-engine`, `batch:lettering` (compile.rs) | ready | Cloud |
| 14 | `batch:auto-punch` | #445 (cream background, fused letters), #447 (10 colours to 8 threads), #469 (halo thread, thin features), #489 (image wizard presets and region table) | `openstitch-digitize` (autopunch.rs, palette.rs, regions.rs, cross.rs), the image dialog | `batch:photo-stitch` (same crate) | ready | Windows (PE-DESIGN Auto Punch references) |
| 15 | `batch:editor-tools` | #472 (align, distribute), #473 (snapping), #474 (precision bar), #475 (select same), #476 (copies along a line, scatter) | `openstitch-app` (canvas, ribbon), `openstitch-engine` object commands | — | #472–#475 in review (#569); #476 follows in its own PR | Cloud |
| 16 | `batch:quality-checks` | #503 (Check Design), #504 (density map), #505 (Review Sewing Order), #506 (random-design test), #457 (export read-back) | `openstitch-engine` (quality module), `openstitch-render`, editor panes | — | running (cloud) | Cloud |
| 17 | `batch:formats` | #480 (VP3), #481 (more machine formats), #482 (thread list files), #488 (extra exports) | `openstitch-formats` (new writers and readers) | — | running (cloud): one stacked PR per issue, #480 VP3 first | Cloud |
| 18 | `batch:icons` | #408 (Design Database folder emoji), #409 (move up/down arrows), #410 (pane window controls), #411 (background Rotate symbols), #412 (menu ▾ caret), #413 (Change Color ■) | `assets/icons` (+ `higgsfield-jobs.tsv`), `openstitch-app` widgets, `tools/check_icons_allow.tsv` | `batch:editor-tools` (app widgets) | ready | Windows (Higgsfield generation and the jobs/LEGAL records) |
| 19 | `batch:satin-quality` | #466 (spikes at cusps, fans at apexes), #520 (corner styles), #521 (short stitches on every satin), #522 (pull compensation per side and %), #523 (automatic underlay by width), #526 (branching shape to satin) | `openstitch-design` (column.rs, skeleton.rs, region.rs), `openstitch-digitize` (columns.rs, for #526) | `batch:stitch-engine-2` (column.rs), `batch:auto-punch` (#526 in Auto Punch) | ready | Cloud |
| 20 | `batch:routing` | #545 (travel under later fills), #546 (branching and auto-routing), #547 (nearest start, Optimize report), #511 (leave out hidden stitching) | `openstitch-design` (compile.rs, route.rs), `openstitch-engine` (optimise) | `batch:stitch-engine-2` (compile.rs) | ready | Cloud |
| 21 | `batch:production` | #452 (test swatch of the selection), #453 (fabric calibration swatch), #507 (parameter test swatches), #508 (fabric and thread-weight presets) | `openstitch-engine` (wizards, fabric), `openstitch-design` (calibration.rs) | `batch:kit-calibration` (calibration cards) | ready | Cloud |
| 22 | `batch:production-output` | #384 (catalogue PDFs pile up in temp), #509 (basting box), #510 (knockdown fill), #512 (Stop after an object, Stop at Jumps), #513 (job sheets) | `openstitch-print`, `openstitch-engine` (wizards), `openstitch-formats` (stop codes) | `batch:production` (wizards), `batch:formats` | ready | Cloud |
| 23 | `batch:fills` | #532 (blends and gradients), #533 (maze and tiled meander), #534 (Fill Angle tool, angle lines), #537 (tatami textures), #538 (wave and flow fills), #539 (tartan fill, idea) | `openstitch-design` (region.rs fill generators, patterns.rs) | `batch:stitch-engine-2` | ready | Cloud (#538's per-region angle in Auto Punch waits on counsel's record; the fill types do not) |
| 24 | `batch:colour` | #484 (CIEDE2000 matching), #485 (merge similar colours), #483 (`.gpl` palettes), #486 (more thread charts), #487 (SVG clip, masks, gradients), #455 (SVG settings from names) | `openstitch-engine` (threads, imports.rs), `openstitch-core` (brands) | — | ready | Cloud |
| 25 | `batch:lettering-library` | #527 (font library and chooser), #530 (single-line Small Text), #529 (Ink/Stitch font folders), #535 (text envelopes), #536 (pixel font, idea) | `openstitch-design` (fonts.rs, envelope_presets.rs), the lettering pane | `batch:lettering` | ready | Windows (#530's compat spec needs an owner PE-DESIGN Small Text capture) |
| 26 | `batch:lettering-text` | #428 (fill text under a Text Transform), #456 (arched text, several lines), #548 (per-object trim and tie, trims between letters) | `openstitch-design` (text.rs, strokes.rs, compile.rs) | `batch:lettering-library`, `batch:routing` | ready | Cloud |
| 27 | `batch:views-hoops` | #516 (stitch-out player), #519 (millimetres or inches), #515 (free multi-hooping), #517 (view toggles), #518 (zoom box, real size), #514 (hoop lists) | `openstitch-app` (panels, canvas), `openstitch-design` (machine.rs, sections.rs) | `batch:editor-tools` (canvas) | ready | Cloud |
| 28 | `batch:ux` | #549 (autosave and recovery), #550 (command palette), #551 (custom shortcuts and layouts), #552 (tabs), #553 (new-object defaults), #554 (getting started) | `openstitch-app` (shell, settings, menus) | `batch:views-hoops` (app panels) | ready | Cloud |
| 29 | `batch:editor-shapes` | #477 (knife tool), #478 (smooth shapes and runs), #479 (continue a line from either end), #497 (more shape tools) | `openstitch-app` (canvas tools), `openstitch-design` (shapes.rs, paths.rs, boolean.rs) | `batch:editor-tools` | ready | Cloud |
| 30 | `batch:editor-objects` | #500 (system clipboard), #501 (stitch table, hand edits kept), #502 (sewing order list) | `openstitch-app` (panes), `openstitch-design` (stitch_edit.rs) | `batch:editor-shapes` | ready | Cloud |
| 31 | `batch:satin-2` | #524 (variable-width satin line), #525 (edit Manual Punch rungs), #528 (more satin looks), #531 (raised satin) | `openstitch-design` (column.rs, lines.rs), the canvas | `batch:satin-quality` | ready | Cloud |
| 32 | `batch:lines` | #542 (back stitch, variable running), #543 (decorative hand-embroidery lines), #544 (motif lines), #540 (hand-stitch look, idea), #541 (ripple line, idea) | `openstitch-design` (lines.rs, motifs) | `batch:stitch-engine` (#464 in lines.rs) | ready | Cloud |
| 33 | `batch:ux-2` | #499 (keyboard drawing, accessibility), #555 (translations), #558 (background open with Cancel), #559 (accent colour), #454 (garment mockups) | `openstitch-app` (theme, i18n, loading), `openstitch-render` (mockups) | `batch:ux` | ready | Windows (screen-reader check; #555 asks the owner which language) |
| 34 | `batch:platform-ideas` | #556 (WebAssembly editor), #498 (Python package), #490 (on-device picture helpers) | new crates / packaging, `openstitch-digitize` | `batch:ux-2`, `batch:auto-punch` | ready | Cloud (owner decisions before merge: PyPI publishing for #498, ML dependencies and model downloads for #490) |
| 35 | `batch:pe-captures` | #373 (Miter limit), #374 (PES versions 4, 5, 7–10), #377 (Connection Points) | `openstitch-design` (column.rs, text.rs), `openstitch-formats` (pes.rs) | — | blocked | Windows |
| 36 | `batch:owner-input` | #130 (PMV reader), #134 (Font Creator fonts), #557 (Finder / Explorer thumbnails), #292 (sew-outs on other machines), #319 (macOS Ready-to-Sew click) | `openstitch-formats`, `openstitch-design` (fonts.rs), OS integration | — | blocked | Windows (#319 on a Mac) |

### Blocked: what each issue waits for

| Issue | Waiting for |
|---|---|
| #373 Miter limit | The owner's PE-DESIGN captures of the Miter limit setting on satin and zigzag corners (black-box) |
| #374 PES versions 4, 5, 7–10 | The owner's PE-DESIGN Save As captures in each version |
| #377 Connection Points | The owner's captures in a built-in PE-DESIGN font (Connection Points is greyed out for TrueType fonts); only measurements are committed |
| #130 PMV reader | Owner-made PMV files and screen access to PE-DESIGN |
| #134 Font Creator fonts | Screen access to PE-DESIGN's Font Creator and owner-drawn `.pef` files (built-in `.pf1` fonts are out of scope) |
| #557 Finder / Explorer thumbnails | The owner's decision on a non-Rust shim or an `unsafe` exception (the issue says so); otherwise closed as won't-do |
| #292 Sew-outs on other machines | Sew-out reports from users of other machines (help wanted); the owner decides whether alpha.13 waits for them |
| #319 macOS Ready-to-Sew click | A Mac session with real screen control (background presses do not reach egui's menus) |

Not issues, still waiting: the T1/T8 image-similarity steps wait for the calibration sew-outs and
the counsel record (`research/beyond/image-similarity-plan.md`).

### Totals

- **36 batches** for 151 issues (`batch:ci` is merged and holds no open issue).
- **By status:** running 9 batches / 30 issues; ready 25 batches / 113 issues; blocked 2 batches / 8 issues.
- **Where:** Cloud 19 batches / 89 issues; Windows 16 batches / 62 issues; `batch:ci` (merged) not counted.

## In flight on 2026-11-04 (history)

| PR | What | Owner |
|---|---|---|
| #299 | Sew-out round 2: locks, zigzag net, T1 card and laurel fixes, calibrated coverage, the regenerated kit | Windows |
| #369 | Export Singer CSD and Viking SHV (#320) | Windows |
| #321 (branch, PR after #369) | Import WMF/EMF vector images as shapes | Windows |
| #395 | Assistant panel polish (#394): framed icon buttons, chat bubbles, light Markdown, keys read back after Remove, throwaway profile (`OPENSTITCH_CONFIG_DIR`, `OPENSTITCH_KEYCHAIN_SERVICE`) | Mac |
| `batch:ci-tooling` (branch) | #357, #366, #367 | Mac |
| `batch:quality-checks` (PR) | #506 random-design test, #503 Check Design, #504 density map, #505 Review Sewing Order, #457 export read-back: engine `quality.rs`, `design.check` / `design.density` / `design.route` / `design.cleanup`, the editor's Check Design window. New bugs from the random-design test: #570, #571 (and seeds on #373) | Cloud session |

## Mac batches

| Batch | Area | Issues | Notes |
|---|---|---|---|
| `batch:image-digitizing` (round 2) | `openstitch-digitize` (photo, the scorer), the image dialog | #291, #129 | After #299 merges (photo.rs). #129's design record and the earlier WIP diff are on the issue |
| `batch:kit-calibration` | `tools/sew_kit.py`, `examples/sew`, `tools/sew_check.py` | #295, #311, #312 | Starts after #299 merges (it owns the kit until then) |
| `batch:ai-verify` | `openstitch-assistant`, the assistant UI | #390 (Claude Code loads the user's own skills), #392 (Ollama run, network off), #393 (composer controls by hand) | After #395. Real runs use a throwaway profile, never the user's key or settings |
| `batch:icons` | `assets/icons`, `assistant_panel.rs`, a CI source scan | #399 (Higgsfield attach, close and settings icons), #400 (guard: no emoji or hand-drawn icons, every icon has a Higgsfield job) | In review. `tools/check_icons.py` runs in CI (job `icons`); the cases it found are allow-listed in `tools/check_icons_allow.tsv` and removed by #408–#413 |
| `batch:ci-tooling` | `.github/workflows`, Cargo profiles, test helpers | #357, #366, #367 | In progress; release workflows need the Windows coordinator's OK |

## Windows batches

| Batch | Area | Issues | Notes |
|---|---|---|---|
| `batch:stitch-engine` | `openstitch-design` (region.rs, regions.rs) | #304, #354, #288, #372 (Half Stitch for Auto satin), #373 (Miter limit) | #288 closes once the regenerated T1 mark is checked (its cause was #297) |
| `batch:formats` | `openstitch-formats`, PES writers | #374 (PES versions 4, 5 and 7–10), #375 (PE-made PES files lose objects on re-save, audit item 9) | After the in-flight format PRs |
| `batch:lettering` | compile.rs, strokes.rs | #376 (stroke-wise fill lettering) | After `batch:stitch-engine` (both touch compile.rs) |
| `batch:assistant-platforms` | the Assistant on Windows (and Linux if CI can host a Secret Service) | #401 | After #395. Throwaway profile only; a real Claude API turn needs the owner's own key, otherwise real turns go through Claude Code |

## Blocked

The alpha.13 plan's "Blocked: what each issue waits for" table is the current list; this one adds the
waits that are not issues. #120 left this table on 2026-11-05: its triage found the writer done and
the rest ready (`batch:pe-formats`).

| Issue | Waiting for |
|---|---|
| #130 PMV motif/pattern reader | Owner-made files and screen access |
| #134 External scalable font containers | Owner-made files and screen access |
| #377 Connection Points | Owner captures in a built-in PE-DESIGN font (Connection Points is greyed out for TrueType fonts); the captures stay outside the repo and only measurements taken from them may be committed |
| T1/T8 image-similarity steps | The calibration sew-outs and the counsel record (see `research/beyond/image-similarity-plan.md`) |

## Session/batch brief template

Coordinators start every batch session with a brief that carries these lines, filled in:

- **Batch:** `batch:<name>`; issues #a #b #c (one commit each, "Closes #a #b #c"); code area and the files it owns.
- **Before the first edit:** an issue for every piece of work (search `gh issue list --state all --search` first; comment on a duplicate instead).
- **Graphics:** any icon, image, illustration, texture or other graphic is generated with Higgsfield, its job recorded in the `higgsfield-jobs.tsv` beside it, with a LEGAL entry; never an emoji, a painter-drawn glyph or an image from elsewhere. `tools/check_icons.py` (CI job `icons`) fails on any graphic under `assets/` without a jobs row.
- **Docs:** FEATURE_MATRIX, AUDIT and ROADMAP updated in the same PR.
- **Before every push:** `cargo fmt` and `cargo clippy`; every PES sent to the owner passes `sew_check`.
- **Finish:** one PR, watched until it merges; then clean up the branch and worktree, report what closed, what moved and any new issues, and stop.
