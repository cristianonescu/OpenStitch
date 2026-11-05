#!/usr/bin/env python3
"""Mean CIEDE2000 between each colour and the thread `threads.remap` picks for it, per metric
(#484): PE-DESIGN's RGB rule against the perceptual metrics.

    cargo build -p stitchtool
    python3 -I tools/thread_match_table.py              # Markdown table on stdout

Two corpora:
- **Brother colours:** the 61 Brother chart colours PE-DESIGN matches to (one rectangle each),
  matched to every thread maker's chart.
- **Example designs:** every PES under examples/ (the gallery and the sew kit), imported as
  stitches, each matched to every maker's chart; the mean over the files.

Everything runs through the engine (`stitchtool exec` with `threads.remap` and `dryRun`), so the
numbers are the command's own. The stitchtool binary is target/debug/stitchtool, or STITCHTOOL.
"""

import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
EXE = Path(os.environ.get("STITCHTOOL") or ROOT / "target" / "debug" / ("stitchtool.exe" if sys.platform == "win32" else "stitchtool"))
METRICS = ["rgb", "ciede2000", "oklab"]


def run(steps, folder):
    script = Path(folder) / "script.json"
    script.write_text(json.dumps(steps), encoding="utf-8")
    out = subprocess.run([str(EXE), "exec", str(script)], capture_output=True, text=True, check=False)
    replies = [json.loads(line) for line in out.stdout.splitlines() if line.strip()]
    if out.returncode != 0:
        raise SystemExit(f"stitchtool failed: {replies[-1] if replies else out.stderr}")
    return [r["result"] for r in replies]


def remaps(charts):
    return [
        {"command": "threads.remap", "params": {"builtin": c, "metric": m, "dryRun": True}}
        for c in charts
        for m in METRICS
    ]


def main():
    if not EXE.exists():
        raise SystemExit(f"{EXE} not found: run `cargo build -p stitchtool` first")
    with tempfile.TemporaryDirectory() as tmp:
        charts = [c for c in run([{"command": "threads.builtinCharts"}], tmp)[0] if c["kind"] == "brand"]
        ids = [c["id"] for c in charts]
        names = {c["id"]: c["name"] for c in charts}
        steps = [{"command": "design.new", "params": {"width": 200, "height": 200}}]
        for i in range(1, 62):
            steps.append({"command": "shape.rectangle", "params": {"x": i % 10 * 10, "y": i // 10 * 10, "width": 8, "height": 8, "fill": i}})
        res = run(steps + remaps(ids), tmp)[len(steps):]
        brother = {(r["chart"], r["metric"]): r["meanDeltaE"] for r in res}
        files = sorted((ROOT / "examples").rglob("*.pes"))
        totals = {}
        for f in files:
            steps = [{"command": "stitches.import", "params": {"path": str(f)}}]
            for r in run(steps + remaps(ids), tmp)[1:]:
                totals.setdefault((r["chart"], r["metric"]), []).append(r["meanDeltaE"])
        designs = {k: sum(v) / len(v) for k, v in totals.items()}
    print(f"Mean ΔE00 between a colour and its match, per metric; {len(files)} example PES files.\n")
    print("| Chart | Brother 1–61: RGB | CIEDE2000 | Oklab | Examples: RGB | CIEDE2000 | Oklab |")
    print("|---|---|---|---|---|---|---|")
    for c in ids:
        row = [f"{brother[(c, m)]:.2f}" for m in METRICS] + [f"{designs[(c, m)]:.2f}" for m in METRICS]
        print(f"| {names[c]} | " + " | ".join(row) + " |")


if __name__ == "__main__":
    main()
