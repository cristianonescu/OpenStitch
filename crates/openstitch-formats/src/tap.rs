//! Tajima TAP (#481): DST's 3-byte records with no header. Read and written exactly as
//! [`crate::dst`] reads and writes the records after its 512-byte header (Roidery's notes,
//! MIT OR Apache-2.0; checked against pyembroidery 1.5.1). No colours. Writing is
//! experimental: no reference file from Tajima software and no sew-out yet.

use crate::{Result, dst};
use openstitch_core::StitchPattern;

pub fn read(data: &[u8]) -> Result<StitchPattern> {
    let mut file = vec![0x20; dst::HEADER_SIZE];
    file.extend_from_slice(data);
    let mut pattern = dst::read(&file)?;
    pattern.metadata.retain(|k, _| !k.starts_with("dst."));
    Ok(pattern)
}

pub fn write(pattern: &StitchPattern) -> Result<Vec<u8>> {
    let mut bytes = dst::write(pattern)?;
    bytes.drain(..dst::HEADER_SIZE);
    Ok(bytes)
}
