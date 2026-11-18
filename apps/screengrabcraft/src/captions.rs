//! Captions made on this computer, in the background: of Video Production recordings after each
//! one (when Settings says so, or the editor asks), and of the sound files and videos put on a
//! recording's timeline or picked in the editor to transcribe (made by another program, say).
//!
//! They're made by a program of their own: this one again, started as
//! `screengrabcraft --transcribe <file> [--again]` ([`run`]), at the lowest priority, on one
//! thread (about a twelfth of the sound's length). That keeps the service small (the model and
//! its working memory go when the captions are made) and out of harm's way, whatever a model
//! file or a recording holds. One is made at a time, never while recording: a recording
//! starting stops the one being made, and it's made again after.
//!
//! - A recording's captions go into it (the editor shows them on its timeline) and beside it, as
//!   `<name>.srt` and `<name>.txt`.
//! - A sound file's or a video's go to the settings folder (`paths::media_captions`), as
//!   subtitles in its own time, made once unless asked `--again`; the editor places them where
//!   it's put on the timeline, and writes them beside the file when asked to transcribe it.
//!
//! How far it has got is written to a small file in the temporary folder
//! (`paths::captions_progress`): the editor shows it. Nothing of what was said is logged.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use screengrabcraft_desktop::{models, paths, store};
use screengrabcraft_media::captions::{self, Cue, Speaker};
use screengrabcraft_media::layers::MediaKind;
use screengrabcraft_media::media_store;
use screengrabcraft_media::recording::{self, FileSource, Header, Recording, RecordingWriter, StreamInfo, StreamKind};

/// The argument the service starts the program that makes captions with.
pub const ARG: &str = "--transcribe";
/// After the file: hear it even if its captions were made before.
pub const AGAIN: &str = "--again";

/// Threads to hear with: one. Measured on ten minutes of speech, one thread took 51 s of the
/// processor (and 51 s to wait for), two took 61 s (31 s to wait for), and 107 s with the
/// computer busy, waiting on each other. One costs the least, and never more than one core.
pub fn threads() -> usize {
    1
}

/// Files waiting for captions (and whether to hear them again), and the one being made.
#[derive(Default)]
pub struct Captioner {
    queue: VecDeque<(PathBuf, bool)>,
    running: Option<(PathBuf, bool, Child, Instant)>,
}

fn name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Starts the program that makes a file's captions.
fn spawn(path: &Path, again: bool) -> std::io::Result<Child> {
    let exe = std::env::current_exe()?;
    let n = threads().to_string();
    // candle sizes its pools from these when it starts: they can only be set for a new program.
    let quiet = |mut c: Command| {
        c.arg(ARG).arg(path).env("CANDLE_NUM_THREADS", &n).env("RAYON_NUM_THREADS", &n);
        if again {
            c.arg(AGAIN);
        }
        c.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
        c
    };
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        // The lowest priority, and no console window.
        const IDLE_PRIORITY_CLASS: u32 = 0x0000_0040;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut c = quiet(Command::new(&exe));
        c.creation_flags(IDLE_PRIORITY_CLASS | CREATE_NO_WINDOW);
        c.spawn()
    }
    #[cfg(not(windows))]
    {
        // `nice` gives it the lowest priority; where there's none it runs as it is.
        let mut nice = Command::new("nice");
        nice.args(["-n", "19"]).arg(&exe);
        quiet(nice).spawn().or_else(|_| quiet(Command::new(&exe)).spawn())
    }
}

impl Captioner {
    /// A file to make captions of (once: asking again while it waits changes nothing, except
    /// that one asked to be heard again is).
    pub fn push(&mut self, path: PathBuf, again: bool) {
        let busy = self.running.as_ref().is_some_and(|(p, _, _, _)| *p == path);
        if busy {
            return;
        }
        if let Some(waiting) = self.queue.iter_mut().find(|(p, _)| *p == path) {
            waiting.1 |= again;
            return;
        }
        log::info!("captions of {} to be made", name(&path));
        self.queue.push_back((path, again));
    }

