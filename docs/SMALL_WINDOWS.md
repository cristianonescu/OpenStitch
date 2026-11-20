# Small windows: every control within reach

What OpenStitch does when its window is smaller than what it shows (#1276): a browser window
of 800 × 500 or 640 × 400 pixels, a desktop window at its smallest (900 × 600), and any of them
with Options ▸ Text size at 150 % or 200 %, which leaves half the room in points. Before this,
a menu, a pane or a dialog taller than the window was cut at the window's edge and could not be
scrolled: the entries below the edge, and the OK and Cancel buttons of a tall dialog, could not
be reached.

## What happens now

One set of helpers (`crates/openstitch-app/src/fit.rs`) makes every surface of the editor fit
the window:

| surface | in a window too small for it |
|---|---|
| Menus, drop-down lists, sub-menus, right-click menus, the application menu | Open below their button (above it when there is more room), never beyond the window, and scroll: wheel, touchpad, or the thin scroll bar at the side, which shows while at rest so a list that goes on is seen to go on. The arrow keys move the highlight and the list follows it; in a number field the arrows change the value, and Tab goes on. Sub-menus open on the side that has room; in a window too narrow for that a sub-menu lies over its menu, and the wheel over it goes on to the menu once the sub-menu has nothing more to scroll that way. |
| Dialogs and prompts | Open over what is on screen and stay inside the window. The content scrolls; the row with OK, Cancel, Apply or Close stays in view below it, ruled off by a line. A dialog that fits is laid out as before. Every combo box's list is no higher and no wider than the window, a longer entry wrapping; a colour button's picker opens in a pop-up that scrolls. |
| Panes (Sewing Order, Attributes, Stitch Simulator; docked, floating, or folded in the narrow layout) | The pane's content scrolls up and down once the pane is lower than it, and sideways once the pane cannot be as wide as its content. A pane with room is as wide as its content, as before. In a window too narrow for all of them (panes on both sides plus the Assistant at 900 points), the Assistant and the Sewing Order pane give way so that each docked pane keeps 200 points and the work area 160. |
| Stitch Simulator docked above or below | Gives way so the work area and the panes beside it keep 160 points of height. |
| Ribbon | The groups that do not fit scroll sideways (wheel, touchpad, a scroll bar under the groups, shown at rest). No command is dropped. |
| Status bar, the simulator's rows of controls, the Attributes pane's tab row, the toolbars of the Design Database, Programmable Stitch Creator and Stitch Design Factory | Each row scrolls sideways once it is wider than its place. |
| Assistant panel | Lays its heading, settings, approval cards and composer out at the height they need with 100 points for the conversation (260 × 220 points at least) and scrolls as a whole when the panel is lower; its settings form scrolls sideways in a panel narrower than it. |
| The window itself | A window lower than 340 points (the ribbon, the status bar and 160 points of work area) scrolls up and down as a page, in the main window and in the other suite windows; the Programmable Stitch Creator and the Stitch Design Factory, which have a panel on each side of their work area, also scroll sideways under 900 points of width. Over the canvas the wheel still zooms and scrolls the design; the page moves by its scroll bar or with the pointer anywhere else. |
| Windows that fill their own size (Stitch Table, the reference window, a floating pane) | Are held to the app's window, and the Stitch Table keeps 360 × 260 points for its columns and a few rows, scrolling when its window is smaller. |

The narrow layout (panes folded into title bars over the canvas, the tabs with Options and
Help in one row that scrolls, the status bar in its narrow form, and in the browser the
Assistant starting closed) starts below a width of 780 points; it was 640. The title bars can
be opened and closed, not dragged; in a window that scrolls as a page they sit 30 points
above the window's foot instead of 100, clear of the top bar.

Tooltips cannot scroll. The tallest one of any command (Angle Lines) is 164 points high at a
window 320 points wide, so every tooltip fits the smallest window of the survey (200 points
high); a test keeps it so.

The desktop window's smallest size stays 900 × 600 pixels.

## How it is checked

- **The walk** (`reach_tests.rs`, in every test run): it opens every ribbon tab and each of
  its drop-down lists, the Options and Help menus, every `--show` scene, every dialog, wizard
  and pane tab, the Point tool's two right-click menus and the rulers', the other suite
  windows with their menus, and whatever each ribbon, menu and Quick Access Toolbar command
  opens; and in each of those every sub-menu, combo box and colour button. The commands,
  scenes and tabs come from the registries, so a new one is walked without touching the test.
  For each control it asks whether it is in view, and if not, turns the mouse wheel over it or
  over the containers around it, as a person would, until it is. It fails for a control that
  cannot be brought into view, for a menu or dialog that reaches beyond the window, and for a
  dialog's pinned button row that is cut, lies over the content or moves with it.
- **Three sizes run with the tests**, six tests each, side by side: 640 × 400 (the narrow
  layout, the smallest browser window), 900 × 600 (the desktop's smallest window: panes
  docked, the Assistant sharing the width, panes scrolling sideways, the Simulator giving
  way) and 640 × 400 at Text size 200 % (320 × 200 points: the window scrolling as a page,
  the pinned rows, menus with no room beside their button). The other seven sizes are the
  survey below, run by hand.
- **It cannot pass on nothing.** A case that opens nothing to measure fails the walk unless
  it is on a named list with how it comes to nothing (`OPENS_NOTHING`: lists and commands
  disabled for the demo design, two contextual tabs, the Select tool's missing right-click
  menu); an entry there that opens something fails too. The commands that open a surface are
  a named list (`COMMANDS_THAT_OPEN`, 28 today): one that stops opening, or a new one that
  starts, fails. So are the dialogs that keep their button row in view at 320 × 200 points
  (`PINNED_AT_320X200`). `the_walk_opens_and_measures_every_surface` counts what was measured
  per kind against today's numbers. Commands the registry marks as not available yet, and
  the eleven that open the system's file dialog, a URL or close the window, are left out
  with that reason.
- **In view** means the whole control, to within one point (a rectangle and the clip it is
  drawn in are snapped to the pixel grid separately), with nothing of another layer lying
  over its middle. A control really larger than the view it scrolls in (a 102-point tab
  button in a pane squeezed to 74 points) cannot be shown at once: it counts as reached when
  both of its ends have been scrolled into view. A control smaller than its view has to be
  in view whole.
- **The source check** (`every_menu_and_dialog_is_made_through_fit`) fails for a window, modal
  prompt, pop-up, menu button, right-click menu, combo box or colour button made with the
  toolkit's own call instead of the helper, and names the helper to use. Panels are not
  pop-ups and have no rule of their own: what is in them is covered by the pane, page and
  row helpers and by the walk.
- **Further tests**: the arrow keys reach every entry of the Options menu and the list
  follows; the wheel goes on through a sub-menu that lies over its menu, and from a list
  inside a dialog (Keyboard Shortcuts, Font Filter Settings, the Font menu) to the dialog; a
  menu opens at its top every time; a settled layout asks for no further frame; an open menu
  is painted opaque; a pane squeezed by a small window gets its width back; in a window too
  low for it the button row scrolls with the content.
- **The survey** is the same walk at ten sizes, run by hand before a change to the helpers
  (`cargo test -p openstitch-app --bin openstitch reach_tests::survey_ -- --ignored`, with
  `OPENSTITCH_REACH_SURVEY` set to a folder for the tables). The tables below are from it.
- **The browser version**: the page test (`web/e2e/page.mjs`, `smallWindowChecks`) opens the
  Options menu and Design Property in a 640 × 400 window, checks through `ui.state`'s
  `surfaces` that both are inside the window, scrolls each to its end with the wheel and
  checks that the dialog's button row has not moved.

## Dialogs without a button row kept in view

A dialog keeps its row in view when it has one row of actions at its end (37 dialogs, plus
Monogram, the Template Wizard's steps, Send to USB and Floppy Image since the review). The
others have none to keep: every control of theirs scrolls with the content and is reached by
the walk.

| dialog | why no row is kept in view |
|---|---|
| Design Settings | every change applies at once; there is no OK or Apply |
| Check Design, Getting Started, Stitch Table, Reference | a window of its own size around a list; nothing to confirm |
| Command Palette | a list; Enter runs the highlighted command |
| Keyboard Shortcuts | Export, Import and Reset are above the list, not an action row at its end |
| Ink/Stitch Fonts, Open Sample | each row of the list is its own button |
| Font Creator (Fonts), Test Swatches | sections, each with its own button under its own fields |
| Modify Image, Fix on the picture, Mark the subject | tools and sliders that act at once |
| About | text with one button (Copy Diagnostics) in it |

A row is kept in view only while that leaves the content 60 points (`fit::PIN_LEAST`): in a
window under about 130 points high the row scrolls with the content, below it.

## Known limits

- The walk asks whether a control can be reached, not whether the layout is pleasant: at
  320 × 200 points most of the editor is reached by scrolling.
- The scroll bars are the toolkit's own (thin, over the content). Touch: a list scrolls by
  dragging it; the page-level scroll of a very low window does not take a drag that starts on
  the canvas.
- Combo boxes use the toolkit's own drop-down, which scrolls by itself; the helper holds it
  to the window's height and width.

## Totals by window size

Controls that could not be brought into view, summed over every surface walked; in brackets
the number of surfaces that lost at least one.

| window (pixels), Text size | points | before | after |
|---|---|---:|---:|
| 1440 × 900, 100 % | 1440 × 900 | 44 (42) | 0 (0) |
| 900 × 600, 100 % | 900 × 600 | 339 (55) | 0 (0) |
| 900 × 600, 150 % | 600 × 400 | 208 (74) | 0 (0) |
| 900 × 600, 200 % | 450 × 300 | 446 (95) | 0 (0) |
| 800 × 500, 100 % | 800 × 500 | 740 (69) | 0 (0) |
| 800 × 500, 150 % | 533 × 333 | 323 (82) | 0 (0) |
| 800 × 500, 200 % | 400 × 250 | 546 (102) | 0 (0) |
| 640 × 400, 100 % | 640 × 400 | 1065 (76) | 0 (0) |
| 640 × 400, 150 % | 427 × 267 | 495 (96) | 0 (0) |
| 640 × 400, 200 % | 320 × 200 | 703 (118) | 0 (0) |

Surfaces walked: 217 before, 243 after (the walk now also opens the ribbon's
drop-down lists, every sub-menu, combo box and colour button, the Point tool's right-click
menus and the other suite windows' menus; it measures a scene that changes the window itself
as the whole window).

The "before" numbers are from the first version of the walk, on the release before: it turned
the wheel only with the pointer on the control or on the nearest view of it, where today's
walk also tries other places in the containers around it. A few of the controls it counted
(a row of a list inside a pane, with the pointer resting on the list) could be reached by
moving the pointer first; the menus, dialogs and panes it counted could not be scrolled at all.
It also took a control as reached when it was in view with another window lying over it,
which today's walk does not: the "after" numbers are by the stricter rules.

## Every surface

"Before" and "after" give the largest number of controls out of reach at any of the ten sizes
(and the size it happened at); "ok" is none at any size; "–" is a surface the survey of the
release before did not open.

### Main window (ribbon, panes, status bar)

| surface | controls | before | after |
|---|---:|---|---|
| Main window, ribbon tab Home | 171 | 94 at 640 × 400, 100 % | ok |
| Main window, ribbon tab Image | 147 | 94 at 640 × 400, 100 % | ok |
| Main window, ribbon tab View | 155 | 94 at 640 × 400, 100 % | ok |
| Main window, ribbon tab Text | 145 | 84 at 640 × 400, 100 % | ok |
| Main window, ribbon tab Outline | 146 | 94 at 640 × 400, 100 % | ok |
| Main window, ribbon tab Manual Punch | 147 | 94 at 640 × 400, 100 % | ok |
| Main window, ribbon tab Shapes | 166 | 94 at 640 × 400, 100 % | ok |
| Main window, pane tab import | 129 | 52 at 640 × 400, 100 % | ok |
| Main window, pane tab color | 127 | 50 at 640 × 400, 100 % | ok |
| Main window, pane tab attributes | 171 | 94 at 640 × 400, 100 % | ok |
| Main window, pane tab text | 132 | 55 at 640 × 400, 100 % | ok |

### Ribbon drop-down lists

| surface | controls | before | after |
|---|---:|---|---|
| Ribbon Home > Select Same (list) | 3 | – | ok |
| Ribbon Home > Text (list) | 9 | – | ok |
| Ribbon Home > Shapes (list) | 6 | – | ok |
| Ribbon Home > Shapes (list) > Insert Preset Shape | 18 | – | ok |
| Ribbon Home > Zoom (list) | 8 | – | ok |
| Ribbon Home > Arrange (list) | 5 | – | ok |
| Ribbon Home > Rotate/Flip (list) | 5 | – | ok |
| Ribbon Home > Size (list) | 6 | – | ok |
| Ribbon Home > Stamp/Emboss (list) | 7 | – | ok |
| Ribbon Home > Decorative Frame (list) | 6 | – | ok |
| Ribbon Home > Align (list) | 11 | – | ok |
| Ribbon Home > Sew Embroidery (list) | 5 | – | ok |
| Ribbon Home > Sew Embroidery (list) > Export | 25 | – | ok |
| Ribbon Home > Sew Embroidery (list) > Export > PES | 11 | – | ok |
| Ribbon Home > Sew Embroidery (list) > Export > EXP | 2 | – | ok |
| Ribbon Home > Sew Embroidery (list) > Export > Garment Mockup | 5 | – | ok |
| Ribbon View > Show (list) | 10 | – | ok |
| Ribbon View > Grid Spacing (list) | 6 | – | ok |
| Ribbon View > Guides (list) | 7 | – | ok |
| Ribbon Text > Select Same (list) | 3 | – | ok |
| Ribbon Text > Text (list) | 9 | – | ok |
| Ribbon Text > Font (list) | 4 | – | ok |
| Ribbon Text > Text Size (list) | 7 | – | ok |
| Ribbon Text > Line color (list) | 5 | – | ok |
| Ribbon Text > Line sew type (list) | 23 | – | ok |
| Ribbon Text > Region color (list) | 5 | – | ok |
| Ribbon Text > Region sew type (list) | 18 | – | ok |
| Ribbon Text > Edit Text (list) | 2 | – | ok |
| Ribbon Text > Edit Character (list) | 1 | – | ok |
| Ribbon Outline > Select Same (list) | 3 | – | ok |
| Ribbon Outline > Line color (list) | 5 | – | ok |
| Ribbon Outline > Line sew type (list) | 23 | – | ok |
| Ribbon Outline > Region color (list) | 5 | – | ok |
| Ribbon Outline > Region sew type (list) | 18 | – | ok |
| Ribbon Outline > Edge radius (list) | 6 | – | ok |
| Ribbon Manual Punch > Select Same (list) | 3 | – | ok |
| Ribbon Manual Punch > Column width (list) | 5 | – | ok |
| Ribbon Manual Punch > Region color (list) | 5 | – | ok |
| Ribbon Manual Punch > Region sew type (list) | 18 | – | ok |
| Ribbon Shapes > Select Same (list) | 3 | – | ok |
| Ribbon Shapes > Smooth tolerance (list) | 5 | – | ok |
| Ribbon Shapes > Shape Effects (list) | 3 | – | ok |
| Ribbon Shapes > Turning Gap (list) | 3 | – | ok |
| Ribbon Shapes > Line color (list) | 5 | – | ok |
| Ribbon Shapes > Line sew type (list) | 23 | – | ok |
| Ribbon Shapes > Region color (list) | 5 | – | ok |
| Ribbon Shapes > Region sew type (list) | 18 | – | ok |
| Ribbon Shapes > Entry/Exit Points (list) | 5 | – | ok |
| Ribbon Shapes > Center/Reference Point (list) | 2 | – | ok |
| Ribbon Home > Sew Embroidery (list) > Export > Thread List | 6 | – | ok |

### Top bar menus (Options, Help) and their sub-menus

| surface | controls | before | after |
|---|---:|---|---|
| Menu Options | 37 | 30 at 640 × 400, 200 % | ok |
| Menu Options > Accent colour | 8 | ok | ok |
| Menu Help | 9 | 1 at 640 × 400, 200 % | ok |
| Menu Options > Language | 2 | ok | ok |
| Menu Options > New Objects | 7 | ok | ok |
| Menu Options > Reset Preferences | 6 | ok | ok |

### Scenes (`--show`): application menu, dialogs, panes, suite windows

| surface | controls | before | after |
|---|---:|---|---|
| Scene update | 5 | 4 at 900 × 600, 200 % | ok |
| Scene stitch-table | 52 | ok | ok |
| Scene large-text | 171 | – | ok |
| Scene keyboard | 147 | – | ok |
| Scene pickers | 18 | ok | ok |
| Scene tabs | 176 | – | ok |
| Scene close-prompt | 3 | 1 at 640 × 400, 200 % | ok |
| Scene realistic | 171 | – | ok |
| Scene solid | 171 | – | ok |
| Scene fabric | 19 | 11 at 640 × 400, 200 % | ok |
| Scene fabric > Thread weight | 4 | ok | ok |
| Scene reference | 2 | ok | ok |
| Scene settings | 26 | 16 at 640 × 400, 200 % | ok |
| Scene settings > Hoop | 68 | ok | ok |
| Scene settings > Section size | 12 | ok | ok |
| Scene measure | 171 | – | ok |
| Scene norulers | 171 | – | ok |
| Scene text | 11 | 1 at 800 × 500, 200 % | ok |
| Scene text > Font | 4 | ok | ok |
| Scene creator | 31 | – | ok |
| Scene sdf | 26 | – | ok |
| Scene attributes | 4 | 1 at 640 × 400, 200 % | ok |
| Scene charts | 9 | 3 at 640 × 400, 200 % | ok |
| Scene charts > New Brothread 100 % polyester (63-spool kit) | 12 | 12 at 900 × 600, 200 % | ok |
| Scene matchThreads | 9 | 4 at 640 × 400, 200 % | ok |
| Scene matchThreads > Chart | 12 | 12 at 900 × 600, 200 % | ok |
| Scene mergeColours | 6 | ok | ok |
| Scene usb | 6 | ok | ok |
| Scene usb > Format | 16 | ok | ok |
| Scene fonts | 23 | 18 at 640 × 400, 200 % | ok |
| Scene monogram | 15 | 3 at 640 × 400, 200 % | ok |
| Scene monogram > Font | 4 | ok | ok |
| Scene monogram > Frame | 9 | – | ok |
| Scene namedrop | 11 | 4 at 640 × 400, 200 % | ok |
| Scene templates | 8 | 1 at 800 × 500, 200 % | ok |
| Scene arrange | 7 | 4 at 640 × 400, 200 % | ok |
| Scene arrange > Arrangement | 7 | ok | ok |
| Scene appmenu | 14 | 7 at 640 × 400, 200 % | ok |
| Scene appmenu > Print | 2 | ok | ok |
| Scene appmenu > Wizard | 7 | ok | ok |
| Scene appmenu > Export | 25 | 11 at 800 × 500, 150 % | ok |
| Scene appmenu > Export > PES | 11 | ok | ok |
| Scene appmenu > Export > EXP | 2 | ok | ok |
| Scene appmenu > Export > Garment Mockup | 5 | ok | ok |
| Scene property | 9 | 6 at 900 × 600, 150 % | ok |
| Scene shortcuts | 46 | 19 at 640 × 400, 200 % | ok |
| Scene floating | 85 | 1 at 640 × 400, 100 % | ok |
| Scene floating > Region sewing type | 18 | ok | ok |
| Scene floating > How Optimise arranges the objects; saved with the design | 2 | ok | ok |
| Scene print | 15 | 5 at 640 × 400, 200 % | ok |
| Scene assistant | 171 | – | ok |
| Scene assistant-notices | 192 | – | ok |
| Scene assistant-questions | 194 | – | ok |
| Scene assistant-suggestions | 182 | – | ok |
| Scene basic | 153 | – | ok |
| Scene getting-started | 7 | ok | ok |
| Scene machine-setup | 8 | 4 at 640 × 400, 200 % | ok |
| Scene machine-setup > Brand | 9 | ok | ok |
| Scene machine-setup > Hoop | 14 | ok | ok |
| Scene startup-wizard | 12 | 10 at 640 × 400, 200 % | ok |
| Scene bare | 6 | – | ok |
| Scene appmenu > Export > Thread List | 6 | ok | ok |
| Scene appmenu > Options | 37 | – | ok |
| Scene appmenu > Options > Language | 2 | – | ok |
| Scene appmenu > Options > New Objects | 7 | – | ok |
| Scene appmenu > Options > Accent colour | 8 | – | ok |
| Scene appmenu > Options > Reset Preferences | 6 | – | ok |
| Scene appmenu > Options > Text size | 5 | – | ok |

### Dialogs opened from other dialogs or by the editor

| surface | controls | before | after |
|---|---:|---|---|
| Dialog Assistant closed hint | 2 | ok | ok |
| Dialog Assistant still working | 2 | 1 at 640 × 400, 200 % | ok |
| Dialog Offset Lines | 8 | ok | ok |
| Dialog Floral Pattern | 7 | ok | ok |
| Dialog Floppy Image | 4 | ok | ok |
| Dialog Ink/Stitch Fonts | 4 | ok | ok |
| Dialog Job Sheet | 18 | 11 at 640 × 400, 200 % | ok |
| Dialog Fit Text to Outline | 14 | 4 at 640 × 400, 200 % | ok |
| Dialog Machine Setup | 8 | 4 at 640 × 400, 200 % | ok |
| Dialog Machine Setup > Brand | 9 | ok | ok |
| Dialog Machine Setup > Hoop | 14 | ok | ok |
| Dialog Command Palette | 182 | ok | ok |
| Dialog Recover Designs | 1 | ok | ok |
| Dialog Check Design | 10 | 1 at 800 × 500, 200 % | ok |
| Dialog Check Design > standard | 13 | ok | ok |
| Dialog Check Design, Density | 10 | 2 at 640 × 400, 200 % | ok |
| Dialog Check Design, Density > standard | 13 | ok | ok |
| Dialog Check Design, Route | 14 | 2 at 640 × 400, 200 % | ok |
| Dialog Check Design, Route > standard | 13 | ok | ok |
| Dialog Test Swatches | 7 | ok | ok |
| Dialog Calibrate Real Size | 5 | ok | ok |
| Dialog Open Sample | 7 | 1 at 640 × 400, 200 % | ok |
| Dialog Startup Wizard | 12 | 10 at 640 × 400, 200 % | ok |
| Dialog Not in the browser version | 3 | 2 at 640 × 400, 200 % | ok |
| Dialog Image to Embroidery | 22 | 15 at 640 × 400, 200 % | ok |
| Dialog Modify Image | 2 | ok | ok |
| Dialog Realistic View Settings | 10 | ok | ok |
| Dialog Font Filter Settings | 10 | 2 at 640 × 400, 200 % | ok |
| Dialog About | 4 | 1 at 640 × 400, 200 % | ok |
| Dialog Report a Problem | 4 | ok | ok |
| Dialog Template Wizard, step 2 | 4 | ok | ok |
| Dialog Template Wizard, step 3 | 5 | ok | ok |
| Dialog Design Database, Search | 58 | 34 at 640 × 400, 200 % | ok |
| Dialog Design Database, Catalogue | 16 | 7 at 640 × 400, 200 % | ok |
| Dialog Design Database, Catalogue > Printable pages (PDF) | 4 | ok | ok |
| Dialog Design Database, Catalogue > Catalog style 4 | 4 | ok | ok |

### Other windows of the suite

| surface | controls | before | after |
|---|---:|---|---|
| Design Database (whole window) | 45 | ok | ok |
| Programmable Stitch Creator, File menu | 7 | – | ok |
| Stitch Design Factory, File menu | 6 | – | ok |
| Stitch Design Factory, File menu > New | 4 | – | ok |
| Design Database, menu File | 13 | – | ok |
| Design Database, menu Edit | 4 | – | ok |
| Design Database, menu Display | 8 | – | ok |
| Design Database, menu Option | 6 | – | ok |
| Design Database, menu Option > Select System Unit | 2 | – | ok |
| Design Database, menu Option > DST Settings | 1 | – | ok |
| Design Database, menu Help | 3 | – | ok |

### Pattern wizards

| surface | controls | before | after |
|---|---:|---|---|
| Wizard background | 16 | 8 at 640 × 400, 200 % | ok |
| Wizard background > Hoop | 3 | ok | ok |
| Wizard background > Echo fill | 5 | ok | ok |
| Wizard background > Running stitch | 3 | ok | ok |
| Wizard applique | 19 | 17 at 640 × 400, 200 % | ok |
| Wizard applique > Running stitch (cut by hand) | 3 | ok | ok |
| Wizard applique > V stitch | 4 | ok | ok |
| Wizard applique > Zigzag stitch | 4 | ok | ok |
| Wizard patch | 15 | 9 at 640 × 400, 200 % | ok |
| Wizard patch > Running stitch (cut by hand) | 3 | ok | ok |
| Wizard cutwork | 23 | 14 at 640 × 400, 200 % | ok |
| Wizard cutwork > Square | 5 | ok | ok |

### Right-click menus

| surface | controls | before | after |
|---|---:|---|---|
| Right click on a shape, Point tool | 5 | – | ok |
| Right click on a shape, Point tool > Delete Direction Line | 2 | – | ok |
| Right click on a point, Point tool | 7 | – | ok |
| Right click on the top ruler | 10 | – | ok |
| Right click on the left ruler | 10 | – | ok |

### What each ribbon, menu and Quick Access Toolbar command opens

| surface | controls | before | after |
|---|---:|---|---|
| Command Arrange Copy | 7 | ok | ok |
| Command Arrange Copy > Arrangement | 7 | ok | ok |
| Command Background fill | 16 | 8 at 640 × 400, 200 % | ok |
| Command Background fill > Hoop | 3 | ok | ok |
| Command Background fill > Echo fill | 5 | ok | ok |
| Command Background fill > Running stitch | 3 | ok | ok |
| Command Applique Wizard | 19 | 17 at 640 × 400, 200 % | ok |
| Command Applique Wizard > Running stitch (cut by hand) | 3 | ok | ok |
| Command Applique Wizard > V stitch | 4 | ok | ok |
| Command Applique Wizard > Zigzag stitch | 4 | ok | ok |
| Command Embroidered Patch Wizard | 15 | 9 at 640 × 400, 200 % | ok |
| Command Embroidered Patch Wizard > Running stitch (cut by hand) | 3 | ok | ok |
| Command Cutwork Wizard | 23 | 14 at 640 × 400, 200 % | ok |
| Command Cutwork Wizard > Square | 5 | ok | ok |
| Command Auto Punch Wizard | 22 | 15 at 640 × 400, 200 % | ok |
| Command Cross Stitch Wizard | 19 | 12 at 640 × 400, 200 % | ok |
| Command Photo Stitch 1 Wizard | 17 | 10 at 640 × 400, 200 % | ok |
| Command Photo Stitch 2 Wizard | 17 | 10 at 640 × 400, 200 % | ok |
| Command Import to Design Center | 17 | 10 at 640 × 400, 200 % | ok |
| Command Import to Design Center > running | 4 | ok | ok |
| Command Import to Design Center > auto | 4 | ok | ok |
| Command Realistic View Settings | 10 | ok | ok |
| Command Reference Window | 2 | ok | ok |
| Command Assistant | 2 | ok | ok |
| Command Font Filter Settings | 10 | 2 at 640 × 400, 200 % | ok |
| Command Create Offset Lines | 8 | ok | ok |
| Command Create Floral Pattern | 7 | ok | ok |
| Command Design Settings | 26 | 16 at 640 × 400, 200 % | ok |
| Command Design Settings > Hoop | 68 | ok | ok |
| Command Design Settings > Section size | 12 | ok | ok |
| Command New | 2 | 1 at 900 × 600, 200 % | ok |
| Command Open Sample | 7 | 1 at 640 × 400, 200 % | ok |
| Command Design Property | 9 | 6 at 900 × 600, 150 % | ok |
| Command Fabric Selector | 19 | 11 at 640 × 400, 200 % | ok |
| Command Fabric Selector > Thread weight | 4 | ok | ok |
| Command Select Color Palette | 9 | 3 at 640 × 400, 200 % | ok |
| Command Select Color Palette > New Brothread 100 % polyester (63-spool kit) | 12 | 12 at 900 × 600, 200 % | ok |
| Command Design Database (whole window) | 45 | ok | ok |
| Command Programmable Stitch Creator (whole window) | 31 | 5 at 640 × 400, 200 % | ok |
| Command Stitch Design Factory (whole window) | 26 | 4 at 800 × 500, 200 % | ok |
| Command Font Creator | 23 | 18 at 640 × 400, 200 % | ok |
| Command Sewing Attribute Sets | 4 | 1 at 640 × 400, 200 % | ok |
| Command User Thread Charts | 9 | 3 at 640 × 400, 200 % | ok |
| Command User Thread Charts > New Brothread 100 % polyester (63-spool kit) | 12 | 12 at 900 × 600, 200 % | ok |

Surfaces of the first survey that the walk now opens under another name or as part of
another row: Ribbon Home > Select Same (list): its button cannot be reached; Ribbon Home > Text (list): its button cannot be reached; Ribbon Home > Arrange (list): its button cannot be reached; Ribbon Home > Rotate/Flip (list): its button cannot be reached; Ribbon Home > Size (list): its button cannot be reached; Ribbon Home > Stamp/Emboss (list): its button cannot be reached; Ribbon Home > Decorative Frame (list): its button cannot be reached; Ribbon Home > Align (list): its button cannot be reached; Ribbon Home > Sew Embroidery (list): its button cannot be reached; Ribbon View > Show (list): its button cannot be reached; Ribbon View > Grid Spacing (list): its button cannot be reached; Ribbon View > Guides (list): its button cannot be reached; Ribbon Text > Select Same (list): its button cannot be reached; Ribbon Text > Font (list): its button cannot be reached; Ribbon Text > Text Size (list): its button cannot be reached; Ribbon Text > Line color (list): its button cannot be reached; Ribbon Text > Line sew type (list): its button cannot be reached; Ribbon Text > Region color (list): its button cannot be reached; Ribbon Text > Region sew type (list): its button cannot be reached; Ribbon Text > Edit Text (list): its button cannot be reached; Ribbon Text > Edit Character (list): its button cannot be reached; Ribbon Outline > Select Same (list): its button cannot be reached; Ribbon Outline > Line color (list): its button cannot be reached; Ribbon Outline > Line sew type (list): its button cannot be reached; Ribbon Outline > Region color (list): its button cannot be reached; Ribbon Outline > Region sew type (list): its button cannot be reached; Ribbon Outline > Edge radius (list): its button cannot be reached; Ribbon Manual Punch > Select Same (list): its button cannot be reached; Ribbon Manual Punch > Column width (list): its button cannot be reached; Ribbon Manual Punch > Region color (list): its button cannot be reached; Ribbon Manual Punch > Region sew type (list): its button cannot be reached; Ribbon Shapes > Select Same (list): its button cannot be reached; Ribbon Shapes > Smooth tolerance (list): its button cannot be reached; Ribbon Shapes > Shape Effects (list): its button cannot be reached; Ribbon Shapes > Turning Gap (list): its button cannot be reached; Ribbon Shapes > Line color (list): its button cannot be reached; Ribbon Shapes > Line sew type (list): its button cannot be reached; Ribbon Shapes > Region color (list): its button cannot be reached; Ribbon Shapes > Region sew type (list): its button cannot be reached; Ribbon Shapes > Entry/Exit Points (list): its button cannot be reached; Ribbon Shapes > Center/Reference Point (list): its button cannot be reached; Main window, ribbon tab Stitches (not shown for this selection); Main window, ribbon tab Hoop Settings (not shown for this selection); Menu Options > Reset Preferences > Save Layout; Menu Options > Text size; Scene keyboard (adds no surface of its own); Scene tabs (adds no surface of its own); Scene realistic (adds no surface of its own); Scene solid (adds no surface of its own); Scene measure (adds no surface of its own); Scene creator (whole window); Scene sdf (whole window); Scene assistant (adds no surface of its own); Scene assistant-notices (adds no surface of its own); Scene assistant-questions (adds no surface of its own); Scene assistant-suggestions (adds no surface of its own); Scene basic (adds no surface of its own); Menu Options > Layouts; Scene pickers (adds no surface of its own); Scene floating (adds no surface of its own); Ribbon Home > Zoom (list): its button cannot be reached.
