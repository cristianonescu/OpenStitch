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
  0 = horizontal rows; 45 = rows rising to the right; 90 = vertical rows. Cross stitch
  (`type: crossStitch`) ignores `angle`: its grid stays on the page axes, as in PE-DESIGN. Set
  its `spacing` (cell size) and `times` (`single`, `double`, `triple`); `crossKind` `half`,
  `upright` or `smyrna` is beyond PE-DESIGN (default `full`).
- **Rotation** of objects (`object.rotate` `degrees`) and of preset shapes (`shape.preset`
  `rotation`) is **clockwise**.
- Densities are rows (or lines) per mm; `pitch` and `stitchLength` are mm.
- `object.setRegion` `angle` (degrees, counter-clockwise) is a straight sewing direction: it turns
  `autoDirection` off unless you pass `autoDirection` in the same call.
- Maze fill (beyond PE-DESIGN): `object.setRegion` `type` `maze` with `spacing` (mm),
  `straightness` (%, default 70) and `mazeSeed` (another seed, another maze). PES 0110 refuses it;
  save PES version 6.
- Meander fill (beyond PE-DESIGN): `object.setRegion` `type` `meander` with `pattern` (a meander
  tile from `patterns.list`, default `vermicelli`), `patternWidth`/`patternHeight` (mm) and `runPitch`.
  PES 0110 refuses it; save PES version 6. Your own tile: `pattern.define` with a pattern
  document (format "openstitch.pattern") of `kind` `meander`, one path from `[0, y]` to `[1, y]` inside the
  unit tile (at most 500 points); a tile that would cross itself, its neighbours or the rows above
  and below is refused.
