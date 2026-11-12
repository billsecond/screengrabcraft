//! The models installed with ScreenGrabCraft: Remove Background's (where its file is, and the
//! model the editor and the command-line program lend their sessions) and the speech model
//! that makes the captions of recordings ([`speech_model`]).
//!
//! The file is looked for, in order: the one `SCREENGRABCRAFT_MODEL` names; the `models` folder
//! in the settings folder (where the owner can put another model of a known family); the one
//! installed with the program (`models` beside the program on Windows and in the portable zip,
//! `../share/screengrabcraft/models` on Linux and BSD, `../Resources/models` in a macOS app);
//! and, for a build in `target/`, `assets/models` in the source tree. Nothing is ever downloaded.

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use screengrabcraft_matte::{Family, MAX_MODEL_BYTES, Matting, Remover};

use crate::paths;

/// The model installed with ScreenGrabCraft: IS-Net general use, its weights in 8 bits.
pub const BUNDLED: &str = "isnet-general-use-int8.onnx";

/// The folders a model is looked for in, in order.
pub fn model_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(s) = paths::settings_dir() {
        dirs.push(s.join("models"));
    }
    if let Ok(exe) = std::env::current_exe()
        && let Some(bin) = exe.parent()
    {
        dirs.push(bin.join("models"));
        dirs.push(bin.join("..").join("share").join("screengrabcraft").join("models"));
        dirs.push(bin.join("..").join("Resources").join("models"));
        // A build in the source tree: target/release or target/<triple>/release.
        for up in bin.ancestors().skip(1).take(3) {
            dirs.push(up.join("assets").join("models"));
        }
    }
    dirs
}

/// The first model file of a known family in `dirs` (in a folder, the installed model first,
/// then by name).
pub fn find(dirs: &[PathBuf]) -> Option<(PathBuf, Family)> {
    for d in dirs {
        let Ok(entries) = std::fs::read_dir(d) else { continue };
        let mut files: Vec<PathBuf> =
            entries.filter_map(Result::ok).map(|e| e.path()).filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("onnx"))).collect();
        files.sort();
        files.sort_by_key(|p| p.file_name() != Some(OsStr::new(BUNDLED)));
        if let Some(found) = files.into_iter().find_map(|f| Family::from_name(&f.file_name()?.to_string_lossy()).map(|fam| (f, fam))) {
            return Some(found);
        }
    }
    None
}

fn read(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > MAX_MODEL_BYTES as u64 {
        return Err(format!("{}: the model file is too large", path.display()));
    }
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

/// The model in `path`, of the family its name says (IS-Net when it doesn't), read when first
/// used.
pub fn open(path: PathBuf) -> Arc<dyn Matting> {
    let name = path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "model".to_string());
    let family = Family::from_name(&name).unwrap_or(Family::IsNet);
    log::info!("background model: {} ({family:?})", path.display());
    Arc::new(Remover::new(name, family, Box::new(move || read(&path))))
}

/// The background model to lend, or why there is none.
pub fn background_model() -> Result<Arc<dyn Matting>, String> {
    if let Some(f) = std::env::var_os("SCREENGRABCRAFT_MODEL").filter(|f| !f.is_empty()) {
        return Ok(open(PathBuf::from(f)));
    }
    match find(&model_dirs()) {
        Some((path, _)) => Ok(open(path)),
        None => Err(format!("The background model isn't installed with this copy of ScreenGrabCraft ({BUNDLED})")),
    }
}

/// The speech model installed with ScreenGrabCraft: Whisper base, English only, its large
/// weights in 8 bits (it makes the captions of recordings).
pub const SPEECH: &str = "whisper-base.en-q8_0.gguf";

/// The first speech model in `dirs`: a `whisper….gguf` (in a folder, the installed one first,
/// then by name).
pub fn find_speech(dirs: &[PathBuf]) -> Option<PathBuf> {
    for d in dirs {
        let Ok(entries) = std::fs::read_dir(d) else { continue };
        let mut files: Vec<PathBuf> = entries
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("gguf")))
            .filter(|p| p.file_name().is_some_and(|n| n.to_string_lossy().to_ascii_lowercase().starts_with("whisper")))
            .collect();
        files.sort();
        files.sort_by_key(|p| p.file_name() != Some(OsStr::new(SPEECH)));
        if let Some(f) = files.into_iter().next() {
            return Some(f);
        }
    }
    None
}

/// The speech model's file, or why there is none: the one `SCREENGRABCRAFT_SPEECH_MODEL` names,
/// else the first in [`model_dirs`]. Nothing is ever downloaded.
pub fn speech_model() -> Result<PathBuf, String> {
    if let Some(f) = std::env::var_os("SCREENGRABCRAFT_SPEECH_MODEL").filter(|f| !f.is_empty()) {
        return Ok(PathBuf::from(f));
    }
    find_speech(&model_dirs()).ok_or_else(|| format!("The speech model isn't installed with this copy of ScreenGrabCraft ({SPEECH})"))
}

/// A model file's bytes, no larger than `max`.
pub fn read_model(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if meta.len() > max {
        return Err(format!("{}: the model file is too large", path.display()));
    }
    std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_installed_speech_model_is_found_first() {
        let base = std::env::temp_dir().join(format!("sgc-speech-models-{}", std::process::id()));
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("llama.gguf"), b"").unwrap();
        std::fs::write(b.join("whisper-tiny.gguf"), b"").unwrap();
        std::fs::write(b.join(SPEECH), b"").unwrap();
        let dirs = vec![base.join("missing"), a.clone(), b.clone()];
        assert_eq!(find_speech(&dirs), Some(b.join(SPEECH)));
        std::fs::remove_file(b.join(SPEECH)).unwrap();
        assert_eq!(find_speech(&dirs), Some(b.join("whisper-tiny.gguf")));
        assert_eq!(find_speech(&[a]), None);
        assert!(read_model(&b.join("whisper-tiny.gguf"), 0).is_ok());
        std::fs::write(b.join("big.gguf"), b"12345").unwrap();
        assert!(read_model(&b.join("big.gguf"), 4).is_err());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn the_installed_model_is_found_first_and_strangers_are_not() {
        let base = std::env::temp_dir().join(format!("sgc-models-{}", std::process::id()));
        let (a, b) = (base.join("a"), base.join("b"));
        std::fs::create_dir_all(&a).unwrap();
        std::fs::create_dir_all(&b).unwrap();
        std::fs::write(a.join("notes.txt"), b"").unwrap();
        std::fs::write(a.join("mystery.onnx"), b"").unwrap();
        std::fs::write(b.join("u2netp.onnx"), b"").unwrap();
        std::fs::write(b.join(BUNDLED), b"").unwrap();
        let dirs = vec![base.join("missing"), a.clone(), b.clone()];
        assert_eq!(find(&dirs), Some((b.join(BUNDLED), Family::IsNet)));
        std::fs::remove_file(b.join(BUNDLED)).unwrap();
        assert_eq!(find(&dirs), Some((b.join("u2netp.onnx"), Family::U2Net)));
        assert_eq!(find(&[a]), None);
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_model_that_isnt_there_is_an_error_when_used() {
        let m = open(PathBuf::from("no/such/isnet-general-use.onnx"));
        assert_eq!(m.name(), "isnet-general-use");
        let pic = screengrabcraft_model::Raster::single(screengrabcraft_model::Color::RED);
        assert!(m.matte(&pic, &mut |_| true).is_err());
    }
}
