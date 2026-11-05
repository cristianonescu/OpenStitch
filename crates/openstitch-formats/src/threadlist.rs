//! Thread list files (#482): a list of threads in order, no stitches. Used for a design's
//! colours in sewing order (an INF beside an EXP for Bernina machines; a list someone sent,
//! applied to a design's colour blocks) and for user thread charts.
//!
//! | Format | Layout |
//! |---|---|
//! | INF | Big-endian: u32 1, u32 8, u32 length of what follows, u32 count; per thread u16 record length (itself included), u16 index, r g b, u16 number (index + 1), the name and a chart string, each ending in `00` |
//! | EDR | `r g b 00` per thread |
//! | COL | Text, CR LF lines: the count, then `index,r,g,b` per thread |
//! | TXT | Text: one thread per line, `#rrggbb` then tab-separated catalogue number, name and brand; `#` lines without a colour are comments. The reader also takes `r,g,b` or `r g b` and a hex colour anywhere in the line, so most hand-made lists read |
//!
//! INF, EDR and COL are laid out as pyembroidery 1.5.1 writes and reads them (its files read,
//! no code copied); it is the test oracle. What INF's chart string means is not documented:
//! the writer puts the brand there (`Unknown` without one, as pyembroidery writes it) and the
//! reader takes it back as the brand. TXT has no common layout; this one is OpenStitch's.

use crate::{FormatError, Result};
use openstitch_core::Thread;
use std::borrow::Cow;

/// The thread of every colour block as a machine file without colours sews them: a colour
/// change starts the next thread, a stop repeats the current one (EXP and DST write both as a
/// colour change). This is the list an INF beside an EXP holds.
pub fn sewing_order(pattern: &openstitch_core::StitchPattern) -> Vec<Thread> {
    crate::repeating_stops_palette(pattern)
}

/// Most threads a list may hold.
pub const MAX_THREADS: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ListType {
    Inf,
    Edr,
    Col,
    Txt,
}

impl ListType {
    pub const ALL: [ListType; 4] = [Self::Inf, Self::Edr, Self::Col, Self::Txt];

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Inf => "inf",
            Self::Edr => "edr",
            Self::Col => "col",
            Self::Txt => "txt",
        }
    }

    pub fn from_extension(ext: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|t| t.extension().eq_ignore_ascii_case(ext))
    }

    pub fn read(self, data: &[u8]) -> Result<Vec<Thread>> {
        let threads = match self {
            Self::Inf => read_inf(data)?,
            Self::Edr => read_edr(data)?,
            Self::Col => read_col(data)?,
            Self::Txt => read_txt(data)?,
        };
        if threads.len() > MAX_THREADS {
            return Err(FormatError::ValueOutOfRange(format!(
                "{} threads (at most {MAX_THREADS})",
                threads.len()
            )));
        }
        Ok(threads)
    }

    pub fn write(self, threads: &[Thread]) -> Result<Vec<u8>> {
        if threads.len() > MAX_THREADS {
            return Err(FormatError::ValueOutOfRange(format!(
                "{} threads (at most {MAX_THREADS})",
                threads.len()
            )));
        }
        Ok(match self {
            Self::Inf => write_inf(threads)?,
            Self::Edr => write_edr(threads),
            Self::Col => write_col(threads),
            Self::Txt => write_txt(threads),
        })
    }
}

fn text(s: &str) -> Option<Cow<'static, str>> {
    let s = s.trim();
    (!s.is_empty()).then(|| Cow::Owned(s.to_owned()))
}

// MARK: INF

fn be32(d: &[u8], at: usize) -> Result<usize> {
    d.get(at..at + 4)
        .map(|b| u32::from_be_bytes([b[0], b[1], b[2], b[3]]) as usize)
        .ok_or_else(|| FormatError::Truncated(format!("INF: need 4 bytes at {at}")))
}

