# Releasing

Only the maintainer creates release tags:

- `vX.Y.Z` is a stable release.
- `vX.Y.Z-alpha.N`, `-beta.N` and `-rc.N` are pre-releases, marked as such on GitHub.

## Release gate

Before a release:

- Every open pull request is merged or resolved.
- CI on `main` is green on macOS, Windows and Linux, and every compatibility spec passes.
- The README is refreshed (below) and the release notes are honest about known differences.

## Merging pull requests

A pull request merges (squash) once its CI run for the current head commit is green on all
three operating systems. Scripts and agents that merge on green have to handle these cases:

- **Cancelled is not failed.** A new push to a pull request cancels that PR's run still in
  progress (the `concurrency` group in `rust.yml`). Until the new run finishes, `gh pr checks`
  lists the cancelled jobs as `fail`. Read the latest run of each workflow for the PR's head
  commit instead, and treat `cancelled` as still pending:

  ```bash
  gh run list --repo cristianonescu/OpenStitch --branch <head-branch> --workflow rust.yml --limit 1 --json headSha,status,conclusion,url
  ```

  Merge only when `headSha` is the PR's current head (`gh pr view <n> --json headRefOid`),
  `status` is `completed` and `conclusion` is `success`. `cancelled` with an older `headSha`
  means a newer run replaces it; wait for that one.
- **No run started.** GitHub starts no `pull_request` run while a PR conflicts with `main`, and
  marking a draft ready for review does not trigger one. After every push, check within a minute
  that a run for the new head exists. If none does, merge `main` in (for a conflict) or close and
  reopen the PR.
- **GitHub 5xx.** `gh` calls fail now and then with HTTP 502 or 503. Retry them after a short
  wait (for example three times, 10 s apart) and treat the result as unknown, not as a failure.
  `gh pr merge` can fail with a 502, or return, before the merge is done: read
  `gh pr view <n> --json state` and wait for `MERGED` before doing anything else.
- **Merge stuck.** If a PR reports "Merge already in progress" for more than about 5 minutes,
  close and reopen it, then merge again.
- **Deleting branches.** Deleting a branch closes every open PR based on it. Delete a PR's head
  branch only after its state is `MERGED`, and retarget PRs based on that branch to `main` first.

## README refresh

The README states what OpenStitch does, what it lacks, its known issues and what is planned,
with current screenshots. Refresh it in a pull request before every release:

