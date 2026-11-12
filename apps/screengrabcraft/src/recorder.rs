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
use screengrabcraft_capture::camera::{self, CameraCapture};
use screengrabcraft_desktop::store;
use screengrabcraft_model::IRect;
use screengrabcraft_ui::recorder::{self, CameraPreview, MicLevel, RecorderChoice, RecorderDevices, RecorderMonitor, RecorderView};

use crate::app::{APP_ID, ICON, Service};

pub struct Recorder {
    pub focus: bool,
    view: RecorderView,
    devices: RecorderDevices,
    meter: Option<Meter>,
    /// The computer's sound heard for its meter.
    sound_meter: Option<Meter>,
    /// The camera, while it's tested.
    camera: Option<CameraTest>,
}

impl Recorder {
    pub fn open() -> Recorder {
        Recorder { focus: true, view: RecorderView::default(), devices: find_devices(), meter: None, sound_meter: None, camera: None }
    }

    /// The microphone meter's reading.
    fn level(&mut self, want: bool, name: &Option<String>, wake: &egui::Context) -> MicLevel {
        read(&mut self.meter, want, AudioSource::Microphone(name.clone()), wake, true)
    }

    /// The computer's sound meter's reading.
    fn sound_level(&mut self, want: bool, name: &Option<String>, wake: &egui::Context) -> MicLevel {
        read(&mut self.sound_meter, want, AudioSource::SystemSound(name.clone()), wake, false)
    }
}

/// How long a microphone is heard before one that has heard nothing at all is said to hear only
/// silence.
const SILENCE_AFTER: Duration = Duration::from_secs(3);

/// A meter's reading. The device is opened when it's wanted (on another thread: it takes a
/// moment) and let go as soon as it isn't. A microphone (`hiss`: it always hears something) that
/// has heard nothing at all for a while hears only silence.
fn read(meter: &mut Option<Meter>, want: bool, source: AudioSource, wake: &egui::Context, hiss: bool) -> MicLevel {
    if !want {
        *meter = None;
        return MicLevel::Off;
    }
    if meter.as_ref().is_none_or(|m| m.source != source) {
        *meter = Some(Meter::open(source, wake.clone()));
    }
    let Some(m) = meter.as_mut() else { return MicLevel::Off };
    if let MeterState::Opening(rx) = &m.state {
        match rx.try_recv() {
            Ok(Ok(cap)) => {
                m.state = MeterState::Open(cap);
                m.opened = Some(Instant::now());
            }
            Ok(Err(e)) => m.state = MeterState::Failed(e),
            Err(TryRecvError::Empty) => {}
            Err(TryRecvError::Disconnected) => m.state = MeterState::Failed("the sound device didn't start".to_string()),
        }
    }
    match &m.state {
        MeterState::Opening(_) => MicLevel::Starting,
        MeterState::Open(cap) => {
            // Only how loud it is matters: what it heard is let go.
            let heard = cap.take();
            m.loudest = heard.iter().flat_map(|b| b.samples.iter()).map(|s| s.unsigned_abs()).fold(m.loudest, u16::max);
            if hiss && m.loudest <= 1 && m.opened.is_some_and(|at| at.elapsed() >= SILENCE_AFTER) {
                return MicLevel::Silent;
            }
            MicLevel::Level(cap.level())
        }
        MeterState::Failed(e) => MicLevel::Failed(e.clone()),
    }
}

/// A sound device heard for a meter.
struct Meter {
    source: AudioSource,
    state: MeterState,
    /// When it opened.
    opened: Option<Instant>,
    /// The loudest it has heard, 0 to 32767.
    loudest: u16,
}

enum MeterState {
    Opening(Receiver<Result<AudioCapture, String>>),
    Open(AudioCapture),
    Failed(String),
}

impl Meter {
    fn open(source: AudioSource, wake: egui::Context) -> Meter {
        let (tx, rx) = channel();
        let opened = source.clone();
        let spawned = std::thread::Builder::new().name("sound-meter".into()).spawn(move || {
            // Let go at once if the window has gone meanwhile (the send fails, the capture is dropped).
            let _ = tx.send(AudioCapture::start(opened, Instant::now()).map_err(|e| e.to_string()));
            wake.request_repaint();
        });
        let state = match spawned {
            Ok(_) => MeterState::Opening(rx),
            Err(e) => MeterState::Failed(e.to_string()),
        };
        Meter { source, state, opened: None, loudest: 0 }
    }
}

/// How long the camera may send nothing, while it's tested, before the owner is told why.
const CAMERA_QUIET: Duration = Duration::from_secs(4);

/// The camera turned on for its Test, as a recording turns it on (in its own program).
struct CameraTest {
    index: u32,
    /// Its name, once it has opened.
    name: String,
    state: CameraState,
    /// Its latest picture.
    picture: Option<egui::TextureHandle>,
    /// When it opened or its last picture came.
    last: Instant,
    /// Whether it has sent any picture.
    sent: bool,
    /// Whether it went quiet while its own microphone was heard (it's tried again once that
    /// microphone isn't).
    blocked: bool,
}

