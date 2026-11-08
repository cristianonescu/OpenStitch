//! The editor's command layout: every command the window offers as an [`Action`], the ribbon
//! tabs and groups that show them, the contextual tabs that appear for the active tool or the
//! selection, the Application menu and the Quick Access Toolbar. The arrangement and command
//! names follow the public online guide of the embroidery suite OpenStitch is compatible with
//! (docs/UI_LAYOUT.md maps each one), so its users find commands where they expect them; icons
//! and visual style are OpenStitch's own.
//!
//! The egui shell (`ribbon.rs` in the binary) draws this model and runs the actions; nothing
//! here depends on egui, so the layout rules are unit-tested.

use super::tools::Tool;

/// What one command shows: label, ScreenTip, shortcut, the engine command it runs (when it maps
/// to one), and whether OpenStitch implements it yet (`false` = shown disabled with a
/// "not yet available" ScreenTip).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Info {
    /// Stable name used in the user's settings (Quick Access Toolbar).
    pub key: &'static str,
    pub label: &'static str,
    pub tip: &'static str,
    /// As written on Windows and Linux ("Ctrl+S"); see [`Info::shortcut_for`].
    pub shortcut: Option<&'static str>,
    pub engine: Option<&'static str>,
    pub available: bool,
}

impl Info {
    /// The shortcut as shown on this platform (Cmd on macOS).
    pub fn shortcut_for(&self, mac: bool) -> Option<String> {
        self.shortcut.map(|s| {
            if mac {
                s.replace("Ctrl+", "Cmd+")
            } else {
                s.to_string()
            }
        })
    }

    /// ScreenTip text: the label, the shortcut and what the command does.
    pub fn screen_tip(&self, mac: bool) -> String {
        let head = match self.shortcut_for(mac) {
            Some(s) => format!("{} ({s})", self.label),
            None => self.label.to_string(),
        };
        if self.available {
            format!("{head}\n{}", self.tip)
        } else {
            let not_yet =
                super::i18n::text("ribbon-not-available", "Not yet available in OpenStitch.");
            format!("{head}\n{not_yet} {}", self.tip)
        }
    }
}

macro_rules! actions {
    ($($variant:ident => $key:literal, $label:literal, $tip:literal, $shortcut:expr, $engine:expr, $available:expr;)*) => {
        /// Every command of the Layout & Editing window.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum Action { $($variant),* }

        impl Action {
            pub const ALL: &'static [Action] = &[$(Action::$variant),*];

            pub fn info(self) -> Info {
                match self {
                    $(Action::$variant => Info {
                        key: $key,
                        label: super::i18n::text(concat!("action-", $key, "-label"), $label),
                        tip: super::i18n::text(concat!("action-", $key, "-tip"), $tip),
                        shortcut: $shortcut,
                        engine: $engine,
                        available: $available,
                    }),*
                }
            }
        }
    };
}

