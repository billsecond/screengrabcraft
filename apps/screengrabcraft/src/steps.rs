//! Step capture: while it runs, each left click takes a picture of the window clicked (the whole
//! screen when there's no window), with a red ring where the click was, as a numbered step of a
//! how-to guide. A small bar at the top of the screen counts the steps and stops it (a click on
//! the bar is no step). Then the guide window opens: the steps put in order, taken out, reworded,
//! and saved as a PDF, a web page or Markdown.
//!
//! The guide is `<captures>/Guide <date> <time>.sgguide`, its pictures in the folder beside it
//! (`… files`). Nothing leaves this computer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{Receiver, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use egui::{RichText, ViewportBuilder, ViewportId, pos2, vec2};
use screengrabcraft_desktop::{launch, paths, store};
use screengrabcraft_model::guide::{GUIDE_EXT, Guide, GuideStep, default_text};
use screengrabcraft_model::{IRect, ImageFormat, Raster};
use screengrabcraft_ui::fit::{self, Fit};
use screengrabcraft_ui::guide::{GuideAction, GuideFormat, GuideView};
use screengrabcraft_ui::theme::Tokens;
use screengrabcraft_ui::widgets;

use crate::app::{APP_ID, ICON, Service};

/// The bar's title, and the guide window's.
pub const BAR_TITLE: &str = "ScreenGrabCraft steps";
const WINDOW_TITLE: &str = "How-to Guide";
/// The bar's size, in points.
const BAR: egui::Vec2 = egui::Vec2 { x: 330.0, y: 56.0 };
/// How often the mouse is looked at.
const POLL: Duration = Duration::from_millis(12);
/// A click on the tray's menu to stop is no step: those this soon before it go.
const TRAY_CLICK: Duration = Duration::from_millis(1500);

/// What the worker sends back.
enum Taken {
    Step(GuideStep),
    Failed(String),
}

/// Steps being taken.
struct Run {
    stop: Arc<AtomicBool>,
    rx: Receiver<Taken>,
    guide: Guide,
    /// When each step was taken.
    times: Vec<Instant>,
    file: PathBuf,
    folder: PathBuf,
    /// The bar's place in points, and its monitor's scale.
    at: (egui::Pos2, f32),
    /// Where the bar is on the desktop as made ([`BAR`] big), and where it is now (wider for
    /// longer words): a click there is no step.
    bar_desk: Option<IRect>,
    bar: Arc<Mutex<Option<IRect>>>,
    failed: u32,
}

/// The guide window.
struct Window {
    view: GuideView,
    file: PathBuf,
    folder: PathBuf,
}

/// Step capture: running, and the guide window.
#[derive(Default)]
pub struct Steps {
    run: Option<Run>,
    window: Option<Window>,
}

impl Steps {
    pub fn running(&self) -> bool {
        self.run.is_some()
    }

    /// Whether one of its windows is up (the service's anchor window stays shown meanwhile).
    pub fn busy(&self) -> bool {
        self.run.is_some() || self.window.is_some()
    }
}

/// The folder a guide's pictures are in: beside it, `<its name> files`.
pub fn pictures_folder(file: &Path) -> PathBuf {
    let stem = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "Guide".to_string());
    file.with_file_name(format!("{stem} files"))
}

