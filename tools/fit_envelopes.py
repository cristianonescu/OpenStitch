#!/usr/bin/env python3
"""Fit the Text Transform presets from PE-DESIGN 11.50 reference files.

    cargo build -p stitchtool
    python tools/fit_envelopes.py > crates/openstitch-design/src/envelope_presets.rs
    cargo fmt --all
    python -I tools/fit_envelopes.py --check       # exit 1 if the committed table differs (CI)
    python -I tools/fit_envelopes.py --self-test   # check --check's comparison (no stitchtool)

Each reference (`compat/reference/text/text-transform-N.pes`, made by pe-gui) is the text
HHHHHHH in Lato Bold at 10 mm, centred, with preset N applied. The straight text's box is
x 12.7..87.3 mm, y 39.7..50.0 mm (PE's status bar: 74.60 x 10.30 mm). For every preset whose
letters keep their x positions, the stems of the H's give the envelope's top and bottom edge:
the highest and lowest needle point of each stem column. Each edge is a least-squares fit of
those samples (fractions of the straight box) on a constant, a slope and eight cosine terms,
dropping outliers; for curved presets each knot (u = k/32 along the text) is the outermost
of the samples near it, so peaks are kept, and the fit fills the gaps between letters. Slants
and fans are fitted from the letters' corners: a fan keeps one edge of the straight box and
scales the other about the text's centre, each letter sheared linearly between them. The arcs
(1 and 2) are not envelopes of this kind and are left out of the table.

The stitchtool binary: target/debug/stitchtool, or the path in STITCHTOOL.
"""

import json
import os
import re
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOL = Path(os.environ.get("STITCHTOOL") or
            ROOT / "target" / "debug" / ("stitchtool.exe" if os.name == "nt" else "stitchtool")
            ).resolve()  # absolute: stitchtool runs in a temporary folder
REF = ROOT / "compat" / "reference" / "text"
TABLE = ROOT / "crates" / "openstitch-design" / "src" / "envelope_presets.rs"
X0, X1, YT, YB = 12.7, 87.3, 39.7, 50.0
W, H = X1 - X0, YB - YT
KNOTS = 33
# Presets whose letters stay at their x positions; 9 and 10 slant.
COLUMN = [3, 4, 5, 6, 7, 8, 11, 12, 15, 16, 17, 18, 19, 20, 21, 22, 23, 24, 25, 26]
SLANT = [9, 10]
# Fans: 13 shrinks the bottom edge about the centre, 14 the top edge.
FAN = [13, 14]
LETTER = 8.7


def needle_points(path):
    with tempfile.TemporaryDirectory() as d:
        (Path(d) / "a.pes").write_bytes(path.read_bytes())
        script = [
            # keepPosition: the stitches where PE-DESIGN put them, not centred on the page (#886).
            {"command": "stitches.import", "params": {"path": "a.pes", "keepPosition": True}},
            {"command": "stitches.points", "params": {"id": 1}},
        ]
        (Path(d) / "s.json").write_text(json.dumps(script))
        out = subprocess.run([str(TOOL), "exec", "s.json"], cwd=d, capture_output=True,
                             text=True, check=True).stdout.splitlines()
    points = json.loads(out[1])["result"]["points"]
    letters, cur = [], []
    for p in points:
        if p["command"] != "stitch":
            if len(cur) > 20:
                letters.append(cur)
            cur = []
        else:
            cur.append((p["x"], p["y"]))
    if len(cur) > 20:
        letters.append(cur)
    return letters


def stem_samples(letters):
    """(u, top, bottom) per 0.3 mm column of each H stem, as fractions of the box."""
    out = []
    for pts in letters:
        bins = {}
        for x, y in pts:
            bins.setdefault(round(x / 0.3), []).append(y)
        tall = max(max(v) - min(v) for v in bins.values())
        for b, ys in sorted(bins.items()):
            # Stems run the letter's full height; the crossbar is a short band between them.
            if len(ys) >= 5 and max(ys) - min(ys) > 0.8 * tall:
                out.append(((b * 0.3 - X0) / W, (min(ys) - YT) / H, (max(ys) - YT) / H))
    return sorted(out)


