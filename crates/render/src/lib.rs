//! Drawing and files: the annotations drawn onto the picture ([`draw`]), text from the
//! bundled fonts ([`text`]), stamps ([`stamp`]), the whole-picture changes and effects
//! ([`effects`], [`adjust`], [`frame`], [`marks`]), reading and writing pictures ([`codec`], [`pdf`]), the project file
//! ([`project`]), and the two capture helpers that are pure picture work: joining a scrolling
//! capture ([`stitch`]) and writing animated GIFs ([`anim`]).
//!
//! Layer L2: Rust only (tiny-skia, ab_glyph, image), no I/O beyond byte buffers, builds for the
//! browser.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod adjust;
pub mod anim;
pub mod canvas;
pub mod codec;
pub mod cut;
pub mod draw;
pub mod effects;
pub mod frame;
pub mod lottie;
pub mod marks;
pub mod matte;
pub mod pdf;
pub mod pointer;
pub mod project;
pub mod stamp;
pub mod stitch;
pub mod text;

pub use codec::{CodecError, decode, encode};
pub use draw::{Options, Renderer, render};
pub use tiny_skia::Pixmap;
