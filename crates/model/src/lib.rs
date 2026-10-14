//! ScreenGrabCraft's data: a captured picture with editable annotations on it ([`Document`]),
//! the annotations' shapes, styles and geometry ([`object`]), and the settings the capture
//! service and the editor share ([`settings`]).
//!
//! Layer L1: no I/O, no drawing, no platform code; it builds for the browser.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod color;
pub mod document;
pub mod geom;
pub mod object;
pub mod raster;
pub mod settings;

pub use color::Color;
pub use document::{DocError, Document, Meta, Order};
pub use geom::{IRect, Pt, Rect};
pub use object::{ArrowHeads, Handle, Kind, Object, ObjectId, Shape, StampKind, Style, TextAlign};
pub use raster::{MAX_PIXELS, MAX_SIDE, Raster, RasterError};
pub use settings::{AfterCapture, CaptureMode, EditorSettings, Hotkey, Hotkeys, ImageFormat, Settings, Stamp, Theme};

/// The program's name, as people see it.
pub const APP_NAME: &str = "ScreenGrabCraft";
/// The editor's name, as people see it.
pub const EDITOR_NAME: &str = "ScreenGrabCraft Editor";
/// The project file's extension: a picture with its annotations still editable.
pub const PROJECT_EXT: &str = "sgcraft";
/// The version, from the workspace's `Cargo.toml`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
/// Where the project lives.
pub const HOMEPAGE: &str = "https://github.com/billsecond/screengrabcraft";
