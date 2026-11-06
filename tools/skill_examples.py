#!/usr/bin/env python3
"""Runs every illustrated example of the Assistant skill (#492) and checks what it sews.

Each skills/openstitch-embroidery/reference/examples/*.json is a `stitchtool exec` script. This
runs it with a `design.export` (PES) and a `design.exportPicture` (PNG) appended, fails when a
script fails, when its `design.check` reports an error, any warning but an expected border
overlap, or when the design does not fit the page, then runs tools/sew_check.py on the PES files.

    python -I tools/skill_examples.py --stitchtool target/release/stitchtool --out DIR

DIR keeps the PES and PNG files (to look at the renders); the default is a temporary folder.
Exits 1 on any failure.
"""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXAMPLES = ROOT / "skills" / "openstitch-embroidery" / "reference" / "examples"
# design.check's overlap warnings each example may have: satin borders over the fill they edge.
BORDER_OVERLAPS = {"badge-arc-text": 1, "cat-face": 2}


def run_example(stitchtool: str, script: Path, out: Path) -> list:
    """The problems with one example (empty when it is good)."""
    steps = json.loads(script.read_text(encoding="utf-8"))
    name = script.stem
    steps += [
        {"command": "design.export", "params": {"path": f"{name}.pes"}},
        {"command": "design.exportPicture", "params": {"path": f"{name}.png"}},
    ]
    runnable = out / f"{name}.json"
    runnable.write_text(json.dumps(steps), encoding="utf-8")
    done = subprocess.run([stitchtool, "exec", str(runnable)], capture_output=True, text=True)
    runnable.unlink()
    if done.returncode != 0:
        return [f"stitchtool exec failed: {(done.stderr or done.stdout).strip()[-400:]}"]
    problems = []
    replies = [json.loads(line) for line in done.stdout.splitlines() if line.strip()]
    for step, reply in zip(steps, replies):
        result = reply.get("result") or {}
        if step["command"] == "design.check":
            # Errors fail, and so does any warning but the overlap one. The overlap warning is
            # expected only where a satin border overlaps the fill it edges (sewability.md); a
            # detail stacked on a fill should be cut out of it (shape.removeOverlap, before the
            # angles are set: #652), so its count is held at what the examples have now.
            findings = result.get("findings", [])
            bad = [f.get("message") for f in findings if f.get("severity") == "error"
                   or "layers of thread" not in str(f.get("message", ""))]
            overlaps = [f for f in findings if "layers of thread" in str(f.get("message", ""))]
            if bad or not result.get("ready", False):
                problems.append(f"design.check: {bad}")
            if len(overlaps) > BORDER_OVERLAPS.get(script.stem, 0):
                problems.append(f"{len(overlaps)} overlap warnings, {BORDER_OVERLAPS.get(script.stem, 0)} expected (borders)")
        if step["command"] == "design.export" and result.get("fitsPage") is False:
            problems.append("the design does not fit its page")
    return problems


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--stitchtool", default="stitchtool")
    ap.add_argument("--out", type=Path)
    args = ap.parse_args()
    scripts = sorted(EXAMPLES.glob("*.json"))
    if len(scripts) < 8:
        print(f"only {len(scripts)} examples in {EXAMPLES}; the skill promises at least 8")
        return 1
    with tempfile.TemporaryDirectory(prefix="skill-examples-") as tmp:
        out = args.out or Path(tmp)
        out.mkdir(parents=True, exist_ok=True)
        failed = 0
        for script in scripts:
            problems = run_example(args.stitchtool, script, out)
            print(f"{script.stem}: {'ok' if not problems else '; '.join(problems)}")
            failed += bool(problems)
        sew = subprocess.run([sys.executable, "-I", str(ROOT / "tools" / "sew_check.py"), str(out), "--hoop", "100x100"])
        if sew.returncode != 0:
            failed += 1
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