    /// Whether captions are being made or waiting.
    pub fn busy(&self) -> bool {
        self.running.is_some() || !self.queue.is_empty()
    }

    /// What's being heard now, for the tray: the file's name.
    pub fn now_hearing(&self) -> Option<String> {
        self.running.as_ref().map(|(p, _, _, _)| name(p))
    }

    /// Looked at every frame: starts the next when nothing is recorded, stops the one being made
    /// when a recording is (it's made again after).
    pub fn tick(&mut self, recording: bool) {
        if let Some((path, again, child, started)) = &mut self.running {
            match child.try_wait() {
                Ok(Some(status)) => {
                    let secs = started.elapsed().as_secs_f32();
                    if status.success() {
                        log::info!("captions of {} made in {secs:.1} s", name(path));
                    } else {
                        log::warn!("captions of {} could not be made ({status}); see captions.log", name(path));
                    }
                    self.running = None;
                }
                Ok(None) if recording => {
                    let _ = child.kill();
                    let _ = child.wait();
                    log::info!("captions of {} put off until the recording is over", name(path));
                    let again = (path.clone(), *again);
                    let _ = std::fs::remove_file(paths::captions_progress(&again.0));
                    self.running = None;
                    self.queue.push_front(again);
                }
                Ok(None) => {}
                Err(e) => {
                    log::warn!("captions of {}: {e}", name(path));
                    self.running = None;
                }
            }
        }
        if self.running.is_some() || recording {
            return;
        }
        while let Some((path, again)) = self.queue.pop_front() {
            if !path.is_file() {
                continue;
            }
            match spawn(&path, again) {
                Ok(child) => {
                    log::info!("making the captions of {} ({} threads, low priority)", name(&path), threads());
                    self.running = Some((path, again, child, Instant::now()));
                    return;
                }
                Err(e) => log::warn!("the captions of {} could not be started: {e}", name(&path)),
            }
        }
    }

    /// The service is stopping: so does the one being made (it's made again when asked).
    pub fn stop(&mut self) {
        if let Some((path, _, mut child, _)) = self.running.take() {
            let _ = child.kill();
            let _ = child.wait();
            let _ = std::fs::remove_file(paths::captions_progress(&path));
            log::info!("captions of {} stopped with the service", name(&path));
        }
    }
}

/// Whether a file is a recording (by its name).
fn is_recording(path: &Path) -> bool {
    path.extension().is_some_and(|e| e.eq_ignore_ascii_case(recording::EXT))
}

/// Whether `path` names a file to make captions of (from the editor: a file that is there, a
/// recording, a sound file or a video by its name).
pub fn wanted(path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path);
    let hearable = is_recording(&p) || matches!(media_store::kind_of(path), Some(MediaKind::Sound | MediaKind::Video));
    (hearable && p.is_file()).then_some(p)
}

/// The files beside a recording the captions are written to: `.srt` and `.txt`.
pub fn sidecars(path: &Path) -> (PathBuf, PathBuf) {
    (path.with_extension("srt"), path.with_extension("txt"))
}

/// Where the program says how far it has got: a number from 0 to 1000, rewritten every percent.
struct Progress {
    file: PathBuf,
    told: u32,
}

impl Progress {
    fn new(of: &Path) -> Progress {
        let file = paths::captions_progress(of);
        if let Some(dir) = file.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let mut p = Progress { file, told: u32::MAX };
        p.tell(0.0);
        p
    }

    fn tell(&mut self, f: f32) {
        let now = (f.clamp(0.0, 1.0) * 1000.0) as u32;
        if self.told == u32::MAX || now >= self.told.saturating_add(10) || (now == 1000 && self.told != 1000) {
            if self.told != u32::MAX && now / 100 > self.told / 100 {
                log::info!("{}0 % heard", now / 100);
            }
            self.told = now;
            let _ = std::fs::write(&self.file, now.to_string());
        }
    }

    /// Done: the file goes. Not: it says why.
    fn end(self, result: &Result<Option<usize>, String>) {
        match result {
            Ok(_) => {
                let _ = std::fs::remove_file(&self.file);
            }
            Err(e) => {
                let _ = std::fs::write(&self.file, format!("error: {e}"));
            }
        }
    }
}

