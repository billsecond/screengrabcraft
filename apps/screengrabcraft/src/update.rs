//! Updates in the capture service: the automatic check (a minute after the start, then every six
//! hours, never while a capture or a recording is under way), the notice when a newer release
//! is found, the tray's items, and the About tab's switch and Check now. The network and the
//! installing are `screengrabcraft_desktop::update`'s; this is when, and what the owner sees.
//!
//! On Windows, in a copy the installer put there, a newer release's MSI is downloaded and
//! checked before the owner hears of it, and Update now installs it (for a copy installed for
//! everyone, Windows asks for permission; one installed just for the owner updates without
//! asking; the service starts again afterwards). Elsewhere the notice opens the release page.

use std::sync::mpsc::{Receiver, Sender, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{Color32, RichText, Ui, ViewportBuilder, ViewportId, pos2, vec2};
use screengrabcraft_desktop::update::{self, Found, Installer, Installing, Schedule, Scope, Version};
use screengrabcraft_desktop::{launch, paths};
use screengrabcraft_ui::theme::Tokens;

use crate::app::{APP_ID, Service};
use crate::tray::Item;

/// How long the notice of an update stays up (longer while the pointer is on it).
const NOTICE: Duration = Duration::from_secs(30);
/// How long an install is waited for before Update now may be pressed again.
const INSTALL_WAIT: Duration = Duration::from_secs(30 * 60);
const DARK: Color32 = Color32::from_rgb(0x1F, 0x1F, 0x25);
const RELEASES: &str = "https://github.com/billsecond/screengrabcraft/releases";

/// Who asked for a check, and so who hears the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Asker {
    /// The automatic check: the owner hears only of an update.
    Timer,
    /// Check for Updates… in the tray: the answer is a notice, whatever it is.
    Tray,
    /// Check now in the settings: the answer is shown there.
    Settings,
}

/// What the check's thread reports.
enum Report {
    Downloading(Version),
    Done(Result<Outcome, String>),
}

enum Outcome {
    UpToDate(Option<Version>),
    Offer(Offer),
}

/// A newer release, waiting for the owner.
#[derive(Clone, Debug)]
pub struct Offer {
    pub version: Version,
    pub page: String,
    /// The checked installer (Windows, installed copies); without it the release page is the way.
    pub installer: Option<Installer>,
    /// Why there is no installer although this copy installs its updates (a failed download).
    pub problem: Option<String>,
}

/// What the About tab says of the last check.
#[derive(Clone, Debug, Default)]
enum Status {
    #[default]
    Idle,
    Checking,
    Downloading(Version),
    UpToDate(Option<Version>),
    Failed(String),
}

pub struct Updates {
    schedule: Schedule,
    job: Option<(Asker, Receiver<Report>)>,
    /// A check asked for, started once nothing is being captured.
    wanted: Option<Asker>,
    status: Status,
    pub offer: Option<Offer>,
    /// The notice of the offer is up until then.
    notice: Option<Instant>,
    /// Later was chosen for this version: no notice of it again in this run.
    dismissed: Option<Version>,
    installing: Option<(Installing, Instant)>,
    /// This copy installs updates itself (Windows, installed by the MSI).
    can_install: bool,
    /// Windows asks for permission to install them (installed for everyone on this PC, not just
    /// for the owner).
    asks_permission: bool,
    /// Checks happen by themselves (not in a run without settings, nor in the Flatpak).
    automatic: bool,
    /// The Flatpak: no network at all.
    flatpak: bool,
    /// The version an update installed just before this start, to be told once.
    just_updated: Option<Version>,
}

impl Updates {
    pub fn new() -> Updates {
        let flatpak = std::env::var_os("FLATPAK_ID").is_some();
        let scope = update::install_scope();
        let can_install = scope.is_some();
        let dir = update::updates_dir();
        let mut just_updated = None;
        if let Some(current) = update::current() {
            match update::take_marker(&dir) {
                Some(v) if v == current => just_updated = Some(v),
                Some(v) => log::warn!("the update to {v} wasn't installed; this is still {current}"),
                None => {}
            }
            update::tidy(&dir, &current);
        }
        Updates {
            schedule: Schedule::new(Instant::now()),
            job: None,
            wanted: None,
            status: Status::Idle,
            offer: None,
            notice: None,
            dismissed: None,
            installing: None,
            can_install,
            asks_permission: scope.is_none_or(Scope::asks_permission),
            automatic: !paths::no_prefs() && !flatpak,
            flatpak,
            just_updated,
        }
    }

