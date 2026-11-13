//! Machine type, hoops and output settings (PE-DESIGN's Design Settings dialog), plus thread
//! length and sewing-time estimates.
//!
//! Brother hoop sizes come only from Brother's public pages; each entry names its source. Which
//! hoop a given machine model accepts is not published in one place, so every hoop is offered
//! for both machine types (PE-DESIGN warns the user not to pick a hoop the machine cannot use).
//!
//! Other brands' hoops ([`OTHER_HOOPS`], #514, beyond PE-DESIGN: off unless chosen) come from
//! each maker's own published specifications, recorded per entry and in `docs/research/hoops.md`.

use crate::fmath::Det;
use openstitch_core::{Command, StitchPattern};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MachineType {
    /// Single-needle home embroidery machine.
    #[default]
    SingleNeedle,
    /// Multi-needle machine (PE-DESIGN's multi-needle icon); enables jump-stitch trimming
    /// settings.
    MultiNeedle,
}

impl MachineType {
    pub fn as_str(self) -> &'static str {
        match self {
            MachineType::SingleNeedle => "single",
            MachineType::MultiNeedle => "multi",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "single" => Some(MachineType::SingleNeedle),
            "multi" => Some(MachineType::MultiNeedle),
            _ => None,
        }
    }

    /// Default speed for estimates, stitches per minute: a typical value, not a calibration.
    pub fn default_speed(self) -> f64 {
        match self {
            MachineType::SingleNeedle => 800.0,
            MachineType::MultiNeedle => 1000.0,
        }
    }

    /// Seconds a colour change adds to the estimate. A single-needle machine waits for the user
    /// to rethread, which is not sewing time, so it adds nothing.
    pub fn color_change_seconds(self) -> f64 {
        match self {
            MachineType::SingleNeedle => 0.0,
            MachineType::MultiNeedle => 5.0,
        }
    }
}

/// Where the needle starts and how large the exported design is (Design Settings ▸ Output).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum SewingArea {
    /// The needle starts at the centre of the Design Page; the design is as large as the page.
    #[default]
    DesignPage,
    /// "Use existing design area": the needle starts at the centre of the stitches, which keep
    /// their real extent.
    DesignArea,
}

impl SewingArea {
    pub fn as_str(self) -> &'static str {
        match self {
            SewingArea::DesignPage => "page",
            SewingArea::DesignArea => "design",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "page" => Some(SewingArea::DesignPage),
            "design" => Some(SewingArea::DesignArea),
            _ => None,
        }
    }
}

/// The shape of a hoop's sewing field.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum HoopShape {
    /// Square or rectangular field: the whole width × height sews.
    #[default]
    Rectangle,
    /// Oval field: the ellipse inscribed in width × height.
    Oval,
    /// Round field: a circle of diameter width (= height).
    Round,
}

impl HoopShape {
    pub fn as_str(self) -> &'static str {
        match self {
            HoopShape::Rectangle => "rectangle",
            HoopShape::Oval => "oval",
            HoopShape::Round => "round",
        }
    }

    /// Whether model point (`x`, `y`) lies in a field of this shape, `w` × `h` with its top-left
    /// corner at the origin, give or take `tol` (all in the same unit).
    pub fn contains(self, w: f64, h: f64, x: f64, y: f64, tol: f64) -> bool {
        match self {
            HoopShape::Rectangle => x >= -tol && y >= -tol && x <= w + tol && y <= h + tol,
            HoopShape::Oval | HoopShape::Round => {
                let (a, b) = (w / 2.0 + tol, h / 2.0 + tol);
                if a <= 0.0 || b <= 0.0 {
                    return false;
                }
                let (dx, dy) = ((x - w / 2.0) / a, (y - h / 2.0) / b);
                dx * dx + dy * dy <= 1.0
            }
        }
    }

    /// The field's outline as `n` points (a closed polygon, not repeating the first point) in a
    /// `w` × `h` box with its top-left corner at the origin; the four corners for a rectangle.
    pub fn outline(self, w: f64, h: f64, n: usize) -> Vec<(f64, f64)> {
        match self {
            HoopShape::Rectangle => vec![(0.0, 0.0), (w, 0.0), (w, h), (0.0, h)],
            HoopShape::Oval | HoopShape::Round => (0..n.max(8))
                .map(|i| {
                    let t = std::f64::consts::TAU * i as f64 / n.max(8) as f64;
                    (w / 2.0 * (1.0 + t.dcos()), h / 2.0 * (1.0 + t.dsin()))
                })
                .collect(),
        }
    }
}

