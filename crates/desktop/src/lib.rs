//! What ScreenGrabCraft's two desktop programs (the capture service and the editor) share:
//! where things are kept ([`paths`], [`store`]), the log ([`logging`]), one running copy each
//! ([`instance`]), the clipboard ([`clipboard`]), the persistent sign-in service and its
//! supervisor ([`service`]), starting the other program ([`launch`]), the graphics backend on
//! Windows ([`graphics`]), the tray icon ([`tray_icon`]), local time ([`time`]) and the version
//! line ([`version`]).
//!
//! Layer L6, desktop only.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod clipboard;
pub mod graphics;
pub mod instance;
pub mod launch;
pub mod logging;
pub mod paths;
pub mod service;
pub mod store;
pub mod time;
pub mod tray_icon;
pub mod version;
