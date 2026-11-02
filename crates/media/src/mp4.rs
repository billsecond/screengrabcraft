//! Writing MP4 files (ISO base media file format, ISO/IEC 14496-12) with an AV1 video track
//! (AV1 Codec ISO Media File Format Binding) and an Opus audio track (Encapsulation of Opus in
//! ISO Base Media File Format).
//!
//! The samples go into one `mdat` as they come, in chunks (a stretch of video, then the sound
//! for the same stretch, so a player reads them side by side), and the index (`moov`) is
//! written at the end.

use std::io::{self, Seek, SeekFrom, Write};

/// The movie's own clock: milliseconds.
const MOVIE_TIMESCALE: u32 = 1000;

/// One coded picture or packet of sound.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sample {
    pub data: Vec<u8>,
    /// How long it lasts, in its track's timescale.
    pub duration: u32,
    /// Whether decoding can start here (a key frame; every packet of sound is one).
    pub sync: bool,
}

/// The video track: its size, its clock and the AV1 configuration record (`av1C`).
#[derive(Clone, Debug)]
pub struct VideoTrack {
    pub width: u32,
    pub height: u32,
    pub timescale: u32,
    pub av1c: Vec<u8>,
}

/// The sound track: Opus at 48 kHz.
#[derive(Clone, Debug)]
pub struct AudioTrack {
    pub channels: u8,
    /// Samples at the start the decoder makes up (the encoder's lookahead), cut by an edit list.
    pub pre_skip: u16,
}

#[derive(Default)]
struct Track {
    sizes: Vec<u32>,
    durations: Vec<u32>,
    /// Sample numbers (from 1) of the key frames.
    sync: Vec<u32>,
    /// Where each chunk starts in the file and how many samples it holds.
    chunks: Vec<(u64, u32)>,
}

impl Track {
    fn duration(&self) -> u64 {
        self.durations.iter().map(|d| u64::from(*d)).sum()
    }
}

pub struct Mp4Writer<W: Write + Seek> {
    out: W,
    pos: u64,
    mdat_at: u64,
    video: Option<(VideoTrack, Track)>,
    audio: Option<(AudioTrack, Track)>,
}

fn bx(kind: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut b = Vec::with_capacity(body.len() + 8);
    b.extend_from_slice(&((body.len() + 8) as u32).to_be_bytes());
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

/// Builds a box body from big-endian fields.
#[derive(Default)]
struct Body(Vec<u8>);

impl Body {
    fn u8(mut self, v: u8) -> Self {
        self.0.push(v);
        self
    }
    fn u16(mut self, v: u16) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn u32(mut self, v: u32) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn u64(mut self, v: u64) -> Self {
        self.0.extend_from_slice(&v.to_be_bytes());
        self
    }
    fn bytes(mut self, v: &[u8]) -> Self {
        self.0.extend_from_slice(v);
        self
    }
    fn zeros(mut self, n: usize) -> Self {
        self.0.resize(self.0.len() + n, 0);
        self
    }
    fn matrix(self) -> Self {
        self.u32(0x0001_0000).u32(0).u32(0).u32(0).u32(0x0001_0000).u32(0).u32(0).u32(0).u32(0x4000_0000)
    }
}

/// `n` in the movie's clock, from a track clock.
fn to_movie(n: u64, timescale: u32) -> u64 {
    (u128::from(n) * u128::from(MOVIE_TIMESCALE) / u128::from(timescale.max(1))).min(u128::from(u64::MAX)) as u64
}

fn too_big() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, "a sample is too large for an MP4 file")
}

impl<W: Write + Seek> Mp4Writer<W> {
    pub fn new(mut out: W, video: Option<VideoTrack>, audio: Option<AudioTrack>) -> io::Result<Mp4Writer<W>> {
        let ftyp = bx(b"ftyp", &Body::default().bytes(b"isom").u32(0x200).bytes(b"isom").bytes(b"iso2").bytes(b"av01").bytes(b"mp41").0);
        out.write_all(&ftyp)?;
        let mdat_at = ftyp.len() as u64;
        // A 64-bit size, filled in at the end.
        out.write_all(&Body::default().u32(1).bytes(b"mdat").u64(0).0)?;
        Ok(Mp4Writer { out, pos: mdat_at + 16, mdat_at, video: video.map(|v| (v, Track::default())), audio: audio.map(|a| (a, Track::default())) })
    }

