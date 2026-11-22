//! What the editor asks of the program hosting it: the clipboard, file dialogs, files, the
//! recent captures and the capture service. The desktop program and the browser build each
//! provide their own; anything a host can't do has a default that says so.

use screengrabcraft_media::captions::{Captions, Cue};
use screengrabcraft_media::recording::{Recording, Source};
use screengrabcraft_media::timeline::Timeline;
use screengrabcraft_model::{CaptureMode, EditorSettings, Library, Raster, Stamp};
use screengrabcraft_render::matte::Matting;

pub use crate::autosave::{SaveJob, Saved};

/// Reads the text in a picture (Grab Text, Hide Personal Details). It runs on a thread of the
/// editor's own.
pub use screengrabcraft_engine::ReadText;
/// A line of text read off a picture, where it is on it (in the picture's pixels), and its words.
pub use screengrabcraft_model::{TextLine, TextWord};

/// A file the owner picked to open (or dropped on the window).
#[derive(Clone, Debug)]
pub struct OpenedFile {
    pub name: String,
    /// Where it is, when the host has paths (not in a browser).
    pub path: Option<String>,
    pub bytes: Vec<u8>,
}

/// The largest subtitle file read (bytes).
pub const MAX_SUBTITLES: u64 = 16 * 1024 * 1024;

/// What a file dialog came back with.
#[derive(Clone, Debug)]
pub enum DialogResult {
    Opened(Vec<OpenedFile>),
    /// A path to save to (the editor writes the file).
    SaveTo(String),
    /// Files to bring into the video editor's media bin (paths; they're read where they are).
    Import(Vec<String>),
    /// Video or sound files to transcribe (paths).
    Transcribe(Vec<String>),
    /// A subtitle file (`.srt`, `.vtt`) to bring into the video editor as captions, read.
    Subtitles(Vec<OpenedFile>),
    /// Video recordings to make videos of, together (paths).
    Batch(Vec<String>),
    /// Pictures to make a new video of, one after another (paths).
    PicturesVideo(Vec<String>),
    /// Pictures for Batch Changes (paths; they're read one at a time).
    Pictures(Vec<String>),
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

/// What a video is made as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VideoFormat {
    /// MP4 with H.264 pictures and AAC sound: every player opens it.
    Mp4,
    /// MP4 with AV1 pictures and Opus sound: smaller, but older players (Windows Media Player
    /// among them) can't open it.
    Mp4Av1,
    /// An animated GIF: no sound, smaller pictures; for short clips.
    Gif,
    /// A see-through video: a QuickTime movie of PNG pictures, clear wherever there's nothing
    /// (the screen hidden, the camera's background taken away); no sound. For putting over
    /// another video in a video editor.
    Mov,
    /// Just the sound, as M4A (AAC): small, every player opens it (Windows' own encoder makes it).
    M4a,
    /// Just the sound, as WAV: as heard, not compressed.
    Wav,
}

impl VideoFormat {
    /// Every kind there is, as the Export window lists them (those this computer can't make are
    /// greyed out there).
    pub const ALL: [VideoFormat; 6] = [VideoFormat::Mp4, VideoFormat::Mp4Av1, VideoFormat::Gif, VideoFormat::Mov, VideoFormat::M4a, VideoFormat::Wav];

    pub fn label(self) -> &'static str {
        match self {
            VideoFormat::Mp4 => "MP4 video: plays everywhere (H.264)",
            VideoFormat::Mp4Av1 => "MP4 video: smaller, newer players only (AV1)",
            VideoFormat::Gif => "Animated GIF: no sound, for short clips",
            VideoFormat::Mov => "See-through video: MOV of PNG pictures, no sound",
            VideoFormat::M4a => "Just the sound: M4A (AAC, small)",
            VideoFormat::Wav => "Just the sound: WAV (as heard, not compressed)",
        }
    }

    /// Why this computer can't make it (shown on it, greyed out).
    pub fn why_not(self) -> &'static str {
        match self {
            VideoFormat::Mp4 => "H.264 videos are made by Windows' own encoder: on Windows only (AV1 plays in newer players)",
            VideoFormat::M4a => "M4A sound is made by Windows' own encoder: on Windows only (WAV is made everywhere)",
            _ => "Not made here",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            VideoFormat::Mp4 | VideoFormat::Mp4Av1 => "mp4",
            VideoFormat::Gif => "gif",
            VideoFormat::Mov => "mov",
            VideoFormat::M4a => "m4a",
            VideoFormat::Wav => "wav",
        }
    }

    /// Its file type in a save dialog.
    pub fn filter(self) -> Filter {
        match self {
            VideoFormat::Mp4 | VideoFormat::Mp4Av1 => ("MP4 video", &["mp4"]),
            VideoFormat::Gif => ("Animated GIF", &["gif"]),
            VideoFormat::Mov => ("QuickTime movie", &["mov"]),
            VideoFormat::M4a => ("M4A sound", &["m4a"]),
            VideoFormat::Wav => ("WAV sound", &["wav"]),
        }
    }

    /// Whether it has a sound track (and so whether subtitles beside it make sense).
    pub fn has_sound(self) -> bool {
        !matches!(self, VideoFormat::Gif | VideoFormat::Mov)
    }

    /// Whether it has pictures (and so whether captions can be put on them).
    pub fn has_picture(self) -> bool {
        !matches!(self, VideoFormat::M4a | VideoFormat::Wav)
    }
}

