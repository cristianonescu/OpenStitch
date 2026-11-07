# Recipes

Worked `run_batch` calls with real command ids and parameters. Each block is the `arguments`
of one `run_batch` call. Ids in later batches assume a fresh design (`design.new`), where ids
count from 1; always confirm them from the results or `object.list`. Check every result with
`view_render` and `design.info` before you report it.

## Round badge: fill, satin border, lettering

```json
{"steps": [
  {"command": "design.new", "params": {"name": "Club badge", "width": 100, "height": 100}},
  {"command": "machine.set", "params": {"hoop": "100x100"}},
  {"command": "shape.ellipse", "params": {"cx": 50, "cy": 50, "rx": 40, "fill": "#1a3d7c", "name": "badge"}},
  {"command": "text.add", "params": {"text": "OPEN", "size": 12, "x": 50, "y": 56, "align": "center", "fill": "White"}}
]}
```

The ellipse is id 1, the text id 2. Then the sewing:

```json
{"steps": [
  {"command": "object.setRegion", "params": {"id": 1, "type": "fill", "angle": 45, "density": 4.5, "underlay": true}},
  {"command": "object.setLine", "params": {"id": 1, "type": "zigzag", "width": 3, "density": 4.5, "color": "Deep Gold"}},
  {"command": "order.optimize"},
  {"command": "design.info"}
]}
```

A zigzag line is the satin border; 2-4 mm wide reads well. `text.add` `y` is the baseline, so
`y` = centre + about half the capital height centres the word.

## Lettering on a path (arc of text)

```json
{"steps": [
  {"command": "design.new", "params": {"name": "Arc", "width": 130, "height": 180}},
  {"command": "machine.set", "params": {"hoop": "130x180"}},
  {"command": "shape.ellipse", "params": {"cx": 65, "cy": 60, "rx": 40, "outline": "Black", "name": "guide"}},
  {"command": "text.add", "params": {"text": "OPEN STITCH CLUB", "size": 8, "path": 1, "fill": "Prussian Blue"}}
]}
```

Then adjust and hide the guide's own stitching:

```json
{"steps": [
  {"command": "text.setPath", "params": {"id": 2, "path": 1, "align": "center", "valign": "bottom"}},
  {"command": "object.setLine", "params": {"id": 1, "type": "none"}}
]}
```

`text.setPath` also takes `otherSide: true` (run along the other side, for the bottom of a
circle) and `upright: true`. `path: null` releases the text.

## Monogram

```json
{"steps": [
  {"command": "design.new", "params": {"name": "Monogram", "width": 100, "height": 100}},
  {"command": "monogram.list"},
  {"command": "monogram.add", "params": {"letters": "JMS", "style": "block", "size": 40, "frame": "ring", "margin": 3, "fill": "Prussian Blue", "frameColor": "Deep Gold"}}
]}
```

Styles: `block`, `script`, `diamond`, `circle`. Frames come from `monogram.list` (`ring`,
`double-ring`, `diamond`, `scallop`, `laurel`, `shield`, `stars`, `corners`). The monogram is
one group; change it later with `monogram.edit`. The laurel is two pieces (a stem with its
leaves each side, leaves at least 4 mm) and the stars one piece on a fine band, so each sews in
one path with a trim only between pieces.

## Appliqué

Make the appliqué from a closed shape or text (here a heart, id 1 in a fresh design):

```json
{"steps": [
  {"command": "shape.preset", "params": {"kind": "heart", "cx": 50, "cy": 50, "width": 50}},
  {"command": "wizard.applique", "params": {"ids": [1], "tackDown": "zigzag", "cover": "zigzag", "coverWidth": 3, "output": "replace"}}
]}
```

It sews a material line, stops, a position line, stops, a tack-down, stops, then the covering
stitch, as one group. `output: "add"` keeps the shape and puts the appliqué around it
(`distance` mm away); the shape sews first, under the fabric, as PE-DESIGN does, or with
`patterns: "last"` after the covering stitch, on top of the fabric (use `replace` when the
shape would only be hidden).

