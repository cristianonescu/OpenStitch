#!/usr/bin/env python3
"""Fails on a local path or a personal detail in the tracked files (#1057).

    python3 -I tools/private_scan.py              # every tracked text file (CI job `secrets`)
    python3 -I tools/private_scan.py --diff main  # only the lines this branch adds (before a push)
    python3 -I tools/private_scan.py --self-test  # each kind is caught on a sample

What it looks for, in any tracked text file: a Windows user folder on any drive (`X:\\Users\\<name>`,
either slash, or Git Bash's `/c/Users/<name>/`), a macOS or Linux home folder (`/Users/<name>/`,
`/home/<name>/`), a drive-letter sandbox path, the app-data notes folders, any e-mail address, and
the key prefixes the release gate's scan names (Anthropic, GitHub, AWS, private-key blocks). Web
addresses are taken out of a line first, so a URL's path or a list address inside a link is not
a match.

A known harmless match goes in `tools/private_scan_allow.tsv`: the file (or `*`), the exact text
matched and the reason. Nothing is allowed by a wider pattern, and an entry that no longer matches
anything is reported, so the list cannot go stale. Prints `file:line: kind: match`; exits 1 on a
finding.
"""

import argparse
import fnmatch
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOW = ROOT / "tools" / "private_scan_allow.tsv"
# The allow list holds the allowed texts themselves, so it is not scanned.
ALLOW_PATH = "tools/private_scan_allow.tsv"

# Files never scanned: binary embroidery and image formats, fonts and archives.
SKIP = (".pes", ".pec", ".dst", ".exp", ".jef", ".vp3", ".pcs", ".sew", ".shv", ".csd", ".xxx",
        ".png", ".jpg", ".jpeg", ".gif", ".bmp", ".ico", ".icns", ".webp", ".ttf", ".otf",
        ".woff", ".woff2", ".zip", ".gz", ".pdf", ".phc", ".pmv", ".mp4", ".wasm")

NAME = r"[A-Za-z0-9][\w .()'-]*?"
KINDS = [
    ("Windows user folder", re.compile(r"(?i)\b[a-z]:[\\/]{1,2}users[\\/]{1,2}" + NAME + r"(?=[\\/]|$|[\s\"'`)])")),
    ("Windows user folder", re.compile(r"(?i)/[a-z]/users/" + NAME + r"(?=/)")),
    ("home folder", re.compile(r"(?<![\w.:])/(?:Users|home)/[A-Za-z0-9][\w.-]*(?=/)")),
    ("sandbox path", re.compile(r"(?i)\b[a-z]:[\\/]{1,2}_?sandbox")),
    ("app-data folder", re.compile(r"(?i)appdata[\\/]{1,2}(?:local|roaming|locallow)\b")),
    # Split so this file does not match itself.
    ("notes folder", re.compile(r"(?i)claude" + "-data|side-session" + "-notes")),
    ("e-mail address", re.compile(r"[A-Za-z0-9._%+-]+@[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.[A-Za-z]{2,}")),
    ("key", re.compile(r"sk-ant-[A-Za-z0-9_-]{10,}|ghp_[A-Za-z0-9]{20,}|github_pat_[A-Za-z0-9_]{10,}"
                       r"|AKIA[0-9A-Z]{16}|-----BEGIN [A-Z ]*PRIVATE KEY")),
]
URL = re.compile(r"\b[a-z][a-z0-9+.-]*://\S+", re.I)


def allow_list(text):
    """[(path glob, exact text, reason)] from the allow file's tab-separated lines."""
    out = []
    for n, line in enumerate(text.splitlines(), 1):
        if not line.strip() or line.startswith("#"):
            continue
        parts = line.split("\t")
        if len(parts) != 3 or not all(p.strip() for p in parts):
            sys.exit(f"tools/private_scan_allow.tsv:{n}: needs file, text and reason, tab-separated")
        out.append(tuple(p.strip() for p in parts))
    return out


def findings(path, lines, allowed, used):
    """(path, line number, kind, match) of each finding not on the allow list; marks the allow
    entries that matched in `used`."""
    out = []
    for n, line in lines:
        bare = URL.sub(" ", line)
        for kind, pattern in KINDS:
            for m in pattern.finditer(bare):
                hit = m.group(0)
                ok = False
                for i, (glob, text, _) in enumerate(allowed):
                    if fnmatch.fnmatch(path, glob) and text in hit or (fnmatch.fnmatch(path, glob) and hit in text):
                        used.add(i)
                        ok = True
                if not ok:
                    out.append((path, n, kind, hit))
    return out


