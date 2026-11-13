//! Making the MP4 with Windows' own encoders: H.264 pictures and AAC sound, what every player
//! opens (Windows Media Player included, which can't play AV1 or Opus). Windows comes with these
//! encoders and their licences; they're asked for through its media transcoder, which pulls the
//! edited video's pictures and sound from here one at a time, as it's ready for them.
//!
//! Nothing here is `unsafe`: the windows crate's Windows Runtime calls are safe Rust.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex, PoisonError};

use windows::Foundation::{TimeSpan, TypedEventHandler};
use windows::Media::Core::{
    AudioStreamDescriptor, MediaStreamSample, MediaStreamSource, MediaStreamSourceSampleRequestedEventArgs, VideoStreamDescriptor,
};
use windows::Media::MediaProperties::{
    AudioEncodingProperties, MediaEncodingProfile, MediaEncodingSubtypes, VideoEncodingProperties, VideoEncodingQuality,
};
use windows::Media::Transcoding::MediaTranscoder;
use windows::Security::Cryptography::CryptographicBuffer;
use windows::Storage::{FileAccessMode, StorageFile};
use windows::core::{HSTRING, Interface};

use crate::MediaError;
use crate::captions::Cue;
use crate::compose::{self, Composer};
use crate::export::{ExportSettings, Exported};
use crate::recording::Recording;
use crate::timeline::Timeline;
use crate::yuv::I420;

/// Sound is handed over 20 ms at a time.
const PIECE: u64 = 960;
/// Windows counts time in tenths of a microsecond.
const TICKS: u64 = 10_000_000;
/// AAC, bits a second.
const AAC_BITRATE: u32 = 192_000;

/// What the transcoder pulls from: the edited video, picture by picture and piece by piece.
struct Feed {
    rec: Arc<Recording>,
    timeline: Arc<Timeline>,
    composer: Composer,
    width: u32,
    height: u32,
    fps: u64,
    frames: u64,
    next_frame: u64,
    samples: u64,
    next_sample: u64,
    /// What went wrong making a picture or a piece of sound (the video then ends there).
    trouble: Option<String>,
}

impl Feed {
    fn picture(&mut self) -> Result<Option<MediaStreamSample>, String> {
        if self.next_frame >= self.frames {
            return Ok(None);
        }
        let f = self.next_frame;
        self.next_frame += 1;
        let pic = self.composer.frame(&self.timeline, f.saturating_mul(1_000_000) / self.fps);
        let yuv = I420::from_rgba(pic.pixels(), pic.width(), pic.height()).ok_or("a picture of the wrong size")?;
        if (yuv.width, yuv.height) != (self.width, self.height) {
            return Err(format!("a picture of {} × {} in a video of {} × {}", yuv.width, yuv.height, self.width, self.height));
        }
        let at = TimeSpan { Duration: i64::try_from(f.saturating_mul(TICKS) / self.fps).unwrap_or(i64::MAX) };
        let sample = sample(&yuv.to_nv12(), at).map_err(|e| e.message())?;
        let long = TimeSpan { Duration: i64::try_from(TICKS / self.fps).unwrap_or(0) };
        sample.SetDuration(long).map_err(|e| e.message())?;
        sample.SetKeyFrame(true).map_err(|e| e.message())?;
        Ok(Some(sample))
    }

    fn sound(&mut self) -> Result<Option<MediaStreamSample>, String> {
        if self.next_sample >= self.samples {
            return Ok(None);
        }
        let first = self.next_sample;
        let n = PIECE.min(self.samples - first);
        self.next_sample += n;
        let mixed = compose::mix(&self.rec, &self.timeline, first, n as usize);
        let mut bytes = Vec::with_capacity(mixed.len() * 2);
        for v in &mixed {
            bytes.extend_from_slice(&((v.clamp(-1.0, 1.0) * 32767.0) as i16).to_le_bytes());
        }
        let at = TimeSpan { Duration: i64::try_from(first.saturating_mul(TICKS) / 48_000).unwrap_or(i64::MAX) };
        let sample = sample(&bytes, at).map_err(|e| e.message())?;
        sample.SetDuration(TimeSpan { Duration: i64::try_from(n * TICKS / 48_000).unwrap_or(0) }).map_err(|e| e.message())?;
        Ok(Some(sample))
    }
}

fn sample(bytes: &[u8], at: TimeSpan) -> windows::core::Result<MediaStreamSample> {
    MediaStreamSample::CreateFromBuffer(&CryptographicBuffer::CreateFromByteArray(bytes)?, at)
}