/// A hoop from a maker's published sizes. Dimensions are the sewing field in millimetres,
/// width × height as the maker lists them.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hoop {
    pub id: &'static str,
    pub width: f64,
    pub height: f64,
    pub label: &'static str,
    /// Multi-position hoop (PE-DESIGN marks these with "*"); designs are split over three
    /// installation positions.
    pub multi_position: bool,
    pub source: &'static str,
    /// The machine brand whose list it is in ([`BRANDS`]).
    pub brand: &'static str,
    pub shape: HoopShape,
    /// Round hoops: the margin between the hoop's inside edge and the sewing field, mm, so the
    /// inside diameter is `width + 2 × clearance`. Zero for other hoops.
    pub clearance: f64,
}

impl Hoop {
    /// Rotate 90° applies to every hoop but a round one, where it changes nothing.
    pub fn can_rotate(&self) -> bool {
        self.shape != HoopShape::Round
    }

    /// Round hoops: the inside diameter the hoop is sold by, mm.
    pub fn inside_diameter(&self) -> Option<f64> {
        (self.shape == HoopShape::Round).then_some(self.width + 2.0 * self.clearance)
    }
}

/// Brands in the order the hoop list shows them: Brother first (PE-DESIGN's list and the
/// default), then the others alphabetically, then commercial round hoops.
pub const BRANDS: [&str; 9] = [
    "Brother",
    "Baby Lock",
    "Bernina",
    "Elna",
    "Husqvarna Viking",
    "Janome",
    "Pfaff",
    "Singer",
    "Commercial round",
];

const GUIDE: &str = "PE-DESIGN 11 guide: Specifying the Design Page Settings";
const UPDATES: &str = "PE-DESIGN 11 update history (support.brother.com ped11update_info)";

const fn hoop(
    id: &'static str,
    width: f64,
    height: f64,
    label: &'static str,
    source: &'static str,
) -> Hoop {
    Hoop {
        id,
        width,
        height,
        label,
        multi_position: false,
        source,
        brand: "Brother",
        shape: HoopShape::Rectangle,
        clearance: 0.0,
    }
}

/// Brother hoop sizes, smallest first. See `docs/research/hoops.md` for the sources.
pub const BROTHER_HOOPS: &[Hoop] = &[
    hoop(
        "100x100",
        100.0,
        100.0,
        "100 × 100 mm (4\" × 4\")",
        "PE-DESIGN 8 manual: Creating Design for Multi-Position Hoops",
    ),
    hoop(
        "130x180",
        130.0,
        180.0,
        "130 × 180 mm (5\" × 7\")",
        "PE-DESIGN 8 manual: Creating Design for Multi-Position Hoops",
    ),
    hoop("100x180", 100.0, 180.0, "100 × 180 mm (4\" × 7\")", UPDATES),
    hoop(
        "160x260",
        160.0,
        260.0,
        "160 × 260 mm (6\" × 10\")",
        "Brother SA441 6\" × 10\" embroidery hoop",
    ),
    hoop(
        "200x300",
        200.0,
        300.0,
        "200 × 300 mm (8\" × 12\")",
        "Brother SA447 / PRH300 8\" × 12\" frames",
    ),
    hoop(
        "254x254",
        254.0,
        254.0,
        "254 × 254 mm (10\" × 10\")",
        UPDATES,
    ),
    hoop(
        "272x272",
        272.0,
        272.0,
        "272 × 272 mm (10-5/8\" × 10-5/8\")",
        UPDATES,
    ),
    hoop(
        "180x360",
        180.0,
        360.0,
        "180 × 360 mm (7\" × 14\")",
        UPDATES,
    ),
    hoop(
        "200x360",
        200.0,
        360.0,
        "200 × 360 mm (8\" × 14\")",
        UPDATES,
    ),
    hoop(
        "240x360",
        240.0,
        360.0,
        "240 × 360 mm (9-1/2\" × 14\")",
        "Brother USA embroidery area listing (9-1/2\" × 14\")",
    ),
    hoop(
        "360x360",
        360.0,
        360.0,
        "360 × 360 mm (14\" × 14\", jumbo frame)",
        UPDATES,
    ),
    hoop(
        "297x465",
        297.0,
        465.0,
        "297 × 465 mm (11-5/8\" × 18-1/4\")",
        UPDATES,
    ),
    Hoop {
        id: "100x172-multi",
        width: 100.0,
        height: 172.0,
        label: "100 × 172 mm * (multi-position)",
        multi_position: true,
        source: GUIDE,
        brand: "Brother",
        shape: HoopShape::Rectangle,
        clearance: 0.0,
    },
    Hoop {
        id: "130x300-multi",
        width: 130.0,
        height: 300.0,
        label: "130 × 300 mm * (multi-position)",
        multi_position: true,
        source: GUIDE,
        brand: "Brother",
        shape: HoopShape::Rectangle,
        clearance: 0.0,
    },
];

pub fn brother_hoop(id: &str) -> Option<&'static Hoop> {
    BROTHER_HOOPS.iter().find(|h| h.id == id)
}

