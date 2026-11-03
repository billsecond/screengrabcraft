//! Updates from ScreenGrabCraft's GitHub releases: the one thing ScreenGrabCraft asks the network
//! for.
//!
//! The capture service asks GitHub for the latest release ([`check`]:
//! `GET https://api.github.com/repos/billsecond/screengrabcraft/releases/latest`), a plain HTTPS
//! request with `User-Agent: ScreenGrabCraft/<version>` and nothing about the owner: no
//! identifier, no cookie, nothing of a capture. The answer is hostile input like any other: at
//! most [`MAX_JSON`] is read, every field is checked, and anything odd is an error that waits
//! for the next check. Drafts and pre-releases are never offered.
//!
//! When the release is newer than this program:
//! - **Windows**, in a copy the MSI installed ([`can_install`]): the release's MSI for this
//!   program's architecture is downloaded into the temporary folder ([`download`], never the
//!   captures folder), checked against the release's `SHA256SUMS.txt` (and against GitHub's own
//!   digest of the file when it gives one) and deleted if it doesn't match. [`install`] runs it.
//! - **Everywhere else** the owner is told, and the release page opens on request.
//!
//! ## Installing and starting again (Windows)
//!
//! The MSI installs either for everyone on this PC (Program Files; the default) or just for the
//! person installing it (`%LocalAppData%\Programs`), and Windows upgrades only a copy installed the
//! same way, so an update keeps this copy's [`Scope`] ([`scope_of`], from where this program
//! is): for everyone, `msiexec` as always, and Windows asks for permission (UAC); just for one
//! person, with `ALLUSERS=2 MSIINSTALLPERUSER=1`, and nothing is asked. While installing, the MSI
//! closes every `screengrabcraft.exe`: the capture service and its supervisor
//! (`packaging/windows`, `util:CloseApplication`). A passive install doesn't show the finish page
//! that would start it again, so [`install`] hands the work to a small helper outside
//! ScreenGrabCraft's own programs: `cmd.exe`, started detached and windowless, which waits for
//! `msiexec /i … /passive /norestart` to end and then starts `screengrabcraft.exe --background`
//! from where this copy is installed, the way signing in does. Whatever the installer did, the
//! service comes back:
//! - installed: the new version starts;
//! - the owner said no to the permission prompt, or the install failed and rolled back: the
//!   service was never closed (or the old version is back), so the new start finds it running,
//!   hands it a no-op and ends; the update stays offered.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

pub use semver::Version;
use serde_json::Value;

/// Where the latest release is asked for.
pub const LATEST: &str = "https://api.github.com/repos/billsecond/screengrabcraft/releases/latest";
/// Where the files of a release are downloaded from: the only place an installer comes from.
pub const DOWNLOADS: &str = "https://github.com/billsecond/screengrabcraft/releases/download/";
/// The checksums every release carries (`sha256sum` format).
pub const SUMS: &str = "SHA256SUMS.txt";
/// The most of GitHub's answer that is read.
pub const MAX_JSON: u64 = 2 * 1024 * 1024;
/// The most of `SHA256SUMS.txt` that is read.
pub const MAX_SUMS: u64 = 256 * 1024;
/// The largest installer that is downloaded.
pub const MAX_INSTALLER: u64 = 500 * 1024 * 1024;
/// The most assets of a release that are looked at.
const MAX_ASSETS: usize = 500;
/// The first automatic check, after the service starts.
pub const FIRST_CHECK: Duration = Duration::from_secs(60);
/// Then one every so often.
pub const EVERY: Duration = Duration::from_secs(6 * 60 * 60);
/// The file in the updates folder that says an install was started (its version).
const MARKER: &str = "installing.txt";

/// A release newer than this program, as GitHub describes it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Release {
    pub version: Version,
    pub assets: Vec<Asset>,
}

/// One file of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Asset {
    pub name: String,
    /// Always under [`DOWNLOADS`].
    pub url: String,
    pub size: u64,
    /// GitHub's own SHA-256 of the file, when it gives one.
    pub digest: Option<[u8; 32]>,
}

impl Release {
    pub fn asset(&self, name: &str) -> Option<&Asset> {
        self.assets.iter().find(|a| a.name == name)
    }

    /// The page that says what's new.
    pub fn page(&self) -> String {
        page(&self.version)
    }
}

/// The release page of a version.
pub fn page(version: &Version) -> String {
    format!("{}/releases/tag/v{version}", screengrabcraft_model::HOMEPAGE)
}

/// What a check found.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Found {
    /// Nothing newer. The latest release's version, when there is one to tell.
    UpToDate(Option<Version>),
    /// A newer release.
    Newer(Release),
}

/// A downloaded installer whose SHA-256 matched the release's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Installer {
    pub path: PathBuf,
    pub sha256: [u8; 32],
}

// ---- versions ----

/// A release's tag (`v0.1.7`) or a version (`0.1.7`) as a version.
pub fn parse_version(tag: &str) -> Option<Version> {
    let t = tag.trim();
    if t.len() > 64 {
        return None;
    }
    let t = t.strip_prefix(['v', 'V']).unwrap_or(t);
    Version::parse(t).ok()
}

/// This program's version.
pub fn current() -> Option<Version> {
    parse_version(crate::version::version()).or_else(|| parse_version(screengrabcraft_model::VERSION))
}

/// Whether `latest` comes after `current`, by semantic versioning's precedence (build metadata
/// doesn't count; a pre-release comes before its release).
pub fn is_newer(latest: &Version, current: &Version) -> bool {
    let l = (latest.major, latest.minor, latest.patch);
    let c = (current.major, current.minor, current.patch);
    l > c || (l == c && latest.pre > current.pre)
}

// ---- reading GitHub's answer ----

fn excerpt(text: &str) -> String {
    let short: String = text.chars().filter(|c| !c.is_control()).take(40).collect();
    format!("{short:?}")
}

/// A file name a release may have: no folders, nothing hidden, nothing strange.
fn plain_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && !name.starts_with('.')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_' | '+'))
}

fn parse_hex32(text: &str) -> Option<[u8; 32]> {
    let t = text.trim();
    if t.len() != 64 || !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let mut out = [0u8; 32];
    for (i, slot) in out.iter_mut().enumerate() {
        let pair = t.get(i * 2..i * 2 + 2)?;
        *slot = u8::from_str_radix(pair, 16).ok()?;
    }
    Some(out)
}

/// A SHA-256 written out, for the log.
pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

