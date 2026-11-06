#!/usr/bin/env python3
"""Regenerates every image under docs/images, and checks that each one has a generator (#565).

    cargo build --release -p openstitch-app -p stitchtool -p openstitch-mcp
    python3 -I tools/docs_images.py               # regenerate everything (about 30 minutes)
    python3 -I tools/docs_images.py --only engine,sdf
    python3 -I tools/docs_images.py --check       # every file has exactly one generator (no build)

The generators, by name (`--only` takes these):
- `screenshots`: tools/screenshots.py, the editor screenshots (docs/images/rust-editor-*,
  database-*, and docs/images/readme/*.png);
- `gallery`: tools/gallery.py, the README's "From image to embroidery" gallery
  (docs/images/readme/gallery);
- `engine`: tools/engine_image.py, a stitch plot of examples/<name>.json for each
  docs/images/engine-<name>.png, and `--templates` for engine-templates.png;
- `digitize`: tools/digitize_examples.py, the digitize-* and convert-* previews through
  openstitch-mcp (built when missing);
- `sdf`: the Stitch Design Factory patterns of examples/sewing-stitches.json exported as SVG by
  `sewing.export` (docs/images/sdf-*.svg);
- `catalogue`: page 1 of a Catalog style 4 catalogue of the example designs, written by
  `cargo run -p openstitch-library --example catalogue` (docs/images/library-catalogue.png).

RECORDS are not regenerated: they are dated evidence of a run described in the document that
shows them, which a new picture would no longer match: the Assistant's screenshots from a live
model session (docs/research/beyond/ai-integration.md) and the image-similarity comparisons with
Python prototypes that are not product code (docs/research/beyond/image-similarity-plan.md).
`--check` fails on any file under docs/images that is neither generated nor a listed record,
and on a generator or record that matches no file, so a new image cannot slip in without a way
to make it again. Uses the release build. Needs Pillow (and what the scripts above need).
"""

import argparse
import fnmatch
import json
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOLS = ROOT / "tools"
IMAGES = ROOT / "docs" / "images"
BIN = ROOT / "target" / "release"
EXE = ".exe" if sys.platform == "win32" else ""
PY = [sys.executable, "-I"]

sys.dont_write_bytecode = True
sys.path.insert(0, str(TOOLS))

RECORDS = {
    "docs/images/assistant-agent-picture.png": "docs/research/beyond/ai-integration.md (#393, live Claude Code session)",
    "docs/images/assistant-ollama-vector-route.png": "docs/research/beyond/ai-integration.md (#393, live Ollama session)",
    "docs/images/assistant-windows-claude-code.png": "docs/research/beyond/ai-integration.md (#582, live Claude Code session on Windows, 2026-11-06, retaken with the #645 subtitle)",
    **{f"docs/images/research/image-similarity/{n}.jpg": "docs/research/beyond/image-similarity-plan.md section 4.1"
       for n in ["g01-ap", "g05-ap", "g07-cs", "g08-ap", "p01-ps1", "p03-ps1", "p04-ps1", "p08-ps2"]},
}

# Example designs in the catalogue (alphabetical in the catalogue; badge also as its PES).
CATALOGUE = ["badge", "drawing", "heart", "lettering", "monograms", "name-drop", "ornaments", "stamps"]


def absolute_paths(steps, base):
    from engine_image import absolute_paths as resolve
    return resolve(steps, base)


def screenshot_outputs():
    import screenshots
    return sorted({out for _, outs in screenshots.SCENES.values() for out, _ in outs})


def engine_outputs():
    return sorted([f"docs/images/engine-{p.stem}.png" for p in (ROOT / "examples").glob("*.json")
                   if (IMAGES / f"engine-{p.stem}.png").exists()] + ["docs/images/engine-templates.png"])


def sdf_patterns():
    steps = json.loads((ROOT / "examples" / "sewing-stitches.json").read_text(encoding="utf-8"))
    return [s["params"]["stitch"] for s in steps if s.get("command") == "sewing.define"]


def sdf_name(pattern):
    return "docs/images/sdf-" + pattern["name"].lower().replace(" ", "-") + ".svg"


def run(cmd, **kw):
    print("+", " ".join(str(c) for c in cmd), flush=True)
    subprocess.run([str(c) for c in cmd], check=True, cwd=kw.pop("cwd", ROOT), **kw)


def exec_script(steps, work):
    script = work / "script.json"
    script.write_text(json.dumps(steps), encoding="utf-8")
    done = subprocess.run([str(BIN / f"stitchtool{EXE}"), "exec", str(script)], cwd=work, capture_output=True,
                          text=True)
    replies = [json.loads(line) for line in done.stdout.splitlines() if line.strip()]
    bad = [r for r in replies if not r.get("ok")]
    if done.returncode or bad:
        sys.exit(f"stitchtool exec: {bad[:1] or done.stderr}")
    return replies


