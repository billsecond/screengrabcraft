//! Starting the other program, and showing files and pages.
//!
//! The two programs are installed side by side (Program Files, `/usr/bin`, the AppImage, the
//! portable zip, the tarballs), so each looks for the other next to itself first. On macOS each
//! is an app of its own: the other is looked for beside this app's bundle (both in
//! Applications), and else asked for by its bundle id.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

pub const EDITOR_BIN: &str = "screengrabcraft-editor";
pub const SERVICE_BIN: &str = "screengrabcraft";
pub const EDITOR_BUNDLE_ID: &str = "io.github.billsecond.screengrabcraft.editor";
pub const SERVICE_BUNDLE_ID: &str = "io.github.billsecond.screengrabcraft";
/// What the capture service tells a running editor before a capture ([`tell_editor`]).
pub const EDITOR_HIDE: &str = "--hide-for-capture";
/// What it tells the editor once a capture is over, if no picture came of it for the editor.
pub const EDITOR_SHOW: &str = "--show-after-capture";
/// What it starts the editor with to keep it ready out of sight ([`ready_editor`]).
pub const EDITOR_STANDBY: &str = "--standby";
/// What it tells an editor kept ready when the owner no longer wants one: it ends.
pub const EDITOR_UNREADY: &str = "--end-standby";
/// What the editor asks the capture service with for a recording's captions
/// (`--captions <recording>`, [`ask_captions`]).
pub const ASK_CAPTIONS: &str = "--captions";
/// What the service opens a recording in the editor with when its captions are being made
/// (`--captions-coming <recording>`): the editor says so until they come.
pub const CAPTIONS_COMING: &str = "--captions-coming";

fn exe_name(bin: &str) -> String {
    if cfg!(target_os = "windows") { format!("{bin}.exe") } else { bin.to_string() }
}

/// A program installed next to this one.
pub fn sibling(bin: &str) -> Option<PathBuf> {
    let here = std::env::current_exe().ok()?;
    let p = here.parent()?.join(exe_name(bin));
    p.exists().then_some(p)
}

/// On macOS: the other app's executable beside this app's bundle
/// (`/Applications/ScreenGrabCraft Editor.app/Contents/MacOS/ScreenGrabCraft Editor`).
fn mac_bundle_sibling(app: &str, exe: &str) -> Option<PathBuf> {
    let here = std::env::current_exe().ok()?;
    // …/X.app/Contents/MacOS/X → …/
    let apps = here.parent()?.parent()?.parent()?.parent()?;
    let p = apps.join(format!("{app}.app")).join("Contents/MacOS").join(exe);
    p.exists().then_some(p)
}

fn spawn(program: &Path, args: &[String]) -> Result<(), String> {
    let mut c = Command::new(program);
    c.args(args).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
    c.spawn().map(|_| ()).map_err(|e| format!("{}: {e}", program.display()))
}

/// Opens the editor with `args` (files to open); a running editor takes them over.
///
/// A running editor is handed them straight away, in a few milliseconds; only when none runs is
/// one started (a start would otherwise load a whole second editor just to hand them over).
pub fn open_editor(args: &[String]) -> Result<(), String> {
    if crate::instance::tell("editor", args) {
        return Ok(());
    }
    if let Some(p) = sibling(EDITOR_BIN) {
        return spawn(&p, args);
    }
    if cfg!(target_os = "macos") {
        if let Some(p) = mac_bundle_sibling("ScreenGrabCraft Editor", "ScreenGrabCraft Editor") {
            return spawn(&p, args);
        }
        let mut c = Command::new("open");
        c.args(["-b", EDITOR_BUNDLE_ID]);
        if !args.is_empty() {
            c.arg("--args").args(args);
        }
        return c.spawn().map(|_| ()).map_err(|e| format!("open: {e}"));
    }
    spawn(Path::new(EDITOR_BIN), args)
}

/// Starts the editor out of sight, ready for the next capture, unless one is running already.
/// The first start of a program can take seconds (Windows checks it before it runs, and more so
/// after an update); done here, while nobody waits, a capture opens in the editor at once.
pub fn ready_editor() -> Result<(), String> {
    if crate::instance::running("editor") {
        return Ok(());
    }
    let p = sibling(EDITOR_BIN).ok_or("the editor isn't installed next to the capture service")?;
    spawn(&p, &[EDITOR_STANDBY.to_string()])
}

/// Tells the editor something if one is running, without starting one; `true` if one took it.
pub fn tell_editor(word: &str) -> bool {
    crate::instance::tell("editor", &[word.to_string()])
}

/// Asks the capture service for something (`--capture region`); a running one takes it over,
/// else it starts.
pub fn ask_service(args: &[String]) -> Result<(), String> {
    if let Some(p) = sibling(SERVICE_BIN) {
        return spawn(&p, args);
    }
    if cfg!(target_os = "macos")
        && let Some(p) = mac_bundle_sibling("ScreenGrabCraft", "ScreenGrabCraft")
    {
        return spawn(&p, args);
    }
    spawn(Path::new(SERVICE_BIN), args)
}