fn asset(v: &Value) -> Option<Asset> {
    let name = v.get("name")?.as_str()?;
    if !plain_name(name) {
        return None;
    }
    let url = v.get("browser_download_url")?.as_str()?;
    let well_formed = url.len() <= 1024 && url.is_ascii() && !url.chars().any(|c| c.is_ascii_whitespace() || c.is_ascii_control());
    if !well_formed || !url.starts_with(DOWNLOADS) || !url.ends_with(&format!("/{name}")) {
        return None;
    }
    let size = v.get("size").and_then(Value::as_u64).unwrap_or(0);
    let digest = v.get("digest").and_then(Value::as_str).and_then(|d| d.strip_prefix("sha256:")).and_then(parse_hex32);
    Some(Asset { name: name.to_string(), url: url.to_string(), size, digest })
}

/// Reads GitHub's description of a release. `Ok(None)` is a release that is not to be offered
/// (a draft or a pre-release); files that look wrong are left out.
pub fn parse_release(bytes: &[u8]) -> Result<Option<Release>, String> {
    if u64::try_from(bytes.len()).unwrap_or(u64::MAX) > MAX_JSON {
        return Err("GitHub's answer was larger than release information can be".to_string());
    }
    let v: Value = serde_json::from_slice(bytes).map_err(|_| "GitHub's answer wasn't release information".to_string())?;
    let obj = v.as_object().ok_or("GitHub's answer wasn't release information")?;
    let tag = obj.get("tag_name").and_then(Value::as_str).ok_or("the release has no version")?;
    let version = parse_version(tag).ok_or_else(|| format!("the release's version {} isn't a version", excerpt(tag)))?;
    let draft = obj.get("draft").and_then(Value::as_bool).unwrap_or(false);
    let pre = obj.get("prerelease").and_then(Value::as_bool).unwrap_or(false);
    if draft || pre || !version.pre.is_empty() {
        return Ok(None);
    }
    let assets = obj.get("assets").and_then(Value::as_array).map(|a| a.iter().take(MAX_ASSETS).filter_map(asset).collect()).unwrap_or_default();
    Ok(Some(Release { version, assets }))
}

/// What a check makes of GitHub's answer, for a program at `current`.
pub fn judge(bytes: &[u8], current: &Version) -> Result<Found, String> {
    Ok(match parse_release(bytes)? {
        Some(r) if is_newer(&r.version, current) => Found::Newer(r),
        Some(r) => Found::UpToDate(Some(r.version)),
        None => Found::UpToDate(None),
    })
}

// ---- which installer, and checking it ----

/// The installer this program updates itself with: the MSI of its own architecture. Only
/// Windows installs updates itself.
pub fn installer_name(version: &Version, os: &str, arch: &str) -> Option<String> {
    let arch = match (os, arch) {
        ("windows", "x86_64") => "x64",
        ("windows", "aarch64") => "arm64",
        ("windows", "x86") => "x86",
        _ => return None,
    };
    Some(format!("screengrabcraft-{version}-windows-{arch}.msi"))
}

/// The installer for this system in `release`.
pub fn pick_installer<'a>(release: &'a Release, os: &str, arch: &str) -> Option<&'a Asset> {
    release.asset(&installer_name(&release.version, os, arch)?)
}

/// The SHA-256 `SHA256SUMS.txt` gives for `name` (`<hash>  <name>` or `<hash> *<name>`).
pub fn sum_for(sums: &str, name: &str) -> Result<[u8; 32], String> {
    let mut found: Option<[u8; 32]> = None;
    for line in sums.lines().take(10_000) {
        let Some(hash) = line.get(..64).and_then(parse_hex32) else { continue };
        let rest = line.get(64..).unwrap_or_default();
        if !rest.starts_with([' ', '\t']) {
            continue;
        }
        let file = rest.trim_start();
        let file = file.strip_prefix('*').unwrap_or(file).trim_end();
        if file != name {
            continue;
        }
        match found {
            Some(h) if h != hash => return Err(format!("{SUMS} gives {name} two different checksums")),
            _ => found = Some(hash),
        }
    }
    found.ok_or_else(|| format!("{SUMS} doesn't list {name}"))
}

/// The SHA-256 of what `r` gives, reading at most `max` bytes.
fn sha256_of(mut r: impl Read, max: u64) -> std::io::Result<[u8; 32]> {
    let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
    let mut buf = vec![0u8; 64 * 1024];
    let mut total = 0u64;
    loop {
        let n = match r.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => n,
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        };
        total = total.saturating_add(u64::try_from(n).unwrap_or(u64::MAX));
        if total > max {
            return Err(std::io::Error::other("larger than an installer can be"));
        }
        ctx.update(buf.get(..n).unwrap_or_default());
    }
    let mut out = [0u8; 32];
    out.copy_from_slice(ctx.finish().as_ref().get(..32).unwrap_or(&[0u8; 32]));
    Ok(out)
}

/// The SHA-256 of a file.
pub fn sha256_file(path: &Path) -> Result<[u8; 32], String> {
    let f = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    sha256_of(std::io::BufReader::new(f), MAX_INSTALLER).map_err(|e| format!("{}: {e}", path.display()))
}

/// Checks a file against its SHA-256 and deletes it if it doesn't match.
pub fn verify_or_delete(path: &Path, expected: &[u8; 32]) -> Result<(), String> {
    let got = sha256_file(path)?;
    if got == *expected {
        return Ok(());
    }
    let _ = std::fs::remove_file(path);
    log::warn!("{} didn't match its checksum ({} instead of {}); deleted", path.display(), hex(&got), hex(expected));
    Err("the downloaded installer didn't match its checksum, so it was deleted".to_string())
}

