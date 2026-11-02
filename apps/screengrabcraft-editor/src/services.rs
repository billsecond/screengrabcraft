//! The desktop's answers to what the editor asks for: native file dialogs (on a worker thread,
//! so the window keeps drawing), files, saving as the owner works (on a thread of its own), the
//! Recycle Bin, the clipboard, the captures folder and its thumbnails, the capture service.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};

use screengrabcraft_desktop::{clipboard, launch, paths, store};
use screengrabcraft_model::{CaptureMode, EditorSettings, ImageFormat, PROJECT_EXT, Raster, Stamp};
use screengrabcraft_ui::services::{DialogResult, Filter, OpenedFile, RecentItem, SaveJob, Saved, Services};

/// Files larger than this are not opened.
const MAX_OPEN: u64 = 512 * 1024 * 1024;
const THUMB: u32 = 200;
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

/// Thumbnails by path: the time of the picture each is of, and the thumbnail once it's made.
type Thumbs = Arc<Mutex<HashMap<String, (u64, Option<Raster>)>>>;

pub struct DesktopServices {
    ctx: egui::Context,
    tx: Sender<DialogResult>,
    rx: Receiver<DialogResult>,
    thumbs: Thumbs,
    thumb_tx: Sender<(String, u64)>,
    /// The thread that writes saves, and its answers; none if it couldn't start (saves are
    /// then written at once).
    save_tx: Option<Sender<SaveJob>>,
    saved_rx: Receiver<Saved>,
}

impl DesktopServices {
    pub fn new(ctx: egui::Context) -> DesktopServices {
        let (tx, rx) = channel();
        let thumbs: Thumbs = Arc::new(Mutex::new(HashMap::new()));
        let (thumb_tx, thumb_rx) = channel::<(String, u64)>();
        let (store_, wake) = (thumbs.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("thumbnails".into()).spawn(move || {
            for (path, modified) in thumb_rx {
                let small = std::fs::read(&path).ok().and_then(|b| screengrabcraft_render::decode(&b).ok()).and_then(|r| shrink(&r));
                let mut map = store_.lock().unwrap_or_else(PoisonError::into_inner);
                // Only if no newer picture has been asked for meanwhile.
                if map.get(&path).is_none_or(|(m, _)| *m == modified) {
                    map.insert(path, (modified, small));
                }
                drop(map);
                wake.request_repaint();
            }
        });
        if let Err(e) = spawned {
            log::warn!("no thumbnail worker: {e}");
        }
        let (save_tx, save_rx) = channel::<SaveJob>();
        let (saved_tx, saved_rx) = channel::<Saved>();
        let wake = ctx.clone();
        let spawned = std::thread::Builder::new().name("autosave".into()).spawn(move || {
            for job in save_rx {
                let _ = saved_tx.send(write_save(&job));
                wake.request_repaint();
            }
        });
        let save_tx = match spawned {
            Ok(_) => Some(save_tx),
            Err(e) => {
                log::warn!("no autosave worker, saves are written as they come: {e}");
                None
            }
        };
        DesktopServices { ctx, tx, rx, thumbs, thumb_tx, save_tx, saved_rx }
    }
}

/// Writes a save. Whatever happens in drawing or encoding, an answer comes back: the editor
/// waits for it before it closes.
fn write_save(job: &SaveJob) -> Saved {
    let write = |path: &str, bytes: &[u8]| store::write_atomic(Path::new(path), bytes).map_err(|e| e.to_string());
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| job.run(&write))) {
        Ok(saved) => {
            if let Err(e) = &saved.result {
                log::warn!("saving failed: {e}");
            }
            saved
        }
        Err(_) => {
            log::error!("saving failed unexpectedly");
            Saved { id: job.id, result: Err("something went wrong while saving".to_string()) }
        }
    }
}