## Fewer trims and less bulk

Beyond PE-DESIGN and off unless asked; each is one undo step.

- Many trims between objects of one thread: `design.setTravelUnder {on: true}` sews a move that
  a later fill will cover as hidden running stitches (it reports trims before and after).
  `object.setConnector {ids, connector: run}` joins an object to the next of its thread with a
  visible running connector; tell the user about any in `longConnectors` (15 mm or more).
- Stiff or bulky overlaps: `object.setHideUnder {ids, on: true, overlap: 1}` (or
  `design.setRemoveHidden {on: true}` for every shape) leaves out the stitching later fills and
  satins hide, keeping 1 mm under their edges; the shapes stay whole. It reports
  `stitchesSaved`.

## Floated fabric, appliqué stops and machines without a cutter

All beyond PE-DESIGN and off unless asked; each is one undo step.

- Floated fabric (not hooped): `design.setBasting {on: true}` adds a basting box sewn first in
  the first thread (`shape: outline` follows the objects; `margin` 5, `stitch` 6 mm;
  `thread: own` for its own colour). It always stays inside the hoop; `design.info` reports
  its stitches in the `basting` part of its `production` field.
- Appliqué or a patch placed mid-design: `object.setStopAfter {ids, on: true}`: the machine
  pauses after those objects without a colour change (a colour change right after is the stop).
- No thread cutter: `design.setStopAtJumps {on: true}` adds a stop before every jump so the
  user can snip the thread; off removes only those. PES writes stops as colour changes to the
  same thread (255 blocks at most); PES 0110 (`pesVersion: 11`) cannot hold these settings.

## Line drawings and touching shapes in one go

Beyond PE-DESIGN and off unless asked; one undo step each.

- Running lines that touch or cross (a tree, a redwork drawing): `shape.autoRoute {ids}` turns
  them into one line per connected piece, sewn without trims; `{redwork: true}` sews every
  line twice and ends where it started. Each thread is routed on its own.
- Touching fills or satins (petals and a stem): `object.branch {ids}` sews them in one go in the
  first one's thread, each starting where the one before touches it; it reports `gapsMM` (0
  where they touch). `object.unbranch {ids}` takes it apart.

## Embroidered patch

```json
{"steps": [
  {"command": "design.new", "params": {"name": "Patch", "width": 100, "height": 100}},
  {"command": "text.add", "params": {"text": "CREW", "size": 14, "x": 50, "y": 56, "align": "center", "fill": "White"}},
  {"command": "wizard.patch", "params": {"ids": [1], "width": 3, "distance": 5, "cutting": "running", "color": "Black"}}
]}
```

A satin border `distance` mm round the patterns, then a cutting line `cutDistance` mm outside
it. `wizard.backgroundFill` fills behind the patterns for a fully stitched patch.

## Towels and fleece: knockdown

Beyond PE-DESIGN and off unless asked; one undo step.