enum CameraState {
    Opening(Receiver<Result<CameraCapture, String>>),
    Open(CameraCapture),
    Failed(String),
}

impl CameraTest {
    /// Whether `mic` is this camera's own microphone.
    fn name_is_own(&self, mic: &str) -> bool {
        camera::is_own_microphone(mic, &self.name)
    }

    fn open(index: u32, wake: egui::Context) -> CameraTest {
        let (tx, rx) = channel();
        let spawned = std::thread::Builder::new().name("camera-test".into()).spawn(move || {
            // Let go at once if the test was stopped meanwhile (the send fails, the camera is dropped).
            let _ = tx.send(CameraCapture::start(index, Instant::now()).map_err(|e| e.to_string()));
            wake.request_repaint();
        });
        let state = match spawned {
            Ok(_) => CameraState::Opening(rx),
            Err(e) => CameraState::Failed(e.to_string()),
        };
        CameraTest { index, name: String::new(), state, picture: None, last: Instant::now(), sent: false, blocked: false }
    }

    /// What it shows now. `mic_heard`: the name of the microphone being heard for its meter.
    fn preview(&mut self, ctx: &egui::Context, mic_heard: Option<&str>) -> CameraPreview {
        if let CameraState::Opening(rx) = &self.state {
            match rx.try_recv() {
                Ok(Ok(cap)) => {
                    self.name = cap.name.clone();
                    self.state = CameraState::Open(cap);
                    self.last = Instant::now();
                }
                Ok(Err(e)) => self.state = CameraState::Failed(format!("The camera didn't start: {e}")),
                Err(TryRecvError::Empty) => {}
                Err(TryRecvError::Disconnected) => self.state = CameraState::Failed("The camera didn't start.".to_string()),
            }
        }
        let mut failed = None;
        if let CameraState::Open(cap) = &self.state {
            // Only the latest picture is shown; the rest are let go.
            if let Some(f) = cap.take().pop() {
                self.last = Instant::now();
                self.sent = true;
                if let Ok(pic) = screengrabcraft_render::decode(&f.jpeg) {
                    let image = egui::ColorImage::from_rgba_unmultiplied([pic.width() as usize, pic.height() as usize], pic.pixels());
                    match &mut self.picture {
                        Some(t) => t.set(image, egui::TextureOptions::LINEAR),
                        None => self.picture = Some(ctx.load_texture("recorder-camera", image, egui::TextureOptions::LINEAR)),
                    }
                }
            } else if self.last.elapsed() >= CAMERA_QUIET {
                // Quiet: why, as plainly as can be said.
                let own_mic = mic_heard.is_some_and(|m| camera::is_own_microphone(m, &cap.name));
                self.blocked = own_mic;
                failed = Some(if own_mic {
                    format!(
                        "{} sends no video while its own microphone is on (the microphone's Test is hearing it). Choose another microphone to record both.",
                        cap.name
                    )
                } else if let Some(why) = cap.trouble() {
                    format!("{} sends no pictures: {why}", cap.name)
                } else if self.sent {
                    format!("{} stopped sending pictures.", cap.name)
                } else {
                    format!("{} sends no pictures. Is another program using it?", cap.name)
                });
            }
        }
        if let Some(why) = failed {
            // Let go of it (its own program is ended), so it isn't held.
            if let CameraState::Open(cap) = std::mem::replace(&mut self.state, CameraState::Failed(why)) {
                cap.abandon();
            }
        }
        match &self.state {
            CameraState::Opening(_) => CameraPreview::Starting,
            CameraState::Open(_) => self.picture.clone().map_or(CameraPreview::Starting, CameraPreview::Live),
            CameraState::Failed(why) => CameraPreview::Failed(why.clone()),
        }
    }
}

/// The camera's Test: turned on while it's wanted, let go as soon as it isn't, and tried again
/// once its own microphone that stopped it is no longer heard.
fn watch(test: &mut Option<CameraTest>, want: Option<u32>, mic_heard: Option<&str>, ctx: &egui::Context) -> CameraPreview {
    let Some(index) = want else {
        *test = None;
        return CameraPreview::Off;
    };
    let again = test.as_ref().is_some_and(|t| t.blocked && matches!(t.state, CameraState::Failed(_)) && !mic_heard.is_some_and(|m| t.name_is_own(m)));
    if test.as_ref().is_none_or(|t| t.index != index) || again {
        *test = Some(CameraTest::open(index, ctx.clone()));
    }
    let Some(t) = test.as_mut() else { return CameraPreview::Off };
    t.preview(ctx, mic_heard)
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
        default_microphone: audio::default_microphone(),
        cameras: None,
        camera_supported: cfg!(any(target_os = "windows", target_os = "macos")),
        system_sound: audio::system_sound_supported(),
        system_sound_platform: cfg!(target_os = "windows"),
        speakers: audio::speakers(),
        default_speaker: audio::default_speaker(),
    }
}

