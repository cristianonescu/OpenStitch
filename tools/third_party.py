#!/usr/bin/env python3
"""The crate list in THIRD_PARTY_NOTICES.md, generated from `cargo metadata` (#1171).

    python -I tools/third_party.py            # print a summary: crate count and licences
    python -I tools/third_party.py --write    # rewrite the generated block in THIRD_PARTY_NOTICES.md
    python -I tools/third_party.py --check    # fail if the block differs from a fresh list
    python -I tools/third_party.py --self-test

The list holds every crate in the normal (not build or dev) dependency graph of what the release
packages and the web page ship, on every target they ship for:

- `openstitch-app` (the editor), `stitchtool`, `openstitch-mcp` and `openstitch-render` (the
  Linux thumbnailer), from the workspace, for macOS (Apple Silicon and Intel), Windows and Linux;
- `openstitch-web` for `wasm32-unknown-unknown` (the browser page);
- the Windows Explorer thumbnail handler, `packaging/windows/thumbnails` (its own workspace).

Proc-macro crates and their dependencies are listed too, though they run only at compile time.
Each crate gets its name, version, licence (the package's SPDX expression, or its licence file
when it gives none) and repository, all read from the published package's Cargo.toml. The
workspace's own crates are left out: they are OpenStitch, under the repository's licence.

Licences outside the permissive set below are reported on every run, so a new kind of licence
is seen before a release. The hand-written sections of THIRD_PARTY_NOTICES.md (fonts, artwork,
code adapted from other projects, licence texts) stay as they are; only the block between the
`crates:start` and `crates:end` markers is generated. `cargo metadata` downloads nothing that
Cargo.lock does not already name, and builds nothing.
"""
import argparse
import json
import re
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
NOTICES = ROOT / "THIRD_PARTY_NOTICES.md"
START, END = "<!-- crates:start -->", "<!-- crates:end -->"
NATIVE = ["aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-pc-windows-msvc", "x86_64-unknown-linux-gnu"]
# (manifest, root packages, targets): what .github/workflows/build-executables.yml and pages.yml build.
SHIPPED = [
    (ROOT / "Cargo.toml", ["openstitch-app", "stitchtool", "openstitch-mcp", "openstitch-render"], NATIVE),
    (ROOT / "Cargo.toml", ["openstitch-web"], ["wasm32-unknown-unknown"]),
    (ROOT / "packaging/windows/thumbnails/Cargo.toml", None, ["x86_64-pc-windows-msvc"]),
]
# SPDX ids of the permissive licences the project has accepted (MIT/Apache/BSD/ISC/Zlib/MPL and
# Unicode-style). A crate whose expression offers none of these as a whole alternative is flagged.
PERMISSIVE = {
    "MIT", "MIT-0", "Apache-2.0", "Apache-2.0 WITH LLVM-exception", "BSD-2-Clause", "BSD-3-Clause",
    "ISC", "Zlib", "MPL-2.0", "Unicode-3.0", "Unicode-DFS-2016",
}


def metadata(manifest, target):
    out = subprocess.run(
        ["cargo", "metadata", "--format-version", "1", "--locked", "--manifest-path", str(manifest),
         "--filter-platform", target],
        cwd=ROOT, capture_output=True, text=True, encoding="utf-8")
    if out.returncode != 0:
        sys.exit(f"cargo metadata failed for {manifest.relative_to(ROOT)} on {target}:\n{out.stderr}")
    return json.loads(out.stdout)


def shipped_packages():
    """{(name, version): package} of every third-party crate the shipped binaries depend on."""
    found = {}
    for manifest, roots, targets in SHIPPED:
        for target in targets:
            meta = metadata(manifest, target)
            pkgs = {p["id"]: p for p in meta["packages"]}
            members = set(meta["workspace_members"])
            nodes = {n["id"]: n for n in meta["resolve"]["nodes"]}
            start = [i for i in members if roots is None or pkgs[i]["name"] in roots]
            if roots is not None and len(start) != len(roots):
                sys.exit(f"not all of {roots} are workspace members of {manifest.relative_to(ROOT)}")
            seen, todo = set(start), list(start)
            while todo:
                for dep in nodes[todo.pop()]["deps"]:
                    if any(k["kind"] is None for k in dep["dep_kinds"]) and dep["pkg"] not in seen:
                        seen.add(dep["pkg"])
                        todo.append(dep["pkg"])
            for i in seen - members:
                p = pkgs[i]
                if p["source"] is None:  # a path dependency: OpenStitch's own crate in another workspace
                    continue
                found[(p["name"], p["version"])] = p
    return found


