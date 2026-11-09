#!/usr/bin/env python3
"""Fails on a broken relative link or image in the repository's Markdown files (#850).

    python -I tools/check_doc_links.py              # check every tracked .md file
    python -I tools/check_doc_links.py --self-test  # check the parser and resolver first

Each link and image target in every tracked `.md` file must name a tracked file or a folder
holding one, matched case-sensitively against `git ls-files` the way GitHub resolves it, so a
link that works on a case-insensitive disk (Windows, macOS) but not on GitHub fails here too. A
link to a deleted doc fails the same way.

Checked: inline links and images `[text](target)` / `![alt](target)`, reference definitions
`[id]: target`, and HTML `<img src=...>` / `<a href=...>`. A target starting with `/` is taken
from the repository root, any other from the file's folder; `#fragment` and `?query` are dropped
and `%XX` escapes decoded. Absolute links into this repository's main branch
(`https://github.com/cristianonescu/OpenStitch/blob/main/<path>`, also `tree/` and `raw/`) are
checked as repository paths. Other URLs (any scheme, `//host`), in-page `#anchors`, fenced code
blocks and inline code spans are ignored. Anchors themselves are not checked.

Exit 0 when every link resolves, 1 when any is broken (each listed as file:line), 2 when git
can't list the files.
"""
import posixpath
import re
import subprocess
import sys
from pathlib import Path
from urllib.parse import unquote

ROOT = Path(__file__).resolve().parent.parent

REPO_URL = re.compile(
    r"^https?://github\.com/cristianonescu/OpenStitch/(?:blob|tree|raw)/main/(?P<path>[^?#]*)",
    re.IGNORECASE)
SCHEME = re.compile(r"^[A-Za-z][A-Za-z0-9+.-]*:")
FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})")
# `[text](target "title")` and `![alt](target)`; the target may hold one level of balanced
# parentheses, or be written in angle brackets.
INLINE = re.compile(
    r"!?\[(?:[^\[\]]|\[[^\[\]]*\])*\]\(\s*(<[^>\n]*>|(?:[^()\s]|\([^()\s]*\))+)"
    r"(?:\s+(?:\"[^\"]*\"|'[^']*'|\([^)]*\)))?\s*\)")
REFDEF = re.compile(r"^ {0,3}\[[^\]]+\]:\s*(<[^>]*>|\S+)")
HTML = re.compile(r"<(?:img|a|source)\b[^>]*?\b(?:src|href)\s*=\s*(\"[^\"]*\"|'[^']*'|[^\s>]+)",
                  re.IGNORECASE)
CODE_SPAN = re.compile(r"(`+)(?:(?!\1).)+?\1")


def tracked_files():
    out = subprocess.run(["git", "ls-files", "-z"], cwd=ROOT, check=True,
                         capture_output=True).stdout.decode("utf-8")
    return {p for p in out.split("\0") if p}


def folders(files):
    dirs = {""}
    for f in files:
        parts = f.split("/")[:-1]
        for i in range(1, len(parts) + 1):
            dirs.add("/".join(parts[:i]))
    return dirs


def links(text):
    """(line number, target) for every link in Markdown `text`, outside code."""
    fence = None
    for n, line in enumerate(text.splitlines(), 1):
        m = FENCE.match(line)
        if fence:
            if m and m.group(1)[0] == fence[0] and len(m.group(1)) >= len(fence) \
                    and not line.strip()[len(m.group(1)):].strip():
                fence = None
            continue
        if m:
            fence = m.group(1)
            continue
        line = CODE_SPAN.sub(lambda s: " " * len(s.group(0)), line)
        for rx in (INLINE, REFDEF, HTML):
            for m in rx.finditer(line):
                target = m.group(1).strip()
                if target[:1] in "\"'" and target[-1:] == target[:1]:
                    target = target[1:-1]
                if target.startswith("<") and target.endswith(">"):
                    target = target[1:-1]
                yield n, target.strip()


