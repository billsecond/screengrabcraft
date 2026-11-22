//! Where things are kept: the settings folder, the captures folder, the logs.
//!
//! - Settings: `%APPDATA%\ScreenGrabCraft` on Windows, `~/Library/Application Support/ScreenGrabCraft`
//!   on macOS, `$XDG_CONFIG_HOME/screengrabcraft` (`~/.config/screengrabcraft`) elsewhere.
//! - Captures: `Pictures/ScreenGrabCraft` (the system's Pictures folder, wherever the owner has
//!   moved it), unless the settings name another folder.
//!
//! `SCREENGRABCRAFT_NO_PREFS=1` makes a run that reads and writes no settings at all (tests,
//! CI): nothing is registered to start at sign-in and no instance lock is taken.

use std::path::{Path, PathBuf};

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

/// The most folders the Recent strip and All captures look in.
const FOLDERS_MAX: usize = 32;

/// Every folder captures are kept in: the captures folder first, then the capture presets' own
/// folders and those captures were kept in before ([`Settings::other_folders`]), each once.
pub fn capture_folders(settings: &Settings) -> Vec<PathBuf> {
    let mut out = vec![captures_dir(settings)];
    let mut seen = vec![whole(&captures_dir(settings))];
    let more = settings.presets.iter().filter_map(|p| p.folder.as_deref()).chain(settings.other_folders.iter().map(String::as_str));
    for f in more.map(str::trim).filter(|f| !f.is_empty()) {
        let p = PathBuf::from(f);
        let w = whole(&p);
        let w = w.trim_end_matches('/').to_string();
        if out.len() >= FOLDERS_MAX {
            break;
        }
        if !seen.iter().any(|s| s.trim_end_matches('/') == w) {
            seen.push(w);
            out.push(p);
        }
    }
    out
}

/// The log folder.
pub fn log_dir() -> Option<PathBuf> {
    settings_dir().map(|d| d.join("logs"))
}

/// A short name made from some text, the same every time (FNV-1a, 64 bits, in hex).
fn key(text: &str) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in text.as_bytes() {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{h:016x}")
}

/// A file's whole path, as one text (the same file the same way, however it was named).
fn whole(path: &Path) -> String {
    std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf()).to_string_lossy().replace('\\', "/").to_lowercase()
}

/// Where the captions of a sound file or video brought into the editor are kept (SubRip): in
/// the settings folder (the temporary folder when there are no settings), under a name made
/// from its path, size and time written, so a changed file is heard again. None when the file
/// isn't there.
pub fn media_captions(path: &Path) -> Option<PathBuf> {
    let meta = std::fs::metadata(path).ok()?;
    let written = meta.modified().ok().and_then(|m| m.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |d| d.as_secs());
    let stem: String = path.file_stem()?.to_string_lossy().chars().filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-').take(40).collect();
    let dir = if no_prefs() { None } else { settings_dir() }.unwrap_or_else(|| std::env::temp_dir().join("ScreenGrabCraft")).join("captions");
    Some(dir.join(format!("{}-{}.srt", stem.trim(), key(&format!("{}|{}|{written}", whole(path), meta.len())))))
}

/// Where the program making a file's captions says how far it has got (in the temporary
/// folder): a number from 0 to 1000, or `error: ` and why it couldn't.
pub fn captions_progress(path: &Path) -> PathBuf {
    std::env::temp_dir().join("ScreenGrabCraft").join("captions").join(format!("{}.progress", key(&whole(path))))
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::CapturePreset;

    use super::*;

    #[test]
    fn captures_are_looked_for_in_the_captures_folder_then_the_presets_and_those_used_before() {
        let base = std::env::temp_dir().join("sgc-folders");
        let (main, own, before) = (base.join("Captures"), base.join("Bugs"), base.join("Old"));
        let text = |p: &Path| p.to_string_lossy().to_string();
        let s = Settings {
            folder: Some(text(&main)),
            presets: vec![
                CapturePreset { folder: Some(text(&own)), ..CapturePreset::default() },
                CapturePreset { folder: Some(format!("{}/", text(&main))), ..CapturePreset::default() },
                CapturePreset::default(),
            ],
            other_folders: vec![text(&before), text(&own), "  ".into()],
            ..Settings::default()
        };
        assert_eq!(capture_folders(&s), vec![main.clone(), own, before]);
        // Without any: the captures folder alone.
        assert_eq!(capture_folders(&Settings::default()), vec![captures_dir(&Settings::default())]);
        // However many there are, a bounded number is looked in.
        let many = Settings { other_folders: (0..100).map(|i| format!("{}/{i}", text(&base))).collect(), ..Settings::default() };
        assert_eq!(capture_folders(&many).len(), FOLDERS_MAX);
    }
}