/// Starts taking steps.
pub fn start(svc: &mut Service) {
    if svc.steps.busy() {
        return;
    }
    let dir = paths::captures_dir(&svc.settings);
    let name = screengrabcraft_model::settings::expand_name("Guide {date} {time}", screengrabcraft_desktop::time::now(), "Guide", 1);
    let file = crate::app::free_path(&dir, &name, GUIDE_EXT);
    let folder = pictures_folder(&file);
    if let Err(e) = std::fs::create_dir_all(&folder) {
        svc.notify("Steps could not be recorded", format!("{}: {e}", folder.display()), None, None, true);
        return;
    }
    // The bar at the top of the primary screen, in the middle.
    let monitors = screengrabcraft_capture::monitors().unwrap_or_default();
    let primary = monitors.iter().find(|m| m.primary).or_else(|| monitors.first());
    let (at, bar_desk) = match primary {
        Some(m) => {
            let s = m.scale.max(0.5);
            let (w, h) = (BAR.x * s, BAR.y * s);
            let x = m.rect.x as f32 + (m.rect.w as f32 - w) / 2.0;
            let y = m.rect.y as f32 + 10.0 * s;
            let points = if cfg!(target_os = "macos") { pos2(x, y) } else { pos2(x / s, y / s) };
            ((points, s), Some(IRect::new(x as i32, y as i32, w.ceil() as u32 + 1, h.ceil() as u32 + 1)))
        }
        None => ((pos2(200.0, 10.0), 1.0), None),
    };
    // Where a click is no step (the bar).
    let bar = Arc::new(Mutex::new(bar_desk));
    let stop = Arc::new(AtomicBool::new(false));
    let (tx, rx) = channel();
    let (worker_stop, worker_bar, worker_folder, ctx) = (stop.clone(), bar.clone(), folder.clone(), svc.ctx.clone());
    let spawned = std::thread::Builder::new().name("steps".into()).spawn(move || {
        // A press under way as it starts (the button that started it) is no step.
        let mut was_down = true;
        let mut n = 0u32;
        while !worker_stop.load(Ordering::SeqCst) {
            if let Some(s) = screengrabcraft_capture::mouse::state() {
                let on_bar = worker_bar.lock().ok().and_then(|b| *b).is_some_and(|b| b.contains(i64::from(s.x), i64::from(s.y)));
                if s.left && !was_down && !on_bar {
                    n = n.saturating_add(1);
                    let taken = std::panic::catch_unwind(|| take_step(n, s.x, s.y, &worker_folder))
                        .unwrap_or_else(|_| Err("the step could not be taken".to_string()));
                    let _ = tx.send(match taken {
                        Ok(step) => Taken::Step(step),
                        Err(e) => Taken::Failed(e),
                    });
                    ctx.request_repaint();
                }
                was_down = s.left;
            }
            std::thread::sleep(POLL);
        }
    });
    if let Err(e) = spawned {
        svc.notify("Steps could not be recorded", e.to_string(), None, None, true);
        return;
    }
    log::info!("step capture started: {}", file.display());
    let title = file.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
    svc.steps.run = Some(Run { stop, rx, guide: Guide { title, steps: Vec::new() }, times: Vec::new(), file, folder, at, bar_desk, bar, failed: 0 });
    svc.sync_steps();
}

/// One step: the window under the click (or its screen), the click marked, written as
/// `step-NNN.png` in the guide's folder.
fn take_step(n: u32, x: i32, y: i32, folder: &Path) -> Result<GuideStep, String> {
    let windows = screengrabcraft_capture::windows();
    let window = windows.iter().find(|w| w.rect.contains(i64::from(x), i64::from(y)));
    let monitors = screengrabcraft_capture::monitors().map_err(|e| e.to_string())?;
    let m = monitors.iter().find(|m| m.rect.contains(i64::from(x), i64::from(y))).or_else(|| monitors.first()).ok_or("there is no screen")?;
    let (info, pic) = screengrabcraft_capture::grab_monitor(m.id).map_err(|e| e.to_string())?;
    let area = window.map_or(info.rect, |w| w.rect);
    let px = info.to_pixels(area, pic.width(), pic.height()).ok_or("the window is off the screen")?;
    let crop = pic.crop(px.x.max(0) as u32, px.y.max(0) as u32, px.w, px.h).ok_or("the window is off the screen")?;
    let click = info.to_pixels(IRect::new(x, y, 1, 1), pic.width(), pic.height());
    let (cx, cy) = click.map_or((0, 0), |c| ((c.x - px.x).max(0) as u32, (c.y - px.y).max(0) as u32));
    let marked = screengrabcraft_render::guide::mark_click(&crop, cx, cy);
    let name = format!("step-{n:03}.png");
    let bytes = screengrabcraft_render::encode(&marked, ImageFormat::Png, 100).map_err(|e| e.to_string())?;
    store::write_atomic(&folder.join(&name), &bytes).map_err(|e| e.to_string())?;
    log::info!("step {n}: {}x{}", marked.width(), marked.height());
    let text = window.map_or_else(|| "Click here".to_string(), |w| default_text(&w.title, &w.app));
    Ok(GuideStep { picture: name, text, click: Some((cx, cy)) })
}

/// Stops taking steps (`from_tray`: the click on the tray's menu that stopped it is no step), and
/// opens the guide window.
pub fn stop(svc: &mut Service, from_tray: bool) {
    let Some(mut run) = svc.steps.run.take() else { return };
    run.stop.store(true, Ordering::SeqCst);
    collect(&mut run);
    if from_tray {
        let now = Instant::now();
        while run.times.last().is_some_and(|t| now.saturating_duration_since(*t) < TRAY_CLICK) {
            run.times.pop();
            run.guide.steps.pop();
        }
    }
    svc.sync_steps();
    if run.guide.steps.is_empty() {
        // Nothing was taken: nothing is left behind (the folder is empty unless a tray click's
        // step was let go).
        let _ = std::fs::remove_dir(&run.folder);
        svc.notify("No steps were recorded", "Each click while it runs is a step.", None, None, false);
        return;
    }
    write_guide(&run.guide, &run.file);
    log::info!("step capture stopped: {} steps", run.guide.steps.len());
    open_window(svc, run.guide, run.file, run.folder);
}

