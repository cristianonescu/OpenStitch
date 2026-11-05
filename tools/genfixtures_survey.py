"""Oracle fixtures for the formats added by the survey batch (#480 VP3, #481, #482, #488).

pyembroidery 1.5.1 (an independent implementation) writes the oracle files from one test design
and reads back the files OpenStitch writes; the tests in
crates/openstitch-formats/tests/survey_formats.rs compare both ways. Only pyembroidery's public
API is used; none of its code is copied into OpenStitch.

    python3 tools/genfixtures_survey.py crates/openstitch-formats/tests/fixtures

Run the tests with OPENSTITCH_REGEN=1 first when a writer changed (they rewrite the
openstitch.* files), then this script for the oracle reads of them.
"""
import json
import os
import sys

import pyembroidery as pe

out = sys.argv[1]
CMD = {pe.STITCH: "stitch", pe.JUMP: "jump", pe.TRIM: "trim", pe.COLOR_CHANGE: "colorChange",
       pe.STOP: "stop", pe.END: "end"}

# Twelve colours (the VP3 acceptance criteria ask for at least ten), each with a brand, a
# catalogue number and a name; the brand and numbers are test strings, not a real chart.
THREADS = [
    ("#e01010", "Poppy", "1001"), ("#1040c0", "Ocean", "1002"), ("#20a040", "Leaf", "1003"),
    ("#f0c020", "Sun", "1004"), ("#802080", "Plum", "1005"), ("#000000", "Night", "1006"),
    ("#ffffff", "Snow", "1007"), ("#f08040", "Apricot", "1008"), ("#40c0c0", "Lagoon", "1009"),
    ("#a06030", "Walnut", "1010"), ("#c0c0c0", "Silver", "1011"), ("#ff80c0", "Blossom", "1012"),
]
BRAND = "Test Brand"


def build():
    """Per colour: a short zig-zag, a 20 mm satin-like stitch, then a trimmed 30 mm move."""
    p = pe.EmbPattern()
    for hex_, name, number in THREADS:
        t = pe.EmbThread()
        t.set(hex_)
        t.description, t.catalog_number, t.brand = name, number, BRAND
        p.add_thread(t)
    seq = []

    def add(cmd, x, y):
        p.add_stitch_absolute(cmd, x, y)
        seq.append([x, y, CMD[cmd]])

    x = y = 0
    for i in range(len(THREADS)):
        if i:
            add(pe.COLOR_CHANGE, x, y)
        for k in range(8):
            add(pe.STITCH, x + k * 15, y + (0 if k % 2 == 0 else 40))
        x, y = x + 105, y + 40
        add(pe.STITCH, x + 200, y)  # 20 mm: longer than one short record
        x += 200
        add(pe.TRIM, x, y)
        y += 300 if i % 2 == 0 else -300  # 30 mm trimmed move
        add(pe.JUMP, x, y)
        add(pe.STITCH, x, y)
    add(pe.END, x, y)
    return p, seq


def dump_read(name):
    r = pe.read(os.path.join(out, name))
    ext = name.split(".")[1]
    stem = name.split(".")[0]
    json.dump({
        "threads": [{"hex": t.hex_color(), "name": t.description, "catalog": t.catalog_number,
                     "brand": t.brand} for t in r.threadlist],
        "stitches": [[s[0], s[1], CMD.get(s[2] & pe.COMMAND_MASK, str(s[2]))] for s in r.stitches],
    }, open(os.path.join(out, "%s_%s_read.json" % (stem, ext)), "w"))


p, seq = build()
json.dump({"threads": [{"hex": h, "name": n, "catalog": c, "brand": BRAND} for h, n, c in THREADS],
           "stitches": seq}, open(os.path.join(out, "survey_input.json"), "w"))
pe.write_vp3(p, os.path.join(out, "oracle.vp3"))
dump_read("oracle.vp3")
# A real OpenStitch design as pyembroidery writes it: machine-formats.pes, the design whose
# PE-DESIGN exports are compat/reference/machine (an older copy of the kit file of that name).
kit = pe.read(os.path.join(out, "machine-formats.pes"))
pe.write_vp3(kit, os.path.join(out, "oracle_kit.vp3"))
dump_read("oracle_kit.vp3")
for ext in ("vp3",):
    if os.path.exists(os.path.join(out, "openstitch." + ext)):
        dump_read("openstitch." + ext)
print("ok")
