//! Writing see-through videos: a QuickTime movie (`.mov`, the QuickTime File Format) whose
//! pictures are PNG files, each with its own see-through parts (32 bits a pixel), one video track
//! and no sound. Video editors put such a movie over another video as it is.
//!
//! The pictures go into one `mdat` as they come (its size written as 64 bits, so it can pass
//! 4 GB), and the index (`moov`) is written at the end: one picture a chunk, each lasting one
//! tick of a clock that ticks once a picture.

use std::io::{self, Seek, SeekFrom, Write};

/// The movie's own clock: milliseconds.
const MOVIE_TIMESCALE: u32 = 1000;

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 8);
    b.extend_from_slice(&u32::try_from(body.len() + 8).unwrap_or(u32::MAX).to_be_bytes());
    b.extend_from_slice(kind);
    b.extend_from_slice(body);
    b
}

fn full(kind: &[u8; 4], version: u8, flags: u32, body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 4);
    b.push(version);
    b.extend_from_slice(&flags.to_be_bytes()[1..]);
    b.extend_from_slice(body);
    bx(kind, &b)
}

fn matrix(b: &mut Vec<u8>) {
    for v in [0x0001_0000u32, 0, 0, 0, 0x0001_0000, 0, 0, 0, 0x4000_0000] {
        b.extend_from_slice(&v.to_be_bytes());
    }
}

/// A movie of PNG pictures being written to `out`.
pub struct MovWriter<W: Write + Seek> {
    out: W,
    width: u32,
    height: u32,
    fps: u32,
    /// Where the `mdat` box starts, and where the next picture goes.
    mdat_at: u64,
    pos: u64,
    /// Each picture's size and where it is.
    sizes: Vec<u32>,
    offsets: Vec<u64>,
}

impl<W: Write + Seek> MovWriter<W> {
    /// A movie `width × height`, `fps` pictures a second.
    pub fn new(mut out: W, width: u32, height: u32, fps: u32) -> io::Result<MovWriter<W>> {
        let mut ftyp = Vec::new();
        ftyp.extend_from_slice(b"qt  ");
        ftyp.extend_from_slice(&0x2005_0300u32.to_be_bytes());
        ftyp.extend_from_slice(b"qt  ");
        let ftyp = bx(b"ftyp", &ftyp);
        out.write_all(&ftyp)?;
        let mdat_at = ftyp.len() as u64;
        // A 64-bit size, filled in at the end.
        let mut head = Vec::with_capacity(16);
        head.extend_from_slice(&1u32.to_be_bytes());
        head.extend_from_slice(b"mdat");
        head.extend_from_slice(&0u64.to_be_bytes());
        out.write_all(&head)?;
        Ok(MovWriter {
            out,
            width: width.clamp(1, 65_535),
            height: height.clamp(1, 65_535),
            fps: fps.clamp(1, 240),
            mdat_at,
            pos: mdat_at + 16,
            sizes: Vec::new(),
            offsets: Vec::new(),
        })
    }

