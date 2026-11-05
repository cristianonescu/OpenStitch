#!/usr/bin/env python3
"""Fails when the installers' file associations for .osd and .pes are missing or disagree.

The installers register two file types (issue #356):

- .osd, OpenStitch's own project format: opened by OpenStitch by default.
- .pes, the Brother embroidery format: offered under "Open with" only, so an installed
  PE-DESIGN or other handler keeps its place as the default.

They are declared in three sources that CI never builds together:

1. packaging/windows/openstitch.wxs (the MSI): the OpenStitch.Design ProgId with the osd
   extension, its content type and an open verb passing "%1"; the OpenStitch.PES ProgId keys
   (description, DefaultIcon, shell\\open\\command running openstitch.exe with "%1"); .pes listed
   under OpenWithProgids and never made the default (no .pes Extension element, no default
   value on Software\\Classes\\.pes); both extensions under Applications\\openstitch.exe\\
   SupportedTypes; every component holding them in a Feature; every [#File] reference real.
2. packaging/macos/Info.plist (the .app): a document type owning the design UTI and an
   Alternate-rank document type for the PES UTI; the design UTI exported with extension osd and
   the PES UTI imported (never exported) with extension pes; both UTIs under the bundle id.
3. packaging/linux/openstitch-mime.xml: *.osd maps to the same content type as the MSI's.

The type names must agree across the MSI and the .app.

    python3 tools/check_file_associations.py
    python3 tools/check_file_associations.py --self-test
"""

import pathlib
import plistlib
import re
import sys
import xml.etree.ElementTree as ET

ROOT = pathlib.Path(__file__).resolve().parent.parent
WXS = ROOT / "packaging/windows/openstitch.wxs"
PLIST = ROOT / "packaging/macos/Info.plist"
MIME = ROOT / "packaging/linux/openstitch-mime.xml"

WIX_NS = "{http://schemas.microsoft.com/wix/2006/wi}"
MIME_NS = "{http://www.freedesktop.org/standards/shared-mime-info}"

DESIGN_PROGID = "OpenStitch.Design"
PES_PROGID = "OpenStitch.PES"
DESIGN_NAME = "OpenStitch Design"
PES_NAME = "PES Embroidery File"
SUPPORTED = r"Software\Classes\Applications\openstitch.exe\SupportedTypes"
CLASSES = "Software\\Classes\\"