/// Any listed hoop by id: Brother's or another brand's.
pub fn find_hoop(id: &str) -> Option<&'static Hoop> {
    brother_hoop(id).or_else(|| OTHER_HOOPS.iter().find(|h| h.id == id))
}

/// The hoops of one brand (as in [`BRANDS`]), in list order.
pub fn brand_hoops(brand: &str) -> impl Iterator<Item = &'static Hoop> + '_ {
    BROTHER_HOOPS
        .iter()
        .chain(OTHER_HOOPS)
        .filter(move |h| h.brand == brand)
}

#[allow(clippy::too_many_arguments)]
const fn other(
    id: &'static str,
    brand: &'static str,
    label: &'static str,
    width: f64,
    height: f64,
    shape: HoopShape,
    multi_position: bool,
    source: &'static str,
) -> Hoop {
    Hoop {
        id,
        width,
        height,
        label,
        multi_position,
        source,
        brand,
        shape,
        clearance: 0.0,
    }
}

const R: HoopShape = HoopShape::Rectangle;
const JANOME_CATALOGUE: &str = "Janome UK accessories catalogue (2021), janome.co.uk/_files/ugd/64fc51_a57f0ed0d23c45c8a6a0ede779236610.pdf";
const ELNA_830L: &str =
    "Elna eXpressive 830L specifications, elna.com/en-si/model_embroidery_ex830l.php";
const ELNA_900: &str =
    "Elna eXpressive 900 specifications, elna.com/en-si/model_embroidery_elna-expressive-900.php";
const MELCO: &str = "Melco standard frames, melco.com/en-gb/standard-frame/ (inside diameter; \"the actual embroidery field is approx. 3 cm smaller\")";

/// Round commercial hoop: `inside_cm` is the inside diameter the maker sells it by; the field
/// is `clearance_mm` smaller on every side.
const fn round(
    id: &'static str,
    label: &'static str,
    inside_mm: f64,
    clearance_mm: f64,
    source: &'static str,
) -> Hoop {
    let field = inside_mm - 2.0 * clearance_mm;
    Hoop {
        id,
        width: field,
        height: field,
        label,
        multi_position: false,
        source,
        brand: "Commercial round",
        shape: HoopShape::Round,
        clearance: clearance_mm,
    }
}

