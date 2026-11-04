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
one group; change it later with `monogram.edit`.

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
(`distance` mm away).

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
`lineSew`, `regionSew`. In OpenStitch mode `threads: "all"` chooses from every built-in chart.
Each returns the new `ids`; Auto Punch, Photo Stitch and Cross Stitch also list the `colors` used.

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
version 6 by default (`pesVersion`: `1`, `1t`, `6`, `6t`, `11`). The result reports
`fitsPage` and the `smallestHoop`. `.osd` keeps everything editable in OpenStitch.