/// Asks the capture service to make a recording's captions (it makes one at a time, never while
/// recording); it starts if it isn't running.
pub fn ask_captions(recording: &str) -> Result<(), String> {
    ask_service(&[ASK_CAPTIONS.to_string(), recording.to_string()])
}

/// Shows a file selected in the file manager, or a folder.
pub fn reveal(path: &Path) -> Result<(), String> {
    let is_dir = path.is_dir();
    #[cfg(target_os = "windows")]
    let result = if is_dir {
        Command::new("explorer").arg(path).spawn()
    } else {
        // Windows' own "open the folder with this selected" (SHOpenFolderAndSelectItems), which
        // picks the file out where Explorer's /select at times only opens the folder. On a thread
        // of its own, as it wants a COM apartment of its own; Explorer's /select if it fails.
        let file = std::path::PathBuf::from(path.to_string_lossy().replace('/', "\\"));
        return std::thread::Builder::new()
            .name("reveal".into())
            .spawn(move || {
                if let Err(e) = opener::reveal(&file) {
                    log::warn!("showing {} selected: {e}; asking Explorer", file.display());
                    // Explorer reads `/select,"C:\a folder\a file"` itself, quotes and all; Rust's
                    // own quoting of the whole argument loses it in a folder with spaces or commas.
                    use std::os::windows::process::CommandExt as _;
                    let _ = Command::new("explorer").raw_arg(select_arg(&file.to_string_lossy())).spawn();
                }
            })
            .map(|_| ())
            .map_err(|e| e.to_string());
    };
    #[cfg(target_os = "macos")]
    let result = if is_dir { Command::new("open").arg(path).spawn() } else { Command::new("open").arg("-R").arg(path).spawn() };
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    let result = {
        let dir = if is_dir { path } else { path.parent().unwrap_or(path) };
        if !is_dir {
            // Linux and the BSDs: the file manager's own "show these items" (Files, Dolphin,
            // Nemo, Caja, Thunar…), asked over D-Bus on a thread of its own as it may take a
            // moment; the folder alone if nothing answers.
            let (file, dir) = (path.to_path_buf(), dir.to_path_buf());
            return std::thread::Builder::new()
                .name("reveal".into())
                .spawn(move || {
                    let shown = Command::new("dbus-send")
                        .args(["--session", "--print-reply", "--dest=org.freedesktop.FileManager1", "--type=method_call"])
                        .args(["/org/freedesktop/FileManager1", "org.freedesktop.FileManager1.ShowItems"])
                        .arg(format!("array:string:{}", file_uri(&file)))
                        .arg("string:")
                        .stdout(std::process::Stdio::null())
                        .stderr(std::process::Stdio::null())
                        .status()
                        .is_ok_and(|s| s.success());
                    if !shown && let Err(e) = Command::new("xdg-open").arg(&dir).spawn() {
                        log::warn!("showing {}: {e}", dir.display());
                    }
                })
                .map(|_| ())
                .map_err(|e| e.to_string());
        }
        Command::new("xdg-open").arg(dir).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Explorer's argument to show a file selected: `/select,"C:\a folder\a file.mp4"`.
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn select_arg(windows_path: &str) -> String {
    format!("/select,\"{}\"", windows_path.replace('"', ""))
}

/// A path as a `file://` address, each byte that isn't plain written as `%XX`.
#[cfg_attr(any(target_os = "windows", target_os = "macos"), allow(dead_code))]
fn file_uri(path: &Path) -> String {
    let whole = std::path::absolute(path).unwrap_or_else(|_| path.to_path_buf());
    let mut out = String::from("file://");
    for b in whole.to_string_lossy().bytes() {
        if b.is_ascii_alphanumeric() || b"/-_.~".contains(&b) {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// Opens a file in the program the system uses for it (a video in its player).
pub fn open_file(path: &Path) -> Result<(), String> {
    let result = if cfg!(target_os = "windows") {
        Command::new("explorer").arg(path).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(path).spawn()
    } else {
        Command::new("xdg-open").arg(path).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Opens a web page in the browser.
pub fn open_url(url: &str) -> Result<(), String> {
    webbrowser::open(url).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_is_told_the_whole_path_in_quotes() {
        assert_eq!(
            select_arg(r"C:\Users\me\OneDrive - Home, Mine\Pictures\A clip.mp4"),
            r#"/select,"C:\Users\me\OneDrive - Home, Mine\Pictures\A clip.mp4""#
        );
        assert_eq!(select_arg(r#"C:\odd"name.txt"#), r#"/select,"C:\oddname.txt""#);
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn a_file_address_has_its_odd_bytes_written_out() {
        assert_eq!(file_uri(Path::new("/home/me/My Clips/café #1.mp4")), "file:///home/me/My%20Clips/caf%C3%A9%20%231.mp4");
    }
}
