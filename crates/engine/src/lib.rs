//! ScreenGrabCraft's editing engine: a [`Session`] (the document, its selection, undo and
//! redo) and the [`registry`] of commands every editing action goes through.
//!
//! Layer L4: no I/O and no UI; it builds for the browser.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod registry;
pub mod session;

pub use registry::{COMMANDS, CommandSpec, describe, find, merge_style, parse_object};
pub use session::Session;

#[derive(Debug, thiserror::Error)]
pub enum EngineError {
    #[error("there is no command `{0}`")]
    Unknown(String),
    #[error("`{0}` can't be used now")]
    Disabled(String),
    #[error("bad parameters: {0}")]
    Params(String),
    #[error("there is no annotation with id {0}")]
    NoObject(u64),
    #[error("{0}")]
    Failed(String),
    #[error("`{0}` failed unexpectedly; nothing was changed")]
    Panicked(String),
    #[error(transparent)]
    Doc(#[from] screengrabcraft_model::DocError),
    #[error(transparent)]
    Picture(#[from] screengrabcraft_model::RasterError),
}

pub type Result<T> = std::result::Result<T, EngineError>;

#[cfg(test)]
mod tests;
