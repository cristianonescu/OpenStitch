#!/usr/bin/env python3
"""Measures `object.deriveSkeleton {branches: true}` against #526's boxes.

Every item of the set (a Y, a five-point star, a ring and the letters K, X, N, R, M, W, A in Lato
Bold, at 10 and 25 mm; and y, outside #526's set but named by skeleton.rs's floor test) is split into satin columns by stitchtool, its stitches are exported as
CSV in design coordinates, and four things are measured on what the machine would sew:

- columns: the column objects made (#526 asks for one per arm);
- iou: the rendered coverage IoU against the shape (box: >= 0.95). The satin's triangles (each
  needle point with the two before it) are rasterised at 0.05 mm against the shape's even-odd
  fill;
- gap: the longest stretch of the outline (mm) farther than 0.15 mm from the sewn area (box:
  <= 0.3 mm);
- join: the shortest overlap (mm) of a column end inside another column's satin, over the ends
  that are not at the outline (box: >= joinOverlap, 0.3 mm);
- corner: the farthest a sharp convex corner of the outline (turn >= 60 degrees) is from a
  needle point (mm; box: <= 0.2 mm).

Every needle point the machine sews counts, except tie-ins and locks (rule B', adopted on #526,
2026-11-12): out-and-back steps back to the same point, at most 2 mm long, within a run's first
or last TIE_ZONE stitches (runs split at jumps, trims and colour changes). OpenStitch ties in with
up to three 0.3 mm steps; a satin's short crossings into a narrow end corner count.

    python3 -I tools/branch_columns.py --stitchtool target/release/stitchtool
    python3 -I tools/branch_columns.py --stitchtool target/release/stitchtool --only K X --json out.json
    python3 -I tools/branch_columns.py --self-test   # the measure on synthetic stitches

Needs numpy and Pillow.
"""

import argparse
import json
import math
import subprocess
import sys
import tempfile
from pathlib import Path

import numpy as np
from PIL import Image, ImageDraw

RES = 0.05            # mm per pixel of the coverage raster
NEAR = 3              # pixels (0.15 mm): an outline point this near the sewn area is reached
TIE_ZONE = 8          # stitches at a run's start and end where a tie-in or lock is looked for
TIE_BACK = 0.05       # mm: an out-and-back step ends this near where it began
TIE_LONGEST = 2.0     # mm: the longest tie step (finish::TIE_LENGTH_RANGE)
LONGEST = 15.0        # mm: a longer stitch is a travel, not satin
SHARP = 60.0          # degrees: a corner turning at least this much is sharp
AT_OUTLINE = 0.5      # mm: a column end this near the outline is a free end, not a join
JOIN_NEAR = 1.0       # mm: an end within this of another column's satin is a join
ITEMS = ["Y", "star", "ring", "K", "X", "N", "R", "M", "W", "A", "y"]
SIZES = [10.0, 25.0]
ARMS = {"Y": 3, "star": 5, "ring": 1, "K": 3, "X": 4, "N": 3, "R": 4, "M": 4, "W": 4, "A": 3, "y": 2}


def contours_mm(obj):
    """The closed contours of an `object.get` result, in mm, cubic curves flattened."""
    out = []
    for c in obj["contours"]:
        if not c.get("closed"):
            continue
        pts = [(p["x"] / 10.0, p["y"] / 10.0) for p in c["points"]]
        curves = c.get("curves") or [None] * len(pts)
        flat = []
        n = len(pts)
        for k in range(n):
            a, b = pts[k], pts[(k + 1) % n]
            cv = curves[k] if k < len(curves) else None
            flat.append(a)
            if cv and cv.get("c1") and cv.get("c2"):
                c1 = (cv["c1"]["x"] / 10.0, cv["c1"]["y"] / 10.0)
                c2 = (cv["c2"]["x"] / 10.0, cv["c2"]["y"] / 10.0)
                for i in range(1, 24):
                    t = i / 24
                    u = 1 - t
                    flat.append((u**3 * a[0] + 3 * u * u * t * c1[0] + 3 * u * t * t * c2[0] + t**3 * b[0],
                                 u**3 * a[1] + 3 * u * u * t * c1[1] + 3 * u * t * t * c2[1] + t**3 * b[1]))
        out.append(flat)
    return out


