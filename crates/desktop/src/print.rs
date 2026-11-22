//! Printing a picture: it's written to a file in the temporary folder, and Windows' own print verb
//! for pictures is asked to print it (Windows then shows its printing window, where the printer,
//! the paper and the copies are chosen). Elsewhere there's no such verb, and Print is greyed out.

use std::path::{Path, PathBuf};

use screengrabcraft_model::{ImageFormat, Raster};

/// Whether pictures can be printed here, or why not.
pub fn available() -> Result<(), String> {
    if cfg!(target_os = "windows") {
        Ok(())
    } else {
        Err("Printing needs Windows for now: save the picture and print it from another program".to_string())
    }
}

/// Where a picture to print is written: the temporary folder, under the picture's name (made safe
/// for a file).
fn print_file(name: &str) -> PathBuf {
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '(' | ')') { c } else { '-' }).take(80).collect();
    let safe = safe.trim();
    let stem = if safe.is_empty() { "Picture" } else { safe };
    std::env::temp_dir().join("ScreenGrabCraft").join("print").join(format!("{stem}.png"))
}

/// Prints a picture (`name` is what it's called, for the printing window and the file).
pub fn print(picture: &Raster, name: &str) -> Result<(), String> {
    available()?;
    let path = print_file(name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let bytes = screengrabcraft_render::encode(picture, ImageFormat::Png, 100).map_err(|e| e.to_string())?;
    crate::store::write_atomic(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    log::info!("printing {}x{} as {}", picture.width(), picture.height(), path.display());
    print_path(&path)
}

/// Hands a picture's file to the system's print verb.
#[cfg(target_os = "windows")]
fn print_path(path: &Path) -> Result<(), String> {
    use winsafe::prelude::Handle as _;
    let file = path.to_string_lossy().replace('/', "\\");
    // The printing window is another program's: this one, in use as it asks, lets it come to the
    // front.
    let _ = winsafe::AllowSetForegroundWindow(None);
    winsafe::HWND::NULL
        .ShellExecute("print", &file, None, None, winsafe::co::SW::SHOWNORMAL)
        .map_err(|e| format!("Windows couldn't print the picture: {e}"))
}

#[cfg(not(target_os = "windows"))]
fn print_path(_path: &Path) -> Result<(), String> {
    Err("Printing needs Windows for now".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_to_print_has_a_safe_name_in_the_temporary_folder() {
        let p = print_file("Capture 2026-11-21 at 10.15.png");
        assert!(p.starts_with(std::env::temp_dir()));
        assert_eq!(p.file_name().and_then(|n| n.to_str()), Some("Capture 2026-11-21 at 10-15-png.png"));
        assert_eq!(print_file("../../x").file_name().and_then(|n| n.to_str()), Some("------x.png"));
        assert_eq!(print_file("").file_name().and_then(|n| n.to_str()), Some("Picture.png"));
    }
}