    /// Whether the notice of an update is (to be) on screen.
    pub fn notice_up(&self) -> bool {
        self.notice.is_some() && self.offer.is_some()
    }

    /// Takes the notice down (a capture is starting; the tray item stays).
    pub fn close_notice(&mut self) {
        self.notice = None;
    }

    fn checking(&self) -> bool {
        self.job.is_some() || self.wanted.is_some()
    }
}

/// The tray's update items: the waiting update, if any, and Check for Updates….
pub fn tray_items(waiting: Option<&str>) -> Vec<(String, Item)> {
    let mut items = Vec::new();
    if let Some(w) = waiting {
        items.push((w.to_string(), Item::InstallUpdate));
    }
    items.push(("Check for Updates…".to_string(), Item::CheckUpdates));
    items
}

/// What the tray's item for an offer says.
fn tray_label(offer: Option<&Offer>) -> Option<String> {
    offer.map(|o| if o.installer.is_some() { format!("Install Update {}", o.version) } else { format!("Get Update {}…", o.version) })
}

fn sync_tray(svc: &mut Service) {
    let label = tray_label(svc.updates.offer.as_ref());
    if let Some(t) = &mut svc.tray
        && t.update != label
    {
        t.update = label;
        t.rebuild(&svc.settings, &svc.keys);
    }
}

/// What happens when an update is installed, for a copy installed for everyone on this PC
/// (`asks`: Windows asks for permission) or just for the owner (nothing is asked).
fn install_note(asks: bool) -> &'static str {
    if asks {
        "Windows asks for permission, then ScreenGrabCraft starts again."
    } else {
        "ScreenGrabCraft closes while it installs, then starts again."
    }
}

fn current_text() -> String {
    update::current().map(|v| v.to_string()).unwrap_or_else(|| screengrabcraft_desktop::version::version().to_string())
}

// ---- checking ----

/// The check itself (on its own thread): the latest release, and on Windows its installer.
fn run(can_install: bool, tx: &Sender<Report>, ctx: &egui::Context) -> Result<Outcome, String> {
    let current = update::current().ok_or("this program's version can't be compared with a release's")?;
    match update::check(&current)? {
        Found::UpToDate(latest) => {
            match &latest {
                Some(v) => log::info!("update check: up to date ({current}; the latest release is {v})"),
                None => log::info!("update check: up to date ({current}; no release to offer)"),
            }
            Ok(Outcome::UpToDate(latest))
        }
        Found::Newer(release) => {
            log::info!("update check: ScreenGrabCraft {} is available (this is {current})", release.version);
            let mut offer = Offer { version: release.version.clone(), page: release.page(), installer: None, problem: None };
            if can_install {
                let _ = tx.send(Report::Downloading(release.version.clone()));
                ctx.request_repaint();
                let dir = update::updates_dir();
                match update::download(&release, &dir, std::env::consts::OS, std::env::consts::ARCH) {
                    Ok(i) => {
                        update::prune(&dir, Some(&i.path));
                        offer.installer = Some(i);
                    }
                    Err(e) => {
                        log::warn!("downloading the update: {e}");
                        offer.problem = Some(e);
                    }
                }
            }
            Ok(Outcome::Offer(offer))
        }
    }
}

fn start_job(svc: &mut Service, asker: Asker) {
    let (tx, rx) = channel();
    let can_install = svc.updates.can_install;
    let ctx = svc.ctx.clone();
    let spawned = std::thread::Builder::new().name("update".into()).spawn(move || {
        let r = run(can_install, &tx, &ctx);
        let _ = tx.send(Report::Done(r));
        ctx.request_repaint();
    });
    match spawned {
        Ok(_) => {
            svc.updates.job = Some((asker, rx));
            svc.updates.status = Status::Checking;
        }
        Err(e) => {
            log::warn!("the update check couldn't start: {e}");
            svc.updates.status = Status::Failed(e.to_string());
        }
    }
}

/// A check is asked for by the owner.
fn ask(svc: &mut Service, asker: Asker) {
    if svc.updates.flatpak {
        svc.notify(
            "ScreenGrabCraft can't check for updates here",
            "The Flatpak has no network access. New versions are on the releases page.",
            None,
            None,
            false,
        );
        return;
    }
    match &mut svc.updates.job {
        // One is running: its answer goes to whoever asked last.
        Some((a, _)) => *a = asker,
        None => svc.updates.wanted = Some(asker),
    }
    svc.ctx.request_repaint();
}

/// The tray's Check for Updates….
pub fn ask_from_tray(svc: &mut Service) {
    ask(svc, Asker::Tray);
}