/// Saves what `r` gives as `dest` if its SHA-256 is `expected` and it is no larger than `max`:
/// written to `dest.part` first and renamed once it has checked out, deleted otherwise.
pub fn save_verified(mut r: impl Read, dest: &Path, max: u64, expected: &[u8; 32]) -> Result<u64, String> {
    let mut part_name = dest.as_os_str().to_owned();
    part_name.push(".part");
    let part = PathBuf::from(part_name);
    let result = (|| -> Result<u64, String> {
        let mut f = std::fs::File::create(&part).map_err(|e| format!("{}: {e}", part.display()))?;
        let mut ctx = ring::digest::Context::new(&ring::digest::SHA256);
        let mut buf = vec![0u8; 64 * 1024];
        let mut total = 0u64;
        loop {
            let n = match r.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => n,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) => return Err(format!("the download stopped: {e}")),
            };
            total = total.saturating_add(u64::try_from(n).unwrap_or(u64::MAX));
            if total > max {
                return Err("the download was larger than an installer can be".to_string());
            }
            let chunk = buf.get(..n).unwrap_or_default();
            ctx.update(chunk);
            f.write_all(chunk).map_err(|e| format!("{}: {e}", part.display()))?;
        }
        f.flush().map_err(|e| format!("{}: {e}", part.display()))?;
        drop(f);
        let got = ctx.finish();
        if got.as_ref() != expected.as_slice() {
            log::warn!("the download didn't match its checksum ({} instead of {})", hex(got.as_ref()), hex(expected));
            return Err("the downloaded installer didn't match its checksum, so it was deleted".to_string());
        }
        std::fs::rename(&part, dest).map_err(|e| format!("{}: {e}", dest.display()))?;
        Ok(total)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

// ---- the network ----

/// The one header that says who asks: the program and its version, nothing about the owner.
pub fn user_agent() -> String {
    format!("ScreenGrabCraft/{}", crate::version::version())
}

/// An HTTPS-only client with no cookies, giving up after `total`.
fn agent(total: Duration) -> ureq::Agent {
    let config = ureq::Agent::config_builder()
        .https_only(true)
        .user_agent(user_agent())
        .max_redirects(5)
        .timeout_global(Some(total))
        .timeout_connect(Some(Duration::from_secs(20)))
        .timeout_per_read(Some(Duration::from_secs(60)))
        .build();
    ureq::Agent::new_with_config(config)
}

/// A network failure in plain words.
fn plain(e: ureq::Error) -> String {
    match e {
        ureq::Error::StatusCode(403 | 429) => "GitHub is limiting requests from this network for now".to_string(),
        ureq::Error::StatusCode(404) => "GitHub has no release of ScreenGrabCraft".to_string(),
        ureq::Error::StatusCode(c) if c >= 500 => format!("GitHub had a problem (error {c})"),
        ureq::Error::StatusCode(c) => format!("GitHub answered with error {c}"),
        ureq::Error::Timeout(_) => "GitHub didn't answer in time".to_string(),
        ureq::Error::HostNotFound => "GitHub couldn't be found: is this computer online?".to_string(),
        ureq::Error::ConnectionFailed => "GitHub couldn't be reached: is this computer online?".to_string(),
        ureq::Error::Io(io) => format!("GitHub couldn't be reached: {io}"),
        ureq::Error::BodyExceedsLimit(_) => "GitHub's answer was larger than expected".to_string(),
        ureq::Error::Tls(_) | ureq::Error::Rustls(_) => "the secure connection to GitHub failed".to_string(),
        other => format!("the connection to GitHub failed: {other}"),
    }
}

fn get(agent: &ureq::Agent, url: &str, accept: &str, max: u64) -> Result<Vec<u8>, String> {
    let mut resp = agent.get(url).header("Accept", accept).call().map_err(plain)?;
    resp.body_mut().with_config().limit(max).read_to_vec().map_err(plain)
}

/// Asks GitHub for the latest release and compares it with `current`.
pub fn check(current: &Version) -> Result<Found, String> {
    let body = get(&agent(Duration::from_secs(60)), LATEST, "application/vnd.github+json", MAX_JSON)?;
    judge(&body, current)
}

/// Downloads the installer for this system (`os`, `arch`: `std::env::consts`) from `release`
/// into `dir` and checks it. One already there and matching is kept.
pub fn download(release: &Release, dir: &Path, os: &str, arch: &str) -> Result<Installer, String> {
    let name = installer_name(&release.version, os, arch).ok_or("there is no installer for this system")?;
    let file = release.asset(&name).ok_or_else(|| format!("the release has no {name}"))?;
    if file.size > MAX_INSTALLER {
        return Err(format!("{name} is larger than an installer can be"));
    }
    let sums = release.asset(SUMS).ok_or_else(|| format!("the release has no {SUMS}"))?;
    // A slow line may need a while for the installer; each read still has a minute.
    let agent = agent(Duration::from_secs(60 * 60));
    let text = get(&agent, &sums.url, "application/octet-stream", MAX_SUMS)?;
    let expected = sum_for(&String::from_utf8_lossy(&text), &name)?;
    if file.digest.is_some_and(|d| d != expected) {
        return Err(format!("GitHub's checksum of {name} and {SUMS} disagree"));
    }
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let dest = dir.join(&name);
    if dest.is_file() {
        if sha256_file(&dest).is_ok_and(|h| h == expected) {
            return Ok(Installer { path: dest, sha256: expected });
        }
        let _ = std::fs::remove_file(&dest);
    }
    let mut resp = agent.get(&file.url).header("Accept", "application/octet-stream").call().map_err(plain)?;
    let reader = resp.body_mut().with_config().limit(MAX_INSTALLER).reader();
    let bytes = save_verified(reader, &dest, MAX_INSTALLER, &expected)?;
    log::info!("downloaded {name} ({bytes} bytes), SHA-256 {}", hex(&expected));
    Ok(Installer { path: dest, sha256: expected })
}

// ---- the updates folder ----

/// Where installers are downloaded: the temporary folder, never the captures folder.
pub fn updates_dir() -> PathBuf {
    std::env::temp_dir().join("ScreenGrabCraft").join("updates")
}

/// Deletes the installers in `dir` other than `keep` (old versions, broken downloads).
pub fn prune(dir: &Path, keep: Option<&Path>) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten().take(1000) {
        let name = e.file_name().to_string_lossy().to_string();
        let ours = name.starts_with("screengrabcraft-") && (name.ends_with(".msi") || name.ends_with(".part"));
        let path = e.path();
        if ours && keep != Some(path.as_path()) && path.is_file() {
            match std::fs::remove_file(&path) {
                Ok(()) => log::info!("removed the old download {name}"),
                Err(err) => log::warn!("removing the old download {name}: {err}"),
            }
        }
    }
}

/// The version of a downloaded installer, from its name.
fn installer_version(file_name: &str) -> Option<Version> {
    let rest = file_name.strip_prefix("screengrabcraft-")?.strip_suffix(".msi")?;
    parse_version(rest.rsplit_once("-windows-")?.0)
}

