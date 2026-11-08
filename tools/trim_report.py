#!/usr/bin/env python3
"""Trims per editable design (#794): every `examples/*.json` script, run as it is, its stitches
counted for trims, and for the trims followed by a stitch in the same thread ("same-thread":
the ones routing could remove; the others come before a colour change).

    python -I tools/trim_report.py [--only NAME,NAME] [--set NAME=JSON] [--json OUT]
                                   [--compare BASE.json]

`--set` applies a design property after each script (`design.setProperties`, for example
`--set autoStart='"nearest"'` or `--set travelUnder=true`) to measure a routing option.
`--json` writes the table; `--compare` reads such a table and fails (exit 1) when a design
gets more trims or more same-thread trims than in it, so a routing change can show it adds
none. The scripts run in a temporary copy of examples/ (their own saves and exports land
there), with the stitchtool from target/release (OPENSTITCH_STITCHTOOL overrides it).
"""
import argparse
import csv
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "examples"
EXE = ".exe" if os.name == "nt" else ""
BIN = Path(os.environ.get("OPENSTITCH_STITCHTOOL", ROOT / "target" / "release" / f"stitchtool{EXE}"))


def count(rows):
    """(trims, same-thread trims, colour changes) of a stitch list's rows."""
    trims = same = changes = 0
    pending = False  # a trim waiting for the next stitch or colour change
    for r in rows:
        c = r["command"]
        if c == "trim":
            trims += 1
            if pending:
                # Two trims in a row: only the last can be routed away.
                pass
            pending = True
        elif c == "colorChange":
            changes += 1
            pending = False
        elif c == "stitch":
            if pending:
                same += 1
            pending = False
    return trims, same, changes


def run_design(script, work, props):
    """Runs one example script in `work` and returns its stitch list rows, or an error."""
    steps = json.loads(script.read_text(encoding="utf-8"))
    text = json.dumps(steps).replace('"../crates/', json.dumps(str(ROOT / "crates"))[:-1] + "/")
    steps = json.loads(text)
    if props:
        steps.append({"command": "design.setProperties", "params": props})
    steps.append({"command": "design.export", "params": {"path": "trim-report.csv"}})
    path = work / "trim-report-script.json"
    path.write_text(json.dumps(steps), encoding="utf-8")
    out = work / "trim-report.csv"
    if out.exists():
        out.unlink()
    done = subprocess.run([str(BIN), "exec", str(path)], cwd=work, capture_output=True, text=True,
                          encoding="utf-8")
    replies = [json.loads(line) for line in done.stdout.splitlines() if line.strip()]
    bad = [r for r in replies if not r.get("ok")]
    if done.returncode or bad or not out.exists():
        msg = (bad[0].get("error") if bad else done.stderr.strip()) or "no stitches"
        return None, str(msg)[:120]
    return list(csv.DictReader(io.StringIO(out.read_text(encoding="utf-8")))), None


def main():
    ap = argparse.ArgumentParser(description=__doc__.split("\n")[0])
    ap.add_argument("--only", help="comma-separated example names (without .json)")
    ap.add_argument("--set", action="append", default=[], metavar="NAME=JSON",
                    help="a design property set after each script")
    ap.add_argument("--json", help="write the table as JSON")
    ap.add_argument("--compare", help="a table from --json: fail when a design gets more trims")
    a = ap.parse_args()
    if not BIN.exists():
        sys.exit(f"{BIN} not found: cargo build --release -p stitchtool (or set OPENSTITCH_STITCHTOOL)")
    props = {}
    for s in a.set:
        name, _, value = s.partition("=")
        props[name] = json.loads(value)
    only = set(a.only.split(",")) if a.only else None
    scripts = [p for p in sorted(EXAMPLES.glob("*.json")) if only is None or p.stem in only]
    table = {}
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp) / "examples"
        shutil.copytree(EXAMPLES, work, ignore=shutil.ignore_patterns("sew"))
        for script in scripts:
            rows, err = run_design(script, work, props)
            if rows is None:
                print(f"{script.stem:24} skipped: {err}")
                continue
            trims, same, changes = count(rows)
            table[script.stem] = {"trims": trims, "sameThread": same, "colorChanges": changes}
    base = json.loads(Path(a.compare).read_text(encoding="utf-8")) if a.compare else {}
    worse = []
    print(f"{'design':24} {'trims':>6} {'same-thread':>12} {'colour changes':>15}")
    for name, r in table.items():
        b = base.get(name)
        note = ""
        if b:
            note = f"   was {b['trims']} / {b['sameThread']}"
            if r["trims"] > b["trims"] or r["sameThread"] > b["sameThread"]:
                worse.append(name)
                note += "  MORE"
        print(f"{name:24} {r['trims']:6} {r['sameThread']:12} {r['colorChanges']:15}{note}")
    tot = lambda k: sum(r[k] for r in table.values())  # noqa: E731
    print(f"{'all ' + str(len(table)) + ' designs':24} {tot('trims'):6} {tot('sameThread'):12} "
          f"{tot('colorChanges'):15}")
    if a.json:
        Path(a.json).write_text(json.dumps(table, indent=1) + "\n", encoding="utf-8")
    if worse:
        sys.exit(f"more trims than the base table: {', '.join(worse)}")


if __name__ == "__main__":
    main()
