# Command icons

One SVG per ribbon icon, rendered by `crates/openstitch-app/src/icons.rs`. They were generated
for OpenStitch with Higgsfield (Recraft V4.1, `model_type` `utility_vector`, 1:1, 1k, white
background), one request per icon, with the colours listed in `prompts.tsv` and this prompt:

> Chunky flat vector UI icon of {subject}. One single bold silhouette filling 80 percent of the
> square, simple geometric shapes, thick dark charcoal outline, no tiny details, no text, no
> shadow, no gradient, plain white background, readable at 16 pixels.

(The six icons of the approved style sample, folder, disk, scissors, needle, hoop and zoom_in,
used the same words in a slightly different order.)

The 102 icons from `connect` on in `prompts.tsv` were added on 2026-10-30 and 2026-10-31 in
three rounds for commands that showed another command's icon (docs/research/asset-audit.md), with the white
background passed as `background_color` and the colours as `colors`; their Higgsfield job ids
are in `higgsfield-jobs.tsv`. `stitch_factory`, `view_solid` and `import_svg` are second
attempts. In 16 of them (`csv`, `fabric`, `measure`, `new_folder`, `thread_chart`,
`thumbs_small`, `wireless`, `add_to_design`, `attribute_sets`, `image_delete`, `image_html`,
`image_modify`, `image_paste`, `network`, `region_reset`, `stitch_simulator`) the fills the snap
left alone were set to the nearest palette colour by hand.
`assistant` (View ▸ Show/Hide ▸ Assistant, B17) was added on 2026-11-04 the same way (2.5 credits); it
needed no hand fixes after `clean_icons.py`. `attach` (a paperclip) and `close`, for the Assistant
panel's buttons and attachment chips (#399), were added on 2026-11-04 the same way (2.5 credits
each); they run no command, so they are listed in `UI` in `icons.rs` and drawn with `paint_ui`.
`smooth_shape`, `smooth_tolerance` and `smooth_stitches` (Smooth Shape, its tolerance and Smooth
Stitches, #478) were added on 2026-11-05 the same way (2.5 credits each); they needed no hand fixes
after `clean_icons.py`.
`knife` (the Knife tool, #477) was added on 2026-11-05 the same way (2.5 credits); it needed no hand
fixes after `clean_icons.py`.
`shape_effects` (Shapes ▸ Edit Outline ▸ Shape Effects, #497) was added on 2026-11-05 the same way
(2.5 credits); it needed no hand fixes after `clean_icons.py`.
The job ids of the first set (2026-10-27) were recovered from the Higgsfield history on 2026-11-04:
each job's output, cleaned with `clean_icons.py`, is byte-identical to its SVG. The `credits`
column is empty where the cost was not recorded at the time.
`tools/check_icons.py` (run in CI) checks that every SVG here has a row in `higgsfield-jobs.tsv`.
Editor commands get them through `glyph` in `icons.rs`, the suite windows' commands by their
key through `suite_glyph`.

`tools/clean_icons.py RAW_DIR assets/icons/commands` strips the generator metadata and the
background and snaps fills to the palette. The charcoal outline (`#2b2f36`) is replaced by the
theme's text colour when an icon is drawn. To add an icon: add a row to `prompts.tsv`, generate
it the same way, clean it, list it in `own!` in `icons.rs` and map it in `glyph` or
`suite_glyph` (a test checks that every SVG is used and renders). See docs/LEGAL_AND_LICENSING.md for where the icons come from.
