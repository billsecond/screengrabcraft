//! What ScreenGrabCraft's two desktop programs (the capture service and the editor) share:
//! where things are kept ([`paths`], [`store`]), the background model's file ([`models`]), the log
//! ([`logging`]), one running copy each ([`instance`]), the clipboard ([`clipboard`]), windows
//! shown and hidden without animation ([`cloak`]), the persistent sign-in service and its
//! supervisor ([`service`]), starting the other program
//! ([`launch`]), the graphics backend on Windows ([`graphics`]), the tray icon ([`tray_icon`]),
//! local time ([`time`]), the version line ([`version`]) and updates from GitHub's releases
//! ([`update`], the only network code).
//!
//! Layer L6, desktop only.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod clipboard;
pub mod cloak;
pub mod graphics;
pub mod instance;
pub mod launch;
pub mod logging;
pub mod models;
pub mod paths;
pub mod service;
pub mod store;
pub mod time;
pub mod tray_icon;
pub mod update;
pub mod version;
