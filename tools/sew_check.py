#!/usr/bin/env python3
"""Checks PES files for problems a machine sew-out shows but the editor may not:

- **Out of the hoop:** the stitched area, centred the way a Brother machine centres a design (on
  the middle of its stitch extents), must fit the hoop's sewing field. The PEC header's width and
  height must also cover the stitches, since the machine reads them to check the size.
- **Stacked needle drops:** too many penetrations in one small spot can break a needle.
  Reported per 0.5 mm cell, over the whole design and within one colour block.
- **Stitching in place:** runs of stitches shorter than 0.3 mm that do not travel.

    python3 -I tools/sew_check.py examples/sew            # every .pes under the folder
    python3 -I tools/sew_check.py file.pes --hoop 100x100

Reads the PEC block's stitches directly (the part the machine sews), not OpenStitch's objects.
The hoop comes from `--hoop`, else from examples/sew/manifest.json, else 100 × 100 mm.
Exits 1 when any file fails.
"""

import argparse
import json
import math
import struct
import sys
from collections import Counter
from pathlib import Path

# Limits. A Brother needle can take repeated penetrations, but dozens in one 0.5 mm spot within a
# colour (dense satin turns, stacked tie-ins, layered fills) cut thread and break needles.
CELL_MM = 0.5
MAX_PER_CELL = 20          # needle drops in one 0.5 mm cell (PE-DESIGN's own files reach 19)
MAX_PER_CELL_COLOUR = 20   # ... within one colour block
TINY_MM = 0.3
MAX_TINY_RUN = 6           # consecutive stitches shorter than 0.3 mm (tie-ins use 3 to 4)
FIELD_SLACK_MM = 0.0       # sewing field must not be exceeded at all
DENS_CELL_MM = 2.0
MAX_DENSITY = 80.0         # PE-DESIGN's wide satin references reach 75


def hoop_from(text):
    w, h = text.lower().replace("×", "x").replace(" ", "").replace("mm", "").split("x")[:2]
    return float(w), float(h)


def read_pec(path):
    """Returns (stitches, header): stitches as (x_mm, y_mm, kind, colour) with kind 'stitch',
    'jump' or 'trim'; header as the PEC graphic block's width and height in mm."""
    data = Path(path).read_bytes()
    pec = struct.unpack_from("<I", data, 8)[0]
    ncol = data[pec + 48] + 1
    g = pec + 512
    # 2 bytes 0, 3-byte offset to the thumbnails, 0x31 0xff 0xf0, width, height (0.1 mm), 0x1e0,
    # 0x1b0, then the stitches. Their first record is a long-form jump from the frame's top-left
    # corner to the first needle point: the machine draws and sews the design inside a frame of
    # the header's width and height, so every stitch must land within it.
    width, height = struct.unpack_from("<hh", data, g + 8)
    i = g + 16
    x = y = 0
    colour = 0
    out = []
    while i < len(data):
        b = data[i]
        if b == 0xFF:
            break
        if b == 0xFE and i + 1 < len(data) and data[i + 1] == 0xB0:
            colour += 1
            i += 3
            continue
        moves = []
        flags = 0
        for _ in range(2):
            b = data[i]
            if b & 0x80:
                flags |= b & 0x70
                v = ((b & 0x0F) << 8) | data[i + 1]
                if v & 0x800:
                    v -= 0x1000
                i += 2
            else:
                v = b if b < 0x40 else b - 0x80
                i += 1
            moves.append(v)
        x += moves[0]
        y += moves[1]
        kind = "trim" if flags & 0x20 else "jump" if flags & 0x10 else "stitch"
        out.append((x / 10.0, y / 10.0, kind, colour))
    return out, (width / 10.0, height / 10.0), ncol


