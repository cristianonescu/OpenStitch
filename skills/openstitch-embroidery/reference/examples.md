# Example designs

Complete, tested designs to start from. Open the one closest to the request with
`skill_reference {"name": "<example>"}` (for example `"strawberry"`), then adapt it: its steps
are a `run_batch` `steps` list (and a `stitchtool exec` script). Each starts with `design.new`,
so ids count from 1 in creation order; when you change the parts, recount the ids or create the
shapes in one batch and style them in the next. Each ends with `design.check`; add
`design.export` only when the user asks for a file. `reference/composition.md` explains the
rules they follow.

| Example | What it shows |
|---|---|
| `strawberry` | A fitted body, leaves and stalk (one `shape.removeOverlap` before any angle is set), fills 90° apart, a satin stalk, small satin seeds sewn over the fill last |
| `mushroom` | A cap in three fitted panels drawn as Bézier paths, each fill at a different angle, a stem under the cap, satin spots cut out of the cap |
| `watermelon` | A half-disc fill with a separate satin rind (an open Bézier path with a 4 mm zigzag line), small satin seeds (ellipses: a pointed satin knots at its tip) |
| `lightbulb` | A face: satin eyes and a bean-stitch smile (`runTimes: 3`) on a fill, a fitted metal base with narrow zigzag thread lines |
| `badge-arc-text` | A round badge: fill with the star cut out of it, lettering on a hidden circular guide, the satin border as its own outline shape (so cutting the fill leaves it whole) |
| `applique-heart` | `wizard.applique` from a heart: material, position and tack-down lines, then a satin cover |
| `monogram` | `monogram.add` with three block letters inside a ring frame, two colours (letters with slanted strokes, such as A, M, N, V, W, X, still sew with gaps at their joins in Lato Bold: #446; check `view_render`) |
| `line-art-cup` | Outline-only line art: closed and open paths sewn as bean stitch, no fills |
| `cat-face` | A cartoon animal: fitted head and ears, eyes and nose cut out of the head, satin borders (the head's as its own outline shape), bean-stitch mouth and whiskers |
| `croissant` | Five fitted segments with alternating fill angles; thin satin borders on the tips and the middle only, so each seam has one border, not two |

All are 100 × 100 mm designs, 55-85 mm across, with the fabric as the background.
`tools/skill_examples.py` runs every one, checks `design.check` (no errors; overlap warnings
only where a border edges its fill), exports it and checks the PES with `tools/sew_check.py`.
