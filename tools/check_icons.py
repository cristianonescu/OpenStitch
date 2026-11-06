#!/usr/bin/env python3
"""Fails when an icon or graphic could have come from somewhere other than Higgsfield.

Every icon, illustration, texture and graphic in OpenStitch is generated with Higgsfield and its
job is recorded (docs/LEGAL_AND_LICENSING.md). Three checks keep it that way (issues #400, #439, #654):

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
2. Provenance outside assets/ (#654): every image file git tracks outside assets/ matches
   exactly one row of tools/graphics_provenance.tsv (pattern, kind, source, note). Kinds:
   `higgsfield` (every file has a row with a job id in the source table, such as
   examples/images/PROVENANCE.md, or is made in code from one, and every row has its file),
   `generated` (the source tool exists and names the files or their folder), `test-input`
   (written by hand for a test; the source that uses it names it) and `evidence` (the owner's
   sew-out scans). A row that matches nothing fails, so the table only shrinks with the files.
3. Source scan of crates/openstitch-app/src: no emoji or other pictographic character shown
   as text anywhere a control, label, combo box, collapsing header, rich text, hover text or
   notice shows it, or in a symbol-only string literal that reaches one through a variable
   (#654). Text marks are allowed: TYPOGRAPHIC everywhere, IN_WORDS ("▸", "→") beside words
   or placeholders; a lone "x" or "×" on a button and a lone ASCII mark ("::", ">|") on a
   control are icons. No painter-drawn icon function (taking a Painter, or painting through
   `ui.painter()`), no icon-sized shape painted inline (points that are all small fixed offsets
   from one point; CANVAS_MARKS lists the design-canvas marks that are positions, not icons),
   and every `icon_button` draws through `crate::icons`. Test modules are skipped. icons.rs and
   art.rs are exempt: they render the Higgsfield SVGs and keep a code-drawn fallback for when
   one cannot be rendered.

tools/check_assets_allow.tsv (path, provenance, reason) lists the graphics without a jobs row
of their own. Provenance `jobs:assets/<dir>/higgsfield-jobs.tsv#name[+name]` means our own
tools made the file from those Higgsfield jobs (the rows must exist with valid ids); `note`
means the job id is not recorded yet, or the file is third-party with its LEGAL entry, and the
reason says how the row goes away. Existing source-scan cases are listed, with the issue that
removes each, in tools/check_icons_allow.tsv (path, label or function, issue). An entry in
either list that no longer applies (file gone, file now has its own jobs row, case no longer
occurs) fails too, so both lists only shrink.

    python3 tools/check_icons.py            # all three checks
    python3 tools/check_icons.py --self-test
"""

import fnmatch
import pathlib
import re
import subprocess
import sys
import tempfile
import unicodedata

ROOT = pathlib.Path(__file__).resolve().parent.parent
SRC = ROOT / "crates/openstitch-app/src"
ALLOW = ROOT / "tools/check_icons_allow.tsv"
ASSET_ALLOW = ROOT / "tools/check_assets_allow.tsv"
GRAPHICS = ROOT / "tools/graphics_provenance.tsv"
JOBS = "higgsfield-jobs.tsv"
GRAPHIC_EXT = {
    ".svg", ".png", ".jpg", ".jpeg", ".webp", ".gif", ".bmp", ".tif", ".tiff", ".avif",
    ".ico", ".icns", ".rgba",
}
TEXT_EXT = {".md", ".tsv", ".txt"}
JOB = re.compile(r"^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$")

