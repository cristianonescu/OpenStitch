//! FAT12 1.44 MB floppy disk images (#876, beyond PE-DESIGN) for older embroidery machines that
//! read only floppy disks, many now fitted with a USB floppy emulator that takes such an image.
//!
//! The image is the standard 3.5" high-density layout: 2,880 sectors of 512 bytes, one reserved
//! (boot) sector, two copies of a 9-sector FAT, a 224-entry root directory and 2,847 one-sector
//! clusters. Files live in the root directory under 8.3 names, which is what those machines read;
//! long names get a clear short name (`MY DESIGN.DST` → `MYDESI~1.DST`).
//!
//! [`FloppyImage::parse`] takes untrusted bytes: anything but the 1.44 MB layout, a cluster
//! chain that leaves the disk, loops, crosses another file or does not match the file's size is
//! a [`FatError`], never a panic. Edits work on the bytes in memory, so a caller that writes the
//! image only after every edit succeeded leaves the file unchanged when one fails.

use std::collections::HashSet;
use std::fmt;

/// Bytes in a 1.44 MB image.
pub const IMAGE_SIZE: usize = 1_474_560;
const SECTOR: usize = 512;
const TOTAL_SECTORS: usize = 2_880;
const FAT_SECTORS: usize = 9;
const FAT_START: usize = 1;
const ROOT_ENTRIES: usize = 224;
const ROOT_START: usize = FAT_START + 2 * FAT_SECTORS; // sector 19
const DATA_START: usize = ROOT_START + ROOT_ENTRIES * 32 / SECTOR; // sector 33
/// Highest cluster number: clusters 2..=LAST_CLUSTER hold data.
const LAST_CLUSTER: u16 = (TOTAL_SECTORS - DATA_START + 1) as u16; // 2848
const END_OF_CHAIN: u16 = 0xFFF;
const MEDIA: u8 = 0xF0;
/// Directory entry attributes.
const ATTR_VOLUME: u8 = 0x08;
const ATTR_DIRECTORY: u8 = 0x10;
const ATTR_ARCHIVE: u8 = 0x20;
const ATTR_LONG_NAME: u8 = 0x0F;
/// The date written on new entries: 2000-01-01, 00:00 (fixed, so an image is reproducible).
const DATE: u16 = (20 << 9) | (1 << 5) | 1;

/// Why an image could not be read or changed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FatError {
    /// Not a FAT12 1.44 MB floppy image.
    NotAFloppy(String),
    /// The image's file table is damaged.
    Damaged(String),
    /// A file of that 8.3 name is already on the disk.
    Exists(String),
    NotFound(String),
    /// Not enough free space, or no free root directory entry.
    Full(String),
    InvalidName(String),
}

impl fmt::Display for FatError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAFloppy(s) => write!(f, "not a 1.44 MB FAT12 floppy image: {s}"),
            Self::Damaged(s) => write!(f, "damaged floppy image: {s}"),
            Self::Exists(s) => write!(f, "{s} is already on the disk"),
            Self::NotFound(s) => write!(f, "{s} is not on the disk"),
            Self::Full(s) => write!(f, "the disk is full: {s}"),
            Self::InvalidName(s) => write!(f, "invalid file name: {s}"),
        }
    }
}

impl std::error::Error for FatError {}

/// One root directory entry: a file, or a folder (shown, never changed).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// 8.3 name as `NAME.EXT` (no dot without an extension).
    pub name: String,
    pub size: u32,
    pub directory: bool,
    /// Root directory slot.
    slot: usize,
    first_cluster: u16,
}

/// A 1.44 MB FAT12 image held in memory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FloppyImage {
    bytes: Vec<u8>,
}

