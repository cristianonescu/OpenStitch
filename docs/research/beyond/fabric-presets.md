# Fabric presets and their advice: sources (#508)

Beyond PE-DESIGN. The Fabric Selector keeps PE-DESIGN's eight fabric types and their values,
first in `fabric.list`. #508 part 1 (`batch:mac-fabric`, 2026-11-07) adds five more fabrics:
polyester woven, piqué knit, organza and sheers, canvas, and structured caps. For every fabric
and thread weight it also gives what to sew with, through `fabric.advise {fabric, weight}`.

## What is sourced and what is not

- **Density, stitch length, pull compensation, underlay:** no stabiliser, needle or thread maker
  publishes these per fabric. The five new fabrics' numbers are OpenStitch's own approximations,
  as the eight's always were (`crates/openstitch-engine/src/fabric.rs`). The only per-fabric
  numbers found were in a competing digitising program's help; they were not used.
- **Backing, topping, needle point, needle size, smallest text:** from the makers' own published
  guidance, paraphrased in OpenStitch's words, with each source cited. The data is
  `crates/openstitch-engine/data/fabric_advice.json`; its source ids are the ones below.
- **Disagreements:** where makers disagree, the advice names both views (`disagree`):
  - leather needles: a cutting point (Sulky, SCHMETZ), a round point because cutting points tear
    the hide in embroidery (Groz-Beckert), or titanium (Madeira UK);
  - caps: a sharp needle and never ballpoint (Madeira USA), or titanium (Madeira UK, Groz-Beckert);
  - towels: tear-away (OESD, Floriani, Sulky) or a heavy cut-away for pile (Madeira USA);
  - denim: cut-away (OESD, Madeira) or tear-away on shirt-weight denim (Madeira, Floriani);
  - knits: Sulky hoops an adhesive tear-away instead of the knit;
  - fleece: Floriani and Sulky also accept an adhesive tear-away.
- **Gaps:**
  - No maker gives a needle for organza. OpenStitch uses the general advice of a smaller needle on
    light fabrics, and says so.
  - No topping is published for most firm fabrics ("None needed").
  - Polyester woven has no backing advice of its own; the poly/cotton and weight-band advice is
    used.

## Thread weight

| No. | Needle | Stitch spacing | Smallest block letters |
|---|---|---|---|
| 60 | 65/9 (Madeira: 60/8–65/9) | 0.35 mm | 3 mm (Madeira USA, published) |
| 40 | 75/11 (Madeira's standard) | 0.40 mm | 6 mm (Madeira USA gives 6–7 mm) |
| 30 | 80/12 (Madeira 75/11–80/12; Sulky 90) | 0.50 mm | 7.5 mm, an estimate: 6 mm × 0.50 / 0.40 |
| 12 | 100/16 | 0.80 mm | 12 mm, an estimate: 6 mm × 0.80 / 0.40 |

The needle `fabric.advise` gives is the thread weight's, or the fabric's own minimum when a maker
gives a larger one (towel, denim and felt 90/14, canvas 100/16, fleece 80/12).

**Density by weight (part 2):** `fabric.apply {fabric, weight}` and `fabric.setDefault` scale the
fabric's fill and satin rows per mm by √(weight / 40): spacing goes with the thread's diameter,
which goes with √tex, and the thread number is inversely proportional to tex. The makers'
spacings above give similar ratios (0.35 / 0.40 = 0.88 for No. 60 against the rule's
0.82; 1.25 for No. 30 against 1.15; 2.0 for No. 12 against 1.83: the rule is a little denser
for thick thread), so the rule stays the simpler one and No. 40
leaves every fabric's values exactly as they are.

## Sources (checked 2026-11-07)

S1 Madeira UK, "Are you new to embroidery?"; S2 Madeira USA, Needle Guide; S3 Madeira USA, Quick
Reference Needle Chart; S4 Madeira USA, Digitizing Density Guide; S5 Madeira USA, Working with 60
Weight Thread; S6 Madeira USA, How to Choose the Correct E-Zee Backing; S7 Madeira GmbH, backing
selector brochure (09/2019); S8 OESD, Stabilizer Guide; S10 RNK Floriani, Stabilizer
Recommendation chart; S11 SCHMETZ, Needle Guide D-87; S12 Groz-Beckert, needles for single- and
multihead embroidery; S13 Sulky of America, thread and needle guide; S14–S20 Sulky of America's
embroidery series (leather, denim, canvas, fleece, felted wool, knits, towels); S22 Brother,
operation manual "Embroidery Needles"; S23 Brother Canada, SA540C. The URLs are in the data file.

Three of the PDFs (S10, S11, S12) were found on resellers' download servers, not on the makers'
own sites; their content is the makers' own documents. Groz-Beckert and Sulky forbid reproducing
their documents without permission, so nothing is copied: only facts, paraphrased and cited.
Sulky's interactive stabiliser selector and Madeira USA's E-Zee selection guide could not be read
and were not used.