def ties(pts):
    """For each stitch record (kind, x, y): whether it belongs to a tie-in or lock."""
    kinds = [p[0] for p in pts]
    xy = [(p[1], p[2]) for p in pts]
    pos, left = [0] * len(pts), [0] * len(pts)
    k = 0
    for i, kd in enumerate(kinds):
        k = k + 1 if kd == "stitch" else 0
        pos[i] = k
    k = 0
    for i in range(len(kinds) - 1, -1, -1):
        k = k + 1 if kinds[i] == "stitch" else 0
        left[i] = k
    tie = [False] * len(pts)
    for j in range(2, len(pts)):
        edge = pos[j] <= TIE_ZONE or left[j] <= TIE_ZONE
        if (edge and kinds[j] == kinds[j - 1] == kinds[j - 2] == "stitch"
                and math.dist(xy[j], xy[j - 2]) < TIE_BACK and math.dist(xy[j], xy[j - 1]) <= TIE_LONGEST):
            tie[j] = tie[j - 1] = True
    return tie


def segment_distance(p, a, b):
    vx, vy = b[0] - a[0], b[1] - a[1]
    l2 = vx * vx + vy * vy or 1e-12
    t = max(0.0, min(1.0, ((p[0] - a[0]) * vx + (p[1] - a[1]) * vy) / l2))
    return math.dist(p, (a[0] + t * vx, a[1] + t * vy))


def measure(outline, skeletons, pts):
    """The boxes for one item. `outline`: closed contours in mm; `skeletons`: each column's
    (points mm, widths mm); `pts`: stitch records (kind, x mm, y mm) in sewing order."""
    xs = [p[0] for c in outline for p in c]
    ys = [p[1] for c in outline for p in c]
    x0, y0 = min(xs) - 1, min(ys) - 1
    w = int((max(xs) - x0 + 1) / RES) + 2
    h = int((max(ys) - y0 + 1) / RES) + 2

    def px(p):
        return ((p[0] - x0) / RES, (p[1] - y0) / RES)

    shp = np.zeros((h, w), bool)
    for c in outline:
        im = Image.new("1", (w, h), 0)
        ImageDraw.Draw(im).polygon([px(p) for p in c], fill=1)
        shp ^= np.array(im, bool)
    sew_im = Image.new("1", (w, h), 0)
    draw = ImageDraw.Draw(sew_im)
    tie = ties(pts)
    kinds = [p[0] for p in pts]
    xy = [(p[1], p[2]) for p in pts]
    for i in range(2, len(pts)):
        if not (kinds[i] == kinds[i - 1] == kinds[i - 2] == "stitch") or tie[i] or tie[i - 1] or tie[i - 2]:
            continue
        a, b, c = xy[i - 2], xy[i - 1], xy[i]
        if math.dist(a, b) <= LONGEST and math.dist(b, c) <= LONGEST:
            draw.polygon([px(a), px(b), px(c)], fill=1)
    sew = np.array(sew_im, bool)
    iou = float((sew & shp).sum() / max((sew | shp).sum(), 1))
    near = sew.copy()
    for oy in range(-NEAR, NEAR + 1):
        for ox in range(-NEAR, NEAR + 1):
            if ox * ox + oy * oy <= NEAR * NEAR:
                near |= np.roll(np.roll(sew, oy, 0), ox, 1)
    gap = 0.0
    for c in outline:
        samples = []
        n = len(c)
        for k in range(n):
            a, b = c[k], c[(k + 1) % n]
            m = max(1, int(math.dist(a, b) / 0.05))
            samples += [((a[0] + (b[0] - a[0]) * t / m, a[1] + (b[1] - a[1]) * t / m), math.dist(a, b) / m)
                        for t in range(m)]
        run = best = 0.0
        for _ in range(2):  # twice round, so a gap across the start counts whole
            for q, step in samples:
                i, j = int(px(q)[1]), int(px(q)[0])
                far = not near[min(max(i, 0), h - 1), min(max(j, 0), w - 1)]
                run = run + step if far else 0.0
                best = max(best, run)
        gap = max(gap, min(best, sum(s for _, s in samples)))
    needles = [q for q, k in zip(xy, kinds) if k == "stitch"]
    corner = 0.0
    for c in outline:
        n = len(c)
        area = sum(c[k][0] * c[(k + 1) % n][1] - c[(k + 1) % n][0] * c[k][1] for k in range(n))
        for k in range(n):
            a, b, e = c[k - 1], c[k], c[(k + 1) % n]
            u, v = (b[0] - a[0], b[1] - a[1]), (e[0] - b[0], e[1] - b[1])
            if math.hypot(*u) < 0.2 or math.hypot(*v) < 0.2:
                continue
            cross = u[0] * v[1] - u[1] * v[0]
            turn = math.degrees(math.atan2(abs(cross), u[0] * v[0] + u[1] * v[1]))
            if turn >= SHARP and (cross > 0) == (area > 0) and needles:
                corner = max(corner, min(math.dist(b, s) for s in needles))
    edges = [(c[k], c[(k + 1) % len(c)]) for c in outline for k in range(len(c))]

    def depth_in(p, sp, ws):
        best = -1e9
        for k in range(len(sp) - 1):
            a, b = sp[k], sp[k + 1]
            vx, vy = b[0] - a[0], b[1] - a[1]
            l2 = vx * vx + vy * vy or 1e-12
            t = max(0.0, min(1.0, ((p[0] - a[0]) * vx + (p[1] - a[1]) * vy) / l2))
            wd = ws[k] + (ws[k + 1] - ws[k]) * t if len(ws) == len(sp) else (ws[0] if ws else 0.0)
            best = max(best, wd / 2 - math.dist(p, (a[0] + t * vx, a[1] + t * vy)))
        return best

    joins = []
    for ci, (sp, _) in enumerate(skeletons):
        if len(sp) < 2:
            continue
        for end in (sp[0], sp[-1]):
            if min(segment_distance(end, a, b) for a, b in edges) < AT_OUTLINE:
                continue
            depths = [depth_in(end, *skeletons[cj]) for cj in range(len(skeletons))
                      if cj != ci and len(skeletons[cj][0]) >= 2]
            if depths and max(depths) > -JOIN_NEAR:
                joins.append(max(depths))
    return {"columns": len(skeletons), "iou": iou, "gap": gap,
            "join": min(joins) if joins else None, "corner": corner}


