#!/usr/bin/env python3
"""Rewrites the CEmbLine pattern blocks of committed PE-DESIGN captures (#1060).

PE-DESIGN 11 writes its default decorative fill pattern into the pattern block of every shape it
saves (a `CEmbLine` body's bytes 386 on: u16 size, u16 polyline count, then per polyline six zero
bytes, u16 1, u16 point count and i16 x, y points in a 1024-unit cell). That pattern is Brother
content and stays out of the repository. This tool replaces every block that is not made of the
project's own patterns (the `.pas` / `.plf` sources in compat/reference/pes and the design crate's
pattern fixtures) with OpenStitch's placeholder rows at the same point counts, so the file keeps its
length and every other byte (the PEC stitches included). For each file it prints the SHA-256 of
everything outside the rewritten blocks before and after, which must match.

usage:
    python3 -I tools/redact_pattern_blocks.py            # list the blocks that are not ours (dry run)
    python3 -I tools/redact_pattern_blocks.py --write    # rewrite them in place
    ... --table FILE                                     # also write the per-file hash table
"""
import hashlib
import math
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
SOURCES = [ROOT / "compat/reference/pes", ROOT / "crates/openstitch-design/tests/fixtures/patterns"]
SCAN = ["compat", "crates", "examples"]


def own_polylines():
    """Every polyline of the project's own pattern sources (@TILELINE i n x y ...)."""
    out = set()
    for d in SOURCES:
        for f in sorted(d.iterdir()):
            if f.suffix.lower() not in (".pas", ".plf"):
                continue
            for line in f.read_text(encoding="latin-1").splitlines():
                if line.startswith("@TILELINE "):
                    v = [int(t) for t in line.split()[3:]]
                    out.add(tuple(zip(v[0::2], v[1::2])))
    return out


def blocks(data, end, begin=0):
    """(start, stop, polylines) of every pattern block in data[begin:end]: found by structure (a
    u16 size, a u16 count, and polylines of six zero bytes, u16 1, a point count and points).
    Start at the first object class: a false match in the header could run past a real block."""
    found, o = [], begin
    while True:
        o = data.find(b"\x00\x00\x00\x00\x00\x00\x01\x00", o, end)
        if o < 0:
            return found
        start = o - 4
        try:
            size, n = struct.unpack_from("<HH", data, start)
            p, lines = start + 4, []
            if not (1 <= n <= 400 and 100 <= size <= 40000):
                raise ValueError
            for _ in range(n):
                if data[p:p + 6] != bytes(6) or struct.unpack_from("<H", data, p + 6)[0] != 1:
                    raise ValueError
                cnt = struct.unpack_from("<H", data, p + 8)[0]
                if not 1 <= cnt <= 4000 or p + 10 + 4 * cnt > end:
                    raise ValueError
                pts = tuple(struct.unpack_from("<hh", data, p + 10 + 4 * k) for k in range(cnt))
                lines.append(pts)
                p += 10 + 4 * cnt
            found.append((start, p, lines))
            o = p
        except (ValueError, struct.error):
            o += 1


def rust_round(v):
    """Rust's f64::round (half away from zero), as the writer rounds; Python's round() is
    half-to-even."""
    return math.floor(v + 0.5) if v >= 0 else -math.floor(-v + 0.5)


def placeholder(lines):
    """OpenStitch's placeholder rows (pes11.rs `pattern_block`) at the same point counts."""
    n = len(lines)
    out = []
    for k, line in enumerate(lines):
        c = len(line)
        y = rust_round((k + 0.5) * 1024 / n)
        out.append(tuple((rust_round(j * 1024 / (max(c, 2) - 1)), y) for j in range(c)))
    return out


def is_placeholder(lines):
    return lines == placeholder(lines)


def outside(data, ranges):
    h, last = hashlib.sha256(), 0
    for s, e in ranges:
        h.update(data[last:s])
        last = e
    h.update(data[last:])
    return h.hexdigest()


def main():
    write = "--write" in sys.argv
    ours = own_polylines()
    files = sorted(f for d in SCAN for f in (ROOT / d).rglob("*") if f.suffix.lower() == ".pes"
                   and "target" not in f.parts)
    changed = blocks_rewritten = 0
    kinds = {}
    table = []
    for f in files:
        data = f.read_bytes()
        if not data.startswith(b"#PES0110"):
            continue
        pec = struct.unpack_from("<I", data, 8)[0]
        # The objects only: the header's pattern lists (fill, motif, stamps) are not shape blocks.
        first = data.find(b"CEmb", 0, pec)
        if first < 0:
            continue
        rewrite = [(s, e, l) for s, e, l in blocks(data, pec, first + 4)
                   if not is_placeholder(l) and not all(line in ours for line in l)]
        if not rewrite:
            continue
        new = bytearray(data)
        for s, e, lines in rewrite:
            p = s + 4
            for line, row in zip(lines, placeholder(lines)):
                for k, (x, y) in enumerate(row):
                    struct.pack_into("<hh", new, p + 10 + 4 * k, x, y)
                p += 10 + 4 * len(line)
        ranges = [(s, e) for s, e, _ in rewrite]
        before, after = outside(data, ranges), outside(bytes(new), ranges)
        assert before == after and len(new) == len(data), f
        assert new[pec:] == data[pec:], f
        changed += 1
        blocks_rewritten += len(rewrite)
        for s, e, _ in rewrite:
            kinds.setdefault(hashlib.sha256(data[s:e]).hexdigest()[:12], []).append(f.name)
        rel = f.relative_to(ROOT).as_posix()
        print(f"{rel}: {len(rewrite)} block(s); outside {before[:16]} = {after[:16]}")
        table.append((rel, hashlib.sha256(data).hexdigest(), hashlib.sha256(bytes(new)).hexdigest(),
                      len(rewrite), before))
        if write:
            f.write_bytes(bytes(new))
    print(f"{changed} files, {blocks_rewritten} blocks {'rewritten' if write else 'to rewrite (dry run)'}")
    for k, names in sorted(kinds.items(), key=lambda kv: -len(kv[1])):
        print(f"  block content {k}: {len(names)} blocks, e.g. {sorted(set(names))[:3]}")
    if "--table" in sys.argv:
        out = Path(sys.argv[sys.argv.index("--table") + 1])
        rows = ["| file | blocks | SHA-256 as saved by PE-DESIGN | SHA-256 now | SHA-256 of every byte outside the blocks (before = after) |",
                "|---|---|---|---|---|"]
        rows += [f"| `{r}` | {n} | `{old}` | `{new}` | `{o}` |" for r, old, new, n, o in table]
        out.write_text("\n".join(rows) + "\n", encoding="utf-8", newline="\n")
        print("table:", out)


if __name__ == "__main__":
    main()
