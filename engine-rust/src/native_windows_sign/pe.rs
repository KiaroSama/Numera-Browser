//! Locate a nonstructural signed-image byte for the real digest control.
use std::{
    fs,
    io::{self, Read, Seek, SeekFrom},
    path::Path,
};

fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "Invalid signed PE digest-control layout",
    )
}
fn u16_at(bytes: &[u8], offset: usize) -> io::Result<u16> {
    Ok(u16::from_le_bytes(
        bytes
            .get(offset..offset + 2)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn u32_at(bytes: &[u8], offset: usize) -> io::Result<u32> {
    Ok(u32::from_le_bytes(
        bytes
            .get(offset..offset + 4)
            .ok_or_else(invalid)?
            .try_into()
            .map_err(|_| invalid())?,
    ))
}
fn read_at(file: &mut fs::File, length: u64, offset: u64, bytes: &mut [u8]) -> io::Result<()> {
    if offset
        .checked_add(bytes.len() as u64)
        .is_none_or(|end| end > length)
    {
        return Err(invalid());
    }
    file.seek(SeekFrom::Start(offset))?;
    file.read_exact(bytes)
}

pub(super) fn executable_byte(path: &Path) -> io::Result<u64> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(invalid());
    }
    let mut file = fs::File::open(path)?;
    let length = file.metadata()?.len();
    let mut dos = [0; 64];
    read_at(&mut file, length, 0, &mut dos)?;
    if &dos[..2] != b"MZ" {
        return Err(invalid());
    }
    let pe = u64::from(u32_at(&dos, 0x3c)?);
    if pe < 64 {
        return Err(invalid());
    }
    let mut coff = [0; 24];
    read_at(&mut file, length, pe, &mut coff)?;
    let sections = usize::from(u16_at(&coff, 6)?);
    let optional_size = usize::from(u16_at(&coff, 20)?);
    if &coff[..4] != b"PE\0\0"
        || !(1..=96).contains(&sections)
        || !(2..=4096).contains(&optional_size)
        || u16_at(&coff, 22)? & 2 == 0
    {
        return Err(invalid());
    }
    let optional_start = pe.checked_add(24).ok_or_else(invalid)?;
    let mut optional = vec![0; optional_size];
    read_at(&mut file, length, optional_start, &mut optional)?;
    let directories = match u16_at(&optional, 0)? {
        0x20b => 112,
        0x10b => 96,
        _ => return Err(invalid()),
    };
    let count = u64::from(u32_at(&optional, directories - 4)?);
    if count < 5
        || count
            .checked_mul(8)
            .and_then(|n| n.checked_add(directories as u64))
            .is_none_or(|end| end > optional_size as u64)
    {
        return Err(invalid());
    }
    let headers = u64::from(u32_at(&optional, 60)?);
    let certificate = u64::from(u32_at(&optional, directories + 32)?);
    let certificate_size = u64::from(u32_at(&optional, directories + 36)?);
    let table = optional_start
        .checked_add(optional_size as u64)
        .ok_or_else(invalid)?;
    let table_size = sections.checked_mul(40).ok_or_else(invalid)?;
    if table
        .checked_add(table_size as u64)
        .is_none_or(|end| end > headers)
        || headers > length
        || certificate < headers
        || !certificate.is_multiple_of(8)
        || !(8..=1024 * 1024).contains(&certificate_size)
        || certificate
            .checked_add(certificate_size)
            .is_none_or(|end| end > length)
    {
        return Err(invalid());
    }
    let mut entries = vec![0; table_size];
    read_at(&mut file, length, table, &mut entries)?;
    let mut ranges = Vec::new();
    for section in entries.as_chunks::<40>().0 {
        let size = u64::from(u32_at(section, 16)?);
        if size == 0 {
            continue;
        }
        let start = u64::from(u32_at(section, 20)?);
        let end = start.checked_add(size).ok_or_else(invalid)?;
        // Authenticode hashes raw image sections. Header/checksum/directory
        // bytes and the attribute certificate table are never control targets.
        if start < headers || end > certificate || end > length {
            return Err(invalid());
        }
        let flags = u32_at(section, 36)?;
        ranges.push((start, end, flags & 0x20000020 == 0x20000020));
    }
    ranges.sort_unstable();
    if ranges.windows(2).any(|pair| pair[0].1 > pair[1].0) {
        return Err(invalid());
    }
    ranges
        .into_iter()
        .find(|range| range.2)
        .map(|range| range.0)
        .ok_or_else(invalid)
}
