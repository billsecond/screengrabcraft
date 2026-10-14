//! Pictures to and from the system clipboard.
//!
//! One clipboard handle is kept for the life of the program: on X11 and Wayland a program serves
//! what it copied for as long as its handle lives, so a picture copied by the capture service
//! stays pasteable after the capture window has gone.

use std::borrow::Cow;
use std::sync::{Mutex, OnceLock, PoisonError};

use screengrabcraft_model::Raster;

fn with<T>(f: impl FnOnce(&mut arboard::Clipboard) -> Result<T, String>) -> Result<T, String> {
    static CLIPBOARD: OnceLock<Mutex<Option<arboard::Clipboard>>> = OnceLock::new();
    let cell = CLIPBOARD.get_or_init(|| Mutex::new(None));
    let mut guard = cell.lock().unwrap_or_else(PoisonError::into_inner);
    if guard.is_none() {
        *guard = Some(arboard::Clipboard::new().map_err(|e| e.to_string())?);
    }
    match guard.as_mut() {
        Some(cb) => f(cb),
        None => Err("no clipboard".to_string()),
    }
}

/// Puts a picture on the clipboard.
pub fn copy_image(r: &Raster) -> Result<(), String> {
    with(|cb| {
        let image = arboard::ImageData { width: r.width() as usize, height: r.height() as usize, bytes: Cow::Borrowed(r.pixels()) };
        cb.set_image(image).map_err(|e| e.to_string())
    })
}

/// The picture on the clipboard, if there is one.
pub fn paste_image() -> Option<Raster> {
    with(|cb| {
        let img = cb.get_image().map_err(|e| e.to_string())?;
        let (w, h) = (u32::try_from(img.width).map_err(|e| e.to_string())?, u32::try_from(img.height).map_err(|e| e.to_string())?);
        Raster::new(w, h, img.bytes.into_owned()).map_err(|e| e.to_string())
    })
    .ok()
}

/// Puts text on the clipboard (a saved file's path).
pub fn copy_text(text: &str) -> Result<(), String> {
    with(|cb| cb.set_text(text.to_string()).map_err(|e| e.to_string()))
}
