//! The capture service as a persistent, per-user service: started when the owner signs in and
//! started again if it stops unexpectedly.
//!
//! It has to run in the owner's session (it captures their screen and lives in their tray), so
//! it is a sign-in service rather than a system service:
//!
//! - **Windows:** the installer adds a `Run` entry for every user
//!   (`HKLM\…\CurrentVersion\Run\ScreenGrabCraft`, `screengrabcraft.exe --background`), and
//!   `--background` is the [`supervise`] loop below. The owner's own switch is Windows' own:
//!   `HKCU\…\Explorer\StartupApproved\Run` (`Run32` for a 32-bit install on 64-bit Windows),
//!   which is what Task Manager and Settings › Apps › Startup change. Without the installer (the
//!   portable zip) the switch is a `Run` entry of the owner's own (`HKCU`).
//! - **macOS:** a LaunchAgent (`~/Library/LaunchAgents/io.github.billsecond.screengrabcraft.plist`)
//!   with `KeepAlive` on an unsuccessful exit, so launchd itself restarts a crash.
//! - **Linux and the BSDs:** an XDG autostart entry. The packages install one for everybody
//!   (`/etc/xdg/autostart`); the owner's switch is an entry of the same name in
//!   `~/.config/autostart` (`Hidden=true` turns it off). Without the packages (the AppImage, the
//!   tarball) the owner's entry is the only one. Its command is `--background`, the supervisor.
//!   The packages also ship a systemd user unit for those who prefer it
//!   (`systemctl --user enable --now screengrabcraft`).

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// The name of the sign-in entries.
pub const NAME: &str = "ScreenGrabCraft";
pub const APP_ID: &str = "io.github.billsecond.screengrabcraft";

/// Whether the service starts at sign-in, and how that is arranged here.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Status {
    pub enabled: bool,
    pub how: String,
}

/// The program to register: the AppImage itself when running from one (its own path is inside
/// a temporary mount), else this executable.
pub fn program() -> Option<PathBuf> {
    if let Some(appimage) = std::env::var_os("APPIMAGE").filter(|v| !v.is_empty()) {
        return Some(PathBuf::from(appimage));
    }
    std::env::current_exe().ok()
}

// ---- Windows ----

#[cfg(target_os = "windows")]
mod imp {
    use super::*;
    use winreg::RegKey;
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, KEY_WOW64_32KEY, KEY_WOW64_64KEY};

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

    /// Where the installer's entry is: the 64-bit view (`Run`) or the 32-bit one (`Run32`).
    fn machine_entry() -> Option<&'static str> {
        let hklm = RegKey::predef(HKEY_LOCAL_MACHINE);
        for (flag, approved) in [(KEY_WOW64_64KEY, "Run"), (KEY_WOW64_32KEY, "Run32")] {
            if let Ok(k) = hklm.open_subkey_with_flags(RUN, KEY_READ | flag)
                && k.get_value::<String, _>(NAME).is_ok()
            {
                return Some(approved);
            }
        }
        None
    }

    fn approval(list: &str) -> Option<bool> {
        let k = RegKey::predef(HKEY_CURRENT_USER).open_subkey(format!(r"{APPROVED}\{list}")).ok()?;
        let v = k.get_raw_value(NAME).ok()?;
        // Task Manager writes 02 (or 06) for on and 03 (or 07, with a time) for off.
        v.bytes.first().map(|b| b & 1 == 0)
    }

    fn set_approval(list: &str, on: bool) -> Result<(), String> {
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(format!(r"{APPROVED}\{list}")).map_err(|e| e.to_string())?;
        let mut bytes = vec![0u8; 12];
        bytes[0] = if on { 2 } else { 3 };
        k.set_raw_value(NAME, &winreg::RegValue { bytes: bytes.into(), vtype: winreg::enums::RegType::REG_BINARY }).map_err(|e| e.to_string())
    }

    pub fn status() -> Status {
        if let Some(list) = machine_entry() {
            let on = approval(list).unwrap_or(true);
            return Status { enabled: on, how: "the installer's sign-in entry (Settings › Apps › Startup)".to_string() };
        }
        let user = RegKey::predef(HKEY_CURRENT_USER).open_subkey(RUN).ok().and_then(|k| k.get_value::<String, _>(NAME).ok());
        Status { enabled: user.is_some() && approval("Run").unwrap_or(true), how: "a sign-in entry of your own (HKCU Run)".to_string() }
    }

    pub fn enable(exe: &Path) -> Result<String, String> {
        if let Some(list) = machine_entry() {
            set_approval(list, true)?;
            return Ok("ScreenGrabCraft starts when you sign in (the installer's entry, switched on)".to_string());
        }
        let (k, _) = RegKey::predef(HKEY_CURRENT_USER).create_subkey(RUN).map_err(|e| e.to_string())?;
        k.set_value(NAME, &format!("\"{}\" --background", exe.display())).map_err(|e| e.to_string())?;
        let _ = set_approval("Run", true);
        Ok("ScreenGrabCraft starts when you sign in".to_string())
    }

    pub fn disable() -> Result<String, String> {
        if let Some(list) = machine_entry() {
            set_approval(list, false)?;
            return Ok("ScreenGrabCraft no longer starts when you sign in (switched off for you)".to_string());
        }
        if let Ok(k) = RegKey::predef(HKEY_CURRENT_USER).open_subkey_with_flags(RUN, winreg::enums::KEY_SET_VALUE) {
            let _ = k.delete_value(NAME);
        }
        Ok("ScreenGrabCraft no longer starts when you sign in".to_string())
    }
}