def licence(p):
    if p.get("license"):
        return " ".join(p["license"].replace("/", " OR ").split())
    if p.get("license_file"):
        return f"see `{Path(p['license_file']).name}` in the package"
    return "none given"


def permissive(expr):
    """True when the SPDX expression can be met with PERMISSIVE licences alone (AND binds tighter
    than OR; `X WITH Y` is one licence)."""
    if expr.startswith(("see ", "none")):
        return False
    # "Apache-2.0 WITH LLVM-exception" is one licence: join it into one token, then split back.
    joined = re.sub(r"([^\s()]+)\s+WITH\s+([^\s()]+)", lambda m: f"{m[1]}\0WITH\0{m[2]}", expr)
    merged = [t.replace("\0", " ") for t in re.findall(r"\(|\)|[^\s()]+", joined)]
    pos = 0

    def expr_or():
        nonlocal pos
        v = expr_and()
        while pos < len(merged) and merged[pos] == "OR":
            pos += 1
            v = expr_and() or v
        return v

    def expr_and():
        nonlocal pos
        v = atom()
        while pos < len(merged) and merged[pos] == "AND":
            pos += 1
            v = atom() and v
        return v

    def atom():
        nonlocal pos
        t = merged[pos]
        pos += 1
        if t == "(":
            v = expr_or()
            pos += 1  # ")"
            return v
        return t in PERMISSIVE

    return expr_or()


def top_level_alternatives(expr):
    """The OR-alternatives of an SPDX expression at parenthesis depth 0, outer parentheses off."""
    alts, depth, cur = [], 0, []
    for t in re.findall(r"\(|\)|[^\s()]+", expr):
        depth += t == "("
        depth -= t == ")"
        if t == "OR" and depth == 0:
            alts.append(" ".join(cur))
            cur = []
        else:
            cur.append(t)
    alts.append(" ".join(cur))
    out = []
    for a in alts:
        a = a.replace("( ", "(").replace(" )", ")")
        while a.startswith("(") and a.endswith(")") and encloses(a):
            a = a[1:-1]
        out.append(a)
    return out


def encloses(a):
    """True when the first "(" of `a` closes at its last character."""
    depth = 0
    for i, c in enumerate(a):
        depth += c == "("
        depth -= c == ")"
        if depth == 0:
            return i == len(a) - 1
    return False


def used_under(expr):
    """The licence OpenStitch takes the crate under: MIT, else Apache-2.0, else the first
    permissive alternative of an OR expression; the whole expression when it has no choice."""
    if expr.startswith(("see ", "none")):
        return expr
    alts = top_level_alternatives(expr)
    if len(alts) == 1:
        return expr
    for want in ("MIT", "Apache-2.0"):
        if want in alts:
            return want
    for a in alts:
        if permissive(a):
            return a
    return expr


def is_mpl(expr):
    return "MPL-" in expr


def repository(p):
    url = p.get("repository") or p.get("homepage") or f"https://crates.io/crates/{p['name']}"
    return url.rstrip("/")


