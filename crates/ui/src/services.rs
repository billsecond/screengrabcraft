//! What the editor asks of the program hosting it: the clipboard, file dialogs, files, the
//! recent captures and the capture service. The desktop program and the browser build each
//! provide their own; anything a host can't do has a default that says so.

use screengrabcraft_model::{CaptureMode, EditorSettings, Raster, Stamp};

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
#[derive(Clone, Debug, PartialEq)]
pub struct RecentItem {
    pub path: String,
    pub name: String,
    /// Seconds since the epoch.
    pub modified: u64,
}

/// File types offered in a save dialog: a name and its extensions.
pub type Filter = (&'static str, &'static [&'static str]);

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

    fn write_file(&self, _path: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("files are not available here".to_string())
    }

    /// Hands a file to the browser to download.
    fn download(&self, _name: &str, _bytes: &[u8]) -> Result<(), String> {
        Err("downloads are not available here".to_string())
    }

    /// The recent captures, newest first.
    fn recent(&self) -> Vec<RecentItem> {
        Vec::new()
    }

    /// A small picture of a recent capture (none until it has been made).
    fn thumbnail(&self, _path: &str) -> Option<Raster> {
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
}

/// A host with nothing: for tests.
pub struct NoServices;

impl Services for NoServices {}