/// Other brands' hoops (#514), each from the maker's own published size (`source`); see
/// `docs/research/hoops.md`. Width × height as the maker publishes them.
pub const OTHER_HOOPS: &[Hoop] = &[
    // Baby Lock: babylock.com accessory pages.
    other(
        "babylock-4x4",
        "Baby Lock",
        "Embroidery Hoop 4\" × 4\" (100 × 100 mm)",
        100.0,
        100.0,
        R,
        false,
        "Baby Lock Embroidery Hoop and Grid (4\" x 4\"), babylock.com/embroidery-hoop-and-grid-4-x-4",
    ),
    other(
        "babylock-5x7",
        "Baby Lock",
        "Embroidery Hoop 5\" × 7\" (130 × 180 mm)",
        130.0,
        180.0,
        R,
        false,
        "Baby Lock Embroidery Hoop and Grid (5\" x 7\"), babylock.com/embroidery-hoop-and-grid-5x7",
    ),
    other(
        "babylock-6x6",
        "Baby Lock",
        "Embroidery Hoop 6\" × 6\" (150 × 150 mm)",
        150.0,
        150.0,
        R,
        false,
        "Baby Lock Embroidery Hoop and Grid (6\" x 6\"), babylock.com/embroidery-hoop-and-grid-6-x-6",
    ),
    other(
        "babylock-8x8",
        "Baby Lock",
        "Embroidery Hoop 8\" × 8\" (200 × 200 mm)",
        200.0,
        200.0,
        R,
        false,
        "Baby Lock Embroidery Hoop and Grid (8\" x 8\"), babylock.com/embroidery-hoop-and-grid-8-x-8",
    ),
    other(
        "babylock-8x12",
        "Baby Lock",
        "Embroidery Hoop 8\" × 12\" (200 × 300 mm)",
        200.0,
        300.0,
        R,
        false,
        "Baby Lock Embroidery Hoop and Grid (8\" x 12\"), babylock.com/embroidery-hoop-and-grid-8-x-12",
    ),
    other(
        "babylock-1x2.5",
        "Baby Lock",
        "Embroidery Hoop 1\" × 2-1/2\" (20 × 60 mm)",
        20.0,
        60.0,
        R,
        false,
        "Baby Lock Embroidery Hoop and Grid (1\" x 2 1/2\"), babylock.com/embroidery-hoop-and-grid-1-x-2-1-2",
    ),
    other(
        "babylock-4x12-border",
        "Baby Lock",
        "Large Continuous Border Hoop 4\" × 12\" (100 × 300 mm)",
        100.0,
        300.0,
        R,
        false,
        "Baby Lock Large Continuous Border Hoop and Grid (4\" x 12\"), babylock.com/large-continuous-border-hoop-and-grid-4-x-12",
    ),
    // Bernina: bernina.com accessory pages (read through the search index; the site refuses
    // automated fetches).
    other(
        "bernina-medium",
        "Bernina",
        "Medium Hoop 100 × 130 mm",
        100.0,
        130.0,
        R,
        false,
        "BERNINA Medium Embroidery Hoop, bernina.com/en-US/Accessories-US/Embroidery-Accessories/Embroidery-Hoops-and-Adapters/Medium-Embroidery-Hoop",
    ),
    other(
        "bernina-large-oval",
        "Bernina",
        "Large Oval Hoop 145 × 255 mm",
        145.0,
        255.0,
        HoopShape::Oval,
        false,
        "BERNINA Large Oval Embroidery Hoop (14.5 x 25.5 cm), bernina.com/en-US/Accessories-US/Embroidery-Accessories/Embroidery-Hoops-and-Adapters/Large-Oval-Embroidery-Hoop",
    ),
    other(
        "bernina-maxi",
        "Bernina",
        "Maxi Hoop 210 × 400 mm",
        210.0,
        400.0,
        R,
        false,
        "BERNINA Maxi Embroidery Hoop, max. embroidery area 210 x 400 mm, bernina.com/Maxi-Hoop",
    ),
    other(
        "bernina-jumbo",
        "Bernina",
        "Jumbo Hoop 260 × 400 mm",
        260.0,
        400.0,
        R,
        false,
        "BERNINA Jumbo Embroidery Hoop, bernina.com/en-US/Accessories-US/Embroidery-Accessories/Embroidery-Hoops-and-Adapters/Jumbo-Embroidery-Hoop",
    ),
    other(
        "bernina-mega",
        "Bernina",
        "Mega Hoop 150 × 400 mm * (three positions)",
        150.0,
        400.0,
        R,
        true,
        "BERNINA Mega Embroidery Hoop (three attachment positions), bernina.com/en-US/Accessories-US/Embroidery-Accessories/Embroidery-Hoops-and-Adapters/Mega-Embroidery-Hoop",
    ),
    // Elna: elna.com model specifications.
    other(
        "elna-sq14b",
        "Elna",
        "SQ14b 140 × 140 mm",
        140.0,
        140.0,
        R,
        false,
        ELNA_830L,
    ),
    other(
        "elna-re20b",
        "Elna",
        "RE20b 140 × 200 mm",
        140.0,
        200.0,
        R,
        false,
        ELNA_830L,
    ),
    other(
        "elna-sq20b",
        "Elna",
        "SQ20b 200 × 200 mm",
        200.0,
        200.0,
        R,
        false,
        ELNA_830L,
    ),
    other(
        "elna-re28b",
        "Elna",
        "RE28b 200 × 280 mm",
        200.0,
        280.0,
        R,
        false,
        "Elna eXpressive 830 specifications, elna.com/en-si/model_embroidery_elna-830.php",
    ),
    other(
        "elna-re36b",
        "Elna",
        "RE36b 200 × 360 mm",
        200.0,
        360.0,
        R,
        false,
        ELNA_830L,
    ),
    other(
        "elna-sq23",
        "Elna",
        "SQ23 230 × 230 mm",
        230.0,
        230.0,
        R,
        false,
        ELNA_900,
    ),
    other(
        "elna-gr",
        "Elna",
        "GR 230 × 300 mm",
        230.0,
        300.0,
        R,
        false,
        ELNA_900,
    ),
    other(
        "elna-m1",
        "Elna",
        "Hoop M1 240 × 200 mm",
        240.0,
        200.0,
        R,
        false,
        "Elna eXpressive 970 specifications, elna.com/en-si/model_embroidery_elna-970.php",
    ),
    // Husqvarna Viking: the SVP Worldwide store (singer.com), brand-labelled product pages.
    other(
        "viking-standard",
        "Husqvarna Viking",
        "Standard Hoop 100 × 100 mm",
        100.0,
        100.0,
        R,
        false,
        "HUSQVARNA VIKING Standard Hoop 100 x 100 mm, singer.com/products/husqvarna-viking-standard-hoop-100-x-100-mm",
    ),
    other(
        "viking-design-5x7",
        "Husqvarna Viking",
        "5\" × 7\" Design Hoop 180 × 130 mm",
        180.0,
        130.0,
        R,
        false,
        "HUSQVARNA VIKING 5\" x 7\" Design Hoop 180 x 130 mm, singer.com/products/husqvarnaviking-5-x-7-design-hoop-180-x-130-mm",
    ),
    other(
        "viking-jewel",
        "Husqvarna Viking",
        "DESIGNER Jewel Hoop 240 × 150 mm",
        240.0,
        150.0,
        R,
        false,
        "HUSQVARNA VIKING DESIGNER Jewel Hoop II 240 x 150 mm, singer.com/products/husqvarna-viking-designer-jewel-hoop-ii-240-x-150-mm",
    ),
    other(
        "viking-mega-endless",
        "Husqvarna Viking",
        "Mega Endless Hoop 260 × 150 mm",
        260.0,
        150.0,
        R,
        false,
        "HUSQVARNA VIKING Mega Endless Embroidery Hoop 260 x 150 mm, singer.com/products/husqvarnaviking-mega-endless-embroidery-hoop-260-x-150-mm",
    ),
    other(
        "viking-crown",
        "Husqvarna Viking",
        "DESIGNER Crown Hoop 260 × 200 mm",
        260.0,
        200.0,
        R,
        false,
        "HUSQVARNA VIKING DESIGNER Crown Hoop 260 x 200 mm, singer.com/products/husqvarna-viking-designer-crown-hoop-260-x-200-mm",
    ),
    other(
        "viking-royal",
        "Husqvarna Viking",
        "DESIGNER Royal Hoop 360 × 200 mm",
        360.0,
        200.0,
        R,
        false,
        "HUSQVARNA VIKING DESIGNER Royal Hoop 360 x 200 mm, singer.com/products/husqvarna-viking-designer-royal-hoop-360-x-200-mm",
    ),
    other(
        "viking-imperial",
        "Husqvarna Viking",
        "DESIGNER Imperial Hoop 360 × 260 mm",
        360.0,
        260.0,
        R,
        false,
        "HUSQVARNA VIKING DESIGNER Imperial Hoop 360 x 260 mm, singer.com/products/husqvarna-viking-designer-imperial-hoop-360x260mm",
    ),
    other(
        "viking-majestic",
        "Husqvarna Viking",
        "DESIGNER Majestic Hoop 360 × 350 mm",
        360.0,
        350.0,
        R,
        false,
        "HUSQVARNA VIKING DESIGNER Majestic Embroidery Hoop 360 x 350 mm, singer.com/products/husqvarnaviking-designer-majestic-embroidery-hoop-360-x-350-mm",
    ),
    // Janome: Janome UK.
    other(
        "janome-re10b",
        "Janome",
        "RE10b 100 × 40 mm",
        100.0,
        40.0,
        R,
        false,
        JANOME_CATALOGUE,
    ),
    other(
        "janome-sq10e",
        "Janome",
        "SQ10E 100 × 100 mm",
        100.0,
        100.0,
        R,
        false,
        "Janome Memory Craft 100E specifications, janome.co.uk/mc100e",
    ),
    other(
        "janome-sq14b",
        "Janome",
        "SQ14b 140 × 140 mm",
        140.0,
        140.0,
        R,
        false,
        JANOME_CATALOGUE,
    ),
    other(
        "janome-re18",
        "Janome",
        "RE18 (Essential Hoop) 140 × 180 mm",
        140.0,
        180.0,
        R,
        false,
        JANOME_CATALOGUE,
    ),
    other(
        "janome-re20b",
        "Janome",
        "RE20b 140 × 200 mm",
        140.0,
        200.0,
        R,
        false,
        JANOME_CATALOGUE,
    ),
    other(
        "janome-asq22",
        "Janome",
        "ASQ22 220 × 220 mm",
        220.0,
        220.0,
        R,
        false,
        JANOME_CATALOGUE,
    ),
    other(
        "janome-giga-d",
        "Janome",
        "Giga Hoop D 230 × 200 mm * (overlap hoop)",
        230.0,
        200.0,
        R,
        true,
        JANOME_CATALOGUE,
    ),
    other(
        "janome-macro",
        "Janome",
        "Macro Hoop 200 × 280 mm * (multi-position)",
        200.0,
        280.0,
        R,
        true,
        JANOME_CATALOGUE,
    ),
    // Pfaff: the SVP Worldwide store (singer.com), brand-labelled product pages.
    other(
        "pfaff-petite-square",
        "Pfaff",
        "creative Petite Square Hoop 80 × 80 mm",
        80.0,
        80.0,
        R,
        false,
        "PFAFF creative Petite Square Hoop, singer.com/products/pfaff-creative-petite-square-hoop",
    ),
    other(
        "pfaff-120-square",
        "Pfaff",
        "creative 120 Square Hoop 120 × 120 mm",
        120.0,
        120.0,
        R,
        false,
        "PFAFF creative 120 Square Hoop 120 x 120 mm, singer.com/products/pfaff-creative-120-square-hoop-120-x-120-mm",
    ),
    other(
        "pfaff-all-fabric",
        "Pfaff",
        "creative All Fabric Hoop II 150 × 150 mm",
        150.0,
        150.0,
        R,
        false,
        "PFAFF creative All Fabric Hoop II, singer.com/products/pfaff-creative-all-fabric-hoop-ii",
    ),
    other(
        "pfaff-master",
        "Pfaff",
        "creative Master Hoop 240 × 150 mm",
        240.0,
        150.0,
        R,
        false,
        "PFAFF creative Master Hoop 240 x 150 mm, singer.com/products/pfaff-creative-master-hoop-240-x-150-mm",
    ),
    other(
        "pfaff-elite",
        "Pfaff",
        "creative Elite Hoop 260 × 200 mm",
        260.0,
        200.0,
        R,
        false,
        "PFAFF creative Elite Hoop 260 x 200, singer.com/products/pfaff-creative-elite-hoop-260-x-200",
    ),
    other(
        "pfaff-grand-quilters",
        "Pfaff",
        "creative Grand Quilters Hoop 260 × 260 mm",
        260.0,
        260.0,
        R,
        false,
        "PFAFF creative Grand Quilters Hoop, singer.com/products/pfaff-creative-grand-quilters-hoop",
    ),
    other(
        "pfaff-deluxe",
        "Pfaff",
        "creative Deluxe Hoop 360 × 200 mm",
        360.0,
        200.0,
        R,
        false,
        "PFAFF creative Deluxe Hoop 360 x 200 mm, singer.com/products/pfaff-creative-deluxe-hoop-360-x-200-mm",
    ),
    other(
        "pfaff-supreme",
        "Pfaff",
        "creative Supreme Hoop 360 × 260 mm",
        360.0,
        260.0,
        R,
        false,
        "PFAFF creative Supreme Hoop 360 x 260 mm, singer.com/products/pfaff-creative-supreme-hoop-360-x-260-mm",
    ),
    // Singer: singer.com.
    other(
        "singer-40x40",
        "Singer",
        "40 × 40 mm Hoop",
        40.0,
        40.0,
        R,
        false,
        "SINGER 40 x 40mm Embroidery Hoop, singer.com/products/singer-40-x-40mm-embroidery-hoop",
    ),
    other(
        "singer-100x100",
        "Singer",
        "100 × 100 mm Hoop",
        100.0,
        100.0,
        R,
        false,
        "SINGER 100 x 100mm Embroidery Hoop, singer.com/products/singer-100-x-100mm-embroidery-hoop",
    ),
    other(
        "singer-170x100",
        "Singer",
        "170 × 100 mm Hoop (SE9180)",
        170.0,
        100.0,
        R,
        false,
        "SINGER SE9180 included 170x100mm hoop, singer.com/products/singer-se9180-sewing-and-embroidery-machine",
    ),
    // Commercial round hoops by inside diameter; the field is 3 cm smaller (15 mm clearance).
    round("round-9cm", "Round 9 cm (field Ø 60 mm)", 90.0, 15.0, MELCO),
    round(
        "round-12cm",
        "Round 12 cm (field Ø 90 mm)",
        120.0,
        15.0,
        MELCO,
    ),
    round(
        "round-15cm",
        "Round 15 cm (field Ø 120 mm)",
        150.0,
        15.0,
        MELCO,
    ),
    round(
        "round-18cm",
        "Round 18 cm (field Ø 150 mm)",
        180.0,
        15.0,
        MELCO,
    ),
    round(
        "round-22cm",
        "Round 22 cm (field Ø 190 mm)",
        220.0,
        15.0,
        MELCO,
    ),
];

