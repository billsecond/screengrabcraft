//! A zip file of stored (uncompressed) entries, as office documents are packed (a word-processor
//! document, slides), and reading one back. Pictures in them are PNGs already, and the words are
//! small, so nothing is compressed. No zip64: an entry or the whole over 4 GB, or more than 65 535
//! entries, is an error.

use crate::codec::CodecError;

/// Builds a zip file in memory, an entry at a time.
#[derive(Default)]
pub struct ZipWriter {
    out: Vec<u8>,
    /// Each entry's name, CRC, size and where its header is.
    entries: Vec<(String, u32, u32, u32)>,
}

/// 1 January 1980, midnight, the earliest a zip file can say (the date isn't the point here).
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;
/// Version 2.0: what stored entries need.
const VERSION: u16 = 20;
/// The names are UTF-8.
const UTF8: u16 = 1 << 11;

fn too_big() -> CodecError {
    CodecError::Write("the file is too large to pack".to_string())
}

impl ZipWriter {
    pub fn new() -> ZipWriter {
        ZipWriter::default()
    }

    /// Adds an entry: its name in the zip (`word/document.xml`) and what's in it.
    pub fn add(&mut self, name: &str, data: &[u8]) -> Result<(), CodecError> {
        if self.entries.len() >= usize::from(u16::MAX) {
            return Err(CodecError::Write("too many files to pack".to_string()));
        }
        let size = u32::try_from(data.len()).map_err(|_| too_big())?;
        let at = u32::try_from(self.out.len()).map_err(|_| too_big())?;
        let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
        let crc = crc32fast::hash(data);
        let o = &mut self.out;
        o.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        for v in [VERSION, UTF8, 0, DOS_TIME, DOS_DATE] {
            o.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc, size, size] {
            o.extend_from_slice(&v.to_le_bytes());
        }
        o.extend_from_slice(&name_len.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        o.extend_from_slice(name.as_bytes());
        o.extend_from_slice(data);
        u32::try_from(self.out.len()).map_err(|_| too_big())?;
        self.entries.push((name.to_string(), crc, size, at));
        Ok(())
    }

    /// The zip file: the entries, then their directory.
    pub fn finish(mut self) -> Result<Vec<u8>, CodecError> {
        let start = u32::try_from(self.out.len()).map_err(|_| too_big())?;
        let count = u16::try_from(self.entries.len()).map_err(|_| too_big())?;
        for (name, crc, size, at) in &self.entries {
            let o = &mut self.out;
            o.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
            for v in [VERSION, VERSION, UTF8, 0, DOS_TIME, DOS_DATE] {
                o.extend_from_slice(&v.to_le_bytes());
            }
            for v in [*crc, *size, *size] {
                o.extend_from_slice(&v.to_le_bytes());
            }
            let name_len = u16::try_from(name.len()).map_err(|_| too_big())?;
            for v in [name_len, 0, 0, 0, 0] {
                o.extend_from_slice(&v.to_le_bytes());
            }
            o.extend_from_slice(&0u32.to_le_bytes());
            o.extend_from_slice(&at.to_le_bytes());
            o.extend_from_slice(name.as_bytes());
        }
        let size = u32::try_from(self.out.len()).map_err(|_| too_big())?.saturating_sub(start);
        let o = &mut self.out;
        o.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
        for v in [0u16, 0, count, count] {
            o.extend_from_slice(&v.to_le_bytes());
        }
        o.extend_from_slice(&size.to_le_bytes());
        o.extend_from_slice(&start.to_le_bytes());
        o.extend_from_slice(&0u16.to_le_bytes());
        u32::try_from(self.out.len()).map_err(|_| too_big())?;
        Ok(self.out)
    }
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(b.get(at..at.checked_add(2)?)?.try_into().ok()?))
}

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at.checked_add(4)?)?.try_into().ok()?))
}

/// The entries of a zip file of stored entries, by its directory: each one's name and what's in
/// it, checked against its CRC. None when it isn't one (or an entry is compressed or damaged).
pub fn entries(zip: &[u8]) -> Option<Vec<(String, Vec<u8>)>> {
    // The directory's end: the last 22 bytes when there's no comment.
    let end = zip.len().checked_sub(22)?;
    if u32_at(zip, end)? != 0x0605_4b50 {
        return None;
    }
    let count = usize::from(u16_at(zip, end + 10)?);
    let mut at = usize::try_from(u32_at(zip, end + 16)?).ok()?;
    let mut out = Vec::with_capacity(count.min(4096));
    for _ in 0..count {
        if u32_at(zip, at)? != 0x0201_4b50 || u16_at(zip, at + 10)? != 0 {
            return None;
        }
        let crc = u32_at(zip, at + 16)?;
        let size = usize::try_from(u32_at(zip, at + 20)?).ok()?;
        let (name_len, extra, comment) = (usize::from(u16_at(zip, at + 28)?), usize::from(u16_at(zip, at + 30)?), usize::from(u16_at(zip, at + 32)?));
        let local = usize::try_from(u32_at(zip, at + 42)?).ok()?;
        let name = String::from_utf8(zip.get(at + 46..(at + 46).checked_add(name_len)?)?.to_vec()).ok()?;
        // The entry itself, after its own header.
        if u32_at(zip, local)? != 0x0403_4b50 {
            return None;
        }
        let from = local.checked_add(30)?.checked_add(usize::from(u16_at(zip, local + 26)?))?.checked_add(usize::from(u16_at(zip, local + 28)?))?;
        let data = zip.get(from..from.checked_add(size)?)?.to_vec();
        if crc32fast::hash(&data) != crc {
            return None;
        }
        out.push((name, data));
        at = at.checked_add(46)?.checked_add(name_len)?.checked_add(extra)?.checked_add(comment)?;
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_are_packed_and_read_back() {
        let mut z = ZipWriter::new();
        z.add("[Content_Types].xml", b"<Types/>").unwrap();
        z.add("word/media/step 1.png", &[0u8, 1, 2, 255]).unwrap();
        z.add("empty", b"").unwrap();
        let bytes = z.finish().unwrap();
        assert_eq!(bytes.get(..4), Some(&b"PK\x03\x04"[..]));
        let back = entries(&bytes).unwrap();
        assert_eq!(back.iter().map(|(n, _)| n.as_str()).collect::<Vec<_>>(), ["[Content_Types].xml", "word/media/step 1.png", "empty"]);
        assert_eq!(back[1].1, vec![0u8, 1, 2, 255]);
        // An empty zip is a zip; anything damaged is none, never a panic.
        assert_eq!(entries(&ZipWriter::new().finish().unwrap()), Some(Vec::new()));
        let mut bad = bytes.clone();
        // A byte of the first entry's words changed: its CRC no longer matches.
        if let Some(b) = bad.get_mut(30 + 19 + 2) {
            *b ^= 0xFF;
        }
        assert!(entries(&bad).is_none());
        for cut in [0, 3, 22, bytes.len() / 2, bytes.len() - 1] {
            let _ = entries(bytes.get(..cut).unwrap_or_default());
        }
        assert!(entries(b"PK\x05\x06").is_none());
    }
}