impl FloppyImage {
    /// An empty, formatted disk with volume label `label` (up to 11 characters, upper-cased;
    /// none when empty).
    pub fn blank(label: &str) -> Self {
        let mut b = vec![0u8; IMAGE_SIZE];
        // Boot sector: a short jump over the BIOS parameter block, then the block itself.
        b[0..3].copy_from_slice(&[0xEB, 0x3C, 0x90]);
        b[3..11].copy_from_slice(b"OPENSTCH");
        put16(&mut b, 11, SECTOR as u16);
        b[13] = 1; // sectors per cluster
        put16(&mut b, 14, FAT_START as u16); // reserved sectors
        b[16] = 2; // FAT copies
        put16(&mut b, 17, ROOT_ENTRIES as u16);
        put16(&mut b, 19, TOTAL_SECTORS as u16);
        b[21] = MEDIA;
        put16(&mut b, 22, FAT_SECTORS as u16);
        put16(&mut b, 24, 18); // sectors per track
        put16(&mut b, 26, 2); // heads
        b[36] = 0x00; // drive number
        b[38] = 0x29; // extended boot signature
        b[39..43].copy_from_slice(&0x4F53_4C31u32.to_le_bytes()); // fixed serial
        let label = volume_label(label);
        b[43..54].copy_from_slice(&label.unwrap_or(*b"NO NAME    "));
        b[54..62].copy_from_slice(b"FAT12   ");
        // Not bootable: the code prints nothing and waits (int 18h falls back to the BIOS).
        b[62..64].copy_from_slice(&[0xCD, 0x18]);
        b[510] = 0x55;
        b[511] = 0xAA;
        let mut image = FloppyImage { bytes: b };
        image.set_fat(0, 0xF00 | u16::from(MEDIA));
        image.set_fat(1, END_OF_CHAIN);
        if let Some(l) = label {
            let at = ROOT_START * SECTOR;
            image.bytes[at..at + 11].copy_from_slice(&l);
            image.bytes[at + 11] = ATTR_VOLUME;
            put16(&mut image.bytes, at + 24, DATE);
        }
        image
    }

    /// Reads an image, checking its layout and every file's cluster chain.
    pub fn parse(bytes: &[u8]) -> Result<Self, FatError> {
        if bytes.len() != IMAGE_SIZE {
            return Err(FatError::NotAFloppy(format!(
                "{} bytes, a 1.44 MB image has {IMAGE_SIZE}",
                bytes.len()
            )));
        }
        let want: [(usize, usize, usize, &str); 7] = [
            (11, 2, SECTOR, "bytes per sector"),
            (13, 1, 1, "sectors per cluster"),
            (14, 2, FAT_START, "reserved sectors"),
            (16, 1, 2, "FAT copies"),
            (17, 2, ROOT_ENTRIES, "root directory entries"),
            (19, 2, TOTAL_SECTORS, "sectors"),
            (22, 2, FAT_SECTORS, "sectors per FAT"),
        ];
        for (at, len, value, what) in want {
            let got = if len == 1 {
                usize::from(bytes[at])
            } else {
                usize::from(get16(bytes, at))
            };
            if got != value {
                return Err(FatError::NotAFloppy(format!(
                    "{what} is {got}, not {value}"
                )));
            }
        }
        let image = FloppyImage {
            bytes: bytes.to_vec(),
        };
        image.check()?;
        Ok(image)
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn into_bytes(self) -> Vec<u8> {
        self.bytes
    }

    /// The volume label, if the disk has one.
    pub fn label(&self) -> Option<String> {
        (0..ROOT_ENTRIES).find_map(|slot| {
            let e = self.raw_entry(slot);
            (e[0] != 0 && e[0] != 0xE5 && e[11] & ATTR_LONG_NAME == ATTR_VOLUME)
                .then(|| String::from_utf8_lossy(&e[..11]).trim_end().to_string())
        })
    }

    /// Files and folders in the root directory, in directory order.
    pub fn entries(&self) -> Vec<Entry> {
        let mut out = Vec::new();
        for slot in 0..ROOT_ENTRIES {
            let e = self.raw_entry(slot);
            if e[0] == 0 {
                break;
            }
            if e[0] == 0xE5 || e[11] & ATTR_LONG_NAME == ATTR_LONG_NAME || e[11] & ATTR_VOLUME != 0
            {
                continue;
            }
            let mut base = e[..8].to_vec();
            if base[0] == 0x05 {
                base[0] = 0xE5; // a name that really starts with 0xE5
            }
            let base = String::from_utf8_lossy(&base).trim_end().to_string();
            let ext = String::from_utf8_lossy(&e[8..11]).trim_end().to_string();
            out.push(Entry {
                name: if ext.is_empty() {
                    base
                } else {
                    format!("{base}.{ext}")
                },
                size: u32::from_le_bytes([e[28], e[29], e[30], e[31]]),
                directory: e[11] & ATTR_DIRECTORY != 0,
                slot,
                first_cluster: get16(e, 26),
            });
        }
        out
    }

    /// Free space, bytes.
    pub fn free_bytes(&self) -> u64 {
        (2..=LAST_CLUSTER).filter(|&c| self.fat(c) == 0).count() as u64 * SECTOR as u64
    }

    /// Space for files on an empty disk, bytes.
    pub fn capacity(&self) -> u64 {
        u64::from(LAST_CLUSTER - 1) * SECTOR as u64
    }

    /// A file's contents.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, FatError> {
        let e = self.find(name)?;
        if e.directory {
            return Err(FatError::NotFound(format!("{name} is a folder")));
        }
        let mut out = Vec::with_capacity(e.size as usize);
        for c in self.chain(&e)? {
            let at = cluster_offset(c);
            out.extend_from_slice(&self.bytes[at..at + SECTOR]);
        }
        out.truncate(e.size as usize);
        Ok(out)
    }