def check_wxs(text):
    """The MSI's associations. Returns (errors, facts) where facts feed the cross checks."""
    errors, facts = [], {}
    try:
        root = ET.fromstring(text)
    except ET.ParseError as e:
        return [f"openstitch.wxs: not XML: {e}"], facts
    w = lambda tag: WIX_NS + tag  # noqa: E731
    files = {f.get("Id") for f in root.iter(w("File"))}
    components = {}
    for c in root.iter(w("Component")):
        components[c.get("Id")] = c
    referenced = {r.get("Id") for f in root.iter(w("Feature")) for r in f.iter(w("ComponentRef"))}

    def owner(el):
        for cid, c in components.items():
            if any(x is el for x in c.iter()):
                return cid
        return None

    def needs_feature(el, what):
        cid = owner(el)
        if cid is None:
            errors.append(f"openstitch.wxs: {what} is outside any Component")
        elif cid not in referenced:
            errors.append(f"openstitch.wxs: {what} is in Component {cid}, which no Feature installs")

    # .osd: the OpenStitch.Design ProgId owns the extension.
    progids = {p.get("Id"): p for p in root.iter(w("ProgId"))}
    design = progids.get(DESIGN_PROGID)
    if design is None:
        errors.append(f"openstitch.wxs: no ProgId {DESIGN_PROGID} (the .osd association)")
    else:
        facts["design_name"] = design.get("Description")
        if design.get("Description") != DESIGN_NAME:
            errors.append(
                f"openstitch.wxs: ProgId {DESIGN_PROGID} Description is "
                f"{design.get('Description')!r}, want {DESIGN_NAME!r}"
            )
        if not design.get("Icon"):
            errors.append(f"openstitch.wxs: ProgId {DESIGN_PROGID} has no Icon")
        exts = {e.get("Id"): e for e in design.iter(w("Extension"))}
        osd = exts.get("osd")
        if osd is None:
            errors.append(f"openstitch.wxs: ProgId {DESIGN_PROGID} has no Extension osd")
        else:
            facts["osd_content_type"] = osd.get("ContentType")
            if not osd.get("ContentType"):
                errors.append("openstitch.wxs: Extension osd has no ContentType")
            verbs = {v.get("Id"): v for v in osd.iter(w("Verb"))}
            verb = verbs.get("open")
            if verb is None:
                errors.append("openstitch.wxs: Extension osd has no open Verb")
            elif "%1" not in (verb.get("Argument") or ""):
                errors.append("openstitch.wxs: the osd open Verb does not pass the file (%1)")
        needs_feature(design, f"ProgId {DESIGN_PROGID}")
    for pid, p in progids.items():
        for e in p.iter(w("Extension")):
            if e.get("Id") == "pes":
                errors.append(
                    f"openstitch.wxs: ProgId {pid} claims Extension pes; .pes is offered under "
                    "Open with only, so the existing default handler is kept"
                )

    # Registry values, keyed by (key, name).
    values = {}
    for r in root.iter(w("RegistryValue")):
        if r.get("Root") != "HKLM":
            errors.append(f"openstitch.wxs: RegistryValue {r.get('Key')} is not under HKLM (a per-machine install)")
        values[(r.get("Key"), r.get("Name"))] = r

    def value(key, name=None, what=""):
        r = values.get((key, name))
        if r is None:
            label = key + (f" [{name}]" if name else "")
            errors.append(f"openstitch.wxs: no RegistryValue {label}{what}")
            return None
        needs_feature(r, f"RegistryValue {key}")
        return r.get("Value")

    for ext in (".osd", ".pes"):
        value(SUPPORTED, ext, " (Open with lists OpenStitch for it)")
    pes_key = CLASSES + PES_PROGID
    desc = value(pes_key, what=f" (the {PES_PROGID} ProgId's description)")
    facts["pes_name"] = desc
    if desc is not None and desc != PES_NAME:
        errors.append(f"openstitch.wxs: {pes_key} is {desc!r}, want {PES_NAME!r}")
    value(pes_key + "\\DefaultIcon", what=f" ({PES_PROGID}'s icon)")
    command = value(pes_key + "\\shell\\open\\command", what=f" ({PES_PROGID}'s open command)")
    if command is not None:
        if "%1" not in command:
            errors.append(f"openstitch.wxs: {PES_PROGID} open command does not pass the file (%1)")
        if not re.search(r"\[#\w+\]", command):
            errors.append(f"openstitch.wxs: {PES_PROGID} open command names no installed File ([#Id])")
    value(CLASSES + ".pes\\OpenWithProgids", PES_PROGID, " (.pes under Open with)")
    for (key, name), r in values.items():
        if key == CLASSES + ".pes" and name is None:
            errors.append(
                "openstitch.wxs: a default value on Software\\Classes\\.pes would take over the "
                "default .pes handler; .pes is Open with only"
            )
    # Every [#File] reference names a File in the package.
    for r in root.iter(w("RegistryValue")):
        for ref in re.findall(r"\[#(\w+)\]", r.get("Value") or ""):
            if ref not in files:
                errors.append(f"openstitch.wxs: RegistryValue {r.get('Key')} names [#{ref}], which is no File Id")
    return errors, facts


