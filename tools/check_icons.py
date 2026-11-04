#!/usr/bin/env python3
"""Fails when an icon or graphic could have come from somewhere other than Higgsfield.

Every icon, illustration and graphic in OpenStitch is generated with Higgsfield and its job is
recorded (docs/LEGAL_AND_LICENSING.md). Two checks keep it that way (issue #400):

1. Provenance: every SVG in assets/icons/commands and assets/illustrations has a row in the
   `higgsfield-jobs.tsv` beside it, and every row has its file; the app icon files
   (assets/icons/openstitch*) come from the logo, whose two jobs are in
   assets/brand/higgsfield-jobs.tsv. Nothing else may sit in assets/icons.
2. Source scan of crates/openstitch-app/src: no emoji or other pictographic character in an
   egui button label (a lone "x" or "×" counts too), no painter-drawn icon function, and every
   `icon_button` draws through `crate::icons`. icons.rs and art.rs are exempt: they render the
   Higgsfield SVGs and keep a code-drawn fallback for when one cannot be rendered.

Existing cases are listed, with the issue that removes each, in tools/check_icons_allow.tsv
(path, label or function, issue). A listed case that no longer occurs fails too, so the list
only shrinks.

    python3 tools/check_icons.py            # both checks
    python3 tools/check_icons.py --self-test
"""

import pathlib
import re
import sys
import unicodedata

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "crates/openstitch-app/src"
ALLOW = ROOT / "tools/check_icons_allow.tsv"
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
    return {r[0]: r[1] for r in rows}


def check_provenance():
    errors = []
    for d in ("assets/icons/commands", "assets/illustrations"):
        folder = ROOT / d
        recorded = jobs(folder / "higgsfield-jobs.tsv")
        files = {f.stem for f in folder.glob("*.svg")}
        for name in sorted(files - set(recorded)):
            errors.append(f"{d}/{name}.svg: no Higgsfield job in {d}/higgsfield-jobs.tsv")
        for name in sorted(set(recorded) - files):
            errors.append(f"{d}/higgsfield-jobs.tsv: {name} has no {name}.svg")
        for name, job in sorted(recorded.items()):
            if not JOB.match(job):
                errors.append(f"{d}/higgsfield-jobs.tsv: {name}: {job!r} is not a job id")
        for f in sorted(folder.iterdir()):
            if f.suffix.lower() in (".png", ".jpg", ".jpeg", ".webp", ".gif", ".ico", ".bmp"):
                errors.append(f"{d}/{f.name}: raster graphic without a recorded job")
    brand = jobs(ROOT / "assets/brand/higgsfield-jobs.tsv")
    for part in ("mark", "lettering"):
        if not JOB.match(brand.get(part, "")):
            errors.append(f"assets/brand/higgsfield-jobs.tsv: no job for the logo {part}")
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
