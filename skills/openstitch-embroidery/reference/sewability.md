# Sewability

Practical rules of thumb for designs that sew cleanly. They are guidance, not limits the
engine enforces: when a request needs something outside them, say so and suggest an
alternative. OpenStitch's defaults (fill and satin at 4.5 rows/mm, fill at 45°, underlay on)
are a sound start; change them only for a reason.

## Lettering

- Capitals at least **5-6 mm** high for satin text; lowercase and thin script fonts need
  more. Below about 4 mm letters close up and lose their shape.
- Very large satin letters (strokes wider than about 7-8 mm) sew loose: use fill text
  (`object.setRegion` `type: "fill"`) or a larger, bolder font.
- Text on a curve needs room: keep at least the text height between the text and the outline
  it follows, and use `otherSide` for text along the bottom of a circle.
- Keep a few millimetres between lettering and borders so the border does not swallow it.

## Satin (columns, borders, zigzag lines)

- Good satin width: about **1.5-7 mm**. Narrower than about 1 mm looks like a running line and
  can bury in the fabric; wider than 7-8 mm makes long, snagging stitches. For wider areas use
  fill, or satin with `splitLength` (2-12 mm) to break long stitches.
  Satin is never split automatically (PE-DESIGN sends 30 mm crossings whole), and
  `tools/sew_check.py` warns on any stitch over 12 mm. Auto Punch sews a region as fill when
  its satin would cross more than 10.5 mm.
- Density around 4-5 lines/mm for covering satin; lower (1-2) for tack-downs and light
  edges.
- Borders sew last, over the fill they edge, and should overlap it slightly.

## Fill

- Default density 4.5 rows/mm suits most fabrics. Lower it (3.5-4) for large areas, stretchy
  or thin fabric and stacked layers; raise it only for small areas on firm fabric.
- Keep `underlay` on for fills and satin over about 2 mm wide; it stabilises the fabric and
  lifts the top stitching.
- `pullCompensation` (0.2-0.4 mm) widens row ends so shapes do not shrink when sewn.
- Choose the sewing `angle` to cross the shape's long narrow parts, and vary it between
  neighbouring areas so they catch the light differently.
- A fill travels from its start point to its first row straight across the region, as
  PE-DESIGN does; dense rows hide it, open ones (low `density`, a light swatch) show it. Set
  `runningStitchPath: "alongOutline"` (or `"onOutline"`) on such a fill so that travel walks
  the edge instead, or place the start point where the first row begins
  (`object.setEntryExit`).
- Large overlapping fills sew thick and stiff: remove hidden parts with `shape.removeOverlap`
  instead of stacking fills.

## Decorative region and line types

- Cross stitch regions use one grid on the page axes from the design origin (`angle` does not
  turn it), so neighbouring cross stitch regions with the same size line up cell for cell; only
  whole cells are sewn, each a full X.
- Stippling honours `spacing` (mean gap between passes about the spacing) and reaches to about
  half a spacing from the outline; it starts and ends inside the shape.
- Concentric circle, spiral, flexible spiral and contour fills end in the middle (no run back
  out); radial lines thin out near the centre.
- Stem, candlewick and chain lines are spaced along curves by distance, so curved paths sew as
  evenly as straight ones.

## Running and detail lines

- Running stitch length about **2-3 mm**; shorter (1.5 mm) on tight curves. Triple running
  stitch (`type: "triple"`) for a bold outline that still reads as a line.
- Details thinner than 1 mm should be running or triple lines, not regions.

## Travel, trims and tiny pieces

- **Tiny specks** (regions under about 2 mm across) sew as knots: merge, enlarge or drop
  them. OpenStitch mode in Auto Punch merges regions under 2 mm² for this reason.
- **Long jumps** between distant pieces of the same colour need trims; keep the pieces of a
  colour close together and use `order.optimize` (or `order.byColor`) to cut colour changes
  and jumps.
- Fewer colour changes means a faster sew-out: reuse threads where the look allows.
- Watch `design.info`: stitch count, `colorChanges`, `trims`, `jumps` and the sewing-time
  estimate. A small badge with thousands of trims or a 100 × 100 mm design with well over
  30,000 stitches deserves another look.

## Size and fabric

- The whole design must fit the hoop's sewing field; leave a few millimetres of margin.
- Scaling a stitched design changes its density; resize shapes and text (which are
  regenerated) rather than converted stitches.
- Dense designs on thin or stretchy fabric pucker: lower density, keep underlay, and suggest a
  stabiliser. The Fabric Selector (Application menu) applies fabric-based settings.
- Always recommend a test sew-out on scrap fabric before the real piece.