/// Opens a guide file in the guide window.
pub fn open(svc: &mut Service, file: &Path) {
    let text = std::fs::metadata(file).ok().filter(|m| m.len() < 16 * 1024 * 1024).and_then(|_| std::fs::read_to_string(file).ok());
    match text.and_then(|t| Guide::from_json(&t)) {
        Some(guide) => {
            let folder = pictures_folder(file);
            open_window(svc, guide, file.to_path_buf(), folder);
        }
        None => svc.notify("The guide could not be opened", file.display().to_string(), None, None, true),
    }
}

fn open_window(svc: &mut Service, guide: Guide, file: PathBuf, folder: PathBuf) {
    let mut pictures: HashMap<String, Option<Raster>> = HashMap::new();
    for s in &guide.steps {
        let pic = std::fs::read(folder.join(&s.picture)).ok().and_then(|b| screengrabcraft_render::decode(&b).ok());
        pictures.insert(s.picture.clone(), pic);
    }
    svc.steps.window = Some(Window { view: GuideView::new(guide, pictures), file, folder });
}

fn write_guide(guide: &Guide, file: &Path) {
    if let Err(e) = store::write_atomic(file, guide.to_json().as_bytes()) {
        log::warn!("writing the guide: {e}");
    }
}

/// The steps taken since last looked.
fn collect(run: &mut Run) {
    while let Ok(t) = run.rx.try_recv() {
        match t {
            Taken::Step(s) => {
                run.guide.steps.push(s);
                run.times.push(Instant::now());
            }
            Taken::Failed(e) => {
                log::warn!("a step was not taken: {e}");
                run.failed = run.failed.saturating_add(1);
            }
        }
    }
}

/// The guide saved as `format`, beside its file: written, and shown in its folder.
fn export(w: &mut Window, format: GuideFormat) -> Result<PathBuf, String> {
    let out = w.file.with_extension(format.extension());
    let folder_name = w.folder.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default();
    let pictures = || -> Vec<Option<Raster>> { w.view.guide.steps.iter().map(|s| w.view.picture(&s.picture).cloned()).collect() };
    let bytes = match format {
        GuideFormat::Pdf => screengrabcraft_render::guide::pdf(&w.view.guide, &pictures()).map_err(|e| e.to_string())?,
        GuideFormat::Document => screengrabcraft_render::guide::docx(&w.view.guide, &pictures()).map_err(|e| e.to_string())?,
        GuideFormat::Slides => screengrabcraft_render::guide::pptx(&w.view.guide, &pictures()).map_err(|e| e.to_string())?,
        GuideFormat::Html => screengrabcraft_render::guide::html(&w.view.guide, &folder_name).into_bytes(),
        GuideFormat::Markdown => screengrabcraft_render::guide::markdown(&w.view.guide, &folder_name).into_bytes(),
    };
    store::write_atomic(&out, &bytes).map_err(|e| format!("{}: {e}", out.display()))?;
    Ok(out)
}

/// Every frame: the steps taken, and the bar and the guide window shown.
pub fn show(svc: &mut Service, ctx: &egui::Context) {
    let mut stop_now = false;
    if let Some(run) = svc.steps.run.as_mut() {
        collect(run);
        let (count, failed, (at, scale)) = (run.guide.steps.len(), run.failed, run.at);
        let fit = bar_fit();
        let (shown, size) = (fit.shown(ctx), fit.size(ctx));
        // Grown for its words: still in the middle, and still no step where it is.
        let grown = size - BAR;
        if let (Some(d), Ok(mut bar)) = (run.bar_desk, run.bar.lock()) {
            let (dw, dh) = ((grown.x * scale).ceil() as i32, (grown.y * scale).ceil() as i32);
            *bar = Some(IRect::new(d.x.saturating_sub(dw / 2), d.y, d.w.saturating_add(dw.max(0) as u32), d.h.saturating_add(dh.max(0) as u32)));
        }
        let builder = ViewportBuilder::default()
            .with_title(BAR_TITLE)
            .with_app_id(APP_ID)
            .with_decorations(false)
            .with_resizable(false)
            .with_taskbar(false)
            .with_window_level(egui::WindowLevel::AlwaysOnTop)
            .with_active(false)
            .with_position(pos2(at.x - grown.x / 2.0, at.y))
            .with_inner_size(size)
            .with_visible(shown);
        ctx.show_viewport_immediate(ViewportId::from_hash_of("steps-bar"), builder, |ui, _| {
            stop_now = fit.face(ui, widgets::popup_frame(ui.ctx()), |ui| bar_ui(ui, count, failed));
        });
        ctx.request_repaint_after(Duration::from_millis(250));
    }
    if stop_now {
        stop(svc, false);
    }
    let Some(mut w) = svc.steps.window.take() else { return };
    let mut builder =
        ViewportBuilder::default().with_title(WINDOW_TITLE).with_app_id(APP_ID).with_inner_size([980.0, 760.0]).with_min_inner_size([640.0, 420.0]);
    if let Ok(icon) = eframe::icon_data::from_png_bytes(ICON) {
        builder = builder.with_icon(icon);
    }
    let mut action = None;
    ctx.show_viewport_immediate(ViewportId::from_hash_of("guide"), builder, |ui, _| {
        action = screengrabcraft_ui::guide::show(&mut w.view, ui);
        if ui.ctx().input(|i| i.viewport().close_requested()) {
            action = Some(GuideAction::Close);
        }
    });
    match action {
        Some(GuideAction::Save) => write_guide(&w.view.guide, &w.file),
        Some(GuideAction::Export(f)) => {
            w.view.message = Some(match export(&mut w, f) {
                Ok(out) => {
                    let _ = launch::reveal(&out);
                    (format!("Saved as {}", out.file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default()), false)
                }
                Err(e) => (format!("It could not be saved: {e}"), true),
            });
        }
        Some(GuideAction::ShowFolder) => {
            let _ = launch::reveal(&w.file);
        }
        Some(GuideAction::Close) => {
            write_guide(&w.view.guide, &w.file);
            return;
        }
        None => {}
    }
    svc.steps.window = Some(w);
}

