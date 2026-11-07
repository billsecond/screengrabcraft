//! Sound for a recording: what the computer plays and what a microphone hears, as 48 kHz
//! 16-bit samples numbered from when the recording started.
//!
//! Through cpal: WASAPI on Windows (where the computer's sound is the speakers' device opened
//! for recording, "loopback"), Core Audio on macOS, ALSA on Linux and the BSDs. macOS, Linux
//! and the BSDs have no built-in way to record what the computer plays, so there only the
//! microphone is offered.
//!
//! A device runs on a thread of its own and hands its sound over in blocks. When a device sends
//! nothing for a while (the computer's sound is silent when nothing plays) the next block is
//! placed where it belongs in time, so the sound never drifts from the picture.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender, channel, sync_channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

use crate::CaptureError;

/// Samples a second, everything is recorded at.
pub const RATE: u32 = 48_000;
/// A gap longer than this between when a block should start and where the last one ended is
/// silence; anything shorter is the device's own timing and is joined up.
const GAP: u64 = RATE as u64 / 20;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AudioSource {
    /// A microphone by name, or the system's default one.
    Microphone(Option<String>),
    /// What the computer plays, through a speaker (an output device) by name or the system's
    /// default one.
    SystemSound(Option<String>),
}

/// Some sound: samples from `first` (per channel, at 48 kHz from the start), interleaved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SoundBlock {
    pub first: u64,
    pub samples: Vec<i16>,
}

/// A sound device being recorded.
pub struct AudioCapture {
    /// 1 or 2.
    pub channels: u8,
    /// The device's name, for the owner.
    pub name: String,
    rx: Receiver<SoundBlock>,
    stop: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
    thread: Option<JoinHandle<()>>,
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn device_name(d: &cpal::Device) -> String {
    d.description().map(|n| n.name().to_string()).unwrap_or_else(|_| "Sound device".to_string())
}

/// The microphones (and other sound inputs) by name.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn microphones() -> Vec<String> {
    let host = cpal::default_host();
    match host.input_devices() {
        Ok(ds) => ds.map(|d| device_name(&d)).collect(),
        Err(e) => {
            log::warn!("listing microphones: {e}");
            Vec::new()
        }
    }
}

/// The speakers (sound outputs) by name: what the computer plays through each can be recorded
/// on Windows.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn speakers() -> Vec<String> {
    match cpal::default_host().output_devices() {
        Ok(ds) => ds.map(|d| device_name(&d)).collect(),
        Err(e) => {
            log::warn!("listing speakers: {e}");
            Vec::new()
        }
    }
}

/// The system's default speaker, by name.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn default_speaker() -> Option<String> {
    cpal::default_host().default_output_device().map(|d| device_name(&d))
}

/// Whether what the computer plays can be recorded here.
#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
pub fn system_sound_supported() -> bool {
    cfg!(target_os = "windows") && cpal::default_host().default_output_device().is_some()
}

/// Turns whatever the device delivers into 48 kHz with at most two channels.
struct Resampler {
    step: f64,
    pos: f64,
    prev: Vec<f32>,
}

impl Resampler {
    fn new(rate: u32, channels: usize) -> Resampler {
        Resampler { step: f64::from(rate.max(1)) / f64::from(RATE), pos: 0.0, prev: vec![0.0; channels] }
    }

    /// `input` interleaved with `ch` channels; the output has `ch` channels at 48 kHz.
    fn run(&mut self, input: &[f32], ch: usize) -> Vec<f32> {
        if ch == 0 {
            return Vec::new();
        }
        let n = input.len() / ch;
        if (self.step - 1.0).abs() < 1e-9 {
            return input.get(..n * ch).unwrap_or_default().to_vec();
        }
        let frame = |i: isize, c: usize| -> f32 {
            if i < 0 { self.prev.get(c).copied().unwrap_or(0.0) } else { input.get(i as usize * ch + c).copied().unwrap_or(0.0) }
        };
        let mut out = Vec::with_capacity(((n as f64 / self.step) as usize + 2) * ch);
        // Position -1 is the last frame of the block before.
        while self.pos <= n as f64 - 1.0 {
            let i = self.pos.floor() as isize;
            let f = (self.pos - self.pos.floor()) as f32;
            for c in 0..ch {
                let (a, b) = (frame(i, c), frame(i + 1, c));
                out.push(a + (b - a) * f);
            }
            self.pos += self.step;
        }
        self.pos -= n as f64;
        if n > 0 {
            self.prev = (0..ch).map(|c| frame(n as isize - 1, c)).collect();
        }
        out
    }
}

/// Keeps a device's blocks in time: numbers them, and jumps over silence the device didn't send.
struct Clock {
    started: Instant,
    next: u64,
}