actions! {
    // Application button menu.
    New => "new", "New", "Start a new design.", Some("Ctrl+N"), Some("design.new"), true;
    Open => "open", "Open", "Open an OpenStitch design, or an embroidery file as a new design.", Some("Ctrl+O"), Some("project.open"), true;
    OpenSample => "openSample", "Open Sample", "Open a ready design from the kit (a satin and fill sampler, shapes, lettering, …) as a new untitled design.", None, None, true;
    Save => "save", "Save", "Save the design.", Some("Ctrl+S"), Some("project.save"), true;
    SaveAs => "saveAs", "Save As", "Save the design under a new name.", Some("Ctrl+Shift+S"), Some("project.save"), true;
    Print => "print", "Print", "Print the design with its colour list through the system print dialog, or save it as a PDF.", None, Some("design.print"), true;
    JobSheet => "jobSheet", "Job Sheet", "Print a client approval sheet or an operator worksheet with the job's customer, order, due date and notes (design.setJob, design.print; beyond PE-DESIGN).", None, Some("design.print"), true;
    DesignProperty => "designProperty", "Design Property", "Sewing information of the design: size, stitches, colours, thread and time.", None, Some("design.info"), true;
    DesignSettings => "designSettings", "Design Settings", "Machine, hoop and Design Page size.", None, Some("machine.set"), true;
    FabricSelector => "fabricSelector", "Fabric Selector", "Sewing attributes suited to a fabric type, for all shape and text patterns or the selection.", None, Some("fabric.apply"), true;
    SelectColorPalette => "colorPalette", "Select Color Palette", "Choose or edit the thread colour charts.", None, Some("threads.charts"), true;
    Wizard => "wizard", "Wizard", "Create a design step by step: templates or an image.", None, None, true;
    Export => "export", "Export", "Write the design as a machine embroidery file.", None, Some("design.export"), true;
    Options => "options", "Options", "Other applications, settings and the colour theme.", None, None, true;
    Exit => "exit", "Exit", "Close OpenStitch.", None, None, true;
    // Quick Access Toolbar extras.
    Undo => "undo", "Undo", "Undo the last change.", Some("Ctrl+Z"), Some("history.undo"), true;
    Redo => "redo", "Redo", "Redo the change undone last.", Some("Ctrl+Y"), Some("history.redo"), true;
    Help => "help", "Help", "Instruction manual, keyboard shortcuts and information about OpenStitch.", None, None, true;
    // Option button.
    DesignDatabase => "designDatabase", "Design Database", "Browse, search and convert embroidery files in folders.", None, Some("library.list"), true;
    StitchCreator => "stitchCreator", "Programmable Stitch Creator", "Edit fill, stamp and motif stitch patterns.", None, Some("pattern.define"), true;
    StitchDesignFactory => "stitchFactory", "Stitch Design Factory", "Edit decorative stitches sewn along lines.", None, Some("sewing.define"), true;
    FontCreator => "fontCreator", "Font Creator", "Create embroidery fonts and map characters.", None, Some("fonts.create"), true;
    AttributeSets => "attributeSets", "Sewing Attribute Sets", "Save and apply named sets of sewing attributes.", None, Some("attributes.save"), true;
    UserThreadCharts => "threadCharts", "User Thread Charts", "Make your own thread colour charts.", None, Some("threads.createChart"), true;
    // Home ▸ Select.
    SelectTool => "select", "Select", "Select objects; drag to move, handles scale and rotate.", Some("V"), None, true;
    PointTool => "point", "Point Edit", "Move, add and delete points of the selected outline.", Some("A"), Some("object.setPoint"), true;
    SelectAll => "selectAll", "Select All", "Select every visible object.", Some("Ctrl+A"), None, true;
    SelectSame => "selectSame", "Select Same", "Select every visible object sewn like the selection: same thread colour, same stitch type, or both.", None, Some("object.list"), true;
    SelectSameColor => "selectSameColor", "Same Thread Colour", "Select every visible object (or stitch pattern) that sews a thread colour of the selection. Alt-click on a stitch pattern does the same for the colour under the pointer.", None, Some("object.list"), true;
    SelectSameType => "selectSameType", "Same Stitch Type", "Select every visible object with the selection's region sew type (or line sew type for outlines).", None, Some("object.list"), true;
    SelectSameBoth => "selectSameBoth", "Same Colour and Type", "Select every visible object that shares both a thread colour and the stitch type with the selection.", None, Some("object.list"), true;
    // Home ▸ Clipboard.
    Cut => "cut", "Cut", "Move the selection to the clipboard; Paste puts it back.", Some("Ctrl+X"), Some("object.paste"), true;
    Copy => "copy", "Copy", "Copy the selection to the clipboard; Paste adds a copy of it, here or in another window.", Some("Ctrl+C"), Some("object.get"), true;
    Paste => "paste", "Paste", "Add the objects on the clipboard where they were copied from.", Some("Ctrl+V"), Some("object.paste"), true;
    Duplicate => "duplicate", "Duplicate", "Add a copy of the selection, slightly offset.", Some("Ctrl+D"), Some("object.duplicate"), true;
    Delete => "delete", "Delete", "Delete the selection.", Some("Delete"), Some("object.delete"), true;
    ArrangeCopy => "arrangeCopy", "Arrange Copy", "Copies in a row or a grid, around a circle, mirrored, along a line or a path, or scattered inside a shape (object.arrangeCopies).", None, Some("object.arrangeCopies"), true;
    CopyAttributes => "copyAttributes", "Copy Attributes", "Remember the sewing attributes of the selected object.", None, None, true;
    PasteAttributes => "pasteAttributes", "Paste Attributes", "Give the selection the copied sewing attributes.", None, Some("object.pasteAttributes"), true;
    // Home ▸ Tools.
    TextTools => "text", "Text", "Text, monograms, name drop lists, templates and fonts.", None, Some("text.add"), true;
    ShapesTools => "shapes", "Shapes", "Draw rectangles, ellipses, outlines and curves, or insert preset shapes.", None, Some("shape.preset"), true;
    Rectangle => "rectangle", "Rectangle", "Drag to draw a rectangle.", Some("R"), Some("shape.rectangle"), true;
    Ellipse => "ellipse", "Circle / Ellipse", "Drag to draw an ellipse.", Some("E"), Some("shape.ellipse"), true;
    ClosedStraight => "polygon", "Closed Straight Line", "Click the corners of a closed outline.", Some("P"), Some("shape.polygon"), true;
    ClosedCurve => "pen", "Closed Curve", "Click for corners, drag for curves; click the first point to close.", Some("B"), Some("shape.path"), true;
    PencilClosed => "pencilClosed", "Pencil (Closed Curve)", "Drag to draw a closed outline freehand.", None, Some("shape.freehand"), true;
    OpenStraight => "openStraight", "Open Straight Line", "Click the points of an open line; double-click or Return ends it.", Some("L"), Some("shape.polygon"), true;
    OpenCurve => "openCurve", "Open Curve", "Pen tool: Return finishes the curve open.", None, Some("shape.path"), true;
    PencilOpen => "pencilOpen", "Pencil (Open Curve)", "Drag to draw an open line freehand.", None, Some("shape.freehand"), true;
    ZoomIn => "zoomIn", "Zoom In", "Magnify the view.", Some("Ctrl+="), None, true;
    ZoomOut => "zoomOut", "Zoom Out", "Reduce the view.", Some("Ctrl+-"), None, true;
    OtherZoom => "zoom", "Zoom", "Fit the Design Page, the selection, or 100 %.", Some("Ctrl+0"), None, true;
    Pan => "pan", "Pan", "Drag the view (also Space+drag or the middle button).", Some("H"), None, true;
    Measure => "measure", "Measure", "Measure a distance on the Design Page.", Some("M"), Some("design.measure"), true;
    // Home ▸ Import.
    ImportPatterns => "import", "Import Patterns", "Add an embroidery file to the design.", Some("Ctrl+Shift+I"), Some("stitches.import"), true;
    ImportSvg => "importSvg", "Import SVG", "Add vector artwork as editable shapes.", None, Some("import.svg"), true;
    ImportPesObjects => "importPesObjects", "Import PE-DESIGN Objects", "Add a PE-DESIGN 11 file's shapes, texts and stitch groups as editable objects.", None, Some("import.pes"), true;
    // Home ▸ Edit.
    Align => "align", "Align", "Line the selection up on an edge or centre, relative to the selection, the last-selected object or the Design Page, or space it evenly.", None, Some("object.align"), true;
    AlignLeft => "alignLeft", "Align Left", "Line up the left edges.", None, Some("object.align"), true;
    AlignCenter => "alignCenter", "Align Centre", "Line up the horizontal centres (one vertical line through every centre).", None, Some("object.align"), true;
    AlignRight => "alignRight", "Align Right", "Line up the right edges.", None, Some("object.align"), true;
    AlignTop => "alignTop", "Align Top", "Line up the top edges.", None, Some("object.align"), true;
    AlignMiddle => "alignMiddle", "Align Middle", "Line up the vertical centres (one horizontal line through every centre).", None, Some("object.align"), true;
    AlignBottom => "alignBottom", "Align Bottom", "Line up the bottom edges.", None, Some("object.align"), true;
    DistributeHorizontal => "distributeHorizontal", "Distribute Horizontally", "Equal gaps between three or more objects from left to right; the outermost two stay put.", None, Some("object.distribute"), true;
    DistributeVertical => "distributeVertical", "Distribute Vertically", "Equal gaps between three or more objects from top to bottom; the outermost two stay put.", None, Some("object.distribute"), true;
    Arrange => "arrange", "Arrange", "Centre the selection or change where it sews in the order.", None, Some("order.move"), true;
    RotateFlip => "rotateFlip", "Rotate/Flip", "Rotate by 90° or mirror the selection.", None, Some("object.rotate"), true;
    Size => "size", "Size", "Scale the selection by a percentage.", None, Some("object.scale"), true;
    Group => "group", "Group", "Group the selection so it moves together.", Some("Ctrl+G"), Some("object.group"), true;
    Ungroup => "ungroup", "Ungroup", "Take the selected groups apart.", Some("Ctrl+Shift+G"), Some("object.ungroup"), true;
    StampEmboss => "stampEmboss", "Stamp/Emboss", "Emboss or engrave a shape, or place stamps in it.", None, Some("shape.emboss"), true;
    ModifyOverlap => "modifyOverlap", "Modify Overlap", "Merge, remove overlap or combine outlines.", None, Some("shape.combine"), true;
    DecorativeFrame => "decorativeFrame", "Decorative Frame", "Add a frame around the selection.", None, Some("shape.decorative"), true;
    FitTextToOutline => "fitText", "Fit Text", "Fit Text to Outline: select a text and a shape, and the text follows the shape's outline. Release Text from Outline takes it off again.", None, Some("text.setPath"), true;
    // Home ▸ Wizards.
    BackgroundFill => "backgroundFill", "Background fill", "Fill around the selection with a background stitch.", None, Some("wizard.backgroundFill"), true;
    Applique => "applique", "Applique Wizard", "Placement, tack-down and cover stitching for an appliqué.", None, Some("wizard.applique"), true;
    Patch => "patch", "Embroidered Patch Wizard", "A patch with a satin border around the selection.", None, Some("wizard.patch"), true;
    Cutwork => "cutwork", "Cutwork Wizard", "Cut lines and the stitching around them.", None, Some("wizard.cutwork"), true;
    // Home ▸ Sew.
    SendEmbroidery => "send", "Sew Embroidery", "Write the design to USB media, or each hooping as its own file.", None, Some("transfer.send"), true;
    TestSwatches => "testSwatches", "Test Swatches", "Write a test to sew first: the selection alone, one object with a setting stepped, or a fabric calibration card (beyond PE-DESIGN).", None, Some("design.testSwatches"), true;
    // The Check Design window: the status bar's Ready to sew indicator and the Sewing Order
    // pane's Review button (beyond PE-DESIGN, #503-#505, #599).
    CheckDesign => "checkDesign", "Check Design", "List the design's sewing problems, each with a way to show it and, where there is one, a fix (the status bar's Ready to sew indicator).", None, Some("design.check"), true;
    DensityMap => "densityMap", "Density Map", "Show where thread layers pile up and step through the hotspots (Check Design, Density map).", None, Some("design.density"), true;
    ReviewSewingOrder => "reviewSewingOrder", "Review Sewing Order", "Step through every move between objects, with trims, jumps and warnings (Check Design, Sewing order).", None, Some("design.route"), true;
    // Image tab.
    OpenImage => "openImage", "Open Image", "Show an image behind the design (PNG, JPEG, BMP).", None, Some("background.set"), true;
    PasteImage => "pasteImage", "Paste Image", "Use the image on the clipboard as the background.", None, Some("background.set"), true;
    SaveImage => "saveImage", "Save Image", "Save the background image as shown to a file, or copy it to the clipboard.", None, Some("background.save"), true;
    ModifyImage => "modifyImage", "Modify Image", "Move, scale, crop or adjust the colours of the background image.", None, Some("background.modify"), true;
    ImageDensity => "imageDensity", "Background Image Density", "Show, hide or fade the background image.", None, Some("background.show"), true;
    ImageRotateRight => "imageRotateRight", "Rotate Right", "Turn the background image 90° clockwise.", None, Some("background.rotate"), true;
    ImageRotateLeft => "imageRotateLeft", "Rotate Left", "Turn the background image 90° anticlockwise.", None, Some("background.rotate"), true;
    DeleteImage => "deleteImage", "Delete Image", "Remove the background image.", None, Some("background.clear"), true;
    AutoPunch => "autoPunch", "Auto Punch Wizard", "Regions and outlines traced from an image.", None, Some("digitize.autoPunch"), true;
    CrossStitch => "crossStitch", "Cross Stitch Wizard", "Cross stitches in the image's colours.", None, Some("digitize.crossStitch"), true;
    PhotoStitch1 => "photoStitch1", "Photo Stitch 1 Wizard", "A photo as fill stitching.", None, Some("digitize.photoStitch"), true;
    PhotoStitch2 => "photoStitch2", "Photo Stitch 2 Wizard", "A photo as zigzag lines.", None, Some("digitize.lineImage"), true;
    DesignCenter => "designCenter", "Import to Design Center", "Trace a line drawing into outlines and regions.", None, Some("digitize.designCenter"), true;
    // View tab.
    SolidView => "solidView", "Solid View", "Stitches drawn as solid colour areas.", None, None, true;
    StitchView => "stitchView", "Stitch View", "Stitches drawn as lines.", None, None, true;
    RealisticView => "realisticView", "Realistic View", "Stitches drawn as thread with shading.", None, None, true;
    ViewThreadTrimming => "viewTrimming", "View Thread Trimming", "Show the jumps between stitched areas.", None, None, true;
    ViewShow => "viewShow", "Show", "One aspect of the stitching at a time: needle points, underlay or top only, trim marks, only the selection, colour by sewing order or one colour, solo colour.", None, None, true;
    RealisticSettings => "realisticSettings", "Realistic View Settings", "Thread width, contrast and brightness of the realistic view.", None, None, true;
    PaneSewingOrder => "paneSewingOrder", "Sewing Order", "Show or hide the Sewing Order pane.", None, None, true;
    PaneSimulator => "paneSimulator", "Stitch Simulator", "Show or hide the Stitch Simulator pane.", None, None, true;
    PaneAttributes => "paneAttributes", "Attributes Setting", "Show or hide the Color / Sewing Attributes / Text Attributes pane.", None, None, true;
    PaneImport => "paneImport", "Import Pane", "Show the Import pane.", None, None, true;
    PaneAssistant => "paneAssistant", "Assistant", "Show or hide the Assistant: ask Claude to make, change or explain the design, or to do anything in the editor.", Some("Ctrl+Shift+A"), None, true;
    ReferenceWindow => "reference", "Reference Window", "An overview of the Design Page; click to move the view.", None, None, true;
    ShowGrid => "showGrid", "Show Grid", "Show the grid on the Design Page.", None, None, true;
    GridAxes => "gridAxes", "Show Grid with Axes", "Grid drawn as solid lines; off, it is dotted.", None, None, true;
    SnapToGrid => "snapToGrid", "Snap to Grid", "Objects move and draw in grid steps.", None, None, true;
    SnapToObjects => "snapToObjects", "Snap to Objects", "A dragged selection snaps to the edges and centres of other objects, the Design Page and the guidelines, with a dashed guide line. Hold Ctrl (Cmd on macOS) to drag freely.", None, None, true;
    PrecisionBar => "precisionBar", "Precision Bar", "Show or hide the bar for typing the selection's exact position, size, rotation and skew.", None, Some("object.scale"), true;
    GridSpacing => "gridSpacing", "Grid Spacing", "Distance between grid lines.", None, None, true;
    Ruler => "ruler", "Ruler", "Show rulers along the work area.", None, None, true;
    Guideline => "guideline", "Guideline", "Click a ruler to add a guideline; drag its marker to move it, click × to delete. Saved with the design.", None, Some("guides.set"), true;
    GuideOptions => "guideOptions", "Guides", "New Guide…, New Guide Layout…, Lock Guides, Set Position…, Clear Selected Guide, Clear Guides and New Guides From Selection; right-click a guideline or a ruler for its menu. Click a guideline to select it (Delete removes it), double-click it for Set Position…, Alt-click it to turn it.", None, Some("view.lockGuides"), true;
    // Contextual tabs.
    Font => "font", "Font", "Font of the selected text.", None, Some("object.setText"), true;
    TextSize => "textSize", "Text Size", "Size of the selected text.", None, Some("object.setText"), true;
    FontFilter => "fontFilter", "Font Filter", "Show only the fonts chosen in Font Filter Settings.", None, None, true;
    FontFilterSettings => "fontFilterSettings", "Font Filter Settings", "Choose the fonts the filter shows.", None, None, true;
    TrueTypeAttributes => "trueType", "TrueType Font Attribute Setting", "Load a TrueType or OpenType font into the design.", None, Some("fonts.loadTrueType"), true;
    EditText => "editText", "Edit Text", "Change the characters of the selected text.", None, Some("object.setText"), true;
    EditCharacter => "editCharacter", "Edit Character", "Offset, rotation and size of the character picked with Point Edit (drag it, its rotate handle or its corners on the canvas).", None, Some("text.setCharacter"), true;
    LineColor => "lineColor", "Line color", "Thread colour of the outline.", None, Some("object.setLine"), true;
    LineSewType => "lineSewType", "Line sew type", "How the outline is sewn (running, triple, zigzag, motif…).", None, Some("object.setLine"), true;
    RegionColor => "regionColor", "Region color", "Thread colour of the region.", None, Some("object.setRegion"), true;
    RegionSewType => "regionSewType", "Region sew type", "How the region is sewn (fill, satin, programmable…).", None, Some("object.setRegion"), true;
    ConvertToBlocks => "toBlocks", "Convert to Blocks", "Turn the stitch pattern into editable regions.", None, Some("object.toShapes"), true;
    ConvertToStitches => "toStitches", "Convert to Stitches", "Turn the selection into a stitch pattern.", None, Some("object.toStitches"), true;
    ConvertToOutline => "toOutline", "Convert to Outline", "Text: one shape per character. Stitches: their outline.", None, Some("object.toShapes"), true;
    EdgeRadius => "edgeRadius", "Edge radius", "Round the corners of the rectangles you draw.", None, Some("shape.rectangle"), true;
    StraightBlock => "punch", "Straight Block", "Click both ends of each sewing-direction line.", Some("U"), Some("shape.column"), true;
    CurvedBlock => "curvedBlock", "Curved Block", "Click both ends of each sewing-direction line; the sides curve through them.", None, Some("shape.column"), true;
    Running => "running", "Running", "Running stitch between clicked points.", None, Some("shape.polygon"), true;
    Feed => "feed", "Feed", "Jump between clicked points.", None, Some("shape.feed"), true;
    SatinSkeleton => "satinSkeleton", "Satin Skeleton", "Click (or drag for curves) the centre line of a satin column; drag the handles beside each point to set its width. Double-click or Return ends it.", None, Some("shape.skeletonSatin"), true;
    SkeletonWidth => "skeletonWidth", "Column width", "Width of the next points the Satin Skeleton tool places.", None, Some("shape.skeletonSatin"), true;
    DeriveSkeleton => "deriveSkeleton", "Satin Skeleton from Shape", "Sew the selected shape as satin along a centre line derived from its outline (an approximation of its medial axis).", None, Some("object.deriveSkeleton"), true;
    SmoothShape => "smoothShape", "Smooth Shape", "Remove the nodes of the selected outlines that deviate less than the smoothing tolerance (curves refitted, corners and open ends kept). A Manual Punch column loses one direction line per click; click again to smooth further.", None, Some("shape.simplify"), true;
    SmoothTolerance => "smoothTolerance", "Smooth tolerance", "How far Smooth Shape may move an outline (default 0.2 mm).", None, Some("shape.simplify"), true;
    Knife => "knife", "Knife", "Click the points of an open cut line; double-click or Return cuts every object it crosses (fills and outlines into both sides, satin columns across, lines where crossed). Shift: 45° steps. A single click then Return splits the line under the pointer. Stays active; Esc leaves.", None, Some("shape.knife"), true;
    ShapeEffects => "shapeEffects", "Shape Effects", "Warp, twist, roughen, pucker & bloat or zigzag the selected outlines, with a preview; the result is ordinary editable geometry (shape.effect).", None, Some("shape.effect"), true;
    GradientTool => "gradientTool", "Gradient", "Drag across the selected gradient fill (made with shape.gradient or shape.blend) from where its first colour should be to where its last should be; every colour of the gradient follows. Click twice instead of dragging, or use the arrow keys and Space. Shift: 15° steps. Stays active; Esc leaves.", None, Some("object.setRegion"), true;
    FillAngleTool => "fillAngleTool", "Fill Angle", "Drag across the selected fills, satins and other regions in the direction their rows should run (PE-DESIGN's straight sewing direction, set on the canvas); they turn when you release. Click twice instead of dragging, or use the arrow keys and Space. Shift: 15° steps. Stays active; Esc leaves.", None, Some("object.setRegion"), true;
    ShapeBuilder => "shapeBuilder", "Shape Builder", "Select two or more overlapping shapes, then drag across the pieces of their overlaps: the pieces you touch become one shape (sewn like the topmost of them). Hold Alt to remove the pieces instead.", None, Some("shape.build"), true;
    LineGap => "lineGap", "Turning Gap", "Leave part of the selected closed line unsewn, the opening for turning in-the-hoop pieces: where it starts along the outline and how long it is. Running, triple, zigzag, E and V lines.", None, Some("object.setLineGap"), true;
    SplitOutline => "splitOutline", "Split Outline", "Draw a cut line that splits the selected shape.", None, Some("shape.split"), true;
    AddDirectionLine => "addDirection", "Add Direction Line", "Draw a curved sewing direction across the region.", None, Some("object.setDirectionCurve"), true;
    DeleteDirectionLine => "deleteDirection", "Delete Direction Line", "Back to a straight sewing direction.", None, Some("object.setDirectionLines"), true;
    ReverseDirectionLine => "reverseDirection", "Reverse Direction Line", "Sew the direction line the other way.", None, Some("object.reverseDirection"), true;
    EditPoint => "editPoint", "Edit Point", "Move the points of the outline.", Some("A"), Some("object.setPoint"), true;
    DeletePoint => "deletePoint", "Delete Point", "Delete the selected point.", Some("Delete"), Some("object.deletePoint"), true;
    SplitAtPoint => "splitAtPoint", "Split at Point", "Split the stitch pattern at the selected needle point.", None, Some("stitches.splitAt"), true;
    ConvertRegionToLine => "regionToLine", "Convert Region to Line", "Sew the outline with a running stitch in the region's colour.", None, Some("object.regionToLine"), true;
    OffsetLines => "offsetLines", "Create Offset Lines", "Lines inside or outside the outline.", None, Some("shape.offsetLines"), true;
    FloralPattern => "floral", "Create Floral Pattern", "A floral pattern inside the outline.", None, Some("shape.floral"), true;
    EntryExit => "entryExit", "Entry/Exit Points", "Where the region and line start and end.", None, Some("object.setEntryExit"), true;
    SewingPoints => "sewingPoints", "Center/Reference Point", "Centre point of radial fills and the reference point of stitch patterns.", None, Some("object.setCenterPoint"), true;
    SplitStitchData => "splitStitchData", "Split Stitch Data", "Draw around stitches to split them off.", None, Some("stitches.splitArea"), true;
    ChangeColor => "changeColor", "Change Color", "Change a thread colour of the stitch pattern.", None, Some("stitches.setThread"), true;
    DivideByColor => "divideByColor", "Divide by Color", "One stitch pattern per thread colour.", None, Some("stitches.divideByColor"), true;
    SmoothStitches => "smoothStitches", "Smooth Stitches", "Even out the selected needle points (or the whole stitch pattern): each moves halfway towards the midpoint of its neighbours. Jumps, trims, colour changes and the ends of each run stay; the number of points does not change.", None, Some("stitches.smooth"), true;
    Connect => "connect", "Connect", "The pattern sewn later starts where the earlier one ends.", None, Some("stitches.connect"), true;
    ConvertToOutlines => "toOutlines", "Convert to Outlines", "One outline per thread colour of the stitches.", None, Some("object.toShapes"), true;
    MoveSections => "moveSections", "Move Sections", "Drag the split lines of a split design to move its sections.", None, Some("sections.set"), true;
    ExportSections => "exportSections", "Export Sections", "One file per hooping of a design larger than the hoop.", None, Some("sections.export"), true;
}

