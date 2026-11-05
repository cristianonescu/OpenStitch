# Answers about OpenStitch

For questions about the program itself. Answer from this file and the command list; when
unsure, say so instead of guessing.

## What OpenStitch is

An independent, cross-platform (macOS, Windows, Linux) embroidery design application written
in Rust. It works like Brother PE-DESIGN 11 and reads and writes the same files, so designs
move between the two and onto Brother machines. It is not affiliated with Brother. It is an
**alpha**: suggest a test sew-out on scrap fabric first.

Features marked "beyond PE-DESIGN" are off unless chosen, so PE-DESIGN-compatible output
stays the default.

## What it can do

- **Drawing:** rectangle, ellipse, polygon, freehand (Pencil), Bézier paths, arcs, preset
  shapes (stars, hearts, arrows, ...), Manual Punch columns.
- **Shape editing:** point editing, move, scale, rotate, flip, group, cut and paste, arranged
  copies (mirror, circle, matrix), merge, remove overlap, boolean combine, split, offset lines.
- **Region sew types:** fill (tatami), satin, programmable fill, motif fill, concentric
  circle, radial, spiral, stippling, net fills, decorative fill, cross stitch; beyond
  PE-DESIGN: contour fill, seeded random stagger, skeleton satin.
- **Line sew types:** running, triple, zigzag (satin line), motif, stem, candlewick, E and V
  stitch, chain, cutting (cutwork needles).
- **Lettering:** TrueType/OpenType text with size, spacing and alignment, satin or fill;
  bundled Lato Bold and Pacifico, or the user's own fonts; Fit Text to Outline, 26 Text
  Transform presets, monograms, templates, Name Drop, own embroidery fonts (Font Creator).
- **Image digitizing:** Auto Punch, Cross Stitch, Photo Stitch 1 and 2, Design Center. Each
  has the PE-DESIGN-compatible mode (default) and, beyond PE-DESIGN, an OpenStitch mode that
  matches colours by appearance. Both are approximations of PE-DESIGN's wizards.
- **Wizards:** appliqué, embroidered patch, cutwork, background fill.
- **Order:** by drag, by colour, PE-DESIGN's optimised order, or a shortest-jump order.
- **Other:** stitch editing, stitches back to shapes, floral patterns, decorative frames,
  stamps, emboss and engrave, hoops and user hoops, multi-position hoops, split designs,
  Design Database (browse, search, batch convert, print catalogues), printing to PDF,
  Programmable Stitch Creator, Stitch Design Factory, Fabric Selector, thread charts.

## Not available

- Exporting CSD or SHV, importing WMF/EMF.
- Wireless transfer, Link, transfer to machine memory or cards, PHX and PEN files, scanners,
  ScanNCut FCM. Send designs to the machine on a **USB stick or SD card**.
- PE-DESIGN's own bundled fonts, motifs and stamps (Brother's content); OpenStitch ships its own.
- Fill text sews each letter in one direction (satin text is sewn stroke by stroke);
  per-character colour is not available yet.

## File formats

| | Formats |
|---|---|
| Read and write | PES, PEC, DST, EXP, JEF, PCS, SEW, XXX, HUS, VIP |
| Read only | SHV, PHC, CSD |
| Editable project | `.osd` (OpenStitch) |
| Import | SVG (as editable shapes), PES 11 shapes (`import.pes`), images PNG, JPEG, BMP |
| Thread charts | CSV in the layout PE-DESIGN's Import Chart reads (`threads.importCsv`, `threads.exportCsv`); GIMP / Inkscape `.gpl` palettes (`threads.importPalette`, `threads.exportPalette`) |

PES is written as version 6 by default; version 1 and PE-DESIGN 11 (`#PES0110`, whose shapes
and text open editable in PE-DESIGN 11) are options. Designs larger than a format allows can
be split into several files.

## Where things are in the editor

The window is laid out like PE-DESIGN 11's Layout & Editing window.

- **File button** (top left): New, Open, Save, Save As, Print, Design Property, Design
  Settings (machine, hoop), Fabric Selector, Select Color Palette, Wizard, Export, Options.
- **Home tab:** Select and Point Edit, Clipboard (cut, copy, paste, duplicate, delete, Arrange
  Copy), Tools (Text, Shapes, Zoom, Pan, Measure), Import (patterns, SVG), Edit (Arrange,
  Rotate/Flip, Size, Group, Fit Text, Stamp/Emboss, Modify Overlap, Decorative Frame), Wizards
  (Background fill, Applique, Embroidered Patch, Cutwork), Sew (Sew Embroidery: USB/SD).
- **Image tab:** background image (open, paste, save, modify, rotate, delete) and Image to
  Stitch: Auto Punch, Cross Stitch, Photo Stitch 1 and 2, Design Center. While Modify Image is
  open the image has handles on the canvas (drag to move, a corner to scale). Where it is drawn:
  `background.setLayer` behind the design (default), on top, or above one object; `ui.state`
  reports it under `background`. It never sews.
- **View tab:** Solid, Stitch and Realistic views, thread trimming, show/hide panes, grid,
  ruler, guidelines, Reference window. Zoom ▾ has Zoom Box (Z: drag a rectangle; `ui.zoom
  {rect}`) and Calibrate Real Size… (stretch a line to 100 mm on a ruler, so 100 % is real
  size on that monitor; `ui.set realSizePointsPerMM`, 0 resets to 96 dpi).
- **Contextual tabs** (orange) appear for the selection or tool: Text, Shapes, Outline, Manual
  Punch, Stitches, Hoop Settings.
- **Panes:** Sewing Order on the left (drag to reorder, sort by colour, optimise); Import,
  Color, Sewing Attributes and Text Attributes on the right; Stitch Simulator below the
  design.
- **Options ▾** (top right): Design Database, Programmable Stitch Creator, Stitch Design
  Factory, Font Creator, Sewing Attribute Sets, User Thread Charts, colour theme.
- **Help ▾:** manual, Ready-to-Sew Designs (the bundled kit), keyboard shortcuts, About.
- Undo and Redo: Quick Access Toolbar, ⌘/Ctrl+Z and ⌘/Ctrl+Y.

## Scripting

Every editor action is an engine command. `stitchtool` runs JSON command scripts,
`openstitch-mcp` is an MCP server for MCP clients, and `openstitch --listen` lets scripts act
on the running editor (127.0.0.1 only, with a per-launch token).

For a user at a terminal, `stitchtool` also has one-line commands built on the same engine
commands: `convert in.dst out.pes` (`stitches.import` + `design.export`; a folder with
`--format pes` is `library.convert`), `info x.pes` (`design.info`), `optimize in out
[--by-color]` (`stitches.divideByColor`, then `order.optimize` or `order.byColor`), `digitize
logo.png out.pes --size 80mm` (`digitize.autoPunch`) and `formats`. When asked which formats
OpenStitch reads or writes, call `formats.list`: every machine format with read and write flags
(PHC, DSB, DSZ, KSM, ZXY and STX are read only), plus the CSV, TXT and G-code exports and .osd.
