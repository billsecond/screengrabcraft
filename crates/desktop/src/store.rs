//! Reading and writing the settings files. A file that is missing or can't be read gives the
//! defaults; writing goes through a temporary file, so a crash mid-write never leaves half a
//! file behind.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use screengrabcraft_model::{EditorSettings, Settings};

use crate::paths;

const SETTINGS: &str = "settings.json";
const EDITOR: &str = "editor.json";
/// Settings files larger than this are not read.
const MAX: u64 = 4 * 1024 * 1024;

fn read(name: &str) -> Option<String> {
    if paths::no_prefs() {
        return None;
    }
    let path = paths::settings_dir()?.join(name);
    let meta = std::fs::metadata(&path).ok()?;
    if meta.len() > MAX {
        log::warn!("{} is too large to be settings; ignored", path.display());
        return None;
    }
    std::fs::read_to_string(&path).ok()
}

/// Writes `bytes` to `path` by way of a temporary file beside it. Each write has its own
/// temporary file, so two at once (a save and an export of `capture.png` and `capture.jpg`)
/// never write into the same one.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    static WRITES: AtomicU64 = AtomicU64::new(0);
    let n = WRITES.fetch_add(1, Ordering::Relaxed);
    let mut name = path.file_name().map(|n| n.to_os_string()).unwrap_or_default();
    name.push(format!(".tmp-{}-{n}", std::process::id()));
    let tmp: PathBuf = path.with_file_name(name);
    std::fs::write(&tmp, bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

fn write(name: &str, json: &str) {
    if paths::no_prefs() {
        return;
    }
    let Some(dir) = paths::ensure_settings_dir() else { return };
    let path = dir.join(name);
    if let Err(e) = write_atomic(&path, json.as_bytes()) {
        log::warn!("saving {} failed: {e}", path.display());
    }
}

pub fn load_settings() -> Settings {
    read(SETTINGS).map(|t| Settings::from_json(&t)).unwrap_or_default()
}

pub fn save_settings(s: &Settings) {
    if let Ok(json) = serde_json::to_string_pretty(s) {
        write(SETTINGS, &json);
    }
}

pub fn load_editor_settings() -> EditorSettings {
    read(EDITOR).map(|t| EditorSettings::from_json(&t)).unwrap_or_else(|| EditorSettings { show_tray: true, ..EditorSettings::default() })
}

pub fn save_editor_settings(s: &EditorSettings) {
    if let Ok(json) = serde_json::to_string_pretty(s) {
        write(EDITOR, &json);
    }
}

/// When the settings file was last changed (the editor reloads the captures folder from it).
pub fn settings_modified() -> Option<std::time::SystemTime> {
    let path = paths::settings_dir()?.join(SETTINGS);
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_writes_at_once_beside_each_other_keep_to_their_own_files() {
        let dir = std::env::temp_dir().join(format!("screengrabcraft-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (png, jpg) = (dir.join("capture.png"), dir.join("capture.jpg"));
        // Same name before the extension: once they shared a temporary file.
        std::thread::scope(|s| {
            s.spawn(|| (0..40).for_each(|i| write_atomic(&png, format!("png {i}").as_bytes()).unwrap()));
            s.spawn(|| (0..40).for_each(|i| write_atomic(&jpg, format!("jpeg {i}").as_bytes()).unwrap()));
        });
        assert_eq!(std::fs::read(&png).unwrap(), b"png 39");
        assert_eq!(std::fs::read(&jpg).unwrap(), b"jpeg 39");
        let left = std::fs::read_dir(&dir).unwrap().filter_map(Result::ok).filter(|e| e.file_name().to_string_lossy().contains(".tmp-")).count();
        assert_eq!(left, 0, "no temporary files left behind");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
