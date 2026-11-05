# Backlog: batches

OpenStitch's open work, grouped into batches. A batch is 3–6 related issues (or FEATURE_MATRIX
planned/partial rows), grouped by **code area first** (the same crates or files), then by
feature, so batches running in parallel do not conflict. One session takes one batch and ships
it as **one PR**: one commit per issue, "Closes #a #b #c", pushed when the whole batch is done
(under about 1,500 changed lines, no stacked PRs). The session watches the PR until it merges,
merging main in only on a conflict or a failure caused by main, and checks after every push that
CI started. It then reports what closed, what moved and any new issues, cleans up and stops; a
fresh session takes the next batch.

Issues in a batch carry the label `batch:<name>`. Merge order: CI, then bugs, then features.
The coordinators (Mac: image digitizing, the AI assistant, the kit and calibration, CI and
tooling, tests; Windows: the stitch engine, formats, lettering and the release gate) refresh
this file at each day's close-out.

Updated 2026-11-04 evening, for the round after v0.1.0-alpha.12. Merged since the morning: #361, #363, #371, #381, #382, #383, #387.

## In flight (finishing as they are)

| PR | What | Owner |
|---|---|---|
| #299 | Sew-out round 2: locks, zigzag net, T1 card and laurel fixes, calibrated coverage, the regenerated kit | Windows |
| #369 | Export Singer CSD and Viking SHV (#320) | Windows |
| #321 (branch, PR after #369) | Import WMF/EMF vector images as shapes | Windows |
| #395 | Assistant panel polish (#394): framed icon buttons, chat bubbles, light Markdown, keys read back after Remove, throwaway profile (`OPENSTITCH_CONFIG_DIR`, `OPENSTITCH_KEYCHAIN_SERVICE`) | Mac |
| `batch:ci-tooling` (branch) | #357, #366, #367 | Mac |

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

## Proposed batches from the open-source survey (2026-11-05)

The 88 issues from the full survey of Morale, threadspool, Roidery, stitch_generator, openhoop and
filum (#472–#559), grouped by code area. They come after the batches above and after the bugs
#458–#471 (merge order: release blockers and P1, CI, Assistant, bugs, planned features, ideas).
No `batch:*` label is set yet: the coordinator labels a batch when a session takes it. The order
below is the suggested order of taking them.

| Proposed batch | Area | Issues | Notes |
|---|---|---|---|
| `batch:ai-assistant` (survey) | `skills/openstitch-embroidery`, `openstitch-mcp`, `openstitch-assistant`, `stitchtool` bench | #491 (memory), #492 (example designs, composition guide), #493 (illustrated bench tasks), #494 (every batch error at once), #495 (only the user's threads) | Assistant comes before bugs in the merge order |
| `batch:quality-checks` | `openstitch-engine` (a new quality module), `openstitch-render`, the editor's panes | #503 (Check Design), #504 (density map), #505 (Review Sewing Order), #457 (export read-back), #506 (random-design test) | #506 first: it guards the others |
| `batch:satin-quality` | `openstitch-design` (column.rs, region.rs, skeleton.rs, lines.rs) | #520 (corner styles), #521 (short stitches on every satin), #522 (compensation per side and %), #523 (automatic underlay by width), #526 (branching shapes to satin) | After `batch:stitch-engine`; next round: #524, #525, #528, #531 |
| `batch:routing` | `openstitch-design` (compile.rs, order), `openstitch-engine` | #545 (travel under later fills), #546 (branching and auto-routing), #547 (nearest start), #511 (leave out hidden stitching), #548 (per-object trim and tie) | Needs a sew-out per change |
| `batch:production` | `openstitch-engine` (wizards, fabric), `openstitch-print` | #507 (parameter test swatches), #508 (fabric and thread-weight presets), #509 (basting box), #510 (knockdown), #512 (Stop at Jumps), #513 (job sheets) | With #452 and #453 |
| `batch:fills` | `openstitch-design` (fill generators) | #532 (blends and gradients), #534 (Fill Angle tool), #533 (maze and tiled meander), #537 (tatami textures), #538 (wave and flow fills) | Ideas #539, #540, #541 later |
| `batch:lines` | `openstitch-design` (lines.rs, motifs) | #542 (back stitch, variable running), #543 (decorative hand-embroidery lines), #544 (motif lines) | After the line bugs #464 |
| `batch:editor-tools` | `openstitch-app` (canvas, ribbon), `openstitch-engine` object commands | #472 (align, distribute), #474 (precision bar), #476 (copies along a line, scatter), #473 (snapping), #475 (select same) | In review (2026-11-05): #472–#475 in one PR (`engine/src/layout.rs`, `app/src/editor/snap.rs`, `editor/precision.rs`, `src/arrange_ui.rs`); #476 (`drawing.rs` `object.arrangeCopies` modes) follows in its own PR to keep the first under the size limit. Next round: #477, #478, #479, #497, #500, #501, #502 |
| `batch:lettering` (survey) | `openstitch-design` (fonts, text), the lettering pane | #527 (font library with size ranges), #530 (single-line text), #529 (Ink/Stitch font folders), #535 (envelopes) | After `batch:lettering`; idea #536 |
| `batch:formats` (survey) | `openstitch-formats` | #480 (VP3), #481 (more machine formats), #482 (thread list files), #488 (extra exports) | After the current `batch:formats` |
| `batch:colour` | `openstitch-engine` (threads, imports.rs), `openstitch-core` (brands) | #484 (perceptual thread matching), #485 (merge similar colours), #483 (`.gpl` palettes), #486 (more charts), #487 (richer SVG import) | |
| `batch:views-hoops` | `openstitch-app` (panels, canvas), `openstitch-design` (machine.rs, sections.rs) | #516 (stitch-out player), #519 (millimetres or inches), #515 (free multi-hooping), #517 (view toggles), #518 (zoom box, real size), #514 (hoop lists) | |
| `batch:image-digitizing` (survey) | `openstitch-digitize`, the image dialog | #489 (presets, region table) | After the current image round; idea #490 |
| `batch:ux` | `openstitch-app` | #549 (autosave and recovery), #550 (command palette), #551 (custom UI), #552 (tabs), #553 (new-object defaults), #554 (getting started) | Next round: #555, #557, #499, #496; ideas #556, #558, #559, #498 |

## Blocked

| Issue | Waiting for |
|---|---|
| #130 PMV motif/pattern reader | Owner-made files and screen access |
| #134 External scalable font containers | Owner-made files and screen access |
| #120 PES 0110 editable-object writer | The owner (the card for it was dismissed on 2026-11-04) |
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
