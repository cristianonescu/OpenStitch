# Thread charts of other brands: sources (B16.1 T5)

OpenStitch mode's image wizards may choose threads from charts other than Brother's
(`threads: all`, or a chart's id; [image-similarity-plan.md](image-similarity-plan.md), T5). The
owner decided on 2026-10-31 that these charts are compiled from the **manufacturers' own
published colour values**, recorded per source, as colour facts. No third-party compiled list
was used: not Ink/Stitch's palettes (GPL-3), not pyembroidery's, not forum or marketplace lists
or other software's thread files. No value was estimated by eye or sampled from a photograph
or scan.

The data lives in `crates/openstitch-core/data/brands/` (OpenStitch's thread-chart CSV columns,
`code,brand,description,R,G,B`); `tools/gen_brand_charts.py` turns it into
`crates/openstitch-core/src/brands.rs`. The PES/PEC chart and its matching, used by the
PE-DESIGN-compatible paths and the PES/PEC writers, are not changed: a thread from another chart
is written to a PES/PEC file as its nearest PEC colour, as any custom colour is.

All sources were fetched on 2026-10-31.

## Compiled

### Madeira Polyneon (`madeiraPolyneon`), 425 colours

- **Source:** Madeira's "No. 86 Digital Version Shade Card", POLYNEON,
  https://www.madeira.com/fileadmin/user_upload/Downloads/Shade_Cards/MADEIRA_POLYNEON.pdf,
  linked from https://www.madeira.com/embroidery-solutions/service/support/shade-cards. The PDF
  was made with InDesign 21.3 on 2026-05-27, 28 pages, SHA-256
  `fa3dffba7c33cc11e6d3a255b068edb2aaa94d7da0664a78eb476517f8d49c8c`.
- **Extraction:** each swatch on pages 3–27 is a vector rectangle filled in an ICC-based sRGB
  colour space (the embedded sRGB IEC61966 profile). Its three components, stored as fractions,
  were multiplied by 255 and rounded, and paired with the four-digit colour number printed
  beneath it.
- **What the values are:** the card says the shades were "digitised using DMIx technology" and
  are "optimised for on-screen viewing". They are Madeira's own screen sRGB, not a measured
  colour (white 1801 is a bluish 235, 234, 255).
- **Range:** the card covers Polyneon as a whole. Polyneon 60, 75 and FR share its colour
  numbers.
- **Left out:**
  - **1822, 1886 and 1990:** each labels two different swatches in the PDF (1822: 255,196,168
    on p. 4 and 227,220,199 on p. 21; 1886: 181,23,62 and 193,184,187; 1990: 224,83,138 and
    106,40,46). They stay out until Madeira confirms which is right.
  - **The 20 multicolour shades** (1510–1519, 1600–1609): they are photographs, not flat
    colours.
- **No names:** the card names no colours, so each colour is named by its number.
  Fluorescent shades are included as the card gives them; sRGB cannot show fluorescence.

### Isacord 40 (`isacord40`), 345 colours

- **Source:** Amann USA's "Isacord 40 Color Chart" on amannusa.com, as archived on 2011-08-01:
  `http://web.archive.org/web/20110801044637id_/http://www.amannusa.com:80/Isacord_chart.html`
  (made by Buzz Design Studios, dated 2011-01-03 in its meta tags; SHA-256
  `ec571fc96806584aa9ca1a43ac57d93d2fce08c5de08ed8939d518c6d0aa78bc`). After 2011 the page became a
  single image.
- **Extraction:** each colour number is followed by a swatch cell `<td bgcolor="#rrggbb">`. Four
  swatches use CSS colour names, taken at their CSS values (0015 white, 0020 black, 0501 yellow,
  1306 red).
- **The owner's choice (2026-10-31):** Amann published several RGB tables that disagree with one
  another. This chart and Ackermann North America's 2002 `Isacord_RGB.pdf` (Isacord's maker before
  Amann) agree on only 68 of the 302 codes they share. Two more tables titled as Amann's are found
  only on retailers' sites. The owner chose this one, the newest on a manufacturer's own site.
