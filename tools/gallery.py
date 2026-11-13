#!/usr/bin/env python3
"""Regenerates the README's "From image to embroidery" gallery from the test images in
examples/images (docs/research/test-images.md).

    cargo build -p openstitch-app -p stitchtool
    python3 tools/gallery.py [--only ID,ID]

Each entry converts one image with `stitchtool exec` (a Design Page sized to the image, the
long side 100 mm, the wizard at the settings named in GALLERY), opens the result in the editor's
Realistic view (`openstitch --show realistic --screenshot`, 1440x900 window), crops the design
area, draws it again at 16 px/mm in tiles (`--zoom`, `--center`; the rows of a fill beat with the
pixel grid at the page-fit zoom, #893) and writes docs/images/readme/gallery/<id>.png (480 px
wide, reduced with Lanczos, 128 colours). The README
table between the `gallery:start` and `gallery:end` markers is rewritten with the source, the
result and a caption (wizard, settings, stitches, trims, colours). The images in MODES are also
converted in both modes (`compatMode`) for the table between the `gallery-modes` markers. `--release` uses the release build. Needs Pillow.
"""

import argparse
import json
import math
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

from PIL import Image

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / ("release" if "--release" in sys.argv else "debug")
IMAGES = ROOT / "examples" / "images"
OUT = ROOT / "docs" / "images" / "readme" / "gallery"
README = ROOT / "README.md"
# The canvas is the area painted in the surround grey around the Design Page; its position moves
# with the panes, the rulers and the platform's fonts (#271), so it is found by its colour.
SURROUND = (180, 184, 190)
LONG_SIDE_MM = 100
# The editor's pointer-position and zoom label in the canvas's bottom-right corner, pixels.
LABEL_W, LABEL_H = 220, 30

# id -> (image, wizard caption, command, parameters)
GALLERY = {
    "p01": ("p01-portrait.jpg", "Photo Stitch 1, colour, defaults",
            "digitize.photoStitch", {"style": "fill"}),
    "p02": ("p02-mono-portrait.jpg", "Photo Stitch 2, mono, defaults",
            "digitize.photoStitch", {"style": "lines", "mode": "mono"}),
    "p04": ("p04-cat-fur.jpg", "Photo Stitch 1, colour, defaults",
            "digitize.photoStitch", {"style": "fill"}),
    "p06": ("p06-sunset.jpg", "Photo Stitch 2, Auto Select",
            "digitize.photoStitch", {"style": "lines"}),
    "p08": ("p08-poppies.jpg", "Photo Stitch 2, Color Set CMYK",
            "digitize.photoStitch", {"style": "lines", "colorSet": "cmyk"}),
    "p12": ("p12-backlit-tree.jpg", "Photo Stitch 1, mono, defaults",
            "digitize.photoStitch", {"style": "fill", "mode": "mono"}),
    "p03": ("p03-storybook.jpg", "Photo Stitch 1, colour, defaults",
            "digitize.photoStitch", {"style": "fill"}),
    "g01": ("g01-emblem.png", "Auto Punch, defaults",
            "digitize.autoPunch", {}),
    "g02": ("g02-balloon.png", "Auto Punch, defaults",
            "digitize.autoPunch", {}),
    "g05": ("g05-robot-cartoon.png", "Auto Punch, defaults",
            "digitize.autoPunch", {}),
    "g08": ("g08-lettering.png", "Auto Punch, defaults",
            "digitize.autoPunch", {}),
    "g04": ("g04-butterfly-outline.png", "Design Center, running line",
            "digitize.designCenter", {}),
    "g07": ("g07-pixel-strawberry.png", "Cross Stitch, defaults",
            "digitize.crossStitch", {}),
    "g15": ("g15-folk-bird.png", "Cross Stitch, defaults",
            "digitize.crossStitch", {}),
}

# OpenStitch's PE-DESIGN-compatible mode next to OpenStitch mode (`compatMode`, B16 and B16.1,
# docs/research/beyond/image-digitizing.md): id -> (image, wizard, command, parameters),
# rendered as <id>-pe and <id>-os.
MODES = {
    "g05": ("g05-robot-cartoon.png", "Auto Punch", "digitize.autoPunch", {}),
    "g06": ("g06-watercolour-lemons.jpg", "Auto Punch", "digitize.autoPunch", {}),
    "p03": ("p03-storybook.jpg", "Auto Punch", "digitize.autoPunch", {}),
    "p01": ("p01-portrait.jpg", "Photo Stitch 1", "digitize.photoStitch", {"style": "fill"}),
    "p06": ("p06-sunset.jpg", "Photo Stitch 1", "digitize.photoStitch", {"style": "fill"}),
    "p10": ("p10-night-street.jpg", "Photo Stitch 1", "digitize.photoStitch", {"style": "fill"}),
    "p08": ("p08-poppies.jpg", "Photo Stitch 2", "digitize.photoStitch", {"style": "lines"}),
    "p04": ("p04-cat-fur.jpg", "Photo Stitch 2", "digitize.photoStitch", {"style": "lines"}),
    "g07": ("g07-pixel-strawberry.png", "Cross Stitch", "digitize.crossStitch", {}),
    "g04": ("g04-butterfly-outline.png", "Design Center", "digitize.designCenter", {}),
}
for _key, (_image, _wizard, _command, _params) in MODES.items():
    GALLERY[f"{_key}-pe"] = (_image, f"{_wizard}, PE-DESIGN-compatible mode", _command, _params)
    GALLERY[f"{_key}-os"] = (_image, f"{_wizard}, OpenStitch mode", _command,
                             {**_params, "compatMode": "openStitch"})


