//! Open With: a capture handed to a program the owner chooses, in the system's own window for
//! choosing one (Windows' "Open with"). The picture as shown in the editor is written to a file in
//! the temporary folder first; a capture's own file is handed over as it is. Elsewhere there's no
//! such window, and Open With is greyed out.

use std::path::{Path, PathBuf};

use screengrabcraft_model::{ImageFormat, Raster};

/// Whether a capture can be opened with another program here, or why not.
pub fn available() -> Result<(), String> {
    if cfg!(target_os = "windows") { Ok(()) } else { Err("Open With needs Windows for now: Show in Folder, then open it from there".to_string()) }
}

/// Where a picture to open with another program is written: the temporary folder, under the
/// picture's name (made safe for a file).
fn picture_file(name: &str) -> PathBuf {
    let safe: String = name.chars().map(|c| if c.is_alphanumeric() || matches!(c, ' ' | '-' | '_' | '(' | ')') { c } else { '-' }).take(80).collect();
    let safe = safe.trim();
    let stem = if safe.is_empty() { "Picture" } else { safe };
    std::env::temp_dir().join("ScreenGrabCraft").join("open-with").join(format!("{stem}.png"))
}

/// The picture as shown (`name` is what it's called), written to a file and handed to the
/// program the owner chooses.
pub fn open_picture_with(picture: &Raster, name: &str) -> Result<(), String> {
    available()?;
    let path = picture_file(name);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let bytes = screengrabcraft_render::encode(picture, ImageFormat::Png, 100).map_err(|e| e.to_string())?;
    crate::store::write_atomic(&path, &bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    log::info!("open with: {}x{} as {}", picture.width(), picture.height(), path.display());
    open_with(&path)
}

/// A file handed to the program the owner chooses, in the system's own window.
pub fn open_with(path: &Path) -> Result<(), String> {
    available()?;
    if !path.is_file() {
        return Err(format!("{} isn't there any more", path.display()));
    }
    choose(path)
}

#[cfg(target_os = "windows")]
fn choose(path: &Path) -> Result<(), String> {
    use winsafe::prelude::Handle as _;
    let file = path.to_string_lossy().replace('/', "\\");
    // The window is another program's: this one, in use as it asks, lets it come to the front.
    let _ = winsafe::AllowSetForegroundWindow(None);
    match winsafe::HWND::NULL.ShellExecute("openas", &file, None, None, winsafe::co::SW::SHOWNORMAL) {
        Ok(()) => Ok(()),
        // Some Windows versions have no "openas" verb for every file: its "Open with" window
        // straight from the shell.
        Err(e) => std::process::Command::new("rundll32.exe")
            .arg(format!("shell32.dll,OpenAs_RunDLL {file}"))
            .spawn()
            .map(|_| ())
            .map_err(|e2| format!("Windows couldn't ask which program to open it with: {e}; {e2}")),
    }
}

#[cfg(not(target_os = "windows"))]
fn choose(_path: &Path) -> Result<(), String> {
    Err("Open With needs Windows for now".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_is_written_under_a_safe_name_in_the_temporary_folder() {
        let p = picture_file("Capture 2026-11-21 at 10.15.30 / \\:*?\"<>|");
        assert!(p.starts_with(std::env::temp_dir()));
        let name = p.file_name().unwrap().to_string_lossy().to_string();
        assert!(name.ends_with(".png") && !name.contains(['/', '\\', ':', '*', '?', '"', '<', '>', '|']), "{name}");
        assert_eq!(picture_file("   ").file_name().unwrap(), "Picture.png");
        assert!(picture_file(&"x".repeat(500)).file_name().unwrap().len() < 100);
    }

    #[test]
    fn a_file_that_isnt_there_is_said_and_elsewhere_it_says_why() {
        let gone = std::env::temp_dir().join("ScreenGrabCraft").join("no-such-capture.png");
        let r = open_with(&gone);
        assert!(r.is_err());
        if cfg!(target_os = "windows") {
            assert!(available().is_ok());
            assert!(r.unwrap_err().contains("isn't there"));
        } else {
            assert!(available().is_err());
        }
    }
}