def shape_steps(item, size):
    if item == "star":
        return [{"command": "shape.preset", "params": {"kind": "star", "cx": 50, "cy": 50, "width": size,
                                                       "fill": "Red"}}]
    if item == "ring":
        return [{"command": "shape.preset", "params": {"kind": "frame", "frameOf": "ellipse", "cx": 50, "cy": 50,
                                                       "width": size, "height": size, "bandWidth": size * 0.2,
                                                       "fill": "Red"}}]
    return [{"command": "text.add", "params": {"text": item, "size": size, "x": 50, "y": 70, "fill": "Red"}},
            {"command": "text.toShapes", "params": {"id": 1}}]


def results(stitchtool, script, tmp):
    (tmp / "s.json").write_text(json.dumps(script), encoding="utf-8")
    out = subprocess.run([stitchtool, "exec", "s.json"], cwd=tmp, capture_output=True, text=True,
                         encoding="utf-8")
    return [json.loads(line) for line in out.stdout.splitlines() if line.strip().startswith("{")]


def run(stitchtool, item, size):
    """Splits one item with stitchtool and measures it."""
    with tempfile.TemporaryDirectory() as tmp:
        tmp = Path(tmp)
        sid = 1 if item in ("star", "ring") else 2
        steps = [{"command": "design.new", "params": {"width": 100, "height": 100}}] + shape_steps(item, size)
        steps += [{"command": "object.get", "params": {"id": sid}},
                  {"command": "object.deriveSkeleton", "params": {"id": sid, "branches": True, "joinOverlap": 0.3}},
                  {"command": "design.export", "params": {"path": "out.csv"}}]
        res = results(stitchtool, steps, tmp)
        bad = [r for r in res if not r.get("ok")]
        if bad:
            return {"error": bad[0].get("error")}
        shape, derived = res[-3]["result"], res[-2]["result"]
        cols = derived.get("ids") or [sid]
        skeletons = []
        for cid in cols:
            got = results(stitchtool, steps[:-1] + [{"command": "object.get", "params": {"id": cid}}], tmp)[-1]
            sk = (got["result"].get("region") or {}).get("skeleton") or {}
            skeletons.append(([(p["x"] / 10.0, p["y"] / 10.0) for p in sk.get("points", [])], sk.get("widths", [])))
        pts = []
        for line in (tmp / "out.csv").read_text(encoding="utf-8").splitlines()[1:]:
            f = line.split(",")
            pts.append((f[1], float(f[2]), float(f[3])))
    return measure(contours_mm(shape), skeletons, pts)


