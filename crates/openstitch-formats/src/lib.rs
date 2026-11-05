//! Embroidery machine file codecs. The readers are checked against pyembroidery 1.5.1 oracle
//! fixtures; the writers lay records out as PE-DESIGN 11.50's exports do
//! (`compat/reference/machine`, see `layout`), and what they write is read back by the readers.
//!
//! Every reader takes untrusted bytes and returns a [`FormatError`] for truncated input, bad
//! offsets or oversized counts; none of them panics. Coordinates follow
//! [`openstitch_core::StitchPattern`]: 0.1 mm units, +y down.

#![forbid(unsafe_code)]

use openstitch_core::StitchPattern;
use std::fmt;

mod io;
mod layout;
pub mod lzh;
mod records;

pub mod csd;
pub mod dsb;
pub mod dst;
pub mod dsz;
pub mod exp;
pub mod gcode;
pub mod hus;
pub mod jef;
pub mod ksm;
pub mod pcs;
pub mod pec;
pub mod pes;
pub mod pes11;
pub mod phc;
pub mod sew;
pub mod shv;
pub mod split;
pub mod stitchlist;
pub mod stx;
pub mod tap;
pub mod tbf;
pub mod threadlist;
pub mod u01;
pub mod vip;
pub mod vp3;
pub mod xxx;
pub mod zxy;

/// Why a file could not be read or written.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FormatError {
    Truncated(String),
    BadSignature(String),
    ValueOutOfRange(String),
    UnsupportedFormat(String),
}

impl fmt::Display for FormatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Truncated(s) => write!(f, "Truncated file: {s}"),
            Self::BadSignature(s) => write!(f, "Unrecognised signature: {s}"),
            Self::ValueOutOfRange(s) => write!(f, "Value out of range: {s}"),
            Self::UnsupportedFormat(s) => write!(f, "Unsupported format: {s}"),
        }
    }
}

impl std::error::Error for FormatError {}

pub type Result<T> = std::result::Result<T, FormatError>;

/// Every supported machine file format.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FileType {
    Pes,
    Pec,
    Dst,
    Exp,
    Jef,
    Pcs,
    Sew,
    Xxx,
    Hus,
    Vip,
    Shv,
    /// Read only.
    Phc,
    Csd,
    /// Husqvarna Viking / Pfaff; beyond PE-DESIGN's set (#480).
    Vp3,
    /// Tajima, DST records without a header; beyond PE-DESIGN's set (#481).
    Tap,
    /// Tajima; beyond PE-DESIGN's set (#481).
    Tbf,
    /// Barudan; beyond PE-DESIGN's set (#481).
    U01,
    /// Barudan, read only (#481).
    Dsb,
    /// ZSK, read only (#481).
    Dsz,
    /// Pfaff, read only (#481).
    Ksm,
    /// ZSK, read only (#481).
    Zxy,
    /// Data Stitch, read only (#481).
    Stx,
}

impl FileType {
    pub const ALL: [FileType; 22] = [
        Self::Pes,
        Self::Pec,
        Self::Dst,
        Self::Exp,
        Self::Jef,
        Self::Pcs,
        Self::Sew,
        Self::Xxx,
        Self::Hus,
        Self::Vip,
        Self::Shv,
        Self::Phc,
        Self::Csd,
        Self::Vp3,
        Self::Tap,
        Self::Tbf,
        Self::U01,
        Self::Dsb,
        Self::Dsz,
        Self::Ksm,
        Self::Zxy,
        Self::Stx,
    ];

    /// Whether OpenStitch can write this format ([`FileType::write`] fails otherwise).
    pub const fn can_write(self) -> bool {
        !matches!(
            self,
            Self::Phc | Self::Dsb | Self::Dsz | Self::Ksm | Self::Zxy | Self::Stx
        )
    }

    /// The formats OpenStitch can write.
    pub fn writable() -> impl Iterator<Item = FileType> {
        Self::ALL.into_iter().filter(|t| t.can_write())
    }