    /// Adds a file under the 8.3 name made from `name` ([`short_name`]); with `overwrite`, a file
    /// already stored under that name is replaced, else it is an [`FatError::Exists`]. Returns
    /// the stored name.
    pub fn add(&mut self, name: &str, data: &[u8], overwrite: bool) -> Result<String, FatError> {
        let taken: Vec<String> = self.entries().into_iter().map(|e| e.name).collect();
        let stored = short_name(name, &taken)?;
        if taken.iter().any(|t| t == &stored) {
            if !overwrite {
                return Err(FatError::Exists(stored));
            }
            self.remove(&stored)?;
        }
        let size = u32::try_from(data.len())
            .map_err(|_| FatError::Full(format!("{name} is too large")))?;
        let needed = data.len().div_ceil(SECTOR);
        let free: Vec<u16> = (2..=LAST_CLUSTER)
            .filter(|&c| self.fat(c) == 0)
            .take(needed)
            .collect();
        if free.len() < needed {
            return Err(FatError::Full(format!(
                "{stored} needs {} bytes, {} are free",
                needed * SECTOR,
                self.free_bytes()
            )));
        }
        let slot = (0..ROOT_ENTRIES)
            .find(|&s| matches!(self.raw_entry(s)[0], 0 | 0xE5))
            .ok_or_else(|| FatError::Full("the root directory holds 224 entries".into()))?;
        for (k, &c) in free.iter().enumerate() {
            let at = cluster_offset(c);
            let chunk = &data[k * SECTOR..((k + 1) * SECTOR).min(data.len())];
            self.bytes[at..at + chunk.len()].copy_from_slice(chunk);
            self.bytes[at + chunk.len()..at + SECTOR].fill(0);
            self.set_fat(c, free.get(k + 1).copied().unwrap_or(END_OF_CHAIN));
        }
        let (base, ext) = stored.split_once('.').unwrap_or((&stored, ""));
        let mut raw = [b' '; 11];
        raw[..base.len()].copy_from_slice(base.as_bytes());
        raw[8..8 + ext.len()].copy_from_slice(ext.as_bytes());
        if raw[0] == 0xE5 {
            raw[0] = 0x05;
        }
        let at = ROOT_START * SECTOR + slot * 32;
        let e = &mut self.bytes[at..at + 32];
        e.fill(0);
        e[..11].copy_from_slice(&raw);
        e[11] = ATTR_ARCHIVE;
        put16(e, 16, DATE); // created
        put16(e, 18, DATE); // accessed
        put16(e, 24, DATE); // written
        put16(e, 26, free.first().copied().unwrap_or(0));
        e[28..32].copy_from_slice(&size.to_le_bytes());
        Ok(stored)
    }