    /// Adds a picture: a PNG file's bytes.
    pub fn push(&mut self, png: &[u8]) -> io::Result<()> {
        let size = u32::try_from(png.len()).map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "a picture is too large for a movie"))?;
        self.out.write_all(png)?;
        self.offsets.push(self.pos);
        self.sizes.push(size);
        self.pos += u64::from(size);
        Ok(())
    }

    /// How many pictures it has.
    pub fn frames(&self) -> usize {
        self.sizes.len()
    }

    /// Writes the index and hands the file back.
    pub fn finish(mut self) -> io::Result<W> {
        let end = self.pos;
        self.out.seek(SeekFrom::Start(self.mdat_at + 8))?;
        self.out.write_all(&(end - self.mdat_at).to_be_bytes())?;
        self.out.seek(SeekFrom::Start(end))?;
        let moov = self.moov();
        self.out.write_all(&moov)?;
        self.out.flush()?;
        Ok(self.out)
    }

    fn moov(&self) -> Vec<u8> {
        let n = u32::try_from(self.sizes.len()).unwrap_or(u32::MAX);
        let duration = (u64::from(n) * u64::from(MOVIE_TIMESCALE) / u64::from(self.fps)).min(u64::from(u32::MAX)) as u32;
        let mut mvhd = Vec::new();
        for v in [0u32, 0, MOVIE_TIMESCALE, duration, 0x0001_0000] {
            mvhd.extend_from_slice(&v.to_be_bytes());
        }
        mvhd.extend_from_slice(&0x0100u16.to_be_bytes());
        mvhd.extend_from_slice(&[0; 10]);
        matrix(&mut mvhd);
        mvhd.extend_from_slice(&[0; 24]);
        mvhd.extend_from_slice(&2u32.to_be_bytes());
        let mvhd = full(b"mvhd", 0, 0, &mvhd);

        let mut tkhd = Vec::new();
        for v in [0u32, 0, 1, 0, duration, 0, 0] {
            tkhd.extend_from_slice(&v.to_be_bytes());
        }
        tkhd.extend_from_slice(&[0; 8]);
        matrix(&mut tkhd);
        tkhd.extend_from_slice(&(self.width << 16).to_be_bytes());
        tkhd.extend_from_slice(&(self.height << 16).to_be_bytes());
        // Enabled, in the movie, in its preview.
        let tkhd = full(b"tkhd", 0, 0x7, &tkhd);

        let mut mdhd = Vec::new();
        for v in [0u32, 0, self.fps, n] {
            mdhd.extend_from_slice(&v.to_be_bytes());
        }
        mdhd.extend_from_slice(&0x55C4u16.to_be_bytes());
        mdhd.extend_from_slice(&0u16.to_be_bytes());
        let mdhd = full(b"mdhd", 0, 0, &mdhd);
        let handler = |component: &[u8; 4], kind: &[u8; 4], name: &str| {
            let mut h = Vec::new();
            h.extend_from_slice(component);
            h.extend_from_slice(kind);
            h.extend_from_slice(&[0; 12]);
            h.push(name.len().min(255) as u8);
            h.extend_from_slice(name.as_bytes());
            full(b"hdlr", 0, 0, &h)
        };
        let mhlr = handler(b"mhlr", b"vide", "Video");

        let vmhd = full(b"vmhd", 0, 1, &[0, 0x40, 0, 0, 0, 0, 0, 0]);
        let dhlr = handler(b"dhlr", b"alis", "Data");
        // The pictures are in this file.
        let alis = full(b"alis", 0, 1, &[]);
        let mut dref = 1u32.to_be_bytes().to_vec();
        dref.extend_from_slice(&alis);
        let dinf = bx(b"dinf", &full(b"dref", 0, 0, &dref));

        // The pictures: PNG, 32 bits a pixel (with their see-through parts).
        let mut entry = Vec::new();
        entry.extend_from_slice(&[0; 6]);
        entry.extend_from_slice(&1u16.to_be_bytes());
        entry.extend_from_slice(&[0; 4]);
        entry.extend_from_slice(&[0; 4]);
        entry.extend_from_slice(&0u32.to_be_bytes());
        entry.extend_from_slice(&0x0000_0200u32.to_be_bytes());
        entry.extend_from_slice(&(self.width as u16).to_be_bytes());
        entry.extend_from_slice(&(self.height as u16).to_be_bytes());
        entry.extend_from_slice(&(72u32 << 16).to_be_bytes());
        entry.extend_from_slice(&(72u32 << 16).to_be_bytes());
        entry.extend_from_slice(&0u32.to_be_bytes());
        entry.extend_from_slice(&1u16.to_be_bytes());
        let mut compressor = [0u8; 32];
        compressor[0] = 3;
        compressor[1..4].copy_from_slice(b"PNG");
        entry.extend_from_slice(&compressor);
        entry.extend_from_slice(&32u16.to_be_bytes());
        entry.extend_from_slice(&0xFFFFu16.to_be_bytes());
        let mut stsd = 1u32.to_be_bytes().to_vec();
        stsd.extend_from_slice(&bx(b"png ", &entry));
        let stsd = full(b"stsd", 0, 0, &stsd);
        let mut stts = 1u32.to_be_bytes().to_vec();
        stts.extend_from_slice(&n.to_be_bytes());
        stts.extend_from_slice(&1u32.to_be_bytes());
        let stts = full(b"stts", 0, 0, &stts);
        let mut stsc = 1u32.to_be_bytes().to_vec();
        for v in [1u32, 1, 1] {
            stsc.extend_from_slice(&v.to_be_bytes());
        }
        let stsc = full(b"stsc", 0, 0, &stsc);
        let mut stsz = 0u32.to_be_bytes().to_vec();
        stsz.extend_from_slice(&n.to_be_bytes());
        for s in &self.sizes {
            stsz.extend_from_slice(&s.to_be_bytes());
        }
        let stsz = full(b"stsz", 0, 0, &stsz);
        // Where each picture is: 32 bits while the file is small enough, else 64.
        let mut offsets = n.to_be_bytes().to_vec();
        let large = self.offsets.last().is_some_and(|o| *o > u64::from(u32::MAX));
        for o in &self.offsets {
            if large {
                offsets.extend_from_slice(&o.to_be_bytes());
            } else {
                offsets.extend_from_slice(&(*o as u32).to_be_bytes());
            }
        }
        let stco = full(if large { b"co64" } else { b"stco" }, 0, 0, &offsets);
        let stbl = bx(b"stbl", &[stsd, stts, stsc, stsz, stco].concat());
        let minf = bx(b"minf", &[vmhd, dhlr, dinf, stbl].concat());
        let mdia = bx(b"mdia", &[mdhd, mhlr, minf].concat());
        let trak = bx(b"trak", &[tkhd, mdia].concat());
        bx(b"moov", &[mvhd, trak].concat())
    }
}