/// What to make a video from.
#[derive(Clone)]
pub struct VideoJob {
    pub recording: std::sync::Arc<Recording>,
    pub timeline: Timeline,
    /// Where it's wanted.
    pub out: String,
    /// Whether `out` was picked in a save dialog (a file there is replaced); else the host picks
    /// a free name beside it if that one is taken.
    pub replace: bool,
    pub format: VideoFormat,
    /// The largest it's made.
    pub max_size: (u32, u32),
    pub fps: u32,
    /// The captions as they fall on the edited video, if there are some: burned into its
    /// pictures if `burn`, written beside it as subtitles (`.srt`, the video's name) if
    /// `subtitles`.
    pub captions: Option<Vec<Cue>>,
    pub burn: bool,
    pub subtitles: bool,
    /// The media bin's pictures and sound, for the layers' items.
    pub media: std::sync::Arc<dyn screengrabcraft_media::layer_draw::MediaStore>,
}

/// What the owner keeps to use again, each kind in a folder of its own in the settings folder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Kept {
    /// The video editor's templates of the owner's: the layers' items kept under a name.
    VideoTemplates,
    /// The video editor's themes of the owner's: colours and a face for the titles.
    VideoThemes,
}

impl Kept {
    /// Its folder, in the settings folder.
    pub fn folder(self) -> &'static str {
        match self {
            Kept::VideoTemplates => "video-templates",
            Kept::VideoThemes => "video-themes",
        }
    }
}

/// Why nothing is kept here.
pub const NOT_KEPT: &str = "Templates and themes of your own are kept by ScreenGrabCraft Editor on the desktop";

/// Why narration can't be recorded here.
pub const NO_NARRATION: &str = "Narration is recorded by ScreenGrabCraft Editor on the desktop, where it can use a microphone";

/// A narration being recorded: the microphone on until it's finished (dropping it turns it off
/// and keeps nothing).
pub trait NarrationTake: Send {
    /// How loud it is just now, 0 to 1 (for a meter).
    fn level(&self) -> f32;
    /// Turns the microphone off; the take written to a file of its own (a WAV beside the
    /// recording): where, and how long it is (µs).
    fn finish(self: Box<Self>) -> Result<(String, u64), String>;
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

    /// Opens a file in the other editor, a window of its own: a recording (`video`) in the video
    /// editor, a picture or a project in the picture editor. False when the host has the one
    /// window only (a browser, the tests): the file is opened here instead.
    fn open_in_other_editor(&self, _path: &str, _video: bool) -> bool {
        false
    }

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

