//! The editor window: menus, the tool bar, the canvas, the properties, the strip of recent
//! captures and the status bar, and the file handling behind them.

use egui::{Color32, Key, KeyboardShortcut, Modifiers, Ui, vec2};
use screengrabcraft_engine::Session;
use screengrabcraft_model::{CaptureMode, Document, EditorSettings, ImageFormat, Kind, ObjectId, PROJECT_EXT, Raster, Rect, StampKind, Style};
use screengrabcraft_render::{Renderer, project};
use serde_json::{Value, json};

use crate::canvas::{Drag, View};
use crate::dialogs::Dialog;
use crate::icons::{self, Icon};
use crate::services::{DialogResult, OpenedFile, Services};
use crate::theme::Tokens;

/// What the pointer does on the canvas.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tool {
    Select,
    Draw(Kind),
    Crop,
}

impl Tool {
    /// The tool bar, in order: the tool, its icon, its name and its key.
    pub const BAR: [(Tool, Icon, &'static str, Key); 16] = [
        (Tool::Select, Icon::Select, "Select and move", Key::V),
        (Tool::Draw(Kind::Arrow), Icon::Arrow, "Arrow", Key::A),
        (Tool::Draw(Kind::Line), Icon::Line, "Line", Key::L),
        (Tool::Draw(Kind::Rect), Icon::Rect, "Rectangle", Key::R),
        (Tool::Draw(Kind::Ellipse), Icon::Ellipse, "Ellipse", Key::O),
        (Tool::Draw(Kind::Text), Icon::Text, "Text", Key::T),
        (Tool::Draw(Kind::Callout), Icon::Callout, "Callout", Key::C),
        (Tool::Draw(Kind::Step), Icon::Step, "Step number", Key::N),
        (Tool::Draw(Kind::Pen), Icon::Pen, "Pen", Key::P),
        (Tool::Draw(Kind::Highlight), Icon::Highlight, "Highlighter", Key::H),
        (Tool::Draw(Kind::Blur), Icon::Blur, "Blur", Key::B),
        (Tool::Draw(Kind::Pixelate), Icon::Pixelate, "Pixelate", Key::X),
        (Tool::Draw(Kind::Spotlight), Icon::Spotlight, "Spotlight", Key::S),
        (Tool::Draw(Kind::Magnify), Icon::Magnify, "Magnify", Key::M),
        (Tool::Draw(Kind::Stamp), Icon::Stamp, "Stamp", Key::G),
        (Tool::Crop, Icon::Crop, "Crop", Key::K),
    ];

    pub fn kind(self) -> Option<Kind> {
        match self {
            Tool::Draw(k) => Some(k),
            _ => None,
        }
    }
}

/// A question waiting for an answer before something replaces unsaved work.
#[derive(Clone, Debug)]
pub enum Pending {
    Open(OpenedFile),
    Picture(Raster, Option<String>),
    Quit,
}

pub struct Editor {
    pub session: Session,
    pub settings: EditorSettings,
    /// The project file the document was opened from or saved to.
    pub project_path: Option<String>,
    /// The picture file it was opened from (a capture), if any.
    pub picture_path: Option<String>,
    /// The name shown in the title.
    pub name: String,
    pub tool: Tool,
    pub stamp: StampKind,
    /// What the shape tools start with, besides their style.
    pub arrow_heads: screengrabcraft_model::ArrowHeads,
    pub rect_radius: f32,
    pub spot_ellipse: bool,
    /// A change made in the properties panel is under way (one undo step until let go).
    pub(crate) props_editing: bool,
    pub view: View,
    pub(crate) renderer: Renderer,
    pub(crate) tiles: Vec<crate::canvas::Tile>,
    pub(crate) texture_key: (u64, Option<ObjectId>),
    pub(crate) checker: Option<egui::TextureHandle>,
    pub(crate) drag: Option<Drag>,
    /// The text annotation being typed into, and its text as typed.
    pub(crate) editing: Option<ObjectId>,
    pub(crate) edit_buffer: String,
    last_dark: Option<bool>,
    pub(crate) crop: Option<Rect>,
    pub(crate) dialog: Option<Dialog>,
    pub(crate) pending: Option<Pending>,
    /// What a save dialog that is open will save as.
    pub(crate) saving: Option<ImageFormat>,
    pub(crate) saving_project: bool,
    toast: Option<(String, f64, bool)>,
    pub(crate) thumbs: std::collections::HashMap<String, egui::TextureHandle>,
    pub(crate) recent: Vec<crate::services::RecentItem>,
    recent_at: f64,
    pub services: Box<dyn Services>,
    /// Set when the window should close (the host closes it).
    pub quit_requested: bool,
    /// Whether the window may close now (no unsaved work, or the owner said so).
    pub close_confirmed: bool,
    fonts_installed: bool,
    theme_applied: Option<screengrabcraft_model::Theme>,
    pub(crate) pointer_px: Option<(i32, i32)>,
}

const TOAST_SECS: f64 = 3.5;

impl Editor {
    pub fn new(services: Box<dyn Services>, settings: EditorSettings) -> Editor {
        let blank =
            Raster::filled(800, 500, screengrabcraft_model::Color::WHITE).unwrap_or_else(|_| Raster::single(screengrabcraft_model::Color::WHITE));
        Editor {
            session: Session::new(Document::new(blank)),
            settings,
            project_path: None,
            picture_path: None,
            name: "Untitled".to_string(),
            tool: Tool::Draw(Kind::Arrow),
            stamp: StampKind::Check,
            arrow_heads: screengrabcraft_model::ArrowHeads::End,
            rect_radius: 0.0,
            spot_ellipse: false,
            props_editing: false,
            view: View::default(),
            renderer: Renderer::new(),
            tiles: Vec::new(),
            texture_key: (0, None),
            checker: None,
            drag: None,
            editing: None,
            edit_buffer: String::new(),
            last_dark: None,
            crop: None,
            dialog: None,
            pending: None,
            saving: None,
            saving_project: false,
            toast: None,
            thumbs: std::collections::HashMap::new(),
            recent: Vec::new(),
            recent_at: f64::NEG_INFINITY,
            services,
            quit_requested: false,
            close_confirmed: false,
            fonts_installed: false,
            theme_applied: None,
            pointer_px: None,
        }
    }

