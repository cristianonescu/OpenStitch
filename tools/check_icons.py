#!/usr/bin/env python3
"""Fails when an icon or graphic could have come from somewhere other than Higgsfield.

Every icon, illustration, texture and graphic in OpenStitch is generated with Higgsfield and its
job is recorded (docs/LEGAL_AND_LICENSING.md). Two checks keep it that way (issues #400, #439):

1. Provenance, over all of assets/ (every folder, however deep): every graphic file has a row
   in the `higgsfield-jobs.tsv` beside it (first column = the file name without its
   extension) or a row in tools/check_assets_allow.tsv. Graphic files are those in GRAPHIC_EXT
   (svg, png, jpg, jpeg, webp, gif, bmp, tif, tiff, avif, ico, icns, rgba); text files
   (TEXT_EXT: md, tsv, txt) are skipped; any other extension fails until it is added to one of
   the two sets, so a new kind of graphic cannot slip past. A new folder with a graphic and no
   jobs row fails. Every jobs row needs a valid job id and either its file beside it or an
   allow-listed file made from it. The logo's two jobs (mark, lettering) must be in
   assets/brand/higgsfield-jobs.tsv; nothing but the app icon files (openstitch*) may sit in
   assets/icons; assets/icons/commands and assets/illustrations hold SVGs only.
2. Source scan of crates/openstitch-app/src: no emoji or other pictographic character in an
   egui button label (a lone "x" or "×" counts too), no painter-drawn icon function, and every
   `icon_button` draws through `crate::icons`. icons.rs and art.rs are exempt: they render the
   Higgsfield SVGs and keep a code-drawn fallback for when one cannot be rendered.

tools/check_assets_allow.tsv (path, provenance, reason) lists the graphics without a jobs row
of their own. Provenance `jobs:assets/<dir>/higgsfield-jobs.tsv#name[+name]` means our own
tools made the file from those Higgsfield jobs (the rows must exist with valid ids); `note`
means the job id is not recorded yet, or the file is third-party with its LEGAL entry, and the
reason says how the row goes away. Existing source-scan cases are listed, with the issue that
removes each, in tools/check_icons_allow.tsv (path, label or function, issue). An entry in
either list that no longer applies (file gone, file now has its own jobs row, case no longer
occurs) fails too, so both lists only shrink.

    python3 tools/check_icons.py            # both checks
    python3 tools/check_icons.py --self-test
"""

import pathlib
import re
import sys
import tempfile
import unicodedata

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "crates/openstitch-app/src"
ALLOW = ROOT / "tools/check_icons_allow.tsv"
ASSET_ALLOW = ROOT / "tools/check_assets_allow.tsv"
JOBS = "higgsfield-jobs.tsv"
GRAPHIC_EXT = {
    ".svg", ".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".tif", ".tiff", ".avif",
    ".ico", ".icns", ".rgba",
}
TEXT_EXT = {".md", ".tsv", ".txt"}
JOB = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")

# Text marks a label may use: ellipsis, middle dot, degrees, dashes, quotes, powers, minus,
# plus-minus and the multiplication sign between words ("3 × 4").
TYPOGRAPHIC = set("…·°—–“”‘’²³−±×")
LABEL_CALL = re.compile(
    r"\b(\w*button|Button::new|Button::selectable|selectable_label|selectable_value|"
    r"SelectableLabel::new|checkbox|toggle_value|radio_value|radio)\s*\("
)
ICON_FN = re.compile(r"\bfn\s+(\w*(?:glyph|icon|chevron|arrow|tick|cross|close|caret)\w*)\s*[<(]", re.I)
EXEMPT = {"icons.rs", "art.rs"}


def strip_comments(src):
    """Blanks `//` comments, keeping string literals and line numbers."""
    out, i, n = [], 0, len(src)
    while i < n:
        c = src[i]
        if c == '"' or (c == "r" and re.match(r'r#*"', src[i:])):
            j = string_end(src, i)
            out.append(src[i:j])
            i = j
        elif src.startswith("//", i):
            j = src.find("\n", i)
            j = n if j < 0 else j
            i = j
        elif c == "'" and re.match(r"'(\\.|[^\\'])'", src[i:]):
            m = re.match(r"'(\\.|[^\\'])'", src[i:])
            out.append(m.group(0))
            i += len(m.group(0))
        else:
            out.append(c)
            i += 1
    return "".join(out)


