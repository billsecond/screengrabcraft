//! Captures on a timer (one every so many seconds, a time-lapse) and one at a set time of day,
//! started and stopped on the Capture page or from the tray. Each is taken on a worker thread and
//! saved straight to the captures folder, nothing coming up over the screen; one that looks as the
//! last did is skipped when the settings say so. None is taken while another capture or a
//! recording is going on.

use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::{Duration, Instant};

use screengrabcraft_model::timed::{self, Fingerprint, TimedArea};
use screengrabcraft_model::{IRect, Raster};

use crate::app::Service;

/// What a worker came back with.
enum Taken {
    /// Kept: its fingerprint.
    Kept(Fingerprint),
    /// Looked as the last one did.
    Same,
    Failed(String),
}

/// Captures on a timer and at a set time, as they're going.
pub struct Timed {
    /// The timer runs: the next capture is due then.
    pub next: Option<Instant>,
    /// A capture at a set time is waiting for then.
    pub at: Option<Instant>,
    /// The last capture kept, to tell an unchanged screen.
    last: Option<Fingerprint>,
    /// A worker is on a capture.
    busy: bool,
    pub taken: u32,
    pub skipped: u32,
    /// Why the last one failed, if it did.
    pub problem: Option<String>,
    tx: Sender<Taken>,
    rx: Receiver<Taken>,
}

impl Default for Timed {
    fn default() -> Timed {
        let (tx, rx) = channel();
        Timed { next: None, at: None, last: None, busy: false, taken: 0, skipped: 0, problem: None, tx, rx }
    }
}

impl Timed {
    pub fn running(&self) -> bool {
        self.next.is_some()
    }

    /// What the Capture page and the tray say of it.
    pub fn status(&self) -> String {
        let s = |n: u32| if n == 1 { "" } else { "s" };
        match (self.taken, self.skipped) {
            (t, 0) => format!("{t} capture{} kept", s(t)),
            (t, k) => format!("{t} capture{} kept, {k} skipped (nothing had changed)", s(t)),
        }
    }
}

/// Starts the timer: the first capture at once.
pub fn start(svc: &mut Service) {
    let t = &mut svc.timed;
    t.next = Some(Instant::now());
    t.last = None;
    t.taken = 0;
    t.skipped = 0;
    t.problem = None;
    log::info!("captures on a timer: every {} s", svc.settings.timed.every_secs);
    svc.sync_timed();
}

pub fn stop(svc: &mut Service) {
    svc.timed.next = None;
    log::info!("captures on a timer stopped: {}", svc.timed.status());
    svc.sync_timed();
}

/// Sets a capture at the time of day the settings say; why not, if it can't be.
pub fn schedule(svc: &mut Service) -> Result<(), String> {
    let (h, m) = timed::parse_clock(&svc.settings.timed.at).ok_or("Write the time as hours and minutes, as 14:30")?;
    let wait = timed::secs_until(screengrabcraft_desktop::time::now(), (h, m));
    svc.timed.at = Some(Instant::now() + Duration::from_secs(u64::from(wait)));
    log::info!("a capture at a set time, in {wait} s");
    Ok(())
}

/// Every frame: takes what's due (when nothing else is going on), and keeps what came back.
pub fn tick(svc: &mut Service, ctx: &egui::Context) {
    while let Ok(answer) = svc.timed.rx.try_recv() {
        let t = &mut svc.timed;
        t.busy = false;
        match answer {
            Taken::Kept(f) => {
                t.taken = t.taken.saturating_add(1);
                t.last = Some(f);
                t.problem = None;
            }
            Taken::Same => t.skipped = t.skipped.saturating_add(1),
            Taken::Failed(e) => {
                log::warn!("a capture on a timer failed: {e}");
                t.problem = Some(e);
            }
        }
        svc.sync_timed();
    }
    let now = Instant::now();
    let due_timer = svc.timed.next.is_some_and(|n| now >= n);
    let due_at = svc.timed.at.is_some_and(|a| now >= a);
    let quiet = svc.overlay.is_none() && svc.recording.is_none() && svc.freezing.is_none() && svc.countdown.is_none();
    if (due_timer || due_at) && quiet && !svc.timed.busy {
        let every = Duration::from_secs(u64::from(svc.settings.timed.every_secs.clamp(timed::MIN_EVERY, timed::MAX_EVERY)));
        if due_timer {
            svc.timed.next = Some(now + every);
        }
        // The set time's capture is kept whatever it looks like.
        let last = if due_at || !svc.settings.timed.skip_unchanged { None } else { svc.timed.last.clone() };
        if due_at {
            svc.timed.at = None;
        }
        take(svc, last, ctx);
    }
    // Woken for the next one.
    let soonest = [svc.timed.next, svc.timed.at].into_iter().flatten().min();
    if let Some(s) = soonest {
        ctx.request_repaint_after(s.saturating_duration_since(now).max(Duration::from_millis(50)));
    }
}

/// Takes a capture on a worker thread, and saves it unless it looks as `last` did.
fn take(svc: &mut Service, last: Option<Fingerprint>, ctx: &egui::Context) {
    let area = svc.settings.timed.area;
    let region = svc.settings.last_region;
    let settings = svc.settings.clone();
    let (tx, ctx) = (svc.timed.tx.clone(), ctx.clone());
    svc.timed.busy = true;
    let spawned = std::thread::Builder::new().name("timed-capture".into()).spawn(move || {
        let answer = std::panic::catch_unwind(move || {
            let pic = match grab(area, region) {
                Ok(p) => p,
                Err(e) => return Taken::Failed(e),
            };
            let f = timed::fingerprint(&pic);
            if last.as_ref().is_some_and(|l| timed::same_scene(l, &f)) {
                return Taken::Same;
            }
            match crate::app::write_capture(&pic, "Timed", &settings, true, false) {
                Ok(_) => Taken::Kept(f),
                Err(e) => Taken::Failed(e),
            }
        })
        .unwrap_or_else(|_| Taken::Failed("the capture failed unexpectedly".to_string()));
        let _ = tx.send(answer);
        ctx.request_repaint();
    });
    if let Err(e) = spawned {
        svc.timed.busy = false;
        svc.timed.problem = Some(e.to_string());
    }
}

/// A picture of what a capture on a timer is of.
fn grab(area: TimedArea, last: Option<IRect>) -> Result<Raster, String> {
    match area {
        TimedArea::Screen => {
            let monitors = screengrabcraft_capture::monitors().map_err(|e| e.to_string())?;
            let (x, y) = screengrabcraft_capture::pointer().unwrap_or((0, 0));
            let m = monitors
                .iter()
                .find(|m| m.rect.contains(i64::from(x), i64::from(y)))
                .or_else(|| monitors.first())
                .ok_or("there is no screen to capture")?;
            screengrabcraft_capture::grab_monitor(m.id).map(|(_, p)| p).map_err(|e| e.to_string())
        }
        TimedArea::AllScreens => {
            let monitors = screengrabcraft_capture::monitors().map_err(|e| e.to_string())?;
            let desktop = monitors.iter().map(|m| m.rect).reduce(|a, b| a.union(b)).ok_or("there is no screen to capture")?;
            screengrabcraft_capture::grab_area(desktop).map_err(|e| e.to_string())
        }
        TimedArea::LastArea => {
            let area = last.ok_or("No area has been captured yet: capture one first, or choose a screen")?;
            screengrabcraft_capture::grab_area(area).map_err(|e| e.to_string())
        }
    }
}