- Colour blends and gradients (beyond PE-DESIGN), for skies, sunsets and petals: `shape.blend`
  `{id, thread2, mix: [a, b]}` fades a fill into a second thread; `shape.gradient` `{id, threads:
  [...] (2-8), stops, kind: linear|radial, angle or from/to (mm), dither: rows|ordered|noise|bands,
  curve}` goes through several. They return the new objects' `ids` (one per colour, grouped).
  Change `gradientDither` / `gradientCurve` with `object.setRegion` on all of those ids together.
  Move where it starts and ends with `gradientFrom` / `gradientTo` ([x, y] mm) on all the ids
  (the editor's Gradient tool does this by dragging).
  PES 0110 refuses them; save PES version 6. Each colour trims where its travel would show
  across the others: expect some trims per blend (a dithered blend more than bands); tell the
  user when their machine has no thread cutter.
- Gradation shapes (beyond PE-DESIGN): with `gradationPattern` on, `object.setRegion`
  `gradationShape` `convex` | `concave` | `centre` | `edges` | `wave` (`linear` is PE-DESIGN's).
  PES 0110 refuses the others. `import.svg` with `gradients: "blend"` brings SVG gradients in as
  gradient fills (one fill per stop, grouped).
- Tartan (plaid) fill (beyond PE-DESIGN): `shape.tartan {id, sett: "R8 G24 W4"}` (letter codes R red,
  G green, B blue, K black, W white, Y yellow, N grey, O orange, P purple, A sky blue, T brown, DB dark
  blue; widths in mm) or `sett: [{color, width}, ...]`; `symmetric` (default true). It returns one object
  id per colour (grouped). `object.setRegion tartan: false` makes one plain again. PES 0110 refuses it.
  Each colour trims where its travel would show across the other stripes: expect a few trims per
  tartan; tell the user when their machine has no thread cutter.
- Stitch angle lines (beyond PE-DESIGN), for leaves, ribbons, U and Z shapes and rings:
  `object.setAngleLines {id, lines: [[[x, y], [x, y]], ...]}` (mm, 2 or more lines across the fill
  in order along it; each line is extended to the outline). The rows turn from one line's
  direction to the next. Lines must not cross inside the shape. `lines: []` removes them. PES 0110
  refuses them; save PES version 6.
- A plain fill's look changes with `object.setRegion` `texture` (`smooth`, PE-DESIGN's default,
  or OpenStitch's `ribs`, `diagonal`, `blocks`, `waves`, `zigzag`, `chevron`, `scallops`, ...):
  same shape, density and stitch count, different needle-hole pattern.
- Wave fill (beyond PE-DESIGN): `object.setRegion` on a fill with `waveHeight` (crest to trough,
  mm, 0 = off) and `waveLength` (mm, default 16). PES 0110 refuses it; save PES version 6.
- Travel hidden under later parts (beyond PE-DESIGN): `object.setRegion` on a fill with
  `runningStitchPath: "insideHidden"` sews its parts in the order that leaves the least travel
  on thread already sewn (a stem between leaves after them; a ring gone round, branches
  first), never with more trims than `inside`. Auto
  Punch sets it on its fills in OpenStitch mode. PES 0110 saves it as `inside` (reading the
  file back gives `inside`).
- Flow fill (beyond PE-DESIGN): `object.setRegion` on a fill with `flowField`: `"radial"` (rows
  radiate from the shape's centre, a sunburst), `"swirl"` (rows circle it), `{kind: radial |
  swirl, center: [x, y]}` for another centre, or `{kind: grid, origin, cell, cols, angles}`;
  `"straight"` turns it off. The rows never cross. PES 0110 refuses it, save PES version 6.
  From a picture: `digitize.autoPunch {compatMode: "openStitch", flowFill: true}` gives each
  fill a grid field that follows the picture's own strokes (fur, petals, hair); PE-DESIGN mode
  refuses it. OpenStitch mode sews a long branching satin stroke (a Y, K or forked line) as one
  column per arm instead of one satin across it (`splitBranches`, on by default; `false` keeps
  one satin).
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
   A zigzag line's corners sharper than 67.5 degrees (PE-DESIGN sews 75 degrees and wider as
   plain corners) follow PE-DESIGN's `sharpCorners` (1: legs to where they cross, the point
   filled as a wedge; 2: split along the bisector, more
   short stitches) and `miterLimit` (0-50 mm from the corner along the bisector; 0, the
   default, cuts the point at the corner, about 4 mm on a 2 mm line at 30 degrees is the full
   point). Raise `miterLimit` for a pointed star or chevron border, keep 0 for a blunt one.
   A satin line (`type: satin`) with one width, no satin shaping and straight segments takes
   the same `sharpCorners` and `miterLimit` (#373); a `cornerStyle` takes precedence, and a
   satin line with width keys, tapers or slant keeps its own corners.
   Satin quality beyond PE-DESIGN (off by default; Manual Punch columns, skeleton satins,
   Auto satin regions and satin text via `object.setRegion`, zigzag lines via
   `object.setLine`; on text a corner style shapes the corners where two strokes are sewn one
   after the other, as Z's, V's and W's, while stems and junctions stay as they are):
   `cornerStyle: auto` for stars, chevrons and badge borders (mitre below `mitreBelow` 75°, cap below `capBelow` 45°,
   `cornerOverlap`, `capMaxLength`); `shortStitchDistance: 0.25` (with `shortStitchInset` %)
   thins the inside of tight curves; `pullLeft`/`pullRight` (mm, `null` resets) and
   `pullPercent` (3 for tapered columns; `pullMax` caps it) compensate per side and by width;
   `underlayStyle: auto` picks a centre walk, edge run or edge + zigzag by width (thresholds
   `underlayAuto {centreBelow, edgeBelow}`, set by `fabric.apply`; on a skeleton or Auto
   satin it sews rows along the column that follow its width instead), `centreWalk` for thin
   satins, and `underlay2 {style, inset, spacing, stitchLength, angle}` adds a second
   layer (`null` removes it; on a fill or fixed-direction satin its rows run at `angle` degrees from
   the fill angle, default 0 = crossed tatami). A fill's `pushCompensation` (0-2 mm) pulls in its
   sides along the rows. Satin looks: `satinStyle` sStitch (corded edge), open (light),
   double (raised X lattice); `jaggedEdge {mode: in|out|both, side, amount, seed}` (or just
   `"in"`; `"none"`/`null` removes it) for fur, grass and feathers; `spacingAt: 0.5` evens a
   curved column's density; `raised: true` (or `{laneSpacing, inset, stitchLength}`) pads
   monograms and rims of 3 mm and wider (narrower ones return a `hints` entry). The looks work
   on every satin: Manual Punch columns, skeleton and Auto satin, fixed-direction satin
   regions, satin text and zigzag lines (`spacingAt` on columns, skeleton and Auto satin).
   PE-DESIGN's feathered edge (`featherSide`, `featherWidth`) also takes `featherMode: out`
   (row ends past the edge) or `both`, and `featherSeed` for a pattern of its own. Read them back
   with `object.get`. Fix a Manual Punch column instead of redrawing it:
   `object.insertRung {id, at}`, `object.deleteRung {id, index}`, `object.moveRung {id, index,
   side: left|right, point}`, `object.swapSides {id}` and `object.untangle {id}`; a line pair
   clicked right-then-left is untangled when the column is made.
   Brush strokes, leaves and calligraphic lines: `object.setLine {type: satin, width}` on any
   path, then `widthKeys: [[at 0-1, left mm, right mm], ...]` (or `object.setWidthKey {id, at,
   width | left, right}`, `object.deleteWidthKey {id, index}`), `taperStart`/`taperEnd` (mm,
   pointed ends) and `slant` (-60 to 60 degrees). A Y, star or stick figure drawn as one
   filled shape: `object.deriveSkeleton {id, branches: true, joinOverlap: 0.3}` makes a
   grouped satin column per arm (the shape stays hidden); a letter's straight stem (K, R)
   stays one straight column through its junction. Every column is sewn out to the shape's
   edges and never past them, and a column that ends on the edge also into that end's corners;
   each column's region shows `outlineEnds` ([first end, last end] on the edge, e.g. `[true,
   true]` for a stem, `[false, true]` for an arm from a junction to the edge), set by the
   command (`object.setRegion` can't set it). Check `warnings` for columns over 10 mm
   and look at the junctions with `view_render`.
   Ripples, echoes and spirals from one outline (beyond PE-DESIGN): `object.setLine {type:
   ripple, strands, target: [x, y]}` (a closed line spirals in to the target as one run; an
   open one repeats `spacing` apart towards the target's side). Motif lines (beyond
   PE-DESIGN): `motifGroups: [[3, 6]]` (three motifs, then a 6 mm gap), `freeStart`/`freeEnd`
   (clean corners), `motifs: ["arrow"]` (used in turn); bundled motifs now include
   square-spiral, star, cross-x, zigzag and arrow (`patterns.list`). A motif band (beyond
   PE-DESIGN): `motifBand: "star"` on a skeleton satin or Manual Punch satin
   (`object.setRegion`) or a satin line (`object.setLine`) sews the motif along the column
   instead of the satin, each copy stretched to the local width, so it follows a taper;
   `bandTile` (mm per copy), `bandMirror`; `motifBand: "none"` back to satin.
   Borders, quilting and crazy-quilt seams (beyond PE-DESIGN): `object.setLine {type: chevron |
   cretan | feather | overlock | alternatingTriangles | doubleZigzag | lattice | stripes, width,
   spacing}` (`height` sets the chevron bar or Cretan/feather side stitch, `strands` a lattice's
   or the stripes' count, `gaps: [1, 2, 1]` pinstripes); overlock edges a patch or appliqué,
   feather and Cretan suit crazy quilts, a lattice or double zigzag reads as a braid or lace;
   an E stitch's `prongAngle: 45` slants its prongs.
   Hand-made lines (beyond PE-DESIGN): `object.setLine {type: backStitch}` for redwork and
   quilt outlines (two stitches forward, one back); `{type: scribble, width, runTimes,
   noiseScale, seed}` for sketchy doodles; on a running line `profile: arc` (or `linear`,
   `sine`, `smoothstep`, `stairs`, `profilePoints`) with `minPasses`/`maxPasses` (odd) and
   `passSpacing` swells it like a brush stroke ("calligraphic", "brush" lines); `hand: true`
   (`handLength`, `handAngle`, `handPasses`, `handExtra`) on running or triple lines, or
   `object.setRegion {handSide: a|b|both, handAmount}` on a satin, gives the uneven
   hand-sewn look for "folk", "naive" or "hand-drawn" requests. The same `seed` always sews
   the same stitches; `shuffle: true` tries another variant (one undo step).
4. Order: `order.optimize` (PE-DESIGN-compatible order) or `order.byColor`; check with
   `object.list` (sewing order). To sew objects in a set order, `order.byClickOrder`
   `{ids: [in sewing order]}` (they start where the earliest of them sews now); to sew a
   shape's outline before its fill, `object.setPartOrder` `{ids, order: lineFirst}` (beyond
   PE-DESIGN; `regionFirst` is PE-DESIGN's order). `order.optimize` returns `before`/`after`
   (colour changes, jumps, trims, travel mm) and a `summary` line: tell the user, and offer
   `history.undo` when a number got worse. `design.setProperties {autoStart: nearest}` (beyond
   PE-DESIGN) starts each shape without its own entry point nearest to where the last ended,
   ends a region without its own exit nearest the next object, and enters a satin part way
   along, where it passes nearest (a skeleton satin or satin line by a lead run under it); from a
   shell, `stitchtool optimize in.osd out.pes --auto-start nearest` sets it before reordering. Set a point by hand with
   `object.setEntryExit` (`regionEntry`, `regionExit`, `lineEntry`, `lineExit`; null clears).
5. Check: `view_render` (look at it), `design.info` (size, stitches, colours, trims, time) and
   `design.check` (sewing problems, errors first; `ready` is false while there are errors).
   Run each finding's `fix` command or fix it yourself, and check again; never report a design
   done while `design.check` has errors. `design.route` names every move between objects
   (untrimmed jumps, long moves), `design.density` shows where thread piles up; it and
   `design.check` take `fabric` (a `fabric.list` id: towel or fleece takes more layers).
   `design.cleanup` deletes objects that sew nothing or are tinier than `minSize` (one undo
   step; a `design.check` fix names it). In the editor, show the user where: `ui.run` with
   action `checkDesign`, `densityMap` or `reviewSewingOrder` opens Check Design on that tab;
   `ui.set` `densityOverlay` and `darkBackground` mark the design, `checkFabric` sets the
   fabric, `routeMap` draws the route, and `reviewStep` (from 1) moves the review and the
   simulator to that step. `ui.state` `check` has `ready`, the `errors` and `warnings`, the
   open `window` and `reviewSteps`. `ui.render` shows what you switched on: the density overlay's
   orange and red cells, and Check Design's rings while its Check tab is open, so look at the
   picture to see where.
   `design.timeline` lists the colour blocks in sewing order with their stitch ranges, trims,
   object starts and machine time ("when does the red start?"); in the editor,
   `ui.simulate {stitch}` or `{step: "block" | "object" | "trim"}` moves the player there.
6. Save only when asked: `project.save` `{path: "name.osd"}` (editable) and/or
   `design.export` `{path: "name.pes"}` (machine file; the extension picks the format). It
   reads the file back (`verify`: warn by default; strict deletes a file that differs and
   fails) and returns `problems`, Check Design's errors: tell the user about any.
   To show how it will look worn, `design.mockup {path: "name.png", garment: "tshirt" | "cap" |
   "tote", placement: "centre" | "leftChest" (T-shirt only), color: "#rrggbb"}` draws it at true
   size on the garment; when the result has `fits: false`, tell the user the design is larger
   than that place suits (`suitsMm`) and offer to resize.

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
- Badges and patches with text round a circle: `text.add` or `object.setText` with `arc`
  (mm, the first line's baseline radius; positive arches up over the top, negative arches
  down under the bottom, read upright). Each line (a `\n` in the text) becomes a ring round the same centre,
  one line spacing inward (up) or outward (down); a ring too tight for its line is refused
  with the radius needed, so retry with that. `arc: null` straightens it. Beyond PE-DESIGN:
  PES 0110 export refuses it (`text.toShapes` first).
- Trims and ties per object (beyond PE-DESIGN; defaults are PE-DESIGN's rules):
  `object.setFinish {ids, trim: auto|always|off, trimDistance, tie: auto|always|off,
  tieCount, tieLength, between}`. For a machine without an automatic cutter: `trim: off`
  (jumps instead of trims; the user cuts them by hand), and on text `between: word` or
  `never` so letters of a word are not trimmed apart. Slippery or thick threads: `tie:
  always` with `tieCount: 2`. `reset: true` goes back to PE-DESIGN's rules; check the result
  with `design.info` (trims, jumps).
- Threads: to change the design to a thread chart ("use my Madeira threads"), run
  `threads.remap` `{builtin | chart, metric: ciede2000, keepDistinct: true, dryRun: true}`
  first and show the user the plan (design colour → thread, ΔE, objects, and `merged`), then
  run it without `dryRun` if they agree (`metric: rgb` is PE-DESIGN's rule). The thread
  makers' charts (New Brothread, Madeira, Isacord, Sulky, Robison-Anton) are `builtin` ids:
  `threads.builtinCharts` lists them with `kind: "brand"`. New Brothread (`newBrothread`, the
  owner's 63-spool kit, numbered with Brother codes) is the default chart: the colour picker
  and the image wizards' OpenStitch mode start from it (a `digitize.*` call in OpenStitch mode
  without `threads` takes the default chart; PE-DESIGN mode always the PEC chart), and a first
  run's "My Threads" holds it. `threads.defaultChart` reads the default; `threads.setDefault {builtin | chart}` changes
  it (a settings change: say what changes and ask first; `builtin: brother` is Brother's PEC
  chart). Give a chart's thread as `color: {builtin: "newBrothread", code: "001"}` (or any
  colour parameter: `fill`, `stroke`); its Madeira codes are `crossReferences`, and
  `threads.find` finds a thread by them ("1801" is 001 White). Every thread the commands
  report has `code` (the code on its spool: Brother 007 for a PEC chart thread, never its PEC
  number), `brand` and `pecIndex` (the PEC chart number a PES file gives it) as separate fields
  (`design.info` `threads`, the `digitize.*` results, `stitches.points` `threadDetails`). Name a
  thread to the user by that code, as every colour list in the window shows it beside the
  swatch; `ui.threadLabels {list}` returns what a list shows. `threads.find`
  `{query}` finds a thread by code or name (`charts: ["brother"]` searches the PEC chart as the
  editor's colour picker search does). After `import.svg` or a `digitize.*` wizard gives
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
Several designs can be open as tabs: `ui.documents` lists them (index, name, unsaved, file),
`{op: "activate", index}` switches (every command then acts on that tab), `{op: "new"}` adds an
empty one, and `{op: "close", index}` closes a tab that has no unsaved changes (otherwise save
it or ask the user). `ui.set openInTabs` makes File ▸ New and Open add a tab.
To draw point by point as the user would from the keyboard, pick a drawing tool with `ui.run`
and step `ui.cursor`: `{x, y, action: "place"}` per point (`"smooth"` for a curved Pen point),
then `{action: "finish"}`; `{width}` sets the next Pen or Satin Skeleton point's width (a Pen
line with widths becomes a satin line). The shape commands (`shape.polygon`, `shape.path`)
remain the direct way to draw.
To fix single needle points of a stitch pattern, `stitches.points {id}` lists them and
`stitches.setPoint` / `insertPoint` / `deletePoints` edit them; `ui.stitchTable {open: true,
select: [indexes]}` shows the user the same rows and highlights them on the canvas. The same
commands take a live shape or text: its generated stitches become hand edits and it stays
editable; a sewing setting change drops them (the edit's reply lists `handEditsDropped`), and
`object.regenerate {ids}` drops them on purpose.
To show the user what a setting would look like before changing it, `ui.preview {command:
"object.setRegion", params: {ids, type}}` swaps in the stitches of that change on the canvas
(no undo step, the design unchanged; `ui.render` shows them too); `ui.preview {clear: true}`
ends it, and running the command keeps the change. `ui.set picturePickers` shows the sewing
types, fill patterns and motifs as pictures in the Sewing Attributes pane.
`ui.set textSize` (100-200) enlarges the whole window for a user who asks for bigger text.
`ui.set rulerOrigin` (`centre`, the default, or `topLeft`) sets where the rulers, the cursor
position and the precision bar's X and Y count from: the Design Page's centre (negative left and
up) or its top-left corner. Commands always take page coordinates from the top-left, whatever
the rulers show.
Guidelines (`guides.set`, `guides.list`, page coordinates) show and snap by default:
`ui.set guides` shows or hides them and `ui.set snapToGuides` turns snapping a dragged selection
to them on or off; both are remembered. Snapping to guides also needs the rulers shown
(`ui.set rulers true`): hidden rulers hide the guidelines and stop the snapping. One guideline at
a time, in page mm: `view.newGuide {orientation: "vertical"|"horizontal", position}`,
`view.moveGuide {orientation, index, position}`, `view.deleteGuide {orientation, index}`,
`view.editGuide {orientation, index, position?, turn?}` (Set Position…, or turned vertical <->
horizontal); `view.clearGuides`, `view.lockGuides {locked}` (locked guidelines still show and
snap but refuse move, edit and delete; saved with the design) and `view.newGuidesFromSelection
{ids}` (the box's edges and centre lines). Each is one undo step and answers the list.
`ui.guides` reads them as the user sees them, each position from the ruler origin with its label
("+12.5 mm"), and acts as the pointer does: `{orientation, index}` selects, then `position` (typed
as in Set Position…: `12.5`, `"12.5 mm"`, `"1/2\""`), `turn` or `delete`; `newGuide:
{orientation, position}` is New Guide… from the ruler origin. `view.guideLayout {columns, rows,
gutter, margin, clear}` (or `columnGutter`, `rowGutter`, `marginTop` … `marginRight`, all mm)
is New Guide Layout…: a line per margin and the edges of each column and row. `ui.set
guideColor "#rrggbb"` sets their colour.
`ui.set language` (`en`, or `pseudo`, a test language that brackets every translated text) is
Options ▸ Language (#555); the ribbon, menus, panes, dialogs, status messages and the printed
worksheet are translated, and no real translation ships
yet, so answer in the user's language yourself rather than switching the window.
Opening or importing a large file in the window runs in the background: `ui.state` shows
`loading` (file, step, seconds), and `ui.loading {cancel: true}` stops it, leaving the design
as it was.
The window's look and keys (#551, #559): `ui.set accent` (`default`, a preset such as `orange`,
or `#rrggbb`); `ui.set shortcuts` takes the changed commands as `{ commandKey: ["Ctrl+Alt+R"] }`
(keys from `ui.actions`; `{}` restores every default; a key another command uses is refused,
so tell the user which); `ui.set paneSewingOrderSide` / `paneAttributesSide` (left, right) and
`paneSimulatorSide` (top, bottom). "Use my lettering layout" is `ui.layout {op: "apply", name}`
(`list`, `save`, `delete` too); `ui.resetPrefs {area}` resets display, editing, panes,
shortcuts, layouts or newObjects (new-shape defaults, Basic/Advanced, start cards) alone (ask first).
For a new user, `ui.gettingStarted` lists the start cards and the kit's samples; `{sample:
"sampler"}` opens one as an untitled design, `{card: "draw"}` runs a card, `{page: true}` opens
Help ▸ Getting Started. "Close these cards" is `{cards: "dismiss"}` (for this session only, as
their ×, Escape or a click on the canvas; `{cards: "show"}` brings them back); "don't show them
again" is `ui.set showStartCards false` (it asks first; Options ▸ Show start cards turns them
back on). A user who knows PE-DESIGN may prefer its start-up dialog:
`ui.startupWizard` opens the Startup Wizard, `{choice}` runs one of its choices (template, images,
hoopAndFabric, importText, importShapes, importLibrary, open, database, manual), and `ui.set
startupWizard true` shows it at every start instead of the start cards. "Make new shapes start at 30° and 4.5 rows/mm" is `ui.set
newObjectDefaults {angle: 30, density: 4.5}` (also stitchLength, pullCompensation, pitch,
underlay; `{}` back to the tools' own); it changes only shapes drawn afterwards.
"Use this shape's settings for new shapes" (the pane's Save as Default) is the same setting:
read the shape with `object.get`, then `ui.set newObjectDefaults` with its region's angle,
density, stitchLength, pullCompensation and underlay and its line's pitch. Read their parameters with
`command_describe` before the first use. `ui.zoom {rect: [x, y, w, h]}` zooms to a rectangle.
`background.setLayer {position: "top" | "bottom" | "afterObject", afterId}` puts the tracing
picture above the design or between objects ("put the picture above the fills"); it never sews.

Units: every command takes millimetres. The user may work in inches (`ui.settings` → `units`,
also named in the editor note with each message); then answer in inches (1 in = 25.4 mm) and
still send mm to commands. `design.print {units: "inch"}` adds a 2-inch scale bar.

Job sheets (#513): "print an approval sheet for my customer" or "a worksheet for the machine
operator": set the job with `design.setJob {customer, order, due, notes, fabric, stabiliser}`
(an edit, undoable; saved with the project, never in machine files), then `design.print {path,
mode: "client" | "operator", detail: "overview" | "detailed", shop}`. Use only what the user
gives for the job fields; never invent a customer or order.

### Web editor (browser)

OpenStitch also runs in a web browser (#556, `crates/openstitch-web`): the page runs the desktop
editor itself (#693). The page exposes `openstitch.run({command, params})`, which takes what the
desktop editor's automation socket takes: every engine command (`object.rotate`, `design.info`,
`history.undo`, ...) and the editor's `ui.*` commands (`ui.state`, `ui.simulate`, ...), on the
editor's own undo history. The page adds its file commands, files as base64: `ui.openFile
{name, base64}` (File ▸ Open; it opens on the next frames, so wait until `ui.state`'s `loading`
is null), `ui.saveFile` (the `.osd` as `{name, base64}`) and `ui.exportFile {format}` (`{files:
[{name, base64}]}`). A command that writes under `/web/save/` (for example `design.export
{path: "/web/save/rose.pes"}`) hands the file to the user as a download. The browser editor has
no Assistant panel and no MCP or automation-socket control (#766): `openstitch.run(...)` is callable
only by code in the page, so an MCP client such as this one cannot reach it. If a user asks you
to work on their browser editor, say so and offer the desktop editor or a design file they can
open there. The system clipboard, printing, machine transfer and choosing folders (the Design
Database, Ink/Stitch font folders) are desktop-only too; say so instead of trying them in the
browser. Pictures, thread charts, fonts and other files can be picked or dropped on the page.
`ui.browserLimits` lists what the browser version cannot do (the Assistant, printing, USB
transfer, updates, folders, clipboard pictures), each with what to do instead; tell the user that
rather than trying it. Editor settings (`ui.set`), thread charts, My Threads and the other user
settings last across visits there, as on the desktop (the page's storage; cleared site data starts
afresh).

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
  `"satin"`, `"run r=3"` or `"stroke.satin w=3"`; the long key names work too
  (`"fill angle=45 density=4.5"`, `"run length=2.5 repeats=3"`, `"stroke.satin stroke.width=3"`).
  `"stroke.satin"` with no width makes a satin line as wide as the SVG stroke (`stroke-width`);
  `"stroke.zigzag"` asks for PE-DESIGN's zigzag line instead.
  The result lists unknown words and refused values (FILE_FORMATS.md has the words).
- A user who wants to script designs in Python can use the `openstitch` package (repository
  `python/README.md`: `Session().exec("shape.ellipse", ...)` with these same commands).
- "Make a thumbnail / preview picture of this file" at a terminal: `stitchtool thumbnail
  design.pes out.png [--size 256]` writes the stitches on a transparent square (16 to 1024
  px), the same picture Finder (Quick Look, with OpenStitch.app installed), Windows Explorer
  (with OpenStitch installed from its MSI) and Linux file managers show. In the editor,
  `view_render` shows the design instead.
- "Open the ready-to-sew designs": `ui.help {item: "sewKit"}` shows the packaged kit in the
  file manager and replies with its `folder`, or with the GitHub `url` it opened instead.
- **Test swatches** (sew a test before the real thing; each writes a new design file to
  `path`, `.pes` or `.osd`, and leaves the open design unchanged; editor: Send ▸ Test
  Swatches…):
  - "Sew a test of this part": `design.testSwatch {ids, path, label?}`, the objects alone,
    centred in the smallest hoop they fit (`label: true` sews their main settings below).
  - "Which density / pull compensation should I use on this?": `design.testSwatches {id, path,
    attribute, start, step, count}` copies one object with one setting stepped (`density`,
    `stitchLength`, `pullCompensation`, `angle`, `underlaySpacing` for regions; `linePitch`,
    `lineWidth`, `lineDensity` for lines), each copy named and labelled with its value;
    `attribute2/start2/step2/count2` makes a rows × columns grid (25 copies at most). A grid
    larger than the page is refused with the size it needs.
  - "Calibrate for my fabric": `fabric.calibrationSwatch {fabric, path}` writes a 100 mm card
    (fill F1–F5, satin S1–S5 at 0.8–1.2 × the fabric's density, satin P1–P5 at 0–0.6 mm pull
    compensation, a 10 mm square). After the user sews it and names the best piece of each
    row, `fabric.calibrate {fabric, name, fill, satin, pull}` (1–5 each) saves a custom
    fabric (`custom-…` in `fabric.list`) that `fabric.apply` gives to objects;
    `fabric.deleteCustom {fabric}` removes it. Both change the user's settings: say so first.
- Thread weight (#508): `fabric.apply {fabric, weight}` with No. 60 for small text sews rows
  closer (density × √(weight / 40)); No. 40 is the default and changes nothing. To make shapes and
  texts drawn from now on start from a fabric, `fabric.setDefault {fabric, weight}` (`fabric:
  "none"` clears it; it changes the user's settings, so say so first; `options.get` shows it).
- The user's machine (#508): `machine.profile` gives brand, hoop, `cutter`, `maxStitchMm` and the
  brand's own `format` (export that format when they ask for "the file for my machine").
  `machine.setProfile {brand, hoop, cutter, maxStitchMm}` (`brand: "none"` clears it) changes the
  user's settings: say so first, and set only what the user tells you (never guess a limit).
  Exports then warn about stitches longer than `maxStitchMm` and trims a machine without a
  cutter will not cut: pass those warnings on and offer `design.setStopAtJumps {on: true}`.
- "What stabiliser / needle should I use on fleece?": `fabric.advise {fabric, weight}` (weight
  12, 30, 40 or 60; default 40) gives backing, topping, needle point and size and the smallest
  legible block-letter height, each from the makers' published guidance with `sources`; say so
  when `disagree` is set, and say "estimate" when `minTextBasis` is not `published`.
  `fabric.list` has 13 fabrics: PE-DESIGN's eight, then polyester, pique, organza, canvas and cap
  (`beyondPeDesign: true`).
- `file` commands (`project.save`, `design.export`, ...) may ask the user first. Never
  overwrite a user's file without being asked to.
- To show the user these dialogs in the editor: `ui.run {action: "testSwatches"}` (Send ▸ Test
  Swatches…) and `ui.run {action: "jobSheet"}` (Print ▸ Job Sheet…).

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
