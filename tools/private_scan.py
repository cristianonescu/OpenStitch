#!/usr/bin/env python3
"""Fails on a local path or a personal detail in the tracked files (#1057, #1226).

    python3 -I tools/private_scan.py              # every tracked file (CI job `secrets`)
    python3 -I tools/private_scan.py --diff main  # only what this branch adds (before a push)
    python3 -I tools/private_scan.py --self-test  # each kind is caught on a sample
    python3 -I tools/private_scan.py FILE...      # these files, tracked or not (a new capture)

What it looks for, in any tracked text file: a Windows user folder on any drive (`X:\\Users\\<name>`,
either slash, or Git Bash's `/c/Users/<name>/`), a macOS or Linux home folder (`/Users/<name>/`,
`/home/<name>/`), a drive-letter sandbox path, the app-data notes folders, any e-mail address, and
the key prefixes the release gate's scan names (Anthropic, GitHub, AWS, private-key blocks). Web
addresses are taken out of a line first, so a URL's path or a list address inside a link is not
a match.

A binary file (an embroidery file, an image, a font: the reference captures in `compat/reference`,
the fixtures under `crates/*/tests/fixtures`, the designs in `examples`, and every other tracked
binary) is read for embedded paths (#1226): its runs of printable text, stored as ASCII or as
UTF-16, are checked for a path that starts at a drive letter, a `..\\` chain, a user-profile or
app-data folder, a folder-and-file path written with backslashes, the notes folders and the
project's local folder names (kept here as hashes, so this file does not name them), and for the
key prefixes. PE-DESIGN stores the path of a source image this way. E-mail addresses are not
looked for in binary files: compressed data matches them by chance, and a font's licence names one.

A known harmless match goes in `tools/private_scan_allow.tsv`: the file (or `*`), the exact text
matched and the reason. Nothing is allowed by a wider pattern, and an entry that no longer matches
anything is reported, so the list cannot go stale. Prints `file:line: kind: match` (for a binary
file `file:byte N (ASCII or UTF-16): kind: match`); exits 1 on a finding.
"""

import argparse
import fnmatch
import hashlib
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
ALLOW = ROOT / "tools" / "private_scan_allow.tsv"
# The allow list holds the allowed texts themselves, so it is not scanned.
ALLOW_PATH = "tools/private_scan_allow.tsv"

# Files read as binary whatever they hold: embroidery and image formats, fonts and archives.
# Any other file with a NUL byte in its first 8192 bytes is binary too.
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

# Binary files (#1226). A run is six or more printable characters, as bytes or as UTF-16 (LE).
ASCII_RUN = re.compile(rb"[\x20-\x7e]{6,}")
UTF16_RUN = re.compile(rb"(?:[\x20-\x7e]\x00){6,}")
# One character of a folder or file name inside a path, and the rest of a path once it has started.
PART = r"[\w .()'&+,#-]"
REST = r"[\w .()'&+,#\\/-]*"
EXT = r"\.[A-Za-z0-9]{2,4}(?![A-Za-z0-9])"
# Each pattern asks for enough structure (a second separator, or a file extension) that packed
# stitch or pixel data does not match it by chance.
BINARY_KINDS = [
    ("drive-letter path", re.compile(r"(?<![A-Za-z0-9])[A-Za-z]:[\\/]{1,2}" + PART + r"+[\\/]" + REST)),
    ("drive-letter path", re.compile(r"(?<![A-Za-z0-9])[A-Za-z]:[\\/]{1,2}" + PART + "+" + EXT)),
    ("parent-folder path", re.compile(r"(?:\.\.[\\/]){2,}" + PART + REST)),
    ("parent-folder path", re.compile(r"\.\.[\\/][A-Za-z0-9]" + PART + r"{2,}(?:[\\/]" + REST + "|" + EXT + ")")),
    ("user-profile folder", re.compile(r"(?i)(?<![a-z0-9])(?:users|home|documents and settings)[\\/]"
                                       + PART + r"+[\\/]" + REST)),
    ("user-profile folder", re.compile(r"(?i)(?<![a-z0-9])(?:desktop|documents|downloads|pictures|onedrive)"
                                       r"[\\/]" + PART + "{3,}" + REST)),
    ("app-data folder", KINDS[4][1]),
    ("notes folder", KINDS[5][1]),
    ("folder path", re.compile(r"[A-Za-z0-9]" + PART + r"{2,}\\(?:" + PART + r"+\\)*[A-Za-z0-9]" + PART + "+" + EXT)),
    ("key", KINDS[7][1]),
]
# SHA-256 of the project's local folder names, lower case: the names themselves are private
# details and stay out of the repository. A name is added with
#   python3 -c "import hashlib; print(hashlib.sha256('the name'.lower().encode()).hexdigest())"
LOCAL_NAMES = {
    "6c7437e2ace35035e1923e4b9321fb3a07e99461d84d1f1916e94d8871a10109",
    "89999a4ccea29b6c9a67c282d83debd70e88f3ad2bc65a539ca6456511371d10",
    "e684dcb55ef35c7865265d270700f9c9be88bf6e3c51f0cd10af8bdcdeeb089b",
    "0bd884911b737b11c6624f1742dcf983a6e43e21503804c28f558e6501a52b9f",
    "d86fa8dbef85f323595f557fe266b68a70588a3508c0840bf26c0f5c3e4a52d3",
    "9128c909e4c86a1d4c5242c715ef42e8fac6d235e2c62c9abab8619755379244",
    "0bbd80b4a1ac36b0d729610d82a8fe6128e045b34be65b4a5c335163fe1bddd0",
}


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


