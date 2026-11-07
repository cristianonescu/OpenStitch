#!/usr/bin/env python3
"""Regenerates the ready-to-sew kit in examples/sew: calibration designs, machine checks and
demo designs for a single-needle Brother machine (written for the NV880E and its 100 × 100,
150 × 150 and 260 × 160 mm hoops), all made by OpenStitch itself.

    cargo build --release -p stitchtool -p openstitch-app
    python3 -I tools/sew_kit.py            # rewrite the .pes files, previews, manifest, README
    python3 -I tools/sew_kit.py --check    # fail if a committed .pes differs from a fresh build
    python3 -I tools/sew_kit.py --check-previews   # fail if a preview lacks a thread's colour

Each design is a `stitchtool exec` script (the KIT list below), except the machine checks, which
are the compatibility specs with a sew-out record (`compat/machine`), exported by
`stitchtool compat --only <spec>` so a record's `fileSha256` matches the kit's file. Each design
gets a preview (the editor's Realistic view, `openstitch --screenshot`, cropped as
`tools/gallery.py` crops), and the kit's README is rewritten between the `kit:start` and
`kit:end` markers: per design, a description, the source image and the preview, the hoop, size,
stitches, trims and threads (Brother catalogue code and name where the colour is on the PEC
chart). manifest.json records each file's SHA-256. OpenStitch's output is the same on macOS,
Windows and Linux, so a fresh build matches the committed files byte for byte until the stitch
engine changes. A preview must show every thread of its design within 20 ΔE of the
thread's RGB (`--check-previews`, #564). Needs Pillow and numpy.
"""

import argparse
import hashlib
import json
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

from PIL import Image

sys.path.insert(0, str(Path(__file__).resolve().parent))
import gallery  # noqa: E402  (design_box: the stitched area of a Realistic-view screenshot)

ROOT = Path(__file__).resolve().parent.parent
BIN = ROOT / "target" / "release"
IMAGES = ROOT / "examples" / "images"
KIT = ROOT / "examples" / "sew"

def retry(write, attempts=20):
    """Runs `write`, retrying while Windows briefly locks a file just written (a virus scan or
    the search indexer opening it shows as EINVAL or a permission error)."""
    import time
    for i in range(attempts):
        try:
            return write()
        except OSError:
            if i == attempts - 1:
                raise
            time.sleep(0.5)

RAMP = (800, 200)  # the generated grey ramp's size in pixels

# NV880E hoops: how the design page is set, and the box a design is fitted into (5 mm margins).
HOOPS = {
    "S": {"label": "100 × 100 mm", "page": [{"command": "machine.set", "params": {"hoop": "100x100"}}],
          "box": (90, 90)},
    "M": {"label": "150 × 150 mm", "page": [{"command": "design.new", "params": {"width": 150, "height": 150}}],
          "box": (140, 140)},
    "L": {"label": "260 × 160 mm", "page": [{"command": "machine.set",
                                              "params": {"hoop": "160x260", "rotate90": True}}],
          "box": (250, 150)},
}

# Card threads by PEC chart index: 29 White, 20 Black, 5 Red, 2 Blue, 13 Yellow, 37 Leaf Green.
CARD_THREADS = [29, 20, 5, 2, 13, 37]
CARD_DENSITIES = [3.0, 4.5, 6.0]          # rows/mm: light, PE-DESIGN's default, dense
RAMP_DENSITIES = [0.5, 1.0, 1.5, 2.0, 3.0, 4.5]
SATIN_WIDTHS = [1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 6.0]


def swatches():
    """Six threads at three densities, and a black coverage ramp, all without underlay. Objects
    go in thread order (Black's column, ramp and mark together), so each thread is sewn once.
    Each swatch is entered at its top-left corner, where its first row starts: entered elsewhere,
    the fill sewed a straight travel across the swatch first (#628), extra thread inside the
    area the calibration measures."""
    cells = []
    for col, thread in enumerate(CARD_THREADS):
        cells += [(col, row, thread, d) for row, d in enumerate(CARD_DENSITIES)]
        if thread == 20:
            cells += [(c, 3, 20, d) for c, d in enumerate(RAMP_DENSITIES)]
    steps = []
    # Object ids count every object, the T1 mark included: each swatch's settings go to its own
    # rectangle (counting swatches only gave every swatch after the mark the next one's, #297).
    n = 0
    for col, row, thread, d in cells:
        n += 1
        steps += [
            {"command": "shape.rectangle", "params": {"x": 9 + col * 14, "y": 9 + row * 15, "width": 12,
                                                      "height": 12, "fill": thread}},
            {"command": "object.setRegion", "params": {"ids": [n], "type": "fill", "density": d,
                                                       "underlay": False, "angle": 0}},
            # Entered where its first row starts (#628).
            {"command": "object.setEntryExit", "params": {"id": n, "regionEntry": [9 + col * 14, 9 + row * 15]}},
        ]
        if (thread, row, col) == (20, 3, len(RAMP_DENSITIES) - 1):
            steps.append({"command": "text.add", "params": {"text": "T1", "size": 6, "x": 9, "y": 77,
                                                            "fill": 20}})
            n += 1
    return steps


# T2 card (#311): two contrasting threads in alternating rows. Black and Yellow, as on the T1 card,
# both far from the owner's pink cotton and from each other.
T2_THREADS = (20, 13)                     # PEC chart: 20 Black (900), 13 Yellow (205)
T2_DENSITIES = [2.0, 3.0, 4.0, 5.0]       # rows/mm of each thread


def t2_cells():
    """The T2 card's swatches in sewing order: (box (x, y, w, h) in mm, PEC thread, rows/mm
    of that thread, column). One row per density; the columns, left to right: solid Black at the
    two threads' total density, Black rows then Yellow rows between them, Yellow then Black,
    solid Yellow at the total density. A fill's rows start half a spacing below its top edge
    (`region::row_positions`), so the second thread's rectangle sits half a spacing lower and
    its rows fall midway between the first thread's. Sewing order: Black, Yellow, Black, so in
    each mixed swatch the thread named second is sewn later and lies on top."""
    black, yellow = T2_THREADS
    out = {1: [], 2: [], 3: []}  # thread block: its swatches
    for row, d in enumerate(T2_DENSITIES):
        y, half = 9 + row * 15, 0.5 / d
        box = lambda col, dy=0.0: (9 + col * 15, round(y + dy, 4), 12, 12)  # noqa: E731
        out[1] += [(box(0), black, 2 * d, 0), (box(1), black, d, 1)]
        out[2] += [(box(1, half), yellow, d, 1), (box(2), yellow, d, 2), (box(3), yellow, 2 * d, 3)]
        out[3] += [(box(2, half), black, d, 2)]
    return [cell for block in (1, 2, 3) for cell in out[block]]


