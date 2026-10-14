//! Where things are kept: the settings folder, the captures folder, the logs.
//!
//! - Settings: `%APPDATA%\ScreenGrabCraft` on Windows, `~/Library/Application Support/ScreenGrabCraft`
//!   on macOS, `$XDG_CONFIG_HOME/screengrabcraft` (`~/.config/screengrabcraft`) elsewhere.
//! - Captures: `Pictures/ScreenGrabCraft` (the system's Pictures folder, wherever the owner has
//!   moved it), unless the settings name another folder.
//!
//! `SCREENGRABCRAFT_NO_PREFS=1` makes a run that reads and writes no settings at all (tests,
//! CI): nothing is registered to start at sign-in and no instance lock is taken.

use std::path::PathBuf;

use screengrabcraft_model::Settings;

/// Whether this run keeps no settings.
pub fn no_prefs() -> bool {
    std::env::var_os("SCREENGRABCRAFT_NO_PREFS").is_some_and(|v| !v.is_empty() && v != "0")
}

/// The settings folder (not created).
pub fn settings_dir() -> Option<PathBuf> {
    let base = dirs::config_dir()?;
    Some(if cfg!(any(target_os = "windows", target_os = "macos")) { base.join("ScreenGrabCraft") } else { base.join("screengrabcraft") })
}

/// The settings folder, created if it isn't there.
pub fn ensure_settings_dir() -> Option<PathBuf> {
    let dir = settings_dir()?;
    if let Err(e) = std::fs::create_dir_all(&dir) {
        log::warn!("could not create {}: {e}", dir.display());
        return None;
    }
    Some(dir)
}

/// Where captures are saved.
pub fn captures_dir(settings: &Settings) -> PathBuf {
    if let Some(f) = settings.folder.as_deref().map(str::trim).filter(|f| !f.is_empty()) {
        return PathBuf::from(f);
    }
    let pictures = dirs::picture_dir().or_else(|| dirs::home_dir().map(|h| h.join("Pictures"))).unwrap_or_else(std::env::temp_dir);
    pictures.join("ScreenGrabCraft")
}

/// The log folder.
pub fn log_dir() -> Option<PathBuf> {
    settings_dir().map(|d| d.join("logs"))
}
