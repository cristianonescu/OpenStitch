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
well), `reference/answers.md` (what OpenStitch can do, where things are, file formats),
`reference/composition.md` (planning an illustrated design) and `reference/examples.md`
(complete example designs: strawberry, mushroom, watermelon, cat face, ...).

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
  Other brands' hoops are in `groups` (`machine.hoops {brand: "Janome"}` lists one brand; ids
  such as `janome-re20b`, `bernina-large-oval`, `round-15cm`). Oval and round hoops sew only
  their ellipse, which `fitsPage` checks; round hoops cannot rotate. Brother is the default.
- Keep the whole design inside the page: `design.export` reports `fitsPage` and the
  `smallestHoop`.
- Larger than the hoop: on a custom page, `sections.set {hoop}` splits it (PE-DESIGN's grid);
  `sections.autoLayout` switches to free hoopings, where each object stays whole in one hooping
  with alignment marks and a stop between them. `sections.list` names objects that fit no
  hooping; `sections.export {path}` writes "Name - hooping k of n" files.
  Free hoopings by hand: `sections.set {mode: "free", hoopings: [{x, y, rotated}], marks:
  "boxX" | "cross" | "none", minOverlap, stopAfterMarks}` (x, y: the hooping's top-left corner
  on the page in mm, in sewing order; `rotated` turns the hoop 90°; consecutive hoopings must
  overlap by `minOverlap`, 16 mm by default, for the marks). Example: `sections.list` says
  "object 7 (60.0 × 40.0 mm) fits no hooping" → read the hoopings' `area`s, then send the
  whole list again with one hooping moved (or one added) so that its area holds the object's
  bounds, and check `sections.list` until `errors` is empty. `{mode: "grid"}` returns to
  PE-DESIGN's sections.

## Build order

0. For an illustrated subject ("a strawberry with leaves", "a cartoon cat"), open the closest
   example first (`skill_reference {"file": "strawberry"}`; `reference/examples.md` lists
   them), plan the regions as `reference/composition.md` says, and adapt the example.
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
   `object.list` (sewing order). To sew objects in a set order, `order.byClickOrder`
   `{ids: [in sewing order]}` (they start where the earliest of them sews now); to sew a
   shape's outline before its fill, `object.setPartOrder` `{ids, order: lineFirst}` (beyond
   PE-DESIGN; `regionFirst` is PE-DESIGN's order). `order.optimize` returns `before`/`after`
   (colour changes, jumps, trims, travel mm) and a `summary` line: tell the user, and offer
   `history.undo` when a number got worse. `design.setProperties {autoStart: nearest}` (beyond
   PE-DESIGN) starts each shape without its own entry point nearest to where the last ended.
5. Check: `view_render` (look at it), `design.info` (size, stitches, colours, trims, time) and
   `design.check` (sewing problems, errors first; `ready` is false while there are errors).
   Run each finding's `fix` command or fix it yourself, and check again; never report a design
   done while `design.check` has errors. `design.route` names every move between objects
   (untrimmed jumps, long moves), `design.density` shows where thread piles up.
   `design.timeline` lists the colour blocks in sewing order with their stitch ranges, trims,
   object starts and machine time ("when does the red start?"); in the editor,
   `ui.simulate {stitch}` or `{step: "block" | "object" | "trim"}` moves the player there.
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
- Move, size, turn: `object.move` `{ids, dx, dy}` (the stitches move with it, unchanged),
  `object.scale` `{ids, sx, sy}`,
  `object.rotate` `{ids, degrees}`, `object.flip`, `object.center`, `object.skew`
  `{ids, angleX, angleY}` (lean; fill directions turn with it).
- Line up and space: `object.align` `{ids, edge: left|right|top|bottom|centerX|centerY,
  to: selection|key|page, keyId}` and `object.distribute` `{ids, axis: x|y, by: gaps|centers}`
  (three or more; the outermost two stay). Groups move whole.
- Repeat a motif: `object.arrangeCopies` `{ids, mode: path, pathId, count, side, orient}`
  along a line, `{mode: scatter, shapeId, count, gap, seed}` inside a closed shape, or
  `{mode: circle, count, radius, mirrorAlternate: true}` for a kaleidoscope.
- Continue in place: `object.extendPath` `{id, end: start|end, points, curves?, widths?}` adds
  to an open line, path or satin skeleton, `object.extendColumn` `{id, end, lines}` to a Manual
  Punch column (points added at the start are listed outward from it).
- Clean up: `shape.simplify` `{ids, tolerance: 0.2}` removes nodes within a tolerance (mm; a
  Manual Punch column loses one direction line per call; `ui.run smoothShape` uses the
  `smoothTolerance` setting), and `stitches.smooth`
  `{id, from, to, iterations, strength}` evens out a stitch pattern's needle points.
- Cut: `shape.knife` `{line: [[x, y], ...]}` splits every object an open line crosses (fills
  into both sides, satin columns across, lines where crossed; `ids` limits it, `skipped` says
  what was not cut through); `{at: [x, y]}` splits the line under a point.
- More shapes: `shape.preset` `{kind: frame, frameOf, bandWidth}` (a band around an open
  middle) or `{kind: spiral, turns, innerRadius, direction}` (an open line);
  `shape.rectangle` `cornerRadii: [tl, tr, br, bl]`; `shape.combine` `op: divide` (every
  overlap its own shape); `shape.effect` `{ids, kind: warp|twist|roughen|puckerBloat|zigzag,
  amount, size, seed}`.
- Shape builder and gaps: `shape.build` `{ids, path: [[x, y], ...], mode: merge|remove}`
  merges (or removes) the overlap pieces a path crosses; `object.setLineGap` `{ids, offset,
  length}` leaves a turning gap in a closed line (`length: null` removes it).
- Find objects like others: `object.list` `{sameAs: [ids], match: color|type|both,
  visible: true}`, or filter by `color`, `regionType`, `lineType`; then `ui.select` the ids.
- Text: `object.setText` (text, font, size, spacing), `text.setPath` (fit to an outline).
- Fonts: `fonts.list` gives each bundled font's `sizeRange` (mm), `stitchTypes`, `kind` and
  `group`; pick a font whose range holds the size. `text.add` returns `sizeWarning` and
  `object.setText` returns `sizeWarnings` (`[{id, warning}]`) when a size lies outside its
  font's range: act on them (a font whose `sizeRange` fits, or a size inside the range) and
  tell the user what you changed. Labels of 3-6 mm: `font: "openstitch-line"` (Small
  Text, a running line along each letter's centre; `object.setLine` `runTimes: 3` for bean
  stitch, or `type: zigzag, width: 1.2` for a narrow satin). Retro or sampler text:
  `font: "openstitch-pixel"` with `pixelStyle` run, cross or block (one pixel = size / 7).
- Ink/Stitch fonts the user installed: their folder is the `inkStitchFolder` setting
  (`ui.settings`; `ui.set` changes it after the user agrees, as the editor asks);
  `fonts.scanFolder {path}` lists the font folders,
  `fonts.importInkStitch {path}` loads one (tell the user its licence and what `notRead`
  lists); then `text.add {font: <id>}`.
- Bent lettering beyond the 26 Text Transform presets: `text.setEnvelope {ids, shape:
  bridge|pennant|perspective|diamond|preset}`, then `text.bendEnvelope {id, edge: top|bottom,
  handle: left|middle|right, dx, dy, symmetric}` (mm on the page; `text.envelopeHandles`
  says where the handles are). PES 0110 export refuses free envelopes: `text.toShapes` first.
- Threads: to change the design to a thread chart ("use my Madeira threads"), run
  `threads.remap` `{builtin | chart, metric: ciede2000, keepDistinct: true, dryRun: true}`
  first and show the user the plan (design colour → thread, ΔE, objects, and `merged`), then
  run it without `dryRun` if they agree (`metric: rgb` is PE-DESIGN's rule). The thread
  makers' charts (Madeira, Isacord, Sulky, Robison-Anton) are `builtin` ids: `threads.builtinCharts`
  lists them with `kind: "brand"`. `threads.find`
  `{query}` finds a thread by code or name. After `import.svg` or a `digitize.*` wizard gives
  many near-identical colours, offer `threads.mergeSimilar` (`dryRun` first; it reports the
  colour changes saved, and `order.byColor` when `suggestSortByColor`).
- The user's own threads: when they say "use my threads", have a thread chart (`threads.charts`
  lists theirs) or name a brand, take **every** colour from that chart: pick entries for new
  objects, or `threads.remap {chart | builtin}` the design. Never quote a catalogue number
  that is not in a chart: look it up with `threads.find {query}` first, and if it is not
  there, give the colour as `#rrggbb` and say no thread matches. From a photo or list of their
  spools, propose a chart (colour, and the code only when the label is legible) and create it
  with `threads.createChart` and `threads.addColor` only after the user confirms; the editor
  asks them too.
- Names and locks: `object.setName` `{ids, name}` names objects in the Sewing Order list
  ("" clears it); `object.setLocked` `{ids, locked}` stops the user moving or deleting them on
  the canvas by accident (your commands still work on them; `object.list` shows `locked`).
- Copy between designs: `object.get` the objects, then `object.paste` `{objects, dx, dy,
  fonts}` in the other design; give `fonts` (the source design's `fonts` entries) when a text
  uses a font loaded into the source design. You never read or write the system clipboard:
  `ui.run copy` / `paste` use the window's own copy.
- Delete: `object.delete` `{ids}`. Undo: `history.undo` (only for your own mistakes).

## Editor commands (`ui.*`)

These act on the editor window, not the design file: `ui.state`, `ui.select`, `ui.actions` and
`ui.run` (the ribbon's actions), `ui.zoom`, `ui.settings` and `ui.set` (view settings),
`ui.render` (what `view_render` uses), `ui.simulate` (the stitch simulator), `ui.help`,
`ui.update`, `ui.scratchWrite` (what `scratch_write` uses), `ui.notesRead`, `ui.notesWrite`
and `ui.notesClear` (what `notes_read` and `notes_write` use; see Memory). To find a command
by words ("where is arrange copy?"), `ui.searchCommands {query}` gives the same list as the
user's Ctrl+K palette, with each command's ribbon place and shortcut; run it with `ui.run`.
After a crash, `ui.recoveryList` shows designs autosave kept; `ui.recoveryRestore {id}`
opens one as an untitled design (tell the user to save it), and `ui.recoveryDiscard`
deletes snapshots (ask first). Autosave is `ui.set autosave` / `autosaveInterval`.
For a new user, `ui.gettingStarted` lists the start cards and the kit's samples; `{sample:
"sampler"}` opens one as an untitled design, `{card: "draw"}` runs a card, `{page: true}` opens
Help ▸ Getting Started. "Make new shapes start at 30° and 4.5 rows/mm" is `ui.set
newObjectDefaults {angle: 30, density: 4.5}` (also stitchLength, pullCompensation, pitch,
underlay; `{}` back to the tools' own); it changes only shapes drawn afterwards. Read their parameters with
`command_describe` before the first use. `ui.zoom {rect: [x, y, w, h]}` zooms to a rectangle.
`background.setLayer {position: "top" | "bottom" | "afterObject", afterId}` puts the tracing
picture above the design or between objects ("put the picture above the fills"); it never sews.

Units: every command takes millimetres. The user may work in inches (`ui.settings` → `units`,
also named in the editor note with each message); then answer in inches (1 in = 25.4 mm) and
still send mm to commands. `design.print {units: "inch"}` adds a 2-inch scale bar.

## Files

- File commands only reach the folders the editor allows (the conversation's scratch folder,
  and folders the user approves). A refused path is not an error to work around: tell the user.
- To import model-made artwork, write it with `scratch_write` (SVG text, or a PNG/JPEG as
  base64) and pass the returned `path` to `import.svg` or a `digitize.*` command. A bare file
  name you just wrote (`heart.svg`) also means the scratch folder's file, but the full path is
  clearer.
- `import.svg` returns `skipped` (text, images, nearly transparent or clipped-away shapes) and
  `approximated` (gradients, patterns, masks, filters): tell the user what was left out instead
  of leaving it unsaid. `gradients: "bands"` turns a gradient fill into 2–8 colour bands;
  `minOpacity` (default 0.1) sets how faint a fill or stroke may be before it is left out.
- When you write an SVG for `import.svg`, you can name each element's sewing and import with
  `settings: true`: give the element an `id` and a `data-name` such as `"fill a=45 s=0.4"`,
  `"satin"`, `"run r=3"` or `"stroke.satin w=3"`; the result lists unknown words and refused
  values (FILE_FORMATS.md has the words).
- A user who wants to script designs in Python can use the `openstitch` package (repository
  `python/README.md`: `Session().exec("shape.ellipse", ...)` with these same commands).
- `file` commands (`project.save`, `design.export`, ...) may ask the user first. Never
  overwrite a user's file without being asked to.

## Memory

You keep short notes about the user between conversations: what you remember is at the end of
your instructions, and `notes_read` shows it again. Use it without asking again (their
machine's hoops, the threads they own, their fabrics, their preferences).

- Write with `notes_write {section, text, mode}` when the user states a **lasting** fact
  ("I sew on an NV880E", "I only have Madeira Polyneon", "I never want fills over 80 mm") or
  confirms that a correction **worked** ("denser satin fixed the gaps on knits": Lessons).
  Not for one-off requests ("make this one blue") or guesses of yours.
- Sections: Machine, Threads, Fabric, Preferences, Lessons. `append` adds one short line; one
  fact per line, with the numbers that matter. `replace` rewrites a section, for example to
  correct a fact the user says is wrong.
- The user is asked before every write and may decline: then do not insist. If memory is off,
  the tools say so: carry on without it.
- The notes hold at most 8 KB. When a write is refused as too long, summarise: `notes_read`,
  then `notes_write` with `mode: "replace"` and a shorter version of a section (Lessons first),
  then write the new fact.
- Never store passwords, keys, tokens, addresses or other personal data the user did not give
  you for this purpose; the notes refuse things that look like keys.

## Undo

Everything you change in one message becomes **one undo step** for the user ("Assistant: ...").
Do not undo the user's own earlier work; to retry, undo or delete your own objects.

## Rules

- Prefer `run_batch`. Every step's parameters are checked before anything runs: a batch with
  mistakes changes nothing and returns **all** of them at once in `errors` (`step`, `command`,
  `param`, `message`); fix them all before resending. `validateOnly: true` checks a batch
  without running it. Once it runs, it stops at the first failure unless `continueOnError`.
- After building or changing anything visible, call `view_render` and look before you answer.
- To check one aspect: `view_render` (or MCP `preview`) takes `layers: "underlay" | "top"`,
  `colorBy: "order" | "mono"`, `only: [ids]`, `solo: block`, `needlePoints`, `trimMarks`; in
  the editor `ui.set` `showLayers`, `colorBy`, `trimMarks`, `needlePoints`, `hideUnselected`,
  `soloColor` set View ▸ Show, which `view_render` then follows. They change only the view,
  never the design.
- Never claim a result you did not check. Report sizes and stitch counts from `design.info`.
- "PE-DESIGN-compatible mode" is the default behaviour of the wizards; "OpenStitch mode"
  (`compatMode: "openStitch"`) is the optional alternative.