fn read_inf(d: &[u8]) -> Result<Vec<Thread>> {
    let count = be32(d, 12)?;
    if count > MAX_THREADS {
        return Err(FormatError::ValueOutOfRange(format!(
            "INF: {count} threads"
        )));
    }
    let mut at = 16;
    let mut out = Vec::with_capacity(count);
    for _ in 0..count {
        let len = d
            .get(at..at + 2)
            .map(|b| usize::from(u16::from_be_bytes([b[0], b[1]])))
            .ok_or_else(|| FormatError::Truncated(format!("INF: record at {at}")))?;
        let rec = d
            .get(at..at + len)
            .filter(|r| r.len() >= 9)
            .ok_or_else(|| FormatError::Truncated(format!("INF: {len}-byte record at {at}")))?;
        let mut t = Thread::rgb(u32::from_be_bytes([0, rec[4], rec[5], rec[6]]));
        let mut strings = rec[9..]
            .split(|&b| b == 0)
            .map(|s| String::from_utf8_lossy(s).into_owned());
        t.name = strings.next().and_then(|s| text(&s));
        t.brand = strings
            .next()
            .filter(|s| s.trim() != "Unknown")
            .and_then(|s| text(&s));
        out.push(t);
        at += len;
    }
    Ok(out)
}

fn write_inf(threads: &[Thread]) -> Result<Vec<u8>> {
    let mut records = Vec::new();
    for (i, t) in threads.iter().enumerate() {
        let clean = |s: &str| s.replace('\0', "");
        let name = clean(t.name.as_deref().unwrap_or(""));
        let chart = clean(t.brand.as_deref().unwrap_or("Unknown"));
        let len = 2 + 2 + 3 + 2 + name.len() + 1 + chart.len() + 1;
        let len = u16::try_from(len).map_err(|_| {
            FormatError::ValueOutOfRange(format!("INF: thread {i}'s strings are too long"))
        })?;
        records.extend(len.to_be_bytes());
        records.extend((i as u16).to_be_bytes());
        records.extend([t.red, t.green, t.blue]);
        records.extend(((i + 1) as u16).to_be_bytes());
        records.extend(name.as_bytes());
        records.push(0);
        records.extend(chart.as_bytes());
        records.push(0);
    }
    let mut out = Vec::with_capacity(records.len() + 16);
    out.extend(1_u32.to_be_bytes());
    out.extend(8_u32.to_be_bytes());
    out.extend(((records.len() + 4) as u32).to_be_bytes());
    out.extend((threads.len() as u32).to_be_bytes());
    out.extend(records);
    Ok(out)
}

// MARK: EDR

fn read_edr(d: &[u8]) -> Result<Vec<Thread>> {
    if !d.len().is_multiple_of(4) {
        return Err(FormatError::Truncated(format!(
            "EDR: {} bytes is not a whole number of 4-byte colours",
            d.len()
        )));
    }
    Ok(d.as_chunks::<4>()
        .0
        .iter()
        .map(|c| Thread::rgb(u32::from_be_bytes([0, c[0], c[1], c[2]])))
        .collect())
}

fn write_edr(threads: &[Thread]) -> Vec<u8> {
    threads
        .iter()
        .flat_map(|t| [t.red, t.green, t.blue, 0])
        .collect()
}

// MARK: COL

fn read_col(d: &[u8]) -> Result<Vec<Thread>> {
    let s = String::from_utf8_lossy(d);
    let mut lines = s.lines().map(str::trim).filter(|l| !l.is_empty());
    let count: usize = lines
        .next()
        .and_then(|l| l.parse().ok())
        .ok_or_else(|| FormatError::BadSignature("COL: no colour count".into()))?;
    let mut out = Vec::new();
    for line in lines.take(count.min(MAX_THREADS)) {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        let rgb: Option<Vec<u8>> = f
            .get(1..4)
            .map(|c| c.iter().filter_map(|v| v.parse().ok()).collect());
        match rgb.as_deref() {
            Some(&[r, g, b]) => out.push(Thread::rgb(u32::from_be_bytes([0, r, g, b]))),
            _ => return Err(FormatError::ValueOutOfRange(format!("COL line '{line}'"))),
        }
    }
    if out.len() != count {
        return Err(FormatError::Truncated(format!(
            "COL: {count} colours announced, {} found",
            out.len()
        )));
    }
    Ok(out)
}

fn write_col(threads: &[Thread]) -> Vec<u8> {
    let mut s = format!("{}\r\n", threads.len());
    for (i, t) in threads.iter().enumerate() {
        s.push_str(&format!("{i},{},{},{}\r\n", t.red, t.green, t.blue));
    }
    s.into_bytes()
}

// MARK: TXT