def gen_screenshots():
    run(PY + [TOOLS / "screenshots.py", "--release"])


def gen_gallery():
    run(PY + [TOOLS / "gallery.py", "--release"])


def gen_engine():
    for out in engine_outputs():
        name = Path(out).stem[len("engine-"):]
        if name == "templates":
            run(PY + [TOOLS / "engine_image.py", "--templates", ROOT / out, "--release"])
        else:
            # Keep the width each picture was published at, so the documents' layout stays put.
            from PIL import Image
            width = Image.open(ROOT / out).width
            run(PY + [TOOLS / "engine_image.py", ROOT / "examples" / f"{name}.json", ROOT / out, "--release",
                      "--width", width])


def gen_digitize():
    run(PY + [TOOLS / "digitize_examples.py", "--release"])


def gen_sdf():
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        steps = json.loads((ROOT / "examples" / "sewing-stitches.json").read_text(encoding="utf-8"))
        steps = [s for s in steps if s.get("command") != "project.save"]
        steps += [{"command": "sewing.export", "params": {"id": p["id"], "path": f"{p['id']}.svg"}}
                  for p in sdf_patterns()]
        exec_script(steps, work)
        for p in sdf_patterns():
            shutil.copy(work / f"{p['id']}.svg", ROOT / sdf_name(p))
            print(sdf_name(p))


def gen_catalogue():
    with tempfile.TemporaryDirectory() as tmp:
        work = Path(tmp)
        designs = work / "designs"
        designs.mkdir()
        for name in CATALOGUE:
            steps = json.loads((ROOT / "examples" / f"{name}.json").read_text(encoding="utf-8"))
            steps = [s for s in steps if s.get("command") not in ("project.save", "design.export")]
            steps = absolute_paths(steps, ROOT / "examples")
            steps.append({"command": "project.save", "params": {"path": str(designs / f"{name}.osd")}})
            if name == "badge":
                steps.append({"command": "design.export", "params": {"path": str(designs / "badge.pes")}})
            script = work / f"{name}.json"
            script.write_text(json.dumps(steps), encoding="utf-8")
            run([BIN / f"stitchtool{EXE}", "exec", script], cwd=ROOT / "examples", stdout=subprocess.DEVNULL)
        # Relative folder names: the catalogue's title shows the folder as given ("Designs in designs").
        run(["cargo", "run", "--manifest-path", ROOT / "Cargo.toml", "--release", "-q", "-j", "2",
             "-p", "openstitch-library", "--example", "catalogue", "--", "designs", "out", "4"], cwd=work)
        from PIL import Image
        Image.open(work / "out" / "catalogue-1.png").convert("RGB").quantize(colors=128).save(
            IMAGES / "library-catalogue.png", optimize=True)
        print("docs/images/library-catalogue.png")


GENERATORS = {
    "screenshots": (gen_screenshots, screenshot_outputs),
    "gallery": (gen_gallery, lambda: ["docs/images/readme/gallery/*"]),
    "engine": (gen_engine, engine_outputs),
    "digitize": (gen_digitize, lambda: ["docs/images/digitize-*.png", "docs/images/convert-*.png"]),
    "sdf": (gen_sdf, lambda: [sdf_name(p) for p in sdf_patterns()]),
    "catalogue": (gen_catalogue, lambda: ["docs/images/library-catalogue.png"]),
}


def check():
    files = sorted(p.relative_to(ROOT).as_posix() for p in IMAGES.rglob("*") if p.is_file())
    problems = []
    claims = {f: [] for f in files}
    for name, (_, outputs) in GENERATORS.items():
        for pattern in outputs():
            hits = [f for f in files if fnmatch.fnmatchcase(f, pattern)]
            if not hits:
                problems.append(f"{name}: {pattern} matches no file under docs/images")
            for f in hits:
                claims[f].append(name)
    for f in RECORDS:
        if f not in claims:
            problems.append(f"record {f} does not exist")
        else:
            claims[f].append("record")
    for f, owners in claims.items():
        if not owners:
            problems.append(f"{f}: no generator (add it to a generator in tools/docs_images.py)")
        elif len(owners) > 1:
            problems.append(f"{f}: claimed by {', '.join(owners)}")
    for p in problems:
        print(p)
    print(f"docs_images: {len(files)} files, {len(problems)} problem(s)")
    return 1 if problems else 0


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true", help="check that every image has a generator")
    ap.add_argument("--only", help="comma-separated generators: " + ", ".join(GENERATORS))
    a = ap.parse_args()
    if a.check:
        return check()
    names = a.only.split(",") if a.only else list(GENERATORS)
    for name in names:
        if name not in GENERATORS:
            sys.exit(f"unknown generator {name}; one of {', '.join(GENERATORS)}")
        GENERATORS[name][0]()
    return check()


if __name__ == "__main__":
    sys.exit(main())
