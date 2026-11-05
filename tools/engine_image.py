#!/usr/bin/env python3
"""Draws an examples/*.json design as a docs/images/engine-*.png stitch plot.

    cargo build -p stitchtool
    python3 tools/engine_image.py examples/programmable-fill.json docs/images/engine-programmable-fill.png

The script runs with `stitchtool exec` in a temporary folder, the design is exported as PES and
its stitches are read back (tools/pescmp.py's reader). Each colour block is drawn in the thread
colour of the object it belongs to (the example's shapes in order, one block each), stitches as
lines and needle points as black dots, on white, at the width given (default 1600 px) with a
small margin. Jumps and trims are not drawn. Needs numpy and Pillow.

    python3 tools/engine_image.py --templates docs/images/engine-templates.png

draws the four built-in templates (`template.open builtin:<id>` filled with sample texts by
`template.fill`) side by side, each in a thin grey frame, at a common scale.
"""

import argparse
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image, ImageDraw

ROOT = Path(__file__).resolve().parent.parent
sys.dont_write_bytecode = True
sys.path.insert(0, str(ROOT / "tools"))
import pescmp  # noqa: E402

EXE = ROOT / "target" / ("release" if "--release" in sys.argv else "debug") / ("stitchtool.exe" if sys.platform == "win32" else "stitchtool")
MARGIN = 16


def absolute_paths(value, base: Path):
    """The script's relative file paths (fonts, images), made absolute against `base`: stitchtool
    resolves them against the script's own folder, and the copy runs from a temporary one."""
    if isinstance(value, dict):
        return {k: (str((base / v).resolve()) if k == "path" and isinstance(v, str) and not Path(v).is_absolute()
                    and (base / v).exists() else absolute_paths(v, base)) for k, v in value.items()}
    if isinstance(value, list):
        return [absolute_paths(v, base) for v in value]
    return value


def run(script: Path, work: Path):
    steps = json.loads(script.read_text(encoding="utf-8"))
    steps = [s for s in steps if s.get("command") not in ("design.export", "project.save")]
    steps = absolute_paths(steps, script.parent)
    steps +=[{"command": "design.export", "params": {"path": str(work / "out.pes")}}, {"command": "design.info"}]
    (work / "script.json").write_text(json.dumps(steps), encoding="utf-8")
    out = subprocess.run([str(EXE), "exec", str(work / "script.json")], capture_output=True, text=True,
                         cwd=ROOT / "examples")
    replies = [json.loads(line) for line in out.stdout.splitlines() if line.strip()]
    if out.returncode or not replies or not replies[-1].get("ok"):
        sys.exit(f"{script}: {[r for r in replies if not r.get('ok')][:1] or out.stderr}")
    # The thread of each colour block, in sewing order (design.info's stitch statistics), so a
    # design whose wizards or combines replaced objects is drawn in its own colours too.
    colours = [tuple(int(h[i:i + 2], 16) for i in (1, 3, 5)) for h in replies[-1]["result"]["stitchStats"]["colors"]]
    return pescmp.read(work / "out.pes"), colours


# The built-in templates in docs/images/engine-templates.png, with the sample texts they show.
TEMPLATES = [
    ("team-badge", {"Name": "Yuki", "Team": "HARRIERS"}),
    ("name-tag", {"Name": "Sam Okafor", "Role": "Sous Chef"}),
    ("initials-ring", {"Initials": "JMK"}),
    ("baby-bib", {"Name": "Amélie", "Date": "12 - 03 - 2026"}),
]


def run_template(template, values, work):
    script = work / "template.json"
    script.write_text(json.dumps([
        {"command": "template.open", "params": {"template": f"builtin:{template}"}},
        {"command": "template.fill", "params": {"values": values}},
    ]), encoding="utf-8")
    return run(script, work)


def templates(out, height=480, gap=40):
    """The four built-in templates side by side, each scaled to `height` and framed."""
    work = Path(tempfile.mkdtemp(prefix="engine-templates-"))
    try:
        tiles = []
        for template, values in TEMPLATES:
            f, colours = run_template(template, values, work)
            sewn = [p for p, k in zip(f["pts"], f["kinds"]) if k == "stitch"]
            w = max(p[0] for p in sewn) - min(p[0] for p in sewn)
            h = max(p[1] for p in sewn) - min(p[1] for p in sewn)
            tile = draw(f, colours, max(2 * MARGIN + 1, round((height - 2 * MARGIN) * w / h) + 2 * MARGIN))
            framed = Image.new("RGB", (tile.width + 2 * gap, height + 2 * gap), "white")
            framed.paste(tile, (gap, gap + (height - tile.height) // 2))
            ImageDraw.Draw(framed).rectangle([1, 1, framed.width - 2, framed.height - 2], outline=(200, 200, 200), width=2)
            tiles.append(framed)
        img = Image.new("RGB", (sum(t.width for t in tiles) + gap * (len(tiles) - 1), tiles[0].height), "white")
        x = 0
        for tile in tiles:
            img.paste(tile, (x, 0))
            x += tile.width + gap
        save(img, out)
        print(out, img.size)
    finally:
        shutil.rmtree(work, ignore_errors=True)


def save(img, out):
    img.quantize(colors=64).save(out, optimize=True)


def draw(f, colours, width):
    pts, kinds = f["pts"], f["kinds"]
    sewn = [i for i, k in enumerate(kinds) if k == "stitch"]
    xs = [pts[i][0] for i in sewn]
    ys = [pts[i][1] for i in sewn]
    x0, y0 = min(xs), min(ys)
    scale = (width - 2 * MARGIN) / (max(xs) - x0)
    height = int(round((max(ys) - y0) * scale)) + 2 * MARGIN
    img = Image.new("RGB", (width, height), "white")
    d = ImageDraw.Draw(img)
    at = lambda i: (MARGIN + (pts[i][0] - x0) * scale, MARGIN + (pts[i][1] - y0) * scale)  # noqa: E731
    for a, b in zip(range(len(kinds)), range(1, len(kinds))):
        if kinds[a] == "stitch" and kinds[b] == "stitch" and pts[a][2] == pts[b][2]:
            block = int(pts[b][2])
            d.line([at(a), at(b)], fill=colours[min(block, len(colours) - 1)], width=1)
    for i in sewn:
        x, y = at(i)
        d.ellipse([x - 1.6, y - 1.6, x + 1.6, y + 1.6], fill="black")
    return img


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("script", type=Path, nargs="?")
    ap.add_argument("out", type=Path, nargs="?")
    ap.add_argument("--templates", type=Path, metavar="OUT", help="draw the four built-in templates into OUT")
    ap.add_argument("--width", type=int, default=1600)
    ap.add_argument("--release", action="store_true", help="use the release build")
    a = ap.parse_args()
    if a.templates:
        return templates(a.templates)
    if not (a.script and a.out):
        ap.error("give SCRIPT and OUT, or --templates OUT")
    work = Path(tempfile.mkdtemp(prefix="engine-image-"))
    try:
        f, colours = run(a.script.resolve(), work)
        img = draw(f, colours, a.width)
        save(img, a.out)
        print(a.out, img.size, len(f["kinds"]), "records")
    finally:
        shutil.rmtree(work, ignore_errors=True)


if __name__ == "__main__":
    main()