pub fn show(svc: &mut Service, ctx: &egui::Context) {
    let Some(mut r) = svc.recorder.take() else { return };
    if svc.settings.production.camera && r.devices.camera_supported && r.devices.cameras.is_none() {
        r.devices.cameras = Some(screengrabcraft_capture::camera::cameras());
    }
    let p = &svc.settings.production;
    let level = r.level(p.microphone && r.view.listen, &p.microphone_name, ctx);
    let sound = r.sound_level(p.system_sound && r.devices.system_sound && r.view.listen, &p.system_sound_name, ctx);
    // The camera's Test: which camera, and which microphone is being heard meanwhile (to say so
    // when the camera's own one stops it).
    let want = (p.camera && r.devices.camera_supported && r.view.watch)
        .then(|| p.camera_index.or_else(|| r.devices.cameras.as_ref().and_then(|l| l.first()).map(|c| c.0)))
        .flatten();
    let mic_heard = (p.microphone && r.view.listen).then(|| p.microphone_name.clone().or_else(|| r.devices.default_microphone.clone())).flatten();
    let camera_view = watch(&mut r.camera, want, mic_heard.as_deref(), ctx);
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
        if let Some(c) = recorder::recorder_ui(ui, &mut r.view, &mut rec, &mut sources, &r.devices, &level, &sound, &camera_view) {
            choice = Some(c);
        }
        if rec != svc.settings.recorder || sources != svc.settings.production {
            svc.settings.recorder = rec;
            svc.settings.production = sources;
            svc.settings = svc.settings.clone().clamped();
            store::save_settings(&svc.settings);
        }
    });
    if r.camera.is_some() {
        ctx.request_repaint_after(Duration::from_millis(33));
    } else if r.meter.is_some() {
        ctx.request_repaint_after(Duration::from_millis(60));
    }
    match choice {
        // The window, the meter and the camera's Test go (the microphone and the camera are let
        // go) before anything is recorded.
        Some(RecorderChoice::Record) => {
            drop(r);
            svc.record_production();
        }
        Some(RecorderChoice::Cancel) => {}
        None => svc.recorder = Some(r),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Recorder's camera Test with the real camera, as the window drives it: its picture comes;
    /// with its own microphone heard (when it has one), a camera that goes quiet is said to be
    /// stopped by it; with that microphone let go, the camera is tried again and comes back. The
    /// camera runs in its own program as it does in the service (this workspace's release
    /// `screengrabcraft.exe`). Nothing is kept. Run by hand only:
    /// `cargo test --release -p screengrabcraft live_camera_test -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn live_camera_test() {
        let helper = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/screengrabcraft.exe");
        if helper.exists() {
            camera::use_helper(helper);
        }
        let ctx = egui::Context::default();
        let mut test = None;
        let until = |test: &mut Option<CameraTest>, mic: Option<&str>, secs: u64, done: &dyn Fn(&CameraPreview) -> bool| {
            let t = Instant::now();
            loop {
                let p = watch(test, Some(0), mic, &ctx);
                if done(&p) || t.elapsed() > Duration::from_secs(secs) {
                    return (p, t.elapsed());
                }
                std::thread::sleep(Duration::from_millis(33));
            }
        };
        let live = |p: &CameraPreview| matches!(p, CameraPreview::Live(_));
        let (p, took) = until(&mut test, None, 8, &live);
        println!("the camera's picture after {} ms", took.as_millis());
        assert!(live(&p), "the camera's picture comes");
        let name = test.as_ref().map(|t| t.name.clone()).unwrap_or_default();
        // Its own microphone, if it has one, heard as the microphone's Test hears it.
        let Some(own) = audio::microphones().into_iter().find(|m| camera::is_own_microphone(m, &name)) else {
            println!("{name} has no microphone of its own: done");
            return;
        };
        let mic = AudioCapture::start(AudioSource::Microphone(Some(own.clone())), Instant::now()).unwrap();
        let (p, took) = until(&mut test, Some(&own), 10, &|p| matches!(p, CameraPreview::Failed(_)));
        drop(mic.stop());
        match &p {
            CameraPreview::Failed(why) => println!("with {own} heard, after {} ms: {why}", took.as_millis()),
            _ => println!("with {own} heard, the camera kept sending pictures for {} s", took.as_secs()),
        }
        if let CameraPreview::Failed(why) = &p {
            assert!(why.contains("its own microphone"), "{why}");
            // The microphone let go: tried again, and back.
            let (p, took) = until(&mut test, None, 8, &live);
            println!("with the microphone let go, the picture back after {} ms", took.as_millis());
            assert!(live(&p), "the camera comes back");
        }
        drop(test.take());
    }
}