/// `dir/name.ext`, or `dir/name (2).ext` and on if that is taken.
fn free_path(dir: &Path, name: &str, ext: &str) -> PathBuf {
    let first = dir.join(format!("{name}.{ext}"));
    if !first.exists() {
        return first;
    }
    for n in 2..10_000 {
        let p = dir.join(format!("{name} ({n}).{ext}"));
        if !p.exists() {
            return p;
        }
    }
    dir.join(format!("{name} {}.{ext}", std::process::id()))
}

/// Moves a file to the Recycle Bin (the Trash). On a thread of its own: the system's bin wants
/// a fresh thread to work on, and nothing that goes wrong there reaches the window.
fn to_trash(path: PathBuf) -> Result<(), String> {
    let worker = std::thread::Builder::new().name("trash".into()).spawn(move || {
        #[allow(unused_mut)]
        let mut bin = trash::TrashContext::default();
        // The Finder way asks for permission to control the Finder; the file manager's doesn't.
        #[cfg(target_os = "macos")]
        {
            use trash::macos::{DeleteMethod, TrashContextExtMacos};
            bin.set_delete_method(DeleteMethod::NsFileManager);
        }
        bin.delete(&path).map_err(|e| e.to_string())
    });
    match worker {
        Ok(handle) => handle.join().unwrap_or_else(|_| Err("the Recycle Bin could not be reached".to_string())),
        Err(e) => Err(e.to_string()),
    }
}

fn shrink(r: &Raster) -> Option<Raster> {
    let f = (THUMB as f32 / r.width() as f32).min(THUMB as f32 / r.height() as f32).min(1.0);
    let (w, h) = (((r.width() as f32 * f).round() as u32).max(1), ((r.height() as f32 * f).round() as u32).max(1));
    screengrabcraft_render::effects::resize(r, w, h).ok()
}

fn read_limited(path: &Path) -> Result<Vec<u8>, String> {
    let meta = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if meta.len() > MAX_OPEN {
        return Err("the file is too large".to_string());
    }
    std::fs::read(path).map_err(|e| e.to_string())
}

impl Services for DesktopServices {
    fn copy_image(&self, picture: &Raster) -> Result<(), String> {
        clipboard::copy_image(picture)
    }

    fn paste_image(&self) -> Option<Raster> {
        clipboard::paste_image()
    }

