# Compatibility audit — 2026-10-19

The 20 highest-impact differences between OpenStitch and PE-DESIGN 11. They cover stitch
generation, editable PES and embroidery-font behaviour. Each entry gives the Rust module, the
evidence, a reproducible test and the proposed fix. The audit was made at `7b02267`.

## What "reference behaviour" means here

Since 2026-10-27 the items are checked against files saved by the owner's PE-DESIGN 11.50
(black box). [COMPATIBILITY_STATUS.md](COMPATIBILITY_STATUS.md) lists, per item, the specs
at the reference tier; the "Next 11" list below records what is still open.

The audit uses the reference evidence that is available and permitted under
[LEGAL_AND_LICENSING.md](LEGAL_AND_LICENSING.md):

- **Guide:** Brother's public PE-DESIGN 11 online guide. Page links are given per item; the full
  index is in [research/pedesign11-manual-toc.md](research/pedesign11-manual-toc.md).
- **Spec:** public format descriptions:
  - pyembroidery's [PesWriter](https://github.com/EmbroidePy/pyembroidery/blob/master/pyembroidery/PesWriter.py)
    and [PesReader](https://github.com/EmbroidePy/pyembroidery/blob/master/pyembroidery/PesReader.py)
  - the [EduTechWiki PES page](https://edutechwiki.unige.ch/en/Embroidery_format_PES)
  - the [libpes wiki](https://github.com/frno7/libpes/wiki/PES-format)
- **Measured:** OpenStitch output, produced with `stitchtool exec` at `7b02267`.

Each item has an evidence grade:

| Grade | Meaning |
|---|---|
| **Documented difference** | The code contradicts the guide or a public spec. It can be fixed now. |
| **Documented, unverified** | The guide or a spec describes the behaviour, but no test proves we match it. |
| **Needs reference** | The behaviour is undocumented. The exact values need a black-box reference (see below). |
| **Defect** | Wrong regardless of PE-DESIGN, for example a needle path no embroidery program produces. |

### REA evidence

No item has REA evidence, for two reasons:

- REA analyses executables, packages and bundles. It rejected a PES file (`oracle.pes`) as an
  unsupported format, so it cannot inspect embroidery data.
- Using REA on PE-DESIGN's own binaries is restricted by the technique register in
  LEGAL_AND_LICENSING.md:
  - decompiling stitch-generation or font code is **prohibited**;
  - decompiling file-format code is **prohibited pending legal review**.

Every item below therefore says "REA: none". The next section explains how undocumented
behaviour can still be settled without decompilation.

### Legitimate ways to settle undocumented behaviour

"Undocumented" is not treated as a permanent blocker. The permitted methods, from cheapest to
dearest:

1. **Black-box runs of a licensed PE-DESIGN 11** (Art. 5(3), allowed):
   - Open OpenStitch outputs in PE-DESIGN, and create the same test shapes there.
   - Save the results as PES and DST, then compare them with the harness below.
   - This settles every "Needs reference" value: densities, underlay geometry, connection
     points, trims and envelope shapes.
   - It uses the owner's licensed PE-DESIGN 11.50 on their own machine; the procedure is in
     [compat/reference/README.md](../compat/reference/README.md).
2. **PES files the owner saves from their own PE-DESIGN** (allowed). Studying fields that no
   public source describes, from those files alone, is allowed as project policy (owner's
   decision, 2026-10-27; see [LEGAL_AND_LICENSING.md](LEGAL_AND_LICENSING.md)). The law is
   unsettled there, and a lawyer's review stays on the owner's list.
3. **Sew-outs on the owner's Brother machine** (allowed). This is the only route to
   Machine Verified status.
4. **Art. 6 decompilation of file-format code, limited to interoperability.** This could
   apply to the editable PES object classes, and REA could do it. It is possible only after the
   legal review confirms the information is not otherwise available. It is not used for
   stitch-generation or font code.

## Summary

| # | Difference | Area | Severity | Grade | Needs reference to finish? |
|---|---|---|---|---|---|
| 1 | Satin sewn as a square wave, not a zigzag (**fixed**, internal regression) | Stitches | 5 | Defect | No |
| 2 | Default PES is truncated v1 with no object section (**fixed**, internal regression; open test pending) | PES | 5 | Documented, unverified | Black-box open test |
| 3 | Trims and jumps inside one fill or satin region; no Running Stitch Path (**fixed**, internal regression) | Stitches | 5 | Documented difference | No |
| 4 | Trims between underlay, top stitching and outline of one object | Stitches | 4 | Documented, unverified | Single-needle policy |
| 5 | Every design is written as one stitch group, nothing editable (**partly fixed**, 2026-10-29: `pesVersion` 11 writes shapes as `CEmbLine`, which PE-DESIGN 11.50 opens editable, and Lato Bold text as `CEmbNText`, which it opens editable and retypable (both reference); stitch patterns as `CEmbOne` stitch groups (reference, `pes-0110-stitches`); Text Transform and text on an outline open editable (reference, 2026-10-29); Hoop Size pages written as PE-DESIGN writes them since 2026-11-04 (#322); object kinds still refused: #608) | PES | 5 | Reference | Yes (the kinds of #608) |
| 6 | One sewing direction per text instead of per stroke (**partly fixed**: satin text sewn stroke by stroke since 2026-10-31, fill text since 2026-11-04 (#376); open: under sewing per stroke, fill text under a Text Transform (#428), satin N, M, A and V corners (#446)) | Fonts | 5 | Documented difference | Angles |
| 7 | Auto direction is one fixed angle (**fixed**, internal regression) | Stitches | 4 | Documented difference | No |
| 8 | Underlay model differs (**partly fixed**: PE's two region types, Density Dense / Medium / Light, the Manual Punch column's edge / piping / zigzag; zigzag lines have under sewing, on by default as PE-DESIGN's, `line-zigzag-default`; open: which type PE applies to the other region kinds, the zigzag's split points) | Stitches | 4 | Documented difference | Geometry |
| 9 | PE-DESIGN-made PES files lose objects, properties and colours on re-save (**partly fixed**, 2026-11-05, #375, #397, #398, #120: an opened file exported unchanged is written back byte for byte; its thread table reaches the stitches; every PE-DESIGN `#PES0110` file imports as objects, texts and stitch groups included, and an edited one exports as `#PES0110`; an edited file still loses the other region types' own settings, motif size and spacing, per-letter colours and unmapped fields) | PES | 4 | Documented difference | No |
| 10 | Hoop and Design Page fields hard-coded in PES (**fixed**, 2026-11-04, #322: `#PES0110` page, hoop, machine type and sections byte for byte against PE-DESIGN's files; v1 and the v6 display fields have no PE-DESIGN reference) | PES | 4 | Reference | No |
| 11 | No Connection Points (Default / Closest / Farther) for text | Fonts | 4 | Documented difference | Geometry |
| 12 | No minimum font sizes or small-text fonts (**fixed**, 2026-11-07, #527, #530: size ranges with a warning, the OpenStitch Line small-text font, compared with PE-DESIGN's recorded Small Text numbers, internal regression) | Fonts | 4 | Documented difference | No |
| 13 | No per-character colour or multi-character selection | Fonts | 4 | Documented difference | Colour grouping |
| 14 | Programmable fill and fill default to 0° instead of 45° (**fixed**: programmable fill internal regression, fill 2026-10-30 from pes-objects r01; type change rule pending) | Stitches | 3 | Documented difference | No |
| 15 | Missing fill, satin and zigzag attributes; wrong ranges (**partly fixed**: Frequency and ranges; Stitch Type 1 and 2 (2 partly: exact at 0°, 30°/45° rows offset); Half Stitch for Auto satin since 2026-11-04; zigzag Sharp Corners and Miter Limit since 2026-11-07, #373) | Stitches | 3 | Documented difference | Geometry |
| 16 | Optimize Sewing Order does not move entry and exit points (**fixed**, reference) | Stitches | 3 | Documented difference | No |
| 17 | Hole sewing not validated or paired (**fixed**, internal regression) | Stitches | 3 | Documented difference | No |
| 18 | PES thread table drops chart names and colour type (**fixed**: round trip; PEC chart colours written as PE-DESIGN writes them since 2026-11-04) | PES | 3 | Documented difference | Chart strings |
| 19 | Design Property metadata cannot be set; lost on import (**fixed**, internal regression) | PES | 3 | Documented difference | No |
| 20 | Character kerning moves one glyph without reflowing the line | Fonts | 3 | Documented, unverified | Reflow rule |

Line references are relative to `crates/`.

---

## 1. Satin is sewn as a square wave, not a zigzag

- **Minimum density (2026-10-28, `sweep-satin-density-0p2/0p3/0p5.pes`, reference tier):** between
  PE-DESIGN's minimum 0.1 lines/mm (parked: PE sews 5 crossings 10 mm apart) and 0.5, the crossings
  are joined by diagonals whose longest stitch OpenStitch matches exactly (11.66, 8.99, 7.21 mm) with
  the counts within 10 %. PE's density field has one decimal, so 0.15 cannot be set.
- **Wide columns (2026-10-28, `machine-satin-width-16/18/20/22/25/30.pes`, reference tier):**
  PE-DESIGN sends satin crossings of 16 to 30 mm to the machine whole (every stitch the full
  width, 209 stitches on each 40 mm column at density 4.5) and OpenStitch matches it: no split
  below 30 mm.

- **Impact:** severity 5.
  - Every satin row goes straight across, then takes a short stitch along the edge to the next
    row.
  - That doubles the stitch count and puts a ~0.33 mm stitch on both edges of every column at
    density 3, which perforates the fabric and ragged-edges the column.
  - It affects region satin, Manual Punch columns and skeleton satin.
- **Module:**
  - `openstitch-design/src/generate.rs:161-169`: `satin` emits `[start, end]`, then
    `[end, start]`.
  - `openstitch-design/src/column.rs:147` and `openstitch-design/src/skeleton.rs:212` have the
    same `k % 2` flip. The comment at `skeleton.rs:201` describes "left, right, left…", which
    the code does not do.
- **Evidence:**
  - Measured: a 5×2 mm rectangle, satin at 90° and density 3, gives 32 points with segment
    lengths 2.0, 0.33, 2.0, 0.33, ….
  - The guide describes satin only in pictures
    ([region sewing attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html)).
- **REA:** none.
- **Grade:** Defect.
- **Measured after the fix (`c5f3bad`):** the same 5×2 mm rectangle sews 15 stitches of
  2.0–2.03 mm, alternating edges, with no edge stitches.
- **PE-DESIGN 11.50 behaviour** (black-box, 2026-10-27; references
  `compat/reference/stitches/satin-zigzag-rect.pes` and `satin-zigzag-column.pes`, under
  sewing off): no square wave, but not a symmetric zigzag either. Fixed-direction and Manual
  Punch satin is a sawtooth: each row (rung) is sewn across along the sewing direction and the
  stitch to the next row slants back, rows 2/density apart (2.0 mm crossings and 2.09–2.15 mm
  slants on the 5×2 mm column). Every satin object starts with a tie-in (0.3 mm out along the
  first stitch and back; three steps out and two back after a first stitch over 3 mm) and ends
  with a lock (0.3 mm back along the last stitch and return; two steps after a last stitch over
  4.25 mm), per `satin-lock-column-3p5.pes` and the other satin references.
  A rectangle starts and ends in the middle of its bottom edge: PE runs along that edge to one
  end, sews back to the middle (the last row 0.2 mm short, reached along the far edge), then
  does the same for the other half: 27 stitches, 7 of them short edge stitches at the start
  point. The Manual Punch column sews 18: tie-in, 15 crossings over 8 rungs, lock. Auto satin
  stays a symmetric zigzag.
- **Status:** fixed to PE-DESIGN's structure, reference tier. Region satin with a fixed
  direction and Manual Punch columns sew the sawtooth (`region::sawtooth_satin`,
  `column::sawtooth`), split at a start point inside the column (`region::split_satin`), with
  PE's tie-in and lock (`region::tie_and_lock`); skeleton and Auto satin keep the symmetric
  zigzag. satin-zigzag-column and -skeleton measure the body without the tie-in and lock
  (`exclude`); satin-zigzag-rect expects PE's observed values. Pull compensation still
  lengthens every row. Regions of several sections (the U, the H) are sewn in PE's order
  (`region::sections_in_order`, satin-sections-h): an opening row along the start point's
  edge, the sections from the far end of the rows back, legs from the tip corner that comes
  first in the outline, the start point's section last; a feathered edge keeps the sawtooth
  and moves the rows' starting ends by PE's fixed pattern (satin-feather). Tests: `crates/openstitch-engine/tests/satin_compat.rs`
  (`region_satin_sews_a_zigzag_across_the_column`, `manual_punch_column_sews_a_zigzag`,
  `skeleton_satin_sews_a_zigzag`, `pull_compensation_widens_the_zigzag`).
- **Test:**
  - Inputs: `design.new` → `shape.rectangle {x:10,y:10,width:5,height:2}` →
    `object.setRegion {id:1,type:"satin",angle:90,density:3}` → `object.toStitches {ids:[1]}`
    → `stitches.points {id:1}`.
  - Assert: consecutive needle points alternate between the two edges, no stitch shorter than
    0.5 mm lies along an edge, and the stitch count is 15 ±1.
  - Repeat with `shape.column` and `shape.skeletonSatin`.
- **Fix:**
  - One needle point per row: left on even rows, right on odd rows (rungs in `column`/`skeleton`).
  - Update `satin_alternates_rails_and_follows_the_turn` and the skeleton tests to assert side
    alternation.
  - Golden fixtures that contain satin change and must be regenerated in the same PR.

## 2. Default PES is truncated v1 with no object section

- **Status (2026-10-19): fixed, internal regression.** `WriteOptions::default` is v6 with the
  stitch group; `FileType::write`, `design.export`, `nameDrop.export`, `transfer.send` and
  `library.convert` follow; `1t` stays as the explicit machine-only choice. Tests:
  `default_pes_is_version_6_with_objects_and_thread_table`, `pes_compat::*_writes_full_v6`,
  `transfer::tests::sent_pes_is_version_6_with_objects_and_threads`. Not verified: the
  black-box open test in PE-DESIGN below is still needed before it can be called Reference
  Verified.

- **Impact:** severity 5.
  - Most paths write `#PES0001` with no object section and no thread table: `design.export`
    without `pesVersion`, lettering output, USB transfer and Design Database convert.
  - A public write-up says PE-DESIGN opens such a file as a blank document.
  - Even if it opens, colours fall back to the PEC chart and names are lost.
  - Machines are unaffected, since they read the PEC section.
- **Module:**
  - `openstitch-formats/src/pes.rs:321-328`: `WriteOptions::default` is V1, truncated.
  - `openstitch-formats/src/lib.rs:159`: `FileType::Pes`.
  - `openstitch-engine/src/commands.rs:2096-2099`: the `"1t"` default.
  - `openstitch-engine/src/lettering.rs:831`.
  - `openstitch-engine/src/transfer.rs:375` and `openstitch-library/src/convert.rs:172` use
    the defaults.
- **Evidence:**
  - Spec: EduTechWiki says a minimal PES "will load normally and display a blank document" in
    PE-DESIGN.
  - pyembroidery's default v1 is not truncated.
- **REA:** none.
- **Grade:** Documented, unverified.
- **Test:**
  - Inputs: `design.new {name:"t1",width:100,height:100}` → `shape.rectangle {fill:"Red",outline:"Black"}`
    → `design.export {path:"a.pes"}`. Do the same through `library.convert` from `oracle.dst`
    and through `transfer.send`.
  - Assert: every user-facing PES has version ≥ 6, at least one `CEmbOne` and a thread table
    (`pes::read_objects`).
  - Black-box: open `1t`, `1`, `6t` and `6` outputs in PE-DESIGN and record what appears.
- **Fix:**
  - `WriteOptions::default` → `{V6, truncated:false}`, and route `FileType::write` for PES
    through it.
  - Change the `"1t"` defaults in `commands.rs` and `lettering.rs`.
  - Keep `1t` as an explicit "machine only, smallest" choice.
  - v6 is already byte-identical to pyembroidery, so the risk is low.

## 3. Trims and jumps inside one fill or satin region; no Running Stitch Path

- **Impact:** severity 5. Concave or holed regions (letters, U shapes) get trims and jumps
  inside the region, where PE-DESIGN runs hidden travel stitches. The result is extra trims,
  loose tails and longer sew time.
- **Module:**
  - `openstitch-design/src/model.rs:760`: `travel = false` for Fill and Satin.
  - `openstitch-design/src/region.rs:165`: rerouting only when `travel` is on.
  - `openstitch-design/src/compile.rs:27-28,74-81`: gaps over 3 mm become TRIM+JUMP.
- **Evidence:**
  - Guide ([region sewing attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html)):
    for Satin and Fill, Running Stitch Path is Inside of region, Along outline or On outline.
    All three travel with running stitches; there is no jump option.
  - Measured: a U polygon (10,10)-(40,10)-(40,40)-(30,40)-(30,20)-(20,20)-(20,40)-(10,40)
    filled at 0° gives 2 trims and 2 jumps inside the object.
- **REA:** none.
- **Grade:** Documented difference.
- **Status:** fixed, internal regression tier only (not reference verified).
  - `RegionSewing::running_stitch_path` (`RunningStitchPath {Inside, AlongOutline, OnOutline}`)
    replaces `travel`; fill and satin default to Inside. Old `.osd` files load: `travel: true`
    is Inside, `travel: false` or no value keeps the jumps they sewed before.
  - `region::reroute_with` and `paths::TravelGraph::outline`: Inside is the shortest path inside
    the region; On outline walks the outline the shorter way round; Along outline walks it
    0.5 mm inside (a placeholder until measured). Both outline variants fall back to Inside
    when the two ends are not on one contour.
  - Exclusions: `object.setRegion {runningStitchPath}` is refused with Auto direction, a
    feathered edge, on text, for other sew types, and Inside with gradation. Where the
    attribute is unavailable the region travels inside (what PE-DESIGN sews there needs a
    reference); gradation turns a stored Inside into Along outline.
  - Exposed in `object.setRegion` (`runningStitchPath`; `travel` still accepted) and the
    Sewing Attributes pane (Running stitch path, hidden for text).
  - Tests: `crates/openstitch-engine/tests/region_compat.rs`
    (`fill_and_satin_travel_between_parts_without_trims` for the U polygon with every value,
    satin and gradation; `the_outline_paths_differ_from_inside_travel`,
    `running_stitch_path_follows_the_guide_exclusions`,
    `running_stitch_path_defaults_and_old_files`, `old_fill_without_travel_still_jumps`) and
    `outline_travel_walks_on_or_just_inside_the_outline`. The golden fixtures did not change:
    their regions have one part each, or come from `.osd` files without `travel`.
- **Test:**
  - Inputs: the U polygon → `object.setRegion {type:"fill",angle:0}` → `object.toStitches` →
    `stitches.points`.
  - Assert: 0 trims inside the object, and every travel segment's midpoint inside the region
    (0.05 mm tolerance).
  - Repeat for satin, and for each Running Stitch Path value.
- **Fix:**
  - Replace `travel: bool` with `RunningStitchPath {Inside, AlongOutline, OnOutline}`,
    defaulting to Inside for Fill and Satin.
  - Implement the outline variants in `region::reroute` / `paths::TravelGraph`.
  - Follow the guide's exclusions: Inside is not available with gradation, and the attribute is
    not available with Auto direction, text or a feathered edge.

## 4. Trims between underlay, top stitching and outline of one object

- **Settled 2026-10-28** (references `machine-jumps.pes`, `machine-trim-bound.pes`,
  `machine-trim-fine.pes`, specs of the same names at the reference tier): between same-colour
  parts PE-DESIGN 11.50 sews a needle-to-needle move of up to 2.24 mm across as a plain stitch
  (no jump at all) and trims a move of 2.33 mm and more (gaps of 0.5, 0.8 and 1.0 mm sewn, 1.1 mm
  and up trimmed, with its exit 0.5 mm inside the right edge and 1 mm up and its entry 0.5 mm
  inside the next left edge: moves of sqrt((gap + 1)^2 + 1)). OpenStitch's compiler now sews a
  move of up to 2.3 mm between objects across (`Compiler::sew_across_distance`) and trims every
  longer one, so no plain interior jump is written. Open: where PE-DESIGN puts the optimized
  exit and entry (0.5 mm inside the edges; OpenStitch's lie on the edges, so the specs draw the
  squares at PE's needle-to-needle moves), and the value PE's multi-needle "minimum jump stitch
  length for trimming" field shows (read the Design Settings dialog; the recordings were made
  with the multi-needle machine type at its default). Also settled (`machine-rbr.pes`, reference
  tier): objects drawn red, blue, red sew as both reds (one block, a trim between them) then blue,
  one colour change, as `order.optimize` does. Also (`machine-rbr-far.pes`, `machine-rbgb.pes`, reference tier): the reds merge even
  70 mm apart, and red, blue, green, blue sews red, both blues, green: same colours merge in
  first-occurrence order across other colours.

- **Impact:** severity 4. One same-coloured rectangle with underlay and an outline sews with
  3 trims and 3 jumps. That means more cuts, more tie-offs and longer sew time.
- **Module:**
  - `openstitch-design/src/compile.rs:114`: the first run of each element calls
    `travel(.., false)`.
  - `compile.rs:277-286`: underlay, top stitching and outline are separate elements.
  - `compile.rs` `Writer::travel`: every frame move between elements is TRIM+JUMP on
    single-needle machines (PE-DESIGN 11.50 trims every interior move: compat spec
    `machine-formats`, evidence); multi-needle machines trim moves over the design's minimum
    jump length (3 mm when unset).
  - `openstitch-design/src/machine.rs:305-310`.
- **Evidence:**
  - Guide, [entry and exit points](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_1858418.html):
    stitching continues from the region exit (R2) to the line entry (L1).
  - Guide, [jump trimming](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_1908282.html):
    per-object on/off with a minimum length, for multi-needle machines only.
  - Measured: a 30×30 rectangle with fill, zigzag underlay and a same-colour outline gives
    trims at stitch indexes 61, 738 and 789 on the default single-needle machine.
- **REA:** none.
- **Grade:** Documented, unverified for the routing. The single-needle trim policy is now
  measured: in 177 PE-DESIGN 11.50 PES files (2026-10-27) every interior frame move carries
  the PEC trim flag and no plain jump occurs inside the design, so PE-DESIGN does write trims
  for single-needle machines, for every move it does not sew across (shortest observed trim
  3.0 mm). OpenStitch's compiler trims every interior move since the `machine-formats` spec.
- **Test:**
  - Inputs: `shape.rectangle {x:50,y:10,width:30,height:30,fill:"Blue",outline:"Blue"}` →
    `object.setRegion {id:1,type:"fill",angle:0,underlay:true,underlayStyle:"zigzag"}` →
    `stitches.points`.
  - Assert: 0 trims inside the object. On multi-needle, trims only for jumps ≥ `minJumpTrim`.
- **Fix:**
  - Route an object's layers as one sequence: underlay end → top start → region exit → line
    entry, with inside travel.
  - Gate trims on machine type, the per-object `trim_jumps` setting and the minimum length.
  - Keep the forced cut before a colour change.

## 5. Every design is written as one stitch group; nothing stays editable in PE-DESIGN

- **Status (2026-10-29):** `design.export` with `pesVersion` 11 (editor: File ▸ Export ▸ PES ▸
  PE-DESIGN 11) writes `#PES0110` with one `CEmbLine` per shape in PE-DESIGN 11.50's own layout:
  fill and satin regions, running, zigzag, triple, E, V, stem, candlewick and chain lines, colours, under sewing, density,
  direction, Frequency, Running Stitch Path, the gradation flag, Bézier outlines, groups, the page
  size and the sewing-order list (`openstitch-formats/src/pes11.rs`, `openstitch-engine/src/pes11_export.rs`;
  FILE_FORMATS.md "PES 0110 object list", "Writer"). The 37 owner files the reader parses
  completely are rewritten from what it read and read back equal, and their object sections are
  byte-identical to PE-DESIGN's outside the documented differences (the pattern block, the
  geometry marks' parameters, the hoop fields). Text, stitch patterns, several outlines per
  object and the other region types and motif lines are refused with the object named; version 6 stays
  the default. **PE-DESIGN's check (2026-10-29, reference tier):** PE-DESIGN 11.50 opens the
  written rectangle with no warning and the shape editable, and re-saves the object section
  unchanged except its own pattern block (spec `pes-0110-editable-shapes`, MANIFEST entry). It
  showed the region direction 0°: the file's value, because OpenStitch's default fill direction
  is 0° where PE-DESIGN's tools start at 45° (r01). The thread records now carry PE-DESIGN's
  BROTHER EMBROIDERY numbers for OpenStitch's chart colours (it showed `16 / Brother` before).
- **Text (2026-10-29):** Lato Bold text is written as `CEmbNText` with one `CTTFLetter` per
  character: size, Satin or Fill with density and colour, a running, triple or zigzag line,
  line breaks and spacing, and the click point PE-DESIGN stores (the first line's start).
  PE-DESIGN keeps no outlines for text and lays it out again from the font. The 18 owner text
  files are rewritten byte-identical outside two PE-side quirks (FILE_FORMATS "Writer").
  Spaces are letters (r27). Centred text is written as PE-DESIGN writes it (alignment field
  and click point), and Text Transform text as `CEmbDText` with PE-DESIGN's preset code and
  default strength (FILE_FORMATS "`CEmbDText`"; spec `pes-0110-editable-text-transform`,
  reference tier: PE-DESIGN opens it editable with the preset at its default level, retypable;
  **open:** OpenStitch's arch sews HHHHHHH 79.9 mm wide (79.6 since 2026-11-08, #428) where PE-DESIGN sews the same object
  79.2 mm (its own text-transform-1: 79.3) round the same 40.8 mm circle, so the spec's width
  check stays at the internal tier until the letters' placement round the circle matches at
  0.3 mm), and text fitted to an outline as `CEmbAText` with its Fit Text to Outline settings
  and the outline it follows (spec `pes-0110-editable-text-fit`, reference: PE-DESIGN 11.50 opens it
  with the text still fitted to the circle and re-fits it when the circle is reshaped; its re-save
  rewrites only the circle's pattern block and the text's box, which it recomputes); Manual
  Punch columns as PE-DESIGN's Straight and Curve Block objects (specs `pes-0110-manual-punch`,
  `pes-0110-curve-block`, reference tier: PE-DESIGN 11.50 opens both as editable blocks and
  re-saves them unchanged but for its own pattern block, 2026-10-31; owner files m01–m11 read and rewritten byte for
  byte outside the pattern block; Text > Convert to Blocks' output (stroke-blocks-H, -A) read as
  blocks and running pieces, its blocks' marks equal but for the floats' last bits, and imported
  as editable objects since 2026-10-31 (`import.pes`: shapes, Manual Punch blocks with their
  direction lines, groups; texts and stitch groups since 2026-11-05, #397). OpenStitch sews the imported H in 342 needle points with 5 trims and the A in 364 with 6, against PE-DESIGN's 285 and 293 with none: each block and running piece is sewn on its own, where PE-DESIGN sews the group as one run (step 4 of the lettering plan).
  hole sewing
  pairs read and written (spec `pes-0110-hole-sewing`, reference pending; one pair per file
  without groups, from the owner's three hole files); a Curve Block written from a design has handles within one
  file unit of PE-DESIGN's, which way PE-DESIGN rounds them not mapped); the Appliqué wizard's stages as PE-DESIGN's appliqué lines and colours (spec
  `pes-0110-applique`; owner files a01–a05 read and rewritten byte for byte outside the
  documented differences; PE-DESIGN 11.50 refused the export of 2026-10-30 with
  "Unexpected file format" (2026-10-31): the colour addendum after the PEC block wrote B, G,
  R and `0D` for every block where PE-DESIGN writes R, G, B and `FA`/`FB`/`FC` with PEC 62–64
  for the appliqué stages; fixed the same day, and PE-DESIGN opens the export, below); a stitch pattern as a `CEmbOne` stitch group (spec
  `pes-0110-stitches`, reference tier: PE-DESIGN opens it as one stitch object and re-saves the
  PES section byte for byte; it re-sews the group without the zero-length stitch that
  `stitches.import` restores on each run's first needle point, issue #135);
  `pes-0110-applique`, reference tier: PE-DESIGN 11.50 opens the export with no group of a
  group of its four lines (number corrected), its lines editable in PE-DESIGN's appliqué colours, and re-saves it
  unchanged but for its pattern blocks, 2026-10-31; owner files a01–a05 read and rewritten byte
  for byte outside the documented differences; **open:** the wizard's V tack-down shows as an E
  stitch (the field telling E from V is not mapped), and the appliqué sews 29 x 29 mm where
  PE-DESIGN's wizard sews the same square 32,6 x 32,8 mm, OpenStitch's outline offset);
  Shapes-gallery shapes (`CEmbSpecialShape`, `CEmbLine` records under another class name)
  round-trip (o-gallery-heart, shape-start-points); programmable fill written with its pattern
  in the header's pattern list (spec `pes-0110-programmable-fill`, reference pending; the list
  byte-identical to obj-progfill-owner-v's for the owner's V; one pattern per design); decorative
  fill with its pattern in the object's pattern block (spec `pes-0110-decorative-fill`,
  reference pending; the block byte-identical to deco-fill-1line-100mm's); the other Round 3
  region types by their codes at PE-DESIGN's default attributes (their own fields not mapped);
  stem, candlewick and chain lines by their codes (spec `pes-0110-line-types`, reference
  pending; the 22 owner line-* files read and rewritten byte for byte outside the documented
  differences), their Start/End options at PE-DESIGN's defaults (open: the sewing end
  PE-DESIGN stores for them, +84 and +86 on the line-*-startend-* files, and the field behind
  their sewn extent). Groups: the group's number is one more than its last
  member's archive number (a01, o-group-two-rects, group-partial); OpenStitch wrote the class's
  number, which PE-DESIGN 11.50 refused ("Unexpected file format") for the appliqué export's
  group of four lines without the square (2026-10-31), fixed; several groups (PE-DESIGN's
  group-two) are not mapped and are left out of the file (open). Refused for
  now: right-aligned and vertical text, stretched fit, rotation, letter spacing, kerning,
  per-character edits, other fonts.
  **PE-DESIGN's check (reference tier, `pes-0110-editable-text`):** it opens the file as
  editable text with every attribute as written and retyping works. Two differences came with
  it: a Hoop Settings tab with no stitch count in the status bar, from the Custom Size page
  the file was on (a 100 × 100 mm design is now written on PE-DESIGN's default Hoop Size page,
  with the PEC header form PE-DESIGN writes for it; confirmation pending, probe p3), and a
  plain click that seemed not to select the text: it does, on a stroke, as PE-DESIGN's own
  text.
- **Object-class samples (2026-10-28):** `crates/openstitch-formats/tests/fixtures/pe11/o-*.pes`
  (PROVENANCE.md "Round 3") hold one owner-drawn example of every object kind PE-DESIGN writes:
  the Outline, Rectangle/Circle/Arc and Manual Punch tools and every region type are `CEmbLine`,
  gallery shapes `CEmbSpecialShape`, plain and vertical text `CEmbNText`, Text Transform
  `CEmbDText`, text fitted to an outline `CEmbAText` (the compat `text-fit-*` files), plus a group,
  the Applique Wizard's output, page 200 × 200, Hoop-Size pages (7.7 KB files: a shorter header)
  and the single-needle machine type. Small Text, Monogram, Motif and Decorative Fill use Brother
  content and have no sample.

- **Impact:** severity 5. In PE-DESIGN, a design saved by OpenStitch arrives as a single
  stitch pattern:
  - only needle points can be edited;
  - shapes and text lose their sew types and attributes;
  - text cannot be retyped;
  - the per-object sewing order is gone.
- **Module:**
  - `openstitch-formats/src/pes.rs:380-392,476-533`: one `CEmbOne`+`CSewSeg` per file.
  - `openstitch-engine/src/commands.rs:2106`: exports the compiled stream.
  - `pes.rs:235-238`: the reader stops at an unknown class.
- **Evidence:**
  - Guide, [types of data](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/03_1219818.html):
    stitch patterns only allow point edits.
  - Spec (EduTechWiki, libpes): `CEmbRect`, `CEmbCirc` and `CEmbLine` have partial field
    lists; `CEmbPunch`, `CSewFigSeg`, `CLetter` and `CEmbNText` are unknown.
- **REA:** none. Decompiling PE-DESIGN's PES writer is prohibited pending legal review (method 4).
- **Grade:**
  - Rectangle, circle and line: documented, unverified.
  - Text and filled outlines: needs reference.
- **Test:**
  - Now: write one `CEmbOne` per OpenStitch object and assert the object count equals the
    design's object count. PEC stays byte-identical to today's output.
  - With a reference: a 40×20 mm rectangle exported as v6 must contain `CEmbRect`, with
    l/t/r/b within ±1 unit. In PE-DESIGN, the file must open with Sew type editable.
- **Fix:**
  - Short term: `pes::write_objects(&[StitchPattern])`, fed by `Compiler::compile_object`,
    giving one stitch group per object in sewing order. This needs one black-box check that
    PE-DESIGN accepts several `CEmbOne`.
  - Long term: shape classes from the public field lists, checked against owner files. Text
    only after method 2 or 4.

## 6. One sewing direction per text instead of per stroke

- **Impact:** severity 5. PE-DESIGN's built-in fonts give each stroke its own sewing direction,
  so satin follows each stroke. OpenStitch gives every glyph in a text the same angle, so
  diagonal and curved strokes are sewn at the wrong angle. This is the largest lettering
  quality gap.
- **Module:**
  - `openstitch-design/src/compile.rs:263-284`: each glyph reuses the object's `RegionSewing`.
  - `openstitch-design/src/region.rs:86-99,162`: the direction is defined in object
    coordinates, not per glyph.
- **Evidence:**
  - Guide, [converting text](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1915274.html):
    Convert to Blocks keeps the multiple sewing-direction lines, while Convert to Outline makes
    the direction uniform.
  - Guide, [region sewing attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html):
    for satin text, Direction cannot be set.
- **Measured (2026-10-30, [research/stroke-lettering.md](research/stroke-lettering.md)):** the
  owner's Lato Bold references show PE-DESIGN cutting each TrueType letter into strokes, for
  Fill as for Satin: the satin H is sewn as the left stem's top part, its bottom part (sewn
  upwards), the crossbar with vertical crossings and the right stem (285 needle points against
  OpenStitch's 243, all horizontal); in B the rows turn with each bowl in steps of 12-20°; x and
  g are sewn in two runs with a trim between (11 trims in "OpenStitch" against 9). The note lists
  the open rules (cutting at junctions and corners, order, under sewing, size) and the owner
  captures requested to settle them.
- **REA:** none. Font internals are in the prohibited category.
- **Grade:** Documented difference. The per-stroke angles are measured on the H, A, B, x and g,
  and the round-5 captures settle the cut. Step 1 is in (2026-10-30, `openstitch_design::strokes`:
  a glyph's strokes, junctions and loops with PE-DESIGN's structure on Lato's I, L, T, H, X, V, O,
  A, S, e, i), and the rungs from each stroke's side edges joined at equal fractions of their
  lengths, as PE-DESIGN's Convert to Blocks lines (H within 2°; A's legs within 10-25° until the
  apex ownership of step 4); PE-DESIGN's own cut imports from its Convert to Blocks files
  (`import.pes`, 2026-10-31: H and A as its five blocks each), and the blocks are in
  PE-DESIGN's order since 2026-10-31 (`GlyphStrokes::blocks`: H's five blocks within 0.5 mm of
  stroke-blocks-H; A's five in its order and directions, the right leg with the apex; X two chevrons). Since then
  satin text is sewn by stroke (block by block, travel along
  the outline, chevrons at a crossing, the corner tip to the column sewn last): the 13 round-5
  satin captures (`stroke-satin-*`) and `text-defaults-h` are within 25 % of PE-DESIGN's needle
  points, 9 % on average against 26 % by region (H 291 against 285, A 283 against 290). Fill
  text is sewn by stroke since 2026-11-04 (#376), as PE-DESIGN's fill H shows: each block a
  row across the stroke and a slanting row back, every crossing split on the Step pitch grid
  moved by the Frequency, rows fanned round L's corner. The 11 round-5 fill captures
  (`stroke-fill-*`) and `text-defaults-fill` are within 25 %, 8 % on average against 22 % by
  region (H 504 against 483, was 595; L 274 against 276). Under a Text Transform the walk
  between a letter's blocks no longer puts a needle point on every point of the warped outline
  (2026-11-07, #428): it is simplified to within 0.1 mm of the outline and split into 2 mm
  stitches as on the straight letter. Satin text under a Transform sewed nearly twice the needle
  points it does straight (HHHHHHH, preset 3: 1831 against 1043), now about as many (1023).
  Fill text under a Text Transform or a free envelope (#535; a free envelope was sewn by stroke
  until 2026-11-07, unlike the preset it copies) keeps its region rows (#428): with the walk fixed, sewing it
  by stroke is nearer PE-DESIGN on the 182 transform captures in every way (needle count 10.9 %
  off on average against 11.2 % by region, was 28 % before the walk; PE → ours 0.24 → 0.19 mm;
  coverage 0.761 → 0.772), but its rows reach the warped outline's corners and five references
  go 0.1 mm past their 0.5 mm bounds (text-transform-1-agjy, -1-openstitch-l25, -2,
  -18-openstitch-l100 and text-transform-10's stem columns). That is the Text Transform layout's
  own error, which the region rows hid: on the arches the left letters sit up to 0.6 mm too far
  along the arc, by region as well. Next, the arches' letter placement and envelopes 10 and 18. Since 2026-11-05 (#444, the
  README's arched title and 4 mm text sewn as broken pieces): a turned letter (fitted to an
  outline, on an arch, turned by Edit Character) is cut into strokes upright and its blocks
  turned back, as PE-DESIGN sews it (text-fit-satin-ahhhh: its H's at -18 to +32 degrees match
  the upright H within 0.1 mm; turned, Lato's I lost its stroke at 35 degrees and P and E fell
  apart); satin crossings are spaced 0.4 mm along the block's outer side between rungs 0.02 mm
  apart (stroke-satin-O: 0.41 mm outside), along the inner side round a corner, instead of
  cutting chords between 3 mm rungs (a 4 mm o's crossings stopped 0.3-0.45 mm inside its
  outline), in blocks that bend, turn a corner or are one or two 3 mm sections long; straight
  blocks and junction squares keep their 3 mm rungs (fine rungs fanned where a junction cuts a
  side short: A's legs at the bar); a bending satin stroke's sides are joined by the centre
  line's progress, so crossings stay across S's spine as PE-DESIGN's do (fill rows keep the
  length pairing their spacing was measured with); and the stroke cut's grid has 200 cells across every letter (it never went
  under 0.02 mm cells, which coarsened 4 mm letters: n lost its stem's top and fell back to
  region). The 13 satin captures and text-defaults-h are now 5.6 % from PE-DESIGN's needle points on average (was
  9.3 %), none further than before: O 338 (was 310) against 358, S 267 (215) against 278, 8 357
  (317) against 373, e 267 (247) against 267, L 136 (176) against 152, H-10 151 (145) against
  149; X 314 against 277 and T 208 against 186 unchanged and the furthest (#446). Fill text is
  unchanged (8.0 %). Under sewing per stroke is
  still open (step 5 of the research note), and no satin capture under 8 mm exists yet.
  Since 2026-11-05 (#465, small lettering): each separate piece of a letter is cut into strokes
  and sewn on its own, with trims between (the i's dot and stem, the pieces a small N is cut
  into), as PE-DESIGN sews its lower-case i in separate runs; cut together, the stem's column
  ran up into the dot, which tapered or vanished at 5-10 mm. stroke-satin-lower-i: IoU 0.81 →
  0.94, mean distance 0.243 → 0.148 mm (ours) and 0.224 → 0.183 mm (PE-DESIGN's), count 121 → 117
  against 128 (PE-DESIGN ties its dot in two runs); stroke-fill-lower-i: count 213 → 201, PE-DESIGN's
  exactly, IoU 0.86 → 0.96. Text under 6 mm travels straight under a stroke still to be sewn
  where the outline walk would go round it (the walk drew boxes round t's bar); from 6 mm up
  the walk stays, since every capture (6 mm and larger) matches it in stitch count. Letters are
  still sewn across up to 2.3 mm (PE-DESIGN's captures trim every letter move, all longer
  than that). The other 226 text captures are unchanged. Since 2026-11-06 (#446, part): X is
  sewn as PE-DESIGN sews it, one diagonal through and the other in two halves meeting it, each
  arm run out to from the crossing under its satin and its level-cut foot sewn corner to
  corner, instead of two chevrons; only a crossing of four arms that all end free takes it (M's
  notch loops and the joins of g and Pacifico keep their chevrons). stroke-satin-X 314 → 269
  needle points against 277 (mean distance 0.267 → 0.182 mm ours, 0.201 → 0.182 mm
  PE-DESIGN's); stroke-fill-X 503 → 470 against 473, in PE-DESIGN's two runs (224 + 249 from
  0-9.6 and 6.8-18.9 mm; ours 220 + 250 from 0-10.4 and 6.5-18.9 mm, main sewed the left and
  right halves), PE → ours 0.214 → 0.220 mm (row phase); text-size-hg 1445 → 1405 against
  1445, its x alone 345 → 305 against 324 and closer both ways (0.228 → 0.211 and 0.215 →
  0.206 mm: main's total matched only because H's +21 and g's −42 cancelled). Letter sweep
  (588 samples): 24 changed (X, x, KNOX), none worse. N's corner and A's and V's slanted feet
  stay open (#446). Round 8 (2026-11-07) added PE-DESIGN references for satin N, M, W, K and Z
  in Lato Bold at 20 and 8 mm and N converted to blocks (`stroke-satin-{n,m,w,k,z}[-8]`,
  `stroke-blocks-satin-n`, all within today's 25 % and 0.3 mm). Measured against them
  (needle points ours / PE-DESIGN, mean distance PE → ours, trims ours / PE): N 283/293,
  0.39 mm, 2/0; N-8 121/123, 0.21 mm, 2/0; M 342/374, 0.38 mm, 2/1; M-8 138/165, 0.27 mm,
  1/0; W 384/363, 0.32 mm, 3/0; W-8 154/155, 0.19 mm; K 314/293, 0.26 mm; K-8 136/128,
  0.14 mm; Z 246/237, 0.24 mm, 1/0; Z-8 95/99, 0.16 mm. PE-DESIGN sews N, W, K and Z in one run
  without a trim (M with one): its strokes meet at the crotches instead of being cut apart, and
  N's and M's PE → ours distances (the spike at the acute corner) are the largest.
- **Test:**
  - Inputs: `text.add {"text":"AVOW","size":30,"x":50,"y":50}` →
    `object.setRegion {"id":1,"type":"satin"}` → `design.generate`.
  - Assert: each diagonal letter has at least 2 dominant stitch-angle clusters, each within ±15°
    of the stroke perpendicular.
  - `object.setRegion` with `angle` on satin text is refused.
- **Fix:**
  - Store direction lines per glyph, in glyph space. Derive them for TrueType glyphs from
    `skeleton.rs`/`column.rs`, and store them in `.osfont`.
  - Transform them with each glyph's placement in `text::layout_in`.
  - Refuse `angle` on satin text.
  - Make Convert to Blocks emit one block per stroke.

## 7. Auto direction is one fixed angle

- **Impact:** severity 4. In PE-DESIGN, Auto rows turn with the shape on curved or bent
  regions. In OpenStitch they stay at one angle.
- **Module:** `openstitch-design/src/region.rs:105-107,222-260`: `auto_angle` returns one
  principal-axis angle.
- **Evidence:** Guide, [region sewing attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html):
  Auto varies the direction to follow the shape. Half Stitch, Stitch Type and Running Stitch
  Path are unavailable with Auto.
- **REA:** none.
- **Grade:** Documented difference.
- **PE-DESIGN 11.50 behaviour** (black-box, 2026-10-27; references `satin-auto-bend-90-nohalf.pes`,
  `satin-auto-bend-60.pes`, `auto-satin-bent-band.pes`): Auto satin starts at the start point, runs
  0.5 mm inside the outline *forward along it* (over a bent band's outer edge, not the shorter way
  round) to the far end, sews back from there with one needle point per rung on alternate edges and
  ends on the start point itself. The rungs lie at whole spacings from the far edge (the 90° band's
  outer edge: 30.0, 29.3, 28.7 … mm), the remainder landing at the start (the 60° band's start leg
  sits 0.05 mm off the lattice). On straight legs the rungs are square to
  the leg; at a bend the outer ends keep an even spacing along the outer edge and the rung
  direction turns linearly with the distance along it, over three band widths for a 90° turn (two
  for 60°), centred on the outer corner; each rung reaches across to the inner edge, so the inner
  ends crowd towards the inner corner. The satin reaches the band's square ends.
- **Bend sweep (2026-10-28, references `satin-auto-bend-30/45/75/90.pes` and their `-nohalf` twins):**
  PE-DESIGN's count is the same at 30°, 45°, 60° and 75° (171–177 stitches on the 5 mm band) and the
  same with Half Stitch on or off; OpenStitch is within 10 % and the longest crossing within 0.5 mm
  at all three (reference tier). **120° (landed 2026-10-30, `satin-auto-bend-120` and `-nohalf`,
  reference tier):** PE-DESIGN sews 161 stitches: square rungs on both legs, a fan of crossings up
  to 9.6 mm (two band widths) round the bend whose inner ends crowd within 2 mm of the inner
  corner, and its needle points 1/3 mm apart along the whole 47.3 mm outer edge. The derived
  skeleton's rungs swap sides round a bend this sharp, so past 100° (`SHARP_BEND`, between the
  90° references the skeleton lattice matches and this one) Auto satin rebuilds its rungs on the
  outer edge (`region::even_outer`: stations every spacing from the far end along the stretch of
  outline that does not pass the reflex corner, each rung square to that edge), and the corner
  fan fans only corners that turn as the inner corner does (not the end cap's, which lie within
  two widths on this band), keeps the rung end nearer the corner, and ends a crossing longer
  than two widths at the inner corner. pescmp: 273 → 171 stitches to PE's 161, mean needle
  distance 0.36 → 0.23 mm, coverage 0.614 → 0.699; the longest crossing 9.7 mm to PE's 9.6.
  Bends up to 90° and the bent band sew exactly as before.
- **Status:** fixed; the corner fan, end reach, forward start travel, the far-end needle lattice
  and the end on the start point follow PE-DESIGN (reference tier: satin-auto-bend-90-nohalf,
  satin-auto-bend-60, satin-auto-bend-30/45/75 (±nohalf), auto-satin-bent-band;
  `region::fan_corners`, `region::reach_ends`, `region::auto_satin`; 2026-10-28: mean needle
  distance 0.20 → 0.08 mm on the 60° band and 0.16 → 0.07 mm on the 90° band, overlap 0.71 → 0.77
  and 0.70 → 0.87; on the 30°/45°/75° bands PE's start-leg lattice sits 0.07-0.27 mm off ours,
  a phase the far end's fan sets that two bends per angle cannot pin down; a density-3 satin's
  overlap turns on such 0.1 mm phases). Earlier: Auto satin
  derives a skeleton and sews along it, each row reaching the outline
  (`skeleton::rungs_within`). Running Stitch Path is hidden with Auto satin, which travels
  inside (item 3). Half Stitch (since 2026-11-04, #372, `halfStitch`, default on): PE-DESIGN's files with it on and off have the same stitches, every other needle point crowded on the inner side of a 90° or 120° bend pulled back along its crossing (120°: points 60, 64 … 88 by 0.8–2.4 mm; 90°: four points about 0.5 mm; 30–75°: none). OpenStitch pulls back every other needle point whose spacing is under 0.3 of the other side's, by 1.4 × (0.3 − that share) of the crossing up to 25 %: at 120° the same points by 1.5–2.4 mm (our fan's inner ends meet at the corner where PE's spread 0.1–0.2 mm, so ours are more crowded), at 90° three points by 0.2–1.1 mm. `satin-auto-bend-120` checks the mean stitch (4.75 mm; PE-DESIGN 4.58 on, 4.75 off). **Jagged regions (2026-11-04, #304):** Auto Punch's PE-compatible storybook picture (1,527 Auto satins over traced outlines) failed sew_check with 23 needle drops in one 0.5 mm spot and 81.4 mm of thread per mm². PE-DESIGN's own Auto Punch of the same picture (round 5, `p03-storybook__autopunch.pes`, 100 × 75 mm) has 22 drops in a spot, 11 stitches under 0.3 mm in a row and 38.8 mm/mm², so its satin is no model here and sew_check's machine limits decide. Three causes, each an OpenStitch rule: a 6 mm blob whose derived skeleton doubled back, so its crossings crossed each other (171 stitches, 68 mm/mm²), now keeps the fixed-direction satin (more than one crossing in ten crossing a neighbour; PE-DESIGN's bends have none); a satin in sections round four holes travelled 20 times through one corner of a hole, and travel needle points now keep 0.5 mm from earlier travels' (moved up to 1.5 mm where both stitches stay inside); and the start travel round a 1.7 mm region dropped the needle on every inset vertex, and now skips points under 0.5 mm. Result: 17 drops in a spot, 46.9 mm/mm², 6 tiny stitches in a row (the limit), compat unchanged. Fill, regions with holes or
  several parts, gradation, feathered edges, split stitches and shapes the skeleton does not
  cover (85 %) keep the principal-axis angle. Test:
  `auto_direction_satin_follows_a_bent_band`.
- **Test:**
  - Inputs: a 90° bent band 5 mm wide → `object.setRegion {type:"satin",autoDirection:true}`.
  - Assert: the stitch angle is within ±10° of perpendicular to the local band axis at 5
    stations along it. The current code fails at the bend.
- **Fix:** For Auto, derive a skeleton (`skeleton::derive`) and sew along it with
  `skeleton::satin`, once item 1 is fixed. Apply the guide's exclusions.

## 8. Underlay model differs; zigzag lines have no underlay

- **Impact:** severity 4. PE-DESIGN's under sewing (Edge only / Single / Double, at Dense /
  Medium / Light) cannot be reproduced:
  - OpenStitch's Double is two crossing ±45° layers.
  - Zigzag outlines have no underlay at all (since 2026-10-27 they have PE-DESIGN's under
    sewing, on by default: `LineSewing::underlay`, `line-zigzag-default`).
- **Settled 2026-10-27/28 from the under sewing sweep (`sweep-satin-us-*`, `sweep-fill-us-*`):**
  PE-DESIGN 11.50's Region sew pane offers two type buttons (the second's icon a double layer
  of crossing rows) and a Density of Dense / Medium / Light; no spacing or inset fields. The
  first type is modelled (`EdgeFill` for fill, `EdgeZigzag` for satin: the edge run 1 mm
  inside, rows 2 mm apart). The second type is modelled since 2026-10-28 as `Cross`
  (`underlayStyle: cross`): the same edge run, then two layers of lines 1 mm inside the
  outline at the sewing direction minus and plus 45°, 2 mm apart (2.83 mm along the edges),
  in 4 mm stitches (a line of exactly 4 mm in two), each layer sewn in two halves from the
  region's far corners towards the start point (the half whose corner comes first along the
  outline from the start point first), straight travel to the corner at 2.5 mm, walks along
  the inset outline at 2 mm, back to the start point at the end; specs sweep-satin-us-type-2
  (395 stitches to PE's 404) and sweep-fill-us-type-2 (2650 to 2651) at the reference tier.
  **Density (2026-10-29, #124):** Dense / Medium / Light are 1.4 / 2.0 / 3.5 mm nominal; PE's
  rows lie on both inset edges with the whole number of gaps nearest to the inset width over the
  nominal (38 mm: 27 gaps of 1.407, 19 of 2.0, 11 of 3.455), modelled as `underlayDensity`
  (`region::inset_rows`; specs sweep-fill-us-density-dense/-light at the reference tier). PE's
  type buttons are exclusive radio buttons (pe-refs-4, round 4: a Satin region enables all five,
  Fill the first row only), so the PE-compatible attribute stays the exclusive `underlayStyle`
  (plus `underlayDensity`); underneath, each style is a set of parts (`underlayParts`: edge,
  zigzag = rows across the direction, lattice, center = piping), and setting the parts
  directly, in any combination, is OpenStitch's own extension with no PE reference.
  OpenStitch's styles from before PE's types were measured (`edge`, `double`, and `zigzag` /
  `edgeZigzag` on fill) keep their original geometry unless parts are set, so saved designs
  sew as before (the badge golden is unchanged). **Manual Punch columns (round-4 captures `satin-col-us-off/-type-1..5`, a
  39.5 × 6 mm straight satin column at 4.5 lines/mm, Medium):** PE sews the column's under
  sewing 0.5 mm inside the rails and the end rungs: the edge run in equal stitches ≤ 2.5 mm
  (39 stitches with the travel from the start point), the piping down the middle (16 × 2.41 mm)
  and a zigzag between the inset rails from the far end back, its stations 38.5 / 19 = 2.03 mm
  apart and every 5.4 mm diagonal split into stitches ≤ 4 mm (42 stitches); the captures show
  edge + piping + zigzag for the first row's two buttons (type 1, the default, and type 2:
  286 stitches, the parts in either order), the edge alone for the second row's outline and
  lines buttons (types 3 and 5: 225) and piping + zigzag for its zigzag button (type 4: 249).
  Modelled in `column::underlay` for a column with several direction lines, the styles mapped
  by `UnderlayStyle::column_parts` (edgeZigzag / double → types 1-2, edge → 3 and 5, zigzag →
  4): 275 / 219 / 239 stitches, IoU ≥ 0.999 against the captures. Specs `satin-col-us-type-1..5`
  and `satin-col-us-off` draw the column with `shape.column` (lines (10,10)-(10,16) and
  (49.5,10)-(49.5,16)), as PE's files are Manual Punch blocks (until 2026-10-29 they drew a
  Rectangle-tool shape, a region with a split satin PE does not sew here); type 5 and the
  plain column, parked by pe-refs-4 at 292 and 207 stitches, landed at 219 / 225 and 186 / 186.
  Still open: which of the two types PE applies to the other region kinds; the zigzag's exact split
  points (PE's follow a 12-diagonal cycle, OpenStitch splits each diagonal evenly).
- **Module:**
  - `openstitch-design/src/model.rs:613-625,822`: the styles and a 2 mm spacing.
  - `openstitch-design/src/region.rs:15,120-156`: satin underlay runs parallel to the top
    stitches, with a 0.5 mm inset.
  - `LineSewing` (`model.rs:928+`) has no underlay field.
- **Evidence:**
  - Guide, [region sewing attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html):
    the types and the context-dependent layers (edge run, zigzag-shaped, piping), with three
    densities.
  - Guide, [line sewing attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293378.html):
    zigzag has under sewing On/Off.
- **REA:** none.
- **Grade:**
  - The option set: documented difference.
  - Spacing and inset: needs reference.
- **Test:**
  - Inputs: a 40×20 mm rectangle in satin and in fill, for each underlay type × density; and a
    3 mm zigzag line with under sewing on.
  - Assert: the pass count and layer type match the guide for the context.
  - Record the spacing and inset against reference files to ±0.1 mm.
- **Fix:**
  - `underlay: {kind: EdgeOnly|Single|Double, density: Dense|Medium|Light}`, mapped per
    context to layers in `region.rs`.
  - `LineSewing.underlay` adds a centre run plus a narrow zigzag before `generate::zigzag`.
  - Calibrate the spacings with method 1.

## 9. PE-DESIGN-made PES files lose objects, properties and colours on re-save

- **Status (2026-11-05, #375, #397, #398, #120): mostly fixed, internal regression over PE-DESIGN's own files.**
  - **Reproduced** on every `#PES0110` file of PE-DESIGN's in compat/reference and the pe11
    fixtures (529 files): File ▸ Open (`stitches.import`) then File ▸ Export ▸ PES, version 6
    or 11, and `import.pes` then version 11. Losses before the fix:
    - Open + export (either version): every editable shape (289 files) and text (233) became
      one stitch pattern; groups (21), hole sewing pairs (9), the programmable fill (7) and
      motif (8) pattern lists gone; the Design Page size became the design's size plus a
      margin (200 × 150, 200 × 200, 300 × 200 pages lost) and a Custom Size page became a
      Hoop Size page in version 11 (387); the thread table was replaced by the PEC chart's
      nearest colours (catalogue numbers, exact RGB, brand and chart lost); unmapped header
      fields gone.
    - `import.pes` + version 11: texts and stitch groups not imported (17 files with shapes and
      texts; 315 with no shapes refused); Spiral, Concentric Circle and Zigzag Net Fill regions
      skipped (9 files); the Design Property dropped (only the name kept); the hole pair of
      hole-sewing-pentagon-star lost; Custom Size pages written as Hoop Size pages (160).
    - 15 files with a `CEmbLine` variant the reader does not parse (tatami-40-*) lose
      everything after it on either path (#398).
  - **Fixed:**
    - File ▸ Open keeps the file with the design (`stitches.import` `keepSource`,
      `Design.source`, kept in `.osd`). `design.export` to PES with no `pesVersion`, or the
      file's own version, writes it back byte for byte while the design is unchanged (a
      fingerprint of the whole design; undo back to the opened state counts as unchanged).
      An edited design is generated from the design and the export warns that the file's
      other content is not kept. `pes_resave::an_unchanged_pe_design_file_is_written_back_byte_for_byte`
      (every PE-DESIGN file above), `the_kept_file_survives_a_project_save_and_another_version_is_converted`,
      `a_changed_design_is_written_from_the_design_with_a_warning`.
    - A `#PES0110` file read as stitches takes each colour block's thread from the file's
      thread table (catalogue number, exact RGB, brand, chart), matched by PEC index and the
      per-block catalogue numbers after the PEC section; BROTHER EMBROIDERY threads show their
      chart names (800 RED) where PE-DESIGN writes the PEC index, and the writer puts the index
      back. `pe_design_thread_tables_reach_the_colour_blocks` (every block with a catalogue
      number, and the RGB on Hoop Size pages, against PE-DESIGN's own list),
      `brother_embroidery_threads_read_with_their_chart_names`.
    - File ▸ Open of a `#PES0110` file uses its Design Page size
      (app `opening_a_pe_design_file_keeps_its_page_and_the_file`).
    - `import.pes` reads Cross Stitch, Concentric Circle, Radial, Spiral, Piping, Stippling,
      Net Fill, Zigzag Net Fill and Flexible Spiral regions (the type and colour; their own
      settings at OpenStitch's defaults, as the writer writes them at PE-DESIGN's) and adopts
      the Design Property into a design that has none
      (`import_pes_reads_the_other_region_types_and_the_design_property`).
  - **Fixed (2026-11-05, `batch:pe-formats`):**
    - #398: the tatami-40 `CEmbLine` "variant" is a moved or resized shape. PE-DESIGN keeps its
      vertices where they were drawn and stores the move as a transform at body +16. The reader
      places the vertices with it before its bounds check, keeps it and writes it back, and
      `import.pes` imports the shape where PE-DESIGN shows it. All 23 tatami-40 files read
      whole, and every `#PES0110` file of PE-DESIGN's now reads with nothing unparsed
      (`moved_and_resized_shapes_keep_their_transform`,
      `moved_and_resized_shapes_import_where_pe_design_shows_them`).
    - #397: `import.pes` imports texts (Lato Bold; Text Transform preset and level, fit to an
      outline, alignment, spacing, line breaks, click point), stitch groups (at their place,
      one thread per colour block), the hole sewing pair, and the programmable, decorative and
      motif patterns (owner patterns only; PE-DESIGN's built-in motif is replaced by
      OpenStitch's default, noted). It no longer refuses files without shapes. All 547
      PE-DESIGN `#PES0110` files import with nothing skipped
      (`import_pes_takes_the_objects_of_every_pe_design_file`). In the editor this is Home ▸
      Import ▸ Import PE-DESIGN Objects.
    - #120: an edited design opened from a `#PES0110` file is exported as `#PES0110` by
      default, not as version 6. Only when version 11 cannot hold an object yet is it written
      as 6, with a warning that names the object (`a_changed_design_is_written_from_the_design_with_a_warning`).
  - **Still lost when a PE-DESIGN file is edited** (documented difference): the other region
    types' own settings, motif size and spacing, per-letter colours and sew types (the first
    letter's are used, noted), the hoop (Design Settings) from the file (item 10), and header
    fields the reader keeps only verbatim. Object kinds the version 11 export still refuses:
    #608.

- **Impact:** severity 4. Opening a user's own PE-DESIGN file and saving it again, even with
  no change, removes:
  - every editable shape and text;
  - Design Property data;
  - hoop and page settings;
  - thread chart identity.
  Files with programmable fills, motifs or feathers also lose their exact RGB colours.
- **Module:**
  - `openstitch-engine/src/commands.rs:2062-2070` and `openstitch-design/src/model.rs:510-521`:
    import keeps only stitches and threads.
  - `openstitch-formats/src/pes.rs:176,181,186,187-191,197,200`: skipped fields, and the thread
    table abandoned on fills, motifs or feathers.
- **Evidence:**
  - Spec: pyembroidery documents and round-trips these fields, and OpenStitch's reader already
    parses most of them.
- **REA:** none.
- **Grade:**
  - Lost metadata: documented difference.
  - Pass-through: needs no field knowledge.
- **Test:**
  - (a) `oracle_v6.pes`, with author and keywords set by `tools/genfixtures.py` → `stitches.import`
    → `design.export pesVersion:6` → `pes::read`. Metadata and threads equal the input, exact.
  - (b) With an owner file: re-export unchanged. The object section is byte-identical.
- **Fix:**
  - Carry metadata and threads on the imported object.
  - Keep the original object-section bytes plus a hash of the stitch stream. If both are
    unchanged at export, write the bytes back and recompute the PEC offset only. Otherwise
    warn and fall back.
  - Since 2026-10-29 the `#PES0110` writer (item 5) writes back everything the reader parses
    (`CEmbLine` shapes, texts, `CEmbOne` stitch groups, thread table with its names, groups, the
    header's page and hoop fields verbatim), so once the import turns the reader's shapes
    into design objects (ROADMAP item 6) a PE-DESIGN file of shapes re-saves with its objects.

## 10. Hoop and Design Page fields are hard-coded in PES

- **Fixed (2026-11-04, #322):** every hoop and page field of a `#PES0110` design is now written
  from the design, checked byte for byte against PE-DESIGN 11.50's files (the field table is on
  #322; the round-6 captures added to `tests/fixtures/pe11/hoops`):
  - **Custom Size page:** the machine type (0 single-needle, 7 multi-needle, with the code `01`
    or `10`) was always multi-needle; a single-needle design now writes PE's single-needle page.
    The two lists after the hoop name, read before as "optional fields", are the page's hoop
    sections, one list per axis: each section after the first, stepping by the Section Size
    less 10 mm from the page's corner, in 0.1 mm from a point where the corner sits at
    100 mm − page / 2 (`pes11::section_starts`; all five of PE's custom-page files, single- and
    multi-needle, 100 × 100 to 300 × 200 mm). They are written from the design's section hoop,
    else PE's for the machine type (90 × 90 of 100 × 100 single-needle, 290 × 190 of 300 × 200
    multi-needle), and the reader parses them (a three-entry list misread the page size).
    `custom_size_pages_are_written_as_pe_design_writes_them`, `section_starts_follow_pe_designs_sections`.
  - **Hoop Size page:** the nv880e hoop fields (mm and 0.1 inch, then the name) are a field of
    `HoopPage` (`nv880e-S`, `M`, `L` as PE writes them; with another hoop id PE keeps the last
    nv880e hoop it showed), and the ids PE gave 180 × 300 (4), 150 × 150 Quilt (0x1A, both
    `02`), 180 × 100 Border (0x21) and ø100 Round (0x16) are documented and tested; 13 hoop files
    in `hoop_size_pages_are_written_as_pe_design_writes_each_hoop`. A single-needle design with
    no hoop on a 100 × 100 page is PE's single-needle 100 × 100 Hoop Size page (it was the
    multi-needle New page).
  - **PEC frame:** a Custom Size page's PEC frame is the design's own size, as on every other
    page since #287, where the 0110 writer still copied PE-DESIGN's section frame (2900 × 1900;
    PE writes 900 × 900 for single-needle). The NV880E refuses a frame larger than its hoop.
  - **Not mapped (no reference to settle them):** the PEC header's 0x26–0x27 (`01 01` in PE's
    single-needle 300 × 200 file only); the grid field (5 in the round-6 files, 10 in the others:
    the 0.5 and 1 mm grids of their sessions; OpenStitch has no grid setting to write); the v6
    header's display fields and the v1 hoop field (no PE-DESIGN v6 or v1 file); OpenStitch's own
    split positions (PE's are its fixed step, written instead); "Use smaller section sizes" and
    pages over 360 mm, whose section positions no PE file shows.
- **Partly fixed (2026-10-29, #125):** `design.export` writes the selected hoop's width and
  height (rotation applied; the custom page when no hoop is chosen) in mm as the v6 header's
  hoop fields, where pyembroidery's constant 100 × 100 stood. PE-DESIGN 11.50 stores the hoop
  the same way in its own `#PES0110` files (u16 100, u16 100, then the hoop name "nv880e-S"
  for a 100 × 100 mm page, in all 163 owner-drawn reference files). Since 2026-10-30 the
  v6 "Design Page Is Custom" field is 1 for a custom page size, "Hoop Rotation" is 1 for a hoop
  turned 90°, and a custom page's size goes in the sixth and seventh u16 (EduTechWiki's
  "DesignWidth" / "DesignHeight"). Checked by opening OpenStitch's exports in PE-DESIGN 11.50
  (reference session, files in the owner's round-5 notes): a 130 × 180 hoop rotated 90°
  (hoop fields 180, 130, rotation 1) opens as Hoop Size 130 × 180 with Rotate 90 Degrees
  ticked; a custom 300 × 200 page written with pyembroidery's constant 200, 200 in the design
  fields opened as a 200 × 200 Custom Size page, which is how the design fields were found.
  Both opened without a warning, with the design centred on the page and the single-needle
  machine type (the v6 header carries neither position nor machine type). The page, grid and
  display fields after them keep pyembroidery's values, and v1 still writes its fixed hoop: the
  public description puts the hoop in the first u16 in one place and the second in another. Over-size designs
  (round-4 capture `hoop-oversize-100.dst`): PE-DESIGN neither rejects a 129 × 105 mm design
  on a 100 × 100 mm page nor promotes it to a larger hoop; Save As warns and saves on Yes,
  Export writes silently. OpenStitch writes it and returns a warning, `fitsPage: false` and
  the smallest Brother hoop it fits (`smallestHoop`); the editor shows the warning. Not
  implemented from #125, against the reference: per-machine colour-change and data-size caps
  that reject an export, and automatic hoop promotion. Also against the issue: the two ASCII
  characters after the header's u16 are "10" in PE's own 0110 files and "01" in files it
  re-saved from OpenStitch exports, all on 100 × 100 mm pages, so they are not the
  "01"/"02"/"10" size code the issue describes; OpenStitch keeps pyembroidery's "02".
- **`#PES0110` machine type and hoop (2026-10-31, #125):** sixteen round-6 captures (one design
  saved on each hoop and machine type, each reopening with the settings it was saved with) map
  them. The header's u16 after the page mode is PE-DESIGN's id of the hoop (single-needle
  100 × 100 0, 130 × 180 1, 160 × 260 2, 180 × 300 4, 200 × 300 6, 150 × 150 Quilt 0x1A,
  240 × 360 0x1C, 100 × 180 0x28; multi-needle 100 × 100 15, ø100 Round 0x16, 180 × 100 Border
  0x21, nv880e 0xFF), a u16 is 1 for Rotate 90 Degrees, another 7 on a multi-needle machine
  and 0 on a single-needle one, and the code after the signature is "01" single-needle ("02"
  for the 150 × 150 Quilt, 180 × 300 and 200 × 300 hoops) and "10" multi-needle: the issue's
  "01"/"02"/"10" codes are the machine type and hoop, not the design size. With Rotate 90 PE
  also turns the PEC's stitches 90° clockwise. The 0110 export now writes a Hoop Size page with
  PE's fields for single-needle 100 × 100, 130 × 180, 160 × 260, 200 × 300, 240 × 360 and
  multi-needle 100 × 100 (each header equal to PE's file,
  `hoop_size_pages_are_written_as_pe_design_writes_each_hoop`; the turned PEC stitch for stitch,
  `a_turned_hoop_turns_the_machine_stitches_as_pe_design_does`), and both PES writers turn the
  PEC for a turned hoop. Left: the other hoops' ids (a Custom Size page of their size until
  then), the 100 × 180 hoop (PE saves it only as `#PES0113`), and the v1 hoop field.

- **Impact (before the fixes):** severity 4. Every v6 file claimed a 100×100 hoop and a fixed
  page, whatever hoop the user chose. Every v1 file claims 130×180. Imported files never set
  the hoop.
  - The guide ([Design Page settings](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/03_1245955.html))
    says the page choice changes the needle start point and what the machine's layout screen
    allows.
- **Module:**
  - `openstitch-formats/src/pes.rs:370-371,430,435-443,490-493`: constants.
  - `openstitch-design/src/machine.rs:241-250`: the hoop is known but never reaches the writer.
  - `pes.rs:176`: the reader skips these fields.
- **Evidence:** Spec: EduTechWiki names the v6 header fields in order. Their units and
  indicator codes are not given.
- **REA:** none.
- **Grade:** Reference since 2026-11-04 (#322) for `#PES0110`: owner files (method 2) settle
  the page, hoop, machine type and section encodings. v6 and v1 stay documented, unverified.
- **Test:**
  - Inputs: `machine.set` with a 130×180 hoop, then a custom 300×200 hoop, then `rotate90`;
    50 mm shape → `design.export pesVersion:6`.
  - Assert: the header fields at their EduTechWiki positions hold the chosen hoop and the flags,
    and the reader round-trips them.
- **Fix:**
  - Add `PesPage {hoop, rotated, custom, page, sewing_area, offset}` to `WriteOptions`, filled
    from `Design.machine`/`Design.page`. Parse the same fields on read.
  - Keep pyembroidery's constants as the fallback until owner files confirm the encodings.

## 11. No Connection Points (Default / Closest / Farther) for text

- **Impact:** severity 4. Users can't choose short joins between letters (fewer trims) or long
  ones (easy to trim by hand). Each letter's start and end is left to the fill generator.
- **Module:**
  - `openstitch-design/src/compile.rs:64-84,263-289`: glyph regions first, then all outlines;
    travel by distance only.
  - `openstitch-design/src/route.rs:217-246`: entry and exit are object-wide.
- **Evidence:** Guide, [connection points](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1817518.html):
  three settings. They are not available for TrueType, user-defined or small-text fonts,
  multi-colour text, or text whose characters are not sewn in a continuous order.
- **REA:** none.
- **Grade:** Documented difference. The exact join geometry needs a reference. Open: #377.
- **Test:**
  - Inputs: an embroidery font (`fonts.convertTrueType`) with "AVA" at 20 mm, for each
    `connection` value.
  - Assert: total join length closest ≤ default ≤ farther, and trims(closest) ≤ trims(default).
  - The setting is refused for TrueType and multi-colour text.
- **Fix:**
  - Add `connection: Default|Closest|Farther` on text. In `compile::elements`, pick each
    glyph's entry and exit on its outline nearest to, or farthest from, its neighbours.
  - Sew each glyph's region and then its outline, unless a reference shows otherwise.

## 12. No minimum font sizes or small-text fonts

- **Status (2026-11-06, #527, #530):** largely done. `fonts.list` reports each bundled font's
  `sizeRange` and `group` (text / smallText); `text.add` and `object.setText` return a
  `sizeWarning` outside it (a warning, as OpenStitch never refuses a size); OpenStitch Line is a
  small-text single-line font sewn as a running line with trims only for dots and letters more
  than 2 mm apart. Compared with PE-DESIGN's recorded Small Text numbers since 2026-11-07 (#530,
  specs `text-small-running-3mm`, `text-small-satin-5mm`; section below). Left: the attributes
  PE-DESIGN disables for small text (OpenStitch removes the region instead).

- **Impact:** severity 4.
  - Text of any size from 1 to 300 mm is accepted without a warning.
  - 3–6 mm text is sewn as an ordinary fill, which puckers and fills in.
  - PE-DESIGN gives each font a minimum size and has a separate small-text class, with no trims
    between characters and restricted attributes.
- **Module:**
  - `openstitch-design/src/text.rs:23-52`: the font list.
  - `openstitch-engine/src/commands.rs:1564,1610,1658`: the 1–300 mm clamp.
- **Evidence:** Guide:
  - [font list](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1169793.html):
    a minimum size per font;
  - [small-text list](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1171127.html):
    3–6 mm recommended;
  - [Small Text](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1811349.html):
    the disabled attributes.
- **REA:** none. Brother's fonts and size tables must not be copied; take minimum sizes from
  testing our own OFL fonts.
- **Grade:** Documented difference.
- **Test:**
  - `fonts.list` reports `minSize` per font.
  - `text.add {"size":4}` below the minimum returns a warning.
  - With a small-text font, `object.setRegion` and `text.setNameDrop` are refused, and there are
    no trims between characters.
- **Fix:**
  - Add `min_size`/`recommended_size` and a `small_text` font class to `fonts`/`.osfont`.
  - Enforce them in the engine, and turn jump trimming off between small-text glyphs.
  - Add an OFL or OpenStitch-made small-text run-stitch font.

## 13. No per-character colour or multi-character selection

- **Impact:** severity 4.
  - Multi-colour names and initials are a core workflow, and letters can't be coloured or sewn
    differently.
  - Point Edit selects one character at a time.
- **Module:**
  - `openstitch-design/src/text.rs:141-158`: `CharEdit` holds x, y, rotation and size only.
  - `openstitch-design/src/compile.rs:278-281`: one thread per object.
  - `openstitch-engine/src/commands.rs:1641-1661`: a contiguous `index..index+count` range.
  - `openstitch-app/src/editor/tools/characters.rs:216`: single selection.
- **Evidence:** Guide, [editing individual characters](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1904502.html):
  box, Ctrl-click or Shift+arrow multi-select; thread colour and sewing attributes per character.
- **REA:** none.
- **Grade:** Documented difference. Whether PE-DESIGN groups same-colour letters in the sewing
  order needs a reference.
- **Test:**
  - Inputs: `text.add {"text":"ABC","size":20,"fill":"Black"}` →
    `text.setCharacter {"id":1,"indices":[0,2],"fill":"Red"}` → `design.generate`.
  - Assert: thread order Red, Black, Red with 2 colour changes; `rotation:20` with indices
    `[0,2]` turns only A and C.
  - App test: dragging a box over A and B selects both.
- **Fix:**
  - Add per-character `region_thread`/`line_thread` to `CharEdit` and per-glyph threads in
    `compile::elements`.
  - Accept `indices` in `text.setCharacter`.
  - Make the selection in `characters.rs` a set, with box, Ctrl-click and arrow keys.

## 14. Programmable fill defaults to 0° instead of 45°

- **Impact:** severity 3. New programmable fills tile horizontally, while PE-DESIGN's default
  is 45°.
- **Module:**
  - `openstitch-design/src/model.rs:764`: `angle: 0.0` for every type.
  - `openstitch-engine/src/commands.rs:1048-1050`: a type change keeps the old angle.
- **Evidence:** Guide, [region sewing attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html):
  the programmable fill example is captioned "45° (default)".
- **REA:** none.
- **Grade:** Documented difference.
- **Status:** fixed for new regions, internal regression tier only (not reference verified).
  `RegionSewing::new` gives programmable fill 45° and every other type 0°; `object.setRegion`
  on an object without a region starts from the requested type's defaults. Not changed: a
  type change keeps the old angle (a default-filled rectangle switched to programmable fill
  stays at 0°); whether PE-DESIGN does the same needs a reference. Test:
  `programmable_fill_defaults_to_45_degrees` (an outline-only 40×40 mm rectangle).
  Reference verified (2026-10-27): PE-DESIGN 11.50's default programmable fill
  (compat/reference/stitches/programmable-fill-45.pes) has its dominant direction at 135° in
  the file (y down), because PE-DESIGN's angles turn counter-clockwise on screen. Since `.osd`
  version 2 OpenStitch stores every direction angle that way too (version-1 files are
  migrated on load), so the stored 45° sews at 135° in the file, as PE's does.
  **Fill Stitch too (2026-10-30):** PE-DESIGN 11.50's shape tools start Fill Stitch at 45° as
  well (pes-objects r01, a fresh design: "Fill … 45 deg"; r08 had to set 90 to differ), so new
  fill regions now start at 45° (`RegionSewing::default_angle`), and an outline change resets
  the direction to its type's default instead of 0°. Saved designs are unaffected: every
  `.osd` region stores its angle (a region without one does not load), so no migration is
  needed; `new_fills_start_at_45_degrees_and_saved_designs_keep_their_angle` pins both. The one
  spec that relied on the old default, hole-sewing-pentagon-star (PE-DESIGN made it at 0°),
  now sets its angle.
- **Test:** `shape.rectangle 40×40` → `object.setRegion {type:"programmableFill"}`. Assert the
  stored angle is 45 and the dominant row direction in the file is 135° ±1° (compat
  programmable-fill-45, reference tier).
- **Fix:**
  - Give each sew type its own default angle in `RegionSewing::new`.
  - Whether a type change keeps the angle needs a reference; leave it as is until then.

## 15. Missing fill, satin and zigzag attributes; wrong ranges

- **Impact:** severity 3. These PE-DESIGN settings can't be set or reproduced:
  - Fill Frequency (fixed ⅓ stagger) — fixed; since 2026-10-29 (#123) the regular Frequency
    brick is one of two stagger strategies, the other OpenStitch's seeded random stagger
    (`stagger: random`, `staggerSeed`), which has no PE-DESIGN counterpart: PE's Fill pane
    offers the Frequency field only (every round-2 sweep's MANIFEST settings list its fields);
  - Stitch Type (turning ends);
  - Half Stitch;
  - zigzag Sharp Corners / Miter limit — fixed 2026-11-07 (#373, reference).
  Two ranges are also too wide: Frequency accepts 0–100 % and E/V run times accept up to 6.
- **Module:**
  - `openstitch-design/src/generate.rs:143` and `region.rs:279`: `phase = (row % 3)/3`.
  - `openstitch-engine/src/commands.rs:1140`: 0–100.
  - `commands.rs:1294-1306`: run times up to 6 for E/V.
- **Evidence:** Guide:
  - [region attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html):
    Frequency 0–50 %, Stitch Type, Half Stitch.
  - [line attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293378.html):
    Miter limit; run times 1–5; E/V stroke times 1–3; Chain Time 1–6.
- **REA:** none.
- **Grade:**
  - Missing options and ranges: documented difference.
  - Turning-end and half-stitch geometry: needs reference.
- **Status:** partly fixed, internal regression tier only (not reference verified).
  - Fill `frequency` (0–50 %, the needle shift from row to row) feeds `generate::row_phase`
    in plain and shaped fill. New fills take PE-DESIGN's default, 30 % (sweep references: the
    Frequency field shows 30 and rows shift 1.2 mm per 4 mm stitch); a file without a Frequency
    (saved before) keeps the ⅓ stagger, so existing designs sew as before.
  - `object.setRegion` refuses Frequency outside 0–50 (all types that use it);
    `object.setLine` limits run times to 1–5 for E/V and every other type, 1–6 for chain.
  - Fill and satin Density take PE-DESIGN 11.50's 0.1–14.0 lines/mm (was 0.5–10), and Pull
    compensation stays 0–2.0 mm, as PE's fields clamp (sweep references, 2026-10-27). Issue
    #139 (2026-10-29) asked for a 0.2 floor; the references contradict it: the MANIFEST entries
    for `sweep-fill-density-min.pes` and `sweep-satin-density-min.pes` record "typing 0 shows
    0,1 (min 0.1)", both files sew at 0.1 lines/mm and pass at the reference tier, and the
    "0,15 rounds to 0,2" note of `sweep-satin-density-0p2` is the field's one-decimal rounding.
    The floor stays 0.1; `attribute-ranges` now checks 0.1 and 14.0 accepted, 0.09 and 14.1
    refused, for fill and satin (0.4 refused for the 0.5–10 types). OpenStitch accepts finer
    values than PE's one-decimal field (0.15 is sewn as 0.15, not rounded to 0.2). The
    `sweep-fill-density-*` and `sweep-fill-pull-*` specs check rows 1/density apart from edge to
    edge and each row end moved out by the pull compensation (the under sewing is not), against
    PE-DESIGN's files.
  - Fill row structure as PE-DESIGN sews it (sweep references, stitch-sweeps track): the rows
    on each side of the start point are sewn as two halves from the far edge inward, the first
    row out and back when a half has an odd number of rows, rows sewn backwards without edge
    points (diagonal joins), straight travel in stitches up to 2 mm, a 0.3 mm tie-in and lock,
    the spacing stretched so the last row lies on the far edge, and the needle grid counted from
    the first row's start. Convex fills match PE-DESIGN's files needle point for needle point
    (`sweep-fill-*` specs, `same-stitches`); regions with several spans per row (U, holes) keep
    the older nearest-row order with PE's row ends and grid.
  - Tests: `frequency_and_run_times_follow_the_guide_ranges`,
    `fill_frequency_sets_the_row_to_row_needle_shift` (0, 25 and 50 %, ±2 % of the stitch).
  - Half Stitch for Auto satin: done 2026-11-04 (#372, item 7's bend sweep).
  - Zigzag Sharp Corners and Miter Limit: done 2026-11-07 (#373) from PE-DESIGN 11.50's
    captures (crates/openstitch-formats/tests/fixtures/pe11/captures-2026-11-07/miter/, copied
    to compat/reference/stitches/line-zigzag-miter-*.pes). The field runs 0–50 mm and each of
    the two buttons keeps its own value (0.0 and 7.0). A corner sharper than 90° has its point
    cut square to the bisector the Miter Limit from the corner; 90° corners are unchanged.
    Button 1 ends each leg where the legs' rails cross and fills the point as a wedge after a
    run out to the cut (stitches of at most 2 mm); button 2 sews the first leg square into the
    cut and the second from it along the bisector. Specs `line-zigzag-miter-v-b1-m0/m1/m3/m50`,
    `-v-b2-m0/m3/m50` and `-l-b2-m7` reach the reference tier: widths exact (40.3/40.2,
    41.2, 43.0, 43.8 mm), stitch counts within 1 % (447/446, 453/455, 462/465, 466/467,
    450/451, 478/481, 485/481, 305/311).
  - Zigzag corner angle and the PES 0110 attribute: done 2026-11-08 (#373 part 2) from the
    coordinator's black-box captures of V lines at 45.0°, 59.9°, 75.3° (both buttons) and
    120.1° (button 1), Miter Limit 0, copied to compat/reference/stitches/
    line-zigzag-miter-v45/v60/v75/v120-*.pes. PE-DESIGN's two buttons sew 45° and 59.9°
    differently and 75.3° identically, with the full mitre (41.3 mm, 299 stitches), so the
    cut starts between 60° and 75°; OpenStitch now cuts below 67.5° (was 90°, which cut the
    75° V to 40.8 mm). Seven specs reach the reference tier: widths exact except 45° button 1
    (40.4 against 40.2), stitch counts within 3 % (377/381, 384/388, 337/342, 349/353,
    291/299, 291/299, 335/340). Left out of the specs and recorded in them: 45° button 1's
    width, 45° button 2's longest stitch (2.60 against 2.92 mm, the fan) and 75.3°'s longest
    stitch (3.55 against 3.86 mm, 0.01 over the tolerance). PES 0110 CEmbLine body +62 holds
    the button (0 or 1), +64 and +66 each button's limit in 0.1 mm (the 75.3° pair differs
    only at +62); OpenStitch now reads, imports, exports and writes them instead of PE's
    defaults (docs/FILE_FORMATS.md). Not known: the exact angle (a 67° capture would narrow
    it), button 2's exact fan, satin lines.
  - **Stitch Type (measured 2026-10-30, round-5 captures `tatami-40-st{1,2}-dir{0,30,90}.pes`,
    `tatami-40-st2-dir45-freq0.pes`, with round 4's 45° pair; a 40 × 40 mm Fill square, density
    4.5, step 4.0, Frequency 30 %, under sewing off):**
    - *Type 1* (the default) is OpenStitch's fill. At 90° it is identical, phase for phase. At
      0°, 30° and 45° the rows step 1.2 mm each, as OpenStitch's, with a constant offset of at
      most 0.3 mm (the angled-fill anchor difference below). Stitches: 2006 (0°), 2032 (30°),
      2068 (45°), 2036 (90°).
    - *Type 2* sews sloped rows. Each pass runs from one edge at row k's line to the other edge
      at row k+1's, rising one row spacing over its length (0.20 mm at 0° and 90°, 0.21 at 45°,
      0.22 at 30°, against 0 for Type 1's flat rows), so consecutive passes meet at the edges in
      one continuous zigzag, with no step at the row ends. The needle grid along each pass follows
      the same Frequency rule as Type 1 (at 0°: 1.2 + 4k, then 2.4 + 4k, then 3.6 + 4k mm from the
      left edge), taken from the row the pass ends on: k · 30 % · 4 mm for row k in both halves,
      so the half sewn downward starts two steps back from what its first row would give
      (tatami-40-st2-dir90). Stitches: 2001 (0°), 2041 (30°), 2054 (45°), 1989 (90°), 2056 (45°,
      Frequency 0).
    - Implemented (2026-10-30, `stitchType: 2` on `object.setRegion`, saved only when 2): the
      fill sews each half's passes from row i's edge to row i + 1's far edge, the grid of the
      row it ends on. Against PE-DESIGN's Type 2 (pescmp, ours / PE stitches, needle-to-nearest
      mean, coverage IoU): 0° 1999 / 2001, 0.001 mm, 1.000 (identical up to the file's 0.1 mm
      rounding); 90° 1986 / 1989, 0.048 mm, 0.969; 45° 2078 / 2054, 0.305 mm, 0.976; 30°
      2042 / 2041, 0.318 mm, 0.849; 45° Frequency 0 2071 / 2056, 0.104 mm, 0.952. The pass
      slopes match at every angle (median rise 0.200 / 0.226 / 0.212 / 0.200 mm at
      0° / 30° / 45° / 90°, PE 0.200 / 0.223 / 0.212 / 0.200). Left: at 30° and 45° PE's rows
      sit about half a spacing from ours (row lattice offset 0.9 against 0.4 rows at 30°, 0.9
      against 0.3 at 45°), the angled-fill anchor difference Type 1 has too (Type 1 at 30°:
      0.205 mm, 0.967); in Type 2 it also moves each pass's grid. Specs: the seven round-5
      files and `tatami-40-stitch-type-2`, which now sews Type 2. The inspector's Fill panel
      offers it as "Stitch type" (1: flat rows, 2: sloped rows; 2026-10-31).
    - Anchor probe (2026-10-31, not landed): OpenStitch stretches angled rows to fill the
      rotated height corner to corner (245 gaps of 0.2230 mm at 30°); PE-DESIGN keeps the
      1/density spacing. Without the stretch the 45° counts match: Type 1 2067 / 2068 and
      Type 2 2053 / 2054 (stretched: 2059, 2078); Type 2 at 30° goes 0.318 → 0.264 mm, IoU
      0.849 → 0.887. But no single row offset (0 to 0.95 rows, with or without the stretch)
      improves both types at both angles: the needle score jumps whenever the tiny corner row
      appears or disappears, because the first row sets the needle-grid anchor, and Type 1 and
      Type 2 move in opposite directions (offset 0.6: Type 1 at 30° 0.189 → 0.301 mm, Type 2
      0.344 → 0.213). The row lattice and the grid anchor have to be fitted together.
    - Round 6 (2026-10-31, 14 captures, specs `tatami-40-pos0-*`, `tatami-40-pos2-*`,
      `tatami-40x20-*`, `tatami-40-pos-center-*`, `tatami-40-pos-right10-*`: the square at
      top-left (0, 0), (13.5, 7.5), (30, 30) and (35, 30) mm and a 40 × 20 mm rectangle at
      (10, 10), Stitch Types 1 and 2 at 30° and 45°; page positions read from the PES header):
      - **Rows sit on the shape, not the page.** Measured from the shape's lowest corner the row
        lattice is the same at every position and for the rectangle (0.89–0.90 rows at 30°);
        measured from the page origin it moves (0.37–0.90). The offset matches rows counted from
        the far side at the exact 1/density spacing (30° square: frac(54.64 / 0.2222) = 0.885;
        rectangle 0.95). At 45° the measure is unreliable (a 45° line through the file's
        0.1 mm grid aliases; fit 0.53–0.57).
      - **PE-DESIGN's stitch count moves with the page position; ours does not.** 40 mm
        square, Type 1 at 30°: 2042 at (0, 0), 2039 at (13.5, 7.5), 2032 at (10.2, 10.0); at
        45°: 2065, 2067, 2068, 2068, 2068. OpenStitch sews 2048 and 2059 everywhere. The
        needle-grid phase along the rows (Type 1, u − k · 1.2 mm mod 4, from the top-left
        corner) is 0.90 mm at 0°, 2.81–2.98 at 30°, 1.05–1.28 at 45° and 1.20 at 90°;
        OpenStitch uses 1.2 at every angle (exact at 90°; the 0.269 mm at 0° is this 0.3 mm).
      - **Candidate rules scored** over all 22 captures (rounds 4–6) at PE's page positions,
        mean over the 16 angled cases (|Δ count|, needle, IoU): today 9.4, 0.298 mm, 0.949;
        exact spacing with rows from the far side 11.8, 0.303, 0.961; that plus the grid
        anchored on the first sewn row 14.2, 0.307, 0.956. Neither lands: both lift coverage
        but move the 45° Type 2 counts away (2078 → 2084 against 2054). The per-angle grid
        constant and PE-DESIGN's position dependence (likely its page-coordinate rounding of
        the anchor) are what remain to model.
- **Test:**
  - `object.setRegion {type:"fill",frequency:60}` and `object.setLine {type:"eStitch",runTimes:6}`
    are refused.
  - Fill a 40×40 square at frequency 0, 25 and 50. The row-to-row needle shift equals the
    frequency ±2 % of the stitch length.
- **Fix:**
  - Add `frequency` to Fill, fed into `tatami_points`.
  - Clamp the ranges.
  - Add `stitch_type`, `half_stitch` and `miter_limit`, with geometry from method 1 (`miter_limit`
    done as `sharpCorners`, #373).

## 16. Optimize Sewing Order does not move entry and exit points

- **Impact:** severity 3. After optimising, travel between same-coloured objects is longer than
  in PE-DESIGN, giving more and longer jumps and trims.
- **Module:** `openstitch-engine/src/order.rs:168,175-224`: nearest object by bounding-box
  centre; `entry`/`exit` are never changed. (The nearest-object sort was itself a difference:
  see the PE-DESIGN behaviour below.)
- **Evidence:** Guide:
  - [optimise order](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_1827935.html):
    rearranges the order **and** entry/exit points so neighbours connect at their closest
    points;
  - [options](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/15_1162908.html):
    the "Optimize Entry/Exit Points" setting.
- **REA:** none.
- **Grade:** Documented difference.
- **Test:**
  - Inputs: three same-colour 10 mm squares at x = 0, 50, 25, in that drawing order, under
    sewing off → `order.optimize` → `design.generate`.
  - Assert (corrected 2026-10-27 after the PE-DESIGN reference): the order stays 0, 50, 25, and
    each jump equals the gap between outlines in that order (40 and 15 mm ±0.5). The first
    version of this item assumed the optimiser also sorts same-colour objects left to right
    (0, 25, 50); PE-DESIGN 11.50 does not.
- **Fix:**
  - After reordering, set each entry to the outline point nearest the previous exit and each
    exit to the point nearest the next entry.
  - Add the persistent option, applied when shapes are created.
- **PE-DESIGN 11.50 behaviour** (black-box, 2026-10-27; reference
  `compat/reference/stitches/optimize-entry-exit.pes` and its MANIFEST notes):
  - Optimize Sewing Order groups colours but keeps drawing order within a colour. It does not
    sort same-colour objects by distance. Red 0, blue 50, red 25 becomes red 0, red 25, blue 50.
  - It moves the exit of each object and the entry of the next to the facing points, but only
    between neighbours of the same colour. Across a colour change it leaves the default points.
  - Options ▸ Optimize Entry/Exit Points (on in the reference install) does the same linking
    as each shape is drawn; clicking Optimize Sewing Order with it off gives the same stitches.
  - With under sewing on (its default), each move is about 1.3 mm longer, because the under
    sewing starts 1 mm inside the outline. The spec turns under sewing off to keep that out.
- **Status:** fixed at the reference tier (spec `optimize-entry-exit`). `order.optimize` keeps
  drawing order within a colour and sets each exit and the next same-colour entry to the
  closest points of the two outlines; Options ▸ Optimize Entry/Exit Points (`options.set {optimizeEntryExit}`, user
  settings, default off — the PE-DESIGN default is unverified) links each new shape the same
  way. Tests in `openstitch-engine/tests/order_holes.rs`:
  `optimize_moves_entry_and_exit_to_the_closest_points`,
  `optimize_entry_exit_option_persists_and_applies_to_new_shapes`,
  `optimize_keeps_colour_groups_and_overlap_order`. The squares keep their order 0, 50, 25, and
  the jumps measure 40.0 and 15.0 mm, as in PE-DESIGN (50.0 and 25.0 without linking).
- **Issue #137 (2026-10-29):** the issue asked for a 2-opt minimum-jump order in place of the
  greedy chaining. PE-DESIGN 11.50 does neither: every capture (`optimize-entry-exit`,
  `machine-rbr`, `machine-rbr-far`, `machine-rbgb`) shows colours grouped in order of first
  use and the drawing order kept within a colour (0, 50, 25 stays 0, 50, 25 with 40 and
  15 mm jumps, where 0, 25, 50 would jump 15 and 15), so the default mode stays PE-DESIGN's
  and the specs stay green. The 2-opt optimiser is an explicit option beyond PE-DESIGN:
  `order.optimize {mode: minJump}` or the design's `optimizeOrder` setting
  (`design.setProperties`, saved in `.osd`; editor: the mode box beside Optimise). It keeps
  PE-DESIGN's colour blocks and overlap order, arranges each block by a greedy nearest-free
  walk from the previous block's last object and then 2-opt reversals of the open path until
  none shortens the outline-to-outline travel (never longer than the greedy order by
  construction), and links the entry and exit points of every neighbouring pair, colour
  changes included. Tests: `openstitch-engine/tests/order_min_jump.rs`. pe-refs-4's capture
  for the issue (round 4, `order/six-before.pes` and `six-after.pes`: six scattered squares in
  three colours before and after Optimize Sewing Order) has an identical stitch stream in both
  files, only four header bytes differ: PE-DESIGN reordered nothing and moved no points, as
  this item predicts.

## 17. Hole sewing is not validated or paired

- **Impact:** severity 3. OpenStitch accepts hole sewing that PE-DESIGN refuses, and cuts every
  later overlapping region out of the first object. That behaves like Remove Overlap, so
  coverage differs.
- **Module:**
  - `openstitch-engine/src/commands.rs:1341-1355`: only checks that a region exists.
  - `openstitch-design/src/compile.rs:249-261`: `holes_above` subtracts every later closed
    region.
- **Evidence:** Guide, [hole sewing](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_1836721.html):
  - applies only to a pair where one region fully encloses the other;
  - refused for self-crossing outlines or multiple direction lines;
  - feathered edges are unavailable with it
    ([attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293518.html)).
- **REA:** none.
- **Grade:** Documented difference.
- **Test:**
  - A 40 mm pentagon with a 20 mm star inside → `object.setHoleSewing {ids:[1,2],enabled:true}`.
    The stitched area equals pentagon minus star (±2 %).
  - A partial overlap, a self-crossing outline, or ≥2 direction lines are refused.
  - `featherWidth` on a hole-sewn object is refused.
- **Fix:**
  - Store the inner object's id on the outer object.
  - Validate full enclosure with `boolean::combine`, and check for self-crossing and the
    direction-line count.
  - Subtract only the paired shape.
- **Status:** fixed at the internal-regression tier (not verified against PE-DESIGN output).
  The outer object stores `holeInner`; `.osd` files saved before pair with the first enclosed
  region sewn after. Tests in `openstitch-engine/tests/order_holes.rs`:
  `hole_sewing_cuts_out_only_the_paired_shape` (measured 829.1 mm² vs pentagon − star
  833.5 mm²), `designs_saved_before_pairs_cut_out_the_enclosed_region_only`,
  `hole_sewing_is_refused_outside_the_guide_limits`.

## 18. PES thread table drops chart names and colour type

- **Update (2026-11-04, #375): chart strings for OpenStitch's PEC chart colours.** PE-DESIGN
  11.50's own Ver.6.0 file (pe11 fixture `r01-rect-v6.pes`) writes a Brother chart colour as it
  does in `#PES0110`: the BROTHER EMBROIDERY number (`005`), the PEC index where the name goes
  (`16`), brand `EMBROIDERY`, type `0x0A`, empty chart. The version 6 writer now writes a
  thread that is exactly a PEC chart entry this way (`pes11::pec_chart_thread_as_pe_writes`;
  other threads as given, so the pyembroidery byte-identical round trips stay), and the reader
  shows such a thread with its chart name (`800` `RED`). Test
  `pec_chart_colours_are_written_in_version_6_as_pe_design_writes_them`; specs
  `pes-default-v6` and `pes-0110-editable-shapes` expect these strings (internal regression),
  and `thread-match-swatches` now compares PE-DESIGN's own thread strings (800, 513, …) with
  OpenStitch's export.

- **Status (2026-10-19): fixed for round trips, internal regression.** `Thread::chart` and
  `Thread::kind` are read and written back; OpenStitch's own colours still get type `0x0A` and
  an empty chart. `pes_v6_thread_chart_and_kind_round_trip_byte_identically` rewrites
  `oracle_v6_chart.pes` (pyembroidery) byte for byte. Still open: the meaning of the type codes
  (chart strings for OpenStitch's colours since 2026-11-04, update above).

- **Impact:** severity 3.
  - Every thread is written as custom colour (`0x0A`) with an empty chart.
  - Re-saving a PE-DESIGN file erases its brand charts.
  - Appliqué markers may not be recognised.
- **Module:**
  - `openstitch-formats/src/pes.rs:195-206` (read: type and chart discarded) and
    `pes.rs:451-458` (write: `0x0A`, empty chart).
  - `openstitch-core/src/lib.rs:51-61`: `Thread` has no chart or type fields.
- **Evidence:**
  - Spec: pyembroidery reads and writes `thread.chart`.
  - EduTechWiki lists type codes (1 list colour, 2 appliqué, A custom, B Brother), with one
    inconsistency.
- **REA:** none.
- **Grade:**
  - Chart string: documented difference.
  - Type codes: unverified.
  - Expected chart strings: needs owner files.
- **Test:** `tools/genfixtures.py` writes `oracle_v6_chart.pes` with chart, brand, catalogue
  and description set. `pes::read` → `write_with(V6)` must be byte-identical to the input.
  Today it fails at the chart byte.
- **Fix:**
  - Add `chart` and `kind` to `Thread` (serde-optional), and read and write them.
  - Default `kind` to `0x0A`.
  - Set chart strings from OpenStitch thread charts only after owner files confirm them.

## 19. Design Property metadata cannot be set; lost on import

- **Status (2026-10-19): fixed, internal regression.** `Design.properties`,
  `design.setProperties` (≤ 255 bytes per field, no control characters except line breaks and
  tabs), compiled into `StitchPattern.metadata`, kept in `.osd`, offered and adopted on
  `stitches.import` (`adoptProperties`), editable in Application menu ▸ Design Property.
  Tests: `pes_compat::design_properties_*`, app `design_property_edits_apply_as_one_undo_step`.

- **Impact:** severity 3.
  - PE-DESIGN users fill in category, author, keywords and comments, which Design Database
    searches.
  - OpenStitch writes only the name, and drops the other four on import.
- **Module:**
  - `openstitch-design/src/model.rs:15-40`: no properties on `Design`.
  - `openstitch-design/src/compile.rs:139`: only the name reaches the writer.
  - `openstitch-formats/src/pes.rs:212-219,432-434`: read and write already exist.
- **Evidence:**
  - Guide, [Design Property](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_1911437.html).
  - Spec: the field order in pyembroidery and EduTechWiki.
- **REA:** none.
- **Grade:** Documented difference.
- **Test:**
  - `design.new {name:"Rose"}` → `design.setProperties {category,author,keywords,comments}` →
    shape → `design.export pesVersion:6` → `pes::read_objects`. All five strings are present,
    byte-exact.
  - The properties survive an `.osd` round trip and are restored on import.
- **Fix:** Add `Design.properties` (serde-default), a `design.setProperties` command, a Design
  Property dialog, and copy the properties into `StitchPattern.metadata` in
  `Compiler::compile`.

## 20. Character kerning moves one glyph without reflowing the line

- **Impact:** severity 3.
  - In PE-DESIGN, dragging a character sideways sets its kerning in mm, a spacing change.
  - In OpenStitch it shifts only that glyph, so letters can overlap and centring doesn't
    update.
  - The text-wide `kerning` flag is a different setting: it turns the font's kerning table on
    or off.
- **Module:**
  - `openstitch-design/src/text.rs:836-883`: `Run::along` applies `e.x` after the pen advance.
  - `text.rs:1068-1072`: alignment.
  - `text.rs:102-105`: the flag.
- **Evidence:** Guide, [text attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1810676.html)
  and [character size and attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1844109.html):
  per-character kerning in mm, default 0, set by a horizontal drag.
- **REA:** none.
- **Grade:** Documented, unverified. Whether later characters reflow needs a reference.
- **Test:**
  - Inputs: `text.add {"text":"ABC","size":20,"align":"left"}` →
    `text.setCharacter {"id":1,"index":0,"kerning":5}`.
  - Assert: B and C shift +5.0 mm and the width grows 5.0 mm (±0.05).
  - With `align:center`, everything shifts −2.5 mm.
- **Fix:**
  - Add `CharEdit.kerning` (mm), added to the pen advance so it counts in the run length.
  - The editor's horizontal drag writes `kerning`.
  - Rename the text flag in the UI to "Use font kerning".

---

## Next 11 (lower impact or reference-bound)

- **Uncalibrated numeric defaults:** cross-cutting, high impact, but needs method 1.
  - Values: satin 3.0 rows/mm, stitch length 4 mm, pull compensation 0, underlay spacing 2 mm,
    line pitch 2.5 mm (motif lines still open; stem, candlewick and chain calibrated below).
  - Lines calibrated from PE-DESIGN 11.50 reference files (2026-10-27, `line-*-default` specs):
    running and triple 2.0 mm, zigzag 2.0 mm wide at 4.5 lines/mm with under sewing and half
    stitch on, E/V strokes 2.5 mm every 5.0 mm at 2.0 mm run pitch. Switching the line type
    gives the new type's defaults, as PE-DESIGN does. Lines tie in and lock like a satin. PE's
    zigzag is sewn between mitred rails, matched along each leg. Its V puts a V at every
    multiple of the spacing, with arms to ±1/5 of it.
  - Then stem, candlewicking and chain (`line-stem-default`, `line-candlewick-default`,
    `line-chain-default`, each matching PE's needle points):
    - Stem: 2.0 mm wide, stations every 1.5 mm per leg, 45°, each sewn top, bottom, top,
      bottom 0.14 mm apart.
    - Candlewicking: 5.0 mm knots 1.0 mm apart per leg; a Medium knot is a {24/11} star.
    - Chain: 4.0 mm triangles sewn 5 times, every 0.75 × size per leg.
    - V, stem, candlewicking and chain tie in with three 0.3 mm steps.
  - Still open: the motif line's defaults. PE's default motif is PE-DESIGN's own artwork, so no
    stitch comparison is possible; candlewicking Low and High, and the chain diamond, are not
    measured.
  - Calibrated from the PE-DESIGN 11.50 reference files (2026-10-27): fill 4.5 rows/mm with
    under sewing on (`EdgeFill`: edge run 1 mm inside the outline at <= 2.5 mm, then tatami
    rows across the fill, 2 mm apart, 4 mm stitches, 30 % shift, 1 mm inset), also on for
    programmable fill. Fill Frequency default 30 % as PE (2026-10-27, sweep references). Still
    open: the programmable fill density.
  - The guide gives word scales only. Fit the mapping from reference files at slider minimum,
    middle and maximum.
- **Line Start/End type (Feed/Run) (calibrated 2026-10-27 for stem, candlewick and chain):**
  PE-DESIGN 11.50 keeps every element of a closed line where it is without the moved points;
  only where the sewing starts and ends changes. An element belongs to a point when it is the
  last to begin at or before it. The pattern starts with the exit point's (L2) element and
  goes round to the one before it, so it also ends there (a stem sews that station again, a
  candlewick sews that knot last). Feed starts with the pattern; Run starts at the entry
  point's (L1) element and sews only the leads (link points, stem station centres, candlewick
  running stitch) to L2's element, the shorter way round, backwards if need be. No jumps or
  trims. OpenStitch does the same (`object.setLine` `startEnd`, `object.setEntryExit`
  `lineExit`; ten specs line-{chain,stem,candlewick}-startend-*, all matching PE's needle
  points in order;
  [guide](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/16_1293444.html)).
  Still open:
  - Motif lines: PE's default motif is PE-DESIGN's own artwork, so no stitch comparison is
    possible; a reference with a simple motif drawn in PE would allow one.
  - Open lines and chain diamonds: not measured; they keep restarting at the entry point.
  - The editor: no Feed/Run radio yet (the engine has it); the L2 marker is there since #547.
- **Line spacing and text size (calibrated 2026-10-27):** PE-DESIGN 11.50's line spacing is
  "% of char height", default 100; its TrueType size is 0.7 em (Lato Bold's H is 1.033 × size)
  and 100 % sets lines the font's Windows ascent apart. Measured in `compat/reference/text`
  (specs text-size-h, text-size-hg, text-line-spacing-50/100/150, all at the reference tier).
  OpenStitch now does the same; `.osd` files before version 3 keep the old capital-height
  basis.
- **Text sewing defaults (calibrated 2026-10-27):** PE-DESIGN 11.50's text tool sews Satin
  Stitch at density 5.0 with under sewing off and pull compensation 0; switched to Fill it is
  density 5.0, under sewing off, Frequency 30 % (pe-gui, specs text-defaults-h and
  text-defaults-fill). OpenStitch now gives new text the same; saved texts keep their stored
  attributes. Stitch counts still differ: Fill 544 vs PE's 483 (fill rows join with a short
  step along the outline where PE runs straight into the next row; the stitch-sweeps track),
  Satin 221 vs 285 (satin structure).
- **Fit Text to Outline (calibrated 2026-10-27):** PE-DESIGN 11.50's dialog has Horizontal
  Alignment left / centre (default) / right / stretched / spaced evenly, Vertical Alignment
  above (default) / centred / below (by the size, not the capital height), Text Orientation
  along / upright, and "the other side"; a circle's outline starts at its bottom, and only one
  text fits an outline. OpenStitch now does the same: specs text-fit-* (eight files) at the
  reference tier; stretched is implemented but has no comparable reference (PE's file is far
  off the page). Fitted texts take no Text Transform, as in PE.
  ([guide](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1810870.html))
  Read stitch by stitch on 2026-10-28 (text-fit-* and the round-3 size/spacing/AHHHH sweeps):
  the circle's running stitch is PE's Bézier sampling (each quarter arc halved until the chord
  fits the 2 mm pitch: 4 × 32 chords; OpenStitch now matches it point for point, 0.04 mm mean,
  text-fit-default's mean needle distance 0.27 → 0.20 mm; #121, 2026-10-29: the halving stays
  where this reference pins it, a flatness-tolerance polyline resampled at the pitch would move
  every needle point; what changed is the 256-chord ceiling, now 2^16, so long or tight curves
  never keep a chord over the pitch; outlines are flattened adaptively to 0.05 mm as before); the letters sit where the font's
  advance box (no kerning) centred on the path's middle puts them (the A's ink starts at 75.04 mm
  from the circle's start in both), character spacing spreading symmetrically. Still different:
  PE sews every letter stroke by stroke with the rows across each stroke (see Text below), the
  letters after the first drift 0.05–0.07 mm/letter earlier in PE, and PE's round strokes end
  0.2–0.5 mm short of the outline's extreme points (B/C/D 5.91/6.42/6.90 mm wide against the
  font's 6.13/6.98/7.34; A and E match).
- **Text Transform (calibrated 2026-10-27):** PE-DESIGN 11.50 has 26 presets (two arches,
  then a 4 × 6 grid; ticking Transform applies preset 3; fitted text cannot be transformed,
  and fitting cancels it). OpenStitch lays out 24 of them, from envelopes fitted to PE's
  output by `tools/fit_envelopes.py` (specs text-transform-N, reference tier: bounds within
  0.5 mm, every H stem's top and bottom within 0.5 mm). 13 and 14 (fan out / in) since
  2026-10-28: each letter keeps its height and is sheared linearly between one edge of the
  straight box and the other edge scaled by 0.70 about the text's centre (13 shrinks the
  bottom, 14 the top; fitted from the letters' corners); specs text-transform-13/-14 at the
  reference tier by their box. Their stitch counts are 41-43 % above PE-DESIGN's (1653 vs
  1171, 1659 vs 1159) against +14 % on the straight-edged presets: the fill rows across
  sheared stems, for the text-fill item below.
- **Text Transform level (round 4, 2026-10-29):** PE-DESIGN 11.50 has no numeric
  "Transform level": ticking Transform puts one or two green diamond handles on the canvas
  and the strength is set by dragging one (the status bar shows nothing). The value is
  stored in the saved file (a float32 0x7E bytes after the last UTF-16 font name of the
  `CEmbDText` object, with the arch radius in 0.1 mm as the float32 4 bytes later): a
  percentage of the font size for the straight-edged presets (default 50; 20 for the bows
  and waves 5, 6, 15, 16, 19, 20, 23, 24), an angle in degrees for the slants (9, 10:
  default 10) and the fans (13, 14: default 30, the handle moving horizontally), and for
  the arches (1, 2) the half-sweep angle (default 30) with the radius that keeps the text
  length (38.4 mm for "OpenStitch", 30.2 mm for "Agjy 123"); the end-of-arc handle moves
  horizontally (41.2 deg / 27.9 mm to 1.15 deg / 1000 mm = straight), while the two arrows
  at the top scale the letters, not the transform. Clicking a preset resets its value to
  the default. Round 4 recorded every preset on the string "OpenStitch" (round letters,
  ascenders, descenders) and "Agjy 123" (descenders and digits) at the default level, and
  at four other handle positions per preset (OpenStitch only; values per file in the owner's
  notes and the MANIFEST). At the default level OpenStitch's envelopes keep the box within
  0.5 mm on 21 of the 26 presets for "OpenStitch" (specs text-transform-N-openstitch)
  and 18 of 26 for "Agjy 123" (text-transform-N-agjy; the arches 1 and 2 and 18 fail
  there as well, by 0.6-1.3 mm);
  parked until 2026-10-30 (owner's round-4 notes, numbers there): 3 (maxY 0.8 mm low), 9 (minX
  1.1 mm), 10 (maxX 1.4 mm), 14 (minX 1.7, maxX 2.1 mm) and 17 (maxY 0.7 mm): the envelopes were
  fitted on the HHHHHHH box and PE-DESIGN applies them to the cap-height box, so letters below
  the baseline (p, g, j, y) and round tops landed differently; all pass since the cap-height box
  (below). Since 2026-10-30 `text.add` / `object.setText` take
  `envelopeLevel` (the stored value; a new preset resets it, as in PE-DESIGN), which scales
  how far the preset's envelope bows from straight (the slants by the shear's tangent, the
  arches' angle with the radius keeping the text length), and the PES 0110 writer stores it.
  Since the same day the envelope warps the cap-height box (the capital line to the baseline,
  as PE-DESIGN does: the envelopes were fitted on HHHHHHH, whose box it is), so letters below
  the baseline go on below the bottom edge instead of squeezing the envelope: 105 of the 108
  level captures and 25 of the 26 default-level captures on "Agjy 123" (all 26 on
  "OpenStitch") keep the box within 0.5 mm (specs `text-transform-N-openstitch[-lL]`,
  `text-transform-N-agjy[-lL]`). Parked (round4\text, not in the repository): arch 2 on
  "Agjy 123" at the default level (minX 0.6 mm) and on "OpenStitch" at level 25 (minX
  0.6 mm), taper 4 at level 100 (maxX 1.1 mm) and slant 9 at level 100 (maxX 0.6 mm). Checked
  across the levels (2026-10-30), no single rule fixes them: arch 2 sits 0.3-0.6 mm left
  (minX) at every level while its right edge moves from -0.2 to +0.3 mm, so a shift would push
  level 100 out; taper 4 is within 0.1 mm at every level but 100 (the full taper); slant 9's
  right edge is already 0.4 mm short at the default and 0.6 mm at level 100 (the fitted envelope,
  not the level scaling). Left parked.
- **Sewing order inside a text (checked 2026-10-27):** not a gap. PE-DESIGN 11.50 also sews
  every letter's region first, then every letter's outline, one colour block each (spec
  text-sewing-order, reference tier). Still open: the guide disables "some editing of
  individual characters" on user-mapped text without naming them; checking which needs
  characters registered with Brother's tools, which the project does not use.
- **Machine format exports (calibrated 2026-10-28):** PE-DESIGN 11.50's DST, EXP, JEF, SEW,
  XXX, PCS, HUS and VIP exports of one OpenStitch design (`compat/reference/machine`, spec
  `machine-formats`, reference tier) showed the writers differed in how they laid out a frame
  move, a trim, a colour change, a stitch longer than a record and the origin. The writers now
  follow PE-DESIGN record for record (EXP and PCS byte for byte; the rules are tabulated in
  FILE_FORMATS.md "PE-DESIGN's record layout"): the design centred on the stream origin, a long
  stitch as jumps plus one stitch (one needle penetration), a frame move as a jump run with no
  trim record (DST padded to three records, XXX one `7E` record, VIP `88`, HUS a zero-length
  jump), the move's remainder folded into the next stitch in JEF, SEW, XXX and VIP, PCS's fixed
  16-colour table indexed by its colour records, HUS's `0x00C8FC5D` magic. The readers follow:
  a single jump no longer than a record is read as the long part of a stitch, not a trim (JEF,
  EXP), `80 02` is an EXP jump, PE-DESIGN's HUS trim marker and XXX `7E` moves read as trims,
  PCS's index byte and fraction byte are read. SEW's start point (settled 2026-10-28,
  `sew-a-moved10.*`, `sew-square-centre.*`): the stream is relative to the design (PE-DESIGN's
  export of the same design moved 10 mm on the page is byte-identical) and starts 15.9 mm left
  of and 9.7 mm above the design centre, the same for the 40 mm design and the 20 x 20 mm
  square, the way to the first needle point sewn as a stitch and split like one; the writer
  starts there and the reader keeps the move, so the SEW compare is record for record from the
  first one. The spec exports OpenStitch's saved a.pes itself (`stitches.import`), the file
  PE-DESIGN opened, so the writers are compared on exactly PE-DESIGN's input whatever the
  stitch generator does later; an imported stitch block's needle points are exported as
  recorded (a 15 mm stitch stays one penetration, as PE-DESIGN exports the opened file; the
  compiler used to split it at the machine's longest stitch). The square's export also shows
  PE-DESIGN exporting its own objects from their
  first needle point, the end of the PEC's first jump (its tie-in: there, 0.3 mm back, there
  again), where OpenStitch's PES reader starts at the first PEC stitch record: one record more
  in PE-DESIGN's stream, the rest identical (`sew_stream_is_design_relative_and_starts_where_pe_design_does`).
  Still open, each settled by one more export: how the folding formats write a frame move shorter
  than one record; VIP's exact piece length (7.5–7.9 mm); JEF's `-1` hoop block; XXX's header
  tables and palette filler; SEW's 340 header bytes; DST's `AX`/`AY`; PE-DESIGN's RGB for chart
  colour 800 RED (`E3 31 1F`) against the PEC chart table's `ED 17 1F`.
- **Concentric Circle and Spiral (calibrated 2026-10-29, #138):** issue #138 assumed PE-DESIGN
  has no concentric or spiral sewing; it has both as region sew types, with only Density
  (default 2.0), Run pitch (2.0) and two Start Point buttons. Fourteen references
  (`concentric-*`, `spiral-*`: a 40 mm circle at defaults, start 2, density 1.0 and 3.0, pitch
  3.0; a 40 × 30 mm rectangle; a 40 mm ring with a 15 mm hole) show:
  - Concentric Circle: circles about the bounding-box centre at d, 2d, … (d = 1 / density);
    the whole circles from the largest inwards, each from one radial seam, counter-clockwise;
    a trim; the cut circles from the smallest outwards as arcs, back and forth per piece, with
    running travel along the outline. The seam is where the first cut circle meets the
    outline (on the rectangle where the 15.5 mm circle leaves the bottom edge, 75.4°).
  - Spiral: not an Archimedean spiral about a centre but the outline offset a row inwards per
    turn (turns 0.5 mm apart on both axes of the rectangle), from R1 on the outline to the
    middle; per turn 4·2ⁿ needle points (64 a turn down to r ≈ 10 mm on the circle, then 32,
    16, 8), corners kept on the rectangle.
  - The second Start Point button sews the same stitches backwards; neither type travels from
    R1 or to R2.
  - A ring made as Set hole sewing with the inner circle's region Not Sewn keeps the hole
    (OpenStitch dropped a paired hole whose inner shape sewed no region; fixed).

  Status: reference tier, all 14 specs pass. Stitch counts within 3 %, bounds within 0.35 mm,
  mean stitch length within 0.15 mm. Open (count checks kept out of the reference tier, not
  loosened):
  - `spiral-circle40-pitch3p0`: PE 1354 stitches, OpenStitch 1284 (−5.2 %). The count is an
    internal-tier check pinning OpenStitch's 1284 at 3 %; bounds and stitch lengths stay at the
    reference tier. Investigated 2026-10-30, turn by turn: PE's outer turns have 64 needle
    points like OpenStitch's (the 4·2ⁿ rule), but below the radius where the rule halves the
    count PE's turns fall off gradually instead of in powers of two: 64 down to r = 16.2 mm,
    then 56, 44, 38, 39, 40, 40, 36, 34, 33, 32 … 28, 25, 22, 19, 18, 16 against OpenStitch's
    32 from r = 15.3 mm and 16 from 7.7 mm. Inside a turn PE splits some base pieces in half and
    not others (adaptive halving per piece), and the pieces next to the axis directions are
    narrower than the rest (about 8.8° against 11–12° at r = 12 mm, pitch 2), although the turns
    are true circles (radius within 0.04 mm all round): the parameterisation is not uniform in
    angle. Neither a cubic Bézier quarter (11.0–11.6° for 8 equal steps) nor a rational
    quadratic one (10.5–11.8°) reproduces it, so the rule is still open. The same pattern shows
    at pitch 2 (turn counts 60, 62, 55, 60 … in the 64 band; 42, 39, 36 … in the 32 band), where
    it cancels out in the total (within 2 %).
  - `spiral-ring40-hole15`: PE 1952 stitches, OpenStitch 1473 (−24.5 %). The spec compares
    bounds and stitch lengths only. Investigated 2026-10-30: PE sews the outline's spiral from
    19.85 mm in to 13.6 mm (908 needle points), then 31 separate pieces round the middle of the
    band (r 13.4–14.0 mm, 397 needle points, 64 trims), then the hole's spiral from 13.6 mm in
    to the hole (647 points). The pieces are wedges about 5.5° wide repeating every 11.25°;
    each has a tie-in and a lock (0.3 mm back-and-forth), 13 of them nothing else, the others
    up to 37 stitches zigzagging inside their 1.3 mm wedge. They are PE filling leftover slivers
    between its two spirals one by one. OpenStitch does not copy them: 64 trims in a 25 mm²
    band is a defect a user would not want, and the band is covered by the two spirals' last
    turns. Kept as a documented difference.

  Other known differences:
  - Concentric chord splits: inside each circle PE's stitches come as single 2.0 mm stitches,
    pairs of 1.0 mm and triples of 1.33 mm: key spans of about 2 and 4 mm split into
    ceil(length / pitch) equal stitches. Rounding the 4·2ⁿ points to 0.1 mm explains only part
    of it (OpenStitch's counts are within 3 %); where PE's key points come from is open.
  - Spiral winding and ending (settled 2026-10-30, round 5: `spiral-square40`,
    `spiral-rect30x40`, `spiral-rect40x30-drag-reverse`, `spiral-ellipse40x30`,
    `spiral-circle40-drag-reverse`, `spiral-triangle`): the drag direction does not matter
    (identical files); what decides is where the start point lies. On an outline vertex (a
    circle's or ellipse's bottom point, a triangle's bottom-left corner) the spiral turns
    counter-clockwise on screen and ends in the middle. Inside an edge (a rectangle's bottom
    middle) it runs clockwise to the next corner, spirals from there and, after a trim, returns to
    that corner and runs back along the edge to the start point, ending where it began. OpenStitch
    sews both cases that way now (`crate::spiral`, closing run as its own run); all six specs pass
    within 3 %. PE-DESIGN shows no R1/R2 markers for Spiral or Concentric Circle regions, so a
    moved start point cannot be captured.
  - Tie-in, lock and the triangle (fixed 2026-10-31): PE-DESIGN ties in at the start and locks
    at the end of every Concentric Circle and Spiral run it separates with a trim (the whole and
    the cut circles; a spiral and its closing run). OpenStitch did neither, so a spiral's start
    point was only a jump target: the triangle's start corner went unsewn (39.0 mm wide against
    PE's 39.7). Each run now gets the tie-in and lock fill and satin use; the triangle measures
    39.8 × 39.3 mm (PE 39.7 × 39.2) and its width is compared again. A cornered turn's sides are
    now split by their own length, between the two rings' sides (triangle 854 stitches against
    PE's 842, +1.4 %, was +3.4 %; mean stitch 1.88 mm against 1.90).
  - Round 6 (2026-10-31, 12 references: concentric square, portrait rectangle and ellipse,
    20 and 36 mm circles, triangle; spiral on the 40 mm circle at pitch 1.0, 2.5 and 4.0, on a
    20 mm circle at pitch 3.0, and two triangles):
    - Concentric order (fixed): when the start point lies farther out than the largest whole
      circle (a portrait rectangle's or ellipse's bottom, a triangle's corner), PE starts there
      on the cut circles, works inwards through them and then through the whole circles to the
      middle, without a trim. OpenStitch does the same now (`concentric-rect30x40`,
      `-ellipse30x40`, `-triangle`).
    - Triangle spirals (fixed): a turn starts on a corner of the outer ring, so the inner ring's
      corner sits just after the start and the sides did not pair; corners within two stitches
      of a turn's ends are now left out of both rings, and each side is split as finely as keeps
      its longest stitch within the pitch and 0.05 mm (PE's own reach 2.06–2.12 mm). Wide
      triangle 960 against 941, default triangle 851 against 842.
    - Still open, each pinned at the internal tier with PE's value in its evidence (no tolerance
      widened): spiral needle counts per turn below the outer power-of-two turns (pitch 4.0:
      859 against PE's 1024, PE keeping about 32 a turn down to r 9 mm; pitch 1.0: 3419 against
      2997, PE falling from 128 to 121, 108 … earlier; 20 mm circle at pitch 3.0: 333 against
      384; the inner tails at 2.5 and 4.0 mm are identical, so below some radius PE's count no
      longer depends on the pitch); concentric counts on the 20 mm circle (490 against 533),
      the square (2443 against 2362) and the triangle (1383 against 1283), where PE's circles
      carry about 2·π·r / pitch + 20 needle points instead of 4·2ⁿ; the density-1.0 triangle
      spiral (441 against 425: more corner stitches on the short inner sides); the 36 mm
      circle's width (36.0 against 35.6) and the portrait ellipse's height (39.0 against 39.5).
  - Spiral round a hole, round 7 (2026-10-31, 6 references: 40 mm rings with 10, 15 and 20 mm
    holes, the 15 mm ring at density 1.0 and pitch 3.0, an offset hole, a square with a square
    hole). Decision (coordinator, 2026-10-31): the default is to sew PE-DESIGN's band, with an
    OpenStitch mode (`compatMode: "openStitch"`, the vocabulary shared with the image wizards)
    keeping today's single travel. What the files show: PE sews the outline's spiral in to the
    middle of the band, then the band as separate trimmed pieces, then the hole's spiral. On
    concentric rings the pieces are about 5.5° wide every 11.25° (31 pieces, 64 trims; every
    22.5° with a 10 mm hole); their needle counts repeat with quarter-circle symmetry (6, 12,
    18, 25, 34, 42); the bigger ones zigzag inside a sliver. An offset hole leaves almost no
    band (3 tiny pieces); a square with a square hole gives three side segments. The pieces are
    the gaps PE's own turn sampling leaves between its last outer and last hole turns, so they
    cannot be reproduced before that sampling rule is known (the same open item as the spiral
    needles per turn). Not implemented yet; the counts of the four concentric-ring files (and
    the offset hole's stitch lengths) are pinned at the internal tier with PE's values in their
    evidence, everything else, and the whole square-with-hole file, at the reference tier.
  - Cut circles on the outline (fixed 2026-10-31): a circle that only grazes the outline or a
    hole's edge (nowhere more than 0.1 mm outside: the 40 mm circle's 20 mm circle, the ring's
    7.5 mm circle on its hole) is sewn as one loop, as PE-DESIGN does, instead of breaking into
    fragments joined by travel where it crosses the flattened outline (ring with a hole: 1541
    stitches against PE's 1581, was 1634 with the tie-ins; 30 mm circle 1140 against 1121).
  - The concentric seam on a Bézier circle stays open. Round 5 adds a 30 mm circle (seam at the
    left edge just above the middle, like the 40 mm one) and a 40 × 30 mm ellipse (seam at the
    top just right of the middle); no single walking rule fits those and the rectangle's 75.4°.
    The new specs `concentric-circle30` and `concentric-ellipse40x30` compare counts, bounds and
    stitch lengths, not the seam.
- **Web editor, batch:mac-wasm (2026-11-06; #556):** the browser build is beyond PE-DESIGN
  (PE-DESIGN 11 runs on Windows only) and changes no output: it runs the engine's own
  `project.open`, `stitches.import`, `design.export` and `project.save`, whose file access now
  goes through `openstitch_engine::fsio` (the same `std::fs` calls on the desktop, memory in the
  browser). CI job `web` checks the browser build's PES, DST, EXP, VP3, XXX, CSV and G-code
  exports are byte-identical to the native build's for reference PES and DST files.
- **Editor, batch:editor-core part 2 (2026-11-06; #549, #550):** autosave and recovery, the
  command palette, the hints line and the property search are beyond PE-DESIGN and change no
  design, file or default PE-DESIGN behaviour: snapshots are written only to OpenStitch's own
  settings folder (never the design's file), and the palette runs the same ribbon commands a
  click runs.
- **Editor, batch:editor-core (2026-11-06; #500, #502):** Copy, Cut and Paste go through the
  system clipboard as OpenStitch object data, and Paste puts objects back where they were copied
  from, as PE-DESIGN 11 does between its windows; nothing PE-DESIGN reads or writes changes. The
  Sewing Order list's rename, lock and click order change only names, a lock flag (`.osd` only)
  and the object order. Sew outline first is beyond PE-DESIGN (off by default, PE-DESIGN's
  region-then-outline order unchanged): PE-DESIGN has no per-shape part order, so a PES 11
  export writes such a shape as two shapes, its outline then its region, and the stitch data
  sews them in that order.
- **Image digitizing, batch:mac-digitize-3 (2026-11-06; #688, #690):** PE-DESIGN-compatible
  output unchanged (scorer identical). OpenStitch mode: outline paths keep vertices at least
  0.4 mm apart and, except on line art, specks (every stitch within 0.8 mm) are left out, so sew_check passes 27 of
  the 30 example images (25 on main; p01 and p09 are #703, g11 is #713; with presets 197 of
  210 runs, every preset clean on g01-g08). Likeness: g05 MS-SSIM 0.8770 → 0.8750 (its
  outlines no longer follow pixel steps; 879 fewer stitches under 1 mm), the other Auto Punch
  cases within 0.0003 or better. Leaving out g11's dots as specks cost MS-SSIM 0.653 → 0.611, so
  line art keeps them. Two golden hashes change (specks left out): emblem Auto Punch
  0xcfe047434a377450 → 0x6f8c38e54e037ddb and gradient Auto Punch 0x9fb965f4afbd6472 →
  0xb90ef983b645fd84; the other 7 are unchanged.
- **Image wizard, batch:mac-wizard (2026-11-06; #489):** the dialog's region table, click-to-fix,
  colour order and Preset row drive `digitize.autoPunch`'s `dryRun`, `regionMap`, `overrides`,
  `order` and `preset`; PE-DESIGN-compatible output with none of them set is unchanged.
  PE-DESIGN has no per-region reasons, overrides, presets or click-to-toggle (toc:143-150), so
  all of it is beyond PE-DESIGN.
- **Image digitizing, batch:mac-digitize-2 (2026-11-06; #447, #469, #489, #490):** PE-DESIGN-compatible
  mode's output is unchanged (scorer identical on g01, g02, g05, g13, p03; every new option is
  OpenStitch mode only, off unless set). New in OpenStitch mode: `angleRule: leastTravel`,
  presets, `digitize.exportOutlines`, `underlap`, `subjectPoint`, `digitize.segment` and
  `lines: edges`. `leastTravel` against `pe` (`--no-dists`, 100 mm): ΔE00 / MS-SSIM g01
  9.44/0.8628 → 8.71/0.8732, g02 10.68/0.7821 → 11.02/0.8130, g05 8.06/0.8770 → 8.04/0.8813, g06
  8.47/0.8245 → 8.27/0.8274, g08 8.61/0.9668 → 7.84/0.9789, g11 unchanged, g13 24.57/0.5749 →
  21.17/0.5714, g15 14.50/0.8700 → 13.54/0.8849, p03 16.01/0.5335 → 15.54/0.5516, d01
  23.80/0.3709 → 20.42/0.3917; thread (sewn plus connections) never higher. `underlap: 0.8`
  is mixed (g01, g02, g08, g13, d01 better; g06, g11, g15, p03 slightly worse; p03 density
  3.98 → 4.66 points/mm², g08 trims 12 → 32), so it stays off by default. Measured and not
  shipped: #447's PE-compatible thread set (p03: main matches 2 of PE-DESIGN's 10 threads;
  CIELAB median cut 3, p03 ΔE00 15.99 → 18.21; CIELAB until 10 distinct 3, 17.54; popularity 2;
  boxes by their commonest colour 1) and #469's Cross Stitch edge cells (g07: own-colour rule
  at 2.5 mm ΔE00 14.19 → 14.53, MS-SSIM 0.7573 → 0.7364; a grid snapped to the art's pixels
  helps at the art's own 4.4 mm cell, 0.5978 → 0.6905, but g07's art pixels are not square,
  34.5 × 33 px, so square cells drift by up to 4 mm).
- **Image digitizing, batch:auto-punch (2026-11-06; #445, #447, #469, #489):** PE-DESIGN-compatible
  mode's output is unchanged on every gallery case (scorer identical, 17 cases). OpenStitch mode:
  a paper background is the fabric showing (#445), thin parts cut by the sliver opening are
  running lines (#469), the region plan, overrides and neighbour angles are new options (#489).
  Measured and not shipped: giving PE-compatible mode as many distinct nearest threads as its
  colour count (#447) gives p03 10 threads with Pewter, as PE-DESIGN's capture has, but
  ΔE00 15.97 → 16.96 and MS-SSIM 0.625 → 0.611 (d01 improves, 23.97 → 23.40); Cross Stitch
  edge cells taking one of their own threads (#469) helps g02 and g07 but g05 drops
  (ΔE00 10.94 → 11.32).
- **Image digitizing (Phase 6, issues #127, #128, #132, #129):** the Auto Punch, Cross Stitch
  and Photo Stitch pipelines were this project's own approximations; the Phase 6 gap analysis
  pins their arithmetic. Done 2026-10-29 (#128): mono Photo Stitch uses integer Rec. 601
  luminance `(30 R + 59 G + 11 B) / 100` and a full-range min/max contrast stretch
  `g' = (g − min) · 255 / (max − min)`, applied only when `max − min` is in (1, 255), instead of
  the 2nd–98th percentile stretch. On a linear grey ramp the two stretches agree (576 against
  573 needle points for the 50 mm Fill ramp, 464 against 456 for Lines); they differ on images
  with a few outlying dark or light pixels, which the percentile stretch ignored and the
  min/max stretch anchors on. Left: the colour mode's thread snap through the equal-weight
  metric (#119), the sepia re-tone, and the capture comparison (stitch density per tone band
  and total counts against pe-refs-4's Photo Stitch files at defaults) once those captures
  land. Done 2026-10-29 (#127): Auto Punch reduces colours from a census of the pixels' 3×3
  modal colours capped at 100 tracked colours (later colours count under their nearest
  tracked one), removes every 4-connected region of at most `max(4, round(param · 100 /
  mmPerPx²))` pixels (the parameter in mm², so `minArea / mmPerPx²`) into its neighbour, and
  decides satin by a centerline-thinness test (stroke width at the deepest point at most
  `satinMaxWidth`, default 6 mm) instead of the longest scan span across the principal axis:
  a bent 4 mm stroke is now satin, a 10 mm blob fill. Left for #127: the recorded defaults
  and the region, satin and stitch counts against pe-refs-4's Auto Punch captures; the
  optional graph-cut foreground step. Done 2026-10-29 (#132): Cross Stitch reduces colours
  through the same census prepass (one implementation, two callers), its cells vote with their
  modal colours, and its defaults are explicit: 2.5 mm cells (0.4 per mm), 8 colours, full
  crosses, each diagonal once (`times: triple` sews it three times), border colour left out.
  On the synthetic grids the cells are the same as before (8 × 4 cells, 6 + 1 coloured);
  what changes is the palette's weighting. Left for #132: PE-DESIGN's recorded defaults from
  the capture, which may move the cell size or the times. Done 2026-10-30 (round-5 test
  images): the census took colours in scan order, so on an anti-aliased image its 100 slots
  filled with edge shades of the top rows (by row 31 of the generated emblem `g01`) and a large
  orange or yellow area counted as grey; PE-DESIGN keeps all four colours. It now takes
  colours most frequent first and counts a colour within RGB distance 32 under a tracked one,
  and the automatic colour count stops at 10 as PE's. Automatic counts against PE: g01 4/4,
  g13 mosaic 10/10, g14 two colours 2/2, g15 folk bird 5/5, every photo 10/10, g05 robot 7/8,
  g08 lettering 4/3, g02 balloon 6/9, g12 stencil 2/4, g11 mandala 5/9. Cross Stitch on the
  pixel-art strawberry sews 6 threads where it sewed 4 (PE 7). Left: PE sews g01's white sun
  rays, where OpenStitch leaves out every region of the background colour, not only those
  touching the border. Done 2026-11-04 (#127): the optional graph-cut foreground step, as Auto
  Punch's opt-in `isolateSubject` in OpenStitch mode only; PE-DESIGN-compatible mode is
  unchanged.
  Compared with PE-DESIGN 11.50 on 2026-10-29 (round-4 captures from the owner's
  four-shape image `owner-shapes.bmp`, 240 × 240 px placed at 84.7 mm: a red disc, a green
  triangle, a yellow rectangle and a blue square on white, every wizard at its defaults):
  - **Auto Punch** (`autopunch-default.pes`; Max. Number of Colors 5, white omitted): PE
    makes 4 fill regions (none satin; OpenStitch agrees) of 4482 stitches, each starting with
    the 1 mm edge run, at 45° except the triangle at 90°. OpenStitch had 4241 (−5.4 %) at 0°
    (the principal axis); with fill at 45° and 5 colours it has 4387 (−2.1 %): red 1481 / 1485,
    blue 1279 / 1329, yellow 851 / 881, green 776 / 787 (PE's at 90°). Open: PE's colour order
    (yellow, blue, red, green, the same in its Cross Stitch; not area, position or chart order)
    and the triangle's 90°, which one image cannot pin down.
  - **Cross Stitch** (`crossstitch-default.pes`; Cross size 2.5 mm, 5 colours, Times Single):
    2866 stitches. PE's grid matches OpenStitch's (cells red 161 / 162, blue 143 / 143, green
    75 / 74; yellow 94 / 88 at the rectangle's partial edge cells). PE sews six stitches per
    cell, all on the diagonals: each diagonal once whole and once in halves through the cell
    centre (median stitch 1.77 mm). OpenStitch sewed four diagonals and two cell-side steps
    (1907 stitches, −33 %); it now sews PE's six (2841, −0.9 %). Open: PE travels from run to
    run along the diagonals with one jump per colour, where OpenStitch jumps between row runs.
  - **Photo Stitch 1** (`photostitch1-default-plus-cross.pes`, items 5–14; Max. Number of
    Colors 10, Run Pitch 3.0 mm, Sew Page Color ticked): 12 255 stitches in 10 threads, the
    page colour (White) first over the white areas (7212), every tone band sewn at 160–210
    needle points per cm². PE's path is not row-based: contour-following meanders of ~3.2 mm
    runs, 1–5 jumps per colour. OpenStitch had 5235 stitches with the white bare (14 per cm²,
    stray dither tints) and 2 mm stitches; with Sew Page Color on by default, 3.0 mm stitches,
    10 colours and 0.25 mm rows it has 12 860 (+5 %, with #119's colour metric): white 157 / 162,
    yellow 181 / 170, green 234 / 211, red 173 / 175, blue 237 / 176 needle points per cm²
    (OpenStitch / PE). Open:
    the path (OpenStitch trims at every run end, about 1100 times here) and PE's thread choice
    (it mixes two or three threads per colour area); both are the stitch-path model of #129,
    which the patent note in docs/research/beyond/patents.md bears on.
  - **Test images (2026-10-30).** `examples/images` holds 30 generated test images: photos,
    illustrations, line art, pixel art, lettering, transparency, noise and sizes. Each is
    mapped to its wizards in docs/research/test-images.md, and PE-DESIGN captures of each are
    requested. The first finding is on the generated flat emblem
    `g01-emblem.png` (navy, orange, yellow, white, anti-aliased). Auto Punch at defaults sews
    its orange as Silver and its yellow as Lavender. The 100-colour census is full by the
    32nd row of the working image, with near-white and navy edge shades, before the orange and
    yellow are reached. Those colours then join their nearest tracked shade. PE-DESIGN's
    round-5 capture keeps all four colours (4 automatic colours, 11 342 stitches), so this is
    a difference; the fix is its own PR. The rest of round 5's first captures are in
    docs/research/test-images.md (Auto Select's four threads on a two-colour image, the
    Color Set threads, Cross Stitch on pixel art).
- **Programmable fill, motif fill and motif line from owner-made patterns (reference,
  2026-10-29, #130/#131):** PE-DESIGN 11.50's fills of six owner-made Programmable Stitch
  Creator files showed OpenStitch's own model was off: PE-DESIGN tiles the pattern on the page
  axes from the region's top-left corner and sews only the row ends and the crossings with the
  tile lines (rows sharing their turning points, 8 mm gaps unsplit); OpenStitch tiled in the
  row's frame and added a staggered grid (3392 and 3209 needle points against PE-DESIGN's 2355
  and 1638, mean distance to PE-DESIGN's nearest needle point 0.63 and 1.03 mm). Now 2329 and
  1636, within 0.2 mm (mean 0.10 and 0.11 mm); specs `prog-fill-square-4pt`,
  `prog-fill-line-2pt`. Motif fill rows now start half a motif below the top edge (bounds
  40 × 37.5 mm as PE-DESIGN, `motif-fill-2pt`); motif lines fit a whole number of stretched
  motifs (`line-motif-zigzag-3pt`). The import mapped a motif's cross offset at twice its size
  (fixed: 1024 cell units are one motif size). A reference point was ignored for programmable
  and motif fills, and a user pattern fell back to the default with a reference point or a
  curved direction (both fixed). The motif fill's travel is PE-DESIGN's since 2026-10-30: rows
  from the one nearest the start point, travel along the outline in even steps of at most
  2 mm, the fill's tie-in and lock (130 needle points against PE-DESIGN's 130, was 103; spec
  `motif-fill-2pt` now checks the count within 3 %). The motif line ties in and locks like
  PE-DESIGN's other lines since 2026-10-30 (23 needle points against PE-DESIGN's 23, was 16;
  spec `line-motif-zigzag-3pt` checks the count within 3 %). Left: which way PE-DESIGN walks
  when both ends of the first motif-fill row are as near.
  Second round of captures, 2026-10-29 (pe-refs-4, inputs drawn by its `wizimg.py`):
  - **Auto Punch bars** (`autopunch-thin-strokes.pes`, nine black 40 mm bars 0.5–4 mm wide;
    `autopunch-thin-strokes-wide.pes`, seven 30 mm bars 5–14 mm; Max. Number of Colors filled
    in as 2): PE makes satin from 0.75 to 6.1 mm (Auto direction, no under sewing, 200–214
    stitches per 40 mm bar whatever the width) and fill for 0.5 mm and from 7.1 mm (under
    sewing on; 90° across the bar up to 12 mm, 45° at 14 mm), widest bar first. OpenStitch now
    satins 0.7–6.5 mm strokes with Auto direction and no under sewing, fills across the stroke
    up to 13 mm and at 45° beyond, and sews each colour's regions largest first: the same
    choices on all 16 bars, 2011 / 3100 stitches against 2018 / 3197. Before, its working copy
    averaged the bar edges into greys that became extra threads and thin grey regions (3446 /
    4639 stitches, 2 and 4 threads). The working copy now keeps each box's most common colour,
    and the colour count is automatic as PE's is (2 for the bars, 5 for the shapes).
    At other working resolutions the choices did not hold (checked 2026-10-30): the chamfer
    width read an even-width stroke one pixel short, so the 0.75 mm bar turned fill at 300,
    350, 500, 700, 800 and 960 px (the 1 mm bar too at 300) and the 7 mm bar satin at 250 px;
    only 400 and 600 px matched PE. `regions::thickness` now adds the pixel when the deepest
    pixels form a two-pixel ridge (a stroke centred between two pixel rows), and every
    resolution from 250 to 960 px gives PE's choice on all 16 bars; widths of 1 to 12 px
    measure exactly. Counts at the default resolution are unchanged (3094 / 4373 on main).
  - **Specks, fill direction and order (2026-10-30, pe-refs-4's `autopunch-specks-default.pes`,
    `-specks-noise+1.pes`, `autopunch-right-triangle.pes`):** on a red square, a blue disc and
    square specks of 1, 2, 3, 5 and 8 px (0.35–2.82 mm) in both colours, PE drops the 1, 2
    and 3 px specks and keeps the 5 and 8 px ones; one more tick of Noise reduction gives a
    byte-identical file. OpenStitch already did the same: its 3×3 modal census removes the 1
    and 2 px specks, and the floor `max(4, round(minArea / mmPerPx²))` = 8 px removes what
    the census leaves of the 3 px one. Stitches 2355 against 2365. The fill direction was the
    leftover: PE gave the same disc 45° on one image and 90° on two others, and the isosceles
    triangle 90°. A single rule gives all 16 fills of the round-4 captures: the direction of
    90°, 45°, 135° and 0° whose rows spread furthest across the region, a diagonal winning
    over 90° only with at least one more row at 4.5 lines/mm (the 45° disc's diagonal spread
    is 0.50 mm wider, the 90° disc's 0.21 mm). It replaces the 13 mm stroke-width rule
    (`autopunch::fill_angle`). Within a colour PE sews blue (first) from the bottom specks up
    to the disc and red (second) from the square down to the specks, as it sews the bars
    bottom first: regions in scan order, reversed for the 1st, 3rd, … colour; it replaces
    "largest first". Counts: shapes 4384 / 4482, moved 4358 / 4436, specks 2355 / 2365,
    right triangle 2502 / 2547, bars 3094 / 3197 and 2011 / 2018.
  - **Thread picks after colour reduction (documented difference, 2026-10-30; pe-refs-4's
    round-5 `four-colours`, `ap-solo-*`, `ap-four-max10`, `ap-grid16-brother/-ver4` captures):**
    Auto Punch's BROTHER EMBROIDERY picks are not always the nearest thread.
    - The picks are a fixed per-colour mapping. Each colour gets the same thread alone as among
      others, and Max. Number of Colors does not change them, so the colour reduction does not
      move flat colours.
    - The Omit Region Colors swatches show the matched threads, not the reduced colours.
    - On 20 flat colours (four squares, a 16-colour grid of primaries, half levels, tints and
      greys), OpenStitch's equal-weight RGB match (#119) gives PE's thread for 13.
    - PE's misses go both ways, so no colour shift explains them:
      - (128, 0, 0) gets 49 Amber Red (181, 75, 100; PE-DESIGN writes 181, 76, 100, corrected 2026-11-12) over 13 nearer threads, four of them PE's
        own picks elsewhere;
      - (128, 128, 255) gets the darker 4 Cornflower Blue;
      - (192, 192, 192) gets the darker 16 Silver;
      - (200, 40, 120) gets 51 rather than 49;
      - (240, 160, 40) gets 12 rather than 52.
    - Twelve distances (RGB variants, YUV, HSV, CIELAB 1976 with and without gamma, CIE94,
      CIEDE2000) were tried, each after 22 pre-shifts (channel quantisation and bin centres,
      saturation, gamma, normalisation). The best explains 15 of 20, so none is adopted.
    - With the Ver4 Option Color chart, PE's picks depend on the other colours in the image
      ((200, 40, 120) gets red2 alone and magenta2 among others) and turn greys into navy,
      maroon and teal. That is treated as a PE-DESIGN quirk, not a model.
    OpenStitch keeps the equal-weight nearest thread.
  - **Colour order** (`autopunch-owner-shapes-moved.pes`, the same four colours with every
    shape moved and recoloured): PE sews yellow, blue, red, green again, so the order is a
    property of the colours. It is the reduced colour's R + G + B, highest first (yellow 480,
    blue 290, red 280, green 250; thread R + G + B would give yellow, red, blue, green).
    OpenStitch now sews colours in that order in Auto Punch and Cross Stitch: 4361 stitches
    against 4436.
  - **Photo Stitch 1 Mono** (`photostitch1-mono-grey-tonal.pes`, a grey gradient with soft
    blobs, values 0–234): no Sew Page Color, light areas unsewn, 2917 stitches, density falling
    about linearly with lightness from 154 needle points per cm² in the darkest band to none
    above grey 210. OpenStitch's Sew Page Color now applies to Color mode only. Its mono gives
    4068 stitches: 143 per cm² in the darkest band, 37 at 180–209 and 2 above 210 as PE, but
    132–181 in the mid-tones where PE has 87–142, because the dithered rows break into short
    runs there (426 trims; PE 2). That is the stitch-path model again.
  - **Photo Stitch 1 path (2026-10-30):** a thread's runs are now sewn nearest first instead
    of row by row with a jump to every run. From the needle, the closest unsewn run end within
    2 cells across and 2 rows down is entered with a travel stitch; a hop to the next row
    leaves out the run's last needle point so the row turns, as a fill row does (those hops
    had been sub-millimetre travel stitches, each a needle point). Mono capture: 3225 stitches
    against PE's 2917 (was 4068), 99 trims against PE's 2 (was 426); needle points per cm²
    by grey band 143 / 146 / 123 / 118 / 97 / 69 / 40 / 3 against PE's 154 / 142 / 125 / 92 /
    87 / 66 / 20 / 1 (0–29 … 210+), was 143 / 181 / 162 / 178 / 132 / 81 / 37 / 2. Colour
    capture: 11 654 stitches against PE's 12 255 (was 12 860), 249 trims against 6 (was 1158);
    white 148 / 162, yellow 173 / 170, green 198 / 211, red 166 / 175, blue 192 / 176 (was blue
    237). Left: PE's paths run along contours, diagonally; OpenStitch's stay horizontal rows,
    and the remaining trims are runs more than 2 cells from any other.
  - **Photo Stitch 2** (`photostitch2-color-owner-shapes.pes`, Line interval 2.0 mm, Max.
    Density 4.5 lines/mm, Direction 0°, Color Set Auto Select): a halftone screen. Each of four
    threads (blue, red, yellow, purple) sews parallel lines 2 mm apart at its own screen angle
    (0°, 45°, 90°, 135°) over the whole mask, with zigzag excursions where its colour is
    needed (white areas 88 needle points per cm², coloured areas 355–426), 14 133 stitches.
    OpenStitch's Lines style sews every colour along horizontal rows (7035 stitches). This is
    #129's model; noted on #129. Measured in detail on 2026-10-30 with the new
    `tools/screencmp.py`: each line is a straight 2.4–2.5 mm run where its colour is not
    needed (about 20 needle points per cm² per thread) and a zigzag 1.8 mm deep advancing
    0.2 mm per stitch where it is (about 300 per cm² in the thread's own area); one jump per
    thread, no trims. The model, the acceptance criteria and the patent facts are in
    docs/research/photo-stitch-2-halftone.md. Built 2026-10-30 after the owner's "Go ahead"
    (`screen.rs`, a fixed angle set, no image-derived direction): 13 701 stitches against
    14 133, angles, spacing, own-area (280–292 / 296–298) and white (21–24 / 21–23) densities
    and one jump a thread as PE. Second round of captures (2026-10-30: the four named Color
    Sets, Direction 30°, Line interval 3 / Max. Density 2.5, Mono, Auto Select on a two-colour
    image):
    - Angles follow the sewing position in every file.
    - A named set is four fixed threads sewn in order; OpenStitch now offers `colorSet`.
    - Direction turns the whole screen.
    - Interval and density act as modelled: 6666 stitches against 6842.
    - Mono follows a tone ramp from 17 % to 86 % darkness, now OpenStitch's too: 4671 against
      4665 stitches, every grey band within 6 % but 150–179.

    Left (open):
    - The mixing law within a set, now narrowed by the colour-ramp captures (hue 0–300° through
      CMYK and RGBK). PE sews the most-used thread first, which OpenStitch now does too: every
      set's thread order matches on the shapes and the ramp. A saturated colour's chromatic
      threads sum to about full coverage, which OpenStitch now applies over its least squares
      (`screen::saturate`). The named sets moved from −8 to −15 % stitches against PE to −1.8
      to −6.4 %. Left: the exact transitions between neighbouring colours, for example the
      CMYK ramp's red, which PE splits evenly between yellow and magenta (−11 % stitches).
    - Auto Select's thread choice. PE always picks four threads; OpenStitch reduces to the
      image's colours, so where PE mixes green from blue and yellow, OpenStitch gives green its
      own thread. Round 5 (eight test images, 2026-10-31) shows the picks come from the 29
      Ver4 Option colours. Seven of the eight are PE's CMYK set (70, 90, 89, 75) with navy or
      purple swapped in. Neither a least-squares mixing optimum (PE's set ranks 62nd to
      6530th) nor a nearest-colour mapping (13 of 32 picks) reproduces them. Round 6's 23
      single-colour swatches show the image's nearest Ver4 colour in the set (14 of 20) and
      fillers from a short list (84, 83, 91, 81). The best simple rule still matches only 68
      of 124 picks, so OpenStitch's Auto Select is unchanged
      (docs/research/photo-stitch-2-halftone.md).
- **Sewing-machine stitch files (PMV, 2026-10-29, #130):** OpenStitch now reads the PMV layout
  it writes (pyembroidery's description: 8-byte magic, 36-byte banner, 56-byte preamble, needle
  count and its double, 6-bit relative x / 5-bit absolute y pairs in 0.25 mm, the length and
  width tables, an 18-byte trailer; docs/FILE_FORMATS.md → PMV) and imports it as a decorative
  stitch. Not yet compared with files PE-DESIGN's Stitch Design Factory writes: the owner-made
  samples (2-point and 4-point stitches, one re-saved) are requested from the reference round;
  the reader refuses any other preamble or table shape until they pin it. PMP and PSF stay
  Blocked.
- **Programmable Stitch Creator sources (2026-10-29, #131):** OpenStitch imports the user's
  own `.pas`, `.pmf` and `.plf` text sources (the `@DIRECTIVE` layout: `@SYSENV` arity gives the
  role, geometry in a 0…1024 cell; docs/FILE_FORMATS.md → Text pattern sources) into
  `openstitch.pattern` fill, motif and decorative patterns, which sew with the design's own
  tile and motif sizes. Not yet pinned: the physical size PE-DESIGN gives the `@SYSENV`
  numbers and how it tiles a programmable fill from such a pattern (owner-made Programmable
  Stitch Creator files and the fills sewn from them are requested from the reference round).
  Brother's bundled patterns are never read.
- **Decorative fill with the owner's free-line pattern (reference, 2026-10-30, #131):**
  PE-DESIGN 11.50's Decorative Fill of `psc-deco-1line.plf` at 100 mm on a 40 mm square
  (`deco-fill-1line-100mm`) centres the tile on the region and sews the line corner to corner in
  1.95 mm stitches, travelling along the outline at 2 mm. OpenStitch imported such a `.plf` as a
  fill tile, so Decorative Fill could not use it and sewed a bundled pattern instead; it now keeps
  the lines free (`freeLines`) and sews the same needle points on the line, the same travel pitch
  and tie-in. Left: PE-DESIGN sews the line three times and walks parts of the outline two or
  three times (292 needle points against 73). The off-centre rectangle (`deco-fill-1line-offcentre`,
  2026-10-30) settled the placement: the tile is centred on the shape's bounding box, which
  OpenStitch does (the same 28 needle points on the line). Still open: the repeat rule (the line
  three times, the outline walked whole: 357 against 83 there), and a two-line tile whose lines
  fall outside the shape (`deco-fill-2lines`, parked: PE-DESIGN walks the border, 245 needle
  points, OpenStitch sews nothing).
- **CSewSeg drops trims:** not a difference (reference, `pes-csewseg-trims`). `CSewSeg` has
  no trim type: PE-DESIGN 11's own Ver.6.0 file stores a trim as a plain jump segment, as
  OpenStitch does, and keeps the trim through the PEC section's `0x20` flag.
- **PES writer:** versions 1, 4–10 (#374, 2026-11-07) and `#PES0110` (item 5); version 6 stays
  the default.
- **Wide satin:** not a difference. PE-DESIGN 11.50 keeps region satin crossings whole up to
  30 mm (`machine-satin-width-16` … `-30`), and so does OpenStitch; a satin is split only with
  its Split Stitch (`splitLength`). `tools/sew_check.py` warns on stitches over 12 mm (#460).
- **Feathered edge:** the guide's exclusions apply since 2026-10-27: not with gradation, Auto
  direction, hole sewing or text (refused when set, not sewn when a file holds them anyway), and
  the default width is PE-DESIGN 11.50's 3.0 mm. The row-end pattern is PE-DESIGN's
  (satin-feather), and since 2026-11-08 (#685) the side (start, end, both) is PE-DESIGN's too.
  Still different: programmable fill (PE offers it; OpenStitch refuses it until it sews it).
- **Gradation:** the pattern is PE-DESIGN's since 2026-10-27: Max/Min density with evenly
  spaced step values (default Max 53 %, Min 5 %, 9 steps), the density falling linearly across
  the region, and rows at equal steps of the cumulative line count: the region holds the
  nearest whole number of gaps to the line count the profile integrates to (a flat pattern
  rounds down like a plain fill), the first row on the start edge and the last on the far
  edge. Satin takes the same rows as its lines (satin-gradation, 2026-10-28, needle for
  needle: 37 stitches as PE-DESIGN's, mean needle distance 0.01 mm): a satin split at its start
  point puts the crossings of each half on every other line counted from that half's own end, a
  hop of one line and a crossing close the half, and the "7.6 mm travel" the parked file showed
  is the diagonal crossing of the widest gap. The same lattice rule gives the uniform split
  satin its 0.3 mm hop (satin-zigzag-rect, unchanged). At the minimum density (0.1 lines/mm,
  sweep-satin-density-min, 2026-10-28, needle for needle) PE-DESIGN puts a crossing on every
  line, 10 mm apart with 11.7 mm diagonals, where counting forward from the end by two lines
  would space them 20 mm with 20.9 mm diagonals. The round-3 column at 0.2, 0.3 and 0.5
  lines/mm (43, 47, 55 stitches, record for record) settled the rule: each half hops to the
  last lattice line at or before the start point (on the start point's own line the crossing
  lands on the start point) and puts its crossings on every other line counting back from the
  hop, plus the far end's line (lines 0, 1, 3, 5, ...); at 3 lines/mm (satin-zigzag-rect) and
  on the gradation rows the same rule gives the crossings OpenStitch already sewed. No period
  limit exists. Two edges of the rule: a gradation row within 0.05 mm of the start point
  (satin-gradation's row 12, 0.02 mm before it in OpenStitch's lattice, on it in PE-DESIGN's
  0.1 mm file) is the second half's hop only, the first half hops to the row before it, as
  PE-DESIGN's file shows; and on a feathered satin a single-line gap next to the far end
  shorter than 1 mm is not crossed, the crossings before the hop spread evenly instead
  (satin-feather at 3 lines/mm: 15 crossings 0.69 mm apart over 29 lines, then the 0.33 mm
  hop). **Settled 2026-10-29 (#122, pe-refs-4's `satin-length-sweep-d4p5/-d1p0/-d0p2`: twelve
  3 mm wide columns 0.5–15 mm long at 4.5, 1.0 and 0.2 lines/mm, read column by column):** plain
  satin has no minimum-crossing threshold, the single-line gap next to the far end is crossed
  at every size measured (0.22, 0.33, 1 and 5 mm), so the spread is a feathered-edge rule
  only. The lattice is stretched to `length × density` rounded half down equal gaps (15 mm at
  4.5 lines/mm: 67 gaps of 0.2239 mm, PE's 14.8 / 14.3 where a nominal lattice gives 14.7 / 14.2;
  1.5 mm: 7 gaps of 0.214 mm in pe-refs-4's `satin-length-sweep-noopt-d4p5`; every earlier satin
  reference has a whole `length × density`). A column whose start point is at one end is
  sewn from its far end back to the start point: the opening row across and back, a straight
  travel inside to the far end's other corner, then the far end's row and every other line
  counted back from the start point's line, every crossing towards the start point's side
  (a 3 mm column at 1 line/mm, needle for needle: rows 3, 2, 0 mm); regions without a Running
  Stitch Path (files saved before it existed) keep their old order, as their travel would be a
  jump. A first stitch of 3.0 mm already takes the three-step tie-in (bound 2.65 mm, between
  PE's 2.3 and 3.0 mm). pescmp on the three sweep files (stitches ours / PE, coverage IoU) before
  → after: 4.5 lines/mm 358 → 438 / 424, 0.919 → 0.965; 1.0: 150 → 232 / 216, 0.693 → 0.779;
  0.2: 98 → 190 / 184, 0.444 → 0.690; `satin-length-sweep-noopt-d4p5` 205 / 202, 0.952 (the
  residue: PE starts the 12 mm column 0.5 mm down its edge, a split satin, and sews columns under
  1 mm differently from one file to the next).
  region-travel-u-gradation passes at the reference tier; all 40 of its rows lie within
  0.12 mm of PE-DESIGN's (0.04 mm on average; the earlier midpoint-density placement drifted
  to 0.4 mm by the far edge), and the round-3 Max/Min sweeps (80/20, 95/50, 50/50, pattern 2)
  fit the same rule within 0.1 mm. Designs saved with the old single far-side percentage
  still sew a continuous fall from 100 %. The guide's exclusions apply: not with a feathered edge, Auto
  direction or text (refused when set, not sewn when a file holds them anyway). Settled by the
  2026-10-28 gradation sweep (references `u-gradation-*.pes`): Max/Min rescaling (80/20, 95/50, a
  flat 50/50) and the mirror pattern 2 (scripted as Max 5 / Min 53) and Direction 90 are at the
  reference tier within the 15 % count tolerance; the dialog's Direction is the region's own
  Direction field; the Max slider stops at 95 %, Min at 5 %; the eight presets are per-step shapes
  (pattern 3: 53/42/32/24/17/12/8/6/5 %, 4: 5/6/8/12/17/24/32/42/53, 5: 53/52/50/46/41/34/26/16/5,
  6: 5/16/26/34/41/46/50/52/53, 7: 5/26/41/50/53/50/41/26/5, 8: 53/32/17/8/5/8/17/32/53) and Blend
  sews two colour blocks (second colour first); their specs check only the routing until the model
  has per-step values and Blend. Raised: without under sewing PE-DESIGN sews 434 stitches on the U
  where OpenStitch sews 366 (`u-gradation-nous.pes`, parked; on a plain 40 × 40 mm square without
  under sewing the gap is within tolerance: `square-gradation-default.pes` 673 / 644 and
  `square-gradation-30-10.pes` 489 / 461 at the reference tier, so the U's legs carry most of it): the under-sewing layer hides a
  difference in the gradation rows themselves. Still missing: per-step editing, Blend, the
  gradation direction arrow, and programmable fill (PE offers it; OpenStitch refuses it until it
  sews it).
- **Over-long moves in PEC (2026-10-29, #126):** the PEC writer split a move over ±2047 units
  into equal pieces computed on unrounded coordinates, so a half-unit fraction could still
  encode a 2048 delta (masked to 12 bits). It now splits on the rounded coordinates with the
  rule PE-DESIGN 11.50 shows in its DST for the 211 × 278 mm move of `long-jump-300.dst`
  (pe-refs-4): the maximum along the longer axis, the proportional share on the other,
  remainder last, never halved (the issue proposed halving). PE-DESIGN's PEC bytes for that
  move (`9e 39 96 95` then 39 × `80 46 00`) do not decode to the DST's vector with the 12-bit
  rule, so its PEC encoding over 204.7 mm stays open and the DST rule is applied at the PEC
  limit; internal regression tier (`pec_splits_moves_over_2047_units_like_pe_design_dst`).
- **Thread-colour matching (2026-10-29, #119):** nearest-chart matching (PEC colour indexes,
  JEF/SEW/HUS/PCS charts, image digitizers, `threads.remap`) used a red-mean weighted RGB
  distance; it is now equal-weight squared Euclidean (ΔR² + ΔG² + ΔB²), and a second
  "approximate" matcher keeps a city-block minimum beside it and returns the city-block winner
  (`openstitch_core::approximate_index`), for the built-in sew palette. Checked against the
  owner's twelve-swatch capture (`compat/reference/pes/thread-match-swatches.pes`, PE-DESIGN
  11.50, 2026-10-29: twelve RGB fills imported from an SVG and the Brother chart entry PE
  picked for each): squared Euclidean against chart entries 1–61 reproduces all twelve picks
  (`pe_design_swatch_picks_are_reproduced`); red-mean missed blue (21 Ultramarine for 405) and
  cyan (53 Light Blue for 019); city block alone missed four. Two findings against the issue's
  text and pyembroidery's table: (1) PE-DESIGN's own thread table writes (253, 217, 222) for
  entry 57 (124 FLESH PINK) where pyembroidery has (255, 204, 204), so the chart table now
  carries PE's value (entry 42 keeps pyembroidery's identical RGB); PE picked 57 for
  rgb(240, 200, 220), so ties go to the later entry, not the earlier one the issue asked for.
  (2) rgb(255, 128, 0) went to 11 Deep Gold (2210 away) and not to 62 Orange (625 away), so
  entries 62–64 are not candidates: matching runs over the first 61 entries (#136's "basic
  only" match), and the three are kept only for files that name them. Done for SVG import since
  2026-10-30 ("Imported colours snap to the chart" below; hex fills stay exact on purpose); the
  note as written then: SVG-import and hex fills stay exact custom threads in OpenStitch
  while PE-DESIGN snaps them to the chart with Brother's catalogue code (800, 513, …); that
  needs the codes for all 61 entries, known since the round-4 chart reading (see below), and goes with
  the palette work (#136). The machine-format references (`machine-formats`, red and black)
  pick the same entries under both metrics.
- **PEC colour indexes block by block (2026-10-30, #119, #136):** PE-DESIGN 11.50's own file of
  the 29 Ver4 Option colours (`tests/fixtures/pe11/ver4-option-color-29.pes`, reference
  session) gives every colour block the plain nearest PEC entry among 1–61: 32 five times, 7,
  14, 25 and 37 twice each. OpenStitch's PEC writer gave each distinct colour a still-unused
  entry instead (so distinct design colours stayed distinct on the machine); it now writes the
  plain nearest entry per block, and the fixture's 29 indexes are reproduced
  (`each_block_gets_its_nearest_chart_entry_as_pe_design_writes`). No spec changed. The same
  file showed the `#PES0110` thread-table reader taking a false, shorter table that starts in
  the middle of the real one (11 records of 30); it now keeps the longest table that parses.
- **Thread-chart CSV that PE-DESIGN imports (2026-10-31, #136):** PE-DESIGN 11.50's Import Chart
  (Option ▸ Options… ▸ Edit User Thread Chart) does read a CSV, "in the order of the Code,
  Brand, Description, R, G, B", OpenStitch's own column order (round-6 check by the reference
  session, files in the owner's `round6/charts`). It refused OpenStitch's export for its `#`
  comment line, imported a column row as a black colour, split quoted fields at their commas,
  showed a lettered code as 000, imported all 1,500 rows of a 1,500-row file, and refused the
  issue's `#userchart` layout. `threads.exportCsv` now writes bare records only, with a comma
  in a field as `;` and a quote as `'`, and PE imports OpenStitch's own exports (the Brother
  chart, the sew palette and a user chart, checked the same day; PE stores an empty code as 0
  and drops punctuation such as `;`, `.` and `'` from the text fields); the
  reader still takes the earlier comment line, column row and quoting. The earlier finding
  below, that charts cannot be exchanged with PE-DESIGN through files, holds for PE-DESIGN's
  own `.btc2` charts only (there is still no export from PE).
- **Thread-chart CSV and the built-in palette (2026-10-29, #136):** PE-DESIGN 11.50 has no
  thread-chart CSV (round-4 check: Options ▸ Edit User Thread Chart only imports; user charts
  are binary `.btc2` files, 89 bytes of high-entropy data for one colour, in the program
  folder), so the issue's `#userchart` schema, 6-character code and 1,499-colour cap have no
  PE-DESIGN counterpart. OpenStitch documents its own CSV (FILE_FORMATS.md "Thread chart
  CSV": the six columns it already imported, plus a column row and RFC 4180 quoting) and adds
  `threads.exportCsv` for user and built-in charts and `threads.builtinCharts`, which marks
  Brother entries 1–61 as the ones PE matches to (#119). Charts cannot be exchanged with
  PE-DESIGN through files; `.btc2` is not decoded (obfuscated, kept in the program folder).
  The 94-entry sew palette is implemented since 2026-10-30 (`openstitch_core::sew_palette`,
  `threads.builtinCharts` `sewPalette`): PE-DESIGN 11.50 has no single 94-row list, but its
  "Ver4 Option Color" chart has exactly 29 rows, and its own file of them
  (`ver4-option-color-29.pes`, reference session) writes ids 66–94, type `0x0B`, brand
  "Option Color" and RGB on a 0/85/170/255 grid: 61 basic + 4 reserved (shown nowhere) + 29,
  as the issue said. The `#PES0110` writer writes an Option Color thread with that record
  (id string 66–94, empty code and chart, type `0x0B`; `as_pe_writes`,
  `option_colours_are_written_with_their_sew_palette_ids` against PE's own file) instead of the
  nearest PEC index it gave before. The matcher (`sew_palette_index`) follows the issue's dual rule and is
  **not verified against PE-DESIGN**: no capture shows where PE-DESIGN matches colours to this
  palette (its vector import does not: the swatch capture sent rgb(0, 0, 255) to 405 BLUE, not
  to the identical Option colour blue2). No default path calls it, so no output a PE
  reference backs changes; it is an API for callers that ask for the palette. PE-DESIGN's
  Auto Punch does match to Ver4 Option Color when it is chosen as the Thread Chart (reference
  session, 2026-10-30, a 4-colour image of rgb(20,20,180), (100,200,60), (200,40,120),
  (240,160,40) on white, default parameters): it picked 66, 69, 85, 84. Squared Euclidean,
  city block, the issue's dual rule and red-mean all give 66, 81, 85, 89; CIE76 Lab gives 66,
  69, 91, 84. The same image with BROTHER EMBROIDERY gave 406, 509, 107, 208 (PEC 21, 37, 51,
  12) where squared Euclidean gives PEC 21, 37, 49, 52. So Auto Punch changes the colours
  before matching (its colour reduction or segmentation), and these picks neither confirm nor
  settle the palette rule; the issue's dual rule does not reproduce them. Left for the image
  track: Auto Punch's colour reduction.
- **Readers against PE-DESIGN's ten-format export (2026-10-29, #135):** pe-refs-4's
  `sample-basic` (a satin S and a fill, exported to every type) found four reader problems.
  (1) PES "drops PE's tie-in records" (471 needle points against PE's 475): not a reader
  bug. That sample's PEC section has none of the 4 zero-length stitches its DST has (2 jump
  landings, 2 repeats; the other 471 points equal), and the status bar counts 1–9 more than
  the PEC in 99 of 114 references: the points are not in the file. (Corrected 2026-10-30: PE's
  PEC writer does not drop zero-length stitches as a rule; 28 of 249 PE references carry them,
  so OpenStitch's PEC writer keeps its own and the "drop them" follow-up was not done.) (2) CSD read as
  2 stitches and 51 colour changes: the mask index was the byte offset minus one; with the
  offset itself PE's CSD equals its DST needle for needle, with PE's colours and colour order
  (fixture `sample-basic.csd`, reference-checked). (3) SHV read an extra colour change at the
  end: pyembroidery starts the records 2 bytes before the colour table's end, PE-DESIGN at the
  end; the reader now follows the word counts (fixture `sample-basic.shv`). (4) Importing PE's
  DST merged its two colour blocks: a DST has no colours, so the stitch block got none and the
  blocks were merged as one colour; each block now gets its own Brother chart colour on import.
  The ten-format sample is now spec `machine-sample-basic` (2026-10-30, reference tier): PE's
  PES is imported as stitches and the eight writable formats are compared with PE's exports
  needle for needle. Three writer fixes made them equal: the design centre floors on an odd
  extent; DST pads the way back to the origin to a trim run only when it is one record; and a
  PE-DESIGN `#PES0110` import gets back the stitch at each jump landing that PE's PEC leaves
  out, which the folding formats (JEF, SEW, VIP, XXX) fold onto. The compare drops zero-length
  stitches on both sides (`dropRepeats`) and the report lists how many per side (section
  "Zero-length stitches left out by `dropRepeats`" of the summary, `droppedRepeats` in
  report.json). For this spec, PE / OpenStitch: DST 4 / 2, EXP 4 / 2, PCS 5 / 3, HUS 6 / 4,
  VIP 0 / 0, JEF 0 / 0, SEW 1 / 1, XXX 2 / 2, SHV 0 / 0, CSD 0 / 0 (SHV and CSD since
  2026-11-04, #320: OpenStitch now writes both, and PE's CSD read and written again comes out
  byte for byte, PE's SHV likewise but for the preview's pixels; PCS and HUS were 5 / 1 and 6 / 2 before their
  writers wrote the landing twice, 2026-10-30). Every repeat OpenStitch writes is at a point where
  PE writes one too (the jump landings, SEW's and XXX's entry points); none is OpenStitch's
  alone. The ones only PE writes:
  - two repeats inside the S's satin, at (17.9, 37.9) and (25.9, 31.9) mm from the design's
    minimum corner, in DST, EXP, PCS and HUS: in PE's own stitch data, absent from its PES, so
    no OpenStitch import can have them;
  - fixed: PCS and HUS write each jump landing twice (the move's own zero-length record, then
    the design's landing stitch) where OpenStitch wrote it once; both writers now keep the
    design's landing stitch after their own (in `machine-formats`, a stitch file PE only opened,
    there is no landing stitch to keep, and nothing changes). What is left in every format is
    the two repeats inside the S.
  The "trim before the colour change" the round-4 notes saw does not occur in this sample.
- **PEC 62–64 at PE-DESIGN's RGB (2026-10-31, #136):** PE-DESIGN's own `#PES0110` files
  carry each colour block's RGB in the colour list after the PEC section, as R, G, B, on a
  Hoop Size page and in re-saves of version 6 files (73 files; a Custom Size page leaves it
  zero, 385 files). The five
  Applique Wizard files (a01–a05, round 4) give PEC 62 (material) (255, 200, 100), 63 (position)
  (255, 200, 150) and 64 (appliqué) (255, 200, 200), where pyembroidery's table had (255, 153,
  0) and (255, 240, 141) for 62 and 63; the chart now has PE's values
  (`pe_design_applique_colours_match_pec_chart_62_to_64`). Matching still never picks 62–64.
  PE's cutting line is PEC 65 at (220, 220, 220), outside the 65-slot chart. Not yet as PE does
  it then (fixed the same day, below): OpenStitch's appliqué stages keep the design colour, so its PEC gives them the nearest of
  1–61 where PE writes 62–64, and its writers put the RGB in that list as B, G, R with `0D` codes
  for every block (PE: `FA`/`FB`/`FC FF FF` for the appliqué colours, `F9` for a cut, `FE FF FF`
  for a thread with no number).
  PE's cutting line is PEC 65 at (220, 220, 220), outside the 65-slot chart. The Appliqué wizard's
  stages take PE's appliqué colours since the same day, and since the same day the wizard puts
  its lines where PE's a01–a05 have them (Add: the covering's centre d + w / 2 + 0.3 mm out,
  the position line w / 2 − 1 mm beyond it, material and tack-down 0.5 mm further; Replace:
  the covering on the outline) and, in Add, sews the pattern first in one group with its
  appliqué, as PE does: OpenStitch's export of a01's design now has PE's object order, group
  and line positions (32.6 mm across, PE 32.6 × 32.8; it was 29 mm with every line at the
  distance, the pattern last and outside the group). Its stitch count still differs (1,735
  against 1,063). The wizard's stages
  take PE's appliqué colours: material 62, position 63, and 64 for
  the tack-down and, unless a colour is given, the covering stitch, so those two sew as one
  block as in PE's files; the PEC keeps a Brother chart entry's own index, so the stages are
  written as PEC 62, 63, 64, and OpenStitch's export of a01's design has PE's PEC colour list and
  colour list after the PEC except for the order (the square sews last in OpenStitch, first in
  PE: the `#PES0110` writer's sewing order). Before, every stage took the pattern's colour, the
  PEC gave them the nearest of 1–61 and the cover was a fourth block. A cutting material line
  stays on the cutwork needles; PE writes its cuts as PEC 65. The `#PES0110` writer writes that list as PE does since the same
  day (R, G, B on a Hoop Size page, zeros on a Custom Size page; `0D` + the BROTHER EMBROIDERY
  number, `FA`/`FB`/`FC FF FF` for the appliqué colours, `FE FF FF` for a thread with no number;
  checked against every numbered block of PE's own files by
  `colour_list_after_the_pec_is_written_as_pe_design_writes_it`); it wrote B, G, R and `0D`
  codes for every block before. The version 6 writer keeps pyembroidery's B, G, R and no codes.
- **Brother catalogue codes (2026-10-29, #119, #136):** pe-refs-4 read PE-DESIGN 11.50's BROTHER
  EMBROIDERY chart from Sewing Attributes ▸ Color: 61 rows, codes 001 WHITE to 900 BLACK, brand
  EMBROIDERY, no row for PEC 62–64 (so the 61-entry match set of #119 is PE's own chart).
  Matched to the PEC chart by name, every code lands on one PEC entry 1–61 and all 30 pairs PE
  writes in the owner's `#PES0110` thread tables agree. pyembroidery's table names PEC 42 and 57
  both "Flesh Pink"; PE's files put 124 FLESH PINK at 57, so 307 LINEN is 42, and the chart
  table now calls 42 "Linen", with PE's RGB (254, 227, 197) since 2026-10-30 (the reference
  session's 307 LINEN sample; every coded thread in PE's own files now matches the chart,
  `pe_design_thread_tables_match_the_pec_chart`). The dialog's own
  first column (1 BLACK, 2 WHITE, 3 RED, …) is not the PEC index. API:
  `openstitch_core::BROTHER_EMBROIDERY`, `brother_embroidery_code`, `pec_index_of_brother_code`.
- **Contour fill (2026-10-30, B1):** an OpenStitch extension, region type `contour`. It
  reuses PE-DESIGN's Spiral Stitch's offset rings but no reference spec sets it, so every
  PE-DESIGN spec is unchanged; its tests are internal regressions. A design using it opens in
  PE-DESIGN as plain stitches. Label it "Implemented (beyond PE-DESIGN)" in FEATURE_MATRIX.md.
- **Imported colours snap to the chart (2026-10-30, #119):** `import.svg` gives every fill and
  stroke the nearest Brother chart thread 1–61, as PE-DESIGN's vector import does, and
  `thread-match-swatches` now imports the very SVG PE-DESIGN imported and compares the whole
  thread table (RGB, name, code, brand, chart, type) with PE's file: equal, reference tier. The
  PES 0110 writer takes the codes of all 61 entries from core (its own table had 25) and picks
  the PEC index among 1–61. Hex colours given to commands (`fill: "#rrggbb"`) stay exact on
  purpose: PE-DESIGN has no free-RGB colour field to match, and exact RGB is what PES v6 keeps.
- **WMF/EMF import (2026-11-04, #321):** `import.metafile` reads both metafile kinds as
  editable shapes like `import.svg` (colours snapped to the chart). `compat/reference` holds
  no PE-DESIGN capture of a WMF or EMF import, so PE's line/region choice and sizing for
  metafiles are not compared: OpenStitch keeps the file's own physical size. A capture
  (placeable WMF and EMF of a known size, imported in PE-DESIGN 11.50 and saved as PES) would
  make it a reference spec.
- **Imported stitch files are centred on the page (2026-10-31, reference):** PE-DESIGN 11.50's
  Import Patterns ▸ from File (the Import pane; no dialog or option) ignores where the file put
  its stitches and centres the pattern on the Design Page: a 20 × 10 mm rectangle saved at
  (10, 10) mm lands at 40–60 × 45–55 mm on a 100 × 100 page and at 90–110 × 70–80 mm on a
  200 × 150 page, for a DST and a PES alike, and a second import lands exactly on the first
  (round-7 captures `import-*.pes`). `stitches.import` placed the stitches at the file's own
  coordinates (only the editor's File ▸ Import centred them, with `object.center`); it now
  centres the needle points' box on the page itself. Spec `stitches-import-centre`, reference
  tier for the 200 × 150 page's position and every size and count. On the 100 × 100 page PE's
  file writes the PEC stitches from the design's own corner (PEC header 20,0 × 10,0 mm) where
  its other files keep page coordinates, so those positions are pinned at PE's ruler values
  (internal tier); a machine reads both framings the same way. Open, not compared: a DST gets
  PE's 007 PRUSSIAN BLUE on import, and PE joins two imports of one colour with a stitch where
  OpenStitch jumps (785 against 784 stitches).
- **Skeleton satin and the Satin Skeleton tool:** OpenStitch extensions with no PE-DESIGN
  counterpart. Label them "Extension" in FEATURE_MATRIX.md.
- **Text fill rows (for the next text session):** since fill took PE-DESIGN's row structure,
  letters (rows with several spans, sewn in the older nearest-row order) have more stitches than
  PE-DESIGN's: text-size-h 639 against PE 483 (main before: 622), the Text Transform specs about
  +14 % (e.g. text-transform-1 1489 → 1701, PE 1300). For text-size-h the cause is that rows
  shorter than one stitch keep their edge points (`generate::chain`, `Chain::Brick`): PE-DESIGN
  drops them on a 20 mm letter's 3.5 mm strokes but keeps the narrow rows at
  hole-sewing-pentagon-star's vertices (with the cut at half a stitch or at 3 mm text-size-h is
  511, but the pentagon's stitchedArea leaves its tolerance). The Text Transform excess does not
  depend on that cut (1685 at 3 mm); likely PE's grid keeping points down to 0.3 mm from a row end
  in narrow strokes, or letters at 45° (the grid anchor of angled fills, sweep-fill-dir-30/45/135).

- **Round-4 references (2026-10-29, PE-DESIGN 11.50 driven black-box; owner's notes folder
  `round4/` holds every file, log and the parked ones):**
  - *Under sewing types (#124):* for a Satin region all five type buttons are enabled (Fill
    enables the first row only) and they are exclusive radio buttons: PE-DESIGN cannot
    combine under sewing types. On a 39.5 x 6 mm Manual Punch column: type 1 287 stitches,
    2 287, 3 226, 4 250, 5 226, off 186. OpenStitch's `edgeZigzag`, `double`, `edge` and
    `zigzag` match types 1-4 within 8 % (specs satin-col-us-type-1..4, reference tier);
    parked: type 5 (OpenStitch `edgeFill` 292 vs 226) and the column without under sewing
    (207 vs 186: the plain column is 11 % over, the under sewing itself is close); both landed
    since (item 8: 219 / 225 and 186 / 186).
  - *Running stitch on a circle (#121):* a 59.9 x 60.4 mm Ellipse-tool circle at run pitch
    2.0 sews 133 stitches; OpenStitch's Bezier halving matches (run-circle-60, reference
    tier, count, mean and longest stitch).
  - *E and V stitch defaults:* list "E Stitch" / "V Stitch", stroke spacing 5.0 mm, arm
    2.5 mm, width 2.0 mm, Run time(s) 1, Stroke time(s) 1 (greyed for V), Arrange two
    buttons; a closed 30 x 20 mm rectangle sews 105 (E) and 87 (V) stitches (specs
    line-e-rect-default, line-v-rect-default, reference tier). Tatami defaults as before:
    step pitch 4.0 mm, Frequency 30 %, density 4.5, direction 45.
  - *Over-long move (#126):* two satin blocks in opposite corners of a 300 x 300 mm page
    (Custom Size accepts 100-1000 mm). PE-DESIGN's DST writes the 211 x 278 mm move as 23
    jump records of (92, -121) units plus one of (90, -118): the vector divided evenly into
    records whose longer axis is the 121-unit maximum, never halved; the runs from and to
    the origin are 11 x (-121, 121) + (-119, 119). OpenStitch's DST agrees on the total
    jump length and the bounds (long-jump-300, reference tier). Its PES stays parked, settled
    2026-10-30: PE-DESIGN does not encode the move in the PEC at all. After the colour change
    its PEC has one jump of (-45.5, +168.5) mm to (0, 180.0) mm (`9e 39 96 95`) and then 46
    stitch records in a straight 280 mm row at y = 180 mm (39 of them the 7.0 mm long form
    `80 46 00`), where the design's second block is a 39.5 x 6 mm satin like the first. The
    block header says 290 x 190 mm; the PEC holds 233 needle points (186 + 47), the status bar
    372 (186 + 186). For a design whose moves leave the 12-bit range, PE's PEC section is a
    stand-in, not the stitches, so it cannot be a reference for a long move; OpenStitch's
    split at 2047 units (#126, as PE's DST splits) stays. Reading it gives 280 x 174.5 mm.
  - *Hoop limits (#125):* a design scaled to 129 x 105 mm on a 100 x 100 mm Hoop-Size page
    is neither rejected nor promoted to a larger hoop. Save As warns "Please keep all designs
    within the Design Page. Saved data cannot be sent to the original card or the embroidery
    machine. Do you want to save the data anyway?" and saves on Yes; Export to DST writes
    the file with no warning (hoop-oversize-export, reference tier on the DST bounds). Its
    PES (settled 2026-10-30, now a formats fixture) has an empty machine section: the PEC's
    "no colours" form and a 13-byte stitch block with only the design's size (1290 x 1050
    units) and the end byte, which is what "cannot be sent to the embroidery machine" means;
    the design is only in the object section (13 threads, `CGroupObject` and `CEmbLine`). The
    211 x 229 mm OpenStitch read was its reader decoding the preview icons as 169 stitches and
    the colour byte `FF` as 256 colours; it now reports that the PEC holds no stitches for this
    129 x 105 mm design (`pe_design_oversize_save_has_no_machine_stitches`). OpenStitch's own
    export of an oversize design still writes its stitches, with a warning (#125, #157); PE
    writes none to the PEC. An empty OpenStitch PES (same "no colours" form) now reads back
    without the 256 Sky Blue threads it used to give.
  - *Thread picks (#119, #136):* twelve 20 x 20 mm squares with chosen RGB fills imported
    from an owner SVG. PE-DESIGN mapped rgb(255,0,0) 800, (0,255,0) 513, (0,0,255) 405,
    (255,255,0) 205, (0,255,255) 019, (255,0,255) 086, (128,128,128) 817, (200,150,100)
    348, (50,100,150) 070, (240,200,220) 124, (20,80,40) 415, (255,128,0) 214 (Brother
    Embroidery codes; PEC indices 5, 14, 2, 13, 32, 25, 39, 35, 4, 57, 38, 11). OpenStitch
    keeps an `#rrggbb` fill as a custom thread with the exact RGB and no chart name, so the
    thread table is not compared yet (thread-match-swatches checks the twelve blocks at the
    reference tier and records the picks for the matcher; the whole table is compared since
    2026-10-30, "Imported colours snap to the chart" above). Thread list to CSV: Layout &
    Editing 11.50 has none (Options > Edit User Thread Chart only imports charts; Design
    Database asks for a software key).
  - *Machine formats (#135), parked:* a two-colour design (20 mm satin S in Red, 10 x 8 mm
    Prussian Blue fill) exported to all ten types PE-DESIGN offers (DST, EXP, PCS, HUS,
    VIP, SHV, JEF, SEW, CSD, XXX; no PHC, PHX, PEN or PEC; no options, no warnings). Every
    writer compare fails by the same needle points: OpenStitch's PES reader drops the
    tie-in records at a block's start (PE-DESIGN's stream starts with a plain 0.2 mm
    stitch out and back, 475 needle points against 471 read; 1 in the first block, 3 in the
    second), so the exports are 4 stitches short of PE-DESIGN's and a trim stands where
    its colour change is. Importing PE-DESIGN's DST instead keeps 475 points but merges the
    two colours into one block (`stitches.import` of a DST). PE-DESIGN's CSD read by
    OpenStitch's spec-derived reader gives 2 stitches and 51 colour changes (the reader is
    wrong against a real file); its SHV reads with one extra colour change at the end. All
    eleven files wait in the notes folder for the reader fixes (made since: "Readers against
    PE-DESIGN's ten-format export" above, spec `machine-sample-basic`).
  - *PES object kinds (#120, #131):* programmable fill, motif fill and motif line with an
    owner-drawn V pattern from Programmable Stitch Creator (`.pas` / `.pmf`: ASCII,
    `@VERSION 10000`, `@SYSENV 1`, `@TILE 1` + `@TILELINE 0 3 ...` or `@LINE 3 ...` in a
    0-1024 cell, Default Size 7.5 mm) read and written back with their stitches
    (pes-objects-owner-patterns, reference tier; the files are also `pe11` fixtures). With
    round 3's satin, fill, running, text and applique files every editable kind has a
    sample.
- **Round-4 batch 4 (2026-10-29, PE-DESIGN 11.50 black-box):**
  - *Tatami stagger (#123):* PE-DESIGN has no random stagger. Its only stagger controls are
    Frequency (0-50 %, default 30) and the two Stitch Type buttons; on a 40 mm square at the
    defaults they give 2068 and 2054 stitches (tatami-40-stitch-type-1/-2, reference tier).
  - *Density floor (#139):* the Density field accepts 0.1 lines/mm; 0.05 and 0 become 0.1,
    0.15 rounds to 0.2, 20 becomes 14.0. The floor is 0.1 lines/mm with one decimal.
  - *Satin length sweep (#122):* twelve 3 mm wide satin columns 0.5-15 mm long at 4.5, 1.0
    and 0.2 lines/mm. 4.5 and 1.0 pass at the reference tier (satin-length-sweep-d4p5/-d1p0);
    0.2 is parked (OpenStitch 145 stitches against PE-DESIGN's 184). PE-DESIGN's needle points
    per column are in the MANIFEST entries for the satin track.
  - *Optimize Sewing Order (#137):* on six scattered squares in three colours it changed
    nothing (identical stitch streams before and after); PE-DESIGN groups same-colour
    objects into one block in first-occurrence order (sew-order-six-objects).
  - *Concentric and spiral fills (#138):* the Region sew pane offers no Direction field for
    Concentric Circle or Spiral Stitch, only Density (default 2.0 lines/mm), Run pitch (2.0 mm)
    and a two-button Start Point choice (the winding).
  - *Pattern sources (#130, #131):* Programmable Stitch Creator writes one type per mode:
    `.pas` (Fill/Stamp, Default Size 7.5 mm), `.pmf` (Motif, 5.0 mm), `.plf` (Decorative Fill,
    100.0 mm, `@SYSENV 10000 10000 1`). A closed square is written with its closing point; two
    separate lines are two `@TILELINE` records with nothing marking the jump. Six minimal
    sources and four designs that use them read and write back (pes-objects-psc-patterns).
  - *Thread charts (#136):* a user chart is stored as `MyChart\<name>.btc2` in PE-DESIGN's
    program folder, 89 bytes of high-entropy binary for one colour: no CSV.
  - *Image wizards (#127, #128, #132):* Auto Punch, Cross Stitch and Photo Stitch 1 at their
    defaults on an owner-made image are fixtures in
    `crates/openstitch-digitize/tests/fixtures/pe11-wizards` with every default recorded.
  - *Not recorded:* Font Creator and Stitch Design Factory output (#134, `.pef`, `.pmv`); the
    owner declined screen-control access to both programs.

- **Fidelity (2026-10-31, B16.1):** a `fidelity` parameter (`standard`, the default, or `high`) on Auto Punch and Photo Stitch, accepted only with `compatMode: openStitch`; PE-DESIGN-compatible mode rejects `high` and is unchanged (compat 443/443).
- **Platform-dependent arched text (found 2026-10-29, fixed 2026-10-30):** the same build measured the PES 0110 Text Transform spec's arch height as 26.0 mm on Windows and macOS and 25.8 mm on Linux CI (PE-DESIGN 11.50: 26.5 mm). Cause: stitch generation called the platform's C math library (`f64::sin`, `cos`, `atan2`, `hypot`… go to glibc, the MSVC runtime or Apple's libm, which differ in the last bit for some arguments), and on the arch a last-bit difference moved a fill row across its threshold (rows 0.2 mm apart at density 5). Every transcendental call in `openstitch-design` and the engine's stitch paths now goes through `openstitch_design::fmath::Det` (the `libm` crate, a pure-Rust port of musl's libm, already in the tree via rustybuzz), so a design sews the same bits everywhere; a source scan (`fmath::tests`) keeps the platform functions out, and `crates/openstitch-engine/tests/determinism.rs` pins the arched text and a rotated satin ellipse to bit-exact fingerprints on all three OSes. Windows output is unchanged (every compat spec, both goldens). PE-DESIGN's re-save (`pes-0110-editable-text-transform.pes`) is back in the repository with its MANIFEST entry: 26.0 mm against 26.5, within the spec's unchanged 0.5 mm.
- **Image digitizing on every platform (2026-10-31, B16.1):** `openstitch-digitize` (Auto Punch, Photo Stitch 1 and 2, Cross Stitch, Design Center, the colour maths in `perceptual.rs`) also called the platform's `sin`, `cos`, `atan2`, `hypot`, `exp`, `powf` and `cbrt`; all 33 calls now go through `openstitch_design::fmath::Det`, and a source scan in the crate (`tests::the_crate_uses_no_platform_transcendental_functions`) keeps them out, so an image converts to the same stitches on Windows, macOS and Linux. No compat spec changed. Golden hashes pin it: `tests/golden.rs` converts three small images (two PNG examples and a synthetic gradient) with Auto Punch, Photo Stitch 1 and Photo Stitch 2 in OpenStitch mode and compares a hash of each result, in the cargo test matrix on macOS, Windows and Linux; identical on macOS arm64 and x86_64 when pinned.
- **Thread selection for the image (2026-10-31, B16.1 T4):** OpenStitch mode only. Auto Punch (and, until its fine rows T2/T3, Photo Stitch 1) chooses its threads from the PEC chart by backward elimination over the image's colours (`perceptual::select_threads`) instead of snapping k-means centres to their nearest thread, where centres collapsed onto one thread (ten asked, seven or eight sewn on the photos). PE-DESIGN-compatible mode, its specs and the PES/PEC chart matching are unchanged (compat 443/443). Photo Stitch 2's Auto Select keeps its palette: nearest-colour selection lowered three of its six cases (the line screen mixes each thread with the fabric), so its threads wait for its own rework (T13).
- **Thread sources and other brands' charts (2026-10-31, B16.1 T5):** OpenStitch mode only, and only when a `threads` source other than `brother` is asked for. `openstitch_core::brands` adds five charts compiled from the manufacturers' published colour values; the PEC chart, its codes and the matching used by the PE-DESIGN-compatible paths and the PES/PEC writers are unchanged (compat 443/443, golden hashes unchanged).
- **Dithering only where the image is tonal (2026-10-31, B16.1 T6; superseded for Photo Stitch 1 by its fine rows, T2/T3):** OpenStitch mode only. Photo Stitch 1 dithered every cell, and on its 3 mm cells the dithering read as noise (the high-key p11 scored 62.0 against PE-DESIGN-compatible mode's 75.5). A tonal map (`palette::tonal_map`) now sews flat cells in their nearest thread and dithers only texture. PE-DESIGN-compatible mode, its specs and its RGB dithering are unchanged (compat 443/443).
- **Cross Stitch OpenStitch mode (2026-10-31, B16.1 T14):** `digitize.crossStitch {compatMode: openStitch}`. PE-DESIGN-compatible mode is unchanged (443/443 specs). A cell takes its mean CIELAB colour, not a vote of modal colours, matched to the census's perceptual palette; crosses are filled (`fill`): each cell's diagonals become bands of parallel lines 0.3 mm apart, sewn as zigzags along each row run, because a single cross of 0.4 mm thread covers about 45 % of a 2.5 mm cell and the fabric then dominates its colour (exact colours with single crosses score 59 on the pixel strawberry, filled 86, the colour blocks 87). Lines per cell drop where a run would exceed its budget of needle points per mm² (short runs pay for their clipped ends): 2.8, stepped down by 0.2 to 2.0 until the whole design holds at most 2.05 per mm², under the owner's limits of 2.1 over the design and 3.0 in any 10 × 10 mm window (2026-10-31). Measured with `tools/image_similarity.py` (flat render): seven images 51.3 → 70.2, strawberry 59.1 → 82.5; design means 0.42–1.94 points/mm², densest 10 × 10 mm window 2.49 (counted on the exported DST), shortest stitch 1.12 mm. Edge part-crosses (`partCrosses`) are an option, off by default (owner, 2026-10-31): +0.0 to +0.4 on the colour-block ceiling, +0.9 on the seven product cases (70.2 → 71.1), within the same limits.
- **Design Center OpenStitch mode (2026-10-31, B16.1 Phase 5):** `digitize.designCenter {compatMode: openStitch}`. PE-DESIGN-compatible mode is unchanged (443/443 specs); `center::sew` is `sew_with` in PE-DESIGN-compatible mode and no public function changed its behaviour. The threshold comes from the image by Otsu's method (it made no difference on the two clean test drawings; it finds grey-on-grey drawings the fixed 128 misses). The larger change is the line sewing: one running stitch draws every line 0.4 mm wide, so the butterfly's 1 mm outlines scored 70.3. Zigzag at the stroke width scored 96.0 but put 3.09 needle points/mm² in its densest 10 × 10 mm window and 2,179 stitches under 1 mm; at a density within 2.1/mm² it fell to 92.9. Bold strokes (median ≥ 0.6 mm) now sew as an even number of parallel running lines across 1.3 × the median width, one path per line with its points at least 1 mm apart: butterfly 95.2, densest window 1.89, 53 stitches under 1 mm (PE mode 242). Thin strokes sew as triple running stitch on the thinned line: teapot 95.1 → 96.4. Region threads are matched by CIEDE2000.
- **Photo Stitch 1 fine rows (2026-10-31, B16.1 T2, T3, T10, T9; OpenStitch mode only):** Photo Stitch 1 sampled one colour per 3 mm Run Pitch along a row, the largest loss the image-similarity plan measured (an exact-colour fill at 3 mm scores 76 against 95 at 0.5 mm on the portrait). In OpenStitch mode, in colour, it now samples every 0.25 mm and lets a stitch end where the thread changes (`rows.rs`, `photo::fine_runs`), holds the owner's caps (2.1 needle points/mm² over the design, 3.0 in any 10 × 10 mm window, counted on the sewn stitches including every trimmed jump's tie-in stitch, about a tenth of the points), weighs errors by a saliency map, and finishes with a local search against the image as the eye blurs it (`optimise.rs`). The nine photos score 55.8 → 73.6 (`travel: allTrims`) or 71.9 (`fewerTrims`, the default; PE-DESIGN-compatible mode 58.7). Found while measuring: the design compiler sews a jump of up to 4 mm inside a stitch object across as a stitch, so the raw stitch block the scorer drew was not what the machine sews; the fine rows plan for it (no compiled stitch under 1 mm). Left: the 85 % target (gap 5.6–15.7 points a photo), trims with `allTrims` (1.5–7× PE-DESIGN-compatible mode's). PE-DESIGN-compatible mode and its 443 specs are unchanged.
- **Photo Stitch 2 within the needle limits (2026-10-31, B16.1 T13, OpenStitch mode only):** B16's Photo Stitch 2 OpenStitch mode sewed up to 3.4 needle points/mm² and, like PE-DESIGN-compatible mode, 160–540 stitches under 1 mm a photo. The owner's limits for Photo Stitch 2 (2026-10-31): at most 3.3 per mm² over the image and no 10 × 10 mm window denser than PE-DESIGN-compatible mode's on the same image. OpenStitch mode now scales Max. Density into them, takes PE-DESIGN's share of each thread (with its full coverage for saturated colours, which scored higher than B16's linear-light mix within the limits), and sews no stitch under 1 mm. Six images: 44.9 against PE-DESIGN-compatible mode's 43.4; the gradient, portrait and fruit stay below it (−2.5, −1.5, −0.3). Measured and left out: a layered coverage model (later lines lie on top; zigzag coverage saturates as 1 − exp(−0.5 mm / advance)), a mix-aware thread choice and five or six threads, all lower within the limits. PE-DESIGN-compatible mode is unchanged.

### Sew-out findings, 2026-11-02 (ready-to-sew kit on the owner's NV880E)

- **PEC frame (fixed 2026-11-03):** the header's width and height are a frame measured from
  where the stitch data starts; the machine reads it as the design's size and sews only inside it.
  OpenStitch wrote a frame of the design's size while the stitches kept their page position, so
  every exported design was cut by its page margin (the lettering lost "ch" and the foot of its
  circle text; the satin card's columns were pressed into a line). A first fix gave such designs
  PE-DESIGN's 290 × 190 mm frame (333 of the 442 references): the NV880E then refused every file
  ("The pattern is too large for the extra large embroidery frame"). The frame is now the design's
  own size with the stitches starting at its top-left corner, as in PE-DESIGN's 108 design-sized
  references; a design turned for a rotated hoop stays on the page. The compat runner puts a
  read-back PES or PEC export back at its page position before measuring, since the measurements
  compare page coordinates with PE-DESIGN's files.
- **Stippling (fixed):** it sewed every vertex of its smoothed meander, about 0.2 mm apart (800
  stitches in place); it now sews at the Run Pitch, at most half the spacing.
- **Flexible spiral (fixed):** its points crowded under 0.3 mm towards the centre (52 in a row);
  points under 0.5 mm from the last are left out.
- **Fill slivers (fixed 2026-11-04, #303):** Auto Punch's storybook picture in OpenStitch mode
  sewed white fill rows 0.3-0.5 mm long, one every 0.22 mm, through a sliver of its traced
  outline at about (99.5, 124) mm; rounded to the PEC's 0.1 mm grid they were 5-7 stitches under
  0.3 mm in a row. PE-DESIGN 11.50 has no minimum row length: its own Auto Punch of the same
  picture has 11 stitches under 0.3 mm in a row (rows 0.2 mm long), while its 74 drawn-fill
  references have at most 6 under 0.5 mm in a row. OpenStitch now joins a fill run of more than
  6 stitches under 0.5 mm into stitches at least 0.5 mm long (beyond PE-DESIGN; drawn fills
  unchanged, compat unchanged).
- **Open then (#290, closed):** Auto Punch on the storybook picture stacks 29 (OpenStitch mode) and 23 (PE-DESIGN-
  compatible) needle drops where satin columns meet, against at most 19 in PE-DESIGN's own files
  (#290). `tools/sew_check.py` reports it. Since #305, #304 and #440 the storybook passes
  `sew_check` (17 drops in a spot, PE-compatible; "Full-size review" below).

### Sew-out findings, 2026-11-04 (round 2, NV880E, white and pink cotton)

- **Locks (fixed, #289, #295):** the sunset photo patch came off the machine covered in loose
  thread tails. Its 296 runs had no tie-in or end lock, nor had Photo Stitch 2, Cross Stitch or
  the runs inside an object the compiler separates with a trim, while PE-DESIGN's 442 reference
  files lock all but one of their 2,590 trimmed runs, image tools included (21 of 21). The
  compiler now ties in and locks every run it trims after (PE-DESIGN's 0.3 mm steps), and the
  image tools lock their own runs.
- **Zigzag net (fixed, #293):** it sewed lumpy. The honeycomb's rows share every horizontal edge,
  so those were sewn twice, and the zigzag fanned at corners and past the outline. It now sews
  each edge once, as PE-DESIGN's band (3 mm, rungs 0.44 mm apart, nets 8 mm apart).
- **T1 card (fixed, #297):** the kit gave the "T1" mark a swatch's fill settings, and every
  later swatch the next one's, so the mark sewed as fill and four threads' swatches were
  mislabelled (the published results are corrected). OpenStitch's 6 mm satin lettering is a
  proper zigzag; the earlier note on its stroke ends came from this.
- **Kit layout (fixed, #296):** the laurel frame overlapped the corners frame in
  `shapes/decorative-frames`; the kit now checks that its frames stand apart.
- **Open:** Photo Stitch 1 trims 49 times per 1,000 stitches where PE-DESIGN's own trims about
  0.4 (#298). PE-compatible mode trims 1.4 per 1,000 since 2026-11-07 (section below); #298
  stays open for OpenStitch mode.
- **Calibration:** one layer of rows `r` per mm hides tanh(0.4 r) of the fabric, within 4 % on
  both cards; the Realistic view and the similarity scorer now draw thread that wide
  (`openstitch_design::calibration`, #291).

### Fabric presets and advice, 2026-11-07 (`batch:mac-fabric`, #508 part 1)

Beyond PE-DESIGN. PE-DESIGN's eight Fabric Selector types and their values are unchanged and
listed first.

- **Five more fabrics:** polyester woven, piqué knit, organza and sheers, canvas, and structured
  caps. Their density, stitch length, pull compensation and underlay are OpenStitch's
  approximations, like the eight: no stabiliser, needle or thread maker publishes these per fabric.
- **`fabric.advise {fabric, weight}`:** backing, topping, needle point and size, and the smallest
  legible block-letter height for No. 12, 30, 40 or 60 thread. The values come from the makers'
  published guidance (`crates/openstitch-engine/data/fabric_advice.json`, 21 sources checked 2026-11-07), and makers'
  disagreements are named (leather needle points, caps, towel and denim backings). Minimum text is
  published for No. 60 (3 mm) and No. 40 (6–7 mm, 6 used); No. 30 and No. 12 are estimates scaled
  by Madeira's stitch spacing, and say so. The Fabric Selector shows backing, topping, needle and
  smallest text.
- **Parts 2 and 3, landed since (#762, #765):** thread weight driving density, a default fabric
  for new objects, machine profiles, and the first-run setup.

### Job sheets, 2026-11-06 (`batch:mac-sewout`, #513)

Beyond PE-DESIGN, off unless chosen: `design.print`'s existing modes and the PE-compatible
output are unchanged.

- **Commands:** `design.setJob` (an undoable edit) stores customer, order, due date, notes, fabric
  and stabiliser in `Design.job`. `design.print {mode: client | operator, detail, shop}` writes the
  client approval sheet or the operator worksheet, as an overview (one page) or detailed. The
  editor has File ▸ Print ▸ Job Sheet….
- **The job stays out of machine files:** it is in the project only. PES, PEC, DST, EXP, JEF,
  VP3 and XXX exports are byte for byte the same with and without a job, and none holds a job
  field (`the_job_never_reaches_a_machine_file`). A project without a job is written exactly as
  before (the field is left out while empty).
- **What the operator sheet reports:**
  - its colour sequence is the compiled pattern's colour blocks, stops counted per block;
  - its sewing time is `design.info`'s `sewingMinutes`.
- **Checked by eye:** both sheets rendered from a two-colour design with text, on A4 and Letter.
- **Not done:**
  - a shop logo image (name only);
  - scripts outside WinAnsi (the PDF uses the standard Helvetica);
  - a print preview in the dialog.

### Knockdown fill, 2026-11-06 (#510, `batch:production`)

Beyond PE-DESIGN (PE-DESIGN 11 has no fill under the design; its background fill sews around
patterns). Nothing changes until `wizard.knockdown` is run: it adds an ordinary Fill object
named Knockdown (no underlay, 1 mm rows, 4 mm stitches) before what it covers, so the file stays
a plain PE-compatible design and PES 0110 export writes it as a fill shape. `order.optimize`
moves it back before the first object it covers. Measured in `tests/knockdown.rs`: two 20 mm
squares 2 mm apart give one 1,100 mm² piece (46 × 24 mm with 2 mm rounded corners, the gap
closed); a ring with a 30 mm hole keeps the hole (no needle point within 12.5 mm of the
centre). Sew-out on terry towel is left for the owner.

### macOS platform, 2026-11-06 (#319, #557, `batch:mac-platform`)

No PE-DESIGN-compatible output changed (no compat spec, golden or writer touched). Help ▸
Ready-to-Sew Designs is now checked in the built macOS bundle (#319): CI runs the bundle's own
`openstitch --sew-kit`, and a recording `open` receives exactly `Contents/Resources/sew`; with
the kit moved away the editor falls back to GitHub. The real click stays the owner's manual
check. Finder shows design thumbnails through a Quick Look extension, a Swift shim over the
Rust renderer (#557; `stitchtool thumbnail` exposes the same renderer); the Explorer half landed
with #669. PE-DESIGN 11 installs Explorer thumbnails for its own files on Windows only.

### Assistant fixes, 2026-11-06 (#580, #384, #494, #495)

Beyond PE-DESIGN only; no PE-DESIGN-compatible output changed (no compat spec, golden or writer
touched). The new batch check (#494) reads the same parameter documentation as the MCP schemas
and finds no problem in any of the repository's scripts: every compat spec's and example's steps
validate clean (`openstitch-engine` `tests/validate.rs`), so a script that ran before still
runs. Print PDFs handed to the operating system are cleaned up after a day (#384). The
Assistant's stream timeout (#580) and thread-chart rules (#495) are in
[ai-integration.md](research/beyond/ai-integration.md) "Assistant fixes".

### Kit review and calibration cards, 2026-11-06 (#295, #441, #311, #312)

- **New sew_check checks (#295):** trims per 1,000 stitches (limit 55: PE-DESIGN's 453 files in
  `compat/reference` reach 52.5, a spiral ring with holes), and hairline zigzag (more than 45 stitches in a row under 1 mm that turn
  back, a satin narrower than 1 mm; PE-DESIGN's small Text Transform lettering reaches 43).
  Kit files that fail for an open engine issue are listed by issue (`sew_check.KNOWN`: the
  storybook's hairline satin #368, the PE-compatible butterfly's 60 trims per 1,000 #627) and
  fail again once they pass (both fixed since, #368 and #627; the list is empty). Long stitches
  stay a warning (#460).
- **1 mm Auto satin:** the satin card's 1 mm column sewed one 39.5 mm stitch along its length
  (fixed in #460's batch); `auto_satin_on_a_narrow_column_never_stitches_along_it` covers 0.8 to
  2 mm columns.
- **Entry travel across calibration swatches (#628, open):** a fill is entered at the outline
  point nearest the previous object but starts its rows at a fixed side, and the straight travel
  between them crossed every T1 swatch (about 19 % more thread than the rows on the 0.5 rows/mm
  ramp swatch). The kit now sets each T1 and T2 swatch's entry at its first row; the engine rule
  is #628.
- **T2 card (#311):** Black and Yellow fills in alternating rows at 2-5 rows/mm each, in both
  sewing orders, with solid swatches at the total density; the kit README gives the fit of the
  later thread's visible share, from which `rows::Rows::strip` is set once it is sewn.
- **Swatch densities (#312):** the kit build measures every T1 and T2 swatch's rows per mm in the
  exported PEC stitches (each within 0.03 rows/mm) and fails beyond 0.2; #297's shifted settings
  are caught. `manifest.json` records the values.
- **Realistic view:** shapes that sew nothing (a text path's guide circle) are no longer outlined
  there unless selected; the lettering preview showed the circle as a sewn grey ring (#441).

### Auto Punch columns: travel through the column, 2026-11-05 (#450 follow-up)

- **Finding:** the README's g08 still showed diagonal threads across both T's and the I after
  #448. The PES shows two causes. (1) A one-section satin (the I; OpenStitch's Auto satin falling
  back to its fixed direction) travelled from its start at the top to its far end by the
  shortest way inside, diagonally through the column, and the travel showed between the rows.
  (2) The T's stem, cut from its bar on pixels, had 0.3-0.5 mm notches at its top; the
  fixed-direction rows broke into slivers there, sewn after the column with four diagonal
  passes on top of it.
- **PE-DESIGN 11.50** (round 5, `g08-lettering__autopunch.pes`, black-box): the I enters at its
  top left, runs 0.5 mm inside its left edge to the bottom, along the bottom edge to the right,
  then sews the satin back up; its objects are whole letters with clean outlines and
  "inside" travel.
- **Fix:** a bar's outline is the convex hull of its traced outline; Auto Punch bars travel along
  the outline (0.5 mm in), and Auto satin that falls back to its fixed direction does the same
  when it is one section without holes (a jagged photo region in many sections, or one with
  holes, keeps the spread inside travel of #304: walking round the outline between sections
  added a third more stitches to p03).
- **Measured** (`tools/image_similarity.py --mode both --no-dists`, main vs this change): g08
  MS-SSIM 0.9376 → 0.9402, dE 10.06 → 9.96, 1,247 → 1,215 stitches (PE-DESIGN 1,268), trims 9;
  g05 and g15 better; the photos within ±0.0011 MS-SSIM and ±0.06 dE with up to 3 % more stitches
  (longer edge travel); 58 of 70 cases unchanged.

### Full-size review of the kit, demos and screenshots, 2026-11-05 (#450)

- **Kit drift:** a fresh release build of main (e9e1e08) rebuilds all 35 kit files byte for byte,
  and all pass `tools/sew_check.py` (storybook PE-compatible 17 drops per spot). The STACKED 23
  reported in #449 came from a build of 4421457, which lacked #440's jagged-satin fix (23 → 17).
  CI's `kit` job now runs `tools/sew_kit.py --check` and `sew_check` on every pull request.
- **PEC frame:** a design reaching 0.5 mm above its page (the name-drop badge's lettering) kept
  its page layout, as pyembroidery's files do, so its stitches ran 4.3 mm past the frame's right
  edge and the machine would cut them off. Only a pattern whose first needle point is the origin
  keeps its layout now.
- **Realistic view:** trimmed travel was drawn as grey dashed lines across every preview and
  screenshot; PE-DESIGN's Realistic Preview shows the sewn piece, where a trimmed jump leaves no
  thread. Travel joining stitches across a trim or colour change is no longer drawn there.
- **Design files:** the review (stitches alone at 15 px/mm, the Realistic view, sew_check) found
  satins 20-30 mm long, cross stitch reading as "+", 2 mm frame ornaments sewn as plain squares,
  an appliqué heart sewn whole under its fabric, a name wider than its shield, 4 mm text, stale
  built-in templates (version 1, old defaults) and monogram letters cut by the Diamond/Circle
  clip; all fixed in the kit script and the examples (the monogram clip in the engine since,
  #458: letters are warped onto the silhouette, and the kit's varied letters are back). Engine causes are #458-#471.

### Kit previews and documentation images, 2026-11-05 (#563-#565)

- **Preview colours (#564):** on decorative-frames, a full-colour Realistic render holds every
  thread exactly (nearest pixel 0 ΔE), but `preview()`'s 128-colour median-cut palette put the
  nearest pixel 61 ΔE from Red, 58 from Orange, 50 from Ultra Marine and 44 from Magenta. 11 of 35
  committed previews lacked a thread. The previews now use 256 octree colours (mean 1.4 ΔE from
  the render), fall back to full colour if a thread would still be lost, and
  `sew_kit.py --check-previews` (CI job `kit`) requires 20 pixels within 20 ΔE of each thread.
- **Documentation images (#565):** `tools/docs_images.py` runs every generator (screenshots,
  gallery, engine plots, the four templates, digitize and convert previews, Stitch Design Factory
  SVGs, the catalogue page) and `--check` fails on a file under `docs/images` without one. The
  Assistant screenshots and the image-similarity comparisons stay as dated records of the runs
  their documents describe. In `--screenshot` runs the wizard dialogs (pattern wizards, Monogram,
  Fonts, Design Settings) open at the top right and the page is fitted beside them.
- **Cotton texture (#563):** its Higgsfield job (Recraft V4.1, 2026-10-27) was found in the
  account's history; the committed file matches a crop of it at correlation 0.9999.

### Auto Punch on bold lettering, 2026-11-05 (#443)

- **Finding:** the README gallery's g08 (navy "STITCH" over a red bar on cream) sewed its letters
  as satin fanning in every direction, with a grey fourth thread inside them and a ragged bar
  (1,984 stitches, 13 % of consecutive stitches turning by more than 45°; after #440's jagged-
  satin fix 1,504 and 5.8 %, letters partly sewn along their strokes, the grey still there).
  PE-DESIGN 11.50's own Auto Punch of the image (round 5, `g08-lettering__autopunch.pes`) sews 7
  satin regions, one per letter and the bar, 2 colours, 1,268 stitches and 0.7 % sharp turns:
  the T's bar across at 90° and its stem at 0°, the H's stems at 0° and its crossbar at 90°, the
  S and the C fanned round their curves, the bar at 90°.
- **Causes, measured:** (1) at the 400 px working size the 768 px image's box-mode copy gives an
  edge shade 0.57 % of the pixels, over the 0.5 % automatic-colour share, so 4 colours (grey);
  at the image's own size the count is 3, as PE's, and the 1-2 px gaps between S-T and T-C
  survive. (2) The 3×3 modal filter votes on exact colours; the image draws its flat colours in
  runs of slightly different shades (the bar's edge row 243,68,82 and 244,79,91; the cream
  254,245,237 and 252,250,240), so where two runs meet the edge pixel lost the vote (8 of the
  bar's 470 top-edge pixels): one-pixel notches every few millimetres. (3) Auto satin derives one
  centre line per region and fans its rungs at outline corners: it cannot follow the junctions
  of the T and the H or the cut ends of the S and the C.
- **Fixed, for flat graphics only** (at most 5 automatic colours at the working size): PE-
  compatible mode traces them at twice the working size, choosing the threads at the working
  size as before (chosen at 800 px, the teapot's black outline became a dark green); pixels keep
  their colour's shades against a flat neighbour (three identical pixels); in PE-compatible mode
  satin regions made of straight bars are split into one satin per bar, straight across, and
  curved strokes sew along non-crossing rungs square to their smoothed centre line. Now 11
  regions, 1,247 stitches, 2 colours, 1.0 % sharp turns; at most 8 needle drops per 0.5 mm spot
  (`tools/sew_check.py`). PE keeps each letter one region and links the letters with short
  stitches (0 trims); OpenStitch trims between them (11).
- **No regression (owner's rule):** against main, scored twice to measure noise (none: the
  scorer is deterministic), likeness without DISTS is unchanged on every photograph and
  painting (the same stitches) and higher on the flat images (g08 +0.34 / +0.95, g11 +1.31, g01
  +0.89 / +0.32, g15 +0.00 / +0.08); no case loses a thread except g08's grey, which PE's own
  file lacks (18 images, both modes). Applying the changes to every image was measured first
  and rejected: tracing photographs at 800 px swapped their threads (the storybook fox's orange,
  #447) and the bar split and the shade labels lowered their likeness by up to 1.4.
- **Open:** OpenStitch mode on the same image sews the cream background and fuses the letters
  across their gaps (#445; fixed 2026-11-06, `batch:auto-punch`). Auto satin itself (drawn shapes) still fans at the corners of
  branched shapes.

### Photo Stitch 1 speed since the locks, 2026-11-05 (#433)

- **Finding:** since #299 counted every trimmed run's tie-in and lock against the needle caps,
  Photo Stitch 1 in OpenStitch mode was much slower: the issue's 16-stripe test image took
  171.2 s at High fidelity (23.1 s before #299) and 37.6 s at Standard (32.0 s), release
  build. A trace of each attempt confirmed the cause: the rows are fitted to a 15 %, then
  7 %, larger budget for the local search (T9) to bring back under the caps, but the locks hold
  about a quarter of the cap that the search cannot free, so both boosted attempts ended over
  (105.4 % of the total cap; then 98.9 % with 33 windows over) and the search ran three times.
  Each search was also slower: the locks make the fit choose fewer, longer runs, and the
  search's cost per run grew with the square of its length.
- **Not taken:** reserving the locks' share of the boosted caps (the issue's suggestion) still
  left the 15 % attempt over by 0.2 % on the stripes, and on the photos it changed which
  attempt held: likeness moved both ways (p04 dE 12.08 → 11.43, p01 14.52 → 15.35, MS-SSIM
  0.573 → 0.519).
- **Fix (no change to any result):** the attempts are independent, so they now run at once on
  their own threads, and the first in the old order that holds the caps is kept; a later
  attempt stops as soon as an earlier one has held (in turn on a one-core machine). The search
  adds, for each cell it scores, only the changed cells within the eye kernel's reach, a slice
  of the change kept in order, so every sum is the same bit for bit.
- **Measured** (release, the issue's stripes at 160 mm, OpenStitch mode, one loaded
  8-thread Windows machine, wall / CPU): High 899 / 616 s on main, 40.6 / 100.7 s with this
  change, 46.9 / 46.2 s before #299; Standard 258 / 105 s on main, 24.2 / 68.2 s with this
  change, 42.0 / 41.5 s before #299 (the attempts that lose cost CPU time, not wall time). The
  DST files are byte-identical to main's; `tools/image_similarity.py --no-dists` gives the
  same numbers on all 16 Photo Stitch cases in both modes and the nine Photo Stitch 1 photos at
  High; `sew_check` passes; compat 454/454.

### Test swatches and the fabric calibration card, 2026-11-06 (`batch:mac-sewout`, #452, #453, #507)

Beyond PE-DESIGN, off unless chosen: PE-DESIGN-compatible output is unchanged.

- **What landed:** `design.testSwatch` (#452), `design.testSwatches` (#507),
  `fabric.calibrationSwatch` and `fabric.calibrate` (#453), and `fabric.deleteCustom`.
  `fabric.list` and `fabric.apply` take the custom fabrics. The editor has Send ▸ Test
  Swatches….
- **How they work:** each swatch is built from the engine's own commands (`object.paste`,
  `object.setRegion`/`setLine`, `text.add` in the single-line font, `machine.set`) on the File
  effect's scratch state. It is written through `design.export` or `project.save`, so the open
  design never changes.
- **Labels:** 3.5 mm running stitch in the piece's own thread, sewn after every piece (one colour
  block per thread). A parameter swatch's columns are as wide as the widest label.
- **Sewability:** `tools/sew_check.py` passes on a selection swatch (fill and satin, labelled), a
  fill density swatch (2 × 2, then 4 × 2 with pull compensation), a satin swatch (4 × 2), a
  running-line pitch swatch and the knit calibration card, all in the 100 × 100 mm hoop.
- **Not done:**
  - no owner sew-out yet;
  - filum-style calibration from measured sizes;
  - the editable fabric table (B5);
  - a live layout preview in the dialog;
  - job sheets (#513), landed since ("Job sheets" above).

### Shape cuts keep the sewing angle, 2026-11-06 (`batch:mac-shapes`, #652)

- **Fixed:** Remove Overlap, Combine, Merge, Split and the shape builder's merge (all through
  `shape_ops::set_region`) set the cut shape's sewing angle back to the region type's default
  (45° for fill). Nothing in PE-DESIGN's references backed that. Now every cut keeps the
  shape's angle and its other region and line sewing attributes and colour. Its entry and exit
  points stay where they are still within 0.5 mm of the new outline and go back to the default
  where the cut took that part away. Before, the knife, Divide, the shape builder and the shape
  effects always reset them; Combine, Merge, Remove Overlap and Split always kept them.
- **Merges** (`shape.merge`, a `shape.build` drag across two shapes) take the topmost parent's
  attributes, the shape sewn last, as `shape.merge` always has.
- **Open:** whether PE-DESIGN 11 keeps a region's Direction after Remove Overlap. The owner's
  capture request is on #652; OpenStitch now keeps it.
- Tests through the commands, one per operation, on a 135° fill (45° is the fill default and
  could not show a reset): `every_cut_keeps_the_sewing_angle_and_attributes`,
  `a_cut_away_exit_point_returns_to_the_default`, `merges_take_the_topmost_parents_angle`.

### Photo Stitch 2 Mono on the coverage curve, and thread search, 2026-11-06 (`batch:mac-photo-2`, #468, #486, #682)

- **Measured and not taken: an OpenStitch-mode Mono on the T1 coverage curve (#468).** The grey
  after Mono's contrast stretch was turned into a target coverage and then into a zigzag share
  by the T1 curve (the run alone is tanh(0.4 · 10/interval); rows = atanh(c / 0.9) / 0.4).
  Variants: darkness in linear light, darkness in sRGB, and PE's ramp with its no-zigzag
  threshold lowered from 17 % to 10 % or 5 %. On the six Photo Stitch 2 images in Mono plus the
  eight-step grey ramp, every variant lowers mean likeness (PE ramp 34.4; variants 33.2–33.9),
  the grey ramp included (35.2 → 34.2–35.1). Both modes keep PE's ramp; the 219 step stays lost.
  Table on #468.
- **#433 check:** `fidelity_high_raises_the_colour_counts` passes in 9.1 s on the Mac (`-j 2`).
- **Thread search in the editor (#682, part of #486):** the PEC chart popup filters its swatches
  by chart number, Brother Embroidery code or name, with `threads.find`'s matching
  (`find_rank`, now public). Tests: `find_in_the_brother_chart_is_the_pickers_search`,
  `pec_picker_search_finds_threads_by_number_or_name`.
- **More thread charts (#486): none added.** No maker's terms allow reuse of its published values
  (Gunold Poly and Cotty: silent; Coats Sylko, Hemingworth, Fil-Tec Glide: forbidden; the rest
  publish none). The table and sources are in research/beyond/thread-charts.md; the owner decides.

### Photo Stitch travel, the sewn sunset and the layered screen, 2026-11-06 (`batch:mac-photo`, #298, #291, #129, #468)

Likeness is `tools/image_similarity.py`'s score (S, C and DISTS) on the calibrated render;
every result is OpenStitch's output. No generator change landed: each one measured lowered
likeness, so the issues stay open with the numbers below.

- **Photo Stitch 1 travel across a thin stripe (#298), measured and not taken.** When a
  thread's island is done and no hidden path leads on, a second search let its routed travel
  (Fewer trims) cross up to 2 mm of an earlier colour within CIEDE2000 `limit` of its own, in
  sight, instead of trimming. Over the nine Photo Stitch 1 cases (OpenStitch mode, main's 2633
  trims, 20.0 per 1,000 stitches, mean likeness 64.21):

  | Limit | Trims | Per 1,000 | Mean likeness |
  |---|---|---|---|
  | ΔE00 5 | 2633 | 20.0 | 64.21 |
  | ΔE00 8 | 2615 | 19.9 | 64.25 |
  | ΔE00 11 | 2505 | 19.1 | 63.76 (p01 −1.7, p05 −2.5) |
  | ΔE00 15 | 2102 | 15.9 | 63.47 (p01 −4.7, p05 −3.3, p04 +2.1) |
  | any colour (an earlier build of the patch, indicative) | about 600 | about 4 | about 61.6 |

  The travel's needle points count against the density caps and its crossings show, so every
  limit that saves trims costs likeness. The patch is kept as a design record outside the repo;
  PE-DESIGN-compatible mode was unchanged by it. PE-DESIGN's own rate (0.4 per 1,000) still
  needs visible travel the owner decides on after the sunset sew-out.
- **The sewn sunset patch against its render (#291).** New `tools/image_similarity.py --scan`
  mode; on the round-2 scan the calibrated render scores 50.3 (flat 55.6, realistic 56.5,
  calibrated with the Realistic view's shading 50.6) and reads 11–19 L* too light in the
  patch's dark middle, where several threads' rows interleave. Details and the table are in
  the image-similarity plan's progress (2026-11-06).
- **Photo Stitch 2's layered coverage solve (#129, #468), measured and not taken:** mean
  likeness 39.7 → 37.2 on the seven Photo Stitch 2 cases; details in the plan.
- **Checked:** #433's timing test (`fidelity_high_raises_the_colour_counts`) 8.8–9.0 s before
  and with the travel patch; workspace tests unchanged (no generator change lands).

### Photo Stitch trims, needle drops, coverage and colour, 2026-11-05 (#298, #468, #129, #291)

All likeness numbers are `tools/image_similarity.py`'s S and C terms (DISTS not installed),
averaged 50/50, on the calibrated render; every result is OpenStitch's output.

- **PE-DESIGN-compatible Photo Stitch 1 trims (#298):** a thread's runs were joined only within
  two cells and two rows, else it jumped to the next run in row order. It now travels to its
  next run along a hidden path when one exists: under its own cells and the darker layers sewn
  after it (the Router OpenStitch mode has used since #302), as PE-DESIGN sews a colour in one
  to five paths. Trims per 1,000 stitches on the nine Photo Stitch 1 cases: 26.6 → 8.7 (p01),
  33.3 → 10.9, 46.3 → 14.8, 39.7 → 14.2, 34.0 → 11.5, 28.7 → 9.4 (sunset), 34.1 → 6.0,
  22.8 → 8.1, 10.7 → 3.3. Likeness rises on eight (p01 43.2 → 45.4, p07 51.0 → 54.6, p10
  32.7 → 34.6) and falls on the high-key photo, 79.39 → 79.24. Over five widths (99.6–100.4 mm)
  that photo scores 78.80–79.39 on main and 0.02–0.15 lower at each width with the change
  (mean −0.07), so the fall is inside the scorer's spread. Travel only under the thread's own
  cells was measured too: more trims, the sunset below main (48.4 against 49.2). PE-DESIGN
  itself trims 6 times in 15,121 stitches, so the gap stays open (#298).
- **Needle drops in columns (#468, Photo Stitch 1 OpenStitch mode):** a fine run whose length
  is a whole number of Run Pitches has stitches of exactly the pitch, which left no room to shift
  its needle points from row to row: on the sunset's full-width bands every row put them in the
  same columns. Such a run's first or last stitch may now reach up to a sixth of a stitch past
  the pitch. The share of needle points on the 30 most used columns falls 28–47 % on the nine
  cases at Standard and High (sunset 0.215 → 0.136). Measured and dropped: one stitch more per
  such run (no stitch past the pitch) makes the density fit choose other rows and lowers
  likeness (portrait 66.5 → 60.7 at Standard); shifting the points inside the fit (p01 at High
  61.8 → 55.2). Now the fit counts the unshifted points (same count, nearly the same cells), so
  stitches, trims and colours are unchanged and likeness moves by at most ±0.06.
- **Photo Stitch 2 without the floor (#468) and on the calibrated coverage curve (#129):**
  measured in OpenStitch mode and not taken. Lifting the needle where a thread's share is below
  8 % lowers likeness on all seven Photo Stitch 2 cases (poppies 39.8 → 35.6, cat 49.7 → 47.3,
  CMYK poppies 34.3 → 33.4); spacing the zigzag so its thread hides the share by the T1 curve
  (tanh 0.4 r) lowers it further (poppies 31.6, cat 44.2), because mid shares then sew about
  half the stitches and the screen reads too light.
- **Sewn thread colour (#291):** a gain and offset per channel in linear light, fitted to the
  T1 swatches of both cards (White on pink left out, as the record says), cuts the mean ΔE00 per
  swatch from 11.6 to 8.0, but only to 10.9 when each thread is left out of its own fit, and it
  turns White 240 into 206/183/182 (the pink card). Six threads do not pin a general model, so
  the Realistic view and the scorer keep catalogue colours. The sheen the issue names (dense
  fills reading lighter) is not in the relabelled data (#297): L* at 6.0 minus 4.5 rows/mm is
  −3.4 to +0.6 for ten thread/card pairs and +4.1 only for Yellow on pink.
- **Checked:** `sew_check` passes on all 29 Photo Stitch 1 PES exports; Photo Stitch 2 output
  is unchanged; compat 454/454; the golden hashes of Photo Stitch 1's OpenStitch mode re-pinned.

### Stitch-engine batch, 2026-11-05 (#460-#464)

- **Long satin stitches (#460):** PE-DESIGN keeps 16-30 mm satin crossings whole
  (`machine-satin-width-*`), so PE-compatible output keeps them; `tools/sew_check.py` now warns
  (does not fail) on stitches over 12 mm. Two OpenStitch defects behind the review's long
  stitches are fixed. (1) A narrow Auto satin column (the kit's 1 mm column) squared its last
  rung against its end edge and turned it into a point 40 mm away: one 39.5 mm stitch at the
  start (kit calibration/satin-and-underlay: longest 39.5 → 6.0 mm). (2) Auto Punch sewed
  regions whose satin falls back to one direction with 12-16 mm crossings, where PE-DESIGN's
  own Auto Punch never passes 10.4 mm (12 round-5 pictures): such a region is now fill
  (storybook PE-compatible: 30 stitches over 12 mm → 0, longest 16.1 → 10.2 mm).
- **Centre-out fills (#461):** PE-DESIGN's references (concentric-*, spiral-*) end the spiral in
  the middle with no run back out, and keep a line of 0.5 mm steps where each whole circle
  starts (kept). Where the start point lies beyond the whole circles PE trims and jumps in
  (concentric-rect30x40 trims at stitch 724, the triangle at 879); OpenStitch ran straight across
  the arcs and now trims as PE does. Without a PE reference: the flexible spiral no longer travels
  20 mm straight back to its end point; contour rings sew on 3-6 mm past their start so the steps
  scatter; the double spiral's arms no longer cross (spacing steady at a row); radial lines start
  only where their neighbours are 0.4 mm away, so the centre no longer piles up (stitches under
  0.3 mm on `sewing-direction.json`'s radial block 60 → 4, longest straight run 33.5 → 23.4 mm).
- **Stippling (#462, no PE-DESIGN reference captured):** the meander ignored `spacing`, stayed
  about 3.5 mm inside the shape and reached its start point by walking the outline (the exposed
  L). The grid is now fitted to the shape, the passes are evened to the spacing and the outer ones
  moved to about half a spacing from the outline and holes, and the meander starts and ends at
  its point nearest the start point. 50 mm square at 2 mm: mean pass spacing 2.35 → 2.27 mm, gap
  to the outline mean / max 2.84 / 6.40 → 1.06 / 1.68 mm, run along the outline 142 → 3 mm, no
  crossings; the Background Fill wizard fills its circle to the edge. Reading of the issue's
  "gap ≤ spacing / 2": mean ≤ 0.55 spacing and largest ≤ 0.9 spacing (two round turns meeting
  the outline side by side leave about 0.8 spacing). Open: a region with a hole in narrow bands
  (a ring at 3 mm) stays in separate pieces whose travel crosses the hole, as on main.
- **Cross stitch and zigzag net regions (#463):** PE-DESIGN 11.50's `o-region-cross.pes` (direction
  45°) sews X's on the page axes; ours turned the grid with the direction and read as "+". The
  grid now stays on the page axes from the design origin (two regions share it), each row is sewn
  out and back as Cross Stitch from an image sews it, and travel runs only along the crosses'
  half diagonals: stitches along a cell side 244 → 5 on the kit's cell (the 5 are the travel from
  the start point), trims per 100 crosses 0 → 0. The zigzag net keeps every needle point inside
  the outline (honeycomb 1.1 mm past the edge → 0), its rungs square to each edge and cut where
  bands meet at a line's end or turn, and travels along the net lines. Open: the net's turned
  grid against PE's page-axis one. Settled 2026-11-07 (#614, round 8): PE-DESIGN 11.50 has no
  Direction for cross stitch (its Sewing Attributes show only Size and Times), so OpenStitch hides
  it. Times Single, Double and Triple cover each diagonal 2, 4 or 6 times, 410, 666 and 922
  stitches on a 20 mm square. OpenStitch sews the same 256 more per step and 18 more at each
  (its route between rows differs; compat `cross-single`, `-double`, `-triple`).
- **Line types on curves (#464):** stem, candlewick and chain placed their elements per
  flattened chord, so on a curve they bunched at bends with gaps between. A leg now runs from one
  contour point to the next and is measured along the curve: the stem is an even rope (spacing
  std dev 0.58 → 0.01 mm, gaps 18 → 0 on the kit's row), knots and links within ±10 % of the set
  spacing. Running stitch splits a stitch while the curve bulges over 0.3 mm from it, so 2 mm
  scallops keep their shape (PE's 60 mm circle keeps 4 × 32 chords). Straight-line output is
  unchanged: the `line-stem-*`, `line-chain-*` and `line-candlewick-*` exports are byte-identical.
  PE-DESIGN sews each chain triangle five times over as well (`line-chain-default`), so that is
  kept.

### Stitch engine batch 4, 2026-11-06 (#567, #459, #467, #470, #471, #368, #609)

- **Moving a shape (#567):** `object.move` by (d, d) mm changed a 20 x 10 mm rectangle's
  satin from 195 to 213 stitches and its fill from 392 to 391-393, because the tatami, satin
  under sewing and piping grids were anchored at the page origin, the writer merged needle
  points that rounded to the same 0.1 mm cell (generators repeat a point up to 0.004 mm away),
  and float noise flipped `ceil` stitch splits and nearest-row ties. Each shape and text is now
  generated with its bounding box's corner at the origin and moved back, points under 0.05 mm
  apart are one penetration wherever they fall on the grid, stitch splits treat a length within
  1e-9 of a whole number of stitches as that number, and the row chain breaks ties to the first
  row. Satin, fill, running line, programmable fill, piping, a 37° satin and text moved by 1,
  0.5, 0.37, 3.5, 12.25, 35, 0.05 and -7.13 mm one after another sew the same needle points
  moved (`moving_a_shape_moves_its_stitches_and_nothing_else`). PE-DESIGN anchors its fill grid
  to the region too (`grid_anchor`, from its references), so (a) of the issue holds without a
  PE-DESIGN reference pair. `DesignObject::apply` also left a line's exit point behind (fixed).
  Compat: 454 of 454 specs pass before and after. One measurement moves tier: the PES 0110 Text
  Transform spec's arch height measured 26.0 mm only because a travel stitch walked 0.2 mm out
  to the top letter's corner and back; the rows span 25.8 mm wherever the text stands, against
  PE-DESIGN's 26.5 (its arch is bent round a 40.8 mm circle, OpenStitch's round 39.6, the known
  difference of item 5), so the height joins the width at the internal-regression tier until the
  arch matches. Golden counts: badge 2780 → 2778 stitches, the determinism pins re-recorded
  (arch 1731 → 1726, rotated satin 357 → 353).
- **Programmable fill stitch length (#459):** PE-DESIGN sews programmable fill crossing to
  crossing (prog-fill-line-2pt 5.3 mm stitches, owner-v up to 10.6 mm) and has no stitch
  length for it, so the default is unchanged and `stitchLength` stays a fill setting.
  `splitLength` (2-12 mm, beyond PE-DESIGN) now also splits the gaps between crossings evenly,
  so the added points line up from row to row: the issue's 12 mm diamonds at 2.2 lines/mm go
  from over 8 mm to at most 4 mm with every crossing kept.
- **Laurel and stars frames (#467):** each laurel side is one region (a 1 mm stem band with its
  leaves, at least 4 mm long, three to nine pairs) and the stars one region on a 1 mm band, so
  the laurel trims once across its opening and the stars not at all
  (`laurel_and_star_frames_sew_each_piece_in_one_path`). Kit: shapes/decorative-frames 42 → 15
  trims, lettering/monograms 56 → 15.
- **Appliqué kept pattern (#470):** PE-DESIGN's own wizard files a01-a04 keep the pattern and
  sew it first (PEC 1, then 62, 63, 64), under the fabric, which is OpenStitch's default.
  `wizard.applique` `patterns: last` sews it after the covering stitch instead (beyond
  PE-DESIGN).
- **Direction curve (#471, in part):** fill and piping rows along a curve are the curve
  (carried straight on past its ends) offset by whole row spacings: on the issue's heart every
  row needle point lies within 10 % of a spacing of its row and no two rows cross
  (`a_direction_curve_fills_with_rows_a_spacing_apart_that_never_cross`); the warp kept rows a
  spacing apart only across the chord, so they bunched where the curve is steep (piping's
  dark bar). Open: programmable and motif fill along a curve still bend their tiles by the
  chord, and emboss and engrave strength against PE-DESIGN.
- **Hairline satin and tiny locks (#368):** the storybook's 0.4-0.9 mm columns sewed 52
  crossings under 1 mm in a row. A region satin with more than 30 such crossings in a row now
  sews that stretch as a running line through their middles, and OpenStitch mode's outlines
  are running lines below 1 mm (0.6 before). A lock after a last stitch under 0.15 mm steps back
  0.3 mm along the run instead of 0.1 mm (PE-DESIGN locks along last stitches of 0.2 mm and
  more, satin-zigzag-rect). No PE-DESIGN reference has a satin under 2 mm wide, so this is
  OpenStitch's own rule. Storybook (OpenStitch mode): hairline 52 → 28, longest run under 0.3 mm
  6 → 5 (a lone 0.3 mm speck tied in and locked); it leaves sew_check's KNOWN list. The kit is
  regenerated (19 files change; all 36 pass `sew_check`; previews rendered on Linux).
- **Platform-independent arch (#609):** fixed 2026-10-30 (above); on Linux the spec measures
  the same as on Windows and macOS, `determinism.rs` pins the bits on all three systems, and
  PE-DESIGN's re-save is the spec's reference.

### Stitch-engine bug batch 3, 2026-11-06 (`batch:engine-bugs-3`: #570, #571, #628, #466, #627, #633)

- **Needle drops past reshaped outlines (#570, #571):** the four random-design seeds (150, 154,
  339, 398) were not E-stitch, chain or fill-row defects: `object.setPoint` reshaped an
  outline and left its start and end points (the old bottom centre) off it, and the fill
  travelled out to them (seed 150: a run of 2 mm stitches to 9.6 mm past the bounds). A region's
  start and end points more than 0.1 mm off its outline are now sewn where they snap to it. All
  four seeds pass and leave the test's `KNOWN` list
  (`a_moved_node_leaves_no_needle_drop_at_the_old_start_point`).
- **Travel across a region (#628), partly:** PE-DESIGN 11.50 runs a fill's travel from its start
  point to its first row straight across, at every density: `sweep-fill-density-min` (0.1
  rows/mm) fails when it walks the outline (37 needle points differ), so Inside keeps it. Running
  Stitch Path Along / On outline now walks that travel along the edge too (12 mm swatch at 0.5
  rows/mm entered mid-bottom: a 13.4 mm straight travel to the top-left corner → none more than
  2 mm inside the swatch).
  A programmable fill's under sewing joins its edge run to its first row along the outline,
  0.5 mm inside, instead of 26 mm straight across under the open diamonds. The 39.5 mm satin
  travel of the comment was fixed in #460. Open: whether the default should leave PE-DESIGN's
  straight travel where rows do not cover it (an owner decision).
- **Cusps and apexes (#466):** a zigzag line round a heart folded its inner needle points over
  the line at the top cusp (stitches crossing into a knot); a needle point nearer the line than
  half its offset is now sewn on the line (a half stitch). The zigzag under sewing's mitre had no
  limit and spiked out at the cusp; it is bevelled past 120°. Skeleton satin takes Half
  Stitch, so the apex of `[[8,82],[20,70],[32,82]]` no longer piles its inner needle points (7
  needle points within 0.2 mm of the one two before → 0). Straight-leg zigzag mitres (a star's
  inner corners) are cut by the Miter Limit since 2026-11-07 (#373: the random-design test's
  33 zigzag spike seeds pass).
- **Design Center line art (#627):** the skeleton splits line art at every junction and each
  piece was sewn alone with a trim. Pieces whose ends meet within 0.8 mm are now sewn as one path
  per figure, walking back over sewn lines (at most 50 mm) to the nearest junction with an
  unsewn line (`joinLines`, default on). Kit butterfly, PE-compatible mode: 91 → 14 trims, 60.0
  → 7.8 per 1,000 stitches, 1,516 → 1,786 stitches; OpenStitch mode's parallel lines are
  unchanged (15.4 per 1,000). No PE-DESIGN Design Center capture exists to compare the routing.
- **Flaky timing test (#633):** `cheap_rules_are_fast_on_a_large_design` compares the check
  with compiling the same design in the same run (best of three each) instead of a bare 50 ms.

### Production aids, 2026-11-06 (#509, #512, #547, `batch:production`)

All beyond PE-DESIGN and off by default: with no basting, no stop and `autoStart: off`, compiled
output is byte-identical (tests `basting_box_sews_first_in_the_first_thread`,
`stop_after_an_object_and_at_jumps`, `nearest_start_shortens_travel`); compat 454/454.

- **Basting (#509).** Sewn first in the first thread with no added colour change (or its own
  block); no tie-in or lock so it pulls out. Inside the field of every listed hoop, oval and
  round included (`basting_box_stays_inside_every_hoop`); the outline form stays at the margin
  within 0.3 mm. PES 0110 export refuses the Production settings (PE-DESIGN's objects cannot
  hold them).
- **Stops (#512).** PES/PEC, DST, EXP and JEF already wrote `Command::Stop` as a colour change
  to the same thread; a PES read back shows it as a repeated thread (test above). A stop that a
  colour change follows is dropped: the colour change stops the machine.
- **Nearest start and the Optimize report (#547).** An object takes the nearest start only when
  its own trims do not rise and its travel (approach included) falls. On the 24 editable example
  scripts (`examples/*.json`; `user-fonts` needs a font file), total travel falls 10.1 % on
  average and no design gets a trim more (cutwork 2,488 → 1,885 mm, monograms 1,500 → 1,128 mm,
  line-types 690 → 110 mm; text, stitch patterns and one-object designs unchanged). The issue's
  20 % target is not reached, and the kit (stitch files) keeps its own start, so #547 stays
  open for part-way satin starts and draggable markers.
- sew_check passes on the heart example with a basting box, an outline basting in its own
  thread, stops after an object plus Stop at Jumps, and nearest start, as on the plain design.
- 2026-11-08: the closed-outline rule has its own fixture (a closed outline drawn from its far
  corner starts on the side facing the previous object: `nearest_start_enters_a_closed_outline_at_its_nearest_point`), and `stitchtool optimize
  --auto-start off|nearest` sets the rule from a shell (by colour on a square and a line drawn
  away from it: travel 70.7 → 22.4 mm, trims 2 → 2). Output with the rule off is unchanged.

### Satin shaping, 2026-11-06 (#520-#523, `batch:satin-1`)

All beyond PE-DESIGN and off by default on Manual Punch columns and zigzag lines; with the
options unset, every one of the 476 files the compat run writes is byte-identical to main and
compat stays 454/454. The shared code is in `column.rs`: `pieces` (corners), `short_stitches`,
`Compensation::sides`, `AutoUnderlay`, `centre_walk`, `underlay_at`.

Coupon (a 3 mm star border, a 4 mm zigzag line with a 30° corner, a 6 mm Manual Punch column
with a 60° corner and a 4 mm curved zigzag line), turned (PE-DESIGN) against `cornerStyle:
auto, shortStitchDistance: 0.25`, read from the exported PES:

| Object | Longest stitch, turned → shaped | Stitches | Note |
|---|---|---|---|
| Star border 3 mm | 9.9 → 6.9 mm | 557 → 975 | tips capped; mitred legs sew square stitches at the same spacing across the stitch, where PE-DESIGN's leaning ones are about 1.8× longer |
| Zigzag line, 30° corner, 4 mm | 11.8 → 7.9 mm | 254 → 383 | `design.check` "satin stitches up to 12.3 mm" gone |
| Column, 60° corner, 6 mm | 12.3 → 6.1 mm | 490 → 500 | mitred |
| Curved zigzag line 4 mm | 4.1 → 4.1 mm | 221 → 221 | 0.5 mm cells with 2+ needle drops 43 → 25; one cell rises from 2 to 3 |

- Corners classify on the rung chain's interior angle (≥ 75° turned, < 75° mitre, < 45° cap
  with `auto`); a cap longer than `capMaxLength` is mitred, a leg shorter than the overlap or
  a stitch spacing stays turned. A closed line with corners starts halfway along its first side.
- A line with no corner sharp enough keeps PE-DESIGN's zigzag (and Half Stitch on curves);
  short stitches then thin it further.
- Not done here: skeleton and Auto satin (#675), satin lettering (#676), fill push and a
  region's second underlay (#677), lap corners. #675, #676 and #677 landed since. No sew-out yet: the owner's T-card scan of a
  tapered column with `pullPercent: 3` is still needed for #522's ±5 % width target.

### Satin shaping on skeleton satin and Auto satin, 2026-11-07 (#675)

The options of #520-#523 on the two satin kinds the batch above left out, beyond PE-DESIGN and
off by default. Unset, a skeleton satin sews `skeleton::satin` and an Auto satin region its
PE-DESIGN rungs with Half Stitch as before: compat 468/468, kit unchanged.

- **Corners.** A skeleton satin builds its corners on one rung per centre-line point (square
  to the bisector, reaching the mitre of the column's edges); an Auto satin region on its
  derived centre line's points, each rung reaching the region's outline. Other shaping keeps
  the smoothly turning rungs (every 1 mm on a skeleton; Auto satin's own lattice).
- **Short stitches, compensation per side and per cent, satin looks** as on columns, on the
  rungs without compensation (`pullCompensation` is then each side's default).
- **Under sewing along the column.** `underlayStyle: auto` sews rows along the column that
  follow its width: one where it is under 3 mm, about one per `underlaySpacing` where it is
  wider (rounded down to an odd number so each stretch ends at its far end), 0.5 mm inside the
  edges, then back along the middle. `centreWalk` and `underlay2` as on a column. Other styles
  keep the region's PE-DESIGN under sewing.

Measured with `stitches.points` (no under sewing; stitches, longest stitch, most needle drops
in one 0.5 mm cell), turned → `cornerStyle: auto` → also `shortStitchDistance: 0.25`:

| Object | Turned | Auto corners | + short stitches |
|---|---|---|---|
| Skeleton satin, 60° corner, 4 mm | 278, 4.2 mm, 4 | 316, 4.0 mm, 5 | 316, 4.0 mm, 4 |
| Skeleton satin, 30° tip, 3 mm | 283, 3.1 mm, 4 | 298, 4.4 mm, 5 | 298, 4.4 mm, 5 |
| Auto satin, 90° bend, 5 mm | 313, 7.3 mm, 7 | 280, 6.8 mm, 3 | 280, 6.8 mm, 2 |
| Auto satin, 45° bend, 5 mm | 307, 5.4 mm, 7 | 293, 5.4 mm, 5 | 293, 5.4 mm, 5 |

Skeleton satin already turns smoothly with Half Stitch (#466), so a mitre there adds the seam's
needle points rather than removing a pile; on Auto satin's fanned bends the constructions
halve the densest cell. Tests `satin_shaping_tests::*`, engine
`skeleton_and_auto_satin_take_the_shaping_through_the_command`. No sew-out yet.

### Ripple lines and motif line placement, 2026-11-07 (#541, #544 part, `batch:lines`, part 3)

Beyond PE-DESIGN and off unless chosen: a motif line without groups, free ends or motifs in
turn is PE-DESIGN's, unchanged (compat `line-motif-zigzag-3pt`); PES 0110 export refuses the
ripple type and the motif plan.

- Ripple: a closed line spirals in to its target as one run (no jump, no trim), the scale
  falling evenly along each turn; an open line is sewn back and forth beside itself, the
  copies `spacing` apart towards the target. The target is dragged on the canvas.
- Motif lines: groups with a longer gap after each, motif-free ends, motifs used in turn; the
  motifs are not stretched to fit (PE-DESIGN's line stretches them), so what does not fit
  before the free end stays running stitch. Five more bundled motifs.
- Not done: the motif band on skeleton satins and columns (#544); done in part 4 below.

### Motif bands along satin columns, 2026-11-07 (#544 part, `batch:lines`, part 4)

Beyond PE-DESIGN and off unless chosen: a satin without `motifBand` sews as before (compat
unchanged); PES 0110 export refuses a motif band.

- A skeleton satin, a Manual Punch satin column or a satin line can sew a motif along its
  column instead of the satin: a whole number of copies, each `bandTile` long (default the
  column's mean width), stretched to fill the column end to end. Each copy is mapped from the
  column's left edge to its right edge at every point along it, so on a 2 to 8 mm taper every
  copy touches both edges within 0.2 mm. `bandMirror` flips every other copy across.
- The under sewing and raised padding of the column stay; only the top satin is replaced.
- Still to do in #544: an image per line-type option (done 2026-11-08, #847; #544 closed).

### Decorative hand-embroidery lines, 2026-11-07 (#543, `batch:lines`, part 2)

Beyond PE-DESIGN and off unless chosen: no PE-compatible output changes (compat unchanged),
and PES 0110 export refuses the new types and a slanted E stitch.

- Eight line types laid out in the line's own frame (distance along, offset across), so they
  follow curves and closed outlines: chevron, Cretan, feather, overlock, alternating
  triangles, double zigzag, lattice, stripes. Every stitch is at most 7 mm (longer ones
  split) and no needle point is sewn three times in place.
- Double zigzag returns as the mirror of its first pass, each return point midway between
  two first-pass points on its side, without a jump; stripes keep their row count, or sit at
  the given gaps (±0.2 mm).
- The E stitch's prong angle (default 0, PE-DESIGN's square prongs, unchanged) slants the
  prongs forward at their full length.
- PE-DESIGN 11.50 has E and V stitch and motif lines only; the defaults here are
  OpenStitch's, to be checked on an NV880E sew-out.

### Hand-made lines, 2026-11-07 (#540, #542, `batch:lines`, part 1)

All beyond PE-DESIGN and off unless chosen: a line or satin without them sews exactly as
before, and PES 0110 export refuses them (PE-DESIGN would sew its own plain stitches).

- **Back stitch** (`type: backStitch`): two forward, one back, each back stitch half the
  forward one; the line ends at its far end.
- **Thickness profile** on running lines: each stitch is sewn back and forth an odd number
  of times between `minPasses` and `maxPasses` as the profile (arc, sine, linear,
  smoothstep, stairs, constant or custom points) goes from 0 to 1, passes side by side.
- **Hand-stitch look** on running and triple lines: seeded smooth variation of stitch
  length (within ±`handLength` %, always 0.5-7 mm) and tilt, passes and the odd extra
  stitch; on satins, one side's needle points move in along their crossings.
- **Scribble** (`type: scribble`): a zigzag sewn back and forth `runTimes` times as one run,
  its side offsets moved by smooth seeded noise and never past the band.
- The same seed sews the same stitches on every platform (value noise over the crate's own
  RNG, no platform maths).

### Satin looks, raised satin and column rung edits, 2026-11-06 (#528, #531, #525 part, `batch:satin-2a`)

All beyond PE-DESIGN and off by default on Manual Punch columns and zigzag lines. With the
options unset, compat stays 454/454 and every compat output is byte-identical.

Test design: a 6 mm Manual Punch column bent at 45° and a 4 mm zigzag line, read back from
the exported PES with `tools/sew_check.py`. All six variants pass, and `design.check` reports
no problems:

| Variant | Density (sew_check) | Most drops in one spot |
|---|---|---|
| normal | 19.7 | 5 |
| sStitch | 10.0 | 5 |
| open | 8.3 | 5 |
| double | 37.2 | 9 |
| jagged (out, right side, 1 mm) | 17.5 | 5 |
| raised | 20.9 | 6 |

- Double satin's second pass starts where the first ends, and each of its needle points sits
  midway between two of the first pass's on the same edge (unit test).
- S-stitch bends each crossing into three short stitches. Its first and last crossings bulge
  up to 0.3 mm past the column's end lines.
- The jagged edge is the same for the same seed. Moving inwards, it never crosses more than
  80 % of the column.
- `spacingAt` 1 (unset) is PE-DESIGN's spacing on the longer side.
- Raised satin sews an even number of lanes, so the last ends at the start, next to the
  satin's first needle point. It is fully covered in the render.
- `shape.column` and `object.setDirectionLines` now turn round a pair clicked
  right-then-left instead of refusing it (`column::untangle`). A real overlap is still
  refused.
- Not done: satin regions and skeleton satin (#685), lettering (#686), the canvas rung-edit
  mode (#525, satin-2b), and the NV880E sew-outs #528 and #531 ask for before the styles are
  advertised. #685, #686 and #525 landed since.

### Satin lines with variable width and branch split, 2026-11-06 (#524, #526 part, `batch:satin-2b`)

Both are beyond PE-DESIGN. They are new line types and commands, so no existing design changes
and every compat output stays byte-identical. PES 0110 export refuses a satin line with the
message it gives other unmapped line types; the machine formats sew it.

- **Satin line (#524):**
  - A satin line with one width all along, square to the line, sews the same needle points as
    skeleton satin of that width (engine test).
  - Width keys interpolate smoothly. Left 0 / right 3 mm puts the whole column on one side.
  - Tapers reach a point at the ends.
  - A 30° slant keeps the edges at their widths (unit test).
- **Branch split (#526), `object.deriveSkeleton {branches: true}` on Lato Bold letters at 10
  and 25 mm:**

  | Letter | Columns |
  |---|---|
  | K | 3 |
  | X | 3 |
  | Y | 2 |
  | R | 4 |
  | N | 1 (an unbranched path) |

  - All ten files pass sew_check.
  - The synthetic Y, plus and star are over 90 % covered, and each arm ends `joinOverlap`
    inside the column it leaves.
  - Not met: on K and R the main stroke bends through the junction and fans, leaving a gap in
    the stem. #526's IoU ≥ 0.95 and "no gap over 0.3 mm" are therefore not reached on letters,
    and #526 stays open.
- **Straight stems, 2026-11-07 (#526).** When the main stroke turns more than 40° within
  1.5 column widths (its ends, which hook into corners, not counted), straight strokes between
  the columns' free ends are tried as the main stroke instead. A stroke qualifies when a line fits
  60 % of the middle of its ridge within a third of its half width. Its column keeps that line
  and the nearer side of each chord as its half width, so it does not widen into the junction.
  The split whose satin covers the most of the region is kept, counting the quadrilaterals
  between 1 mm crossings, so a fan's gaps count. Share of the letter's inner cells sewn
  (Lato Bold):

  | Letter | 10 mm before → after | 25 mm before → after |
  |---|---|---|
  | K | 0.902 → 0.985 | 0.926 → 0.984 |
  | R | 0.800 → 0.943 | 0.836 → 0.945 |
  | X, Y, N | unchanged | unchanged |

  K and R now sew their stems from top to bottom as one straight column, with no gap. The legs
  still turn into the stem along their ridge and fan a little where they join. The R's bowl
  leaves a strip by the top of the stem, which is why R stays under 0.95. All ten letters pass
  `tools/sew_check.py`. Tests `letter_tests::*`; the Y, plus and star tests are unchanged.

### Hidden stitching and hidden travel, 2026-11-06 (#511, #545, `batch:routing`)

Beyond PE-DESIGN and off by default: with no object or design setting on, compiled output is
byte-identical (tests `hidden_stitching_is_left_out_with_an_overlap`,
`travel_under_a_later_fill_replaces_the_trim`, `run_connectors_join_objects_of_one_thread`).
PE-DESIGN's destructive `shape.removeOverlap` is unchanged.

- **Leave out hidden stitching (#511).** The region is generated over its outline minus the later
  fills and satins shrunk by the overlap; the generators are not changed, only the area they are
  given, and the outline is sewn as drawn. Two overlapping 30 mm squares with a 1 mm overlap: the
  stitches saved are within 15 % of the hidden 14 × 14 mm share of the lower fill; no lower-fill
  needle point lies deeper than 1.2 mm under the upper square, and some lie in the 1 mm band.
- **Travel under later fills and run connectors (#545).** A move into an object of the same
  thread (no colour change or stop before it) is sewn as ≤ 2 mm running stitches when a path,
  straight or round one corner, lies 0.5 mm inside the fills sewn from that object on. A small
  square under a larger one of the same thread: 1 trim between them with it off, 0 with it on.
  On the editable example scripts few objects of one thread lie under later fills (401 → 400
  trims), and the kit's stitch files keep their own moves, so the jumps-to-running pass and
  per-design kit reductions remain open in #545 (closed since, after #789 and #804).
- **Fill travel on curved outlines (#679).** The travel graph and the row splitting tested every
  point against `Contour::path`, which flattened the Béziers again on each call; both now flatten
  each outline once (the same points). Fill layers of a 60 mm ring with a 30 mm hole on Bézier
  curves (debug build, 5 builds): 2.7-5.1 s before, 0.48-0.65 s after, within 1.1-1.4× of the
  same ring flattened first (`paths::tests::curved_layers_timing`). PES output byte-identical on
  the 24 exportable example scripts; compat 454/454.

### Auto-route, Redwork and branching, 2026-11-06 (#546, `batch:routing`)

Beyond PE-DESIGN; nothing changes until `shape.autoRoute` or `object.branch` is run.

- **Auto-route.** The lines are split at crossings and where an end comes within `join`
  (0.5 mm) of another line; each connected piece is one walk. With more than two odd vertices,
  the shortest paths between them are sewn twice, paired by a minimum matching (exact by
  bitmask up to 16, greedy above), leaving the two whose pairing costs most as the walk's
  ends; Hierholzer's walk takes the straightest unused line at each vertex. A "Y" of three
  lines: one line, no trims, the shortest leg sewn twice (test
  `a_tree_of_lines_sews_with_no_trims`); a 2 × 2 window grid doubles exactly 20 mm.
- **Redwork.** Every line twice, the walk closed: a branch of three lines is 2 × 60 mm and ends
  where it started.
- **Branching.** Touching shapes in one thread, ordered nearest-next, each entered at the
  closest point to the one before (the `minJump` link of `order.optimize`): a chain of three
  touching rectangles in two colours sews with no colour change and no trim, and stays so after
  a member is moved; moved apart, that member trims as usual. Satin branching with the
  centre-run underlay of the B4 sketch (§5) is not done.

### Tie-off after tiny last stitches, 2026-11-07 (#703)

A run that ends in stitches of 0.3 mm or less (a fill's last clipped row pieces, a satin
narrowing to a point) followed by its lock sewed up to 7 stitches in one spot, which
`tools/sew_check.py` refuses (limit 6). The writer's tie-off now drops the run's own needle
points just before the lock that make the run longer than 6 (0.35 mm counts as in place, for
the machine file's 0.1 mm grid); the last needle point and the lock stay. Stitch files and other
fixed needle points never lock, so they are not touched.

Measured with Auto Punch in OpenStitch mode on p01 and p09 (long side 100 mm), with no preset
and with each of the six presets: before, p01 with `lineArt` failed with 7 in place (the other
13 runs passed on main by then); after, all 14 pass, the longest run in place 4-6. Compat
454/454, and its 501 written files are byte-identical apart from two JEF time stamps.
Tests `tie_off_tests::*`.

### Tie-in at a pointed start, 2026-11-07 (#732)

The start of the case above. A run that begins at a pointed tip (a fill's rows 0.3 mm long
and 0.2 mm apart, a satin narrowing to a point) after the object's own one-step tie-in, out
0.3 mm along the first stitch and back, sewed 7 stitches in one spot: Auto Punch in OpenStitch
mode with the `photo` preset on g11 (long side 100 mm), where small fill pieces joined by a
running connector carry their own tie (#713). Now, when the tie-in and the tip together would
sew more than 6 stitches in place (0.35 mm counts as in place), the tie-in steps out to the
tip's first needle point beyond a stitch in place (at most 1 mm away) and back, instead of
0.3 mm along the first crossing. Every needle point of the tip is kept. Only the object's own
ties (OpenStitch mode and `object.setFinish`) do this, and a tie set longer than 6 stitches by
itself is sewn as set; PE-DESIGN's own tie-in is unchanged.

Measured: g11 with no preset and the six presets, `tools/sew_check.py --hoop 130x180`, was 6 of
7 (photo: 7 in place), now 7 of 7; the same stitch counts, one tie-in step moved in each file
(two with photo). Likeness of g11 with the
photo preset (`tools/image_similarity.py --only g11-ap --mode openstitch`) unchanged: ΔE00 7.39,
MS-SSIM 0.6360. Nine other images with four presets each (36 files) are byte-identical. Compat
468/468; the ready-to-sew kit is unchanged. Test
`finish_tests::a_pointed_start_ties_in_along_the_tip` (the g11 piece itself).

### PE-compatible Photo Stitch 1: one path a colour, 2026-11-07 (#298)

PE-DESIGN 11.50 sews Photo Stitch 1 nearly one path a colour. Round-8 capture of the sunset (p06)
at 90 mm, defaults: 11 trims in 8,683 stitches (1.3 per 1,000), 0-3 trims a colour, a jump only
at each colour's start, every stitch 2.5-4.5 mm (Run Pitch 3.0). Its travel is in sight.
OpenStitch's PE-compatible mode jumped and trimmed wherever no hidden way led to the next run:
108 trims (10.8 per 1,000) on the same picture.

Now, where no hidden way leads on, the thread travels in sight to its nearest unsewn run in
stitches of at most 3 mm, and trims only past 40 mm (at 20 mm: 18 trims). The sunset at 90 mm
sews 14 trims in 9,744 stitches (1.4 per 1,000). OpenStitch mode is unchanged: its Trims setting
is the user's (and the owner's call after a sew-out, see #298).

Likeness of PE-compatible mode on the nine Photo Stitch 1 cases (`tools/image_similarity.py
--mode pe --no-dists`, before → after): trims 1,667 → 120. dE00 is better on seven, for example
p07 18.32 → 17.89 and p06 19.12 → 18.82, and worse on p05 (+0.17) and p11 (+0.22). MS-SSIM falls
0.002-0.025 (p11 0.699 → 0.675), as the travel PE-DESIGN also sews now shows in the render.
Test `digitize::compatible_photo_stitch_trims_about_as_rarely_as_pe_design`.

### Auto Punch line art: dots without knots, a colour's pieces joined, 2026-11-07 (#713)

OpenStitch mode only, on line art; PE-compatible mode is unchanged. g11 (the mandala, 100 mm)
failed sew_check with every preset but patch: its drawn dots sewed as knots (tie-in, rows and
lock: 7 in one spot) and its 1,015 pieces trimmed one by one in drawing order (77.5 trims per
1,000 stitches).

- A colour's pieces are put in nearest-next order (one thread, so nothing that shows changes)
  and two whose bounds are 2 mm or less apart are joined by a running connector (#545).
- The pieces a connector joins, and the drawn dots (every stitch within 0.8 mm, which the speck
  rule leaves out elsewhere, #690), lose their fills' own tie-in and lock (`Finish.tie_count`,
  #548); the writer still ties them in after a trim and locks them before one.
- A fill whose rows would stand in place on a hairline turns its rows along it.

Measured (debug build, `tools/image_similarity.py --no-dists`, `sew_check --hoop 130x180`):

| | sew_check (none + 6 presets) | trims / 1,000 | MS-SSIM / dE00 |
|---|---|---|---|
| main | 1/7 | 77.5 | 0.6529 / 7.23 |
| this change | 6/7 | 45.2 | 0.6529 / 7.23 |

The photo preset still sews 7 in place at pointed satin tips after their tie-in (#732, stitch
engine). Stars or thinned points for the dots were tried and lowered MS-SSIM to 0.633-0.652
(table on #713).

## Order of work

1. **Fix without a reference:** items 1, 3, 7, 14, 15 (ranges), 16 and 17, plus the
   documented PES fields: 2, 18 and 19. Each comes with the test above and becomes an internal
   regression.
2. **Black-box checks of OpenStitch output in the owner's PE-DESIGN:** items 2 and 5 (short
   term), 4 and 9. These are the first Reference Verified rows.
3. **Reference capture (method 1):** calibrate the defaults and settle 6, 8, 11, 13 and 20.
4. **Output-file study (method 2):** editable shapes and text (5), PES v7–v11 and undocumented
   fields, from the owner's PE-DESIGN files only (track `pes-objects`). Decompiling (method 4)
   stays prohibited pending legal review.

The harness for steps 2 and 3 is built (2026-10-19). It is `stitchtool compat`, in the
crate `crates/openstitch-compat`, and is described in
[COMPATIBILITY_TESTS.md](COMPATIBILITY_TESTS.md). It has four parts:

- **Test specs:** `compat/specs/<area>/<id>.json`. Each holds the command script, the exports and
  the measurements listed in the items above, with tolerances and evidence. The tests for items
  1–3, 7 and 14–19 are converted; the Rust tests stay.
- **Runner:** it executes each script headlessly in a sandboxed session, as `stitchtool exec`
  does, and writes `target/compat/report.json` and `summary.md`. CI runs it on the ubuntu-latest
  job.
- **Semantic comparer:** it compares stitch coordinates, commands, colour changes, jumps, trims,
  order, per-block counts, threads, dimensions, metadata and the PES object summary, with
  normalised origins. Every difference is reported with its location.
- **Status tiers, kept apart:** internal regression, oracle, reference (PE-DESIGN files in
  `compat/reference/` with a provenance manifest) and machine verified (sew-out records in
  `compat/machine/`). The generated [COMPATIBILITY_STATUS.md](COMPATIBILITY_STATUS.md) shows the
  tier each item and FEATURE_MATRIX row has reached.

The audit items' reference checks are written and wait for the owner's files: the specs report
them as "pending reference". Step 2 starts when the owner adds the first file, following
[compat/reference/README.md](../compat/reference/README.md). (The first files came on
2026-10-27; specs still pending a file are listed in COMPATIBILITY_STATUS.md.)

### PES versions 4, 5 and 7–10 written as PE-DESIGN saves them, 2026-11-07 (#374, `batch:formats-pes`)

From PE-DESIGN 11.50's own Save As files (24, fixtures/pe11/captures-2026-11-07/pes-versions; owner notes
`pes-versions`), black-box only.

- PE-DESIGN saves every version below Ver.11.0 as stitch data. A design with editable objects
  is offered Ver.6.0 and up only, so versions 4 and 5 exist only for stitch-data designs.
- `design.export` `pesVersion` 4, 5, 7, 8, 9 and 10 write PE-DESIGN's layouts. The props design
  (compat pes-design-properties) comes out byte for byte in all six versions, except PE-DESIGN's
  pictures (PEC thumbnails, the 144-byte thread-number pictures) and the source's metadata
  encoding. The turned 130 × 180 hoop matches from the thread table on; the header's hoop
  fields match too, in each version's own way of writing a turned hoop.
- Two reader and writer fixes came with it: v10's long run lengths (`FFFF` + u32) are read; a
  thread of colour type 0 gets the `FE FF FF` code after the PEC section, as PE-DESIGN writes it
  (also in `#PES0110`).
- Not written: the thread-number pictures (blank, as in `#PES0110`), a custom page's extra picture
  after the addendum, and the hoop id and name in v9/v10.

### Satin N, W, Z and M in one run, 2026-11-07 (#446)

PE-DESIGN 11.50 sews satin N, W, K and Z in Lato Bold in one run and M with one trim
(stroke-satin-N/M/W/K/Z.pes at 20 and 8 mm, round 8, #719). OpenStitch cut the letters apart at
their acute corners and trimmed to each piece: N three trims at 20 mm (with its last), W four,
Z two, M three.

- A piece cut out of one stroke with no junction (N's stems and diagonal) is sewn from the end
  whose far end lands nearest a piece still to sew, so the needle runs on into it.
- A letter's next piece up to 12 mm away, straight through the letter, is travelled to in
  running stitches instead of trimmed to (N's left stem ends at the notch, 8.6 mm below the
  corner where the diagonal starts; the diagonal's satin covers that way).

Trims including the last, PE / before / after (stitchtool info on the spec's script):

| | N 20 | N 8 | M 20 | M 8 | W 20 | W 8 | K 20 | K 8 | Z 20 | Z 8 |
|---|---|---|---|---|---|---|---|---|---|---|
| PE-DESIGN | 1 | 1 | 2 | 1 | 1 | 1 | 1 | 1 | 1 | 1 |
| before | 3 | 3 | 3 | 2 | 4 | 1 | 1 | 1 | 2 | 1 |
| after | 1 | 1 | 2 | 2 | 1 | 1 | 1 | 1 | 1 | 1 |

Needle points stay within the specs' 25 % (N 20 mm 283 → 274 against 294). Words: NNN at 6 mm
9 → 3 trims, KNOX at 8 mm 7 → 3, ROUND AND ROUND at 8 mm 19 → 12, MAZE WORK at 10 mm 10 → 9.
No travel is sewn outside the letters (`small_lettering_shows_no_travel_outside_its_letters`);
compat 476/476. Still open on #446: N's corner spike (the diagonal's rungs fan into the corner,
where PE-DESIGN sews a small corner block), M at 8 mm, A's and V's slanted feet.

### Design Center: small rings sew round, 2026-11-12 (#469)

OpenStitch mode only. A ring of the drawing traces as a few uneven points, and the line's points
were thinned to at least 1 mm apart, so a 2 mm ring sewed as a square and a 6 mm one as 13 uneven
sides. A small closed line close to a circle (`center::small_circle`: a least-squares fit, every
point within a fifth of the radius, at most 30 mm long) now sews each pass as an even polygon
round that circle, as many sides of at least 1 mm as fit (five at least), starting where the
traced line started; other small round loops are sampled evenly along their curve. Rings of
2–6 mm (0.4 mm stroke): sides 1.0–1.07 mm, worst sag inside the fitted circle 0.11–0.24 mm (was
0.11–0.38 mm). Every ring already started and ended on its line (no stub inside; the only point
inside is the closing lock on the last side). PE-compatible mode is unchanged (it sews the traced
line at its run pitch, 11–26 stitches a ring); there is no PE-DESIGN capture of Design Center on
small circles to change it against. Kit `S-design-center-butterfly-openstitch`: 4,231 → 4,482
stitches, 66 trims, 8.7 → 9.0 min.

### Embossing and engraving as strong as PE-DESIGN's, 2026-11-12 (#471)

PE-DESIGN 11.50's round-10 captures (`471-*.pes` in the owner's notes; the plain square is
`compat/reference/stitches/stamp-plain-square-baseline.pes`, the three stamped ones wait for PES
0110 import of a shape that carries a stamp, #959): a 42 mm square
with its default Fill (4.5 lines/mm, 45°, under sewing) and 2 mm Zigzag line, and a 16.5 × 16 mm
circle in the middle converted with Stamp/Emboss > Convert from Outline. OpenStitch builds the
same with `shape.emboss`. Inside the circle against outside (1 mm in from the circle and 3 mm in
from the square's edge), fill block only:

| effect | PE-DESIGN | OpenStitch before | OpenStitch now |
|---|---|---|---|
| none | 4.0 mm stitches, 125 / 128 needle points per cm² | 4.0 mm, 124 / 128 | same |
| engraved region | 1.9 mm, 262 per cm² | 1.0 mm, 480 | 1.9 mm, 262 (2 mm steps) |
| embossed region | 56 per cm² (13 within 6 mm of the centre: under sewing only); whole stitches 12–14.3 mm, a few short ends | 110 per cm²; split evenly, 7–8 mm | 26 per cm² (11 within 6 mm); whole stitches up to 14.5 mm, the rest last |
| engraved line | +118 needle points on a ring about 1 mm inside the circle | +142 on the circle | same |

The engraved region and the emboss were the weak ones: an engraved region twice as dense as
PE-DESIGN's, and an embossed one split into 7 mm stitches, so the raised look hardly showed
(the full-size review's "barely visible"). Both now use stamp constants of their own
(`STAMP_ENGRAVE_STEP` 2 mm, `STAMP_EMBOSS_MAX` 14.5 mm), and the compiler no longer splits an
embossed stamp's stitches at its 12 mm maximum; `tools/sew_check.py` warns on them as it does on
PE-DESIGN's own 14.3 mm. The embossed band right inside the circle keeps more needle points in
PE-DESIGN's file, where its edge lies about 1 mm further in (#956). Left: the engraved line's ring sits on
the tool's outline, where PE-DESIGN's lies about 1 mm inside, at the inner edge of the tool
shape's 2 mm line (one capture cannot tell whether that offset follows the line width, #956);
PE-DESIGN's fill stops at the inner edge of its zigzag line (40 × 40 mm), OpenStitch's reaches
the outline (42 mm, #955); programmable-fill patterns' own engraved and embossed areas keep 1 mm and
7 mm, with no reference yet. Kit: `fills/holes-and-emboss` 10,733 → 10,112 stitches, 13.7 →
12.9 min, trims 5.

### Photo Stitch 1 trims rather than cross a face in sight, 2026-11-12 (#915)

PE-DESIGN-compatible Photo Stitch 1 travelled in sight to a colour's nearest unsewn run up to
40 mm away (#298), and the darkest threads, sewn last, have nothing to hide under: on the
portrait p01 (long side 100 mm) 31.7 mm of near-black thread crossed the face's pink.
PE-DESIGN 11.50's capture of the same image (`compat/reference/image/photostitch1-p01-portrait-100mm.pes`,
round 10) runs its stitches in every direction and trims 37 times in 13,396 stitches (2.8 per
1,000); none of its ten most visible straight stretches lies over the face, and the longest is
12.5 mm, dark on dark.

An in-sight travel is now taken only when at most 8 mm of it (`PE_TRAVEL_SHOWN_MM`) lies over
cells that are neither the thread's own nor a later layer's; of the 16 nearest run ends the
first such one is sewn, and with none the thread trims. Trims per 1,000 stitches, the longest
travel left showing, and the longest dark travel over a colour at least 0.3 lighter:

| photo | before | 8 mm (taken) | contrast-aware: 12 mm, 4 mm dark on light |
|---|---|---|---|
| p01 portrait | 0.8; 31.7 mm | 1.9; 8.4 mm; 4.5 mm (grey on grey) | 1.6; 11.5 mm; 7.7 mm dark grey on skin |
| p03 storybook | 0.7 | 2.3 | 1.5 |
| p04 cat fur | 0.6 | 2.2 | 1.6 |
| p06 sunset, 100 mm | 1.3 | 2.9 | 2.0 |
| p06 sunset, 90 mm (PE-DESIGN 1.27) | 1.4; 18.5 mm | 2.8; 9.5 mm | 2.2; 13.4 mm |
| p10 night street | 0.6 | 1.2 | 1.0 |
| p12 backlit tree | 0.8 | 2.0 | 1.9 |

The contrast-aware rule trimmed less but let dark grey show across the portrait's skin, so the
plain 8 mm rule was taken. Against PE-DESIGN the portrait trims less (1.9 against 2.8) and the
sunset more (2.8 against 1.27): PE-DESIGN's paths follow contours, ours stay in rows, so where
its runs meet a row's neighbour ours have to cross. Guarded by
`compatible_photo_stitch_trims_about_as_rarely_as_pe_design` (sunset at 90 mm at most 3.5 per
1,000; portrait 1.4–3.5) and the spec `photostitch1-p01-portrait-travel`. OpenStitch mode is
unchanged.

Kit (regenerated with `sew_kit.py --only`), before → after: `M-photo1-portrait-pe-compatible`
13 → 38 trims, 26,537 → 26,563 stitches, 33.8 → 35.2 min; `S-photo1-sunset-pe-compatible`
14 → 27 trims, 9,744 → 9,796 stitches, 12.9 → 13.6 min. The portrait's preview shows no long
dark line across the face; the sunset's long dark lines across the sun and its reflection are
gone, and short dark stretches stay in the upper right sky.

### Photo Stitch 1's in-sight travel limited by contrast and along a chain, 2026-11-12 (#915)

#936's rule let one in-sight travel show at most 8 mm. Two things still read as dark lines on
the portrait's face:
- several such travels end to end;
- dark thread over the light skin within the 8 mm.

The rule now, for PE-compatible mode only:
- **Two sums along a chain.** It adds up what the thread's in-sight travels show since its last trim, hidden move, or run of its own at least 2 mm long (`PE_CHAIN_BREAK_MM`). Each sum has a limit:
  - **12 mm in all** (`PE_TRAVEL_SHOWN_MM`; PE-DESIGN's own longest visible stretch on p01 is 12.5 mm);
  - **nothing dark on light** (`PE_TRAVEL_DARK_SHOWN_MM` = 0): no travel over a colour, or the fabric, at least 0.3 lighter in luminance than the thread (`DARK_ON_LIGHT`), the test batch-production's measure uses.
- **No trim or hidden move needed to end a chain.** Breaking it only there trimmed far more, for example the sunset at 90 mm at 3.2 per 1,000.

Measured with batch-production's `photo915.py` (straight travel stretches of 3 mm or more across the rows; "showing" is where neither the thread nor a later one lays 70 % of a full cell). Main is #936's 8 mm rule:

| photo | trims per 1,000, main → now | dark on light showing, mm | longest dark on light, mm | longest showing, any colour, mm |
|---|---|---|---|---|
| p01 portrait (PE-DESIGN 2.8) | 2.0 → **2.4** | 35 → **12** | 7.8 → **4.5** | 10.3 → 11.9 |
| p03 | 2.3 → 2.6 | 13 → 0 | 5.6 → 0.2 | 12.5 → 12.6 |
| p04 | 2.2 → 2.7 | 63 → 17 | 8.3 → 5.1 | 11.7 → 15.6 |
| p06 sunset, 100 mm | 2.5 → 2.3 | 10 → 4 | 5.2 → 2.7 | 9.4 → 14.8 |
| p06 sunset, 90 mm (PE-DESIGN 1.27) | 2.65 → 2.74 | 5 → 0 | 5.0 → 0 | 9.5 → 10.5 |
| p10 | 1.2 → 2.2 | 55 → 35 | 6.9 → 4.1 | 10.0 → 11.3 |
| p12 | 2.0 → 2.4 | 36 → 18 | 7.6 → 3.6 | 12.1 → 12.2 |

The longest stretches showing in any colour are dark on dark or the same tone. The measure reads a few a little longer than 12 mm for two reasons:
- **Coverage:** a cell a later thread owns counts as covered in the rule, but only when that thread lays 70 % of a full cell in the measure.
- **Merged travels:** the measure joins travels on one straight line, even across a 2 mm run that breaks the chain.

Likewise the dark on light it still finds (p01's 4.5 mm of dark grey on the face's gold) lies over cells the dithered grid gives to a later thread, so the rule sees it as covered.

**Settings measured** ("all", "dark" and "break" in mm; p01's trims are the trim commands in the exported PES, which `image/photostitch1-p01-portrait-travel` counts, floor 19):

| all / dark / break | p01 trims per 1,000 (PES trim commands) | sunset 90 mm per 1,000 | p01 dark on light, mm / longest | longest showing, mm | spec |
|---|---|---|---|---|---|
| 8 per travel (main, #936) | 2.0 | 2.65 | 35 / 7.8 | 9–12 | passes |
| 12 / 4 / no break | 2.4 | 3.2 | 15 / 5.2 | 9–16 | – |
| 20 / 4 / 2 | 1.1 | 1.5 | 22 / 5.2 | 15–22 | fails |
| 16 / 3 / 2 | 1.6 | 2.1 | 18 / 5.2 | 13–21 | fails |
| 12 / 3 / 2 | 1.6 (15) | 2.2 | 21 / 7.8 | 12–16 | fails (15 < 19) |
| 20 / 0 / 2 | 2.4 | 2.5 | 12 / 4.5 | 13–22 | – |
| **12 / 0 / 2 (taken)** | **2.4 (26)** | 2.74 | **12 / 4.5** | 10–16 | passes (PE-DESIGN 27) |

Allowing any dark on light lets p01 trim less than the spec allows. PE-DESIGN trims more on the portrait (2.8 per 1,000) than on the sunset (1.27), and travel rules on our rows can't separate the two, so the sunset's 1.3–1.5 per 1,000 is left for a later step.

### PEC chart colours against every PE-DESIGN capture, 2026-11-12 (#915)

The new portrait reference failed `pe_design_thread_tables_match_the_pec_chart`: PE-DESIGN
writes 333 AMBER RED (PEC 49) as (181, 76, 100), the chart had pyembroidery's (181, 75, 100).
Every BROTHER EMBROIDERY thread in the raw thread tables of the 1,421 `#PES0110` captures on
the owner's PC (compat/reference, the pe11 fixtures and all reference rounds) was compared
with the chart: 55 of the 61 codes are used, 53 agree. 333 differs in all 14 files that use it
(Photo Stitch 1 and Auto Punch, rounds 5, 8 and 10) and is corrected, in New Brothread too.
812 CREAM YELLOW differs in one Auto Punch file whose thread table also holds a Ver4 Option
Color, so it waits for a capture from the BROTHER EMBROIDERY chart; 337, 810, 612, 079, 542
and 202 are in no capture yet. Effect: OpenStitch mode's gradient golden hashes (the colour, and
Amber Red one place earlier in Auto Punch's lightness order) and the kit's
`S-photo1-sunset-openstitch` (Amber Red sewn before Vermilion: 117 → 102 trims, 7,923 → 7,929
stitches, 15.9 → 15.1 min). No PE-compatible thread choice changed in the kit.

### A's apex runs on; run-on arms start along their end cuts, 2026-11-12 (#446)

PE-DESIGN 11.50 runs A's right leg (`\`) on to the apex's flat top and stops the left leg
against it, as it does at V's foot (stroke-satin-A: the right leg's first crossings level, 0-6
degrees; the left leg's first ones along the right leg's inner edge, 56-65 degrees). The
slanted run-on now takes top apexes as well as feet (`strokes::slanted_run_on`). A run-on arm's
level end edge is dropped from its sides, keeping its two corners, so its crossings start
along that edge rather than fanning into the tip; until now this was kept to X's arms
(`GlyphStrokes::sides_along`). An end stopped against a run-on arm keeps each side to where it
meets that arm, so its last crossing lies along the stop line. Fill text keeps the sides its
rows were measured with (#376). Lato Bold, needle points: main (0d085861's stroke code,
measured on 448155a6) → now (8b3f4463, release build), against PE-DESIGN:
- satin A 283 → 284 (290): the right leg is 103 → 111 crossings (114), from the apex's top,
  its first crossings 171-179 degrees where main's were 138-155;
- satin V 223 → 226 (228); satin M 350 → 358 (374); satin W 402 → 382 (363);
- fill A 461 → 469 (479); fill V 398 unchanged (397); satin M at 8 mm unchanged (148, 165);
- further: satin W at 8 mm 159 → 162 (155).

N, X, K, Z and S are unchanged. Every spec passes, 485/485. Still open on #446:
- PE-DESIGN's order at a run-on foot: the run-on arm sewn to the meeting point, the foot's tip as
  a second pass, and a fan into the stopped arm's foot. Tried as a split of the run-on block, it
  reached only fill text and took fill V from 398 to 383, so it was left out.
- A's left foot (its crossings tilt early: about 22 degrees at its middle, PE-DESIGN's about 9).
- The bar's reach into the legs.

### A slanted arm runs on at V's, M's and W's feet, 2026-11-12 (#446)

At a foot where two slanted arms leaning opposite ways meet, PE-DESIGN 11.50 runs one arm on to
the outline and stops the other against it, as it does with N's stems (stroke-satin-V: the left
arm down to the baseline; M's middle V, W's feet). OpenStitch cut both arms at the corner and left
an unsewn wedge between them. The arm that runs on is the one falling to the right (`\`, the
thick stroke of a classic V), and only at a foot, where the arms come down into the cut and go up
out of it (`strokes::slanted_run_on`, called from `run_on_stems` beside its upright case); the
other arm stops half the run-on arm's half width from its centre line. Top apexes (A's, W's
middle) stay cut as before: running A's right leg on fanned its top crossings to 144° against
PE-DESIGN's near-level ones. Lato Bold, needle points main (7d785c86) → now (448155a6, release
build) against PE-DESIGN: satin V 208 → 223 (228), M 347 → 350 (374), M at 8 mm 145 → 148 (165),
fill V 377 → 398 (397); further: W 384 → 402 (363) and W at 8 mm 156 → 159 (155), whose feet are
now sewn where main left wedges. Satin and fill A are unchanged (283, 461). Every spec passes,
485/485. Still open on #446: A's apex, A's left foot (the level cut on a slanted leg) and the
crossings' slant along slanted arms (PE-DESIGN's are level).

### Satin N's stems run on past its corners, 2026-11-08 (#446)

PE-DESIGN 11.50 sews satin N's stems the whole height of the letter and stops the diagonal
against them (stroke-satin-N, -N-8, stroke-blocks-satin-N, round 8). OpenStitch cut N's centre
line at its two acute corners, so each stem stopped at the notch's depth (7.9 mm below the top at
20 mm) and the diagonal's crossings fanned into the corner beyond it: the spike of #446. Now,
where an upright stem (within 12° of vertical) meets a slanted piece at such a cut, the stem's
centre line runs on to the outline and the slanted piece stops at the stem's edge, its satin
starting where both its sides run (`strokes::run_on_stems`, `StrokeEnd::Against`). N's stems
are full height at both corners; needle points 274 → 282 at 20 mm (PE-DESIGN 293) and 116 → 118
at 8 mm (123). No other text spec changes; compat stays at 478/478.

Still open on #446: the diagonal's first crossings still pivot near the notch over the small
triangle above it, where PE-DESIGN sews a separate corner block (stopping the diagonal 1.5 stem
half-widths out instead lost needle points and left a gap: 272); M, whose corners come from
thinning junctions rather than cuts, is unchanged, as are M at 8 mm and A's and V's feet.

### Fill text under a Text Transform sewn by stroke; envelope 10 refitted, 2026-11-11 (#428)

PE-DESIGN sews fill text under a Text Transform stroke by stroke, as straight text
(text-transform-9: level rows on the slanted stems). OpenStitch kept region rows there because
three references failed by stroke. Two causes, both fixed:

- A free end whose end edge the warp tilts (descenders, stem feet) lost its lower corner: a
  side is trimmed to the stretch both sides cover, and the leading corner projects past the
  other side's last point. A free end now keeps each side's corner
  (`strokes::sides_along`); on straight text only O's two specs change their files, every check
  the same. text-transform-18-openstitch-l100 passes.
- Envelope 10 (a back slant) was a parallelogram held inside the straight box, the top edge from
  0 to 1 − d and the bottom from d to 1. PE-DESIGN's reaches 0.4 mm left of the box and its
  letters are 0.6 % further apart. `tools/fit_envelopes.py` now fits the slants as a
  parallelogram with one slope and an offset per edge, least squares over every letter's corners
  in the reference's own frame (`stitches.import` centres a file, which moved this one 0.15 mm).
  Only preset 10 is replaced in `envelope_presets.rs`: the generator no longer reproduces the
  other presets' committed values, so they are left as they are.

Envelope 10, by region, before → after (PE-DESIGN): text-transform-10 min-x 12.6 → 12.4
(12.3), stem column 3 bottom 48.6 → 49.2 (49.0), column 1 42.3 → 42.7 (42.5), column 5 40.5 →
40.7 (40.9), column 8 bottom 47.8 → 48.2 (47.8), column 14 top 47.4 → 47.2 (47.4); -openstitch
16.7 → 16.4 (16.4), -l0 17.1 → 16.6 (16.6), -l25 17.1 → 16.7 (16.7), -l75 15.3 → 15.2 (15.2),
-agjy 25.1 → 24.9 (25.2), -l100 12.6 → 12.8 (12.7): all within tolerance, the boxes nearer, two
columns 0.2–0.4 mm further. No other envelope changes. By stroke every PE-DESIGN reference
passes; the internal pes-0110 transform snapshot is re-taken (arch 79.6 → 79.8 mm wide, 25.7 →
26.1 mm high, PE-DESIGN's re-sew 79.20 × 26.50).

### Satin M's stems run on past its top corners, 2026-11-11 (#446)

PE-DESIGN 11.50 sews satin M's stems the whole height of the letter, as it does N's, and stops
each diagonal against its stem (stroke-satin-M, -M-8, round 8). In OpenStitch, the thinning left a
small loop in each of M's acute top corners. The notch's tip is narrower than a grid cell, so a
few outside cells there are sealed off, and the thinning goes round them. Each corner then became
a junction instead of a corner cut, so `run_on_stems` never saw it:
- at 20 mm, the left corner fanned;
- at 20 mm, the right diagonal's top was a single travel line beside a gap;
- at 8 mm, the letter sewed in two runs, 0.3 mm short.

Now such a loop is pruned as a thinning knot (`Graph::pruned`) when all of these hold:
- it loops back to its own junction;
- it is under 4 times the junction's clearance (`NOTCH_LOOP`);
- it goes round at most 3 sealed outside cells (`KNOT_CELLS`; counters are larger, and are found
  by flooding the outside from the grid's border);
- the junction's two other branches leave it straight over 6 clearances (`KNOT_ARM`).

The last condition keeps the knots in Pacifico's curved a and m: pruning those piled 22 needle
drops into a 0.5 mm cell at 7 mm, above sew_check's 20. M's corners then become cuts. The stem
runs on from its own line, dropping the thinning's bend into the corner (`STEM_BEND`). Against
PE-DESIGN:

| | needle points | trims | mean needle distance, ours / PE's | coverage IoU |
|---|---|---|---|---|
| M 20 mm | 343 → 347 (PE 374) | 1 → 0 (PE 1) | 0.267 / 0.317 → 0.224 / 0.235 mm | 0.709 → 0.783 |
| M 8 mm | 138 → 145 (PE 165) | 1 → 0 (PE 0) | 0.171 / 0.270 → 0.136 / 0.160 mm | 0.714 → 0.820 |

M at 8 mm is now 8.3 mm tall, as PE-DESIGN's (8.0 before). A sweep of A–Z, a–z and 0–9
in Lato Bold at 8 and 20 mm changes 9 of 124 glyphs:
- M;
- N, by one needle point;
- 4: its stem runs straight to the top instead of fanning;
- 6: it sews its bowl as stroke satin, not fixed-direction rows with travel across the counter;
- 9 at 8 mm: its tail becomes satin along the stroke.

There are no PE-DESIGN references for the digits.

Still open on #446:
- M at 20 mm now has no trim where PE-DESIGN trims once.
- V's foot and the foot of M's middle V: PE-DESIGN runs one arm on to the baseline and stops the
  other against it.
- A's feet and V's top ends fan.
- N's corner block.

### No visible connectors in blends and tartan; #914's figures corrected, 2026-11-12 (#901)

#914 measured the longest exposed travel only on stretches of two or more stitches. The design
compiler also sews a single straight stitch across a gap of up to 4 mm between one object's
runs (its connect distance), and in a blend those stitches crossed the other colours: with
single stitches counted, main shows 3.2 mm across the rows on the radial noise blend of
`blend_travel_never_runs_far_across_the_rows` (now counting them; it fails on main and passes
here); #914's 1.9 mm for the kit swatch left those stitches out. The owner chose no visible connectors:
a blend or tartan colour's runs are its separate pieces, so the compiler trims between them
instead of sewing that stitch, and `join_within` first chains the runs end to end wherever a
hidden way (at most 2 mm showing) links one run's end to another's (shortest links first, each
run entered and left once, no loops).

| kit swatch | trims | stitches | sewing |
|---|---|---|---|
| `fills/gradients` (three small dithered blends) | 58 → 162 | 9,054 → 9,685 | 14.3 → 20.4 min |
| `fills/tartan` | 6 (unchanged) | 3,514 | 4.7 min |

A dithered blend breaks each colour into many small pieces, hence the trims; `rows` and `bands`
trim far less. Fewer trims with the same look needs a dither that keeps a colour's pieces
connected (#951, Future).

### Blend travel hidden before trimming, and tartan too, 2026-11-12 (#901)

Beyond PE-DESIGN, so there is no reference. #891 kept each blend colour's travel to 2 mm, but at
a cost: the kit's gradients design went from 1 trim to 71. Now `paths::join_within` hides a
colour's travel before it trims. Travel counts as hidden only over 1 mm cells where the colour's
own rows cover at least 70 % of what rows at the row spacing would sew there; over thinner cover,
the other colours' rows between its own show it. The join takes the least exposed of these ways:
- the inside path;
- back along the run just sewn (up to 30 mm) to its point nearest the next piece, then across.

A way is sewn when no more than 2 mm of it shows. Pieces are still sewn nearest first, but where
the nearest can only be reached by a trim, the nearest of the next six that can be reached hidden
goes first.

Tartan colours had the same problem: long travel lines crossed the plaid in the kit's preview.
They now use the same join, with the outline walk added, since a tartan colour's own stripes
reach the edge. Like blend colours, they skip start- and end-point travel. Blends don't use the
outline walk: there, a white line along the blue swatch's bottom edge showed.

| kit design | trims | jumps | needle points | longest exposed travel |
|---|---|---|---|---|
| fills/gradients | 71 → 49 | 9 → 9 | 8,859 → 9,054 | 2.6 → 1.9 mm |
| fills/tartan | 3 → 3 | 3 → 3 | 3,567 → 3,514 | 6.1 → 0.0 mm |

Exposed travel is measured as in the test: straight stretches more than 10° off a colour's
row directions, over cells it covers under 70 %, away from the outline. Before #891 the gradients
design had 1 trim and 28.6 mm of exposed travel. #901's target of a third of the 71 trims (about
23) is not met under the 70 % rule. An earlier draft reached 16, but it counted travel within
0.15 mm of any of the colour's stitches as hidden, which also hid lines crossing rows shared with
other colours.

`blend_travel_never_runs_far_across_the_rows` judges each colour on its own thread: no more than
2 mm of a straight stretch across the rows may lie over cells where that colour covers under 70 %.

### Blend and gradient travel stays short, 2026-11-11 (#891)

Beyond PE-DESIGN, so there is no reference. Each colour of a blend or gradient sews only its own
share of the shared rows, so nothing covers its travel. In the kit's blends, long straight travel
crossed the other colours in each colour's own thread: a purple line ran across the radial blend's
gold centre (`examples/sew/previews/gradients.png`). It came from two places:
- the travel between a colour's pieces, which took the shortest path inside the shape however far
  it was;
- the travel from each colour object's start point and to its end point.

Now a colour travels at most 2 mm between its pieces (`paths::join_within`, `GRADIENT_TRAVEL`) and
trims longer gaps. It has no start- or end-point travel, as Concentric Circle and Spiral already
do; its colour change trims there anyway.

The kit's gradients design (4 swatches, 9 colours) goes from 1 trim to 71 and 9,133 needle points
to 8,859; its longest straight stretch across the rows is now 3.0 mm, against 22 mm before. Test:
`blend_travel_never_runs_far_across_the_rows`.

### Sharp Corners and Miter Limit on satin lines, 2026-11-09 (#373)

PE-DESIGN has no satin line type, so there's no reference; this keeps OpenStitch's satin line
(#524) consistent with zigzag lines (#759, #841). A satin line with one width all along (no width
keys, tapers or slant), no satin shaping and straight segments sewed as plain skeleton satin, so
a corner sharper than 67.5° fanned out to its full point. Such a line with such a corner is now
sewn as the zigzag line of its width (`lines::zigzag_rails_cornered`), so its Sharp Corners button
and Miter Limit (`object.setLine sharpCorners/miterLimit`, stored for every line type) cut the
point as on a zigzag line; with no corner under 67.5°, curved, or with a corner style or other
satin shaping, it sews as before (a corner style takes precedence, as on zigzag lines). A satin
line whose width varies keeps its own corners: the zigzag corner assumes one half-width per side.

### Corner styles on satin text, 2026-11-08 (#782)

A letter is cut into strokes at its sharp corners (strokes.rs), so a satin's corner style
(#520) had no corner to shape on text. Two earlier attempts kept letters whole through their
corners and re-paired the outline sides there; that looped at stem-to-diagonal corners and the
crossings fanned rather than meeting a mitre line. Now the cut stays, and with a style set
`compile::join_corners` sews two blocks sewn one after the other through a corner (they share
the corner's inner point, turning by more than 60°) as one column: each leg keeps its own 3 mm
crossings, ending short of the corner, and a crossing from the inner point to the outline's
outer tip joins them, so `column::satin_with` mitres or caps it as on skeleton satin. Lato Bold
at 20 mm, needle points turn / mitre / cap: Z 249 / 297 / 262, V 209 / 230 / 215, W 385 / 462 /
444; at 8 mm Z 97 / 124 / 116, W 157 / 194 / 180, M 140 / 155 / 151. `auto` mitres these corners
(all wider than its 45° cap angle). Stems (N's run-on corners, #446), junctions (A's apex, K,
M's notches) and right angles (L, not cut) stay turned. Unset, text is unchanged: compat
485/485.

### CSD and SHV against PE-DESIGN's exports of #349's designs, 2026-11-07 (#349, `batch:machine-formats`)

PE-DESIGN 11.50 exported OpenStitch's `c17.pes` (17 colour blocks) and `long.pes` (a short move,
15 mm satin stitches) to CSD and SHV (fixtures/pe11/captures-2026-11-07/machine).

- **CSD:** a design of more than 14 colour blocks is two files, `_A` with 14 blocks and `_B`
  with the rest, each from the design's origin; `design.export` now splits it the same way
  without being asked (the parts of any split design keep the whole design's origin). A colour
  change counts the file's blocks (`85 02`, `85 03`, …). A stitch over 12.7 mm is a `90` move
  record for each 12.7 mm piece and a stitch for the rest. The palette names PE's own colour for
  14 Brother colours (800 RED is `E3 31 1F`) and fills unused slots with PE's fill (slot 1
  `00 00 FF`). pe-long.CSD and pe-c17_A.Csd come out byte for byte. PE's `_B` names the design's
  first three colours in its palette, not its blocks'; OpenStitch names the blocks' own colours.
- **SHV:** the file's name and PE's chart entries (800 RED is 41, not the nearest 3) match. The
  records are measured but not yet written: PE starts each block on the stitch after the
  jump's landing, writes a stitch over 12.7 mm as one `80 01` jump and an 8 mm move as a plain
  record, and writes `80 03` before its 74 mm moves (not before 31.7 mm). The six header bytes
  `C4 28 00 30 00 00` are the same in all three PE exports.

### SVG stroke width as a satin line's width, 2026-11-07 (#701, rest of #524)

Beyond PE-DESIGN and opt-in (`import.svg {settings: true}` with names); the default import is
unchanged. `stroke.satin`, and `satin` on a shape with no fill, now make OpenStitch's satin line
(#524) rather than PE-DESIGN's zigzag line, as wide as the SVG stroke: its `stroke-width` scaled
by the element's transform, rounded to 0.1 mm, unless `w=` gives a width. `stroke.zigzag` asks
for the zigzag line. A stroke outside the satin line's 0.5–10 mm is listed in `settings.invalid`
and the line keeps its default width.

### Small Text against PE-DESIGN's recorded numbers, 2026-11-07 (#530)

PE-DESIGN 11.50's Small Text fonts are Brother's, so the round-8 capture files stay in the
owner's notes; two compat specs use only the numbers recorded on #530, as expected values
(internal-regression tier):

| | PE-DESIGN (recorded) | OpenStitch | checked |
|---|---|---|---|
| Hello 0123, 3 mm, Utah SR / OpenStitch Line | 22.5 × 3.2 mm, 165 stitches, running, ~8 trims | 16.0 × 3.0 mm, 63 stitches, running, no trim inside | height ±0.3 mm, longest stitch ≤ 2 mm, no interior trim |
| abc ijt, 5 mm, Utah Bold S / Lato Bold satin | 23.4 × 6.4 mm, 336 stitches, satin | 18.9 × 6.5 mm, 316 stitches | stitches ±25 %, height ±0.3 mm |

Widths and the running line's stitch count follow each font. PE-DESIGN's trims between running
letters and its refusal of a size outside a font's range are deliberate differences
(OpenStitch joins letters by running stitch up to 1.2 × the size, and warns on size, #527).

### Satin looks and raised satin on satin regions and skeleton satin, 2026-11-08 (#685)

Beyond PE-DESIGN and off unless chosen: a satin with no look set sews as before (compat
unchanged). A fixed-direction satin region now takes `satinStyle` (open: rows three times as far
apart; S-stitch; double), `jaggedEdge`, short stitches and `raised` padding lanes under each
section, as Manual Punch columns do; skeleton satin and Auto satin, which took the style and
jagged edge with their shaping (#675), now also take the raised padding along their column.
`spacingAt` stays a column option. PE-DESIGN's feathered edge keeps its own modes (#528's
In/Out/one-side seeds on the feathered edge are not part of this).

The rest of #685, the same day, again off unless chosen (compat unchanged):
- `spacingAt` on skeleton satin and Auto satin: the rungs are spaced evenly on a line that far
  across the column from its shorter side (0 the inner edge of a bend), from rungs sampled every
  1 mm (skeleton) or 0.1 mm (Auto satin). Unset keeps each kind's own spacing: Auto satin on the
  outer edge (PE-DESIGN's, = 1), skeleton satin along its centre line (= 0.5). On a half-circle
  skeleton satin 8 mm wide (inner radius 26 mm, centre 30 mm) at 4.5 lines/mm, spacingAt 0 sews
  373 crossings against 425 (26/30), 0.5 the same 425; a straight column is unchanged.
- The feathered edge (`object.setRegion featherMode in|out|both`, `featherSeed`): out lengthens
  the chosen row ends past the edge by PE-DESIGN's shares of the depth, both moves each end in
  or out by up to half of it, and a seed draws each row end's share from a pseudo-random
  sequence instead of PE-DESIGN's 12-row pattern; the side (start, end, both) is PE-DESIGN's.
  In with no seed is PE-DESIGN's feathered edge, number for number.

### Ruler origin, 2026-11-08 (#799 part 1)

At the owner's request the rulers count from the Design Page's centre by default (negative left
and up), for every user, with Options ▸ Ruler origin (`ui.set rulerOrigin`) switching back to
the top-left corner. The cursor position and the precision bar's X and Y follow the same origin;
the design and every command keep page coordinates from the top-left. PE-DESIGN 11.50's own
rulers (captured black-box, recorded on #799) also put 0 at the page centre and count up both
ways without a sign (… 20 10 0 10 20 …): OpenStitch's ruler labels are unsigned the same way,
while the cursor position, the precision bar and the guide labels keep their sign.

### Guidelines on by default, dragged from the rulers, snapped to, 2026-11-08 (#799 part 2a)

At the owner's request guidelines are shown by default and a dragged selection snaps to them
(Options ▸ Snap to guides), both remembered; existing users get them on (neither was saved
before). A guideline is dragged out of a ruler even while they are hidden, which shows them, and
dropped onto either ruler or off the canvas it is removed. While dragged or hovered it shows its
signed distance from the ruler origin in the chosen units (one decimal in mm, three in inches).
Selecting, Delete, the guide menu, Lock and Clear Guides, Set Position… and the ruler menu follow
in part 2b and part 3.

### Guide editing, Lock Guides and Set Position…, 2026-11-08 (#799 part 2b)

A click on a guideline's marker, or on the line with the Select tool, selects it without moving
it (drawn thicker); Delete or Backspace removes the selected one before any selected objects.
With the Select tool a guideline is also dragged on the canvas. Alt-click turns it vertical ↔
horizontal: on the canvas where it was clicked, on its marker across the page centre. A
double-click, or View ▸ Guides ▸ Set Position…, types its offset from the ruler origin in the
chosen units through the units parser (a unit typed with it wins); invalid text keeps the dialog
open with an error. View ▸ Guides also has Lock Guides (locked guidelines show and snap but are
not moved, selected or deleted and lose their ×; a ruler still adds new ones; saved in `.osd`),
Clear Selected Guide, Clear Guides and New Guides From Selection (the box's edges and centre
lines). Each is one undo step through the new `view.*` commands; `ui.guides` reads the guidelines
as shown and acts as the pointer does. The right-click menus, New Guide… and Guide Layout…, the
guide colour, the ruler cursor marker and zoom-aware steps are part 3.

### Guide and ruler menus, New Guide…, New Guide Layout…, guide colour, 2026-11-08 (#799 part 3)

A right-click on a guideline (its line, with any tool, or its ruler marker) opens its menu: Set
Position…, Clear Selected Guide, Lock Guides, Clear Guides. A right-click elsewhere on a ruler or
its corner opens the ruler menu: Millimetres or Inches, the ruler origin, Show guides, Snap to
guides, New Guide…, New Guide Layout…, Lock Guides, Clear Guides. New Guide… takes an orientation
and a position from the ruler origin (the units parser, a unit typed with it wins). New Guide
Layout… (as photo editors' guide layouts) takes columns and rows with their gutters and four
margins in the chosen units, previews the guidelines dashed on the canvas while open, and adds
them in one undo step, optionally clearing the old ones (`view.guideLayout`). Each margin gives a
line and each column or row its two edges, shared where there is no gutter. Options ▸ Guide
colour sets the guidelines' colour (`ui.set guideColor`, `#rrggbb`). The cursor marker on both
rulers and the 1-2-5 ruler steps that follow the zoom were already there. Not done: setting the
ruler origin by dragging from the rulers' corner box (optional in #799; the origin stays the
page's centre or top-left corner). No photocraft code was copied, so no notice is added.

### Arched text spacing, 2026-11-08 (#428)

PE-DESIGN's arches (Text Transform presets 1 and 2) place the letters slightly closer along the
arc than OpenStitch did. The arch now sets its letters 0.05 mm closer (`ARCH_LETTER_SPACING`)
and its centre shift is -0.004 of the width (`ARCH_SHIFT`, was -0.0055), chosen from a sweep of
shifts -0.0085..0 and spacings 0..-0.1 mm over every arch spec (batch-lettering-text). Every
PE-DESIGN reference still passes (478/478). The PES 0110 editable-text snapshot's arch width
moves 79.9 -> 79.6 mm, nearer PE-DESIGN's own re-save of 79.20 mm; its height 25.8 -> 25.7 mm
(PE-DESIGN 26.50).

Fill text under a Text Transform still sews by region rows. By stroke, with this spacing, the
arches pass; three references remain 0.1-0.2 mm past their 0.5 mm bounds: text-transform-10's
slanted stem foot (48.4 mm against 49.0) and the descenders of 17-openstitch-l75 and
18-openstitch-l100 (50.8 against 51.4 and 51.5), where the warped bottom edge's lower corner
lies past the stroke's last crossing.

### Auto Punch colours: a small saturated colour keeps its thread, 2026-11-08 (#447)

PE-DESIGN-compatible mode, flat graphics only. PE-DESIGN 11.50's Auto Punch of the calibration
image `447-patches.png` (17 patches on white, round 9, BROTHER EMBROIDERY, automatic 10, then 6
and 4 colours) keeps the 0.17 % red patch's own thread (Red) at every count, while median cut
gave it no box of its own. Now a colour smaller than 0.5 % of the pixels, saturated (CIELAB
chroma >= 50) and at least 30 from every colour with a real share keeps its own box (at most 3),
in a census of at most 40 colours (a photo's small saturated colours are highlights PE-DESIGN
merges).

Patches matching PE-DESIGN's thread (of 17): automatic 11 -> 12, 6 colours 9 -> 8, 4 colours
10 -> 16 (threads exactly PE-DESIGN's: Khaki, Pewter, Red); 30 -> 36 of 51. The loss at 6:
the box red takes costs the Warm Gray PE-DESIGN keeps there, as PE-DESIGN merges blue, teal and
purple into one Pewter cluster that median cut keeps apart. Still different: PE-DESIGN's
grey80 + olive + green -> Dark Olive and teal -> Emerald Green at 10 (CIELAB nearest is Teal
Green, so PE-DESIGN's thread choice is not a plain nearest colour), grey200 joining the
background at 4. The storybook p03's orange (this issue's headline) is a photo gradient, not
a small distinct colour, and stays merged.

### Auto Punch keeps as many threads as colours, 2026-11-08 (#447)

PE-DESIGN-compatible mode (Auto Punch and Cross Stitch, the census palettes). PE-DESIGN keeps as
many threads as it proposes colours; OpenStitch matched each median-cut box to its nearest PEC
thread, and two boxes that matched one thread left one thread fewer (p03, the storybook: 10
colours proposed, 8 threads sewn, the fox's orange merged into a brown). Now a box whose nearest
thread another box took takes its nearest unused thread when that is at most twice as far
(squared RGB).

Automatic colour counts matching PE-DESIGN's proposals (g01, g02, g05, g08, g11, g12, g13, g14,
g15, p03, p07): 6 -> 7 of 11. p03 sews 10 threads (PE-DESIGN 10) with Pumpkin for the fox;
d01 sews 10. Likeness (`tools/image_similarity.py --mode pe --no-dists`): p03 dE00 16.02 ->
17.05, MS-SSIM 0.623 -> 0.649; d01 23.97 -> 22.37, 0.391 -> 0.430; every other Auto Punch case
unchanged. A ratio of 4 also matched g11's 9 but gave it a Flesh Pink for a grey and doubled
its stitches. Photo Stitch's palette is unchanged.

### Trims inside one object: letters, cutwork lines and a trim report, 2026-11-08 (#794)

Beyond PE-DESIGN's own routing; no stitch output changes.

- **Report:** `tools/trim_report.py` runs every editable `examples/*.json` design and counts its
  trims and its same-thread trims (followed by a stitch in the same thread, the ones routing
  could remove). Main: 458 trims, 260 same-thread over 29 designs (background-fill 59 / 47,
  cutwork 72 / 56, lettering 33 / 30, as #794 measured). `--set autoStart='"nearest"'`: 443 / 245
  (cutwork 61 / 45, monograms 25 / 12, applique 39 / 16), and `--compare` shows no design
  gaining a trim.
- **Letters:** over the 239 text specs with a PE-DESIGN reference, OpenStitch sews 1,782 trims
  and PE-DESIGN 2,126. Only `stroke-satin-m-8` has more (2 against 1, M at 8 mm, open on #446).
  The trims between letters are PE-DESIGN's own, so letters are left as they are.
- **Cutwork lines:** as #800 measured, 49 of cutwork's 56 same-thread trims are between the
  cutting lines' segments, which the cutting needle can't travel between. They stay;
  `autoStart: nearest` already removes 11 of the others.

### Programmable fill along a direction curve tiles the curve's own frame, 2026-11-12 (#923)

No PE-DESIGN reference covers a direction curve. Programmable fill along a curve used the chord
warp: in the curve's chord frame, (x, y) → (x, y − f(x)), its rows and tiles straightened and
bent back. The map is one-to-one, so it added no crossings. But it shifted the tiles sideways
across the chord, so they didn't follow the curve. Now the rows are the curve's offsets, as fill
and piping along a curve have been since #651 (`direction::guided_lines`). The pattern is tiled
in the curve's own frame, along it and across it (`direction::Guide::frame`), from the reference
point (else the region's top-left corner). Each row is a straight line in that frame, so its
needle points are where it crosses the tiled pattern (`regions::tile_crossings`, unchanged),
carried back onto the curved row. Long gaps are split along the row, often enough to stay
within a third of a row spacing of it. Patterns with embossed or engraved areas, and regions
with stamps, keep the chord warp.

On a 40 × 38 mm square with `examples/sewing-direction.json`'s S curve (programmable fill
defaults):

| | stitches | crossings | thread per 1 mm cell, p10 / median | R |
|---|---|---|---|---|
| straight (no curve) | 1,326 | 2,210 | 2.34 / 2.92 | – |
| S curve, chord warp | 1,470 | 2,228 | 2.52 / 3.12 | 0.07 |
| S curve, curve frame | 1,799 | **2,094** | 2.38 / 2.94 | **0.79** |

R measures how tightly the needle points' distances to the curve cluster at the row spacing
(1 = every point on an offset row). The crossings are the pattern's own: the straight pattern
has 2,210. Along a U-shaped curve the inside of the U was left unsewn, by plain fill along it
too; the rows there are sewn since #927 (below). Motif fill keeps the chord warp. Test:
`programmable_fill_along_a_curve_keeps_to_the_curves_offsets`.

Re-measured 2026-11-12 with the U-shaped (C) curve of #927 added. All three rows come from one
script, through `stitchtool exec` with programmable fill's defaults (2.5 rows/mm), with #927's
and #947's row fixes in. The figures differ a little from the table above, which was measured
another way:
- coverage here counts cells 1 mm in from the square's edges;
- R here is taken against the curve sampled and carried straight on past its ends.

| | stitches | crossings | thread per 1 mm cell, p10 / median | R |
|---|---|---|---|---|
| straight (no curve) | 1,330 | 2,188 | 2.38 / 2.98 | – |
| S curve | 1,837 | 2,040 | 2.45 / 3.04 | 0.69 |
| C (U) curve | 1,944 | **1,435** | 2.37 / 3.27 | **0.68** |

Along the C, R is well above #923's 0.4, and the crossings stay below the straight pattern's
(#923's targets).

### Open OpenStitch fills travel along the outline, 2026-11-12 (#628)

A fill entered away from its first row sews a straight travel from the start point to that
row across the region. PE-DESIGN 11.50 does the same (sweep-fill-density-min,
tatami-40-pos-*), so plain shapes keep it: the owner's decision of 2026-11-12. OpenStitch's own
default differs.

On a 20 mm square, under sewing off, entered at the middle of its bottom edge, the longest
straight stretch across the rows inside the square:

| fill | 4.5 rows/mm | 2 rows/mm | 1 row/mm |
|---|---|---|---|
| plain fill (Inside) | 22.4 mm | 22.4 | 22.4 |
| Inside, hidden (Auto Punch's OpenStitch mode), before | 22.4 | 22.4 | 22.4 |
| Inside, hidden, now | 22.4 | **2.1** (one travel stitch) | **≤ 2.1** |
| colour blend / gradient layer | 8.6 (travel between its pieces, #901) | 6.4 | 4.5 |
| tartan layer | its own warp rows only | – | – |

- **Inside, hidden, when open:** an open fill set to Inside, hidden (under 3 rows/mm, where one layer of rows hides less than about 85 % of the fabric on the T1 cards) now travels along the outline, its entry included (`RegionSewing::travel_path`).
- **When dense:** it keeps Inside, hidden, whose section order (#469, #930, #942) hides the travel under thread still to come.
- **Blends, gradients and tartan:** these already start at the piece nearest the entry and join their pieces by #901's hidden-or-trimmed travel, so no straight entry travel crosses them.
- **Auto Punch's OpenStitch mode** sews at 4.5 rows/mm, so its fills change only once the user lowers their density. The PE-compatible output and the kit are unchanged.

Test: `an_open_openstitch_fill_entered_away_from_its_rows_sews_no_travel_across_it`.

### Rows inside a U-shaped direction curve, 2026-11-12 (#927)

A fill along a direction curve that bulges deep into its shape (a 40 × 38 mm square with a curve
from its left to its right edge bulging down into it, #927's example) sewed no rows inside the U:
273 of the square's 1 mm cells stayed empty, only travel crossing them. Programmable fill along
the same curve had the same hole. The rows are the guide offset by whole row spacings, and
`direction::guided_lines` made them from the nearest to the farthest distance any point of the
region has from the guide, but it took those distances at the outline's vertices only. Inside a
U, the middle of an edge and the inside of the U lie farther from the guide than every corner,
so the deeper rows were never made. The distances are now also taken on a grid over the region's
box (64 steps across, at least a row spacing apart, plus a step's margin each way).

On #927's square (cells 1 mm in from its edges, 38 × 36), thread per 1 mm cell, p10 / median,
against the same fill without the curve:

| | empty cells | p10 / median | without the curve | crossings between row stitches |
|---|---|---|---|---|
| fill along the U, before | 644 of 1,368 | – | 4.39 / 4.50 | – |
| fill along the U, now | 0 | 4.30 / 4.50 | 4.39 / 4.50 | 0 |
| programmable fill along the U, now | 0 | 2.36 / 2.49 | 2.38 / 2.48 | 1 (a tile below the U) |

Inside the U the rows close into corners on its middle. The S curve's rows on #923's square
are unchanged (the same rows and needle points at 2.5 and 2.22 mm spacing): its farthest points
are corners. A box under an arch had the same hole along its bottom edge, whose middle lies
farther from the arch than its corners: 91 rows made there before, 161 now.

The rows below the U split round the square's bottom corners and merge further up. The sweep
through the rows climbed past the merge and reached the strip right of the U's arm only at the
end, by travel over every row sewn since: 153 pairs of stitches crossed in the top layer, all of
them travel against a row. Rows along a direction curve now sew such a split-off lobe first,
from its far end back to the merge (`regions::order_guided_rows`, #947; straight fills keep
`order_rows`). Top-layer crossings: 153 → **0** along the U (2,923 → 2,908 stitches). On
#923's S-curve square, the final travel along the outline's top edge goes too: 194 → **0**
crossings (2,642 → 2,616 stitches); its rows are the same, sewn in a different order. Test:
`fill_along_a_u_shaped_curve_covers_the_inside_of_the_u`.

### Travel walks the outline where the way inside runs round a tip, 2026-11-09 (#471)

Region travel between separate parts (`region::reroute_with`, Running Stitch Path Inside) takes
the shortest way inside the region. Where a row ends on an outline that runs nearly along the
rows, that way could lead out to a tip and back over the row just sewn: region-types' piping
(1.2 lines/mm at 30 degrees) ran 13 mm to its lobe's tip and back where the outline between the
two row ends is 4 mm, three passes over 5 mm, the "dark bar" #471 reported. Now, where both ends
lie on the outline and the walk along it is under three quarters of the inside way, the walk is
taken. Piping's densest 1 mm cell: 3.78 → 2.23 mm of thread per mm² (median 1.16), cells over
twice the median 0.9 % → 0 % (`piping_travel_keeps_off_the_rows_at_a_lobe_tip`). The same rule
touches 8 kit designs by a few stitches (fills, Auto Punch's storybook and balloon, monograms,
background fill; trims unchanged, each within ±2 % of its stitches and sew_check ok); compat
unchanged.
