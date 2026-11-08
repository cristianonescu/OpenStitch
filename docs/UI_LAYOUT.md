# Editor window layout

The OpenStitch editor arranges its commands the way the Layout & Editing window of Brother
PE-DESIGN 11 does, so people moving from it find each command where they expect it. The
arrangement, command names and workflow follow Brother's public online guide:
[Layout & Editing Window](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/03_1245825.html),
[Ribbon menu tabs](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_2156079.html)
and [Application button menu](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_2152475.html).
Icons, colours and visual style are OpenStitch's own (see
[LEGAL_AND_LICENSING.md](LEGAL_AND_LICENSING.md#user-interface)).

![Editor window](images/rust-editor-overview.png)

| Guide element | OpenStitch |
|---|---|
| 1 Application button | **File** button at the top left: New, Open, Save, Save As, Print ▸, Design Property, Design Settings, Fabric Selector, Select Color Palette, Wizard ▸, Export ▸, Recent Files, Options, Exit ([screenshot](images/rust-editor-appmenu.png)) |
| 2 Ribbon | Tabs Home, Image, View plus contextual tabs (orange) for the active tool or selection; large and small buttons, dropdown arrows, group captions, ScreenTips with shortcuts |
| 3 Quick Access Toolbar | Above the ribbon (or below it, Options ▸ or ▾ ▸ Show below the Ribbon): New, Open, Save, Undo, Redo, Zoom, Help by default; ▾ adds or removes commands. Saved in `ui.json` next to the user settings file |
| 4 Option button | **Options ▾** at the top right: Design Database, Programmable Stitch Creator, Stitch Design Factory, Font Creator, Sewing Attribute Sets, User Thread Charts, colour theme (Light, Dark, System); beyond PE-DESIGN: Accent colour (#559), Layouts (save and apply where the panes are, #551) Reset Preferences (one area at a time, #551), Show start cards and New Objects (#553, #554) |
| 5 Help button | **Help ▾**: instruction manual (this repository), Ready-to-Sew Designs (the kit packaged with the app, `examples/sew`; GitHub when none is packaged), Ready-to-Sew Designs Online, keyboard shortcuts, About |
| 6 Sewing Order pane | Docked left: buttons at the top (sew first / earlier / later / last, sort by colour, optimise, frames by thread colour), then Sew in click order, Lock and Sew outline first (#502), Stop after (#512), Leave out hidden stitching (#511) and Then: run (#545), beyond PE-DESIGN; Branch / Unbranch, Auto-route lines and Redwork (#546, beyond PE-DESIGN); Swap sides and Untangle for a selected Manual Punch column (#525, beyond PE-DESIGN); Optimise reports its before/after colour changes, jumps, trims and travel in the status bar (#547); one frame per object with a thumbnail and its thread colours; drag to reorder; double-click a name to rename it (Return keeps, Esc cancels); locked objects show "(locked)" |
| 7 Import / Color / Sewing Attributes / Text Attributes | Docked right as four tabs |
| 8 Stitch Simulator pane | Docked under the work area. Under PE-DESIGN's controls, the stitch-out player (#516, beyond PE-DESIGN): counters (stitch and machine time at the needle), Prev/Next block, Prev/Next object and Next trim, the Auto, Pause at trims and Follow needle toggles; a lane per colour block with trim ticks and colour-change / stop markers (click or drag to seek); one button per block named by its thread. Keys while part of the design is shown: ←/→ stitch, Shift block, Ctrl object, T next trim, Home/End |
| 9–10 Design Page, work area | White page on a grey work area (light theme) |
| 11 Status bar | Design size, stitch count, the ScreenTip of the command under the pointer, else the last message and the hints line (#550): what clicks and keys do with the active tool, and while Shift, Ctrl (Cmd) or Alt is held what that key changes; at the right, the Check Design indicator ("Ready to sew", or the problem count and the first reason; beyond PE-DESIGN) |
| 12–15 View mode, Show grid, Design Property, zoom | Right of the status bar: Solid / Stitch / Realistic, Show Grid, Design Property, zoom percentage (type or drag) and zoom slider |

**Check Design** (beyond PE-DESIGN, #503–#505) opens from the status-bar indicator or the
Sewing Order pane's **Review sewing order** button. It has three tabs, judged for the fabric
chosen at the top:

- **Problems**: what is likely to sew badly, errors first (outside the hoop, satin too wide or
  narrow, thin fills, small text, long stitches, untrimmed jumps, stacked or dense thread,
  stitching in place, tiny objects, self-crossing outlines). Clicking a row selects and zooms to
  its objects; **Fix** runs the suggested command as one undo step. While the tab is open the
  canvas rings each problem and draws stitches past the hoop in red.
- **Density map**: thread layers per 1 mm cell (one layer = one default fill), heavy cells in
  amber and too-dense cells in red over the design, the share of each class, and the hotspots
  with **Show** / **Next**. **Dark background** puts the design on near-black to review light
  threads; it changes only the view.
- **Sewing order**: every object in sewing order with the move to the next ("Trim, then
  18.6 mm", "Jump of 6.0 mm, not trimmed"), the totals (trims, trims per 1,000 stitches, thread
  changes, travel) and warnings for untrimmed jumps and long same-thread moves, each with a fix.
  **Step by step** moves the Stitch Simulator to the end of the chosen step and marks its start
  (+), end (×) and the labelled move into it; **Route map** numbers every start and draws each
  move as an arrow (red: untrimmed jump, blue: thread change).

The same checks are the engine commands `design.check`, `design.density`, `design.route` and
`design.cleanup`, so `stitchtool exec`, the MCP server and the Assistant see what the window
shows. `design.export` reads every file it writes back and compares it with the stitches meant
for it (`verify`: warn, strict or off), and lists Check Design's errors as `problems`.

Panes 6, 7 and 8 are shown or hidden from View ▸ Show/Hide or their × button, and float as a
window (⧉) or dock again (⊟ Dock) ([screenshot](images/rust-editor-floating-panes.png)).

Commands OpenStitch does not implement yet are shown disabled, with a ScreenTip that says
"Not yet available in OpenStitch", rather than left out. The tables below are the model in
`crates/openstitch-app/src/editor/ribbon.rs`; `ribbon::tests` and the binary's
`ribbon::tests` check that every button runs an existing action or is marked unavailable, that
each engine command named exists, and that the contextual tabs follow the tool and selection.

## View, Image and Text commands

Behaviour follows the guide's pages
[Changing the display of the embroidery design](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_2051337.html),
[Specifying Basic Display Settings](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/03_1246217.html),
[Changing the size, angle and position of the image](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/06_1804142.html),
[Saving the image data](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/06_1804194.html),
[Setting text attributes](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1810676.html) (Font Filter) and
[Using Fabric Selector](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/03_1246198.html).

- **Solid View** draws every stitch 0.7 mm wide without shading, so the rows of a region merge
  into flat areas of thread colour. Solid, Stitch and Realistic are exclusive (ribbon and status
  bar).
- **Realistic View Settings**: thread width (0.2–1 mm, default 0.4), contrast and brightness
  (−100–100), shown at once and kept in `ui.json`; Default restores them.
- **Show Grid with Axes**: on, the grid is drawn with solid lines; off, dotted.
- **Guideline** (also turns the rulers on): click or drag on the top ruler for a vertical
  guideline, on the left ruler for a horizontal one; drag its marker in the ruler to move it,
  click the × beside the marker (or drop it outside the canvas) to delete it. With Show Grid and
  Snap to Grid on, guidelines move in grid steps. The status bar shows the position while
  dragging. Up to 100 per ruler; they are saved in the `.osd` (`guides.set`, one Undo per
  change) and hidden when Ruler or Guideline is off. A click selects a guideline (its marker,
  or the line with the Select tool, which also drags it); Delete removes it, Alt-click turns
  it, a double-click opens Set Position… (#799).
- **Guides ▾** (#799): Lock Guides, Set Position…, Clear Selected Guide, Clear Guides, New
  Guides From Selection (`view.lockGuides`, `view.editGuide`, `view.deleteGuide`,
  `view.clearGuides`, `view.newGuidesFromSelection`).
- **Options ▾ ▸ Editing** (beyond PE-DESIGN, #474, #475): Nudge step (0.1 mm by default as in
  PE-DESIGN; Shift moves ten times as far), Nudge by the grid step while snapping (with Snap to
  Grid on, an arrow moves one grid step) and Click inside outlines selects them (off: the inside
  of a shape that sews only its outline is not a hit, its line must be clicked, as in
  PE-DESIGN). Kept in `ui.json`; the Assistant reads and sets them with `ui.settings` / `ui.set`
  (`nudgeStep`, `nudgeByGrid`, `clickInsideOutlines`, `snapToObjects`, `precisionBar`,
  `alignTo`).
- **Options ▾ ▸ Units** (#519): Millimetres (default, as PE-DESIGN) or Inches, the same setting
  as the Design Database's Select System Unit. It changes every length the editor shows and
  accepts: precision bar, rulers, grid presets, measure tool, pointer position, lettering
  sizes, Design Page and hoop sizes (hoops show both units), split positions, background image,
  shape tool sizes, Design Property, status bar and the print scale bar. A field takes `2"`,
  `2 in`, `1 1/2"`, `50.8 mm` or `5 cm` in either mode. Stitch settings stay in mm. Kept in
  `ui.json`; the Assistant reads and sets it with `ui.settings` / `ui.set` (`units`).
- **Alt-click on a stitch pattern** selects every stitch pattern that sews the thread colour
  under the pointer (PE-DESIGN's selection of stitch patterns by thread colour, #475).
- **Save Image** ▸ to File… writes the background image as shown (turned, cropped, colours
  adjusted) as PNG, BMP or JPEG (`background.save`); ▸ to Clipboard copies it.
- **Modify Image** opens a window with the image's position and width (the height keeps its
  proportions), a crop of each side (the kept part stays where it was, at the same scale),
  brightness, contrast and saturation; the image is outlined in red while the window is open.
  Each change is one `background.modify` (one Undo); the image file is never changed.
- **Rotate Right / Left** turn the background image 90° about its centre (`background.rotate`).
- **Font Filter Settings** lists the built-in fonts and the fonts in the design with check
  boxes (All / None per group, Default shows every font); OK turns the filter on. **Font
  Filter** switches filtering on and off; it applies to every font list. A filter that would
  hide every font is ignored. Both are kept in `ui.json`.
- **Fabric Selector** (Application menu): pick a fabric type to read a description and advice
  and see the recommended settings; OK applies them to every shape and text pattern on the
  Design Page, or, with the check box cleared, to the selected ones (`fabric.apply`, one Undo).
  Beyond PE-DESIGN (#508): a **thread weight** choice (No. 60, 40, 30, 12) scales the fill and
  satin density and shows that weight's needle and smallest text, and **New shapes and texts
  start from this fabric** makes OK also set the default for objects drawn afterwards
  (`fabric.setDefault`; cleared, OK removes it).
  It sets the density, stitch length, pull compensation and underlay of fill, programmable
  fill and satin regions; colours and sew types stay. The values are OpenStitch's own
  approximations (`openstitch_engine::fabric::FABRICS`), not PE-DESIGN's.

## Where the previous menus went

| Previous menu item | Now |
|---|---|
| File ▸ New, Open, Save, Save As, Print, Export, Quit | Application menu (Print ▸ Print: system print dialog or Save PDF; Export ▸, Exit) |
| File ▸ Import into design, Import SVG (also WMF/EMF) | Home ▸ Import |
| File ▸ Background Image (choose, paste, show/hide, opacity, remove) | Image ▸ Open Image, Paste Image, Background Image Density, Delete Image |
| File ▸ Export Sections, Send to USB Media | Home ▸ Sew ▸ Sew Embroidery; Hoop Settings tab; Application menu ▸ Export ▸ |
| (beyond PE-DESIGN) Garment mockup (#454) | Application menu ▸ Export ▸ Garment Mockup ▸ fabric colour, then T-shirt (centre chest, left chest), Cap or Tote bag: a PNG of the design at true size on the garment (`design.mockup`) |
| (beyond PE-DESIGN) Default thread chart (#796) | Colour picker (Sewing Attributes ▸ Colour, Home ▸ Line/Region Color, Change Color): the default chart's swatches (New Brothread until changed) with a search by code, name or Madeira code and a Brother PEC tab beside it; Options ▸ User Thread Charts ▸ Default chart changes it (`threads.setDefault`) |
| (beyond PE-DESIGN) Thread codes on swatches (#798) | Every colour list (picker grid or List, picker button, Sewing Order, simulator bar, Thread Colours pane, Change Color, colour matching, user charts, Design Database, image wizards): the thread's code under or beside its swatch, full detail on hover |
| File ▸ Design Database | Options ▸ Design Database |
| Edit ▸ Undo, Redo | Quick Access Toolbar (⌘/Ctrl+Z, ⌘/Ctrl+Y) |
| Edit ▸ Select All, Duplicate, Delete, Arrange Copies | Home ▸ Select, Clipboard |
| Edit ▸ Group, Ungroup | Home ▸ Edit |
| Edit ▸ Copy / Paste Attributes | Sewing Attributes pane |
| Edit ▸ Sewing Attribute Sets, User Thread Charts | Options ▸; Application menu ▸ Select Color Palette |
| Shapes ▸ Insert presets | Home ▸ Tools ▸ Shapes ▸ Insert Preset Shape |
| Shapes ▸ Merge, Remove Overlap, Combine | Home ▸ Edit ▸ Modify Overlap |
| Shapes ▸ Split with Outline, Sewing Direction, Entry/Exit Points | Shapes tab ▸ Split Outline, Add/Delete Direction Line, Center/Reference Point, Entry/Exit Points (R1 region entry, R2 region exit, L1 line entry, L2 line exit since #547: picked on the outline, their markers dragged on the canvas) |
| Shapes ▸ Offset Lines, Floral Pattern | Shapes tab ▸ Create |
| Shapes ▸ Decorative Frame, Stamp/Emboss | Home ▸ Edit |
| Lettering ▸ Text, Monogram, Name Drop List, Template Wizard, Save as Template, Fonts, Load TrueType Font | Home ▸ Tools ▸ Text ▾ (and Text tab, Text Attributes pane) |
| Convert ▸ Convert to Stitch Pattern, Region to Line, to Outline/Blocks | Text, Shapes and Stitches tabs |
| Stitches ▸ Divide by Colour, Split at Point, Connect, Split Off Area | Stitches tab |
| Design ▸ Design Settings, Image to Embroidery, pattern wizards | Application menu ▸ Design Settings, Wizard ▸; Image tab; Home ▸ Wizards |
| Tools ▸ Programmable Stitch Creator, Stitch Design Factory | Options ▸ |
| View ▸ Fit, travel, realistic, rulers, grid, snap to grid, grid spacing, reference window, measure | Home ▸ Tools ▸ Zoom ▾, Measure; View tab |
| Toolbar tools and hints | Home ▸ Select / Tools, contextual tabs; hints in the status bar |
| (none in PE-DESIGN) Command palette | Ctrl+K (Cmd+K): a box over the canvas; type words, Up/Down, Return runs the command as its button would; recent commands first; disabled ones greyed with the reason; a menu command brings its ribbon tab forward (#550) |
| (none in PE-DESIGN) Property search | A search box at the top of the Sewing Attributes pane: only the rows whose label or parameter match stay (#550) |
| (none in PE-DESIGN) Autosave and recovery | Options ▸ Autosave for crash recovery (on), Autosave at most every N s, Clear Recovery Files; at start, a Recover Designs window lists designs a crashed window left: Restore (untitled, "recovered from …" in the title), Discard, Later (#549) |

## Commands by tab

### Home

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select | `V` | `Action::SelectTool` | view/UI only | yes |
| Select | Point Edit | `A` | `Action::PointTool` | `object.setPoint` | yes |
| Select | Select All | `Ctrl+A` | `Action::SelectAll` | view/UI only | yes |
| Select | Select Same ▾ (Same Thread Colour, Same Stitch Type, Same Colour and Type; beyond PE-DESIGN, #475) |  | `Action::SelectSame`, `SelectSameColor`, `SelectSameType`, `SelectSameBoth` | `object.list` (`sameAs`, `match`) | yes |
| Clipboard | Paste | `Ctrl+V` | `Action::Paste` | `object.duplicate` | yes |
| Clipboard | Cut | `Ctrl+X` | `Action::Cut` | `object.delete`, then `object.paste` on Paste | yes |
| Clipboard | Copy | `Ctrl+C` | `Action::Copy` | view/UI only | yes |
| Clipboard | Duplicate | `Ctrl+D` | `Action::Duplicate` | `object.duplicate` | yes |
| Clipboard | Delete | `Delete` | `Action::Delete` | `object.delete` | yes |
| Clipboard | Arrange Copy (mirror, circle, matrix; beyond PE-DESIGN, #476: Along a line, Scatter in shape with Shuffle, Mirror every other on the circle) |  | `Action::ArrangeCopy` | `object.arrangeCopies` | yes |
| Tools | Text |  | `Action::TextTools` | `text.add` | yes |
| Tools | Shapes |  | `Action::ShapesTools` | `shape.preset` | yes |
| Tools | Zoom In | `Ctrl+=` | `Action::ZoomIn` | view/UI only | yes |
| Tools | Zoom Out | `Ctrl+-` | `Action::ZoomOut` | view/UI only | yes |
| Tools | Zoom (Fit Design Page, Zoom to Selection, Zoom Box (Z, beyond PE-DESIGN, #518), 50–400 %, Calibrate Real Size… (#518)) | `Ctrl+0` | `Action::OtherZoom` | view/UI only | yes |
| Tools | Pan | `H` | `Action::Pan` | view/UI only | yes |
| Tools | Measure | `M` | `Action::Measure` | `design.measure` | yes |
| Import | Import Patterns | `Ctrl+Shift+I` | `Action::ImportPatterns` | `stitches.import` | yes |
| Import | Import SVG |  | `Action::ImportSvg` | `import.svg`, `import.metafile` for `.wmf`/`.emf` | yes |
| Edit | Arrange |  | `Action::Arrange` | `order.move` | yes |
| Edit | Align ▾ (Relative to Selection / Last Selected / Design Page; Align Left, Centre, Right, Top, Middle, Bottom; Distribute Horizontally, Vertically; beyond PE-DESIGN, #472) |  | `Action::Align`, `AlignLeft` … `AlignBottom`, `DistributeHorizontal`, `DistributeVertical` | `object.align`, `object.distribute` | yes |
| Edit | Rotate/Flip |  | `Action::RotateFlip` | `object.rotate` | yes |
| Edit | Size |  | `Action::Size` | `object.scale` | yes |
| Edit | Group | `Ctrl+G` | `Action::Group` | `object.group` | yes |
| Edit | Ungroup | `Ctrl+Shift+G` | `Action::Ungroup` | `object.ungroup` | yes |
| Edit | Fit Text ▾ (Fit Text to Outline…, Release Text from Outline) |  | `Action::FitTextToOutline` | `text.setPath` | yes |
| Edit | Stamp/Emboss |  | `Action::StampEmboss` | `shape.emboss` | yes |
| Edit | Modify Overlap |  | `Action::ModifyOverlap` | `shape.combine` | yes |
| Edit | Decorative Frame |  | `Action::DecorativeFrame` | `shape.decorative` | yes |
| Wizards | Background fill |  | `Action::BackgroundFill` | `wizard.backgroundFill` | yes |
| Wizards | Applique Wizard |  | `Action::Applique` | `wizard.applique` | yes |
| Wizards | Embroidered Patch Wizard |  | `Action::Patch` | `wizard.patch` | yes |
| Wizards | Cutwork Wizard |  | `Action::Cutwork` | `wizard.cutwork` | yes |
| Sew | Sew Embroidery |  | `Action::SendEmbroidery` | `transfer.send` | yes |

### Image

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Image | Open Image |  | `Action::OpenImage` | `background.set` | yes |
| Image | Paste Image |  | `Action::PasteImage` | `background.set` | yes |
| Image | Save Image |  | `Action::SaveImage` | `background.save` | yes |
| Image | Modify Image |  | `Action::ModifyImage` | `background.modify` | yes |
| Image | Background Image Density |  | `Action::ImageDensity` | `background.show` | yes |
| Image | Rotate Right |  | `Action::ImageRotateRight` | `background.rotate` | yes |
| Image | Rotate Left |  | `Action::ImageRotateLeft` | `background.rotate` | yes |
| Image | Delete Image |  | `Action::DeleteImage` | `background.clear` | yes |
| Image to Stitch | Auto Punch Wizard |  | `Action::AutoPunch` | `digitize.autoPunch` | yes |
| Image to Stitch | Cross Stitch Wizard |  | `Action::CrossStitch` | `digitize.crossStitch` | yes |
| Image to Stitch | Photo Stitch 1 Wizard |  | `Action::PhotoStitch1` | `digitize.photoStitch` | yes |
| Image to Stitch | Photo Stitch 2 Wizard |  | `Action::PhotoStitch2` | `digitize.lineImage` | yes |
| Design Center | Import to Design Center |  | `Action::DesignCenter` | `digitize.designCenter` | yes |

### View

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| View | Solid View |  | `Action::SolidView` | view/UI only | yes |
| View | Stitch View |  | `Action::StitchView` | view/UI only | yes |
| View | Realistic View |  | `Action::RealisticView` | view/UI only | yes |
| View | View Thread Trimming |  | `Action::ViewThreadTrimming` | view/UI only | yes |
| View | Show ▾ (beyond PE-DESIGN, #517): Needle Points; Underlay and top / Underlay only / Top only; Trim Marks; Hide Unselected; Colour by Thread / Sewing order / Monochrome; Solo Colour |  | `Action::ViewShow` | view/UI only | yes |
| View | Realistic View Settings |  | `Action::RealisticSettings` | view/UI only | yes |
| Show/Hide | Sewing Order |  | `Action::PaneSewingOrder` | view/UI only | yes |
| Sewing Order pane | Knockdown / Update knockdown (beyond PE-DESIGN, #510): a knockdown fill under the selection or the whole design with the default settings; on a selected knockdown, rebuild it |  | Sewing Order pane button | `wizard.knockdown` | yes |
| Show/Hide | Stitch Simulator |  | `Action::PaneSimulator` | view/UI only | yes |
| Show/Hide | Attributes Setting |  | `Action::PaneAttributes` | view/UI only | yes |
| Show/Hide | Import Pane |  | `Action::PaneImport` | view/UI only | yes |
| Show/Hide | Assistant ➕ | Ctrl+Shift+A | `Action::PaneAssistant` | view/UI only; the panel runs engine and `ui.*` commands (B17) | yes |
| Show/Hide | Precision Bar (beyond PE-DESIGN, #474): a row under the ribbon with the selection's X, Y (top-left or centre), W, H, Lock aspect, Rotate and Skew; Enter commits, Esc reverts |  | `Action::PrecisionBar` | `object.move`, `object.scale`, `object.rotate`, `object.skew` | yes |
| Show/Hide | Reference Window |  | `Action::ReferenceWindow` | view/UI only | yes |
| Grid | Show Grid |  | `Action::ShowGrid` | view/UI only | yes |
| Grid | Show Grid with Axes |  | `Action::GridAxes` | view/UI only | yes |
| Grid | Snap to Grid |  | `Action::SnapToGrid` | view/UI only | yes |
| Grid | Snap to Objects (beyond PE-DESIGN, #473): a dragged selection snaps to other objects' edges and centres, the page and the guidelines, with dashed guides; Ctrl (Cmd) drags freely |  | `Action::SnapToObjects` | view/UI only (the drag ends as one `object.move`) | yes |
| Grid | Grid Spacing |  | `Action::GridSpacing` | view/UI only | yes |
| Ruler | Ruler |  | `Action::Ruler` | view/UI only | yes |
| Guideline | Guideline |  | `Action::Guideline` | `guides.set` | yes |
| Guideline | Guides ▾: Lock Guides, Set Position…, Clear Selected Guide, Clear Guides, New Guides From Selection (#799) |  | `Action::GuideOptions` | `view.lockGuides`, `view.editGuide`, `view.deleteGuide`, `view.clearGuides`, `view.newGuidesFromSelection` | yes |

### Text (contextual: text selected)

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select | `V` | `Action::SelectTool` | view/UI only | yes |
| Select | Point Edit | `A` | `Action::PointTool` | `object.setPoint` | yes |
| Select | Select All | `Ctrl+A` | `Action::SelectAll` | view/UI only | yes |
| Text | Text |  | `Action::TextTools` | `text.add` | yes |
| Text | Font |  | `Action::Font` | `object.setText` | yes |
| Text | Text Size |  | `Action::TextSize` | `object.setText` | yes |
| Text | Font Filter |  | `Action::FontFilter` | view/UI only | yes |
| Text | Font Filter Settings |  | `Action::FontFilterSettings` | view/UI only | yes |
| Text | TrueType Font Attribute Setting |  | `Action::TrueTypeAttributes` | `fonts.loadTrueType` | yes |
| Color / Sew Type | Line color |  | `Action::LineColor` | `object.setLine` | yes |
| Color / Sew Type | Line sew type |  | `Action::LineSewType` | `object.setLine` | yes |
| Color / Sew Type | Region color |  | `Action::RegionColor` | `object.setRegion` | yes |
| Color / Sew Type | Region sew type |  | `Action::RegionSewType` | `object.setRegion` | yes |
| Edit | Edit Text |  | `Action::EditText` | `object.setText` | yes |
| Edit | Fit Text ▾ (Fit Text to Outline…, Release Text from Outline) |  | `Action::FitTextToOutline` | `text.setPath` | yes |
| Edit | Edit Character |  | `Action::EditCharacter` | `text.setCharacter` | yes |
| Edit | Convert to Blocks |  | `Action::ConvertToBlocks` | `object.toShapes` | yes |
| Edit | Convert to Stitches |  | `Action::ConvertToStitches` | `object.toStitches` | yes |
| Edit | Convert to Outline |  | `Action::ConvertToOutline` | `object.toShapes` | yes |

### Outline — Shapes (Outline) (contextual: an Outline tool: straight line, curve or pencil)

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select | `V` | `Action::SelectTool` | view/UI only | yes |
| Select | Point Edit | `A` | `Action::PointTool` | `object.setPoint` | yes |
| Select | Select All | `Ctrl+A` | `Action::SelectAll` | view/UI only | yes |
| Shapes | Closed Straight Line | `P` | `Action::ClosedStraight` | `shape.polygon` | yes |
| Shapes | Closed Curve | `B` | `Action::ClosedCurve` | `shape.path` | yes |
| Shapes | Pencil (Closed Curve) |  | `Action::PencilClosed` | `shape.freehand` | yes |
| Shapes | Open Straight Line | `L` | `Action::OpenStraight` | `shape.polygon` (`closed: false`) | yes |
| Shapes | Open Curve |  | `Action::OpenCurve` | `shape.path` | yes |
| Shapes | Pencil (Open Curve) |  | `Action::PencilOpen` | `shape.freehand` (`closed: false`) | yes |
| Color / Sew Type | Line color |  | `Action::LineColor` | `object.setLine` | yes |
| Color / Sew Type | Line sew type |  | `Action::LineSewType` | `object.setLine` | yes |
| Color / Sew Type | Region color |  | `Action::RegionColor` | `object.setRegion` | yes |
| Color / Sew Type | Region sew type |  | `Action::RegionSewType` | `object.setRegion` | yes |

### Outline — Shapes (Rectangle) (contextual: Rectangle tool)

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select | `V` | `Action::SelectTool` | view/UI only | yes |
| Select | Point Edit | `A` | `Action::PointTool` | `object.setPoint` | yes |
| Select | Select All | `Ctrl+A` | `Action::SelectAll` | view/UI only | yes |
| Shapes | Rectangle | `R` | `Action::Rectangle` | `shape.rectangle` | yes |
| Shapes | Edge radius |  | `Action::EdgeRadius` | `shape.rectangle` (`cornerRadius`) | yes |
| Color / Sew Type | Line color |  | `Action::LineColor` | `object.setLine` | yes |
| Color / Sew Type | Line sew type |  | `Action::LineSewType` | `object.setLine` | yes |
| Color / Sew Type | Region color |  | `Action::RegionColor` | `object.setRegion` | yes |
| Color / Sew Type | Region sew type |  | `Action::RegionSewType` | `object.setRegion` | yes |

### Manual Punch (contextual: a Manual Punch tool)

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select | `V` | `Action::SelectTool` | view/UI only | yes |
| Select | Point Edit | `A` | `Action::PointTool` | `object.setPoint` | yes |
| Select | Select All | `Ctrl+A` | `Action::SelectAll` | view/UI only | yes |
| Manual Punch | Straight Block | `U` | `Action::StraightBlock` | `shape.column` | yes |
| Manual Punch | Curved Block |  | `Action::CurvedBlock` | `shape.column` (`curved: true`) | yes |
| Manual Punch | Running |  | `Action::Running` | `shape.polygon` (`closed: false`) | yes |
| Manual Punch | Feed |  | `Action::Feed` | `shape.feed` | yes |
| Manual Punch | Satin Skeleton |  | `Action::SatinSkeleton` | `shape.skeletonSatin`; `object.setSkeleton` (editing) | yes |
| Manual Punch | Column width |  | `Action::SkeletonWidth` | `shape.skeletonSatin` (`widths`) | yes |
| Color / Sew Type | Region color |  | `Action::RegionColor` | `object.setRegion` | yes |
| Color / Sew Type | Region sew type |  | `Action::RegionSewType` | `object.setRegion` | yes |

### Shapes — Shapes/Circle/Arc (contextual: shape selected, Circle or Point Edit tool)

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select | `V` | `Action::SelectTool` | view/UI only | yes |
| Select | Point Edit | `A` | `Action::PointTool` | `object.setPoint` | yes |
| Select | Select All | `Ctrl+A` | `Action::SelectAll` | view/UI only | yes |
| Edit Outline | Split Outline |  | `Action::SplitOutline` | `shape.split` | yes |
| Edit Outline | Knife (beyond PE-DESIGN, #477) |  | `Action::Knife` | `shape.knife` | yes |
| Edit Outline | Add Direction Line |  | `Action::AddDirectionLine` | `object.setDirectionCurve` | yes |
| Edit Outline | Delete Direction Line |  | `Action::DeleteDirectionLine` | `object.setDirectionLines` | yes |
| Edit Outline | Reverse Direction Line |  | `Action::ReverseDirectionLine` | `object.reverseDirection` | yes |
| Edit Outline | Edit Point | `A` | `Action::EditPoint` | `object.setPoint` | yes |
| Edit Outline | Edit Point on a satin line (beyond PE-DESIGN, #700): each width key shows a handle at both edges; drag one to set that side, double-click the line to add a key, click a handle and press Delete to remove the key |  | `Action::EditPoint` | `object.setWidthKey`, `object.deleteWidthKey` | yes |
| Edit Outline | Edit Point on a Manual Punch column (beyond PE-DESIGN, #525): the rungs are shown instead of the outline; drag an end, double-click a side to add a rung, click an end and press Delete to remove it |  | `Action::EditPoint` | `object.moveRung`, `object.insertRung`, `object.deleteRung` | yes |
| Edit Outline | Delete Point | `Delete` | `Action::DeletePoint` | `object.deletePoint` | yes |
| Edit Outline | Split at Point |  | `Action::SplitAtPoint` | `stitches.splitAt` | yes |
| Edit Outline | Satin Skeleton from Shape |  | `Action::DeriveSkeleton` | `object.deriveSkeleton` | yes |
| Edit Outline | Smooth Shape (beyond PE-DESIGN, #478) |  | `Action::SmoothShape` | `shape.simplify` | yes |
| Edit Outline | Smooth tolerance (beyond PE-DESIGN, #478) |  | `Action::SmoothTolerance` | `shape.simplify` (`tolerance`) | yes |
| Edit Outline | Shape Effects (beyond PE-DESIGN, #497) |  | `Action::ShapeEffects` | `shape.effect` | yes |
| Edit Outline | Shape Builder (beyond PE-DESIGN, #497) |  | `Action::ShapeBuilder` | `shape.build` | yes |
| Edit Outline | Turning Gap (beyond PE-DESIGN, #497) |  | `Action::LineGap` | `object.setLineGap` | yes |
| Color / Sew Type | Line color |  | `Action::LineColor` | `object.setLine` | yes |
| Color / Sew Type | Line sew type |  | `Action::LineSewType` | `object.setLine` | yes |
| Color / Sew Type | Region color |  | `Action::RegionColor` | `object.setRegion` | yes |
| Color / Sew Type | Region sew type |  | `Action::RegionSewType` | `object.setRegion` | yes |
| Convert | Convert Region to Line |  | `Action::ConvertRegionToLine` | `object.regionToLine` | yes |
| Convert | Convert to Stitches |  | `Action::ConvertToStitches` | `object.toStitches` | yes |
| Create | Create Offset Lines |  | `Action::OffsetLines` | `shape.offsetLines` | yes |
| Create | Create Floral Pattern |  | `Action::FloralPattern` | `shape.floral` | yes |
| Create | Entry/Exit Points |  | `Action::EntryExit` | `object.setEntryExit` | yes |
| Create | Center/Reference Point |  | `Action::SewingPoints` | `object.setCenterPoint` | yes |

### Stitches (contextual: stitch pattern selected)

**Stitch Table** (beyond PE-DESIGN, #501): the Sewing Attributes pane's **Stitch Table** button
opens a window listing every needle point of the selected stitch pattern (index, X and Y in mm,
command, colour block). Type or drag X and Y to move a point exactly; **Insert** adds a point
halfway along the line into the first selected row, **Delete** removes the selected rows; each
is one Undo. Click, Ctrl+click and Shift+click select rows, which the Point tool shows on the
canvas. A selected shape or text (a fill, satin, line or lettering) has the same table: editing
its points keeps it a live object with **stitches edited by hand**, sewn while its settings and
shape stay as they are (moving it keeps them); a sewing setting change, a reshape, a rotation
or a resize discards them with a message, and **Regenerate** (in the table and in Sewing
Attributes) drops them on purpose.

![Stitch Table](images/rust-editor-stitch-table.png)

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select | `V` | `Action::SelectTool` | view/UI only | yes |
| Select | Point Edit | `A` | `Action::PointTool` | `object.setPoint` | yes |
| Select | Select All | `Ctrl+A` | `Action::SelectAll` | view/UI only | yes |
| Edit | Split Stitch Data |  | `Action::SplitStitchData` | `stitches.splitArea` | yes |
| Edit | Change Color |  | `Action::ChangeColor` | `stitches.setThread` | yes |
| Edit | Divide by Color |  | `Action::DivideByColor` | `stitches.divideByColor` | yes |
| Edit | Split at Point |  | `Action::SplitAtPoint` | `stitches.splitAt` | yes |
| Edit | Connect |  | `Action::Connect` | `stitches.connect` | yes |
| Edit | Smooth Stitches (beyond PE-DESIGN, #478) |  | `Action::SmoothStitches` | `stitches.smooth` | yes |
| Convert | Convert to Blocks |  | `Action::ConvertToBlocks` | `object.toShapes` | yes |
| Convert | Convert to Outline |  | `Action::ConvertToOutline` | `object.toShapes` | yes |
| Convert | Convert to Outlines |  | `Action::ConvertToOutlines` | `object.toShapes` | yes |

### Hoop Settings (contextual: design larger than the hoop)

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select | `V` | `Action::SelectTool` | view/UI only | yes |
| Select | Point Edit | `A` | `Action::PointTool` | `object.setPoint` | yes |
| Select | Select All | `Ctrl+A` | `Action::SelectAll` | view/UI only | yes |
| Hoop Settings | Move Sections |  | `Action::MoveSections` | `sections.set` (`splitsX` / `splitsY`) | yes |
| Hoop Settings | Design Settings (hoop list grouped by brand, Brother first, with a filter; other brands and round hoops beyond PE-DESIGN, #514; an oval or round hoop's field is drawn on the canvas) |  | `Action::DesignSettings` | `machine.set`, `machine.hoops` | yes |
| Hoop Settings | Export Sections (free hoopings: "Name - hooping k of n") |  | `Action::ExportSections` | `sections.export` | yes |
| Design Settings | Split design ▸ Hoopings: Grid (PE-DESIGN) or Free (#515, beyond PE-DESIGN): hooping list (x, y, Turned 90°, Remove), Add left / right / up / down, Auto layout, Marks (Boxed X, Cross, None), overlap, Stop after the marks; objects that fit no hooping outlined red on the canvas |  | `Action::DesignSettings` | `sections.set` (`mode`, `hoopings`, `marks`, `minOverlap`, `stopAfterMarks`), `sections.autoLayout` | yes |
| Design Settings | Production (beyond PE-DESIGN, #509, #512, #547): Basting box (Box / Follow outline, margin, stitch, Own thread), Stop at jumps (machine without thread cutter), Start each object nearest the last, Travel under later fills (#545), Leave out hidden stitching (#511) |  | `Action::DesignSettings` | `design.setBasting`, `design.setStopAtJumps`, `design.setProperties` (`autoStart`), `design.setTravelUnder`, `design.setRemoveHidden` | yes |

### How the drawing and editing commands work

Behaviour follows the guide pages
[Drawing outlines](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_1835552.html),
[Using Manual Punch Tool](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/07_1892457.html),
[Specifying the straight sewing direction](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/04_1869360.html)
and [Adjusting the split positions](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/07_1925948.html).

- **Keyboard drawing** (beyond PE-DESIGN, #499): with a drawing tool, an arrow key shows a
  cross-hair cursor and moves it (the nudge step, Shift ten times it); **Space** clicks there,
  **Shift+Space** places a smooth Pen or Satin Skeleton point, **Return** finishes, **Esc**
  cancels and **Backspace** takes back the last point. Rectangle and Ellipse: the first Space
  starts the shape, the second ends it. While the Pen or Satin Skeleton draws, **[** and **]**
  (or the scroll wheel; Ctrl+scroll still zooms) narrow and widen the next point by 0.5 mm, or
  the point under the pointer; a Pen line drawn with widths becomes a satin line (#707).
  ![Keyboard drawing](images/rust-editor-keyboard-draw.png)
- **Cut** removes the selection (one Undo) and keeps it; **Paste** adds it back where it was
  (`object.paste`). After Copy, Paste duplicates the copied objects as before.
- **Open Straight Line** (`L`): click the points; double-click or Return ends the line (it never
  closes on the first point). **Pencil**: press and drag; releasing ends the stroke. The trail
  is simplified to within about 1.5 screen points and smoothed into Bézier curves; turns
  sharper than 75° stay corners (`shape.freehand`, `tolerance`).
- **Edge radius** sets the corner radius of the rectangles drawn next with the Rectangle tool.
- **Curved Block** is clicked like Straight Block (both ends of each direction line); the sides
  curve through the ends and the stitching follows them. **Running** and **Feed**: click the
  points, double-click or Return ends; Running makes a running-stitch line, Feed a pattern of
  jump stitches only (the frame moves, the thread is not cut). Each Manual Punch part is its
  own pattern; switching tools ends the part (the guide switches within one input).
- **Reverse Direction Line** reverses every direction line of the selected shape (or turns a
  single sewing direction by 180°).
- **Satin Skeleton** (Manual Punch tab; OpenStitch's own tool, PE-DESIGN has no direct
  equivalent): click the centre line of a satin column like the Closed/Open Curve pen (click
  for a corner, drag for a smooth point); every point carries a column width, shown as a
  handle on either side of the line, and dragging a handle sets that point's width. New points
  take **Column width**. Double-click or Return ends the line as one satin object sewn
  perpendicular to it (`shape.skeletonSatin`; curves are flattened, widths interpolated
  between the points). With nothing being drawn the tool shows the selected object's
  skeleton: dragging a width handle or a centre point is one `object.setSkeleton` (the outline
  follows when it is the column's own).
- **Satin Skeleton from Shape** (Shapes ▸ Edit Outline) sews the selected shape as satin along
  a centre line derived from its outline — an approximation of the medial axis (grid
  clearance, the longest path through the shape, corner ends trimmed; branches are not
  followed). Edit the result with the Satin Skeleton tool.
- **Continue a line** (beyond PE-DESIGN, #479): with the Closed/Open Curve pen, Open Straight
  Line, Running, Straight/Curved Block or Satin Skeleton tool active and one finished open
  line, path, column or skeleton of that kind selected, the pointer becomes a hand within 6
  screen points of either end. Click there and keep adding points (or direction lines);
  Return or a double-click adds them to the same object (`object.extendPath`,
  `object.extendColumn`, one Undo), Esc leaves it unchanged. On a skeleton, a click without a
  drag on an end point continues it (a drag still moves the point), and new points keep the
  end's width. Closed outlines are not offered.
- **Smooth Shape** (Shapes ▸ Edit Outline, beyond PE-DESIGN, #478) removes the nodes of the
  selected shapes that deviate less than **Smooth tolerance** (0.2 mm): curves are refitted,
  corners and open ends kept, a Manual Punch column loses one direction line per click. The
  status line says how many nodes went. **Smooth Stitches** (Stitches ▸ Edit) evens out the
  selected needle points' span, or the whole pattern, keeping jumps, trims and run ends.
- **Knife** (Shapes ▸ Edit Outline, beyond PE-DESIGN, #477): click the points of an open cut
  line (Shift: 45° steps); a double-click or Return cuts every visible object the line crosses
  in one Undo (`shape.knife`): fills and closed outlines into both sides, Manual Punch columns
  and satin skeletons across the column (the cut is a direction line of both halves), lines
  where crossed, a closed outline cut once opening there. A single click then Return (or a
  double-click with no line) splits the line under the pointer. The tool stays active for more
  cuts; Esc drops the line, and with none leaves the tool. Split Outline is unchanged.
- **More shapes** (beyond PE-DESIGN, #497): Home ▸ Tools ▸ Shapes adds Frame, Round frame,
  Rounded frame and Heart frame (a band around an open middle, `shape.preset kind: frame`) and
  Spiral line (`kind: spiral`). Shapes (Rectangle) ▸ Edge radius ▸ Different radius per corner
  sets four radii (`cornerRadii`). Home ▸ Edit ▸ Modify Overlap ▸ Divide cuts every overlap of
  the selected shapes into its own shape, sewn like the topmost one (`shape.combine op:
  divide`). Shapes ▸ Edit Outline ▸ Shape Effects warps, twists, roughens, puckers or bloats, or
  zigzags the selected outlines, with a preview of the result before Apply (`shape.effect`).
- **Shape Builder** (Shapes ▸ Edit Outline, beyond PE-DESIGN, #497): with two or more
  overlapping shapes selected, drag across the pieces of their overlaps; on release the pieces
  touched become one shape, sewn like the topmost of them (`shape.build`, one Undo); hold Alt
  to remove them instead. **Turning Gap** leaves part of the selected closed running, triple,
  zigzag, E or V line unsewn: where it starts along the outline and how long it is
  (`object.setLineGap`); the line is tied in and locked at both ends of the gap.
- **Change Color** lists the selected stitch pattern's threads; pick one and its new colour.
- **Editing characters** (after the guide's
  [Editing individual characters](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/05_1904502.html)):
  with **Point Edit** on the selected text, click a character to select it; its box gets the
  usual handles. Drag inside it to move the character (Shift locks one axis), the top handle
  to turn it about its middle (Shift: 15° steps), any other handle to resize it (its capital
  height). Each drag is one `text.setCharacter`, so one Undo; moves follow the text's own
  direction when it is rotated, mirrored, on a path or in a Name Drop box. **Text ▸ Edit
  Character** shows the selected character's offset, rotation and size to type, and resets
  it. One character at a time (the guide also selects several).
- **Move Sections** (on/off) lets you drag the red split lines of a split design; releasing
  sets the new split position. Positions that would make a section larger than the hoop
  allows are refused and the status bar says why.

### Application menu

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| New | `Ctrl+N` | `Action::New` | `design.new` | yes |
| Open | `Ctrl+O` | `Action::Open` | `project.open` | yes |
| Save | `Ctrl+S` | `Action::Save` | `project.save` | yes |
| Save As | `Ctrl+Shift+S` | `Action::SaveAs` | `project.save` | yes |
| Print |  | `Action::Print` | `design.print` | yes |
| Design Property |  | `Action::DesignProperty` | `design.info` | yes |
| Design Settings (hoop list grouped by brand, Brother first, with a filter; other brands and round hoops beyond PE-DESIGN, #514; an oval or round hoop's field is drawn on the canvas) |  | `Action::DesignSettings` | `machine.set`, `machine.hoops` | yes |
| Fabric Selector |  | `Action::FabricSelector` | `fabric.apply` | yes |
| Select Color Palette |  | `Action::SelectColorPalette` | `threads.charts` | yes |
| Wizard |  | `Action::Wizard` | view/UI only | yes |
| Export |  | `Action::Export` | `design.export` | yes |
| Exit |  | `Action::Exit` | view/UI only | yes |

**Unsaved changes.** The title bar shows a `•` after the design's name while it has changes
that are not saved. Closing the window (the X button, Alt+F4, File ▸ Exit), File ▸ New and
File ▸ Open (also a recent file, or Open in Layout & Editing from the Design Database) then
ask "Save changes to NAME?" ([screenshot](images/rust-editor-close-prompt.png)): **Save**
(Enter) saves (Save As when the design has no file yet) and goes on, **Don't Save** goes on
and discards the changes, **Cancel** (Esc) returns to the editor. A cancelled Save As returns
to the editor too. As PE-DESIGN 11 does. On macOS Cmd+Q and Cmd+W close through the same
prompt, also while the Design Database, Stitch Creator or Stitch Design Factory fills the
window: the editor installs no default application menu (its Quit would terminate the app
without asking, and its items were named after the process, "Quit openstitch"), so Cmd+M
minimises through the editor and Cmd+H, About, Services and Hide Others are not available
from a menu (Help ▸ About OpenStitch stays; see docs/DISTRIBUTION.md "macOS application
menu").

### Designs as tabs (beyond PE-DESIGN, #552)

Several designs can be open in one window. A tab strip under the ribbon shows one tab per
design (its file or design name, `•` while it has unsaved changes, **Close**) and **New Tab**;
it appears when two designs are open or **Options ▸ Open designs in tabs** is on. Each tab keeps
its own undo history, selection, zoom and simulator; Ctrl+Tab and Ctrl+Shift+Tab move between
them. With Open designs in tabs on, File ▸ New and Open add a tab instead of replacing the
design; off (the default, as PE-DESIGN), they replace it after the unsaved-changes prompt.
Closing a tab with unsaved changes asks about that tab only; closing the window asks about
each tab with unsaved changes in turn. The Assistant uses `ui.documents`.

![Two designs as tabs](images/rust-editor-tabs.png)

**Large files open in the background (beyond PE-DESIGN, #558).** File ▸ Open, a recent file
and File ▸ Import run on a worker thread, so the window keeps repainting. After 0.3 s a progress
window shows the file, the step (reading the file, building the stitches, finishing), the
seconds so far and **Cancel** (Esc). Cancel, or an edit made meanwhile, leaves the current
design as it was; a finished open is one undo step, as before. The Assistant sees it as
`ui.state` `loading` and can stop it with `ui.loading {cancel: true}`.

### Text size and accessibility (beyond PE-DESIGN, #499)

**Options ▸ Text size** (100 %, 125 %, 150 %, 175 %, 200 %) enlarges the whole window on top of
the system's display scale; ribbon groups that no longer fit scroll under a solid scroll bar and
the status bar's controls move to a row of their own. Contrast, focus and screen-reader support:
[ACCESSIBILITY.md](ACCESSIBILITY.md).

### Language (#555)

**Options ▸ Language** chooses the interface language (`ui.set language`; `--language CODE` at
launch). The texts are looked up
by message id in `assets/i18n/<code>.ftl` (a subset of Project Fluent's syntax); English, the text
in the code, is the source and fills in any id a translation lacks. `assets/i18n/en.ftl` lists
every id with its English text and is generated from the code (a test fails when it is out of
date). So far the ribbon is translated (command labels and ScreenTips, tab names and captions, and
group captions) and, since part 2, the Application menu's lists, Options, Help and File ▸ Export
(their entries and tooltips; message ids `menu-…` in `crates/openstitch-app/src/editor/ui_texts.rs`),
and, since part 3, the panes (Sewing Order, Color, Sewing and Text Attributes, Import), the
Settings, Print, View (Fabric Selector, Realistic, Background Image, Font Filter, View ▸ Show),
Machine Setup and Startup Wizard dialogs (`ui-…`), and since part 4 every other dialog and pane
of the window (lettering, the image and pattern wizards, the Design Database, Stitch Creator,
editing, sections, swatches, updates, keyboard shortcuts, job sheet, transfer and the rest; 726
texts in all), and the window titles: each window has a fixed id, so a translated title keeps
its place. Since part 5, status messages are messages with variables in Fluent's syntax
(`{ $path }`, and plural choices such as `{ $n -> [one] { $n } design *[other] { $n } designs }`,
each language with its plural rule), filled in by `tf!("id", name = value)`; a test checks that
every call passes exactly its message's variables and that a translation uses only those. Since
part 6 every status message goes through them, and the printed sheets (job sheets, actual-size
and reduced prints, positioning sheets, Print and Stitch) print their words in the window's
language too (`openstitch_print::texts`, ids `print-…`, set whenever the language changes). Only
the first real translation is left: the owner chooses the language. **Pseudo-locale (test)** shows every
translated text accented and in brackets ("[Ópéñ]"), so text that is still hard-coded stands out.
No real translation ships yet.

### Machine Setup (beyond PE-DESIGN, #508)

**Help ▸ Machine Setup**, also shown once on the first interactive start with the user's own
settings: the brand (Brother first, then the other brands of the hoop list), a hoop of that brand,
whether the machine cuts threads, and, when its manual says, the longest stitch it sews. **Save**
stores it (`machine.setProfile`, user settings): New starts on that hoop, and every export warns
about stitches longer than the machine sews and about trims a machine without a cutter will not
cut (unless Stop at Jumps is on). The window names the brand's own file format. **Not now**
closes it; either way it is not asked again at start (`machineSetupDone` in `ui.json`), and Help
asks again any time. Without a profile everything is as before.

### Getting started (beyond PE-DESIGN, #554)

An empty design shows start cards over the Design Page (× at the top right, Escape or a click on
the canvas closes them for the session; **Don't show this again** at the bottom turns them off
until Options ▸ Show start cards, #797); the Application menu has **Open Sample**
under Open, and Help has **Getting Started** (the steps of docs/GETTING_STARTED.md, each with a
button). The Sewing Attributes pane has **Basic / Advanced** and **Save as Default** beside Copy
and Paste Attributes, and its Region and Line titles fold their sections (#553).

![Start cards](images/rust-editor-start-cards.png)

**Startup Wizard (PE-compatible start-up, #657).** Application menu ▸ Wizard ▸ **Startup Wizard**
opens PE-DESIGN 11's "How do you want to produce embroidery?" in its layout (from the
coordinator's black-box capture of PE-DESIGN 11.50, text only). Left column: Open Instruction
Manual; **Create With Ease** (Use Template Design, Create Embroidery Patterns Using Images);
**Set and Create New Page** (Set Hoop Size and Fabric); **Import Embroidery Patterns** (Text,
Outline Shapes, Design Library). Right column: **Most Recently Used Files** with Open, Open
Embroidery Design, Start Design Database to Manage Embroidery Files. Bottom: **Always show Wizard
at Startup** (`ui.set startupWizard`; ticked, it opens on every start and the start cards stay
hidden while it is open) and **Close This Wizard**. Every choice closes the wizard and runs the
matching OpenStitch action (`ui.startupWizard {choice}`):

| PE-DESIGN choice | OpenStitch action |
|---|---|
| Open Instruction Manual | Help ▸ Getting Started |
| Use Template Design | the Template Wizard |
| Create Embroidery Patterns Using Images | Image to Embroidery |
| Set Hoop Size and Fabric | Design Settings, then the Fabric Selector |
| Import: Text | the Text dialog (PE's Import pane category Text) |
| Import: Outline Shapes | the Import pane (OpenStitch's Import pane has no categories) |
| Import: Design Library | File ▸ Open Sample (the ready-to-sew kit) |
| Recent file, Open | opens it (asks about unsaved changes first) |
| Open Embroidery Design | File ▸ Open |
| Start Design Database | Options ▸ Design Database (OpenStitch's, in the editor) |

Unlike PE-DESIGN, a choice does not first replace the open design with an empty page; Open and
the samples ask about unsaved changes as usual. Recent files are listed by name, without PE's
stitch thumbnails.

**Picture pickers (beyond PE-DESIGN, #656).** With **Options ▸ Picture pickers with live
preview** on (off by default, PE-DESIGN's lists), the Sewing Attributes pane's Region and Line
Type, programmable fill and net patterns and motifs open as a grid of pictures: each is the
program's own stitches of that choice on a sample shape. Hovering a picture re-stitches the
selection on the canvas without changing the design or adding an undo step; moving off the
pictures brings the design back, and a click makes the change as one undo step. The Assistant
previews the same way with `ui.preview`.

![Picture picker with a hover preview](images/rust-editor-pickers.png)

### Option button

Beyond PE-DESIGN (#551, #559), after the theme: **Accent colour ▸** (Theme default, six presets,
Custom), **Layouts ▸** (each saved layout with Delete; the side each docked pane sits on; a name field
and Save Layout) and
**Reset Preferences ▸** (Display, Editing, Panes, Keyboard shortcuts, Saved layouts). Help ▸
Keyboard Shortcuts edits the shortcuts (Find, All commands, Add/remove/Reset per command, Reset
All, Export…, Import…; the keys that cannot change are listed below). In Layouts, Sewing Order
and Attributes dock left or right, the Stitch Simulator top or bottom.

![Accent colour, light theme](images/rust-editor-accent-light.png)

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Design Database |  | `Action::DesignDatabase` | `library.list` | yes |
| Programmable Stitch Creator |  | `Action::StitchCreator` | `pattern.define` | yes |
| Stitch Design Factory |  | `Action::StitchDesignFactory` | `sewing.define` | yes |
| Font Creator |  | `Action::FontCreator` | `fonts.create` | yes |
| Sewing Attribute Sets |  | `Action::AttributeSets` | `attributes.save` | yes |
| User Thread Charts |  | `Action::UserThreadCharts` | `threads.createChart` | yes |

### Quick Access Toolbar (default)

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| New | `Ctrl+N` | `Action::New` | `design.new` | yes |
| Open | `Ctrl+O` | `Action::Open` | `project.open` | yes |
| Save | `Ctrl+S` | `Action::Save` | `project.save` | yes |
| Undo | `Ctrl+Z` | `Action::Undo` | `history.undo` | yes |
| Redo | `Ctrl+Y` | `Action::Redo` | `history.redo` | yes |
| Zoom | `Ctrl+0` | `Action::OtherZoom` | view/UI only | yes |
| Help |  | `Action::Help` | view/UI only | yes |


## Other suite windows

Design Database, Programmable Stitch Creator and Stitch Design Factory are separate programs in
the suite OpenStitch is compatible with. OpenStitch opens each from the Option button
(**Options ▾**) and shows it in place of Layout & Editing until its **Exit** (Application menu or
toolbar) returns there; the window title names the program. Their commands follow the guide
pages linked under each heading and are the model in
`crates/openstitch-app/src/editor/suite.rs`; the binary draws them with the same ribbon code as
the editor (`ribbon::button`, `ribbon::group`, `ribbon::body`, `suite_chrome.rs`). As in the
editor, commands OpenStitch lacks are shown disabled with a "Not yet available in OpenStitch"
ScreenTip. `suite::tests` check every command has a place, a ScreenTip and an existing engine
command; the binary's `suite_chrome::tests` run every command (or draw its list) or check it is
marked unavailable. Group names are OpenStitch's where the guide shows the ribbon only as a
picture.

### Programmable Stitch Creator

Guide: [Programmable Stitch Creator Window](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/12_1155973.html),
[Application button menu](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/12_1170101.html),
[Ribbon menu tabs](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/12_1170222.html).

![Programmable Stitch Creator](images/rust-editor-stitch-creator.png)

| Guide element | OpenStitch |
|---|---|
| Application button | **File**: New Fill/Stamp Pattern, New Motif Pattern, New Decorative Fill Pattern, Open, Save (to the user library folder), Save As (an `openstitch.pattern` file), Exit |
| Ribbon | Home and View tabs below |
| Quick Access Toolbar | Open, Save, Add to Design |
| Help button | **Help ▾** at the top right |
| Design Page, work area | The tile (or motif cell) with its grid and the neighbouring repeats; Kind, Id, Name (and Row shift) above it |
| Preview window | Right: the pattern sewn in a region (View ▸ Show/Hide Preview window) |
| Status bar | Tool hint or the ScreenTip under the pointer; kind, lines, points, zoom |

OpenStitch additions: **New Line** (Home ▸ Draw) starts another line of a fill or decorative
pattern; **Add to Design** (Home ▸ Design) puts the pattern in the open design in one Undo step,
where the Sewing Attributes pane offers it; the **User library** pane at the left lists saved
patterns (Edit, Add to Design, Delete file). Cut, Copy, Paste, Duplicate, Delete and the flips
work on the current line.

The Region tools click a closed area of a fill/stamp tile (the tile wraps, so a line leaving one
side closes at the other): Engrave shades it red, Emboss blue, Reset clears it; the areas travel
in the pattern's `regions` (docs/FILE_FORMATS.md) and programmable fill sews them engraved or
embossed. Split at Point splits the line at the point last clicked (fill/stamp tiles). The
template is an image of the user's own (PNG, JPEG, BMP), shown behind the tile and never saved
in the pattern; Modify Template drags it (its corner scales it), Background Image Density offers
Hide, 25, 50, 75 and 100 % (F6 steps through them). Guideline shows rulers along the top and
left: a click adds a guideline (on the grid while it is shown, at most 100 per ruler), a drag
moves it, a right-click deletes it. Pan drags the magnified Design Page.

#### Home

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select Tool |  | `CreatorAction::SelectTool` | view/UI only | yes |
| Select | Edit Point Tool |  | `CreatorAction::EditPoint` | view/UI only | yes |
| Draw | Draw Line Tool |  | `CreatorAction::DrawLine` | view/UI only | yes |
| Draw | New Line |  | `CreatorAction::NewLine` | view/UI only | yes |
| Draw | Region (Reset) Tool |  | `CreatorAction::RegionReset` | view/UI only | yes |
| Draw | Region (Engrave) Tool |  | `CreatorAction::RegionEngrave` | view/UI only | yes |
| Draw | Region (Emboss) Tool |  | `CreatorAction::RegionEmboss` | view/UI only | yes |
| Zoom | Zoom In | `Ctrl+=` | `CreatorAction::ZoomIn` | view/UI only | yes |
| Zoom | Zoom Out | `Ctrl+-` | `CreatorAction::ZoomOut` | view/UI only | yes |
| Zoom | Other Zoom Tools |  | `CreatorAction::OtherZoom` | view/UI only | yes |
| Zoom | Pan |  | `CreatorAction::Pan` | view/UI only | yes |
| Clipboard | Paste | `Ctrl+V` | `CreatorAction::Paste` | view/UI only | yes |
| Clipboard | Cut | `Ctrl+X` | `CreatorAction::Cut` | view/UI only | yes |
| Clipboard | Copy | `Ctrl+C` | `CreatorAction::Copy` | view/UI only | yes |
| Clipboard | Duplicate | `Ctrl+D` | `CreatorAction::Duplicate` | view/UI only | yes |
| Clipboard | Delete | `Delete` | `CreatorAction::Delete` | view/UI only | yes |
| Edit | Flip vertically |  | `CreatorAction::FlipVertical` | view/UI only | yes |
| Edit | Flip horizontally |  | `CreatorAction::FlipHorizontal` | view/UI only | yes |
| Edit | Split at Point |  | `CreatorAction::SplitAtPoint` | view/UI only | yes |
| Design | Add to Design |  | `CreatorAction::AddToDesign` | `pattern.define` | yes |

#### View

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Template | Open Template |  | `CreatorAction::OpenTemplate` | view/UI only | yes |
| Template | Modify Template |  | `CreatorAction::ModifyTemplate` | view/UI only | yes |
| Template | Delete Image |  | `CreatorAction::DeleteImage` | view/UI only | yes |
| Template | Background Image Density |  | `CreatorAction::ImageDensity` | view/UI only | yes |
| Show/Hide | Show/Hide Preview window |  | `CreatorAction::PreviewWindow` | view/UI only | yes |
| Show/Hide | Show/Hide Status Bar |  | `CreatorAction::StatusBar` | view/UI only | yes |
| Grid | Show Grid with Axes |  | `CreatorAction::GridAxes` | view/UI only | yes |
| Grid | Grid interval |  | `CreatorAction::GridInterval` | view/UI only | yes |
| Guideline | Guideline |  | `CreatorAction::Guideline` | view/UI only | yes |

#### Application menu

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| New Fill/Stamp Pattern |  | `CreatorAction::NewFill` | view/UI only | yes |
| New Motif Pattern |  | `CreatorAction::NewMotif` | view/UI only | yes |
| New Decorative Fill Pattern |  | `CreatorAction::NewDecorative` | view/UI only | yes |
| Open | `Ctrl+O` | `CreatorAction::Open` | `pattern.import` | yes |
| Save | `Ctrl+S` | `CreatorAction::Save` | view/UI only | yes |
| Save As | `Ctrl+Shift+S` | `CreatorAction::SaveAs` | `pattern.export` | yes |
| Exit |  | `CreatorAction::Exit` | view/UI only | yes |

#### Quick Access Toolbar and buttons

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Open | `Ctrl+O` | `CreatorAction::Open` | `pattern.import` | yes |
| Save | `Ctrl+S` | `CreatorAction::Save` | view/UI only | yes |
| Add to Design |  | `CreatorAction::AddToDesign` | `pattern.define` | yes |
| Help |  | `CreatorAction::Help` | view/UI only | yes |

### Stitch Design Factory

Guide: [Stitch Design Factory Window](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/14_1181468.html),
[Application button menu](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/14_1209830.html),
[Ribbon menu tabs](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/14_1207694.html).

![Stitch Design Factory](images/rust-editor-stitch-design-factory.png)

| Guide element | OpenStitch |
|---|---|
| Application button | **File**: New ▸ (decorative, satin, decorative satin, cross), Open and Save As (`openstitch.sewing` files), Save (into the project, one Undo), Export (JSON, SVG, experimental PMV), Exit |
| Ribbon | Home and View tabs below |
| Quick Access Toolbar | Open, Save, Export |
| Options button | **Options ▾**: colour theme |
| Help button | **Help ▾** |
| Import pane | Left: the patterns kept in the project (Edit, Delete) |
| Preview pane | Bottom: the pattern repeated along a seam |
| Design area | The pattern with its width band, grid (Grid Spacing), the end of the first repeat and two more repeats, the start (blue square) and end (red square) points and the template image behind; landscape, or portrait with Horizontal Orientation off; Kind, Id, Name, Width and the kind's settings above it |
| Status bar | Tool hint or the ScreenTip under the pointer; template and its density, needle points, length, width, grid spacing, zoom |
| Stitch Simulator | A window: the editor simulator's controls (Play/Pause, start, previous/next needle point, end, slider, speed) sewing three repeats needle by needle |

Single Stitch and Triple Stitch apply to cross patterns (each diagonal once or three times).
Delete removes the selected point (Select Point tool, or the last point clicked), or the
template while Modify Template is on.
Select Object selects the whole pattern (the window edits one); dragging it or the arrow keys
move it by grid steps (Shift: straight), refused where it would leave the width band. The Pen
Tool adds needle points, satin stations or cross cells along a dragged stroke, or replaces a
decorative satin outline with it. Swap exchanges the start and end points (`swapped` in the
document: the same needle points sewn in reverse order). Duplicate puts a copy of the pattern
where it ends (not decorative satin, one outline). Selected object zoom fits the pattern to the
design area; Pan drags it. Templates are images of the user's own (PNG, JPEG, BMP, GIF, TIFF):
Modify Template draws a red frame (drag inside: move, its corner: scale, click outside or
Escape: finish), Template Density fades it (0 % hides it, F6 steps), Delete Image removes it.
Horizontal Orientation lays the page landscape (the feed running to the right, OpenStitch's
default); off, the feed runs upwards. Grid Spacing: 0.25, 0.5, 1, 2 or 5 mm, also what points
snap to.

#### Home

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Select | Select Object |  | `FactoryAction::SelectObject` | view/UI only | yes |
| Select | Select Point |  | `FactoryAction::SelectPoint` | view/UI only | yes |
| Draw | Line Tool |  | `FactoryAction::LineTool` | view/UI only | yes |
| Draw | Pen Tool |  | `FactoryAction::PenTool` | view/UI only | yes |
| Zoom | Zoom in | `Ctrl+=` | `FactoryAction::ZoomIn` | view/UI only | yes |
| Zoom | Zoom Out | `Ctrl+-` | `FactoryAction::ZoomOut` | view/UI only | yes |
| Zoom | Selected object zoom |  | `FactoryAction::SelectedZoom` | view/UI only | yes |
| Zoom | Pan |  | `FactoryAction::Pan` | view/UI only | yes |
| Stitch | Single Stitch |  | `FactoryAction::SingleStitch` | view/UI only | yes |
| Stitch | Triple Stitch |  | `FactoryAction::TripleStitch` | view/UI only | yes |
| Stitch | Swap |  | `FactoryAction::Swap` | `sewing.swap` | yes |
| Edit | Flip Vertical |  | `FactoryAction::FlipVertical` | view/UI only | yes |
| Edit | Flip Horizontal |  | `FactoryAction::FlipHorizontal` | view/UI only | yes |
| Edit | Duplicate | `Ctrl+D` | `FactoryAction::Duplicate` | `sewing.duplicate` | yes |
| Edit | Delete | `Delete` | `FactoryAction::Delete` | view/UI only | yes |

#### View

| Group | Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|---|
| Template | Open Template |  | `FactoryAction::OpenTemplate` | view/UI only | yes |
| Template | Modify Template |  | `FactoryAction::ModifyTemplate` | view/UI only | yes |
| Template | Delete Image |  | `FactoryAction::DeleteImage` | view/UI only | yes |
| Template | Template Density | `F6` | `FactoryAction::TemplateDensity` | view/UI only | yes |
| Display | Needle Drop Point |  | `FactoryAction::NeedleDropPoint` | view/UI only | yes |
| Display | Horizontal Orientation |  | `FactoryAction::HorizontalOrientation` | view/UI only | yes |
| Show/Hide | Import Pane |  | `FactoryAction::ImportPane` | `sewing.list` | yes |
| Show/Hide | Preview Window |  | `FactoryAction::PreviewWindow` | view/UI only | yes |
| Show/Hide | Status Bar |  | `FactoryAction::StatusBar` | view/UI only | yes |
| Grid | Show Grid |  | `FactoryAction::ShowGrid` | view/UI only | yes |
| Grid | Snap to Grid |  | `FactoryAction::SnapToGrid` | view/UI only | yes |
| Grid | Grid Spacing |  | `FactoryAction::GridSpacing` | view/UI only | yes |
| Simulator | Stitch Simulator |  | `FactoryAction::StitchSimulator` | view/UI only | yes |

#### Application menu

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| New |  | `FactoryAction::New` | view/UI only | yes |
| Open | `Ctrl+O` | `FactoryAction::Open` | `sewing.import` | yes |
| Save | `Ctrl+S` | `FactoryAction::Save` | `sewing.define` | yes |
| Save As | `Ctrl+Shift+S` | `FactoryAction::SaveAs` | view/UI only | yes |
| Export |  | `FactoryAction::Export` | `sewing.export` | yes |
| Exit |  | `FactoryAction::Exit` | view/UI only | yes |

#### Quick Access Toolbar and buttons

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Open | `Ctrl+O` | `FactoryAction::Open` | `sewing.import` | yes |
| Save | `Ctrl+S` | `FactoryAction::Save` | `sewing.define` | yes |
| Export |  | `FactoryAction::Export` | `sewing.export` | yes |
| Help |  | `FactoryAction::Help` | view/UI only | yes |
| Options |  | `FactoryAction::Options` | view/UI only | yes |

### Design Database

Guide: [Design Database Window](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/11_1142905.html), [List of menus](https://support.brother.com/g/s/hf/htmldoc/ped/im/ped11/en/PED11_EN/pages/11_1142946.html).

The guide gives the Design Database a classic menu bar and toolbar rather than a ribbon, so
OpenStitch does the same: menus File, Edit, Display, Option, Help in the guide's order, a toolbar
with the guide's icon buttons (Cut, Copy, Paste, Large Thumbnails, Small Thumbnails, Details,
Preview, Property) plus labelled ones, and a status bar.

![Design Database](images/database-browse.png)

| Guide element | OpenStitch |
|---|---|
| Toolbar | Exit, Parent Folder, Choose Folder · Cut, Copy, Paste · Large/Small Thumbnails, Details · Preview, Property · Search, Convert Format, Open in / Import Into Layout & Editing · Card Reading (Blocked), Writing List |
| Folder pane | Left: folder tree; drop files on a folder to move them there |
| Contents pane | Thumbnails or details of the folder; double-click opens in Layout & Editing |
| Preview and Property | Right; each shown or hidden from Display |
| Writing list, Send to selector | Transfer area under the contents (toolbar ▸ Writing List): Send To lists the USB media found; Add, Remove, Write; right-click Add to / Remove from Writing List, or drag designs into the list. Machine memory, wireless, original card and Link are listed disabled (Blocked: undocumented protocols) |
| Card reading button | Blocked: Brother has not published the USB card reader/writer protocol or the card data layout, and OpenStitch will not reverse-engineer them; the button stays disabled and its ScreenTip says why |
| Status bar | Folder summary or the ScreenTip under the pointer |

OpenStitch differences: File ▸ Print Setup opens the catalogue dialog (Print hands the PDF to
the system print dialog, Save keeps it), File ▸ Print prints with those options through the
system print dialog (on Linux, the PDF viewer), and Print Preview writes page images; File ▸ Rename (F2)
renames in every view; Option ▸ Application Color offers OpenStitch's Light, Dark and System
themes instead of Blue, Black, Silver and Aqua; Help ▸ Customer Support opens the project's issue
tracker.

#### File menu

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Open in Layout & Editing | `Enter` | `DbAction::OpenInEditor` | `project.open` | yes |
| Import Into Layout & Editing |  | `DbAction::ImportIntoEditor` | `stitches.import` | yes |
| Create New Folder |  | `DbAction::NewFolder` | view/UI only | yes |
| Convert Format |  | `DbAction::ConvertFormat` | `library.convert` | yes |
| Delete | `Delete` | `DbAction::Delete` | view/UI only | yes |
| Rename | `F2` | `DbAction::Rename` | view/UI only | yes |
| Search |  | `DbAction::Search` | `library.search` | yes |
| Print Setup |  | `DbAction::PrintSetup` | view/UI only | yes |
| Print Preview |  | `DbAction::PrintPreview` | view/UI only | yes |
| Print |  | `DbAction::Print` | `library.catalog` | yes |
| Create Image File and HTML |  | `DbAction::ImageHtml` | `library.catalog` | yes |
| Create CSV |  | `DbAction::Csv` | `library.catalog` | yes |
| Exit |  | `DbAction::Exit` | view/UI only | yes |

#### Edit menu

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Cut | `Ctrl+X` | `DbAction::Cut` | view/UI only | yes |
| Copy | `Ctrl+C` | `DbAction::Copy` | view/UI only | yes |
| Paste | `Ctrl+V` | `DbAction::Paste` | view/UI only | yes |
| Select All | `Ctrl+A` | `DbAction::SelectAll` | view/UI only | yes |

#### Display menu

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Large Thumbnails |  | `DbAction::LargeThumbnails` | view/UI only | yes |
| Small Thumbnails |  | `DbAction::SmallThumbnails` | view/UI only | yes |
| Details |  | `DbAction::Details` | view/UI only | yes |
| Preview |  | `DbAction::Preview` | view/UI only | yes |
| Property |  | `DbAction::Property` | view/UI only | yes |
| Refresh |  | `DbAction::Refresh` | view/UI only | yes |
| Toolbar |  | `DbAction::Toolbar` | view/UI only | yes |
| Status Bar |  | `DbAction::StatusBar` | view/UI only | yes |

#### Option menu

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Select System Unit |  | `DbAction::SystemUnit` (one setting with the editor's Options ▸ Units, #519) | view/UI only | yes |
| Application Color |  | `DbAction::ApplicationColor` | view/UI only | yes |
| USB Link |  | `DbAction::UsbLink` | view/UI only | **not yet** (disabled, ScreenTip says so) |
| Wireless Link |  | `DbAction::WirelessLink` | view/UI only | **not yet** (disabled, ScreenTip says so) |
| DST Settings |  | `DbAction::DstSettings` | view/UI only | yes |
| Network Machine Settings |  | `DbAction::NetworkMachines` | view/UI only | **not yet** (disabled, ScreenTip says so) |

#### Help menu

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Online Instruction Manual |  | `DbAction::Manual` | view/UI only | yes |
| About Design Database |  | `DbAction::About` | view/UI only | yes |
| Customer Support |  | `DbAction::Support` | view/UI only | yes |

#### Toolbar

| Command | Shortcut | OpenStitch action | Engine command | Available |
|---|---|---|---|---|
| Exit |  | `DbAction::Exit` | view/UI only | yes |
| Parent Folder | `Alt+Up` | `DbAction::ParentFolder` | view/UI only | yes |
| Choose Folder |  | `DbAction::ChooseFolder` | view/UI only | yes |
| Cut | `Ctrl+X` | `DbAction::Cut` | view/UI only | yes |
| Copy | `Ctrl+C` | `DbAction::Copy` | view/UI only | yes |
| Paste | `Ctrl+V` | `DbAction::Paste` | view/UI only | yes |
| Large Thumbnails |  | `DbAction::LargeThumbnails` | view/UI only | yes |
| Small Thumbnails |  | `DbAction::SmallThumbnails` | view/UI only | yes |
| Details |  | `DbAction::Details` | view/UI only | yes |
| Preview |  | `DbAction::Preview` | view/UI only | yes |
| Property |  | `DbAction::Property` | view/UI only | yes |
| Search |  | `DbAction::Search` | `library.search` | yes |
| Convert Format |  | `DbAction::ConvertFormat` | `library.convert` | yes |
| Open in Layout & Editing | `Enter` | `DbAction::OpenInEditor` | `project.open` | yes |
| Import Into Layout & Editing |  | `DbAction::ImportIntoEditor` | `stitches.import` | yes |
| Card Reading |  | `DbAction::ReadCard` | view/UI only | **Blocked** (disabled, ScreenTip says why: undocumented card reader protocol) |
| Writing List |  | `DbAction::WritingBox` | view/UI only | yes |
