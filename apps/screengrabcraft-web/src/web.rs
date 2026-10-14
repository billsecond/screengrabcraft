//! The browser shell: the editor's services in a browser (the file picker, downloads, settings in
//! local storage, dropped files), and the eframe web runner.

use std::cell::RefCell;
use std::rc::Rc;

use screengrabcraft_model::{Color, EditorSettings, Raster};
use screengrabcraft_ui::Editor;
use screengrabcraft_ui::services::{DialogResult, Filter, OpenedFile, Services};
use wasm_bindgen::JsCast as _;

const CANVAS_ID: &str = "screengrabcraft_canvas";
const LOADING_ID: &str = "screengrabcraft_loading";
const SETTINGS_KEY: &str = "screengrabcraft.editor.v1";

pub fn start() {
    eframe::WebLogger::init(log::LevelFilter::Info).ok();
    wasm_bindgen_futures::spawn_local(async {
        let Some(document) = web_sys::window().and_then(|w| w.document()) else {
            log::error!("no document");
            return;
        };
        let Some(canvas) = document.get_element_by_id(CANVAS_ID).and_then(|e| e.dyn_into::<web_sys::HtmlCanvasElement>().ok()) else {
            log::error!("missing <canvas id=\"{CANVAS_ID}\">");
            return;
        };
        let mut options = eframe::WebOptions::default();
        if query().contains("webgl")
            && let eframe::egui_wgpu::WgpuSetup::CreateNew(create) = &mut options.wgpu_options.wgpu_setup
        {
            create.instance_descriptor.backends = eframe::wgpu::Backends::GL;
        }
        let result = eframe::WebRunner::new()
            .start(
                canvas,
                options,
                Box::new(move |cc| {
                    let settings = stored(SETTINGS_KEY).map(|t| EditorSettings::from_json(&t)).unwrap_or_default();
                    let services = WebServices { ctx: cc.egui_ctx.clone(), answers: Rc::new(RefCell::new(Vec::new())) };
                    let mut editor = Editor::new(Box::new(services), settings);
                    editor.settings.show_tray = false;
                    editor.open_picture(welcome(), Some("Welcome".to_string()));
                    editor.session.dirty = false;
                    Ok(Box::new(Shell(editor)))
                }),
            )
            .await;
        if let Some(el) = document.get_element_by_id(LOADING_ID) {
            match result {
                Ok(()) => el.remove(),
                Err(e) => el.set_inner_html(&format!(
                    "<p>ScreenGrabCraft Editor failed to start: {e:?}</p><p>A browser with WebGPU or WebGL2 is required.</p>"
                )),
            }
        }
    });
}

struct Shell(Editor);

impl eframe::App for Shell {
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.0.logic(ctx);
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.0.ui(ui);
    }
}

/// The first picture: a plain board to try the tools on.
fn welcome() -> Raster {
    Raster::filled(1200, 700, Color::rgb(0xF4, 0xF5, 0xF8)).unwrap_or_else(|_| Raster::single(Color::WHITE))
}

fn query() -> String {
    web_sys::window().and_then(|w| w.location().search().ok()).unwrap_or_default()
}

fn storage() -> Option<web_sys::Storage> {
    web_sys::window()?.local_storage().ok()?
}

fn stored(key: &str) -> Option<String> {
    storage()?.get_item(key).ok()?
}

struct WebServices {
    ctx: egui::Context,
    answers: Rc<RefCell<Vec<DialogResult>>>,
}

impl Services for WebServices {
    fn request_open(&self) {
        let (answers, ctx) = (self.answers.clone(), self.ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let picked = rfd::AsyncFileDialog::new()
                .add_filter("Pictures and projects", &["png", "jpg", "jpeg", "gif", "bmp", "webp", "sgcraft"])
                .pick_file()
                .await;
            let answer = match picked {
                Some(f) => {
                    let bytes = f.read().await;
                    DialogResult::Opened(vec![OpenedFile { name: f.file_name(), path: None, bytes }])
                }
                None => DialogResult::Cancelled,
            };
            answers.borrow_mut().push(answer);
            ctx.request_repaint();
        });
    }

    fn request_save(&self, _suggested_name: &str, _filters: &[Filter]) {}

    fn poll_dialog(&self) -> Option<DialogResult> {
        let mut a = self.answers.borrow_mut();
        (!a.is_empty()).then(|| a.remove(0))
    }

    fn has_paths(&self) -> bool {
        false
    }

    fn download(&self, name: &str, bytes: &[u8]) -> Result<(), String> {
        download(name, bytes)
    }

    fn save_settings(&self, settings: &EditorSettings) {
        if let (Some(s), Ok(json)) = (storage(), serde_json::to_string(settings)) {
            let _ = s.set_item(SETTINGS_KEY, &json);
        }
    }

    fn open_url(&self, url: &str) {
        if let Some(w) = web_sys::window() {
            let _ = w.open_with_url_and_target(url, "_blank");
        }
    }

    fn read_dropped(&self, file: egui::DroppedFileHandle) {
        let (answers, ctx) = (self.answers.clone(), self.ctx.clone());
        wasm_bindgen_futures::spawn_local(async move {
            let name = file.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Dropped picture".to_string());
            match file.bytes_async().await {
                Ok(bytes) => answers.borrow_mut().push(DialogResult::Opened(vec![OpenedFile { name, path: None, bytes }])),
                Err(e) => log::warn!("{name}: {e}"),
            }
            ctx.request_repaint();
        });
    }
}

/// Hands `bytes` to the browser as a download named `name`.
fn download(name: &str, bytes: &[u8]) -> Result<(), String> {
    let js = |e: wasm_bindgen::JsValue| format!("{e:?}");
    let window = web_sys::window().ok_or("no window")?;
    let document = window.document().ok_or("no document")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let opts = web_sys::BlobPropertyBag::new();
    opts.set_type(mime_for(name));
    let blob = web_sys::Blob::new_with_u8_array_sequence_and_options(&parts, &opts).map_err(js)?;
    let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(js)?;
    let a: web_sys::HtmlAnchorElement = document.create_element("a").map_err(js)?.dyn_into().map_err(|_| "not an anchor")?;
    a.set_href(&url);
    a.set_download(name);
    a.style().set_property("display", "none").map_err(js)?;
    let body = document.body().ok_or("no body")?;
    body.append_child(&a).map_err(js)?;
    a.click();
    a.remove();
    // Revoked after the click has been dispatched; the download keeps its own reference.
    let revoke = wasm_bindgen::closure::Closure::once_into_js(move || {
        web_sys::Url::revoke_object_url(&url).ok();
    });
    window.set_timeout_with_callback_and_timeout_and_arguments_0(revoke.unchecked_ref(), 10_000).map_err(js)?;
    Ok(())
}

fn mime_for(name: &str) -> &'static str {
    let ext = name.rsplit('.').next().map(str::to_ascii_lowercase).unwrap_or_default();
    screengrabcraft_model::ImageFormat::from_extension(&ext).map(|f| f.mime()).unwrap_or(if ext == "sgcraft" {
        "application/json"
    } else {
        "application/octet-stream"
    })
}