/// Tidies the updates folder when the program starts: broken downloads go, and so do installers
/// of versions this program has caught up with (installed, or installed some other way).
pub fn tidy(dir: &Path, current: &Version) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    for e in entries.flatten().take(1000) {
        let name = e.file_name().to_string_lossy().to_string();
        let broken = name.starts_with("screengrabcraft-") && name.ends_with(".part");
        let done = broken || installer_version(&name).is_some_and(|v| !is_newer(&v, current));
        let path = e.path();
        if done && path.is_file() {
            match std::fs::remove_file(&path) {
                Ok(()) => log::info!("removed the old download {name}"),
                Err(err) => log::warn!("removing the old download {name}: {err}"),
            }
        }
    }
}

/// The version whose install was started before this program started, if one was; the note
/// is removed.
pub fn take_marker(dir: &Path) -> Option<Version> {
    let path = dir.join(MARKER);
    let meta = std::fs::metadata(&path).ok()?;
    let text = if meta.len() <= 256 { std::fs::read_to_string(&path).ok() } else { None };
    let _ = std::fs::remove_file(&path);
    text.and_then(|t| parse_version(&t))
}

// ---- the timer ----

/// When the automatic checks happen: [`FIRST_CHECK`] after the start, then [`EVERY`] after
/// the last check; never while a capture or a recording is running (the check waits for it to
/// end), never when they are turned off.
#[derive(Clone, Copy, Debug)]
pub struct Schedule {
    next: Instant,
}

impl Schedule {
    pub fn new(started: Instant) -> Schedule {
        Schedule { next: started.checked_add(FIRST_CHECK).unwrap_or(started) }
    }

    /// Whether a check is due at `now`: they are `on`, nothing is being captured (`busy`), and
    /// it's time.
    pub fn due(&self, now: Instant, on: bool, busy: bool) -> bool {
        on && !busy && now >= self.next
    }

    /// A check started (automatic or asked for): the next automatic one is [`EVERY`] later.
    pub fn started(&mut self, now: Instant) {
        self.next = now.checked_add(EVERY).unwrap_or(now);
    }

    /// How long until the next check is due.
    pub fn wait(&self, now: Instant) -> Duration {
        self.next.saturating_duration_since(now)
    }
}

// ---- installing (Windows) ----

/// Who the MSI installed a copy for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    /// Everyone on this PC: Program Files, HKLM. Windows asks for permission to change it.
    Machine,
    /// Just the person who installed it: `%LocalAppData%\Programs`, HKCU. Nothing is asked.
    User,
}

impl Scope {
    /// What `msiexec` is given so an update stays in this scope: nothing for everyone (the MSI's
    /// own default), `ALLUSERS=2 MSIINSTALLPERUSER=1` for one person (Windows reads the second
    /// only with the first).
    pub fn msi_properties(self) -> &'static str {
        match self {
            Scope::Machine => "",
            Scope::User => " ALLUSERS=2 MSIINSTALLPERUSER=1",
        }
    }

    /// Whether Windows asks for permission (UAC) to install an update in this scope.
    pub fn asks_permission(self) -> bool {
        self == Scope::Machine
    }
}