/// The speech model, read.
fn model() -> Result<screengrabcraft_speech::Whisper, String> {
    let model_path = models::speech_model()?;
    let started = Instant::now();
    let bytes = models::read_model(&model_path, screengrabcraft_speech::MAX_MODEL_BYTES as u64)?;
    let model = screengrabcraft_speech::load(&bytes).map_err(|e| e.to_string())?;
    log::info!("speech model {} read in {:.2} s", model.name, started.elapsed().as_secs_f32());
    Ok(model)
}

/// Hears a recording and writes its captions; the number of lines, or none when there was
/// nothing to hear.
fn make_recording(path: &Path, progress: &mut dyn FnMut(f32)) -> Result<Option<usize>, String> {
    let rec = Recording::open(Box::new(FileSource::open(path).map_err(|e| e.to_string())?)).map_err(|e| e.to_string())?;
    if !rec.header.streams.iter().any(|s| Speaker::of(s.kind).is_some()) {
        log::info!("{} has no sound to hear", name(path));
        return Ok(None);
    }
    let mut model = model()?;
    let secs = rec.duration_us as f32 / 1e6;
    let heard = Instant::now();
    let caps = screengrabcraft_speech::transcribe(&mut model, &rec, progress, &AtomicBool::new(false)).map_err(|e| e.to_string())?;
    let took = heard.elapsed().as_secs_f32();
    log::info!("{secs:.0} s of recording heard in {took:.1} s: {} lines", caps.cues.len());
    drop(rec);
    // Into the recording (the editor reads them from there), then beside it.
    let chunk = recording::captions_bytes(&caps).map_err(|e| e.to_string())?;
    let mut f = std::fs::OpenOptions::new().append(true).open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    f.write_all(&chunk).and_then(|()| f.sync_data()).map_err(|e| format!("{}: {e}", path.display()))?;
    let (srt, txt) = sidecars(path);
    store::write_atomic(&txt, captions::to_text(&caps.cues).as_bytes()).map_err(|e| format!("{}: {e}", txt.display()))?;
    // The .srt last: the editor takes it as the sign that they're there.
    store::write_atomic(&srt, captions::to_srt(&caps.cues).as_bytes()).map_err(|e| format!("{}: {e}", srt.display()))?;
    Ok(Some(caps.cues.len()))
}

/// Sound (interleaved stereo at 48 kHz) as a recording of one track, the computer's sound, in
/// one channel: what the speech model hears a recording's tracks from.
pub fn sound_recording(stereo: &[i16], duration_us: u64) -> Result<Recording, String> {
    let streams = vec![StreamInfo { id: 1, kind: StreamKind::SystemSound, name: String::new(), width: 0, height: 0, channels: 1 }];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 30, streams }).map_err(|e| e.to_string())?;
    let mut at = 0u64;
    for block in stereo.chunks(9600 * 2) {
        let mono: Vec<i16> =
            block.chunks(2).map(|f| ((i32::from(f.first().copied().unwrap_or(0)) + i32::from(f.get(1).copied().unwrap_or(0))) / 2) as i16).collect();
        w.audio(1, at, &mono).map_err(|e| e.to_string())?;
        at += mono.len() as u64;
    }
    let bytes = w.finish(duration_us.max(1)).map_err(|e| e.to_string())?;
    Recording::open(Box::new(bytes)).map_err(|e| e.to_string())
}

/// Hears a sound file or a video's sound and writes its captions where the editor looks for
/// them (unless they were made before and it isn't asked `again`).
fn make_media(path: &Path, again: bool, progress: &mut dyn FnMut(f32)) -> Result<Option<usize>, String> {
    let out = paths::media_captions(path).ok_or("the file isn't there")?;
    if !again && out.is_file() {
        log::info!("the captions of {} were made before", name(path));
        return Ok(None);
    }
    let ext = path.extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let (sound, duration) = media_store::decode_sound_file(file, &ext)?;
    let rec = sound_recording(&sound, duration)?;
    drop(sound);
    let mut model = model()?;
    let heard = Instant::now();
    let caps = screengrabcraft_speech::transcribe(&mut model, &rec, progress, &AtomicBool::new(false)).map_err(|e| e.to_string())?;
    log::info!("{:.0} s of sound heard in {:.1} s: {} lines", duration as f32 / 1e6, heard.elapsed().as_secs_f32(), caps.cues.len());
    let cues: Vec<Cue> = caps.cues.into_iter().map(|c| Cue { speaker: Speaker::Media, ..c }).collect();
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    store::write_atomic(&out, captions::to_srt(&cues).as_bytes()).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(Some(cues.len()))
}

