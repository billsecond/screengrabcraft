//! ScreenGrabCraft Editor in the browser.
//!
//! The same editor as the desktop program, through eframe's web runner (WebGPU where available,
//! WebGL2 otherwise). Build with `trunk build --release` in this directory (output in
//! `dist/web`). A browser can't capture the screen the way the capture service does, so pictures
//! come in through Open, a drop on the page, or `?demo`; Save and Export download.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_arch = "wasm32")]
fn main() {
    web::start();
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    eprintln!("screengrabcraft-web only runs in the browser: build it with `trunk build --release` in apps/screengrabcraft-web");
}
