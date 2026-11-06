# Composition: illustrated designs

How to turn "a strawberry with leaves and seeds, about 60 mm" into a design that reads well
and sews cleanly. Before you start, open the example closest to the subject
(`skill_reference {"name": "examples"}` lists them; `skill_reference {"name": "strawberry"}`
returns one) and adapt it: change sizes, colours and parts rather than inventing every step.

## Plan in stitched regions

Write the plan before the first command: one line per region with its **colour**, **sew type**,
**fill angle**, **layer** (what it sits on) and **why it is there**. A region that has no reason
(a highlight smaller than 2 mm, a shadow nobody will see) is left out.

- Simplify the subject into **bold regions**. Embroidery cannot show marks smaller than about
  2 mm across or lines thinner than about 1 mm (`reference/sewability.md`); merge or drop them.
- **4-7 colours** suit most illustrated designs. Each colour is a thread change on a
  single-needle machine; reuse a colour for parts that can share it.
- **The fabric is the background.** Do not fill behind the subject unless the user asks for a
  patch; white highlights on white fabric are left unsewn, not filled white.

## Size and place

- The subject should be **55-85 %** of the hoop's shorter side (55-85 mm in a 100 × 100 mm
  hoop) unless the user gives a size; a size the user names wins.
- Centre the subject on its **real extents** (the union of every part, `object.list` bounds),
  not on its first shape: `object.center {ids: [every id]}` after building (it centres them as one), or work out
  the coordinates around the page centre (50, 50 on a 100 mm page).

## Layer order

Sew from the back of the picture to the front:

1. **Base fills**: the large body regions (the berry, the cap, the melon flesh).
2. **Major regions** on top or beside them (the leaves, the stalk, the rind).
3. **Borders**: satin outlines, only where they are needed (below).
4. **Details** last: seeds, eyes, small marks, running-stitch line work.

Put the order right with `order.byClickOrder {ids: [in sewing order]}`, or `order.byColor`
when the colours do not overlap. Read `object.list` to confirm.

## Shapes that fit together

- Panels that touch (the cap's three sections, the melon's flesh and rind) should **meet**, not
  stack. Draw them overlapping, then `shape.removeOverlap` (or `shape.combine {op: divide}`)
  so each area is sewn once; stacked fills sew thick and stiff.
- Cutting a shape (`shape.removeOverlap`, `shape.combine`, `shape.split`, `shape.knife`,
  `shape.build`, `shape.effect`) keeps its sewing angle and other sewing attributes; a merge
  (`shape.merge`, a `shape.build` drag) takes the topmost shape's (the one sewn last). Entry
  and exit points stay where they are still on the new outline (#652).
- Give **neighbouring fills different angles**, at least 20° apart (for example 45° and 135°,
  or 30° and 90°), so they catch the light differently and the seam between them reads.
- A detail on top of a fill (seeds on a berry) is small enough to sew over the fill: keep it,
  as the eye reads it as raised.

## Borders

- Add a **satin border only where it separates fitted pieces** or defines the silhouette
  against fabric of a similar colour: `object.setLine {type: "zigzag", width: 2-3}`. A border
  around every part makes a cartoon look heavy and adds trims.
- Borders sew **after** the fills they edge (`object.setPartOrder` keeps `regionFirst`, the
  default) and in a darker or contrasting colour.
- A thin outline-only drawing (line art, an icon) is running or triple stitch:
  `object.setLine {type: "running", runTimes: 3}` for a bold bean-stitch line.

## Faces and small marks

- Eyes: ellipses of 2-4 mm filled; a mouth: an open path with a running or triple line, or a
  zigzag 1-1.5 mm wide.
- Seeds, dots, stars under 3 mm: a small satin region (`object.setRegion {type: "satin"}`)
  sews cleaner than a fill.
- Text inside an illustration: at least 5-6 mm capitals (`reference/sewability.md`).

## Check before you report

1. `view_render` and look: is the subject recognisable, are all asked parts present?
2. `design.check`: no errors (long stitches, untrimmed long jumps); fix and check again. Its
   "layers of thread where objects overlap" warning is expected where a satin border overlaps
   the fill it edges; for a detail stacked on a fill, cut the fill with `shape.removeOverlap`
   and then set that fill's angle again (#652).
3. `design.info`: report the size, colours and stitches from it, not from your plan.
