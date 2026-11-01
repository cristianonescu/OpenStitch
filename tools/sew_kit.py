#!/usr/bin/env python3
"""Regenerates the ready-to-sew kit in examples/sew: calibration designs, machine checks and
demo designs for a single-needle Brother machine (written for the NV880E and its 100 × 100,
150 × 150 and 260 × 160 mm hoops), all made by OpenStitch itself.

    cargo build --release -p stitchtool -p openstitch-app
    python3 -I tools/sew_kit.py            # rewrite the .pes files, previews, manifest, README
    python3 -I tools/sew_kit.py --check    # fail if a committed .pes differs from a fresh build

Each design is a `stitchtool exec` script (the KIT list below), except the machine checks, which
are the compatibility specs with a sew-out record (`compat/machine`), exported by
`stitchtool compat --only <spec>` so a record's `fileSha256` matches the kit's file. Each design
gets a preview (the editor's Realistic view, `openstitch --screenshot`, cropped as
`tools/gallery.py` crops), and the kit's README is rewritten between the `kit:start` and
`kit:end` markers: per design, a description, the source image and the preview, the hoop, size,
stitches, trims and threads (Brother catalogue code and name where the colour is on the PEC
chart). manifest.json records each file's SHA-256. OpenStitch's output is the same on macOS,
Windows and Linux, so a fresh build matches the committed files byte for byte until the stitch
engine changes. Needs Pillow.
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
    go in thread order (Black's column, ramp and mark together), so each thread is sewn once."""
    cells = []
    for col, thread in enumerate(CARD_THREADS):
        cells += [(col, row, thread, d) for row, d in enumerate(CARD_DENSITIES)]
        if thread == 20:
            cells += [(c, 3, 20, d) for c, d in enumerate(RAMP_DENSITIES)]
    steps = []
    for n, (col, row, thread, d) in enumerate(cells, 1):
        steps += [
            {"command": "shape.rectangle", "params": {"x": 9 + col * 14, "y": 9 + row * 15, "width": 12,
                                                      "height": 12, "fill": thread}},
            {"command": "object.setRegion", "params": {"ids": [n], "type": "fill", "density": d,
                                                       "underlay": False, "angle": 0}},
        ]
        if (thread, row, col) == (20, 3, len(RAMP_DENSITIES) - 1):
            steps.append({"command": "text.add", "params": {"text": "T1", "size": 6, "x": 9, "y": 77,
                                                            "fill": 20}})
    return steps


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
    ("rect", {"type": "satin", "angle": 90}),
    ("rect", {"type": "programmableFill", "pattern": "diamonds"}),
    ("rect", {"type": "motifFill", "pattern": "heart"}),
    ("rect", {"type": "piping"}),
    ("rect", {"type": "crossStitch", "spacing": 2.5}),
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
UNDERLAYS = [
    ("rect", {"type": "fill", "underlay": False}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "edge"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "zigzag"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "edgeZigzag"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "double"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "edgeFill"}),
    ("rect", {"type": "fill", "underlay": True, "underlayStyle": "cross"}),
    ("rect", {"type": "satin", "angle": 90, "underlay": True, "underlayParts": ["center"]}),
    ("rect", {"type": "fill", "underlay": True, "underlayDensity": "light"}),
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
        {"command": "shape.column", "params": {"lines": [[[40, 70], [40, 90]], [[50, 68], [52, 92]],
                                                         [[60, 70], [62, 90]]], "curved": True, "fill": 32}},
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
    """The decorative frames: five Shape-tool frames and an imported one."""
    steps = []
    for i, ((x, y), kind) in enumerate(zip(cells(5, size=20, gap=12, x0=12, y0=12),
                                           ["scallop", "wave", "zigzag", "scallopOval", "corners"]), 1):
        steps += [
            {"command": "shape.ellipse", "params": {"cx": x + 10, "cy": y + 10, "rx": 4, "fill": THREADS[i]}},
            {"command": "shape.decorative", "params": {"kind": kind, "ids": [len([s for s in steps if s["command"]
                                                                                 .startswith("shape.")]) + 1],
                                                       "margin": 2, "size": 2, "color": 20}},
        ]
    steps.append({"command": "decoration.add", "params": {"frame": "laurel", "x": 68, "y": 46, "width": 18,
                                                          "height": 18, "color": 37}})
    return steps


def lettering():
    """Both fonts, a Text Transform arch, text on a circle, and a script line."""
    return [
        {"command": "text.add", "params": {"text": "OpenStitch", "size": 10, "x": 50, "y": 18, "align": "center",
                                           "fill": 2}},
        {"command": "text.add", "params": {"text": "Pacifico", "font": "pacifico", "size": 10, "x": 50, "y": 34,
                                           "align": "center", "fill": 5}},
        {"command": "text.add", "params": {"text": "ARCH", "size": 9, "x": 50, "y": 52, "align": "center",
                                           "envelope": 1, "fill": 37}},
        {"command": "shape.ellipse", "params": {"cx": 50, "cy": 76, "rx": 16, "outline": 40}},
        {"command": "text.add", "params": {"text": "ROUND AND ROUND", "size": 5, "fill": 7}},
        {"command": "text.setPath", "params": {"id": 5, "path": 4, "align": "spaced"}},
    ]