def string_end(src, i):
    """The index just past the string literal starting at `i`."""
    m = re.match(r'r(#*)"', src[i:])
    if m:
        end = src.find('"' + m.group(1), i + len(m.group(0)))
        return len(src) if end < 0 else end + 1 + len(m.group(1))
    j = i + 1
    while j < len(src) and src[j] != '"':
        j += 2 if src[j] == "\\" else 1
    return j + 1


def call_args(src, open_paren):
    """The text between the parenthesis at `open_paren` and its match."""
    depth, j = 0, open_paren
    while j < len(src):
        c = src[j]
        if c == '"' or (c == "r" and re.match(r'r#*"', src[j:])):
            j = string_end(src, j)
            continue
        if c in "([{":
            depth += 1
        elif c in ")]}":
            depth -= 1
            if depth == 0:
                return src[open_paren + 1 : j]
        j += 1
    return src[open_paren + 1 :]


def literals(args):
    out, j = [], 0
    while j < len(args):
        if args[j] == '"' or (args[j] == "r" and re.match(r'r#*"', args[j:]) and (j == 0 or not args[j - 1].isalnum())):
            k = string_end(args, j)
            body = args[j:k]
            out.append(re.sub(r'^r?#*"|"#*$', "", body))
            j = k
        else:
            j += 1
    return out


def pictographic(label):
    """Why `label` is drawn like an icon, or None."""
    words = re.sub(r"\{[^}]*\}", "", label).strip()
    if words in ("x", "X", "×"):
        return "a lone cross"
    for ch in label:
        if ord(ch) < 128 or ch in TYPOGRAPHIC or unicodedata.category(ch)[0] in "LNZ":
            continue
        if unicodedata.category(ch) in ("Mn", "Mc", "Cf"):  # variation selectors, joiners
            continue
        return f"U+{ord(ch):04X} {unicodedata.name(ch, '?')}"
    return None


def scan_source(path, src):
    """(line, kind, key, why) for every icon that is not a Higgsfield one in `src`."""
    src = strip_comments(src)
    line = lambda pos: src.count("\n", 0, pos) + 1
    found = []
    for m in LABEL_CALL.finditer(src):
        if src[: m.start()].rstrip().endswith("fn"):
            continue
        args = call_args(src, m.end() - 1)
        if m.group(1) == "icon_button":
            if "icons::" not in args:
                found.append((line(m.start()), "icon_button", "icon_button", "draws without crate::icons"))
            continue
        for lit in literals(args):
            why = pictographic(lit)
            if why:
                found.append((line(m.start()), "label", lit, why))
    if path.name not in EXEMPT:
        for m in ICON_FN.finditer(src):
            head = src[m.start() : src.find("{", m.end())]
            # A helper taking a drawing closure (`impl FnOnce(&Painter, ..)`) draws no icon itself.
            if "Painter" in re.sub(r"\bFn(Once|Mut)?\([^)]*\)", "", head):
                found.append((line(m.start()), "painter", m.group(1), "painter-drawn icon function"))
    return found


def check_source(allow):
    errors, seen = [], set()
    for path in sorted(SRC.rglob("*.rs")):
        rel = path.relative_to(ROOT).as_posix()
        for ln, kind, key, why in scan_source(path, path.read_text(encoding="utf-8")):
            if (rel, key) in allow:
                seen.add((rel, key))
                continue
            errors.append(
                f"{rel}:{ln}: {kind} {key!r}: {why}. Use a Higgsfield icon from assets/icons "
                "(assets/icons/commands/README.md), not an emoji, symbol or painter drawing."
            )
    for rel, key in sorted(set(allow) - seen):
        errors.append(f"{ALLOW.relative_to(ROOT)}: {rel} {key!r} no longer occurs; remove its row")
    return errors


def jobs(tsv):
    rows = [l.split("\t") for l in tsv.read_text(encoding="utf-8").splitlines()[1:] if l.strip()]
    return {r[0]: (r[1] if len(r) > 1 else "") for r in rows}