    fn chunk(out: &mut W, pos: &mut u64, track: &mut Track, samples: &[Sample]) -> io::Result<()> {
        if samples.is_empty() {
            return Ok(());
        }
        let count = u32::try_from(samples.len()).map_err(|_| too_big())?;
        track.chunks.push((*pos, count));
        for s in samples {
            let size = u32::try_from(s.data.len()).map_err(|_| too_big())?;
            out.write_all(&s.data)?;
            *pos += u64::from(size);
            track.sizes.push(size);
            track.durations.push(s.duration);
            if s.sync {
                let n = u32::try_from(track.sizes.len()).map_err(|_| too_big())?;
                track.sync.push(n);
            }
        }
        Ok(())
    }

    /// The video's configuration record, once it's known (from the first key frame).
    pub fn set_av1c(&mut self, record: Vec<u8>) {
        if let Some((v, _)) = &mut self.video {
            v.av1c = record;
        }
    }

    /// A stretch of video, as one chunk.
    pub fn write_video(&mut self, samples: &[Sample]) -> io::Result<()> {
        match &mut self.video {
            Some((_, t)) => Self::chunk(&mut self.out, &mut self.pos, t, samples),
            None => Err(io::Error::new(io::ErrorKind::InvalidInput, "this file has no video track")),
        }
    }

    /// A stretch of sound, as one chunk.
    pub fn write_audio(&mut self, samples: &[Sample]) -> io::Result<()> {
        match &mut self.audio {
            Some((_, t)) => Self::chunk(&mut self.out, &mut self.pos, t, samples),
            None => Err(io::Error::new(io::ErrorKind::InvalidInput, "this file has no sound track")),
        }
    }

    /// Writes the index and hands the output back.
    pub fn finish(mut self) -> io::Result<W> {
        let mdat_size = self.pos - self.mdat_at;
        self.out.seek(SeekFrom::Start(self.mdat_at + 8))?;
        self.out.write_all(&mdat_size.to_be_bytes())?;
        self.out.seek(SeekFrom::Start(self.pos))?;
        let moov = self.moov();
        self.out.write_all(&moov)?;
        self.out.flush()?;
        Ok(self.out)
    }

    fn moov(&self) -> Vec<u8> {
        let mut traks = Vec::new();
        let mut longest = 0u64;
        let mut id = 1u32;
        if let Some((v, t)) = &self.video {
            let d = to_movie(t.duration(), v.timescale);
            longest = longest.max(d);
            traks.extend(video_trak(id, v, t, d));
            id += 1;
        }
        if let Some((a, t)) = &self.audio {
            let shown = t.duration().saturating_sub(u64::from(a.pre_skip));
            let d = to_movie(shown, 48_000);
            longest = longest.max(d);
            traks.extend(audio_trak(id, a, t, d));
            id += 1;
        }
        let mvhd = full(
            b"mvhd",
            1,
            0,
            &Body::default().u64(0).u64(0).u32(MOVIE_TIMESCALE).u64(longest).u32(0x0001_0000).u16(0x0100).zeros(10).matrix().zeros(24).u32(id).0,
        );
        bx(b"moov", &[mvhd, traks].concat())
    }
}

fn tkhd(id: u32, duration: u64, audio: bool, width: u32, height: u32) -> Vec<u8> {
    full(
        b"tkhd",
        1,
        3,
        &Body::default()
            .u64(0)
            .u64(0)
            .u32(id)
            .u32(0)
            .u64(duration)
            .zeros(8)
            .u16(0)
            .u16(0)
            .u16(if audio { 0x0100 } else { 0 })
            .u16(0)
            .matrix()
            .u32(width.min(0xFFFF) << 16)
            .u32(height.min(0xFFFF) << 16)
            .0,
    )
}

fn mdhd(timescale: u32, duration: u64) -> Vec<u8> {
    // Language "und", packed as three five-bit letters.
    full(b"mdhd", 1, 0, &Body::default().u64(0).u64(0).u32(timescale).u64(duration).u16(0x55C4).u16(0).0)
}

fn hdlr(kind: &[u8; 4], name: &str) -> Vec<u8> {
    full(b"hdlr", 0, 0, &Body::default().u32(0).bytes(kind).zeros(12).bytes(name.as_bytes()).u8(0).0)
}