def run(args, cwd):
    return subprocess.run(args, cwd=cwd, check=True, capture_output=True, text=True).stdout


def convert(work, key):
    image, _, command, params = GALLERY[key]
    w, h = Image.open(IMAGES / image).size
    scale = LONG_SIDE_MM / max(w, h)
    width, height = round(w * scale, 1), round(h * scale, 1)
    steps = [
        {"command": "design.new", "params": {"width": width + 10, "height": height + 10}},
        {"command": command, "params": {"path": image, "width": width, **params}},
        {"command": "design.info"},
        {"command": "project.save", "params": {"path": f"{key}.osd"}},
    ]
    (work / f"{key}.json").write_text(json.dumps(steps), encoding="utf-8")
    replies = [json.loads(line) for line in run([str(BIN / "stitchtool"), "exec", f"{key}.json"], work).splitlines()
               if line.strip()]
    return replies[2]["result"]["stitchStats"]


def shoot(work, key, size_mm=None):
    """Screenshots the design and crops it. With the design's size (`size_mm`, width and height
    in mm), a crop whose proportions are more than 25 % off the design's is retaken, up to three
    times: a screenshot taken before the window settled framed g08 off-centre with its top cut off
    and the status bar inside (#441, #443)."""
    shot = work / f"{key}.png"
    for attempt in range(3):
        run([str(BIN / "openstitch"), f"{key}.osd", "--show", "realistic", "--no-splash",
             "--size", "1440x900", "--screenshot", str(shot)], work)
        im = Image.open(shot).convert("RGB")
        box = design_box(im)
        if not size_mm or framed(box, size_mm):
            break
        print(f"{key}: crop {box} does not match the design's {size_mm} mm, retaking", file=sys.stderr)
    else:
        sys.exit(f"gallery.py: {key}: no screenshot framed the design")
    def tile(*view):
        run([str(BIN / "openstitch"), f"{key}.osd", "--show", "realistic", "--no-splash",
             "--size", "1440x900", *view, "--screenshot", "tile.png", "--viewport-json", "tile.json"], work)
        info = json.loads((work / "tile.json").read_text(encoding="utf-8"))
        return Image.open(work / "tile.png").convert("RGB"), info
    im = sharp_crop(tile)
    im = im.resize((480, round(im.height * 480 / im.width)), Image.LANCZOS)
    OUT.mkdir(parents=True, exist_ok=True)
    dst = OUT / f"{key}.png"
    im.quantize(colors=128, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(dst, optimize=True)
    return dst


def framed(box, size_mm, pad=12):
    """Whether the crop `box` (with design_box's padding) has the design's proportions, within
    25 %."""
    w, h = box[2] - box[0] - 2 * pad, box[3] - box[1] - 2 * pad
    dw, dh = size_mm
    if w <= 0 or h <= 0 or dw <= 0 or dh <= 0:
        return False
    return abs((w / h) / (dw / dh) - 1.0) <= 0.25


def design_box(im):
    """The stitched area of the canvas, with a small margin: pixels that are coloured or dark
    (thread), not the light grey fabric, the grey surround or the rulers. Only the Design Page
    (the light fabric inside the surround) is searched: the editor draws the pointer's position
    and the zoom on the surround below it while the mouse is over the canvas, which stretched
    g08's crop to the canvas corner (#443)."""
    sx, sy = [], []
    for yy in range(0, im.height, 4):
        for xx in range(0, im.width, 4):
            if im.getpixel((xx, yy)) == SURROUND:
                sx.append(xx)
                sy.append(yy)
    if not sx:
        sys.exit("gallery.py: no canvas found in the screenshot (surround colour changed?)")
    left, right, top, bottom = min(sx), max(sx), min(sy), max(sy)
    px, py = [], []
    for yy in range(top, bottom, 4):
        for xx in range(left, right, 4):
            if min(im.getpixel((xx, yy))) >= 200:
                px.append(xx)
                py.append(yy)
    if px:
        left, right, top, bottom = min(px), max(px), min(py), max(py)
    # The pointer label sits in the canvas's bottom-right corner, over the page when the page
    # fills the view (g01): grey pixels there are its text, not thread.
    label_x, label_y = max(sx) - LABEL_W, max(sy) - LABEL_H
    xs, ys = [], []
    for yy in range(top, bottom, 2):
        for xx in range(left, right, 2):
            r, g, b = im.getpixel((xx, yy))
            if xx > label_x and yy > label_y and max(r, g, b) - min(r, g, b) < 30:
                continue
            if max(r, g, b) - min(r, g, b) > 60 or max(r, g, b) < 120:
                xs.append(xx)
                ys.append(yy)
    if not xs:
        return (left, top, right, bottom)
    pad = 12
    return (max(left, min(xs) - pad), max(top, min(ys) - pad),
            min(right, max(xs) + pad), min(bottom, max(ys) + pad))


# Pixels per mm the design is drawn at before it is reduced (#893). At the page-fit zoom (about
# 4-7 px/mm) fill rows lie a pixel or so apart, and the light seams between their anti-aliased
# edges beat with the pixel grid into stripes. From about 15 px/mm the rows resolve: a 0-degree
# fill at 4.5 rows/mm reduced to 4 px/mm then has a row-to-row spread of 9 grey levels instead of
# 32, at the same mean colour (within 4 levels), so the match with the T1 scans holds.
SHARP_PX_PER_MM = 16.0


def canvas_and_page(im):
    """The canvas (the grey surround's extent) and the Design Page on it (the light fabric inside
    it), as (left, top, right, bottom) pixel boxes of a page-fit screenshot."""
    sx, sy = [], []
    for yy in range(0, im.height, 2):
        for xx in range(0, im.width, 2):
            if im.getpixel((xx, yy)) == SURROUND:
                sx.append(xx)
                sy.append(yy)
    canvas = (min(sx), min(sy), max(sx), max(sy))
    px, py = [], []
    for yy in range(canvas[1], canvas[3]):
        row = [xx for xx in range(canvas[0], canvas[2]) if min(im.getpixel((xx, yy))) >= 200]
        if row:
            px += [row[0], row[-1]]
            py.append(yy)
    return canvas, (min(px), min(py), max(px) + 1, max(py) + 1)


# Margin round the stitches in a preview, mm (about the 12 px design_box keeps at the page fit).
PAD_MM = 2.0
# Pixels of each tile kept clear of the canvas's edges (rulers, the pointer's label).
TILE_MARGIN = 40


def crop_box(bounds_mm, pad_mm=PAD_MM, px_per_mm=SHARP_PX_PER_MM):
    """The preview's box in mm round the stitch bounds `[left, top, right, bottom]`, on the
    1/px_per_mm grid, and its size in pixels (#1051)."""
    p = px_per_mm
    x0, y0 = math.floor((bounds_mm[0] - pad_mm) * p) / p, math.floor((bounds_mm[1] - pad_mm) * p) / p
    x1, y1 = math.ceil((bounds_mm[2] + pad_mm) * p) / p, math.ceil((bounds_mm[3] + pad_mm) * p) / p
    return (x0, y0, x1, y1), (round((x1 - x0) * p), round((y1 - y0) * p))


def sharp_crop(shot, pad_mm=PAD_MM):
    """The design drawn at [`SHARP_PX_PER_MM`] round the stitches the editor draws: tiles of the
    canvas at that fixed scale, each centred on its part of the design and placed on whole
    pixels (`--px-per-mm`, `--center`), pasted together (#893). Everything is placed by
    millimetres (the stitch bounds the editor reports), never by a screenshot's pixels, so the
    preview does not depend on the window's layout (#1051). `shot(*view)` takes a screenshot
    with the extra view arguments and returns the image and its `--viewport-json` (scale,
    offset, canvas, stitch bounds)."""
    p = SHARP_PX_PER_MM
    # A first look at the page tells the stitch bounds and the canvas's size.
    _, info = shot("--px-per-mm", f"{p:g}")
    if not info.get("stitchBoundsMm"):
        sys.exit("gallery.py: the design has no stitches to preview")
    c = info["canvasPx"]
    usable = (int(c[2] - c[0]) - 2 * TILE_MARGIN, int(c[3] - c[1]) - 2 * TILE_MARGIN - 2 * LABEL_H)
    if min(usable) < 64:
        sys.exit(f"gallery.py: the canvas is too small for preview tiles ({c})")
    (x0, y0, x1, y1), (w, h) = crop_box(info["stitchBoundsMm"], pad_mm)
    out = Image.new("RGB", (w, h))
    v = 0
    while v < h:
        u, row_h = 0, None
        while u < w:
            tw, th = min(usable[0], w - u), min(usable[1], h - v)
            cx, cy = x0 + (u + tw / 2) / p, y0 + (v + th / 2) / p
            im, info = shot("--px-per-mm", f"{p:g}", "--center", f"{cx:.6f},{cy:.6f}")
            if abs(info["pxPerMm"] - p) > 1e-6:
                sys.exit(f"gallery.py: the tile was drawn at {info['pxPerMm']} px/mm, not {p}")
            # Pixel (u, v) of the preview is mm (x0 + u / p, y0 + v / p): on the screenshot at
            # offset + mm × p, a whole number (the offset is on whole pixels, x0 on the 1/p grid).
            sx = info["offsetPx"][0] + x0 * p + u
            sy = info["offsetPx"][1] + y0 * p + v
            if abs(sx - round(sx)) > 1e-3 or abs(sy - round(sy)) > 1e-3:
                sys.exit(f"gallery.py: the tile is not on whole pixels ({sx}, {sy})")
            sx, sy = round(sx), round(sy)
            c = info["canvasPx"]
            if sx < c[0] + TILE_MARGIN - 1 or sy < c[1] + TILE_MARGIN - 1 or \
                    sx + tw > c[2] - TILE_MARGIN + 1 or sy + th > c[3] - TILE_MARGIN - LABEL_H + 1:
                sys.exit(f"gallery.py: tile {sx},{sy} {tw}x{th} reaches past the canvas {c}")
            out.paste(im.crop((sx, sy, sx + tw, sy + th)), (u, v))
            u += tw
            row_h = th
        v += row_h
    return out


def result(s):
    if not s:
        return ""
    n = len(s["colors"])
    return f"{s['stitches']:,} stitches, {s['trims']} trims, {n} colour{'' if n == 1 else 's'}"


def table(stats):
    rows = ["| Image | OpenStitch (Realistic view) | Wizard and result |", "|---|---|---|"]
    for key, (image, wizard, _, _) in GALLERY.items():
        if key[-3:] in ("-pe", "-os"):
            continue
        rows.append(f"| <img src=\"examples/images/{image}\" width=\"200\"> "
                    f"| <img src=\"docs/images/readme/gallery/{key}.png\" width=\"260\"> "
                    f"| **{wizard}**<br>{result(stats.get(key))} |")
    return "\n".join(rows)


def modes_table(stats):
    rows = ["| Image | OpenStitch, PE-DESIGN-compatible mode (default) | OpenStitch, OpenStitch mode ➕ |", "|---|---|---|"]
    for key, (image, wizard, _, _) in MODES.items():
        rows.append(f"| <img src=\"examples/images/{image}\" width=\"200\"><br>{wizard} "
                    f"| <img src=\"docs/images/readme/gallery/{key}-pe.png\" width=\"260\"><br>"
                    f"{result(stats.get(key + '-pe'))} "
                    f"| <img src=\"docs/images/readme/gallery/{key}-os.png\" width=\"260\"><br>"
                    f"{result(stats.get(key + '-os'))} |")
    return "\n".join(rows)


def main():
    p = argparse.ArgumentParser()
    p.add_argument("--only", help="comma-separated ids")
    p.add_argument("--release", action="store_true", help="use the release build (faster)")
    a = p.parse_args()
    keys = a.only.split(",") if a.only else list(GALLERY)
    cache = OUT / "stats.json"
    stats = json.loads(cache.read_text(encoding="utf-8")) if cache.exists() else {}
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        for key in keys:
            shutil.copy(IMAGES / GALLERY[key][0], work / GALLERY[key][0])
            s = convert(work, key)
            stats[key] = {"stitches": s["stitches"], "colors": s["colors"], "trims": s["trims"]}
            dst = shoot(work, key, (s["widthMM"], s["heightMM"]))
            print(f"{key}: {s['stitches']} stitches, {len(s['colors'])} colours, {dst.stat().st_size // 1024} KB")
    cache.write_text(json.dumps(stats, indent=1, sort_keys=True) + "\n", encoding="utf-8", newline="\n")
    text = README.read_text(encoding="utf-8")
    for marker, make in (("gallery", table), ("gallery-modes", modes_table)):
        text, n = re.subn(rf"(<!-- {marker}:start -->\n).*?(<!-- {marker}:end -->)",
                          lambda m: m.group(1) + make(stats) + "\n" + m.group(2), text, flags=re.S)
        if n != 1:
            sys.exit(f"README.md has no {marker}:start / {marker}:end markers")
    README.write_text(text, encoding="utf-8", newline="\n")


if __name__ == "__main__":
    sys.exit(main())