    /// Deletes a file (by its 8.3 name, any case). Folders are not removed.
    pub fn remove(&mut self, name: &str) -> Result<(), FatError> {
        let e = self.find(name)?;
        if e.directory {
            return Err(FatError::InvalidName(format!(
                "{name} is a folder; only files are removed"
            )));
        }
        for c in self.chain(&e)? {
            self.set_fat(c, 0);
        }
        self.bytes[ROOT_START * SECTOR + e.slot * 32] = 0xE5;
        Ok(())
    }

    fn find(&self, name: &str) -> Result<Entry, FatError> {
        let want = name.trim().to_ascii_uppercase();
        self.entries()
            .into_iter()
            .find(|e| e.name == want)
            .ok_or_else(|| FatError::NotFound(name.to_string()))
    }

    /// Checks every entry's chain: inside the disk, no loop, no cluster shared with another
    /// entry, and (for files) as long as the size says.
    fn check(&self) -> Result<(), FatError> {
        let mut used: HashSet<u16> = HashSet::new();
        for e in self.entries() {
            let chain = self.chain(&e)?;
            if !e.directory {
                let want = (e.size as usize).div_ceil(SECTOR);
                if chain.len() != want {
                    return Err(FatError::Damaged(format!(
                        "{} is {} bytes but its chain holds {} clusters",
                        e.name,
                        e.size,
                        chain.len()
                    )));
                }
            }
            for c in chain {
                if !used.insert(c) {
                    return Err(FatError::Damaged(format!(
                        "{} shares cluster {c} with another file",
                        e.name
                    )));
                }
            }
        }
        Ok(())
    }

    /// The clusters of `e`, in order.
    fn chain(&self, e: &Entry) -> Result<Vec<u16>, FatError> {
        let mut out = Vec::new();
        let mut c = e.first_cluster;
        if c == 0 {
            return if e.size == 0 || e.directory {
                Ok(out)
            } else {
                Err(FatError::Damaged(format!(
                    "{} has {} bytes but no clusters",
                    e.name, e.size
                )))
            };
        }
        let mut seen = HashSet::new();
        loop {
            if !(2..=LAST_CLUSTER).contains(&c) {
                return Err(FatError::Damaged(format!(
                    "{}'s chain leaves the disk at cluster {c}",
                    e.name
                )));
            }
            if !seen.insert(c) {
                return Err(FatError::Damaged(format!(
                    "{}'s chain loops at cluster {c}",
                    e.name
                )));
            }
            out.push(c);
            let next = self.fat(c);
            match next {
                0xFF8..=0xFFF => return Ok(out),
                0 => {
                    return Err(FatError::Damaged(format!(
                        "{}'s chain runs into free cluster {c}",
                        e.name
                    )));
                }
                0xFF7 => {
                    return Err(FatError::Damaged(format!(
                        "{}'s chain runs into a bad cluster after {c}",
                        e.name
                    )));
                }
                _ => c = next,
            }
        }
    }

    fn raw_entry(&self, slot: usize) -> &[u8] {
        let at = ROOT_START * SECTOR + slot * 32;
        &self.bytes[at..at + 32]
    }

    /// FAT entry `n` (from the first copy).
    fn fat(&self, n: u16) -> u16 {
        let at = FAT_START * SECTOR + usize::from(n) * 3 / 2;
        let v = get16(&self.bytes, at);
        if n.is_multiple_of(2) {
            v & 0xFFF
        } else {
            v >> 4
        }
    }