    /// Whether the look changed since the last frame (the canvas's checks are redrawn).
    pub(crate) fn theme_changed(&mut self, ctx: &egui::Context) -> bool {
        let dark = ctx.global_style().visuals.dark_mode;
        let changed = self.last_dark.is_some_and(|d| d != dark);
        self.last_dark = Some(dark);
        changed
    }

    // ---- messages ----

    pub fn toast(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        self.toast = Some((text.into(), ctx.input(|i| i.time), false));
    }

    pub fn error(&mut self, ctx: &egui::Context, text: impl Into<String>) {
        let text = text.into();
        log::warn!("{text}");
        self.toast = Some((text, ctx.input(|i| i.time), true));
    }

    // ---- documents ----

    /// Runs an engine command and reports a failure.
    pub fn run(&mut self, ctx: &egui::Context, id: &str, params: Value) -> Option<Value> {
        match self.session.execute(id, &params) {
            Ok(v) => Some(v),
            Err(e) => {
                self.error(ctx, e.to_string());
                None
            }
        }
    }

    fn replace_document(&mut self, doc: Document, name: String, project: Option<String>, picture: Option<String>) {
        self.session.replace(doc);
        self.name = name;
        self.project_path = project;
        self.picture_path = picture;
        self.drag = None;
        self.editing = None;
        self.crop = None;
        self.view.fit_pending = true;
        if self.tool == Tool::Crop {
            self.tool = Tool::Select;
        }
    }

    /// Opens a picture or project from its bytes (asks first if there is unsaved work).
    pub fn open_file(&mut self, ctx: &egui::Context, file: OpenedFile) {
        if self.session.dirty {
            self.pending = Some(Pending::Open(file));
            return;
        }
        self.open_now(ctx, file);
    }

    fn open_now(&mut self, ctx: &egui::Context, file: OpenedFile) {
        let stem = std::path::Path::new(&file.name).file_stem().and_then(|s| s.to_str()).unwrap_or("Untitled").to_string();
        if project::sniff(&file.bytes) {
            match project::load(&file.bytes) {
                Ok((doc, skipped)) => {
                    self.replace_document(doc, stem, file.path.clone(), None);
                    if skipped > 0 {
                        self.error(ctx, format!("{skipped} annotation(s) in the file could not be read and were left out"));
                    }
                }
                Err(e) => self.error(ctx, format!("{}: {e}", file.name)),
            }
        } else {
            match screengrabcraft_render::decode(&file.bytes) {
                Ok(r) => {
                    let mut doc = Document::new(r);
                    doc.meta.source = Some(file.name.clone());
                    // A capture's annotations are kept beside it, if they were saved before.
                    let project = file.path.as_ref().map(|p| sibling_project(p));
                    if let Some(pp) = &project
                        && let Ok(bytes) = self.services.read_file(pp)
                        && let Ok((saved, _)) = project::load(&bytes)
                    {
                        self.replace_document(saved, stem, project.clone(), file.path.clone());
                        self.toast(ctx, "Opened with the annotations saved for it");
                        return;
                    }
                    self.replace_document(doc, stem, None, file.path.clone());
                }
                Err(e) => self.error(ctx, format!("{}: {e}", file.name)),
            }
        }
        if let Some(p) = &file.path {
            self.settings.remember(p);
            self.services.save_settings(&self.settings);
        }
    }

    /// Opens a file by path through the host.
    pub fn open_path(&mut self, ctx: &egui::Context, path: &str) {
        match self.services.read_file(path) {
            Ok(bytes) => {
                let name = std::path::Path::new(path).file_name().and_then(|s| s.to_str()).unwrap_or(path).to_string();
                self.open_file(ctx, OpenedFile { name, path: Some(path.to_string()), bytes });
            }
            Err(e) => self.error(ctx, format!("{path}: {e}")),
        }
    }

