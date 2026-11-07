# Garment mockups

The garment pictures behind `design.mockup` and File ▸ Export ▸ Garment Mockup (#454): a
T-shirt, a cap and a tote bag, each a plain white garment with no print, logo or label. They
were generated for OpenStitch with Higgsfield (GPT Image 2.6, 1:1, on 2026-11-07) under the
project owner's account, then cut out with Higgsfield's background remover. The prompts:

| File | Generation job | Prompt |
|---|---|---|
| `tshirt.png` | `d859d786-dfa9-4139-9ddb-c52374eca57b` | Product flat-lay photo of a plain white cotton crew-neck T-shirt, front view, perfectly centered and symmetrical, laid flat, no print, no logo, no label, soft even studio lighting with gentle fabric folds, on a seamless plain light grey background, orthographic top-down view |
| `cap.png` | `bdb55e21-5db7-45f2-a792-126b3d8b7a6b` | Product photo of a plain white six-panel cotton baseball cap, straight-on front view showing the flat front panel centered and symmetrical, curved brim facing the camera, no logo, no print, no label, soft even studio lighting, on a seamless plain light grey background |
| `tote.png` | `95c5b4b0-9725-4788-9795-32a3f5eaec6e` | Product flat-lay photo of a plain white cotton canvas tote bag with two handles, front view, perfectly centered and symmetrical, laid flat, no print, no logo, no label, soft even studio lighting, on a seamless plain light grey background, orthographic top-down view |

`higgsfield-jobs.tsv` holds the background-removal job of each file, which took the generation
job above as its input. No reference images were used. The cut-outs (1024 px RGBA) were
turned into what is stored here by a one-off script: luminance only, scaled so the garment's
97th-percentile brightness is white, kept with the alpha channel as grey + alpha, and resized
to 768 × 768 px (Lanczos). The engine tints that grey with the fabric colour.

Scales and placements (`crates/openstitch-engine/src/mockup.rs`) come from real garment sizes
measured on the cut-outs' masks: an adult M T-shirt 520 mm across the chest (636 px of 1024),
a cap front 115 mm from the brim seam to the button (550 px), a tote bag 380 mm wide (612 px).