fn dinf() -> Vec<u8> {
    bx(b"dinf", &full(b"dref", 0, 0, &Body::default().u32(1).bytes(&full(b"url ", 0, 1, &[])).0))
}

fn tables(t: &Track, entry: Vec<u8>, with_sync: bool) -> Vec<u8> {
    let stsd = full(b"stsd", 0, 0, &Body::default().u32(1).bytes(&entry).0);
    // Durations, run-length coded.
    let mut runs: Vec<(u32, u32)> = Vec::new();
    for d in &t.durations {
        match runs.last_mut() {
            Some((n, last)) if *last == *d => *n += 1,
            _ => runs.push((1, *d)),
        }
    }
    let mut stts = Body::default().u32(runs.len() as u32);
    for (n, d) in &runs {
        stts = stts.u32(*n).u32(*d);
    }
    let mut stsc_runs: Vec<(u32, u32)> = Vec::new();
    for (i, (_, n)) in t.chunks.iter().enumerate() {
        if stsc_runs.last().is_none_or(|(_, last)| last != n) {
            stsc_runs.push((i as u32 + 1, *n));
        }
    }
    let mut stsc = Body::default().u32(stsc_runs.len() as u32);
    for (first, n) in &stsc_runs {
        stsc = stsc.u32(*first).u32(*n).u32(1);
    }
    let mut stsz = Body::default().u32(0).u32(t.sizes.len() as u32);
    for s in &t.sizes {
        stsz = stsz.u32(*s);
    }
    let mut co64 = Body::default().u32(t.chunks.len() as u32);
    for (at, _) in &t.chunks {
        co64 = co64.u64(*at);
    }
    let mut parts = vec![stsd, full(b"stts", 0, 0, &stts.0)];
    if with_sync {
        let mut stss = Body::default().u32(t.sync.len() as u32);
        for s in &t.sync {
            stss = stss.u32(*s);
        }
        parts.push(full(b"stss", 0, 0, &stss.0));
    }
    parts.extend([full(b"stsc", 0, 0, &stsc.0), full(b"stsz", 0, 0, &stsz.0), full(b"co64", 0, 0, &co64.0)]);
    bx(b"stbl", &parts.concat())
}

fn video_trak(id: u32, v: &VideoTrack, t: &Track, duration: u64) -> Vec<u8> {
    let mut name = [0u8; 32];
    let label = b"AV1 (ScreenGrabCraft)";
    name[0] = label.len() as u8;
    if let Some(dst) = name.get_mut(1..=label.len()) {
        dst.copy_from_slice(label);
    }
    // BT.709 colours, limited range: what `yuv` makes.
    let colr = bx(b"colr", &Body::default().bytes(b"nclx").u16(1).u16(1).u16(1).u8(0).0);
    let entry = bx(
        b"av01",
        &Body::default()
            .zeros(6)
            .u16(1)
            .zeros(16)
            .u16(v.width.min(0xFFFF) as u16)
            .u16(v.height.min(0xFFFF) as u16)
            .u32(0x0048_0000)
            .u32(0x0048_0000)
            .u32(0)
            .u16(1)
            .bytes(&name)
            .u16(0x0018)
            .u16(0xFFFF)
            .bytes(&bx(b"av1C", &v.av1c))
            .bytes(&colr)
            .0,
    );
    let minf = bx(b"minf", &[full(b"vmhd", 0, 1, &Body::default().u16(0).u16(0).u16(0).u16(0).0), dinf(), tables(t, entry, true)].concat());
    let mdia = bx(b"mdia", &[mdhd(v.timescale, t.duration()), hdlr(b"vide", "VideoHandler"), minf].concat());
    bx(b"trak", &[tkhd(id, duration, false, v.width, v.height), mdia].concat())
}