/// A hoop size the user added (Edit User Hoop). Millimetres.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserHoop {
    pub id: u32,
    pub width: f64,
    pub height: f64,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
}

/// The selected hoop: a Brother hoop id, `user-<n>` for a user hoop, or `None` for a custom
/// Design Page size.
pub fn user_hoop_key(id: u32) -> String {
    format!("user-{id}")
}

/// Design Settings stored with a design. Everything defaults to what designs had before these
/// settings existed, and the field is left out of `.osd` files while it is at the default.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MachineSettings {
    pub machine: MachineType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hoop: Option<String>,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub rotate90: bool,
    pub sewing_area: SewingArea,
    /// Multi-needle only: jumps at least this long (mm) are trimmed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_jump_trim: Option<f64>,
    /// Stitches per minute for the sewing-time estimate; the machine type's default when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub user_hoops: Vec<UserHoop>,
    /// Split design: hoop sections of a custom Design Page.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sections: Option<crate::sections::SectionSettings>,
    /// Multi-position hoops: group patterns by installation position.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub optimize_hoop_change: bool,
    /// Jumbo frame: overlap of Side 1 and Side 2 around the middle, mm.
    #[serde(skip_serializing_if = "is_zero")]
    pub jumbo_overlap: f64,
    /// The Design Page is a Custom Size page even where PE-DESIGN would hold it on a Hoop Size
    /// one (a 100 x 100 mm page, a known hoop): PE-DESIGN 11.50's Design Settings choice, which
    /// its PES 0110 header stores and its stitches follow ([`crate::Design::hoop_size_page`],
    /// #1013). Off by default, as a new design in PE-DESIGN starts on a Hoop Size page.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub custom_page: bool,
}