    /// Lower-case file extension without the dot.
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Pes => "pes",
            Self::Pec => "pec",
            Self::Dst => "dst",
            Self::Exp => "exp",
            Self::Jef => "jef",
            Self::Pcs => "pcs",
            Self::Sew => "sew",
            Self::Xxx => "xxx",
            Self::Hus => "hus",
            Self::Vip => "vip",
            Self::Shv => "shv",
            Self::Phc => "phc",
            Self::Csd => "csd",
            Self::Vp3 => "vp3",
            Self::Tap => "tap",
            Self::Tbf => "tbf",
            Self::U01 => "u01",
            Self::Dsb => "dsb",
            Self::Dsz => "dsz",
            Self::Ksm => "ksm",
            Self::Zxy => "zxy",
            Self::Stx => "stx",
        }
    }

    /// Case-insensitive lookup by extension (without the dot).
    pub fn from_extension(ext: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.extension().eq_ignore_ascii_case(ext))
    }

    /// Lookup by a path's extension.
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Option<Self> {
        path.as_ref()
            .extension()
            .and_then(|e| e.to_str())
            .and_then(Self::from_extension)
    }

    /// Reads with each format's default options.
    pub fn read(self, data: &[u8]) -> Result<StitchPattern> {
        match self {
            Self::Pes => pes::read(data),
            Self::Pec => pec::read(data),
            Self::Dst => dst::read(data),
            Self::Exp => exp::read(data),
            Self::Jef => jef::read(data),
            Self::Pcs => pcs::read(data),
            Self::Sew => sew::read(data),
            Self::Xxx => xxx::read(data),
            Self::Hus => hus::read(data),
            Self::Vip => vip::read(data),
            Self::Shv => shv::read(data),
            Self::Phc => phc::read(data),
            Self::Csd => csd::read(data),
            Self::Vp3 => vp3::read(data),
            Self::Tap => tap::read(data),
            Self::Tbf => tbf::read(data),
            Self::U01 => u01::read(data),
            Self::Dsb => dsb::read(data),
            Self::Dsz => dsz::read(data),
            Self::Ksm => ksm::read(data),
            Self::Zxy => zxy::read(data),
            Self::Stx => stx::read(data),
        }
    }

    /// Writes with each format's default options (JEF stamps the current UTC time).
    pub fn write(self, pattern: &StitchPattern) -> Result<Vec<u8>> {
        match self {
            Self::Pes => pes::write(pattern),
            Self::Pec => pec::write(pattern),
            Self::Dst => dst::write(pattern),
            Self::Exp => exp::write(pattern),
            Self::Jef => jef::write(pattern),
            Self::Pcs => pcs::write(pattern),
            Self::Sew => sew::write(pattern),
            Self::Xxx => xxx::write(pattern),
            Self::Hus => hus::write(pattern),
            Self::Vip => vip::write(pattern),
            Self::Shv => shv::write(pattern),
            Self::Csd => csd::write(pattern),
            Self::Vp3 => vp3::write(pattern),
            Self::Tap => tap::write(pattern),
            Self::Tbf => tbf::write(pattern),
            Self::U01 => u01::write(pattern),
            Self::Phc | Self::Dsb | Self::Dsz | Self::Ksm | Self::Zxy | Self::Stx => Err(
                FormatError::UnsupportedFormat(format!("{self} is read only")),
            ),
        }
    }
}

impl fmt::Display for FileType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.extension())
    }
}

/// One thread per colour block for formats that encode a stop as a colour change: a stop
/// repeats the current thread instead of advancing.
pub(crate) fn repeating_stops_palette(pattern: &StitchPattern) -> Vec<openstitch_core::Thread> {
    use openstitch_core::Command;
    let Some(first) = pattern.threads.first() else {
        return Vec::new();
    };
    let mut palette = vec![first.clone()];
    let mut block = 0;
    for s in &pattern.stitches {
        match s.command {
            Command::ColorChange => {
                block += 1;
                palette.push(pattern.threads[block.min(pattern.threads.len() - 1)].clone());
            }
            Command::Stop => palette.push(palette[palette.len() - 1].clone()),
            _ => {}
        }
    }
    palette
}