- **Notes:** nine colours are marked "Neon" on the page and named so here; 0501 and 1306 are plain
  CSS yellow and red, stand-ins rather than values. The page names no colours, so each colour is
  named by its number.

### Sulky Rayon 40 (`sulkyRayon40`), 335 colours

- **Source:** Gunold GmbH (Sulky's maker and European distributor), "SULKY 100% Viscose RGB Color
  Card", linked from https://shop.gunold.de/en/colour-card-sulky/99810000:
  https://shop.gunold.de/media/fa/c3/a8/1761145019/Sulky_RGB_ColorCard.pdf (2025-10-22; SHA-256
  `afbdc4ad30e5acd614861b599fa1f984cf8aa48a2eba4a303c838728aca9d6c6`).
- **Extraction:** each swatch is a vector rectangle filled in DeviceRGB; each component times 255,
  rounded, paired with the number printed above it. 1147 and 1039 share one merged rectangle, so
  both are 235, 0, 0.
- **Names:** from Sulky of America's own 2008 chart, https://www.sulky.com/fullpanel/uploads/files/rayonchart.pdf,
  matched by number (276 of the 335; some are cut short there, such as "Christmas Gree"); the others
  are named by their number.
- **Notes:** the card covers Sulky's viscose colours, sold as 40 and 30 weight; Sulky's 2008 chart
  says 40 weight comes in 388 colours. The card says "an exact color match can only be guaranteed
  using the original color chart".

### Robison-Anton Rayon 40 (`robisonAntonRayon40`, 420 colours) and Polyester 40 (`robisonAntonPolyester40`, 452 colours)

- **Source:** robison-anton.com's online colour charts, as archived in 2007 (robison-anton.com
  refuses connections today; Robison-Anton is now part of A&E):
  - Super Strength Rayon: `http://web.archive.org/web/20071225201835id_/http://www.robison-anton.com:80/cgi-bin/rayoncc.pl`
    (SHA-256 `e438e70275f4d7e9b9d64f73135120f19186eacd8585b91209ec18b17ce5dcc1`);
  - Super Brite Polyester: `http://web.archive.org/web/20071024012612id_/http://www.robison-anton.com:80/cgi-bin/polycc.pl`
    (SHA-256 `fef972fb2665e56164396d9a4b414ff25805272766534ab3e4910f91308212b0`).

  The archive shows the same content in 59 captures up to 2017. The archived product pages give
  both lines as size 40.
- **Extraction:** each colour cell carries its RA #122 number, a PANTONE reference and its name,
  followed by a swatch cell `<td bgcolor="RRGGBB">`.
- **Notes:** the page says the colours "should be used only as guides". The values look like
  on-screen renderings of each colour's PANTONE reference (PANTONE 1895 is `FCBFC9` in both).

## Not compiled: no official numeric values

| Brand / line | Searched | Why not |
|---|---|---|
| Madeira Classic 40 (rayon) | madeira.com's shade card (thread photographs) and downloads; the web archive's madeira.de, madeira.co.uk, madeira.com and madeirausa.com, including the colour-card pages, the 2002–2007 UK colour search and the 2007 `Classic_list_of_colours.pdf` | No numeric colour anywhere official; Madeira's "Color Definition Package" is a commercial CD-ROM; the RGB tables found (EZ Stitch, EduTechWiki) are third-party compilations |
| Brothread | brothread.com (a parked domain, also in its only archived capture); marketplace listings | No maker data; the kits use Brother's colour numbers, which the Brother/PEC chart already covers |

Possible follow-ups (the owner's call): ask Madeira for Classic's digital values and for the right
swatch of Polyneon 1822, 1886 and 1990; ask Amann which Isacord RGB table is current. Users can also
import any chart they have as a user thread chart (CSV) and choose it as the source or as the
threads they own.
