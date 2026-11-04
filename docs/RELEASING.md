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
   refreshed; pull requests regenerate the other blocks without `--issues`. `--check` exits 1
   when a block derived from the repository is out of date.
2. **Features:** add a line for each feature the release adds, in its group; mark features
   PE-DESIGN 11 does not have with **➕**. Take the facts from the FEATURE_MATRIX rows.
3. **Hand-written lists:** check "Also missing", "Other things to know" and "Planned" against
   `docs/ROADMAP.md`; remove what the release fixed.
4. **Release line:** set "Current release" in "Status at a glance" to the new tag and its notes.
5. **Screenshots:** build the app and regenerate them from that build. Drive the editor with
   `--script` and `--screenshot` only (no screen control):

   ```bash
   cargo build -p openstitch-app -p stitchtool
   ```

   ```bash
   python -I tools/screenshots.py
   ```

   ```bash
   python -I tools/gallery.py
   ```

   The second command converts the test images in `examples/images` and rewrites the README's
   "From image to embroidery" gallery between its markers. Check that it shows OpenStitch's
   results only, never PE-DESIGN's.

   `docs/images/readme/` holds the README's set. A new feature worth showing gets a scene in
   `tools/screenshots.py` with a `docs/images/readme/` output and a cell in the README's
   Screenshots table. Never use PE-DESIGN screenshots or Brother artwork; new illustrations and
   icons are made with Higgsfield (see [LEGAL_AND_LICENSING.md](LEGAL_AND_LICENSING.md)).
6. **Links:** check that the README's links to docs and issues still resolve.

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