fn finish(svc: &mut Service, asker: Asker, result: Result<Outcome, String>) {
    match result {
        Ok(Outcome::UpToDate(latest)) => {
            svc.updates.status = Status::UpToDate(latest);
            // An offer the release no longer stands behind goes.
            svc.updates.offer = None;
            svc.updates.notice = None;
            sync_tray(svc);
            if asker == Asker::Tray {
                svc.notify("ScreenGrabCraft is up to date", format!("Version {} is the latest.", current_text()), None, None, false);
            }
        }
        Ok(Outcome::Offer(offer)) => {
            svc.updates.status = Status::Idle;
            // The automatic check tells of an update once it's ready, and once a run if put off.
            let complete = offer.installer.is_some() || !svc.updates.can_install;
            let tell = match asker {
                Asker::Tray => true,
                Asker::Settings => false,
                Asker::Timer => complete && svc.updates.dismissed.as_ref() != Some(&offer.version),
            };
            svc.updates.offer = Some(offer);
            if tell {
                svc.updates.notice = Instant::now().checked_add(NOTICE);
            }
            sync_tray(svc);
        }
        Err(e) => {
            log::warn!("the update check failed: {e}");
            if asker == Asker::Tray {
                svc.notify("ScreenGrabCraft couldn't check for updates", e.clone(), None, None, true);
            }
            svc.updates.status = Status::Failed(e);
        }
    }
}

/// Every frame of the service: answers, the timer, and the install being waited for.
pub fn tick(svc: &mut Service, ctx: &egui::Context) {
    let now = Instant::now();
    if let Some(v) = svc.updates.just_updated.take() {
        svc.notify(format!("ScreenGrabCraft {v} is installed"), "What's new is on its release page.", None, None, false);
    }
    // The check's answer.
    let mut done = None;
    if let Some((asker, rx)) = &svc.updates.job {
        loop {
            match rx.try_recv() {
                Ok(Report::Downloading(v)) => svc.updates.status = Status::Downloading(v),
                Ok(Report::Done(r)) => {
                    done = Some((*asker, r));
                    break;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    done = Some((*asker, Err("the check stopped unexpectedly".to_string())));
                    break;
                }
            }
        }
    }
    if let Some((asker, r)) = done {
        svc.updates.job = None;
        finish(svc, asker, r);
    }
    // A check to start: asked for, or the timer's. Never while capturing.
    let busy = svc.capturing();
    if svc.updates.job.is_none() && !busy {
        let auto = svc.updates.automatic && svc.settings.check_updates;
        let asker = svc.updates.wanted.take().or_else(|| svc.updates.schedule.due(now, auto, busy).then_some(Asker::Timer));
        if let Some(a) = asker {
            svc.updates.schedule.started(now);
            start_job(svc, a);
        }
    }
    // The notice's time runs only while it can be seen (not during a capture).
    if busy {
        if svc.updates.notice.is_some() {
            svc.updates.notice = now.checked_add(NOTICE);
        }
    } else if svc.updates.notice.is_some_and(|until| now >= until) {
        svc.updates.notice = None;
    }
    // Still here when the helper ended: the install didn't happen.
    let mut not_installed = false;
    if let Some((helper, at)) = &mut svc.updates.installing {
        if helper.ended() {
            not_installed = true;
        } else if now.saturating_duration_since(*at) > INSTALL_WAIT {
            svc.updates.installing = None;
        }
    }
    if not_installed {
        svc.updates.installing = None;
        log::warn!("the update installer ended and this program is still running: it wasn't updated");
        svc.notify(
            "ScreenGrabCraft wasn't updated",
            "The install was cancelled or didn't finish. Install Update in the tray tries again.",
            None,
            None,
            true,
        );
    }
    // Wake up for what comes next.
    let u = &svc.updates;
    if u.installing.is_some() || u.job.is_some() || u.wanted.is_some() {
        ctx.request_repaint_after(Duration::from_secs(1));
    } else if let Some(until) = u.notice {
        ctx.request_repaint_after(until.saturating_duration_since(now).max(Duration::from_millis(250)));
    } else if u.automatic && svc.settings.check_updates {
        ctx.request_repaint_after(u.schedule.wait(now).max(Duration::from_secs(1)));
    }
}

// ---- installing ----