def check_plist(data):
    """The .app's associations. Returns (errors, facts)."""
    errors, facts = [], {}
    try:
        p = plistlib.loads(data)
    except Exception as e:  # plistlib raises several types on malformed input
        return [f"Info.plist: not a property list: {e}"], facts
    bundle = p.get("CFBundleIdentifier") or ""
    if not bundle:
        errors.append("Info.plist: no CFBundleIdentifier")
    design_uti, pes_uti = bundle + ".design", bundle + ".pes"

    def declared(key):
        out = {}
        for t in p.get(key) or []:
            spec = t.get("UTTypeTagSpecification") or {}
            exts = spec.get("public.filename-extension") or []
            if isinstance(exts, str):
                exts = [exts]
            out[t.get("UTTypeIdentifier")] = (t, [e.lower() for e in exts])
        return out

    exported = declared("UTExportedTypeDeclarations")
    imported = declared("UTImportedTypeDeclarations")
    if design_uti not in exported:
        errors.append(f"Info.plist: {design_uti} is not in UTExportedTypeDeclarations")
    elif "osd" not in exported[design_uti][1]:
        errors.append(f"Info.plist: exported {design_uti} has no filename extension osd")
    if pes_uti in exported:
        errors.append(f"Info.plist: {pes_uti} is exported; PES is Brother's format, so it is imported only")
    if pes_uti not in imported:
        errors.append(f"Info.plist: {pes_uti} is not in UTImportedTypeDeclarations")
    elif "pes" not in imported[pes_uti][1]:
        errors.append(f"Info.plist: imported {pes_uti} has no filename extension pes")
    for uti, (_, exts) in {**exported, **imported}.items():
        if not (uti or "").startswith(bundle + "."):
            errors.append(f"Info.plist: type {uti!r} is not under the bundle id {bundle!r}")
        if not exts:
            errors.append(f"Info.plist: type {uti!r} declares no filename extension")

    docs = {}
    for d in p.get("CFBundleDocumentTypes") or []:
        for uti in d.get("LSItemContentTypes") or []:
            docs[uti] = d
    for uti in docs:
        if uti not in exported and uti not in imported:
            errors.append(f"Info.plist: document type {uti} is declared in neither UT*TypeDeclarations")
    for uti, rank, what, key in (
        (design_uti, "Owner", ".osd", "design_name"),
        (pes_uti, "Alternate", ".pes (alternate document type)", "pes_name"),
    ):
        d = docs.get(uti)
        if d is None:
            errors.append(f"Info.plist: no CFBundleDocumentTypes entry for {uti} ({what})")
            continue
        facts[key] = d.get("CFBundleTypeName")
        if d.get("CFBundleTypeRole") not in ("Editor", "Viewer"):
            errors.append(f"Info.plist: document type {uti} role is {d.get('CFBundleTypeRole')!r}")
        if d.get("LSHandlerRank") != rank:
            errors.append(
                f"Info.plist: document type {uti} LSHandlerRank is {d.get('LSHandlerRank')!r}, want {rank!r}"
            )
    return errors, facts


def check_mime(text):
    """The Linux shared MIME types: glob -> type."""
    try:
        root = ET.fromstring(text)
    except ET.ParseError as e:
        return [f"openstitch-mime.xml: not XML: {e}"], {}
    globs = {}
    for t in root.iter(MIME_NS + "mime-type"):
        for g in t.iter(MIME_NS + "glob"):
            globs[g.get("pattern")] = t.get("type")
    errors = [f"openstitch-mime.xml: no glob {g}" for g in ("*.osd", "*.pes") if g not in globs]
    return errors, {"osd_content_type": globs.get("*.osd")}


def cross_check(wxs, plist, mime):
    errors = []
    for key, what in (("design_name", ".osd type name"), ("pes_name", ".pes type name")):
        a, b = wxs.get(key), plist.get(key)
        if a is not None and b is not None and a != b:
            errors.append(f"{what}: MSI says {a!r}, Info.plist says {b!r}")
    a, b = wxs.get("osd_content_type"), mime.get("osd_content_type")
    if a is not None and b is not None and a != b:
        errors.append(f".osd content type: MSI says {a!r}, openstitch-mime.xml says {b!r}")
    return errors


def check(wxs_text, plist_data, mime_text):
    e1, f1 = check_wxs(wxs_text)
    e2, f2 = check_plist(plist_data)
    e3, f3 = check_mime(mime_text)
    return e1 + e2 + e3 + cross_check(f1, f2, f3)


def sources():
    """The three sources, with CRLF checkouts read as LF."""
    def text(f):
        return f.read_text(encoding="utf-8").replace("\r\n", "\n")

    return text(WXS), text(PLIST).encode("utf-8"), text(MIME)