/// A colour in a free-form line: `#rrggbb` anywhere, else the first three integers 0–255.
fn txt_colour(line: &str) -> Option<(Thread, String)> {
    if let Some(i) = line.find('#') {
        let hex = line.get(i + 1..i + 7)?;
        if hex.chars().all(|c| c.is_ascii_hexdigit()) {
            let t = Thread::from_hex(hex)?;
            let rest = format!("{}{}", &line[..i], &line[i + 7..]);
            return Some((t, rest));
        }
        return None;
    }
    let nums: Vec<u8> = line
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter_map(|v| v.parse().ok())
        .take(3)
        .collect();
    (nums.len() == 3).then(|| {
        (
            Thread::rgb(u32::from_be_bytes([0, nums[0], nums[1], nums[2]])),
            String::new(),
        )
    })
}

fn read_txt(d: &[u8]) -> Result<Vec<Thread>> {
    let s = String::from_utf8_lossy(d);
    let mut out = Vec::new();
    for line in s.lines().map(|l| l.trim_start_matches('\u{feff}').trim()) {
        if line.is_empty() {
            continue;
        }
        match txt_colour(line) {
            Some((mut t, rest)) => {
                // OpenStitch's own lines: the colour, then a tab before each field (empty
                // fields kept, so the columns stay in place).
                let mut f = rest.split('\t').skip(1).map(str::trim);
                if line.starts_with('#') {
                    t.catalog_number = f.next().and_then(text);
                    t.name = f.next().and_then(text);
                    t.brand = f.next().and_then(text);
                }
                out.push(t);
            }
            None if line.starts_with('#') => {}
            None => {
                return Err(FormatError::ValueOutOfRange(format!(
                    "TXT: no colour in '{line}'"
                )));
            }
        }
        if out.len() > MAX_THREADS {
            break;
        }
    }
    Ok(out)
}

fn write_txt(threads: &[Thread]) -> Vec<u8> {
    let field = |s: Option<&str>| s.unwrap_or("").replace(['\t', '\r', '\n'], " ");
    let mut s = String::from("# OpenStitch thread list: colour, catalogue number, name, brand\n");
    for t in threads {
        s.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            t.hex(),
            field(t.catalog_number.as_deref()),
            field(t.name.as_deref()),
            field(t.brand.as_deref())
        ));
    }
    s.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn threads() -> Vec<Thread> {
        let mut a = Thread::rgb(0xE01010).with_name("Poppy");
        a.catalog_number = Some("1001".into());
        a.brand = Some("Acme".into());
        vec![a, Thread::rgb(0x1040C0).with_name("Ocean"), Thread::rgb(0)]
    }

    #[test]
    fn every_list_round_trips_its_colours_in_order() {
        for ty in ListType::ALL {
            let back = ty
                .read(&ty.write(&threads()).expect("write"))
                .expect("read");
            let hexes: Vec<String> = back.iter().map(Thread::hex).collect();
            assert_eq!(hexes, ["#e01010", "#1040c0", "#000000"], "{ty:?}");
        }
    }

    #[test]
    fn inf_and_txt_keep_names_and_txt_keeps_every_string() {
        let inf = ListType::Inf
            .read(&ListType::Inf.write(&threads()).expect("write"))
            .expect("read");
        assert_eq!(inf[0].name.as_deref(), Some("Poppy"));
        assert_eq!(inf[0].brand.as_deref(), Some("Acme"));
        assert_eq!(inf[1].brand, None);
        let txt = ListType::Txt
            .read(&ListType::Txt.write(&threads()).expect("write"))
            .expect("read");
        assert_eq!(txt, threads());
    }

    #[test]
    fn txt_reads_hand_made_lists() {
        let back = ListType::Txt
            .read(b"# my list\n255, 0, 0\nThread 2: #00ff00 Green\n0 0 255\n")
            .expect("read");
        let hexes: Vec<String> = back.iter().map(Thread::hex).collect();
        assert_eq!(hexes, ["#ff0000", "#00ff00", "#0000ff"]);
        assert!(ListType::Txt.read(b"no colour here").is_err());
    }

    #[test]
    fn damaged_lists_are_errors_not_panics() {
        for ty in ListType::ALL {
            let good = ty.write(&threads()).expect("read");
            for n in 0..good.len() {
                let _ = ty.read(&good[..n]);
            }
        }
        assert!(ListType::Edr.read(&[1, 2, 3]).is_err());
        assert!(ListType::Col.read(b"3\r\n0,1,2,3\r\n").is_err());
        let mut huge = ListType::Inf.write(&threads()).expect("read");
        huge[12..16].copy_from_slice(&u32::MAX.to_be_bytes());
        assert!(ListType::Inf.read(&huge).is_err());
    }
}
