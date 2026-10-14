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
pub fn open_editor(args: &[String]) -> Result<(), String> {
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

/// Shows a file selected in the file manager, or a folder.
pub fn reveal(path: &Path) -> Result<(), String> {
    let is_dir = path.is_dir();
    let result = if cfg!(target_os = "windows") {
        if is_dir { Command::new("explorer").arg(path).spawn() } else { Command::new("explorer").arg(format!("/select,{}", path.display())).spawn() }
    } else if cfg!(target_os = "macos") {
        if is_dir { Command::new("open").arg(path).spawn() } else { Command::new("open").arg("-R").arg(path).spawn() }
    } else {
        let dir = if is_dir { path } else { path.parent().unwrap_or(path) };
        Command::new("xdg-open").arg(dir).spawn()
    };
    result.map(|_| ()).map_err(|e| e.to_string())
}

/// Opens a web page in the browser.
pub fn open_url(url: &str) -> Result<(), String> {
    webbrowser::open(url).map_err(|e| e.to_string())
}
