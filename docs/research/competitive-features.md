# Competitive features: what other digitizing products offer beyond PE-DESIGN 11

Research date: 2026-10-28. Public sources only (feature pages, online help, release notes,
tutorials, forum threads); no software was installed and no vendor code was read. Every
feature below names its sources by URL; claims seen on one page only are marked
**(single source)** and inferences are marked **(inferred)**. Vendor feature names are used
only to identify the feature; OpenStitch gives its own commands generic names (see
"Licensing notes").

Products covered: Wilcom EmbroideryStudio e4–2026 ("ES") and Hatch 3/4; BERNINA Embroidery
Software V9 (Wilcom-based); Husqvarna/Pfaff mySewnet Embroidery Software (the Premier+ 2
engine); Janome Artistic Digitizer 2 ("AD"); Tajima DG16/DG17 by Pulse ("DG"); Embird
(Basic, Studio, Sfumato, Font Engine); Ink/Stitch 3.3 (GPL: concepts and documented
parameters only); Embrilliance (Essentials, Enthusiast, StitchArtist L1–L3, Density Repair
Kit); Floriani Total Control U ("FTCU") and SewWhat-Pro ("SWP"); Brother's own PE-DESIGN 11
updates and companion apps.

The raw survey notes per product (about 300 feature bullets) are kept outside the repository
in the session's notes folder; this document is the distilled, cross-checked version.

## Contents

