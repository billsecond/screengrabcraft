//! The desktop's answers to what the editor asks for: native file dialogs (on a worker thread,
//! so the window keeps drawing), files, the clipboard, the captures folder and its thumbnails,
//! the capture service.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, Sender, channel};
use std::sync::{Arc, Mutex, PoisonError};

use screengrabcraft_desktop::{clipboard, launch, paths, store};
use screengrabcraft_model::{CaptureMode, EditorSettings, PROJECT_EXT, Raster, Stamp};
use screengrabcraft_ui::services::{DialogResult, Filter, OpenedFile, RecentItem, Services};

/// Files larger than this are not opened.
const MAX_OPEN: u64 = 512 * 1024 * 1024;
const THUMB: u32 = 200;
const PICTURES: &[&str] = &["png", "jpg", "jpeg", "gif", "bmp", "webp"];

pub struct DesktopServices {
    ctx: egui::Context,
    tx: Sender<DialogResult>,
    rx: Receiver<DialogResult>,
    thumbs: Arc<Mutex<HashMap<String, Option<Raster>>>>,
    thumb_tx: Sender<String>,
}

impl DesktopServices {
    pub fn new(ctx: egui::Context) -> DesktopServices {
        let (tx, rx) = channel();
        let thumbs: Arc<Mutex<HashMap<String, Option<Raster>>>> = Arc::new(Mutex::new(HashMap::new()));
        let (thumb_tx, thumb_rx) = channel::<String>();
        let (store_, wake) = (thumbs.clone(), ctx.clone());
        let spawned = std::thread::Builder::new().name("thumbnails".into()).spawn(move || {
            for path in thumb_rx {
                let small = std::fs::read(&path).ok().and_then(|b| screengrabcraft_render::decode(&b).ok()).and_then(|r| shrink(&r));
                store_.lock().unwrap_or_else(PoisonError::into_inner).insert(path, small);
                wake.request_repaint();
            }
        });
        if let Err(e) = spawned {
            log::warn!("no thumbnail worker: {e}");
        }
        DesktopServices { ctx, tx, rx, thumbs, thumb_tx }
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

    fn recent(&self) -> Vec<RecentItem> {
        let dir = paths::captures_dir(&store::load_settings());
        let Ok(entries) = std::fs::read_dir(&dir) else { return Vec::new() };
        let mut items: Vec<RecentItem> = entries
            .filter_map(Result::ok)
            .filter(|e| e.path().extension().and_then(|x| x.to_str()).is_some_and(|x| PICTURES.contains(&x.to_ascii_lowercase().as_str())))
            .filter_map(|e| {
                let modified = e.metadata().ok()?.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
                let path: PathBuf = e.path();
                Some(RecentItem { name: e.file_name().to_string_lossy().to_string(), path: path.to_string_lossy().to_string(), modified })
            })
            .collect();
        items.sort_by_key(|i| std::cmp::Reverse(i.modified));
        items.truncate(40);
        items
    }

    fn thumbnail(&self, path: &str) -> Option<Raster> {
        let mut map = self.thumbs.lock().unwrap_or_else(PoisonError::into_inner);
        match map.get(path) {
            Some(t) => t.clone(),
            None => {
                map.insert(path.to_string(), None);
                let _ = self.thumb_tx.send(path.to_string());
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
