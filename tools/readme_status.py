#!/usr/bin/env python3
"""Regenerates the status blocks of README.md from the project's own records.

    python -I tools/readme_status.py                    # show what would change, write nothing
    python -I tools/readme_status.py --write            # update README.md
    python -I tools/readme_status.py --check            # exit 1 if README.md is out of date
    python -I tools/readme_status.py --issues --write   # also refresh the GitHub issue lists
    python -I tools/readme_status.py --self-test        # check the guards below (CI runs it)

It refuses to run (exit 2, naming each file and line) while README.md or any source below holds
a merge-conflict marker (`<<<<<<< `, `||||||| `, `=======`, `>>>>>>> ` at the start of a line;
a Markdown setext underline of exactly seven `=` reads the same, so the docs use `#` headings),
and warns when the matrix row count drops by more than a tenth (#406).

Pull requests run it without --issues: the issue lists and the open-issue count change whenever
anyone opens or closes an issue, so they are refreshed once per release (docs/RELEASING.md)
instead of in every PR, where they went stale within minutes and conflicted with the next PR.
Without --issues those parts of README.md are kept as they are.

Sources:
- docs/FEATURE_MATRIX.md: the row counts per status, the Blocked and Documented rows (not yet
  implemented) and the "Planned (beyond PE-DESIGN)" rows.
- docs/COMPATIBILITY_STATUS.md: the spec totals and how many matrix rows reach each tier.
- docs/COMPATIBILITY_AUDIT.md: the summary items not marked **fixed** (known differences).
- `gh issue list` on cristianonescu/OpenStitch, with --issues only: open issues (bugs under known
  issues, the rest under planned) and their count in the glance table.

Each block sits between `<!-- status:NAME -->` and `<!-- /status:NAME -->` in README.md; the
text around the blocks is written by hand and is never touched. A Documented row whose feature
also has an Implemented row (the matrix keeps the guide's row next to the one that implemented
it) is left out of the not-yet-implemented list.
"""

import argparse
import difflib
import json
import re
import shutil
import subprocess
import sys
from collections import Counter
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
README = ROOT / "README.md"
MATRIX = ROOT / "docs" / "FEATURE_MATRIX.md"
STATUS = ROOT / "docs" / "COMPATIBILITY_STATUS.md"
AUDIT = ROOT / "docs" / "COMPATIBILITY_AUDIT.md"
REPO = "cristianonescu/OpenStitch"
ISSUES = f"https://github.com/{REPO}/issues"

# Git's conflict markers (#406): `<<<<<<< ours`, `||||||| base` (diff3), `=======`,
# `>>>>>>> theirs`. A table parse stops at the first one, so the counts would silently shrink.
MARKER = re.compile(r"^(?:<{7}|\|{7}|>{7})(?: |$)|^={7}$")


def conflict_markers(paths):
    """(path, line number, line) of every conflict-marker line in `paths`."""
    found = []
    for path in paths:
        text = path.read_text(encoding="utf-8")
        for n, line in enumerate(text.splitlines(), 1):
            if MARKER.match(line):
                found.append((path, n, line))
    return found


def matrix_row_count(readme):
    """The glance table's matrix row count in README.md, or None."""
    m = re.search(r"^\| Feature matrix rows \| (\d+):", readme, re.M)
    return int(m.group(1)) if m else None


def shrank(before, after):
    """Whether the matrix row count dropped sharply (more than a tenth): a sign of a cut-short
    parse rather than rows really removed."""
    return before is not None and after < before * 0.9


BLOCK = re.compile(r"<!-- status:([\w-]+) -->\n(.*?)<!-- /status:\1 -->", re.S)


def status_blocks(text):
    """The generated blocks of a README text, by name."""
    return {m.group(1): m.group(2) for m in BLOCK.finditer(text)}


def changed_blocks(base_text, head_text):
    """Names of the generated blocks a change adds, removes or edits (#632)."""
    base, head = status_blocks(base_text), status_blocks(head_text)
    return sorted(n for n in base.keys() | head.keys() if base.get(n) != head.get(n))


