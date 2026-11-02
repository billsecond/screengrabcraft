//! The Recorder's window, which Video Production opens (the tray item, the Capture page's button,
//! its shortcut): the owner chooses the area and the inputs, and presses Record. Its contents are
//! drawn by `screengrabcraft_ui::recorder`; every choice goes into the settings as it's made.
//! Record closes it and starts the recording ([`Service::record_production`]).
//!
//! The monitors and microphones are listed when it opens, the cameras once the camera is on (on a
//! Mac, listing them asks the owner). Nothing is turned on, but for the microphone's meter, and
//! that only while the owner has asked for it (turning the microphone on here, or Test).

use std::sync::mpsc::{Receiver, TryRecvError, channel};
use std::time::{Duration, Instant};

use egui::{ViewportBuilder, ViewportId};
use screengrabcraft_capture::audio::{self, AudioCapture, AudioSource};
use screengrabcraft_desktop::store;
use screengrabcraft_model::IRect;
use screengrabcraft_ui::recorder::{self, MicLevel, RecorderChoice, RecorderDevices, RecorderMonitor, RecorderView};

use crate::app::{APP_ID, ICON, Service};

pub struct Recorder {
    pub focus: bool,
    view: RecorderView,
    devices: RecorderDevices,
    meter: Option<Meter>,
}

impl Recorder {
    pub fn open() -> Recorder {
        Recorder { focus: true, view: RecorderView::default(), devices: find_devices(), meter: None }
    }

    /// The meter's reading. The microphone is opened when it's wanted (on another thread: it
    /// takes a moment) and let go as soon as it isn't.
    fn level(&mut self, want: bool, name: &Option<String>, wake: &egui::Context) -> MicLevel {
        if !want {
            self.meter = None;
            return MicLevel::Off;
        }
        if self.meter.as_ref().is_none_or(|m| m.name != *name) {
            self.meter = Some(Meter::open(name.clone(), wake.clone()));
        }
        let Some(m) = self.meter.as_mut() else { return MicLevel::Off };
        if let MeterState::Opening(rx) = &m.state {
            match rx.try_recv() {
                Ok(Ok(cap)) => m.state = MeterState::Open(cap),
                Ok(Err(e)) => m.state = MeterState::Failed(e),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => m.state = MeterState::Failed("the microphone didn't start".to_string()),
            }
        }
        match &m.state {
            MeterState::Opening(_) => MicLevel::Starting,
            MeterState::Open(cap) => {
                // Only how loud it is matters: what it heard is let go.
                drop(cap.take());
                MicLevel::Level(cap.level())
            }
            MeterState::Failed(e) => MicLevel::Failed(e.clone()),
        }
    }
}

/// The microphone heard for the meter.
struct Meter {
    name: Option<String>,
    state: MeterState,
}

enum MeterState {
    Opening(Receiver<Result<AudioCapture, String>>),
    Open(AudioCapture),
    Failed(String),
}

impl Meter {
    fn open(name: Option<String>, wake: egui::Context) -> Meter {
        let (tx, rx) = channel();
        let source = AudioSource::Microphone(name.clone());
        let spawned = std::thread::Builder::new().name("mic-meter".into()).spawn(move || {
            // Let go at once if the window has gone meanwhile (the send fails, the capture is dropped).
            let _ = tx.send(AudioCapture::start(source, Instant::now()).map_err(|e| e.to_string()));
            wake.request_repaint();
        });
        let state = match spawned {
            Ok(_) => MeterState::Opening(rx),
            Err(e) => MeterState::Failed(e.to_string()),
        };
        Meter { name, state }
    }
}

/// What this computer has to record with. Cameras are looked for once the camera is on.
fn find_devices() -> RecorderDevices {
    let monitors = screengrabcraft_capture::monitors().unwrap_or_default();
    let rects: Vec<IRect> = monitors.iter().map(|m| m.rect).collect();
    // Sizes in pixels: a Mac's desktop is in points.
    let px = |v: u32, scale: f32| if cfg!(target_os = "macos") { (v as f32 * scale.max(1.0)).round() as u32 } else { v };
    RecorderDevices {
        monitors: monitors
            .iter()
            .map(|m| RecorderMonitor {
                name: m.name.clone(),
                x: m.rect.x,
                y: m.rect.y,
                width: px(m.rect.w, m.scale),
                height: px(m.rect.h, m.scale),
                primary: m.primary,
            })
            .collect(),
        all_screens: crate::record::all_screens_size(&rects).map(|(_, size)| size),
        microphones: audio::microphones(),
        cameras: None,
        camera_supported: cfg!(any(target_os = "windows", target_os = "macos")),
        system_sound: audio::system_sound_supported(),
        system_sound_platform: cfg!(target_os = "windows"),
    }
}

pub fn show(svc: &mut Service, ctx: &egui::Context) {
    let Some(mut r) = svc.recorder.take() else { return };
    if svc.settings.production.camera && r.devices.camera_supported && r.devices.cameras.is_none() {
        r.devices.cameras = Some(screengrabcraft_capture::camera::cameras());
    }
    let p = &svc.settings.production;
    let level = r.level(p.microphone && r.view.listen, &p.microphone_name, ctx);
    let mut builder = ViewportBuilder::default()
        .with_title("ScreenGrabCraft Recorder")
        .with_app_id(APP_ID)
        .with_inner_size(recorder::SIZE)
        .with_min_inner_size(recorder::SIZE)
        .with_resizable(false);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        builder = builder.with_icon(icon);
    }
    let mut choice = None;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("recorder"), builder, |ui, _| {
        if r.focus {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Focus);
            r.focus = false;
        }
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            choice = Some(RecorderChoice::Cancel);
        }
        let mut rec = svc.settings.recorder.clone();
        let mut sources = svc.settings.production.clone();
        if let Some(c) = recorder::recorder_ui(ui, &mut r.view, &mut rec, &mut sources, &r.devices, &level) {
            choice = Some(c);
        }
        if rec != svc.settings.recorder || sources != svc.settings.production {
            svc.settings.recorder = rec;
            svc.settings.production = sources;
            svc.settings = svc.settings.clone().clamped();
            store::save_settings(&svc.settings);
        }
    });
    if r.meter.is_some() {
        ctx.request_repaint_after(Duration::from_millis(60));
    }
    match choice {
        // The window and the meter go (the microphone is let go) before anything is recorded.
        Some(RecorderChoice::Record) => {
            drop(r);
            svc.record_production();
        }
        Some(RecorderChoice::Cancel) => {}
        None => svc.recorder = Some(r),
    }
}
