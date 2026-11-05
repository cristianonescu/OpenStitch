<p align="center">
  <picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/brand/openstitch-wordmark-dark.svg">
    <img src="assets/brand/openstitch-wordmark.svg" alt="OpenStitch" width="420">
  </picture>
</p>

# OpenStitch

An independent, cross-platform (macOS, Windows, Linux) embroidery design application written in
Rust. It aims to work like Brother PE-DESIGN 11 and to read and write the same files, so designs
move between the two and onto Brother machines.

OpenStitch is an **alpha**: good for trying out and testing, not for production work. Do a test
sew-out on scrap fabric first.

Not affiliated with Brother Industries. No Brother code or assets are included; see
[docs/LEGAL_AND_LICENSING.md](docs/LEGAL_AND_LICENSING.md).

![The OpenStitch editor: Application menu, Quick Access Toolbar and ribbon at the top, sewing order on the left, the design page in the middle, sewing attributes on the right and the stitch simulator below](docs/images/readme/editor.png)

## Contents

- [New: the Assistant](#new-the-assistant)
- [Status at a glance](#status-at-a-glance)
- [Screenshots](#screenshots)
- [From image to embroidery](#from-image-to-embroidery)
- [Ready-to-sew designs](#ready-to-sew-designs)
- [Features](#features)
- [Not yet implemented](#not-yet-implemented)
- [Known issues](#known-issues)
- [Planned](#planned)
- [Download a build](#download-a-build)
- [Build from source](#build-from-source)
- [MCP server](#mcp-server-claude-code-and-other-mcp-clients)
- [Contributing](#contributing)
- [Releases](#releases)
- [Documents](#documents)
- [License](#license)

## New: the Assistant

**Alpha 12 adds an AI chat that can drive the whole editor.** Ask it for a design ("a 70 mm round
badge with a navy satin border and ANNA in gold"), to change what is open ("make the lettering
satin and 10 % bigger"), to explain or check a design, or drop in a picture and ask for embroidery.
It works through OpenStitch's own commands, so what it makes is an ordinary design you keep
editing by hand, and everything one message changes is a single undo step.

![The Assistant beside the design: asked to turn a dropped pixel-art strawberry picture into embroidery about 50 mm wide, it calls OpenStitch's commands, notices the first result came out 34.4 mm wide, rescales it, checks it and summarises the result, with Undo these changes below](docs/images/assistant-agent-picture.png)

*A real run, not a mock-up: Claude Code (on the user's own account) turning a dropped picture
into a 50 × 63 mm strawberry, correcting its own first attempt, then checking the result.*

- **Your choice of AI:** the Claude API with your own key (kept in the system keychain), a model
  on your computer through Ollama or LM Studio (no key, works offline), OpenRouter or any
  OpenAI- or Anthropic-compatible service, or a coding agent you already use (Claude Code, Codex,
  OpenCode).
- **You stay in control:** design changes run at once; saving, exporting, opening files, sending
  to the machine and quitting ask first (**Allow once**, **Always allow** or **Deny**).
- **Private:** your messages and design go only to the AI you chose; OpenStitch has no server and
  no telemetry. **Options ▸ Show AI features** hides it completely.
- **Open it:** it opens beside the design; **View ▸ Show/Hide ▸ Assistant** or Ctrl+Shift+A
  (Cmd+Shift+A) brings it back.

Benchmark on 12 embroidery tasks: Claude Code 12/12, Ollama gpt-oss:20b 8/12. It has been tested
on macOS so far; Windows and Linux testing is
[#401](https://github.com/cristianonescu/OpenStitch/issues/401). Details in the
[alpha 12 notes](docs/releases/v0.1.0-alpha.12.md).

## Status at a glance

Current release: **v0.1.0-alpha.12** ([notes](docs/releases/v0.1.0-alpha.12.md)).

OpenStitch is checked against PE-DESIGN 11.50 itself. Simple test designs made in a licensed copy
through its normal interface are committed under `compat/reference/`. The compatibility harness
(`stitchtool compat`) compares OpenStitch's output with them spec by spec.

<!-- status:glance -->
| | |
|---|---|
| Feature matrix rows | 290: 163 implemented (3 of them partially compatible), 0 documented only, 10 blocked, 117 planned beyond PE-DESIGN |
| Compatibility specs | 454: 454 pass, 0 fail ([status](docs/COMPATIBILITY_STATUS.md)) |
| Matrix rows covered by specs | 36: 29 at the reference tier, 5 at the internal-regression tier, 2 at the oracle tier |
| Open audit differences | 10 ([audit](docs/COMPATIBILITY_AUDIT.md#summary)) |
| Open GitHub issues | 153 ([list](https://github.com/cristianonescu/OpenStitch/issues)) |
<!-- /status:glance -->

Tiers, weakest first: *internal regression* (our own earlier output), *oracle* (an independent
implementation), *reference* (PE-DESIGN 11.50's own output), *machine* (a recorded sew-out; none
yet). Every feature and its evidence is in [docs/FEATURE_MATRIX.md](docs/FEATURE_MATRIX.md).

## Screenshots

| | |
|---|---|
| ![Realistic view of a satin-stitched star](docs/images/readme/realistic-view.png) | ![Stitch simulator part-way through sewing a design](docs/images/readme/stitch-simulator.png) |
| **Realistic view:** see how the stitches will look on fabric; thread is drawn as wide as it covers at its row density, measured on real sew-outs. | **Stitch simulator:** play the design back stitch by stitch, with jumps and trims. |
| ![Drawing tools with shapes, hearts and a boolean merge](docs/images/readme/drawing.png) | ![Text in several fonts with the Text Attributes pane](docs/images/readme/lettering.png) |
| **Drawing and editing:** shapes, Bézier outlines, arranged copies and shape operations. | **Lettering:** TrueType text with PE-DESIGN's size, spacing and transforms. |
| ![Monograms with decorative frames and the Monogram dialog](docs/images/readme/monogram.png) | ![Auto Punch turning an image into embroidery regions](docs/images/readme/auto-punch.png) |
| **Monograms:** letters in a frame, kept editable. | **Image to embroidery:** Auto Punch, Cross Stitch and Photo Stitch-style conversion. |
| ![Appliqué wizard with material, position and tack-down lines](docs/images/readme/applique.png) | ![Design Database browsing folders of designs as thumbnails](docs/images/readme/design-database.png) |
| **Wizards:** appliqué, patch, cutwork, background fill and split designs. | **Design Database:** browse, search, organise and convert your design files. |
| ![Programmable Stitch Creator drawing a fill pattern](docs/images/readme/stitch-creator.png) | ![Sending a design to a USB stick for the machine](docs/images/readme/usb-transfer.png) |
| **Programmable Stitch Creator:** draw your own fill and motif stitch patterns. | **Machine transfer:** write the design to a USB stick or SD card. |
| ![Update dialog with the release notes and the Update button](docs/images/readme/update.png) | ![The editor in the dark theme](docs/images/readme/dark-theme.png) |
| **Updates:** the editor updates itself from GitHub releases. | **Light or dark** theme. |

## From image to embroidery

Test images from [examples/images](examples/images), generated for OpenStitch (see their
[provenance](examples/images/PROVENANCE.md)), converted by the image wizards and shown in the
editor's Realistic view. **Every picture here is OpenStitch's own output, none is
PE-DESIGN's.** They use OpenStitch's PE-DESIGN-compatible mode (the default): OpenStitch's own
reimplementation of PE-DESIGN 11's method, at the settings named. `python3 tools/gallery.py`
regenerates them.

<!-- gallery:start -->
| Image | OpenStitch (Realistic view) | Wizard and result |
|---|---|---|
| <img src="examples/images/p01-portrait.jpg" width="200"> | <img src="docs/images/readme/gallery/p01.png" width="260"> | **Photo Stitch 1, colour, defaults**<br>14,454 stitches, 385 trims, 7 colours |
| <img src="examples/images/p02-mono-portrait.jpg" width="200"> | <img src="docs/images/readme/gallery/p02.png" width="260"> | **Photo Stitch 2, mono, defaults**<br>19,995 stitches, 1 trims, 1 colour |
| <img src="examples/images/p04-cat-fur.jpg" width="200"> | <img src="docs/images/readme/gallery/p04.png" width="260"> | **Photo Stitch 1, colour, defaults**<br>25,204 stitches, 839 trims, 8 colours |
| <img src="examples/images/p06-sunset.jpg" width="200"> | <img src="docs/images/readme/gallery/p06.png" width="260"> | **Photo Stitch 2, Auto Select**<br>18,526 stitches, 4 trims, 4 colours |
| <img src="examples/images/p08-poppies.jpg" width="200"> | <img src="docs/images/readme/gallery/p08.png" width="260"> | **Photo Stitch 2, Color Set CMYK**<br>35,734 stitches, 4 trims, 4 colours |
| <img src="examples/images/p12-backlit-tree.jpg" width="200"> | <img src="docs/images/readme/gallery/p12.png" width="260"> | **Photo Stitch 1, mono, defaults**<br>4,261 stitches, 130 trims, 1 colour |
| <img src="examples/images/p03-storybook.jpg" width="200"> | <img src="docs/images/readme/gallery/p03.png" width="260"> | **Photo Stitch 1, colour, defaults**<br>19,888 stitches, 789 trims, 8 colours |
| <img src="examples/images/g01-emblem.png" width="200"> | <img src="docs/images/readme/gallery/g01.png" width="260"> | **Auto Punch, defaults**<br>8,655 stitches, 10 trims, 3 colours |
| <img src="examples/images/g02-balloon.png" width="200"> | <img src="docs/images/readme/gallery/g02.png" width="260"> | **Auto Punch, defaults**<br>7,748 stitches, 12 trims, 5 colours |
| <img src="examples/images/g05-robot-cartoon.png" width="200"> | <img src="docs/images/readme/gallery/g05.png" width="260"> | **Auto Punch, defaults**<br>11,547 stitches, 232 trims, 6 colours |
| <img src="examples/images/g08-lettering.png" width="200"> | <img src="docs/images/readme/gallery/g08.png" width="260"> | **Auto Punch, defaults**<br>1,215 stitches, 11 trims, 2 colours |
| <img src="examples/images/g04-butterfly-outline.png" width="200"> | <img src="docs/images/readme/gallery/g04.png" width="260"> | **Design Center, running line**<br>1,635 stitches, 92 trims, 1 colour |
| <img src="examples/images/g07-pixel-strawberry.png" width="200"> | <img src="docs/images/readme/gallery/g07.png" width="260"> | **Cross Stitch, defaults**<br>4,691 stitches, 190 trims, 6 colours |
| <img src="examples/images/g15-folk-bird.png" width="200"> | <img src="docs/images/readme/gallery/g15.png" width="260"> | **Cross Stitch, defaults**<br>5,679 stitches, 303 trims, 4 colours |
<!-- gallery:end -->

Auto Punch, Photo Stitch 1 and 2, Cross Stitch and Design Center have two methods, both run by
OpenStitch (the image dialog's Method choice):

- **PE-DESIGN-compatible** (the default, labelled "PE-DESIGN" in the dialog): OpenStitch's own
  reimplementation of PE-DESIGN 11's method, so a design converts the way it would in
  PE-DESIGN. It is not PE-DESIGN's output, and it does not sew PE-DESIGN's exact stitches.
- **OpenStitch** (➕): colours are reduced, matched to threads and mixed by how they look, and
  Auto Punch merges specks under 3 mm² ([research](docs/research/beyond/image-digitizing.md)).
  Since B16.1 ([plan](docs/research/beyond/image-similarity-plan.md)) the threads are chosen
  for the image as a whole, Photo Stitch 1 dithers only where the image is tonal, a Fidelity
  choice (Standard / High) trades colours for likeness, Cross Stitch takes each cell's mean
  colour, and Design Center finds its line threshold from the image (Otsu). Photo Stitch 1 samples
  the image every 0.25 mm in rows that stay within 2.1 needle points per mm² (no 10 × 10 mm area
  above 3.0), with a Trims choice (fewer trims or the best likeness). Auto Punch sews the
  background and sews dark outlines as satin on top, in one covering layer; it can instead
  isolate the subject by a graph cut and leave out everything around it, with strokes you
  brush on the image for parts it should keep or drop. Threads can come from several brands'
  charts (Brother, Madeira Polyneon, Isacord, Sulky, Robison-Anton) or only those you own. Every
  OpenStitch mode sews the same stitches on macOS, Windows and Linux.

The table below shows both modes, both from OpenStitch. OpenStitch mode measures better on
average, not on every image, so try both. On the eight Photo Stitch 1 test photos its likeness
score (`tools/image_similarity.py`, Brother threads) averages 72.3 against 58.5 and is higher
on every photo (the night street 67.0 against 45.6, the bright high-key photo 76.1 against
75.5), with more trims; Photo Stitch 2 averages 43.3 against 41.7, and the test portrait sews
truer in its PE-DESIGN-compatible mode, whose three threads suit its skin and background.

<!-- gallery-modes:start -->
| Image | OpenStitch, PE-DESIGN-compatible mode (default) | OpenStitch, OpenStitch mode ➕ |
|---|---|---|
| <img src="examples/images/g05-robot-cartoon.png" width="200"><br>Auto Punch | <img src="docs/images/readme/gallery/g05-pe.png" width="260"><br>11,547 stitches, 232 trims, 6 colours | <img src="docs/images/readme/gallery/g05-os.png" width="260"><br>13,594 stitches, 129 trims, 7 colours |
| <img src="examples/images/g06-watercolour-lemons.jpg" width="200"><br>Auto Punch | <img src="docs/images/readme/gallery/g06-pe.png" width="260"><br>14,751 stitches, 285 trims, 9 colours | <img src="docs/images/readme/gallery/g06-os.png" width="260"><br>14,852 stitches, 191 trims, 10 colours |
| <img src="examples/images/p03-storybook.jpg" width="200"><br>Auto Punch | <img src="docs/images/readme/gallery/p03-pe.png" width="260"><br>35,761 stitches, 700 trims, 8 colours | <img src="docs/images/readme/gallery/p03-os.png" width="260"><br>30,181 stitches, 382 trims, 10 colours |
| <img src="examples/images/p01-portrait.jpg" width="200"><br>Photo Stitch 1 | <img src="docs/images/readme/gallery/p01-pe.png" width="260"><br>14,454 stitches, 385 trims, 7 colours | <img src="docs/images/readme/gallery/p01-os.png" width="260"><br>12,853 stitches, 308 trims, 11 colours |
| <img src="examples/images/p06-sunset.jpg" width="200"><br>Photo Stitch 1 | <img src="docs/images/readme/gallery/p06-pe.png" width="260"><br>12,881 stitches, 370 trims, 11 colours | <img src="docs/images/readme/gallery/p06-os.png" width="260"><br>9,820 stitches, 128 trims, 11 colours |
| <img src="examples/images/p10-night-street.jpg" width="200"><br>Photo Stitch 1 | <img src="docs/images/readme/gallery/p10-pe.png" width="260"><br>13,625 stitches, 464 trims, 7 colours | <img src="docs/images/readme/gallery/p10-os.png" width="260"><br>10,059 stitches, 182 trims, 9 colours |
| <img src="examples/images/p08-poppies.jpg" width="200"><br>Photo Stitch 2 | <img src="docs/images/readme/gallery/p08-pe.png" width="260"><br>32,934 stitches, 4 trims, 4 colours | <img src="docs/images/readme/gallery/p08-os.png" width="260"><br>31,312 stitches, 4 trims, 4 colours |
| <img src="examples/images/p04-cat-fur.jpg" width="200"><br>Photo Stitch 2 | <img src="docs/images/readme/gallery/p04-pe.png" width="260"><br>28,028 stitches, 4 trims, 4 colours | <img src="docs/images/readme/gallery/p04-os.png" width="260"><br>27,471 stitches, 4 trims, 4 colours |
| <img src="examples/images/g07-pixel-strawberry.png" width="200"><br>Cross Stitch | <img src="docs/images/readme/gallery/g07-pe.png" width="260"><br>4,691 stitches, 190 trims, 6 colours | <img src="docs/images/readme/gallery/g07-os.png" width="260"><br>9,552 stitches, 192 trims, 6 colours |
| <img src="examples/images/g04-butterfly-outline.png" width="200"><br>Design Center | <img src="docs/images/readme/gallery/g04-pe.png" width="260"><br>1,635 stitches, 92 trims, 1 colour | <img src="docs/images/readme/gallery/g04-os.png" width="260"><br>6,866 stitches, 67 trims, 1 colour |
<!-- gallery-modes:end -->

## Ready-to-sew designs

[**examples/sew**](examples/sew) holds embroidery files made by OpenStitch, ready to copy to a
machine: a design for each feature (every region and line sew type, fill, satin and underlay
options, shapes and frames, lettering and monograms, the appliqué, patch, cutwork and background
fill wizards, and each image tool in both modes), calibration designs, and the compatibility
checks that need a sew-out. They are PES files sized for a Brother NV880E's 100 × 100, 150 × 150
and 260 × 160 mm hoops, each with a preview, its threads and how to sew it. Sewing the calibration
designs and [sending the photos](examples/sew/README.md#sending-results) helps tune OpenStitch to
real thread and fabric. In the editor, **Help ▸ Ready-to-Sew Designs** opens them.
The first calibration results, from a Brother NV880E with New Brothread thread, are in the kit's
[Calibration results](examples/sew/README.md#calibration-results); they will change as OpenStitch
improves.

**Help test on your machine.** So far the kit has been sewn on one machine, a Brother NV880E. Results
from other Brother, Babylock, Bernette, Janome and Singer machines (PES and the other machine
formats), other threads and other fabrics are very welcome: sew a few designs from the kit and send
them with the [Sew-out / calibration report](https://github.com/cristianonescu/OpenStitch/issues/new?template=sew-out-report.yml)
form. [TESTING.md](TESTING.md) says what to sew per machine and format, and the kit's
[Sending results](examples/sew/README.md#sending-results) says what to include.

## Features

Features marked **➕** go beyond PE-DESIGN 11: they are off unless you choose them, so
PE-DESIGN-compatible output stays the default.

### Editing

- **Editor window laid out like PE-DESIGN 11's Layout & Editing window:** Application menu,
  Home / Image / View ribbon with contextual tabs, Quick Access Toolbar, Sewing Order and
  attribute panes ([docs/UI_LAYOUT.md](docs/UI_LAYOUT.md)). Light or dark theme.
- **Drawing:** rectangle, ellipse, polygon, Pencil (freehand), Bézier pen, arcs and preset
  shapes (stars, hearts, arrows and more), Manual Punch columns and curved blocks.
- **Shape editing:** point editing, move/scale/rotate/flip, group, cut and paste, arranged copies
  (mirror, circle, matrix), merge, remove overlap, boolean operations, split, offset lines.
  Bézier curves survive these operations.
- **Sewing attributes and order:** thread colour from the Brother chart or your own charts, sew
  type, direction (straight, auto or curved), entry and exit points, hole sewing, saved
  attribute sets, Copy Sewing Attributes. Sewing order by drag, by colour or optimised as
  PE-DESIGN does, plus a **➕** shortest-jump order (`minJump`).
- **Stitch pattern editing:** move, insert and delete needle points; split, connect, divide by
  colour and change a block's colour.
- **Conversions:** text and shapes to stitches, stitches back to shapes, text to outlines, region
  to line.
- **Ornaments:** floral patterns, decorative frames, stamps, embossing and engraving, using
  OpenStitch's own designs.
- **Workspace:** rulers, grid, measure tool, Reference window, Design Page and Brother hoop list
  with user hoops, undo/redo, stitch simulator, realistic and solid views, Fabric Selector.

### Stitch types

PE-DESIGN 11.50's defaults, tie-in and lock, measured from its own files. Every run of stitches
that ends in a trim ties in and locks, as PE-DESIGN sews them, the image tools included.

- **Regions:** fill (tatami with Frequency, PE-DESIGN's two Stitch Types (flat or sloped rows),
  under sewing, gradation and feathered edge), satin
  (PE-DESIGN's sawtooth and Auto satin), programmable fill, motif fill, concentric circle,
  radial, spiral, flexible spiral, stippling, net fill and zigzag net fill (PE-DESIGN's satin band
  along each net line).
- **➕ Contour fill:** rows that follow the outline (rings, single or double spiral, echo
  lines, round or mitred corners).
- **➕ Random tatami stagger,** seeded and reproducible, beside PE-DESIGN's Frequency brick.
- **Lines:** running, triple, zigzag, motif, stem, candlewicking, E and V, chain and cutting
  (cutwork needles), with Start/End Feed or Run as PE-DESIGN sews them.
- **Patterns:** OpenStitch's own fill, stamp and motif library, or your own patterns drawn in
  the Programmable Stitch Creator or imported from PE-DESIGN's `.pas`, `.pmf` and `.plf` files.
- **➕ Skeleton satin:** a satin column sewn along a derived centre line.

### Lettering

- **Text:** multi-line TrueType/OpenType text with PE-DESIGN's text size, character and line
  spacing, alignment and Satin or Fill sewing. Satin and Fill letters are sewn stroke by
  stroke, in PE-DESIGN's block order. Bundled open-licence fonts (Lato Bold, Pacifico)
  or your own fonts, embedded in the design. Complex scripts (Arabic, Indic, right-to-left) are
  shaped.
- **Fit Text to Outline** and all 26 **Text Transform** presets, checked against PE-DESIGN.
- **Monograms**, **templates** with a Template Wizard, and **Name Drop** (one design, a list of
  names).
- **Your own embroidery fonts:** map stitch designs to characters, or build a font from TrueType
  glyphs and drawn shapes (Font Creator).

### Image digitizing

- **Auto Punch**, **Cross Stitch** and **Photo Stitch 1 and 2**: an image becomes embroidery
  regions, cross stitches or photo-style lines. These are independent approximations of
  PE-DESIGN's wizards, compared with its output.
- **Design Center** pipeline (image → outline → figure → sew settings) and the Stitch Wizard.
- ➕ **OpenStitch mode for Auto Punch, Photo Stitch 1 and Photo Stitch 2**: colours reduced,
  matched to threads and mixed by how they look (CIELAB, CIEDE2000, a linear-light mix in Photo
  Stitch 2); Auto Punch also merges specks and keeps line art's lines. The PE-DESIGN-compatible
  mode (OpenStitch's reimplementation of PE-DESIGN's method) stays the default; for some photos
  it still sews truer, so try both.
- **Background images** (PNG, JPEG, BMP or the clipboard): place, crop, rotate, adjust and trace
  over.

### Wizards and tools

- **Appliqué, embroidered patch, cutwork and background fill** wizards (approximations of
  PE-DESIGN's). The Applique Wizard's lines sit at PE-DESIGN's distances, in its appliqué
  colours.
- **Split designs** for large designs, **multi-position hoops** and the **jumbo frame**.
- **Design Database:** browse folders as thumbnails, search, organise, convert formats in batch
  and print catalogues.
- **Programmable Stitch Creator** and **Stitch Design Factory** (sewing-machine stitches; PMV
  export is experimental).
- **Printing:** actual-size sheets, PDF, and the operating system's printer.

### Files and formats

- **Read and write:** PES, PEC, DST, EXP, JEF, PCS, SEW, XXX, HUS, VIP, SHV, CSD: every type
  PE-DESIGN 11.50's Export offers. The machine formats match PE-DESIGN 11.50's own exports record
  for record or byte for byte (SHV's preview picture only approximately).
- **Read only:** PHC (read only in PE-DESIGN too).
- **PE-DESIGN 11 PES (`#PES0110`):** OpenStitch reads the metadata, thread table and shapes. Saved
  as "PE-DESIGN 11", its shapes and Lato Bold text open **editable** in PE-DESIGN 11.50 (text
  retypable), including Fit Text to Outline and Text Transform, Manual Punch blocks, stitch
  groups, grouped appliqué, hole sewing, programmable and decorative fill, the other region types
  and stem, candlewick and chain lines, with the Design Settings' hoop and machine type.
  `import.pes` opens PE-DESIGN's shapes and blocks as editable objects. A PE-DESIGN file opened
  with File ▸ Open and exported again unchanged is written back byte for byte, objects and all.
  Details in
  [docs/FILE_FORMATS.md](docs/FILE_FORMATS.md).
- **PES versions 1, 6 and 11** on export, with Design Property (category, author, keywords,
  comments). Version 6 with its stitch group is the default. Every PES version is read (1 to 11).
- **SVG import** as editable shapes, colours snapped to the Brother chart.
- **WMF and EMF import** (placeable and standard WMF, EMF) as editable shapes.
- **Editable `.osd` projects**, and large designs split into several files when a format's limits
  are exceeded.
- **Thread charts:** Brother's 64-colour PEC chart with its catalogue numbers, PE-DESIGN's
  colour matching, and your own charts as CSV, exported in the layout PE-DESIGN's Import Chart
  reads.

### Machine transfer

- **USB stick or SD card:** the editor and the Design Database find removable drives on every OS
  and write the machine file to them, asking before overwriting.
- Wireless transfer and Link are not available; see [Not yet implemented](#not-yet-implemented).

### Scripting and MCP

- **➕ `stitchtool`:** run JSON command scripts, list every command, or drive a JSON-lines
  control channel. Every editor action is the same engine command.
- **➕ `openstitch-mcp`:** an MCP server for Claude Code and other MCP clients, with one tool per
  command, batches and PNG previews; file access only to folders you grant.
- **➕ Live editor socket:** `openstitch --listen` lets scripts and MCP act on the running editor
  (127.0.0.1 only, per-launch token).
- **➕ AI assistant (B17):** View ▸ Show/Hide ▸ Assistant (Ctrl/Cmd+Shift+A) opens a chat where
  Claude makes, changes and explains designs, and can do anything in the editor.
  - It uses your own Claude API key, kept in the system keychain, or any OpenAI-compatible
    endpoint: Ollama or LM Studio on this computer (no key, works offline), OpenRouter and others.
    Data goes only to the address in the Assistant's settings.
  - Files, sending to the machine and quitting ask first.
  - Everything one message changes is one undo step, and Save as script writes the changes out.
  - Options ▸ Show AI features turns it all off.
  - Pictures: 📎 or drop files on the window to attach them; **View** and **Selection** send the
    design and what is selected.
  - Tried with real models (2026-11-04): Claude Code (signed in, over ACP) passed 12 of 12
    benchmark tasks; gpt-oss:20b on Ollama 8 of 12, and it made a badge with the network off;
    qwen3:14b 3 of 12. Smaller local models often pick wrong commands, so check the result. Not
    tried yet: the Claude API, LM Studio, OpenRouter and image generators.
    [Details](docs/research/beyond/ai-integration.md#round-2-real-local-models-the-composer-by-hand-claude-code-without-the-users-setup-2026-11-04-390-392-393).
- **➕ Assistant tools (B17, the base for the AI chat):**
  - `ui.*` editor commands on the socket reach everything the window offers: every ribbon and
    menu command, the selection, zoom, settings, a live render, the simulator, Help and
    updates.
  - `openstitch-mcp --assistant` offers them, with every engine command, as seven tools and a
    bundled embroidery skill.

### Installation and updates

- **Packages** for macOS (universal `.app`), Windows (MSI and portable zip) and Linux (AppImage,
  tarball, Flatpak), with `.osd` and `.pes` file associations.
- **➕ Ready-to-sew kit** in every package except the Flatpak (see
  [Ready-to-sew designs](#ready-to-sew-designs)): Help ▸ Ready-to-Sew Designs opens it in the
  file manager. Help ▸ Ready-to-Sew Designs Online opens it on GitHub.
- **➕ Linux file-manager thumbnails** for `.osd` and `.pes` files.
- **➕ Self-update** from GitHub releases, checked against the release's checksums (see
  [Updates](#updates)).

## Not yet implemented

PE-DESIGN 11 features OpenStitch does not have yet, from the feature matrix:

<!-- status:not-implemented -->
_None._
<!-- /status:not-implemented -->

Also missing, from the [roadmap](docs/ROADMAP.md#still-to-do):

- **Stroke-wise lettering under a Text Transform:** transformed Fill text still sews each
  letter by region (#428), and no lettering has under sewing per stroke yet.
- **Text details:** per-character colour, multi-character selection, Connection Points, minimum
  font sizes and small-text fonts, kerning that reflows the line.
- **Satin and fill details:** Miter limit (needs PE-DESIGN captures, #373), gradation per-step
  editing and Blend.
- **PES:** opening PE-DESIGN's texts and stitch groups as editable OpenStitch objects; motif
  fill, motif lines, several groups, right-aligned and vertical text in the editable PES 11
  writer; writing PES versions 4, 5 and 7–10 (waiting for PE-DESIGN's own files of them).

**Not planned, by design:**

- **PE-DESIGN's bundled fonts, motifs, stamps, floral and decorative patterns** are Brother's
  content. OpenStitch ships its own designs and open-licence fonts instead, and imports patterns
  you draw yourself.

**Blocked:**

<!-- status:blocked -->
- **Import PHX** (Files)
- **Import PEN** (Files)
- **Background image from a scanner** (Files)
- **ScanNCut FCM export/import** (Files)
- **Transfer directly to machine memory, wireless network, original card** (Machine)
- **Link function** (Machine)
- **Transfer by wireless network, USB cable to machine memory, original card, USB/Wireless Link** (Design Database)
- **Brother PMP and PSF files** (Stitch Design Factory)
- **File-manager thumbnails, macOS Quick Look and Windows Explorer** (OS integration)
- **PEN thumbnails** (OS integration)
<!-- /status:blocked -->

Wireless transfer, Link, card reading and PHX use undocumented protocols or formats that the
project will not reverse-engineer. PEN is an encrypted purchase format: reading it would
circumvent its protection. ScanNCut FCM has no Brother specification and malformed files can
reboot the machine. Scanners, Quick Look and Explorer thumbnails need native code (`unsafe`),
which the workspace forbids for now.

## Known issues

Open differences from PE-DESIGN 11 that you may notice, from the
[compatibility audit](docs/COMPATIBILITY_AUDIT.md):

<!-- status:known-issues -->
- [Trims between underlay, top stitching and outline of one object](docs/COMPATIBILITY_AUDIT.md#4-trims-between-underlay-top-stitching-and-outline-of-one-object)
- [Every design is written as one stitch group, nothing editable](docs/COMPATIBILITY_AUDIT.md#5-every-design-is-written-as-one-stitch-group-nothing-stays-editable-in-pe-design) (partly fixed)
- [One sewing direction per text instead of per stroke](docs/COMPATIBILITY_AUDIT.md#6-one-sewing-direction-per-text-instead-of-per-stroke)
- [Underlay model differs](docs/COMPATIBILITY_AUDIT.md#8-underlay-model-differs-zigzag-lines-have-no-underlay) (partly fixed)
- [PE-DESIGN-made PES files lose objects, properties and colours on re-save](docs/COMPATIBILITY_AUDIT.md#9-pe-design-made-pes-files-lose-objects-properties-and-colours-on-re-save) (partly fixed)
- [No Connection Points (Default / Closest / Farther) for text](docs/COMPATIBILITY_AUDIT.md#11-no-connection-points-default--closest--farther-for-text)
- [No minimum font sizes or small-text fonts](docs/COMPATIBILITY_AUDIT.md#12-no-minimum-font-sizes-or-small-text-fonts)
- [No per-character colour or multi-character selection](docs/COMPATIBILITY_AUDIT.md#13-no-per-character-colour-or-multi-character-selection)
- [Missing fill, satin and zigzag attributes; wrong ranges](docs/COMPATIBILITY_AUDIT.md#15-missing-fill-satin-and-zigzag-attributes-wrong-ranges) (partly fixed)
- [Character kerning moves one glyph without reflowing the line](docs/COMPATIBILITY_AUDIT.md#20-character-kerning-moves-one-glyph-without-reflowing-the-line)
<!-- /status:known-issues -->

Other things to know:

- **No sew-out yet:** no OpenStitch-written file has a recorded sew-out on a Brother machine.
- **Image digitizing is approximate:** Auto Punch, Cross Stitch and Photo Stitch are compared with
  PE-DESIGN but do not sew the same stitches.
- **Builds are not code-signed:** macOS and Windows warn on first launch (see
  [Download a build](#download-a-build)).
- **macOS:** double-clicking a `.osd` or `.pes` file in Finder does not open it; use File ▸ Open.
- **Updates:** versions before v0.1.0-alpha.4 have no updater and must be replaced by hand.

Bugs reported on GitHub:

<!-- status:issues-bugs -->
- [#290](https://github.com/cristianonescu/OpenStitch/issues/290) Auto Punch storybook pictures stack needle drops and fail sew_check
- [#368](https://github.com/cristianonescu/OpenStitch/issues/368) Storybook (OpenStitch mode): a hairline satin crosses 20 times in place and a lock sews 6 tiny stitches
- [#384](https://github.com/cristianonescu/OpenStitch/issues/384) Print: catalogue PDFs pile up in the temp folder (openstitch-print)
- [#398](https://github.com/cristianonescu/OpenStitch/issues/398) PES 0110 reader: CEmbLine variant in tatami-40 references stops the object list
- [#420](https://github.com/cristianonescu/OpenStitch/issues/420) B17 Assistant settings: Coding agent mode still offers an API key for the hidden address
- [#421](https://github.com/cristianonescu/OpenStitch/issues/421) CI: macos-latest jobs cancelled while queued (0 steps, often at exactly 15:00) during the 2026-11-04 Actions incident
- [#425](https://github.com/cristianonescu/OpenStitch/issues/425) B17 Ollama: mistral-small3.2 answers every bench task with an empty message
- [#429](https://github.com/cristianonescu/OpenStitch/issues/429) B17 Ollama: gpt-oss streams sometimes stop without a finish reason ("The connection closed before the reply finished")
- [#433](https://github.com/cristianonescu/OpenStitch/issues/433) Digitizing got about 8x slower with #299: fidelity_high_raises_the_colour_counts 12 s → 97 s
- [#441](https://github.com/cristianonescu/OpenStitch/issues/441) Kit designs, previews and README screenshots look sloppy: overlaps, cut-off previews, ragged lettering, jump lines
- [#446](https://github.com/cristianonescu/OpenStitch/issues/446) Satin text: N's acute corner sews as a spike, slanted arm ends fan, X sewn as chevrons (+13 %)
- [#450](https://github.com/cristianonescu/OpenStitch/issues/450) Review every calibration, demo and kit design and every screenshot for the defects #443 and #444 found
- [#458](https://github.com/cristianonescu/OpenStitch/issues/458) Diamond and Circle monograms cut letters: an outer letter loses its stem or bowl (K, E, M, O)
- [#459](https://github.com/cristianonescu/OpenStitch/issues/459) Programmable fill: the pattern does not show and stitches run 8-12 mm whatever stitchLength is
- [#460](https://github.com/cristianonescu/OpenStitch/issues/460) Region satin has no stitch-length limit: 20-30 mm stitches on wide shapes
- [#461](https://github.com/cristianonescu/OpenStitch/issues/461) Concentric, spiral, contour and radial fills: visible seams and a run back out from the centre
- [#462](https://github.com/cristianonescu/OpenStitch/issues/462) Stippling ignores spacing, leaves a 3-6 mm margin and runs an exposed L along the outline
- [#463](https://github.com/cristianonescu/OpenStitch/issues/463) Cross stitch and zigzag net regions: half crosses, travel per column, honeycomb leaving the region
- [#464](https://github.com/cristianonescu/OpenStitch/issues/464) Line sew types: stem stitch is not a rope, chain links uneven, candlewick spacing varies along the path
- [#465](https://github.com/cristianonescu/OpenStitch/issues/465) Small lettering: the i loses its dot, travel shows inside small N's and between letters, t crossbars loop
- [#466](https://github.com/cristianonescu/OpenStitch/issues/466) Satin and zigzag covers spike at heart cusps and fan at skeleton apexes
- [#467](https://github.com/cristianonescu/OpenStitch/issues/467) Laurel and stars frames trim leaf by leaf (28-31 trims in one colour)
- [#468](https://github.com/cristianonescu/OpenStitch/issues/468) Photo Stitch 1/2: colours off-hue, every layer sewn everywhere, needle drops in columns
- [#469](https://github.com/cristianonescu/OpenStitch/issues/469) Auto Punch / Cross Stitch from images: edge cells become a halo thread, thin features dropped, travel sewn on top
- [#470](https://github.com/cristianonescu/OpenStitch/issues/470) Appliqué wizard "add" keeps the original fill and sews it before the appliqué fabric
- [#471](https://github.com/cristianonescu/OpenStitch/issues/471) Fill with a direction curve sews a chaotic crosshatch; piping leaves a dense bar
<!-- /status:issues-bugs -->

[Report a bug](https://github.com/cristianonescu/OpenStitch/issues/new).

## Planned

Next, from [docs/ROADMAP.md](docs/ROADMAP.md):

- **Lettering:** stroke-wise sewing of transformed fill text and under sewing per stroke, as
  PE-DESIGN does it.
- **PES:** PE-DESIGN's text opened as editable objects; motif fill and lines, several groups and
  the remaining hoops in the PES 11 writer.
- **Stitches:** the remaining fill, satin and line details listed above, and branching skeletons.
- **Machine tier:** a first recorded sew-out on a Brother machine.
- **Image digitizing:** run-to-run travel in Cross Stitch, still fewer trims in Photo Stitch 1
  (hidden routed travel took OpenStitch mode from 68 to 25 trims per 1,000 stitches; PE-DESIGN
  sews a colour in one to five paths), and closer Photo Stitch output.
- **Distribution:** code signing and a Flathub listing.

Open enhancement issues:

<!-- status:issues-planned -->
- [#120](https://github.com/cristianonescu/OpenStitch/issues/120) Add a `#PES0110` editable-object-section writer
- [#129](https://github.com/cristianonescu/OpenStitch/issues/129) (Optional, larger) Photo-stitch rotating-line coverage density model
- [#130](https://github.com/cristianonescu/OpenStitch/issues/130) Add a PMV motif/pattern reader
- [#134](https://github.com/cristianonescu/OpenStitch/issues/134) Import external scalable embroidery font containers
- [#291](https://github.com/cristianonescu/OpenStitch/issues/291) Fit the Realistic view and the similarity scorer to the T1 calibration measurements
- [#292](https://github.com/cristianonescu/OpenStitch/issues/292) Sew-out reports wanted: other machines, and the non-PES formats
- [#295](https://github.com/cristianonescu/OpenStitch/issues/295) Review every ready-to-sew kit design against the sew-out findings
- [#298](https://github.com/cristianonescu/OpenStitch/issues/298) Photo Stitch 1 trims far more than PE-DESIGN: travel between a thread's runs instead
- [#311](https://github.com/cristianonescu/OpenStitch/issues/311) T2 calibration card: two threads in alternating rows, to measure how much a later row covers an earlier one
- [#312](https://github.com/cristianonescu/OpenStitch/issues/312) Kit build: check each calibration swatch's density in the exported PES
- [#319](https://github.com/cristianonescu/OpenStitch/issues/319) macOS: verify Help ▸ Ready-to-Sew Designs opens Finder at the bundled kit with a real click
- [#349](https://github.com/cristianonescu/OpenStitch/issues/349) SHV and CSD export: settle the parts one PE-DESIGN reference could not (preview, palette fill, long stitches)
- [#356](https://github.com/cristianonescu/OpenStitch/issues/356) Tests: weak points that need a product seam or a CI step (rest of #351)
- [#373](https://github.com/cristianonescu/OpenStitch/issues/373) Miter limit for satin and zigzag corners (PE-DESIGN attribute)
- [#374](https://github.com/cristianonescu/OpenStitch/issues/374) Write PES versions 4, 5 and 7–10 (readers done; blocked on PE-DESIGN Save As captures)
- [#377](https://github.com/cristianonescu/OpenStitch/issues/377) Connection Points (Default / Closest / Farther) for text
- [#397](https://github.com/cristianonescu/OpenStitch/issues/397) import.pes: PE-DESIGN texts and stitch groups as design objects
- [#401](https://github.com/cristianonescu/OpenStitch/issues/401) Test the AI Assistant and its key handling on Windows and Linux (so far macOS only)
- [#403](https://github.com/cristianonescu/OpenStitch/issues/403) CI: the warm Windows build is spent compiling ~100 test targets; consolidate integration tests per crate
- [#406](https://github.com/cristianonescu/OpenStitch/issues/406) readme_status.py: refuse inputs with merge-conflict markers instead of counting a truncated matrix
- [#408](https://github.com/cristianonescu/OpenStitch/issues/408) Design Database: the folder list draws a 📁 emoji, not the Higgsfield folder icon
- [#409](https://github.com/cristianonescu/OpenStitch/issues/409) Move up/down buttons use arrow characters (▲ ▼ ↑ ↓) instead of icons
- [#410](https://github.com/cristianonescu/OpenStitch/issues/410) Pane window controls use × ⊟ ⧉ characters instead of icons
- [#411](https://github.com/cristianonescu/OpenStitch/issues/411) Background image Rotate Left/Right buttons draw ⟲ ⟳ symbols
- [#412](https://github.com/cristianonescu/OpenStitch/issues/412) Menu buttons show a ▾ character as the drop-down caret
- [#413](https://github.com/cristianonescu/OpenStitch/issues/413) Change Color menu draws the thread colour as a ■ character
- [#426](https://github.com/cristianonescu/OpenStitch/issues/426) Flaky test: acp::an_agent_that_ends_at_once_fails_the_start under a loaded machine
- [#428](https://github.com/cristianonescu/OpenStitch/issues/428) Fill text under a Text Transform: sew by stroke once warped letters are cut reliably
- [#431](https://github.com/cristianonescu/OpenStitch/issues/431) B17 gate: a bare file name the assistant just wrote with scratch_write asks for permission
- [#432](https://github.com/cristianonescu/OpenStitch/issues/432) B17 settings: choosing the Ollama or LM Studio preset keeps the Claude model name
- [#439](https://github.com/cristianonescu/OpenStitch/issues/439) Higgsfield guard: cover every graphic in assets/, not only icons and illustrations
- [#445](https://github.com/cristianonescu/OpenStitch/issues/445) OpenStitch-mode Auto Punch sews a cream background and fuses letters across their gaps (g08)
- [#447](https://github.com/cristianonescu/OpenStitch/issues/447) Auto Punch (PE-compatible) maps 10 colours to 8 threads on p03 and drops the fox's orange; PE-DESIGN keeps 10
- [#452](https://github.com/cristianonescu/OpenStitch/issues/452) Sew a test swatch of the selection
- [#453](https://github.com/cristianonescu/OpenStitch/issues/453) Fabric calibration swatch: sew a graded card and start settings from the chosen rows
- [#454](https://github.com/cristianonescu/OpenStitch/issues/454) Garment mockups: show the design on a T-shirt, cap or tote as an image
- [#455](https://github.com/cristianonescu/OpenStitch/issues/455) SVG import: read stitch settings from layer and object names (opt-in)
- [#456](https://github.com/cristianonescu/OpenStitch/issues/456) Arched text with several lines as concentric rings
- [#457](https://github.com/cristianonescu/OpenStitch/issues/457) Read every exported file back and check it before reporting the export done
- [#472](https://github.com/cristianonescu/OpenStitch/issues/472) Align and distribute objects (to the sewing field or to each other, space evenly)
- [#473](https://github.com/cristianonescu/OpenStitch/issues/473) Snap to other objects while dragging, with smart guide lines
- [#474](https://github.com/cristianonescu/OpenStitch/issues/474) Precision bar: type exact position, size, rotation and skew; grid-step nudge
- [#475](https://github.com/cristianonescu/OpenStitch/issues/475) Select same colour or stitch type, and click inside an outline to select it
- [#476](https://github.com/cristianonescu/OpenStitch/issues/476) Copies along a line, scattered copies inside a shape, and kaleidoscope circles
- [#477](https://github.com/cristianonescu/OpenStitch/issues/477) Knife tool: cut many objects at once with an open line (fills, satins and lines)
- [#478](https://github.com/cristianonescu/OpenStitch/issues/478) Smooth shapes (remove extra nodes within a tolerance) and smooth stitch runs
- [#479](https://github.com/cristianonescu/OpenStitch/issues/479) Continue drawing a finished line or satin column from either end
- [#480](https://github.com/cristianonescu/OpenStitch/issues/480) Open and save Husqvarna/Pfaff VP3 files with thread brand, number and name
- [#481](https://github.com/cristianonescu/OpenStitch/issues/481) Read and write more machine formats: Tajima TAP/TBF, Barudan U01/DSB, ZSK DSZ, KSM, ZXY, STX
- [#482](https://github.com/cristianonescu/OpenStitch/issues/482) Thread list files: design colours in sewing order as CSV, INF beside EXP, INF/EDR/COL/TXT import and export
- [#483](https://github.com/cristianonescu/OpenStitch/issues/483) Import and export thread charts as GIMP/Inkscape .gpl palettes
- [#484](https://github.com/cristianonescu/OpenStitch/issues/484) Match a design to a thread chart by perceived colour (CIEDE2000), with a preview, keeping distinct colours apart
- [#485](https://github.com/cristianonescu/OpenStitch/issues/485) Merge similar thread colours in a design
- [#486](https://github.com/cristianonescu/OpenStitch/issues/486) More built-in thread charts from makers' own colour data, with search by number or name
- [#487](https://github.com/cristianonescu/OpenStitch/issues/487) SVG import: apply clip paths, masks and opacity, gradients as colour bands, and report what was skipped
- [#488](https://github.com/cristianonescu/OpenStitch/issues/488) Extra exports: stitch list CSV/TXT, multi-format ZIP bundle, stitch-out video, G-code, export repeats
- [#489](https://github.com/cristianonescu/OpenStitch/issues/489) Image wizard: purpose presets, a region table with reasons and overrides, click-to-fix on the picture, sew order and fill-angle choice
- [#490](https://github.com/cristianonescu/OpenStitch/issues/490) On-device helpers for picture digitizing: one-click subject cut-out, line drawing from photos, text found as lettering
- [#491](https://github.com/cristianonescu/OpenStitch/issues/491) Assistant remembers the user's machine, threads and preferences between conversations
- [#492](https://github.com/cristianonescu/OpenStitch/issues/492) Assistant skill: complete illustrated example designs and a composition guide
- [#493](https://github.com/cristianonescu/OpenStitch/issues/493) Assistant benchmark: illustrated-subject tasks scored on structure and on the render
- [#494](https://github.com/cristianonescu/OpenStitch/issues/494) run_batch and stitchtool exec report every parameter error at once before running
- [#495](https://github.com/cristianonescu/OpenStitch/issues/495) Assistant uses only the user's own threads and never invents catalogue numbers
- [#496](https://github.com/cristianonescu/OpenStitch/issues/496) stitchtool one-line commands: convert, info, optimize, digitize, formats
- [#497](https://github.com/cristianonescu/OpenStitch/issues/497) More shape tools: band frames, per-corner radii, spiral line, gap in an outline, divide, shape builder, shape effects
- [#498](https://github.com/cristianonescu/OpenStitch/issues/498) Python package for scripting OpenStitch designs
- [#499](https://github.com/cristianonescu/OpenStitch/issues/499) Keyboard-only drawing and an accessibility pass (text size, contrast, focus, screen reader)
- [#500](https://github.com/cristianonescu/OpenStitch/issues/500) Copy and paste objects through the system clipboard, also between editor windows
- [#501](https://github.com/cristianonescu/OpenStitch/issues/501) Stitch table for exact needle-point editing, and hand edits kept on live objects
- [#502](https://github.com/cristianonescu/OpenStitch/issues/502) Sewing order list: rename, lock, sew in click order, fill and outline order per object
- [#503](https://github.com/cristianonescu/OpenStitch/issues/503) Check Design pane: list sewing problems with located objects and one-click fixes
- [#504](https://github.com/cristianonescu/OpenStitch/issues/504) Density map: see where thread piles up, judged for the chosen fabric and thread
- [#505](https://github.com/cristianonescu/OpenStitch/issues/505) Review Sewing Order: step through every move with labels, a route map and warnings for long or untrimmed jumps
- [#506](https://github.com/cristianonescu/OpenStitch/issues/506) Random-design test: seeded designs must never crash, sew stitches over 12.1 mm or stray outside their shape
- [#507](https://github.com/cristianonescu/OpenStitch/issues/507) Parameter test swatches: copies of the selection with one setting stepped, each labelled with its value
- [#508](https://github.com/cristianonescu/OpenStitch/issues/508) Fabric and thread-weight presets with stabiliser and needle advice, machine profiles and a first-run machine setup
- [#509](https://github.com/cristianonescu/OpenStitch/issues/509) Basting box: a tacking run sewn first round the design, kept inside the hoop
- [#510](https://github.com/cristianonescu/OpenStitch/issues/510) Knockdown fill under the design or selection for towels and fleece
- [#511](https://github.com/cristianonescu/OpenStitch/issues/511) Leave out stitching hidden under later objects, with an overlap margin and without cutting the shapes
- [#512](https://github.com/cristianonescu/OpenStitch/issues/512) Stop after an object, and Stop at Jumps for machines without a thread cutter
- [#513](https://github.com/cristianonescu/OpenStitch/issues/513) Job sheets: client approval and operator worksheets with the colour sequence and job details
- [#514](https://github.com/cristianonescu/OpenStitch/issues/514) Hoop lists for other machine brands (Janome, Bernina, Husqvarna Viking/Pfaff, Baby Lock, Elna, Singer) and round hoops
- [#515](https://github.com/cristianonescu/OpenStitch/issues/515) Multi-hooping with free, overlapping hoopings that keep objects whole, with alignment marks and a stop
- [#516](https://github.com/cristianonescu/OpenStitch/issues/516) Stitch-out player: colour-block timeline, machine time, and stepping by colour, object or trim
- [#517](https://github.com/cristianonescu/OpenStitch/issues/517) View toggles: needle points, underlay only, trim marks, hide unselected, colour by order or monochrome, solo colour
- [#518](https://github.com/cristianonescu/OpenStitch/issues/518) Zoom box, display-calibrated real size, and a reference image that can sit between objects
- [#519](https://github.com/cristianonescu/OpenStitch/issues/519) Millimetres or inches throughout the editor
- [#520](https://github.com/cristianonescu/OpenStitch/issues/520) Satin corner styles: mitre and cap for sharp corners on satin lines, columns and borders
- [#521](https://github.com/cristianonescu/OpenStitch/issues/521) Short stitches on the inside of curves for every satin (columns, skeleton satin, borders, lettering)
- [#522](https://github.com/cristianonescu/OpenStitch/issues/522) Pull compensation per side and as a percentage of width; push compensation for fills
- [#523](https://github.com/cristianonescu/OpenStitch/issues/523) Automatic underlay chosen by column width and fabric, with a centre walk and a second underlay layer
- [#524](https://github.com/cristianonescu/OpenStitch/issues/524) Satin line with variable width: width while drawing, width handles anywhere, tapered ends, off-centre columns
- [#525](https://github.com/cristianonescu/OpenStitch/issues/525) Edit Manual Punch satin columns on the canvas: add, delete and drag rungs, swap sides, untangle twisted pairs
- [#526](https://github.com/cristianonescu/OpenStitch/issues/526) Convert a branching shape to satin: one column per arm, joined with an overlap, also inside Auto Punch
- [#527](https://github.com/cristianonescu/OpenStitch/issues/527) Embroidery font library with recommended size ranges, a font chooser with live sample, and a size warning
- [#528](https://github.com/cristianonescu/OpenStitch/issues/528) More satin looks: S-stitch, open zigzag, double satin, one-sided jagged edge, density measured across the column
- [#529](https://github.com/cristianonescu/OpenStitch/issues/529) Import Ink/Stitch-format embroidery font folders (font.json + SVG glyphs)
- [#530](https://github.com/cristianonescu/OpenStitch/issues/530) Single-line (centre-line) running text for small labels, like PE-DESIGN's Small Text
- [#531](https://github.com/cristianonescu/OpenStitch/issues/531) Raised (padded) satin: padding lanes sewn under the satin for a 3D look
- [#532](https://github.com/cristianonescu/OpenStitch/issues/532) Colour blends and gradient fills: two-colour blend, multicolour gradients with dithering, density profiles
- [#533](https://github.com/cristianonescu/OpenStitch/issues/533) Maze (space-filling) and tiled meander fills
- [#534](https://github.com/cristianonescu/OpenStitch/issues/534) Fill Angle drag tool and stitch angle lines that turn the rows inside one fill
- [#535](https://github.com/cristianonescu/OpenStitch/issues/535) Text envelopes with draggable handles (bridge, pennant, perspective, diamond)
- [#536](https://github.com/cristianonescu/OpenStitch/issues/536) Pixel (bitmap) font text sewn as short runs
- [#537](https://github.com/cristianonescu/OpenStitch/issues/537) Tatami textures: named needle-hole patterns, stepped and wave row shifts
- [#538](https://github.com/cristianonescu/OpenStitch/issues/538) Wave fill, and a flow fill whose rows follow the picture when digitizing
- [#539](https://github.com/cristianonescu/OpenStitch/issues/539) Tartan (plaid) fill
- [#540](https://github.com/cristianonescu/OpenStitch/issues/540) Hand-stitch look and scribble line with seeded smooth randomness
- [#541](https://github.com/cristianonescu/OpenStitch/issues/541) Ripple line: copies shrinking towards a target point or spiralling in as one run
- [#542](https://github.com/cristianonescu/OpenStitch/issues/542) Back stitch and a variable running stitch that follows a thickness profile
- [#543](https://github.com/cristianonescu/OpenStitch/issues/543) Decorative hand-embroidery lines: chevron, Cretan, feather, overlock, triangles, double zigzag, lattice, stripes, slanted E-stitch
- [#544](https://github.com/cristianonescu/OpenStitch/issues/544) Motif lines: groups with gaps, free ends, alternating or cycling motifs, motifs that scale with a band, more built-in motifs
- [#545](https://github.com/cristianonescu/OpenStitch/issues/545) Travel under later fills and run connectors instead of jumps and trims between same-thread objects
- [#546](https://github.com/cristianonescu/OpenStitch/issues/546) Branching and auto-routing: touching objects and line networks sewn in one go without trims
- [#547](https://github.com/cristianonescu/OpenStitch/issues/547) Start each object nearest to where the last ended, and report what Optimize changed
- [#548](https://github.com/cristianonescu/OpenStitch/issues/548) Per-object trim and tie settings, and trimming between letters, words or lines of text
- [#549](https://github.com/cristianonescu/OpenStitch/issues/549) Autosave and recovery after a crash or power cut
- [#550](https://github.com/cristianonescu/OpenStitch/issues/550) Command palette (search every command), a modifier-aware hints line, and search in the properties
- [#551](https://github.com/cristianonescu/OpenStitch/issues/551) Customisable shortcuts, tool band, dockable panels and saved layouts
- [#552](https://github.com/cristianonescu/OpenStitch/issues/552) Several designs open as tabs in one window
- [#553](https://github.com/cristianonescu/OpenStitch/issues/553) Defaults for new objects, Basic/Advanced stitch settings, and picture pickers with live hover preview
- [#554](https://github.com/cristianonescu/OpenStitch/issues/554) Getting started: a first-run sample design, start cards and an in-app first-steps page
- [#555](https://github.com/cristianonescu/OpenStitch/issues/555) Translate the interface into other languages
- [#556](https://github.com/cristianonescu/OpenStitch/issues/556) Run the editor in a web browser (WebAssembly build)
- [#557](https://github.com/cristianonescu/OpenStitch/issues/557) Design thumbnails in macOS Finder (Quick Look) and Windows Explorer
- [#558](https://github.com/cristianonescu/OpenStitch/issues/558) Open and import large files in the background with progress and Cancel
- [#559](https://github.com/cristianonescu/OpenStitch/issues/559) Accent colour choice beside the Light / Dark / System theme
<!-- /status:issues-planned -->

### Beyond PE-DESIGN

After PE-DESIGN compatibility, OpenStitch adds what other digitizing programs offer and
PE-DESIGN 11 does not. Each item is sketched in
[docs/research/beyond](docs/research/beyond/readiness.md) and will be off unless chosen:

<!-- status:beyond-planned -->
- Image similarity, Phase 0 (measurement): the similarity score (0.4 MS-SSIM, 0.4 CIEDE2000, 0.2 DISTS on a thread render at actual size from 40 cm) with a sewability report, a calibration test card and a realistic render fitted to its photographed sew-out, deterministic digitizing maths and cross-platform golden hashes (B16.1)
- Image similarity, Phase 2 (Photo Stitch 2): least-squares thread shares with 4–6 threads and a sew-out-calibrated coverage model, at a design mean ≤ 3.3 needle points/mm²; no fixed target (the ≥ 75 target was dropped on 2026-10-31: with the line look the best measured is about 48), it must beat PE-DESIGN-compatible mode on every photo (B16.1)
- Image similarity, Phase 3 (Photo Stitch 1 High fidelity): fine colour sampling with variable stitch boundaries, rate-constrained rows under 2.1 needle points/mm², a saliency-weighted budget, metric-driven local search; firm target 85 % (B16.1)
- Image similarity, Phase 4 (Auto Punch): a sewn coloured background, SLIC regions that keep line art, skeleton satin for outlines; target 95 % for logos, clip art and lettering (B16.1)
- Image similarity, Phase 5 (Cross Stitch and Design Center): OpenStitch modes for both, with mean-Lab cells, edge part-crosses and an Otsu line threshold; targets g07 ≥ 85, g03 and g04 ≥ 95 (B16.1)
- Density profiles on fills and satin columns, two-colour blend command, hidden travel for open fills (B6)
- 3D puff satin mode: density, caps, no underlay or pull compensation, foam stop, ordering (B7)
- Crosshatch, curved crosshatch and double-zigzag lace as a region type (B8)
- Carving lines drawn on fills; knockdown (laydown) fill sewn first (B12)
- Satin corner handling (mitre / cap / lap) and short stitches on inner curves for columns and regions (B3)
- Pull compensation in mm per side and %, push compensation, automatic underlay by width with first and second underlay, editable fabric table (B5)
- Satin line with variable width: width keys, two rails, tapered ends (B10)
- Auto-routed running stitches (Euler path), Redwork, satin branching with one entry and exit (B4)
- Closest join per object, auto start/end, continue digitizing, automatic overlap removal (B11)
- Guided fill with two guides; stitch-angle maps with several direction lines per region (B2)
- Maze and tiled meander fills, zigzag (fur) variant, seeded hand-stitch randomisation (B9)
- Quality tools: density map, troubleshoot rules, thread-brand conversion, automatic hoop selection (B13)
- Stitch-file resize with density recalculation and density tuning (B14)
- Lettering: baseline model, saved kerning pairs, team names from CSV, closest-point letter connections (B15)
- Test swatch of the selection: the selected objects centred in the smallest hoop they fit and exported or sent on their own, the design unchanged (#452)
- Fabric calibration swatch: a graded card of fill density, satin density and pull compensation sewn on the user's fabric, the chosen rows saved as a custom fabric entry beside the Fabric Selector's types (#453)
- Garment mockups: the Realistic preview placed on a T-shirt, cap or tote picture (OpenStitch's own artwork) and saved as an image (#454)
- SVG import reads stitch settings from layer and object names (opt-in naming syntax; the default import unchanged) (#455)
- Arched text with several lines as concentric rings around one centre (#456)
- Export read-back check: every exported or transferred file read back with OpenStitch's reader and compared with the written pattern (stitches, colour changes, bounds, PEC frame) (#457)
- AI assistant tool surface (B17, phase A0): everything the user can do in the editor is reachable from the assistant. The automation socket answers ui.* editor commands beside the engine's (state, every ribbon/menu command through ui.run, selection, zoom, settings, live render, simulator, Help, updates, a scratch folder for SVGs and pictures). openstitch-mcp --assistant offers seven meta tools (commands_list, command_describe, execute, run_batch, view_render, scratch_write, skill_reference) over them, and a bundled embroidery skill (skills/openstitch-embroidery)
- AI assistant panel (B17, phase A1): a chat beside the design (View ▸ Show/Hide ▸ Assistant, Ctrl/Cmd+Shift+A). Claude API with the user's own key in the OS keychain (default claude-opus-5-5, streaming, adaptive thinking). The model drives the editor through the A0 tools in-process. A permission gate judges each command by its effect: files, machine transfer, Exit and undoing earlier work ask (Allow once / Always allow / Deny, declined after 4 minutes), and there is an optional change limit. Everything one message changes is one undo step, Save as script writes it out, and messages typed during a turn are queued. Preferences: Options ▸ Show AI features (on by default, owner 2026-11-04) and the panel's settings. The panel opens on first start and then as the user left it (panelOpen in ui.json; an older ui.json reads as open), and closing it says how to reopen it (View ▸ Show/Hide ▸ Assistant, the shortcut, and Options ▸ Show AI features when AI is off) until "Don't show this again" (#422)
- AI assistant providers (B17, phases A2-A3). A2: besides the Claude API, any OpenAI-compatible /chat/completions endpoint, with presets for Ollama and LM Studio on this computer (no key needed) and OpenRouter. The settings choose the protocol, list the endpoint's models, set the request timeout (30-3600 s) and an optional reasoning effort, and warn when an Ollama model has no tools capability. A notice shows when a model answers an edit request without using its tools. A3: coding agents over the Agent Client Protocol (Claude Code via claude-agent-acp, Codex via codex-acp, OpenCode and others) on the user's own account. They are found on PATH and in the usual install folders, with install commands for missing ones. Each runs in its own process group in an empty folder, with no file system or terminal declared and Claude Code's own shell, file and web tools turned off. Its only way into the editor is openstitch-mcp --assistant on a private loopback socket whose every request passes the panel's gate
- AI assistant pictures (B17, phase A4): the composer's 📎 attaches up to 8 files per message (pictures PNG/JPEG/BMP sent as images, scaled to 2048 px and re-encoded under 3.75 MB; SVG and text sent as text; stitch files offered to stitches.import), each copied into the scratch folder. **View** sends a picture of the design and **Selection** the selected objects' ids, kinds and sizes, shown as chips. An attached picture is digitized with digitize.autoPunch in OpenStitch mode; a model-written SVG goes through scratch_write and import.svg. An optional image generator (an OpenAI-compatible /images/generations address, its key in the keychain) adds generate_image; without one the tool points the model at the vector route. OpenAI-compatible endpoints get tool-result pictures as a user message with image parts
- AI assistant benchmark (B17, phase A5): stitchtool bench-assistant sends 12 embroidery tasks (badge, monogram, arc text, appliqué, star, two colours, a wider rectangle, recolour, delete, picture to embroidery, vector route, a question) to the Claude API, an OpenAI-compatible endpoint or an ACP coding agent, through the assistant's own tools over a headless editor. Each task is scored on the resulting design (object.list, design.info), failures are sorted into categories, and it writes JSON and Markdown reports
- Align objects to each other or to the sewing field and space three or more evenly (object.align, object.distribute, Home ▸ Align) (#472)
- Snap dragged objects to other objects' edges and centres and to the Design Page centre, with dashed smart guides (View ▸ Snap to Objects) (#473)
- Numeric position, size, rotation and skew of the selection (precision bar), object.skew, and arrow nudge by the grid step when snapping (#474)
- Edit ▸ Select Same (thread colour / stitch type) for shapes and stitch patterns; optional click inside unfilled outlines to select them (#475)
- Arrange Copies along a path, scattered inside a shape (seeded) and as a kaleidoscope (alternate copies mirrored), sewn colour by colour (#476)
- Knife: an open cut line splits every crossed fill, outline, Manual Punch column and line in one step (shape.knife) (#477)
- Smooth Shape removes nodes within a tolerance (shape.simplify, 0.2 mm) and Smooth Stitches relaxes needle points of stitch patterns (stitches.smooth) (#478)
- Continue digitizing: pick the tool and click an end of a finished line or column to add more points (B11) (#479)
- Extra machine formats beyond PE-DESIGN: Tajima TAP/TBF and Barudan U01 write, DSB, DSZ, KSM, ZXY, STX read (#481)
- Thread list files: the design's colours in sewing order as CSV, an optional INF beside EXP, and INF/EDR/COL/TXT lists imported, exported and applied to colour blocks (#482)
- GIMP/Inkscape .gpl palettes imported and exported as user thread charts (#483)
- Perceptual thread matching (CIEDE2000) for threads.remap as an opt-in, with a match preview and keep-distinct rule; PE's RGB rule stays the default (#484)
- Merge similar colours: threads closer than a ΔE threshold joined into one, previewed and undoable (#485)
- More built-in thread charts compiled from each maker's own published colour data, searchable by number or name (#486)
- Vector image import (PE-compat area): clip paths, masks and opacity applied, gradients as optional colour bands, and a report of skipped elements (#487)
- Extra exports: stitch list CSV/TXT, multi-format ZIP bundle, stitch-out video, G-code and export-time repeats (#488)
- Image wizard (OpenStitch mode): purpose presets, a region table with reasons and overrides, click-to-fix on the picture, colour order, varied or least-travel fill angles and outline SVG export (#489)
- Optional on-device helpers for digitizing: one-click subject segmentation, line drawing from photos and OCR to lettering (#490)
- Assistant memory: per-user notes read into every conversation, written through the permission gate, viewable and clearable (#491)
- Assistant skill with complete illustrated example designs and a composition guide (#492)
- Assistant benchmark with illustrated-subject tasks scored on structure and on the rendered result (#493)
- Batches validate every step's parameters first and report all errors at once before running anything (#494)
- Assistant picks colours only from the user's own or named thread chart and never quotes a catalogue number that is not in a chart (#495)
- stitchtool one-line commands: convert, info, optimize, digitize and formats (#496)
- Shape tools beyond PE: band frames, per-corner radii, spiral line, gap in a closed outline, Divide, shape builder and shape effects (warp, twist, roughen) (#497)
- Python package (pyo3 wheel) for scripting designs with the engine's command set (#498)
- Draw shapes, lines and Manual Punch columns from the keyboard; follow the system text size; WCAG contrast; visible focus; screen-reader names checked in CI (#499)
- Cut/Copy/Paste objects through the system clipboard so they survive between two editor windows and app restarts (#500)
- Stitch table (coordinates and command per needle point, insert/delete) and hand stitch edits stored on a live object until a setting changes (#501)
- Sewing Order list with rename, lock, sew-in-click-order and per-object part order (outline before fill), plus linked clones (#502)
- Check Design pane and design.check: located sewing problems (outside hoop, satin width, thin fills, small text, long stitches, untrimmed jumps, stacked layers, tiny objects) with one-click fixes and a ready-to-sew status (#503) (#503)
- Density map overlay and design.density: thread layers and needle holes per 1 mm cell judged against the chosen fabric, hotspots with Show/Next, dark background for light threads (#504) (#504)
- Review Sewing Order: step and route modes naming every move between objects, with warnings for long same-thread moves and untrimmed jumps (design.route) (#505) (#505)
- Seeded random-design invariant test in CI: no crash, no stitch over 12.1 mm, no stitch outside its object, deterministic and round-trip stable (#506) (#506)
- Parameter test swatches: copies of the selected object with one or two settings stepped, labelled with their values, laid out in the hoop as a new design (design.testSwatches) (#507) (#507)
- More fabric presets, thread weight driving density and stabiliser/needle/minimum-text advice, named machine profiles and a first-run machine setup (#508) (#508)
- Basting box or outline: a tacking run sewn first in the first thread, at a margin and clipped to the hoop (#509) (#509)
- Knockdown (laydown) fill sewn first under the design or selection, gaps between nearby shapes closed, one or two crossed layers (B12) (#510) (#510)
- Live remove-hidden-stitching: stitches under later objects left out at compile time with an overlap margin, shapes unchanged, stitches saved reported (B11) (#511) (#511)
- Machine stops: stop after a chosen object and a one-click Stop at Jumps for machines without a thread cutter (#512) (#512)
- Job sheets: client approval and operator worksheets with job fields, colour sequence, stops, time and branding (W7) (#513) (#513)
- Hoop lists for other machine brands and round commercial hoops, each from the maker's published sizes (#514) (#514)
- Free multi-hooping: overlapping hoopings placed freely, objects kept whole, alignment marks followed by a stop, per-hooping files (W1) (#515) (#515)
- Stitch-out player: colour-block timeline with trim ticks, machine time at the needle, stepping by colour/object/trim, pause at trims, follow the needle (#516) (#516)
- View toggles: needle points, underlay or top only, trim marks, hide unselected, colour by sewing order or monochrome, solo colour (#517) (#517)
- Zoom box, real size calibrated to the display, and a reference image placed between objects in the sewing order with canvas handles (#518) (#518)
- Millimetres or inches throughout the editor (Inspector, rulers, grid, lettering, hoops, print), stitch settings kept in mm (#519) (#519)
- Satin corner styles (turn / mitre below 75° / cap below 45°, 0.5 mm overlap) on satin regions, columns, skeleton satin, zigzag lines and borders (B3) (#520)
- Short stitches on inner curves for every satin kind (Manual Punch, skeleton satin, borders, lettering), with distance and inset settings (B3) (#521)
- Pull compensation per side and as a percentage of stitch width, plus push compensation on straight-row fills (B5) (#522)
- Automatic underlay by column width and fabric, centre-walk underlay, width-following underlay count and an independent second underlay layer (B5) (#523)
- Satin line on any path with width keys, width set while drawing, tapered ends, per-side widths and a constant slant (B10) (#524)
- Manual Punch column editing on the canvas: insert, delete and drag rungs, swap sides, automatic untangle of reversed pairs (#525)
- Convert to satin with branches: one column per arm of a Y, star or letter, overlapping joins and corner rungs, also in Auto Punch's OpenStitch mode (#526)
- Bundled embroidery fonts (licence-clean, incl. an OpenStitch centre-line satin font) with a recommended size range per font, a chooser showing the user's own words, and an out-of-range warning (#527)
- Satin styles S-stitch, open zigzag and double satin, a one-sided seeded jagged edge, and fractional density spacing across curved columns (#528)
- Read embroidery font folders in Ink/Stitch's published format (font.json + per-glyph SVG with stitch parameters, kerning) from a user font folder; no fonts bundled (#529)
- Small Text: single-line fonts sewn as one running (or bean) line along each letter's centre for 3–6 mm labels (#530)
- Raised satin: padding lanes sewn under the satin for a 3D look without foam (first step of B7) (#531)
- Two-colour blend command, multicolour gradient fills with dithering, named density profiles and SVG linear gradients imported as blends (B6) (#532)
- Maze (space-filling curve) and tiled meander fills sewn as one continuous line with own-drawn tiles (B9) (#533)
- Stitch angle lines: several direction lines on one fill with rows turning smoothly between them (B2); the canvas Fill Angle tool is the PE-compat sewing-direction gesture (#534)
- Free text envelopes edited with on-canvas handles (bridge, pennant, perspective, diamond; symmetric bending with a modifier) (#535)
- Pixel-font lettering: text from a bitmap font sewn as short runs or cross stitches per pixel (#536)
- Tatami textures: named needle-hole arrangements, steps x repetitions and wave/triangle/arc row shifts on plain fills (#537)
- Wave fill (height, length) and an image-derived flow fill whose rows follow the picture's direction field in Auto Punch's OpenStitch mode (#538)
- Tartan fill: warp and weft stripe sets woven in several threads inside one shape (deferred in the roadmap) (#539)
- Seeded hand-stitch randomisation on lines and satins and a scribble line with smooth noise (B9) (#540)
- Ripple line: copies of a line shrinking towards a draggable target, closed shapes spiralling in as one run (#541)
- Back stitch line and a variable running stitch whose passes follow a thickness profile, on a shared named-profile type (#542)
- Hand-embroidery decorative line types (chevron, Cretan, feather, overlock, alternating triangles, double zigzag, lattice, stripes) and a prong angle for E-stitch (#543)
- Motif line placement by groups and gaps, free ends, cycling and mirrored motifs, width-scaled motif bands and more bundled motifs (#544)
- Travel under later fills and per-object run connectors replacing jumps and trims between same-thread objects (B6 hidden travel, B4) (#545)
- Branching and auto-routing: touching objects and running-line networks (Redwork) sewn in one go with travel hidden inside (B4) (#546)
- Automatic nearest start per object by stitch type and a before/after report from Optimize (B11) (#547)
- Per-object trim and tie policies (when far / always / off, tie count and length) and text trimming per letter, word or line (#548)
- Autosave snapshots of open designs and a recovery offer at the next start (never overwriting the original file) (#549)
- Command palette searching every ribbon/menu command (recent first), a hints line for the current tool and modifiers, and a properties search box (#550)
- Editable keyboard shortcuts (several per command, clash check, import/export), customisable tool band, dockable panels with saved layouts, per-area preference reset (#551)
- Several designs open as tabs in one editor window, each with its own undo, selection and view (#552)
- A settings page for what new objects start with, a Basic/Advanced split of the sewing attributes, and picture pickers that preview a style on hover (#553)
- First-run start cards (draw, letter, digitize a picture, import, open a sample), File ▸ Open Sample and Help ▸ Getting Started (#554)
- Interface translations: a string catalogue with a language choice in Preferences (English default), printed worksheets in the chosen language (#555)
- A browser (WebAssembly) build of the editor with in-browser file open/save and download export (#556)
- Thumbnails of .osd and machine files in macOS Finder (Quick Look extension) and Windows Explorer (thumbnail handler) (#557)
- Open/import runs off the UI thread with a progress bar and Cancel, so huge files never freeze the window (#558)
- An accent (highlight) colour choice in Preferences beside the existing Light / Dark / System themes (#559)
<!-- /status:beyond-planned -->

## Download a build

The latest pre-release is on the
[Releases](https://github.com/cristianonescu/OpenStitch/releases) page, with notes in
[docs/releases](docs/releases/). It is an alpha for testing, not for production work: do a test
sew-out on scrap fabric first.

Every merge to `main` also builds the executables for all three operating systems. Open the
repository on GitHub, go to **Actions → Rust workspace**, pick the latest run on `main` and
download the artifact for your system:

| Artifact | Contents |
|---|---|
| `openstitch-main-<sha>-macos-universal` | `OpenStitch-…-macos-universal.zip` (the `OpenStitch.app` editor, Apple Silicon and Intel) and `openstitch-cli-…tar.gz` (`stitchtool`, `openstitch-mcp`) |
| `openstitch-main-<sha>-windows-x86_64` | `OpenStitch-…-windows-x86_64.msi` (installer with `.osd`/`.pes` associations), a portable `.zip` of the editor, and `openstitch-cli-…zip` |
| `openstitch-main-<sha>-linux-x86_64` | `OpenStitch-…-linux-x86_64.AppImage` (single file: `chmod +x` and run; `--install-integration` adds the menu entry, `.osd`/`.pes` types and thumbnails), `OpenStitch-…-linux-x86_64.tar.gz` (editor with `install.sh`, desktop entry, MIME types and thumbnailer) and `openstitch-cli-…tar.gz` |

A Flatpak bundle (`openstitch-flatpak-x86_64`) is built by the **Flatpak** workflow when the
Linux packaging changes, and can be built locally from
`packaging/flatpak/io.github.cristianonescu.OpenStitch.yml`.

Details are in [docs/DISTRIBUTION.md](docs/DISTRIBUTION.md).

The builds are not code-signed yet:
- **macOS:** unzip and move `OpenStitch.app` to Applications, then right-click → Open the first
  time (or run `xattr -dr com.apple.quarantine OpenStitch.app`).
- **Windows:** choose *More info → Run anyway* in the SmartScreen prompt.

### Updates

The editor keeps itself current. At a plain start it asks GitHub whether a newer release
exists. If so, a dialog shows the release notes with three choices:

- **Dismiss.**
- **Don't remind me about this version:** quiet until a newer one appears.
- **Update:** downloads the package for your system, checks it against the release's
  `SHA256SUMS`, installs it and restarts OpenStitch.

The portable zip, the MSI (Windows Installer asks for permission), the macOS app and the Linux
AppImage and tarball all update in place; a Flatpak updates through Flatpak. In Options,
*Check for updates at startup* turns the check off, *Include pre-releases* opts a stable build
into alphas and betas, and *Check for Updates Now* asks right away. Builds from source and the
artifacts of `main` have no release tag and never check. Details:
[docs/DISTRIBUTION.md](docs/DISTRIBUTION.md#updates).

## Build from source

### Prerequisites

- **Rust** stable via [rustup](https://rustup.rs) (CI builds with the current stable; the
  workspace uses the 2024 edition):
  - macOS: `brew install rustup && rustup default stable`
  - Windows: install rustup-init and the "Desktop development with C++" build tools; the
    toolchain is the MSVC one (`x86_64-pc-windows-msvc`, rustup's default on Windows), which
    is also what embeds the icon in `openstitch.exe`
  - Linux: the rustup installer from rustup.rs
- **Linux only, at run time:** the desktop libraries every X11/Wayland desktop already has
  (`libxkbcommon`, `libwayland-client` or `libX11`, and Vulkan or OpenGL drivers). Building needs
  no extra `-dev` packages.

### Build

```bash
git clone https://github.com/cristianonescu/OpenStitch.git
cd OpenStitch
cargo build --release
```

The binaries are written to `target/release/`: `openstitch` (the editor), `stitchtool` and
`openstitch-mcp` (`.exe` on Windows).

### Run

```bash
cargo run --release -p openstitch-app                   # the desktop editor
cargo run --release -p openstitch-app -- design.osd     # open a design or machine file
```

```bash
cargo run -p stitchtool -- exec examples/badge.json   # run a command script → badge.pes + badge.osd
cargo run -p stitchtool -- exec examples/heart.json   # Bézier example
cargo run -p stitchtool -- commands                   # list every command (JSON)
cargo run -p stitchtool -- serve                      # JSON-lines control channel on stdio
```

```bash
cargo run -p openstitch-app -- --listen 0 --listen-root ~/Embroidery   # editor with the automation socket
cargo run -p stitchtool -- connect                    # JSON lines to the running editor
cargo run -p openstitch-mcp -- --live                 # MCP tools act on the running editor
```

The socket listens on 127.0.0.1 only and every request must carry the per-launch token that the
editor prints and writes to `automation.json` (readable by you only) in the OpenStitch config
directory; clients read it from there. See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md#automation-socket-live-editor).

A command script is a JSON array of `{"command": …, "params": {…}}` steps; see `examples/`.
Lengths are in millimetres and angles in degrees. Sewing directions turn counter-clockwise on
screen, as in PE-DESIGN (45 = rows rising to the right); `.osd` files from before format
version 2 stored them clockwise and are converted when opened.

### Test

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo run -p stitchtool -- compat
```

Debug and test builds optimise the dependencies, the stitch pipeline (`openstitch-core`,
`-design`, `-engine`) and image digitizing (`openstitch-digitize`; `[profile.dev.package]` in
`Cargo.toml`), so the editor's and the Auto Punch tests run in minutes instead of an hour; debug
assertions and overflow checks stay on. The first build takes a few minutes longer.

Tests make their temporary folders with `openstitch_testutil::TempDir`, which deletes the folder
when the test ends. Set `OPENSTITCH_KEEP_TEMP=1` to keep a failing test's folders; their paths
are printed.

CI runs the same checks on macOS, Windows and Linux for every pull request, checks that
`docs/COMPATIBILITY_STATUS.md` matches the harness's result, and scans the history for secrets
with gitleaks. CI builds with line-tables-only debug info (`CARGO_PROFILE_DEV_DEBUG`), and a new
push to a pull request cancels that PR's run still in progress; runs on `main` and on tags are
never cancelled. Only runs on `main` save the build cache; pull requests restore it, so they
don't evict it from the repository's 10 GB of Actions cache.

## MCP server (Claude Code and other MCP clients)

`openstitch-mcp` exposes every engine command as an MCP tool. It also adds `run_batch` (up to 256
commands at once) and `preview` (a PNG of the generated stitches). It talks JSON-RPC over stdio
and opens no network port.

File commands can only touch the directories you grant:
- `--root DIR` is read/write and `--read-root DIR` is read-only; both can be repeated.
- With no roots, every file command is refused.

Register it with Claude Code, from this checkout:

```bash
claude mcp add openstitch -- cargo run -q --release -p openstitch-mcp --manifest-path "$PWD/Cargo.toml" -- --root ~/Embroidery
```

or build once and register the binary (faster start-up, no cargo output on stdio):

```bash
cargo build --release -p openstitch-mcp
```

```bash
claude mcp add openstitch -- "$PWD/target/release/openstitch-mcp" --root ~/Embroidery --read-root ~/Documents/Designs
```

To let an MCP client control the whole editor (the base of the B17 AI assistant), start the
editor with `--listen 0` and register `openstitch-mcp --assistant`. It offers seven tools
instead of one per command: `commands_list`, `command_describe`, `execute`, `run_batch`,
`view_render`, `scratch_write` and `skill_reference`. They reach every engine command and the
editor's `ui.*` commands: ribbon and menu commands, selection, zoom, settings, the simulator,
Help and updates. A bundled embroidery skill (`skills/openstitch-embroidery`) tells the model
how to build designs that sew well.

Then ask Claude, for example, to "make a 60 × 40 mm badge with a red satin border, preview it and
export badge.pes". `--read-only` blocks every editing command and `--no-files` blocks all file
access; run `openstitch-mcp --help` for these and the size-limit flags. Other clients (Claude
Desktop, IDEs) take the same command and arguments in their MCP server configuration.

## Contributing

- Work on a branch and open a pull request to `main`; nobody pushes to `main` directly.
- A PR is merged once CI is green on all three operating systems. Merge scripts must treat a
  cancelled (superseded) run as pending; see
  [docs/RELEASING.md](docs/RELEASING.md#merging-pull-requests).
- Every merge to `main` builds the executables (see [Download a build](#download-a-build)).
- Every PR that changes a feature updates [docs/FEATURE_MATRIX.md](docs/FEATURE_MATRIX.md); the
  README's status lists are generated from it (`python -I tools/readme_status.py --write`). The
  GitHub issue lists are refreshed only at each release (`--issues`), so PRs don't carry them.
- Rules for the code (clean-room boundaries, command registry, golden tests) are in
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) and [docs/LEGAL_AND_LICENSING.md](docs/LEGAL_AND_LICENSING.md).

## Releases

Only the maintainer creates release tags:

- `vX.Y.Z` is a stable release.
- `vX.Y.Z-alpha.N`, `-beta.N` and `-rc.N` are pre-releases, marked as such on GitHub.

Pushing a tag on `main` runs the **Release** workflow: it checks the tag against the workspace
version, runs the full test suite, builds all three platforms and creates a **draft** GitHub
release with the packages and the notes from `docs/releases/<tag>.md`. The full procedure,
including the README refresh done at every release, is in [docs/RELEASING.md](docs/RELEASING.md).

## Documents

- [Research log](docs/RESEARCH.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Editor window layout](docs/UI_LAYOUT.md)
- [Feature matrix](docs/FEATURE_MATRIX.md)
- [File formats](docs/FILE_FORMATS.md)
- [Compatibility tests](docs/COMPATIBILITY_TESTS.md) and the generated
  [compatibility status](docs/COMPATIBILITY_STATUS.md)
- [Compatibility audit](docs/COMPATIBILITY_AUDIT.md)
- [Test coverage audit](docs/TEST_COVERAGE_AUDIT.md): features without test evidence, and the plan
- [Adding a PE-DESIGN reference file](compat/reference/README.md)
- [Legal and licensing](docs/LEGAL_AND_LICENSING.md)
- [Roadmap](docs/ROADMAP.md) and the [backlog](docs/BACKLOG.md): open work grouped into batches, one PR per batch
- [Distribution and packaging](docs/DISTRIBUTION.md)
- [Releasing](docs/RELEASING.md) and the [release notes](docs/releases/)
- [Test images for the image wizards](docs/research/test-images.md)
- Handoffs (history): [2026-10-30](docs/HANDOFF-2026-10-30.md), [2026-10-29](docs/HANDOFF-2026-10-29.md),
  [2026-10-28](docs/HANDOFF-2026-10-28.md), [2026-10-27](docs/HANDOFF-2026-10-27.md),
  [2026-10-19](docs/HANDOFF-2026-10-19.md), [2026-10-18](docs/HANDOFF-2026-10-18.md)

`tools/` holds the Python scripts that generate fixtures, data tables, screenshots and the
README's status lists (`pip install pyembroidery pillow`, run with `python -I`).

## License

OpenStitch is licensed under either of

- Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE))
- MIT license ([LICENSE-MIT](LICENSE-MIT))

at your option. Unless you explicitly state otherwise, any contribution you intentionally submit
for inclusion in OpenStitch, as defined in the Apache-2.0 license, is dual-licensed as above,
without any additional terms or conditions.

Bundled fonts and third-party crates keep their own licences; see
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).