    /// A new file in the captures folder for a video recording made here (a video of pictures),
    /// named the way captures are (`source` is what it's of, for the name).
    fn new_recording_path(&self, _source: &str) -> Result<String, String> {
        Err("new videos are made by ScreenGrabCraft Editor on the desktop".to_string())
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

    /// Asks for pictures for Batch Changes; the answer comes through [`Services::poll_dialog`] as
    /// [`DialogResult::Pictures`].
    fn request_pictures(&self) {}

    /// Whether there's a file at `path` (Batch Changes never writes over one).
    fn file_exists(&self, _path: &str) -> bool {
        false
    }

    /// Prints a picture (`name` is what it's called): the system shows its printing window.
    fn print(&self, _picture: &Raster, _name: &str) -> Result<(), String> {
        Err("Printing needs ScreenGrabCraft on the desktop".to_string())
    }

    /// Whether pictures can be printed here, or why not (Print is greyed out, saying so).
    fn can_print(&self) -> Result<(), String> {
        Err("Printing needs ScreenGrabCraft on the desktop".to_string())
    }

    /// Whether a capture can be handed to a program the owner chooses, or why not (Open With is
    /// greyed out, saying so).
    fn can_open_with(&self) -> Result<(), String> {
        Err("Open With needs ScreenGrabCraft on the desktop".to_string())
    }

    /// The picture as shown (`name` is what it's called) handed to a program the owner chooses, in
    /// the system's own window for choosing one.
    fn open_picture_with(&self, _picture: &Raster, _name: &str) -> Result<(), String> {
        Err("Open With needs ScreenGrabCraft on the desktop".to_string())
    }

    /// A capture's file handed to a program the owner chooses.
    fn open_file_with(&self, _path: &str) -> Result<(), String> {
        Err("Open With needs ScreenGrabCraft on the desktop".to_string())
    }

    /// What's known of these captures (their tags, whether each is a favourite, the program each
    /// was taken from), under their paths as given.
    fn library(&self, _paths: &[String]) -> Library {
        Library::default()
    }

    /// Runs a library command (`library.tag`, `library.favorite`, …) on the library as the host
    /// keeps it (the editor runs it on its own copy as well).
    fn run_library(&self, _id: &str, _params: &serde_json::Value) -> Result<serde_json::Value, String> {
        Err("tags and favourites are kept by ScreenGrabCraft on the desktop".to_string())
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

    /// Shows a file selected in its folder in the file manager, or a folder (the captures folder
    /// when `path` is none).
    fn open_folder(&self, _path: Option<&str>) {}

    /// Whether there is a capture service to ask (not in a browser).
    fn can_capture(&self) -> bool {
        false
    }

    /// Asks the capture service for a new capture.
    fn new_capture(&self, _mode: CaptureMode) -> Result<(), String> {
        Err("capturing is done by the ScreenGrabCraft capture service on the desktop".to_string())
    }

    fn save_settings(&self, _settings: &EditorSettings) {}

    /// Whether the owner's templates and themes are kept here (in the settings folder).
    fn can_keep(&self) -> bool {
        false
    }

    /// The files of a kind kept (a name each, and what's in it), in the order of their names.
    fn kept(&self, _kind: Kept) -> Vec<(String, Vec<u8>)> {
        Vec::new()
    }

    /// Keeps a file of a kind under a name; one kept under that name before is replaced.
    fn keep(&self, _kind: Kept, _name: &str, _bytes: &[u8]) -> Result<(), String> {
        Err(NOT_KEPT.to_string())
    }

    /// Lets go of the file of a kind kept under a name.
    fn forget(&self, _kind: Kept, _name: &str) -> Result<(), String> {
        Err(NOT_KEPT.to_string())
    }

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

    /// What videos can be made here, the one to offer first first (none in a browser).
    fn video_formats(&self) -> Vec<VideoFormat> {
        Vec::new()
    }

    /// Asks for files to bring into the video editor's media bin (pictures, sound, recordings);
    /// the answer comes through [`Services::poll_dialog`] as [`DialogResult::Import`].
    fn request_import(&self) {}

    /// Starts reading a file for the media bin (on a thread of the host's).
    fn load_media(&self, _path: &str) -> Result<screengrabcraft_media::media_store::MediaJob, String> {
        Err("media is brought in by the editor on the desktop".to_string())
    }

    /// Starts making a video (on a thread of the host's).
    fn export_video(&self, _job: VideoJob) -> Result<VideoExport, String> {
        Err("videos are made by the editor on the desktop".to_string())
    }

    /// Plays the edited sound (the layers' media's too, from `media`) from `from_us`, `rate` times
    /// as fast as recorded (1, 2, 4 or 8; faster at its own pitch), or at -1 backwards (the sound
    /// turned round), until it's dropped.
    fn play_sound(
        &self,
        _recording: std::sync::Arc<Recording>,
        _timeline: Timeline,
        _media: std::sync::Arc<dyn screengrabcraft_media::layer_draw::MediaStore>,
        _from_us: u64,
        _rate: i32,
    ) -> Option<Box<dyn SoundOut>> {
        None
    }

    /// Whether narration can be recorded here (there are microphones to ask for).
    fn can_narrate(&self) -> bool {
        false
    }

    /// The microphones narration can be recorded with, by name. Listing them turns none on.
    fn microphones(&self) -> Vec<String> {
        Vec::new()
    }

    /// Turns a microphone on (by name; none: the one Video Production records with, as the
    /// Recorder has it, else the system's default) to record narration for the recording at
    /// `beside`: the take is written beside it when it's finished.
    fn record_narration(&self, _mic: Option<&str>, _beside: &str) -> Result<Box<dyn NarrationTake>, String> {
        Err(NO_NARRATION.to_string())
    }

    /// Opens a file in the program the system uses for it (a video in its player).
    fn open_with_system(&self, _path: &str) {}

    /// A recording's captions when the file beside it that says they're made (`.srt`) has
    /// changed since `seen` (a time of the host's, 0 at first): that time, and the recording's
    /// captions if it has some. Cheap to ask again and again.
    fn poll_captions(&self, _recording: &str, _seen: u64) -> Option<(u64, Option<Captions>)> {
        None
    }

    /// Asks for the captions of a recording, a sound file or a video to be made (by the capture
    /// service, in the background). A sound file's or a video's made before are kept.
    fn make_captions(&self, _path: &str) -> Result<(), String> {
        Err("captions are made by the ScreenGrabCraft capture service on the desktop".to_string())
    }

    /// [`Services::make_captions`], hearing it again even if its captions were made before.
    fn make_captions_again(&self, path: &str) -> Result<(), String> {
        self.make_captions(path)
    }

    /// How far the captions of a file being made have got (0 to 1), or why they couldn't be
    /// made; none when nothing is being made of it.
    fn captions_progress(&self, _path: &str) -> Option<Result<f32, String>> {
        None
    }

    /// The captions of a sound file or a video (in its own time) when they've been made or
    /// changed since `seen` (a time of the host's, 0 at first): that time, and the lines.
    fn poll_media_captions(&self, _path: &str, _seen: u64) -> Option<(u64, Vec<Cue>)> {
        None
    }

    /// Whether recordings, and what's put on their timelines, are transcribed by themselves
    /// (Settings, Recording).
    fn auto_captions(&self) -> bool {
        false
    }

    /// Asks for pictures to make a new video of; the answer comes through
    /// [`Services::poll_dialog`] as [`DialogResult::PicturesVideo`].
    fn request_pictures_video(&self) {}

    /// Asks for video recordings to make videos of, together; the answer comes through
    /// [`Services::poll_dialog`] as [`DialogResult::Batch`].
    fn request_batch(&self) {}

    /// Asks for video or sound files to transcribe; the answer comes through
    /// [`Services::poll_dialog`] as [`DialogResult::Transcribe`].
    fn request_transcribe(&self) {}

    /// Asks for a subtitle file (`.srt` or `.vtt`) to bring in as the video's captions; the
    /// answer comes through [`Services::poll_dialog`] as [`DialogResult::Subtitles`], the file
    /// read (at most [`MAX_SUBTITLES`] bytes).
    fn request_subtitles(&self) {}

    /// The model Remove Background runs (on a thread of the editor's own; reading its file is
    /// left to the model's first use), or why there is none here.
    fn background_model(&self) -> Result<std::sync::Arc<dyn Matting>, String> {
        Err("Remove Background needs ScreenGrabCraft Editor on the desktop: its model doesn't run in the browser".to_string())
    }

    /// What reads the text in a picture (Grab Text), on this computer, or why there is none here.
    fn text_reader(&self) -> Result<std::sync::Arc<dyn ReadText>, String> {
        Err("Grab Text needs ScreenGrabCraft on the desktop: the browser has no text reader".to_string())
    }

    /// Whether the host keeps pictures of your own as stamps (in the editor's settings folder;
    /// the browser keeps none).
    fn keeps_own_stamps(&self) -> bool {
        false
    }

    /// The names of the pictures kept as stamps, oldest first.
    fn own_stamps(&self) -> Vec<String> {
        Vec::new()
    }

    /// A picture kept as a stamp, by its name.
    fn own_stamp(&self, _name: &str) -> Option<Raster> {
        None
    }

    /// Keeps a picture (a PNG file's bytes) as a stamp, and answers its name.
    fn add_own_stamp(&self, _png: &[u8]) -> Result<String, String> {
        Err("pictures of your own as stamps are kept on the computer: not here".to_string())
    }

    /// Takes a picture out of the stamps.
    fn remove_own_stamp(&self, _name: &str) -> Result<(), String> {
        Err("pictures of your own as stamps are kept on the computer: not here".to_string())
    }
}

/// A host with nothing, not even paths (nothing is saved by itself): for tests.
pub struct NoServices;

impl Services for NoServices {
    fn has_paths(&self) -> bool {
        false
    }
}