impl Action {
    pub fn from_key(key: &str) -> Option<Action> {
        Action::ALL.iter().copied().find(|a| a.info().key == key)
    }

    /// Commands that open a list of choices (drawn with a dropdown arrow).
    pub fn is_menu(self) -> bool {
        use Action::*;
        matches!(
            self,
            Print
                | Wizard
                | Export
                | Help
                | Options
                | TextTools
                | ShapesTools
                | OtherZoom
                | Arrange
                | Align
                | SelectSame
                | RotateFlip
                | Size
                | StampEmboss
                | ModifyOverlap
                | DecorativeFrame
                | FitTextToOutline
                | SendEmbroidery
                | SaveImage
                | ImageDensity
                | GridSpacing
                | ViewShow
                | GuideOptions
                | Font
                | TextSize
                | EditText
                | LineColor
                | LineSewType
                | RegionColor
                | RegionSewType
                | EntryExit
                | SewingPoints
                | EdgeRadius
                | ChangeColor
                | SkeletonWidth
                | SmoothTolerance
                | ShapeEffects
                | LineGap
                | EditCharacter
        )
    }

    /// On/off commands (drawn pressed while on).
    pub fn is_toggle(self) -> bool {
        use Action::*;
        matches!(
            self,
            SolidView
                | StitchView
                | RealisticView
                | ViewThreadTrimming
                | PaneSewingOrder
                | PaneSimulator
                | PaneAttributes
                | PaneImport
                | PaneAssistant
                | ReferenceWindow
                | ShowGrid
                | GridAxes
                | SnapToGrid
                | SnapToObjects
                | PrecisionBar
                | Ruler
                | Guideline
                | FontFilter
                | Pan
                | Measure
                | MoveSections
        )
    }

