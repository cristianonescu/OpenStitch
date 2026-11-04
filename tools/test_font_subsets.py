"""Builds the test fonts in crates/openstitch-engine/tests/fixtures/fonts (#345).

Each is the Regular (wght 400, wdth 100) instance of a Noto variable font from google/fonts,
cut down to the characters the lettering tests set, with every OpenType layout feature kept and
hinting dropped. Provenance, checksums and licences: that folder's PROVENANCE.md.

Usage, with fontTools (tested with 4.60.1) and the google/fonts files downloaded under
<src>/<family folder>/ (for example <src>/notosanshebrew/NotoSansHebrew[wdth,wght].ttf):

    python3 -I tools/test_font_subsets.py <src> crates/openstitch-engine/tests/fixtures/fonts

The output is byte for byte the same on every run (the source fonts' timestamps are kept).
"""

import sys
from pathlib import Path

from fontTools import subset
from fontTools.ttLib import TTFont
from fontTools.varLib import instancer

# Output name: (source file under <src>, the characters kept).
JOBS = {
    "NotoSansHebrew-TestSubset.ttf": (
        "notosanshebrew/NotoSansHebrew[wdth,wght].ttf",
        "ab שלום",
    ),
    "NotoSansArabic-TestSubset.ttf": (
        "notosansarabic/NotoSansArabic[wdth,wght].ttf",
        "ab بسلام",
    ),
    "NotoSansDevanagari-TestSubset.ttf": (
        "notosansdevanagari/NotoSansDevanagari[wdth,wght].ttf",
        "ab किष्",
    ),
    "NotoSansJP-TestSubset.ttf": (
        "notosansjp/NotoSansJP[wght].ttf",
        "ab 中文日本",
    ),
}


def main() -> None:
    src, out = Path(sys.argv[1]), Path(sys.argv[2])
    out.mkdir(parents=True, exist_ok=True)
    for name, (path, chars) in JOBS.items():
        font = TTFont(src / path, recalcTimestamp=False)
        axes = {a.axisTag: a.defaultValue for a in font["fvar"].axes}
        axes.update({k: v for k, v in {"wght": 400, "wdth": 100}.items() if k in axes})
        font = instancer.instantiateVariableFont(font, axes, updateFontNames=True)
        opts = subset.Options()
        opts.layout_features = ["*"]
        opts.hinting = False
        opts.name_IDs = ["*"]
        opts.notdef_outline = True
        sub = subset.Subsetter(opts)
        sub.populate(unicodes=sorted({ord(c) for c in chars}))
        sub.subset(font)
        font.recalcTimestamp = False
        font.save(out / name)
        print(name, (out / name).stat().st_size, "bytes,", len(font.getGlyphOrder()), "glyphs")


if __name__ == "__main__":
    main()