LINEAR = {3, 4, 5, 6, 7, 8, 11, 12}
# Mirror pairs (the second is the first read right to left) and left-right symmetric presets:
# their samples are pooled, so both sides get the same fit.
PAIRS = [(3, 4), (5, 6), (7, 8), (11, 12), (19, 20)]
SYMMETRIC = {15, 16, 17, 18, 21, 22, 23, 24, 25, 26}


def basis(u, linear=False):
    """Constant and slope, plus eight cosine terms for bows, peaks, waves and double waves."""
    import math
    if linear:
        return [1.0, u]
    return [1.0, u] + [math.cos(j * math.pi * u) for j in range(1, 9)]


def solve(a, b):
    n = len(b)
    m = [row[:] + [b[i]] for i, row in enumerate(a)]
    for c in range(n):
        p = max(range(c, n), key=lambda r: abs(m[r][c]))
        m[c], m[p] = m[p], m[c]
        for r in range(n):
            if r != c and m[c][c] != 0:
                f = m[r][c] / m[c][c]
                m[r] = [x - f * y for x, y in zip(m[r], m[c])]
    return [m[i][n] / m[i][i] for i in range(n)]


def fit(samples, k, linear=False):
    """Least squares on `basis`, dropping samples 0.03 of the box off the fit, twice."""
    pts = [(u, v[k - 1]) for u, *v in samples]
    m = len(basis(0.0, linear))
    for _ in range(3):
        rows = [basis(u, linear) for u, _ in pts]
        a = [[sum(r[i] * r[j] for r in rows) for j in range(m)] for i in range(m)]
        b = [sum(r[i] * y for r, (_, y) in zip(rows, pts)) for i in range(m)]
        c = solve(a, b)
        f = lambda u, c=c: sum(ci * bi for ci, bi in zip(c, basis(u, linear)))
        kept = [(u, y) for u, y in pts if abs(f(u) - y) < 0.03]
        if len(kept) == len(pts) or len(kept) < 12:
            break
        pts = kept
    return f


def interpolate(samples, u, k):
    if u <= samples[0][0]:
        return samples[0][k]
    if u >= samples[-1][0]:
        return samples[-1][k]
    for a, b in zip(samples, samples[1:]):
        if a[0] <= u <= b[0]:
            t = 0 if b[0] == a[0] else (u - a[0]) / (b[0] - a[0])
            return a[k] + t * (b[k] - a[k])
    return samples[-1][k]


def samples_for(n):
    """The stem samples of preset n, pooled with its mirror twin's or its own mirror image."""
    own = stem_samples(needle_points(REF / f"text-transform-{n}.pes"))
    for a, b in PAIRS:
        if n in (a, b):
            twin = stem_samples(needle_points(REF / f"text-transform-{b if n == a else a}.pes"))
            return sorted(own + [(1 - u, t, bt) for u, t, bt in twin])
    if n in SYMMETRIC:
        return sorted(own + [(1 - u, t, bt) for u, t, bt in own])
    return own


def column_preset(n):
    s = samples_for(n)
    us = [k / (KNOTS - 1) for k in range(KNOTS)]
    ft, fb = fit(s, 1, n in LINEAR), fit(s, 2, n in LINEAR)
    if n in LINEAR:
        return [(u, ft(u)) for u in us], [(u, fb(u)) for u in us]
    return [(u, knot(s, 1, ft, u)) for u in us], [(u, knot(s, 2, fb, u)) for u in us]


def knot(samples, k, smooth, u):
    """The outermost (highest top, lowest bottom) of the samples within 1/64 of u that lie
    near the smooth fit (the crossbars and stem edges do not); the smooth fit where there
    are none."""
    near = [v[k - 1] for su, *v in samples
            if abs(su - u) <= 1 / 64 and abs(v[k - 1] - smooth(su)) < 0.06]
    if not near:
        return smooth(u)
    return min(near) if k == 1 else max(near)


def true_min_x(n):
    """The reference's left edge as the compat runner reads it (mm): `stitches.import` centres
    a file's box on the page, which moves a slant whose box is off the straight text's centre
    (preset 10: 0.15 mm), so the slants are fitted in the reference's own frame (#428)."""
    with tempfile.TemporaryDirectory() as d:
        report = Path(d) / "r.json"
        subprocess.run([str(TOOL), "compat", "--only", f"text/text-transform-{n}",
                        "--work", str(Path(d) / "w"), "--report", str(report),
                        "--summary", str(Path(d) / "s.md")],
                       cwd=ROOT, capture_output=True, text=True, check=False)
        specs = json.loads(report.read_text(encoding="utf-8"))["specs"]
        spec = next(x for x in specs if x["id"] == f"text-transform-{n}")
    return next(c["expected"] for c in spec["checks"] if c["measurement"] == "bounds-min-x")