def block(found):
    rows = sorted(found.values(), key=lambda p: (p["name"].lower(), p["version"]))
    lines = [
        START,
        "<!-- Generated by tools/third_party.py from `cargo metadata`; do not edit by hand. -->",
        "",
        f"{len(rows)} crates. *Licence* is the package's own SPDX expression (`OR` offers a choice);",
        "*Used under* is the one OpenStitch takes: MIT, else Apache-2.0, else the first permissive",
        "choice, or the whole expression when there is no choice.",
        "",
        "| Crate | Version | Licence | Used under | Repository |",
        "|---|---|---|---|---|",
    ]
    for p in rows:
        lic = licence(p)
        lines.append(f"| {p['name']} | {p['version']} | {lic} | {used_under(lic)} | <{repository(p)}> |")
    mpl = [p for p in rows if is_mpl(licence(p)) and is_mpl(used_under(licence(p)))]
    lines += ["", "**Crates used under MPL-2.0.** OpenStitch uses them unmodified, from crates.io; their source"
              " at the version used is linked." if mpl else
              "**Crates used under MPL-2.0:** none in the shipped programs.", ""]
    for p in mpl:
        lines.append(f"- {p['name']} {p['version']}: <https://docs.rs/crate/{p['name']}/{p['version']}/source/>")
    lines.append(END)
    return "\n".join(lines)


def splice(text, new):
    m = re.search(re.escape(START) + r".*?" + re.escape(END), text, flags=re.S)
    if not m:
        sys.exit(f"{NOTICES.name} has no {START} / {END} markers")
    return text[:m.start()] + new + text[m.end():]


def read_notices():
    raw = NOTICES.read_bytes().decode("utf-8")
    return raw.replace("\r\n", "\n"), "\r\n" in raw


def self_test():
    assert permissive("MIT OR Apache-2.0")
    assert permissive("(MIT OR Apache-2.0) AND Unicode-3.0")
    assert permissive("Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT")
    assert not permissive("(MIT OR Apache-2.0) AND OFL-1.1 AND Ubuntu-font-1.0")
    assert permissive("Apache-2.0 OR GPL-2.0-only")
    assert not permissive("LGPL-2.1-or-later")
    assert not permissive("CC0-1.0")
    assert not permissive("see `LICENSE` in the package")
    assert licence({"license": "MIT/Apache-2.0"}) == "MIT OR Apache-2.0"
    assert used_under("Apache-2.0 OR GPL-2.0-only") == "Apache-2.0"
    assert used_under("Zlib OR Apache-2.0 OR MIT") == "MIT"
    assert used_under("Unlicense OR MIT") == "MIT"
    assert used_under("BSD-3-Clause OR Apache-2.0") == "Apache-2.0"
    assert used_under("(MIT OR Apache-2.0) AND Unicode-3.0") == "(MIT OR Apache-2.0) AND Unicode-3.0"
    assert used_under("BSL-1.0") == "BSL-1.0"
    assert used_under("(Apache-2.0 AND ISC) OR MIT") == "MIT"
    assert top_level_alternatives("(A OR B) AND C OR (D)") == ["(A OR B) AND C", "D"]
    assert used_under("MPL-2.0 OR LGPL-2.1-or-later") == "MPL-2.0"
    text = f"a\n{START}\nold\n{END}\nb\n"
    assert splice(text, "NEW") == "a\nNEW\nb\n"
    print("third_party self-test passed")


def main():
    ap = argparse.ArgumentParser()
    g = ap.add_mutually_exclusive_group()
    g.add_argument("--write", action="store_true", help="rewrite the generated block")
    g.add_argument("--check", action="store_true", help="fail if the generated block is stale")
    g.add_argument("--self-test", action="store_true", help="check the licence and splice helpers")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    found = shipped_packages()
    flagged = sorted({(p["name"], licence(p)) for p in found.values() if not permissive(licence(p))})
    kinds = {}
    for p in found.values():
        kinds[licence(p)] = kinds.get(licence(p), 0) + 1
    if a.write or a.check:
        text, crlf = read_notices()
        new = splice(text, block(found))
    if a.write:
        out = new.replace("\n", "\r\n") if crlf else new
        NOTICES.write_bytes(out.encode("utf-8"))
        print(f"{NOTICES.name}: {len(found)} crates written")
    elif a.check:
        if new != text:
            sys.exit(f"{NOTICES.name} is stale: run python -I tools/third_party.py --write")
        print(f"{NOTICES.name} lists all {len(found)} crates")
    else:
        print(f"{len(found)} crates")
        for k, n in sorted(kinds.items(), key=lambda x: -x[1]):
            print(f"{n:5} {k}")
    for name, lic in flagged:
        print(f"outside the permissive set: {name}: {lic}")


if __name__ == "__main__":
    sys.exit(main())