/// Answers one of the transcoder's requests: the next picture or piece of sound, or nothing
/// (that stream is over: the end, cancelled, or something went wrong).
fn answer(
    feed: &Mutex<Feed>,
    cancel: &AtomicBool,
    progress: &AtomicU32,
    args: &MediaStreamSourceSampleRequestedEventArgs,
) -> windows::core::Result<()> {
    let request = args.Request()?;
    let is_video = request.StreamDescriptor()?.cast::<VideoStreamDescriptor>().is_ok();
    let mut feed = feed.lock().unwrap_or_else(PoisonError::into_inner);
    if cancel.load(Ordering::SeqCst) || feed.trouble.is_some() {
        return Ok(());
    }
    // A panic must never cross into Windows: it would end the program.
    let made = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| if is_video { feed.picture() } else { feed.sound() }))
        .unwrap_or_else(|_| Err("something went wrong while making the video".to_string()));
    match made {
        Ok(Some(s)) => {
            if is_video {
                progress.store((feed.next_frame.saturating_mul(1000) / feed.frames.max(1)).min(999) as u32, Ordering::SeqCst);
            }
            request.SetSample(&s)
        }
        Ok(None) => Ok(()),
        Err(e) => {
            feed.trouble = Some(e);
            Ok(())
        }
    }
}

/// A Windows path for a file (the media system wants it whole, with backslashes).
fn windows_path(p: &Path) -> Result<HSTRING, MediaError> {
    let whole = std::path::absolute(p)?;
    Ok(HSTRING::from(whole.to_string_lossy().replace('/', "\\")))
}

/// Bits a second for the pictures: about a tenth of a bit a pixel, which keeps text sharp.
fn bitrate(width: u32, height: u32, fps: u64) -> u32 {
    let b = u64::from(width) * u64::from(height) * fps / 10;
    b.clamp(1_500_000, 16_000_000) as u32
}

/// One go at it, with the computer's video hardware or without.
#[allow(clippy::too_many_arguments)]
fn transcode(
    rec: &Arc<Recording>,
    timeline: &Arc<Timeline>,
    captions: &Option<Arc<Vec<Cue>>>,
    part: &Path,
    size: (u32, u32),
    fps: u64,
    with_sound: bool,
    hardware: bool,
    progress: &Arc<AtomicU32>,
    cancel: &Arc<AtomicBool>,
) -> Result<(), MediaError> {
    let err = |e: windows::core::Error| MediaError::Encode(e.message());
    let (w, h) = size;
    let length = timeline.length();
    let mut composer = Composer::new(rec.clone(), timeline, w, h);
    composer.set_captions(captions.clone());
    let feed = Arc::new(Mutex::new(Feed {
        rec: rec.clone(),
        timeline: timeline.clone(),
        composer,
        width: w,
        height: h,
        fps,
        frames: length.saturating_mul(fps).div_ceil(1_000_000).max(1),
        next_frame: 0,
        samples: compose::to_samples(length),
        next_sample: 0,
        trouble: None,
    }));

    let video =
        VideoStreamDescriptor::Create(&VideoEncodingProperties::CreateUncompressed(&MediaEncodingSubtypes::Nv12().map_err(err)?, w, h).map_err(err)?)
            .map_err(err)?;
    let source = if with_sound {
        let sound = AudioStreamDescriptor::Create(&AudioEncodingProperties::CreatePcm(48_000, 2, 16).map_err(err)?).map_err(err)?;
        MediaStreamSource::CreateFromDescriptors(&video, &sound).map_err(err)?
    } else {
        MediaStreamSource::CreateFromDescriptor(&video).map_err(err)?
    };
    source.SetCanSeek(false).map_err(err)?;
    source.SetDuration(TimeSpan { Duration: i64::try_from(length.saturating_mul(10)).unwrap_or(i64::MAX) }).map_err(err)?;
    let (f, c, p) = (feed.clone(), cancel.clone(), progress.clone());
    source
        .SampleRequested(&TypedEventHandler::<MediaStreamSource, MediaStreamSourceSampleRequestedEventArgs>::new(move |_, args| {
            match args.as_ref() {
                Some(a) => answer(&f, &c, &p, a),
                None => Ok(()),
            }
        }))
        .map_err(err)?;

    let profile = MediaEncodingProfile::CreateMp4(VideoEncodingQuality::HD1080p).map_err(err)?;
    let v = profile.Video().map_err(err)?;
    v.SetWidth(w).map_err(err)?;
    v.SetHeight(h).map_err(err)?;
    v.SetBitrate(bitrate(w, h, fps)).map_err(err)?;
    let rate = v.FrameRate().map_err(err)?;
    rate.SetNumerator(fps as u32).map_err(err)?;
    rate.SetDenominator(1).map_err(err)?;
    let aspect = v.PixelAspectRatio().map_err(err)?;
    aspect.SetNumerator(1).map_err(err)?;
    aspect.SetDenominator(1).map_err(err)?;
    if with_sound {
        profile.SetAudio(&AudioEncodingProperties::CreateAac(48_000, 2, AAC_BITRATE).map_err(err)?).map_err(err)?;
    } else {
        profile.SetAudio(None::<&AudioEncodingProperties>).map_err(err)?;
    }

    std::fs::File::create(part)?;
    let file = StorageFile::GetFileFromPathAsync(&windows_path(part)?).map_err(err)?.join().map_err(err)?;
    let stream = file.OpenAsync(FileAccessMode::ReadWrite).map_err(err)?.join().map_err(err)?;
    let transcoder = MediaTranscoder::new().map_err(err)?;
    transcoder.SetHardwareAccelerationEnabled(hardware).map_err(err)?;
    let prepared = transcoder.PrepareMediaStreamSourceTranscodeAsync(&source, &stream, &profile).map_err(err)?.join().map_err(err)?;
    if !prepared.CanTranscode().map_err(err)? {
        let why = prepared.FailureReason().map_err(err)?;
        let _ = stream.Close();
        return Err(MediaError::Encode(format!("Windows can't make this video ({why:?})")));
    }
    let made = prepared.TranscodeAsync().map_err(err)?.join().map_err(err);
    let _ = stream.Close();
    made?;
    if cancel.load(Ordering::SeqCst) {
        return Err(MediaError::Encode("cancelled".to_string()));
    }
    if let Some(t) = feed.lock().unwrap_or_else(PoisonError::into_inner).trouble.take() {
        return Err(MediaError::Encode(t));
    }
    Ok(())
}