- Pile fabric (towel, fleece, velvet): `wizard.knockdown` `{}` for the whole design, or
  `{ids: [...]}` for the selection, puts a light Fill named Knockdown under it, sewn first
  (`reach` 2 mm past the edge, `gapClose` 3 mm, `spacing` 1 mm, `layers` 1 or 2, `angle`,
  `color` defaults to the first covered object's thread, `shape: rectangle`, `keepHoles`).
  After edits, `wizard.knockdown {update: <its id>}` rebuilds it. `order.optimize` keeps it first.

## Image to embroidery

The picture must be a file the editor can read: one the user attached, or one you wrote with
`scratch_write` (`base64` PNG or JPEG). Pass the returned `path`.

| Picture | Command |
|---|---|
| Logo, clip art, flat colours | `digitize.autoPunch` |
| Photo | `digitize.photoStitch` (`style: "fill"` or `"lines"`) |
| Pixel art, cross-stitch look | `digitize.crossStitch` |
| Line drawing | `digitize.designCenter` |

Every wizard defaults to the **PE-DESIGN-compatible mode**. `compatMode: "openStitch"` is
OpenStitch's own mode (Auto Punch, Photo Stitch, Cross Stitch, Design Center): colours chosen
by how they look (CIELAB, CIEDE2000) and, for Auto Punch, specks merged and line art kept. For
some pictures the compatible mode still sews truer: try both and compare with `view_render`.

```json
{"steps": [
  {"command": "design.new", "params": {"name": "Logo", "width": 100, "height": 100}},
  {"command": "digitize.autoPunch", "params": {"path": "/path/from/scratch_write/logo.png", "width": 60, "compatMode": "openStitch"}}
]}
```

Useful options: Auto Punch `colors` (0 = automatic), `skipBackground`; Photo Stitch
`colors`, `mode: "mono"` with `thread`; Cross Stitch `cellSize` (mm); Design Center
`lineSew`, `regionSew`, `joinLines` (on by default: running and triple lines that meet are sewn
as one path, walking back over lines already sewn instead of a trim after every piece; `false`
keeps one piece per line). In OpenStitch mode `threads: "all"` chooses from every built-in chart.
Each returns the new `ids`; Auto Punch, Photo Stitch and Cross Stitch also list the `colors` used.

Auto Punch also returns a `plan`: every traced region with its `region` number, `color`,
`widthMm`, the `type` it got (`fill`, `satin`, `line`, `skip`) and why (`reason` code and a
`why` sentence, such as "0.6 mm wide ... running line"). To correct a part, run it with
`dryRun: true` first (nothing is added), read the plan, then convert with `overrides`:

```json
{"command": "digitize.autoPunch", "params": {"path": "/path/from/scratch_write/logo.png", "width": 60, "compatMode": "openStitch", "overrides": [{"region": 3, "type": "satin"}, {"region": 7, "type": "skip"}, {"region": 2, "angle": 30}]}}
```

Regions are numbered by the run's own options, so keep every other option the same between the
dry run and the conversion. Thin lines and outlines can only be skipped. In OpenStitch mode
`angleRule: "varyNeighbours"` turns a fill of 20 mm² or more away from a touching fill as
large that runs within 20° of it, so fitted panels read as separate pieces, and `angleRule: "leastTravel"` lays each fill of 10 mm² or more the way
(every 15°) that sews the least thread, so the least travel between its sections; parts under
1 mm wide (ropes, whiskers) are sewn as running lines, a region whose stitches would all fall
within 0.8 mm is left out as a speck (`reason: "speck"`; an override can still sew it), and a
white, cream or off-white paper background is left out as the fabric showing. On line art the
same small regions are drawn dots, so they are kept: `reason: "dot"`, sewn as a small fill
tied only where a trim cuts it. Line art's small pieces in one thread that lie within 2 mm of
each other are joined by a short running connector instead of a trim (fewer trims, no knots).

`order: ["#rrggbb", ...]` sews those colours first (the plan's `color` values; the others
follow brightest first). Region numbers follow the sewing order, so dry-run again after
changing it before you send `overrides`; colours a run does not sew come back in
`orderIgnored`. `regionMap: true` adds the region under each cell of a small grid
(`{width, height, runs: [[region, count], ...]}`, -1 for none), to name the region under a
point the user means ("the hat" at the top left).

The image dialog (Design ▸ Image to Embroidery, Auto Punch) does the same with buttons:
Preview regions (a dry run), a Regions table where each row's Stitch can be changed, Fix on
the picture (click a part to leave it out or put it back), a Colour order list and a Preset
row; Convert then sends the choices as `overrides`, `order` and `preset`.

A purpose `preset` sets several Auto Punch options at once, only those the call leaves unset,
and implies OpenStitch mode: `logo`, `patch` (background sewn too), `lineArt`, `illustration`,
`photo`, `pixelArt`. `digitize.presets` lists each one's options. Name the preset from what the
user says the picture is for ("a patch for my jacket" → `patch`).

`digitize.exportOutlines` takes the same parameters plus `out` (an `.svg` file) and saves the
traced regions as SVG paths in their thread colours, in mm, without changing the design; give it
to another program, or bring it back with `import.svg` (one object per region). It writes a file,
so the editor asks the user first.

`underlap` (OpenStitch mode, mm, 0.8 is typical) makes each fill reach under the touching
colour sewn after it, so no fabric shows along seams; it adds stitches, so leave it off for
thin lettering.

Helpers that need no model and send nothing anywhere:
- **Cut out the subject with one click.** `digitize.segment` `{path, point: [x, y]}` (fractions
  of the picture: `[0.5, 0.5]` is its centre) returns the subject's `box` and `share`, and with
  `out` writes the mask as a PNG. Convert with Auto Punch's `isolateSubject: true,
  subjectPoint: [x, y]`: the background is learnt from the border and only the part holding
  the click is sewn. Add `subjectStrokes` / `backgroundStrokes` when it takes too much or too
  little.
- **Line drawing from a photo.** `lines: "edges"` on `digitize.lineImage`, `digitize.figure`
  or `digitize.designCenter` traces the photo's edges instead of its dark pixels
  (`edgeStrength` 1-100, default 20: lower keeps fainter edges). Sew it with
  `digitize.designCenter` `{lines: "edges", regionSew: "none"}` for redwork.

## SVG artwork

Write the SVG, then import it as editable shapes (fills become fill regions, strokes running
lines, colours snapped to the Brother chart unless `exactColors: true`):

1. `scratch_write` `{"name": "star.svg", "text": "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"40mm\" height=\"40mm\" viewBox=\"0 0 40 40\"><path d=\"M20 2 L38 38 L2 38 Z\" fill=\"#d81e05\"/></svg>"}` returns `{"path": ...}`.
2. Then:

```json
{"steps": [
  {"command": "import.svg", "params": {"path": "<the returned path>", "width": 40}},
  {"command": "object.list"}
]}
```

Give the SVG real sizes (`width="40mm"`) or pass `width`. Keep paths closed for fills, and give
the imported shapes sew types with `object.setRegion` / `object.setLine` as for any shape.

## The user's own threads from a palette

A GIMP or Inkscape palette (`.gpl`) becomes a user thread chart; a colour named "1800 Ruby" gets
thread code 1800. Then change the design to those threads:

```json
{"steps": [
  {"command": "threads.importPalette", "params": {"path": "spools.gpl"}},
  {"command": "threads.remap", "params": {"chart": "<the returned chart>"}}
]}
```

`threads.importPalette` refuses a chart name already in use (pass `name`) and a file with bad
lines (it names them; `partial: true` imports the rest). `threads.exportPalette`
`{chart | builtin, path}` writes a chart as a palette.

## A shape with a curve

`shape.path` takes vertices and one optional cubic Bézier per segment
(`[[c1x, c1y], [c2x, c2y]]` or `null`; the last segment closes the path). A leaf:

```json
{"steps": [
  {"command": "shape.path", "params": {"points": [[20, 50], [80, 50]], "closed": true, "curves": [[[35, 20], [65, 20]], [[65, 80], [35, 80]]], "fill": "Leaf Green"}},
  {"command": "object.setLine", "params": {"id": 1, "type": "running", "pitch": 2.5, "color": "Deep Green"}}
]}
```

## Save and export (only when the user asks)

```json
{"steps": [
  {"command": "project.save", "params": {"path": "badge.osd"}},
  {"command": "design.export", "params": {"path": "badge.pes"}},
  {"command": "design.export", "params": {"path": "badge.dst"}}
]}
```

The extension picks the format (pes, pec, dst, exp, jef, pcs, sew, xxx, hus, vip). PES is
version 6 by default (`pesVersion`: `1`, `1t`, `6`, `6t`, `11`), except for a design opened from a
PE-DESIGN 11 (`#PES0110`) file, which is written back as `11` unless another version is asked
for (what version 11 cannot hold yet is then written as 6, with a warning). The result reports
`fitsPage` and the `smallestHoop`. `.osd` keeps everything editable in OpenStitch.
