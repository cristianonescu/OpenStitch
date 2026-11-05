# Hoops and machine types

Source data for `openstitch_design::machine::BROTHER_HOOPS` and `OTHER_HOOPS`. Only the makers'
public pages are used; nothing comes from the PE-DESIGN installation or from another program's
hoop list.

## What the PE-DESIGN 11 guide says

[Specifying the Design Page Settings](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/03_1245955.html):

- Machine Type: the Page Size choices depend on it. A multi-needle machine icon exists; the
  guide shows the choices only as images.
- Hoop Size: the list is an image. In the text, only the multi-position hoops are named:
  100 × 172 mm and 130 × 300 mm, marked "*".
- Custom Size: width and height typed in, for split designs. No limits are given.
- Rotate 90 Degrees: not for cap, cylinder or round frames, and not for user hoops.
- User hoops: width, height, optional comment; up/down ordering; added at the bottom of the
  list. Designs in a user hoop size cannot be saved in previous-version formats.
- Output ▸ sewing area: "Design Page area" (needle starts at the page centre; the design is as
  large as the page) or "Use existing design area" (needle starts at the design centre).
- Jump Stitch Trimming: multi-needle machines only; a minimum jump length for trimming.

## Hoop sizes and where each comes from

| Hoop (mm) | Source |
|---|---|
| 100 × 100, 130 × 180 | PE-DESIGN 8 manual, "Creating Design For Multi-Position Hoops" (manualslib page 184) |
| 160 × 260 | Brother SA441 6" × 10" hoop page (brother-usa.com/products/sa441) |
| 200 × 300 | Brother SA447 and PRH300 8" × 12" frame pages |
| 240 × 360 | Brother USA category listing of a 9-1/2" × 14" embroidery area |
| 272 × 272 | PE-DESIGN 11 update history, version 11.10 |
| 180 × 360 | update history, 11.20 |
| 100 × 180, 254 × 254 | update history, 11.30 |
| 297 × 465 | update history, 11.40 |
| 200 × 360 | update history, 11.50 |
| 360 × 360 (jumbo frame) | update history, 11.23 Design Database notes |
| 100 × 172 *, 130 × 300 * | PE-DESIGN 11 guide (multi-position) |

Update history: <https://support.brother.com/g/s/hf/ped_info/ped11/one_click_update/en/ped11update_info.html>.

## Gaps

- The full image-only hoop list (cap, cylinder and round frames, small hoops) is not in text
  form anywhere public we found. Add entries only with a citable source.
- Which hoop each machine model accepts is not published in one table, so every hoop is offered
  for both machine types. PE-DESIGN, too, leaves this to the user.
- Thread-length and sewing-time models are first-principles estimates (1 mm per penetration,
  10 mm per trim, bobbin = needle / 3; 800 / 1000 stitches per minute, 3 s per trim, 5 s per
  multi-needle colour change). They are not calibrated against PE-DESIGN's Design Property
  figures, which needs a licensed installation.

## Other brands and round hoops (#514)

`OTHER_HOOPS` carries each hoop's source in its `source` field; the sizes are the sewing field
as the maker publishes it, in the maker's width × height order. Read on 2026-11-05.

| Brand | Hoops (mm) | Source |
|---|---|---|
| Baby Lock | 4" × 4" 100 × 100, 5" × 7" 130 × 180, 6" × 6" 150 × 150, 8" × 8" 200 × 200, 8" × 12" 200 × 300, 1" × 2-1/2" 20 × 60, Large Continuous Border 100 × 300 | babylock.com accessory pages ("Embroidery Hoop and Grid ...") |
| Bernina | Medium 100 × 130, Large Oval 145 × 255 (oval), Maxi 210 × 400, Jumbo 260 × 400, Mega 150 × 400 (three attachment positions) | bernina.com accessory pages (en-US); bernina.com answers automated fetches with 403, so the figures were read through a web search index of those pages |
| Elna | SQ14b 140 × 140, RE20b 140 × 200, SQ20b 200 × 200, RE36b 200 × 360 | elna.com/en-si eXpressive 830L page |
| Elna | RE28b 200 × 280; SQ23 230 × 230, GR 230 × 300; Hoop M1 240 × 200 | elna.com/en-si eXpressive 830, 900 and 970 pages |
| Husqvarna Viking | Standard 100 × 100, 5" × 7" Design 180 × 130, DESIGNER Jewel 240 × 150, Mega Endless 260 × 150, Crown 260 × 200, Royal 360 × 200, Imperial 360 × 260, Majestic 360 × 350 | brand-labelled product pages on singer.com, the store of SVP Worldwide (maker of Singer, Husqvarna Viking and Pfaff); husqvarnaviking.com redirected to a host this session could not reach |
| Janome | RE10b 100 × 40, SQ14b 140 × 140, RE18 140 × 180, RE20b 140 × 200, ASQ22 220 × 220, Giga Hoop D 230 × 200 (overlap hoop), Macro Hoop 200 × 280 (multi-position) | Janome UK accessories catalogue PDF (janome.co.uk/_files/ugd/64fc51_a57f0ed0d23c45c8a6a0ede779236610.pdf); its RE10b inch figure is a misprint, the mm figure is used |
| Janome | SQ10E 100 × 100 | janome.co.uk/mc100e |
| Pfaff | creative Petite Square 80 × 80, 120 Square 120 × 120, All Fabric II 150 × 150, Master 240 × 150, Elite 260 × 200, Grand Quilters 260 × 260, Deluxe 360 × 200, Supreme 360 × 260 | brand-labelled product pages on singer.com (as Husqvarna Viking) |
| Singer | 40 × 40, 100 × 100, 170 × 100 (SE9180) | singer.com product pages |
| Commercial round | Melco round 9, 12, 15, 18, 22 cm inside diameter; field 3 cm smaller (15 mm clearance) | melco.com/en-gb/standard-frame/: "The dimensions indicate the diameter of the inside of the frame without the safety clearance. The actual embroidery field is approx. 3 cm smaller." |

Left out: Baby Lock's 14" × 14" jumbo (the page gives the frame, not the field), Ricoma's hoops
(inches only and no shape stated), Husqvarna Viking's Mega Turnable hoop (how it turns is not
stated), Pfaff's Grand Dream hoop (no product page found).

JEF hoop codes: JEF's header has codes for 110 × 110, 50 × 50, 140 × 200, 126 × 110 and
200 × 200 mm. A selected Janome or Elna hoop of exactly one of those sizes (RE20b, SQ20b) writes
its code; any other hoop leaves the writer's choice (the smallest code the design fits). HUS,
VIP, SHV, EXP, XXX and PCS headers have no hoop code; DST has none either.
