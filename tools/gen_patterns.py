#!/usr/bin/env python3
"""Generates OpenStitch's own stitch-pattern library (crates/openstitch-design/patterns/).

Every pattern here is an original drawing made from simple maths (sines, arcs, cycloids,
straight lines). None is derived from Brother's .pmf/.pas/.plf files.

Format (`openstitch.pattern`, version 1), one JSON file per pattern:

    {"format": "openstitch.pattern", "version": 1,
     "kind": "motif" | "fill" | "decorative" | "meander",
     "id": "wave", "name": "Wave", "rowShift": 0.5 (optional), "paths": [[[x, y], ...], ...]}

- motif: one path from (0, 0) to (1, 0); x runs along the line (-0.5...1.5, so loops may
  overlap the neighbours), y across it in -1...1.
- fill: segments in the unit tile [0, 1]²; needle points of programmable fill fall on them.
- decorative: one path per tile from (0, y0) to (1, y0) (x may reach -0.5...1.5, y stays in
  0...1) so tiles chain into rows; rows repeat
  every tile height, shifted by rowShift tile widths.
- meander: one path from (0, y0) to (1, y0) inside the unit tile (0...1) that never crosses
  itself, its neighbours in the row, or the rows above and below; chained into rows like
  decorative paths, then joined along the outline (#533).

Run: python3 -I tools/gen_patterns.py
"""

import json
import math
import os

OUT = os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "crates",
                   "openstitch-design", "patterns")


def r(points):
    return [[round(x, 4) + 0.0, round(y, 4) + 0.0] for x, y in points]


def sample(f, n, t0=0.0, t1=1.0):
    return [f(t0 + (t1 - t0) * i / n) for i in range(n + 1)]


def motifs():
    yield "wave", "Wave", [sample(lambda t: (t, -0.5 * math.sin(2 * math.pi * t)), 16)]
    yield "scallop", "Scallop", [sample(
        lambda t: (0.5 - 0.5 * math.cos(math.pi * t), -0.8 * math.sin(math.pi * t)), 16)]
    yield "peak", "Peak", [[(0, 0), (0.5, -0.9), (1, 0)]]
    yield "diamond", "Diamond", [[(0, 0), (0.5, -0.6), (1, 0), (0.5, 0.6), (0, 0), (1, 0)]]
    # Heart above the line, tip on the line at x = 0.5.
    heart = []
    for i in range(33):
        t = math.pi + 2 * math.pi * i / 32
        hx = 16 * math.sin(t) ** 3
        hy = 13 * math.cos(t) - 5 * math.cos(2 * t) - 2 * math.cos(3 * t) - math.cos(4 * t)
        heart.append((0.5 + hx / 40, -(hy + 17) / 29))
    yield "heart", "Heart", [[(0, 0)] + heart + [(1, 0)]]
    upper = sample(lambda t: (0.2 + 0.7 * t, -0.9 * t - 0.35 * math.sin(math.pi * t)), 10)
    lower = sample(lambda t: (0.9 - 0.7 * t, -0.9 * (1 - t) + 0.3 * math.sin(math.pi * t)), 10)
    yield "leaf", "Leaf", [[(0, 0)] + upper + lower[1:] + [(1, 0)]]
    a = 1.8
    yield "loop", "Loop", [sample(
        lambda t: (t / (2 * math.pi) - a * math.sin(t) / (2 * math.pi), -(1 - math.cos(t)) / 2.2),
        32, 0, 2 * math.pi)]


def fills():
    yield "diamonds", "Diamonds", [[(0, 0.5), (0.5, 0), (1, 0.5), (0.5, 1), (0, 0.5)]]
    yield "waves", "Waves", [sample(lambda t: (t, 0.5 + 0.3 * math.sin(2 * math.pi * t)), 16)]
    yield "bricks", "Bricks", [
        [(0, 0.25), (1, 0.25)], [(0, 0.75), (1, 0.75)],
        [(0.25, 0.25), (0.25, 0.75)], [(0.75, 0.75), (0.75, 1)], [(0.75, 0), (0.75, 0.25)]]
    yield "circles", "Circles", [sample(
        lambda t: (0.5 + 0.35 * math.cos(t), 0.5 + 0.35 * math.sin(t)), 24, 0, 2 * math.pi)]
    yield "chevrons", "Chevrons", [[(0, 0.7), (0.5, 0.3), (1, 0.7)]]
    yield "basket", "Basket weave", [
        [(0.05, 0.17), (0.45, 0.17)], [(0.05, 0.33), (0.45, 0.33)],
        [(0.67, 0.05), (0.67, 0.45)], [(0.83, 0.05), (0.83, 0.45)],
        [(0.17, 0.55), (0.17, 0.95)], [(0.33, 0.55), (0.33, 0.95)],
        [(0.55, 0.67), (0.95, 0.67)], [(0.55, 0.83), (0.95, 0.83)]]