def allowed_hit(path, hit, allowed, used):
    """Whether the allow list covers this match; marks the entries that did in `used`."""
    ok = False
    for i, (glob, text, _) in enumerate(allowed):
        if fnmatch.fnmatch(path, glob) and (text in hit or hit in text):
            used.add(i)
            ok = True
    return ok


def findings(path, lines, allowed, used, kinds=None):
    """(path, line number, kind, match) of each finding not on the allow list; marks the allow
    entries that matched in `used`."""
    out = []
    for n, line in lines:
        bare = URL.sub(" ", line)
        for kind, pattern in kinds or KINDS:
            for m in pattern.finditer(bare):
                hit = m.group(0)
                if not allowed_hit(path, hit, allowed, used):
                    out.append((path, n, kind, hit))
    return out


def text_runs(data):
    """[(where, text)] of a binary file's runs of printable text, ASCII and UTF-16."""
    out = [(f"byte {m.start()} (ASCII)", m.group(0).decode("ascii")) for m in ASCII_RUN.finditer(data)]
    out += [(f"byte {m.start()} (UTF-16)", m.group(0).decode("utf-16-le")) for m in UTF16_RUN.finditer(data)]
    return out


def local_names(text):
    """The project's local folder names in a run: each name between separators, also without a
    first character (a string's length byte is often printable)."""
    out = []
    for part in re.split(r"[\\/]", text):
        for name in (part, part[1:]):
            name = name.strip()
            if name and hashlib.sha256(name.lower().encode("utf-8")).hexdigest() in LOCAL_NAMES:
                out.append(name)
                break
    return out


def binary_findings(path, data, allowed, used):
    """The findings of one binary file: embedded paths, folder names and keys (#1226). A path that
    several patterns match is reported once, under the first."""
    runs = text_runs(data)
    out, seen = [], []
    for found in findings(path, runs, allowed, used, BINARY_KINDS):
        _, where, _, hit = found
        if not any(w == where and (hit in h or h in hit) for w, h in seen):
            seen.append((where, hit))
            out.append(found)
    for where, text in runs:
        for name in local_names(text):
            if not allowed_hit(path, name, allowed, used):
                out.append((path, where, "local folder name", name))
    return out


def is_binary(path, data):
    return path.lower().endswith(SKIP) or b"\0" in data[:8192]


def tracked():
    names = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, capture_output=True, check=True).stdout
    return [n for n in names.decode("utf-8").split("\0") if n and n != ALLOW_PATH]


def read_bytes(path):
    try:
        return (ROOT / path).read_bytes()
    except OSError:
        return None


def text_lines(data):
    return list(enumerate(data.decode("utf-8", errors="replace").splitlines(), 1))


def added_lines(base):
    """{path: [(line number, text)]} of the lines `git diff base...HEAD` adds to text files."""
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


def changed_files(base):
    """The files `git diff base...HEAD` adds or changes (a binary one is read whole)."""
    names = subprocess.run(["git", "diff", "--name-only", "-z", "--diff-filter=ACMR", f"{base}...HEAD"],
                           cwd=ROOT, capture_output=True, check=True).stdout
    return [n for n in names.decode("utf-8").split("\0") if n and n != ALLOW_PATH]


def scan_files(paths, allowed, used, binary_only=False, root=ROOT):
    """(findings, text files read, binary files read) of these files."""
    found, texts, binaries = [], 0, 0
    for path in paths:
        try:
            data = (root / path).read_bytes()
        except OSError:
            continue
        if is_binary(path, data):
            binaries += 1
            found += binary_findings(path, data, allowed, used)
        elif not binary_only:
            texts += 1
            found += findings(path, text_lines(data), allowed, used)
    return found, texts, binaries


def a_tracked_pes():
    """A PE-DESIGN reference file to plant a path in: the smallest one in compat/reference/pes."""
    files = sorted((ROOT / "compat" / "reference" / "pes").glob("*.pes"), key=lambda p: (p.stat().st_size, p.name))
    assert files, "compat/reference/pes holds no .pes file"
    return files[0]


