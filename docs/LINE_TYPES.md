# Line sew types

Every line sew type, and the motif line's own options (#544), each sewn on the same curved
line across 84 mm. Each picture is the stitches the engine sends to the machine, drawn by
`tools/engine_image.py` from `examples/line-type-<name>.json` (regenerate them with
`python3 -I tools/docs_images.py --only engine`); the command beside it is the one in that
script. Beyond PE-DESIGN: groups, free ends, motifs in turn and the hand-embroidery lines are
OpenStitch's own; PE-DESIGN's defaults are unchanged.

## Running

![Running](images/engine-line-type-running.png)

`object.setLine {"type": "running"}`

## Triple

![Triple](images/engine-line-type-triple.png)

`object.setLine {"type": "triple"}`

## Zigzag

![Zigzag](images/engine-line-type-zigzag.png)

`object.setLine {"type": "zigzag", "width": 3, "halfStitch": true}` (the curve); `object.setLine {"type": "zigzag", "width": 3, "sharpCorners": 1, "underSewing": true}` (the closed outline below it: PE-DESIGN's first Sharp Corners button at its acute corners, #373, and under sewing in its two runs, #964)

## Satin line

![Satin line](images/engine-line-type-satin.png)

`object.setLine {"type": "satin"}`

## Back stitch

![Back stitch](images/engine-line-type-back-stitch.png)

`object.setLine {"type": "backStitch"}`

## Stem

![Stem](images/engine-line-type-stem.png)

`object.setLine {"type": "stem", "width": 2.5, "spacing": 1.5}`

## Chain

![Chain](images/engine-line-type-chain.png)

`object.setLine {"type": "chain", "width": 3, "chainShape": "diamond"}`

## Candlewicking

![Candlewicking](images/engine-line-type-candlewick.png)

`object.setLine {"type": "candlewick", "width": 3, "spacing": 6}`

## E stitch

![E stitch](images/engine-line-type-e-stitch.png)

`object.setLine {"type": "eStitch", "width": 3, "spacing": 3}`

## V stitch

![V stitch](images/engine-line-type-v-stitch.png)

`object.setLine {"type": "vStitch", "width": 3, "spacing": 4}`

## Scribble

![Scribble](images/engine-line-type-scribble.png)

`object.setLine {"type": "scribble"}`

## Chevron

![Chevron](images/engine-line-type-chevron.png)

`object.setLine {"type": "chevron"}`

## Cretan

![Cretan](images/engine-line-type-cretan.png)

`object.setLine {"type": "cretan"}`

## Feather

![Feather](images/engine-line-type-feather.png)

`object.setLine {"type": "feather"}`

## Overlock

![Overlock](images/engine-line-type-overlock.png)

`object.setLine {"type": "overlock"}`

## Alternating triangles

![Alternating triangles](images/engine-line-type-alternating-triangles.png)

`object.setLine {"type": "alternatingTriangles"}`

## Double zigzag

![Double zigzag](images/engine-line-type-double-zigzag.png)

`object.setLine {"type": "doubleZigzag"}`

## Lattice

![Lattice](images/engine-line-type-lattice.png)

`object.setLine {"type": "lattice"}`

## Stripes

![Stripes](images/engine-line-type-stripes.png)

`object.setLine {"type": "stripes"}`

## Ripple

![Ripple](images/engine-line-type-ripple.png)

`object.setLine {"type": "ripple"}`

## Cutting

![Cutting](images/engine-line-type-cutting.png)

`object.setLine {"type": "cutting", "leaveUncut": true}`

## Motif

![Motif](images/engine-line-type-motif.png)

`object.setLine {"type": "motif", "motif": "heart", "width": 6, "height": 5, "spacing": 1}`

## Motif line in groups with free ends

![Motif line in groups with free ends](images/engine-line-type-motif-groups.png)

`object.setLine {"type": "motif", "motif": "heart", "width": 5, "height": 4, "spacing": 0.5, "groupSize": 3, "groupGap": 6, "freeStart": 6, "freeEnd": 6}`

## Motifs in turn

![Motifs in turn](images/engine-line-type-motif-in-turn.png)

`object.setLine {"type": "motif", "motif": "heart", "width": 5, "height": 4, "spacing": 1, "motifs": ["star"]}`

## Motifs in turn, mirror-alternate

![Motifs in turn, mirror-alternate](images/engine-line-type-motif-mirror-alternate.png)

`object.setLine {"type": "motif", "motif": "heart", "width": 5, "height": 4, "spacing": 1, "motifs": ["star"], "arrange": "alternate"}`