// ---- macOS ----

#[cfg(target_os = "macos")]
mod imp {
    use super::*;

    fn plist_path() -> Option<PathBuf> {
        dirs::home_dir().map(|h| h.join("Library/LaunchAgents").join(format!("{APP_ID}.plist")))
    }

    fn xml_escape(s: &str) -> String {
        s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
    }

    pub fn plist(exe: &Path) -> String {
        format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{APP_ID}</string>
  <key>ProgramArguments</key>
  <array>
    <string>{}</string>
    <string>--tray</string>
  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <dict>
    <key>SuccessfulExit</key>
    <false/>
  </dict>
  <key>ThrottleInterval</key>
  <integer>10</integer>
  <key>ProcessType</key>
  <string>Interactive</string>
  <key>LimitLoadToSessionType</key>
  <string>Aqua</string>
</dict>
</plist>
"#,
            xml_escape(&exe.to_string_lossy())
        )
    }

    pub fn status() -> Status {
        let on = plist_path().is_some_and(|p| p.exists());
        Status { enabled: on, how: "a LaunchAgent (~/Library/LaunchAgents)".to_string() }
    }

    pub fn enable(exe: &Path) -> Result<String, String> {
        let path_text = exe.to_string_lossy();
        if path_text.contains("/AppTranslocation/") || path_text.starts_with("/Volumes/") {
            return Err("Move ScreenGrabCraft to the Applications folder first, then turn this on".to_string());
        }
        let p = plist_path().ok_or("no home folder")?;
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        crate::store::write_atomic(&p, plist(exe).as_bytes()).map_err(|e| e.to_string())?;
        Ok("ScreenGrabCraft starts when you sign in, and again if it stops".to_string())
    }

    pub fn disable() -> Result<String, String> {
        if let Some(p) = plist_path()
            && p.exists()
        {
            std::fs::remove_file(&p).map_err(|e| e.to_string())?;
        }
        Ok("ScreenGrabCraft no longer starts when you sign in".to_string())
    }
}

// ---- Linux and the BSDs ----

#[cfg(all(unix, not(target_os = "macos")))]
mod imp {
    use super::*;

    fn file_name() -> String {
        format!("{APP_ID}.desktop")
    }

    fn user_entry() -> Option<PathBuf> {
        dirs::config_dir().map(|c| c.join("autostart").join(file_name()))
    }