def decoratives():
    yield "waves", "Waves", 0.0, [sample(
        lambda t: (t, 0.5 + 0.35 * math.sin(2 * math.pi * t)), 24)]
    yield "scales", "Scales", 0.5, [sample(
        lambda t: (t, 0.9 - 0.8 * math.sin(math.pi * t)), 16)]
    a = 1.9
    yield "loops", "Loops", 0.5, [sample(
        lambda t: (t / (2 * math.pi) - a * math.sin(t) / (2 * math.pi),
                   0.8 - 0.35 * (1 - math.cos(t))), 40, 0, 2 * math.pi)]
    yield "chevrons", "Chevrons", 0.0, [[(0, 0.8), (0.5, 0.2), (1, 0.8)]]
    yield "meander", "Meander", 0.5, [[(0, 0.8), (0.25, 0.8), (0.25, 0.2), (0.75, 0.2),
                                       (0.75, 0.8), (1, 0.8)]]
    yield "clouds", "Clouds", 0.33, [
        sample(lambda t: (0.6 * t, 0.8 - 0.55 * math.sin(math.pi * t)), 12)
        + sample(lambda t: (0.6 + 0.4 * t, 0.8 - 0.35 * math.sin(math.pi * t)), 8)[1:]]


def hilbert(order):
    """Cells (x, y) of a Hilbert curve of `order` on a 2^order grid, from (0, 0) to (n-1, 0)."""
    n = 2 ** order
    cells = []
    for d in range(n * n):
        x = y = 0
        t = d
        s = 1
        while s < n:
            rx = 1 & (t // 2)
            ry = 1 & (t ^ rx)
            if ry == 0:
                if rx == 1:
                    x, y = s - 1 - x, s - 1 - y
                x, y = y, x
            x += s * rx
            y += s * ry
            t //= 4
            s *= 2
        cells.append((x, y))
    return n, cells


def meanders():
    """Meander tiles (#533): one path from (0, y0) to (1, y0) inside the unit tile that never
    crosses itself, its neighbours in the row, or the rows above and below (rows repeat every
    tile height). Own drawings from simple maths."""
    for pid, name, order in (("hilbert", "Hilbert", 2), ("hilbert-fine", "Hilbert, fine", 3)):
        n, cells = hilbert(order)
        c = lambda i: (i + 0.5) / n
        pts = [(c(x), 1 - c(y)) for x, y in cells]
        yield pid, name, 0.0, [[(0, pts[0][1])] + pts + [(1, pts[-1][1])]]
    # Square serpentine: four strokes a quarter tile apart.
    pts = [(0, 0.125)]
    for k in range(4):
        x = 0.125 + 0.25 * k
        lo, hi = (0.125, 0.875) if k % 2 == 0 else (0.875, 0.125)
        pts += [(x, lo), (x, hi)]
    yield "serpentine", "Serpentine", 0.0, [pts + [(1, 0.125)]]
    # The same strokes joined by half circles.
    pts = [(0, 0.125), (0.125, 0.125)]
    for k in range(4):
        x = 0.125 + 0.25 * k
        up = k % 2 == 0
        pts.append((x, 0.75 if up else 0.25))
        if k < 3:
            cy = 0.75 if up else 0.25
            for i in range(1, 12):
                a = math.pi * i / 12
                pts.append((x + 0.125 - 0.125 * math.cos(a), cy + (0.125 if up else -0.125) * math.sin(a)))
            pts.append((x + 0.25, cy))
    pts += [(0.875, 0.125), (1, 0.125)]
    yield "switchback", "Switchback", 0.0, [pts]
    # Curtate trochoids: x keeps growing, so the S-turns never cross.
    yield "vermicelli", "Vermicelli", 0.5, [sample(
        lambda t: (t - 0.85 * math.sin(4 * math.pi * t) / (4 * math.pi),
                   0.5 + 0.34 * math.sin(4 * math.pi * t + 0.6 * math.sin(2 * math.pi * t))), 64)]
    yield "ribbon-candy", "Ribbon candy", 0.0, [sample(
        lambda t: (t - 0.5 * math.sin(6 * math.pi * t) / (6 * math.pi),
                   0.5 - 0.34 * math.cos(6 * math.pi * t)), 72)]
    # Three half-ellipse arches on short flat feet (no cusps: neighbouring strands stay apart).
    pts = [(0, 0.15)]
    w = 1 / 3 - 0.08
    for k in range(3):
        c = (k + 0.5) / 3
        pts.append((c - w / 2, 0.15))
        for i in range(1, 24):
            a = math.pi * i / 24
            pts.append((c - (w / 2) * math.cos(a), 0.15 + 0.7 * math.sin(a)))
        pts.append((c + w / 2, 0.15))
    yield "clamshell", "Clamshell", 0.5, [pts + [(1, 0.15)]]
    yield "sawtooth", "Sawtooth", 0.0, [
        [(0, 0.15)] + [(0.125 + 0.25 * k, 0.85 if k % 2 == 0 else 0.15) for k in range(4)] + [(1, 0.15)]]
    steps = [0.15, 0.38, 0.62, 0.85, 0.62, 0.38]
    pts = [(0, 0.15)]
    for k, y in enumerate(steps + [0.15]):
        x = (k + 0.5) / 7
        for p in ((x, pts[-1][1]), (x, y)):
            if p != pts[-1]:
                pts.append(p)
    yield "staircase", "Staircase", 0.0, [pts + [(1, 0.15)]]
    # A line with puzzle knobs, one up and one down per tile.
    pts = [(0, 0.5)]
    for cx, up in ((0.25, True), (0.75, False)):
        sgn = 1 if up else -1
        pts += [(cx - 0.07, 0.5), (cx - 0.05, 0.5 + sgn * 0.12)]
        a0 = math.atan2(-0.11, -0.05)
        for i in range(1, 24):
            a = a0 - 2 * math.pi * i / 24 * (1 - 2 * math.atan2(0.05, 0.11) / (2 * math.pi))
            pts.append((cx + 0.11 * math.cos(a), 0.5 + sgn * (0.23 + 0.11 * math.sin(a))))
        pts += [(cx + 0.05, 0.5 + sgn * 0.12), (cx + 0.07, 0.5)]
    yield "jigsaw", "Jigsaw", 0.0, [pts + [(1, 0.5)]]
    tops = [0.85, 0.6, 0.75, 0.5, 0.8, 0.65]
    pts = [(0, 0.15)]
    for k, top in enumerate(tops):
        x = (k + 0.5) / 6
        lo, hi = (0.15, top) if k % 2 == 0 else (top, 0.15)
        pts += [(x, lo), (x, hi)]
    yield "skyline", "Skyline", 0.0, [pts + [(1, 0.15)]]
    yield "ripple", "Ripple", 0.0, [sample(
        lambda t: (t, 0.5 + 0.35 * math.sin(8 * math.pi * t) * math.sin(math.pi * t)), 96)]


def write(kind, pid, name, paths, row_shift=None):
    doc = {"format": "openstitch.pattern", "version": 1, "kind": kind, "id": pid, "name": name}
    if row_shift:
        doc["rowShift"] = row_shift
    doc["paths"] = [r(p) for p in paths]
    path = os.path.join(OUT, kind, pid + ".json")
    os.makedirs(os.path.dirname(path), exist_ok=True)
    with open(path, "w") as f:
        json.dump(doc, f, separators=(",", ":"))
        f.write("\n")


def main():
    for pid, name, paths in motifs():
        write("motif", pid, name, paths)
    for pid, name, paths in fills():
        write("fill", pid, name, paths)
    for pid, name, shift, paths in decoratives():
        write("decorative", pid, name, paths, shift)
    for pid, name, shift, paths in meanders():
        write("meander", pid, name, paths, shift)


if __name__ == "__main__":
    main()