fn is_zero(v: &f64) -> bool {
    *v == 0.0
}

impl MachineSettings {
    pub fn is_default(&self) -> bool {
        *self == MachineSettings::default()
    }

    pub fn user_hoop(&self, key: &str) -> Option<&UserHoop> {
        let id: u32 = key.strip_prefix("user-")?.parse().ok()?;
        self.user_hoops.iter().find(|h| h.id == id)
    }

    pub fn next_user_hoop_id(&self) -> u32 {
        self.user_hoops.iter().map(|h| h.id + 1).max().unwrap_or(1)
    }

    /// Design Page width × height in mm for the selected hoop with rotation applied, if a hoop is selected
    /// and still exists.
    pub fn hoop_size(&self) -> Option<(f64, f64)> {
        let key = self.hoop.as_deref()?;
        let (w, h) = match (find_hoop(key), self.user_hoop(key)) {
            // The jumbo frame's Design Page is smaller than the frame (guide: 350 × 350).
            (Some(b), _) if b.id == crate::sections::JUMBO => (
                crate::sections::JUMBO_PAGE_MM,
                crate::sections::JUMBO_PAGE_MM,
            ),
            (Some(b), _) => (b.width, b.height),
            (None, Some(u)) => (u.width, u.height),
            _ => return None,
        };
        Some(if self.rotate90 { (h, w) } else { (w, h) })
    }