def self_test():
    """Each kind is caught on a sample (built here, so this file does not match itself); URLs,
    allowed text and unused entries behave; a path planted in a copy of a PES file is found."""
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

    # Binary files (#1226): each kind of embedded path, stored as ASCII and as UTF-16, between
    # bytes that are not text.
    paths = {
        "drive-letter path": ["E:" + s + "Designs" + s + "rose.bmp", "f:/" + "work/photo one.jpg", "G:" + s + "scan.png"],
        "parent-folder path": [".." + s + ".." + s + "My Designs" + s + "photo.bmp", "../" + "pictures/cat.png"],
        "user-profile folder": ["Users" + s + "Alex" + s + "rose.bmp", "Desktop" + s + "rose.bmp", "Documents/" + "scans/a.png"],
        "app-data folder": ["AppData" + s + "Roaming" + s + "x"],
        "notes folder": ["Claude" + "-Data", "side-session" + "-notes"],
        "folder path": ["My Designs" + s + "photo.bmp", "work" + s + "round 2" + s + "scan.png"],
        "key": ["ghp_" + "b" * 24],
    }
    for kind, texts in paths.items():
        for t in texts:
            for label, stored in (("ASCII", t.encode("ascii")), ("UTF-16", t.encode("utf-16-le"))):
                got = binary_findings("a.pes", b"\x00\x01\xff\x05" + stored + b"\x00\x00\x02\xfe", [], set())
                assert [k for _, _, k, _ in got] == [kind] and label in got[0][1], (t, label, got)
    # A known local folder name is found by its hash, also behind a printable length byte.
    name = "scan" + "-self-test-folder"
    LOCAL_NAMES.add(hashlib.sha256(name.encode()).hexdigest())
    try:
        for stored in (name.upper() + "/x", "$" + name + "/x"):
            got = binary_findings("a.pes", b"\x00" + stored.encode("utf-16-le") + b"\x00\x00\x00", [], set())
            assert [k for _, _, k, _ in got] == ["local folder name"], (stored, got)
    finally:
        LOCAL_NAMES.discard(hashlib.sha256(name.encode()).hexdigest())
    # Not paths: a font or thread name, a version, a ratio, a file name on its own, a web address.
    for t in ["Lato Bold/Regular", "PE-DESIGN 11.50", "width 3:4 and 16:9 here", "photo-one.bmp",
              "https://example.org/" + "a/b.png", "#PES0110 CEmbLine"]:
        assert binary_findings("a.pes", b"\x00" + t.encode("ascii") + b"\x00", [], set()) == [], t
    # The allow list covers a binary file's match too.
    allowed = allow_list("a.pes\t" + "Designs" + s + "rose.bmp\ta sample\n")
    used = set()
    assert binary_findings("a.pes", ("Designs" + s + "rose.bmp").encode("ascii"), allowed, used) == [] and used == {0}

    # A copy of a PE-DESIGN reference file: clean as it is, a finding once a path is planted in it,
    # as PE-DESIGN stores an image's path, in either encoding.
    source = a_tracked_pes()
    clean = source.read_bytes()
    with tempfile.TemporaryDirectory() as folder:
        for label, planted in (("ASCII", (".." + s + ".." + s + "My Designs" + s + "photo.bmp").encode("ascii")),
                               ("UTF-16", ("E:" + s + "Designs" + s + "rose.bmp").encode("utf-16-le"))):
            copy = Path(folder) / f"copy-{label}.pes"
            copy.write_bytes(clean)
            assert binary_findings(copy.name, copy.read_bytes(), [], set()) == [], (source.name, "is not clean")
            at = len(clean) // 2
            copy.write_bytes(clean[:at] + b"\x00" + planted + b"\x00\x00" + clean[at:])
            got = binary_findings(copy.name, copy.read_bytes(), [], set())
            assert len(got) == 1 and label in got[0][1], (label, got)
    print("private_scan self-test passed.")


def main():
    sys.stdout.reconfigure(encoding="utf-8")
    ap = argparse.ArgumentParser()
    ap.add_argument("--diff", metavar="BASE", help="scan only what was added since BASE")
    ap.add_argument("--self-test", action="store_true", help="check the patterns on samples, then exit")
    ap.add_argument("files", nargs="*", help="scan these files instead of the tracked ones")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    allowed = allow_list(ALLOW.read_text(encoding="utf-8"))
    used = set()
    if a.files:
        found, texts, binaries = scan_files(a.files, allowed, used, root=Path.cwd())
    elif a.diff:
        added = sorted(added_lines(a.diff).items())
        found = []
        for path, lines in added:
            found += findings(path, lines, allowed, used)
        more, _, binaries = scan_files(changed_files(a.diff), allowed, used, binary_only=True)
        found += more
        texts = len(added)
    else:
        found, texts, binaries = scan_files(tracked(), allowed, used)
    for path, n, kind, hit in found:
        print(f"{path}:{n}: {kind}: {hit}")
    whole = not a.diff and not a.files
    stale = [allowed[i] for i in range(len(allowed)) if i not in used] if whole else []
    for glob, text, reason in stale:
        print(f"tools/private_scan_allow.tsv: no longer matches anything: {glob}\t{text} ({reason})")
    if found or stale:
        print("Remove the local path or personal detail (or, if it is a harmless placeholder, add it to "
              "tools/private_scan_allow.tsv with the reason) (#1057, #1226).", file=sys.stderr)
        return 1
    what = "changed " if a.diff else ""
    print(f"No local paths or personal details in {texts} {what}text files and {binaries} {what}binary files.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
