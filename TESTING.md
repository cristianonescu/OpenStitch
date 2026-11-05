# Testing OpenStitch on your machine

Every sew-out so far has been on one machine, a Brother Innov-is NV880E. Everything else is untested
on real hardware: other machines, and the formats **DST, EXP, JEF, PCS, HUS, VIP, SEW and XXX**,
which have only been compared byte for byte with the files PE-DESIGN 11.50 exports. A machine that
reads one of those files and sews it correctly (or doesn't) tells us something no computer test can.

You need a machine, a few metres of thread, a bit of fabric and, for the best results, a flatbed
scanner. Results are collected in issue
[#292](https://github.com/cristianonescu/OpenStitch/issues/292) and reported with the
[Sew-out / calibration report](https://github.com/cristianonescu/OpenStitch/issues/new?template=sew-out-report.yml)
form (a free GitHub account is needed).

## 1. What to sew

The ready-to-sew kit is in [`examples/sew`](examples/sew/README.md) (also **Help ▸ Ready-to-Sew
Designs** in the editor, and a download with each release). Its files are PES, sized for
100 × 100 mm, 150 × 150 mm and 260 × 160 mm hoops; its "How to sew them" section lists needle,
thread and stabiliser. A machine that does not read PES needs the kit's designs exported to its
format first (section 2).

### The machine checks matter most

Start with the four files in [`examples/sew/machine-checks`](examples/sew/machine-checks), all for
the 100 × 100 mm hoop and each a minute or less of sewing. They test what only a machine shows:

| File | What to look for |
|---|---|
| `machine-formats.pes` | **The one for non-PES formats.** The two Red lines sew with one thread cut between them (no thread across the 40 mm gap); the machine stops once for Black; the 15 mm satin sews whole, with no needle holes along its long edges; the machine shows the design as 40 × 40 mm. |
| `satin-zigzag-rect.pes` | The satin is a zigzag across the column: no needle holes or ragged thread along the two long edges. |
| `shape-start-points.pes` | Seven colour stops in drawing order; each shape starts and ends at the bottom middle of its outline; every move between shapes is cut (no thread on top). |
| `pes-default-v6.pes` | Opens and sews on your machine (PES machines only). |

### What to sew, by machine

| Machine | File format | Sew |
|---|---|---|
| Brother, Baby Lock, Bernette (PES machines) | PES | The four machine checks, then the calibration designs ([step 3](#3-scanning-or-photographing-the-result)) and anything else in the kit you like. Machines other than the NV880E matter: please say the model. |
| Janome, Elna, Singer, Husqvarna Viking, Pfaff, Tajima-style and commercial machines | JEF, SEW, XXX, HUS, VIP, PCS, DST, EXP: whichever your manual says the machine reads | `machine-formats.pes` exported to your format, then `satin-zigzag-rect.pes` and `shape-start-points.pes` the same way. |
| Husqvarna Viking and Pfaff machines that read **VP3** (Designer, Sapphire, Jade, Epic, creative…) | VP3 (new, experimental: no VP3 sew-out yet) | `machine-formats.pes` exported as `.vp3`. Please report whether the machine opens it, shows each colour's thread brand, number and name, cuts between the two Red lines and sews the 15 mm satin whole. |

If your machine reads several formats, sew the same check in each: a result that differs between
formats is exactly what we are after. Check your machine's manual for the formats it reads; if you
are unsure, try one, and say so in the report.

## 2. Exporting to your machine's format

1. Open the kit file in OpenStitch (**File ▸ Open**).
2. **Export** (in the File group) writes the design as a machine embroidery file; give the file
   name the format's extension (`.jef`, `.dst`, `.hus`,
   `.vip`, `.sew`, `.xxx`, `.pcs`, `.exp`, `.vp3`).
3. Copy the file to the machine's USB stick or card the way your manual describes. Some machines
   need a short file name (8 characters) or a particular folder.

Without the editor, `stitchtool` does the same: a script with `design.export` and a path (or the
`format` option) writes any of these formats. Sew the exported file as it is; if you resize, rotate
or move it on the machine, say so in the report.

Please keep the exported file and mention its name: it lets us compare it byte for byte with what
OpenStitch writes.

## 3. Scanning or photographing the result

Needed only if you sew the calibration designs (see the kit README's
[Calibrating OpenStitch](examples/sew/README.md#calibrating-openstitch): `calibration/t1-swatches.pes`,
`calibration/ps2-grey-ramp.pes`, `calibration/satin-and-underlay.pes` and
`image-tools/S-photo1-sunset-openstitch.pes`). For the machine checks a clear photo and your notes
are enough.

- **A flatbed scan is best:** Photo Mode (not Document Mode), Colour, 600 dpi, PNG, no rotation,
  skew correction and every automatic correction (exposure, colour restoration, backlight) off.
  Put a plain white paper strip on the glass next to the fabric as a colour reference. Several
  pieces per scan are fine if they lie flat and do not overlap.
- **With a camera:** flat, straight from above, even daylight without direct sun or flash, a white
  paper or grey card in the frame, the same distance for every piece.
- **Fabric:** white or light grey is best for calibration; a coloured fabric shows through the
  White swatches.
- **Thread for the T1 card:** write down the brand, colour name and code you used for each of
  White 001, Black 900, Red 800, Blue 405, Yellow 205 and Leaf Green 509.

Attach the original PNG files, not screenshots, so the full resolution is kept.

## 4. Reporting

Open the [Sew-out / calibration report](https://github.com/cristianonescu/OpenStitch/issues/new?template=sew-out-report.yml)
form and fill in:

- **Machine** (brand and model) and **format** the machine read, **file exported** (name, and
  OpenStitch version), **hoop**;
- **files sewn**, and any resizing, rotating or moving on the machine;
- **thread**, **needle**, **fabric and stabiliser**;
- **how the images were made** (scanner or camera and its settings) and the **scans or photos**;
- **what you saw**: the machine's on-screen messages word for word (a file it would not open, a
  size it showed, a warning), thread breaks, loops, puckering, gaps, stitches piling up in one
  spot, or "all fine".

A failure is as useful as a pass: if the machine refuses a file, report the file name, format and
the exact message. A short report with the machine, the file and what happened is better than none.

## 5. What happens with the results

Calibration results are published in the kit README's
[Calibration results](examples/sew/README.md#calibration-results), with the setup (machine, thread,
needle, fabric) and no personal details, with your consent (the publishing box on the form). Machine-check results become
records in [`compat/machine`](compat/machine/README.md) and can move a compatibility spec to the
machine tier. Problems you find become GitHub issues, linked back to your report.

Thank you for sewing.