def resolve(source, target):
    """The repository path `target` names from file `source`, or None if it is not a repo link.
    A path leaving the repository resolves to '..' so it is reported."""
    m = REPO_URL.match(target)
    if m:
        path = m.group("path")
        base = ""
    else:
        if not target or target.startswith("#") or target.startswith("//") or SCHEME.match(target):
            return None
        path = re.split(r"[?#]", target, maxsplit=1)[0]
        if not path:
            return None
        base = "" if path.startswith("/") else posixpath.dirname(source)
    path = unquote(path).lstrip("/")
    joined = posixpath.normpath(posixpath.join(base, path)) if path else (base or ".")
    if joined == ".":
        return ""
    return joined


def check(files):
    dirs = folders(files)
    lower = {p.lower(): p for p in files | dirs}
    broken = []
    for md in sorted(f for f in files if f.lower().endswith(".md")):
        try:
            text = (ROOT / md).read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as e:
            broken.append(f"{md}: cannot read: {e}")
            continue
        for n, target in links(text):
            path = resolve(md, target)
            if path is None or path in files or path in dirs:
                continue
            hint = ""
            if path.startswith(".."):
                hint = " (leaves the repository)"
            elif path.lower() in lower:
                hint = f" (case differs: {lower[path.lower()]})"
            broken.append(f"{md}:{n}: broken link '{target}' -> {path}{hint}")
    return broken


def self_test():
    md = ("# T\n[a](docs/A.md) ![i](img/x.png \"t\") [b](<with space.md>)\n"
          "```\n[skip](nope.md)\n```\n"
          "`[skip](code.md)` [ext](https://example.com/x) [anchor](#top) [mail](mailto:x@y)\n"
          "[ref]: ../up.md\n<img src=\"pic.png\" width=10> <a href='dir/'>d</a>\n"
          "[repo](https://github.com/cristianonescu/OpenStitch/blob/main/README.md#l1)\n"
          "[q](f.md?x=1#y) [paren](a_(b).md) [enc](my%20file.md)\n"
          "~~~~\n[skip](tilde.md)\n```\n~~~~\n")
    got = list(links(md))
    want = [(2, "docs/A.md"), (2, "img/x.png"), (2, "with space.md"),
            (6, "https://example.com/x"), (6, "#top"), (6, "mailto:x@y"), (7, "../up.md"),
            (8, "pic.png"), (8, "dir/"),
            (9, "https://github.com/cristianonescu/OpenStitch/blob/main/README.md#l1"),
            (10, "f.md?x=1#y"), (10, "a_(b).md"), (10, "my%20file.md")]
    assert sorted(got) == sorted(want), got
    assert resolve("docs/x.md", "A.md") == "docs/A.md"
    assert resolve("docs/x.md", "../README.md") == "README.md"
    assert resolve("docs/x.md", "/tools/a.py") == "tools/a.py"
    assert resolve("docs/x.md", "../../out.md") == "../out.md"
    assert resolve("docs/x.md", "https://example.com") is None
    assert resolve("docs/x.md", "#a") is None
    assert resolve("docs/x.md", "my%20file.md#s") == "docs/my file.md"
    assert resolve("a/b.md", "https://github.com/cristianonescu/OpenStitch/tree/main/docs") == "docs"
    files = {"README.md", "docs/Guide.md", "docs/x.md"}
    dirs = folders(files)
    assert dirs == {"", "docs"}
    print("self-test passed")


def main(argv):
    if "--self-test" in argv:
        self_test()
        return 0
    self_test()
    try:
        files = tracked_files()
    except (OSError, subprocess.CalledProcessError) as e:
        print(f"cannot list the tracked files: {e}", file=sys.stderr)
        return 2
    broken = check(files)
    for line in broken:
        print(line, file=sys.stderr)
    count = sum(1 for f in files if f.lower().endswith(".md"))
    if broken:
        print(f"{len(broken)} broken link(s) in {count} Markdown files.", file=sys.stderr)
        return 1
    print(f"Every relative link and image resolves in {count} Markdown files.")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
