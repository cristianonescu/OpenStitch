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

Updated 2026-11-04, for the round after v0.1.0-alpha.12.

## In flight (finishing as they are)

| PR | What | Owner |
|---|---|---|
| #299 | Sew-out round 2: locks, zigzag net, T1 card and laurel fixes, calibrated coverage, the regenerated kit | Windows |
| #361 | SVG import keeps `<line>` elements (#341) | Windows |
| #303 (session) | Fill rows clipped to a notch | Windows |
| #320 | Export Singer CSD and Viking SHV | Windows |
| #321 | Import WMF/EMF vector images as shapes | Windows |
| #363 | PES 0110: page, hoop, machine type and sections from the design (#322) | Windows |

## Mac batches

| Batch | Area | Issues | Notes |
|---|---|---|---|
| `batch:image-digitizing` | `openstitch-digitize` (autopunch, grabcut, photo, screen), the image dialog | #350, #355, #291, #129 | #350 is built on `feat/autopunch-strokes` (81d659b); #129 and #291 use the T1 card measurements |
| `batch:kit-calibration` | `tools/sew_kit.py`, `examples/sew`, `tools/sew_check.py` | #295, #311, #312 | Starts after #299 merges (it owns the kit until then) |
| `batch:ai-assistant` | `openstitch-assistant`, the assistant UI, `openstitch-mcp` | #364, #365 | #365 includes the first real-model run (Claude, a local model, a coding agent) |
| `batch:ci-tooling` | `.github/workflows`, Cargo profiles, test helpers | #357, #366, #367 | Release workflows need the Windows coordinator's OK |
| `batch:tests` | new test files only | #345 plus coverage gaps from `docs/TEST_COVERAGE_AUDIT.md` | #345 is built on `test/bundled-script-fonts` (c1e7aba) |

## Windows batches

| Batch | Area | Issues | Notes |
|---|---|---|---|
| `batch:stitch-engine` | `openstitch-design` (region.rs, regions.rs) | #304, #354, #288, Half Stitch for Auto satin, Miter limit | #288 closes once the regenerated T1 mark is checked (its cause was #297); the last two issues are to be opened |
| `batch:formats` | `openstitch-formats`, PES writers | PES versions 4, 5 and 7–10; audit item 9 (PE-made PES files lose objects on re-save) | After the in-flight format PRs; issues to be opened |
| `batch:lettering` | compile.rs, strokes.rs | stroke-wise fill lettering, Connection Points (audit items 6 and 11) | After `batch:stitch-engine` (both touch compile.rs) |

## Blocked

| Issue | Waiting for |
|---|---|
| #130 PMV motif/pattern reader | Owner-made files and screen access |
| #134 External scalable font containers | Owner-made files and screen access |
| #120 PES 0110 editable-object writer | The owner (the card for it was dismissed on 2026-11-04) |
| T1/T8 image-similarity steps | The calibration sew-outs and the counsel record (see `research/beyond/image-similarity-plan.md`) |