# Text marks a label may use: ellipsis, middle dot, degrees, dashes, quotes, powers, minus,
# plus-minus, the multiplication sign between words ("3 × 4"), a list bullet and the dots that
# mask a saved key, and the foot and inch marks.
TYPOGRAPHIC = set("…·°—–“”‘’²³−±×•●′″")
# Marks that are text inside words (a menu path "View ▸ Show/Hide", "3 colours → 2") but an icon
# on their own (a lone "→" between swatches, #653).
IN_WORDS = set("▸→")
# Canvas marks: positions on the design drawn as PE-DESIGN draws them (trim marks, guide
# handles), not icons on a control. (path, key) of the inline glyphs that are such marks.
CANVAS_MARKS = {
    ("crates/openstitch-app/src/view_ui.rs", "inline draw_show"): "trim marks on the design",
    ("crates/openstitch-app/src/view_ui.rs", "inline draw_guide_markers"): "guide handles on the rulers",
}
# A string literal made of symbols only ("✓", "✗", "▾"), anywhere in the source: a symbol used
# as an icon, even when it reaches a label through a variable (#654).
LITERAL = re.compile(r'"((?:\\.|[^"\\])*)"')
LABEL_CALL = re.compile(
    r"\b(\w*button|Button::new|Button::selectable|selectable_label|selectable_value|"
    r"SelectableLabel::new|checkbox|toggle_value|radio_value|radio|"
    # Text shown anywhere else (#654): combo boxes, collapsing headers, labels, rich text,
    # hover and tooltip text, headings and notices.
    r"selected_text|CollapsingHeader::new|collapsing|label|Label::new|colored_label|"
    r"RichText::new|on_hover_text|on_disabled_hover_text|on_hover_text_at_pointer|"
    r"show_tooltip_text|heading|small|strong|weak|monospace|hyperlink_to|\w*[Nn]otice\w*)\s*\("
)
# A glyph drawn in place: three or more points (or a line's two) that are all a fixed small
# offset from one point, `c + vec2(-3.5, -2.0)`, so the shape is an icon-sized drawing (#654).
OFFSET = re.compile(r"\w+\s*\+\s*(?:egui::)?vec2\(\s*(-?[\d.]+)\s*,\s*(-?[\d.]+)\s*\)")
GLYPH_SIZE = 12.0
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


def pictographic(label, button=True, ascii=True):
    """Why `label` is drawn like an icon, or None. On a button a lone "x" or "×" is a close
    icon; in a label it is the sign between two fields ("w × h")."""
    words = re.sub(r"\{[^}]*\}", "", label).strip()
    if button and words in ("x", "X", "×"):
        return "a lone cross"
    # ASCII marks standing alone on a control ("::" as a drag handle, ">|" as a player button),
    # not text (#654); separators and format strings are text.
    if (
        ascii
        and "{" not in label
        and len(words) <= 3
        and re.fullmatch(r"[!-/:-@\[-`|~]+", words or " ")
        and words not in ("-", "+", "%", "/", ",", ".", ";", ":", "?", "()")
    ):
        return f"the mark {words!r} on its own"
    # Words, or a format string's placeholders ("{} ▸ {group}": a menu path being built).
    has_words = bool(re.search(r"[^\W\d_]{2,}", words)) or "{" in label
    for ch in label:
        if ord(ch) < 128 or ch in TYPOGRAPHIC or unicodedata.category(ch)[0] in "LNZ":
            continue
        if ch in IN_WORDS and has_words:
            continue
        if unicodedata.category(ch) in ("Mn", "Mc", "Cf"):  # variation selectors, joiners
            continue
        return f"U+{ord(ch):04X} {unicodedata.name(ch, '?')}"
    return None


def scan_source(path, src):
    """(line, kind, key, why) for every icon that is not a Higgsfield one in `src`."""
    src = strip_comments(src)
    # Test modules may hold old labels on purpose (a test that the old rows clashed).
    tests = src.find("#[cfg(test)]")
    if tests >= 0:
        src = src[:tests]
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
        button = "button" in m.group(1).lower() or m.group(1) in ("Button::new", "Button::selectable")
        for lit in literals(args):
            why = pictographic(lit, button)
            hit = (line(m.start()), "label", lit, why)
            if why and hit not in found:  # a RichText inside a label is one label
                found.append(hit)
    if path.name not in EXEMPT:
        labelled = {key for _, kind, key, _ in found if kind == "label"}
        for m in LITERAL.finditer(src):
            lit = m.group(1)
            bare = re.sub(r"\{[^}]*\}|\s", "", lit)
            if not bare or re.search(r"\w", bare) or lit in labelled:
                continue
            why = pictographic(lit, button=False, ascii=False)
            if why:
                labelled.add(lit)
                found.append((line(m.start()), "symbol", lit, why + " used as an icon"))
        for m in ICON_FN.finditer(src):
            brace = src.find("{", m.end())
            head = src[m.start() : brace]
            body = call_args(src, brace) if brace >= 0 else ""
            # A helper taking a drawing closure (`impl FnOnce(&Painter, ..)`) draws no icon itself;
            # one that paints, through a `Painter` parameter or `ui.painter()` (#654), does.
            delegates = re.search(r"\bFn(Once|Mut)?\([^)]*Painter", head)
            takes = "Painter" in re.sub(r"\bFn(Once|Mut)?\([^)]*\)", "", head)
            paints = re.search(r"\bpainter\(\)|\bShape::|\.(line_segment|circle\w*|rect_\w+)\(", body)
            if takes or (paints and not delegates and "icons::" not in body):
                found.append((line(m.start()), "painter", m.group(1), "painter-drawn icon function"))
        for start, key in inline_glyphs(src):
            found.append((line(start), "inline", key, "icon-sized shape painted in place"))
    return found


