# PE-DESIGN 11 minimal designs (`pes11` tests)

**Pattern blocks redacted (2026-11-14, #1060).** PE-DESIGN writes its default decorative pattern into the pattern block of every shape it saves; that is Brother content, so every such block in these files was rewritten to OpenStitch's placeholder rows at the same length, every other byte unchanged (`tools/redact_pattern_blocks.py`). The SHA-256 values here are the redacted files'; the hashes as PE-DESIGN saved them are in `docs/research/2026-11-14/pattern-block-redaction-table.md`.

Made by the owner's PE-DESIGN 11.50 (Layout & Editing) on Windows 11 Pro 10.0.26200, on
2026-10-27, by black-box GUI use only (track `pe-gui`), for mapping the `#PES0110` object list
(docs/FILE_FORMATS.md, "PES 0110 object list"). Studying these files is allowed as project
policy: docs/LEGAL_AND_LICENSING.md. Each rN file is the baseline B with one property changed.

**Common:** PE-DESIGN freshly restarted at 20:41 (it had closed); every file: App menu > New, Design Page 100 x 100 mm (as last used), grid 1 mm, Snap to Grid on; Save As, type "Layout & Editing Ver.11.0 File (*.PES)" (#PES0110) unless stated. Page origin top-left, y down.

**Baseline B:** Home > Shapes > Rectangle/Circle/Arc > Rectangle, dragged corner to corner (10,10)-(20,20) mm. Attributes as the freshly restarted tool shows them (= PE defaults recorded in round 1 notes.md, house buttons): Edge radius 0.0; Line sew Zigzag Stitch, colour 800 EMBROIDERY RED, Under sewing on, Zigzag width 2.0 mm, Density 4.5 line/mm, Half Stitch on, Sharp Corners first button, Miter Limit 0.0; Region sew Fill Stitch, colour 005 EMBROIDERY SILVER, Under sewing on (first type, Density Medium), Density 4.5 line/mm, Gradation off, Direction Manual straight line 45 degrees, Stitch Type first, Running Stitch Path first, Step pitch 4.0 mm, Frequency 30 %, Pull compensation 0.0 mm. Status: 12,00 x 12,00 mm (the 2 mm zigzag straddles the outline), 471 stitches, Sewing Order: 1 fill (silver), 2 zigzag (red).

**Text tool (r12–r15):** Home > Text > first tool "A"; as the freshly restarted tool shows it: font Lato (the repo's Lato-Bold.ttf, installed per-user earlier; PE lists "Lato", style Bold), size 10.0, line Not Sewn (Line), region Satin Stitch, region colour 800 EMBROIDERY RED, Text Attributes all default (Kerning 0, Vertical Offset 0, Rotate 0, Character Spacing 0, Line Spacing 100 %, Alignment Left, horizontal, Transform off). The click sets the baseline-left of the text. A first r12 placed with the click at (10,10) put the letter at y 0..10 mm; Save As warned "Please change the object size or position. Do you want to save the data anyway?" -> No (discarded), so the click was made at (10,20) instead: the letter's top-left is at about (10,10).

**Round 2:** (2026-10-27 22:24-22:33): PE-DESIGN 11.50 freshly restarted at 22:24 (it had closed), so tool attributes were at defaults again; same COMMON and BASELINE B as above. Colours are set in the Color pane (Line / Region radio, BROTHER EMBROIDERY chart). Tools remember the last colour/type, so each file was checked and set back to B before its one change.

| File | SHA-256 | Made as |
|---|---|---|
| `r01-rect.pes` | `4f8d070fee21798d3ab3e89fbfe0c4ab7da7ae79726470c3008bd2caf5eadd18` | B. 12,00 x 12,00 mm, 471 st. |
| `r01-rect-v6.pes` | `7e06d9f3bfedce86d05f7f1998582f72738c5f5d462a24240eeb59850229056e` | B, saved as type Ver.6.0 (`#PES0060`; PE-DESIGN warns that the data is saved as stitch data). Its thread table writes the two Brother colours as PE-DESIGN writes them in `#PES0110`: `005` / `16` / `EMBROIDERY` and `800` / `5` / `EMBROIDERY` (added 2026-11-04 for #375). |
| `r02-rect-w20.pes` | `05e2e4d4688552b00ecaf0d6b55824ca254ac2337bcd91226a52ea0b215ad934` | B but (10,10)-(30,20). 22,00 x 12,00 mm, 745 st. |
| `r03-rect-moved.pes` | `e6bc51794f88c04796e6769d514eff4acbece2d0b3e3b6195abe2a112ca0ff6f` | B but (30,30)-(40,40). 12,00 x 12,00 mm, 473 st. |
| `r04-rect-satin.pes` | `00941ea312b33a1930585750fdce88fe39138d1834d2b8f232966a10de4e9f37` | B, then (shape selected) Shapes tab > Sew > region type Satin Stitch (satin defaults: Under sewing on Medium, Density 4.5, Direction 45). 12,00 x 12,00 mm, 370 st. |
| `r05-rect-blue.pes` | `fcc9bbcff955a4129e17730749db0f1303c7225ae3be0d1c05268f6e670285c1` | B, then Color pane, Region, BROTHER EMBROIDERY 007 PRUSSIAN BLUE. 12,00 x 12,00 mm, 471 st. |
| `r06-rect-nounder.pes` | `d48d6e45f33d708cfc86a181be265be3aded26eb3716893e9c3d71d176190825` | B, then Sewing Attributes > Region sew > Under sewing unticked. 12,00 x 12,00 mm, 423 st. |
| `r07-rect-noline.pes` | `dcd764c8132a162b4b5070e7c29fa293c8f75030986a7b32c8da5b6f7194fc40` | B, then line type Not Sewn (Line). 10,00 x 10,00 mm, 230 st. |
| `r08-rect-dir90.pes` | `ad76adc642916be5505279f44a29a6217552d909d88d4cb263c3ce2911aba591` | PE's default fill Direction is already 45 degrees, so a 45-degree file would equal r01; made with Direction Manual straight line 90 degrees instead. 12,00 x 12,00 mm, 462 st. |
| `r09-rect-density.pes` | `7a7718c53b2cdf42f4a758a9744ff90dfbe5898126f92a95b0c11a3855caad41` | B, region Density 6.0 line/mm. 12,00 x 12,00 mm, 530 st. |
| `r10-circle.pes` | `30aa1800fe74ee2a5ec4012357fa55c1ad1eb0bce3579be231fd61ab2c6a8891` | Shapes > Ellipse (Rectangle/Circle/Arc row, 2nd) dragged (10,10)-(20,20), same default attributes as B (Zigzag red / Fill silver, values as above). 12,00 x 12,00 mm, 400 st. |
| `r11-line.pes` | `d07ea120183432aa03979fab78f7ba6aa4295e6a743a9c7a8c682b0247dc1c74` | Shapes > Outline > open straight line, click (10,10), double-click (30,10); Select tool. Line Zigzag Stitch 800 RED, defaults (under sewing on, width 2.0, density 4.5, half stitch on). 20,00 x 2,00 mm, 118 st. |
| `r12-text-a.pes` | `505ed96e7340d306a55b6b2b3836e0a5f1356959c46d1e31a1feed6bf773d047` | "A", click (10,20). 10,10 x 10,30 mm, 155 st; box about (10,9.7)-(20.1,20). |
| `r13-text-a-20.pes` | `3afdeeec119e211c82084c8cb77163fc9134843f9fb7dd6b881904f236c09ebb` | r12, then (text selected) size 20.0: the letter grows upward past the page top, so it was dragged down 10 mm (snap) to stay on the page: box about (10,9.4)-(30.1,30), baseline y 30. 20,10 x 20,60 mm, 290 st. |
| `r14-text-ab.pes` | `a1e941016670496f251799e46bdee13243cf305698a66f0f7545a3dba4f990a9` | "AB" (size set back to 10.0 on the tool), click (10,20). 18,80 x 10,30 mm, 363 st. |
| `r15-rect-text.pes` | `c7795f1f360c0e31fee246afafe36dda0f5650d7f67dcb40be567a45acb06aaa` | B, then Text tool "A" size 10 with the click at (40,50) (top-left about (40,40)). Whole design 41,00 x 41,00 mm, 626 st; Sewing Order: 1 fill silver; 2 red = zigzag + A (same colour, one block). |
| `r16-two-rects.pes` | `a0b4878bdbdb01f1b9f36f9499643e47c764ca234332e70394116cd8589a98a5` | B, then a second Rectangle (40,10)-(50,20), same attributes. 42,00 x 12,00 mm, 944 st; Sewing Order 1 silver, 2 red, 3 silver, 4 red. |
| `r17-line-silver.pes` | `1e71589326dcd590f7ecce665be348016f3ea9a5006dbb3c83378c58c7f4b262` | B, Color pane > Line > 005 EMBROIDERY SILVER (same as the region). PE then shows ONE colour block (Sewing Order 1 item with both). 12,00 x 12,00 mm, 467 st. |
| `r18-colours-swapped.pes` | `85835bdfb3eacde11c2e406657f3416f79095d4a8fd12ee432f901b7d7ae2788` | B, region 800 EMBROIDERY RED and line 005 EMBROIDERY SILVER. 12,00 x 12,00 mm, 471 st (blocks: 1 red fill, 2 silver zigzag). |
| `r19-line-running.pes` | `ffe5d875bef845d321ff03a3890f8a0d46d93ca4d6b668a233b58de72089970a` | B, Shapes tab > Sew > line type Running Stitch (defaults Run pitch 2.0 mm, Run time(s) 1). 10,00 x 10,00 mm, 256 st. |
| `r20-line-running-pitch.pes` | `f82135f6704f19eea6cdc8c7c7502f5d86a67f6745f8486258eb6c0926ee8628` | r19 (same design), Run pitch 3.0 mm. 10,00 x 10,00 mm, 251 st. |
| `r21-zigzag-width.pes` | `65c1cb6310d5c34b89d6c9306005b6d5a8d9469128893ec237646b04733851af` | B, line Zigzag width 3.0 mm (default 2.0). 13,00 x 13,00 mm, 484 st. |
| `r22-line-noUnder.pes` | `17d8c20304e97b1f4fbe2d248a51ed913ebf15270dc1d3400821fe49ce93d3d2` | B, Sewing Attributes > Line sew > Under sewing unticked. 12,00 x 12,00 mm, 426 st. |
| `r23-text-fill.pes` | `f7e7e2b6d7799c46139981403bd4db7b0dd7081472c5bf2b020d80d1c0e27e61` | r12's text `A` (Text tool, Lato, size 10, click (10,20)), region Fill Stitch (the text fill defaults: under sewing off, density 5.0). 187 st. Copied into the repo 2026-10-29 for the `#PES0110` text writer (#120). |
| `r24-text-blue.pes` | `8c8c7fef1f2d2d1a69300b7fb484a9f8c7e431e5c27bd84db96319528dc0914c` | r12's text `A`, region Satin, region colour 007 EMBROIDERY PRUSSIAN BLUE. 155 st. |
| `r25-text-b.pes` | `e5e0bc6b7e8b545e3bde1944852c661789279882ad5032b7c6f9d399d5203063` | r12 but the letter `B`. 7,70 x 10,30 mm, 207 st. |
| `r26-text-moved.pes` | `1450b6f8f655ec92a930c10be2afcf489e7cab43e24c9e1adea2a4c083c2861f` | r12 with the click 10 mm further right, at (20,20): box about (20,9.7)-(30.1,20). 155 st. |
| `r27-text-a-space-b.pes` | `70c2c73817f116c07a28ea3334fce45e64788b74e2fdb497484c70f7529c5ba5` | Round 4 (2026-10-29, session pe-refs-4, PE-DESIGN 11.50, Windows 11 Pro 10.0.26300): New design (page as PE-DESIGN started: Hoop Size 100 x 100 mm `nv880e-S`), Text tool at its defaults (Lato, 10,0, line Not Sewn, Satin Stitch 800 RED), click at (10,20) mm, typed `A B`, Select. 22,30 x 15,90 mm, 364 st; Save As Ver.11.0 (#PES0110). Shows that a space is stored as a letter. |

## Round 3 (2026-10-28): the remaining object kinds

Made by the owner's PE-DESIGN 11.50 (Layout & Editing, restarted at 14:26) on Windows 11 Pro 10.0.26300, 2026-10-28, by black-box GUI use only (session pe-refs-3), one object kind per file so that every class PE-DESIGN writes from owner-drawn content has a sample. Common: App menu > New, Design Page 100 x 100 mm (grid 1 mm, Snap to Grid on) unless the row says otherwise; one object drawn with the tool named, attributes as the tool showed them (the Shapes tools remember the last attributes: each row records the Sew group); Select tool; Save As "Layout & Editing Ver.11.0 File (*.PES)" (#PES0110). Sizes and counts from the status bar. Tool tooltips (Home > Shapes menu): Outline row = Closed Straight Line, Closed Curve (shortcut X), Pencil(Closed Curve), Open Straight Line, Open Curve, Pencil(Open Curve); Rectangle/Circle/Arc row = Rectangle, Ellipse, Arc and String, Round Rectangle, Arc, Fan; Manual Punch row = Straight Block (Z), Curve Block, Running, Feed (B); Shapes = the gallery. The Small Text and Monogram tools offer only PE's built-in fonts and were not recorded; Motif Stitch and Decorative Fill use PE's built-in patterns and were skipped; a Manual Punch Feed-only design saves with no PEC section and was discarded. Text uses Lato (the repo's Lato-Bold.ttf installed per user). Class names seen (a plain scan of the object section): `CEmbLine` (the Outline, Rectangle/Circle/Arc and Manual Punch tools, every region type), `CEmbSpecialShape` (gallery heart; also in the applique), `CEmbNText` (vertical text), `CEmbDText` (Text Transform), `CEmbAText` (Fit Text to Outline, in the compat text-fit-* references).

| File | SHA-256 | Made as |
|---|---|---|
| ~~`o-applique-heart.pes`~~ | withdrawn 2026-11-14 | Removed (#1060): its heart came from PE-DESIGN's Shapes gallery, PE-DESIGN library geometry. |
| `o-arc-string.pes` | `b58402095d128cba8799e974d75823d1676df7bb969a9ce64e99456f82e60400` | Arc and String tool, circle dragged (10,10)-(40,40) mm, then "Choose Arc start point" click at 3 o'clock (40,25), end point at 12 o'clock (25,10). Line Running Stitch (red), region Not Sewn (Region) (the tool's attributes). 30,00 x 30,00 mm, 65 st. |
| `o-arc.pes` | `ef3d92b0c054dcf5deda04437060fa1d7b2a7c81313b25d4ffea620e7f57367a` | Arc tool, circle (10,10)-(40,40), start 3 o'clock, end 12 o'clock. Line Running Stitch (open arc, region greyed). 30,00 x 30,00 mm, 53 st. |
| `o-closed-curve.pes` | `71a4578f887efb09cea84177c098179aea005c4f05bf63d697c8a5f18d390f1d` | Closed Curve tool, clicks (10,10) (40,10) (40,40), double-click (10,40) (the curve bulges outside the points). Line Running Stitch, region Not Sewn (as the tool showed). Total 42,00 x 42,60 mm. |
| `o-closed-freehand.pes` | `4ee4413128a20f318cef8e3eeabc8b608840e4799525e0a58732330e581f20ff` | Pencil(Closed Curve), one stroke (10,10)->(20,8)->(30,10)->(40,14)->(44,24)->(38,34)->(26,38)->(14,32)->(10,22). Line Running Stitch, region Not Sewn. 33,20 x 29,80 mm, 97 st. |
| `o-fan.pes` | `b3bffac0bd71596f6b02dd547e7131974b1a9a2cde99fd6155ae5b58196ca8b4` | Fan tool, same circle and points. Line Running Stitch, region Not Sewn. 30,00 x 30,00 mm, 69 st. |
| ~~`o-gallery-heart.pes`~~ | withdrawn 2026-11-14 | Removed (#1060): its heart came from PE-DESIGN's Shapes gallery, PE-DESIGN library geometry. |
| `o-group-two-rects.pes` | `72107115b1342ed0a91f5d22ef9caf818d8b351615c2aee0ef9cef410272eb37` | Rectangle tool (its attributes now line Running Stitch red, region Not Sewn) dragged (10,10)-(30,30) and (40,10)-(60,30) (the second needs the tool picked again); Ctrl+A; Home > Edit > Group > Group. 50,00 x 20,00 mm, 90 st. |
| `o-hoop-100x100-s.pes` | `696e3f7651518f0da5460f0fd9fc24a9cad18e1a985b8ffcb46d919cf35e4482` | Page Size = Hoop Size, list (multi-needle hoops: 360x60 Wide Cap Frame, 90x80 Cylinder, 180x100 Border, 300x100 Border, 70x200 Sleeve, 44x38/50x50/70x41/33x75 Compact, diam. 100/130/160 Round, 260x160 nv880e-L, 150x150 nv880e-M, 100x100 nv880e-S) -> "100 x 100 mm nv880e-S". 767 st. |
| `o-hoop-130x180-single.pes` | `b072832a341530431e10cfae3b17b1cc223f1e4f22cc1ffa74fc564a7728054e` | Machine Type = first radio (single-needle icon); Hoop Size list then shows 100x100, 100x180, 130x180, 150x150 (Quilt Frame), 160x260 (Color LCD), 160x260 (Monochrome LCD), 180x300, 180x360, 200x200 (Quilt Frame), 200x300, 240x240 (Quilt Frame L), 240x360, 254x254, 272x272 (Quilt Frame LL), 272x408 -> "130 x 180 mm". 767 st. |
| `o-machine-single-100x100.pes` | `080a9843cdc22c076dae4c94b706da4b134b225c68196c10a0b05fbd440dccbf` | single-needle, Custom Size 100 x 100 mm. 767 st. |
| `o-manual-curve.pes` | `666a4204291deb3bde75cd7f7ae8a6c09695c15a8dca06001925d1e2dbbbdf25` | Manual Punch > Curve Block, the same six points. 40,00 x 20,00 mm, 379 st. |
| `o-manual-running.pes` | `81669eff727294184103262367f17e9f9c5bf74ce4e1167e7d8792fb4dc1cae8` | Manual Punch > Running: clicks (10,10) (30,20), double-click (50,10). Running Stitch (red). 40,00 x 10,00 mm, 29 st. |
| `o-manual-straight.pes` | `eca2d7c46edddaa6fe4b28c610aa7e82a13c5f5eb9b99799e838435183d1501d` | Manual Punch > Straight Block: clicks (10,10) (10,30) (30,10) (30,30) (50,10), double-click (50,30). Manual Punch tab shows Straight/Curve/Running/Feed; block = Satin Stitch (red, tool default), line Not Sewn. 40,00 x 20,00 mm, 379 st. |
| `o-open-curve.pes` | `1a1a2f91361fd9a5a2b933bc8d6be71f8bb37d5d044222437ec06b73d3d3c5ac` | Open Curve tool, clicks (10,20) (30,10), double-click (50,20). Line Running Stitch. 40,00 x 10,00 mm, 37 st. |
| `o-open-freehand.pes` | `8529eae490348821f90bf92ae5d20591d2484d6c44fc18c143706f6af94eafa2` | Pencil(Open Curve), one stroke (10,20)->(20,12)->(30,10)->(40,14)->(50,22)->(60,20). Line Running Stitch. 42,70 x 11,80 mm, 55 st. |
| `o-page-200x200.pes` | `de0109721f5d7cb40ee972183fe70b52eed256a8d85938cc4adea8e87091b357` | Custom Size Width 200, Height 200 mm. (The view re-zoomed to 55 %; the rectangle stays at the same page coordinates (10,10)-(30,30) of the larger page.) 767 st. |
| `o-region-concentric.pes` | `8cf5b99b1bf5b287709ebcd026b35572b795a5ca95bb2b8cb26e7473721f95d4` | Concentric Circle Stitch. 20,00 x 20,00 mm, 729 st. |
| `o-region-cross.pes` | `6482fbc1f7c1fdfaf5c9d51a384b48c6e5e6bed9bce27cd5d29b307310ee3f33` | Cross Stitch. 20,00 x 20,00 mm, 450 st. |
| `o-region-flexible-spiral.pes` | `69c21652f3ef948bcb24c5e31ccbcacfb56e5437ddcde36a03c0f739cbd8aeba` | Flexible Spiral Stitch. 20,00 x 20,00 mm, 175 st. |
| `o-region-net-fill.pes` | `d7a0623ea71e9888d6b6aca0b51446ed7c05e0b3ab5fe4f93036a00130182692` | Net Fill Stitch. 20,00 x 20,00 mm, 251 st. |
| `o-region-piping.pes` | `9b94598a1e672cd0dc4b7cdeddfe2ba7db5d6af45ddf9fc9f6297f46195aea96` | Piping Stitch. 20,00 x 20,00 mm, 203 st. |
| `o-region-radial.pes` | `a5f48eea40e075637cab26682beff921f210bca1f7a91432d4b52a262bd6a0c8` | Radial Stitch. 20,00 x 20,00 mm, 994 st. |
| `o-region-spiral.pes` | `8d0ffa08cf6d6972ac455098ef296862844682775e75cf08e2f57175957a6cc9` | Spiral Stitch. 20,00 x 20,00 mm, 500 st. |
| `o-region-stippling.pes` | `51d9bd2f74fcf358fbc02a9f54b14b52ed860b8fe0f6a61812dfdb1e2926bdf7` | Stippling Stitch. 20,00 x 20,00 mm, 108 st. |
| `o-region-zigzag-net.pes` | `669f6dc460ec56fea9fc43aba50a41dbad9cc67f210318dcd23c103b5146f935` | Zigzag Net Fill Stitch. 20,00 x 20,00 mm, 767 st. |
| `o-rounded-rect.pes` | `5c4094062c3ca2c23ed374c63fbd24645b8436fd7df30b549a390ab9d7887ca6` | Round Rectangle tool dragged (10,10)-(40,30). Line Running Stitch, region Not Sewn. 30,00 x 20,00 mm, 56 st. |
| `o-text-transform-1.pes` | `4e7d824f7dc8840d27bd32f6db2074d571ac9d1cc15ac3df8c6e5688c8c7600e` | that text, Text Attributes > Transform ticked, first preset (row 1 left, the arched "ABC"). 23,00 x 8,00 mm, 348 st. |
| `o-text-vertical.pes` | `f72efd958c8b031c44fd3314fbafd62669abb41fcec2d56511fcc7fe0a6b2e45` | the same text, Transform unticked again, Direction = vertical button (second). 8,00 x 24,40 mm, 407 st. |
| `o-text-10-letters.pes` | `b873248689736452a1a09915048a5635c38c7d1feb329cb0c607a691853a2e3b` | Home > Text > first tool "A" (Lato, size 8.0, line Not Sewn, region Satin Stitch, 800 RED), click (10,50) mm, ABCDEFGHIJ, Direction horizontal; ten letter records in one text. 69,30 x 8,40 mm, 1141 st. |

## Round 4 (2026-10-29): owner-drawn programmable-fill and motif patterns

Made by the owner's PE-DESIGN 11.50 (Layout & Editing) on Windows 11 Pro 10.0.26300, 2026-10-29, by black-box GUI use only (session pe-refs-4). The patterns were drawn in Programmable Stitch Creator (Option menu) and saved as `owner-v.pas` (Fill/Stamp, Default Size 7.5 mm) and `owner-v.pmf` (Motif, Default Size 7.5 mm), both kept in `compat/reference/pes/` (ASCII: `@VERSION 10000`, `@SYSENV 1`, `@TILE 1` + `@TILELINE 0 3 0 0 512 768 1024 0`, or `@LINE 3 0 512 512 768 1024 512`). App menu > New (100 x 100 mm), one rectangle, Save As "Layout & Editing Ver.11.0 File (*.PES)" (#PES0110). The same three files are the compat references of `pes-objects-owner-patterns`. Test `round_3_object_files_read_their_stitches_and_stop_cleanly` covers them too.

| File | SHA-256 | Made as |
|---|---|---|
| `obj-progfill-owner-v.pes` | `18b1f690af97c749f05b0a436f9f13aecf62555f255f1a96d6d2f768b4ec0e28` | Rectangle tool dragged (20,20)-(50,40) mm, line Running Stitch (red, pitch 2.0), region Prog. Fill Stitch with the owner-drawn pattern owner-v.pas (compat/reference/pes; a V across the 8 x 8 cell, made in Programmable Stitch Creator), 7.5 x 7.5 mm, Direction 0, Offset row 0 %, Base Sewing off; region density 4.5, direction 45, under sewing off, 007 PRUSSIAN BLUE. 30,00 x 20,00 mm, 641 st. |
| ~~`obj-motif-fill-owner-v.pes`~~ | withdrawn 2026-11-13 | Removed from the repository: the capture's header motif list also held a PE-DESIGN built-in motif, which is Brother content (PE-DESIGN keeps a Pattern2 for every motif fill and fills it with its built-in motif, even under "Pattern1 only"). Replaced by the two clean captures below. |
| `motif-fill-clean-p1.pes` | `582bd05cf3045040a8f16c256dc50c1a37828a5d206c83f48e9c1f37aba5ca50` | Round 13 (the coordinator's capture, 2026-11-13, PE-DESIGN 11.50, Windows 11 Pro): a 40 x 40 mm square (Rectangle tool); line Motif Stitch with the project's `psc-motif-zigzag-3pt.pmf` at 5,0 x 8,0 mm, Spacing 2,0; region Motif Stitch, Pattern1 the same motif at 8,0 x 8,0 mm, Pattern2 replaced by the project's `psc-motif-2pt.pmf` (so no built-in motif is listed), arrangement "Pattern1 only". 186 st in PE-DESIGN. The header lists only the project's two motifs. |
| `motif-fill-clean-p1p2.pes` | `1d16c0309eec8f6fd25ac3fd591d8974de247b67c5671515c7df3dee4aacd8ad` | Round 13, the same with the arrangement "Pattern1 and 2". 194 st. |
| `obj-motif-line-owner-v.pes` | `60cfe1373c22f310f7b467a514f6029a639618984d92f94ab5ad5394cdbef22b` | The same rectangle, region Not Sewn, line Motif Stitch with owner-v.pmf, 5.0 x 5.0 mm, H/V-Arrange first buttons, Offset 0.0, Spacing 0.0, Run pitch 5.0, Start/End Type Feed. 30,00 x 20,00 mm, 47 st. |

**Round 4 machine exports (#135):** made by the owner's PE-DESIGN 11.50 (Layout & Editing) on
Windows 11 Pro 10.0.26300, 2026-10-29 11:43-11:47, by black-box GUI use only (track
pe-refs-4). Design Page 100 x 100 mm. Text tool "S", size 20.0, region Satin Stitch, 800
EMBROIDERY RED (14,00 x 21,10 mm, 278 st); Rectangle (60,60)-(70,68) mm, line Not Sewn,
region Fill Stitch, 007 EMBROIDERY PRUSSIAN BLUE (10,00 x 8,00 mm, 197 st). Sewing order: the
S, then the rectangle. Save As PES, then App menu > Export... to each type, no options dialog,
no warning. Three of the eleven files are kept here for the readers' tests:

| File | SHA-256 | Made as |
|---|---|---|
| `sample-basic.csd` | `b405685a744df0dd4395279c561822aea4010d8460c7887404aaa9b7e52b841b` | Export, type Singer (*.CSD). |
| `sample-basic.dst` | `c29378ba32fda42d1b5e5499cdec5c4594ab70ac49dc5fa69ee710000c8b855b` | Export, type Tajima (*.DST). |
| `sample-basic.shv` | `508c945b5a4e9047da8cda1fd69240a6add83cd9183d1460bffdb3ba0c9dc727` | Export, type Husqvarna/Viking (*.SHV). |

## Round 4 (2026-10-30): Manual Punch blocks

Made by the owner's PE-DESIGN 11.50 (Layout & Editing) on Windows 11 Pro 10.0.26300,
2026-10-30 08:58-09:06 (m09-m11 later the same day), by black-box GUI use only (session pe-refs-4, round 4's capture log). App menu
> New (Hoop Size 100 x 100 mm, grid snap on); Home > Shapes > Manual Punch row (first icon
Straight Block, second Curve Block); clicks at page mm from the top-left. A block object comes in
with line Not Sewn, region Satin Stitch, 007 PRUSSIAN BLUE (the session's last colour), under
sewing on, density 4,5, Direction "Manual straight line" (angle greyed: the block sets it), Half
Stitch on, Running Stitch Path first. Save As "Layout & Editing Ver.11.0 File" (#PES0110).
`m06-straight-1-blue` (m01 with 007 chosen explicitly) equals m01 but for the PEC label and is
not kept.

| File | SHA-256 | Made as |
|---|---|---|
| `m01-straight-1.pes` | `0197c470e310435900c7810747d7f97bd41268ecfe887bfb9f0ffe9f95419ec6` | Straight Block: clicks (10,10) (10,30) (30,10), double-click (30,30). One block. 20,00 x 20,00 mm, 208 st. |
| `m03-straight-3.pes` | `2f6a091c27ee6a13511540200e5410e5aa463c0f4a2b5017bf53b14da9b39244` | Straight Block: clicks (10,10) (10,30) (30,10) (30,30) (50,10) (50,30) (70,10), double-click (70,30). One object, 60,00 x 20,00 mm, 550 st. |
| `m04-straight-1-fill.pes` | `d169dcc1cfbd659d455496468fb8f3cd293a02ab76980bd2e199f8c1e4e7be53` | m01, then region Fill Stitch (the block offers the whole region list): under sewing on, density 4,5, direction greyed at 45, step 4,0, frequency 30 %. 652 st. |
| `m05-straight-1-density.pes` | `23ebb6122a4c4778f742461ccbb2d5b4bd83d4865070a44cf57cc00c49a271f5` | m01 (Satin), then density 6,0. 238 st. (Saved as "4,5.pes" by a missed click and renamed; content unchanged.) |
| `m07-curve-1.pes` | `b970e05335f2d22649b139b7491ba5370b25209e7eed9bec5f04ccb631cab6c2` | Curve Block, the same four clicks as m01. 20,00 x 20,00 mm, 208 st. |
| `m08-straight-1-line.pes` | `4f46a7198c97f11a443e139eae84b6316ea600a5e870a7bb31de6f441443a553` | m01, then line Running Stitch (run pitch 2,0 mm, run time 1, 007). 253 st. |
| `m09-curve-bent.pes` | `b7ddf48a62724dd7e5d636eea213a431d5236195e79c6d9b43953d45afc69a75` | Curve Block: clicks (10,10) (10,30) (30,20) (30,40) (50,10), double-click (50,30); the block sags through the middle pair. 40,00 x 30,00 mm, 421 st. |
| `m10-curve-uneven.pes` | `ba4087375f473715217951a32b1b1f54f9cdcaac387e76dcb96c213c06124142` | Curve Block: clicks (10,10) (10,30) (20,25) (20,45) (60,10), double-click (60,30); unequal chords. 50,80 x 35,00 mm, 529 st. |
| `m11-curve-4.pes` | `ce26049eadee96817d4bb716df8dc7bc2af4597a7762e7afc934820cef72d837` | Curve Block: clicks (10,10) (10,30) (25,25) (25,45) (45,25) (45,45) (60,10), double-click (60,30); four lines. 50,00 x 37,00 mm, 537 st. |

## Round 4 (2026-10-30): Applique Wizard

Made by the owner's PE-DESIGN 11.50 (Layout & Editing) on Windows 11 Pro 10.0.26300,
2026-10-30 09:06-09:10, by black-box GUI use only (session pe-refs-4, round 4's capture log). App menu
> New (Hoop Size 100 x 100 mm); Rectangle tool (10,10)-(30,30) mm at the tool state (line Not
Sewn, region Satin Stitch, Direction 45, under sewing on, density 4,5, 007), 280 st; Ctrl+A,
Home > Wizards > Applique Wizard; Save As "Layout & Editing Ver.11.0 File" (#PES0110); Ctrl+Z
back to the square before the next variant. Dialog defaults: Applique Material Yes / Running
Stitch, Applique Position, Tack down Yes / V Stitch, Covering Stitch Zigzag 3,0 mm / 4,5
line/mm, Output Pattern Add, Distance 3,0 mm.

| File | SHA-256 | Made as |
|---|---|---|
| `a01-rect-applique.pes` | `5d93cddbf8afad0f280a7f3b4a931a8473fe7c2692763c7ef017232e75e95931` | Dialog defaults: four Sewing Order items (square; material; position; covering group with the tack-down). 32,60 x 32,80 mm, 1063 st; PEC colours 1, 62, 63, 64. |
| `a02-applique-cutting.pes` | `15ffc177e7850e94283a37f6d996fe44927374d6b674e1a0bb1295ab3c5e6c3b` | Applique Material Cutting: 1390 st; PEC colours 1, 65 (four cutting blocks), 63, 64. |
| `a03-applique-no-tackdown.pes` | `fee6f2f751623e9ebeb3701ac6c5e00e1b97bd50b51d3775d178b005f28a5b7e` | Tack down No: 970 st; PEC colours 1, 62, 63, 64. |
| `a04-applique-cover-4mm.pes` | `d446d00ea341cdcd5953d0c86530e77ef810391ac513e51d35300196b8d3c021` | Covering width 4,0 mm: 34,60 x 34,70 mm, 1107 st. |
| `a05-applique-replace.pes` | `ed820109683b353ef717c697dcffcc280a15598a7399b7e56dcbc55226121ad1` | Output Pattern Replace (distance greyed): the square replaced; three items, 23,00 x 23,00 mm, 537 st; PEC colours 62, 63, 64. |
**Oversize design (#125, round 4 group F):** made by the owner's PE-DESIGN 11.50 (Layout &
Editing), Windows 11 Pro 10.0.26300, 2026-10-29 12:03-12:04, black-box GUI use only (track
pe-refs-4). Design Settings > Page Size = Hoop Size 100 x 100 mm (multi-needle, nv880e-S); the
twelve-swatch design, Ctrl+A, Home > Size... 150 %: 129,00 x 105,00 mm, 17225 st, outside the
page. Save As PES: "Please keep all designs within the Design Page. Saved data cannot be sent
to the original card or the embroidery machine. Do you want to save the data anyway?" -> Yes.
The PEC section is the empty form: no colours, a 13-byte stitch block with the design's size.

| File | SHA-256 | Made as |
|---|---|---|
| `oversize-hoop100.pes` | `09f0af888e86dbce3a4e7ff79a9010434dcc6b48c27bb06669a1b38b499a17c5` | as above |
**Linen thread sample (#119/#136):** made by the owner's PE-DESIGN 11.50 (Layout & Editing),
Windows 11 Pro 10.0.26300, 2026-10-30, black-box GUI use only (reference session). New (Hoop
100 x 100), Rectangle (40,40)-(60,60) mm, line Not Sewn; Color tab, BROTHER EMBROIDERY, row
307 LINEN; region Fill Stitch at defaults (under sewing on, Medium, density 4.5, 45 degrees,
step 4.0, frequency 30 %); 20,00 x 20,00 mm, 707 st; Save As PES (#PES0110). Thread table:
"307" / PEC index "42" / EMBROIDERY, RGB (254, 227, 197), plus the unsewn line slot 800.

| File | SHA-256 | Made as |
|---|---|---|
| `thread-307-linen.pes` | `11d775bda5f1696257c8ec2ee2a6dab5b9853118c2c3c4a349aeff9238ca8ef2` | as above |
## Round 5 (2026-10-30): Convert to Blocks

Made by the owner's PE-DESIGN 11.50 (Layout & Editing) on Windows 11 Pro 10.0.26300,
2026-10-30 10:37-11:04, by black-box GUI use only (session pe-refs-4 for the pattern-formats
track, round 4's capture log "STROKE-WISE LETTERING", round 5's text-stroke captures). App menu > New (Hoop Size
100 x 100 mm); Home > Text > Text tool, Lato (Lato-Bold.ttf from crates/openstitch-design/fonts,
installed for the user), size 20,0, line Not Sewn, region Satin at the text defaults (under
sewing off, density 5,0); click (50,50) mm; the letter; Select tool; Text ribbon > Convert to
Blocks; Save As "Layout & Editing Ver.11.0 File" (#PES0110).

| File | SHA-256 | Made as |
|---|---|---|
| `stroke-blocks-H.pes` | `94999fe43ef0a5e76196f0d3f37d5092fc5aedd80dcfe0fb1781248b5b03dbe1` | H converted to blocks: one group of five blocks and three running pieces; 285 st (as the text). |
| `stroke-blocks-A.pes` | `cf6684d3ffbeb6d04d3b795a257fc8a3263f3bfa00cb0438b810b94dbf1e3831` | A converted to blocks: one group of five blocks and four running pieces; 293 st (the text: 290). |

**Ver4 Option Color (#136):** made by the owner's PE-DESIGN 11.50 (Layout & Editing), Windows
11 Pro 10.0.26300, 2026-10-30, black-box GUI use only (reference session; owner's round-4 log,
section "2026-10-30 11:51"). Hoop Size 100 x 100 page; 29 Fill squares of 10 mm, under sewing
off, line Not Sewn, coloured with the Thread Chart "Ver4 Option Color" rows 1-29 in order (each
square's colour checked); 85,00 x 70,00 mm, 5302 st; Save As PES (#PES0110). Thread records:
empty code, RGB on a 0/85/170/255 grid, type 0x0B, id string 66-94 (row n = 65 + n), brand
"Option Color", empty chart; plus the unsewn line slot 800 EMBROIDERY.

| File | SHA-256 | Made as |
|---|---|---|
| `ver4-option-color-29.pes` | `d61a1be3dcb661807da5c42e87752263a0c0b88431b8488117957a8e07829ffd` | as above |

## Round 6 (2026-10-31): groups

Made by the owner's PE-DESIGN 11.50 (Layout & Editing) on Windows 11 Pro 10.0.26300,
2026-10-31, by black-box GUI use only (session pe-refs-4). App menu > New (Hoop Size 100 x 100
mm); Rectangle tool at its default (line Zigzag Stitch, region Fill Stitch, red); 10 mm squares,
each 12 x 12 mm with its zigzag; a pair selected with a drag box, Home > Group > Group; Save As
"Layout & Editing Ver.11.0 File" (#PES0110).

| File | SHA-256 | Made as |
|---|---|---|
| `group-partial.pes` | `2013163b265da93528f741bfc576b6b66bbc6f42a040fc6959031c60d2d4361d` | Squares at (10,10), (30,10), (50,10) mm; the first two grouped. 52 x 12 mm, 1257 st (the group 32 x 12 mm, 838 st). |
| `group-two.pes` | `cbb8d591820f18acbdd93c4863a8fda2acb985e108df7e8abf96dced5756b482` | group-partial with a fourth square at (70,10) mm; the last two grouped as well. 72 x 12 mm, 1676 st. |
| `608-groups-three.pes` | `99b642266081038d659d6deb0fd6b2efa2f9c3ccdd661170d1d60078a939ff2e` | Round 12 (2026-11-13, the coordinator's capture for #608, same PE-DESIGN and OS): a new page (the hoop page about 180 x 130 mm), six squares drawn with the Rectangle tool, about 10 mm each and about 15 mm apart in a row, at that PC's defaults (region Fill, line Zigzag); grouped 1+2, then 3+4, then 5+6 (each pair with a selection box, Home > Group > Group). |
| `608-groups-3-and-2.pes` | `44ba625b791eda30ebffa890e4cba689e5733bd94a13ac288a75caa308909de0` | Round 12, the same way: five squares; 4+5 grouped first, then 1+2+3. |
**Machine type and hoop (#125, round 6):** made by the owner's PE-DESIGN 11.50 (Layout &
Editing), Windows 11 Pro 10.0.26300, 2026-10-31 11:42-12:08, black-box GUI use only (reference
session; owner's round-6 hoop capture log). One design for every file: New, Rectangle
(40,40)-(60,60) mm, line Not Sewn, Fill Stitch at defaults, 800 EMBROIDERY RED, 20,00 x 20,00
mm, 707 st; only Design Settings changed (Page Size = Hoop Size, the machine type and hoop
below), then Save As "Layout & Editing Ver.11.0 File" (#PES0110). Every file reopens with the
settings it was saved with. The 100 x 180 capture (PE-DESIGN saves that hoop only as
#PES0113) stays in the owner's notes; the other eight were added for #322 (2026-11-04). The
Border and Round files were saved after the nv880e-L one and keep its hoop fields; on the two
Custom Size pages ("Use smaller section sizes" off) the greyed hoop is the last one chosen.

| File | SHA-256 | Made as |
|---|---|---|
| `hoops/s-100x100.pes` | `6d70322c9d71e9e5cb3d4c881b1c9dd12ff2703bd6fbcce5025b0fb190dc144d` | single-needle, Hoop Size 100 x 100 mm |
| `hoops/s-130x180.pes` | `1344a1239f44a3d582e11d352d02f705cbe2fe964c3381793b930109b8a91ee2` | single-needle, Hoop Size 130 x 180 mm |
| `hoops/s-130x180-rot90.pes` | `6bcd6bd198a407124539fbf3c2ebef40c47743097bc2e2ad97a9df3f973499e7` | single-needle, Hoop Size 130 x 180 mm, Rotate 90 Degrees |
| `hoops/s-160x260-color.pes` | `64abe7460e03a8941f68b3e94385f89e9e66aeb1b6bf02f0e267d161c8ce2476` | single-needle, Hoop Size 160 x 260 mm (Color LCD) |
| `hoops/s-200x300.pes` | `d805489869cb964aae208e5ec2a6a7c44829854846966d398367c28568f1c90e` | single-needle, Hoop Size 200 x 300 mm |
| `hoops/s-240x360.pes` | `04fc458681e36cc41e75ac9f57ed4854486fb8bfcb08f1940a50107f6c1bbdb3` | single-needle, Hoop Size 240 x 360 mm |
| `hoops/m-100x100-S.pes` | `4147ad67abb073f9314c585a099e2090854102f2c6bbaa4fd1201fa79156da09` | multi-needle, Hoop Size 100 x 100 mm nv880e-S |
| `hoops/s-180x300.pes` | `db77047a0ea93e2502783a639c66465c94f6ba0d6d99122d651d6de8f2b47622` | single-needle, Hoop Size 180 x 300 mm |
| `hoops/s-150x150-quilt.pes` | `ab2df7c003661a82ba3529bf00e8f6fb247a77e438bffa82334ec0a142b4ed50` | single-needle, Hoop Size 150 x 150 mm (Quilt Frame) |
| `hoops/m-150x150-M.pes` | `e753d2ddf09aa3b8b5c2dd798800b35189d6082cc1a746ae9a9732ad44d060ec` | multi-needle, Hoop Size 150 x 150 mm nv880e-M |
| `hoops/m-260x160-L.pes` | `889f7cc916edec3aea1f3f76a77cad96e7590c81cc77ee1b6e07c8a2656552d0` | multi-needle, Hoop Size 260 x 160 mm nv880e-L |
| `hoops/m-180x100-border.pes` | `d14d311021a9f83756b318d97d90cd3f93f6208ca0be686e825e095dfab469d0` | multi-needle, Hoop Size 180 x 100 mm (Border Frame) |
| `hoops/m-round-100.pes` | `f42775fed6d6371d895a8ad6a66fb02aa7340065f5ca4c629a1c4d45aa9304a2` | multi-needle, Hoop Size ø100 mm (Round Frame) |
| `hoops/s-custom-300x200.pes` | `b3cd7307bfc8435bc75fadd5b8202b69024b0ecc5b3e38efe4afd68dcf3e0d01` | single-needle, Custom Size 300 x 200 mm, Section Size 90 x 90 (100 x 100); preview 4 x 3 sections |
| `hoops/m-custom-300x200.pes` | `35047b0339cae36f6b6ab5dd3e84cb86f0b416d5f4c232121b5b57a08e1539e9` | multi-needle, Custom Size 300 x 200 mm, Section Size 290 x 190 (300 x 200); preview 2 x 2 sections; greyed hoop ø100 Round |


## Captures of 2026-11-07 (`captures-2026-11-07/`, coordinator, black-box GUI use only)

Made by the owner's PE-DESIGN 11.50 (Layout & Editing) on Windows 11 Pro on 2026-11-07, at the owner's request, for issues #374, #349 and #373. The full steps and findings are on those issues. Inputs are OpenStitch's own test designs; the outputs are PE-DESIGN's saves and exports of them, kept here so writers can be checked byte for byte. No Brother built-in font output is included.

**`pes-versions/` (#374):** the sources (`props` = compat/reference/pes/pes-design-properties.pes, `text` = r14-text-ab.pes, `two` = r16-two-rects.pes, `rot` = hoops/s-130x180-rot90.pes, `page` = o-page-200x200.pes) opened in PE-DESIGN, then App menu > Save As with each "Layout & Editing Ver.N.0 File (*.PES)" type. Every save below Ver.11.0 warns "This embroidery data will be saved in the old file format. OK to save the data as stitch data?" (OK). Designs with editable objects (text, two, page) offer only Ver.11.0 to Ver.6.0, so they have no v5 or v4. Stitch-data designs (props, rot) offer down to Ver.1.0.

**`machine/` (#349):** `c17.pes` and `long.pes` are OpenStitch's inputs (see #349). They were opened in PE-DESIGN, then App menu > Export… with "Husqvarna/Viking (*.SHV)" and "Singer (*.CSD)". No warnings. With 17 colour blocks PE-DESIGN writes **two** CSD files, `pe-c17_A.Csd` and `pe-c17_B.Csd`.

**`miter/` (#373):** a Zigzag Stitch line (Under sewing on, width 2.0 mm, density 4.5, Half Stitch on, colour red), saved as Ver.11.0. `miter-a-*` is the 90-degree L of compat/reference/stitches/line-zigzag-default.pes; `miter-b-*` is a V drawn with snap at (10,20), (50,20), (15,40) mm (29.7 degrees). The file names give Sharp Corners (sc1 or sc2) and Miter Limit in mm. The field ranges from 0 to 50 mm, and each Sharp Corners button keeps its own value. Stitch counts and sizes are on #373.


| `pes-versions/` file | bytes | SHA-256 |
|---|---|---|
| `page-v10.pes` | 21403 | `68a0f7d6b453588a6480f91270ef55c6b3f27878d2be3dc5c44d8141333dc82f` |
| `page-v7.pes` | 21348 | `403688b88ac2376217c679cb30d6b5cf035e7101925e499ee035ef6bb8ffbfe6` |
| `page-v8.pes` | 21358 | `6dfe894ad45cd3505990a23aca70d92179394fb69fd7ed66710c2a93dcdcc2c4` |
| `page-v9.pes` | 21387 | `3de7282e921d77caf4193977daa12fe8a576da39f92ececb62dd8b21079e21ac` |
| `props-v10.pes` | 6867 | `2989d03ae9982b7167cbf89adcfd6a3e79ef2c2875eeeab9e88682f62b091e63` |
| `props-v4.pes` | 6577 | `fc1b73178ce795d928ab9b58bab2bc36e5b9cd8755900d9620d1636abf798768` |
| `props-v5.pes` | 6807 | `aa71515bd9d4fdbbb5149c4514209e7c98c41cc314288fd6d86a6bf0ed44c49a` |
| `props-v7.pes` | 6824 | `1a6f2a3d6c0d98e2669e3994692836622e040cdf26754160924308533b931696` |
| `props-v8.pes` | 6834 | `98cf2a4da4162521ae4963a8da73b36c22b38d3f617e90f06939c3319a159dad` |
| `props-v9.pes` | 6855 | `6dd77cfb87f2ff157f9792147c822705511706c82fdd2f4b7a418a13f7ebb20e` |
| `rot-v10.pes` | 5824 | `4ef6a2995b703867f9a5df87fc5ec75ddbf843e520fd5a6247e1b578de1c1635` |
| `rot-v4.pes` | 5520 | `88c7390b83f3660aea5fe57c84371f7d61d28d8a1c8117cccb0d4637cc0211ee` |
| `rot-v5.pes` | 5756 | `4dc5bb05674242e2457bf6961860fd096a2d0339c32ee7f2783961b0a4c14e52` |
| `rot-v7.pes` | 5773 | `89e8f9b2639a29cc688939b82004e975cf6075fcee2395c16b0e1cf7fbf03d58` |
| `rot-v8.pes` | 5783 | `5d653b13a80767f7569671080cf873cca9c59bc6ff989c1d13625d4c0cd6c412` |
| `rot-v9.pes` | 5812 | `4dfe56479db0bb0c8a3b83549b9df6e3352a6c8b090e111077d1d424d48368b7` |
| `text-v10.pes` | 19055 | `c984f8190ed692027a7b225e1f8a0dcd79bfab0ea1bf8f822740a2a84f78d191` |
| `text-v7.pes` | 18996 | `b5fffdc8ac23787479b7de36a7d0b8146fb7d868791531084b5e25faaa8f9e33` |
| `text-v8.pes` | 19006 | `622cf7db0add15da23e3da14b035f75a415c2fcd9152fa497b6fba4a4b2730d3` |
| `text-v9.pes` | 19035 | `0c2b079840d479d471756d24579cebe9c14ba0f6852b456989b6ba37372c9178` |
| `two-v10.pes` | 26090 | `7281d1461569d40f27e1201e636398ac85b3b6e2e50157ffe0b571686a5a0504` |
| `two-v7.pes` | 26003 | `11e27739aebd8228114fd3bea8aefa3764a6c9f9a7f655bfd364642c0d1b8def` |
| `two-v8.pes` | 26013 | `f19454ccff1bd591e94a23d0f20fcc99da4ffcee203ab90cecb52741d8d451df` |
| `two-v9.pes` | 26054 | `e2036977d316939875772334bd95584a4c1ae963c2808f17925be17e4f6f2c73` |

| `machine/` file | bytes | SHA-256 |
|---|---|---|
| `c17.pes` | 19273 | `9004d5b1af9431ed310be0cc3aa1af55a6463ac98e6abe9abef3637352205823` |
| `long.pes` | 2891 | `55ac558f4bf5893e4f6764b57ca76778c75fecb4872a588fc2c64eddebb6aa2b` |
| `pe-c17.SHV` | 6472 | `48f6c6a2574209db442d327d1c981bb4e19206fa6f1578df549901f17fb74422` |
| `pe-c17_A.Csd` | 4545 | `75df858ca16b042ebb61674e73fcee06436d4cfb25f1a958c8b40bf49c58be91` |
| `pe-c17_B.Csd` | 1038 | `ecc16c86dbaa9598ea5cec2a7eb4c053a2099027dca1f4b8548e5e3f9ecdc8e2` |
| `pe-long.SHV` | 1091 | `22fd9e90b5ca7a9ab78f9ddb34eea9f80f33d9d3134a108417bb37c6434335fd` |
| `pe-long.CSD` | 807 | `5035316851b094de574881c09121af7fead0daa4d2b3e9fe92739c06880d3ea3` |

| `miter/` file | bytes | SHA-256 |
|---|---|---|
| `miter-a-sc1-m0.pes` | 22007 | `fe6f6bba47419b99050c448c99b39b46ab817f9c7256f4c68720386e54aafa3c` |
| `miter-a-sc2-m0.pes` | 22007 | `5025a915973e7e6c310c198e4354edf3df3493d5d5a4f3bb44f9e0888b186443` |
| `miter-a-sc2-m7auto.pes` | 22007 | `07948534e6fbb08990d3cb84305954185299f0af6e80bdadaf4bac29199ab940` |
| `miter-b-sc1-m0.pes` | 7048 | `ac24934929e35759028f1718d92ee226022fe6bc1853ec82b2bbf40a59a7dafd` |
| `miter-b-sc1-m1.pes` | 7066 | `1f6d4473f35c07be4f35ddf9714ae292da78349ef600431e4b94a4628bb6bdf6` |
| `miter-b-sc1-m3.pes` | 7086 | `da775297d88f8adf11d3fafc09768f0378c266d42df314244fe9b782009efed5` |
| `miter-b-sc1-max50.pes` | 7090 | `d9e3a00e3a687a63302ee820ae1e42b97ff0fa3fdff9dcfea8b7f45ad8c68811` |
| `miter-b-sc2-m0.pes` | 7058 | `4037f3ee2198c54dc0238a49cea2de63dda4675121af35e545c7363a56fc04ac` |
| `miter-b-sc2-m3.pes` | 7118 | `790e55c4b8080cdd8ccc5ad8aa5007f16431118c888ee0ed883039d19ee65763` |
| `miter-b-sc2-m50.pes` | 7118 | `549457799cc5f6f9c8405b0e4b1d4c8c7d0d1165c1b047930a717ad6e28958c5` |

## Captures of 2026-11-13 (`captures-2026-11-13/`, coordinator, black-box GUI use only)

Made by the owner's PE-DESIGN 11.50 (Layout & Editing) on Windows on 2026-11-13 (capture round 11), for issue #349. The input is OpenStitch's own test design; the output is PE-DESIGN's export of it, kept here so the SHV writer can be checked byte for byte.

**`machine/` (#349):** `moves.pes` is OpenStitch's input, made with `stitchtool exec moves.json` (the script is on #349): PES version 6, stitches only, all 005 SILVER; 16 running-stitch dashes 2 mm long, two per row on rows y = 10 … 80 mm, a trim before each of the 15 moves (35.3 to 70.3 mm, along the rows and diagonally between them). It was opened in PE-DESIGN with no warning (status bar 74.00 x 70.00 mm), then App menu > Export… > "Husqvarna/Viking (*.SHV)" as `pe-moves.SHV`, with no warning or dialog.

| `machine/` file | bytes | SHA-256 |
|---|---|---|
| `moves.pes` | 2471 | `554a52d1d6db622c201a6aa960f5d6712eb3adaa658a5703629072f154deb754` |
| `pe-moves.SHV` | 3154 | `3e0ba37103c3510dc25f4a964bb0c9ce265a62b58cae0f59445983515674c3d9` |
