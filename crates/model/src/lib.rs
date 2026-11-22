//! ScreenGrabCraft's data: a captured picture with editable annotations on it ([`Document`]),
//! the annotations' shapes, styles and geometry ([`object`]), and the settings the capture
//! service and the editor share ([`settings`]).
//!
//! Layer L1: no I/O, no drawing, no platform code; it builds for the browser.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod color;
pub mod document;
pub mod geom;
pub mod guide;
pub mod library;
pub mod object;
pub mod presets;
pub mod quick;
pub mod raster;
pub mod settings;
pub mod timed;
pub mod text;

pub use color::Color;
pub use document::{Align, DocError, Document, Meta, Order};
pub use geom::{IRect, Pt, Rect};
pub use library::{CaptureInfo, Library};
pub use object::{
    ArrowHeads, BadgeShape, CalloutForm, FontName, Gradient, GradientKind, Handle, HeadShape, Kind, Object, ObjectId, Shape, ShapeOptions, StampKind,
    StepLabels, Style, TailSpot, TextAlign,
};
pub use quick::QuickStyle;
pub use raster::{MAX_PIXELS, MAX_SIDE, Raster, RasterError};
pub use settings::{
    AfterCapture, CaptureMode, CapturePreset, EditorSettings, FillTool, Hotkey, Hotkeys, ImageFormat, MonitorChoice, PresetArea, RecordArea,
    RecorderOptions, Settings, Stamp, Theme,
};
pub use timed::{TimedArea, TimedCaptures};
pub use text::{TextLine, TextWord};

/// The program's name, as people see it.
pub const APP_NAME: &str = "ScreenGrabCraft";
/// The editor's name, as people see it: the Image Editor, for pictures and screen captures.
pub const EDITOR_NAME: &str = "ScreenGrabCraft Image Editor";
/// The video editor's name (the same program, a window of its own for recordings).
pub const VIDEO_EDITOR_NAME: &str = "ScreenGrabCraft Video Editor";
/// The project file's extension: a picture with its annotations still editable.
pub const PROJECT_EXT: &str = "sgcraft";
/// The version, from the workspace's `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Where the project lives.
pub const HOMEPAGE: &str = "https://github.com/billsecond/screengrabcraft";
