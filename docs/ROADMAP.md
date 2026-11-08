# Roadmap

| Phase | Goal | State |
|---|---|---|
| 1 Tooling | Rust workspace, CI on three OSes, fixture generators | Done. |
| 2 Reference research | Guide-based feature inventory; reference files from a licensed PE-DESIGN 11 | Done: inventory from the public guide; reference files made in a licensed PE-DESIGN 11.50 since 2026-10-27 (`compat/reference/`, provenance in `MANIFEST.json`). |
| 3 Core engine | Stitch model, threads, document operations, stitch generation | Done: design model, all guide sew types, compiler, command registry, undo, `.osd` projects, CLI and JSON control, MCP. |
| 4 File interoperability | Every format PE-DESIGN 11 reads or writes; round-trip tests | DST, EXP, JEF, PCS, SEW, XXX, HUS and VIP written record for record as PE-DESIGN 11.50 exports them (reference tier, 2026-10-28), CSD and SHV too since 2026-11-04 (#320); PES, PEC and the readers at oracle tier with fuzzing; PHC read-only, as in PE-DESIGN; PES v1/v6 object section; PE-DESIGN 11 (`#PES0110`) metadata, thread table and editable shapes read, and shapes written as editable objects (`pesVersion` 11, 2026-10-29, unverified in PE-DESIGN); SVG and image import; split exports. |
| 5 Desktop interface | Cross-platform editor (egui) | Done: drawing, Bézier pen and point editing, selection, inspector, object list, undo, simulator, wizards, Design Database mode, Stitch Creator, Stitch Design Factory, printing, USB transfer, self-update, unsaved-changes prompt on close/new/open (macOS: Cmd+Q and Cmd+W too, no system menu). |
| 6 Advanced | Digitizing, lettering, complex fills, suite programs | Done as independent approximations: Auto Punch, Cross Stitch, Photo Stitch, Design Center; full lettering suite and Font Creator. |
| 7 Validation | Reference suite, gap closing | In progress: the compatibility harness (`stitchtool compat`) compares OpenStitch's output with PE-DESIGN 11.50's files spec by spec; [COMPATIBILITY_STATUS.md](COMPATIBILITY_STATUS.md) shows the tier each row has reached. No sew-out on a machine yet. |

## Platform decision (2026-10-18)

The target is every OS, so OpenStitch is a Rust workspace (`crates/`) with one egui/eframe app
for macOS, Windows and Linux. CI builds and tests all three.

## Done (2026-10-19)

- **Drawing and editing:**
  - Bézier outlines and point editing; arcs and preset shapes; group/ungroup; arrange copies
    with a canvas preview and drag placement; snap to grid.
  - Merge, remove overlap, boolean combine, split, offset lines (`i_overlay`).
  - Paste sewing attributes; entry/exit points with draggable markers.
  - Floral pattern and original decorative frames; embossing, engraving and stamps.
  - Sewing order (`order.move`, `order.byColor`, `order.optimize`) with per-object jump trimming;
    `order.optimize` in PE-DESIGN's mode by default, with OpenStitch's shortest-jump (2-opt) mode
    as a design setting (beyond PE-DESIGN, #137).
  - Sewing direction (round fill, centre and reference points); Manual Punch; Satin Skeleton
    tool and skeletons derived from shapes.
  - User settings: saved attribute sets and user thread charts.
- **Data types:** conversions between shape, text and stitch patterns, region → line, and
  stitch-pattern editing (needle points, split, connect, divide by colour).
- **Stitch types:**
  - Line types: motif (own library), stem, candlewicking, E/V, chain, cutting, zigzag half stitch.
  - Satin and fill options: auto direction, underlays, gradation, feather, split, travel.
  - Region types: programmable fill, motif fill, piping, cross stitch, concentric circle,
    radial, spiral, flexible spiral, stippling, net fill, zigzag net fill, decorative fill;
    concentric circle and spiral calibrated to PE-DESIGN 11.50 (#138, 14 references;
    open: the spiral's inner-turn needle counts, the concentric chord splits and its seam on
    Bézier outlines; the spiral's winding and closing run settled with round 5; tie-in and lock
    per run, grazing cut circles and the triangle spiral settled 2026-10-31; round 6 added the
    outside-in concentric order and 12 specs, the per-turn and per-circle needle counts still
    open; round 7 documents the spiral-round-a-hole band, to be sewn by default once PE's turn
    sampling is known, with an OpenStitch mode (`compatMode`) keeping the single travel, listed
    under Beyond PE-DESIGN when it lands).
- **Lettering:** text objects with Lato Bold and Pacifico (SIL OFL), text on a path, user
  TrueType/OpenType fonts (CJK through any font with the glyphs), shaping of right-to-left and
  complex scripts (`rustybuzz`, `unicode-bidi`), kerning, vertical text, per-character edits
  (on the canvas with Point Edit),
  user-mapped text, monograms, Name Drop, templates and the Template Wizard, Font Creator (including
  characters traced from an image in one step).
- **Wizards:** background fill (echo, stippling, decorative, crosshatch, stripes), appliqué,
  embroidered patch, cutwork.
- **Suite programs:** Programmable Stitch Creator (`openstitch.pattern`), Stitch Design Factory
  (`openstitch.sewing`, SVG and experimental PMV export and import), Design Database (`openstitch-library`:
  browse, search, organise, convert, catalogues; `library.*` commands).
- **Machine and workspace:** machine types, Brother and user hoops, sewing area, thread length
  and sewing time, measure tool, rulers, grid, Reference window, realistic preview; split
  designs, multi-position hoops and jumbo frame; printing (actual size, reduced, positioning
  sheets, Print and Stitch; `openstitch-print`); transfer via USB media.
- **Automation:** per-command capabilities (read-only mode, file access off) for MCP and
  `stitchtool serve`; loopback socket transport for the live editor (`openstitch --listen`,
  per-launch token, file roots; `stitchtool connect`, `openstitch-mcp --live`).
- **Distribution:** per-OS packages on every merge to `main` (macOS universal `.app`, Windows
  MSI + zip, Linux tarball with thumbnailer, Linux AppImage with per-user desktop integration);
  Flatpak manifest built in CI; tag-only draft releases.

## Done (2026-10-27)

Everything below was measured black-box on a licensed PE-DESIGN 11.50 through its normal
interface, with the resulting files committed under `compat/reference/` and compared by the
harness. The details per item are in [COMPATIBILITY_AUDIT.md](COMPATIBILITY_AUDIT.md) and
[COMPATIBILITY_TESTS.md](COMPATIBILITY_TESTS.md).

- **Fill:** PE-DESIGN's defaults (4.5 lines/mm, under sewing on), its under sewing structure,
  density and pull-compensation ranges and effects, Frequency as a row-to-row needle shift,
  gradation as PE's Max/Min pattern with a linear fall, the feathered edge's width and the
  guide's exclusions.
- **Satin:** PE-DESIGN's sawtooth, tie-in and lock, under sewing, section order, split at the
  start point, feathered edge; Auto satin's spacing, corner fan, band ends and start travel.
- **Lines:** PE-DESIGN's defaults, tie-in and lock for running, triple, zigzag (mitred rails,
  under sewing, half stitch), E/V, stem, candlewicking and chain.
- **Lettering:** text size (0.7 em) and line spacing (% of the character height), the text
  tool's sewing defaults (Satin, density 5.0, no under sewing), sewing order inside a text, Fit
  Text to Outline's options, 24 of the 26 Text Transform presets.
- **Shapes and order:** the shape tools' start and end points; Optimize Sewing Order keeps the
  drawing order within a colour; direction angles turn counter-clockwise on screen.
- **PES:** PE-DESIGN opens OpenStitch's v6 files with their stitches, colours and Design
  Property; Design Property strings in Windows-1252 with CR LF; no zero-length stitch records;
  trims kept PEC-only as PE-DESIGN does; the reader parses PE-DESIGN 11's `#PES0110` metadata,
  thread table and editable shapes (`CEmbLine`; chapter "PES 0110 object list" in
  [FILE_FORMATS.md](FILE_FORMATS.md)); a missing PEC section is an error.
- **Editor and project:** OpenStitch logo, command icons and illustrations generated with
  Higgsfield (code-drawn icons as the fallback); DirectX 12 on Windows with OpenGL as the
  fallback; a gitleaks job in CI; the editor's tests down from ~45 to a few minutes per CI
  run (optimised dependencies and stitch pipeline in debug builds, one stitch compile for the
  demo design); the repository renamed to `cristianonescu/OpenStitch`;
  releases v0.1.0-alpha.1 and v0.1.0-alpha.2 ([docs/releases](releases/)).
  2026-11-07 (#705, `batch:editor-2`): the app's binary tests no longer touch the OS clipboard
  (their parallel use of macOS's pasteboard crashed the test binary now and then), and their
  temporary folders are per test.

## Still to do

Assets (owner's rule of 2026-10-30, [research/asset-audit.md](research/asset-audit.md)): the
audit is done in three rounds (102 command icons for commands that showed another command's
icon, including the suite windows' own commands, and ten empty-state and dialog illustrations,
all generated with Higgsfield, 295 credits). New windows and commands get their icon the same way
when they are added.

Compatibility, from the open items in [COMPATIBILITY_AUDIT.md](COMPATIBILITY_AUDIT.md):

WMF/EMF import (#321, 2026-11-04): landed as `import.metafile` (own safe-Rust record player;
File ▸ Import SVG… opens `.wmf`/`.emf` too). Next: a PE-DESIGN capture of a WMF and an EMF import
to check PE's sizing and line/region choice for metafiles.

Sew-outs (first one 2026-11-02, the ready-to-sew kit on the owner's NV880E): every PES now has a PEC
frame of the design's size holding its stitches (2026-11-03), stippling and the flexible spiral no longer stitch in place, and
`tools/sew_check.py` checks a file for all three plus stacked needle drops. The second sew-out
(2026-11-04) added tie-ins and end locks to every trimmed run, a zigzag net as PE-DESIGN sews it,
the T1 card's corrected swatches and mark, and the first calibration: the Realistic view and the
similarity scorer draw thread as wide as it covers at its row density. Next: Photo Stitch 1's
trims (#298), Auto Punch's stacked satin points on the storybook picture (#290, the Mac's track),
then a sew-out of the regenerated kit.

Done 2026-11-06 (#295, #441, #311, #312): the kit has a T2 card (two threads in alternating rows,
to measure how far a later row covers its neighbour for Photo Stitch 1), the build checks every
calibration swatch's density in the exported file, and `sew_check` checks trims per 1,000
stitches and hairline satins with limits from PE-DESIGN's own files. Next: sew the T2
card and set `strip` from it; engine follow-ups #627 (Design Center line-art trims) and #628
(entry travel across a region).

Done 2026-11-06 (`batch:engine-bugs-3`, #570, #571, #466, #627, #633; #628 in part): start and end
points left off a reshaped outline snap to it (the four random-design seeds pass); zigzag lines
no longer fold at cusps and their under sewing bevels sharp corners; skeleton satin takes Half
Stitch at an apex; Design Center joins line art into one path per figure (`joinLines`, the
butterfly 60 → 7.8 trims per 1,000 stitches); Along / On outline walks a fill's entry travel and
a programmable fill's under sewing joins along the outline. Open on #628: PE-DESIGN's straight
entry travel stays the default (its sweep-fill-density-min reference); owner decision whether
open rows should leave it.

Done 2026-11-05 (#433): Photo Stitch 1 in OpenStitch mode is fast again after the locks of
#299: its fitting attempts run at once and its local search scores only the cells in reach,
with the same stitches as before (High on the issue's stripes 899 → 41 s, Standard 258 → 24 s,
on a loaded machine).

Done 2026-11-05 (`batch:photo-stitch`, #298, #468): PE-DESIGN-compatible Photo Stitch 1 travels
hidden between a thread's runs instead of jumping (trims per 1,000 stitches 11–46 → 3–15 on the
nine photos), and OpenStitch mode staggers the needle points of runs that are a whole number of
Run Pitches long (no more columns of needle drops on the sunset). Measured and not taken:
Photo Stitch 2 without its running-stitch floor and on the T1 coverage curve (#129), and a
general sewn-colour model from the six T1 threads (#291); details in COMPATIBILITY_AUDIT.

Measured 2026-11-06 (`batch:mac-photo`, #298, #291, #129, #468), nothing taken: Photo Stitch 1
travel across up to 2 mm of a similar earlier colour saves trims only by costing likeness
(−20 % trims for −0.7 points); `tools/image_similarity.py --scan` scores the sewn sunset patch
against its render, where the calibrated widths read too light in the dark middle (50.3 against
the flat render's 55.6); Photo Stitch 2's layered coverage solve scores lower (39.7 → 37.2).
Details in COMPATIBILITY_AUDIT and the image-similarity plan.

Done 2026-11-06 (`batch:stitch-engine-4 (2026-11-06)`, #567, #459, #467, #470, #471, #368,
#609): moving a shape moves its stitches and nothing else (each object is sewn in a frame at
its own corner, needle points 0.05 mm apart are one penetration wherever they fall on the
export grid, float-noise ties no longer flip a stitch split or a row order, and a line's end
point moves with it); fill and piping along a direction curve sew rows that are the curve's
offsets, a row spacing apart all along it, never crossing and straight past its ends;
programmable fill can split its crossing-to-crossing stitches at `splitLength` (beyond
PE-DESIGN, which keeps them whole); the laurel and star frames sew each piece in one path; the
appliqué wizard can sew a kept pattern on top of the fabric (`patterns: last`; PE-DESIGN and the
default sew it first, under the fabric); hairline satin and outlines sew as a running line and a
lock after a tiny last stitch steps a full 0.3 mm (the storybook passes `sew_check`); the arched
text is the same on all three systems (#609, verified on Linux: the spec's reference re-save is
in the repository). Left for later: programmable and motif fill along a direction curve still
bend their tiles by the chord, and emboss and engrave strength against PE-DESIGN (#471).

Done 2026-11-05 (#460-#464): the stitch-engine follow-ups of the review. Narrow Auto satin no
longer starts with a 39.5 mm stitch, Auto Punch sews regions whose satin would cross more than
10.5 mm as fill (PE-DESIGN's own maximum is 10.4 mm), and `sew_check` warns on stitches over
12 mm (PE-DESIGN sends wide satin whole, so it is not split by default); concentric, spiral,
contour and radial fills end without a run back out, scatter their ring joins and thin the radial
centre; stippling keeps its spacing and fills to the outline; cross stitch regions sew whole X's
on one page-axis grid and zigzag nets stay inside the outline; stem, candlewick and chain are
spaced along curves. Left for later: what Direction should do on a cross stitch region (needs a
PE-DESIGN reference) and cross kinds (#614), travel that crosses a hole between separate pieces
of a fill (#615), the zigzag net's turned grid against PE-DESIGN's (#616), motif lines' runs of
short stitches (#617), and a Beyond-PE rope stitch (#618). Done 2026-11-07 (`batch:fills-1`):
#615 (a regression test; joins already stay inside the band), #616 (the net on the page axes,
within 0.1 mm of PE-DESIGN's), #617, and two stippling bugs found on the way: seed 993's runaway
allocation (#638) and a narrow band that sewed nothing (#708). #614 continues: PE-DESIGN's cross
stitch has no Direction and only Times (single, double, triple), captured 2026-11-07.
Done 2026-11-08 (`batch:fills-2`, #532 part 2b): the Gradient tool (Shapes ▸ Edit Outline): drag
across a gradient fill to set where it starts and ends; every colour follows in one undo step.
With it, #532 is done.

Done 2026-11-08 (`batch:fills-2`, #532 part 2a): named gradation shapes (`gradationShape`: convex,
concave, centre, edges, wave; linear stays PE-DESIGN's) with PE-DESIGN's row placement, and
`import.svg` `gradients: "blend"` turning SVG gradients into gradient fills along their own axis.
The canvas Gradient tool follows.

Done 2026-11-08 (`batch:fills-2`, #532 part 1): colour blends and gradient fills. `shape.gradient`
and `shape.blend` make one fill object per colour; each sews its share of one shared set of rows
(rows, ordered, noise or band dithering), so the colours together are one even layer. Named
density profiles, the canvas Gradient tool and SVG gradient import follow in part 2.

Done 2026-11-07 (`batch:fills-2`, #537): tatami textures, 14 named needle-hole patterns on plain
fills (smooth is PE-DESIGN's Frequency brick, the rest are OpenStitch's) built from stepped and
wave, triangle or arc row shifts, off by default, with kit swatches.

Done 2026-11-05 (#450): every kit, demo and screenshot design reviewed at full size (stitches alone
at 15 px/mm and the Realistic view). Kit and demo designs fixed, built-in templates regenerated,
CI checks that the committed kit is what the code makes and passes `sew_check`. Engine follow-ups,
in rough order of what shows most: Diamond/Circle monograms (#458), programmable fill (#459),
satin stitch length (#460), centre-out fill seams (#461), stippling (#462), cross stitch and
zigzag net regions (#463), line types (#464), small lettering (#465), satin cusps (#466), frame
trims (#467), Photo Stitch density and stagger (#468), image tools' edge cells and thin features
(#469), appliqué "add" order (#470), direction-curve fills (#471).

Done 2026-11-07 (`batch:fills-2`, #538 wave part): wave fill. A fill's `waveHeight` and
`waveLength` make its rows ripple as one sine across the row direction; the region is sheared
straight, sewn as a plain fill and sheared back, so under sewing, stagger and travel carry over
and height 0 is the plain fill. The spacing across the rows stays within ±15 % (40° slope cap).
The flow fill (rows following a picture's direction field) stays planned.

Done 2026-11-07 (`batch:fills-2`, #533 meander part): meander fill, region type `meander`, with
12 own-drawn tiles of the new pattern kind `meander`, chained into rows and joined along the
outline. A tile that would cross itself or its neighbours is refused, so user tiles from the
Stitch Creator sew cleanly too. With the maze, #533 is done.

Done 2026-11-07 (`batch:fills-2`, #533 maze part): maze fill, region type `maze`. It is the
stippling walk with its spanning tree biased to run straight on (`straightness`, default 70 %) and
its own `mazeSeed`, sewn with square turns: one path that never crosses itself, its passes a
spacing apart. Stippling's output is unchanged. The tiled meander stays planned.

Done 2026-11-07 (#614): cross stitch as PE-DESIGN 11.50 offers it. Its Sewing Attributes show
only Size and Times (Single | Double | Triple) and no Direction (round 8 references), so
OpenStitch hides Direction for it and ignores `angle`. `times` covers each diagonal 2, 4 or 6
times and adds 256 stitches per step on a 20 mm square, as PE-DESIGN's files do (compat
`cross-single`, `-double`, `-triple`). PES 0110 reads and writes Times and Size. The old
`triple` switch is Times Double. Beyond PE-DESIGN: `crossKind` half (/), upright (+) and Smyrna
(X with a + over it), each line of neighbouring cells sewn out and back.

Done 2026-11-05 (#563-#565): the kit previews show their real thread colours (a 128-colour
palette had turned Ultra Marine green and Red and Magenta alike; CI now checks every preview
against its threads), every image under `docs/images` has a generator that
`tools/docs_images.py` runs at the release gate (CI checks none is missing), the wizard dialogs
no longer cover the design in screenshots, and the cotton texture's Higgsfield job is recorded.

Done 2026-11-05 (#443): Auto Punch on flat graphics (bold lettering, logos) sews clean satin
columns as PE-DESIGN does (T and H bar by bar, S and C along their curve, the image's own colours
only); photographs convert as before. Next for lettering images: OpenStitch mode's sewn cream
background and fused letters (#445); for photographs, keep as many threads as colours (#447).

batch:auto-punch (2026-11-06): OpenStitch mode leaves a paper-coloured background out as the
fabric (#445, done); Auto Punch reports a region plan with reasons, takes per-region overrides
and a dry run, and can turn touching fills apart (#489, engine part; the dialog's table,
presets and click-to-fix stay open); OpenStitch mode keeps thin parts such as ropes as
running lines (#469, part). Measured and left open: one thread per colour in PE-compatible
mode (#447: p03 gains Pewter but loses likeness) and Cross Stitch's edge cells (#469: g05
loses likeness).

batch:mac-digitize-2 (2026-11-06): Auto Punch lays fills for the least travel
(`angleRule: leastTravel`), takes purpose presets and saves its outlines as SVG (#489, engine
part; the dialog's table, click-to-fix, Preset row and colour order stay open); OpenStitch mode
fills can reach under the next colour (`underlap`, #469 part); a click cuts out the subject and
a photo's edges become a Design Center line drawing, with no model (#490; OCR and learned
models moved to #678, Future). Measured and left open again: PE-compatible mode's thread set
on p03 (#447: CIELAB splitting and popularity reductions match 1-3 of PE-DESIGN's 10 threads)
and Cross Stitch's edge cells on pixel art (#469: every rule tried lowers the likeness score).

batch:mac-wizard (2026-11-06): the image dialog previews Auto Punch's regions, lists them with
the stitch each got and why, lets any row or a click on the picture change it, warns when
more than a quarter is left out, and takes a colour order and a purpose preset (#489, done).

batch:mac-digitize-3 (2026-11-06): OpenStitch-mode Auto Punch no longer sews stitches in place
along staircase outlines (#688) or as knots in specks (#690); touching fills turn apart without
lowering the likeness, and every purpose preset passes sew_check on the gallery (#690). Left: a
fill's or satin's short ends plus the tie-off on two photos (#703, stitch engine) and the
mandala's drawn dots sewn as knots (#713). Both done (#703, #738); the photo preset's pointed
fill and satin tips on the mandala, whose tie-in plus first rows sewed 7 in place, tie in along
the tip since #732.

Done 2026-10-30: stitch generation is bit-identical on Windows, macOS and Linux (transcendental
math through the `libm` crate instead of each platform's C library); the arched Text Transform
spec, deferred at alpha.6 because Linux measured it 0.2 mm lower, has its PE-DESIGN reference
back.
Done 2026-10-31: image digitizing (`openstitch-digitize`) uses the same deterministic
math, with its own source scan and cross-platform golden hashes of OpenStitch-mode output
(B16.1, the first steps of the image-similarity plan).
Done 2026-10-31: Photo Stitch 1's OpenStitch mode samples colour every 0.25 mm along a row
with stitch ends where the thread changes, within the owner's needle caps (2.1 per mm² over the
design, 3.0 per 10 mm window), weighting detail by saliency, with a local search against the
image as the eye sees it (B16.1 T2, T3, T10, T9): the nine photos score 55.8 → 73.6, short of
the 85 % target by 5.6–15.7 points a photo.
Done 2026-10-31: Photo Stitch 2's OpenStitch mode holds the owner's needle limits (3.3 per mm²,
no window denser than PE-DESIGN-compatible mode's) with no stitch under 1 mm (B16.1 T13): six images
44.9 against PE-DESIGN-compatible mode's 43.4, three still below it.
Done 2026-10-31: imported stitch files land centred on the Design Page, as PE-DESIGN's Import
Patterns ▸ from File places them (spec `stitches-import-centre`); left: a DST's colour on
import (PE: 007 PRUSSIAN BLUE) and the joining stitch between two imports of one colour.
Also 2026-10-30: new fill regions start at 45°, as PE-DESIGN 11.50's shape tools do
(pes-objects r01); saved designs keep their stored angle.

1. **Fill:** Round 4 (2026-10-29) confirmed PE-DESIGN has no random tatami stagger
   (Frequency and two Stitch Type buttons only) and a 0.1 lines/mm density floor. PE-DESIGN's row structure (halves from the start point, brick row ends, 30 %
   Frequency default); the programmable fill density; Stitch Type 2's row lattice at 30° and 45°
   (sloped rows sewn since 2026-10-30, `stitchType: 2`: identical to PE-DESIGN at 0°, 0.048 mm
   at 90°; at 30°/45° PE's rows sit about half a spacing from ours, as Type 1's do. Round 6
   (2026-10-31) pinned the rows to the shape at the exact 1/density spacing, counted from the
   far side; left to model: the per-angle needle-grid constant and PE-DESIGN's dependence on
   the page position, which ours lacks); Half Stitch; feathered-edge patterns; gradation presets, per-step editing and Blend. Gradation
   rows follow PE-DESIGN's cumulative-density placement since 2026-10-28 (#95: every row of
   the U within 0.12 mm, overlap 0.83 → 0.89); left for the U: PE's section order for fills in
   several parts (right leg, left leg, then the top bar ending at the first vertex). Under
   sewing Density Dense / Medium / Light and the combinable under sewing parts (edge, rows,
   lattice, piping) are done since 2026-10-29 (#124: sweep-fill-us-density-dense/-light at the
   reference tier; the second type, `cross`, since 2026-10-28, specs sweep-fill-us-type-2 and
   sweep-satin-us-type-2); left: the button-to-capture mapping of a Manual Punch column's five
   under sewing types once pe-refs-4 notes it, and the zigzag's exact split points.
   Density floor: 0.1 lines/mm for fill and satin, as PE-DESIGN 11.50's field and the
   `sweep-*-density-min` references (#139's 0.2 floor was checked against them on 2026-10-29
   and not adopted; `attribute-ranges` covers the boundaries).
2. **Satin:** Miter limit for zigzag lines is done since 2026-11-07 (#373: PE-DESIGN's Sharp
   Corners buttons and Miter Limit from the owner's captures, eight specs at the reference
   tier); left: the angle where PE-DESIGN starts cutting (captured at 29.7° and 90°), button
   2's exact fan, the attribute in PES objects, and satin lines. Half Stitch for Auto satin
   is done since 2026-11-04 (#372): every other crowded inner needle point at a bend pulled
   back along its crossing, measured on satin-auto-bend-90/-120 against their -nohalf twins.
   Jagged Auto Punch regions sew within sew_check's limits since 2026-11-04 (#304: doubled-back
   Auto satin falls back to one direction, travels spread their needle points, short travel
   stitches skipped). Auto satin's needle lattice (whole spacings from the far end, the
   remainder at the start point), its forward start travel and its end on the start point
   follow PE-DESIGN since 2026-10-28 (satin-auto-bend-60: mean needle distance 0.20 → 0.09 mm);
   left: PE's exact remainder handling on the start leg (0.05 mm) and the fan's inner crowding
   (0.3-0.4 mm against our 0.25-0.3 mm), both below the file's 0.1 mm grid in effect. Done 2026-10-28 as well: the gradation pattern on a split
   satin (satin-gradation, needle for needle). Also done 2026-10-28: the minimum-density
   sawtooth (sweep-satin-density-min, needle for needle: the crossings count back from the
   start point's line, which the round-3 column at 0.2, 0.3 and 0.5 lines/mm confirmed record
   for record). Done 2026-10-29 (#122): plain satin has no minimum-crossing threshold (the
   far end's single-line gap is crossed at 0.22–5 mm, pe-refs-4's satin-length-sweep), its
   lattice stretches to whole gaps, and a column started at one end is sewn from its far end
   back to the start point; left: PE's entry/exit optimisation between objects (it moves start
   points). The Auto satin fan on sharp bends follows PE-DESIGN since 2026-10-30
   (satin-auto-bend-120 at the reference tier: 171 stitches to PE's 161, was 273).
3. **Lines:** Start/End Type Feed/Run for stem, candlewicking and chain, with the L2 marker and
   the Feed/Run choice in the editor; open lines and chain diamonds; candlewicking Low and
   High. The motif line cannot be compared: PE-DESIGN's default motif is Brother's own artwork.
4. **Lettering:** stroke-wise sewing of TrueType text as PE-DESIGN does it (every letter is
   split into strokes, each sewn as a column with its rows across the stroke, for Fill and Satin
   text alike; the 'H' references match only because an H's stems are vertical; measured and
   planned in [research/stroke-lettering.md](research/stroke-lettering.md), 2026-10-30; the
   captures are in, the stroke cut (step 1, `strokes::strokes`) and the rungs are done, and
   PE-DESIGN's Convert to Blocks files import as their blocks (`import.pes`, 2026-10-31); next, the
   stroke ownership and order, travel and trims, then sewing text by stroke) — the structural
   captures are in, the stroke cut (step 1, `strokes::strokes`), the rungs and the block order
   (`GlyphStrokes::blocks`, 2026-10-31) are done, and PE-DESIGN's Convert to Blocks files import
   as their blocks (`import.pes`, 2026-10-31), and satin text is sewn by stroke since 2026-10-31
   (`stroke-satin-*`, 9 % from PE-DESIGN's needle points on average), fill text since 2026-11-04
   (#376, `stroke-fill-*`, 8 % on average, under a Text Transform still by region: #428, waiting on
   the arches' letter placement; the walk between a warped letter's blocks simplified since
   2026-11-07),
   turned letters cut upright and satin crossings along the outer side since 2026-11-05 (#444,
   satin 5.6 % on average, no capture further than before, the README's arched and 4 mm text whole), small lettering's i dots, pieces and travel since
   2026-11-05 (#465); next, under sewing per
   stroke, N's corner and A's and V's slanted feet (#446; X sewn as PE-DESIGN's, one diagonal through, since 2026-11-06), and a satin capture of small text) — the structural
   gap behind text-fit-*, text-transform-* and text-sewing-order; the fitted circle's running
   stitch follows PE-DESIGN's Bézier halving since 2026-10-28 (text-fit-default mean needle
   distance 0.27 → 0.20 mm); Text Transform presets 13 and 14 (fan out / in) done 2026-10-28; round 4 (2026-10-29)
   showed PE-DESIGN's transform level is a dragged handle whose value is stored per file
   (percent of the font size, degrees for slants, fans and arches): the level parameter and the
   cap-height box are in since 2026-10-30 (`envelopeLevel`; 105 of 108 level captures and 51 of
   52 default-level captures at the reference tier); left: arch 2 on "Agjy 123" and at level 25,
   taper 4 and slant 9 at level 100 (0.6-1.1 mm).
   distance 0.27 → 0.20 mm; #121 kept it and lifted the 256-chord ceiling, 2026-10-29); Text
   Transform presets 13 and 14 (fan out / in) done 2026-10-28.
   Left: per-character colour and
   multi-character selection; Connection Points; minimum font sizes and small-text fonts;
   kerning that reflows the line; editing several characters at once on the canvas; shaping for
   embroidery fonts.
5. **Machine stream:** the per-format compares of DST, EXP, JEF, PCS, SEW, XXX, HUS and VIP
   against PE-DESIGN's exports pass since 2026-10-28 (the writers follow PE-DESIGN's record
   layout; FILE_FORMATS.md "PE-DESIGN's record layout"; the SEW stream from PE-DESIGN's own
   start point since the round-3 SEW exports). Since 2026-10-28 a move of up to 2.3 mm between
   same-colour objects is sewn across as PE-DESIGN does (machine-jumps, machine-trim-bound,
   machine-trim-fine). Left: how the folding formats write a move shorter than one record (one
   export each, listed in the audit); PE's optimized exit and entry 0.5 mm inside the edges
   (fill track) and the multi-needle minimum-length field's value; the same compares on a
   design with short moves. The parked references (satin gradation, Text Transform presets 13
   and 14, under sewing type 2, satin density minimum, the trim bound) have all landed.
   Round 4 (2026-10-29): PE-DESIGN's DST divides an over-long move evenly into
   121-unit records (long-jump-300, reference tier) and its oversize export carries no
   hoop check (hoop-oversize-export). Parked with their numbers: the ten-format export of
   a two-colour design (the PES reader drops PE-DESIGN's tie-in needle points, 4 per
   file; `stitches.import` of a DST merges its two colours; the CSD reader misreads a real
   file; SHV one colour change over), PE-DESIGN's PES of the 350 mm move (PEC encoding
   beyond 204.7 mm unknown) and of the oversize design (211 x 229 mm read for 129 x 105).
6. **PES:** PE-DESIGN 11's own `#PES0110` layout is written (`pesVersion` 11, issue #120):
   shapes as `CEmbLine`, Lato Bold text as `CEmbNText`, Text Transform text as `CEmbDText`,
   fitted text as `CEmbAText`, gallery shapes as `CEmbSpecialShape`, stitch patterns as `CEmbOne`
   stitch groups, Manual Punch Straight and Curve Blocks and the Applique Wizard's objects
   (2026-10-29/30). PE-DESIGN opens the shapes, the text, the Text Transform and the fitted text
   editable (references `pes-0110-editable-shapes`, `-text`, `-text-transform`, `-text-fit`). The
   selected hoop is written to the v6 header in mm and an over-size export warns instead of
   failing, as PE-DESIGN does (2026-10-29, #125), with its "Design Page Is Custom" and "Hoop
   Rotation" flags and a custom page's size since 2026-10-30 (opened in PE-DESIGN 11.50 as the
   hoop and page they describe). Round 4 added owner-drawn Programmable Stitch
   Creator sources (`.pas`, `.pmf`, `.plf`), imported since 2026-10-29 (#131); Font Creator fonts
   wait for the owner (#134). Left: PE-DESIGN opening a written appliqué
   (`pes-0110-applique`: refused on 2026-10-31, "Unexpected file format"; the colour addendum is
   being corrected) — the stitch group, Straight and Curve Block exports open in PE-DESIGN since
   2026-10-31 (references); the fitted text's box as PE-DESIGN computes it (PE-DESIGN recomputes it
   on opening); PE-DESIGN's confirmation of the Hoop Size page (probe p3); the design's machine
   type and hoop in the header; turn the `#PES0110` texts and stitch groups the reader parses
   into editable OpenStitch objects (#397; shapes import since 2026-10-30, and an opened file
   re-saves byte for byte while unchanged since 2026-11-04, #375); right-aligned and vertical text; several programmable fill patterns in one design (one is written since 2026-10-31), motif fill and the attribute fields of the other region types (written by their codes at PE-DESIGN's defaults since 2026-10-31), motif lines and the Start/End options and sewing end of stem, candlewick and chain lines (written by their codes since 2026-10-31),
   several hole sewing pairs in one file or with groups (one pair without groups is written since
   2026-10-31) and several outlines per object; the hoop in v1 (the public
   description contradicts itself); the hoop name and id in PES v9/v10 (PES v4, v5 and
   v7–v10 are written since 2026-11-07 from PE-DESIGN's own Save As files, #374, as
   [LEGAL_AND_LICENSING.md](LEGAL_AND_LICENSING.md) allows).
7. **Editing details:** skeletons that branch (a derived skeleton follows a shape's longest
   stroke only) and an exact medial axis instead of the grid approximation.
8. **Formats:** the SHV preview's pixels as PE-DESIGN draws them, and SHV records as PE-DESIGN
   writes block starts, stitches over 12.7 mm and long moves (#349: measured on PE's exports of
   2026-11-07; CSD matches them since 2026-11-07: 14 blocks a file, long stitches, palette).
   #320 wrote both formats from one PE-DESIGN export each, 2026-11-04. The CSD and SHV readers match PE-DESIGN's own
   exports since 2026-10-29 (#135), PEC colour indexes are the plain nearest entry per
   block as in PE-DESIGN's own files (2026-10-30), and a DST import keeps its colour blocks apart. The eight
   machine-format writers (DST, EXP, PCS, HUS, VIP, JEF, SEW, XXX) match PE-DESIGN's exports of
   its own design since 2026-10-30 (`machine-sample-basic`, zero-length stitches aside and
   counted per side in the report; PCS and HUS write a jump landing twice as PE does, so only
   the two repeats PE's PES cannot carry are left). PE's PEC keeps zero-length stitches (28 of
   249 references), so the planned "drop them in the PEC writer" was not done. PEC moves over
   204.7 mm split as PE-DESIGN's DST does since 2026-10-29 (#126); PE's own PEC does not encode
   such a move (its `long-jump-300` PES has a stand-in row instead), settled 2026-10-30. PE's
   oversize PES has an empty machine section, which the reader reports instead of misreading
   (2026-10-30). Thread charts export as CSV in the layout
   PE-DESIGN's Import Chart reads since 2026-10-31 (#136; bare `code,brand,description,R,G,B`
   records; PE has no CSV export and keeps its charts as binary `.btc2`). Colour matching is equal-weight
   squared Euclidean with an "approximate" city-block matcher since 2026-10-29 (#119), checked
   against the owner's twelve-swatch capture; the 61 BROTHER EMBROIDERY catalogue codes are known
   since the round-4 chart reading, PEC 42 (307 LINEN) has PE-DESIGN's RGB and every coded thread
   in PE's files matches the chart (2026-10-30), and SVG imports snap to them as PE-DESIGN's do
   since 2026-10-30 (`thread-match-swatches` compares the thread table). The 94-entry sew palette is
   listed since 2026-10-30 (PE's Ver4 Option Color chart as ids 66–94, `sewPalette`; the 0110 writer writes those
   threads with their ids as PE does); where
   PE-DESIGN matches colours to it is not verified (no capture). PEC 62–64 have PE-DESIGN's appliqué
   RGB, the `#PES0110` colour list after the PEC is written as PE writes it, and the Appliqué
   wizard's stages sew in PE's appliqué colours as PEC 62–64, since 2026-10-31. The `#PES0110`
   header carries the machine type and hoop as PE writes them for six hoops since 2026-10-31
   (#125), and every page and hoop field from the design since 2026-11-04 (#322: a Custom Size
   page's machine type and hoop sections, the nv880e hoop fields, the ids of 13 hoops; the PEC
   frame stays the design's size on a Custom Size page too). Left: the user-chart CSV PE-DESIGN
   imports (#136); the v6 display fields and the v1 hoop field (no PE-DESIGN file to compare).
9. **Machine tier:** a first sew-out of an OpenStitch-written file on a Brother machine, recorded
   in `compat/machine/`. No spec has reached this tier.
10. **Distribution:** code signing (owner's certificates); Flathub submission (offline cargo
    sources, owner's account). Blocked on `unsafe` FFI, which the workspace forbids: macOS
    Apple-Event file opening (winit 0.30/eframe 0.37 deliver no file-open event; a delegate
    needs objc2 `define_class!` with `unsafe` blocks, see DISTRIBUTION.md "macOS double-click")
    and PES thumbnails on macOS and Windows.
11. **Image digitizing (Phase 6, #127 #128 #132, #129 optional):** Photo Stitch mono tone
    pipeline done 2026-10-29 (#128: integer 30/59/11 luminance, full-range min/max stretch
    with the (1, 255) guard); Auto Punch's 3×3 modal colour census capped at 100 colours, the
    `max(4, round(param · 100 / mmPerPx²))` small-region floor at 4-connectivity and the
    centerline-thinness satin-vs-fill test done 2026-10-29 (#127); Cross Stitch shares that
    prepass with documented grid and per-cell defaults since 2026-10-29 (#132); Auto Punch's
    satin bounds hold at every working resolution since 2026-10-30 (exact stroke widths for
    even pixel counts), its fill direction and region order are PE's on every capture since
    2026-10-30 (largest row spread; scan order alternating by colour; specks kept and dropped
    as PE; PE's thread picks after reduction checked on 20 flat colours: a per-colour mapping
    no tested distance reproduces, kept as a documented difference). Compared with
    as PE), and its census keeps large colours behind anti-aliased edge shades since
    2026-10-30 (taken most frequent first, colours within RGB 32 merged, at most 10 automatic
    colours: PE's count on 4 of 9 flat round-5 test images and all 4 photos). Compared with
    PE-DESIGN 11.50's round-4 captures on 2026-10-29: Cross Stitch's six diagonal stitches per
    cell (−0.9 % stitches), Photo Stitch 1's Sew Page Color, 3.0 mm Run Pitch and 10 colours
    (+5 %); second round (bars, moved shapes, mono, Photo Stitch 2): Auto Punch's satin/fill
    bounds 0.7–6.5 mm, Auto-direction satin without under sewing, fill across strokes up to
    13 mm and at 45° beyond, automatic colour count, PE's colour order (R + G + B, highest
    first, also in Cross Stitch), Sew Page Color in Color mode only (Auto Punch within 3 % of
    PE's stitches on all four captures). Photo Stitch 1's runs are joined nearest first since
    2026-10-30 (mono +11 % stitches, was +39 %; trims 99 and 249, were 426 and 1158). Next:
    run-to-run travel in Cross Stitch; PE's contour-following meanders for Photo Stitch 1 and
    Photo Stitch 2's exact colour transitions within a Color Set and Auto Select's thread
    choice (rounds 5 and 6: four Ver4 Option threads, the image's nearest colours plus fillers
    from a short list; the best simple rule matches 68 of 124 picks) (its line
    screen, #129, is built since 2026-10-30 after the owner's decision: −3.1 % stitches, angles,
    spacing and densities as PE's; named Color Sets, Direction, interval/density and Mono's
    tone ramp as PE's; harness `tools/screencmp.py`, design note
    docs/research/photo-stitch-2-halftone.md). Test images since 2026-10-30: 30 generated
    images in `examples/images` covering photos, illustrations, line art, pixel art, lettering,
    transparency, noise and sizes, with PE captures requested and a README gallery
    (`tools/gallery.py`, part of the release checklist in docs/RELEASING.md). They found that
    Auto Punch's 100-colour census fills with edge shades on anti-aliased images, so orange
    and yellow become grey, where PE-DESIGN keeps them (round-5 capture); the fix is next.

## Beyond PE-DESIGN

PE-DESIGN compatibility stays the primary goal and the phases above come first. After that,
OpenStitch adds what other digitizing products offer and PE-DESIGN 11 does not. The survey
of Wilcom EmbroideryStudio and Hatch, Bernina V9, mySewnet, Janome Artistic Digitizer, Tajima
DG by Pulse, Embird, Ink/Stitch (concepts only), Embrilliance and Floriani is in
[research/competitive-features.md](research/competitive-features.md): a comparison matrix,
per-feature notes with the stitch-generation idea, effort and licensing, and a ranked
backlog. Every item lands as a new attribute or command that is off unless chosen, so the
PE-DESIGN reference specs stay green. The rows are listed as "Planned (beyond PE-DESIGN)" in
[FEATURE_MATRIX.md](FEATURE_MATRIX.md).

Done beyond PE-DESIGN so far: the seeded random tatami stagger (#123, 2026-10-29: `stagger:
random` with `staggerSeed`, reproducible; PE-DESIGN's Fill pane offers the regular Frequency
brick only, which stays the default). Fill slivers without stitches in place (#303, 2026-11-04): a fill
run with more than 6 stitches under 0.5 mm in a row is sewn as a zigzag of stitches at least
0.5 mm long. PE-DESIGN sews such slivers as they are (its own Auto Punch of the storybook picture
has 11 stitches under 0.3 mm in a row), while its 74 drawn-fill references never exceed 6, so
they are unchanged.

In progress for alpha.9: **image similarity (B16.1)**, a measured plan to make the image
wizards' results look like their images, in [research/beyond/image-similarity-plan.md]
(research/beyond/image-similarity-plan.md). One agreed score (structure, colour and a
perceptual term on a thread render seen at actual size) judges every change; the targets
are 95 % for logos, clip art and lettering and 85 % for Photo Stitch 1 at High fidelity, at
PE-DESIGN's sewing density. All of it lives in the wizards' OpenStitch mode: the
PE-DESIGN-compatible mode and its specs stay as they are, and every result is the same on
macOS, Windows and Linux. Phases 0–5 are rows in [FEATURE_MATRIX.md](FEATURE_MATRIX.md).

| Phase | Backlog items | Scope |
|---|---|---|
| A Fill library | B1 contour/spiral fill · B6 density profiles and two-colour blend · B7 3D puff mode · B8 crosshatch and lace · B12 carving lines and knockdown fill | `openstitch-design` generators and inspector rows |
| B Satin quality | B3 corner handling (mitre/cap/lap) and short stitches · B5 pull/push compensation, automatic underlay by width, fabric table · B10 variable-width satin line | model change with `.osd` migration; needs a sew-out |
| C Routing | B4 auto-routed running stitches and Redwork, then satin branching · B11 closest join, auto start/end, continue digitizing, overlap removal | `route.rs`, `order.rs`, Pen tool |
| D Turning fills | B2 two-guide fills and stitch-angle maps · B9 maze and tiled meander fills, randomisation | builds on A and C |
| E Tools and lettering | B13 density map, troubleshoot, thread-brand conversion, auto hoop · B14 stitch-file resize with density · B15 baselines, saved kerning, team names, closest-point letter connections | engine commands and editor panes |

Status of each item (sketch = the algorithm, parameters, model change and test specs, written
before any code; the ranking and the proposed tracks are in
[research/beyond/readiness.md](research/beyond/readiness.md)):

| Item | Feature | Phase | Sketch | Status |
|---|---|---|---|---|
| B1 | Contour (offset) fill | A | [research/beyond/contour-fill.md](research/beyond/contour-fill.md) | Done 2026-10-30: region type `contour` with rings, single and double spiral, echo lines (0–30) and round or mitred corners, on the ring tree of PE-DESIGN's Spiral Stitch (`crate::spiral`); open: repeats, bevel join, compat specs |
| B2 | Two-guide fills and stitch-angle maps | D | [research/beyond/guided-fill-and-angle-maps.md](research/beyond/guided-fill-and-angle-maps.md) | Sketched; not started |
| B3 | Satin corners and short stitches | B | [research/beyond/satin-corners-and-short-stitches.md](research/beyond/satin-corners-and-short-stitches.md) | Done 2026-11-06 for Manual Punch columns and zigzag lines (#520, #521): `cornerStyle` turn/auto/mitre/cap with `mitreBelow` 75°, `capBelow` 45°, `cornerOverlap` 0.5 mm, `capMaxLength` 8 mm; `shortStitchDistance`/`shortStitchInset`; off by default. Satin looks (#528) on columns and zigzag lines 2026-11-06: S-stitch, open, double, jagged edge, `spacingAt`. Skeleton satin and Auto satin regions 2026-11-07 (#675); satin lettering 2026-11-07 (#676, #686: short stitches, compensation, looks and raised; letters are cut at their sharp corners, so corner styles do not apply). Open: lap corners |
| B4 | Auto-routed running stitches, Redwork, satin branching | C | [research/beyond/auto-route-and-branching.md](research/beyond/auto-route-and-branching.md) | Branch split started 2026-11-06 (#526): `object.deriveSkeleton {branches}` makes a column per arm (Y, plus, star); letters with bent junctions (K, R) keep a straight stem since 2026-11-07 (#526; R's bowl still leaves a strip by the stem); auto-route, Redwork and branching of touching shapes shipped 2026-11-06 (#546, `batch:routing`: `shape.autoRoute`, `object.branch`); satin branching with a centre-run underlay not started |
| B5 | Compensation model, automatic underlay by width, fabric table | B | [research/beyond/compensation-and-underlay.md](research/beyond/compensation-and-underlay.md) | Done 2026-11-06 for Manual Punch columns and zigzag lines (#522, #523): `pullLeft`/`pullRight`/`pullPercent`/`pullMax`; `underlayStyle` auto (by average width, fabric thresholds from `fabric.apply`) and centreWalk; a column's `underlay2`. Skeleton satin and Auto satin regions 2026-11-07 (#675), with rows that follow the column's width. Open: fill push and region second underlay (#677) |
| B6 | Density profiles, two-colour blend, hidden travel | A | [research/beyond/density-profiles-and-blend.md](research/beyond/density-profiles-and-blend.md) | Hidden travel between objects shipped 2026-11-06 (#545, `batch:routing`: `design.setTravelUnder`, run connectors); round several corners and along running lines already sewn 2026-11-07 (#545); density profiles and blend not started |
| B7 | 3D puff mode | A | [research/beyond/puff-mode.md](research/beyond/puff-mode.md) | First step done 2026-11-06 (#531): raised satin (padding lanes) on Manual Punch columns and zigzag lines; foam mode not started |
| B8 | Crosshatch, curved crosshatch, lace | A | [research/beyond/crosshatch-and-lace.md](research/beyond/crosshatch-and-lace.md) | Sketched; not started |
| B9 | Maze and meander fills, fur, randomisation | D | [research/beyond/meander-maze-and-randomisation.md](research/beyond/meander-maze-and-randomisation.md) | Sketched; not started |
| B10 | Variable-width satin line | B | [research/beyond/variable-width-satin-line.md](research/beyond/variable-width-satin-line.md) | Engine done 2026-11-06 (#524): line type `satin` with width keys, tapers, per-side widths and slant; canvas gestures #700, SVG stroke width #701 |
| B11 | Closest join, auto start/end, continue digitizing, overlap removal | C | [research/beyond/closest-join-and-overlap.md](research/beyond/closest-join-and-overlap.md) | Sketched. Continue digitizing shipped (#479: `object.extendPath`, `object.extendColumn`, the drawing tools continue a selected line from either end); nearest start per object (`autoStart: nearest`) and the Optimize before/after report shipped 2026-11-06 (#547, `batch:production`); part-way satin starts, draggable markers and overlap removal not started. Related, shipped 2026-10-29 (#137): `order.optimize` mode `minJump`, a 2-opt shortest-jump order per colour block with every neighbouring pair linked, as a design setting (`optimizeOrder`); PE-DESIGN's order stays the default |
| B12 | Carving lines, knockdown fill | A | [research/beyond/carving-and-knockdown.md](research/beyond/carving-and-knockdown.md) | Knockdown shipped 2026-11-06 (#510, `batch:production`): `wizard.knockdown` (selection or whole design, reach, gap closing, one or two crossed layers, holes kept, rectangle option, `update`), kept first by `order.optimize`. Carving lines not started |
| B13 | Density map, troubleshoot, thread conversion, auto hoop | E | [research/beyond/quality-tools.md](research/beyond/quality-tools.md) | Density map and troubleshoot rules done (Check Design, #503/#504); thread conversion and auto hoop not started |
| B14 | Stitch-file resize with density | E | [research/beyond/stitch-file-resize.md](research/beyond/stitch-file-resize.md) | Sketched; not started |
| B15 | Lettering: baselines, kerning, team names, connections | E | [research/beyond/lettering-depth.md](research/beyond/lettering-depth.md) | Sketched; not started |
| B16 | Image wizards' "OpenStitch" mode beside the PE-DESIGN default: perceptual colour reduction (CIELAB k-means, CIEDE2000 thread matching) and region cleanup for Auto Punch first | — | [research/beyond/image-digitizing.md](research/beyond/image-digitizing.md) | Auto Punch's OpenStitch mode done 2026-10-31 (`compatMode: openStitch`, image dialog Mode): measured on the test images (`tools/image_eval.py`), mean ΔE00 9.1 → 8.6, stitches −14 %, trims −48 %, specks 220 → 10; line art keeps its lines since 2026-10-31 (mandala 2,195 stitches, PE mode 2,590); Photo Stitch 1 in colour since 2026-10-31 (perceptual palette, CIELAB dithering: mean ΔE00 10.0 → 8.2 on nine photos, trims +13 %); Photo Stitch 2 in colour since 2026-10-31 (perceptual Auto Select, linear-light mix: mean ΔE00 19.8 → 16.2 on seven images); Cross Stitch measured, no gain at B16 (B16.1 added its OpenStitch mode, #263); next: Photo Stitch 1's trims, the Photo Stitch 2 portrait, SLIC |
| B16.1 | Image similarity towards 95 %: an agreed score (0.4 MS-SSIM structure, 0.4 CIEDE2000 colour, 0.2 DISTS, on a thread render seen at actual size from 40 cm), then a shared colour engine, Photo Stitch 2, Photo Stitch 1 (High fidelity), Auto Punch, Cross Stitch and Design Center, all in OpenStitch mode | — | [research/beyond/image-similarity-plan.md](research/beyond/image-similarity-plan.md) | 2026-10-31 (#270): Phase 0 landed except the calibration card (deterministic maths, golden hashes, the scorer `tools/image_similarity.py`); Phase 1's thread selection, tonal dithering and Fidelity landed; Phase 5 landed (Cross Stitch mean 51.3 → 70.2, Design Center g03 96.4, g04 95.2). Landed for alpha.9 (2026-11-01, batches #270, #278, #282): Auto Punch 83.6, Photo Stitch 1 72.3, Cross Stitch 70.9, Design Center 95.8, Photo Stitch 2 43.8, each against PE-DESIGN-compatible mode's 78.9, 58.5, 51.3, 82.7, 42.8. Next: Photo Stitch 2 phase 2 (#129, waits for the calibration card's sew-out: the card is designed in `examples/sew/calibration`), T8 (waits for the counsel record). 2026-11-04: #127's graph-cut step landed as Auto Punch's opt-in `isolateSubject` (OpenStitch mode), with hand-marked subject and background strokes (#350); a cut that finds no subject is an error (#355) |
| B16.1 C | Image similarity, Auto Punch (Phase 4): sew a coloured background (T7), SLIC regions at 800 px (T11), skeleton satin outlines (T12), per-region fill angle (T8) | — | [research/beyond/image-similarity-plan.md](research/beyond/image-similarity-plan.md) | T7 done 2026-10-31 (OpenStitch mode sews any background but the fabric's colour, in one covering layer without under sewing; balloon 78.8 → 87.6); T11 done 2026-10-31 (800 px, 2 mm² floor: mean 79.0 → 81.7, specks 0 but the mandala's dots and dashes; SLIC measured lower and not used); T12 done 2026-10-31 (dark outlines as skeleton satin sewn on top, satin only for strokes up to 2.5 mm: mean 82.1 → 83.6, every case above PE-DESIGN-compatible mode); next: T8's fill angle once patents.md records counsel's opinion, re-measure with T4/T5 |
| B17 | AI assistant: a chat panel that generates, edits and explains embroidery through the engine commands, with Claude (default `claude-opus-5-5`), other endpoints, local models or ACP coding agents, on the user's own account and key | — | [research/beyond/ai-integration.md](research/beyond/ai-integration.md) | Plan approved 2026-11-04 (owner: everything the user can do must be possible from the chat). A0 landed 2026-11-04: `ui.*` editor commands on the socket, `openstitch-mcp --assistant`, the embroidery skill, and coverage tests that fail when a feature has no assistant route; A1 landed 2026-11-04 (#306): the Assistant panel with Claude, keychain key, permission gate, one undo per message, Save as script; A2 landed 2026-11-04 (#317): OpenAI-compatible endpoints, Ollama/LM Studio/OpenRouter presets, model lists, local models without a key; A3 landed 2026-11-04 (#338): coding agents (Claude Code, Codex, OpenCode) over ACP on the user's own account, reaching the editor only through the gated private socket; A4 (#364): attachments, attach view and selection, pictures digitized in OpenStitch mode, the vector route, an optional image generator; A5 (#365): `stitchtool bench-assistant` (12 tasks scored on the design); the first real run (2026-11-04): Claude Code over ACP passed 12 of 12 tasks; round 2 (2026-11-04, #390, #392, #393): Claude Code without the user's own setup (−37 % input tokens), Ollama gpt-oss:20b 8 of 12 and the badge with the network off, the composer clicked through in the running editor, fixes #407, #415, #416, #418, #419, #423, #424, #427; Windows and Linux (2026-11-05, #401): the keychain round trip in CI on all three systems, Claude Code starting on Windows, #420, and the Ollama causes of #425 and #429 found and worked around, and Claude Code passed the bench 12 of 12 on Windows (#582); settings and memory (2026-11-06, `batch:assistant-settings`): a bare scratch file name no longer asks (#431), the Ollama and LM Studio presets pick a model with tools instead of keeping the Claude model (#432, tested against a mock server), and the Assistant remembers the user between conversations behind the permission gate (#491); assistant fixes (2026-11-06, `batch:assistant-fixes`): a stream that sends no reply for the request timeout ends the turn, keep-alives or not, and Stop ends a silent turn at once (#580, proved with fake providers), reasoning from OpenAI-compatible models shows as Thinking, batches report every parameter error before running (#494), the Assistant keeps to the user's own threads and the gate asks before a thread chart changes (#495), print PDFs older than a day are removed (#384); Claude Code passed the full bench 15 of 15 on Windows, memory pair included, and a real editor turn on Windows checked the permission prompt, queue, Stop and undo (#582, #401); the skill and bench (2026-11-06, `batch:assistant-skill`): ten example designs and a composition guide (#492), illustrated, Auto Punch and small-lettering bench tasks with an optional render judge (#493); next: the Claude API and an image generator (no key or generator here) |

B17, the AI assistant, is the one beyond-PE-DESIGN item that is on by default (the owner's
choice, 2026-11-04, as in SolidCraft): it adds no stitch behaviour, changes designs only through
the existing engine commands, and sends nothing until the user opens it. Its plan is
[research/beyond/ai-integration.md](research/beyond/ai-integration.md).

From the open-source scan of 2026-11-05 ([research/competitive-features.md](research/competitive-features.md),
Updates; concepts only, no code from the GPL or unlicensed projects), ranked by user benefit
and effort: 1. export read-back check (#457, done in `batch:quality-checks`); 2. test swatch of the
selection (#452, **done** 2026-11-06 in `batch:mac-sewout`: `design.testSwatch`); 3. fabric calibration swatch (#453, **done** in `batch:mac-sewout`: `fabric.calibrationSwatch` and `fabric.calibrate`; B5's
fabric table still to come); 4. arched text with several lines as concentric rings (#456, **done** 2026-11-06 in `batch:lettering-text`: `arc` on `text.add`/`object.setText`, the Text Attributes pane's Arc row and a centre handle on the canvas);
5. SVG import reading stitch settings from layer names (#455 **done**: name tokens with short or long key names, dashes and paint-order as `import.svg {settings: true}`; Ink/Stitch attributes moved to #661); 6. garment mockups (#454,
**done** 2026-11-07, `platform`: `design.mockup` on a T-shirt, cap or tote bag at true size, File ▸
Export ▸ Garment Mockup). Not taken: staggered satin splits, sharp satin corners as fans and converting a filled
shape to a centre-line satin (OpenStitch has them: `splitLength` staggered, Auto satin's corner
fan, Shapes ▸ Satin Skeleton from Shape), a prompt-to-spec LLM design agent (B17 already drives
the engine's commands), and stitch_generator's effect catalogue (a checklist for B9, not a
feature of its own).

The full survey of the same six projects (2026-11-05, second pass: 544 features read from their
docs, release notes and wikis, each checked against OpenStitch's docs and code; per-repo notes in
[research/competitive-features.md](research/competitive-features.md), Updates) opened 88 issues
(#472–#559) and added details to 16 existing ones. Better results where
OpenStitch already has the feature count too ("improve"). Ranked by user benefit and effort, none
started:

| Rank | Theme | Issues | Ties to |
|---|---|---|---|
| 1 | Sewing quality checks before the needle: Check Design pane, density map, Review Sewing Order, export read-back, random-design invariant test (**done**, `batch:quality-checks`; engine `quality.rs`); the Assistant opens and drives the window through `ui.run` / `ui.set` and reads the indicator in `ui.state` (#599, 2026-11-07) | #503, #504, #505, #457, #506, #599 | B13, D15 |
| 2 | Satin quality: corner styles, short stitches on every satin, compensation per side and in %, automatic underlay by width, branching shapes to satin, variable-width satin line | #520, #521, #522, #523, #526, #524 (and the bug #460) | B3, B5, B10 |
| 3 | Fewer trims: travel under later fills, branching and auto-routing, nearest start per object, leave out hidden stitching. Done in `batch:routing` (2026-11-06): travel under later fills and run connectors (#545), leaving out hidden stitching (#511); nearest start in `batch:production` (#547, partly) | #545, #546, #547, #511 | B4, B6, B11 |
| 4 | Assistant: memory between conversations (**done** 2026-11-06, `batch:assistant-settings`: notes beside ui.json, read in every conversation, written only behind the permission gate), every batch error at once and only the user's threads (both **done** 2026-11-06, `batch:assistant-fixes`), example designs and a composition guide, illustrated benchmark tasks (both **done** 2026-11-06, `batch:assistant-skill`: ten tested example designs and `reference/composition.md` served by `skill_reference`; eight new bench tasks scored on structure, `design.check` and, with `--judge`, a vision model's score of the render) | #491, #492, #493, #494, #495 | B17 |
| 5 | Production: parameter test swatches, fabric and thread-weight presets, basting box, knockdown, Stop at Jumps, job sheets. Done in `batch:production` (2026-11-06): basting box (#509), Stop after and Stop at Jumps (#512), with nearest start and the Optimize report (#547) from item 3. `batch:mac-sewout` (2026-11-06): parameter test swatches (#507), with the selection swatch (#452) and the fabric calibration card (#453), and job sheets for the client and the operator (#513: `design.setJob`, `design.print` modes `client` and `operator`). `batch:mac-fabric` (2026-11-07), #508 part 1: five more fabrics and the makers' backing, topping, needle and smallest-text advice per fabric and thread weight (`fabric.advise`); `fabric-2` (2026-11-07) part 2: thread weight scales density (`fabric.apply {weight}`) and a default fabric for new shapes and texts (`fabric.setDefault`); part 3: the user's machine (`machine.setProfile`), its hoop for new designs, export warnings for long stitches and missing thread cutters, Help ▸ Machine Setup on the first start | #507, #508, #509, #510, #512, #513 (with #452, #453) | B5, B12, D6, W7 |
| 6 | Fills: colour blends and gradients, Fill Angle tool and angle lines, maze and tiled meander, tatami textures, wave and flow fills | #532, #534, #533, #537, #538 | B2, B6, B9 |
| 7 | Editing: align and distribute, precision bar, copies along a line and scatter, snapping, select same, knife, smoothing, continue drawing, shape tools, stitch table, sewing-order list. Done in `batch:editor-tools` (2026-11-05): align and distribute (#472), snapping (#473), precision bar, skew and nudge step (#474), select same and click inside (#475); copies along a line, scatter and kaleidoscope (#476) in a second PR. `batch:editor-shapes`: continuing a line, skeleton or column from either end (#479); Smooth Shape and Smooth Stitches (#478); the Knife (#477; every cut keeps the shape's sewing angle and attributes since `batch:mac-shapes`, #652); frames, per-corner radii, spiral lines, Divide and shape effects (#497, part 1); turning gaps and the shape builder (#497, part 2). `batch:editor-core` (2026-11-06): copy and paste through the system clipboard between windows (#500); Sewing Order list rename, lock, sew in click order and sew outline first (#502; linked clones moved to #644) | #472, #474, #476, #473, #475, #477, #478, #479, #497, #501 (done 2026-11-07, `batch:editor-2`: Stitch Table and hand edits kept on live objects), #502, #500 | B11 |
| 8 | Lettering: font library with size ranges, single-line text, Ink/Stitch-format font folders, envelopes with handles. `batch:lettering-library` (2026-11-06): size ranges, the font list with the user's text and the size warning (#527), OpenStitch Line for Small Text (#530; the PE-DESIGN capture is pending), OpenStitch Pixel (#536); second PR: free envelopes with handles (#535), Ink/Stitch font folders (#529) | #527, #530, #529, #535 | B15 |
| 9 | Formats and threads: VP3 (read and write landed for v0.1.0-alpha.13, writing experimental until a sew-out, #480), TAP/TBF/U01 read and write and DSB/DSZ/KSM/ZXY/STX read (#481), thread list files (#482), stitch lists, G-code, export repeats, pictures, stitch-out videos and ZIP bundles (#488), perceptual thread matching and merge similar colours (#484, #485, `threads.remap` metrics and `threads.mergeSimilar`), thread search across charts (`threads.find`, part of #486) and in the editor's colour picker (#682), New Brothread as the built-in default threads with Madeira cross-references (#796, alpha.13), richer SVG import (#487: clip paths, masks, opacity, gradient bands and a report of what was left out), more formats, thread list files, `.gpl` palettes (#483, `threads.importPalette` / `threads.exportPalette`), more charts, extra exports | #480, #484, #487, #481, #482, #483, #485, #486, #488 | W3, W6 |
| 10 | Views and hoops: stitch-out player, millimetres or inches, free multi-hooping, view toggles, zoom box and real size, hoop lists for other brands. `batch:views-hoops` (2026-11-05): millimetres or inches (#519); hoop lists for other brands and round hoops (#514); stitch-out player (#516); zoom box, real size, reference image layer (#518); free multi-hooping (#515); view toggles (#517) | #516, #519, #515, #517, #518, #514 | W1, W4 |
| 11 | Image wizard: purpose presets, region table with reasons and overrides. batch:auto-punch (2026-11-06): the engine's plan, reasons, overrides, dry run and neighbour angle rule (#489 part); thin features as lines (#469 part); paper background as fabric (#445). batch:mac-digitize-2 (2026-11-06): least-travel angles, presets, outline SVG (#489 part); underlap (#469 part); no-model picture helpers (#490, **done**). batch:mac-wizard (2026-11-06): the dialog's region table, click-to-fix, colour order and Preset row (#489, **done**) | #489 (and the bug #469) | B16 |
| 12 | Workflow and platform: autosave and recovery, command palette, custom UI, tabs, new-object defaults, getting started, translations, Finder/Explorer thumbnails, accessibility, `stitchtool` one-line verbs. `batch:ci-tooling` (2026-11-06): `stitchtool convert`, `info`, `optimize`, `digitize` and `formats` with the `formats.list` command (#496). `batch:editor-core` (2026-11-06): autosave and recovery (#549); command palette, hints line and property search (#550). `batch:editor-ux` (2026-11-06): editable keyboard shortcuts, panes on either side, saved layouts and per-area preference reset (#551); part 2: new-object defaults and Basic/Advanced sewing attributes (#553; picture pickers split to #656), start cards, Open Sample and Getting Started (#554; PE Startup Wizard split to #657). `batch:editor-2` (2026-11-06): several designs as tabs with their own undo, selection, zoom and simulator; Options ▸ Open designs in tabs (off by default), Ctrl+Tab, per-tab unsaved prompts, `ui.documents` (#552); picture pickers for sewing types, fill patterns and motifs with a live hover preview and `ui.preview` (#656, the rest of #553); keyboard-only drawing with `ui.cursor` (#499 part 1) and widths while drawing with `[` `]` and the wheel (#707); accessibility: screen-reader names audited in every dialog, WCAG AA theme contrast, focus outline, Options ▸ Text size (#499 part 2, docs/ACCESSIBILITY.md). `platform` (2026-11-07): translations part 1, Options ▸ Language with Fluent-style catalogues, English fallback, a generated `assets/i18n/en.ftl` and a pseudo-locale, the ribbon translated (#555; menus, dialogs, messages, print and a first real language follow) | #549, #550, #551, #552, #553, #554, #555, #557, #499, #496 | W9 |
| 13 | Satin, line and fill looks: satin variants, raised satin, rail and rung editing, hand-stitch and scribble, back stitch and variable running stitch, decorative hand-embroidery lines, motif lines, per-object trim and tie | #528, #531, #525, #540, #542, #543, #544, #548 | B7, B9 |
| Ideas | Tartan fill, ripple line, pixel text (**done**, #536, `batch:lettering-library`), on-device digitizing helpers (**done** without models, #490, `batch:mac-digitize-2`; models #678), browser build (first slice **done** 2026-11-06, #556, `batch:mac-wasm`: open, view, simulate, whole-design edits, save and export in the browser; checked in Chromium, Firefox and WebKit (#694, `batch:mac-wasm-2`); the full editor window builds for wasm32 (#693 phase B part 1, 2026-11-07, `platform`) and the page runs it (part 2, 2026-11-07: picker, downloads, one-thread jobs); hosting #695; the full editor #693, its phase A done: the editor library builds for wasm32), background open (**done**, #558, `batch:editor-2`: measured 0.84-1.12 s for a 500k-stitch DST, so built), accent colour (**done**, #559, `batch:editor-ux`), Python package (**done** 2026-11-06, `batch:assistant-skill`: pure-Python `openstitch` over `stitchtool serve`; PyPI waits for the owner) | #539, #541, #536, #490, #556, #558, #559, #498 | — |

Skipped as already in OpenStitch (the survey's "have" rows, 261 of 544): tatami, gradient,
contour, spiral, radial, motif, cross-stitch, guided and stippling fills; running, triple,
E-stitch, zigzag, stem, motif and candlewick lines; satin columns from rails and from a centre
line, Half Stitch, staggered splits, underlay passes, pull compensation, fill travel inside the
shape, fill start and end points, ties and trims; appliqué with stops; lettering on a path and
arcs, multi-line text, monograms, per-letter moves, batch lettering (Name Drop); node editing,
booleans, weld, groups, outlines and offsets, convert outline and fill, rebuild shapes from
stitches; copy/paste of stitch settings; three views and the simulator; rulers, guides, measure;
thread charts with matching, owned threads, sort by colour; hoop presets, custom hoops, split
for multi-hooping; worksheets and catalogue PDFs; the design library; PES, PEC, DST, EXP, JEF,
XXX, HUS and imports of machine files; SVG import with `<use>`, `<style>` and CSS; signed
updates; desktop installers; the automation socket, `stitchtool` and the MCP server; the
Assistant's prompt-to-design. Where those projects do one of these better, the improvement is
an issue or a comment above (#460, #462, #463, #464, #446, #447, #471, #291).

From the competitor scan of 2026-11-07 ([research/competitive-features.md](research/competitive-features.md),
Updates; openhoop and filum, concepts only): Join Lines and Break Apart (#744, editing, rank 7
theme) and find and filter in the Sewing Order list (#745, workflow, rank 12 theme), both ideas
for Future; radial, contour and guide-following colour blends added to #532, herringbone and
ladder lines noted on #543.

Deferred: linear colour-gradient and tartan fills, shape and vine fills, photo
thread-painting, sequin/chenille/bead types (machine-specific), the TrueType turning-satin
rework and an intelligent multi-hoop split with editable lines (VP3, TAP, TBF, U01, DSB, DSZ,
KSM, ZXY and STX landed with #480 and #481).
The patents named in the research document are checked by number before the related item
ships; the facts per patent and the items each touches are in
[research/beyond/patents.md](research/beyond/patents.md) (shown active: Brother's
US 7996103 for the deferred direction-aware photo stitch, and BriTon Leap's families for cut
files from stitch data and for tracing a bitmap of stitch data, which concern
`object.toShapes`, B12 on stitch blocks and B14's fill fallback; the rest shown expired or
ceased). Not legal advice: the list for the owner's lawyer.

## Pattern and font files (Phase 6, #130 #131 #134)

Only files the owner makes in PE-DESIGN's own tools (Programmable Stitch Creator, Stitch
Design Factory, Font Creator) and saves through the normal interface are studied; Brother's
bundled patterns, motifs and fonts are never read. Done: PMV read (2026-10-29, the documented
layout, every byte checked). Waiting for the owner-made samples: PE-DESIGN's own PMV files (a
2- and a 4-point decorative stitch), Programmable Stitch Creator saves (a 2-point line, a
4-point square, two separate lines; a motif; a decorative-fill row) with a programmable fill
and a motif fill sewn from them, and a Font Creator font of one and two owner-drawn glyphs
with text set in it. Each lands as its own PR with the fixtures' provenance in a MANIFEST.
bundled patterns, motifs and fonts are never read. Done: import of `.pas`/`.pmf`/`.plf` text
sources in the documented `@DIRECTIVE` layout, checked against six owner-made Programmable
Stitch Creator files, and programmable fill, motif fill and motif line tiled as PE-DESIGN 11.50
sews them (2026-10-29: four reference specs); the motif fill's travel and ties and the motif
line's tie-in and lock as PE-DESIGN (2026-10-30: 130 and 23 needle points as PE); free decorative
lines placed and spaced as PE-DESIGN sews them (2026-10-30, `deco-fill-1line-100mm`). Next: how often
PE-DESIGN repeats a free decorative line and its outline travel (needs a second reference). Waiting for
the owner-made samples: PE-DESIGN's own PMV files and a Font Creator font of one and two
owner-drawn glyphs with text set in it (pe-refs-4 could not reach Font Creator and Stitch Design
Factory). Each lands as its own PR with the fixtures' provenance in a MANIFEST.

## Blocked

Wireless transfer, Link, USB Link, network machine settings and the Design Database's Card
Reading (Brother's USB card reader/writer) are Blocked: their protocols are undocumented and
will not be reverse-engineered. PHX and PEN import are Blocked for the same reason (PEN is an
encrypted purchase format). See the Blocked rows in [FEATURE_MATRIX.md](FEATURE_MATRIX.md).

## Dependencies on the owner

- New reference files, made in the owner's licensed PE-DESIGN 11.50 for the specs that ask for
  one ([compat/reference/README.md](../compat/reference/README.md)). Recorded so far (2026-10-28):
  fill and satin sweeps, the gradation sweep (Max/Min, the eight presets, Blend, Direction, no
  under sewing), Auto satin bends 30–120°, the Fit-text sweep (spacing, size, AHHHH, satin), one
  sample of every PES object kind, and two machine-stream designs (trim threshold, colour order).
  Still to record: satin bends on a curved band; Fit text on a line and a square, kerning on "AVA"; Stamp from an
  owner-drawn pattern, a two-colour text, an editable hole-sewing pair, the Cutwork and Embroidered
  Patch wizards, non-ASCII Design Property text; frame moves between 2.2 and 2.4 mm for the exact trim bound (0.5–2.0 mm gaps and four-colour
  ordering are recorded), satin columns over 30 mm wide (16–30 mm are sent whole), and the
  other-format exports once their writers match.
- A test sew-out of an OpenStitch-written file on the owner's Brother machine (the machine
  tier).
- A lawyer's review of the policy on studying PE-DESIGN output files (allowed as project policy
  since 2026-10-27; [LEGAL_AND_LICENSING.md](LEGAL_AND_LICENSING.md)).
- Code-signing certificates (macOS, Windows) and a Flathub account.
- The decision on `unsafe` in one small, audited crate per OS (macOS file opening, Quick Look
  and Explorer thumbnails). Decided 2026-11-06: another tech stack is fine where an OS needs
  it. `batch:mac-platform` (2026-11-06) shipped #557's macOS half (a Swift Quick Look
  extension over the Rust renderer, plus `stitchtool thumbnail`); each MSI now replaces an installed OpenStitch of the same version instead of adding another entry (#742, 2026-11-07, checked on the built MSI in CI); the Explorer half shipped on 2026-11-07 (#669, `platform`: a Rust COM thumbnail handler outside the workspace, installed by the MSI). #319's Ready-to-Sew Designs path is checked in the built macOS bundle, with the real
  click left to the owner.