def t2_card():
    """The T2 calibration card (#311): 16 swatches 12 mm square, all fills at 0° without underlay
    (as T1), and a satin "T2" mark below the bottom-left swatch."""
    steps = []
    for n, ((x, y, w, h), thread, d, _) in enumerate(t2_cells(), 1):
        steps += [
            {"command": "shape.rectangle", "params": {"x": x, "y": y, "width": w, "height": h, "fill": thread}},
            {"command": "object.setRegion", "params": {"ids": [n], "type": "fill", "density": d,
                                                       "underlay": False, "angle": 0}},
            {"command": "object.setEntryExit", "params": {"id": n, "regionEntry": [x, y]}},
        ]
    steps.append({"command": "text.add", "params": {"text": "T2", "size": 6, "x": 9, "y": 77,
                                                    "fill": T2_THREADS[0]}})
    return steps


def t1_cells():
    """The T1 card's swatches as t2_cells lists them."""
    out = []
    for col, thread in enumerate(CARD_THREADS):
        out += [((9 + col * 14, 9 + row * 15, 12, 12), thread, d, col) for row, d in enumerate(CARD_DENSITIES)]
        if thread == 20:
            out += [((9 + c * 14, 54, 12, 12), 20, d, c) for c, d in enumerate(RAMP_DENSITIES)]
    return out


def satin_ladder():
    """Satin columns 1 to 6 mm wide, and a fill square without and with underlay."""
    steps, n, x = [], 0, 9.0
    for w in SATIN_WIDTHS:
        n += 1
        steps += [
            {"command": "shape.rectangle", "params": {"x": x, "y": 9, "width": w, "height": 40, "fill": 20}},
            {"command": "object.setRegion", "params": {"ids": [n], "type": "satin", "autoDirection": True,
                                                       "underlay": False}},
        ]
        x += w + 5
    for i, underlay in enumerate([False, True]):
        n += 1
        steps += [
            {"command": "shape.rectangle", "params": {"x": 9 + i * 45, "y": 58, "width": 36, "height": 30,
                                                      "fill": 5}},
            {"command": "object.setRegion", "params": {"ids": [n], "type": "fill", "density": 4.5,
                                                       "underlay": underlay, "angle": 45}},
            # Entered at a corner, the travel to the first row runs along the edge, not across
            # the single layer (#628).
            {"command": "object.setEntryExit", "params": {"id": n, "regionEntry": [9 + i * 45, 88]}},
        ]
    return steps