/// Update now (the notice, the tray's Install Update, the About tab).
pub fn install_now(svc: &mut Service) {
    let Some(offer) = svc.updates.offer.clone() else { return };
    let Some(installer) = offer.installer.clone() else {
        // Nothing to install here: the release page has it.
        if let Err(e) = launch::open_url(&offer.page) {
            svc.notify("The release page couldn't be opened", e, None, None, true);
        }
        svc.updates.notice = None;
        return;
    };
    let asks = svc.updates.asks_permission;
    if svc.updates.installing.is_some() {
        let detail = if asks { "Windows may be asking for permission." } else { "ScreenGrabCraft starts again when it's done." };
        svc.notify("The update is being installed", detail, None, None, false);
        return;
    }
    svc.updates.notice = None;
    match update::install(&installer, &offer.version) {
        Ok(helper) => {
            svc.updates.installing = Some((helper, Instant::now()));
            svc.notify(format!("Installing ScreenGrabCraft {}", offer.version), install_note(asks), None, None, false);
        }
        Err(e) => {
            log::warn!("installing the update: {e}");
            if !installer.path.is_file() {
                // It was deleted (it no longer matched): downloaded again.
                if let Some(o) = &mut svc.updates.offer {
                    o.installer = None;
                }
                svc.updates.wanted = Some(Asker::Timer);
                sync_tray(svc);
            }
            svc.notify("The update couldn't be installed", e, None, None, true);
        }
    }
}

// ---- what the owner sees ----

/// A desktop point in egui points for a window placed on a monitor of this scale.
fn points(x: f32, y: f32, scale: f32) -> egui::Pos2 {
    if cfg!(target_os = "macos") { pos2(x, y) } else { pos2(x / scale.max(0.5), y / scale.max(0.5)) }
}

enum Act {
    Install,
    Page,
    Later,
}

/// The notice of an update, in the corner where the capture notices are (above one, if one is
/// up). Never while capturing: it would be in the picture.
pub fn show(svc: &mut Service, ctx: &egui::Context) {
    if svc.updates.notice.is_none() || svc.capturing() {
        return;
    }
    let Some(offer) = svc.updates.offer.clone() else { return };
    let size = vec2(380.0, 112.0);
    let above = if svc.notice.is_some() { 96.0 + 12.0 } else { 0.0 };
    let at = screengrabcraft_capture::monitors()
        .ok()
        .and_then(|ms| ms.into_iter().next())
        .map(|m| points(m.rect.right() as f32 - 24.0 * m.scale, m.rect.bottom() as f32 - (72.0 + above) * m.scale, m.scale) - size)
        .unwrap_or(pos2(40.0, 40.0));
    let builder = ViewportBuilder::default()
        .with_title("ScreenGrabCraft update")
        .with_app_id(APP_ID)
        .with_decorations(false)
        .with_resizable(false)
        .with_taskbar(false)
        .with_window_level(egui::WindowLevel::AlwaysOnTop)
        .with_active(false)
        .with_position(at)
        .with_inner_size(size);
    let ready = offer.installer.is_some();
    let title = if ready {
        format!("ScreenGrabCraft {} is ready to install", offer.version)
    } else {
        format!("ScreenGrabCraft {} is available", offer.version)
    };
    let detail = match (&offer.problem, ready) {
        (Some(_), _) => format!("You have {}. The download didn't work, so get it from its release page.", current_text()),
        (None, true) => format!("You have {}. {}", current_text(), install_note(svc.updates.asks_permission)),
        (None, false) => format!("You have {}. Get it from its release page.", current_text()),
    };
    let mut act = None;
    let mut hold = false;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("update-notice"), builder, |ui, _| {
        egui::CentralPanel::default().frame(egui::Frame::NONE.fill(DARK).inner_margin(egui::Margin::same(12)).corner_radius(10)).show(ui, |ui| {
            ui.style_mut().visuals = egui::Visuals::dark();
            hold = ui.ui_contains_pointer();
            ui.label(RichText::new(&title).strong().size(15.0).color(Color32::WHITE));
            ui.add(egui::Label::new(RichText::new(&detail).size(12.0).color(Color32::from_gray(190))).wrap());
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ready {
                    if ui.button(RichText::new("Update now").strong()).clicked() {
                        act = Some(Act::Install);
                    }
                } else if ui.button(RichText::new("Get it").strong()).clicked() {
                    act = Some(Act::Page);
                }
                if ui.button("Later").clicked() {
                    act = Some(Act::Later);
                }
                if ready && ui.link("What's new").clicked() {
                    act = Some(Act::Page);
                }
            });
        });
    });
    match act {
        Some(Act::Install) => install_now(svc),
        Some(Act::Page) => {
            let _ = launch::open_url(&offer.page);
            if !ready {
                svc.updates.notice = None;
            }
        }
        Some(Act::Later) => {
            svc.updates.notice = None;
            svc.updates.dismissed = Some(offer.version.clone());
        }
        None if hold => {
            if let Some(until) = &mut svc.updates.notice {
                *until = (*until).max(Instant::now().checked_add(Duration::from_secs(2)).unwrap_or(*until));
            }
        }
        None => {}
    }
}