def enclosing_fn(src, pos):
    """The name of the last `fn` before `pos` (the function a glyph is painted in)."""
    names = re.findall(r"\bfn\s+(\w+)", src[:pos])
    return names[-1] if names else "(top level)"


def inline_glyphs(src):
    """(position, key) of each list of points that are all small fixed offsets from a point:
    `vec![c + vec2(-3.5, -2.0), c + vec2(3.5, -2.0), c + vec2(0.0, 2.0)]` (a caret) or
    `line_segment([x + vec2(-2.5, -2.5), x + vec2(2.5, 2.5)], ..)` (half a cross). The key is
    `inline <enclosing fn>`, so allow rows survive moved lines."""
    out, keys = [], set()
    for m in re.finditer(r"(vec!\s*\[|line_segment\(\s*\[)", src):
        items = call_args(src, m.end() - 1)
        parts = [p.strip() for p in re.split(r",(?![^()]*\))", items) if p.strip()]
        need = 2 if m.group(1).startswith("line_segment") else 3
        if len(parts) < need:
            continue
        offs = [OFFSET.fullmatch(p) for p in parts]
        if all(offs) and all(abs(float(v)) <= GLYPH_SIZE for o in offs for v in o.groups()):
            key = f"inline {enclosing_fn(src, m.start())}"
            if key not in keys:
                keys.add(key)
                out.append((m.start(), key))
    return out


def check_source(allow):
    errors, seen = [], set()
    for path in sorted(SRC.rglob("*.rs")):
        rel = path.relative_to(ROOT).as_posix()
        for ln, kind, key, why in scan_source(path, path.read_text(encoding="utf-8")):
            if (rel, key) in CANVAS_MARKS:
                seen.add(("canvas", rel, key))
                continue
            if (rel, key) in allow:
                seen.add((rel, key))
                continue
            errors.append(
                f"{rel}:{ln}: {kind} {key!r}: {why}. Use a Higgsfield icon from assets/icons "
                "(assets/icons/commands/README.md), not an emoji, symbol or painter drawing."
            )
    for rel, key in sorted(set(allow) - seen):
        errors.append(f"{ALLOW.relative_to(ROOT)}: {rel} {key!r} no longer occurs; remove its row")
    for rel, key in sorted(CANVAS_MARKS):
        if ("canvas", rel, key) not in seen:
            errors.append(f"tools/check_icons.py CANVAS_MARKS: {rel} {key!r} no longer occurs; remove it")
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


def tracked_graphics(root):
    """Every graphic file of the repository outside assets/ (git's list when there is one, so
    build output and local worktrees are not counted)."""
    try:
        names = subprocess.run(
            ["git", "ls-files"], cwd=root, capture_output=True, text=True, check=True
        ).stdout.splitlines()
    except (OSError, subprocess.CalledProcessError):
        names = [
            p.relative_to(root).as_posix()
            for p in root.rglob("*")
            if p.is_file() and not any(part.startswith(".") or part == "target" for part in p.relative_to(root).parts)
        ]
    return sorted(
        n for n in names
        if pathlib.PurePosixPath(n).suffix.lower() in GRAPHIC_EXT and not n.startswith("assets/")
    )


def load_graphics_rules(path):
    """[(pattern, kind, source, note)] from tools/graphics_provenance.tsv."""
    rows = []
    for l in path.read_text(encoding="utf-8").splitlines()[1:]:
        if l.strip():
            pattern, kind, source, note = l.split("\t")
            rows.append((pattern, kind, source, note))
    return rows


def table_jobs(md):
    """{file name: job id or 'from <name>'} from a Markdown table of Higgsfield images: a row
    whose cells hold the file and a job id, or (images made in code) the file and the prefix of
    the image it was made from ("| d02-emblem-small.png | g01 | Scaled ... |")."""
    out = {}
    for l in md.read_text(encoding="utf-8").splitlines():
        cells = [c.strip() for c in l.strip().strip("|").split("|")] if l.startswith("|") else []
        if len(cells) < 2 or not re.fullmatch(r"[\w.-]+\.\w+", cells[0]):
            continue
        job = next((c for c in cells[1:] if JOB.match(c)), None)
        if job:
            out[cells[0]] = job
        elif re.fullmatch(r"[a-z]\d+", cells[1]):
            out[cells[0]] = "from " + cells[1]
    return out