/// The boxes of a QuickTime or MP4 file at one level: kind, where its body starts, how long it is.
pub fn boxes(data: &[u8]) -> Vec<([u8; 4], usize, usize)> {
    let mut out = Vec::new();
    let mut at = 0usize;
    while at + 8 <= data.len() && out.len() < 10_000 {
        let size32 = data.get(at..at + 4).and_then(|b| b.try_into().ok()).map_or(0, u32::from_be_bytes) as u64;
        let Some(kind) = data.get(at + 4..at + 8).and_then(|b| <[u8; 4]>::try_from(b).ok()) else { break };
        let (head, size) = match size32 {
            1 => (16usize, data.get(at + 8..at + 16).and_then(|b| b.try_into().ok()).map_or(0, u64::from_be_bytes)),
            0 => (8, (data.len() - at) as u64),
            s => (8, s),
        };
        let Ok(size) = usize::try_from(size) else { break };
        if size < head || at + size > data.len() {
            break;
        }
        out.push((kind, at + head, size - head));
        at += size;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find<'a>(data: &'a [u8], path: &[&[u8; 4]]) -> Option<&'a [u8]> {
        let mut here = data;
        for kind in path {
            let (_, at, len) = boxes(here).into_iter().find(|(b, _, _)| b == *kind)?;
            here = here.get(at..at + len)?;
        }
        Some(here)
    }

    #[test]
    fn a_movie_of_png_pictures_keeps_them_and_says_where_they_are() {
        let pics: Vec<Vec<u8>> = (0..3u8).map(|k| vec![0x89, b'P', b'N', b'G', k, k, k]).collect();
        let mut w = MovWriter::new(std::io::Cursor::new(Vec::new()), 64, 48, 30).unwrap();
        for p in &pics {
            w.push(p).unwrap();
        }
        assert_eq!(w.frames(), 3);
        let data = w.finish().unwrap().into_inner();
        let top: Vec<[u8; 4]> = boxes(&data).iter().map(|b| b.0).collect();
        assert_eq!(top, vec![*b"ftyp", *b"mdat", *b"moov"]);
        assert_eq!(find(&data, &[b"ftyp"]).unwrap().get(..4), Some(&b"qt  "[..]));
        let stbl = find(&data, &[b"moov", b"trak", b"mdia", b"minf", b"stbl"]).unwrap();
        let stsd = find(stbl, &[b"stsd"]).unwrap();
        assert_eq!(stsd.get(12..16), Some(&b"png "[..]), "PNG pictures");
        // 32 bits a pixel: they keep what's see-through.
        assert_eq!(stsd.get(8 + 8 + 74..8 + 8 + 76), Some(&[0u8, 32][..]));
        let stsz = find(stbl, &[b"stsz"]).unwrap();
        assert_eq!(stsz.get(8..12), Some(&3u32.to_be_bytes()[..]));
        // Each picture is where the index says.
        let stco = find(stbl, &[b"stco"]).unwrap();
        for (k, p) in pics.iter().enumerate() {
            let at = u32::from_be_bytes(stco.get(8 + k * 4..12 + k * 4).unwrap().try_into().unwrap()) as usize;
            assert_eq!(data.get(at..at + p.len()), Some(&p[..]));
        }
        // A second's worth at 30 a second, the clock ticking once a picture.
        let mdhd = find(&data, &[b"moov", b"trak", b"mdia", b"mdhd"]).unwrap();
        assert_eq!((mdhd.get(12..16), mdhd.get(16..20)), (Some(&30u32.to_be_bytes()[..]), Some(&3u32.to_be_bytes()[..])));
        // Junk read as boxes is nothing, not a panic.
        assert!(boxes(&[0, 0, 0, 200, b'm', b'o', b'o', b'v']).is_empty());
        assert!(boxes(&[0, 0, 0, 1, b'm', b'd', b'a', b't', 255, 255, 255, 255, 255, 255, 255, 255]).is_empty());
        assert!(boxes(&[1, 2, 3]).is_empty());
    }
}
