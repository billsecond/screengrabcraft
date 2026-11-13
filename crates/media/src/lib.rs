//! Video: the recording file with a track per source ([`recording`]), the edits to it
//! ([`timeline`]), what was said in it ([`captions`]), and making MP4 files from pictures and
//! sound ([`mp4`], [`av1`], [`opus`], [`yuv`]).
//!
//! Layer L2: Rust only, no I/O beyond what it's handed. The encoders are not in the browser
//! build.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(not(target_arch = "wasm32"))]
pub mod av1;
pub mod captions;
#[cfg(target_os = "windows")]
pub mod clip;
pub mod compose;
#[cfg(not(target_arch = "wasm32"))]
pub mod export;
#[cfg(target_os = "windows")]
mod h264;
pub mod layer_draw;
pub mod layers;
pub mod media_store;
pub mod mp4;
#[cfg(not(target_arch = "wasm32"))]
pub mod opus;
pub mod pointer;
pub mod recording;
pub mod stock;
pub mod timeline;
pub mod yuv;

#[derive(Debug, thiserror::Error)]
pub enum MediaError {
    #[error("making the video: {0}")]
    Encode(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
}
