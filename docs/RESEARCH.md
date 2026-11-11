# Research log

OpenStitch specifies PE-DESIGN 11's behaviour from Brother's public online user's guide,
published and open-source descriptions of embroidery file formats, independent tests, and
(since 2026-10-27) black-box observation of a licensed PE-DESIGN 11.50 through its normal
interface. See [LEGAL_AND_LICENSING.md](LEGAL_AND_LICENSING.md) for the boundaries.

## 2026-10-18: Machine formats (public sources)

Sources: pyembroidery 1.5.1 readers and writers (MIT); the format behaviour they implement is
widely corroborated by other open implementations. PES and PEC have since been corroborated
against files written by PE-DESIGN 11.50 (below); the other formats have not. Details in
FILE_FORMATS.md.

Findings confirmed by tests against pyembroidery output:

- The PEC section is fixed-layout: a 512-byte header, the stitch block, then (colours + 1)
  48×38 icons.
- PEC stitch deltas use a 7-bit short form for −64 < d < 63, otherwise a 12-bit long form with
  flag bits 0x20 (trim) / 0x10 (jump).
- The DST header's `ST:` counts every move record, including the three jumps that encode a trim.

## 2026-10-18: Official online user's guide

Source: Brother's public PE-DESIGN 11 online user's guide (Version C,
support.brother.com). Headings with links are in `research/pedesign11-manual-toc.md`.

- Seven applications: Layout & Editing, Design Center, Design Database, Programmable Stitch
  Creator, Font Creator, Stitch Design Factory, Link.
- Three data types: stitch, text and shape patterns, convertible between them.
- File formats: the authoritative list is in FILE_FORMATS.md.
- 8 line sew types and 15 region sew types, each with its attribute set.
- Wizards: Auto Punch, Cross Stitch, Photo Stitch 1/2, appliqué, patch, cutwork, background
  fill, split / multi-position / jumbo frame, Print and Stitch, Name Drop, templates.
- Machine transfer: USB media, direct to machine memory, wireless, original card, USB/wireless
  Link.

## 2026-10-18: PhotoCraft (architecture reference)

Studied PhotoCraft's command-engine, architecture, control-protocol and testing docs
(github.com/storytold/photocraft). The principles adopted are in ARCHITECTURE.md
("Engine-first layering").

## 2026-10-27: PE-DESIGN 11.50 reference files (black-box)

Source: simple test designs made by the owner in a licensed PE-DESIGN 11.50 through its normal
interface, saved as PES and committed under `compat/reference/` with their provenance in
`MANIFEST.json`. The harness (`stitchtool compat`) compares OpenStitch's output with them; the
per-item findings are in [COMPATIBILITY_AUDIT.md](COMPATIBILITY_AUDIT.md) and the file
observations in [COMPATIBILITY_TESTS.md](COMPATIBILITY_TESTS.md) ("Reference tier").

- PE-DESIGN 11 saves `#PES0110` by default; its Design Property strings are Windows-1252 with
  CR LF line breaks, and its v6 thread-table strings are MFC Unicode strings. Its files hold no
  zero-length stitch record and no lead-in jump segment; a trim is a plain jump segment in
  `CSewSeg` and the PEC record's `0x20` flag. The editable shapes (`CEmbLine`) are described in
  FILE_FORMATS.md ("PES 0110 object list"), from the owner's files only, as the technique
  register allows.
- Fill and satin default to 4.5 lines/mm with under sewing on; satin is a sawtooth with a tie-in
  and a lock; gradation is a Max/Min pattern (53 % to 5 %) falling linearly; lines default to
  2.0 mm with a tie-in and lock; the text tool sews Satin at density 5.0 without under sewing,
  at 0.7 em, with line spacing in % of the character height.
- Direction angles turn counter-clockwise on screen; shapes start and end at the lowest point of
  the vertical centre line; Optimize Sewing Order keeps the drawing order within a colour.

## Next research steps

The 2026-10-27 list is done: the fill row structure, Half Stitch, line Start/End Type and Text
Transform presets 13 and 14 are calibrated from reference files; PE-DESIGN's exports to the
other machine formats are in `compat/reference/machine/`; shapes and text are written as
objects PE-DESIGN 11.50 opens editable, with its hoop and Design Page fields; the ready-to-sew
kit was sewn on the owner's NV880E (COMPATIBILITY_AUDIT.md, "Sew-out findings"). What is still
open:

- The reference-bound items and "Next 11" in [COMPATIBILITY_AUDIT.md](COMPATIBILITY_AUDIT.md),
  and the "pending reference" specs in [COMPATIBILITY_STATUS.md](COMPATIBILITY_STATUS.md).
- PES object kinds the `#PES0110` writer still refuses (#608), and the fields an edited
  PE-DESIGN file still loses on re-save (audit item 9), from the owner's files only.
- Sew-out records in `compat/machine/` (none yet, so no spec is at the machine tier), and
  reports from other machines and for the non-PES formats (#292).
- Research beyond PE-DESIGN: [ROADMAP.md](ROADMAP.md) ("Beyond PE-DESIGN") and
  `research/beyond/`.

## Research notes in `research/`

- [asset-audit.md](research/asset-audit.md): icons and illustrations the editor lacked
  (2026-10-30) and the Higgsfield rounds that filled them.
- [competitive-features.md](research/competitive-features.md): what other digitizing products
  offer beyond PE-DESIGN 11, the comparison matrix and the B1–B15 backlog (2026-10-28, with
  later scans).
- [hoops.md](research/hoops.md): hoop sizes per brand and their public sources.
- [pedesign11-manual-toc.md](research/pedesign11-manual-toc.md): the headings of PE-DESIGN 11's
  online guide, with links.
- [photo-stitch-2-halftone.md](research/photo-stitch-2-halftone.md): Photo Stitch 2 as a line
  screen, measured and rebuilt (#129).
- [split-print-transfer.md](research/split-print-transfer.md): split designs, multi-position
  hoops, jumbo frame, printing and USB transfer from the guide.
- [stroke-lettering.md](research/stroke-lettering.md): how PE-DESIGN cuts TrueType letters
  into strokes, and OpenStitch's stroke-wise lettering.
- [test-images.md](research/test-images.md): the test image library for the image wizards and
  the PE-DESIGN captures of it.
- [updater.md](research/updater.md): the self-update survey, decision and verification.
- [2026-10-27/](research/2026-10-27/): the first reference sessions' analyses (fill, satin,
  lines, text, machine streams, PES objects, Optimize Sewing Order).
- [beyond/](research/beyond/): one note per feature beyond PE-DESIGN, and the patent facts.