def pr_guard(base_ref):
    """Feature PRs leave the generated README blocks alone (#632): they changed on nearly every
    merge and turned green PRs CONFLICTING. The coordinator refreshes them on main in a
    docs-only PR (branch `docs/readme-status-*`, which CI skips)."""
    import subprocess
    try:
        base = subprocess.run(["git", "show", f"{base_ref}:README.md"], cwd=ROOT, check=True,
                              capture_output=True).stdout.decode("utf-8")
    except (OSError, subprocess.CalledProcessError) as e:
        print(f"cannot read README.md at {base_ref}: {e}", file=sys.stderr)
        return 2
    changed = changed_blocks(base, README.read_text(encoding="utf-8"))
    if not changed:
        print("README.md generated blocks are untouched.")
        return 0
    for name in changed:
        print(f"README.md: the generated <!-- status:{name} --> block differs from {base_ref}",
              file=sys.stderr)
    print("Feature PRs don't edit the generated README blocks (#632): restore them with "
          f"`git checkout {base_ref} -- README.md` and re-add your hand-written lines. The "
          "coordinator refreshes the blocks on main after merging.", file=sys.stderr)
    return 1


def self_test():
    """Conflict markers are found with file and line; ordinary lines are not."""
    import tempfile
    with tempfile.TemporaryDirectory() as d:
        p = Path(d) / "FEATURE_MATRIX.md"
        p.write_text("| Area | Feature | Status |\n|---|---|---|\n<<<<<<< HEAD\n| a | b | c |\n"
                     "||||||| base\n=======\n| a | b | d |\n>>>>>>> feature\n"
                     "Heading\n=======\nx ======= y\n<<<<<<<<\n", encoding="utf-8")
        got = [(n, line) for _, n, line in conflict_markers([p])]
        want = [(3, "<<<<<<< HEAD"), (5, "||||||| base"), (6, "======="),
                (8, ">>>>>>> feature"), (10, "=======")]
        assert got == want, got
        clean = Path(d) / "README.md"
        clean.write_text("| Feature matrix rows | 120: 100 implemented |\n== x ==\n",
                         encoding="utf-8")
        assert conflict_markers([clean]) == []
        assert matrix_row_count(clean.read_text(encoding="utf-8")) == 120
    assert shrank(120, 24) and not shrank(120, 115) and not shrank(None, 3)
    base = "x\n<!-- status:glance -->\n| a | 1 |\n<!-- /status:glance -->\nhand\n"
    assert changed_blocks(base, base.replace("hand", "hand, edited")) == []
    assert changed_blocks(base, base.replace("| a | 1 |", "| a | 2 |")) == ["glance"]
    assert changed_blocks(base, base + "<!-- status:new -->\ny\n<!-- /status:new -->\n") == ["new"]
    print("readme_status self-test passed.")
    return 0


def table_rows(text, header):
    """Rows of the first Markdown table whose header line starts with `header`."""
    rows, inside = [], False
    for line in text.splitlines():
        if line.startswith(header):
            inside = True
            continue
        if inside:
            if not line.startswith("|"):
                break
            if line.startswith("|---"):
                continue
            rows.append([c.strip() for c in line.strip().strip("|").split(" | ")])
    return rows


def kind(status):
    s = status.lower()
    if s.startswith("planned"):
        return "Planned (beyond PE-DESIGN)"
    for k in ("Verified Compatible", "Partially Compatible", "Implemented", "Blocked",
              "Documented", "Not Investigated"):
        if s.startswith(k.lower()):
            return k
    return status


def short(feature, limit=110):
    """The feature's name: the text before its first colon or semicolon, without short
    parenthetical notes."""
    name = re.split(r": |; ", feature, maxsplit=1)[0]
    name = re.sub(r" \([^():]{1,40}\)", "", name)
    name = re.split(r" \(", name, maxsplit=1)[0].strip().rstrip(".").replace("`", "")
    return name if len(name) <= limit else name[: limit - 1].rstrip() + "…"