def check_outside_assets(root, rules, files, rules_name="tools/graphics_provenance.tsv"):
    """Every graphic outside assets/ matches exactly one rule of tools/graphics_provenance.tsv,
    and the rule's record holds for it (#654): a Higgsfield job in its table, the tool that
    generates it, or a test input / evidence note naming what uses it."""
    errors, hits = [], {r[0]: [] for r in rules}
    for f in files:
        matched = [r for r in rules if fnmatch.fnmatchcase(f, r[0])]
        if not matched:
            errors.append(
                f"{f}: a graphic with no provenance. Generate it with Higgsfield and record the job "
                f"(a higgsfield-jobs.tsv or a provenance table), or record what it is in {rules_name}"
            )
        elif len(matched) > 1:
            errors.append(f"{f}: matches {len(matched)} rows of {rules_name}; make the patterns distinct")
        else:
            hits[matched[0][0]].append(f)
    for pattern, kind, source, note in rules:
        got = hits[pattern]
        if not got:
            errors.append(f"{rules_name}: {pattern} matches no graphic; remove its row")
            continue
        if kind not in ("higgsfield", "generated", "test-input", "evidence"):
            errors.append(f"{rules_name}: {pattern}: kind {kind!r} is not higgsfield, generated, test-input or evidence")
            continue
        if not note.strip():
            errors.append(f"{rules_name}: {pattern}: the note must say what the files are")
        if kind == "evidence":
            continue
        src = root / source
        if not src.is_file():
            errors.append(f"{rules_name}: {pattern}: source {source} does not exist")
            continue
        if kind == "higgsfield":
            table = table_jobs(src)
            folder = pathlib.PurePosixPath(source).parent
            for f in got:
                name = pathlib.PurePosixPath(f).name
                rec = table.get(name)
                if rec is None:
                    errors.append(f"{f}: no row with a Higgsfield job in {source}")
                elif rec.startswith("from ") and not any(
                    k.startswith(rec[5:] + "-") and JOB.match(v) for k, v in table.items()
                ):
                    errors.append(f"{f}: made from {rec[5:]!r}, which has no job row in {source}")
            for name in sorted(table):
                if (folder / name).as_posix() not in got:
                    errors.append(f"{source}: {name} has a row but no file")
        else:
            text = src.read_text(encoding="utf-8", errors="replace")
            names = {pathlib.PurePosixPath(f).name for f in got}
            folder = pathlib.PurePosixPath(pattern.rstrip("/*")).name
            if not any(n in text for n in names) and not (pattern.endswith("/*") and folder in text):
                errors.append(f"{rules_name}: {pattern}: {source} names none of its files")
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
        # #654: text shown outside buttons, symbols through variables, painting through ui.
        'ComboBox::from_id_salt("m").selected_text("▾")': "U+25BE",
        'egui::CollapsingHeader::new("✓ done")': "U+2713",
        'ui.collapsing("✗ failed", |ui| {})': "U+2717",
        'ui.label("→")': "U+2192",
        'ui.label(RichText::new("::").weak())': "'::' on its own",
        'resp.on_hover_text("Settings ⚙")': "U+2699",
        'Item::Notice("in the settings (⚙) first".into())': "U+2699",
        'ui.heading("★ Favourites")': "U+2605",
        'let s = if ok { "✓" } else { "✗" };': "used as an icon",
        'ui.add(egui::Button::new(">|"))': "'>|' on its own",
        "fn arrow(ui: &egui::Ui, c: Pos2) { ui.painter().circle_filled(c, 2.0, RED); }": "painter-drawn",
        "p.add(Shape::convex_polygon(vec![c + vec2(-3.5, -2.0), c + vec2(3.5, -2.0), c + vec2(0.0, 2.0)], RED, Stroke::NONE));": "painted in place",
        "painter.line_segment([x + egui::vec2(-2.5, -2.5), x + egui::vec2(2.5, 2.5)], s);": "painted in place",
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
        # #654: marks inside words and separators are text; computed shapes are not glyphs.
        'ui.label("Hide (View ▸ Show/Hide shows it again)")',
        'let path = format!("{} ▸ {group}", tab);',
        'ui.label(format!("{} colours → {}", a, b))',
        'ui.label("×")',
        'ui.label(", ")',
        'ui.label(RichText::new("●●●●  saved"))',
        'ui.label("•  {l}")',
        'ui.label(format!("{w}″"))',
        "fn icon_button(ui: &mut Ui, tip: &str, draw: impl FnOnce(&egui::Painter, Rect)) -> Response { ui.painter().rect(r, 0.0, c, s); }",
        "p.add(Shape::convex_polygon(vec![to, to - d + n, to - d - n], c, Stroke::NONE));",
        "p.add(Shape::convex_polygon(vec![c + vec2(-30.0, 0.0), c + vec2(30.0, 0.0), c + vec2(0.0, 40.0)], c, Stroke::NONE));",
        '#[cfg(test)]\nmod t { fn f(ui: &mut Ui) { ui.collapsing("✓ x", |_| {}); } }',
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
    return fails + self_test_assets() + self_test_outside()


