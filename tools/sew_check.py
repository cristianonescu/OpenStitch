#!/usr/bin/env python3
"""Checks PES files for problems a machine sew-out shows but the editor may not:

- **Out of the hoop:** the stitched area must fit the hoop's sewing field.
- **The PEC frame:** the header's width and height, measured from where the stitch data starts.
  The machine reads it as the design's size (a frame larger than the hoop is refused) and sews
  only inside it (stitches outside it are cut off), so it must hold every stitch and fit the hoop.
- **Stacked needle drops:** too many penetrations in one small spot can break a needle.
  Reported per 0.5 mm cell, over the whole design and within one colour block.
- **Stitching in place:** runs of stitches shorter than 0.3 mm that do not travel.

    python3 -I tools/sew_check.py examples/sew            # every .pes under the folder
    python3 -I tools/sew_check.py file.pes --hoop 100x100
    python3 -I tools/sew_check.py file.vp3 --hoop 100x100   # VP3 (#480): no PEC frame to check
    python3 -I tools/sew_check.py file.u01 --hoop 100x100   # also .tbf and .tap (#481)

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


def read_vp3(path):
    """Husqvarna/Pfaff VP3 (docs/FILE_FORMATS.md "VP3"), as read_pec returns it. VP3 has no PEC
    frame: the header returned is the stitches' own extent, and the points are moved so the
    extent starts at (0, 0)."""
    data = Path(path).read_bytes()
    be16 = lambda at: struct.unpack_from(">H", data, at)[0]  # noqa: E731
    i = 6 + 2 + be16(6)  # magic, producer
    i += 7  # file block tag and length
    i += 2 + be16(i) + 16 + 4 + 5  # settings, bounds, record count, colours, 0C 00 01
    i += 7 + 8 + 3 + 24  # design block, centre, 00 00 00, extents
    i += 2 + be16(i) + 2 + 16 + 6  # notes, 64 64, matrix, "xxPP" 01 00
    i += 2 + be16(i)  # producer
    ncol = be16(i)
    i += 2
    x = y = 0
    out = []
    for colour in range(ncol):
        end = i + 7 + struct.unpack_from(">I", data, i + 3)[0]
        i += 7 + 8  # block tag, length, start
        n = data[i]
        i += 2 + 6 * n + 2  # colours, transition, rgb + 3 each, type, weight
        for _ in range(3):  # catalogue number, name, brand
            i += 2 + be16(i)
        i += 8 + 7 + 3  # shift, stitch block tag and length, 0A F6 00
        moving = True
        while i + 1 < end - 1:
            a, b = data[i], data[i + 1]
            i += 2
            if a == 0x80:
                if b == 0x03:
                    out.append((x / 10.0, y / 10.0, "trim", colour))
                    moving = True
                elif b == 0x01:
                    dx, dy = struct.unpack_from(">hh", data, i)
                    i += 6  # the two words and 80 02
                    x += dx
                    y += dy
                    out.append((x / 10.0, y / 10.0, "jump" if moving else "stitch", colour))
                continue
            x += a - 256 if a > 127 else a
            y += b - 256 if b > 127 else b
            out.append((x / 10.0, y / 10.0, "stitch", colour))
            moving = False
        i = end
    xs = [s[0] for s in out] or [0]
    ys = [s[1] for s in out] or [0]
    mx, my = min(xs), min(ys)
    out = [(s[0] - mx, s[1] - my, s[2], s[3]) for s in out]
    return out, (max(xs) - mx, max(ys) - my), ncol


def _framed(out, ncol):
    """Moves the points so their extent starts at (0, 0); the header is that extent."""
    xs = [s[0] for s in out] or [0]
    ys = [s[1] for s in out] or [0]
    mx, my = min(xs), min(ys)
    out = [(s[0] - mx, s[1] - my, s[2], s[3]) for s in out]
    return out, (max(xs) - mx, max(ys) - my), ncol


def read_records(path):
    """Tajima TAP/TBF and Barudan U01 (docs/FILE_FORMATS.md, #481), as read_vp3 returns them."""
    data = Path(path).read_bytes()
    ext = Path(path).suffix.lower()
    x = y = 0
    colour = 0
    sewn = False
    out = []
    s8 = lambda b: b - 256 if b > 127 else b  # noqa: E731
    if ext == ".tap":
        for i in range(0, len(data) - 2, 3):
            b0, b1, b2 = data[i], data[i + 1], data[i + 2]
            if b2 & 0xF3 == 0xF3:
                break
            if b2 & 0xC3 == 0xC3:
                colour += 1
                continue
            bit = lambda b, n: (b >> n) & 1  # noqa: E731
            dx = (81 * (bit(b2, 2) - bit(b2, 3)) + 27 * (bit(b1, 2) - bit(b1, 3)) + 9 * (bit(b0, 2) - bit(b0, 3))
                  + 3 * (bit(b1, 0) - bit(b1, 1)) + (bit(b0, 0) - bit(b0, 1)))
            dy = (81 * (bit(b2, 5) - bit(b2, 4)) + 27 * (bit(b1, 5) - bit(b1, 4)) + 9 * (bit(b0, 5) - bit(b0, 4))
                  + 3 * (bit(b1, 7) - bit(b1, 6)) + (bit(b0, 7) - bit(b0, 6)))
            x += dx
            y -= dy
            out.append((x / 10.0, y / 10.0, "jump" if b2 & 0x80 else "stitch", colour))
    elif ext == ".tbf":
        for i in range(0x600, len(data) - 2, 3):
            dx, dy, c = s8(data[i]), s8(data[i + 1]), data[i + 2]
            if c in (0x80, 0x90):
                x += dx
                y -= dy
                out.append((x / 10.0, y / 10.0, "stitch" if c == 0x80 else "jump", colour))
                sewn |= c == 0x80
            elif c == 0x86:
                out.append((x / 10.0, y / 10.0, "trim", colour))
            elif c == 0x81:
                colour += sewn
            elif c != 0x40:
                break
    else:  # .u01
        for i in range(0x100, len(data) - 2, 3):
            c, by, bx = data[i], data[i + 1], data[i + 2]
            dy = by if c & 0x40 else -by
            dx = -bx if c & 0x20 else bx
            low = c & 0x1F
            if low > 0x17:
                break
            if low <= 0x05:
                x += dx
                y += dy
                out.append((x / 10.0, y / 10.0, "jump" if low & 1 else "stitch", colour))
                sewn |= not low & 1
            elif low in (6, 7):
                out.append((x / 10.0, y / 10.0, "trim", colour))
            elif low >= 9:
                colour += sewn
    return _framed(out, colour + 1)


def check(path, hoop):
    suffix = Path(path).suffix.lower()
    reader = read_vp3 if suffix == ".vp3" else read_records if suffix in (".tap", ".tbf", ".u01") else read_pec
    st, (hw, hh), ncol = reader(path)
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
    # The machine reads the PEC frame as the design's size: a frame larger than the hoop is
    # refused ("The pattern is too large for the extra large embroidery frame", NV880E, for a
    # 290 × 190 mm frame), whatever the stitches' own size.
    frame_fits = (hw <= hoop[0] + FIELD_SLACK_MM and hh <= hoop[1] + FIELD_SLACK_MM) or (
        hw <= hoop[1] + FIELD_SLACK_MM and hh <= hoop[0] + FIELD_SLACK_MM)
    if not frame_fits and reader is read_pec:
        problems.append(f"REFUSED BY THE MACHINE: PEC frame {hw:.1f} × {hh:.1f} mm is larger than the "
                        f"{hoop[0]:.0f} × {hoop[1]:.0f} mm hoop")
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