    /// The selected listed hoop, if any (not a user hoop or a custom page).
    pub fn listed_hoop(&self) -> Option<&'static Hoop> {
        self.hoop.as_deref().and_then(find_hoop)
    }

    /// The shape of the selected hoop's field; a rectangle for user hoops and custom pages.
    pub fn hoop_shape(&self) -> HoopShape {
        self.listed_hoop().map_or(HoopShape::Rectangle, |h| h.shape)
    }

    /// Multi-needle "minimum jump stitch length for trimming" (mm) when the design sets none.
    /// PE-DESIGN's own default is not observed yet; 3 mm is what OpenStitch sewed before.
    pub const DEFAULT_MIN_JUMP_TRIM: f64 = 3.0;

    /// Travel longer than this (0.1 mm) is trimmed by the compiler: on multi-needle machines
    /// the design's minimum jump length, else `fallback` (the compiler's single-needle rule).
    pub fn trim_distance(&self, fallback: f64) -> f64 {
        match (self.machine, self.min_jump_trim) {
            (MachineType::MultiNeedle, Some(mm)) => mm * 10.0,
            (MachineType::MultiNeedle, None) => Self::DEFAULT_MIN_JUMP_TRIM * 10.0,
            _ => fallback,
        }
    }

    pub fn speed(&self) -> f64 {
        self.speed.unwrap_or(self.machine.default_speed())
    }
}

/// Thread allowance per needle penetration (mm): the needle thread goes into the fabric and
/// back. A modelling constant, not calibrated against PE-DESIGN.
pub const PENETRATION_ALLOWANCE_MM: f64 = 1.0;
/// Needle thread left as tails at each trim (mm).
pub const TRIM_TAIL_MM: f64 = 10.0;
/// Bobbin thread as a fraction of the needle thread (the usual 1:3 rule of thumb).
pub const BOBBIN_RATIO: f64 = 1.0 / 3.0;
/// Seconds a thread trim takes.
pub const TRIM_SECONDS: f64 = 3.0;

/// Thread consumption and sewing time for a compiled design.
#[derive(Clone, Debug, PartialEq)]
pub struct Estimate {
    /// Needle thread per colour block, mm.
    pub needle_thread: Vec<f64>,
    pub bobbin_thread: f64,
    pub seconds: f64,
}

impl Estimate {
    pub fn needle_total(&self) -> f64 {
        self.needle_thread.iter().sum()
    }
}