def grey_ramp_png(path):
    """An eight-step grey ramp, white to black (an uncompressed-filter PNG written by hand, so
    its bytes never change)."""
    w, h, steps = *RAMP, 8
    row = bytes(255 - round(255 * (x * steps // w) / (steps - 1)) for x in range(w))
    raw = b"".join(b"\x00" + row for _ in range(h))

    def chunk(kind, data):
        return struct.pack(">I", len(data)) + kind + data + struct.pack(">I", zlib.crc32(kind + data))

    path.write_bytes(b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 0, 0, 0, 0))
                     + chunk(b"IDAT", zlib.compress(raw, 9)) + chunk(b"IEND", b""))


def digitize(command, image, hoop, mode="openStitch", **params):
    """One image conversion fitted into the hoop's box and centred."""
    w, h = RAMP if image == "grey-ramp.png" else Image.open(IMAGES / image).size
    bw, bh = HOOPS[hoop]["box"]
    width = round(min(bw, bh * w / h), 1)
    return [{"command": command, "params": {"path": image, "width": width, "compatMode": mode, **params}}]


THREADS = [2, 5, 37, 12, 7, 21, 25, 32, 24]  # Blue, Red, Leaf Green, Orange, Magenta, Ultramarine, …


def cells(n, cols=3, size=26.0, gap=4.0, x0=7.0, y0=7.0):
    """Top-left corners of an n-cell grid on a 100 mm page, row by row."""
    return [(x0 + (i % cols) * (size + gap), y0 + (i // cols) * (size + gap)) for i in range(n)]


def region_grid(variants, size=26.0):
    """One shape per variant (`circle` or a rectangle), each sewn with its own region settings."""
    steps = []
    for n, ((shape, region), (x, y)) in enumerate(zip(variants, cells(len(variants), size=size)), 1):
        color = THREADS[(n - 1) % len(THREADS)]
        if shape == "circle":
            steps.append({"command": "shape.ellipse", "params": {"cx": x + size / 2, "cy": y + size / 2,
                                                                 "rx": size / 2, "fill": color}})
        else:
            steps.append({"command": "shape.rectangle", "params": {"x": x, "y": y, "width": size, "height": size,
                                                                   "fill": color}})
        steps.append({"command": "object.setRegion", "params": {"ids": [n], **region}})
    return steps


def line_rows(variants):
    """One gently curved 84 mm line per variant, 10 mm apart."""
    steps = []
    for n, line in enumerate(variants, 1):
        y = 8 + (n - 1) * 10.5
        steps += [
            {"command": "shape.path", "params": {"points": [[8, y], [92, y]], "closed": False,
                                                 "curves": [[[36, y - 5], [64, y + 5]]],
                                                 "outline": THREADS[(n - 1) % len(THREADS)]}},
            {"command": "object.setLine", "params": {"ids": [n], **line}},
        ]
    return steps


REGION_TYPES_A = [
    ("rect", {"type": "fill"}),
    # A 26 mm satin split at 7 mm, so no stitch is long enough to snag (#450).
    ("rect", {"type": "satin", "angle": 90, "splitLength": 7}),
    ("rect", {"type": "programmableFill", "pattern": "diamonds"}),
    ("rect", {"type": "motifFill", "pattern": "heart"}),
    ("rect", {"type": "piping"}),
    # At 0° the cells are crosses (X); at the default 45° they read as "+" (#450).
    ("rect", {"type": "crossStitch", "spacing": 2.5, "angle": 0}),
    ("circle", {"type": "concentricCircle"}),
    ("circle", {"type": "radial"}),
    ("circle", {"type": "spiral"}),
]
REGION_TYPES_B = [
    ("circle", {"type": "flexibleSpiral"}),
    ("rect", {"type": "stippling"}),
    ("rect", {"type": "netFill", "pattern": "square"}),
    ("rect", {"type": "zigzagNetFill", "pattern": "honeycomb"}),
    ("rect", {"type": "decorativeFill", "pattern": "waves"}),
    ("circle", {"type": "contour", "contourStrategy": "rings"}),
    ("circle", {"type": "contour", "contourStrategy": "doubleSpiral"}),
    ("rect", {"type": "programmableFill", "pattern": "bricks", "baseSewing": True}),
    ("rect", {"type": "motifFill", "pattern": "leaf"}),
]
FILL_OPTIONS = [
    ("rect", {"type": "fill", "density": 4.5, "angle": 45}),
    ("rect", {"type": "fill", "gradation": 20}),
    ("rect", {"type": "fill", "gradationPattern": True}),
    ("rect", {"type": "fill", "featherSide": "both", "featherWidth": 4}),
    ("rect", {"type": "fill", "stitchType": 2}),
    ("rect", {"type": "fill", "stagger": "random", "staggerSeed": 7}),
    ("rect", {"type": "fill", "frequency": 0}),
    ("circle", {"type": "fill", "autoDirection": True}),
    ("rect", {"type": "satin", "angle": 90, "pullCompensation": 0.4, "splitLength": 7}),
]
# OpenStitch's named tatami textures (#537), on fills at 0° so the needle-hole patterns read across.
TEXTURES_A = [("rect", {"type": "fill", "angle": 0, "texture": t}) for t in
              ["smooth", "ribs", "brick", "diagonal", "steepDiagonal", "steps", "stairs", "blocks", "scattered"]]
TEXTURES_B = [("rect", {"type": "fill", "angle": 0, "texture": t}) for t in
              ["waves", "ripples", "zigzag", "chevron", "scallops"]]
UNDERLAYS = [
    ("rect", {"type": "fill", "underlay": False}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "edge"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "zigzag"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "edgeZigzag"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "double"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "edgeFill"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "cross"}),
    ("rect", {"type": "satin", "angle": 90, "splitLength": 7, "underlay": True, "underlayParts": ["center"]}),
    ("rect", {"type": "fill", "underlay": True, "underlayDensity": "light"}),
]
# Wave fill (#538, beyond PE-DESIGN): height and length in mm.
WAVES = [
    ("rect", {"type": "fill", "angle": 0, "waveHeight": 2, "waveLength": 16}),
    ("rect", {"type": "fill", "angle": 0, "waveHeight": 3, "waveLength": 10}),
    ("rect", {"type": "fill", "angle": 0, "waveHeight": 5, "waveLength": 24}),
    ("rect", {"type": "fill", "angle": 45, "waveHeight": 3, "waveLength": 14}),
    ("circle", {"type": "fill", "angle": 0, "waveHeight": 3, "waveLength": 12}),
    ("rect", {"type": "fill", "angle": 90, "waveHeight": 8, "waveLength": 8}),
]
# Maze fill (#533, beyond PE-DESIGN).
MAZES = [
    ("rect", {"type": "maze"}),
    ("rect", {"type": "maze", "straightness": 0}),
    ("rect", {"type": "maze", "straightness": 95}),
    ("circle", {"type": "maze"}),
    ("rect", {"type": "maze", "mazeSeed": 7, "angle": 45}),
    ("rect", {"type": "maze", "spacing": 3, "runPitch": 3}),
]
LINE_TYPES = [
    {"type": "running"},
    {"type": "triple"},
    {"type": "zigzag", "width": 2},
    {"type": "motif", "motif": "wave", "height": 4},
    {"type": "stem", "width": 2},
    {"type": "candlewick", "width": 2},
    {"type": "eStitch", "width": 3},
    {"type": "vStitch", "width": 3},
    {"type": "chain", "width": 2},
]


def shapes():
    """Shape tools: presets, a skeleton satin, a Manual Punch column, a floral pattern."""
    presets = [("star", {"sides": 5}), ("heart", {}), ("crescent", {}), ("arrow", {}), ("teardrop", {}),
               ("polygon", {"sides": 6})]
    steps = [{"command": "shape.preset", "params": {"kind": kind, "cx": x + 13, "cy": y + 13, "width": 24,
                                                     "fill": THREADS[i], **extra}}
             for i, ((kind, extra), (x, y)) in enumerate(zip(presets, cells(6)))]
    steps += [
        {"command": "shape.skeletonSatin", "params": {"points": [[8, 82], [20, 70], [32, 82]],
                                                      "widths": [1.5, 4, 1.5], "fill": 24}},
        # Manual Punch rungs are the column's crossings: 6 mm rungs make a 6 mm column (20 mm
        # rungs sewed 20-24 mm stitches, #450).
        {"command": "shape.column", "params": {"lines": [[[40, 77], [40, 83]], [[51, 74], [51, 80]],
                                                         [[62, 77], [62, 83]]], "curved": True, "fill": 32}},
        {"command": "shape.ellipse", "params": {"cx": 80, "cy": 80, "rx": 12, "outline": 20}},
        {"command": "shape.floral", "params": {"id": 9, "density": 3, "pattern": 12, "color": 7}},
    ]
    return steps


def holes_and_emboss():
    """Hole sewing, and an engraved line, an engraved region and an embossed region."""
    steps = [
        {"command": "shape.ellipse", "params": {"cx": 27, "cy": 27, "rx": 20, "fill": 2}},
        {"command": "shape.ellipse", "params": {"cx": 27, "cy": 27, "rx": 8, "fill": 13}},
        {"command": "object.setHoleSewing", "params": {"ids": [1, 2], "enabled": True}},
    ]
    n = 2
    for (x, y), effect in zip([(53, 7), (7, 53), (53, 53)], ["engravedLine", "engravedRegion", "embossedRegion"]):
        steps += [
            {"command": "shape.rectangle", "params": {"x": x, "y": y, "width": 40, "height": 40, "fill": 5}},
            {"command": "shape.preset", "params": {"kind": "star", "cx": x + 20, "cy": y + 20, "width": 26,
                                                   "outline": 20}},
            {"command": "shape.emboss", "params": {"id": n + 1, "tool": n + 2, "effect": effect}},
        ]
        n += 2
    return steps


def frames():
    """The decorative frames: five Shape-tool frames and an imported one. Ornaments 5 mm long (at
    2 mm a scallop or wave was shorter than one running stitch and sewed as a plain square, #450); the
    corners frame keeps 2 mm brackets, which at 5 mm cross its circle."""
    steps = []
    for i, ((x, y), kind) in enumerate(zip(cells(5, size=20, gap=12, x0=12, y0=12),
                                           ["scallop", "wave", "zigzag", "scallopOval", "corners"]), 1):
        steps += [
            {"command": "shape.ellipse", "params": {"cx": x + 10, "cy": y + 10, "rx": 4, "fill": THREADS[i]}},
            {"command": "shape.decorative", "params": {"kind": kind, "ids": [len([s for s in steps if s["command"]
                                                                                 .startswith("shape.")]) + 1],
                                                       "margin": 3, "size": 2 if kind == "corners" else 5,
                                                       "color": 20}},
        ]
    # decoration.add frames a box, as PE-DESIGN's Decorative Import does: the laurel's opening is a
    # small part of it, so a 14 mm box makes a frame about 32 x 28 mm. It goes below the grid, clear
    # of the other frames (#296; at (68, 46) with an 18 mm box it overlapped the corners frame).
    steps.append({"command": "decoration.add", "params": {"frame": "laurel", "x": 43, "y": 74, "width": 14,
                                                          "height": 14, "color": 37}})
    return steps


# Colour blocks that must not overlap one another, as groups of block indexes (sewing order): each
# frame with its circle, and the laurel (#296).
APART = {"shapes/decorative-frames.pes": [[0, 1], [2, 3], [4, 5], [6, 7], [8, 9], [10]]}


def check_objects(rel, objects):
    """Exits when the T1 card's objects are not sewn as meant: every swatch fill, the mark satin
    lettering (#297: the mark once took a swatch's fill settings)."""
    if rel != "calibration/t1-swatches.pes":
        return
    text = [o for o in objects if o.get("kind") == "text"]
    shapes = [o for o in objects if o.get("kind") == "shape"]
    if len(text) != 1 or text[0].get("region") != "satin":
        sys.exit(f"{rel}: the T1 mark must be satin lettering, got {text}")
    if any(o.get("region") != "fill" for o in shapes):
        sys.exit(f"{rel}: every swatch must be a fill")


# Calibration cards whose swatch densities are measured in the exported file (#312): the build
# fails when a swatch's rows per mm differ from what the card says it sews by more than
# DENSITY_TOLERANCE, and the manifest records the measured values.
CARDS = {"calibration/t1-swatches.pes": t1_cells, "calibration/t2-alternating-rows.pes": t2_cells}
DENSITY_TOLERANCE = 0.2   # rows/mm
SWATCH_INSET = 2.0        # mm cut from each side of a swatch before its rows are counted


def stitch_runs(st):
    """Runs of needle drops in one colour without a jump or trim between them: (colour, points)."""
    runs, cur = [], None
    for x, y, kind, colour in st:
        if kind != "stitch":
            cur = None
            continue
        if cur is None or cur[0] != colour:
            cur = (colour, [])
            runs.append(cur)
        cur[1].append((x, y))
    return runs


def row_density(points):
    """Rows per mm of fill rows at 0° (horizontal): every needle point of a row has the same y,
    so the rows are the distinct y values (PEC's 0.1 mm grid; rows closer than that cannot be
    told apart, i.e. above 10 rows/mm)."""
    ys = sorted({round(y, 1) for _, y in points})
    if len(ys) < 3:
        return None
    return (len(ys) - 1) / (ys[-1] - ys[0])


def measure_card(rel, path):
    """Each swatch's measured rows/mm in the exported PES, in t1_cells/t2_cells order. The PEC
    frame starts at the stitches' top-left corner, not the page's, so the swatches are found by
    matching the sewn runs' box centres to the swatches' centres, then cut out SWATCH_INSET inside
    their box and measured on their own colour block's needle points. Exits on a swatch that
    is missing or sewn at the wrong density."""
    import sew_check  # noqa: E402
    st, _, _ = sew_check.read_pec(path)
    cells = CARDS[rel]()
    block, blocks, last = -1, [], None
    for _, thread, _, _ in cells:
        block += thread != last
        last = thread
        blocks.append(block)
    runs = [(c, pts) for c, pts in stitch_runs(st) if len(pts) > 30]
    def centre(pts):
        xs, ys = [p[0] for p in pts], [p[1] for p in pts]
        return (min(xs) + max(xs)) / 2, (min(ys) + max(ys)) / 2
    want = [(x + w / 2, y + h / 2) for (x, y, w, h), _, _, _ in cells]

    def matched(t):
        return sum(any(c == b and abs(cx - t[0] - wx) < 1 and abs(cy - t[1] - wy) < 1
                       for c, pts in runs for cx, cy in [centre(pts)])
                   for b, (wx, wy) in zip(blocks, want))
    shifts = [(cx - want[0][0], cy - want[0][1]) for c, pts in runs if c == blocks[0] for cx, cy in [centre(pts)]]
    t = max(shifts, key=matched, default=None)
    if t is None or matched(t) != len(cells):
        sys.exit(f"{rel}: could not find every swatch in the exported stitches")
    out = []
    for ((x, y, w, h), thread, d, _), b in zip(cells, blocks):
        x0, y0 = x + t[0] + SWATCH_INSET, y + t[1] + SWATCH_INSET
        pts = [(px, py) for px, py, kind, c in st if kind == "stitch" and c == b
               and x0 <= px <= x0 + w - 2 * SWATCH_INSET and y0 <= py <= y0 + h - 2 * SWATCH_INSET]
        got = row_density(pts)
        if got is None or abs(got - d) > DENSITY_TOLERANCE:
            sys.exit(f"{rel}: the swatch at ({x:g}, {y:g}) mm sews {got and round(got, 2)} rows/mm, "
                     f"the card says {d:g} (#312)")
        out.append({"boxMM": [x, y, w, h], "thread": thread, "rowsPerMM": d, "measuredRowsPerMM": round(got, 2)})
    return out


def check_apart(rel, path):
    """Exits when two groups of colour blocks in APART[rel] overlap (their stitch boxes intersect)."""
    sys.path.insert(0, str(ROOT / "tools"))
    import sew_check  # noqa: E402
    st, _, _ = sew_check.read_pec(path)
    boxes = []
    for group in APART[rel]:
        pts = [(x, y) for x, y, k, c in st if k == "stitch" and c in group]
        boxes.append((min(p[0] for p in pts), min(p[1] for p in pts), max(p[0] for p in pts), max(p[1] for p in pts)))
    for i, a in enumerate(boxes):
        for j, b in enumerate(boxes[i + 1:], i + 1):
            if a[0] < b[2] and b[0] < a[2] and a[1] < b[3] and b[1] < a[3]:
                sys.exit(f"{rel}: colour blocks {APART[rel][i]} and {APART[rel][j]} overlap")


def lettering():
    """Both fonts, a Text Transform arch, text on a circle, and a script line."""
    return [
        {"command": "text.add", "params": {"text": "OpenStitch", "size": 10, "x": 50, "y": 18, "align": "center",
                                           "fill": 2}},
        {"command": "text.add", "params": {"text": "Pacifico", "font": "pacifico", "size": 10, "x": 50, "y": 34,
                                           "align": "center", "fill": 5}},
        {"command": "text.add", "params": {"text": "ARCH", "size": 9, "x": 50, "y": 48, "align": "center",
                                           "envelope": 1, "fill": 37}},
        {"command": "shape.ellipse", "params": {"cx": 50, "cy": 76, "rx": 16, "outline": 40}},
        {"command": "text.add", "params": {"text": "ROUND AND ROUND", "size": 5, "fill": 7}},
        {"command": "text.setPath", "params": {"id": 5, "path": 4, "align": "spaced"}},
        # The circle only guides the text: not sewn (a thin grey ring read as a stray line, #441).
        {"command": "object.setLine", "params": {"ids": [4], "type": "none"}},
    ]


def monograms():
    return [
        {"command": "monogram.add", "params": {"letters": "OS", "style": "block", "size": 16, "x": 40, "y": 40,
                                               "frame": "ring", "fill": 2, "frameColor": 20}},
        {"command": "monogram.add", "params": {"letters": "ABC", "style": "script", "size": 16, "x": 110, "y": 40,
                                               "frame": "laurel", "fill": 5, "frameColor": 37}},
        # Diamond and Circle letters are warped onto the silhouette, keeping every stroke (#458).
        {"command": "monogram.add", "params": {"letters": "OSK", "style": "diamond", "size": 16, "x": 40, "y": 110,
                                               "frame": "diamond", "fill": 7, "frameColor": 20}},
        {"command": "monogram.add", "params": {"letters": "DBK", "style": "circle", "size": 16, "x": 110, "y": 110,
                                               "frame": "stars", "fill": 21, "frameColor": 12}},
    ]


def wizard(kind):
    """A heart (or two) through one of the wizards; NV880E has no cutwork needles, so cutting lines are sewn
    for scissors."""
    heart = {"command": "shape.preset", "params": {"kind": "heart", "cx": 50, "cy": 50, "width": 50, "fill": 5}}
    if kind == "applique":
        # Replace: the heart becomes the appliqué; "add" kept its full fill, sewn first under the
        # fabric (#450).
        return [heart, {"command": "wizard.applique", "params": {"ids": [1], "material": "running",
                                                                 "output": "replace"}}]
    if kind == "patch":
        return [dict(heart, params=dict(heart["params"], width=40)),
                {"command": "wizard.patch", "params": {"ids": [1], "cutting": "running", "color": 21}}]
    if kind == "cutwork":
        return [{"command": "shape.ellipse", "params": {"cx": 50, "cy": 50, "rx": 25, "fill": 2}},
                {"command": "wizard.cutwork", "params": {"ids": [1], "cutting": "running"}}]
    return [dict(heart, params=dict(heart["params"], width=36)),
            {"command": "shape.ellipse", "params": {"cx": 50, "cy": 50, "rx": 44, "outline": 20}},
            {"command": "wizard.backgroundFill", "params": {"ids": [1], "area": 2, "fillType": "stippling",
                                                             "spacing": 3, "color": 32}}]


FEATURES = [
    ("fills/region-sew-types-1.pes", "Region sew types (1 of 2)",
     "Nine region sew types, left to right, top to bottom: fill, satin, programmable fill (diamonds), motif "
     "fill (heart), piping, cross stitch, concentric circle, radial, spiral.", region_grid(REGION_TYPES_A)),
    ("fills/region-sew-types-2.pes", "Region sew types (2 of 2)",
     "Flexible spiral, stippling, net fill (square), zigzag net fill (honeycomb), decorative fill (waves), "
     "contour fill as rings and as a double spiral (beyond PE-DESIGN), programmable fill (bricks) with base "
     "sewing, motif fill (leaf).", region_grid(REGION_TYPES_B)),
    ("fills/fill-options.pes", "Fill and satin options",
     "Fill at 45°, continuous gradation to 20 %, gradation pattern, feathered edges, Stitch Type 2 (sloped "
     "rows), OpenStitch's random stagger, frequency 0 (no stagger), Auto direction on a circle, and satin with "
     "pull compensation and split stitches.", region_grid(FILL_OPTIONS)),
    ("fills/tatami-textures-1.pes", "Tatami textures (1 of 2)",
     "OpenStitch's named needle-hole textures on a plain fill at 0°, beyond PE-DESIGN except the first: smooth "
     "(PE-DESIGN's Frequency brick), ribs, brick, diagonal, steep diagonal, steps, stairs, blocks, scattered.",
     region_grid(TEXTURES_A)),
    ("fills/tatami-textures-2.pes", "Tatami textures (2 of 2)",
     "Curved needle-hole textures, beyond PE-DESIGN: waves, ripples, zigzag, chevron, scallops.",
     region_grid(TEXTURES_B)),
    ("fills/underlay-styles.pes", "Underlay styles",
     "No underlay, edge, zigzag, edge + zigzag, double (crossing layers), PE-DESIGN's first type (edge and "
     "rows), its second (edge and lattice), centre piping under a satin, and light underlay density.",
     region_grid(UNDERLAYS)),
    ("fills/holes-and-emboss.pes", "Hole sewing, engraving and embossing",
     "Top left: hole sewing (the blue ring is sewn around the yellow centre, not under it). Then a star as an "
     "engraved line, an engraved region and an embossed region in a red fill.", holes_and_emboss()),
    ("fills/wave-fill.pes", "Wave fill (beyond PE-DESIGN)",
     "Fills whose rows ripple as a sine, height and length in mm: 2 and 16, 3 and 10, 5 and 24 (top row, "
     "rows across), 3 and 14 at 45°, 3 and 12 on a circle, and 8 and 8 at 90° (steeper than 40°, so sewn at "
     "40°).", region_grid(WAVES)),
    ("fills/maze-fill.pes", "Maze fill (beyond PE-DESIGN)",
     "One path of straight corridors and square turns that never crosses itself, 2 mm apart: straightness "
     "70 % (default), 0 % and 95 % (top row), on a circle, another seed at 45°, and 3 mm spacing.",
     region_grid(MAZES)),
    ("lines/line-sew-types.pes", "Line sew types",
     "Top to bottom: running, triple, zigzag, motif (wave), stem, candlewick, E stitch, V stitch, chain.",
     line_rows(LINE_TYPES)),
    ("shapes/shape-tools.pes", "Shape tools",
     "Preset star, heart, crescent, arrow, teardrop and hexagon; then a skeleton satin of changing width, a "
     "curved Manual Punch column, and a floral pattern inside a circle.", shapes()),
    ("shapes/decorative-frames.pes", "Decorative frames",
     "Five Shape-tool frames (scallop, wave, zigzag, scallop oval, corners) around small circles, and the "
     "laurel frame from the decorative library.", frames()),
    ("lettering/fonts-and-text.pes", "Lettering",
     "Lato Bold and Pacifico, a Text Transform arch, and text spaced around a circle (text on a path).",
     lettering()),
    ("lettering/monograms.pes", "Monograms",
     "The four monogram styles (block, script, diamond, circle), each in a decorative frame, for the 150 × 150 mm hoop.", monograms()),
    ("wizards/applique.pes", "Appliqué wizard",
     "A heart as appliqué: material line, stop, position line, stop, tack-down, stop, covering zigzag. Lay "
     "the fabric at the first stop, trim it at the second.", wizard("applique")),
    ("wizards/patch.pes", "Embroidered patch wizard",
     "A heart with a satin border and a sewn cutting line outside it, to cut the patch out with scissors.",
     wizard("patch")),
    ("wizards/cutwork.pes", "Cutwork wizard",
     "A circle cutout: stabilising run, a sewn cut line (the NV880E has no cutwork needles, so cut by hand at "
     "the stop), stabiliser run, net fill and covering satin. Use water-soluble stabiliser.", wizard("cutwork")),
    ("wizards/background-fill.pes", "Background fill wizard",
     "Stippling inside a circle around a heart, keeping a 3 mm gap.", wizard("background")),
]


def kit():
    """Every design: file, hoop, title, description, steps (None for a machine check), source image."""
    designs = [
        ("calibration/t1-swatches.pes", "S", "T1 calibration card: thread swatches and coverage ramp",
         "Six threads (White, Black, Red, Blue, Yellow, Leaf Green) as 12 mm fills at 3.0, 4.5 and 6.0 "
         "rows/mm, then Black at 0.5, 1, 1.5, 2, 3 and 4.5 rows/mm, all without underlay; the \"T1\" "
         "mark is below the bottom-left swatch. Photographed under the protocol below, it calibrates the Realistic "
         "view and the similarity scorer (image-similarity plan, T1), shows how each density covers "
         "the fabric (#129), and whether a single layer without underlay sews cleanly. The card's photo "
         "patch is the Photo Stitch 1 sunset demo in OpenStitch mode (`image-tools/S-photo1-sunset-openstitch.pes`).",
         swatches(), None),
        ("calibration/t2-alternating-rows.pes", "S", "T2 calibration card: two threads in alternating rows",
         "How much a row sewn later covers its neighbour of another thread, which Photo Stitch 1's fine "
         "rows need (`rows::Rows::strip`, #311). Black 900 and Yellow 205 as 12 mm fills at 0° without "
         "underlay, one row of swatches per density: 2, 3, 4 and 5 rows/mm of each thread, top to "
         "bottom. Left to right: solid Black at the two threads' total (4, 6, 8, 10 rows/mm), Black "
         "rows with Yellow rows midway between them and sewn later, the same with Yellow sewn first, "
         "and solid Yellow at the total. The thread sewn later lies on top, so the two middle "
         "columns differ only in sewing order. The \"T2\" mark is below the bottom-left swatch. "
         "Sew it on the same fabric and thread as the T1 card and scan it the same way.",
         t2_card(), None),
        ("calibration/ps2-grey-ramp.pes", "S", "Photo Stitch 2 coverage ramp",
         "An eight-step grey ramp (`grey-ramp.png`, white to black) as Photo Stitch 2 sews it in one "
         "Black thread: how much of each grey its line screen covers, the data the Photo Stitch 2 "
         "coverage model needs (#129).",
         # Mono has one method, so no compatMode.
         [{"command": "digitize.photoStitch", "params": {"path": "grey-ramp.png", "width": 90,
                                                         "style": "lines", "mode": "mono"}}],
         "grey-ramp.png"),
        ("calibration/satin-and-underlay.pes", "S", "Satin widths and underlay",
         "Black satin columns 1, 1.5, 2, 2.5, 3, 4 and 6 mm wide and 40 mm long (OpenStitch mode keeps "
         "satin for strokes up to 2.5 mm), then two Red fills at 4.5 rows/mm, without underlay (left) "
         "and with it (right): whether narrow satins sew whole and whether the single layer holds.",
         satin_ladder(), None),
    ]
    for spec in ["satin-zigzag-rect", "machine-formats", "shape-start-points", "pes-default-v6"]:
        designs.append((f"machine-checks/{spec}.pes", "S", spec, None, None, None))
    designs += [(rel, "M" if rel == "lettering/monograms.pes" else "S", title, about, steps, None)
                for rel, title, about, steps in FEATURES]
    for hoop, name, tool, command, image, extra in [
        ("S", "autopunch-balloon", "Auto Punch", "digitize.autoPunch", "g02-balloon.png", {}),
        ("S", "cross-stitch-strawberry", "Cross Stitch", "digitize.crossStitch", "g07-pixel-strawberry.png", {}),
        ("S", "design-center-butterfly", "Design Center", "digitize.designCenter", "g04-butterfly-outline.png", {}),
        ("S", "photo1-sunset", "Photo Stitch 1", "digitize.photoStitch", "p06-sunset.jpg", {"style": "fill"}),
        ("M", "photo1-portrait", "Photo Stitch 1", "digitize.photoStitch", "p01-portrait.jpg", {"style": "fill"}),
        ("M", "photo2-poppies", "Photo Stitch 2", "digitize.photoStitch", "p08-poppies.jpg", {"style": "lines"}),
        ("L", "autopunch-storybook", "Auto Punch", "digitize.autoPunch", "p03-storybook.jpg", {}),
    ]:
        for mode, tag, label in (("openStitch", "openstitch", "OpenStitch mode"),
                                 ("peDesign", "pe-compatible", "PE-DESIGN-compatible mode")):
            designs.append((f"image-tools/{hoop}-{name}-{tag}.pes", hoop, f"{tool}, {label}",
                            f"`{image}` through {tool} at its defaults in {label}, fitted to the "
                            f"{HOOPS[hoop]['label']} hoop. Sew both modes of the same image to compare them "
                            "on fabric.",
                            digitize(command, image, hoop, mode, **extra), image))
    return designs


def spec_of(spec_id):
    for path in (ROOT / "compat" / "specs").rglob("*.json"):
        d = json.loads(path.read_text(encoding="utf-8"))
        if d.get("id") == spec_id:
            return d
    sys.exit(f"no compat spec {spec_id}")


def pec_names():
    """Brother catalogue code and name by PEC chart index ('900 Black' for 20)."""
    rgb, codes = chart_codes()
    return {i: codes[i] for i in rgb if i in codes}


def thread_names():
    """Brother catalogue code and name for each PEC chart colour, from openstitch-core."""
    rgb, codes = chart_codes()
    return {hexc: codes[i] for i, hexc in rgb.items() if i in codes}


def chart_codes():
    """({PEC index: '#rrggbb'}, {PEC index: 'code Name'}) from openstitch-core."""
    charts = (ROOT / "crates/openstitch-core/src/charts.rs").read_text(encoding="utf-8")
    pec = charts[charts.index("PEC_CHART"):]
    pec = pec[:pec.index("];")]
    rgb = {int(i): f"#{int(r):02x}{int(g):02x}{int(b):02x}" for r, g, b, i in re.findall(
        r'Thread::chart\(\s*(\d+),\s*(\d+),\s*(\d+),\s*"[^"]+",\s*"(\d+)"', pec)}
    brother = (ROOT / "crates/openstitch-core/src/brother.rs").read_text(encoding="utf-8")
    brother = brother[brother.index("BROTHER_EMBROIDERY:"):]
    codes = {int(i): f"{code} {name.title()}" for i, code, name in
             re.findall(r'\((\d+), "(\d+)", "([^"]+)"\)', brother[:brother.index("];")])}
    # 62-64 are PE-DESIGN's Applique Wizard colours, not catalogue threads: named for their step.
    codes.update({62: "Appliqué material (PEC 62)", 63: "Appliqué position (PEC 63)", 64: "Appliqué (PEC 64)"})
    return rgb, codes


def exec_script(work, script):
    (work / "kit.json").write_text(json.dumps(script), encoding="utf-8")
    done = subprocess.run([str(BIN / "stitchtool"), "exec", "kit.json"], cwd=work, capture_output=True, text=True)
    replies = [json.loads(line) for line in done.stdout.splitlines() if line.strip()]
    bad = [r for r in replies if not r.get("ok")]
    if done.returncode or bad:
        sys.exit(f"{bad[:1] or done.stderr}")
    return replies


def machine_check(work, spec_id):
    """Runs the spec as `stitchtool compat` does and returns its sew-out file's bytes."""
    spec = spec_of(spec_id)
    name = re.search(rf"target/compat/work/{re.escape(spec_id)}/(\S+?\.pes)", spec["machine"]["make"]).group(1)
    subprocess.run([str(BIN / "stitchtool"), "compat", "--only", spec_id], cwd=ROOT, capture_output=True,
                   check=True)
    return (ROOT / "target" / "compat" / "work" / spec_id / name).read_bytes(), spec


# A preview shows a thread when at least PREVIEW_PIXELS of its pixels are within PREVIEW_DE
# (CIE76) of the thread's RGB. The Realistic view shades thread, so most of it sits a few ΔE
# away; a 128-colour median-cut palette had pulled whole threads 40-60 ΔE off (#564).
PREVIEW_DE = 20.0
PREVIEW_PIXELS = 20


def lab(rgb):
    """CIE L*a*b* (D65) of an array of sRGB triples (0-255)."""
    import numpy as np
    a = np.asarray(rgb, float) / 255
    a = np.where(a > 0.04045, ((a + 0.055) / 1.055) ** 2.4, a / 12.92)
    xyz = a @ np.array([[0.4124, 0.3576, 0.1805], [0.2126, 0.7152, 0.0722], [0.0193, 0.1192, 0.9505]]).T
    f = xyz / np.array([0.9505, 1.0, 1.089])
    f = np.where(f > 0.008856, np.cbrt(f), 7.787 * f + 16 / 116)
    return np.stack([116 * f[..., 1] - 16, 500 * (f[..., 0] - f[..., 1]), 200 * (f[..., 1] - f[..., 2])], -1)


def missing_threads(im, colours):
    """The thread colours ('#rrggbb') the image does not show (see PREVIEW_DE)."""
    import numpy as np
    pixels = lab(np.asarray(im.convert("RGB"))).reshape(-1, 3)
    out = []
    for hexc in dict.fromkeys(colours):
        c = lab([[int(hexc[i:i + 2], 16) for i in (1, 3, 5)]])[0]
        if int((np.linalg.norm(pixels - c, axis=1) < PREVIEW_DE).sum()) < PREVIEW_PIXELS:
            out.append(hexc)
    return out


def thread_colours(threads):
    """'#rrggbb' of each manifest thread ('405 Blue' or '#rrggbb')."""
    rgb = {name: hexc for hexc, name in thread_names().items()}
    return [t if t.startswith("#") else rgb[t] for t in threads]


def preview(work, pes, dst, colours):
    shot = work / "shot.png"
    subprocess.run([str(BIN / "openstitch"), str(pes), "--show", "realistic", "--no-splash", "--size", "1440x900",
                    "--screenshot", str(shot)], cwd=work, check=True, capture_output=True)
    im = Image.open(shot).convert("RGB")
    im = im.crop(gallery.design_box(im))
    im = im.resize((360, round(im.height * 360 / im.width)), Image.LANCZOS)
    if missing_threads(im, colours):
        sys.exit(f"{dst.name}: the render itself lacks threads {missing_threads(im, colours)}")
    # 256 octree colours keep every thread (mean 1.4 ΔE from the render on decorative-frames);
    # a preview that would still lose one is kept in full colour.
    small = im.quantize(colors=256, method=Image.Quantize.FASTOCTREE, dither=Image.Dither.NONE)
    if missing_threads(small, colours):
        small = im
    retry(lambda: small.save(dst, optimize=True))


def check_previews():
    """Every committed preview shows each of its design's threads (#564)."""
    manifest = json.loads((KIT / "manifest.json").read_text(encoding="utf-8"))
    bad = []
    for rel, m in manifest.items():
        lost = missing_threads(Image.open(KIT / m["preview"]), thread_colours(m["threads"]))
        if lost:
            bad.append(f"{m['preview']}: no pixels of {', '.join(lost)}")
    if bad:
        sys.exit("previews with wrong thread colours (run tools/sew_kit.py):\n" + "\n".join(bad))
    print(f"examples/sew previews show their threads ({len(manifest)} files)")


def build(out, previews):
    """Builds every kit file into `out` (and the previews when asked); returns the manifest."""
    names = thread_names()
    manifest = {}
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        for sub in [prefix for prefix, _, _ in SECTIONS] + ["previews/"]:
            (out / sub).mkdir(parents=True, exist_ok=True)
        grey_ramp_png(work / "grey-ramp.png")
        retry(lambda: shutil.copy(work / "grey-ramp.png", out / "calibration" / "grey-ramp.png"))
        for image in IMAGES.iterdir():
            shutil.copy(image, work / image.name)
        for rel, hoop, title, about, steps, image in kit():
            if steps is None:
                data, spec = machine_check(work, title)
                title = f"Machine check: {spec['title']}"
                make = re.sub(rf"target/compat/work/{re.escape(spec['id'])}/\S+?\.pes", "this file",
                              spec["machine"]["make"])
                about = (make + f" The record goes in `compat/machine/{spec['machine']['record']}` "
                         f"(spec `{spec['id']}`).")
                retry(lambda: (work / "out.pes").write_bytes(data))
                (work / "out.osd").unlink(missing_ok=True)
                replies = exec_script(work, [{"command": "stitches.import", "params": {"path": "out.pes"}},
                                             {"command": "design.info"}])
                info = replies[-1]["result"]
            else:
                replies = exec_script(work, HOOPS[hoop]["page"] + steps + [
                    {"command": "design.info"}, {"command": "design.export", "params": {"path": "out.pes"}},
                    {"command": "project.save", "params": {"path": "out.osd"}}, {"command": "object.list"}])
                objects = replies.pop()["result"]
                objects = objects if isinstance(objects, list) else objects.get("objects", [])
                check_objects(rel, objects)
                replies.pop()
                export = replies[-1]["result"]
                if not export["fitsPage"] or export["warnings"]:
                    sys.exit(f"{rel}: does not fit the {HOOPS[hoop]['label']} hoop: {export['warnings']}")
                info = replies[-2]["result"]
                data = (work / "out.pes").read_bytes()
            retry(lambda: (out / rel).write_bytes(data))
            if rel in APART:
                check_apart(rel, out / rel)
            measured = measure_card(rel, out / rel) if rel in CARDS else None
            stats = info["stitchStats"]
            png = f"previews/{Path(rel).stem}.png"
            if previews:
                # The design as built, in its own page orientation (a .pes for a 260 × 160 hoop opens
                # turned to the machine's 160 × 260).
                preview(work, work / "out.osd" if steps is not None else out / rel, out / png, stats["colors"])
            manifest[rel] = {
                "title": title,
                "about": about,
                "hoop": HOOPS[hoop]["label"],
                "sizeMM": [stats["widthMM"], stats["heightMM"]],
                "stitches": stats["stitches"],
                "trims": stats["trims"],
                "threads": [names.get(c, c) for c in stats["colors"]],
                "sewingMinutes": info["estimates"]["sewingMinutes"],
                "image": (f"calibration/{image}" if image == "grey-ramp.png" else
                          f"../images/{image}" if image else None),
                "preview": png,
                "sha256": hashlib.sha256(data).hexdigest(),
            }
            if measured:
                pec = pec_names()
                manifest[rel]["swatches"] = [dict(m, thread=pec[m["thread"]]) for m in measured]
    return manifest


SECTIONS = [
    ("calibration/", "Calibration", "For calibrating OpenStitch against real thread and fabric. Sew these first "
     "and send the photos (see *Sending results* above)."),
    ("machine-checks/", "Machine checks", "The compatibility specs that need a sew-out to reach the machine tier "
     "(`compat/machine`). Each file is byte for byte what `stitchtool compat` exports, so a record's "
     "`fileSha256` matches it."),
    ("fills/", "Fills and region options", "Every region sew type and the fill, satin and underlay options."),
    ("lines/", "Lines", "Every line sew type."),
    ("shapes/", "Shapes and frames", "The shape tools and decorative frames."),
    ("lettering/", "Lettering and monograms", "The fonts, Text Transform, text on a path and monograms."),
    ("wizards/", "Wizards", "Appliqué, embroidered patch, cutwork and background fill."),
    ("image-tools/", "Image to embroidery", "Each image tool on a test image, in both of its modes: "
     "PE-DESIGN-compatible mode (OpenStitch's reimplementation of PE-DESIGN 11's method) and OpenStitch mode. "
     "The source image is on the left."),
]


def describe(manifest):
    out = []
    for prefix, heading, intro in SECTIONS:
        out.append(f"### {heading}\n")
        out.append(intro + "\n")
        for rel, m in manifest.items():
            if not rel.startswith(prefix):
                continue
            out.append(f"#### {m['title']}\n")
            out.append(f"[`{rel}`]({rel}) · {m['hoop']} hoop · {m['sizeMM'][0]:g} × {m['sizeMM'][1]:g} mm · "
                       f"{m['stitches']:,} stitches · {m['trims']} trim{'' if m['trims'] == 1 else 's'} · about {m['sewingMinutes']:g} min\n")
            out.append(m["about"] + "\n")
            pics = ([f'<img src="{m["image"]}" height="180" alt="Source image">'] if m["image"] else []) + [
                f'<img src="{m["preview"]}" height="180" alt="Realistic view of {rel}">']
            out.append(" ".join(pics) + "\n")
            out.append("Threads, in sewing order: " + ", ".join(m["threads"]) + ".\n")
    return "\n".join(out)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="compare a fresh build with the committed .pes files")
    ap.add_argument("--check-previews", action="store_true",
                    help="check that every committed preview shows its threads' colours (no build needed)")
    a = ap.parse_args()
    if a.check_previews:
        return check_previews()
    if a.check:
        with tempfile.TemporaryDirectory() as tmp:
            manifest = build(Path(tmp), previews=False)
        old = json.loads((KIT / "manifest.json").read_text(encoding="utf-8"))
        stale = sorted(f for f in set(manifest) | set(old)
                       if old.get(f, {}).get("sha256") != manifest.get(f, {}).get("sha256"))
        if stale:
            sys.exit("examples/sew is stale; run tools/sew_kit.py: " + ", ".join(stale))
        print(f"examples/sew is current ({len(manifest)} files)")
        return
    manifest = build(KIT, previews=True)
    (KIT / "manifest.json").write_text(json.dumps(manifest, indent=1, ensure_ascii=False) + "\n",
                                       encoding="utf-8", newline="\n")
    readme = KIT / "README.md"
    text, n = re.subn(r"(<!-- kit:start -->\n).*?(<!-- kit:end -->)",
                      lambda m: m.group(1) + describe(manifest) + "\n" + m.group(2),
                      readme.read_text(encoding="utf-8"), flags=re.S)
    if n != 1:
        sys.exit("examples/sew/README.md has no kit:start / kit:end markers")
    readme.write_text(text, encoding="utf-8", newline="\n")
    for rel, m in manifest.items():
        print(f"{rel}: {m['stitches']} stitches, {len(m['threads'])} threads, {m['sizeMM'][0]:g} × {m['sizeMM'][1]:g} mm")


if __name__ == "__main__":
    sys.exit(main())