def self_test_outside():
    """The provenance rule outside assets/ (#654) on a throwaway tree."""
    job = "05366b9a-20ee-4131-b48b-cabadc10af97"
    fails = []
    with tempfile.TemporaryDirectory() as tmp:
        root = pathlib.Path(tmp)

        def put(rel, text=""):
            (root / rel).parent.mkdir(parents=True, exist_ok=True)
            (root / rel).write_text(text, encoding="utf-8")

        put("pics/PROVENANCE.md", f"| File | Job |\n|---|---|\n| g01-a.png | {job} |\n\n| d02-b.png | g01 | scaled |\n")
        put("tools/gen.py", "writes renders/out.png")
        put("tests/t.rs", 'include_bytes!("leaf.svg")')
        rules = [
            ("pics/*", "higgsfield", "pics/PROVENANCE.md", "test images"),
            ("renders/*", "generated", "tools/gen.py", "renders"),
            ("fixtures/leaf.svg", "test-input", "tests/t.rs", "hand-written"),
            ("scans/*", "evidence", "-", "sew-out scans"),
        ]
        files = ["fixtures/leaf.svg", "pics/d02-b.png", "pics/g01-a.png", "renders/out.png", "scans/s.jpg"]

        def expect(what, rules, files, want):
            got = check_outside_assets(root, rules, files)
            if want is None and got:
                fails.append(f"outside self-test {what}: flagged {got}")
            elif want is not None and not any(want in e for e in got):
                fails.append(f"outside self-test {what}: missed {want!r}: {got}")

        expect("all recorded", rules, files, None)
        expect("a graphic with no rule", rules, files + ["docs/new/logo.png"], "docs/new/logo.png: a graphic with no provenance")
        expect("two rules", rules + [("pics/g01-a.png", "evidence", "-", "x")], files, "matches 2 rows")
        expect("stale rule", rules + [("old/*", "evidence", "-", "x")], files, "old/* matches no graphic")
        expect("unknown kind", [("pics/*", "drawn", "-", "x")] + rules[1:], files, "kind 'drawn'")
        expect("no job row", rules, files + ["pics/g02-c.png"], "pics/g02-c.png: no row with a Higgsfield job")
        put("pics/PROVENANCE.md", f"| File | Job |\n|---|---|\n| g01-a.png | {job} |\n| g03-gone.png | {job} |\n\n| d02-b.png | g01 | scaled |\n")
        expect("row without its file", rules, files, "g03-gone.png has a row but no file")
        put("pics/PROVENANCE.md", f"| File | Job |\n|---|---|\n| g01-a.png | {job} |\n\n| d02-b.png | g09 | scaled |\n")
        expect("made from an image with no job", rules, files, "made from 'g09'")
        put("pics/PROVENANCE.md", f"| File | Job |\n|---|---|\n| g01-a.png | {job} |\n\n| d02-b.png | g01 | scaled |\n")
        expect("generator gone", [rules[0], ("renders/*", "generated", "tools/none.py", "r")] + rules[2:], files, "tools/none.py does not exist")
        put("tests/t.rs", "nothing here")
        expect("source names none", rules, files, "tests/t.rs names none of its files")
    return fails


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
        errors = (
            self_test()
            + check_provenance()
            + check_outside_assets(ROOT, load_graphics_rules(GRAPHICS), tracked_graphics(ROOT))
            + check_source(load_allow())
        )
    for e in errors:
        print(e)
    print(f"check_icons: {len(errors)} problem(s)")
    sys.exit(1 if errors else 0)


if __name__ == "__main__":
    main()