1. [The PE-DESIGN 11 baseline](#1-the-pe-design-11-baseline)
2. [Comparison matrix](#2-comparison-matrix)
3. [Per-feature notes](#3-per-feature-notes)
   - 3.1 Fill types
   - 3.2 Contour and outline treatments
   - 3.3 Digitizing tools
   - 3.4 Workflow
4. [Ranked top-15 backlog and phases](#4-ranked-top-15-backlog-and-phases)
5. [Licensing notes](#5-licensing-notes)
6. [Sources](#6-sources)

---

## 1. The PE-DESIGN 11 baseline

What OpenStitch already matches is in [FEATURE_MATRIX.md](../FEATURE_MATRIX.md). The points
that matter for this survey, read from Brother's public guide and update history:

- **Versions.** The product is PE-DESIGN 11 (2018). Brother's one-click updates run 11.01 to
  11.50 (September 2026). There is no separately sold "11.5": 11.50 is a free update that adds
  the 200 × 360 mm hoop and the PES v11.5 file version. The updates are almost entirely hoops,
  file versions, wireless Link and bug fixes; only 11.10 (satin/fill/programmable fill/chain
  sewing), 11.21 (spiral editing, SVG/WMF import), 11.23 (TrueType conversion, decorative fill
  on Auto Punch shapes) and 11.40 (open freehand curves) touched stitch generation.
  Sources: [Brother update history](https://support.brother.com/g/s/hf/ped_info/ped11/one_click_update/en/ped11update_info.html).
- **Stitch palette.** 14 region sew types and 9 line sew types, listed on the guide's
  [region](http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1185920.html)
  and [line](http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1185303.html)
  pages. OpenStitch implements all of them.
- **Engine limits, read from the attribute pages** (so **(inferred)** rather than from a
  public complaint): one unitless pull-compensation slider and no push compensation; Gradation
  is a density pattern, not a colour blend; direction is one angle, one manual curve, or Auto
  (no multi-angle maps); zigzag lines have one width; underlay is a fixed preset set; corner
  control is a mitre limit on zigzag lines only; no branching, no 3D puff mode, no chenille,
  sequin, bead or cording types; no thread-brand conversion command; no density-preserving
  resize of imported stitch data (the guide's "Enlarging/reducing" supplement warns about
  it). Sources: [region attributes](http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html),
  [line attributes](http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293378.html).
- **Public criticism** is about stagnation since 2018, Windows problems, Auto Punch quality,
  the dongle and the price, not about specific missing stitch types
  ([forum 2025](https://forum.embroideres.com/forums/topic/25500-too-bad-that-pe-design-seems-to-either-be-dead-or-dying/),
  [t-shirtforums](https://www.t-shirtforums.com/threads/trouble-with-pe-design-next.192989/)).
  The feature gaps below are therefore ranked by what digitizers use elsewhere, not by
  complaint volume.
- **Brother companions** that overlap: My Design Center / My Design Snap (on-machine line-art
  conversion, stippling and decorative fills) and Artspira+ (paid auto-digitizing, line
  tracing, portrait conversion). Sources:
  [My Design Snap](https://support.brother.com/g/s/hf/mobileapp_info/mydesignsnap/mds_en.html?ap0=en),
  [Artspira](https://brother-usa.com/home/sewing-embroidery/mobile-apps) (single source).

## 2. Comparison matrix

Legend: ● has it · ◐ partial or a simpler form · ○ not found · ? not verified. "OS" is
OpenStitch at `main` on 2026-10-28; "PE" is PE-DESIGN 11.50. A ● in the OS column means the
feature exists; a ○ with a backlog number (B1–B15) means it is in the ranked backlog of
section 4. Product levels are given in the notes, not here.

Since 2026-10-28 (status as of 2026-11-09; the current state is in
[FEATURE_MATRIX.md](../FEATURE_MATRIX.md), not in this table): OpenStitch has gained contour
fill (F1, B1), density profiles and colour blends (F5, F6, #532), maze and meander fills (F8,
#533), knockdown fill (F12, #510), travel under later fills (F17, #545), auto-route, Redwork and
partial branching (D1, D2, O10, #546), satin lines with variable width (O2, #524), satin corner
handling and short stitches in part (O3, O4, #520, #521, #373), automatic underlay by width,
pull compensation per side and in %, and push compensation (D4, D5, #522, #523, #677), and a
density map (D15, `design.density`, #504); D9 now has all 26 Text Transform presets.

| # | Feature | PE | OS | ES / Hatch | Bernina V9 | mySewnet | Janome AD | Tajima DG | Embird | Ink/Stitch | Embrilliance | FTCU / SWP |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| **Fills** | | | | | | | | | | | | |
| F1 | Contour (offset) fill, spiral strategies | ○ | ◐ (echo in Background Fill) | ● | ● | ● | ? | ● (Cascade) | ● (Contour effect) | ● | ● (echo, 3 styles) | ? |
| F2 | Guided / curved fill, one guide | ◐ (manual curve) | ● (direction curve) | ● (Florentine) | ● | ● (MultiWave) | ? | ● (Wave) | ● (Wave effect) | ● (3 strategies) | ● (curved fill) | ● (wave fill) |
| F3 | Curved fill, two or more guides | ○ | ○ B2 | ● (Liquid) | ? | ● (MultiWave, several lines) | ? | ? | ○ | ○ | ○ | ? |
| F4 | Stitch-angle maps (several angles in one region) | ○ | ○ B2 | ● | ● | ◐ | ? | ● (angle lines) | ● (direction lines) | ◐ (rungs on satin) | ● (inclines, L2) | ? |
| F5 | Density gradient profiles | ◐ (Max/Min pattern) | ◐ | ● (Accordion, 8 presets + custom) | ● | ● (gradient density) | ? | ● (density profile) | ● (linear/central) | ● (end row spacing) | ● (gradient fill) | ● |
| F6 | Two-colour blending by complementary density | ○ | ○ B6 | ● | ● | ● (multicolour gradient) | ? | ● (Auto Colour Blend) | ◐ | ● (gradient blocks, multicolour satin) | ◐ | ● (colour blend) |
| F7 | Crosshatch / curved crosshatch / lace mesh | ◐ (Net fill, fixed patterns) | ◐ (Net fill; crosshatch in Background Fill) | ◐ | ? | ● (3 styles, curved, double zigzag) | ● (Net fill) | ? | ? | ◐ (patterns) | ● (freestanding background) | ? |
| F8 | Meander / maze fill with pattern tiles | ◐ (Stippling) | ◐ (Stippling) | ● (Maze, Stipple) | ● | ● (QuiltStipple) | ◐ | ● (Fractal fill) | ● (Sfumato meanders) | ● (tile library, zigzag variant) | ● (4 styles) | ? |
| F9 | Hand-stitch randomisation (length, width, angle) | ○ | ◐ (Random Shift in decorative fill) | ● (Hand Stitch effect) | ? | ○ | ○ | ● (random density, jagged) | ● (random shift, broadening) | ● (seeded jitter) | ○ | ○ |
| F10 | 3D puff (foam) satin mode | ○ | ○ B7 | ● (Raised) | ● | ○ | ○ | ● (DG17 corner processing) | ○ | ◐ (Stop command) | ● (Satin 3D Foam Auto-Underlay) | ○ |
| F11 | Carving / emboss lines drawn on a fill | ◐ (stamps) | ◐ (stamps, emboss/engrave) | ● (Carving Stamp, Flexi Splits) | ● | ◐ (Emboss) | ○ | ● (Line and Region Carving) | ● (Carving object) | ● (patterns) | ● (Fill Embossing, Carving Lines L3) | ● (custom emboss) |
| F12 | Knockdown / laydown (nap) fill | ○ | ○ B12 | ● (Laydown) | ? | ○ | ○ | ○ | ○ | ● (Knockdown fill) | ● (Knockdown Stitches) | ● (SWP nap tack) |
| F13 | Linear colour-gradient fill from a vector gradient | ○ | ○ | ○ | ○ | ○ | ○ | ○ | ○ | ● | ○ | ○ |
| F14 | Tartan fill | ○ | ○ | ○ | ○ | ○ | ○ | ○ | ○ | ● | ○ | ○ |
| F15 | Shape / motif array fills (shape fill, space fill, floral array, vine) | ◐ (motif fill) | ◐ (motif fill) | ● (Motifs Advanced, gradient motifs) | ● | ● (Shape Fill, 120 shapes) | ● (Space, Floral Array, Vine, Bitmap arrays) | ● (programmed fills, 165+) | ● (3 × 3 multi-motif grid) | ◐ | ● (motif fill, user motifs L3) | ◐ |
| F16 | Photo thread-painting with density-modulated meanders | ◐ (Photo Stitch 1/2) | ◐ (Photo Stitch 1/2) | ● (Colour PhotoStitch, Reef) | ● | ● (PhotoStitch modes) | ● (Photo-Stitch, PaintStitch, Sketch) | ● (single source) | ● (Sfumato) | ○ | ○ | ● |
| F17 | Trapunto (travel runs moved to the edge of open fills) | ○ | ○ | ● | ● | ○ | ○ | ○ | ○ | ◐ (underpath off) | ○ | ○ |
| F18 | Cross-stitch fill from bitmap | ● | ● | ● (add-on) | ● | ● (Cross Stitcher) | ● | ● | ● (plug-in) | ● (3.3) | ● | ● |
| F19 | Sequin, chenille, bead, cording types | ○ | ○ | ● (ES Elements) | ◐ (Couching, PunchWork) | ○ | ◐ (Crystals) | ● | ○ | ○ | ○ | ○ |
| **Outlines** | | | | | | | | | | | | |
| O1 | Run / bean / backstitch / stemstitch families | ● (running, triple, stem) | ● | ● | ● | ● (double, triple, double zigzag) | ● | ● (bean, half bean, two ply) | ● | ● (repeat sequences) | ● (7 styles incl. Sashiko) | ● |
| O2 | Satin line with variable width (two independent rails) | ○ (zigzag line, one width) | ◐ (Manual Punch columns) | ● (Column B) | ● | ● (Satin Column) | ● | ● (Column, Steil) | ● (Column, pressure-sensitive) | ● (satin column with rungs) | ● (satin column, inclines) | ● (single-line satin) |
| O3 | Satin corner handling (mitre / cap / lap) | ◐ (mitre limit on zigzag) | ◐ | ● (Smart Corners, 3 types) | ● | ◐ (font digitizing only) | ? | ● (Custom Cornering, Steil Thickness) | ● (Auto Corners: roof/folded/split) | ◐ (via rungs) | ? | ? |
| O4 | Short stitches on inner curves | ○ | ◐ (zigzag lines only) | ● | ● | ? | ? | ● | ● (Auto Shortening) | ● (multi-level inset) | ? | ● (auto-split) |
| O5 | Motif runs with distribution / tapering | ● (motif line) | ● | ● (Motifs Advanced) | ● | ● (Tapered Motifs) | ● | ● (Programmed Run Arrange) | ● (Samples, min/max length) | ◐ (patterns) | ● (Motif Run gradient) | ● |
| O6 | E-stitch / blanket | ● (E/V) | ● | ● | ● | ○ | ? | ● (corner-aligned) | ● (Overlock mode) | ● | ● | ? |
| O7 | Appliqué: placement / tack-down / cover, pre-cut vs cut-in-place, partial cover | ● (wizard) | ● (wizard) | ● (4 layers, frame-out, partial, combine components) | ● (with holes) | ● (property of any area) | ● | ● (auto appliqué, text order by letter or by stage) | ● (tack-down offset) | ◐ (tutorial) | ● (Appliqadabra, cut files) | ● (SWP cut-file export) |
| O8 | Cutwork needle lines (4 angles) | ● | ● | ◐ (vector cut line) | ● (CutWork) | ● | ● (Cut outline) | ◐ (laser) | ○ | ● (segmentation per brand) | ○ | ○ |
| O9 | Richelieu bars, feathered satin column | ◐ (feather on regions) | ◐ (feather on regions) | ◐ (Jagged Edge) | ◐ | ● | ○ | ◐ | ◐ (random broadening) | ◐ (random width) | ● (L2) | ○ |
| O10 | Redwork: continuous single-colour outline routing | ○ | ○ B4 | ● (ES 2026) | ○ | ○ | ○ | ○ | ○ | ● | ○ | ● (Redwork tool) |
| **Digitizing tools** | | | | | | | | | | | | |
| D1 | Branching (one object, several columns, travel under) | ○ | ○ B4 | ● | ● (new in V9) | ○ | ○ | ● | ◐ (auto connections) | ◐ (auto-route satin) | ● (object branching) | ○ |
| D2 | Auto-route running stitches (graph routing, trims instead of jumps) | ○ | ◐ (order.optimize) | ◐ | ◐ | ◐ (Intelligent split only) | ◐ (Sequence manager) | ● (Auto Sequence) | ◐ | ● | ◐ (Sequence Mode) | ● (Optimize Design Sequence) |
| D3 | Closest join / auto start-end / continue digitizing | ◐ (Optimize Entry/Exit) | ◐ (entry/exit optimise) | ● | ● | ○ | ○ | ● (Auto Start/Stop) | ● (re-routed connections) | ◐ (nearest point flags) | ○ | ● (Optimize Entry/Exit) |
| D4 | Automatic underlay chosen by column width, first + second underlay | ◐ (presets) | ◐ (presets) | ● | ● | ◐ (None/Low/Med/High) | ? | ● (4 types, order) | ● (auto by size) | ● (3 kinds, insets) | ◐ | ● (Save2Sew) |
| D5 | Pull compensation in mm and %, per side; push compensation | ◐ (one slider) | ◐ (one value) | ● (ES 2026 per side) | ● | ◐ (0–20 scale) | ? | ● (X/Y pull, push) | ● | ● (mm + %, asymmetric, push) | ● (min / % / max) | ◐ |
| D6 | Fabric table bundling density, underlay, compensation | ● (Fabric Selector) | ● (approximation) | ● (Auto Fabric) | ● | ○ | ● (welcome screen) | ● (recipes) | ● (Styles Table) | ○ | ● (Project Advisor) | ● (Save2Sew) |
| D7 | Remove overlapping stitches / auto clip with overlap distance | ● (Remove Overlap of shapes) | ● (shape level) | ● | ● | ○ | ● | ● (Auto Clip) | ○ | ○ | ● (Essentials, appliqué) | ○ |
| D8 | Knife / slice (straight, rectangle, curved) | ● (Split Outline) | ● (shape.split) | ● | ● | ○ | ○ | ● (3 kinds) | ○ | ● (cut satin) | ● (CUT) | ● (Slice) |
| D9 | Lettering baselines (arc, circle, spiral, any path) and envelopes | ● (26 Transform presets, fit to outline) | ● (24 of 26) | ● (9 baselines, Enveloping) | ● | ● | ● (Text Art) | ● (any curve, envelopes) | ● (spirals, node-editable) | ◐ (along path) | ● (circle, spiral, custom L3) | ● (distortions) |
| D10 | Saved kerning pairs / kerning classes | ◐ (manual kerning) | ◐ | ● (saved per font) | ● | ? | ? | ● (classes, TTF import) | ? | ● (hkern in font) | ● (AlphaTricks auto) | ? |
| D11 | Team names / batch lettering from CSV | ◐ (Name Drop) | ◐ (Name Drop) | ● (Team Names, Matrix sequencing) | ● | ○ | ○ | ● | ○ | ● (batch lettering) | ◐ | ● (Name Drops) |
| D12 | TrueType → turning satin quality (skeleton-based) | ◐ (11.23 fix) | ◐ (Font Creator) | ● | ● | ● (Font Create) | ● (any TTF) | ● | ● (Font Engine) | ○ (SVG fonts) | ◐ | ◐ |
| D13 | Closest-point connected fonts (no trims between letters) | ○ | ○ | ◐ | ◐ | ○ | ○ | ● (180–200 fonts) | ● | ◐ | ● (connecting scripts) | ● |
| D14 | Stitch-file resize with density recalculation / density tuning | ○ (objects only) | ○ B14 | ◐ | ◐ | ● (Stitch Editor) | ◐ | ● (Outline Processor, Master Density) | ● (target stitch count) | ◐ (Outline → vector) | ● (Density Repair Kit ±25 %) | ● (SWP constant-density resize) |
| D15 | Validation layer (ambiguous satins, density map, troubleshoot) | ○ | ○ B13 | ◐ | ◐ | ○ | ○ | ○ | ● (Density Map, X-ray) | ● (Troubleshoot, Density Map) | ● (WireLine map) | ○ |
| **Workflow** | | | | | | | | | | | | |
| W1 | Multi-hoop split with editable split lines and alignment stitch styles | ◐ (fixed sections) | ◐ (fixed sections) | ● (object-level split) | ● | ● (Intelligent / Straight, editable lines) | ? | ◐ | ● (auto-split over any hoop set) | ○ | ● (patented auto-split) | ● (SWP positional) |
| W2 | Automatic smallest-hoop selection | ○ | ○ | ● (Auto Hoop) | ? | ○ | ◐ | ○ | ○ | ○ | ○ | ○ |
| W3 | Thread-brand conversion (nearest colour across charts) | ○ | ○ B13 | ● | ● | ● (My Threads) | ● (Colour Manager) | ● | ● (sort by similarity, owned threads) | ● (Apply Palette) | ● (one-click) | ● (SWP) |
| W4 | Realistic 3D preview and stitch simulator | ● | ● | ● (TrueView, Stitch Player) | ● | ● (Life View, Design Player) | ● | ● | ● (1–10 000 sps) | ● | ● | ● |
| W5 | Vector import and live link to a drawing program | ◐ (SVG/EMF/WMF) | ◐ (SVG) | ● (CorelDRAW embedded) | ● (CorelDRAW SE) | ◐ | ◐ | ● (AI/CDR/EPS/DXF, DrawFusion) | ◐ (EMF/WMF/CMX) | ● (is Inkscape) | ◐ | ◐ (WMF/EMF/AI/SVG) |
| W6 | Machine formats beyond PE-DESIGN's set (VP3/VP4, DSB/DSZ, TBF, U01, TAP, KSM, Z01, EMB read) | ○ | ◐ (VP3, TAP, TBF, U01 read and write; DSB, DSZ, KSM read; #480, #481) | ● | ● | ● (VP3/VP4) | ◐ | ● | ● (≈ 60 formats) | ● (VP3, TBF, U01, G-code) | ● (VP3, TAP, DSZ, EXP+) | ● (≈ 30 read) |
| W7 | Operator worksheet / approval print with barcodes | ◐ (catalog print) | ◐ (catalog, templates) | ● | ◐ | ◐ | ○ | ● | ● | ● (PDF layouts) | ● (templates) | ◐ |
| W8 | Batch and command-line format conversion | ◐ (Design Database batch) | ● (`stitchtool`, `library.convert`) | ◐ | ◐ | ○ | ○ | ◐ | ● (Manager batch) | ● (zip batch) | ◐ | ● (SWP CLI) |
| W9 | Shell thumbnails | ◐ (Windows) | ◐ (Linux) | ○ | ○ | ○ | ○ | ○ | ● (Iconizer) | ○ | ● (Thumbnailer) | ○ |

## 3. Per-feature notes

Each note gives: what it does; who has it; how PE-DESIGN handles it; the stitch-generation
idea in OpenStitch's own words; effort against the existing crates (S = days, M = one to
three weeks, L = more than that, counted for one engineer including tests and the editor
pane); user value (High / Medium / Low, for home and small-shop digitizers, OpenStitch's
audience); licensing notes. "design" is `crates/openstitch-design`, "digitize" is
`crates/openstitch-digitize`, "formats" is `crates/openstitch-formats`, "engine" is
`crates/openstitch-engine`, "app" is the editor.

### 3.1 Fill types

#### F1. Contour (offset) fill with spiral strategies

- **What.** The region is filled by nested copies of its own outline, each inset by the row
  spacing, sewn as running stitch. Variants: rings linked by short connectors (inner-to-outer
  or outer-to-inner); a single spiral that blends consecutive rings so the whole fill is one
  path from the outside in; a double spiral that goes in and comes back out so the fill starts
  and ends on the border. Echo variants put the rings outside the shape too, with rounded,
  square or diagonal corner styles.
- **Who.** Ink/Stitch Contour Fill (strategies Inner to outer / Single spiral / Double spiral,
  join style round/mitre/bevel, avoid self-crossing, clockwise, smoothness, bean repeats)
  [[IS-contour]](https://inkstitch.org/docs/stitches/contour-fill/); mySewnet Contour Fill and
  Echo Fill (gap 2–20 mm, 1–30 lines, corner style)
  [[MSW-area]](https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Area_Types.htm);
  Wilcom/Hatch Contour Fill and Spiral/Ripple
  [[Hatch-dig]](https://hatchembroidery.com/features/digitizing); Pulse Cascade Fill
  [[DG15]](https://embroidery.pulsemicro.com/dg15-levels.html); Embird Fill effect Contour /
  Square / Rounded from a focus point
  [[Embird-fill]](https://www.embird.net/machine-embroidery-tutorials/studio-next/par_fill.htm);
  Embrilliance Contour (echo) in three styles **(single source)**.
- **PE-DESIGN.** Concentric Circle, Spiral and Flexible Spiral sew types radiate from a
  centre point, and the Background Fill wizard makes echo lines around a pattern; there is no
  fill that follows the region's own outline inward.
- **Idea.** Inset the region polygon repeatedly by the row spacing with a chosen join style;
  stop when the offset collapses. For ring mode, sew each ring and travel to the next along
  the shortest connector; for single spiral, walk ring *k* while interpolating towards ring
  *k+1* by arc-length fraction, so the path never closes; for double spiral, run the single
  spiral on every second ring inward and on the others outward. Bottleneck shapes split into
  sub-regions when an inset produces several polygons; "avoid self-crossing" chooses ring order
  per sub-region. Smooth with a bounded deviation and subdivide to the run pitch.
- **Effort.** S–M. design already offsets outlines (`shape.offsetLines`, `i_overlay`) and has
  the concentric/spiral generators and `RegionType`; the work is a new `RegionType::Contour`
  with strategy, join style, spacing and run pitch, plus the inspector row.
- **Value.** High. It is the most-used "quilting look" fill in the home market and the
  Ink/Stitch and mySewnet communities treat it as basic.
- **Licensing.** Concept is generic polygon offsetting. US 5541847 (medial-axis concentric
  placement) has expired. Implement from the description above, not from Ink/Stitch code.

#### F2 / F3 / F4. Guided fills with one or two guides, and stitch-angle maps

- **What.** Rows of a fill or satin follow a user-drawn curve instead of a straight angle
  (one guide); with two guides the rows interpolate between them so the flow changes across
  the shape; with several angle lines across a region the direction turns progressively
  between consecutive lines ("turning fill" without splitting the object).
- **Who.** Wilcom Florentine (one guide: must cross both sides, at least 3 points, stitch
  length at most 4 mm advised) and Liquid (two guides)
  [[ES-florentine]](https://docs.wilcom.com/embroiderystudio/e4/en/MainHelp/Decorative/curves/Create_curved_fills_with_Florentine_Effect.htm),
  [[ES-curves]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Decorative/curves/curves-10.htm);
  stitch angle lines [[ES-angles]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/quality/quality-3.htm);
  mySewnet MultiWave Fill (one or more wave lines, optional motif) **(single source)**
  [[MSW-area]](https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Area_Types.htm);
  Pulse Wave Fill and angle lines [[DG16]](https://fonts.pulsemicro.com/Home/Embroidery);
  Embird Wave effect and direction lines [[Embird-fill]](https://www.embird.net/machine-embroidery-tutorials/studio-next/par_fill.htm);
  Ink/Stitch Guided Fill with Copy / Parallel offset / Buffer strategies
  [[IS-guided]](https://inkstitch.org/docs/stitches/guided-fill/); Embrilliance Curved Fill
  and inclines [[EM-SA]](https://www.embrilliance.com/stitchartist).
- **PE-DESIGN.** "Manual curve" direction on Fill, Programmable Fill, Piping and Motif is a
  one-guide version. OpenStitch has it (`direction_curve`). Two guides and angle maps do not
  exist.
- **Idea.** Treat the guides as a sparse direction field. One guide: rows are copies of the
  guide translated along its normal by the spacing (cheap, uneven density on tight bends),
  or true parallel offsets (even density, cusps at corners), or a buffer grown both ways
  (handles self-crossing guides). Two guides: parameterise both by arc length, and row *t*
  is the point-wise blend `(1 − t)·A(s) + t·B(s)`; clip every row to the region. Angle maps:
  for a region crossed by angle lines L₁…Lₙ in sewing order, the strip between Lᵢ and Lᵢ₊₁ is
  filled with rows whose direction rotates linearly from Lᵢ to Lᵢ₊₁ (each row is a short
  polyline whose tangent is the interpolated angle at its position), and strips are sewn in
  order with the travel on the strip boundary. Keep the stitch length short (≤ 4 mm) when
  rows bend; underlay stays a straight-angle fill.
- **Effort.** M for two guides (extend `direction_curve` to a pair and the row generator in
  `region.rs`); M–L for angle maps (new attribute, strip decomposition, editor handles for
  the lines, reuse of the Manual Punch direction-line UI).
- **Value.** High. Turning fills are the single biggest visual difference between hobby and
  professional output (leaves, petals, hair).
- **Licensing.** WO2005061774 (turning complex fill) and US 6587745 (curved line fill) have
  expired; check the owner's jurisdiction before relying on that. "Florentine", "Liquid" and
  "MultiWave" are vendor names; use "guided fill" and "angle lines".

#### F5 / F6. Density profiles and two-colour blending

- **What.** The row spacing follows a profile along the fill direction (linear, convex,
  concave, wave, user-defined), so the fill fades from dense to open. Blending sews the same
  region twice in two colours with complementary profiles, bottom colour dense where the top
  is open, so one colour melts into the other.
- **Who.** Wilcom Accordion Spacing (8 presets; ES 2026 adds custom profiles), Colour Blending,
  Multi Blend with 20+ profiles in Hatch 4
  [[ES-designing]](https://wilcom.com/embroiderystudio/designing),
  [[ES-blend]](https://wilcom.com/resources/blog/color-blending); mySewnet gradient and
  multicolour gradient density on Pattern Fill, Spiral Fill and Satin Column (underlay
  suppressed under gradients) [[MSW-pattern]](https://softwarehelp.mysewnet.com/MSW/100/Digitizing/Topics/Pattern_Fill_—_Options.htm);
  Pulse Density Profile and Auto Colour Blend [[DG15]](https://embroidery.pulsemicro.com/dg15-levels.html);
  Embird linear/central gradient added to the base spacing
  [[Embird-fill]](https://www.embird.net/machine-embroidery-tutorials/studio-next/par_fill.htm);
  Ink/Stitch End row spacing and Convert to Gradient Blocks
  [[IS-fill]](https://inkstitch.org/docs/stitches/fill-stitch/),
  [[IS-filltools]](https://inkstitch.org/docs/fill-tools/); FTCU Wave Gradient and Colour
  Blend [[FTCU]](https://www.mulqueen.com/shop/c/p/Floriani-Total-Control-Professional-Embroidery-Software---FTCU-x44586193.htm).
- **PE-DESIGN.** Gradation is a single-layer density pattern (Max/Min, 53 % to 5 % by default,
  measured black-box) with no colour component.
- **Idea.** OpenStitch's `Gradation` already maps a fraction of the fill extent to a density
  factor. Generalise the map to a profile curve (presets plus a user polyline), apply it to
  satin columns too (spacing along the column), and add a `shape.blend` command that
  duplicates the object with a second thread and the mirrored profile, sewing the lighter
  layer first. Travel runs for open layers should follow the outline (OpenStitch's "Along
  outline" running-stitch path) so they are not visible, which is what Wilcom calls Trapunto
  (F17).
- **Effort.** S for profiles, S for the blend command, S for the editor. Satin-column
  gradient is S once profiles exist.
- **Value.** High. Colour blending is the feature most often shown in Hatch and mySewnet
  marketing, and it costs little.
- **Licensing.** Generic. "Accordion Spacing" and "Trapunto" are Wilcom's names.

#### F7. Crosshatch, curved crosshatch and lace mesh as a region type

- **What.** Two or more families of parallel running lines at different angles, open, with
  no cover; a double-zigzag line type makes the mesh strong enough for free-standing lace;
  curved variants bend the families along a guide.
- **Who.** mySewnet Crosshatch (Diamond / Square / Parallel, gap, angle, stitch type) and
  Curved Crosshatch **(single source)** [[MSW-area]](https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Area_Types.htm);
  Embrilliance Freestanding Background [[EM-SA]](https://www.embrilliance.com/stitchartist);
  Janome Net Fill [[AD]](https://janome.com.au/product/janome-artistic-digitizer).
- **PE-DESIGN.** Net Fill (6 fixed patterns) and Zigzag Net Fill (4 patterns) cover the
  straight cases with fixed pattern geometry. OpenStitch has both, and the Background Fill
  wizard has a crosshatch option.
- **Idea.** Reuse the tatami row generator with the cover removed: generate family A at
  angle α (rows joined along the outline), then family B at α + 90° (square) or at ±α
  (diamond); the connecting travel runs along the outline. For lace, sew each line as a
  zigzag of the given width and return along it offset by half a period (double zigzag).
  Curved crosshatch is F2 applied to each family.
- **Effort.** S (a `RegionType::Crosshatch` over the existing row code and the existing
  zigzag line generator).
- **Value.** Medium. Quilting and free-standing lace users ask for it; PE-DESIGN's net fills
  partly cover it.
- **Licensing.** Generic.

#### F8 / F9. Meander and maze fills, and hand-stitch randomisation

- **What.** One continuous, non-self-crossing path fills the region (stippling at a chosen
  spacing, or a maze-like run); pattern tiles (hexagon, vermicelli, space-filling curves) set
  the character; a zigzag variant turns the meander into fur. Randomisation jitters stitch
  length, column width and angle with a fixed seed so output is reproducible.
- **Who.** Wilcom Maze Fill (never crosses itself) and Hand Stitch effect, Hatch 3
  [[ES-hand]](https://wilcom.com/resources/blog/natural-organic-hand-stitch-effect-for-machine-embroidery),
  [[Hatch-3]](https://hatchembroidery.com/resources/blog/whats-new-in-hatch-embroidery-3);
  Ink/Stitch Meander Fill (pattern tile, scale, angle, clip, smoothness, zigzag spacing and
  width, seeded randomisation) [[IS-meander]](https://inkstitch.org/docs/stitches/meander-fill/);
  Pulse Fractal Fill (random runs, low count) and random density
  [[DG16]](https://fonts.pulsemicro.com/Home/Embroidery); Embird Random Shift and Max.
  Random Broadening [[Embird-fill]](https://www.embird.net/machine-embroidery-tutorials/studio-next/par_fill.htm);
  mySewnet QuiltStipple **(single source)**.
- **PE-DESIGN.** Stippling Stitch exposes run pitch and spacing (plus a motif); Decorative
  Fill has Random Shift 0–50 %. OpenStitch has both.
- **Idea.** Maze: lay a grid of cell size = spacing over the region, build the cell adjacency
  graph (cells inside the region), take a random spanning tree and walk it depth-first; the
  wall-following path around the tree is a single non-crossing curve that visits every cell;
  smooth with a bounded spline. Tiles: clip a scaled, rotated tile curve to the region and
  join the pieces along the outline. Fur: replace each path segment by a zigzag of the given
  width. Randomisation: per object, a seeded generator (seed = object id) perturbs stitch
  length, rail width and angle by the given percentages, applied after generation so the
  underlying geometry stays deterministic.
- **Effort.** M for maze and tiles (new generator; OpenStitch's stippling generator gives the
  curve smoothing and clipping); S for randomisation (a post-pass in `compile.rs` with
  per-attribute percentages).
- **Value.** Medium–High. Randomisation is cheap and gives the "hand-made" look that Hatch
  3 made a headline; maze fill is a popular low-stitch-count fill.
- **Licensing.** Generic. Ink/Stitch's tile library is GPL: draw OpenStitch's own tiles.

#### F10. 3D puff (foam) satin mode

- **What.** A per-object mode that prepares satin for foam: higher density, no underlay (or
  opt-in), no pull compensation, closed column ends ("caps") sewn before the top satin so
  the foam is cut cleanly, no stitch shortening on inner corners, a machine stop before the
  puff elements so the operator lays the foam, and puff objects sequenced last.
- **Who.** Hatch Raised Embroidery and ES Raised Satin [[Hatch-dig]](https://hatchembroidery.com/features/digitizing);
  Bernina V9 3D/puffy **(dealer listing)**; Embrilliance Satin 3D Foam Auto-Underlay ("adds
  capping and joins") [[EM-L2]](https://embrilliance.com/product/stitchartist-level-2-upgrade);
  Pulse DG17 "Automatic Processing for 3D Embroidery" generating foam-cutting corners
  [[DG17]](https://www.tajima.com/product/dg/) (thin detail); Ink/Stitch uses a Stop command only.
- **PE-DESIGN.** No foam attribute; satin density presets only **(inferred)**.
- **Idea.** Puff rules from the public digitizing guides: column width 3–12 mm (warn outside),
  spacing about 0.16–0.18 mm (roughly twice normal density), push the column ends outward
  by 0.3–0.5 mm and add a cap, that is a short satin segment across each open end sewn
  perpendicular to the column before the top layer; disable short-stitch thinning; insert a
  colour-change stop before the first puff object; sort puff objects after flat ones. Where a
  satin border surrounds a flat fill, the border alone cuts the foam.
  Sources: [[Wilcom puff Q&A]](https://wilcom.com/resources/blog/qa-from-3d-puff-embroidery-webinar),
  [[Ignition Drawing 1]](https://blog.ignitiondrawing.com/working-with-3d-puff-embroidery-part-1/print/),
  [[Ignition Drawing 2]](https://blog.ignitiondrawing.com/working-with-3d-puff-embroidery-part-2/print/).
- **Effort.** S–M. A `puff: bool` on `RegionSewing` and `LineSewing` (zigzag lines), density
  override, cap generation in `column.rs`/`skeleton.rs`, a stop in `compile.rs`, and an
  order rule in `order.rs`.
- **Value.** High for cap and hat work, a large share of small-shop embroidery.
- **Licensing.** Generic. Pulse's "3D Drawing Method" is undescribed, so nothing to avoid.

#### F11 / F12. Carving lines and knockdown (laydown) fills

- **What.** Carving: the user draws lines over a fill; needle penetrations are forced along
  the lines so the fill shows a crease, without a second object. Knockdown: a light open fill
  sewn first, a few millimetres larger than the design, to flatten pile (towels, fleece) so
  no water-soluble topping is needed.
- **Who.** Pulse Line and Region Carving [[DG15]](https://embroidery.pulsemicro.com/dg15-levels.html);
  Embird Carving object [[Embird-objects]](https://www.embird.net/studio/manual/0075objects.htm);
  Wilcom Carving Stamp / Flexi Splits, Hatch Carving Stamps
  [[ES-designing]](https://wilcom.com/embroiderystudio/designing); Embrilliance Carving
  Lines (L3) [[EM-SA]](https://www.embrilliance.com/stitchartist); Ink/Stitch patterns on
  fills [[IS-patterns]](https://inkstitch.org/docs/stitches/patterns/). Knockdown: Wilcom
  Laydown (ES 2025, Hatch 3) [[ES-2025]](https://wilcom.com/resources/blog/top-10-new-features-of-wilcom-embroiderystudio-2025);
  Ink/Stitch Selection to Knockdown Fill [[IS-filltools]](https://inkstitch.org/docs/fill-tools/);
  Embrilliance Knockdown Stitches (bi-directional, about 4 mm stitches, inflation)
  [[EM-ent]](https://www.embrilliance.com/enthusiast); SWP nap tack
  [[SWP]](https://sandscomputing.com/products-shop/sewwhatpro-embroidery-editor/).
- **PE-DESIGN.** Stamps (emboss/engrave) from a pattern library; OpenStitch has stamps and
  its pattern tiles carry embossed/engraved regions. No free-drawn carving line, no knockdown.
- **Idea.** Carving: for every fill row, find its intersections with the carving polylines
  and insert needle points there (snapping the neighbouring points so no stitch is shorter
  than the minimum); a region carve applies a different pattern tile inside the drawn area.
  Knockdown: offset the union of the selected objects' outlines by the inflation, fill it at
  a low density (about 2 mm spacing, 4 mm stitches) in two perpendicular passes, colour it
  with the fabric thread, and insert it first in the sewing order.
- **Effort.** S each (`stamp.rs` already masks needle points for stamps; knockdown is a
  Background Fill variant placed first).
- **Value.** Medium (carving) and Medium–High (knockdown: towels are a staple of the home
  market and the feature is cheap).
- **Licensing.** Generic.

#### F13 / F14 / F15. Linear colour-gradient fill, tartan, shape and generative fills

- **What.** Linear gradient: an SVG gradient on the shape becomes colour sections with rows
  perpendicular to the gradient direction and consistent needle positions across sections.
  Tartan: warp and weft stripe sets rendered as a woven fill, with an editor that imports
  Scottish Register codes. Shape fills: a chosen shape repeated in rows, circles or arrays
  (mySewnet Shape Fill with 120 shapes; Janome Space Fill, Floral Array, Bitmap arrays, Vine
  Fill that grows branches and blooms inside a region).
- **Who.** Ink/Stitch Linear Gradient Fill and Tartan Fill
  [[IS-gradient]](https://inkstitch.org/docs/stitches/linear-gradient-fill/),
  [[IS-tartan]](https://inkstitch.org/docs/stitches/tartan-fill/); mySewnet Shape Fill
  [[MSW-area]](https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Area_Types.htm);
  Janome AD 2 fills **(single source each)** [[AD]](https://janome.com.au/product/janome-artistic-digitizer);
  Embird multi-motif grid [[Embird-fill]](https://www.embird.net/machine-embroidery-tutorials/studio-next/par_fill.htm).
- **PE-DESIGN.** Motif Fill and Decorative Fill tile one pattern; OpenStitch's own pattern
  library does the same.
- **Idea.** Gradient: cut the region into bands along the gradient axis, one per colour stop
  pair, and sew each band as a fill whose rows are perpendicular to the axis, with the row
  phase shared across bands. Tartan: two crosshatch families (F7) whose lines are grouped
  into stripes by colour; narrow stripes become running stitches. Shape fill: place the
  shape on a lattice, clip at the boundary, sew each instance as a running or motif line.
  Vine fill: a space-colonisation growth (seeds on the boundary, branches grow towards
  unfilled attractors) followed by leaf motifs at branch tips.
- **Effort.** M (gradient), M (tartan editor), M (shape fill), L (vine).
- **Value.** Low–Medium. Visually striking but niche; gradient overlaps F6.
- **Licensing.** Generic. Ink/Stitch's tartan palette import format is public data.

#### F16. Photo thread-painting with density-modulated meanders

- **What.** A photo is split into user-drawn regions; each region gets a base colour and
  5–9 derived shades; each shade layer is a meander whose local density follows image
  darkness, with the darkest areas as plain fill; longer stitches give a fuzzier look.
  Alternative families: colour quantisation with per-colour random stippling (Colour
  PhotoStitch), single-colour darkness-weighted runs (Reef), direction-aware stitches along an
  image gradient field.
- **Who.** Embird Sfumato Stitch (Fidelity %, Style, Thread Shades 1–9, Shade Threshold
  0–255, spacing 0.35–0.45 mm, carvings, about 100 px/cm input)
  [[Sfumato]](https://embird.net/sfumato-stitch),
  [[Sfumato-params]](https://www.embird.net/machine-embroidery-tutorials/studio-next/par_sfu.htm);
  Hatch 3 Colour PhotoStitch (400 × 400 mm limit) and Reef PhotoStitch
  [[Hatch-3]](https://hatchembroidery.com/resources/blog/whats-new-in-hatch-embroidery-3);
  mySewnet PhotoStitch modes, Janome Photo-Stitch / PaintStitch / Sketch **(single source)**.
- **PE-DESIGN.** Photo Stitch 1 (row-based running/satin by darkness) and Photo Stitch 2
  (few-colour layered fills); OpenStitch approximates both in `digitize/photo.rs`.
- **Idea.** Quantise luminance per region into N shades; for shade *i* build a density map
  `d(x, y)` from how far the pixel is below the shade's threshold; generate a meander (F8)
  whose cell size varies with `d` (a quadtree subdivision where darker cells split further),
  and sew the layers light to dark so later shades cover earlier ones. Direction-aware
  variant: Sobel gradient field, stitches perpendicular to the gradient.
- **Effort.** L (a new conversion in digitize plus editor masks).
- **Value.** Medium. Portrait embroidery is a visible showcase but a small share of use.
- **Licensing.** US 7996103 (Sobel-direction photo stitches) was filed in the late 2000s and
  may still be in force: avoid the direction-aware variant until checked. "Sfumato",
  "PhotoStitch" and "Reef" are vendor names.

#### F17. Trapunto (hidden travel in open fills)

- **What.** For open (low-density) fills, travel runs are moved to the object edge and
  overlapping rows removed, so no stitching shows between the rows.
- **Who.** Wilcom/Hatch Trapunto [[ES-specialty]](https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Decorative/specialty/specialty-14.htm);
  Ink/Stitch "Underpath" off puts travel on the border [[IS-fill]](https://inkstitch.org/docs/stitches/fill-stitch/).
- **PE-DESIGN.** Running Stitch Path "Along outline" / "On outline" places the travel on the
  outline; OpenStitch has it. Missing is the automatic choice for open fills and the removal
  of the doubled first/last rows.
- **Idea.** When spacing exceeds a threshold (about 1 mm), force "On outline" travel, sew the
  outline connector once, and skip the overlapping half-rows at section joins.
- **Effort.** S. **Value.** Medium (pairs with F5/F6). **Licensing.** Generic; "Trapunto" is
  a quilting term, fine to use.

#### F18 / F19. Cross-stitch from bitmap; sequins, chenille, beads, cording

- F18 is at parity (PE-DESIGN and OpenStitch both have it); Ink/Stitch 3.3's assistant adds
  grid checks and bitmap colour reduction, which OpenStitch's `cross.rs` partly covers.
- F19 is machine-dependent (sequin devices, chenille heads, bead feeders, cording feet) and
  sold as options by Wilcom and Pulse
  [[ES-sequins]](https://wilcom.com/embroiderystudio/elements/sequins-advanced/),
  [[DG16]](https://fonts.pulsemicro.com/Home/Embroidery). Brother home machines do not
  support them, so they are out of scope for OpenStitch's audience; noted for completeness.
  Bernina's Couching and PunchWork **(single source)** are likewise foot-specific.

### 3.2 Contour and outline treatments

#### O2. Satin line with variable width

- **What.** A satin outline whose width changes along the path: two independent rails (the
  user edits either side), or a centreline with width keys, or pen pressure.
- **Who.** Wilcom Column B (two independent edges) and Column C (fixed width with Round Sharp
  Corners) [[ES-corners]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/quality/quality-11.htm);
  mySewnet Satin Column (pairs of points, patterns, gradient density; about 8–10 mm advised)
  **(single source)** [[MSW-area]](https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Area_Types.htm);
  Pulse Column, Steil and Steil Thickness, Brush Tool (calligraphy with tapered ends)
  [[DG17-notes]](https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG17);
  Embird pressure-sensitive columns **(single source)** [[Embird-tools]](https://www.embird.net/digitizing-tools/);
  Ink/Stitch satin column with rungs [[IS-satin]](https://inkstitch.org/docs/stitches/satin-column/);
  FTCU single-line satin input [[FTCU]](https://www.mulqueen.com/shop/c/p/Floriani-Total-Control-Professional-Embroidery-Software---FTCU-x44586193.htm).
- **PE-DESIGN.** The Zigzag line has one width; variable width needs a Manual Punch column.
  OpenStitch has Manual Punch columns and skeleton satin, which already take rungs.
- **Idea.** Store width keys `(s, w_left, w_right)` along the line's arc length; the rails are
  the offsets of the centreline by the interpolated widths; rungs are generated from the
  normals and fed to the existing `column::satin`. Tapered ends are keys with zero width.
  Editor: drag handles on the line.
- **Effort.** S–M (attribute, rail generation, inspector; the satin itself exists).
- **Value.** High. Calligraphic lettering and borders depend on it.
- **Licensing.** Generic. "Steil" is a trade term in common use.

#### O3 / O4. Satin corner handling and short stitches on inner curves

- **What.** Per-object corner treatment for satin columns and zigzag lines: mitre (two
  segments cut on the bisector, slightly overlapping), cap (square end plus a short
  perpendicular segment that closes the corner), lap (two overlapping segments, one under
  the other), with angle thresholds and limits. Short stitches: where the inside of a bend
  crowds the needle points, alternate stitches are pulled in from the inner rail by one or
  more inset levels.
- **Who.** Wilcom Smart Corners: Mitre ("Mitre below" angle, Overlap), Cap ("Cap below" 20°
  default, Max length), Lap ("Lap below" 110° default, Full or Split overlap, Max Extension);
  mitre and cap can combine [[ES-corners]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/quality/quality-11.htm),
  [[ES-corners-2]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/quality/quality-9.htm);
  Bernina [[Bernina-corners]](https://softwarehelp.bernina.com/en/MainHelp/PART_Digitizing/quality/Controlling_corner_stitching.htm);
  Pulse Custom Cornering, mitred steil, penetrations forced onto sharp corners
  [[DG16-notes]](https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG16);
  Embird Auto Corners (roof / folded / split) and Auto Shortening
  [[Embird-column]](https://www.embird.net/studio/manual/3125par_cmn.htm); Ink/Stitch
  Short stitch distance and multi-level Short stitch inset
  [[IS-satin]](https://inkstitch.org/docs/stitches/satin-column/); mySewnet Mitered Corners
  in Font Digitizing [[MSW-mitre]](https://softwarehelp.mysewnet.com/MSW/120/FontDigitizing/Topics/Mitered_Corners.htm).
- **PE-DESIGN.** "Sharp Corners: Miter limit" on zigzag lines only. OpenStitch mitres zigzag
  rails and pulls every other inner point towards the centre on crowded bends (zigzag lines
  only).
- **Idea.** At each corner of a column centreline compute the interior angle θ. Cap when
  θ < cap threshold: end both rails square at the corner, then emit a cap segment whose
  rungs stay parallel to the incoming direction and extend to the outer corner point. Mitre
  when θ < mitre threshold: split the column at the corner, clip each half's rungs at the
  bisector, and overlap by the configured distance. Lap when θ > lap threshold: continue
  the first segment past the corner by the max extension and start the second on top. Short
  stitches: for each rung, if the inner-rail spacing to the previous rung is below the
  distance, shorten the rung's inner end by inset level k (cycling through the levels).
- **Effort.** M (corner detection and the three constructions in `column.rs`/`skeleton.rs`,
  with the zigzag-line mitre code as the start; short stitches generalise the existing
  zigzag rule to regions and columns).
- **Value.** High. Corners and inner curves are where satin quality is judged.
- **Licensing.** Generic constructions; "Smart Corners" is Wilcom's name.

#### O7. Appliqué: partial cover, combined sequencing, cut files

- **What.** Beyond the placement / tack-down / cover sequence: cover stitching only between
  two points on an overlapping piece (partial cover); resequencing several appliqués so all
  placements sew first, then all tack-downs, then covers (one fabric stop per stage); a
  frame-out move so the hoop comes forward for trimming; cut-file export (SVG/PLT/FCM) for a
  pre-cut workflow; appliqué text sewn by letter or by stage.
- **Who.** Wilcom four-layer appliqué, Partial Appliqué, Combine Appliqué Components
  [[ES-applique]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Mixed/applique/applique-6.htm),
  [[ES-partial]](https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Mixed/applique/applique-13.htm);
  Pulse Applique Font Sewing Order [[DG17-notes]](https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG17);
  Embrilliance cut files and hidden-stitch removal under overlapping appliqué
  [[EM-ess]](https://www.embrilliance.com/essentials); SWP Appliqué Cutter
  [[SWP]](https://sandscomputing.com/products-shop/sewwhatpro-embroidery-editor/).
- **PE-DESIGN.** The Appliqué wizard makes the four stages for one shape at a time; ScanNCut
  FCM export covers cut files. OpenStitch's wizard produces the stages as line objects.
- **Idea.** Partial cover: the cover line gets start and end points on the outline and sews
  clockwise between them. Combine: a command that groups the stage objects of several
  appliqués by stage name (OpenStitch already names them) and reorders them stage by stage.
  Cut files: write the material outline as SVG (already possible through shapes) with the
  placement colour.
- **Effort.** S each. **Value.** Medium. **Licensing.** Generic; "Appliqadabra" is Embrilliance's.

#### O9. Richelieu bars and feathered satin columns

- **What.** Short satin bars across a cutwork opening at intervals; satin columns whose
  needle points on one or both rails are randomised for a feathered edge.
- **Who.** mySewnet Richelieu Bars and Feathered Satin **(single source)**
  [[MSW-area]](https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Area_Types.htm);
  Embrilliance Richelieu bars (L2); Embird per-edge random broadening; Ink/Stitch random
  width; Wilcom Jagged Edge.
- **PE-DESIGN.** Feathered edge exists on satin and fill regions (OpenStitch has it).
- **Idea.** Feathered column: the region feather applied to rail points. Bars: along a
  column centreline, every *n* mm emit a short zigzag column perpendicular to it, clipped to
  the opening.
- **Effort.** S. **Value.** Low–Medium (cutwork users). **Licensing.** Generic.

#### O10. Redwork routing

- **What.** A line drawing is converted so the whole design sews as one continuous
  single-colour path that starts and ends at the same point, every line covered exactly twice
  (so it reads as a bean stitch with no visible travel).
- **Who.** Ink/Stitch Redwork (connect lines below a distance, minimum path length, bean
  repeats for the top layer) [[IS-stroke]](https://inkstitch.org/docs/stroke-tools/); Wilcom
  ES 2026 Redwork [[ES-designing]](https://wilcom.com/embroiderystudio/designing); FTCU Redwork tool.
- **PE-DESIGN.** None; My Design Center on machines produces line designs but without this routing.
- **Idea.** See D1/D2: doubling every edge makes every vertex even, so an Euler circuit
  exists; Hierholzer's algorithm yields the closed walk; sew the second pass of each edge
  with the bean repeats.
- **Effort.** S once D2 exists. **Value.** High for line-art and My Design Center style work.

### 3.3 Digitizing tools

#### D1 / D2. Branching and auto-routed running stitches

- **What.** Branching: several satin or run pieces are digitized as one object with one
  entry and one exit; the engine orders the branches, sews travel runs out along the
  branches (as underlay, hidden under later satin) and the cover on the way back, so a tree
  of columns has one trim. Auto-route running: reorder running-stitch strokes to minimise
  jumps, adding nodes at intersections and hidden travel along already-sewn lines.
- **Who.** Wilcom Branching, Bernina V9 Branching (new) [[ES-designing]](https://wilcom.com/embroiderystudio/designing),
  [[Bernina-connectors]](https://softwarehelp.bernina.com/en/MainHelp/PART_Digitizing/connectors/connectors-1.htm);
  Pulse Branching plus Automatic Overlap / Fix Gap at branch ends
  [[DG16-IE]](https://tajimaeurope.com/en/pulse-software-solutions/embroidery-software-tajima-software-dg16-pulse/illustrator-extreme/);
  Embrilliance object branching with sub-branches [[EM-SA]](https://www.embrilliance.com/stitchartist);
  Ink/Stitch Auto-Route Satin Columns and Auto-Route Running Stitch (add nodes at
  intersections, trim jumps, preserve order) [[IS-satintools]](https://inkstitch.org/docs/satin-tools/),
  [[IS-stroke]](https://inkstitch.org/docs/stroke-tools/); the routing idea is discussed
  openly on the Inkscape developer list
  [[inkscape-devel]](https://lists.inkscape.org/hyperkitty/list/inkscape-devel@lists.inkscape.org/thread/C7XJD67MCZNGO2WJOARMERURZ7GQYTXE/).
- **PE-DESIGN.** Optimize Sewing Order and Optimize Entry/Exit Points reorder whole objects;
  Connect Points joins outline paths. OpenStitch has an overlap-aware colour-grouping
  optimiser and entry/exit linking; its derived skeleton follows one stroke only (ROADMAP
  "Editing details").
- **Idea.** Build a graph: vertices at stroke endpoints and intersections, edges = strokes
  with weight = length. A single pass covering every edge exists when at most two vertices
  have odd degree (Euler path); otherwise pair the odd vertices by minimum-weight matching
  over shortest paths (route inspection / Chinese postman) and duplicate those paths; then
  Hierholzer's walk gives the order. Duplicated edges are the travel runs; prefer duplicating
  edges that a later satin will cover. For satin branching, the graph is on column
  centrelines: a depth-first walk from the entry sews the centre-run underlay outward and
  the satin on the return, so each branch's cover is sewn after its children (one trim at the
  end). Branch junctions get an overlap of about half the column width (Pulse's Fix Gap).
- **Effort.** M for the running-stitch router (`route.rs`, an `order.autoRoute` command);
  L for satin branching (branching skeletons, junction overlaps, editor input of several
  strokes as one object).
- **Value.** High. Branching is the feature Bernina chose to headline V9, and auto-routing
  makes line-art designs sew without trims.
- **Licensing.** Classic graph algorithms. Do not read Ink/Stitch's implementation.

#### D3. Closest join, auto start/end, continue digitizing

- **What.** Each object's entry and exit are placed at the points nearest the previous and
  next objects, recomputed on demand; the design's first and last needle positions are set
  automatically (centre or a hoop position); a closed object can be extended from its start
  or end without a new object.
- **Who.** Wilcom Closest Join (global, "Apply Closest Join" shortcut J), Auto Start & End,
  Continue Digitizing (ES 2026, Hatch 4) [[ES-closest]](https://docs.wilcom.com/embroiderystudio/e4/en/MainHelp/Quality/connectors/Apply_closest_join.htm),
  [[ES-designing]](https://wilcom.com/embroiderystudio/designing); Pulse Auto Start/Stop,
  Design Connections (trims by distance, locks) [[DG15]](https://embroidery.pulsemicro.com/dg15-levels.html);
  FTCU Optimize Entry/Exit Points.
- **PE-DESIGN.** Optimize Entry/Exit Points (11.01) is the equivalent for the whole design;
  OpenStitch has it at the reference tier. Missing: the per-object re-apply, auto start/end
  and continue digitizing.
- **Idea.** For objects i and i+1, choose exit(i) and entry(i+1) as the closest pair of
  outline points (OpenStitch's `route::nearest`), honouring regions whose fill start is
  fixed; auto start/end adds a leading and trailing jump to the chosen position. Continue
  digitizing appends to the path of the selected object with the Pen tool.
- **Effort.** S (engine commands over existing routing; editor toggles).
- **Value.** Medium. **Licensing.** Generic.

#### D4 / D5 / D6. Automatic underlay by width, pull and push compensation, fabric tables

- **What.** Underlay chosen from column width and fabric: none below about 1 mm, centre
  run for 1–3 mm, edge run (inset about 0.3–0.5 mm) for 3–4 mm, zigzag at 2–3 mm spacing
  above that, tatami or double tatami under large fills, optionally a first and a second
  underlay. Pull compensation as an absolute amount per side (0.2 mm woven, 0.35 mm knit,
  0.4 mm fleece are published starting points) or a percentage of the width, asymmetric,
  negative allowed; push compensation shortens column ends. Fabric tables bundle all of it.
- **Who.** Wilcom Auto Underlay with first and second underlay, lettering width rules (< 5 mm
  none, 6–10 mm centre run, > 10 mm edge run), Auto Fabric, ES 2026 Separate Pull
  Compensation [[ES-underlay]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/stabilizing/stabilizing-7.htm),
  [[ES-pull]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/stabilizing/stabilizing-17.htm),
  [[Wilcom-pushpull]](https://wilcom.com/resources/blog/push-and-pull-compensation),
  [[Hatch-fabric]](https://blog.hatchembroidery.com/how-to-use-auto-fabric-or-create-your-own-custom-fabric-settings/);
  Pulse X/Y pull, push compensation, four underlay types with order, fabric recipes
  [[DG-12]](https://dgml14.pulsemicro.com/pulse-products-12reasonsforpulse.htm); Embird
  underlay by object size, Styles Table [[Embird-tools]](https://www.embird.net/digitizing-tools/);
  Ink/Stitch pull compensation mm and % (two values for asymmetry), push compensation
  (3.3), three satin underlays with fixed and proportional insets
  [[IS-satin]](https://inkstitch.org/docs/stitches/satin-column/); Embrilliance pull
  compensation min / % / max [[EM-SA]](https://www.embrilliance.com/stitchartist); FTCU
  Save2Sew [[FTCU]](https://www.mulqueen.com/shop/c/p/Floriani-Total-Control-Professional-Embroidery-Software---FTCU-x44586193.htm);
  a general underlay primer [[Underlay-101]](https://support.coldesi.com/Portals/8/Documents/Liberty%20Docs/Underlay%20101.pdf).
- **PE-DESIGN.** Under sewing presets (Edge only, Single, Region only, Edge and region,
  Double) and one pull-compensation slider; Fabric Selector applies density, pull
  compensation and underlay globally. OpenStitch matches that at the reference tier.
- **Idea.** Keep PE-DESIGN's attributes as the compatible default and add an "automatic"
  underlay style that picks from the width-threshold table (measured along the rungs for
  satin, from the area for fills), with per-fabric multipliers; replace the scalar pull
  compensation by `{left_mm, right_mm, percent, push_mm}` with the scalar kept as a
  compatibility alias in `.osd` and PES round trips; make the Fabric Selector table
  editable and per-object overridable.
- **Effort.** M altogether (model change with migration, generators in `region.rs`,
  `column.rs`, `skeleton.rs`, inspector rows).
- **Value.** High. This is where sew-out quality is decided, and the published tables make
  the defaults defensible.
- **Licensing.** Generic published practice.

#### D7. Remove overlapping stitches / auto clip

- **What.** Stitches of a lower object hidden under an upper one are removed, with an
  overlap distance kept so no gap shows; satin can clip fills and appliqué.
- **Who.** Pulse Remove Overlap Stitches and Auto Clip [[DG16-IE]](https://tajimaeurope.com/en/pulse-software-solutions/embroidery-software-tajima-software-dg16-pulse/illustrator-extreme/);
  Wilcom Remove Overlaps; Janome remove overlaps; Embrilliance for appliqué.
- **PE-DESIGN.** Remove Overlap works on shapes (OpenStitch has it as `shape.merge` and
  `shape.combine`); nothing at stitch level, so imported stitch data cannot be clipped.
- **Idea.** Boolean-subtract the upper outline, shrunk by the overlap distance, from the
  lower region before generation (shape level, already possible), and for stitch blocks drop
  needle points inside the clip polygon and re-link the runs with travel on the boundary.
- **Effort.** S (shape level as an automatic option), M (stitch level).
- **Value.** Medium. **Licensing.** Generic.

#### D9 / D10 / D11 / D13. Lettering engine depth

- **What.** Baselines as a model (free, fixed-length that compresses spacing, vertical, arc
  clockwise and counter-clockwise, full circle with radius X/Y, any path) with per-letter
  rotation to the tangent; saved kerning pairs per font and kerning classes; team names from
  a CSV with one design per row and matrix sequencing of the output files; fonts whose letters
  connect at the nearest point so a word sews without trims; small-text fonts with shortened
  stitches; lock stitches inside letters.
- **Who.** Wilcom 9 baselines, saved kerning, Enveloping, Team Names, Expert Resequencing,
  Alphabet Creator [[ES-lettering]](https://wilcom.com/embroiderystudio/lettering),
  [[ES-baselines]](https://docs.wilcom.com/embroiderystudio/26/en/OnlineHelp/Lettering/lettering_create/Apply_baselines.htm);
  Pulse Visual Kerning, kerning classes, TTF kerning import, closest-point fonts, lock
  stitches inside text, WordIT, team names [[DG16-notes]](https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG16);
  Embird Font Engine baselines (spirals), envelopes, nearest-point connections
  [[Embird-font]](https://www.embird.net/machine-embroidery-tutorials/common/font_engine.htm);
  Ink/Stitch font.json with hkern pairs, batch lettering, lettering along path
  [[IS-lettering]](https://inkstitch.org/docs/lettering/); Embrilliance connecting scripts,
  AlphaTricks auto kerning [[EM-alpha]](https://embrilliance.com/product/alphatricks/);
  FTCU Closest Point Connect fonts.
- **PE-DESIGN.** 26 Transform presets, Fit Text to Outline, kerning per character, Name
  Drop, monograms, Font Creator. OpenStitch has them (24 of 26 presets), with shaping and
  kerning from the font's tables.
- **Idea.** Baselines: a `Baseline` enum (line, arc with radius and sweep, circle, path) with
  letters placed by arc length and rotated to the tangent, which generalises Fit Text to
  Outline; fixed-length baselines scale letter spacing. Kerning: a per-font map of pair
  adjustments saved with the design font (`DesignFont`) and applied after the OpenType
  tables. Team names: extend Name Drop to a CSV with several fields and write one file per
  row with a naming pattern. Closest-point connection: for each glyph pair, choose the exit
  of glyph *i* and the entry of glyph *i+1* as the nearest outline points and link them with
  a hidden run under the next satin (D3 applied inside text; OpenStitch's Connection Points
  work is the start).
- **Effort.** M overall (S for CSV team names, S for saved kerning, M for the baseline
  model, M for closest-point connection quality).
- **Value.** High for lettering-heavy users (the majority of hobby use).
- **Licensing.** Generic. Bundled fonts stay SIL OFL.

#### D12. TrueType to turning satin

- **What.** Glyph outlines become satin columns whose stitch angle follows each stroke, with
  fill where strokes are wider than the satin limit.
- **Who.** All vendors; the public descriptions are Wilcom's
  [[ES-ttf]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Lettering/lettering_custom/lettering_custom-2.htm)
  and the Baby Lock note that satin over 10 mm falls back to fill
  [[BabyLock]](https://babylock.com/amfile/file/download/file/1629/product/585); Brother
  itself shipped TrueType conversion fixes in 11.23.
- **PE-DESIGN.** Converts TrueType in Layout & Editing and Font Creator. OpenStitch's Font
  Creator converts glyphs through its derived skeleton (grid approximation, longest stroke
  only, per ROADMAP).
- **Idea.** Exact medial axis (Voronoi of the outline samples), pruned by a length
  threshold, split at branch points into strokes; for each stroke cast normals to find the
  two rails; local width = 2 × medial radius picks satin (< about 8–10 mm) or fill; junctions
  become caps or laps (O3); stroke order follows writing order or closest join; branching
  (D1) sews a glyph as one object.
- **Effort.** L (the exact medial axis is already on the roadmap; the quality work is in
  the junctions and ordering).
- **Value.** High. **Licensing.** US 8532810 (skeleton-based satin from strokes) is from the
  2010s and may still be in force in some jurisdictions; US 6934599 (vector font to satin)
  has expired. OpenStitch's existing skeleton satin should be reviewed against the claims
  of 8532810 by the owner's lawyer before this item is extended.

#### D14. Stitch-file resize with density recalculation and density tuning

- **What.** Resize imported stitch data while keeping density (re-spacing satin and fill
  stitches), or target a stitch count; tune density ±25 % without changing size; remove
  hidden stitches, de-clump, limit density by fabric; reconstruct vector objects from
  stitches.
- **Who.** Embrilliance Density Repair Kit and Advanced Sizing (50–250 %)
  [[EM-DRK]](https://www.embrilliance.com/density-repair-kit) **(single source)**,
  [[EM-ess]](https://www.embrilliance.com/essentials); Embird Editor resize with density,
  "prescribed stitch count" [[Embird-resize]](https://www.embird.net/sw/embird/tutorial/resclc/resclc.htm);
  Pulse Outline Processor, Master Density [[DG16-IE]](https://tajimaeurope.com/en/pulse-software-solutions/embroidery-software-tajima-software-dg16-pulse/illustrator-extreme/);
  mySewnet Stitch Editor density change **(single source)**; SWP constant-density resize
  [[SWP]](https://sandscomputing.com/products-shop/sewwhatpro-embroidery-editor/); Ink/Stitch
  Outline (stitch file to vector) [[IS-stroke]](https://inkstitch.org/docs/stroke-tools/).
- **PE-DESIGN.** Object data is regenerated on resize; imported stitch data is scaled as is
  (the guide's supplement warns). OpenStitch's stitch-to-shape conversion is an approximation.
- **Idea.** Segment a stitch block into runs by stitch-length and direction statistics:
  satin columns (alternating long stitches with a stable rail pair), fills (parallel rows at
  a constant spacing), runs (short collinear stitches). For satin, keep the rails and
  regenerate the zigzag at the target spacing; for fills, keep the row direction and
  regenerate the rows; runs are resampled to the pitch. Density tuning changes only the
  spacing. This is the same segmentation OpenStitch's stitch→shape conversion needs.
- **Effort.** M (segmentation in `stitch_edit.rs`, regeneration with existing generators).
- **Value.** Medium–High: the home market resizes purchased designs constantly, and Brother
  machines have no density recalculation.
- **Licensing.** Embrilliance describes its auto-split as patented and sells the Density
  Repair Kit; the segmentation above is generic signal analysis. Check Embrilliance's patents
  by number before shipping an auto-split beyond the current fixed-section splitter (W1).

#### D15 / W3. Validation layer and thread-brand conversion

- **What.** A troubleshooting view (ambiguous satins, missing rungs, overlong stitches, tiny
  stitches, density hot spots as a coloured map); a command that maps every thread in the
  design to the nearest colour of a chosen brand chart, with "owned threads" filtering.
- **Who.** Ink/Stitch Troubleshoot Objects and Density Map
  [[IS-trouble]](https://inkstitch.org/docs/troubleshoot/), [[IS-visualize]](https://inkstitch.org/docs/visualize/);
  Embird Density Map, X-ray, thread similarity sort and owned threads
  [[Embird-tools]](https://www.embird.net/digitizing-tools/); Embrilliance WireLine and
  one-click brand conversion [[EM-DRK]](https://www.embrilliance.com/density-repair-kit),
  [[EM-ess]](https://www.embrilliance.com/essentials); mySewnet My Threads; Janome Colour
  Manager; SWP thread library.
- **PE-DESIGN.** User thread charts and colour-chart import; no conversion command and no
  density view **(inferred)**.
- **Idea.** Density map: count needle points per cell of a 1 mm grid, blur, colour by
  thresholds (green/yellow/red), draw as an overlay. Troubleshoot: rules over the design
  model and the compiled stream (stitch length outside [min, max], satin width over the
  limit, rail crossings, fills with no rows). Thread conversion: nearest colour in CIE Lab
  (ΔE2000) within the chosen chart, with an "owned" subset from user settings; OpenStitch
  already carries the PEC, JEF, SEW, HUS and SHV charts as colour facts.
- **Effort.** S each. **Value.** Medium–High (cheap and visible). **Licensing.** Thread
  charts stay colour facts only.

### 3.4 Workflow

#### W1. Multi-hoop split with editable split lines

- **What.** Split a design across several hoopings with an "intelligent" cut that routes
  through gaps and crosses the fewest stitches, or a straight line; the split line is
  editable with mid-points every 20 mm; overlap from 9 mm up to half the hoop; alignment
  stitches as corner Ls or a basting box; compensation stitches along the cut; trim or run
  between parts; object-level splitting for multi-position hoops.
- **Who.** mySewnet Split Project Wizard and Stitch Editor Split Design
  [[MSW-split]](https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Split_Embroidery.htm),
  [[MSW-splitlines]](https://softwarehelp.mysewnet.com/MSW/100/StitchEditor/Topics/Split_Design.htm);
  Embird Auto-Split over overlapping hoop areas [[Embird-split]](https://embird.net/sw/embird/tutorial/autosplt/autosplt.htm);
  Hatch 3 multi-position hoops splitting objects [[Hatch-3]](https://hatchembroidery.com/resources/blog/whats-new-in-hatch-embroidery-3);
  Embrilliance auto-split (patented) [[EM-hoop]](https://embrilliance.com/Help/Platform%20Win%201143/multipositionhooping.htm);
  SWP positional-hoop split.
- **PE-DESIGN.** Split designs into fixed Design Page sections, multi-position hoops by area,
  jumbo frame sides. OpenStitch matches all three.
- **Idea.** Treat the overlap zone as a corridor; find the cut as a shortest path through the
  corridor over a cost field where stitch-free cells are cheap and stitches cost by density
  (Dijkstra on the 1 mm grid); let the user drag its mid-points; objects crossing the cut are
  clipped (shape level) or split at the nearest stitch (stitch level); add alignment stitches
  in a separate colour at the corridor's corners.
- **Effort.** M. **Value.** Medium (large-design users). **Licensing.** Check the
  Embrilliance patent's claims (auto-split for multi-position hoops) before shipping the
  object-level automatic split.

#### W2. Automatic hoop selection

- Smallest hoop in the Design Page list that contains the design, offered after import.
  Wilcom Auto Hoop [[ES-hoops]](https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Production/hoops/hoops-3.htm).
  OpenStitch has the hoop list (`BROTHER_HOOPS`): effort S, value Medium, generic.

#### W5. Vector import quality

- OpenStitch imports SVG as editable shapes; the professional products embed CorelDRAW or
  are Inkscape. The actionable gap is import fidelity (strokes to satin lines with the stroke
  width, dashed strokes to running stitches, gradients to F13, text to text objects) rather
  than a live link. Effort M, value Medium, generic. Sources: Pulse AI/CDR/DXF import
  [[DG17-notes]](https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG17);
  Ink/Stitch Stroke to Satin [[IS-satintools]](https://inkstitch.org/docs/satin-tools/).

#### W6. Machine formats

- Formats the surveyed products write that OpenStitch does not: Husqvarna VP3 and VP4,
  Barudan DSB/DSZ/U01, Tajima TBF, Happy TAP, Pfaff KSM, ZSK Z01, Melco EXP+ (Embrilliance),
  G-code (Ink/Stitch). Sources: [[ES-formats]](https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Management/formats/formats-6.htm),
  [[Embird-formats]](https://www.embird.net/sw/embird/formats.htm),
  [[IS-formats]](https://inkstitch.org/docs/file-formats/), [[EM-ess]](https://www.embrilliance.com/essentials).
  PE-DESIGN writes PES, PEC, DST, EXP, JEF, SEW, XXX, PCS, HUS, VIP, CSD, SHV; OpenStitch
  matches the writable ones. VP3 is the most requested in the home market (Husqvarna/Pfaff
  owners) and pyembroidery (MIT) has a reader and writer to use as the oracle: effort M.
  DSB/DSZ/U01/TBF/TAP/KSM are DST relatives: S each once a reference exists. Value Medium.
  Licensing: pyembroidery MIT, no copying of data tables beyond colour facts.

#### W7 / W8 / W9. Worksheets, batch conversion, thumbnails

- Operator worksheets with thread list, stitch counts and a barcode (Pulse, Wilcom,
  Ink/Stitch Print PDF [[IS-print]](https://inkstitch.org/docs/print-pdf/)): OpenStitch's
  catalogue and template printing is the base; effort S. Batch and command-line conversion:
  OpenStitch already has `stitchtool` and `library.convert`, ahead of most. Shell thumbnails
  on macOS and Windows are Blocked on `unsafe` FFI (ROADMAP item 10); Embird's Iconizer and
  Embrilliance's Thumbnailer show the demand.

## 4. Ranked top-15 backlog and phases

Ranking = user value × distinctiveness ÷ effort, with the constraint that nothing here
changes PE-DESIGN-compatible defaults: every new behaviour is a new attribute or command
that is off unless chosen, so the reference specs stay green. Effort is for one engineer.

| Rank | Backlog item | Notes | Effort | Value | Phase |
|---|---|---|---|---|---|
| B1 | Contour (offset) fill with ring, single-spiral and double-spiral strategies, join styles, echo outside | F1 | S–M | High | A |
| B2 | Guided fill with two guides, and stitch-angle maps (several direction lines per region) | F2–F4 | M (two guides), M–L (angle maps) | High | D |
| B3 | Satin corner handling (mitre / cap / lap with thresholds) and short stitches on inner curves for columns and regions | O3, O4 | M | High | B |
| B4 | Auto-routed running stitches (Euler path / route inspection), Redwork, then satin branching | D1, D2, O10 | M + L | High | C |
| B5 | Pull and push compensation model (mm per side, %, push), automatic underlay by width with first and second underlay, editable fabric table | D4–D6 | M | High | B |
| B6 | Density profiles (presets and custom) on fills and columns, two-colour blend command, hidden travel for open fills | F5, F6, F17 | S–M | High | A |
| B7 | 3D puff satin mode (density, caps, no underlay or pull, stop, ordering) | F10 | S–M | High | A |
| B8 | Crosshatch / curved crosshatch / double-zigzag lace as a region type | F7 | S | Medium | A |
| B9 | Maze and tiled meander fills, zigzag (fur) variant, seeded hand-stitch randomisation | F8, F9 | M + S | Medium–High | D |
| B10 | Satin line with variable width (width keys, two rails, tapered ends) | O2 | S–M | High | B |
| B11 | Closest join per object, auto start/end, continue digitizing, automatic overlap removal | D3, D7 | S + S | Medium | C |
| B12 | Carving lines drawn on fills, knockdown (laydown) fill | F11, F12 | S + S | Medium–High | A |
| B13 | Quality tools: density map, troubleshoot rules, thread-brand conversion, auto hoop | D15, W3, W2 | S each | Medium–High | E |
| B14 | Stitch-file resize with density recalculation and density tuning (segmentation of stitch blocks) | D14 | M | Medium–High | E |
| B15 | Lettering depth: baseline model, saved kerning pairs, team names from CSV, closest-point letter connections | D9–D11, D13 | M | High | E |

Deferred (below the line): F13 linear colour gradient, F14 tartan, F15 shape/vine fills,
F16 photo thread-painting (L; patent check), F19 sequin/chenille/bead (machine-specific),
D12 TrueType turning-satin rework (L; patent check; already on the roadmap as the exact
medial axis), W1 intelligent split with editable lines (patent check), W6 extra formats
(VP3 first), W5 vector import fidelity.

### Proposed phases

Each phase is one release train after the PE-DESIGN compatibility phases, lands behind new
attributes so the compatibility specs are unaffected, and ends with its own reference-free
regression specs (internal tier) and editor screenshots.

- **Phase A — Fill library (B1, B6, B7, B8, B12).** All inside `openstitch-design` with
  small inspector additions. Five new looks in a few weeks: contour/spiral, density profiles
  and colour blends, puff mode, crosshatch and lace, carving and knockdown.
- **Phase B — Satin quality (B3, B5, B10).** The model change for compensation and underlay,
  corners and short stitches, variable-width satin lines. This is the phase that changes how
  sew-outs look; it needs a sew-out test on the owner's machine.
- **Phase C — Routing (B4 running part, B11).** The graph router, redwork, closest join,
  auto start/end, overlap removal. Then satin branching as the long item.
- **Phase D — Turning fills (B2, B9).** Two-guide fills, angle maps, maze and tiled meanders,
  randomisation. Builds on Phase A's generators and Phase C's travel rules.
- **Phase E — Tools and lettering (B13, B14, B15).** Density map, troubleshoot, thread
  conversion, auto hoop, stitch-file resize, the lettering baseline model, kerning, team
  names, closest-point connections.

Follow-up research planned (one PR each): algorithm sketches with test cases for B1, B2, B3,
B4 and B5.

## 5. Licensing notes

- **Ink/Stitch is GPL.** This document uses its public documentation to describe behaviour
  and parameters. No Ink/Stitch source was read for this survey, and none may be read or
  copied when implementing: OpenStitch is MIT/Apache and re-implements every idea from the
  descriptions here ([LEGAL_AND_LICENSING.md](../LEGAL_AND_LICENSING.md)). Ink/Stitch's
  fonts, meander tiles and tartan data are GPL assets: OpenStitch draws its own.
- **Vendor names are trademarks or product names.** Florentine, Liquid, Accordion Spacing,
  Smart Corners, TrueView, Hatch, Sfumato, PhotoStitch, Reef, MultiWave, QuiltStipple,
  ExpressDesign, Save2Sew, Appliqadabra, AlphaTricks, Density Repair Kit, Steil Thickness,
  Fractal Fill and the like identify the vendors' features in this document only. OpenStitch
  commands use generic names (contour fill, guided fill, density profile, corner style,
  knockdown fill, thread conversion).
- **Patents named in the sources, to check by number before the related item ships.** The
  facts per patent (holder, dates, status shown, what claim 1 covers, the item it touches,
  a design-around in plain words) are in [beyond/patents.md](beyond/patents.md). Correction
  to the first version of this list: US 8532810 is Vistaprint's auto-digitizing
  fragment-ordering patent (priority 1998, shown expired), not a skeleton-satin patent, so
  D12 and OpenStitch's skeleton satin have no live patent on this list. Shown active and
  therefore the ones to read: Brother's US 7996103 (per-pixel angle features with a
  coverage-and-colour test for connectors; F16's direction-aware variant only, to 2030),
  BriTon Leap's US 9492937 / US 10023982 (appliqué cut files derived from stitch data; O7
  cut files from imported stitch files) and US 10358753 / US 11060220 (tracing a bitmap of
  stitch data to add outline or fill stitches; `object.toShapes`'s coverage trace, B12 on
  stitch blocks, B14's fill fallback, to about 2035). Shown expired or ceased: US 5541847,
  US 6934599, US 6587745, WO2005061774, US 6600966 (Embrilliance's hoop split). This is
  not legal advice; it is the list for the owner's lawyer.
- **No vendor artwork.** Pattern tiles, motifs, meander tiles, fonts and thread charts beyond
  colour facts are OpenStitch's own, as today.
- **pyembroidery (MIT)** remains the oracle for any new format (W6).

## 6. Sources

Grouped by product. Pages seen only through search summaries are marked (summary).

**Brother / PE-DESIGN**
- https://support.brother.com/g/s/hf/ped_info/ped11/one_click_update/en/ped11update_info.html
- http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/
- http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1185920.html
- http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1185303.html
- http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html
- http://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293378.html
- https://www.brother-usa.com/p/home/sewing-embroidery/pe-design-11-software
- https://www.brother-usa.com/blogs/stitching-sewcial/Stipple-Fill-Stitches
- https://support.brother.com/g/s/hf/mobileapp_info/mydesignsnap/mds_en.html?ap0=en
- https://brother-usa.com/home/sewing-embroidery/mobile-apps
- https://forum.embroideres.com/forums/topic/25500-too-bad-that-pe-design-seems-to-either-be-dead-or-dying/
- https://www.t-shirtforums.com/threads/trouble-with-pe-design-next.192989/
- https://wiki.cci.arts.ac.uk/books/etextiles-lab/page/pe-design-11/revisions/3139

**Wilcom EmbroideryStudio / Hatch / Bernina**
- https://wilcom.com/embroiderystudio/designing
- https://wilcom.com/embroiderystudio/lettering
- https://wilcom.com/embroiderystudio/e3comparison
- https://wilcom.com/embroiderystudio/elements/sequins-advanced/
- https://wilcom.com/resources/blog/top-10-new-features-of-wilcom-embroiderystudio-2025
- https://wilcom.com/resources/blog/natural-organic-hand-stitch-effect-for-machine-embroidery
- https://wilcom.com/resources/blog/get-creative-with-embroiderystudio-e45
- https://wilcom.com/resources/blog/closest-join
- https://wilcom.com/resources/blog/color-blending
- https://wilcom.com/resources/blog/push-and-pull-compensation
- https://wilcom.com/resources/blog/curved-fills-florentine-and-liquid
- https://wilcom.com/resources/blog/qa-from-3d-puff-embroidery-webinar
- https://docs.wilcom.com/embroiderystudio/e4/en/MainHelp/Decorative/curves/Create_curved_fills_with_Florentine_Effect.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Decorative/curves/curves-10.htm
- https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Decorative/specialty/specialty-14.htm
- https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Decorative/specialty/specialty-15.htm
- https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Digitizing/stitches/stitches-15.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/quality/quality-3.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/quality/quality-9.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/quality/quality-11.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/stabilizing/stabilizing-7.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/stabilizing/stabilizing-10.htm (summary)
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Quality/stabilizing/stabilizing-17.htm
- https://docs.wilcom.com/embroiderystudio/26/en/OnlineHelp/Quality/underlays/Underlay_Pull_Compensation.htm
- https://docs.wilcom.com/embroiderystudio/26/en/OnlineHelp/Lettering/lettering_create/Apply_baselines.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Lettering/lettering_custom/lettering_custom-2.htm
- https://docs.wilcom.com/embroiderystudio/e4/en/MainHelp/Quality/connectors/Apply_closest_join.htm
- https://docs.wilcom.com/embroiderystudio/e4/en/MainHelp/Production/hoops/Setting_auto_start_end.htm (summary)
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Production/hoops/hoops-3.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Mixed/applique/applique-1.htm
- https://docs.wilcom.com/embroiderystudio/28/en/OnlineHelp/Mixed/applique/applique-6.htm
- https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Mixed/applique/applique-13.htm
- https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Management/formats/formats-6.htm
- https://docs.wilcom.com/embroiderystudio/27/en/OnlineHelp/Automatic/automatic_photos/automatic_photos-8.htm
- https://hatchembroidery.com/features
- https://hatchembroidery.com/features/digitizing
- https://hatchembroidery.com/resources/blog/whats-new-in-hatch-embroidery-3
- https://blog.hatchembroidery.com/how-to-use-auto-fabric-or-create-your-own-custom-fabric-settings/
- https://www.echidnasewing.com.au/hatch-embroidery-digitizer-version-3-update-from-v
- https://www.bernina.com/en-AU/Campaigns-AU/V9/Difference-between-Embroidery-Software-8-and-9 (summary; bernina.com refused direct fetches)
- https://softwarehelp.bernina.com/en/MainHelp/PART_Digitizing/connectors/connectors-1.htm
- https://softwarehelp.bernina.com/en/MainHelp/PART_Digitizing/quality/Controlling_corner_stitching.htm
- https://softwarehelp.bernina.com/en/MainHelp/PART_Digitizing/quality/quality-5.htm
- https://www.swingdesign.com/pages/wilcom-embroiderystudio-2026-comparison-designing-decorating-editing-lettering

**mySewnet / Premier+ / Janome / Floriani / SewWhat-Pro**
- https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Area_Types.htm
- https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Line_Types.htm
- https://softwarehelp.mysewnet.com/MSW/100/Digitizing/Topics/Pattern_Fill_—_Options.htm
- https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Express_Border_Options.htm
- https://softwarehelp.mysewnet.com/MSW/140/Digitizing/Topics/Crosshatch_Fill.htm
- https://softwarehelp.mysewnet.com/MSW/100/Digitizing/Topics/Curved_Crosshatch_Fill.htm
- https://softwarehelp.mysewnet.com/MSW/140/Embroidery/Topics/Split_Embroidery.htm
- https://softwarehelp.mysewnet.com/MSW/100/StitchEditor/Topics/Split_Design.htm
- https://softwarehelp.mysewnet.com/MSW/120/FontDigitizing/Topics/Mitered_Corners.htm
- https://janome.com.au/product/janome-artistic-digitizer (janome.com and the DrawStitch manual refused fetches)
- https://installations.drawstitch.com/Artistic_Digitizer/manuals/v2_5/en/manual_embroidery_sequence.html
- https://www.mulqueen.com/shop/c/p/Floriani-Total-Control-Professional-Embroidery-Software---FTCU-x44586193.htm
- https://sandscomputing.com/products-shop/sewwhatpro-embroidery-editor/
- https://www.machineembroiderygeek.com/what-does-sewwhat-pro-do/

**Tajima DG / Pulse**
- https://embroidery.pulsemicro.com/dg15-levels.html
- https://fonts.pulsemicro.com/Home/Embroidery
- https://tajimaeurope.com/en/pulse-software-solutions/embroidery-software-tajima-software-dg16-pulse/illustrator-extreme/
- https://www.tajima.com/product/dg/
- https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG16
- https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG17
- https://dgml14.pulsemicro.com/pulse-products-12reasonsforpulse.htm
- https://dgml14.pulsemicro.com/pulse-products-13.1-whatsnew.htm
- https://community.pulsemicro.com/topic148-how-to-make-stitch-design-more-dense.aspx

**Embird**
- https://www.embird.net/digitizing-tools/
- https://www.embird.net/studio/manual/0075objects.htm
- https://www.embird.net/machine-embroidery-tutorials/studio-next/par_fill.htm
- https://www.embird.net/studio/manual/3125par_cmn.htm
- https://www.embird.net/machine-embroidery-tutorials/studio-next/par_out.htm
- https://embird.net/sfumato-stitch
- https://www.embird.net/machine-embroidery-tutorials/studio-next/par_sfu.htm
- https://www.embird.net/machine-embroidery-tutorials/common/font_engine.htm
- https://www.embird.net/sw/embird/tutorial/resclc/resclc.htm
- https://embird.net/sw/embird/tutorial/autosplt/autosplt.htm
- https://www.embird.net/sw/embird/formats.htm

**Ink/Stitch (documentation only)**
- https://inkstitch.org/docs/stitches/fill-stitch/
- https://inkstitch.org/docs/stitches/contour-fill/
- https://inkstitch.org/docs/stitches/guided-fill/
- https://inkstitch.org/docs/stitches/meander-fill/
- https://inkstitch.org/docs/stitches/circular-fill/
- https://inkstitch.org/docs/stitches/linear-gradient-fill/
- https://inkstitch.org/docs/stitches/tartan-fill/
- https://inkstitch.org/docs/stitches/ripple-stitch/
- https://inkstitch.org/docs/stitches/satin-column/
- https://inkstitch.org/docs/stitches/patterns/
- https://inkstitch.org/docs/satin-tools/
- https://inkstitch.org/docs/stroke-tools/
- https://inkstitch.org/docs/fill-tools/
- https://inkstitch.org/docs/troubleshoot/
- https://inkstitch.org/docs/visualize/
- https://inkstitch.org/docs/lettering/
- https://inkstitch.org/docs/print-pdf/
- https://inkstitch.org/docs/file-formats/
- https://inkstitch.org/en/2026-07-31-v3.3.0
- https://lists.inkscape.org/hyperkitty/list/inkscape-devel@lists.inkscape.org/thread/C7XJD67MCZNGO2WJOARMERURZ7GQYTXE/

**Embrilliance**
- https://www.embrilliance.com/stitchartist
- https://embrilliance.com/product/stitchartist-level-2-upgrade
- https://www.embrilliance.com/essentials
- https://www.embrilliance.com/enthusiast
- https://embrilliance.com/Help/Platform%20Win%201143/multipositionhooping.htm
- https://embrilliance.com/product/alphatricks/
- https://www.embrilliance.com/density-repair-kit

**Digitizing practice and algorithms**
- https://blog.ignitiondrawing.com/working-with-3d-puff-embroidery-part-1/print/
- https://blog.ignitiondrawing.com/working-with-3d-puff-embroidery-part-2/print/
- https://blog.ignitiondrawing.com/corners-and-capping/print/
- https://impressionsmagazine.com/process-technique/embroidery-tips-for-corners-and-capping/16307/
- https://support.coldesi.com/Portals/8/Documents/Liberty%20Docs/Underlay%20101.pdf
- https://mrxstitch.com/pull-compensation/
- https://babylock.com/amfile/file/download/file/1629/product/585
- https://www.iaeng.org/publication/IMECS2018/IMECS2018_pp117-122.pdf
- https://cgl.uwaterloo.ca/poster/xinling_SIGGRAPH11.pdf
- https://media.disneyanimation.com/technology/publications/2020/EmbroiderySiggraph20Official.pdf

**Patents (technique descriptions only; expiry to be checked)**
- https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/8532810
- https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/6934599
- https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/5541847
- https://image-ppubs.uspto.gov/dirsearch-public/print/downloadPdf/7996103
- https://patents.google.com/patent/WO2005061774A1/en
- https://patents.justia.com/patent/6587745

---

## Updates

### 2026-11-05 scan

Period checked: 2026-10-28 to 2026-11-05.

**Commercial products.** No material release or feature announcement in the period.
Checked: Wilcom blog (newest post 2026-10-08) and Hatch blog (newest 2026-10-25, a free-design
post); BERNINA (V9 still current, no V10 found); mySewnet (no 2026 release notes found);
Janome Artistic Digitizer (2.0 still current); Tajima DG (DG17 17.1.1 already listed on the
download page, nothing dated in the period); Embird (the "what is new" page now headlines
Embird 2026 but gives no date, so it is not counted here and is left for a full re-survey);
Ink/Stitch (newest post is v3.3.0 of 2026-07-31, already covered); Embrilliance (no dated
release found); PE-DESIGN 11 one-click update history (11.50 of September 2026, already covered).
Sources: https://wilcom.com/resources/blog, https://hatchembroidery.com/resources/blog,
https://inkstitch.org/news, https://www.embird.net/what-is-new/,
https://support.brother.com/g/s/hf/ped_info/ped11/one_click_update/en/ped11update_info.html,
https://cloud.pulsemicro.com/Download.

#### Open-source projects

First scan of each repository: baseline entries. Features were read from READMEs, release
lists and commit titles only. None of their code may be copied, ported or paraphrased into
OpenStitch; the GPL and unlicensed repositories are concept-only, exactly as Ink/Stitch is
(section 5).

- **vomitselfie/Morale** (GPL-3.0, Python/PySide6; created 2026-08, last push 2026-11-01;
  latest release v0.3.0 of 2026-09-27, a 0.4.0 release commit on 2026-10-30). A cross-platform
  digitizing app: artwork auto-digitizing, about 120 embroidery fonts plus system fonts,
  drawing tools, stitch preview and a pre-sew check. The 0.4.0 commits add blanket, meander
  and cross-stitch types, sharp satin corners sewn as fans instead of refused (O3; OpenStitch's Auto satin already
  sews a corner fan), staggered satin split points so long satins do not form a groove
  (OpenStitch has it: `splitLength` splits long rows into staggered stitches),
  travel kept inside fills with chosen start/end points (D3), test swatches of the selection
  (#452),
  copy/paste of stitch settings and "select same stitch type", stitch settings read from SVG
  layer and object names (W5, #455), and an INF colour file written beside EXP for Bernina.
  Ideas for OpenStitch: test swatch of a selection (#452), settings carried in SVG names
  (#455). https://github.com/vomitselfie/Morale (single source)
- **alexandra03/threadspool** (no licence: all rights reserved, concept-only; Python; created
  2026-07, last push 2026-07-21, no releases). An experiment where an LLM agent writes a
  design spec from a natural-language prompt and a deterministic engine compiles it to VP3,
  with a notes file of learned format facts. Relevant to OpenStitch's AI-feature track as
  prior art for prompt-to-design; also a VP3 writer (W6, OpenStitch ○). No issue: the
  Assistant (B17) already turns a prompt into a design through the engine's commands.
  https://github.com/alexandra03/threadspool (single source)
- **lork-fork/Roidery** (Apache-2.0, Rust; created 2026-10-14, last push 2026-10-20, no
  releases). An Illustrator-style digitizer built on a vector editor. Beyond OpenStitch today:
  two-rail satin blocks with draggable rungs and width tapering (O2, OS ◐), a "convert filled
  shape to centre-line satin" command (D12-adjacent; OpenStitch has it as Shapes ▸ Satin
  Skeleton from Shape, `object.deriveSkeleton`), a fill-angle drag tool (F4,
  OS ○), contour and circular fills (F1), garment mockups (T-shirt, cap, tote) exported as
  images for customers (new row, #454), a one-page production worksheet (W7), match-to-chart thread
  conversion with imported charts (W3, OS ○), and VP3, U01, TAP export plus DSB/DSZ/HUS reading
  (W6, OS ○). Optional download of the Ink/Stitch community fonts (OFL/GPL assets, not for
  OpenStitch). Licence is permissive, but the project rule still applies: ideas only.
  https://github.com/lork-fork/Roidery (single source)
- **bastanja/stitch_generator** (MIT, Python; created 2020, v1.0.0 on 2026-10-14). A library,
  not an app: turns paths with a width function into stitch effects (satin, motif, meander,
  scribble with noise) and offers "subdivision functions" that split a length into equal or
  varying segments shifted per row. Relevant to F8/F9 (meander, hand-stitch randomisation)
  and O5 (motif distribution); the documented catalogue of decorative stitch effects is a
  useful checklist for B9 (no issue of its own). https://github.com/bastanja/stitch_generator (single source)
- **EvenDog/openhoop** (GPL-3.0, Swift, macOS; created 2026-10-15, last push and release
  v0.9.3 on 2026-11-05, releases almost daily). A Mac digitizer that has sewn real designs on
  a Brother SE600. Beyond OpenStitch today: branching of touching shapes without trims (D1,
  OS ○ B4), short stitches on curves and split wide satin (O4), contour fills that spiral on
  cornered shapes (F1), colour blend from one thread into a second with a levels-style mix
  control (F6, OS ○ B6), arched lettering with several lines as concentric rings (D9, #456),
  envelope lettering and monograms, laydown and basting box (F12, OS ○ B12), 21 thread-brand
  charts (W3), multi-hooping with alignment marks (W1), user-named custom hoops, SVG import of
  `<use>`, `<style>` and gradients as gradient fills (W5), and every exported file read back
  and checked before saving (D15-adjacent, #457). https://github.com/EvenDog/openhoop (single source)
- **benkl/filum** (GPL-3.0, Rust engine + TypeScript UI, web and Tauri desktop; created
  2026-11-04, release 0.1.0 the same day). Its README states the Rust engine is a port of the
  Ink/Stitch algorithms, so it is GPL-derived: concepts only, never a source. Beyond
  OpenStitch today: auto-digitize presets (logo, patch, line art, photo, pixel art), 134
  bundled fonts, knockdown and basting (F12), a stitch-out player with a colour-block
  timeline, fabric calibration that sews a swatch and starts settings from measured values
  (new row, #453; ties into OpenStitch's own calibration-scan work), G-code and video export, and a
  browser build. https://github.com/benkl/filum (single source)

No comparison-matrix row changes: the matrix compares the surveyed commercial products and
Ink/Stitch; the open-source projects above are tracked here. New "Planned (beyond PE-DESIGN)"
rows in FEATURE_MATRIX.md and a ranking in ROADMAP.md: export read-back check (#457), selection
test swatch (#452), fabric calibration swatch (#453), arched text with several lines (#456),
settings from SVG names (#455), garment mockups (#454). Satin split staggering and convert fill
to centre-line satin were candidates but OpenStitch already has both.

#### Open-source projects: full survey (2026-11-05, second pass)

The owner asked for every missing feature and every place where these projects do better. Six
research passes, one per project, read each project's README, docs, wiki, user guide, every
release note and changelog, issue tracker and example files; code only to understand what a
feature does, never how. Each feature (544 in all) was checked against FEATURE_MATRIX.md,
ROADMAP.md, BACKLOG.md, this document's matrix and a search of `crates/` and `tools/`, and
classed as have, partial, missing or "improve" (we have it, theirs is measurably better). The
same features from several projects became one issue citing all of them. Result: 88 new issues
(#472–#559) and new details on 16 existing ones (#291, #292, #446, #447, #453, #454, #455, #456,
#457, #458, #460, #462, #463, #464, #469, #471). The ranking is in ROADMAP.md (Beyond PE-DESIGN),
the planned rows in FEATURE_MATRIX.md, the proposed batches in BACKLOG.md. Ideas only: none of
their code may be copied, ported or paraphrased; the GPL and unlicensed projects are
concept-only, and the thread charts and fonts some of them ship (from Ink/Stitch, GPL) are not
a source for OpenStitch's data either.

- **Morale** (GPL-3.0): 97 features; have 55, partial 28 (13 of them "improve"), missing 14.
  Main gaps: an in-app Check design with click-to-select (#503), density and coverage review
  (#504), test swatches stepping one setting (#507), branch splitting into satin columns
  (#526), the image wizard's per-region table with reasons and overrides (#489), richer SVG
  import (#487), autosave and recovery (#549), align and distribute (#472). Improvements: satin
  split on by default (#460), the inside-only corner fan (#520), meander spacing (#462),
  distinct artwork colours kept on separate threads (#447). Already in OpenStitch: underlay out
  and back, Half Stitch, travel inside fills, fill start and end points, E-stitch, cross
  stitch, the ripple centre, guides, copy/paste of stitch settings, design notes, catalogue PDF,
  batch conversion.
- **threadspool** (no licence): 42 features; have 26, improve 7, partial 3, missing 6. The
  first pass said no issue was needed; the second found concrete Assistant gaps: memory
  between conversations (#491), a bank of complete example designs and a composition guide in
  the skill (#492), illustrated benchmark tasks scored on the render (#493), every batch error
  reported at once (#494), only the user's own threads (#495); plus VP3 (#480) and single-line
  text (#530). Already in OpenStitch: prompt to design through engine commands, any agent
  harness, deterministic compile, self-check on the render, short-stitch filter, ties, trims,
  hoop bounds.
- **Roidery** (Apache-2.0): 75 features; have 39, partial 15 (8 "improve"), missing 21. Main
  gaps: Check Design with one-click fixes (#503), travel under later fills (#545), branching
  Convert to Satin (#526), short stitches on every satin (#521), a density × pull-compensation
  test grid (#507), the Fill Angle drag tool (#534), Remove Overlaps with a margin (#511),
  VP3/TAP/U01/DSB/DSZ (#480, #481), Ink/Stitch-format fonts (#529), garment mockup details
  (#454). Already in OpenStitch: the fills, satin underlays, the Sew-Out Player, the worksheet,
  headless SVG to machine file, the JSON control channel and MCP server, signed updates.
- **stitch_generator** (MIT, a library): 75 features; have 37, partial 18, missing 20, 3
  "improve" (stem as a rope and spacing by length along curves, #464). Main gaps: variable
  running stitch and back stitch (#542), hand-embroidery decorative lines (#543), motif
  placement patterns (#544), variable underlay (#523), scribble with smooth noise (#540),
  tatami row shifts (#537), stroke alignment (#524), a Python package (#498, idea).
- **openhoop** (GPL-3.0): 128 features from the user guide and all 36 release notes; have 53,
  partial 45 (22 "improve"), missing 30. Main gaps: smart satin corners (#520), colour blend
  (#532), Review Sewing Order (#505), density map (#504), branching (#546), test swatches
  (#507), automatic underlay by width and compensation in % (#523, #522), raised satin (#531),
  basting and laydown (#509, #510), Stop at Jumps (#512), free multi-hooping (#515), copies
  along a line (#476), hoop lists for other brands (#514), size-tuned fonts (#527), envelopes
  (#535), tabs (#552), a random-design crash hunt (#506). Its 21 thread charts come from
  Ink/Stitch (GPL) and are not a source for OpenStitch.
- **filum** (GPL-3.0, an Ink/Stitch port): 127 features; have 51, partial 42 (10 "improve"),
  missing 34. Main gaps: a thread- and fabric-aware density map (#504), a live Issues panel
  (#503), a pre-export check of hoop and colour limit (#457), fabric calibration from one card
  (#453), leaving out hidden stitching without cutting shapes (#511), the stitch-out player's
  colour-block timeline (#516), gradients and blends with dithering (#532), perceptual thread
  fitting and merging similar colours (#484, #485), auto-digitize presets (#489), command
  palette (#550), job sheets (#513), extra exports including video and G-code (#488).

### 2026-11-07 scan

Period checked: 2026-11-05 to 2026-11-07.

**Commercial products.** No material release or feature announcement in the period.
Checked: Wilcom blog (newest post still 2026-10-08) and Hatch blog (newest still 2026-10-25);
BERNINA (V9 still current); mySewnet and Embrilliance (no dated release found); Janome Artistic
Digitizer (2.0 still current); Floriani Total Control U and SewWhat-Pro (nothing new found);
Embird (the "what is new" page still headlines Embird 2026 without a date); Ink/Stitch (newest
post still v3.3.0 of 2026-07-31); PE-DESIGN 11 one-click update history (11.50 of September 2026,
already covered). Tajima DG by Pulse: the download page now lists DG17.5 (17.5.1.8445), whose
release notes are dated 2026-10-02, before the 2026-11-05 scan, so not counted as new; the
Appliqué Font Sewing Order it adds is already cited in section 3.2. Its other changes for a later
full re-survey: the Create Offsets dialog can make the offset directly as a run, satin (steil),
cross-stitch or original-type object, a right-to-left order for multi-line vertical text, and
smoother curve-to-straight transitions in the Enhanced Column tool.
Sources: https://wilcom.com/resources/blog, https://hatchembroidery.com/resources/blog,
https://inkstitch.org/news, https://www.embird.net/what-is-new/,
https://support.brother.com/g/s/hf/ped_info/ped11/one_click_update/en/ped11update_info.html,
https://cloud.pulsemicro.com/Download,
https://cloud.pulsemicro.com/Download/DownloadReleaseNotes?name=DG17.

#### Open-source projects

Ideas only, read from release lists and commit titles; no code may be copied, ported or
paraphrased (openhoop and filum are GPL-3.0).

- **vomitselfie/Morale**, **alexandra03/threadspool**, **lork-fork/Roidery**,
  **bastanja/stitch_generator**: no commits or releases since 2026-11-05.
- **EvenDog/openhoop** (GPL-3.0; releases 0.9.4, 0.10.0, 0.10.1, 0.11.0 and 0.12.0 between
  2026-11-06 and 2026-11-07). New since the full survey:
  - Join Lines (open lines or satin columns joined end to end into one object) and Break Apart
    (lettering, monograms, copies along a line and fills with holes split into parts). OpenStitch
    has text to shapes, ungroup, split along a cut outline, the knife, continue drawing (#479) and
    compile-time `joinLines`, but no edit command for either: new issue #744 (new row).
  - Colour blends that are radial, contour-shaped, follow a fill's guide line or run on wave rows,
    sewn as dense as plain tatami: F6 / B6, comment on #532.
  - Six hand-embroidery line styles (chain, feather, cross stitch, herringbone, ladder,
    candlewick); herringbone and ladder are the ones OpenStitch lacks: O-row family, comment on
    #543.
  - Picture tatami patterns whose needle holes draw diamonds, hearts and similar shapes, plus 18
    more tatami textures: already #537. Recognize Objects (imported stitches turned into resizable
    fills and satins): OpenStitch has it (rebuild shapes from stitches). Only-underlay view: done
    (#517). A Design Library window of folder thumbnails: OpenStitch has the Design Database.
    Content additions (11 fonts, 13 line-art drawings, motifs, basic shapes, monogram ornaments):
    not features; OpenStitch's own content is tracked elsewhere.
  https://github.com/EvenDog/openhoop/releases (single source)
- **benkl/filum** (GPL-3.0; 0.2.0 on 2026-11-05, commits to 2026-11-06). New since the full
  survey: a find and filter box for the layers list (new issue #745, Sewing Order list); a
  cut preview with a thread-cutter setting, uncut jumps drawn as thread and trim marks (OpenStitch
  has trim marks #517 and Stop at Jumps #512: no issue); a finished-piece view (OpenStitch's
  Realistic view); gradient colours sharing one set of rows and rows along the gradient flagged
  (comment on #532); vector editing additions (curvature tool, segment bending, join, average
  and compound paths; join folded into #744). https://github.com/benkl/filum/commits/main
  (single source)

No comparison-matrix row changes (the open-source projects are tracked here, not in the matrix).
New "Planned (beyond PE-DESIGN)" rows in FEATURE_MATRIX.md and a note in ROADMAP.md (Beyond
PE-DESIGN): Join Lines and Break Apart (#744), find and filter in the Sewing Order list (#745).

### 2026-11-08 scan

Period checked: 2026-11-07 to 2026-11-08.

**Commercial products.** No material release or feature announcement in the period.
Checked: Wilcom blog (newest post still 2026-10-08) and Hatch blog (newest still 2026-10-25);
BERNINA (V9 still current, no V10 found); mySewnet and Embrilliance (no dated release found);
Tajima DG by Pulse (DG17.5, 17.5.1.8445, already noted on 2026-11-07); Ink/Stitch (newest post
still v3.3.0 of 2026-07-31); PE-DESIGN 11 one-click update history (11.50 of September 2026,
already covered).
Sources: https://wilcom.com/resources/blog, https://hatchembroidery.com/resources/blog,
https://inkstitch.org/news, https://cloud.pulsemicro.com/Download,
https://support.brother.com/g/s/hf/ped_info/ped11/one_click_update/en/ped11update_info.html.

#### Open-source projects

Ideas only, read from release lists and commit titles; no code may be copied, ported or
paraphrased (openhoop and filum are GPL-3.0).

- **vomitselfie/Morale**, **alexandra03/threadspool**, **lork-fork/Roidery**,
  **bastanja/stitch_generator**: no commits or releases since 2026-11-07.
- **EvenDog/openhoop** (GPL-3.0; no new release after 0.12.0, commits on 2026-11-08):
  - Hand stitch for fills: fill rows that look sewn by hand. OpenStitch has the hand-stitch look
    on running and triple lines and on one side of satins (#540), not on fills: new issue #817
    (new row).
  - Recognize Objects (turning imported stitches back into objects) now measures each satin's
    spacing, reads a fill's tatami pattern from its needle holes and closes satin rings.
    OpenStitch's `object.toShapes` blocks recover the region type and direction but not the
    spacing or pattern: new issue #818 (new row).
  - Contour fill spiralling corner to corner on stars and pointy shapes (F1, already in the
    2026-11-05 baseline); interface tidying (press-and-hold tool choices, "mm" on every setting,
    a hoop-fit warning for line art): not material.
  https://github.com/EvenDog/openhoop/commits/main (single source)
- **benkl/filum** (GPL-3.0; no new release after 0.2.0, commits on 2026-11-07): its Issues panel
  now finds lines sewn on top of each other (OpenStitch's Check Design reports `stacked` needle
  drops, #503: no issue); editing several objects at once (OpenStitch's Inspector edits a
  multi-selection); per-object lock stitches (OpenStitch has per-object ties, #548); pull
  compensation off until a fabric is calibrated (a default choice, noted on the fabric
  calibration idea #453 only if that work picks it up); Ink/Stitch default settings adopted
  and a third bug scan: not material. https://github.com/benkl/filum/commits/main (single source)

No comparison-matrix row changes (the open-source projects are tracked here, not in the matrix).
New "Planned (beyond PE-DESIGN)" rows in FEATURE_MATRIX.md and a note in ROADMAP.md (Beyond
PE-DESIGN): hand-stitch look for fills (#817), rebuilt objects keeping the original's sewing
(#818).

### 2026-11-11 survey: lb2002/openstitch and linker2k/myembro

The owner asked for a full feature survey of two new open-source projects, by the method of the
2026-11-05 second pass: every user-facing feature listed from the README, docs, user guide, every
release note and changelog, the issue tracker (both trackers are empty) and the example files;
code read only to learn what a feature does (menu and message texts), never how. Each feature
was checked against FEATURE_MATRIX.md, ROADMAP.md, the open and closed issues and a search of
`crates/` and `tools/`, and classed as have, partial, missing or "improve" (we have it, theirs is
measurably better). Ideas only: none of their code, data, fonts, thread charts, machine tables,
sample designs or artwork may be copied, ported or paraphrased. Result: 20 new issues
(#865–#884, all milestone Future) and notes on three existing ones (#453, #486, #555).

#### lb2002/openstitch ("OpenStitch Studio")

- **What it is.** A desktop digitizing program in Python (PySide6) for Windows, macOS and Linux.
  It shares OpenStitch's name and is a different, unrelated project: never mix the two up in
  issues, links or release notes. Picture-to-embroidery, manual fill, satin and running tools
  with a node editor, lettering, a "virtual stitch-out" check, and export for home and
  commercial machines.
- **Licence as found.** GitHub detects no standard licence ("Other"). The LICENSE file and the
  README state AGPL-3.0-or-later, with the author keeping the right to sell commercial licences.
  A repository with no licence at all would be all rights reserved; this one is copyleft, so
  either way it is concept-only for OpenStitch, exactly as Ink/Stitch is (section 5).
- **State.** Created 2026-10-27, last push 2026-11-05, one star, no issues or forks. Releases
  v0.1.0 (2026-10-27), v0.3.0 (2026-10-28) and v0.4.0 (2026-10-31). Its own README says nothing
  has been sewn on a real machine yet.
- **Counts.** 87 features: have 60, partial 19 (3 of them "improve"), missing 8.
- **Main gaps (missing or partial), with their issues.**
  - A fabric pull preview in the design check: how the fabric will draw in, where an outline and
    its fill will drift apart, a puckering warning, every finding tagged as a fixed rule or an
    estimate, and a calibration from one sewn and measured test bar (#865, new row).
  - More check rules and one verdict: thread ends without ties, sections under three stitches,
    very short stitches, more colours than the machine has needles, fabric showing through a
    fill, small text judged per font, and a score out of 100 (#866).
  - Send to machine with a short name for older machines, the format chosen from the machine,
    the thread order shown and no silent overwrite (part of #867).
  - Machine profiles that carry a needle count and a format (part of #868).
  - Hoop "strategies": on caps, sewing centre-out and bottom-up; on jacket backs, a stronger
    underlay and more compensation (#874, new row).
  - 3D puff over foam with perforating end caps (#875; OpenStitch's planned B7, which had no
    issue; raised satin #531 is the first step).
  - Photos as wavy lines in one thread, two (sepia) or four (cyan, magenta, yellow, black)
    (#873, with myembro's thread sketch).
  - Floppy-disk images for USB floppy emulators and a small network bridge for old commercial
    machines, with a choice of trim style in EXP (#876, low priority).
  - A guided tour that lights up the real controls (#877); a crash report with "your work was
    saved" and Help ▸ Report a problem (#878).
  - Auto-Fit to the hoop and "pick the smallest hoop that fits" (#879; the open part of B13).
  - Tests: every setting must change the stitches (this found a density setting of theirs that
    did nothing), and random clicks and keys on the real window (#880).
- **Improvements (we have it, theirs is better).** Nine interface languages, starting in the
  system language, with numbers in each language's format (OpenStitch: #555, partly done; noted
  there). Calibration by a measured width instead of choosing a row by eye (#865; noted on
  #453). The send-to-machine window's checks (#867). A Gunold Poly thread chart (noted on #486,
  only from the maker's own values).
- **Already in OpenStitch.** Picture digitizing with perceptual colour reduction and thread
  matching, stitch type by stroke width, satin with turning angles, short stitches, split satin
  and underlay chosen by width, tatami and contour fills with travel inside the shape, pull
  compensation, hidden stitching left out with an overlap (#511), fabric presets with backing
  and needle advice (#508, with more fabrics than theirs), appliqué, small single-line text
  (#530), SVG import, lettering on arcs, the design check with Show and Fix (#503), the density
  map (#504), export read-back (#457), thread charts and CSV import, hoops including round ones
  (#514), production sheets (#513), DST/PES/PEC/EXP/JEF/VP3/U01, start cards and samples (#554),
  light and dark themes, autosave and recovery (#549), realistic view and PNG export, the
  simulator, undo, colour-order optimising that respects overlaps, drag and drop of files.

#### linker2k/myembro

- **What it is.** An Android app in Kotlin (Jetpack Compose) that views, combines, letters and
  digitizes designs for Brother machines on a phone and sends them to the machine. Its stitch
  engine is, by its own README, a port of Ink/Stitch and other GPL and open tools, so it is
  GPL-derived twice over.
- **Licence as found.** GPL-3.0-or-later (LICENSE file, README and the repository description;
  GitHub detects GPL-3.0). Concept-only. Its 72 embroidery fonts come from Ink/Stitch and its
  136 sample designs and emoji art are its own or third-party content: none is a source for
  OpenStitch.
- **State.** Created 2026-10-22, last push 2026-11-08, one star, no issues. Eleven releases,
  v0.4.0-beta.1 (2026-10-27) to v0.4.0-beta.11 (2026-11-07), after earlier alpha builds. Sewn on
  one real machine (a Brother SE400), including sending over a USB cable.
- **Counts.** 82 features: have 52, partial 17 (4 of them "improve"), missing 13.
- **Main gaps (missing or partial), with their issues.**
  - Sending rules taken from Brother's manuals: FAT32, top folder, about 200 items, plain file
    names, free space; a machine connected by USB cable seen as a small disk, with a list to
    choose from when it is full; a transfer history and copyable diagnostics (#867, new row).
    This matters for the "Transfer directly to machine memory" row, which is Blocked as
    undocumented: for models with a USB-to-computer port no protocol is needed.
  - A machine list by model name (58 Brother models with area, hoops and connection, each
    marked tested or "from the manual") (#868, new row).
  - One backup ZIP of designs, projects, fonts and settings, restored on another device (#869).
  - "Ask AI" without any key: the app writes the request for any chatbot, the reply is pasted
    back, checked, shown as before and after with the check result, then applied as one undo
    step (#870, new row).
  - An emoji picker with search that turns a drawing into a design at a chosen size (#871).
  - A list of free-design sites with each site's terms as plain tags (#872).
  - A "thread texture" photo mode: the picture as layered thread lines, like a sketch (#873).
  - Korean and Japanese kana lettering sewn as satin along each stroke's centre line, in writing
    order, with trims only between separate groups (#881).
  - Font list with favourites, filters by kind and script, fonts lacking a typed character
    greyed out with the reason, the licence shown and fonts fetched on demand (#882).
  - Design list with favourites, a "fits my hoop" filter, sorting, and import of ZIP archives
    and folders with duplicate detection (#883).
  - Phone-only features (touch gestures, share-sheet intake, the photo picker, phone-to-machine
    over USB-OTG, voice input, a chatbot relay, drafts kept by the system) are one list: #884.
- **Improvements (we have it, theirs is better).** 136 ready samples against OpenStitch's small
  kit (content, tracked with the kit; the browser's samples are #833). USB sending rules and
  the cable route (#867). The machine model list (#868). Its sew-out check blocks saving and
  sending on an error where OpenStitch warns unless `verify: strict` is set (part of #866).
- **Already in OpenStitch.** PES (all versions), PEC, DST, JEF, EXP and VP3 read and written;
  realistic view, simulator and stitch-out player; statistics and sewing time per colour;
  placing, scaling, rotating, mirroring, aligning, distributing and snapping objects (#472,
  #473); the sewing-order list; thread and fabric colours; custom hoops and multi-position
  hoops; picture digitizing as silhouette, outline or colours with noise and detail controls,
  distinct colours kept on distinct threads (#447, #484), My Threads with a list of threads to
  buy; per-region stitch type, texture, angle (#534) and order; lettering with size ranges
  (#527), single-line fonts and user TrueType fonts including CJK glyphs; ties, trims only where
  needed (#548), travel under later stitching (#545); the design check with locations (#503);
  autosave (#549); themes; an MCP server and the Assistant.

No comparison-matrix row changes (the open-source projects are tracked here, not in the matrix).
New "Planned (beyond PE-DESIGN)" rows in FEATURE_MATRIX.md and a note in ROADMAP.md (Beyond
PE-DESIGN) for #865–#884. Not verified here: neither program was run (no screen control; the
Android app needs a phone), so their features are as their own documents describe them, and
neither project shows a sew-out of most of what it lists. Sources:
https://github.com/lb2002/openstitch (README, CHANGELOG, release notes; single source),
https://github.com/linker2k/myembro (README, docs/manual/user-guide.md, docs/faq.md, release
notes; single source).
