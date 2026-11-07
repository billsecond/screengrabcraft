//! What the editor asks of the program hosting it: the clipboard, file dialogs, files, the
//! recent captures and the capture service. The desktop program and the browser build each
//! provide their own; anything a host can't do has a default that says so.

use screengrabcraft_media::recording::{Recording, Source};
use screengrabcraft_media::timeline::Timeline;
use screengrabcraft_model::{CaptureMode, EditorSettings, Raster, Stamp};
use screengrabcraft_render::matte::Matting;

pub use crate::autosave::{SaveJob, Saved};

/// A file the owner picked to open (or dropped on the window).
#[derive(Clone, Debug)]
pub struct OpenedFile {
    pub name: String,
    /// Where it is, when the host has paths (not in a browser).
    pub path: Option<String>,
    pub bytes: Vec<u8>,
}

/// What a file dialog came back with.
#[derive(Clone, Debug)]
pub enum DialogResult {
    Opened(Vec<OpenedFile>),
    /// A path to save to (the editor writes the file).
    SaveTo(String),
    Cancelled,
}

/// A capture in the recent strip.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct RecentItem {
    pub path: String,
    pub name: String,
    /// When it was last written, in milliseconds since the epoch (a picture written again has
    /// a new thumbnail).
    pub modified: u64,
    /// That day, locally, as `2026-11-07` (empty when the host can't tell): All captures groups
    /// by it.
    pub day: String,
    /// That time of day, locally, as `14:05` (or empty).
    pub time: String,
}

/// File types offered in a save dialog: a name and its extensions.
pub type Filter = (&'static str, &'static [&'static str]);

/// A video the host is making (an MP4 from a recording, as edited).
#[derive(Clone, Default)]
pub struct VideoExport {
    /// 0 to 1000.
    pub progress: std::sync::Arc<std::sync::atomic::AtomicU32>,
    /// Set to stop it; nothing is left behind.
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    /// The file made, or why not, once it's done.
    pub done: std::sync::Arc<std::sync::Mutex<Option<Result<String, String>>>>,
}

/// What to make a video from.
#[derive(Clone)]
pub struct VideoJob {
    pub recording: std::sync::Arc<Recording>,
    pub timeline: Timeline,
    /// Where it's wanted; the host picks a free name beside it if that one is taken.
    pub out: String,
    /// The largest it's made.
    pub max_size: (u32, u32),
    pub fps: u32,
}

/// Sound the host is playing; dropping it stops it.
pub trait SoundOut {
    /// Where on the timeline the sound heard now is (µs), when the device says.
    fn position_us(&self) -> Option<u64>;
}

pub trait Services {
    /// Puts a picture on the clipboard.
    fn copy_image(&self, _picture: &Raster) -> Result<(), String> {
        Err("copying pictures is not available here".to_string())
    }

    /// A picture on the clipboard, if there is one.
    fn paste_image(&self) -> Option<Raster> {
        None
    }

    /// Asks for a file to open; the answer comes through [`Services::poll_dialog`].
    fn request_open(&self) {}

    /// Asks where to save; the answer comes through [`Services::poll_dialog`].
    fn request_save(&self, _suggested_name: &str, _filters: &[Filter]) {}

    /// The answer of the last dialog, once there is one.
    fn poll_dialog(&self) -> Option<DialogResult> {
        None
    }

    /// Whether the host saves to paths (else saving is a download, [`Services::download`]).
    fn has_paths(&self) -> bool {
        true
    }

    fn read_file(&self, _path: &str) -> Result<Vec<u8>, String> {
        Err("files are not available here".to_string())
    }

    /// Writes a whole file (by way of a temporary one, so a crash never leaves half of it).
    fn write_file(&self, _path: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("files are not available here".to_string())
    }

    /// Starts an automatic save. A host with threads writes it on one of its own and answers
    /// `None`; the result then comes through [`Services::poll_saved`]. By default it is written
    /// here and now, through [`Services::write_file`].
    fn start_save(&self, job: SaveJob) -> Option<Saved> {
        Some(job.run(&|path, bytes| self.write_file(path, bytes)))
    }

    /// A save [`Services::start_save`] took on that has finished.
    fn poll_saved(&self) -> Option<Saved> {
        None
    }

    /// A new file in the captures folder for a picture that has none yet, named the way
    /// captures are (`source` is what the picture is of, for the name).
    fn new_picture_path(&self, _source: &str) -> Result<String, String> {
        Err("there is no captures folder here".to_string())
    }

    /// Moves a file to the Recycle Bin (the Trash), never deleting it for good. A file that
    /// isn't there is let be.
    fn trash(&self, _path: &str) -> Result<(), String> {
        Err("files can't be moved to the Recycle Bin here".to_string())
    }

    /// Hands a file to the browser to download.
    fn download(&self, _name: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("downloads are not available here".to_string())
    }

    /// The recent captures, newest first.
    fn recent(&self) -> Vec<RecentItem> {
        Vec::new()
    }

    /// Today, locally, as `2026-11-07` (empty when the host can't tell).
    fn today(&self) -> String {
        String::new()
    }

    /// A small picture of a recent capture as it was at `modified` (none until it has been
    /// made).
    fn thumbnail(&self, _path: &str, _modified: u64) -> Option<Raster> {
        None
    }

    /// Shows a folder (the captures folder when `path` is none) in the file manager.
    fn open_folder(&self, _path: Option<&str>) {}

    /// Asks the capture service for a new capture.
    fn new_capture(&self, _mode: CaptureMode) -> Result<(), String> {
        Err("capturing is done by the ScreenGrabCraft capture service on the desktop".to_string())
    }

    fn save_settings(&self, _settings: &EditorSettings) {}

    /// Now, in local time, for naming files.
    fn now(&self) -> Stamp {
        Stamp::default()
    }

    /// Opens a web address in the browser.
    fn open_url(&self, _url: &str) {}

    /// A file dropped on the window that only the host can read (in a browser, reading is
    /// asynchronous); it comes back through [`Services::poll_dialog`] as opened.
    fn read_dropped(&self, _file: egui::DroppedFileHandle) {}

    // ---- video ----

    /// A video recording file, read where it's needed (they can be large).
    fn open_recording(&self, _path: &str) -> Result<Box<dyn Source>, String> {
        Err("video recordings are opened by the editor on the desktop".to_string())
    }

    /// Adds bytes to the end of a file (a recording's edits).
    fn append_file(&self, _path: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("files are not available here".to_string())
    }

    /// Starts making an MP4 (on a thread of the host's).
    fn export_video(&self, _job: VideoJob) -> Result<VideoExport, String> {
        Err("videos are made by the editor on the desktop".to_string())
    }

    /// Plays the edited sound from `from_us`, until it's dropped.
    fn play_sound(&self, _recording: std::sync::Arc<Recording>, _timeline: Timeline, _from_us: u64) -> Option<Box<dyn SoundOut>> {
        None
    }

    /// Opens a file in the program the system uses for it (a video in its player).
    fn open_with_system(&self, _path: &str) {}

    /// The model Remove Background runs (on a thread of the editor's own; reading its file is
    /// left to the model's first use), or why there is none here.
    fn background_model(&self) -> Result<std::sync::Arc<dyn Matting>, String> {
        Err("Remove Background needs ScreenGrabCraft Editor on the desktop: its model doesn't run in the browser".to_string())
    }
}

/// A host with nothing, not even paths (nothing is saved by itself): for tests.
pub struct NoServices;

impl Services for NoServices {
    fn has_paths(&self) -> bool {
        false
    }
}