1. **Status lists:** run `python -I tools/readme_status.py --issues`. It shows what changed in
   the generated blocks (status at a glance, not yet implemented, blocked, known issues, GitHub
   issues, planned beyond PE-DESIGN) from `docs/FEATURE_MATRIX.md`,
   `docs/COMPATIBILITY_STATUS.md`, `docs/COMPATIBILITY_AUDIT.md` and `gh issue list`. Then run it
   with `--issues --write`. This is the only place the GitHub issue lists and count are
   refreshed. Feature pull requests leave every generated block alone: CI's `readme_status
   --pr-guard` step fails a PR that changes one (#632). After a wave of merges the blocks are
   refreshed on `main` without `--issues`, in a docs-only PR from a `docs/readme-status-*`
   branch, which the guard skips. `--check` exits 1
   when a block derived from the repository is out of date. It refuses to run (exit 2, naming
   each file and line) while README.md or a source still holds merge-conflict markers, and
   warns when the matrix row count drops by more than a tenth (#406).
2. **Features:** add a line for each feature the release adds, in its group; mark features
   PE-DESIGN 11 does not have with **➕**. Take the facts from the FEATURE_MATRIX rows.
3. **Hand-written lists:** check "Also missing", "Other things to know" and "Planned" against
   `docs/ROADMAP.md`; remove what the release fixed.
4. **Release line:** set "Current release" in "Status at a glance" to the new tag and its notes.
5. **Screenshots and documentation images:** build the release binaries and regenerate every
   image under `docs/images` from that build. The editor is driven with `--script` and
   `--screenshot` only (no screen control):

   ```bash
   cargo build --release -p openstitch-app -p stitchtool -p openstitch-mcp
   ```

   ```bash
   python -I tools/docs_images.py
   ```

   It runs every generator (`tools/screenshots.py`, `tools/gallery.py`, `tools/engine_image.py`
   for the engine plots and the four templates, `tools/digitize_examples.py`, the Stitch Design
   Factory SVGs and the catalogue page; `--only NAME,NAME` picks some) and then its own
   `--check`, which CI's `kit` job also runs: every file under `docs/images` must have a
   generator or be one of the dated records listed in the script. `gallery.py` converts the
   test images in `examples/images` and rewrites the README's "From image to embroidery"
   gallery between its markers. Check that it shows OpenStitch's results only, never
   PE-DESIGN's. In screenshots the wizard dialogs open at the top right and the page is fitted
   beside them (#565); look at each image at full size.

   `docs/images/readme/` holds the README's set. A new feature worth showing gets a scene in
   `tools/screenshots.py` with a `docs/images/readme/` output and a cell in the README's
   Screenshots table. Never use PE-DESIGN screenshots or Brother artwork; new illustrations and
   icons are made with Higgsfield (see [LEGAL_AND_LICENSING.md](LEGAL_AND_LICENSING.md)).
6. **Ready-to-sew kit:** CI's `kit` job fails when a fresh release build of the kit differs
   from the committed `examples/sew` files or a file fails `tools/sew_check.py` (#450). After
   regenerating it (`python -I tools/sew_kit.py`), open every preview in
   `examples/sew/previews` at full size and look at the stitches before the release. The job
   also fails when a preview lacks one of its design's thread colours
   (`tools/sew_kit.py --check-previews`, #564).
7. **Links:** check that the README's links to docs and issues still resolve.

## Steps

1. Set `version` in the root `Cargo.toml` (for example `0.2.0`) through a normal pull request.
   A pre-release of that version uses the same number: `0.2.0` → `v0.2.0-beta.1`.
2. Add release notes as `docs/releases/<tag>.md`, also through a pull request, together with
   the README refresh.
3. Smoke-test the macOS package from the latest `main` run (download the
   `openstitch-main-<sha>-macos-universal` artifact first):

   ```bash
   tools/smoke-macos.sh artifact
   ```

   Launch the Windows and Linux packages once by hand; there is no script for them yet.
4. After the pull requests are merged, tag that commit on `main` and push the tag:

   ```bash
   git tag v0.2.0-beta.1
   ```

   ```bash
   git push origin v0.2.0-beta.1
   ```

5. The **Release** workflow then:
   - checks the tag's form, that `X.Y.Z` matches the workspace version, and that the tag is on
     `main`;
   - runs the full test suite and builds all three platforms;
   - creates a **draft** GitHub release with the packages. The notes start with
     `docs/releases/<tag>.md` when it exists, followed by the generated list of changes.

   Review the draft and publish it.

## Duplicate "OpenStitch 0.1.0" entries on Windows (#742)

Before #742 every MSI (each `main` build and each self-update, all version 0.1.0 with a new
ProductCode) installed beside the last instead of replacing it, so **Settings ▸ Apps** (or
*Programs and Features*) lists one "OpenStitch 0.1.0" per install, all in
`C:\Program Files\OpenStitch`. MSIs built since replace any installed OpenStitch of the same or
an older version (`AllowSameVersionUpgrades`; CI checks the built MSI's Upgrade table), and the
self-updater's `msiexec /i` relies on that.

To clean up a PC that already has several entries:

1. Install the newest MSI once (or let the editor update itself): it removes every older
   OpenStitch entry and leaves one.
2. If entries remain, uninstall them in **Settings ▸ Apps**, keeping the newest; Windows
   Installer counts shared files, so removing one entry leaves the files the others still use.
3. Open OpenStitch once from the Start menu to check it starts; **Repair** in Settings ▸ Apps
   restores anything missing.
