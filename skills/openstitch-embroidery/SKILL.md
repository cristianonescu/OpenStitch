---
name: openstitch-embroidery
description: Build and edit machine-embroidery designs in the OpenStitch editor through its command engine. Use before creating shapes, lettering, sew types, wizards or image digitizing, and when answering questions about OpenStitch.
---

# OpenStitch embroidery

You work on the design open in the OpenStitch editor. Every change is an engine command
(`design.new`, `shape.rectangle`, `object.setRegion`, ...) or an editor command (`ui.*`). Find
commands with `commands_list`, read their parameters with `command_describe`, run them with
`execute` or, better, several at once with `run_batch`.

More detail: `reference/recipes.md` (worked batches), `reference/sewability.md` (what sews
well), `reference/answers.md` (what OpenStitch can do, where things are, file formats).

## Units and coordinates

- Lengths are **millimetres**. The Design Page's origin is its **top-left corner**; x grows to
  the right, y grows **down**. A 100 × 100 mm page has its centre at (50, 50).
- **Sewing angles** (`object.setRegion` `angle`) are degrees **counter-clockwise on screen**,
  0 = horizontal rows; 45 = rows rising to the right; 90 = vertical rows.
- **Rotation** of objects (`object.rotate` `degrees`) and of preset shapes (`shape.preset`
  `rotation`) is **clockwise**.
- Densities are rows (or lines) per mm; `pitch` and `stitchLength` are mm.
- Colours: `"#rrggbb"`, a Brother thread name (`"Black"`, `"White"`, `"Red"`, `"Blue"`,
  `"Prussian Blue"`, `"Deep Gold"`, `"Leaf Green"`, ...; case-insensitive), or a Brother chart
  number 1-64. A name that is not in the chart is refused; use `#rrggbb` when unsure.
- `object.get` returns outline points in **0.1 mm** units (x 100 = 10 mm). Commands always take
  mm.

## The Design Page and hoops

- `design.new` `{name, width, height}` starts an empty design (default 100 × 100 mm). It
  replaces the open design (undoable); ask before discarding the user's work.
- `machine.hoops` lists Brother hoop ids (`100x100`, `130x180`, `160x260`, `200x300`, ...) and
  the current settings. `machine.set` `{hoop: "130x180"}` sizes the Design Page to that hoop;
  `{hoop: "custom", width, height}` sets a custom page; `machine: "single" | "multi"`.
- Keep the whole design inside the page: `design.export` reports `fitsPage` and the
  `smallestHoop`.

## Build order

1. `design.new` (and `machine.set` for the hoop), unless the user wants to edit what is open.
2. Shapes and lettering: `shape.rectangle`, `shape.ellipse`, `shape.preset`, `shape.path`,
   `shape.polygon`, `text.add`, `monogram.add`, `import.svg`, or a `digitize.*` wizard.
   Each returns the new object's `id` (`{"id": 3}`); wizards return `ids`.
3. Sew types and colours: `object.setRegion` (fill, satin, ...) and `object.setLine`
   (running, triple, zigzag, ...). A shape made with `fill` gets a fill region; with `outline`,
   a running line. Text is satin by default. A satin border or outline is a line:
   `object.setLine` with `type: zigzag`, about 2-4 mm wide (the badge recipe in
   `reference/recipes.md`).
4. Order: `order.optimize` (PE-DESIGN-compatible order) or `order.byColor`; check with
   `object.list` (sewing order).
5. Check: `view_render` (look at it), `design.info` (size, stitches, colours, trims, time) and
   `design.check` (sewing problems, errors first; `ready` is false while there are errors).
   Run each finding's `fix` command or fix it yourself, and check again; never report a design
   done while `design.check` has errors. `design.route` names every move between objects
   (untrimmed jumps, long moves), `design.density` shows where thread piles up.
6. Save only when asked: `project.save` `{path: "name.osd"}` (editable) and/or
   `design.export` `{path: "name.pes"}` (machine file; the extension picks the format).

Object ids: after `design.new` the first object is id 1 and ids count up in creation order, but
read the ids from each step's result (or `object.list`) instead of guessing. A `run_batch`
cannot pass one step's result into a later step, so create objects in one batch, read the ids,
then style them in the next.

## Editing what is there

- `ui.state` tells you the editor's state, including the **selection**; "this", "it" and
  "the selected text" mean the selected objects. `ui.select` changes the selection.
- `object.list` lists objects (id, kind, bounds in mm, sew types, colours, text);
  `object.get` gives one object in full.
- Move, size, turn: `object.move` `{ids, dx, dy}`, `object.scale` `{ids, sx, sy}`,
  `object.rotate` `{ids, degrees}`, `object.flip`, `object.center`, `object.skew`
  `{ids, angleX, angleY}` (lean; fill directions turn with it).
- Line up and space: `object.align` `{ids, edge: left|right|top|bottom|centerX|centerY,
  to: selection|key|page, keyId}` and `object.distribute` `{ids, axis: x|y, by: gaps|centers}`
  (three or more; the outermost two stay). Groups move whole.
- Repeat a motif: `object.arrangeCopies` `{ids, mode: path, pathId, count, side, orient}`
  along a line, `{mode: scatter, shapeId, count, gap, seed}` inside a closed shape, or
  `{mode: circle, count, radius, mirrorAlternate: true}` for a kaleidoscope.
- Find objects like others: `object.list` `{sameAs: [ids], match: color|type|both,
  visible: true}`, or filter by `color`, `regionType`, `lineType`; then `ui.select` the ids.
- Text: `object.setText` (text, font, size, spacing), `text.setPath` (fit to an outline).
- Delete: `object.delete` `{ids}`. Undo: `history.undo` (only for your own mistakes).

## Editor commands (`ui.*`)

These act on the editor window, not the design file: `ui.state`, `ui.select`, `ui.actions` and
`ui.run` (the ribbon's actions), `ui.zoom`, `ui.settings` and `ui.set` (view settings),
`ui.render` (what `view_render` uses), `ui.simulate` (the stitch simulator), `ui.help`,
`ui.update`, `ui.scratchWrite` (what `scratch_write` uses). Read their parameters with
`command_describe` before the first use.

## Files

- File commands only reach the folders the editor allows (the conversation's scratch folder,
  and folders the user approves). A refused path is not an error to work around: tell the user.
- To import model-made artwork, write it with `scratch_write` (SVG text, or a PNG/JPEG as
  base64) and pass the returned `path` to `import.svg` or a `digitize.*` command.
- `file` commands (`project.save`, `design.export`, ...) may ask the user first. Never
  overwrite a user's file without being asked to.

## Undo

Everything you change in one message becomes **one undo step** for the user ("Assistant: ...").
Do not undo the user's own earlier work; to retry, undo or delete your own objects.

## Rules

- Prefer `run_batch`; a malformed batch changes nothing, and it stops at the first failure.
- After building or changing anything visible, call `view_render` and look before you answer.
- To check one aspect: `preview {layers: "underlay" | "top", colorBy: "order" | "mono", only: [ids],
  solo: block}` (MCP), or in the editor `ui.set` `showLayers`, `colorBy`, `trimMarks`,
  `needlePoints`, `hideUnselected`, `soloColor`. They change only the view, never the design.
- Never claim a result you did not check. Report sizes and stitch counts from `design.info`.
- "PE-DESIGN-compatible mode" is the default behaviour of the wizards; "OpenStitch mode"
  (`compatMode: "openStitch"`) is the optional alternative.
