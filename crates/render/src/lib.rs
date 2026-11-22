//! Drawing and files: the annotations drawn onto the picture ([`draw`]), text from the
//! bundled fonts ([`text`]), stamps ([`stamp`]), the whole-picture changes and effects
//! ([`effects`]), reading and writing pictures ([`codec`], [`pdf`]), the project file
//! ([`project`]), and the capture helpers that are pure picture work: joining a scrolling
//! capture ([`stitch`]), writing animated GIFs ([`anim`]) and cutting a capture to its shape
//! ([`area`]); QR codes read and made ([`qr`]).
//!
//! Layer L2: Rust only (tiny-skia, ab_glyph, image), no I/O beyond byte buffers, builds for the
//! browser.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod anim;
pub mod area;
pub mod canvas;
pub mod codec;
pub mod cut;
pub mod draw;
pub mod effects;
pub mod guide;
pub mod lottie;
pub mod matte;
pub mod pdf;
pub mod pointer;
pub mod project;
pub mod qr;
pub mod stamp;
pub mod stitch;
pub mod text;

pub use codec::{CodecError, decode, encode};
pub use draw::{Options, Renderer, render};
pub use tiny_skia::Pixmap;
