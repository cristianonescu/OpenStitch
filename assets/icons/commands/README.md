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
`move_up` and `move_down` (the reorder buttons of the Sewing Order, thread chart and user hoop
lists, #409), `pane_dock` and `pane_float` (the panes' dock and float buttons, #410) and
`menu_caret` (the drop-down marker of menu buttons, #412) were added on 2026-11-06 the same way
(2.5 credits each). `line_gap` (Shapes ▸ Edit Outline ▸ Turning Gap, #635) was added the same day
on the third attempt (7.5 credits in all; the first two, a U-turn arrow and a downward arrow
through a square, read as other commands at 16 pixels and were not kept). They run no command, so they are listed in `UI` in `icons.rs` and drawn with
`paint_ui` or, inside a button, `ui_image`; so are `close`, `folder` and the two
`image_rotate_*` icons where buttons outside the ribbon use them (#408, #410, #411). After
`clean_icons.py`, `pane_dock`'s arrow, generated a bright mint, was set to the palette green by hand.
`shape_builder` (the Shape Builder tool, #497) was added on 2026-11-05 the same way (2.5 credits);
after `clean_icons.py` its arrow, generated as a green gradient, was set to the palette green by
hand and the gradient removed.
`status_done`, `status_failed` and `status_running` (the Assistant panel's tool-step marks, #653)
were added on 2026-11-06 the same way (2.5 credits each); after `clean_icons.py`, `status_done`'s
badge, generated a bright mint, was set to the palette green by hand. They run no command, so they
are in `UI`; so is `hand`, used as the Sewing Order rows' drag grip.
`test_swatches` and `job_sheet` (Send ▸ Test Swatches…, #452/#453/#507, and Print ▸ Job Sheet…,
#513) were added on 2026-11-06 the same way (2.5 credits each); after `clean_icons.py`, `job_sheet`'s
check mark, generated a bright mint, was set to the palette green by hand.
`player_start`, `player_back`, `player_forward` and `player_end` (the Stitch Simulator's start,
previous, next and end buttons, #653) were added on 2026-11-06 the same way (2.5 credits each,
plus 7.5 for three attempts not kept: a framed skip-to-start and step-back that did not match
their pairs, and a skip-to-start that pointed right). `player_start` is `player_end` mirrored
(a `transform` group around its paths), so its row in `higgsfield-jobs.tsv` names `player_end`'s
job with 0 credits.
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