    /// Starts editing a fresh picture (a paste, or a capture handed over without a file).
    pub fn open_picture(&mut self, raster: Raster, name: Option<String>) {
        if self.session.dirty {
            self.pending = Some(Pending::Picture(raster, name));
            return;
        }
        let n = name.unwrap_or_else(|| "Pasted picture".to_string());
        self.replace_document(Document::new(raster), n, None, None);
    }

    /// Pasted text becomes a text box in the middle of the view.
    fn paste_text(&mut self, ctx: &egui::Context, text: &str) {
        let text: String = text.chars().take(4000).collect();
        let style = self.tool_style(Kind::Text);
        let (dw, dh) = (self.session.doc.width() as f32, self.session.doc.height() as f32);
        let width = (dw * 0.4).clamp(160.0, 520.0).min(dw);
        let c = self.view.to_image(self.view.canvas.center());
        let at = screengrabcraft_model::Pt::new((c.x - width / 2.0).clamp(0.0, (dw - width).max(0.0)), c.y.clamp(0.0, (dh - 40.0).max(0.0)));
        let rect = screengrabcraft_render::text::fit_box(&text, &style, at, Some(width));
        self.session.begin("Paste Text");
        match self.session.doc.add(screengrabcraft_model::Shape::Text { rect, text }, style) {
            Ok(id) => {
                self.session.selection = vec![id];
                self.session.commit();
                self.tool = Tool::Select;
            }
            Err(e) => {
                self.session.cancel();
                self.error(ctx, e.to_string());
            }
        }
    }