    /// Sets FAT entry `n` in both copies.
    fn set_fat(&mut self, n: u16, value: u16) {
        for copy in 0..2 {
            let at = (FAT_START + copy * FAT_SECTORS) * SECTOR + usize::from(n) * 3 / 2;
            let old = get16(&self.bytes, at);
            let new = if n.is_multiple_of(2) {
                (old & 0xF000) | (value & 0xFFF)
            } else {
                (old & 0x000F) | (value << 4)
            };
            put16(&mut self.bytes, at, new);
        }
    }
}

/// The 8.3 name a file called `name` is stored under, unique among `taken` (8.3 names): upper
/// case, characters DOS does not allow in a name as `_`, spaces and extra dots dropped; a base
/// over eight characters, or one that changed, becomes its first six (or fewer) characters and
/// `~1`, `~2`, …, as Windows does. A name already in 8.3 form is kept even if taken, so it can
/// be replaced.
pub fn short_name(name: &str, taken: &[String]) -> Result<String, FatError> {
    let file = name.rsplit(['/', '\\']).next().unwrap_or(name).trim();
    let (base, ext) = match file.rfind('.') {
        Some(i) if i > 0 => (&file[..i], &file[i + 1..]),
        _ => (file, ""),
    };
    let clean = |s: &str| -> (String, bool) {
        let mut out = String::new();
        let mut lossy = false;
        for ch in s.chars() {
            let up = ch.to_ascii_uppercase();
            if up == ' ' || up == '.' {
                lossy = true;
            } else if up.is_ascii_alphanumeric() || "!#$%&'()-@^_`{}~".contains(up) {
                out.push(up);
            } else {
                out.push('_');
                lossy = true;
            }
        }
        (out, lossy)
    };
    let (base, lossy_base) = clean(base);
    let (ext, lossy_ext) = clean(ext);
    if base.is_empty() {
        return Err(FatError::InvalidName(format!(
            "{name:?} has no usable name"
        )));
    }
    let lossy_ext = lossy_ext || ext.len() > 3;
    let ext: String = ext.chars().take(3).collect();
    let dotted = |b: &str| {
        if ext.is_empty() {
            b.to_string()
        } else {
            format!("{b}.{ext}")
        }
    };
    if base.len() <= 8 && !lossy_base && !lossy_ext {
        return Ok(dotted(&base));
    }
    for n in 1..=999_999u32 {
        let tail = format!("~{n}");
        let keep = 8 - tail.len();
        let candidate = dotted(&format!("{}{tail}", &base[..base.len().min(keep)]));
        if !taken.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(FatError::Full("no free short name".into()))
}

fn volume_label(label: &str) -> Option<[u8; 11]> {
    let clean: Vec<u8> = label
        .chars()
        .map(|c| c.to_ascii_uppercase())
        .filter(|c| c.is_ascii_alphanumeric() || " -_".contains(*c))
        .take(11)
        .map(|c| c as u8)
        .collect();
    if clean.iter().all(|&c| c == b' ') {
        return None;
    }
    let mut out = [b' '; 11];
    out[..clean.len()].copy_from_slice(&clean);
    Some(out)
}

fn cluster_offset(c: u16) -> usize {
    (DATA_START + usize::from(c) - 2) * SECTOR
}

fn get16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes([b[at], b[at + 1]])
}