def load_asset_allow(path):
    """{asset path: (provenance, reason)} from tools/check_assets_allow.tsv."""
    allow = {}
    if path.exists():
        for l in path.read_text(encoding="utf-8").splitlines()[1:]:
            if l.strip():
                rel, prov, reason = l.split("\t")
                allow[rel] = (prov, reason)
    return allow


def check_assets(root, allow, allow_name="tools/check_assets_allow.tsv"):
    """Every graphic anywhere under root/assets has a jobs row beside it or an allow entry."""
    errors, used, sources, stems, tables = [], set(), set(), {}, {}
    assets = root / "assets"
    for tsv in sorted(assets.rglob(JOBS)):
        d = tsv.parent.relative_to(root).as_posix()
        tables[d] = jobs(tsv)
        for name, job in sorted(tables[d].items()):
            if not JOB.match(job):
                errors.append(f"{d}/{JOBS}: {name}: {job!r} is not a job id")
    for rel, (prov, _) in sorted(allow.items()):
        if not prov.startswith("jobs:"):
            if prov != "note":
                errors.append(f"{allow_name}: {rel}: provenance {prov!r} is neither jobs:... nor note")
            continue
        tsv, _, names = prov[len("jobs:") :].partition("#")
        d = tsv.rsplit("/", 1)[0]
        if not tsv.endswith("/" + JOBS) or d not in tables or not names:
            errors.append(f"{allow_name}: {rel}: {prov!r} names no {JOBS} under assets/")
            continue
        for name in names.split("+"):
            if name not in tables[d]:
                errors.append(f"{allow_name}: {rel}: {tsv} has no row {name!r}")
            sources.add((d, name))
    for f in sorted(f for f in assets.rglob("*") if f.is_file()):
        rel = f.relative_to(root).as_posix()
        d = f.parent.relative_to(root).as_posix()
        ext = f.suffix.lower()
        if ext in TEXT_EXT:
            continue
        if ext not in GRAPHIC_EXT:
            errors.append(
                f"{rel}: unknown file type {ext or '(none)'}; add it to GRAPHIC_EXT (and record "
                "its Higgsfield job) or to TEXT_EXT in tools/check_icons.py"
            )
            continue
        stems.setdefault(d, set()).add(f.stem)
        own = f.stem in tables.get(d, {})
        if rel in allow:
            used.add(rel)
            if own:
                errors.append(f"{allow_name}: {rel} now has its own row in {d}/{JOBS}; remove its allow row")
        elif not own:
            errors.append(
                f"{rel}: no Higgsfield job in {d}/{JOBS} and no row in {allow_name}. Generate every "
                "graphic with Higgsfield and record its job beside the file"
            )
    for rel in sorted(set(allow) - used):
        errors.append(f"{allow_name}: {rel} no longer exists; remove its row")
    for d, rows in sorted(tables.items()):
        for name in sorted(rows):
            if name not in stems.get(d, set()) and (d, name) not in sources:
                errors.append(f"{d}/{JOBS}: {name} has no graphic beside it and no allow-listed file made from it")
    return errors


def check_provenance():
    errors = check_assets(ROOT, load_asset_allow(ASSET_ALLOW))
    for d in ("assets/icons/commands", "assets/illustrations"):
        for f in sorted((ROOT / d).iterdir()):
            if f.suffix.lower() in GRAPHIC_EXT and f.suffix.lower() != ".svg":
                errors.append(f"{d}/{f.name}: this folder holds SVGs only")
    brand = jobs(ROOT / "assets/brand" / JOBS)
    for part in ("mark", "lettering"):
        if not JOB.match(brand.get(part, "")):
            errors.append(f"assets/brand/{JOBS}: no job for the logo {part}")
    for f in sorted((ROOT / "assets/icons").iterdir()):
        if f.is_file() and not f.name.startswith("openstitch"):
            errors.append(f"assets/icons/{f.name}: icons go in assets/icons/commands with a job row")
    return errors