def zigzag(x0, x1, y0, y1, step):
    """Satin across the band y0..y1 from x0 to x1 as a column sews it: across its first rung,
    then one needle point per rung on alternate edges, and across its last rung."""
    n = round((x1 - x0) / step)
    pts = [("stitch", x0, y0)]
    for k in range(n + 1):
        pts.append(("stitch", x0 + (x1 - x0) * k / n, y1 if k % 2 == 0 else y0))
    last = pts[-1]
    pts.append(("stitch", x1, y0 if last[2] == y1 else y1))
    return pts


def self_test():
    """The measure on synthetic stitches: a full satin meets the boxes, a tie-in doesn't count as
    cover, a half-sewn band shows its gap, a short crossing into a corner counts."""
    fails = []
    rect = [[(0.0, 0.0), (10.0, 0.0), (10.0, 4.0), (0.0, 4.0)]]
    skel = [([(0.0, 2.0), (10.0, 2.0)], [4.0, 4.0])]
    full = [("jump", 0.0, 0.0)] + zigzag(0.0, 10.0, 0.0, 4.0, 0.2)
    m = measure(rect, skel, full)
    if not (m["iou"] >= 0.95 and m["gap"] <= 0.3 and m["corner"] <= 0.2):
        fails.append(f"a full satin: {m}")
    half = [("jump", 0.0, 0.0)] + zigzag(0.0, 5.0, 0.0, 4.0, 0.2)
    m = measure(rect, skel, half)
    if not (m["iou"] < 0.6 and m["gap"] > 4.0):
        fails.append(f"a half-sewn band: {m}")
    # A tie-in alone sews nothing: out-and-back 0.3 mm steps at the run's start.
    tie = [("jump", 9.0, 2.0), ("stitch", 9.0, 2.0), ("stitch", 9.3, 2.0), ("stitch", 9.0, 2.0),
           ("stitch", 9.3, 2.0), ("stitch", 9.0, 2.0)]
    if any(not t for t in ties(tie)[2:]):
        fails.append(f"a tie-in not recognised: {ties(tie)}")
    m = measure(rect, skel, half + tie)
    if m["iou"] > 0.6:
        fails.append(f"a tie-in counted as cover: {m}")
    # A satin into a pointed end: its last crossings are 0.1-0.2 mm and still count.
    tri = [[(0.0, 0.0), (10.0, 2.0), (0.0, 4.0)]]
    pts = [("jump", 0.0, 0.0), ("stitch", 0.0, 0.0)]
    for k in range(51):
        x = 10.0 * k / 50
        half_w = 2.0 * (1 - x / 10.0)
        pts.append(("stitch", x, 2.0 + half_w if k % 2 == 0 else 2.0 - half_w))
    m = measure(tri, [([(0.0, 2.0), (10.0, 2.0)], [4.0, 0.0])], pts)
    if not (m["corner"] <= 0.2 and m["iou"] >= 0.95):
        fails.append(f"a satin into a point: {m}")
    for f in fails:
        print("FAIL", f)
    print(f"branch_columns self-test: {len(fails)} problem(s)")
    return 1 if fails else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--stitchtool", type=Path, help="the stitchtool binary to split with")
    ap.add_argument("--only", nargs="+", choices=ITEMS, help="measure these items only")
    ap.add_argument("--json", type=Path, help="also write the rows as JSON")
    ap.add_argument("--self-test", action="store_true", help="test the measure on synthetic stitches")
    a = ap.parse_args()
    if a.self_test:
        return self_test()
    if not a.stitchtool:
        ap.error("--stitchtool is required (or --self-test)")
    rows = []
    print("| item | mm | columns (arms) | IoU | gap mm | join mm | corner mm |")
    print("|---|---|---|---|---|---|---|")
    for item in a.only or ITEMS:
        for size in SIZES:
            r = run(str(a.stitchtool), item, size)
            r.update(item=item, size=size, arms=ARMS[item])
            rows.append(r)
            if "error" in r:
                print(f"| {item} | {size:.0f} | error: {r['error']} | | | | |")
                continue
            join = "–" if r["join"] is None else f"{r['join']:.2f}"
            print(f"| {item} | {size:.0f} | {r['columns']} ({r['arms']}) | {r['iou']:.3f} | {r['gap']:.2f} | "
                  f"{join} | {r['corner']:.2f} |", flush=True)
    if a.json:
        a.json.write_text(json.dumps(rows, indent=1), encoding="utf-8")
    return 0


if __name__ == "__main__":
    sys.exit(main())