def matrix():
    rows = table_rows(MATRIX.read_text(encoding="utf-8"), "| Area | Feature | Status")
    rows = [r for r in rows if len(r) >= 3]
    documented = [r for r in rows if kind(r[2]) == "Documented"]
    blocked = [r for r in rows if kind(r[2]) == "Blocked"]
    planned = [r for r in rows if kind(r[2]).startswith("Planned")]
    counts = Counter(kind(r[2]) for r in rows)
    return rows, counts, documented, blocked, planned


def compat():
    text = STATUS.read_text(encoding="utf-8")
    m = re.search(r"Specs: (\d+) · pass (\d+) · fail (\d+)", text)
    specs = tuple(int(x) for x in m.groups()) if m else (0, 0, 0)
    rows = table_rows(text, "| Area | Feature | Tier")
    tiers = Counter()
    for r in rows:
        # A row is only as far as its weakest spec: the first tier listed.
        tiers[r[2].split(";")[0].strip()] += 1
    return specs, tiers


def anchor(heading):
    a = re.sub(r"[^\w\- ]", "", heading.strip().lower())
    return a.replace(" ", "-")


def audit():
    text = AUDIT.read_text(encoding="utf-8")
    heads = {}
    for line in text.splitlines():
        m = re.match(r"## (\d+)\. (.*)", line)
        if m:
            heads[m.group(1)] = anchor(f"{m.group(1)}. {m.group(2)}")
    items = []
    for r in table_rows(text, "| # | Difference"):
        num, diff = r[0], r[1]
        if "**fixed**" in diff:
            continue
        partly = "**partly fixed**" in diff
        title = re.split(r" \(\*\*", diff, maxsplit=1)[0].strip()
        items.append((num, title, partly, heads.get(num, "")))
    return items


def issues(offline):
    if offline:
        return None
    gh = shutil.which("gh") or r"C:\Program Files\GitHub CLI\gh.exe"
    try:
        out = subprocess.run(
            [gh, "issue", "list", "--repo", REPO, "--state", "open", "--limit", "200",
             "--json", "number,title,labels"],
            capture_output=True, text=True, encoding="utf-8", check=True, timeout=60).stdout
    except (OSError, subprocess.SubprocessError) as e:
        print(f"gh issue list failed ({e}); issue blocks left as they are", file=sys.stderr)
        return None
    found = json.loads(out)
    found.sort(key=lambda i: i["number"])
    return found


def link_list(entries):
    return "\n".join(entries) if entries else "_None._"


def issue_row(readme):
    """The glance table's existing open-issue row, kept as it is when the issues are not fetched."""
    m = re.search(r"^\| Open GitHub issues \|.*$", readme, re.M)
    return [m.group(0)] if m else []