/// The program that makes a file's captions (`--transcribe <file> [--again]`).
pub fn run(path: &str, again: bool) -> ExitCode {
    let path = Path::new(path);
    log::info!("making the captions of {} ({} threads)", name(path), std::env::var("CANDLE_NUM_THREADS").unwrap_or_else(|_| "all".into()));
    let mut progress = Progress::new(path);
    let result = {
        let mut tell = |f: f32| progress.tell(f);
        if is_recording(path) { make_recording(path, &mut tell) } else { make_media(path, again, &mut tell) }
    };
    progress.end(&result);
    match result {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            log::error!("the captions of {} could not be made: {e}", name(path));
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_model_is_the_one_the_speech_crate_expects() {
        assert_eq!(models::SPEECH, screengrabcraft_speech::BUNDLED);
    }

    #[test]
    fn a_file_is_asked_for_once_and_only_one_with_sound() {
        let dir = std::env::temp_dir().join(format!("sgc-captioner-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("Video.sgvideo");
        std::fs::write(&rec, b"").unwrap();
        for f in ["notes.txt", "Song.mp3", "Interview.mp4", "Logo.png"] {
            std::fs::write(dir.join(f), b"").unwrap();
        }
        let ok = |f: &str| wanted(&dir.join(f).to_string_lossy()).is_some();
        assert!(ok("Video.sgvideo") && ok("Song.mp3") && ok("Interview.mp4"));
        assert!(!ok("notes.txt") && !ok("Logo.png") && !ok("gone.sgvideo"));
        let mut c = Captioner::default();
        c.push(rec.clone(), false);
        c.push(rec.clone(), true);
        assert_eq!(c.queue.len(), 1);
        assert!(c.queue[0].1, "asked again, it's heard again");
        // While recording nothing starts.
        c.tick(true);
        assert!(c.running.is_none() && c.busy());
        // A file that went meanwhile is let go.
        std::fs::remove_file(&rec).unwrap();
        c.tick(false);
        assert!(!c.busy());
        assert_eq!(sidecars(&rec), (dir.join("Video.srt"), dir.join("Video.txt")));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn progress_is_told_every_percent_and_an_error_is_kept() {
        let dir = std::env::temp_dir().join(format!("sgc-progress-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let of = dir.join("Song.mp3");
        let mut p = Progress::new(&of);
        let file = p.file.clone();
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "0");
        p.tell(0.005);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "0", "less than a percent isn't told");
        p.tell(0.42);
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "420");
        p.end(&Err("it has no sound".into()));
        assert_eq!(std::fs::read_to_string(&file).unwrap(), "error: it has no sound");
        let p = Progress::new(&of);
        p.end(&Ok(Some(3)));
        assert!(!file.exists(), "done, it goes");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn sound_becomes_a_recording_of_the_computers_sound() {
        let tone: Vec<i16> = (0..48_000 * 3)
            .flat_map(|i| {
                let v = ((i as f32 * 0.05).sin() * 9000.0) as i16;
                [v, v]
            })
            .collect();
        let rec = sound_recording(&tone, 3_000_000).unwrap();
        let s = rec.header.streams.first().unwrap();
        assert_eq!((s.kind, s.channels), (StreamKind::SystemSound, 1));
        assert_eq!(rec.duration_us, 3_000_000);
        let heard = rec.sound(1, 48_000, 100);
        assert!(heard.iter().any(|v| v.unsigned_abs() > 4000), "the sound is in it");
        assert!(sound_recording(&[], 0).is_ok());
    }
}