    /// The packages' entry for everybody, if installed.
    fn system_entry() -> Option<PathBuf> {
        let dirs = std::env::var("XDG_CONFIG_DIRS").ok().filter(|v| !v.is_empty()).unwrap_or_else(|| "/etc/xdg".to_string());
        dirs.split(':').map(|d| PathBuf::from(d).join("autostart").join(file_name())).find(|p| p.exists())
    }

    fn hidden(path: &Path) -> bool {
        std::fs::read_to_string(path).is_ok_and(|t| t.lines().any(|l| l.trim().eq_ignore_ascii_case("Hidden=true")))
    }

    pub fn entry(exe: &Path, hidden: bool) -> String {
        let quoted = format!("\"{}\"", exe.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\""));
        format!(
            "[Desktop Entry]\nType=Application\nName=ScreenGrabCraft\nComment=Screen capture service in the tray\nExec={quoted} --background\nIcon={APP_ID}\nTerminal=false\nNoDisplay=true\nX-GNOME-Autostart-enabled={}\nHidden={}\n",
            !hidden, hidden
        )
    }

    pub fn status() -> Status {
        if std::env::var_os("FLATPAK_ID").is_some() {
            return Status { enabled: false, how: "your desktop's autostart settings (Flatpak)".to_string() };
        }
        let user = user_entry();
        match (system_entry(), user) {
            (Some(_), Some(u)) if u.exists() => {
                Status { enabled: !hidden(&u), how: "the package's autostart entry, with your own setting".to_string() }
            }
            (Some(_), _) => Status { enabled: true, how: "the package's autostart entry (/etc/xdg/autostart)".to_string() },
            (None, Some(u)) => Status { enabled: u.exists() && !hidden(&u), how: "an autostart entry of your own (~/.config/autostart)".to_string() },
            (None, None) => Status { enabled: false, how: "no autostart folder".to_string() },
        }
    }

    pub fn enable(exe: &Path) -> Result<String, String> {
        if std::env::var_os("FLATPAK_ID").is_some() {
            return Err("In the Flatpak, turn this on in your desktop's settings for apps that start at sign-in".to_string());
        }
        let u = user_entry().ok_or("no configuration folder")?;
        if system_entry().is_some() {
            // The package's entry is enough; the owner's override goes.
            if u.exists() {
                std::fs::remove_file(&u).map_err(|e| e.to_string())?;
            }
        } else {
            if let Some(dir) = u.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            crate::store::write_atomic(&u, entry(exe, false).as_bytes()).map_err(|e| e.to_string())?;
        }
        Ok("ScreenGrabCraft starts when you sign in, and again if it stops".to_string())
    }

    pub fn disable() -> Result<String, String> {
        let u = user_entry().ok_or("no configuration folder")?;
        if system_entry().is_some() {
            let exe = program().unwrap_or_else(|| PathBuf::from("screengrabcraft"));
            if let Some(dir) = u.parent() {
                std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
            }
            crate::store::write_atomic(&u, entry(&exe, true).as_bytes()).map_err(|e| e.to_string())?;
        } else if u.exists() {
            std::fs::remove_file(&u).map_err(|e| e.to_string())?;
        }
        Ok("ScreenGrabCraft no longer starts when you sign in".to_string())
    }
}

#[cfg(not(any(target_os = "windows", unix)))]
mod imp {
    use super::*;
    pub fn status() -> Status {
        Status { enabled: false, how: "not available on this system".to_string() }
    }
    pub fn enable(_exe: &Path) -> Result<String, String> {
        Err("not available on this system".to_string())
    }
    pub fn disable() -> Result<String, String> {
        Err("not available on this system".to_string())
    }
}

pub fn status() -> Status {
    imp::status()
}

/// Registers this program (the capture service) to start at sign-in.
pub fn enable() -> Result<String, String> {
    let exe = program().ok_or("this program's own path is unknown")?;
    enable_with(&exe)
}

/// Registers the capture service at `exe` to start at sign-in (the command-line program names
/// the service installed beside it).
pub fn enable_with(exe: &Path) -> Result<String, String> {
    if crate::paths::no_prefs() {
        return Err("not while running without settings (SCREENGRABCRAFT_NO_PREFS)".to_string());
    }
    imp::enable(exe)
}