fn put16(b: &mut [u8], at: usize, v: u16) {
    b[at..at + 2].copy_from_slice(&v.to_le_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_blank_disk_has_the_floppy_layout_and_all_space_free() {
        let d = FloppyImage::blank("Designs");
        assert_eq!(d.bytes().len(), IMAGE_SIZE);
        assert_eq!(d.free_bytes(), 2_847 * 512);
        assert_eq!(d.capacity(), 2_847 * 512);
        assert_eq!(d.label().as_deref(), Some("DESIGNS"));
        assert!(d.entries().is_empty());
        assert_eq!(FloppyImage::parse(d.bytes()), Ok(d));
    }

    #[test]
    fn files_are_added_read_replaced_and_removed() {
        let mut d = FloppyImage::blank("");
        let a: Vec<u8> = (0..1_300u32).map(|i| i as u8).collect();
        assert_eq!(d.add("rose.dst", &a, false), Ok("ROSE.DST".into()));
        assert_eq!(d.read("rose.dst"), Ok(a.clone()));
        assert_eq!(d.free_bytes(), (2_847 - 3) * 512);
        assert_eq!(
            d.add("ROSE.DST", b"x", false),
            Err(FatError::Exists("ROSE.DST".into()))
        );
        assert_eq!(d.add("ROSE.DST", b"x", true), Ok("ROSE.DST".into()));
        assert_eq!(d.read("ROSE.DST"), Ok(b"x".to_vec()));
        assert_eq!(d.free_bytes(), (2_847 - 1) * 512);
        d.remove("rose.dst").expect("remove");
        assert!(d.entries().is_empty());
        assert_eq!(d.free_bytes(), 2_847 * 512);
        assert_eq!(
            d.remove("rose.dst"),
            Err(FatError::NotFound("rose.dst".into()))
        );
        let back = FloppyImage::parse(d.bytes()).expect("still valid");
        assert_eq!(back, d);
    }

    #[test]
    fn a_zero_byte_file_takes_no_cluster() {
        let mut d = FloppyImage::blank("");
        d.add("EMPTY.EXP", &[], false).expect("add");
        let e = &d.entries()[0];
        assert_eq!((e.size, e.first_cluster), (0, 0));
        assert_eq!(d.read("EMPTY.EXP"), Ok(Vec::new()));
        FloppyImage::parse(d.bytes()).expect("valid");
    }

    #[test]
    fn long_names_get_clear_unique_short_names() {
        let t = |s: &[&str]| s.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(short_name("rose.dst", &[]), Ok("ROSE.DST".into()));
        assert_eq!(short_name("My Design.dst", &[]), Ok("MYDESI~1.DST".into()));
        assert_eq!(
            short_name("My Design.dst", &t(&["MYDESI~1.DST"])),
            Ok("MYDESI~2.DST".into())
        );
        assert_eq!(
            short_name("butterfly-large.exp", &[]),
            Ok("BUTTER~1.EXP".into())
        );
        assert_eq!(short_name("a+b.u01", &[]), Ok("A_B~1.U01".into()));
        assert_eq!(short_name("dir/sub\\README", &[]), Ok("README".into()));
        assert_eq!(short_name("design.json", &[]), Ok("DESIGN~1.JSO".into()));
        assert!(short_name("...", &[]).is_err());
        // Ten taken names move the tail to two digits and the base to five characters.
        let many: Vec<String> = (1..=9).map(|n| format!("MYDESI~{n}.DST")).collect();
        assert_eq!(
            short_name("My Design.dst", &many),
            Ok("MYDES~10.DST".into())
        );
    }

    #[test]
    fn a_full_disk_refuses_and_stays_as_it_was() {
        let mut d = FloppyImage::blank("");
        let big = vec![7u8; 2_847 * 512 - 512];
        d.add("BIG.DST", &big, false).expect("fits");
        let before = d.clone();
        let e = d.add("TWO.DST", &[1u8; 1_025], false).expect_err("full");
        assert!(matches!(e, FatError::Full(_)), "{e}");
        assert_eq!(d, before);
        d.add("ONE.DST", &[1u8; 512], false)
            .expect("the last cluster");
        assert_eq!(d.free_bytes(), 0);
        FloppyImage::parse(d.bytes()).expect("full image valid");
    }

    #[test]
    fn the_root_directory_holds_224_entries() {
        let mut d = FloppyImage::blank("");
        for n in 0..224 {
            d.add(&format!("F{n}.DST"), &[], false).expect("entry");
        }
        let e = d.add("F224.DST", &[], false).expect_err("no slot");
        assert!(matches!(e, FatError::Full(_)), "{e}");
    }

    fn damaged(f: impl FnOnce(&mut Vec<u8>)) -> FatError {
        let mut d = FloppyImage::blank("");
        d.add("A.DST", &[1u8; 1_200], false).expect("add");
        d.add("B.DST", &[2u8; 600], false).expect("add");
        let mut b = d.into_bytes();
        f(&mut b);
        FloppyImage::parse(&b).expect_err("damaged")
    }

    #[test]
    fn hostile_and_damaged_images_fail_cleanly() {
        let set = |b: &mut Vec<u8>, n: u16, v: u16| {
            let mut d = FloppyImage {
                bytes: std::mem::take(b),
            };
            d.set_fat(n, v);
            *b = d.bytes;
        };
        // A chain that loops back on itself.
        assert!(damaged(|b| set(b, 4, 2)).to_string().contains("loops"));
        // A chain that leaves the disk.
        assert!(damaged(|b| set(b, 3, 3_000)).to_string().contains("leaves"));
        // A chain that runs into a free cluster.
        assert!(damaged(|b| set(b, 3, 0x300)).to_string().contains("free"));
        // A bad-cluster mark inside a chain.
        assert!(damaged(|b| set(b, 2, 0xFF7)).to_string().contains("bad"));
        // A size that does not match the chain.
        let e = damaged(|b| {
            let at = ROOT_START * SECTOR + 28;
            b[at..at + 4].copy_from_slice(&5_000u32.to_le_bytes());
        });
        assert!(e.to_string().contains("chain holds"), "{e}");
        // Two files sharing a cluster.
        let e = damaged(|b| {
            let at = ROOT_START * SECTOR + 32 + 26;
            b[at..at + 2].copy_from_slice(&3u16.to_le_bytes());
            let size = ROOT_START * SECTOR + 32 + 28;
            b[size..size + 4].copy_from_slice(&512u32.to_le_bytes());
        });
        assert!(
            e.to_string().contains("shares") || e.to_string().contains("chain holds"),
            "{e}"
        );
        // A first cluster of 1, outside the data area.
        let e = damaged(|b| {
            let at = ROOT_START * SECTOR + 26;
            b[at..at + 2].copy_from_slice(&1u16.to_le_bytes());
        });
        assert!(e.to_string().contains("leaves"), "{e}");
        // A directory entry whose first cluster points past the end of the disk.
        let e = damaged(|b| {
            let at = ROOT_START * SECTOR + 26;
            b[at..at + 2].copy_from_slice(&0xFFF0u16.to_le_bytes());
        });
        assert!(e.to_string().contains("leaves"), "{e}");
        // Not a floppy at all, and a truncated one.
        assert!(matches!(
            FloppyImage::parse(&[0u8; 1_000]),
            Err(FatError::NotAFloppy(_))
        ));
        let whole = FloppyImage::blank("").into_bytes();
        for cut in [0, 511, 512 * 19, IMAGE_SIZE - 1] {
            assert!(matches!(
                FloppyImage::parse(&whole[..cut]),
                Err(FatError::NotAFloppy(_))
            ));
        }
        // A boot sector with impossible sizes.
        for (at, value) in [(13usize, 4u8), (12, 0), (16, 0), (17, 0xFF), (22, 200)] {
            let mut b = whole.clone();
            b[at] = value;
            assert!(
                matches!(FloppyImage::parse(&b), Err(FatError::NotAFloppy(_))),
                "byte {at} = {value}"
            );
        }
        // Random bytes of the right size never panic.
        let mut x: u32 = 1;
        let noise: Vec<u8> = (0..IMAGE_SIZE)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                x as u8
            })
            .collect();
        assert!(FloppyImage::parse(&noise).is_err());
    }
}