def blocks(offline, readme):
    rows, counts, documented, blocked, planned = matrix()
    (total, passed, failed), tiers = compat()
    open_audit = audit()
    found = issues(offline)

    out = {}
    done = counts["Implemented"] + counts["Partially Compatible"] + counts["Verified Compatible"]
    tier_text = ", ".join(f"{n} at the {t} tier" for t, n in
                          sorted(tiers.items(), key=lambda x: -x[1]))
    out["glance"] = "\n".join([
        "| | |",
        "|---|---|",
        f"| Feature matrix rows | {len(rows)}: {done} implemented"
        f" ({counts['Partially Compatible']} of them partially compatible),"
        f" {counts['Documented']} documented only, {counts['Blocked']} blocked,"
        f" {counts['Planned (beyond PE-DESIGN)']} planned beyond PE-DESIGN |",
        f"| Compatibility specs | {total}: {passed} pass, {failed} fail"
        f" ([status](docs/COMPATIBILITY_STATUS.md)) |",
        f"| Matrix rows covered by specs | {sum(tiers.values())}: {tier_text} |",
        f"| Open audit differences | {len(open_audit)}"
        f" ([audit](docs/COMPATIBILITY_AUDIT.md#summary)) |",
    ] + ([f"| Open GitHub issues | {len(found)} ([list]({ISSUES})) |"] if found is not None
         else issue_row(readme)))

    out["not-implemented"] = link_list(
        [f"- **{short(r[1])}** ({r[0]})" for r in documented])
    out["blocked"] = link_list(
        [f"- **{short(r[1])}** ({r[0]})" for r in blocked])
    out["known-issues"] = link_list(
        [f"- [{title}](docs/COMPATIBILITY_AUDIT.md#{a})" + (" (partly fixed)" if partly else "")
         for num, title, partly, a in open_audit])
    out["beyond-planned"] = link_list([f"- {r[1].replace('`', '')}" for r in planned])
    if found is not None:
        bugs = [i for i in found if any(lb["name"] == "bug" for lb in i["labels"])]
        rest = [i for i in found if i not in bugs]
        out["issues-bugs"] = link_list(
            [f"- [#{i['number']}]({ISSUES}/{i['number']}) {i['title']}" for i in bugs])
        out["issues-planned"] = link_list(
            [f"- [#{i['number']}]({ISSUES}/{i['number']}) {i['title']}" for i in rest])
    return out


def apply(text, generated):
    missing = []
    for name, body in generated.items():
        tag = re.escape(name)
        pattern = re.compile(rf"(<!-- status:{tag} -->\n)(?:.*?\n)?(<!-- /status:{tag} -->)", re.S)
        if not pattern.search(text):
            missing.append(name)
            continue
        text = pattern.sub(lambda m: m.group(1) + body + "\n" + m.group(2), text)
    return text, missing


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    p = argparse.ArgumentParser()
    p.add_argument("--write", action="store_true", help="update README.md")
    p.add_argument("--check", action="store_true", help="exit 1 if README.md is out of date")
    p.add_argument("--issues", action="store_true",
                   help="also refresh the GitHub issue lists and count (release gate)")
    p.add_argument("--offline", action="store_true",
                   help="skip the GitHub issue list (the default; kept for old commands)")
    p.add_argument("--self-test", action="store_true",
                   help="check the conflict-marker and row-count guards, then exit")
    p.add_argument("--pr-guard", metavar="BASE",
                   help="exit 1 if README.md's generated blocks differ from git ref BASE (#632)")
    a = p.parse_args()
    if a.self_test:
        return self_test()
    if a.pr_guard:
        return pr_guard(a.pr_guard)

    markers = conflict_markers([README, MATRIX, STATUS, AUDIT])
    if markers:
        for path, n, line in markers:
            print(f"{path.relative_to(ROOT).as_posix()}:{n}: merge-conflict marker {line!r}",
                  file=sys.stderr)
        print("Resolve the conflicts first; nothing was written.", file=sys.stderr)
        return 2

    old = README.read_text(encoding="utf-8")
    new, missing = apply(old, blocks(a.offline or not a.issues, old))
    before, after = matrix_row_count(old), matrix_row_count(new)
    if after is not None and shrank(before, after):
        print(f"warning: feature matrix rows drop from {before} to {after}; check "
              "docs/FEATURE_MATRIX.md before writing", file=sys.stderr)
    for name in missing:
        print(f"README.md has no <!-- status:{name} --> block", file=sys.stderr)
    diff = list(difflib.unified_diff(old.splitlines(), new.splitlines(), "README.md (now)",
                                     "README.md (generated)", lineterm="", n=0))
    if not diff:
        print("README.md status blocks are current.")
        return 1 if a.check and missing else 0
    print("\n".join(diff))
    if a.write:
        README.write_text(new, encoding="utf-8", newline="\n")
        print("README.md updated.")
        return 0
    return 1 if a.check else 0


if __name__ == "__main__":
    sys.exit(main())
