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


def commands(stitches):
    """The stream with pyembroidery's needle sets as OpenStitch reads them: a needle set after
    something is sewn is a colour change; one before (the needle the design starts on) is
    left out."""
    out, sewn = [], False
    for x, y, c in stitches:
        c &= pe.COMMAND_MASK
        if c == pe.NEEDLE_SET:
            if sewn:
                out.append([x, y, "colorChange"])
            continue
        sewn |= c == pe.STITCH
        out.append([x, y, CMD.get(c, str(c))])
    return out


def dump_read(name):
    r = pe.read(os.path.join(out, name))
    ext = name.split(".")[1]
    stem = name.split(".")[0]
    json.dump({
        "threads": [{"hex": t.hex_color(), "name": t.description, "catalog": t.catalog_number,
                     "brand": t.brand} for t in r.threadlist],
        "stitches": commands(r.stitches),
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
# #481: pyembroidery writes U01 and TBF, and TAP as its DST without the 512-byte header (it has
# no TAP writer). It reads DSB, DSZ and KSM but cannot write them:
# those three are assembled below from the layouts in crates/openstitch-formats/src/{dsb,dsz,
# ksm}.rs, and pyembroidery's read of them is the oracle.
for ext in ("u01", "tbf"):
    pe.write(p, os.path.join(out, "oracle." + ext))
    dump_read("oracle." + ext)
_dst = os.path.join(out, "survey.dst.tmp")
pe.write_dst(p, _dst)
open(os.path.join(out, "oracle.tap"), "wb").write(open(_dst, "rb").read()[512:])
os.remove(_dst)
dump_read("oracle.tap")


def records(seq, encode, max_step=127):
    """Relative records for the survey design: (kind, dx, dy) with +y down, moves over
    max_step split into jumps, a needle record first and at each colour change."""
    out = [("needle", 0, 0)]
    xx = yy = 0
    for x, y, cmd in seq:
        if cmd == "end":
            break
        if cmd in ("trim", "colorChange"):
            out.append(("trim" if cmd == "trim" else "needle", 0, 0))
            continue
        dx, dy = x - xx, y - yy
        while abs(dx) > max_step or abs(dy) > max_step:
            sx, sy = max(-max_step, min(max_step, dx)), max(-max_step, min(max_step, dy))
            out.append(("jump", sx, sy))
            xx += sx; yy += sy; dx -= sx; dy -= sy
        out.append((cmd, dx, dy))
        xx, yy = x, y
    return b"".join(encode(*r) for r in out)


def dsb(kind, dx, dy):
    if kind in ("stitch", "jump"):
        ctrl = (0x80 if kind == "stitch" else 0x81) | (0x40 if dy >= 0 else 0) | (0x20 if dx < 0 else 0)
        return bytes([ctrl, abs(dy), abs(dx)])
    return bytes([{"trim": 0xE7, "needle": 0xE9}[kind], 0, 0])


def dsz(kind, dx, dy):
    if kind in ("stitch", "jump"):
        ctrl = (0x80 if kind == "stitch" else 0x81) | (0x20 if dy >= 0 else 0) | (0x40 if dx < 0 else 0)
        return bytes([abs(dy), abs(dx), ctrl])
    return bytes([0, 0, {"trim": 0x9B, "needle": 0x83}[kind]])


def ksm(kind, dx, dy):
    # A jump after a trim or needle change is a plain move in "moving" state; a stitch ends it
    # with 0x17 when it follows a jump. Simplest faithful form: every jump 0x00 while moving,
    # and the first stitch after a move written as 0x17 (a jump that ends the move) plus the
    # stitch itself; here stitches after a move land on the jump's point, so 0x17 zero moves.
    sign = (0x20 if dy >= 0 else 0) | (0x40 if dx < 0 else 0)
    if kind == "stitch":
        return bytes([0, 0, 0x17]) + bytes([abs(dy), abs(dx), sign]) if ksm.moving else bytes([abs(dy), abs(dx), sign])
    if kind == "jump":
        return bytes([abs(dy), abs(dx), sign])
    return bytes([0, 0, {"trim": 0x07, "needle": 0x0B}[kind]])


def ksm_records(seq):
    out = bytearray()
    for r in records_list(seq):
        kind = r[0]
        out += ksm(*r)
        if kind in ("trim", "needle"):
            ksm.moving = True
        elif kind == "stitch":
            ksm.moving = False
    return bytes(out)


def records_list(seq):
    got = []
    records(seq, lambda *r: got.append(r) or b"")
    return got


ksm.moving = False
header = b"LA:survey".ljust(0x1FF, b" ") + b"\x1a"
open(os.path.join(out, "oracle.dsb"), "wb").write(header + records(seq, dsb) + bytes([0xF8, 0, 0]))
open(os.path.join(out, "oracle.dsz"), "wb").write(header + records(seq, dsz) + bytes([0, 0, 0xFF]))
open(os.path.join(out, "oracle.ksm"), "wb").write(bytes(0x200) + ksm_records(seq) + bytes([0, 0, 0x1F]))
for ext in ("dsb", "dsz", "ksm"):
    dump_read("oracle." + ext)

# #482: thread lists. pyembroidery writes and reads INF, EDR and COL (its TXT is a stitch list,
# not a thread list, so OpenStitch's TXT list has no oracle).
for ext in ("inf", "edr", "col"):
    pe.write(p, os.path.join(out, "oracle." + ext))
    dump_read("oracle." + ext)

for ext in ("vp3", "tap", "u01", "tbf", "inf", "edr", "col"):
    if os.path.exists(os.path.join(out, "openstitch." + ext)):
        dump_read("openstitch." + ext)
print("ok")