def monograms():
    return [
        {"command": "monogram.add", "params": {"letters": "OS", "style": "block", "size": 16, "x": 40, "y": 40,
                                               "frame": "ring", "fill": 2, "frameColor": 20}},
        {"command": "monogram.add", "params": {"letters": "ABC", "style": "script", "size": 16, "x": 110, "y": 40,
                                               "frame": "laurel", "fill": 5, "frameColor": 37}},
        {"command": "monogram.add", "params": {"letters": "OSK", "style": "diamond", "size": 16, "x": 40, "y": 110,
                                               "frame": "diamond", "fill": 7, "frameColor": 20}},
        {"command": "monogram.add", "params": {"letters": "XYZ", "style": "circle", "size": 16, "x": 110, "y": 110,
                                               "frame": "stars", "fill": 21, "frameColor": 12}},
    ]


def wizard(kind):
    """A heart (or two) through one of the wizards; NV880E has no cutwork needles, so cutting lines are sewn
    for scissors."""
    heart = {"command": "shape.preset", "params": {"kind": "heart", "cx": 50, "cy": 50, "width": 50, "fill": 5}}
    if kind == "applique":
        return [heart, {"command": "wizard.applique", "params": {"ids": [1], "material": "running"}}]
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
    ("fills/underlay-styles.pes", "Underlay styles",
     "No underlay, edge, zigzag, edge + zigzag, double (crossing layers), PE-DESIGN's first type (edge and "
     "rows), its second (edge and lattice), centre piping under a satin, and light underlay density.",
     region_grid(UNDERLAYS)),
    ("fills/holes-and-emboss.pes", "Hole sewing, engraving and embossing",
     "Top left: hole sewing (the blue ring is sewn around the yellow centre, not under it). Then a star as an "
     "engraved line, an engraved region and an embossed region in a red fill.", holes_and_emboss()),
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
         "mark is the top left. Photographed under the protocol below, it calibrates the Realistic "
         "view and the similarity scorer (image-similarity plan, T1), shows how each density covers "
         "the fabric (#129), and whether a single layer without underlay sews cleanly. The card's photo "
         "patch is the Photo Stitch 1 sunset demo in OpenStitch mode (`image-tools/S-photo1-sunset-openstitch.pes`).",
         swatches(), None),
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


def thread_names():
    """Brother catalogue code and name for each PEC chart colour, from openstitch-core."""
    charts = (ROOT / "crates/openstitch-core/src/charts.rs").read_text(encoding="utf-8")
    pec = charts[charts.index("PEC_CHART"):]
    pec = pec[:pec.index("];")]
    rgb = {int(i): f"#{int(r):02x}{int(g):02x}{int(b):02x}" for r, g, b, i in re.findall(
        r'Thread::chart\(\s*(\d+),\s*(\d+),\s*(\d+),\s*"[^"]+",\s*"(\d+)"', pec)}
    brother = (ROOT / "crates/openstitch-core/src/brother.rs").read_text(encoding="utf-8")
    brother = brother[brother.index("BROTHER_EMBROIDERY:"):]
    codes = {int(i): f"{code} {name.title()}" for i, code, name in
             re.findall(r'\((\d+), "(\d+)", "([^"]+)"\)', brother[:brother.index("];")])}
    return {hexc: codes[i] for i, hexc in rgb.items() if i in codes}


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


def preview(work, pes, dst):
    shot = work / "shot.png"
    subprocess.run([str(BIN / "openstitch"), str(pes), "--show", "realistic", "--no-splash", "--size", "1440x900",
                    "--screenshot", str(shot)], cwd=work, check=True, capture_output=True)
    im = Image.open(shot).convert("RGB")
    im = im.crop(gallery.design_box(im))
    im = im.resize((360, round(im.height * 360 / im.width)), Image.LANCZOS)
    im.quantize(colors=128, method=Image.Quantize.MEDIANCUT, dither=Image.Dither.NONE).save(dst, optimize=True)


def build(out, previews):
    """Builds every kit file into `out` (and the previews when asked); returns the manifest."""
    names = thread_names()
    manifest = {}
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        for sub in [prefix for prefix, _, _ in SECTIONS] + ["previews/"]:
            (out / sub).mkdir(parents=True, exist_ok=True)
        grey_ramp_png(work / "grey-ramp.png")
        shutil.copy(work / "grey-ramp.png", out / "calibration" / "grey-ramp.png")
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
                (work / "out.pes").write_bytes(data)
                (work / "out.osd").unlink(missing_ok=True)
                replies = exec_script(work, [{"command": "stitches.import", "params": {"path": "out.pes"}},
                                             {"command": "design.info"}])
                info = replies[-1]["result"]
            else:
                replies = exec_script(work, HOOPS[hoop]["page"] + steps + [
                    {"command": "design.info"}, {"command": "design.export", "params": {"path": "out.pes"}},
                    {"command": "project.save", "params": {"path": "out.osd"}}])
                replies.pop()
                export = replies[-1]["result"]
                if not export["fitsPage"] or export["warnings"]:
                    sys.exit(f"{rel}: does not fit the {HOOPS[hoop]['label']} hoop: {export['warnings']}")
                info = replies[-2]["result"]
                data = (work / "out.pes").read_bytes()
            (out / rel).write_bytes(data)
            stats = info["stitchStats"]
            png = f"previews/{Path(rel).stem}.png"
            if previews:
                # The design as built, in its own page orientation (a .pes for a 260 × 160 hoop opens
                # turned to the machine's 160 × 260).
                preview(work, work / "out.osd" if steps is not None else out / rel, out / png)
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
    a = ap.parse_args()
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