def self_test():
    """Breaks the real sources one way at a time; each break must be caught."""
    wxs, plist, mime = sources()
    fails = [f"self-test: the real sources fail: {e}" for e in check(wxs, plist, mime)]

    def sub(text, old, new):
        if old not in text:
            fails.append(f"self-test: {old!r} not found; update the self-test")
        return text.replace(old, new, 1)

    w = lambda old, new: (sub(wxs, old, new), plist, mime)  # noqa: E731
    p = lambda old, new: (wxs, sub(plist.decode(), old, new).encode(), mime)  # noqa: E731
    pes_cmd = r'Key="Software\Classes\OpenStitch.PES\shell\open\command"'
    cases = {
        "osd ProgId gone": (w('Id="OpenStitch.Design"', 'Id="OpenStitch.Other"'), "no ProgId OpenStitch.Design"),
        "osd extension gone": (w('Extension Id="osd"', 'Extension Id="osx"'), "has no Extension osd"),
        "osd verb without %1": (w('Argument="&quot;%1&quot;"', 'Argument=""'), "osd open Verb does not pass"),
        "pes made the default": (
            w('<Extension Id="osd"', '<Extension Id="pes" /><Extension Id="osd"'),
            "claims Extension pes",
        ),
        "pes Open with gone": (w('Name="OpenStitch.PES"', 'Name="OpenStitch.X"'), "(.pes under Open with)"),
        "pes SupportedTypes gone": (w('Name=".pes"', 'Name=".pez"'), "[.pes]"),
        "osd SupportedTypes gone": (w('Name=".osd"', 'Name=".osx"'), "[.osd]"),
        "pes command gone": (w(pes_cmd, pes_cmd.replace("command", "cmd")), "open command)"),
        "pes command bad File": (w("[#OpenStitchExe]", "[#Missing]"), "[#Missing], which is no File Id"),
        "pes component not installed": (
            w('<ComponentRef Id="PesAssociation" />', ""),
            "PesAssociation, which no Feature installs",
        ),
        "pes name differs": (w('Value="PES Embroidery File"', 'Value="PES File"'), "want 'PES Embroidery File'"),
        "pes alternate gone": (
            p("<string>Alternate</string>", "<string>Owner</string>"),
            "want 'Alternate'",
        ),
        "pes document type gone": (
            p(
                "<array>\n        <string>io.github.cristianonescu.openstitch.pes</string>\n      </array>\n    </dict>\n  </array>",
                "<array>\n      </array>\n    </dict>\n  </array>",
            ),
            "no CFBundleDocumentTypes entry for io.github.cristianonescu.openstitch.pes",
        ),
        "osd exported extension gone": (p("<string>osd</string>", "<string>osx</string>"), "no filename extension osd"),
        "pes imported extension gone": (p("<string>pes</string>", "<string>pez</string>"), "no filename extension pes"),
        "pes import gone": (
            p("<key>UTImportedTypeDeclarations</key>", "<key>UTImportedTypeDeclarations2</key>"),
            "is not in UTImportedTypeDeclarations",
        ),
        "pes exported": (
            p("<key>UTImportedTypeDeclarations</key>", "<key>UTExportedTypeDeclarations</key>"),
            "openstitch.pes is exported",
        ),
        "osd owner rank lost": (p("<string>Owner</string>", "<string>Default</string>"), "want 'Owner'"),
        "mac type name differs": (p("<string>OpenStitch Design</string>", "<string>OpenStitch Project</string>"), ".osd type name"),
        "mime content type differs": (
            (wxs, plist, sub(mime, 'type="application/x-openstitch-design"', 'type="application/x-osd"')),
            ".osd content type",
        ),
        "broken xml": ((wxs[:-20], plist, mime), "not XML"),
    }
    for what, ((a, b, c), want) in cases.items():
        got = check(a, b, c)
        if not any(want in e for e in got):
            fails.append(f"self-test {what}: missed {want!r}: {got}")
    return fails


def main():
    if "--self-test" in sys.argv:
        errors = self_test()
    else:
        errors = self_test() + check(*sources())
    for e in errors:
        print(e, file=sys.stderr)
    if errors:
        print(f"check_file_associations: {len(errors)} problem(s)", file=sys.stderr)
        return 1
    print("check_file_associations: .osd and .pes associations agree in the MSI, the .app and the MIME types")
    return 0


if __name__ == "__main__":
    sys.exit(main())