/// A path as Windows compares them: no quotes, no `\\?\`, backslashes, any case.
fn plain_path(s: &str) -> String {
    s.trim().trim_matches('"').trim_start_matches(r"\\?\").replace('/', "\\").to_lowercase()
}

/// The scope of the installed program at `exe`, from where it is: in `%LocalAppData%\Programs`
/// (where Windows puts a per-user install; `local_app_data` is `%LocalAppData%`) it is the one
/// person's, anywhere else (Program Files) everyone's.
pub fn scope_of(exe: &Path, local_app_data: Option<&Path>) -> Scope {
    let Some(base) = local_app_data.map(|p| plain_path(&p.to_string_lossy())) else { return Scope::Machine };
    let base = base.trim_end_matches('\\');
    if base.is_empty() {
        return Scope::Machine;
    }
    if plain_path(&exe.to_string_lossy()).starts_with(&format!("{base}\\programs\\")) { Scope::User } else { Scope::Machine }
}

/// The command line `cmd.exe /d /v:off /s /c "…"` runs to install: msiexec, waited for (in
/// `scope`, so the update replaces this copy), then the capture service started again. Paths that
/// cmd would read differently (`%`, quotes, control characters) are refused rather than passed on.
pub fn helper_line(msiexec: &str, msi: &str, log: &str, service: &str, scope: Scope) -> Result<String, String> {
    for p in [msiexec, msi, log, service] {
        if p.trim().is_empty() || p.chars().any(|c| c == '"' || c == '%' || c.is_control()) {
            return Err(format!("the path {} can't be handed to the installer safely", excerpt(p)));
        }
    }
    let properties = scope.msi_properties();
    Ok(format!(r#"start "" /wait "{msiexec}" /i "{msi}"{properties} /passive /norestart /l*v "{log}" & start "" "{service}" --background"#))
}

/// Whether `registered` (a path the installer wrote) names `exe`.
pub fn same_program(registered: &str, exe: &Path) -> bool {
    let r = plain_path(registered);
    !r.is_empty() && r == plain_path(&exe.to_string_lossy())
}

/// Whether this copy can install updates itself: on Windows, the capture service the MSI
/// installed (the portable zip and a build in the source tree only tell the owner).
pub fn can_install() -> bool {
    install_scope().is_some()
}

/// Who the MSI installed this copy for, when it did (Windows only; see [`can_install`]).
pub fn install_scope() -> Option<Scope> {
    #[cfg(target_os = "windows")]
    {
        let exe = std::env::current_exe().ok()?;
        windows::installed_by_msi(&exe).then(|| scope_of(&exe, dirs::data_local_dir().as_deref()))
    }
    #[cfg(not(target_os = "windows"))]
    {
        None
    }
}

/// The helper running an install.
pub struct Installing {
    helper: std::process::Child,
}

impl Installing {
    /// Whether the helper has finished. A program that is still running to see it wasn't
    /// updated: the owner said no to the permission prompt, or the install failed (a successful
    /// install closes this program first).
    pub fn ended(&mut self) -> bool {
        !matches!(self.helper.try_wait(), Ok(None))
    }
}

/// Runs `installer` for `version` (see the module's notes): checked once more, then handed to
/// the helper. Returns once the helper has started; the installer asks for permission and
/// closes this program while it works.
pub fn install(installer: &Installer, version: &Version) -> Result<Installing, String> {
    #[cfg(target_os = "windows")]
    {
        windows::install(installer, version)
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = (installer, version);
        Err("ScreenGrabCraft installs updates itself only on Windows; get this one from its release page".to_string())
    }
}

#[cfg(target_os = "windows")]
mod windows {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    use super::*;

    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    const CREATE_BREAKAWAY_FROM_JOB: u32 = 0x0100_0000;
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    /// msiexec's log of the last install, in the updates folder.
    const INSTALL_LOG: &str = "install.log";
    const APP_PATH: &str = r"Software\Microsoft\Windows\CurrentVersion\App Paths\screengrabcraft.exe";

    /// Whether the MSI installed `exe`: its App Paths entry names it (in HKLM for everyone on
    /// this PC, in HKCU for one person).
    pub fn installed_by_msi(exe: &Path) -> bool {
        use winreg::RegKey;
        use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
        [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER]
            .into_iter()
            .any(|root| RegKey::predef(root).open_subkey(APP_PATH).and_then(|k| k.get_value::<String, _>("")).is_ok_and(|v| same_program(&v, exe)))
    }

    fn system32() -> PathBuf {
        let root = std::env::var_os("SystemRoot").or_else(|| std::env::var_os("windir")).map(PathBuf::from);
        root.unwrap_or_else(|| PathBuf::from(r"C:\Windows")).join("System32")
    }

    pub fn install(installer: &Installer, version: &Version) -> Result<Installing, String> {
        // Checked again: it waited in the temporary folder since it was downloaded.
        verify_or_delete(&installer.path, &installer.sha256)?;
        let exe = std::env::current_exe().map_err(|e| format!("this program's own path is unknown: {e}"))?;
        if !installed_by_msi(&exe) {
            return Err("this copy of ScreenGrabCraft wasn't installed by its installer, so it doesn't update itself".to_string());
        }
        let system = system32();
        let (cmd, msiexec) = (system.join("cmd.exe"), system.join("msiexec.exe"));
        if !cmd.is_file() || !msiexec.is_file() {
            return Err("Windows Installer wasn't found".to_string());
        }
        let dir = installer.path.parent().map(Path::to_path_buf).unwrap_or_else(updates_dir);
        let log_file = dir.join(INSTALL_LOG);
        // The update replaces this copy only in the scope it was installed in; for one person,
        // msiexec doesn't ask for permission.
        let scope = scope_of(&exe, dirs::data_local_dir().as_deref());
        let line =
            helper_line(&msiexec.to_string_lossy(), &installer.path.to_string_lossy(), &log_file.to_string_lossy(), &exe.to_string_lossy(), scope)?;
        let marker = dir.join(MARKER);
        let _ = std::fs::write(&marker, version.to_string());
        let spawn = |flags: u32| {
            Command::new(&cmd)
                .raw_arg(format!("/d /v:off /s /c \"{line}\""))
                .creation_flags(flags)
                // Not the install folder, which the installer replaces.
                .current_dir(&system)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
        };
        let detached = CREATE_NO_WINDOW | CREATE_NEW_PROCESS_GROUP;
        // Out of any job this program runs in, so the helper outlives it; a job that doesn't
        // allow that still lets it run, since nothing ends the job when this program closes.
        let started = spawn(detached | CREATE_BREAKAWAY_FROM_JOB).or_else(|_| spawn(detached));
        match started {
            Ok(helper) => {
                let whom = if scope == Scope::User { "just for this user" } else { "for everyone on this PC" };
                log::info!("installing ScreenGrabCraft {version} with {} ({whom})", installer.path.display());
                Ok(Installing { helper })
            }
            Err(e) => {
                let _ = std::fs::remove_file(&marker);
                Err(format!("the installer couldn't be started: {e}"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> Version {
        Version::parse(s).unwrap()
    }

    fn release_json(tag: &str, extra: &str) -> String {
        format!(
            r#"{{"tag_name":"{tag}","draft":false,"prerelease":false,"html_url":"https://github.com/billsecond/screengrabcraft/releases/tag/{tag}"{extra},
            "assets":[
              {{"name":"screengrabcraft-0.1.8-windows-x64.msi","size":14741504,"digest":"sha256:{h}",
                "browser_download_url":"https://github.com/billsecond/screengrabcraft/releases/download/{tag}/screengrabcraft-0.1.8-windows-x64.msi"}},
              {{"name":"screengrabcraft-0.1.8-windows-arm64.msi","size":13312000,"digest":null,
                "browser_download_url":"https://github.com/billsecond/screengrabcraft/releases/download/{tag}/screengrabcraft-0.1.8-windows-arm64.msi"}},
              {{"name":"SHA256SUMS.txt","size":2393,
                "browser_download_url":"https://github.com/billsecond/screengrabcraft/releases/download/{tag}/SHA256SUMS.txt"}},
              {{"name":"evil.msi","size":1,"browser_download_url":"https://evil.example/evil.msi"}},
              {{"name":"../up.msi","size":1,"browser_download_url":"https://github.com/billsecond/screengrabcraft/releases/download/{tag}/../up.msi"}},
              {{"name":42}},
              "not an asset"
            ]}}"#,
            h = "bb753463a59a07ceee009e371044621dd5ee6db4e1077a6d2e388a84ed8cf2f1"
        )
    }

    #[test]
    fn versions_compare_by_semver() {
        assert_eq!(parse_version("v0.1.7"), Some(v("0.1.7")));
        assert_eq!(parse_version(" V1.2.3 "), Some(v("1.2.3")));
        assert_eq!(parse_version("0.2.0-rc.1"), Some(v("0.2.0-rc.1")));
        for bad in ["", "v", "latest", "1.2", "v1.2.3.4", "vv1.2.3", &"9".repeat(100)] {
            assert_eq!(parse_version(bad), None, "{bad}");
        }
        assert!(is_newer(&v("0.1.8"), &v("0.1.7")));
        assert!(is_newer(&v("0.2.0"), &v("0.1.10")));
        assert!(is_newer(&v("0.1.10"), &v("0.1.9")), "numbers, not text");
        assert!(is_newer(&v("1.0.0"), &v("0.99.99")));
        assert!(!is_newer(&v("0.1.7"), &v("0.1.7")));
        assert!(!is_newer(&v("0.1.6"), &v("0.1.7")));
        assert!(is_newer(&v("0.2.0"), &v("0.2.0-rc.1")), "a release comes after its pre-release");
        assert!(!is_newer(&v("0.2.0-rc.1"), &v("0.2.0")));
        assert!(!is_newer(&v("0.1.7+build.5"), &v("0.1.7")), "build metadata doesn't count");
        assert!(current().is_some(), "this program's own version reads");
    }

    #[test]
    fn a_release_is_read_and_its_odd_files_left_out() {
        let r = parse_release(release_json("v0.1.8", "").as_bytes()).unwrap().unwrap();
        assert_eq!(r.version, v("0.1.8"));
        let names: Vec<&str> = r.assets.iter().map(|a| a.name.as_str()).collect();
        assert_eq!(names, ["screengrabcraft-0.1.8-windows-x64.msi", "screengrabcraft-0.1.8-windows-arm64.msi", "SHA256SUMS.txt"]);
        let x64 = r.asset("screengrabcraft-0.1.8-windows-x64.msi").unwrap();
        assert_eq!(x64.size, 14_741_504);
        assert_eq!(x64.digest.map(|d| hex(&d)).as_deref(), Some("bb753463a59a07ceee009e371044621dd5ee6db4e1077a6d2e388a84ed8cf2f1"));
        assert_eq!(r.asset("screengrabcraft-0.1.8-windows-arm64.msi").unwrap().digest, None);
        assert_eq!(r.page(), "https://github.com/billsecond/screengrabcraft/releases/tag/v0.1.8");
        assert!(DOWNLOADS.starts_with(screengrabcraft_model::HOMEPAGE));
    }

    #[test]
    fn newer_or_up_to_date() {
        let json = release_json("v0.1.8", "");
        assert!(matches!(judge(json.as_bytes(), &v("0.1.7")), Ok(Found::Newer(r)) if r.version == v("0.1.8")));
        assert_eq!(judge(json.as_bytes(), &v("0.1.8")), Ok(Found::UpToDate(Some(v("0.1.8")))));
        assert_eq!(judge(json.as_bytes(), &v("0.2.0")), Ok(Found::UpToDate(Some(v("0.1.8")))), "a newer build isn't downgraded");
    }

    #[test]
    fn drafts_and_prereleases_are_never_offered() {
        let draft = release_json("v0.1.8", "").replace(r#""draft":false"#, r#""draft":true"#);
        assert_eq!(parse_release(draft.as_bytes()), Ok(None));
        let pre = release_json("v0.1.8", "").replace(r#""prerelease":false"#, r#""prerelease":true"#);
        assert_eq!(parse_release(pre.as_bytes()), Ok(None));
        let rc = release_json("v0.2.0-rc.1", "");
        assert_eq!(parse_release(rc.as_bytes()), Ok(None), "a pre-release version, whatever the flag says");
        assert_eq!(judge(rc.as_bytes(), &v("0.1.7")), Ok(Found::UpToDate(None)));
    }

    #[test]
    fn junk_missing_fields_and_huge_answers_are_errors() {
        for junk in [
            &b""[..],
            b"null",
            b"[]",
            b"\"v9.9.9\"",
            b"<html>rate limited</html>",
            b"{",
            b"{}",
            br#"{"tag_name":null}"#,
            br#"{"tag_name":"nightly"}"#,
            br#"{"tag_name":12}"#,
            b"\xff\xfe\x00{",
        ] {
            assert!(parse_release(junk).is_err(), "{:?}", String::from_utf8_lossy(junk));
        }
        // Missing everything but the version: a release with no files (nothing to download).
        let bare = parse_release(br#"{"tag_name":"v0.1.8"}"#).unwrap().unwrap();
        assert!(bare.assets.is_empty());
        assert!(pick_installer(&bare, "windows", "x86_64").is_none());
        // Files of the wrong shape are dropped, the rest kept.
        let odd = parse_release(br#"{"tag_name":"v0.1.8","draft":"no","assets":{"a":1}}"#).unwrap().unwrap();
        assert!(odd.assets.is_empty());
        // Too large to be read at all.
        let huge = format!(r#"{{"tag_name":"v0.1.8","body":"{}"}}"#, "x".repeat(usize::try_from(MAX_JSON).unwrap()));
        assert!(parse_release(huge.as_bytes()).unwrap_err().contains("larger"));
        // Nested beyond the parser's limit: an error, not a stack overflow.
        let deep = format!("{}{}", "[".repeat(100_000), "]".repeat(100_000));
        assert!(parse_release(deep.as_bytes()).is_err());
        // A long hostile version doesn't end up whole in the message.
        let long = format!(r#"{{"tag_name":"{}"}}"#, "z".repeat(10_000));
        assert!(parse_release(long.as_bytes()).unwrap_err().len() < 120);
    }

    #[test]
    fn the_installer_is_the_msi_of_this_architecture() {
        let r = parse_release(release_json("v0.1.8", "").as_bytes()).unwrap().unwrap();
        assert_eq!(pick_installer(&r, "windows", "x86_64").map(|a| a.name.as_str()), Some("screengrabcraft-0.1.8-windows-x64.msi"));
        assert_eq!(pick_installer(&r, "windows", "aarch64").map(|a| a.name.as_str()), Some("screengrabcraft-0.1.8-windows-arm64.msi"));
        assert_eq!(pick_installer(&r, "windows", "x86"), None, "not in this release");
        assert_eq!(installer_name(&v("0.1.8"), "windows", "x86").as_deref(), Some("screengrabcraft-0.1.8-windows-x86.msi"));
        for (os, arch) in
            [("macos", "aarch64"), ("macos", "x86_64"), ("linux", "x86_64"), ("linux", "aarch64"), ("freebsd", "x86_64"), ("windows", "riscv64")]
        {
            assert_eq!(installer_name(&v("0.1.8"), os, arch), None, "{os} {arch}");
            assert_eq!(pick_installer(&r, os, arch), None, "{os} {arch}");
        }
    }

    #[test]
    fn sha256sums_are_read() {
        let sums = "5770786aa3698e70d0e5df504c6344210407103e7884481d110f45db4ab8a13b  screengrabcraft-0.1.7-freebsd-x86_64.tar.gz\n\
                    bb753463a59a07ceee009e371044621dd5ee6db4e1077a6d2e388a84ed8cf2f1  screengrabcraft-0.1.7-windows-x64.msi\r\n\
                    3CF423082F9767E4184A6412E7F76D1D5093913F13E645C60AD5CE1CF2BA882A *screengrabcraft-0.1.7-windows-arm64.msi\n\
                    garbage line\n\
                    é\n";
        let x64 = sum_for(sums, "screengrabcraft-0.1.7-windows-x64.msi").unwrap();
        assert_eq!(hex(&x64), "bb753463a59a07ceee009e371044621dd5ee6db4e1077a6d2e388a84ed8cf2f1");
        let arm = sum_for(sums, "screengrabcraft-0.1.7-windows-arm64.msi").unwrap();
        assert_eq!(hex(&arm), "3cf423082f9767e4184a6412e7f76d1d5093913f13e645c60ad5ce1cf2ba882a");
        assert!(sum_for(sums, "screengrabcraft-0.1.7-windows-x86.msi").unwrap_err().contains("doesn't list"));
        assert!(sum_for(sums, "screengrabcraft-0.1.7-windows-x64").is_err(), "the whole name");
        let twice = format!("{sums}{}  screengrabcraft-0.1.7-windows-x64.msi\n", "0".repeat(64));
        assert!(sum_for(&twice, "screengrabcraft-0.1.7-windows-x64.msi").unwrap_err().contains("two"));
        let same_twice = format!("{sums}{sums}");
        assert!(sum_for(&same_twice, "screengrabcraft-0.1.7-windows-x64.msi").is_ok());
        assert!(sum_for("", "x").is_err());
        assert!(sum_for(&"+".repeat(64), "x").is_err());
        assert!(sum_for(&format!("{}x  name", "a".repeat(63)), "name").is_err(), "not hex");
    }

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("sgc-update-test-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn sha(bytes: &[u8]) -> [u8; 32] {
        sha256_of(bytes, u64::MAX).unwrap()
    }

    #[test]
    fn a_download_is_kept_only_if_it_matches() {
        let dir = scratch("download");
        let data = b"an installer, honestly".repeat(1000);
        let want = sha(&data);
        assert_eq!(hex(&sha(b"abc")), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");

        let dest = dir.join("screengrabcraft-9.9.9-windows-x64.msi");
        assert_eq!(save_verified(&data[..], &dest, MAX_INSTALLER, &want), Ok(u64::try_from(data.len()).unwrap()));
        assert_eq!(std::fs::read(&dest).unwrap(), data);
        assert!(verify_or_delete(&dest, &want).is_ok());
        assert!(dest.exists());

        // Tampered with after the download: deleted when checked again.
        std::fs::write(&dest, b"something else").unwrap();
        assert!(verify_or_delete(&dest, &want).unwrap_err().contains("deleted"));
        assert!(!dest.exists());

        // A download that doesn't match is never kept, nor its part file.
        let mut bad = data.clone();
        bad[10] ^= 1;
        assert!(save_verified(&bad[..], &dest, MAX_INSTALLER, &want).unwrap_err().contains("checksum"));
        assert!(!dest.exists());
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0, "no part file left");

        // Nor one larger than allowed.
        assert!(save_verified(&data[..], &dest, 100, &want).unwrap_err().contains("larger"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);

        // Nor one that broke off.
        struct Broken(usize);
        impl Read for Broken {
            fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
                if self.0 == 0 {
                    return Err(std::io::Error::new(std::io::ErrorKind::ConnectionReset, "reset"));
                }
                self.0 -= 1;
                buf[0] = 1;
                Ok(1)
            }
        }
        assert!(save_verified(Broken(5), &dest, MAX_INSTALLER, &want).unwrap_err().contains("stopped"));
        assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 0);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn old_downloads_are_pruned_and_the_marker_read_once() {
        let dir = scratch("prune");
        let keep = dir.join("screengrabcraft-0.1.9-windows-x64.msi");
        for name in [
            "screengrabcraft-0.1.8-windows-x64.msi",
            "screengrabcraft-0.1.9-windows-x64.msi",
            "screengrabcraft-0.1.9-windows-x64.msi.part",
            "install.log",
            "notes.txt",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        prune(&dir, Some(&keep));
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, ["install.log", "notes.txt", "screengrabcraft-0.1.9-windows-x64.msi"]);
        prune(&dir, None);
        assert!(!keep.exists());
        prune(&dir.join("not there"), None);

        // At the start: what this version has caught up with goes, what's still to come stays.
        for name in [
            "screengrabcraft-0.1.6-windows-x64.msi",
            "screengrabcraft-0.1.7-windows-arm64.msi",
            "screengrabcraft-0.1.8-windows-x64.msi",
            "screengrabcraft-0.1.8-windows-x64.msi.part",
            "screengrabcraft-junk.msi",
        ] {
            std::fs::write(dir.join(name), b"x").unwrap();
        }
        tidy(&dir, &v("0.1.7"));
        let mut left: Vec<String> = std::fs::read_dir(&dir).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().to_string()).collect();
        left.sort();
        assert_eq!(left, ["install.log", "notes.txt", "screengrabcraft-0.1.8-windows-x64.msi", "screengrabcraft-junk.msi"]);
        assert_eq!(installer_version("screengrabcraft-0.2.0-rc.1-windows-arm64.msi"), Some(v("0.2.0-rc.1")));
        tidy(&dir.join("not there"), &v("0.1.7"));

        assert_eq!(take_marker(&dir), None);
        std::fs::write(dir.join(MARKER), "0.1.9\n").unwrap();
        assert_eq!(take_marker(&dir), Some(v("0.1.9")));
        assert_eq!(take_marker(&dir), None, "read once");
        std::fs::write(dir.join(MARKER), "x".repeat(10_000)).unwrap();
        assert_eq!(take_marker(&dir), None);
        assert!(!dir.join(MARKER).exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checks_wait_for_captures_and_recordings() {
        let t0 = Instant::now();
        let mut s = Schedule::new(t0);
        assert!(!s.due(t0, true, false), "not at once");
        let first = t0 + FIRST_CHECK;
        assert!(s.due(first, true, false));
        assert!(!s.due(first, false, false), "turned off");
        assert!(!s.due(first, true, true), "never during a capture or a recording");
        // The recording goes on for a while: still nothing, then the check comes when it ends.
        let later = first + Duration::from_secs(600);
        assert!(!s.due(later, true, true));
        assert!(s.due(later, true, false));
        s.started(later);
        assert!(!s.due(later + Duration::from_secs(60), true, false));
        assert_eq!(s.wait(later), EVERY);
        assert!(s.due(later + EVERY, true, false));
        assert_eq!(s.wait(later + EVERY + EVERY), Duration::ZERO);
    }

    #[test]
    fn the_helper_runs_the_installer_then_starts_the_service() {
        // For everyone on this PC: exactly as before per-user installs existed.
        let line = helper_line(
            r"C:\Windows\System32\msiexec.exe",
            r"C:\Users\Ann Lee\AppData\Local\Temp\ScreenGrabCraft\updates\screengrabcraft-0.1.8-windows-x64.msi",
            r"C:\Users\Ann Lee\AppData\Local\Temp\ScreenGrabCraft\updates\install.log",
            r"C:\Program Files\ScreenGrabCraft\screengrabcraft.exe",
            Scope::Machine,
        )
        .unwrap();
        assert_eq!(
            line,
            r#"start "" /wait "C:\Windows\System32\msiexec.exe" /i "C:\Users\Ann Lee\AppData\Local\Temp\ScreenGrabCraft\updates\screengrabcraft-0.1.8-windows-x64.msi" /passive /norestart /l*v "C:\Users\Ann Lee\AppData\Local\Temp\ScreenGrabCraft\updates\install.log" & start "" "C:\Program Files\ScreenGrabCraft\screengrabcraft.exe" --background"#
        );
        // Just for one person: the per-user properties, so the update replaces that copy and
        // Windows asks nothing; the service starts again from the same place.
        let line = helper_line(
            r"C:\Windows\System32\msiexec.exe",
            r"C:\Users\Ann Lee\AppData\Local\Temp\ScreenGrabCraft\updates\screengrabcraft-0.1.8-windows-x64.msi",
            r"C:\Users\Ann Lee\AppData\Local\Temp\ScreenGrabCraft\updates\install.log",
            r"C:\Users\Ann Lee\AppData\Local\Programs\ScreenGrabCraft\screengrabcraft.exe",
            Scope::User,
        )
        .unwrap();
        assert_eq!(
            line,
            r#"start "" /wait "C:\Windows\System32\msiexec.exe" /i "C:\Users\Ann Lee\AppData\Local\Temp\ScreenGrabCraft\updates\screengrabcraft-0.1.8-windows-x64.msi" ALLUSERS=2 MSIINSTALLPERUSER=1 /passive /norestart /l*v "C:\Users\Ann Lee\AppData\Local\Temp\ScreenGrabCraft\updates\install.log" & start "" "C:\Users\Ann Lee\AppData\Local\Programs\ScreenGrabCraft\screengrabcraft.exe" --background"#
        );
        // `&`, `^` and parentheses are plain text inside quotes; `%` and quotes are not.
        for scope in [Scope::Machine, Scope::User] {
            assert!(helper_line("a", r"C:\Users\R&D (1)^\x.msi", "c", "d", scope).is_ok());
            for bad in [r"C:\Users\50%off\x.msi", "C:\\a\"b", "C:\\a\nb", "", " "] {
                assert!(helper_line("a", bad, "c", "d", scope).is_err(), "{bad:?}");
                assert!(helper_line("a", "b", "c", bad, scope).is_err(), "{bad:?}");
            }
        }
    }

    #[test]
    fn the_scope_is_where_the_program_is() {
        let local = Path::new(r"C:\Users\Ann Lee\AppData\Local");
        let user = |p: &str| scope_of(Path::new(p), Some(local));
        assert_eq!(user(r"C:\Users\Ann Lee\AppData\Local\Programs\ScreenGrabCraft\screengrabcraft.exe"), Scope::User);
        // Windows' paths come in any case, with `\\?\`, forward slashes, a trailing separator.
        assert_eq!(user(r"\\?\c:\users\ann lee\appdata\local\PROGRAMS\ScreenGrabCraft\screengrabcraft.exe"), Scope::User);
        assert_eq!(user("C:/Users/Ann Lee/AppData/Local/Programs/ScreenGrabCraft/screengrabcraft.exe"), Scope::User);
        assert_eq!(
            scope_of(
                Path::new(r"C:\Users\Ann Lee\AppData\Local\Programs\x\screengrabcraft.exe"),
                Some(Path::new(r"C:\Users\Ann Lee\AppData\Local\"))
            ),
            Scope::User
        );
        // Program Files (64- or 32-bit) is everyone's; so is anything else.
        assert_eq!(user(r"C:\Program Files\ScreenGrabCraft\screengrabcraft.exe"), Scope::Machine);
        assert_eq!(user(r"C:\Program Files (x86)\ScreenGrabCraft\screengrabcraft.exe"), Scope::Machine);
        // Near misses: elsewhere in LocalAppData, a folder whose name only starts the same,
        // another person's Programs.
        assert_eq!(user(r"C:\Users\Ann Lee\AppData\Local\ScreenGrabCraft\screengrabcraft.exe"), Scope::Machine);
        assert_eq!(user(r"C:\Users\Ann Lee\AppData\Local\ProgramsOld\ScreenGrabCraft\screengrabcraft.exe"), Scope::Machine);
        assert_eq!(user(r"C:\Users\Bo\AppData\Local\Programs\ScreenGrabCraft\screengrabcraft.exe"), Scope::Machine);
        assert_eq!(user(r"C:\Users\Ann Lee\AppData\Local\Programs"), Scope::Machine);
        // Without a LocalAppData (or an empty one) nothing is taken for a per-user install.
        assert_eq!(scope_of(Path::new(r"C:\Users\Ann Lee\AppData\Local\Programs\x.exe"), None), Scope::Machine);
        assert_eq!(scope_of(Path::new(r"\programs\x.exe"), Some(Path::new(""))), Scope::Machine);
        assert_eq!(scope_of(Path::new(""), Some(local)), Scope::Machine);

        assert!(Scope::Machine.asks_permission());
        assert!(!Scope::User.asks_permission());
        assert_eq!(Scope::Machine.msi_properties(), "");
        assert_eq!(Scope::User.msi_properties(), " ALLUSERS=2 MSIINSTALLPERUSER=1");
    }

    #[test]
    fn the_installed_program_is_recognised() {
        let exe = Path::new(r"C:\Program Files\ScreenGrabCraft\screengrabcraft.exe");
        assert!(same_program(r"C:\Program Files\ScreenGrabCraft\screengrabcraft.exe", exe));
        assert!(same_program(r#""c:\program files\screengrabcraft\SCREENGRABCRAFT.EXE""#, exe));
        assert!(same_program(
            r"C:\Program Files\ScreenGrabCraft\screengrabcraft.exe",
            Path::new(r"\\?\C:\Program Files\ScreenGrabCraft\screengrabcraft.exe")
        ));
        assert!(!same_program(r"C:\Program Files\ScreenGrabCraft\screengrabcraft.exe", Path::new(r"D:\portable\screengrabcraft.exe")));
        assert!(!same_program("", Path::new("")));
    }
}