def tracked():
    names = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout
    return [n for n in names.decode("utf-8").split("\0") if n and not n.lower().endswith(SKIP) and n != ALLOW_PATH]


def read_lines(path):
    try:
        data = (ROOT / path).read_bytes()
    except OSError:
        return []
    if b"\0" in data[:8192]:
        return []
    return list(enumerate(data.decode("utf-8", errors="replace").splitlines(), 1))


def added_lines(base):
    """{path: [(line number, text)]} of the lines `git diff base...HEAD` adds."""
    diff = subprocess.run(["git", "diff", "--unified=0", "--no-color", f"{base}...HEAD"], cwd=ROOT,
                          capture_output=True, check=True).stdout.decode("utf-8", errors="replace")
    out, path, n = {}, None, 0
    for line in diff.splitlines():
        if line.startswith("+++ "):
            path = line[6:] if line.startswith("+++ b/") else None
        elif line.startswith("@@"):
            n = int(re.match(r"@@ -\S+ \+(\d+)", line).group(1))
        elif path and line.startswith("+") and not path.lower().endswith(SKIP) and path != ALLOW_PATH:
            out.setdefault(path, []).append((n, line[1:]))
            n += 1
    return out


def scan(files, allowed):
    used, found = set(), []
    for path, lines in files:
        found += findings(path, lines, allowed, used)
    return found, used


def self_test():
    """Each kind is caught on a sample (built here, so this file does not match itself); URLs,
    allowed text and unused entries behave."""
    s = "\\"
    samples = {
        "Windows user folder": ["C:" + s + "Users" + s + "Alex" + s + "x.pes", "d:/" + "Users/alex/x", "/c/" + "Users/alex/x"],
        "home folder": ["/" + "Users/alex/Desktop", "/" + "home/alex/src"],
        "sandbox path": ["D:" + s + "_sandboxes" + s + "x", "e:/" + "sandbox/y"],
        "app-data folder": ["AppData" + s + "Local" + s + "Temp"],
        "notes folder": ["Claude" + "-Data", "side-session" + "-notes"],
        "e-mail address": ["alex.someone" + "@" + "mailbox.org"],
        "key": ["sk-ant-" + "a" * 20, "ghp_" + "b" * 24, "AKIA" + "C" * 16],
    }
    for kind, texts in samples.items():
        for t in texts:
            got = findings("a.md", [(1, f"see {t} here")], [], set())
            assert [k for _, _, k, _ in got] == [kind], (t, got)
    # Not matches: web addresses, a URL's own path, ordinary text.
    for t in ["https://brother-usa.com/" + "home/sewing", "list " + "https://lists.x.org/a@lists.x.org/t",
              "C:" + s + "Program Files" + s + "OpenStitch", "the users folder"]:
        assert findings("a.md", [(1, t)], [], set()) == [], t
    # The allow list: exact text in a file, the reason required, an unused entry reported.
    allowed = allow_list("# c\nsrc/t.rs\t/" + "home/me/\ttest placeholder\n*\tteam" + "@" + "fonts.org\tlicence\n")
    used = set()
    assert findings("src/t.rs", [(1, '"/' + 'home/me/a.osd"')], allowed, used) == [] and used == {0}
    assert findings("src/u.rs", [(1, '"/' + 'home/me/a.osd"')], allowed, set()) != []
    assert findings("x", [(1, "(team" + "@" + "fonts.org)")], allowed, used) == [] and used == {0, 1}
    try:
        allow_list("a\tb\n")
    except SystemExit:
        pass
    else:
        raise AssertionError("an entry without a reason is refused")
    print("private_scan self-test passed.")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--diff", metavar="BASE", help="scan only the lines added since BASE")
    ap.add_argument("--self-test", action="store_true", help="check the patterns on samples, then exit")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    allowed = allow_list(ALLOW.read_text(encoding="utf-8"))
    if a.diff:
        files = sorted(added_lines(a.diff).items())
    else:
        files = [(p, read_lines(p)) for p in tracked()]
    found, used = scan(files, allowed)
    for path, n, kind, hit in found:
        print(f"{path}:{n}: {kind}: {hit}")
    stale = [] if a.diff else [allowed[i] for i in range(len(allowed)) if i not in used]
    for glob, text, reason in stale:
        print(f"tools/private_scan_allow.tsv: no longer matches anything: {glob}\t{text} ({reason})")
    if found or stale:
        print("Remove the local path or personal detail (or, if it is a harmless placeholder, add it to "
              "tools/private_scan_allow.tsv with the reason) (#1057).", file=sys.stderr)
        return 1
    print(f"No local paths or personal details in {len(files)} {'changed ' if a.diff else ''}files.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