/// Makes `out` (written beside it first, then put in place) from the recording as edited, with
/// captions (in timeline time) burned into the pictures if there are some. `progress` goes from
/// 0 to 1000; setting `cancel` stops it and leaves nothing behind.
pub fn export(
    rec: Arc<Recording>,
    timeline: &Timeline,
    captions: Option<Arc<Vec<Cue>>>,
    out: &Path,
    s: ExportSettings,
    progress: &AtomicU32,
    cancel: &AtomicBool,
) -> Result<Exported, MediaError> {
    let length = timeline.length();
    if length == 0 {
        return Err(MediaError::Encode("there's nothing left to make a video of".to_string()));
    }
    let (w, h) = crate::export::video_size(&rec, &s);
    let fps = u64::from(s.fps.clamp(1, 60));
    let with_sound = timeline.tracks.iter().any(|t| !t.kind.is_video() && !t.muted && !t.clips.is_empty());
    let part: PathBuf = out.with_extension("mp4.part");
    let timeline = Arc::new(timeline.clone());
    // The transcoder's requests come on Windows' own threads: these are shared with them, and
    // the caller's cancel and progress passed to and fro meanwhile.
    let (inner_progress, inner_cancel, done) = (Arc::new(AtomicU32::new(0)), Arc::new(AtomicBool::new(false)), AtomicBool::new(false));
    let made = std::thread::scope(|scope| {
        let watcher = scope.spawn(|| {
            while !done.load(Ordering::SeqCst) {
                if cancel.load(Ordering::SeqCst) {
                    inner_cancel.store(true, Ordering::SeqCst);
                }
                progress.store(inner_progress.load(Ordering::SeqCst), Ordering::SeqCst);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        });
        let go = |hardware: bool| transcode(&rec, &timeline, &captions, &part, (w, h), fps, with_sound, hardware, &inner_progress, &inner_cancel);
        // The video hardware first; if it can't, the processor.
        let mut r = go(true);
        if let Err(e) = &r
            && !inner_cancel.load(Ordering::SeqCst)
        {
            log::warn!("the video hardware couldn't make the video ({e}); trying again on the processor");
            inner_progress.store(0, Ordering::SeqCst);
            r = go(false);
        }
        done.store(true, Ordering::SeqCst);
        let _ = watcher.join();
        r
    });
    if let Err(e) = made {
        let _ = std::fs::remove_file(&part);
        return Err(e);
    }
    std::fs::rename(&part, out)?;
    progress.store(1000, Ordering::SeqCst);
    let frames = length.saturating_mul(fps).div_ceil(1_000_000);
    Ok(Exported { width: w, height: h, frames, bytes: std::fs::metadata(out)?.len(), duration_us: length })
}