fn audio_trak(id: u32, a: &AudioTrack, t: &Track, duration: u64) -> Vec<u8> {
    let dops = bx(b"dOps", &Body::default().u8(0).u8(a.channels).u16(a.pre_skip).u32(48_000).u16(0).u8(0).0);
    let entry =
        bx(b"Opus", &Body::default().zeros(6).u16(1).zeros(8).u16(u16::from(a.channels)).u16(16).u16(0).u16(0).u32(48_000 << 16).bytes(&dops).0);
    let minf = bx(b"minf", &[full(b"smhd", 0, 0, &Body::default().u16(0).u16(0).0), dinf(), tables(t, entry, false)].concat());
    let mdia = bx(b"mdia", &[mdhd(48_000, t.duration()), hdlr(b"soun", "SoundHandler"), minf].concat());
    // The decoder's first samples are its warm-up: the edit list starts the sound after them.
    let elst = full(b"elst", 1, 0, &Body::default().u32(1).u64(duration).u64(u64::from(a.pre_skip)).u16(1).u16(0).0);
    bx(b"trak", &[tkhd(id, duration, true, 0, 0), bx(b"edts", &elst), mdia].concat())
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::*;

    /// The boxes at the top of a file, as (kind, start, size).
    fn top(b: &[u8]) -> Vec<(String, usize, u64)> {
        let mut at = 0usize;
        let mut out = Vec::new();
        while at + 8 <= b.len() {
            let size32 = u32::from_be_bytes(b[at..at + 4].try_into().unwrap());
            let kind = String::from_utf8_lossy(&b[at + 4..at + 8]).to_string();
            let size = if size32 == 1 { u64::from_be_bytes(b[at + 8..at + 16].try_into().unwrap()) } else { u64::from(size32) };
            out.push((kind, at, size));
            at += size as usize;
        }
        out
    }

    fn find(b: &[u8], kind: &[u8; 4]) -> Option<usize> {
        b.windows(4).position(|w| w == kind)
    }

    #[test]
    fn a_file_is_ftyp_mdat_moov_and_the_sizes_add_up() {
        let video = VideoTrack { width: 64, height: 48, timescale: 30, av1c: vec![0x81, 0, 0, 0] };
        let audio = AudioTrack { channels: 2, pre_skip: 312 };
        let mut w = Mp4Writer::new(Cursor::new(Vec::new()), Some(video), Some(audio)).unwrap();
        w.write_video(&[Sample { data: vec![1; 100], duration: 1, sync: true }, Sample { data: vec![2; 10], duration: 1, sync: false }]).unwrap();
        w.write_audio(&vec![Sample { data: vec![3; 50], duration: 960, sync: true }; 4]).unwrap();
        w.write_video(&[Sample { data: vec![4; 7], duration: 1, sync: true }]).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let boxes = top(&bytes);
        let kinds: Vec<&str> = boxes.iter().map(|b| b.0.as_str()).collect();
        assert_eq!(kinds, ["ftyp", "mdat", "moov"]);
        assert_eq!(boxes.iter().map(|b| b.2).sum::<u64>(), bytes.len() as u64);
        assert_eq!(boxes[1].2, 16 + 100 + 10 + 200 + 7);
        for k in [b"av01", b"av1C", b"Opus", b"dOps", b"stss", b"co64", b"elst"] {
            assert!(find(&bytes, k).is_some(), "{}", String::from_utf8_lossy(k));
        }
    }

    #[test]
    fn chunk_offsets_point_at_the_samples() {
        let video = VideoTrack { width: 2, height: 2, timescale: 30, av1c: vec![] };
        let mut w = Mp4Writer::new(Cursor::new(Vec::new()), Some(video), None).unwrap();
        w.write_video(&[Sample { data: b"first".to_vec(), duration: 1, sync: true }]).unwrap();
        w.write_video(&[Sample { data: b"second".to_vec(), duration: 1, sync: false }]).unwrap();
        let bytes = w.finish().unwrap().into_inner();
        let co64 = find(&bytes, b"co64").unwrap();
        let n = u32::from_be_bytes(bytes[co64 + 8..co64 + 12].try_into().unwrap());
        assert_eq!(n, 2);
        let at = |i: usize| u64::from_be_bytes(bytes[co64 + 12 + i * 8..co64 + 20 + i * 8].try_into().unwrap()) as usize;
        assert_eq!(&bytes[at(0)..at(0) + 5], b"first");
        assert_eq!(&bytes[at(1)..at(1) + 6], b"second");
    }

    #[test]
    fn writing_to_a_track_the_file_lacks_is_an_error() {
        let mut w = Mp4Writer::new(Cursor::new(Vec::new()), None, Some(AudioTrack { channels: 1, pre_skip: 0 })).unwrap();
        assert!(w.write_video(&[Sample { data: vec![0], duration: 1, sync: true }]).is_err());
    }
}
