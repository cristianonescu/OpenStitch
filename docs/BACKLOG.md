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

Updated 2026-11-16: alpha.13 to alpha.17 have shipped their plans (summaries below).
Development is paused after alpha.17 until the owner says to resume. The open work is the
Future milestone. The sections after the summaries are the 2026-11-04 round, kept for history.

## Shipped in alpha.17 (2026-11-16)

4 pull requests merged after `v0.1.0-alpha.16`, all on 2026-11-16, small fixes the owner gave the
go-ahead for after alpha 16:

- **The Assistant:** the question card's recommended answer comes first, marked, and a question
  left open takes it (#1205); clearer wording on question cards and after an approved step
  (#1198).
- **Files:** a template opens as an untitled design, so it can no longer be saved over the
  project that was open; a test swatch's Open it asks about unsaved changes (#1179).
- **macOS:** Finder thumbnails and Quick Look previews now work, with the renderer linked into the
  extension (#1182).
- **Licences:** CI checks the third-party notices on every pull request, and the browser version
  publishes them and links them from Help ▸ About (#1175).
- **Tested:** automatic tests and CI on the three systems (with CI's macOS thumbnail check); the
  question card live with Claude Code on Windows; the Quick Look fix on a Mac on its branch
  build. The macOS release build was not checked by hand (the Mac is shut down), the Windows MSI
  was not installed, and real Safari was not run.

Still open and in the Future milestone: #1180 and #1170 (the Assistant panel), #1192 (laying out
only the Sewing Order rows on screen), and the parked #1043/#1071 and #1131/#1088.

Development is paused after alpha.17 until the owner says to resume.

What shipped, by area: [releases/v0.1.0-alpha.17.md](releases/v0.1.0-alpha.17.md).

## Shipped in alpha.16 (2026-11-16)

12 pull requests merged after `v0.1.0-alpha.15`, all on 2026-11-16, for what the owner asked for
after alpha 15. By area:

- **The Assistant:** a coding agent sees the pictures you attach (#1168); the chat is kept with
  the project and goes on where it stopped, with a setting to turn it off (#1169, #1176, #1178,
  #1187); it asks questions before a new design, suggests replies and offers fix chips (#1172);
  a coding agent waits for the answer on an approval card (#1188); it sees its own save (#1196),
  and every file it writes lands beside the design (#1199).
- **Closing:** quitting, closing a tab and replacing unsaved work always ask, in every
  permission mode; closing while the Assistant works asks first; a chat is unsaved work (#1186).
- **Thread colours:** colours named by thread code, never by hex value, with "≈" for the nearest
  thread (#1184); one Brother thread named two ways sews as one colour block (#1197).
- **Editor:** the Sewing Order list's small pictures are the objects' own stitches (#1185).
- **Documentation:** the README reworked for new users, every Markdown document checked, the
  third-party notices generated from `cargo metadata`, the docs images regenerated (#1171).
- **Tested:** Windows end to end, 19 of 19 with Claude Code and the real editor's tools on
  989a6e17; macOS phase 2 with Claude Code; the browser version in WebKit (not in a real Safari).

Still open and in the Future milestone: #1182 (macOS Finder thumbnails and Quick Look), #1179
(templates and test swatches keep the open file's path), #1175 (notices for the browser version,
and the CI check), #1198 (Assistant wording), #1180 and #1170 (the Assistant panel), and the
parked #1043/#1071 and #1131/#1088.

Development was paused after alpha.16, then resumed for alpha.17 by the owner's go-ahead.

What shipped, by area: [releases/v0.1.0-alpha.16.md](releases/v0.1.0-alpha.16.md).

## Shipped in alpha.15 (2026-11-16)

41 pull requests merged after `v0.1.0-alpha.14`, on 2026-11-14 and 2026-11-15. By area:

- **Stitches as PE-DESIGN 11.50 sews them:** a fill's under sewing path and needle points
  (#1088 in part, #1132), the radial fill (#1050), the plain net's start and default Spacing
  (#1071 in part, #1110), the zigzag net's Spacing and rungs (#1109, #1058), the programmable
  fill's default density (#1102), motif lines (#1068, #1123, #1130), start and end points of
  closed straight-line and imported SVG shapes (#1103, #1088), satin e (#1080), Text Transform
  (#1079, #1151, #1157), section starts that tie in (#1076), satin corners (#1040, #1048) and
  the skeleton satin's outline (#1136).
- **Formats:** PES 0110 keeps a motif line's Run pitch and Start/End Type (#1068), a
  candlewick's Density choice (#1142), a zigzag net's Spacing and Density (#1109, #1122) and a
  closed line's start (#1139); `.osd` version 6; safe saving (#1093).
- **Editor:** inspector ranges that hold every stored value (#1109, #1124), the Realistic
  view's freeze on piled-up rows (#1084), controls without effect removed (#1039, #1041).
- **Checks and tests:** Check Design's readiness level, crowded needle points, low coverage
  and small text per font (#1036), stitches in place counted on the file's grid (#1135), the
  settings test over each whole range (#1050, #1046), clean references (#1075).

Parked, still open:

- #1043 and #1071 (net fills: PE-DESIGN's travel along the outline and its stitch counts), on
  the branch `fix/zigzag-net-outline-travel-1043`, not merged.
- #1131 and #1088 (a fill's own row count on small squares).

Development pauses after this release by the owner's decision. Nothing is planned for a next
alpha; the open issues stay in the Future milestone.

What shipped, by area: [releases/v0.1.0-alpha.15.md](releases/v0.1.0-alpha.15.md).

## Shipped in alpha.14 (2026-11-14)

44 pull requests merged after `v0.1.0-alpha.13`, on 2026-11-13 and 2026-11-14. By area:

- **Stitches as PE-DESIGN 11.50 sews them:** the stitch in place on a Hoop Size page (#1013),
  joins sewn across (#987), satin letter starts (#1017), locks and tie-ins (#1032), the zigzag
  net's Direction (#1043, in part) and net fills' Direction (#1054).
- **Formats:** PES 0110 stamps (#959), groups, vertical text and motif lines and fills (#608),
  PE-DESIGN's default pattern taken out of every committed capture (#1060), the SHV preview's
  size and pixel values (#989, in part).
- **Editor:** Find objects (#745), smallest hoop and Fit to hoop (#879), Report a Problem
  (#878), the Design Settings page mode (#1056), the Realistic view's 4× MSAA (#907).
- **Checks and tests:** Check Design's new findings (#866), the setting-effect and monkey
  tests (#880), the private-details scan in CI (#1057).

Left open: #1043 (the zigzag net's travel along the outline), #989 (where the SHV preview's
lines fall), #1075 (clean captures for the withdrawn references) and #1024 (the PES 0110 object
kinds that need a PE-DESIGN capture first).

What shipped, by area: [releases/v0.1.0-alpha.14.md](releases/v0.1.0-alpha.14.md).

## Shipped in alpha.13 (2026-11-13)

The owner froze the scope of `v0.1.0-alpha.13` on 2026-11-05 (151 open issues then, in 36
batches by code area). By 2026-11-13 the milestone held 247 issues, and every one is closed
except #828, the release gate itself. The plan covered the stitch engine and satin quality,
lettering, fills, routing and fewer trims, PE-DESIGN formats and machine formats, Auto Punch,
Cross Stitch and the image wizard, the Assistant, quality checks, production tools, the editor
and its views, icons, CI and the ready-to-sew kit.

Moved to the Future milestone, still open: #130 (PMV reader), #134 (external scalable font
containers), #291 (Realistic view and scorer fitted to T1), #292 (sew-outs on other machines),
#298 (Photo Stitch 1 trims), #319 (macOS Ready-to-Sew click), #377 (Connection Points), #468
(Photo Stitch colours and needle drops) and #486 (more thread charts).

What shipped, by area: [releases/v0.1.0-alpha.13.md](releases/v0.1.0-alpha.13.md).

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

The alpha.13 plan's "Blocked: what each issue waits for" table was the current list until
2026-11-13; this one adds the waits that are not issues. Since 2026-11-13, #130, #134 and #377
are in the Future milestone (see "Shipped in alpha.13" above). #120 left this table on 2026-11-05: its triage found the writer done and
the rest ready (`batch:pe-formats`).

| Issue | Waiting for |
|---|---|
| #130 PMV motif/pattern reader | Owner-made files and screen access |
| #134 External scalable font containers | Owner-made files and screen access |
| ~~#377 Connection Points~~ | Closed as not planned on 2026-11-14. Was waiting for: owner captures in a built-in PE-DESIGN font (Connection Points is greyed out for TrueType fonts); the captures stay outside the repo and only measurements taken from them may be committed |
| T1/T8 image-similarity steps | The calibration sew-outs and the counsel record (see `research/beyond/image-similarity-plan.md`) |

## Session/batch brief template

Coordinators start every batch session with a brief that carries these lines, filled in:

- **Batch:** `batch:<name>`; issues #a #b #c (one commit each, "Closes #a #b #c"); code area and the files it owns.
- **Before the first edit:** an issue for every piece of work (search `gh issue list --state all --search` first; comment on a duplicate instead).
- **Graphics:** any icon, image, illustration, texture or other graphic is generated with Higgsfield, its job recorded in the `higgsfield-jobs.tsv` beside it, with a LEGAL entry; never an emoji, a painter-drawn glyph or an image from elsewhere. `tools/check_icons.py` (CI job `icons`) fails on any graphic under `assets/` without a jobs row.
- **Docs:** FEATURE_MATRIX, AUDIT and ROADMAP updated in the same PR.
- **Before every push:** `cargo fmt` and `cargo clippy`; every PES sent to the owner passes `sew_check`.
- **Finish:** one PR, watched until it merges; then clean up the branch and worktree, report what closed, what moved and any new issues, and stop.
