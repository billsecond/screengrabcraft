//! The captures' library file (`library.json` in the settings folder): each capture's tags,
//! whether it's a favourite, and the program and window it was taken from. Nothing of the
//! pictures themselves is in it, only their paths.
//!
//! The capture service and the editors each change it a command at a time
//! ([`screengrabcraft_engine::library`]), over the file as it is then, so none of them puts back
//! another's old copy. Paths are kept whole and alike however they were written
//! ([`key`]); what's asked for comes back under the paths as asked.

use std::path::Path;
use std::sync::Mutex;

use screengrabcraft_model::Library;
use serde_json::Value;

use crate::{paths, store};

const FILE: &str = "library.json";
/// A library file larger than this is not read.
const MAX: u64 = 32 * 1024 * 1024;

/// One change at a time in this program.
static WRITING: Mutex<()> = Mutex::new(());

/// A capture's path as the library keeps it: whole, with forward slashes, and in lower case where
/// the system doesn't mind capitals.
pub fn key(path: &str) -> String {
    let whole = std::path::absolute(Path::new(path)).map(|p| p.to_string_lossy().to_string()).unwrap_or_else(|_| path.to_string());
    let whole = whole.replace('\\', "/");
    if cfg!(any(target_os = "windows", target_os = "macos")) { whole.to_lowercase() } else { whole }
}

/// The library as the file has it now (empty when there's none, or no settings).
fn load() -> Library {
    if paths::no_prefs() {
        return Library::default();
    }
    let Some(path) = paths::settings_dir().map(|d| d.join(FILE)) else { return Library::default() };
    match std::fs::metadata(&path) {
        Ok(m) if m.len() <= MAX => std::fs::read_to_string(&path).map(|t| Library::from_json(&t)).unwrap_or_default(),
        Ok(_) => {
            log::warn!("{} is too large to be the captures' library; ignored", path.display());
            Library::default()
        }
        Err(_) => Library::default(),
    }
}

fn save(lib: &Library) -> Result<(), String> {
    if paths::no_prefs() {
        return Ok(());
    }
    let dir = paths::ensure_settings_dir().ok_or("there is no settings folder")?;
    let json = serde_json::to_string(lib).map_err(|e| e.to_string())?;
    store::write_atomic(&dir.join(FILE), json.as_bytes()).map_err(|e| e.to_string())
}

/// What's known of these captures, under their paths as given.
pub fn for_paths(asked: &[String]) -> Library {
    let all = load();
    let mut out = Library::default();
    for p in asked {
        let info = all.get(&key(p));
        if !info.is_empty() {
            out.set(p, info);
        }
    }
    out
}

/// Runs a library command (its `path` made a [`key`]) over the file as it is now, and writes it
/// if it changed. The answer is the command's.
pub fn run(id: &str, params: &Value) -> Result<Value, String> {
    let mut params = params.clone();
    if let Some(p) = params.get("path").and_then(Value::as_str).map(key)
        && let Some(map) = params.as_object_mut()
    {
        map.insert("path".to_string(), Value::String(p));
    }
    let _one = WRITING.lock().unwrap_or_else(|e| e.into_inner());
    let mut lib = load();
    let before = lib.clone();
    let answer = screengrabcraft_engine::library::run(&mut lib, id, &params).map_err(|e| e.to_string())?;
    if lib != before {
        save(&lib)?;
    }
    Ok(answer)
}

/// Notes the program and the window a capture just kept was taken from.
pub fn taken_from(capture: &Path, program: &str, title: &str) {
    let params = serde_json::json!({"path": capture.to_string_lossy(), "program": program, "title": title});
    if let Err(e) = run("library.source", &params) {
        log::warn!("noting where a capture came from: {e}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_is_kept_whole_and_alike() {
        let a = key("captures/Capture 1.png");
        assert!(std::path::Path::new(&a).is_absolute() || a.starts_with('/'), "{a}");
        assert!(!a.contains('\\'));
        if cfg!(target_os = "windows") {
            assert_eq!(key(r"C:\Pictures\Capture.PNG"), key("c:/pictures/capture.png"));
        }
    }
}