/// The About tab's part: the switch, Check now and what it found.
pub fn about(svc: &mut Service, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(14.0);
    ui.label(RichText::new("Updates").strong());
    if svc.updates.flatpak {
        ui.label(
            RichText::new(
                "The Flatpak has no network access, so ScreenGrabCraft can't check for updates here. New versions are on the releases page.",
            )
            .size(12.0)
            .color(t.muted),
        );
        ui.hyperlink_to("github.com/billsecond/screengrabcraft/releases", RELEASES);
        return;
    }
    let mut on = svc.settings.check_updates;
    if ui.checkbox(&mut on, "Check for updates automatically").changed() {
        svc.settings.check_updates = on;
        svc.apply_settings();
    }
    ui.label(
        RichText::new("A minute after it starts and then every six hours, ScreenGrabCraft asks GitHub for its latest release. Nothing about you or your captures is sent.")
            .size(12.0)
            .color(t.muted),
    );
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        let checking = svc.updates.checking();
        if ui.add_enabled(!checking, egui::Button::new("Check now")).clicked() {
            ask(svc, Asker::Settings);
            svc.updates.status = Status::Checking;
        }
        let current = current_text();
        let (text, bad) = match &svc.updates.status {
            Status::Checking => ("Checking…".to_string(), false),
            Status::Downloading(v) => (format!("Downloading {v}…"), false),
            Status::UpToDate(Some(v)) if v.to_string() != current => (format!("Up to date (the latest release is {v})."), false),
            Status::UpToDate(_) => (format!("Up to date: {current} is the latest."), false),
            Status::Failed(e) => (format!("Couldn't check: {e}."), true),
            Status::Idle => (String::new(), false),
        };
        if !text.is_empty() {
            ui.label(RichText::new(text).color(if bad { t.danger } else { t.text }));
        }
    });
    let Some(offer) = svc.updates.offer.clone() else { return };
    ui.add_space(4.0);
    let installing = svc.updates.installing.is_some();
    match &offer.installer {
        Some(_) => {
            ui.label(RichText::new(format!("ScreenGrabCraft {} is ready to install.", offer.version)).strong());
            ui.horizontal(|ui| {
                let label = if installing { "Installing…".to_string() } else { format!("Update now to {}", offer.version) };
                if ui.add_enabled(!installing, egui::Button::new(label)).clicked() {
                    install_now(svc);
                }
                ui.hyperlink_to("What's new", &offer.page);
            });
            ui.label(RichText::new(install_note(svc.updates.asks_permission)).size(12.0).color(t.muted));
        }
        None => {
            ui.label(RichText::new(format!("ScreenGrabCraft {} is available.", offer.version)).strong());
            if let Some(p) = &offer.problem {
                ui.label(RichText::new(format!("The download didn't work: {p}.")).size(12.0).color(t.danger));
            }
            ui.hyperlink_to("Get it from its release page", &offer.page);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_tray_offers_the_waiting_update_first() {
        assert_eq!(tray_items(None), vec![("Check for Updates…".to_string(), Item::CheckUpdates)]);
        let items = tray_items(Some("Install Update 0.1.8"));
        assert_eq!(items.first(), Some(&("Install Update 0.1.8".to_string(), Item::InstallUpdate)));
        assert_eq!(items.len(), 2);
        let v = Version::new(0, 1, 8);
        let ready =
            Offer { version: v.clone(), page: update::page(&v), installer: Some(Installer { path: "x.msi".into(), sha256: [0; 32] }), problem: None };
        assert_eq!(tray_label(Some(&ready)).as_deref(), Some("Install Update 0.1.8"));
        let page_only = Offer { installer: None, ..ready };
        assert_eq!(tray_label(Some(&page_only)).as_deref(), Some("Get Update 0.1.8…"));
        assert_eq!(tray_label(None), None);
    }

    #[test]
    fn only_an_install_for_everyone_mentions_the_permission_prompt() {
        assert!(install_note(Scope::Machine.asks_permission()).contains("permission"));
        assert!(!install_note(Scope::User.asks_permission()).contains("permission"));
    }
}