def slant_preset(n):
    """A parallelogram fitted to the letters' corners (#428): each edge x = a + b u (fractions
    of the straight box, u along the straight text), one slope b for both edges and an offset
    each, least squares over every letter's left and right corner on the top and bottom edge
    (left and right together, so the needle points' reach past the outline cancels). The
    letters are put in the reference's own frame first ([`true_min_x`])."""
    letters = needle_points(REF / f"text-transform-{n}.pes")
    shift = true_min_x(n) - min(x for l in letters for x, _ in l)
    pitch = (W - LETTER) / (len(letters) - 1)
    rows = []  # (u, frac, edge 0 top / 1 bottom)
    for i, l in enumerate(letters):
        ys = [y for _, y in l]
        top, bot = min(ys), max(ys)
        at_top = [x + shift for x, y in l if y < top + 0.5]
        at_bot = [x + shift for x, y in l if y > bot - 0.5]
        u_l, u_r = i * pitch / W, (i * pitch + LETTER) / W
        rows += [(u_l, (min(at_top) - X0) / W, 0), (u_r, (max(at_top) - X0) / W, 0),
                 (u_l, (min(at_bot) - X0) / W, 1), (u_r, (max(at_bot) - X0) / W, 1)]
    # Unknowns: a_top, a_bottom, b.
    basis_row = lambda u, e: [1.0 - e, float(e), u]
    a = [[sum(basis_row(u, e)[r] * basis_row(u, e)[c] for u, _, e in rows) for c in range(3)]
         for r in range(3)]
    b = [sum(basis_row(u, e)[r] * v for u, v, e in rows) for r in range(3)]
    a_top, a_bot, slope = solve(a, b)
    us = [k / (KNOTS - 1) for k in range(KNOTS)]
    top = [(round(a_top + slope * u, 6), 0.0) for u in us]
    bottom = [(round(a_bot + slope * u, 6), 1.0) for u in us]
    return top, bottom


def corner_scales(letters):
    """(top, bottom) scale of the letters' corners about the text's centre, least squares over
    every letter's two top and two bottom corners against the straight text's."""
    pitch = (W - LETTER) / (len(letters) - 1)
    c = X0 + W / 2
    num = {1: 0.0, 2: 0.0}
    den = {1: 0.0, 2: 0.0}
    for i, pts in enumerate(letters):
        top, bottom = min(y for _, y in pts), max(y for _, y in pts)
        for k, edge in ((1, top), (2, bottom)):
            near = [x for x, y in pts if abs(y - edge) < 0.4]
            for straight, moved in ((X0 + i * pitch, min(near)), (X0 + i * pitch + LETTER, max(near))):
                num[k] += (moved - c) * (straight - c)
                den[k] += (straight - c) ** 2
    return num[1] / den[1], num[2] / den[2]


def fan_preset(n):
    """A trapezoid: each edge is the straight text scaled about its centre (PE-DESIGN 11.50:
    13 keeps the top and shrinks the bottom to 0.70, 14 the other way round at 0.69)."""
    st, sb = corner_scales(needle_points(REF / f"text-transform-{n}.pes"))
    us = [k / (KNOTS - 1) for k in range(KNOTS)]
    top = [(round(0.5 + (u - 0.5) * st, 4), 0.0) for u in us]
    bottom = [(round(0.5 + (u - 0.5) * sb, 4), 1.0) for u in us]
    return top, bottom


def fmt(edge):
    return "[" + ", ".join(f"({x:.4f}, {y:.4f})" for x, y in edge) + "]"