pub fn disable() -> Result<String, String> {
    if crate::paths::no_prefs() {
        return Err("not while running without settings (SCREENGRABCRAFT_NO_PREFS)".to_string());
    }
    imp::disable()
}

/// What the supervisor decides after the service ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum After {
    /// It ended on purpose (Quit, another copy already running, the installer closing it).
    Stop,
    /// It failed: start it again after this long.
    Restart(Duration),
    /// It failed too often lately: give up.
    GiveUp,
}

/// Restarts within this window count towards giving up.
const WINDOW: Duration = Duration::from_secs(120);
const MAX_RESTARTS: usize = 5;

/// Decides what follows an exit: `code` None is a crash without a code (a signal).
pub fn after(code: Option<i32>, recent: &mut Vec<Instant>, now: Instant) -> After {
    if code == Some(0) {
        return After::Stop;
    }
    recent.retain(|t| now.duration_since(*t) < WINDOW);
    if recent.len() >= MAX_RESTARTS {
        return After::GiveUp;
    }
    recent.push(now);
    // 1 s, 2 s, 4 s, 8 s, 16 s.
    After::Restart(Duration::from_secs(1u64 << (recent.len() - 1).min(4)))
}

/// `--background`: runs `exe --tray` and starts it again whenever it ends unexpectedly. Returns
/// the exit code to end with.
pub fn supervise(exe: &Path, extra: &[String]) -> i32 {
    let mut recent = Vec::new();
    loop {
        let started = Instant::now();
        let status = Command::new(exe).arg("--tray").args(extra).status();
        let code = match status {
            Ok(s) => s.code(),
            Err(e) => {
                log::error!("could not start the capture service: {e}");
                return 1;
            }
        };
        log::info!("the capture service ended with {code:?} after {} s", started.elapsed().as_secs());
        match after(code, &mut recent, Instant::now()) {
            After::Stop => return 0,
            After::GiveUp => {
                log::error!("the capture service stopped {MAX_RESTARTS} times in {} s; not starting it again", WINDOW.as_secs());
                return 1;
            }
            After::Restart(wait) => {
                log::warn!("starting the capture service again in {} s", wait.as_secs());
                std::thread::sleep(wait);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_supervisor_restarts_failures_and_gives_up() {
        let mut recent = Vec::new();
        let t0 = Instant::now();
        assert_eq!(after(Some(0), &mut recent, t0), After::Stop);
        assert_eq!(after(Some(101), &mut recent, t0), After::Restart(Duration::from_secs(1)));
        assert_eq!(after(None, &mut recent, t0), After::Restart(Duration::from_secs(2)));
        assert_eq!(after(Some(1), &mut recent, t0), After::Restart(Duration::from_secs(4)));
        assert_eq!(after(Some(1), &mut recent, t0), After::Restart(Duration::from_secs(8)));
        assert_eq!(after(Some(1), &mut recent, t0), After::Restart(Duration::from_secs(16)));
        assert_eq!(after(Some(1), &mut recent, t0), After::GiveUp);
        // Long after, failures count afresh.
        let later = t0 + WINDOW + Duration::from_secs(1);
        assert_eq!(after(Some(1), &mut recent, later), After::Restart(Duration::from_secs(1)));
    }

    #[cfg(all(unix, not(target_os = "macos")))]
    #[test]
    fn autostart_entries() {
        let on = imp::entry(Path::new("/opt/My Apps/screengrabcraft"), false);
        assert!(on.contains("Exec=\"/opt/My Apps/screengrabcraft\" --background"));
        assert!(on.contains("Hidden=false"));
        assert!(imp::entry(Path::new("/x"), true).contains("Hidden=true"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn launch_agent() {
        let p = imp::plist(Path::new("/Applications/ScreenGrabCraft.app/Contents/MacOS/ScreenGrabCraft"));
        assert!(p.contains("<string>--tray</string>") && p.contains("<key>SuccessfulExit</key>"));
    }
}