    /// The drawing tool the command picks, if it is one.
    pub fn tool(self) -> Option<Tool> {
        use Action::*;
        Some(match self {
            SelectTool => Tool::Select,
            PointTool | EditPoint => Tool::Point,
            Rectangle => Tool::Rectangle,
            Ellipse => Tool::Ellipse,
            ClosedStraight => Tool::Polygon,
            ClosedCurve | OpenCurve => Tool::Pen,
            StraightBlock => Tool::Punch,
            OpenStraight => Tool::Line,
            PencilClosed => Tool::Pencil,
            PencilOpen => Tool::PencilOpen,
            CurvedBlock => Tool::CurvedBlock,
            Running => Tool::Running,
            Feed => Tool::Feed,
            SatinSkeleton => Tool::Skeleton,
            Knife => Tool::Knife,
            ShapeBuilder => Tool::ShapeBuilder,
            GradientTool => Tool::Gradient,
            FillAngleTool => Tool::FillAngle,
            _ => return None,
        })
    }
}

/// Ribbon tabs: the three fixed ones and the contextual ones.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TabId {
    Home,
    Image,
    View,
    Text,
    Outline,
    OutlineRect,
    ManualPunch,
    Shapes,
    Stitches,
    HoopSettings,
}

impl TabId {
    pub const FIXED: [TabId; 3] = [TabId::Home, TabId::Image, TabId::View];
    pub const ALL: [TabId; 10] = [
        TabId::Home,
        TabId::Image,
        TabId::View,
        TabId::Text,
        TabId::Outline,
        TabId::OutlineRect,
        TabId::ManualPunch,
        TabId::Shapes,
        TabId::Stitches,
        TabId::HoopSettings,
    ];