    fn request_open(&self) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let start = paths::captures_dir(&store::load_settings());
        let _ = std::thread::Builder::new().name("open-dialog".into()).spawn(move || {
            let picked = pollster::block_on(
                rfd::AsyncFileDialog::new()
                    .set_title("Open")
                    .set_directory(&start)
                    .add_filter("Pictures and projects", &["png", "jpg", "jpeg", "gif", "bmp", "webp", PROJECT_EXT])
                    .add_filter("ScreenGrabCraft project", &[PROJECT_EXT])
                    .add_filter("All files", &["*"])
                    .pick_file(),
            );
            let answer = match picked {
                Some(f) => {
                    let path = f.path().to_path_buf();
                    match read_limited(&path) {
                        Ok(bytes) => {
                            DialogResult::Opened(vec![OpenedFile { name: f.file_name(), path: Some(path.to_string_lossy().to_string()), bytes }])
                        }
                        Err(e) => {
                            log::warn!("{}: {e}", path.display());
                            DialogResult::Cancelled
                        }
                    }
                }
                None => DialogResult::Cancelled,
            };
            let _ = tx.send(answer);
            ctx.request_repaint();
        });
    }

    fn request_save(&self, suggested_name: &str, filters: &[Filter]) {
        let (tx, ctx) = (self.tx.clone(), self.ctx.clone());
        let name = suggested_name.to_string();
        let filters: Vec<(String, Vec<String>)> =
            filters.iter().map(|(n, e)| ((*n).to_string(), e.iter().map(|x| (*x).to_string()).collect())).collect();
        let start = paths::captures_dir(&store::load_settings());
        let _ = std::thread::Builder::new().name("save-dialog".into()).spawn(move || {
            let mut d = rfd::AsyncFileDialog::new().set_title("Save").set_file_name(&name).set_directory(&start);
            for (n, exts) in &filters {
                d = d.add_filter(n, exts);
            }
            let answer = match pollster::block_on(d.save_file()) {
                Some(f) => DialogResult::SaveTo(f.path().to_string_lossy().to_string()),
                None => DialogResult::Cancelled,
            };
            let _ = tx.send(answer);
            ctx.request_repaint();
        });
    }

    fn poll_dialog(&self) -> Option<DialogResult> {
        self.rx.try_recv().ok()
    }

    fn read_file(&self, path: &str) -> Result<Vec<u8>, String> {
        read_limited(Path::new(path))
    }

    fn write_file(&self, path: &str, bytes: &[u8]) -> Result<(), String> {
        store::write_atomic(Path::new(path), bytes).map_err(|e| e.to_string())
    }

    fn start_save(&self, job: SaveJob) -> Option<Saved> {
        match &self.save_tx {
            Some(tx) => match tx.send(job) {
                Ok(()) => None,
                Err(back) => Some(write_save(&back.0)),
            },
            None => Some(write_save(&job)),
        }
    }

    fn poll_saved(&self) -> Option<Saved> {
        self.saved_rx.try_recv().ok()
    }

    fn new_picture_path(&self, source: &str) -> Result<String, String> {
        let s = store::load_settings();
        let dir = paths::captures_dir(&s);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let name = screengrabcraft_model::settings::expand_name(&s.file_name, screengrabcraft_desktop::time::now(), source, 1);
        // The editor writes pictures, not recordings or documents.
        let format = match s.format {
            ImageFormat::Gif | ImageFormat::Pdf => ImageFormat::Png,
            f => f,
        };
        Ok(free_path(&dir, &name, format.extension()).to_string_lossy().to_string())
    }

    fn trash(&self, path: &str) -> Result<(), String> {
        let p = PathBuf::from(path);
        if std::fs::symlink_metadata(&p).is_err() {
            return Ok(());
        }
        to_trash(p)?;
        log::info!("moved {} to the bin", Path::new(path).file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_default());
        Ok(())
    }

    fn recent(&self) -> Vec<RecentItem> {
        let dir = paths::captures_dir(&store::load_settings());
        let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
        let mut items: Vec<RecentItem> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| PICTURES.contains(&x.to_ascii_lowercase().as_str())))
            .filter_map(|e| {
                let since = e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?;
                let modified = u64::try_from(since.as_millis()).unwrap_or(u64::MAX);
                let path: PathBuf = e.path();
                Some(RecentItem { name: e.file_name().to_string_lossy().to_string(), path: path.to_string_lossy().to_string(), modified })
            })
            .collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.modified));
        items.truncate(40);
        items
    }

    fn thumbnail(&self, path: &str, modified: u64) -> Option<Raster> {
        let mut map = self.thumbs.lock().unwrap_or_else(PoisonError::into_inner);
        match map.get(path) {
            Some((m, t)) if *m == modified => t.clone(),
            _ => {
                map.insert(path.to_string(), (modified, None));
                let _ = self.thumb_tx.send((path.to_string(), modified));
                None
            }
        }
    }

    fn open_folder(&self, path: Option<&str>) {
        let target = match path {
            Some(p) => PathBuf::from(p),
            None => {
                let d = paths::captures_dir(&store::load_settings());
                let _ = std::fs::create_dir_all(&d);
                d
            }
        };
        if let Err(e) = launch::reveal(&target) {
            log::warn!("showing {}: {e}", target.display());
        }
    }

    fn new_capture(&self, mode: CaptureMode) -> Result<(), String> {
        // The service takes this window out of the picture first, as for any capture.
        launch::ask_service(&["--capture".to_string(), mode.arg().to_string()])
    }

    fn save_settings(&self, settings: &EditorSettings) {
        store::save_editor_settings(settings);
    }

    fn now(&self) -> Stamp {
        screengrabcraft_desktop::time::now()
    }

    fn open_url(&self, url: &str) {
        let _ = launch::open_url(url);
    }
}
