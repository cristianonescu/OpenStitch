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

## Blocked

| Issue | Waiting for |
|---|---|
| #130 PMV motif/pattern reader | Owner-made files and screen access |
| #134 External scalable font containers | Owner-made files and screen access |
| #120 PES 0110 editable-object writer | The owner (the card for it was dismissed on 2026-11-04) |
| #377 Connection Points | Owner captures in a built-in PE-DESIGN font (Connection Points is greyed out for TrueType fonts); the captures stay outside the repo and only measurements taken from them may be committed |
| T1/T8 image-similarity steps | The calibration sew-outs and the counsel record (see `research/beyond/image-similarity-plan.md`) |