    pub fn label(self) -> &'static str {
        super::i18n::text(&format!("tab-{}", self.key()), self.english_label())
    }

    /// The tab's name in English (the source of [`TabId::label`]).
    pub fn english_label(self) -> &'static str {
        match self {
            TabId::Home => "Home",
            TabId::Image => "Image",
            TabId::View => "View",
            TabId::Text => "Text",
            TabId::Outline | TabId::OutlineRect => "Outline",
            TabId::ManualPunch => "Manual Punch",
            TabId::Shapes => "Shapes",
            TabId::Stitches => "Stitches",
            TabId::HoopSettings => "Hoop Settings",
        }
    }

    /// Caption over a contextual tab (what it is for); `None` for the fixed tabs.
    pub fn context(self) -> Option<&'static str> {
        self.english_context()
            .map(|c| super::i18n::text(&format!("tab-{}-context", self.key()), c))
    }

    /// The contextual caption in English (the source of [`TabId::context`]).
    pub fn english_context(self) -> Option<&'static str> {
        match self {
            TabId::Home | TabId::Image | TabId::View => None,
            TabId::Text => Some("Text"),
            TabId::Outline => Some("Shapes (Outline)"),
            TabId::OutlineRect => Some("Shapes (Rectangle)"),
            TabId::ManualPunch => Some("Shapes (Manual Punch)"),
            TabId::Shapes => Some("Shapes (Shapes/Circle/Arc)"),
            TabId::Stitches => Some("Stitches"),
            TabId::HoopSettings => Some("Hoop Settings"),
        }
    }

    /// `--ribbon` value.
    pub fn key(self) -> &'static str {
        match self {
            TabId::Home => "home",
            TabId::Image => "image",
            TabId::View => "view",
            TabId::Text => "text",
            TabId::Outline => "outline",
            TabId::OutlineRect => "rectangle",
            TabId::ManualPunch => "punch",
            TabId::Shapes => "shapes",
            TabId::Stitches => "stitches",
            TabId::HoopSettings => "hoop",
        }
    }

    pub fn from_key(key: &str) -> Option<TabId> {
        TabId::ALL.into_iter().find(|t| t.key() == key)
    }

    pub fn groups(self) -> &'static [Group] {
        match self {
            TabId::Home => HOME,
            TabId::Image => IMAGE,
            TabId::View => VIEW,
            TabId::Text => TEXT,
            TabId::Outline => OUTLINE,
            TabId::OutlineRect => OUTLINE_RECT,
            TabId::ManualPunch => MANUAL_PUNCH,
            TabId::Shapes => SHAPES,
            TabId::Stitches => STITCHES,
            TabId::HoopSettings => HOOP,
        }
    }
}

/// One column of a ribbon group: a large button, or up to three small ones stacked. `A` is
/// the window's command type (the suite windows have their own, see [`super::suite`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item<A: 'static = Action> {
    Large(A),
    Small(&'static [A]),
}

/// A ribbon group: its buttons and the caption under them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Group<A: 'static = Action> {
    pub name: &'static str,
    pub items: &'static [Item<A>],
}

impl<A: Copy> Group<A> {
    /// The caption in this thread's language (message id `group-<name as a slug>`).
    pub fn caption(&self) -> &'static str {
        super::i18n::text(
            &format!("group-{}", super::i18n::slug(self.name)),
            self.name,
        )
    }

    pub fn actions(&self) -> impl Iterator<Item = A> + '_ {
        self.items
            .iter()
            .flat_map(|i| match i {
                Item::Large(a) => std::slice::from_ref(a),
                Item::Small(a) => a,
            })
            .copied()
    }
}

use Action as A;
use Item::{Large, Small};

const SELECT: Group = Group {
    name: "Select",
    items: &[
        Large(A::SelectTool),
        Small(&[A::PointTool, A::SelectAll, A::SelectSame]),
    ],
};

const COLORS: Group = Group {
    name: "Color / Sew Type",
    items: &[
        Small(&[A::LineColor, A::LineSewType]),
        Small(&[A::RegionColor, A::RegionSewType, A::FillAngleTool]),
    ],
};