/// The bar's window: [`BAR`], or wider when its words need more.
fn bar_fit() -> Fit {
    Fit::new("steps-bar", BAR)
}

/// The bar while steps are taken: a red dot, how many, and Stop. True when Stop was pressed.
fn bar_ui(ui: &mut egui::Ui, count: usize, failed: u32) -> bool {
    let t = Tokens::get(ui.ctx());
    let words = match (count, failed) {
        (0, _) => "Click what to show, step by step".to_string(),
        (n, 0) => format!("{n} step{}", if n == 1 { "" } else { "s" }),
        (n, f) => format!("{n} step{}, {f} missed", if n == 1 { "" } else { "s" }),
    };
    let ((), stop) = fit::ends(
        ui,
        |ui| {
            let (dot, _) = ui.allocate_exact_size(vec2(14.0, 14.0), egui::Sense::hover());
            ui.painter().circle_filled(dot.center(), 6.0, egui::Color32::from_rgb(220, 40, 40));
            ui.label(RichText::new(words).color(t.text));
        },
        |ui| widgets::button(ui, "Stop").on_hover_text("Stop, and put the steps in order and into words").clicked(),
    );
    stop
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;

    use super::*;

    #[test]
    fn a_guides_pictures_are_in_a_folder_beside_it() {
        let f = Path::new("C:/Pictures/ScreenGrabCraft/Guide 2026-11-21 10.00.00.sgguide");
        assert_eq!(pictures_folder(f), Path::new("C:/Pictures/ScreenGrabCraft/Guide 2026-11-21 10.00.00 files"));
    }

    #[test]
    fn the_bar_counts_the_steps_and_stops() {
        let mut h = Harness::builder().with_size(BAR).build_ui_state(
            |ui, s: &mut (usize, bool)| {
                egui::CentralPanel::default().show(ui, |ui| {
                    if bar_ui(ui, s.0, 0) {
                        s.1 = true;
                    }
                });
            },
            (0usize, false),
        );
        h.run_steps(2);
        h.get_by_label("Click what to show, step by step");
        h.state_mut().0 = 3;
        h.run_steps(2);
        h.get_by_label("3 steps");
        h.get_by_label("Stop").click();
        h.run_steps(2);
        assert!(h.state().1);
    }

    /// The bar's words never run under Stop: with the interface's font, at every display scale,
    /// the bar grows to them.
    #[test]
    fn the_bars_words_stay_clear_of_stop_at_every_scale() {
        for scale in crate::fit_check::SCALES {
            for (count, failed) in [(0, 0), (12, 3), (1_234, 567)] {
                let (mut h, size, found) =
                    crate::fit_check::fitted(bar_fit(), widgets::popup_frame, scale, screengrabcraft_model::Theme::Light, move |ui| {
                        let _ = bar_ui(ui, count, failed);
                    });
                assert!(found.is_empty(), "{count} steps, {failed} missed, at {scale}: {found:?}");
                assert!(size.x >= BAR.x && size.y == BAR.y, "{size:?}");
                crate::fit_check::shot(&mut h, &format!("steps-bar-{count}"), scale);
            }
        }
    }
}