def load_allow():
    allow = {}
    for l in ALLOW.read_text(encoding="utf-8").splitlines()[1:]:
        if l.strip():
            path, key, issue = l.split("\t")
            allow[(path, key)] = issue
    return allow


def self_test():
    bad = {
        'ui.button("📎")': "U+1F4CE",
        'ui.small_button("×")': "lone cross",
        'ui.add(egui::Button::new("↑").small())': "U+2191",
        'ui.selectable_label(on, format!("📁 {}", name))': "U+1F4C1",
        'ui.small_button(\n    format!("⚙ {x}"),\n)': "U+2699",
        'icon_button(ui, "Close", close_glyph)': "without crate::icons",
        "fn close_glyph(p: &egui::Painter, r: Rect) {}": "painter-drawn",
    }
    good = [
        'ui.button("Print…")',
        'ui.button("Rotate −90°")',
        'ui.button(format!("{w} × {h} mm"))',
        'ui.checkbox(&mut f.holes, "Create an appliqué")',
        'icon_button(ui, "Close", |p, r, c| crate::icons::paint_ui(p, r, "close", c))',
        "// ui.button(\"📎\")",
        "fn draw_page(painter: &egui::Painter) {}",
        "fn icon_button(ui: &mut Ui, draw: impl FnOnce(&egui::Painter, Rect)) -> Response {}",
    ]
    fails = []
    for src, want in bad.items():
        got = scan_source(pathlib.Path("x.rs"), src)
        if not got or want not in got[0][3]:
            fails.append(f"missed {src!r}: {got}")
    for src in good:
        got = scan_source(pathlib.Path("x.rs"), src)
        if got:
            fails.append(f"flagged {src!r}: {got}")
    return fails + self_test_assets()


def self_test_assets():
    """The assets/ provenance rule on a throwaway tree."""
    job = "05366b9a-20ee-4131-b48b-cabadc10af97"
    head = "name\thiggsfield_job_id\n"
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)

        def put(rel, text=""):
            (root / rel).parent.mkdir(parents=True, exist_ok=True)
            (root / rel).write_text(text, encoding="utf-8")

        def expect(what, allow, want):
            got = check_assets(root, allow)
            if want is None and got:
                fails.append(f"assets self-test {what}: flagged {got}")
            elif want is not None and not any(want in e for e in got):
                fails.append(f"assets self-test {what}: missed {want!r}: {got}")

        star = "assets/stickers/new/star.png"
        put("assets/brand/" + JOBS, f"{head}mark\t{job}\n")
        put("assets/brand/mark.svg")
        put("assets/brand/README.md")
        expect("recorded graphic", {}, None)
        put(star)
        expect("graphic in a new folder", {}, f"{star}: no Higgsfield job")
        expect("allow-listed graphic", {star: ("note", "t")}, None)
        put("assets/stickers/new/star.xcf")
        expect("unknown file type", {star: ("note", "t")}, "unknown file type .xcf")
        (root / "assets/stickers/new/star.xcf").unlink()
        (root / star).unlink()
        expect("stale allow entry", {star: ("note", "t")}, f"{star} no longer exists")
        put("assets/brand/mark-64.png")
        derived = {"assets/brand/mark-64.png": ("jobs:assets/brand/" + JOBS + "#mark", "t")}
        expect("derived graphic", derived, None)
        wrong = {"assets/brand/mark-64.png": ("jobs:assets/brand/" + JOBS + "#lettering", "t")}
        expect("derived from a missing row", wrong, "has no row 'lettering'")
        put("assets/brand/" + JOBS, f"{head}mark\t{job}\nmark-64\t{job}\n")
        expect("allow entry made redundant", derived, "now has its own row")
        put("assets/brand/" + JOBS, f"{head}mark\t{job}\ngone\t{job}\n")
        expect("jobs row without its file", {}, "gone has no graphic beside it")
        put("assets/brand/" + JOBS, f"{head}mark\tnot-a-job\n")
        expect("bad job id", {}, "is not a job id")
    return fails


def main():
    if "--self-test" in sys.argv:
        errors = self_test()
    else:
        errors = self_test() + check_provenance() + check_source(load_allow())
    for e in errors:
        print(e)
    print(f"check_icons: {len(errors)} problem(s)")
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