const HOME: &[Group] = &[
    SELECT,
    Group {
        name: "Clipboard",
        items: &[
            Large(A::Paste),
            Small(&[A::Cut, A::Copy, A::Duplicate]),
            Small(&[A::Delete, A::ArrangeCopy]),
        ],
    },
    Group {
        name: "Tools",
        items: &[
            Large(A::TextTools),
            Large(A::ShapesTools),
            Small(&[A::ZoomIn, A::ZoomOut, A::OtherZoom]),
            Small(&[A::Pan, A::Measure]),
        ],
    },
    Group {
        name: "Import",
        items: &[
            Large(A::ImportPatterns),
            Small(&[A::ImportSvg, A::ImportPesObjects]),
        ],
    },
    Group {
        name: "Edit",
        items: &[
            Small(&[A::Arrange, A::RotateFlip, A::Size]),
            Small(&[A::Group, A::Ungroup, A::FitTextToOutline]),
            Small(&[A::StampEmboss, A::ModifyOverlap, A::DecorativeFrame]),
            Small(&[A::Align]),
        ],
    },
    Group {
        name: "Wizards",
        items: &[
            Large(A::BackgroundFill),
            Small(&[A::Applique, A::Patch, A::Cutwork]),
        ],
    },
    Group {
        name: "Sew",
        items: &[Large(A::SendEmbroidery)],
    },
];

const IMAGE: &[Group] = &[
    Group {
        name: "Image",
        items: &[
            Large(A::OpenImage),
            Small(&[A::PasteImage, A::SaveImage, A::ModifyImage]),
            Small(&[A::ImageDensity, A::ImageRotateRight, A::ImageRotateLeft]),
            Large(A::DeleteImage),
        ],
    },
    Group {
        name: "Image to Stitch",
        items: &[
            Large(A::AutoPunch),
            Large(A::CrossStitch),
            Large(A::PhotoStitch1),
            Large(A::PhotoStitch2),
        ],
    },
    Group {
        name: "Design Center",
        items: &[Large(A::DesignCenter)],
    },
];

const VIEW: &[Group] = &[
    Group {
        name: "View",
        items: &[
            Large(A::SolidView),
            Large(A::StitchView),
            Large(A::RealisticView),
            Small(&[A::ViewThreadTrimming, A::RealisticSettings, A::ViewShow]),
        ],
    },
    Group {
        name: "Show/Hide",
        items: &[
            Small(&[A::PaneSewingOrder, A::PaneSimulator, A::PaneAttributes]),
            Small(&[A::PaneImport, A::ReferenceWindow, A::PaneAssistant]),
            Small(&[A::PrecisionBar]),
        ],
    },
    Group {
        name: "Grid",
        items: &[
            Large(A::ShowGrid),
            Small(&[A::GridAxes, A::SnapToGrid, A::GridSpacing]),
            Small(&[A::SnapToObjects]),
        ],
    },
    Group {
        name: "Ruler",
        items: &[Large(A::Ruler)],
    },
    Group {
        name: "Guideline",
        items: &[Large(A::Guideline), Small(&[A::GuideOptions])],
    },
];

const TEXT: &[Group] = &[
    SELECT,
    Group {
        name: "Text",
        items: &[
            Large(A::TextTools),
            Small(&[A::Font, A::TextSize, A::FontFilter]),
            Small(&[A::FontFilterSettings, A::TrueTypeAttributes]),
        ],
    },
    COLORS,
    Group {
        name: "Edit",
        items: &[
            Large(A::EditText),
            Small(&[A::FitTextToOutline, A::EditCharacter]),
            Small(&[
                A::ConvertToBlocks,
                A::ConvertToStitches,
                A::ConvertToOutline,
            ]),
        ],
    },
];

const OUTLINE: &[Group] = &[
    SELECT,
    Group {
        name: "Shapes",
        items: &[
            Small(&[A::ClosedStraight, A::ClosedCurve, A::PencilClosed]),
            Small(&[A::OpenStraight, A::OpenCurve, A::PencilOpen]),
        ],
    },
    COLORS,
];

const OUTLINE_RECT: &[Group] = &[
    SELECT,
    Group {
        name: "Shapes",
        items: &[Large(A::Rectangle), Small(&[A::EdgeRadius])],
    },
    COLORS,
];

const MANUAL_PUNCH: &[Group] = &[
    SELECT,
    Group {
        name: "Manual Punch",
        items: &[
            Small(&[A::StraightBlock, A::CurvedBlock]),
            Small(&[A::Running, A::Feed]),
            Small(&[A::SatinSkeleton, A::SkeletonWidth]),
        ],
    },
    Group {
        name: "Color / Sew Type",
        items: &[Small(&[A::RegionColor, A::RegionSewType])],
    },
];

const SHAPES: &[Group] = &[
    SELECT,
    Group {
        name: "Edit Outline",
        items: &[
            Large(A::SplitOutline),
            Large(A::Knife),
            Large(A::GradientTool),
            Small(&[
                A::AddDirectionLine,
                A::DeleteDirectionLine,
                A::ReverseDirectionLine,
            ]),
            Small(&[A::EditPoint, A::DeletePoint, A::SplitAtPoint]),
            Small(&[A::DeriveSkeleton, A::SmoothShape, A::SmoothTolerance]),
            Small(&[A::ShapeEffects, A::ShapeBuilder, A::LineGap]),
        ],
    },
    COLORS,
    Group {
        name: "Convert",
        items: &[Small(&[A::ConvertRegionToLine, A::ConvertToStitches])],
    },
    Group {
        name: "Create",
        items: &[
            Small(&[A::OffsetLines, A::FloralPattern]),
            Small(&[A::EntryExit, A::SewingPoints]),
        ],
    },
];

const STITCHES: &[Group] = &[
    SELECT,
    Group {
        name: "Edit",
        items: &[
            Large(A::SplitStitchData),
            Small(&[A::ChangeColor, A::DivideByColor, A::SplitAtPoint]),
            Small(&[A::Connect, A::SmoothStitches]),
        ],
    },
    Group {
        name: "Convert",
        items: &[Small(&[
            A::ConvertToBlocks,
            A::ConvertToOutline,
            A::ConvertToOutlines,
        ])],
    },
];

const HOOP: &[Group] = &[
    SELECT,
    Group {
        name: "Hoop Settings",
        items: &[
            Small(&[A::MoveSections]),
            Large(A::DesignSettings),
            Large(A::ExportSections),
        ],
    },
];

/// Application button menu, top to bottom; [`Action::Options`] and [`Action::Exit`] sit at
/// the bottom.
pub const APPLICATION_MENU: [Action; 12] = [
    A::New,
    A::Open,
    A::OpenSample,
    A::Save,
    A::SaveAs,
    A::Print,
    A::DesignProperty,
    A::DesignSettings,
    A::FabricSelector,
    A::SelectColorPalette,
    A::Wizard,
    A::Export,
];

/// Commands inside the Application menu's Print and Sew Embroidery lists (beyond PE-DESIGN,
/// #452, #507, #513).
pub const SUBMENU_ACTIONS: [Action; 2] = [A::JobSheet, A::TestSwatches];

/// The Option button: other applications and settings.
pub const OPTION_MENU: [Action; 6] = [
    A::DesignDatabase,
    A::StitchCreator,
    A::StitchDesignFactory,
    A::FontCreator,
    A::AttributeSets,
    A::UserThreadCharts,
];

