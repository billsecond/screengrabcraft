//! Captions of Video Production recordings, made on this computer after each recording.
//!
//! They're made by a program of their own: this one again, started as
//! `screengrabcraft --transcribe <recording>` ([`run`]), at the lowest priority, on one thread
//! (about a twelfth of the recording's length). That keeps the service small (the model and
//! its working memory go when the captions are made) and out of harm's way, whatever a model
//! file or a recording holds. One is made at a time, never while recording: a recording
//! starting stops the one being made, and it's made again after.
//!
//! The captions go into the recording (the editor shows them on its timeline) and beside it,
//! as `<name>.srt` and `<name>.txt`. Nothing of what was said is logged.

use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitCode, Stdio};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

use screengrabcraft_desktop::{models, store};
use screengrabcraft_media::captions::{self, Speaker};
use screengrabcraft_media::recording::{self, FileSource, Recording};

/// The argument the service starts the program that makes captions with.
pub const ARG: &str = "--transcribe";

/// Threads to hear with: one. Measured on ten minutes of speech, one thread took 51 s of the
/// processor (and 51 s to wait for), two took 61 s (31 s to wait for), and 107 s with the
/// computer busy, waiting on each other. One costs the least, and never more than one core.
pub fn threads() -> usize {
    1
}

/// Recordings waiting for captions, and the one being made.
#[derive(Default)]
pub struct Captioner {
    queue: VecDeque<PathBuf>,
    running: Option<(PathBuf, Child, Instant)>,
}

fn name(path: &Path) -> String {
    path.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()
}

/// Starts the program that makes a recording's captions.
fn spawn(path: &Path) -> std::io::Result<Child> {
    let exe = std::env::current_exe()?;
    let n = threads().to_string();
    // candle sizes its pools from these when it starts: they can only be set for a new program.
    let quiet = |mut c: Command| {
        c.arg(ARG).arg(path).env("CANDLE_NUM_THREADS", &n).env("RAYON_NUM_THREADS", &n);
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
    /// A recording to make captions of (once: asking again while it waits changes nothing).
    pub fn push(&mut self, path: PathBuf) {
        let busy = self.running.as_ref().is_some_and(|(p, _, _)| *p == path);
        if !busy && !self.queue.contains(&path) {
            log::info!("captions of {} to be made", name(&path));
            self.queue.push_back(path);
        }
    }

    /// Whether captions are being made or waiting.
    pub fn busy(&self) -> bool {
        self.running.is_some() || !self.queue.is_empty()
    }

    /// Looked at every frame: starts the next when nothing is recorded, stops the one being made
    /// when a recording is (it's made again after).
    pub fn tick(&mut self, recording: bool) {
        if let Some((path, child, started)) = &mut self.running {
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
                    let path = path.clone();
                    self.running = None;
                    self.queue.push_front(path);
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
        while let Some(path) = self.queue.pop_front() {
            if !path.is_file() {
                continue;
            }
            match spawn(&path) {
                Ok(child) => {
                    log::info!("making the captions of {} ({} threads, low priority)", name(&path), threads());
                    self.running = Some((path, child, Instant::now()));
                    return;
                }
                Err(e) => log::warn!("the captions of {} could not be started: {e}", name(&path)),
            }
        }
    }

    /// The service is stopping: so does the one being made (it's made again when asked).
    pub fn stop(&mut self) {
        if let Some((path, mut child, _)) = self.running.take() {
            let _ = child.kill();
            let _ = child.wait();
            log::info!("captions of {} stopped with the service", name(&path));
        }
    }
}

/// Whether `path` names a recording to make captions of (from the editor: a file that is
/// there, a recording by its name).
pub fn wanted(path: &str) -> Option<PathBuf> {
    let p = PathBuf::from(path);
    let ok = p.extension().is_some_and(|e| e.eq_ignore_ascii_case(recording::EXT)) && p.is_file();
    ok.then_some(p)
}

/// The files beside a recording the captions are written to: `.srt` and `.txt`.
pub fn sidecars(path: &Path) -> (PathBuf, PathBuf) {
    (path.with_extension("srt"), path.with_extension("txt"))
}

/// Hears a recording and writes its captions; the number of lines, or none when there was
/// nothing to hear.
fn make(path: &Path) -> Result<Option<usize>, String> {
    let rec = Recording::open(Box::new(FileSource::open(path).map_err(|e| e.to_string())?)).map_err(|e| e.to_string())?;
    if !rec.header.streams.iter().any(|s| Speaker::of(s.kind).is_some()) {
        log::info!("{} has no sound to hear", name(path));
        return Ok(None);
    }
    let model_path = models::speech_model()?;
    let started = Instant::now();
    let bytes = models::read_model(&model_path, screengrabcraft_speech::MAX_MODEL_BYTES as u64)?;
    let mut model = screengrabcraft_speech::load(&bytes).map_err(|e| e.to_string())?;
    drop(bytes);
    log::info!("speech model {} read in {:.2} s", model.name, started.elapsed().as_secs_f32());
    let secs = rec.duration_us as f32 / 1e6;
    let heard = Instant::now();
    let mut told = 0u32;
    let mut progress = |p: f32| {
        let tenth = (p * 10.0) as u32;
        if tenth > told {
            told = tenth;
            log::info!("{}0 % heard", tenth);
        }
    };
    let caps = screengrabcraft_speech::transcribe(&mut model, &rec, &mut progress, &AtomicBool::new(false)).map_err(|e| e.to_string())?;
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

/// The program that makes a recording's captions (`--transcribe <recording>`).
pub fn run(path: &str) -> ExitCode {
    let path = Path::new(path);
    log::info!("making the captions of {} ({} threads)", name(path), std::env::var("CANDLE_NUM_THREADS").unwrap_or_else(|_| "all".into()));
    match make(path) {
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
    fn a_recording_is_asked_for_once_and_only_a_recording() {
        let dir = std::env::temp_dir().join(format!("sgc-captioner-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let rec = dir.join("Video.sgvideo");
        std::fs::write(&rec, b"").unwrap();
        std::fs::write(dir.join("notes.txt"), b"").unwrap();
        assert_eq!(wanted(&rec.to_string_lossy()), Some(rec.clone()));
        assert_eq!(wanted(&dir.join("notes.txt").to_string_lossy()), None);
        assert_eq!(wanted(&dir.join("gone.sgvideo").to_string_lossy()), None);
        let mut c = Captioner::default();
        c.push(rec.clone());
        c.push(rec.clone());
        assert_eq!(c.queue.len(), 1);
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
}