/// Estimates thread length and sewing time. Needle thread is the sewn path plus
/// [`PENETRATION_ALLOWANCE_MM`] per stitch and [`TRIM_TAIL_MM`] per trim; time is every stitch and
/// jump at `settings.speed()`, plus [`TRIM_SECONDS`] per trim and the machine type's colour-change
/// time.
pub fn estimate(pattern: &StitchPattern, settings: &MachineSettings) -> Estimate {
    let mut needle = vec![0.0];
    let mut last: Option<(f64, f64)> = None;
    let (mut stitches, mut jumps, mut trims, mut changes) = (0usize, 0usize, 0usize, 0usize);
    for s in &pattern.stitches {
        let here = (s.x, s.y);
        match s.command {
            Command::Stitch => {
                stitches += 1;
                let run = last.map_or(0.0, |(x, y)| (s.x - x).dhypot(s.y - y) / 10.0);
                *needle.last_mut().expect("non-empty") += run + PENETRATION_ALLOWANCE_MM;
            }
            Command::Jump => jumps += 1,
            Command::Trim => {
                trims += 1;
                *needle.last_mut().expect("non-empty") += TRIM_TAIL_MM;
            }
            Command::ColorChange => {
                changes += 1;
                needle.push(0.0);
            }
            Command::Stop | Command::End => {}
        }
        last = Some(here);
    }
    let total: f64 = needle.iter().sum();
    let seconds = (stitches + jumps) as f64 * 60.0 / settings.speed()
        + trims as f64 * TRIM_SECONDS
        + changes as f64 * settings.machine.color_change_seconds();
    Estimate {
        needle_thread: needle,
        bobbin_thread: total * BOBBIN_RATIO,
        seconds,
    }
}

#[cfg(test)]
mod hoop_list_tests {
    use super::*;

    #[test]
    fn every_hoop_has_a_size_a_brand_and_a_source() {
        let mut ids = std::collections::BTreeSet::new();
        for h in BROTHER_HOOPS.iter().chain(OTHER_HOOPS) {
            assert!(h.width > 0.0 && h.height > 0.0, "{}", h.id);
            assert!(h.source.len() > 10, "{} needs a source", h.id);
            assert!(BRANDS.contains(&h.brand), "{}: brand {}", h.id, h.brand);
            assert!(ids.insert(h.id), "duplicate id {}", h.id);
            assert_eq!(find_hoop(h.id), Some(h));
            assert!(!h.label.is_empty());
        }
        // Brother stays PE-DESIGN's list, unchanged and first.
        assert_eq!(BRANDS[0], "Brother");
        assert!(BROTHER_HOOPS.iter().all(|h| h.brand == "Brother"
            && h.shape == HoopShape::Rectangle
            && h.clearance == 0.0));
        assert!(OTHER_HOOPS.iter().all(|h| h.brand != "Brother"));
        for brand in [
            "Janome",
            "Bernina",
            "Husqvarna Viking",
            "Pfaff",
            "Baby Lock",
            "Elna",
            "Singer",
            "Commercial round",
        ] {
            assert!(brand_hoops(brand).count() >= 3, "{brand}");
        }
        assert_eq!(brand_hoops("Brother").count(), BROTHER_HOOPS.len());
    }

    #[test]
    fn round_hoops_are_sold_by_inside_diameter_and_do_not_rotate() {
        let r = find_hoop("round-15cm").expect("round");
        assert_eq!(r.shape, HoopShape::Round);
        assert_eq!((r.width, r.height), (120.0, 120.0));
        assert_eq!(r.inside_diameter(), Some(150.0));
        assert!(!r.can_rotate());
        let oval = find_hoop("bernina-large-oval").expect("oval");
        assert!(oval.can_rotate() && oval.inside_diameter().is_none());
        let m = MachineSettings {
            hoop: Some("janome-re20b".into()),
            rotate90: true,
            ..MachineSettings::default()
        };
        assert_eq!(m.hoop_size(), Some((200.0, 140.0)));
        assert_eq!(m.hoop_shape(), HoopShape::Rectangle);
    }

    #[test]
    fn oval_and_round_fields_contain_only_their_ellipse() {
        let (w, h) = (145.0, 255.0);
        let oval = HoopShape::Oval;
        assert!(oval.contains(w, h, w / 2.0, h / 2.0, 0.0));
        assert!(oval.contains(w, h, 0.0, h / 2.0, 0.0), "edge midpoint");
        assert!(
            !oval.contains(w, h, 5.0, 5.0, 0.0),
            "the box corner is outside"
        );
        assert!(HoopShape::Rectangle.contains(w, h, 5.0, 5.0, 0.0));
        assert!(!HoopShape::Rectangle.contains(w, h, -1.0, 5.0, 0.5));
        for (x, y) in oval.outline(w, h, 64) {
            let (dx, dy) = ((x - w / 2.0) / (w / 2.0), (y - h / 2.0) / (h / 2.0));
            assert!((dx * dx + dy * dy - 1.0).abs() < 1e-9);
            assert!(oval.contains(w, h, x, y, 1e-6));
        }
        assert_eq!(HoopShape::Rectangle.outline(w, h, 64).len(), 4);
        assert_eq!(HoopShape::Round.outline(10.0, 10.0, 3).len(), 8);
    }
}