/// The Align dropdown (Home ▸ Edit), beyond PE-DESIGN (#472).
pub const ALIGN_MENU: [Action; 8] = [
    A::AlignLeft,
    A::AlignCenter,
    A::AlignRight,
    A::AlignTop,
    A::AlignMiddle,
    A::AlignBottom,
    A::DistributeHorizontal,
    A::DistributeVertical,
];

/// The Select Same dropdown (Home ▸ Select), beyond PE-DESIGN (#475).
pub const SELECT_SAME_MENU: [Action; 3] =
    [A::SelectSameColor, A::SelectSameType, A::SelectSameBoth];

impl Action {
    /// `object.align`'s edge or `object.distribute`'s axis (as `"x"` / `"y"`) of an Align
    /// dropdown entry.
    pub fn align_edge(self) -> Option<&'static str> {
        Some(match self {
            A::AlignLeft => "left",
            A::AlignCenter => "centerX",
            A::AlignRight => "right",
            A::AlignTop => "top",
            A::AlignMiddle => "centerY",
            A::AlignBottom => "bottom",
            A::DistributeHorizontal => "x",
            A::DistributeVertical => "y",
            _ => return None,
        })
    }

    /// `object.list`'s `match` of a Select Same entry.
    pub fn same_match(self) -> Option<&'static str> {
        Some(match self {
            A::SelectSameColor => "color",
            A::SelectSameType => "type",
            A::SelectSameBoth => "both",
            _ => return None,
        })
    }
}

/// Quick Access Toolbar default.
pub const QAT_DEFAULT: [Action; 7] = [
    A::New,
    A::Open,
    A::Save,
    A::Undo,
    A::Redo,
    A::OtherZoom,
    A::Help,
];

/// Commands that may be added to the Quick Access Toolbar.
pub fn qat_candidates() -> Vec<Action> {
    let mut v: Vec<Action> = QAT_DEFAULT.to_vec();
    for a in [
        A::SaveAs,
        A::Print,
        A::Export,
        A::ImportPatterns,
        A::DesignSettings,
        A::DesignProperty,
        A::ZoomIn,
        A::ZoomOut,
        A::Duplicate,
        A::Delete,
        A::SelectAll,
        A::Group,
        A::Ungroup,
        A::Measure,
        A::RealisticView,
        A::ShowGrid,
        A::SendEmbroidery,
        A::DesignDatabase,
    ] {
        if !v.contains(&a) {
            v.push(a);
        }
    }
    v
}

/// What decides the contextual tabs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    pub tool: Tool,
    /// The selection holds text, shapes, stitch patterns.
    pub text: bool,
    pub shape: bool,
    pub stitches: bool,
    /// The design is larger than one hooping.
    pub sections: bool,
}

impl Default for Facts {
    fn default() -> Self {
        Facts {
            tool: Tool::Select,
            text: false,
            shape: false,
            stitches: false,
            sections: false,
        }
    }
}

/// Contextual tabs shown for the active tool and the selection, in ribbon order.
pub fn contextual_tabs(f: &Facts) -> Vec<TabId> {
    let mut v = Vec::new();
    let mut add = |t: TabId| {
        if !v.contains(&t) {
            v.push(t);
        }
    };
    match f.tool {
        Tool::Polygon | Tool::Pen | Tool::Line | Tool::Pencil | Tool::PencilOpen => {
            add(TabId::Outline)
        }
        Tool::Rectangle => add(TabId::OutlineRect),
        Tool::Punch | Tool::CurvedBlock | Tool::Running | Tool::Feed | Tool::Skeleton => {
            add(TabId::ManualPunch)
        }
        Tool::Ellipse | Tool::Point | Tool::Knife | Tool::ShapeBuilder | Tool::Gradient => {
            add(TabId::Shapes)
        }
        Tool::Select | Tool::FillAngle => {}
    }
    if f.text {
        add(TabId::Text);
    }
    if f.shape {
        add(TabId::Shapes);
    }
    if f.stitches {
        add(TabId::Stitches);
    }
    if f.sections {
        add(TabId::HoopSettings);
    }
    v
}

/// The contextual tab a tool brings forward when it is picked.
pub fn tab_for_tool(tool: Tool) -> Option<TabId> {
    contextual_tabs(&Facts {
        tool,
        ..Facts::default()
    })
    .first()
    .copied()
}