    fn paste(&mut self, ctx: &egui::Context, as_new: bool) {
        let Some(pic) = self.services.paste_image() else {
            let text = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Paste(t) if !t.trim().is_empty() => Some(t.clone()),
                    _ => None,
                })
            });
            match text {
                Some(t) if !as_new => self.paste_text(ctx, &t),
                _ => self.error(ctx, "There is no picture on the clipboard"),
            }
            return;
        };
        if as_new {
            self.open_picture(pic, None);
            return;
        }
        // Into the picture, as a placed picture no larger than most of it.
        let (dw, dh) = (self.session.doc.width() as f32, self.session.doc.height() as f32);
        let (pw, ph) = (pic.width() as f32, pic.height() as f32);
        let f = (dw * 0.8 / pw).min(dh * 0.8 / ph).min(1.0);
        let (w, h) = (pw * f, ph * f);
        let rect = Rect::new((dw - w) / 2.0, (dh - h) / 2.0, w, h);
        self.session.begin("Paste");
        let key = match self.session.doc.add_image(pic) {
            Ok(k) => k,
            Err(e) => {
                self.session.cancel();
                self.error(ctx, e.to_string());
                return;
            }
        };
        match self.session.doc.add(screengrabcraft_model::Shape::Image { rect, image: key }, Kind::Image.default_style()) {
            Ok(id) => {
                self.session.selection = vec![id];
                self.session.commit();
                self.tool = Tool::Select;
            }
            Err(e) => {
                self.session.cancel();
                self.error(ctx, e.to_string());
            }
        }
    }

    /// The document flattened.
    pub fn flattened(&mut self) -> Option<Raster> {
        screengrabcraft_render::render(&self.session.doc)
    }

    pub fn copy(&mut self, ctx: &egui::Context) {
        let Some(pic) = self.flattened() else { return };
        match self.services.copy_image(&pic) {
            Ok(()) => self.toast(ctx, "Copied to the clipboard"),
            Err(e) => self.error(ctx, e),
        }
    }

    /// Ctrl+S: the project file it has, else ask.
    pub fn save(&mut self, ctx: &egui::Context) {
        let path = self.project_path.clone().or_else(|| self.picture_path.as_ref().map(|p| sibling_project(p)));
        match path {
            Some(p) if self.services.has_paths() => self.write_project(ctx, &p),
            _ => self.save_as(),
        }
    }

    pub fn save_as(&mut self) {
        self.saving = None;
        self.saving_project = true;
        let name = format!("{}.{PROJECT_EXT}", self.name);
        if self.services.has_paths() {
            self.services.request_save(&name, &[("ScreenGrabCraft project", &[PROJECT_EXT])]);
        } else if let Ok(bytes) = project::save(&self.session.doc) {
            let _ = self.services.download(&name, &bytes);
            self.session.dirty = false;
        }
    }

    pub fn export(&mut self, format: ImageFormat) {
        self.saving = Some(format);
        self.saving_project = false;
        let name = format!("{}.{}", self.name, format.extension());
        if self.services.has_paths() {
            let exts: &'static [&'static str] = match format {
                ImageFormat::Png => &["png"],
                ImageFormat::Jpeg => &["jpg", "jpeg"],
                ImageFormat::Webp => &["webp"],
                ImageFormat::Bmp => &["bmp"],
                ImageFormat::Gif => &["gif"],
                ImageFormat::Pdf => &["pdf"],
            };
            self.services.request_save(&name, &[(format.label(), exts)]);
        } else if let Some(pic) = self.flattened()
            && let Ok(bytes) = screengrabcraft_render::encode(&pic, format, 92)
        {
            let _ = self.services.download(&name, &bytes);
        }
    }

    fn write_project(&mut self, ctx: &egui::Context, path: &str) {
        match project::save(&self.session.doc) {
            Ok(bytes) => match self.services.write_file(path, &bytes) {
                Ok(()) => {
                    self.project_path = Some(path.to_string());
                    self.session.dirty = false;
                    self.settings.remember(path);
                    self.services.save_settings(&self.settings);
                    self.toast(ctx, format!("Saved {}", file_name(path)));
                }
                Err(e) => self.error(ctx, format!("{path}: {e}")),
            },
            Err(e) => self.error(ctx, e.to_string()),
        }
    }

    fn write_export(&mut self, ctx: &egui::Context, path: &str, format: ImageFormat) {
        let format = std::path::Path::new(path).extension().and_then(|e| e.to_str()).and_then(ImageFormat::from_extension).unwrap_or(format);
        let Some(pic) = self.flattened() else { return };
        match screengrabcraft_render::encode(&pic, format, 92) {
            Ok(bytes) => match self.services.write_file(path, &bytes) {
                Ok(()) => {
                    self.settings.export_format = format;
                    self.services.save_settings(&self.settings);
                    self.toast(ctx, format!("Exported {}", file_name(path)));
                }
                Err(e) => self.error(ctx, format!("{path}: {e}")),
            },
            Err(e) => self.error(ctx, e.to_string()),
        }
    }

    fn poll_dialogs(&mut self, ctx: &egui::Context) {
        let Some(answer) = self.services.poll_dialog() else { return };
        match answer {
            DialogResult::Opened(files) => {
                if let Some(f) = files.into_iter().next() {
                    self.open_file(ctx, f);
                }
            }
            DialogResult::SaveTo(path) => {
                if self.saving_project && !path.to_ascii_lowercase().ends_with(&format!(".{PROJECT_EXT}")) {
                    // Saved under a picture's name: that is an export.
                    let f = std::path::Path::new(&path).extension().and_then(|e| e.to_str()).and_then(ImageFormat::from_extension);
                    match f {
                        Some(f) => self.write_export(ctx, &path, f),
                        None => self.write_project(ctx, &format!("{path}.{PROJECT_EXT}")),
                    }
                } else if self.saving_project {
                    self.write_project(ctx, &path);
                } else {
                    let f = self.saving.unwrap_or(ImageFormat::Png);
                    self.write_export(ctx, &path, f);
                }
                self.saving = None;
                self.saving_project = false;
                if matches!(self.pending, Some(Pending::Quit)) && !self.session.dirty {
                    self.close_confirmed = true;
                    self.quit_requested = true;
                    self.pending = None;
                }
            }
            DialogResult::Cancelled => {
                self.saving = None;
                self.saving_project = false;
            }
        }
    }

    /// Asked to close the window: true when it may close now.
    pub fn close_requested(&mut self) -> bool {
        if self.close_confirmed || !self.session.dirty {
            return true;
        }
        self.pending = Some(Pending::Quit);
        false
    }

    // ---- frame ----

    /// Work done every frame, shown or not: dialogs that answered, files dropped on the window.
    pub fn logic(&mut self, ctx: &egui::Context) {
        if !self.fonts_installed {
            crate::install_fonts(ctx);
            self.fonts_installed = true;
        }
        if self.theme_applied != Some(self.settings.theme) {
            crate::theme::apply(ctx, self.settings.theme);
            self.theme_applied = Some(self.settings.theme);
        }
        self.poll_dialogs(ctx);
        let dropped: Vec<egui::DroppedFileHandle> = ctx.input(|i| i.raw.dropped_files.clone());
        for f in dropped.into_iter().take(1) {
            let name = f.path().file_name().map(|n| n.to_string_lossy().to_string()).unwrap_or_else(|| "Dropped picture".to_string());
            #[cfg(not(target_arch = "wasm32"))]
            match f.bytes() {
                Ok(bytes) => {
                    let path = f.path().is_absolute().then(|| f.path().to_string_lossy().to_string());
                    self.open_file(ctx, OpenedFile { name, path, bytes });
                }
                Err(e) => self.error(ctx, format!("{name}: {e}")),
            }
            // A browser reads it later; the host hands it over as an opened file.
            #[cfg(target_arch = "wasm32")]
            {
                let _ = name;
                self.services.read_dropped(f);
            }
        }
        let now = ctx.input(|i| i.time);
        if now - self.recent_at > 3.0 {
            self.recent = self.services.recent();
            self.recent_at = now;
        }
        let title = format!("{}{} — {}", if self.session.dirty { "• " } else { "" }, self.name, screengrabcraft_model::EDITOR_NAME);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(title));
    }

    /// The window's contents.
    pub fn ui(&mut self, ui: &mut Ui) {
        let ctx = ui.ctx().clone();
        self.shortcuts(&ctx);
        self.menu_bar(ui);
        self.tool_bar(ui);
        self.status_bar(ui);
        if self.settings.show_tray {
            crate::strip::show(self, ui);
        }
        crate::props::panel(self, ui);
        crate::canvas::show(self, ui);
        crate::dialogs::show(self, &ctx);
        self.confirm(&ctx);
        self.paint_toast(&ctx);
    }

    fn shortcuts(&mut self, ctx: &egui::Context) {
        if self.dialog.is_some() || self.pending.is_some() {
            return;
        }
        let typing = ctx.egui_wants_keyboard_input() || self.editing.is_some();
        let cmd = |k: Key| KeyboardShortcut::new(Modifiers::COMMAND, k);
        let cmd_shift = |k: Key| KeyboardShortcut::new(Modifiers::COMMAND | Modifiers::SHIFT, k);
        let pressed = |s: KeyboardShortcut| ctx.input_mut(|i| i.consume_shortcut(&s));
        if pressed(cmd(Key::O)) {
            self.services.request_open();
        }
        if pressed(cmd_shift(Key::S)) {
            self.save_as();
        } else if pressed(cmd(Key::S)) {
            self.save(ctx);
        }
        if pressed(cmd(Key::E)) {
            self.export(self.settings.export_format);
        }
        if pressed(cmd_shift(Key::N)) {
            let _ = self.services.new_capture(CaptureMode::Region);
        }
        if typing {
            return;
        }
        if pressed(cmd_shift(Key::Z)) || pressed(cmd(Key::Y)) {
            self.run(ctx, "edit.redo", json!({}));
        } else if pressed(cmd(Key::Z)) {
            self.run(ctx, "edit.undo", json!({}));
        }
        if pressed(cmd(Key::A)) {
            self.tool = Tool::Select;
            self.run(ctx, "edit.selectAll", json!({}));
        }
        if pressed(cmd(Key::D)) && !self.session.selected().is_empty() {
            self.run(ctx, "edit.duplicate", json!({}));
        }
        if pressed(cmd_shift(Key::V)) {
            self.paste(ctx, true);
        } else if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Paste(_)))) || pressed(cmd(Key::V)) {
            self.paste(ctx, false);
        }
        if ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy))) || pressed(cmd(Key::C)) {
            self.copy(ctx);
        }
        if pressed(cmd(Key::Num0)) {
            self.view.fit_pending = true;
        }
        if pressed(cmd(Key::Num1)) {
            self.view.set_zoom(1.0, None);
        }
        if pressed(cmd(Key::Plus)) || pressed(cmd(Key::Equals)) {
            self.view.zoom_by(1.25, None);
        }
        if pressed(cmd(Key::Minus)) {
            self.view.zoom_by(0.8, None);
        }
        let plain = |k: Key| ctx.input_mut(|i| i.consume_key(Modifiers::NONE, k));
        if (plain(Key::Delete) || plain(Key::Backspace)) && !self.session.selected().is_empty() {
            self.run(ctx, "edit.delete", json!({}));
        }
        if plain(Key::Escape) {
            if self.crop.is_some() {
                self.crop = None;
            } else if self.drag.is_some() {
                self.session.cancel();
                self.drag = None;
            } else {
                self.run(ctx, "edit.selectNone", json!({}));
            }
        }
        if plain(Key::Enter) && self.crop.is_some() {
            self.apply_crop(ctx);
        }
        for (key, dx, dy) in [(Key::ArrowLeft, -1.0, 0.0), (Key::ArrowRight, 1.0, 0.0), (Key::ArrowUp, 0.0, -1.0), (Key::ArrowDown, 0.0, 1.0)] {
            let big = ctx.input_mut(|i| i.consume_key(Modifiers::SHIFT, key));
            let small = !big && plain(key);
            if (big || small) && !self.session.selected().is_empty() {
                let f = if big { 10.0 } else { 1.0 };
                self.run(ctx, "edit.nudge", json!({"dx": dx * f, "dy": dy * f}));
            }
        }
        for (tool, _, _, key) in Tool::BAR {
            if plain(key) {
                self.set_tool(tool);
            }
        }
    }

    pub fn set_tool(&mut self, tool: Tool) {
        if self.tool == Tool::Crop && tool != Tool::Crop {
            self.crop = None;
        }
        self.tool = tool;
        if tool == Tool::Crop {
            self.crop = Some(self.session.doc.bounds());
            self.session.select(Vec::new());
        }
    }

    pub fn apply_crop(&mut self, ctx: &egui::Context) {
        if let Some(r) = self.crop.take() {
            let r = r.finite();
            if r.w >= 1.0 && r.h >= 1.0 && r != self.session.doc.bounds() {
                self.run(ctx, "image.crop", json!({"x": r.x.round(), "y": r.y.round(), "w": r.w.round(), "h": r.h.round()}));
                self.view.fit_pending = true;
            }
        }
        self.tool = Tool::Select;
    }

    fn menu_bar(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        egui::Panel::top("menu_bar").frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(8, 3))).show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    if ui.add(egui::Button::new("New Capture").shortcut_text("Ctrl+Shift+N")).clicked() {
                        if let Err(e) = self.services.new_capture(CaptureMode::Region) {
                            self.error(&ctx, e);
                        }
                        ui.close();
                    }
                    if ui.button("New from Clipboard").clicked() {
                        self.paste(&ctx, true);
                        ui.close();
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Open…").shortcut_text("Ctrl+O")).clicked() {
                        self.services.request_open();
                        ui.close();
                    }
                    let recent = self.settings.recent.clone();
                    ui.add_enabled_ui(!recent.is_empty() && self.services.has_paths(), |ui| {
                        ui.menu_button("Open Recent", |ui| {
                            for p in recent.iter().take(12) {
                                if ui.button(file_name(p)).on_hover_text(p).clicked() {
                                    self.open_path(&ctx, p);
                                    ui.close();
                                }
                            }
                        });
                    });
                    ui.separator();
                    if ui.add(egui::Button::new("Save").shortcut_text("Ctrl+S")).clicked() {
                        self.save(&ctx);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Save As…").shortcut_text("Ctrl+Shift+S")).clicked() {
                        self.save_as();
                        ui.close();
                    }
                    ui.menu_button("Export As", |ui| {
                        for f in ImageFormat::ALL {
                            if ui.button(f.label()).clicked() {
                                self.export(f);
                                ui.close();
                            }
                        }
                    });
                    if ui.add(egui::Button::new("Copy Picture").shortcut_text("Ctrl+C")).clicked() {
                        self.copy(&ctx);
                        ui.close();
                    }
                    if self.services.has_paths() {
                        ui.separator();
                        if ui.button("Show Captures Folder").clicked() {
                            self.services.open_folder(None);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Close Window").clicked() {
                        self.quit_requested = true;
                        ui.close();
                    }
                });
                ui.menu_button("Edit", |ui| {
                    let undo = format!("Undo {}", self.session.undo_label().unwrap_or(""));
                    if ui.add_enabled(self.session.can_undo(), egui::Button::new(undo.trim()).shortcut_text("Ctrl+Z")).clicked() {
                        self.run(&ctx, "edit.undo", json!({}));
                        ui.close();
                    }
                    let redo = format!("Redo {}", self.session.redo_label().unwrap_or(""));
                    if ui.add_enabled(self.session.can_redo(), egui::Button::new(redo.trim()).shortcut_text("Ctrl+Y")).clicked() {
                        self.run(&ctx, "edit.redo", json!({}));
                        ui.close();
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Paste").shortcut_text("Ctrl+V")).clicked() {
                        self.paste(&ctx, false);
                        ui.close();
                    }
                    let any = !self.session.selected().is_empty();
                    if ui.add_enabled(any, egui::Button::new("Duplicate").shortcut_text("Ctrl+D")).clicked() {
                        self.run(&ctx, "edit.duplicate", json!({}));
                        ui.close();
                    }
                    if ui.add_enabled(any, egui::Button::new("Delete").shortcut_text("Del")).clicked() {
                        self.run(&ctx, "edit.delete", json!({}));
                        ui.close();
                    }
                    ui.separator();
                    if ui.add(egui::Button::new("Select All").shortcut_text("Ctrl+A")).clicked() {
                        self.tool = Tool::Select;
                        self.run(&ctx, "edit.selectAll", json!({}));
                        ui.close();
                    }
                    ui.separator();
                    for (label, to) in
                        [("Bring to Front", "front"), ("Bring Forward", "forward"), ("Send Backward", "backward"), ("Send to Back", "back")]
                    {
                        if ui.add_enabled(any, egui::Button::new(label)).clicked() {
                            self.run(&ctx, "object.order", json!({"to": to}));
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui
                        .add_enabled(!self.session.doc.objects.is_empty(), egui::Button::new("Flatten Annotations"))
                        .on_hover_text("Draw every annotation into the picture for good")
                        .clicked()
                    {
                        self.run(&ctx, "object.flatten", json!({}));
                        ui.close();
                    }
                });
                ui.menu_button("Image", |ui| {
                    if ui.add(egui::Button::new("Crop").shortcut_text("K")).clicked() {
                        self.set_tool(Tool::Crop);
                        ui.close();
                    }
                    if ui.button("Trim Borders").clicked() {
                        self.run(&ctx, "image.trim", json!({}));
                        self.view.fit_pending = true;
                        ui.close();
                    }
                    if ui.button("Resize Image…").clicked() {
                        self.dialog = Some(Dialog::resize(&self.session.doc));
                        ui.close();
                    }
                    if ui.button("Canvas Size…").clicked() {
                        self.dialog = Some(Dialog::canvas());
                        ui.close();
                    }
                    ui.separator();
                    for (label, deg) in [("Rotate Right", 90), ("Rotate Left", 270), ("Rotate 180°", 180)] {
                        if ui.button(label).clicked() {
                            self.run(&ctx, "image.rotate", json!({"degrees": deg}));
                            self.view.fit_pending = true;
                            ui.close();
                        }
                    }
                    if ui.button("Flip Horizontal").clicked() {
                        self.run(&ctx, "image.flip", json!({"horizontal": true}));
                        ui.close();
                    }
                    if ui.button("Flip Vertical").clicked() {
                        self.run(&ctx, "image.flip", json!({"horizontal": false}));
                        ui.close();
                    }
                });
                ui.menu_button("Effects", |ui| {
                    for (label, d) in [
                        ("Border…", Dialog::border()),
                        ("Drop Shadow…", Dialog::shadow()),
                        ("Torn Edge…", Dialog::torn()),
                        ("Round Corners…", Dialog::round()),
                        ("Brightness and Contrast…", Dialog::brightness()),
                    ] {
                        if ui.button(label).clicked() {
                            self.dialog = Some(d);
                            ui.close();
                        }
                    }
                    ui.separator();
                    if ui.button("Grayscale").clicked() {
                        self.run(&ctx, "effect.grayscale", json!({}));
                        ui.close();
                    }
                    if ui.button("Invert Colors").clicked() {
                        self.run(&ctx, "effect.invert", json!({}));
                        ui.close();
                    }
                });
                ui.menu_button("View", |ui| {
                    if ui.add(egui::Button::new("Zoom to Fit").shortcut_text("Ctrl+0")).clicked() {
                        self.view.fit_pending = true;
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Actual Size").shortcut_text("Ctrl+1")).clicked() {
                        self.view.set_zoom(1.0, None);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Zoom In").shortcut_text("Ctrl++")).clicked() {
                        self.view.zoom_by(1.25, None);
                        ui.close();
                    }
                    if ui.add(egui::Button::new("Zoom Out").shortcut_text("Ctrl+-")).clicked() {
                        self.view.zoom_by(0.8, None);
                        ui.close();
                    }
                    ui.separator();
                    if ui.checkbox(&mut self.settings.show_tray, "Recent Captures").changed() {
                        self.services.save_settings(&self.settings);
                    }
                    ui.menu_button("Theme", |ui| {
                        for (label, th) in [
                            ("Follow the system", screengrabcraft_model::Theme::System),
                            ("Light", screengrabcraft_model::Theme::Light),
                            ("Dark", screengrabcraft_model::Theme::Dark),
                        ] {
                            if ui.radio(self.settings.theme == th, label).clicked() {
                                self.settings.theme = th;
                                self.services.save_settings(&self.settings);
                                ui.close();
                            }
                        }
                    });
                });
                ui.menu_button("Help", |ui| {
                    if ui.button("Keyboard Shortcuts").clicked() {
                        self.dialog = Some(Dialog::Shortcuts);
                        ui.close();
                    }
                    if ui.button(format!("About {}", screengrabcraft_model::EDITOR_NAME)).clicked() {
                        self.dialog = Some(Dialog::About);
                        ui.close();
                    }
                });
            });
        });
    }

    fn tool_bar(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        egui::Panel::top("tool_bar")
            .frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(8, 4)))
            .show_separator_line(true)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = vec2(2.0, 0.0);
                    if icons::button(ui, Icon::Capture, "New capture (Ctrl+Shift+N)", false, true).clicked()
                        && let Err(e) = self.services.new_capture(CaptureMode::Region)
                    {
                        self.error(&ctx, e);
                    }
                    if icons::button(ui, Icon::Open, "Open (Ctrl+O)", false, true).clicked() {
                        self.services.request_open();
                    }
                    if icons::button(ui, Icon::Save, "Save (Ctrl+S)", false, true).clicked() {
                        self.save(&ctx);
                    }
                    if icons::button(ui, Icon::Copy, "Copy the picture (Ctrl+C)", false, true).clicked() {
                        self.copy(&ctx);
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                    for (tool, icon, label, key) in Tool::BAR {
                        let tip = format!("{label} ({})", key.name());
                        if icons::button(ui, icon, &tip, self.tool == tool, true).clicked() {
                            self.set_tool(tool);
                        }
                    }
                    ui.add_space(6.0);
                    ui.separator();
                    ui.add_space(6.0);
                    let undo_tip = format!("Undo {} (Ctrl+Z)", self.session.undo_label().unwrap_or(""));
                    if icons::button(ui, Icon::Undo, &undo_tip, false, self.session.can_undo()).clicked() {
                        self.run(&ctx, "edit.undo", json!({}));
                    }
                    let redo_tip = format!("Redo {} (Ctrl+Y)", self.session.redo_label().unwrap_or(""));
                    if icons::button(ui, Icon::Redo, &redo_tip, false, self.session.can_redo()).clicked() {
                        self.run(&ctx, "edit.redo", json!({}));
                    }
                    let any = !self.session.selected().is_empty();
                    if icons::button(ui, Icon::Delete, "Delete the selection (Del)", false, any).clicked() {
                        self.run(&ctx, "edit.delete", json!({}));
                    }
                    if self.tool == Tool::Crop {
                        ui.add_space(10.0);
                        if ui.button("Apply Crop").clicked() {
                            self.apply_crop(&ctx);
                        }
                        if ui.button("Cancel").clicked() {
                            self.crop = None;
                            self.tool = Tool::Select;
                        }
                    }
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if icons::button(ui, Icon::Fit, "Zoom to fit (Ctrl+0)", false, true).clicked() {
                            self.view.fit_pending = true;
                        }
                        if icons::button(ui, Icon::ZoomIn, "Zoom in (Ctrl++)", false, true).clicked() {
                            self.view.zoom_by(1.25, None);
                        }
                        let pct = format!("{:.0}%", self.view.zoom * 100.0);
                        if ui.add(egui::Button::new(pct).frame(false).min_size(vec2(48.0, 26.0))).on_hover_text("Actual size (Ctrl+1)").clicked() {
                            self.view.set_zoom(1.0, None);
                        }
                        if icons::button(ui, Icon::ZoomOut, "Zoom out (Ctrl+-)", false, true).clicked() {
                            self.view.zoom_by(0.8, None);
                        }
                    });
                });
            });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        let t = Tokens::get(ui.ctx());
        egui::Panel::bottom("status_bar")
            .exact_size(24.0)
            .frame(egui::Frame::NONE.fill(t.toolbar).inner_margin(egui::Margin::symmetric(10, 0)))
            .show(ui, |ui| {
                ui.horizontal_centered(|ui| {
                    ui.spacing_mut().item_spacing = vec2(16.0, 0.0);
                    let small = |s: String| egui::RichText::new(s).size(12.0).color(t.muted);
                    ui.label(small(format!("{} × {} px", self.session.doc.width(), self.session.doc.height())));
                    ui.label(small(format!("{:.0}%", self.view.zoom * 100.0)));
                    if let Some((x, y)) = self.pointer_px {
                        let c = u32::try_from(x).ok().zip(u32::try_from(y).ok()).and_then(|(x, y)| self.session.doc.base.pixel(x, y));
                        let colour = c.map(|c| format!("  {}", c.to_hex())).unwrap_or_default();
                        ui.label(small(format!("{x}, {y}{colour}")));
                    }
                    let n = self.session.doc.objects.len();
                    if n > 0 {
                        ui.label(small(format!("{n} annotation{}", if n == 1 { "" } else { "s" })));
                    }
                    if let Some(c) = self.crop {
                        ui.label(small(format!("Crop to {:.0} × {:.0} — Enter to apply, Esc to cancel", c.w, c.h)));
                    }
                });
            });
    }

    fn confirm(&mut self, ctx: &egui::Context) {
        let Some(pending) = self.pending.clone() else { return };
        let what = match &pending {
            Pending::Quit => "before closing",
            _ => "before opening another picture",
        };
        let mut choice: Option<u8> = None;
        egui::Modal::new(egui::Id::new("unsaved")).show(ctx, |ui| {
            ui.set_width(380.0);
            ui.heading("Save your changes?");
            ui.add_space(4.0);
            ui.label(format!("“{}” has changes that aren't saved. Save them {what}?", self.name));
            ui.add_space(10.0);
            ui.horizontal(|ui| {
                if ui.button("Save").clicked() {
                    choice = Some(0);
                }
                if ui.button("Don't Save").clicked() {
                    choice = Some(1);
                }
                if ui.button("Cancel").clicked() {
                    choice = Some(2);
                }
            });
        });
        let quitting = matches!(pending, Pending::Quit);
        match choice {
            Some(0) => {
                self.save(ctx);
                if !self.session.dirty {
                    self.pending = None;
                    self.go_on(ctx, pending);
                } else if !quitting {
                    // A save dialog is open; a quit goes on once it saved (`poll_dialogs`).
                    self.pending = None;
                }
            }
            Some(1) => {
                self.pending = None;
                self.session.dirty = false;
                self.go_on(ctx, pending);
            }
            Some(2) => self.pending = None,
            _ => {}
        }
    }

    fn go_on(&mut self, ctx: &egui::Context, pending: Pending) {
        match pending {
            Pending::Open(f) => self.open_now(ctx, f),
            Pending::Picture(r, n) => {
                self.session.dirty = false;
                self.open_picture(r, n);
            }
            Pending::Quit => {
                self.close_confirmed = true;
                self.quit_requested = true;
            }
        }
    }

    fn paint_toast(&mut self, ctx: &egui::Context) {
        let Some((text, at, bad)) = self.toast.clone() else { return };
        let now = ctx.input(|i| i.time);
        if now - at > TOAST_SECS {
            self.toast = None;
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(250));
        let t = Tokens::get(ctx);
        egui::Area::new(egui::Id::new("toast")).anchor(egui::Align2::CENTER_BOTTOM, vec2(0.0, -48.0)).interactable(false).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).fill(if bad { t.danger } else { t.panel }).show(ui, |ui| {
                ui.label(egui::RichText::new(text).color(if bad { Color32::WHITE } else { t.text }));
            });
        });
    }

    /// The style the current tool draws with.
    pub fn tool_style(&self, kind: Kind) -> Style {
        let mut s = self.settings.style(kind);
        if kind == Kind::Stamp && !self.settings.styles.contains_key(&Kind::Stamp) {
            s.fill = screengrabcraft_render::stamp::default_fill(self.stamp);
        }
        s
    }
}

/// `capture.png` → `capture.sgcraft`, beside it.
pub fn sibling_project(picture: &str) -> String {
    let p = std::path::Path::new(picture);
    p.with_extension(PROJECT_EXT).to_string_lossy().to_string()
}

pub fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_name().and_then(|s| s.to_str()).unwrap_or(path).to_string()
}