def check(path, hoop):
    st, (hw, hh), ncol = read_pec(path)
    drops = [s for s in st if s[2] == "stitch"]
    problems = []
    xs = [s[0] for s in st]
    ys = [s[1] for s in st]
    w, h = max(xs) - min(xs), max(ys) - min(ys)
    # A hoop fits either way round (the 260 × 160 mm hoop is used upright on the NV880E).
    fits = (w <= hoop[0] + FIELD_SLACK_MM and h <= hoop[1] + FIELD_SLACK_MM) or (
        w <= hoop[1] + FIELD_SLACK_MM and h <= hoop[0] + FIELD_SLACK_MM)
    if not fits:
        problems.append(f"OUT OF HOOP: stitches span {w:.1f} × {h:.1f} mm, hoop {hoop[0]:.0f} × {hoop[1]:.0f}")
    over = (-min(xs), -min(ys), max(xs) - hw, max(ys) - hh)
    if max(over) > 0.15:
        problems.append(f"CROPPED ON THE MACHINE: stitches leave the PEC frame ({hw:.1f} × {hh:.1f} mm) by "
                        f"{over[0]:.1f} left, {over[1]:.1f} top, {over[2]:.1f} right, {over[3]:.1f} bottom mm")
    cells = Counter((math.floor(s[0] / CELL_MM), math.floor(s[1] / CELL_MM)) for s in drops)
    (cell, n), = cells.most_common(1) or [((0, 0), 0)]
    if n > MAX_PER_CELL:
        problems.append(f"STACKED: {n} needle drops in one {CELL_MM} mm spot at "
                        f"({cell[0] * CELL_MM - min(xs):.1f}, {cell[1] * CELL_MM - min(ys):.1f}) mm")
    percol = Counter((s[3], math.floor(s[0] / CELL_MM), math.floor(s[1] / CELL_MM)) for s in drops)
    (pc, pn), = percol.most_common(1) or [((0, 0, 0), 0)]
    if pn > MAX_PER_CELL_COLOUR and pn != n:
        problems.append(f"STACKED IN ONE COLOUR: {pn} drops in one spot (colour {pc[0] + 1})")
    # Thread laid per mm² in 2 mm cells (stitch length split by its midpoint's cell): one layer of
    # fill at PE-DESIGN's 4.5 rows/mm lays about 4.5 mm/mm²; stacked layers show as far more.
    lay = Counter()
    prev = None
    for s in st:
        if s[2] == "stitch" and prev is not None:
            mx, my = (s[0] + prev[0]) / 2, (s[1] + prev[1]) / 2
            lay[(math.floor(mx / DENS_CELL_MM), math.floor(my / DENS_CELL_MM))] += math.hypot(s[0] - prev[0], s[1] - prev[1])
        prev = s
    (dcell, dlen), = lay.most_common(1) or [((0, 0), 0.0)]
    density = dlen / DENS_CELL_MM ** 2
    if density > MAX_DENSITY:
        problems.append(f"DENSE: {density:.1f} mm of thread per mm² around "
                        f"({dcell[0] * DENS_CELL_MM - min(xs):.0f}, {dcell[1] * DENS_CELL_MM - min(ys):.0f}) mm")
    run = best = 0
    prev = None
    for s in st:
        if s[2] == "stitch" and prev is not None and math.hypot(s[0] - prev[0], s[1] - prev[1]) < TINY_MM:
            run += 1
            best = max(best, run)
        else:
            run = 0
        prev = s
    if best > MAX_TINY_RUN:
        problems.append(f"IN PLACE: {best} stitches in a row shorter than {TINY_MM} mm")
    return {
        "size": (w, h), "header": (hw, hh), "stitches": len(drops), "colours": ncol,
        "max_cell": n, "max_cell_colour": pn, "tiny_run": best, "density": density,
        "problems": problems,
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("paths", nargs="+")
    ap.add_argument("--hoop", help="hoop as WxH in mm, e.g. 100x100")
    a = ap.parse_args()
    files = []
    for p in map(Path, a.paths):
        files += sorted(p.rglob("*.pes")) if p.is_dir() else [p]
    failed = 0
    for f in files:
        hoop = None
        if a.hoop:
            hoop = hoop_from(a.hoop)
        else:
            for parent in f.parents:
                m = parent / "manifest.json"
                if m.is_file():
                    entry = json.loads(m.read_text(encoding="utf-8")).get(f.relative_to(parent).as_posix())
                    if entry and "hoop" in entry:
                        hoop = hoop_from(entry["hoop"])
                    break
        hoop = hoop or (100.0, 100.0)
        r = check(f, hoop)
        status = "FAIL" if r["problems"] else "ok"
        failed += bool(r["problems"])
        print(f"{status:4} {f.as_posix()}  {r['size'][0]:.1f}×{r['size'][1]:.1f} mm (header "
              f"{r['header'][0]:.1f}×{r['header'][1]:.1f}, hoop {hoop[0]:.0f}×{hoop[1]:.0f})  "
              f"max/spot {r['max_cell']} (colour {r['max_cell_colour']})  tiny run {r['tiny_run']}  "
              f"density {r['density']:.1f}")
        for p in r["problems"]:
            print("     ", p)
    print(f"{len(files) - failed}/{len(files)} pass")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