impl Clock {
    /// Where a block of `frames` that ends now starts.
    fn place(&mut self, frames: u64, now: Instant) -> u64 {
        let end = (now.saturating_duration_since(self.started).as_secs_f64() * f64::from(RATE)) as u64;
        let should = end.saturating_sub(frames);
        if should > self.next + GAP {
            self.next = should;
        }
        let first = self.next;
        self.next += frames;
        first
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn to_f32(data: &cpal::Data) -> Option<Vec<f32>> {
    use cpal::SampleFormat as F;
    Some(match data.sample_format() {
        F::F32 => data.as_slice::<f32>()?.to_vec(),
        F::F64 => data.as_slice::<f64>()?.iter().map(|s| *s as f32).collect(),
        F::I16 => data.as_slice::<i16>()?.iter().map(|s| f32::from(*s) / 32768.0).collect(),
        F::I32 => data.as_slice::<i32>()?.iter().map(|s| *s as f32 / 2_147_483_648.0).collect(),
        F::U16 => data.as_slice::<u16>()?.iter().map(|s| (f32::from(*s) - 32768.0) / 32768.0).collect(),
        F::U8 => data.as_slice::<u8>()?.iter().map(|s| (f32::from(*s) - 128.0) / 128.0).collect(),
        F::I8 => data.as_slice::<i8>()?.iter().map(|s| f32::from(*s) / 128.0).collect(),
        _ => return None,
    })
}

/// The first two channels of `ch`.
fn keep_two(input: Vec<f32>, ch: usize) -> Vec<f32> {
    if ch <= 2 {
        return input;
    }
    input.chunks_exact(ch).flat_map(|f| f.iter().take(2).copied()).collect()
}

impl AudioCapture {
    /// Starts recording a device; sample numbers count from `started`.
    pub fn start(source: AudioSource, started: Instant) -> Result<AudioCapture, CaptureError> {
        let stop = Arc::new(AtomicBool::new(false));
        let level = Arc::new(AtomicU32::new(0));
        let (tx, rx) = channel::<SoundBlock>();
        let (ready_tx, ready_rx) = sync_channel::<Result<(u8, String), String>>(1);
        let (s, l) = (stop.clone(), level.clone());
        let thread = std::thread::Builder::new()
            .name("sound".into())
            .spawn(move || run(source, started, tx, ready_tx, s, l))
            .map_err(|e| CaptureError::Device(e.to_string()))?;
        match ready_rx.recv_timeout(Duration::from_secs(10)) {
            Ok(Ok((channels, name))) => Ok(AudioCapture { channels, name, rx, stop, level, thread: Some(thread) }),
            Ok(Err(e)) => Err(CaptureError::Device(e)),
            Err(_) => {
                stop.store(true, Ordering::SeqCst);
                Err(CaptureError::Device("the sound device didn't start".to_string()))
            }
        }
    }

    /// The sound that came in since last time.
    pub fn take(&self) -> Vec<SoundBlock> {
        self.rx.try_iter().collect()
    }

    /// How loud it is just now, 0 to 1 (for a meter).
    pub fn level(&self) -> f32 {
        f32::from_bits(self.level.load(Ordering::Relaxed))
    }

    /// Stops the device; what came in up to now is returned. A device stuck in its driver isn't
    /// waited for longer than a moment.
    pub fn stop(mut self) -> Vec<SoundBlock> {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(t) = self.thread.take() {
            crate::join_within(t, crate::STOP_WAIT, "a sound device");
        }
        self.take()
    }
}

impl Drop for AudioCapture {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

#[cfg(any(target_os = "windows", target_os = "macos", target_os = "linux"))]
fn run(
    source: AudioSource,
    started: Instant,
    tx: Sender<SoundBlock>,
    ready: std::sync::mpsc::SyncSender<Result<(u8, String), String>>,
    stop: Arc<AtomicBool>,
    level: Arc<AtomicU32>,
) {
    let host = cpal::default_host();
    let device = match &source {
        AudioSource::SystemSound(Some(name)) => {
            host.output_devices().ok().and_then(|mut ds| ds.find(|d| device_name(d) == *name)).or_else(|| host.default_output_device())
        }
        AudioSource::SystemSound(None) => host.default_output_device(),
        AudioSource::Microphone(Some(name)) => {
            host.input_devices().ok().and_then(|mut ds| ds.find(|d| device_name(d) == *name)).or_else(|| host.default_input_device())
        }
        AudioSource::Microphone(None) => host.default_input_device(),
    };
    let Some(device) = device else {
        let _ = ready.send(Err(match source {
            AudioSource::SystemSound(_) => "there is no sound output to record".to_string(),
            AudioSource::Microphone(_) => "there is no microphone".to_string(),
        }));
        return;
    };
    let name = device_name(&device);
    let config = match source {
        AudioSource::SystemSound(_) => device.default_output_config(),
        AudioSource::Microphone(_) => device.default_input_config(),
    };
    let config = match config {
        Ok(c) => c,
        Err(e) => {
            let _ = ready.send(Err(format!("{name}: {e}")));
            return;
        }
    };
    let ch = usize::from(config.channels().max(1));
    let out_ch = ch.min(2);
    let mut resampler = Resampler::new(config.sample_rate(), out_ch);
    let mut clock = Clock { started, next: 0 };
    let lvl = level.clone();
    let stream = device.build_input_stream_raw(
        config.config(),
        config.sample_format(),
        move |data: &cpal::Data, _: &cpal::InputCallbackInfo| {
            let now = Instant::now();
            let Some(raw) = to_f32(data) else { return };
            let two = keep_two(raw, ch);
            let at48 = resampler.run(&two, out_ch);
            let peak = at48.iter().fold(0f32, |m, s| m.max(s.abs()));
            lvl.store(peak.min(1.0).to_bits(), Ordering::Relaxed);
            let frames = (at48.len() / out_ch) as u64;
            if frames == 0 {
                return;
            }
            let first = clock.place(frames, now);
            let samples = at48.iter().map(|s| (s.clamp(-1.0, 1.0) * 32767.0) as i16).collect();
            let _ = tx.send(SoundBlock { first, samples });
        },
        move |e| log::warn!("sound device: {e}"),
        None,
    );
    let stream = match stream {
        Ok(s) => s,
        Err(e) => {
            let _ = ready.send(Err(format!("{name}: {e}")));
            return;
        }
    };
    if let Err(e) = stream.play() {
        let _ = ready.send(Err(format!("{name}: {e}")));
        return;
    }
    let _ = ready.send(Ok((out_ch as u8, name)));
    while !stop.load(Ordering::SeqCst) {
        std::thread::sleep(Duration::from_millis(20));
    }
    drop(stream);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_device_at_48k_passes_through_untouched() {
        let mut r = Resampler::new(48_000, 2);
        let x: Vec<f32> = (0..20).map(|i| i as f32).collect();
        assert_eq!(r.run(&x, 2), x);
    }

    #[test]
    fn a_44k1_device_comes_out_at_48k_without_steps_at_block_edges() {
        let mut r = Resampler::new(44_100, 1);
        let ramp: Vec<f32> = (0..44_100).map(|i| i as f32 / 44_100.0).collect();
        let mut out = Vec::new();
        for block in ramp.chunks(441) {
            out.extend(r.run(block, 1));
        }
        assert!((out.len() as i64 - 48_000).abs() <= 2, "{}", out.len());
        // A ramp stays a ramp: every step the same size, across every block edge.
        let step = 44_100.0 / 48_000.0 / 44_100.0;
        assert!(out.windows(2).all(|w| ((w[1] - w[0]) - step).abs() < 1e-5));
    }

    #[test]
    fn more_than_two_channels_keep_the_first_two() {
        assert_eq!(keep_two(vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0], 3), vec![1.0, 2.0, 4.0, 5.0]);
    }

    #[test]
    fn blocks_join_up_but_silence_the_device_skipped_is_kept_as_time() {
        let t0 = Instant::now();
        let mut c = Clock { started: t0, next: 0 };
        // 10 ms blocks arriving on time.
        assert_eq!(c.place(480, t0 + Duration::from_millis(10)), 0);
        assert_eq!(c.place(480, t0 + Duration::from_millis(21)), 480);
        // Nothing for a second (nothing was playing): the next block lands where it belongs.
        let first = c.place(480, t0 + Duration::from_millis(1030));
        assert_eq!(first, 48 * 1030 - 480);
        // Late by a little only: joined up, no gap.
        assert_eq!(c.place(480, t0 + Duration::from_millis(1060)), first + 480);
    }
}

/// The BSDs: no sound devices yet.
#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn microphones() -> Vec<String> {
    Vec::new()
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn system_sound_supported() -> bool {
    false
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn speakers() -> Vec<String> {
    Vec::new()
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
pub fn default_speaker() -> Option<String> {
    None
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn run(
    _source: AudioSource,
    _started: Instant,
    _tx: Sender<SoundBlock>,
    ready: std::sync::mpsc::SyncSender<Result<(u8, String), String>>,
    _stop: Arc<AtomicBool>,
    _level: Arc<AtomicU32>,
) {
    let _ = ready.send(Err("sound can't be recorded on this system yet".to_string()));
}