def generate():
    """The table's Rust source as the tool writes it (before `cargo fmt`)."""
    out = [
        "//! Text Transform envelopes, generated by `tools/fit_envelopes.py` from PE-DESIGN 11.50",
        "//! reference files (compat/reference/text/text-transform-N.pes). Do not edit by hand.",
        "//! Each preset's top and bottom edge at u = k/32 along the text, as fractions of the straight",
        "//! text's box: (x, y), x from its left, y from its top (y down).",
        "",
        "// Fitted values, some of which happen to be close to well-known constants.",
        "#![allow(clippy::approx_constant)]",
        "",
        "pub type Edge = [(f64, f64); 33];",
        "",
        "/// (preset number, top edge, bottom edge).",
        "pub static ENVELOPES: &[(u8, Edge, Edge)] = &[",
    ]
    for n in sorted(COLUMN + SLANT + FAN):
        if n in COLUMN:
            top, bottom = column_preset(n)
        elif n in SLANT:
            top, bottom = slant_preset(n)
        else:
            top, bottom = fan_preset(n)
        out.append(f"    ({n}, {fmt(top)}, {fmt(bottom)}),")
    out.append("];")
    return "\n".join(out) + "\n"


def table(text):
    """{preset: [every (x, y) of its top edge, then its bottom edge, as written]} from the
    table's Rust source, formatted or not."""
    body = text.split("ENVELOPES", 1)[-1]
    presets = {}
    for m in re.finditer(r"\(\s*(\d+),\s*\[(.*?)\],\s*\[(.*?)\],?\s*\)", body, re.S):
        pairs = re.findall(r"\(\s*([-\d.]+),\s*([-\d.]+)\s*\)", m.group(2) + m.group(3))
        presets[int(m.group(1))] = [(float(x), float(y)) for x, y in pairs]
    return presets


def differences(fitted, committed):
    """Presets whose points differ (beyond the 4 written decimals), are missing or are extra."""
    a, b = table(fitted), table(committed)
    out = []
    for n in sorted(set(a) | set(b)):
        if n not in a or n not in b:
            out.append((n, "only in " + ("the committed table" if n in b else "the tool's output")))
            continue
        worst = max((max(abs(p[0] - q[0]), abs(p[1] - q[1])) for p, q in zip(a[n], b[n])),
                    default=0.0)
        if len(a[n]) != len(b[n]) or worst > 0.00005:
            out.append((n, f"{len(a[n])} against {len(b[n])} points, up to {worst:.4f} apart"))
    return out


def self_test():
    """--check's comparison: a hand-edited or missing preset is found; formatting is not."""
    edge = lambda v: fmt([(k / 32, v) for k in range(KNOTS)])
    src = ("pub static ENVELOPES: &[(u8, Edge, Edge)] = &[\n"
           f"    (3, {edge(0.25)}, {edge(0.75)}),\n    (9, {edge(0.0)}, {edge(1.0)}),\n];\n")
    formatted = src.replace(", (", ",\n            (").replace("[(", "[\n            (")
    assert table(src)[3][0] == (0.0, 0.25) and len(table(src)[9]) == 2 * KNOTS, table(src)
    assert differences(src, formatted) == [], differences(src, formatted)
    edited = src.replace("(0.0312, 0.2500)", "(0.0312, 0.2501)", 1)
    assert [n for n, _ in differences(src, edited)] == [3], differences(src, edited)
    missing = src.replace(f"    (9, {edge(0.0)}, {edge(1.0)}),\n", "")
    assert differences(src, missing) == [(9, "only in the tool's output")], differences(src, missing)
    committed = TABLE.read_text(encoding="utf-8")
    assert sorted(table(committed)) == sorted(COLUMN + SLANT + FAN), sorted(table(committed))
    print("fit_envelopes self-test passed.")


def main():
    if "--self-test" in sys.argv:
        return self_test()
    if not TOOL.exists():
        sys.exit(f"build stitchtool first: {TOOL}")
    fitted = generate()
    if "--check" in sys.argv:
        found = differences(fitted, TABLE.read_text(encoding="utf-8"))
        for n, what in found:
            print(f"envelope preset {n}: {what}", file=sys.stderr)
        if found:
            sys.exit(f"{TABLE.relative_to(ROOT).as_posix()} differs from what "
                     "tools/fit_envelopes.py fits from the references (#886)")
        print(f"{TABLE.relative_to(ROOT).as_posix()} matches the fit of every preset.")
        return
    sys.stdout.write(fitted)


if __name__ == "__main__":
    main()