/// Every action placed on a ribbon tab.
pub fn ribbon_actions() -> Vec<Action> {
    let mut v = Vec::new();
    for t in TabId::ALL {
        for g in t.groups() {
            for a in g.actions() {
                if !v.contains(&a) {
                    v.push(a);
                }
            }
        }
    }
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique_and_round_trip() {
        let mut keys: Vec<_> = Action::ALL.iter().map(|a| a.info().key).collect();
        for &a in Action::ALL {
            assert_eq!(Action::from_key(a.info().key), Some(a));
        }
        keys.sort_unstable();
        let n = keys.len();
        keys.dedup();
        assert_eq!(keys.len(), n, "duplicate action key");
        for t in TabId::ALL {
            assert_eq!(TabId::from_key(t.key()), Some(t));
        }
    }

    #[test]
    fn no_group_shows_a_command_twice() {
        for t in TabId::ALL {
            for g in t.groups() {
                let mut seen = std::collections::BTreeSet::new();
                for a in g.actions() {
                    assert!(seen.insert(a), "{t:?} / {}: {a:?} twice", g.name);
                }
            }
        }
    }

    #[test]
    fn every_action_has_a_place_and_a_screen_tip() {
        let ribbon = ribbon_actions();
        // Copy/Paste Attributes live in the Sewing Attributes pane.
        let help = [
            A::Undo,
            A::Redo,
            A::Help,
            A::Options,
            A::Exit,
            A::CopyAttributes,
            A::PasteAttributes,
            // The status bar's indicator and the Sewing Order pane open Check Design (#599).
            A::CheckDesign,
            A::DensityMap,
            A::ReviewSewingOrder,
        ];
        for &a in Action::ALL {
            let placed = ribbon.contains(&a)
                || APPLICATION_MENU.contains(&a)
                || OPTION_MENU.contains(&a)
                || SUBMENU_ACTIONS.contains(&a)
                || help.contains(&a)
                || ALIGN_MENU.contains(&a)
                || SELECT_SAME_MENU.contains(&a)
                || a.tool().is_some()
                || [A::Ellipse, A::Rectangle].contains(&a);
            assert!(placed, "{a:?} is in no tab or menu");
            let info = a.info();
            assert!(!info.label.is_empty() && !info.tip.is_empty(), "{a:?}");
            let tip = info.screen_tip(false);
            assert_eq!(tip.contains("Not yet available"), !info.available, "{a:?}");
            if let Some(s) = info.shortcut {
                assert!(tip.contains(s), "{a:?}");
            }
        }
        assert_eq!(A::Save.info().shortcut_for(true).as_deref(), Some("Cmd+S"));
    }

    #[test]
    fn groups_are_named_and_columns_hold_at_most_three() {
        for t in TabId::ALL {
            assert!(!t.groups().is_empty(), "{t:?}");
            for g in t.groups() {
                assert!(!g.name.is_empty());
                for i in g.items {
                    if let Item::Small(a) = i {
                        assert!((1..=3).contains(&a.len()), "{t:?} {}", g.name);
                    }
                }
            }
            assert_eq!(t.context().is_none(), TabId::FIXED.contains(&t));
        }
        let home: Vec<&str> = TabId::Home.groups().iter().map(|g| g.name).collect();
        assert_eq!(
            home,
            [
                "Select",
                "Clipboard",
                "Tools",
                "Import",
                "Edit",
                "Wizards",
                "Sew"
            ]
        );
    }

    #[test]
    fn contextual_tabs_follow_tool_and_selection() {
        let f = |tool, text, shape, stitches, sections| Facts {
            tool,
            text,
            shape,
            stitches,
            sections,
        };
        assert!(contextual_tabs(&Facts::default()).is_empty());
        assert_eq!(
            contextual_tabs(&f(Tool::Polygon, false, false, false, false)),
            [TabId::Outline]
        );
        assert_eq!(
            contextual_tabs(&f(Tool::Pen, false, false, false, false)),
            [TabId::Outline]
        );
        assert_eq!(tab_for_tool(Tool::Rectangle), Some(TabId::OutlineRect));
        assert_eq!(tab_for_tool(Tool::Punch), Some(TabId::ManualPunch));
        for t in [Tool::CurvedBlock, Tool::Running, Tool::Feed, Tool::Skeleton] {
            assert_eq!(tab_for_tool(t), Some(TabId::ManualPunch), "{t:?}");
        }
        for t in [Tool::Line, Tool::Pencil, Tool::PencilOpen] {
            assert_eq!(tab_for_tool(t), Some(TabId::Outline), "{t:?}");
        }
        assert_eq!(tab_for_tool(Tool::Ellipse), Some(TabId::Shapes));
        assert_eq!(tab_for_tool(Tool::Select), None);
        assert_eq!(
            contextual_tabs(&f(Tool::Select, true, true, true, true)),
            [
                TabId::Text,
                TabId::Shapes,
                TabId::Stitches,
                TabId::HoopSettings
            ]
        );
        assert_eq!(
            contextual_tabs(&f(Tool::Point, false, true, false, false)),
            [TabId::Shapes],
            "no duplicates"
        );
    }

    #[test]
    fn every_engine_command_named_exists() {
        let session = openstitch_engine::Session::new(std::env::temp_dir());
        let ids: Vec<&str> = session.commands().iter().map(|c| c.id).collect();
        for &a in Action::ALL {
            if let Some(e) = a.info().engine {
                assert!(ids.contains(&e), "{a:?} names unknown command {e}");
            }
        }
    }

    #[test]
    fn quick_access_toolbar_defaults() {
        let c = qat_candidates();
        assert!(QAT_DEFAULT.iter().all(|a| c.contains(a)));
        assert_eq!(
            QAT_DEFAULT.map(|a| a.info().label),
            ["New", "Open", "Save", "Undo", "Redo", "Zoom", "Help"]
        );
    }

    /// Every message of the ribbon in English, as `assets/i18n/en.ftl` lists them (#555).
    pub(crate) fn english_messages() -> Vec<(String, &'static str)> {
        use crate::editor::i18n;
        assert_eq!(i18n::language(), i18n::ENGLISH);
        let mut out = vec![(
            "ribbon-not-available".to_string(),
            "Not yet available in OpenStitch.",
        )];
        for &a in Action::ALL {
            let info = a.info();
            out.push((format!("action-{}-label", info.key), info.label));
            out.push((format!("action-{}-tip", info.key), info.tip));
        }
        for t in TabId::ALL {
            out.push((format!("tab-{}", t.key()), t.english_label()));
            if let Some(c) = t.english_context() {
                out.push((format!("tab-{}-context", t.key()), c));
            }
        }
        // The menus' texts (#555 part 2).
        for (id, text) in crate::editor::ui_texts::UI_TEXTS {
            out.push((id.to_string(), text));
        }
        // The printed sheets' words (#555).
        for (id, text) in openstitch_print::texts::TEXTS {
            out.push((id.to_string(), text));
        }
        for t in TabId::ALL {
            for g in t.groups() {
                let id = format!("group-{}", i18n::slug(g.name));
                match out.iter().find(|(i, _)| *i == id) {
                    Some((_, text)) => assert_eq!(*text, g.name, "{id} names two captions"),
                    None => out.push((id, g.name)),
                }
            }
        }
        out
    }

    fn english_catalogue() -> String {
        let mut f = String::from(
            "# OpenStitch interface texts in English (#555): the source every translation copies.\n\
             # Generated from the code: OPENSTITCH_WRITE_I18N=1 cargo test -p openstitch-app --lib\n\
             # the_english_catalogue_is_current. A translation is assets/i18n/<code>.ftl with the\n\
             # same ids and translated texts (see crates/openstitch-app/src/editor/i18n.rs).\n",
        );
        for (id, text) in english_messages() {
            f.push_str(&format!("{id} = {}\n", crate::editor::i18n::escape(text)));
        }
        f
    }

    #[test]
    fn the_english_catalogue_is_current() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../assets/i18n/en.ftl");
        let want = english_catalogue();
        if std::env::var_os("OPENSTITCH_WRITE_I18N").is_some() {
            std::fs::write(&path, &want).expect("write en.ftl");
        }
        let got = std::fs::read_to_string(&path)
            .expect("assets/i18n/en.ftl")
            .replace("\r\n", "\n");
        assert!(
            got == want,
            "assets/i18n/en.ftl is out of date: OPENSTITCH_WRITE_I18N=1 cargo test -p openstitch-app --lib the_english_catalogue_is_current"
        );
        // It reads back as the same texts, so a copy of it is a valid catalogue.
        let parsed = crate::editor::i18n::parse(&got).expect("en.ftl parses");
        let messages = english_messages();
        assert_eq!(parsed.len(), messages.len(), "ids are unique");
        for ((id, text), (pid, ptext)) in messages.iter().zip(&parsed) {
            assert_eq!((id.as_str(), *text), (*pid, ptext.as_str()));
        }
    }

    #[test]
    fn shipped_catalogues_only_use_known_ids() {
        let ids: std::collections::BTreeSet<String> =
            english_messages().into_iter().map(|(id, _)| id).collect();
        for (code, source) in crate::editor::i18n::shipped() {
            for (id, _) in crate::editor::i18n::parse(source).expect("parses") {
                assert!(ids.contains(id), "assets/i18n/{code}.ftl: unknown id {id}");
            }
        }
    }

    #[test]
    fn the_pseudo_locale_reaches_every_ribbon_text() {
        use crate::editor::i18n;
        let english = english_messages();
        assert!(i18n::set_language(i18n::PSEUDO));
        for &a in Action::ALL {
            let info = a.info();
            assert!(
                info.label.starts_with('[') && info.label.ends_with(']'),
                "{a:?} label"
            );
            assert!(info.tip.starts_with('['), "{a:?} tip");
            let tip = info.screen_tip(false);
            assert!(!tip.contains("Not yet available"), "{a:?}: {tip}");
        }
        for t in TabId::ALL {
            assert!(t.label().starts_with('['), "{t:?}");
            assert!(t.context().is_none_or(|c| c.starts_with('[')), "{t:?}");
            for g in t.groups() {
                assert!(g.caption().starts_with('['), "{t:?} {}", g.name);
            }
        }
        // Keys stay English: settings and the Quick Access Toolbar keep working.
        assert_eq!(Action::from_key("open"), Some(A::Open));
        assert!(i18n::set_language(i18n::ENGLISH));
        assert_eq!(A::Open.info().label, "Open");
        assert!(!english.is_empty());
    }
}
