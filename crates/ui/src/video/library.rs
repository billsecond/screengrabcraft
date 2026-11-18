//! The video editor's library, at the left: the Library tab's ready-made titles, lower thirds,
//! callouts and overlays, and transitions; the Media tab's files brought in (pictures, sound,
//! other recordings). Each is dragged onto the timeline (onto a layer, or the band above them for
//! a layer of its own) or added at the playhead with its +.
//!
//! Also here: the item chosen on the timeline, changed in the panel at the right (its words,
//! colour, size, place, how it comes and goes) and on the picture itself (dragged to move it, by
//! a corner to resize it), and the camera moved and resized on the picture the same way.

use egui::{Color32, Pos2, Rect, RichText, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use screengrabcraft_media::layer_draw::{self, MediaStore};
use screengrabcraft_media::layers::{self, AnimKind, Group, Item, MediaKind, What};
use screengrabcraft_media::media_store::{self, MediaJob, MediaState};
use screengrabcraft_media::recording::StreamKind;
use screengrabcraft_model::Raster;
use serde_json::json;

use super::VideoView;
use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::theme::Tokens;
use crate::widgets;

/// The library panel's width.
pub const LIBRARY_W: f32 = 300.0;
/// A card's thumbnail.
const THUMB: Vec2 = vec2(126.0, 71.0);
/// How near (points) the picture's middle and edges catch what's dragged on it.
const CATCH: f32 = 6.0;
/// How near a corner the pointer must be to resize what's chosen on the picture.
const HANDLE: f32 = 10.0;

/// What's dragged from the library onto the timeline.
#[derive(Clone, Debug, PartialEq)]
pub enum LibDrop {
    Template(&'static str),
    Media(u32),
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LibTab {
    #[default]
    Library,
    Media,
}

/// A file being brought in: its name, where it is, and the reading of it.
pub struct Importing {
    pub name: String,
    pub path: String,
    pub job: MediaJob,
}

/// A drag on the picture: what's moved or resized, where it was taken hold of.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CanvasDrag {
    Item { layer: u32, item: u32, grab: Vec2, resize: Option<(f32, f32)> },
    Camera { grab: Vec2, resize: Option<(f32, f32)> },
}

/// A card's picture is drawn on a video this large (16:9), then made a third as large: the card
/// shows the item as it will be on the video, where it will be and as large.
const THUMB_DRAWN: (u32, u32) = (768, 432);
/// How many cards' pictures are drawn in one frame (the rest in the next ones).
const THUMBS_PER_FRAME: usize = 4;
/// The preview shown while the pointer is on a card: as large as this (points), playing.
const PREVIEW: Vec2 = vec2(384.0, 216.0);

/// The made-up screen under the library's pictures at a point (shares of its width and height): a
/// program's window on a desktop, as a screen recording looks, kept faint so that what's put on
/// it is what the eye finds. Drawn by this code.
fn scene(x: f32, y: f32) -> [u8; 3] {
    let inside = |x0: f32, y0: f32, x1: f32, y1: f32| x >= x0 && x < x1 && y >= y0 && y < y1;
    let (left, top, right, bottom) = (0.07, 0.08, 0.93, 0.86);
    if !inside(left, top, right, bottom) {
        // The desktop, and the window's soft shadow on it.
        let k = (x + y) / 2.0;
        let shade = if inside(left + 0.006, top + 0.012, right + 0.006, bottom + 0.016) { 0.85 } else { 1.0 };
        return [((88.0 + 30.0 * k) * shade) as u8, ((108.0 + 36.0 * k) * shade) as u8, ((140.0 + 30.0 * k) * shade) as u8];
    }
    let bar = top + 0.055;
    if y < bar {
        // Its title bar, with a name in it.
        return if inside(0.45, top + 0.02, 0.55, top + 0.035) { [204, 208, 216] } else { [226, 229, 235] };
    }
    if x < 0.25 {
        // A side bar of places, the second one chosen.
        for (i, wide) in [0.10f32, 0.08, 0.11, 0.07, 0.09, 0.10].iter().enumerate() {
            let ly = bar + 0.06 + i as f32 * 0.075;
            if inside(0.095, ly, 0.095 + wide, ly + 0.022) {
                return if i == 1 { [246, 248, 255] } else { [210, 214, 222] };
            }
            if i == 1 && inside(0.085, ly - 0.018, 0.235, ly + 0.04) {
                return [178, 192, 236];
            }
        }
        return [231, 234, 239];
    }
    // A heading, three numbers on cards, a chart and lines of words.
    if inside(0.285, bar + 0.04, 0.50, bar + 0.075) {
        return [198, 203, 212];
    }
    for i in 0..3 {
        let cx = 0.285 + i as f32 * 0.205;
        if inside(cx, bar + 0.11, cx + 0.185, bar + 0.27) {
            if inside(cx + 0.02, bar + 0.135, cx + 0.09, bar + 0.152) {
                return [218, 222, 229];
            }
            if inside(cx + 0.02, bar + 0.18, cx + 0.12 + 0.015 * i as f32, bar + 0.235) {
                return [196, 201, 210];
            }
            return [249, 250, 252];
        }
    }
    if inside(0.285, bar + 0.31, 0.70, bottom - 0.04) {
        let floor = bottom - 0.08;
        for (i, tall) in [0.12f32, 0.2, 0.15, 0.26, 0.19, 0.31, 0.24, 0.34].iter().enumerate() {
            let bx = 0.31 + i as f32 * 0.048;
            if inside(bx, floor - tall, bx + 0.03, floor) {
                return if i == 7 { [184, 198, 240] } else { [212, 220, 242] };
            }
        }
        return [249, 250, 252];
    }
    if inside(0.72, bar + 0.31, 0.905, bottom - 0.04) {
        for i in 0..6 {
            let ly = bar + 0.35 + i as f32 * 0.06;
            if inside(0.74, ly, 0.88 - 0.02 * (i % 3) as f32, ly + 0.018) {
                return [220, 224, 230];
            }
        }
        return [249, 250, 252];
    }
    [240, 242, 246]
}

/// The made-up screen as a picture `w × h`, toned down to the middle: white words and dark plates
/// both read on it.
fn backdrop(w: u32, h: u32) -> Option<Raster> {
    let (w, h) = (w.clamp(1, 4096), h.clamp(1, 4096));
    let mut px = Vec::with_capacity(w as usize * h as usize * 4);
    let tone = |c: u8, lift: f32| (f32::from(c) * 0.58 + lift) as u8;
    for y in 0..h {
        for x in 0..w {
            let [r, g, b] = scene(x as f32 / w as f32, y as f32 / h as f32);
            px.extend_from_slice(&[tone(r, 22.0), tone(g, 30.0), tone(b, 46.0), 255]);
        }
    }
    Raster::new(w, h, px).ok()
}

/// A template as it is on a `w × h` video `at` µs after it starts: placed and sized as it will
/// be, over the made-up screen.
fn template_frame(t: &layers::Template, w: u32, h: u32, at: u64, texts: &mut layer_draw::TextCache) -> Option<Raster> {
    let mut pic = backdrop(w, h)?;
    let mut tl = screengrabcraft_media::timeline::Timeline {
        tracks: Vec::new(),
        camera: Default::default(),
        captions: Default::default(),
        layers: Vec::new(),
        media: Vec::new(),
        pointer: Default::default(),
    };
    tl.place(t.item.clone(), None, 0).ok()?;
    layer_draw::draw_layers(&mut pic, &tl, &layer_draw::NoMedia, at.min(t.item.len.saturating_sub(1)), texts, &mut Default::default());
    Some(pic)
}

/// When a template is shown on its card: a transition part-way across (as it covers the
/// picture; one with words at its middle, where they are), anything else once it has come in.
fn held_at(t: &layers::Template) -> u64 {
    match &t.item.what {
        What::Text { style, .. } if screengrabcraft_media::stock::is_stinger(*style) && style.has_words() => t.item.len / 2,
        What::Text { style, .. } if screengrabcraft_media::stock::is_stinger(*style) => t.item.len / 100 * 32,
        What::Transition { .. } => t.item.len / 5 * 2,
        _ => t.item.len / 2,
    }
}

/// A template drawn as its card shows it: the whole video, a third the size it's drawn at.
fn template_thumbnail(t: &layers::Template) -> Option<Raster> {
    let (w, h) = THUMB_DRAWN;
    let pic = template_frame(t, w, h, held_at(t), &mut Default::default())?;
    screengrabcraft_media::compose::scale(&pic, w / 3, h / 3)
}

fn texture(ctx: &egui::Context, name: &str, pic: &Raster) -> egui::TextureHandle {
    let img = egui::ColorImage::from_rgba_unmultiplied([pic.width() as usize, pic.height() as usize], pic.pixels());
    ctx.load_texture(name, img, egui::TextureOptions::LINEAR)
}

fn mins(us: u64) -> String {
    let s = us / 1_000_000;
    format!("{}:{:02}", s / 60, s % 60)
}

impl VideoView {
    // ---- bringing files in ----

    /// Files to bring into the media bin: each read on a thread of the host's.
    pub fn import_paths(&mut self, ed: &mut Editor, ctx: &egui::Context, paths: &[String]) {
        for path in paths.iter().take(50) {
            let name = crate::editor::file_name(path);
            if media_store::kind_of(path).is_none() {
                ed.error(ctx, format!("{name}: not a picture, a sound file or a video this program reads"));
                continue;
            }
            if self.importing.iter().any(|i| &i.path == path) || self.session.timeline.media.iter().any(|m| &m.path == path) {
                continue;
            }
            match ed.services.load_media(path) {
                Ok(job) => self.importing.push(Importing { name, path: path.clone(), job }),
                Err(e) => ed.error(ctx, format!("{name}: {e}")),
            }
        }
        self.lib_tab = LibTab::Media;
        self.library_open = true;
    }

    /// Files read since the last frame: into the bin (one undo step each), and the media of the
    /// recording's layers read again when it's opened.
    pub(super) fn poll_media(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let mut done = Vec::new();
        self.importing.retain(|i| {
            let r = i.job.lock().ok().and_then(|mut d| d.take());
            match r {
                Some(r) => {
                    done.push((i.name.clone(), i.path.clone(), r));
                    false
                }
                None => true,
            }
        });
        for (name, path, r) in done {
            match r {
                Ok(loaded) => {
                    let kind = serde_json::to_value(loaded.kind).unwrap_or(json!("picture"));
                    let p = json!({"kind": kind, "name": name, "path": path, "duration": loaded.duration, "width": loaded.width, "height": loaded.height});
                    match self.session.execute("video.addMedia", &p) {
                        Ok(v) => {
                            let id = v.get("id").and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
                            self.library.insert(id, loaded.content);
                            self.shown = None;
                            ed.toast(ctx, format!("{name} is in the media bin"));
                            // What's said in it, transcribed by itself when Settings says so.
                            let hears = self.library.has_sound(id) == Some(true);
                            self.media_added(ed, ctx, id, &path, hears);
                        }
                        Err(e) => ed.error(ctx, format!("{name}: {e}")),
                    }
                }
                Err(e) => ed.error(ctx, format!("{name}: {e}")),
            }
        }
        // The bin's files not read yet (a recording opened again): read where they are.
        let unread: Vec<(u32, String)> =
            self.session.timeline.media.iter().filter(|m| !self.library.has(m.id)).map(|m| (m.id, m.path.clone())).collect();
        for (id, path) in unread {
            self.library.loading(id);
            match ed.services.load_media(&path) {
                Ok(job) => self.rereading.push((id, job)),
                Err(e) => self.library.failed(id, e),
            }
        }
        let mut arrived = false;
        self.rereading.retain(|(id, job)| {
            let r = job.lock().ok().and_then(|mut d| d.take());
            match r {
                Some(Ok(loaded)) => {
                    self.library.insert(*id, loaded.content);
                    arrived = true;
                    false
                }
                Some(Err(e)) => {
                    self.library.failed(*id, e);
                    arrived = true;
                    false
                }
                None => true,
            }
        });
        if arrived {
            self.shown = None;
        }
        if !self.importing.is_empty() || !self.rereading.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    /// Puts a template or a file of the bin on the timeline: at `at` on `layer` (or a new one),
    /// the nearest place it fits; it's then chosen.
    pub fn drop_on_timeline(&mut self, ed: &mut Editor, ctx: &egui::Context, what: &LibDrop, layer: Option<u32>, at: u64) {
        let mut p = match what {
            LibDrop::Template(key) => json!({"template": key}),
            LibDrop::Media(id) => json!({"media": id}),
        };
        if let Some(o) = p.as_object_mut() {
            o.insert("at".into(), json!(at));
            o.insert("layer".into(), layer.map_or(json!("new"), |l| json!(l)));
        }
        match self.session.execute("video.place", &p) {
            Ok(v) => {
                let get = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).unwrap_or(0) as u32;
                self.item = Some((get("layer"), get("item")));
                self.camera_chosen = false;
                self.selected = None;
            }
            Err(e) => ed.error(ctx, e.to_string()),
        }
    }

    /// Puts plain words on the video at the playhead, on a layer of their own: they're chosen,
    /// and their box in the panel at the right takes the keys.
    pub fn add_text(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let before = self.item;
        let at = self.playhead;
        self.drop_on_timeline(ed, ctx, &LibDrop::Template("title"), None, at);
        self.type_words = self.item != before;
    }

    // ---- the panel at the left ----

    pub(super) fn library_panel(&mut self, ed: &mut Editor, ui: &mut Ui) {
        if !self.library_open {
            return;
        }
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        egui::Panel::left("video_library")
            .exact_size(LIBRARY_W)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(12, 10)))
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    for (tab, label) in [(LibTab::Library, "Library"), (LibTab::Media, "Media")] {
                        if ui
                            .add(egui::Button::new(RichText::new(label).size(14.0)).selected(self.lib_tab == tab).min_size(vec2(82.0, 28.0)))
                            .clicked()
                        {
                            self.lib_tab = tab;
                        }
                    }
                });
                ui.add_space(8.0);
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| match self.lib_tab {
                    LibTab::Library => self.library_tab(ed, ui, &ctx),
                    LibTab::Media => self.media_tab(ed, ui, &ctx),
                });
            });
    }

    fn library_tab(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        ui.label(RichText::new("Drag one onto the timeline, or + to add it at the playhead.").size(12.0).color(t.muted));
        let lib = layers::library();
        ui.add_space(6.0);
        // Looked for by name: everything that matches, whatever its section.
        ui.add(egui::TextEdit::singleline(&mut self.lib_search).hint_text("Search the library").desired_width(f32::INFINITY));
        ui.add_space(6.0);
        let wanted = self.lib_search.trim().to_lowercase();
        if !wanted.is_empty() {
            let found: Vec<&layers::Template> =
                lib.iter().filter(|x| x.name.to_lowercase().contains(&wanted) || x.group.label().to_lowercase().contains(&wanted)).collect();
            if found.is_empty() {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Nothing in the library is called \u{201c}{}\u{201d}.", self.lib_search.trim())).color(t.muted));
            }
            let mut budget = THUMBS_PER_FRAME;
            let mut hovered = false;
            for row in found.chunks(2) {
                ui.horizontal(|ui| {
                    for tpl in row {
                        self.template_card(ed, ui, ctx, tpl, tpl.group.label(), &mut budget, &mut hovered);
                    }
                });
            }
            if !hovered {
                self.lib_hover = None;
            }
            return;
        }
        // A section for each kind, opened and closed by its heading (the first is open).
        let mut budget = THUMBS_PER_FRAME;
        let mut hovered = false;
        for g in Group::ALL {
            let cards: Vec<&layers::Template> = lib.iter().filter(|x| x.group == g).collect();
            egui::CollapsingHeader::new(RichText::new(g.label()).size(14.0).strong().color(t.text))
                .id_salt(("library-section", g.label()))
                .default_open(g == Group::LowerThirds)
                // (Not set in from its heading: the cards keep the whole width of the panel.)
                .show_unindented(ui, |ui| {
                    ui.add_space(4.0);
                    for row in cards.chunks(2) {
                        ui.horizontal(|ui| {
                            for tpl in row {
                                self.template_card(ed, ui, ctx, tpl, "", &mut budget, &mut hovered);
                            }
                        });
                    }
                    ui.add_space(6.0);
                });
            ui.add_space(2.0);
        }
        if !hovered {
            self.lib_hover = None;
        }
    }

    /// A template's card: its picture drawn the first time it's shown (a few a frame), and while
    /// the pointer is on it, a larger preview playing it.
    #[allow(clippy::too_many_arguments)]
    fn template_card(
        &mut self,
        ed: &mut Editor,
        ui: &mut Ui,
        ctx: &egui::Context,
        tpl: &layers::Template,
        detail: &str,
        budget: &mut usize,
        hovered: &mut bool,
    ) {
        let key = format!("template-{}", tpl.key);
        if !self.thumbs.contains_key(&key) {
            if *budget > 0 {
                *budget -= 1;
                if let Some(pic) = template_thumbnail(tpl) {
                    self.thumbs.insert(key.clone(), texture(ctx, &key, &pic));
                }
            } else {
                ctx.request_repaint();
            }
        }
        let tex = self.thumbs.get(&key).cloned();
        let (added, resp) = self.card(ui, LibDrop::Template(tpl.key), tex.as_ref(), None, tpl.name, detail, None);
        if added {
            let at = self.playhead;
            self.drop_on_timeline(ed, ctx, &LibDrop::Template(tpl.key), None, at);
        }
        if resp.hovered() && !resp.dragged() && egui::DragAndDrop::payload::<LibDrop>(ctx).is_none() {
            *hovered = true;
            if let Some(tex) = self.preview_frame(ctx, tpl) {
                resp.on_hover_ui(|ui| {
                    ui.image((tex.id(), PREVIEW));
                    ui.label(RichText::new(format!("{}: {:.1} s", tpl.name, tpl.item.len as f64 / 1e6)).size(12.0));
                });
            }
        }
    }

    /// The preview of a template with the pointer on its card: the moment it's at, played over
    /// and over (with a short rest at its end), drawn as on a video.
    fn preview_frame(&mut self, ctx: &egui::Context, tpl: &layers::Template) -> Option<egui::TextureHandle> {
        let now = ctx.input(|i| i.time);
        let since = match self.lib_hover {
            Some((k, s)) if k == tpl.key => s,
            _ => {
                self.lib_hover = Some((tpl.key, now));
                now
            }
        };
        let len = tpl.item.len.max(1);
        let round = len as f64 / 1e6 + 0.8;
        let at = ((((now - since).max(0.0)) % round) * 1e6) as u64;
        let ppp = ctx.pixels_per_point().clamp(1.0, 2.0);
        let (w, h) = ((PREVIEW.x * ppp).round() as u32, (PREVIEW.y * ppp).round() as u32);
        let pic = template_frame(tpl, w, h, at, &mut self.preview_texts)?;
        let img = egui::ColorImage::from_rgba_unmultiplied([pic.width() as usize, pic.height() as usize], pic.pixels());
        match &mut self.preview_tex {
            Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
            None => self.preview_tex = Some(ctx.load_texture("library-preview", img, egui::TextureOptions::LINEAR)),
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(40));
        self.preview_tex.clone()
    }

    fn media_tab(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        if widgets::primary_button(ui, "Import Media…").on_hover_text("Pictures, sound files (MP3, WAV, M4A, FLAC, Ogg) and recordings").clicked() {
            ed.services.request_import();
        }
        ui.add_space(4.0);
        ui.label(RichText::new("Or drag files here from your folders. Then drag them onto the timeline.").size(12.0).color(t.muted));
        ui.add_space(8.0);
        for i in &self.importing {
            ui.horizontal(|ui| {
                ui.add(egui::Spinner::new().size(14.0));
                ui.label(RichText::new(format!("Reading {}…", i.name)).size(12.5).color(t.muted));
            });
        }
        let media = self.session.timeline.media.clone();
        if media.is_empty() && self.importing.is_empty() {
            ui.add_space(20.0);
            ui.label(RichText::new("No media yet.").color(t.muted));
        }
        let mut remove = None;
        for row in media.chunks(2) {
            ui.horizontal(|ui| {
                for m in row {
                    let key = format!("media-{}", m.id);
                    let state = self.library.state(m.id);
                    if !self.thumbs.contains_key(&key)
                        && state == Some(MediaState::Ready)
                        && m.kind != MediaKind::Sound
                        && let Some(pic) = self.library.picture(m.id, 0)
                        && let Some(small) = screengrabcraft_media::compose::scale(
                            &pic,
                            192,
                            (192.0 * pic.height() as f32 / pic.width().max(1) as f32).max(1.0) as u32,
                        )
                    {
                        self.thumbs.insert(key.clone(), texture(ctx, &key, &small));
                    }
                    let tex = self.thumbs.get(&key).cloned();
                    let icon = match m.kind {
                        MediaKind::Sound => Some(Icon::Speaker),
                        _ if tex.is_none() => Some(if m.kind == MediaKind::Video { Icon::Play } else { Icon::Capture }),
                        _ => None,
                    };
                    let detail = match (m.kind, &state) {
                        (_, Some(MediaState::Failed(_))) => "Can't be read".to_string(),
                        (_, Some(MediaState::Loading)) => "Reading…".to_string(),
                        (MediaKind::Picture, _) => format!("{} × {}", m.width, m.height),
                        _ => mins(m.duration),
                    };
                    let (added, _) = self.card(ui, LibDrop::Media(m.id), tex.as_ref(), icon, &m.name, &detail, Some(&mut remove));
                    if added {
                        let at = self.playhead;
                        self.drop_on_timeline(ed, ctx, &LibDrop::Media(m.id), None, at);
                    }
                    if remove == Some(m.id) {
                        remove = None;
                        self.run(ed, ctx, "video.removeMedia", json!({"id": m.id}));
                        self.thumbs.remove(&key);
                    }
                }
            });
        }
    }

    /// One card: its picture (or icon), its name and a line under it; dragged onto the timeline,
    /// or + adds it at the playhead (true, with the card's response). A file of the bin can be taken out of it (its id goes
    /// in `remove`).
    #[allow(clippy::too_many_arguments)]
    fn card(
        &mut self,
        ui: &mut Ui,
        what: LibDrop,
        tex: Option<&egui::TextureHandle>,
        icon: Option<Icon>,
        name: &str,
        detail: &str,
        remove: Option<&mut Option<u32>>,
    ) -> (bool, egui::Response) {
        let t = Tokens::get(ui.ctx());
        let id = ui.id().with(("card", format!("{what:?}")));
        let mut added = false;
        let inner = ui.dnd_drag_source(id, what.clone(), |ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(THUMB.x, THUMB.y + 34.0), Sense::hover());
            let p = ui.painter();
            let pic = Rect::from_min_size(rect.min, THUMB);
            p.rect_filled(pic, 6.0, t.control);
            if let Some(tex) = tex {
                let s = tex.size_vec2();
                let f = (THUMB.x / s.x).min(THUMB.y / s.y);
                p.image(tex.id(), Rect::from_center_size(pic.center(), s * f), Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            if let Some(i) = icon {
                icons::paint(p, Rect::from_center_size(pic.center(), vec2(30.0, 30.0)), i, t.muted);
            }
            p.rect_stroke(pic, 6.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
            let galley = p.layout(name.to_string(), egui::FontId::proportional(12.0), t.text, THUMB.x);
            p.galley_with_override_text_color(pos2(rect.min.x, pic.max.y + 3.0), galley, t.text);
            if !detail.is_empty() {
                p.text(pos2(rect.min.x, rect.max.y - 2.0), egui::Align2::LEFT_BOTTOM, detail, egui::FontId::proportional(11.0), t.muted);
            }
            rect
        });
        let rect = inner.inner;
        let card_resp = inner.response;
        card_resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
        // + at its top right.
        let plus = Rect::from_min_size(pos2(rect.max.x - 26.0, rect.min.y + 4.0), vec2(22.0, 22.0));
        let pr = ui.interact(plus, id.with("add"), Sense::click());
        pr.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Add {name}")));
        if card_resp.hovered() || pr.hovered() {
            ui.painter().circle_filled(plus.center(), 11.0, t.accent);
            ui.painter().text(plus.center(), egui::Align2::CENTER_CENTER, "+", egui::FontId::proportional(16.0), Color32::WHITE);
        }
        if pr.on_hover_text("Add it at the playhead, on a layer of its own").clicked() || card_resp.double_clicked() {
            added = true;
        }
        if let (Some(remove), LibDrop::Media(id)) = (remove, &what) {
            card_resp.context_menu(|ui| {
                if ui.button("Remove from the Media Bin").clicked() {
                    *remove = Some(*id);
                    ui.close();
                }
            });
        }
        (added, card_resp)
    }

    // ---- the item chosen: the panel at the right ----

    /// The panel of the item chosen on the timeline, at the right (in the camera's place). True
    /// when it's shown.
    pub(super) fn item_panel(&mut self, ed: &mut Editor, ui: &mut Ui) -> bool {
        let Some((layer, id)) = self.item else { return false };
        let Some(item) = self.session.timeline.item(layer, id).cloned() else {
            self.item = None;
            return false;
        };
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let media = self.session.timeline.media.clone();
        egui::Panel::right("video_item")
            .exact_size(super::SIDE_W)
            .resizable(false)
            .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(14)))
            .show(ui, |ui| {
                egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| {
                    ui.label(RichText::new(item.label(&media)).size(16.0).color(t.text));
                    ui.label(RichText::new(format!("{} to {}", super::clock(item.at), super::clock(item.end()))).size(12.0).color(t.muted));
                    ui.add_space(10.0);
                    let set = |s: &mut VideoView, p: serde_json::Value, dragging: bool| {
                        let mut p = p;
                        if let Some(o) = p.as_object_mut() {
                            o.insert("layer".into(), json!(layer));
                            o.insert("item".into(), json!(id));
                        }
                        if dragging {
                            let _ = s.session.drag("video.setItem", &p);
                        } else {
                            let _ = s.session.execute("video.setItem", &p);
                        }
                    };
                    let mut first = true;
                    let on_picture = !matches!(item.what, What::Transition { .. }) && item.layer_kind(&media) == layers::LayerKind::Picture;
                    if let What::Text { style, title, subtitle, .. } = &item.what
                        && style.has_words()
                    {
                        super::section(ui, "Text", &mut first);
                        let mut w = title.clone();
                        let r = ui.add(egui::TextEdit::multiline(&mut w).desired_rows(2).desired_width(f32::INFINITY).hint_text("What it says"));
                        if self.type_words {
                            // Just added: typing replaces the words it came with.
                            self.type_words = false;
                            r.request_focus();
                            let mut state = egui::text_edit::TextEditState::load(ui.ctx(), r.id).unwrap_or_default();
                            let all = egui::text::CCursorRange::two(egui::text::CCursor::new(0), egui::text::CCursor::new(w.chars().count()));
                            state.cursor.set_char_range(Some(all));
                            state.store(ui.ctx(), r.id);
                        }
                        self.text_change(r, || json!({"title": w.clone()}), &set);
                        if style.has_subtitle() {
                            let mut s2 = subtitle.clone();
                            let r = ui.add(egui::TextEdit::singleline(&mut s2).desired_width(f32::INFINITY).hint_text("Second line"));
                            self.text_change(r, || json!({"subtitle": s2.clone()}), &set);
                        }
                    }
                    let coloured =
                        matches!(&item.what, What::Text { style, .. } if !matches!(style, layers::TextStyle::Spotlight | layers::TextStyle::Blur));
                    if coloured || on_picture {
                        super::section(ui, "Look", &mut first);
                    }
                    if let What::Text { color, .. } = &item.what
                        && coloured
                    {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Colour").size(12.0).color(t.muted));
                            let [r, g, b] = layers::color(color, [0x6e, 0x5a, 0xd6]);
                            let mut c = Color32::from_rgb(r, g, b);
                            if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                                set(self, json!({"color": format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())}), false);
                            }
                        });
                        ui.add_space(4.0);
                    }
                    if on_picture {
                        let size_label = if matches!(item.what, What::Text { .. }) { "Size" } else { "Width, as a share of the video's" };
                        let (lo, hi) = if matches!(item.what, What::Text { .. }) { (0.3, 2.5) } else { (0.05, 1.0) };
                        self.slider(ui, size_label, item.place.w, lo, hi, "w", &set);
                        self.slider(ui, "Opacity", item.place.opacity, 0.1, 1.0, "opacity", &set);
                        super::section(ui, "Position", &mut first);
                        self.slider(ui, "Across", item.place.x, -0.5, 1.5, "x", &set);
                        self.slider(ui, "Down", item.place.y, -0.5, 1.5, "y", &set);
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("Or drag it on the picture: past the edges too. Drag a corner to resize it.").size(12.0).color(t.muted),
                        );
                        super::section(ui, "Animation", &mut first);
                        egui::Grid::new("item-animation").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                            for (label, key, anim) in [("Entrance", "intro", item.intro), ("Exit", "outro", item.outro)] {
                                ui.label(RichText::new(label).size(12.0).color(t.muted));
                                egui::ComboBox::from_id_salt(("anim", key)).width(150.0).selected_text(anim.kind.label()).show_ui(ui, |ui| {
                                    for k in AnimKind::ALL {
                                        if ui.selectable_label(anim.kind == k, k.label()).clicked() {
                                            set(self, json!({ key: k }), false);
                                        }
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    }
                    ui.add_space(16.0);
                    if widgets::button(ui, "Delete").on_hover_text("Take it off the timeline (Del)").clicked() {
                        self.run(ed, &ctx, "video.deleteItem", json!({"layer": layer, "item": id}));
                        self.item = None;
                    }
                });
            });
        true
    }

    /// A text being typed: each change one step of a single undo step, kept when typing stops.
    fn text_change(&mut self, r: egui::Response, p: impl FnOnce() -> serde_json::Value, set: &dyn Fn(&mut VideoView, serde_json::Value, bool)) {
        if r.gained_focus() {
            self.session.begin("Change Words");
        }
        if r.changed() {
            set(self, p(), true);
        }
        if r.lost_focus() {
            self.session.commit();
        }
    }

    /// A number of the item's placement, as a percentage slider (a drag is one undo step).
    #[allow(clippy::too_many_arguments)]
    fn slider(&mut self, ui: &mut Ui, label: &str, value: f32, lo: f32, hi: f32, key: &str, set: &dyn Fn(&mut VideoView, serde_json::Value, bool)) {
        let mut v = value * 100.0;
        let r = super::named_slider(ui, label, &mut v, lo * 100.0..=hi * 100.0, "%");
        let p = |v: f32| json!({ key: v / 100.0 });
        if r.drag_started() {
            self.session.begin("Change");
        }
        if r.changed() {
            set(self, p(v), r.dragged());
        }
        if r.drag_stopped() {
            self.session.commit();
        }
    }

    // ---- on the picture ----

    /// Where something chosen is on the picture shown in `pic` (screen points): the item chosen,
    /// or the camera.
    fn chosen_rect(&self, pic: Rect) -> Option<Rect> {
        let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
        let to_screen = |r: screengrabcraft_model::Rect| {
            let f = pic.width() / vw.max(1.0);
            Rect::from_min_size(pic.min + vec2(r.x * f, r.y * f), vec2(r.w * f, r.h * f))
        };
        if let Some((layer, id)) = self.item {
            let item = self.session.timeline.item(layer, id)?;
            let r = layer_draw::item_rect(item, &self.session.timeline, self.library.as_ref(), vw, vh)?;
            return Some(to_screen(r));
        }
        if self.camera_chosen {
            let (x, y, w, h) = self.camera_place()?;
            return Some(to_screen(screengrabcraft_model::Rect::new(x, y, w, h)));
        }
        None
    }

    /// The camera's place on the video (pixels), if it's on the picture.
    fn camera_place(&self) -> Option<(f32, f32, f32, f32)> {
        let tl = &self.session.timeline;
        let i = tl.find(StreamKind::Camera)?;
        let track = tl.tracks.get(i)?;
        if track.muted || tl.find(StreamKind::Screen).is_none() {
            return None;
        }
        let s = self.rec.stream(track.stream)?;
        let aspect = s.width as f32 / s.height.max(1) as f32;
        Some(screengrabcraft_media::compose::camera_rect(&tl.camera, aspect, self.composer.width as f32, self.composer.height as f32))
    }

    /// The items on the picture at the playhead, top first, with where they are (video pixels).
    fn items_on_picture(&self) -> Vec<(u32, u32, screengrabcraft_model::Rect)> {
        let tl = &self.session.timeline;
        let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
        let t = self.playhead;
        tl.layers
            .iter()
            .rev()
            .filter(|l| l.kind == layers::LayerKind::Picture && !l.hidden)
            .filter_map(|l| {
                let i = l.items.iter().find(|i| i.at <= t && t < i.end())?;
                Some((l.id, i.id, layer_draw::item_rect(i, tl, self.library.as_ref(), vw, vh)?))
            })
            .collect()
    }

    /// The picture as a designer: what's chosen outlined with its corners; click to choose an
    /// item (or the camera), drag it to move it, drag a corner to resize it.
    pub(super) fn designer(&mut self, ui: &mut Ui, pic: Rect) {
        let t = Tokens::get(ui.ctx());
        let ctx = ui.ctx().clone();
        let f = pic.width() / (self.composer.width as f32).max(1.0);
        // All round the picture too: what was moved off it is still taken hold of there.
        let resp = ui.interact(ui.max_rect(), ui.id().with("designer"), Sense::click_and_drag());
        let video_at = |p: Pos2| ((p - pic.min) / f).to_pos2();
        let corners = |r: Rect| [r.left_top(), r.right_top(), r.left_bottom(), r.right_bottom()];
        let chosen = self.chosen_rect(pic);
        if let Some(r) = chosen {
            ui.painter().rect_stroke(r, 0.0, Stroke::new(1.5, t.accent), StrokeKind::Outside);
            for c in corners(r) {
                ui.painter().rect_filled(Rect::from_center_size(c, vec2(9.0, 9.0)), 2.0, Color32::WHITE);
                ui.painter().rect_stroke(Rect::from_center_size(c, vec2(9.0, 9.0)), 2.0, Stroke::new(1.0, t.accent), StrokeKind::Outside);
            }
        }
        if let Some(p) = resp.hover_pos()
            && let Some(r) = chosen
        {
            if corners(r).iter().any(|c| c.distance(p) <= HANDLE) {
                ctx.set_cursor_icon(egui::CursorIcon::ResizeNwSe);
            } else if r.contains(p) {
                ctx.set_cursor_icon(egui::CursorIcon::Grab);
            }
        }
        // What's under the pointer: the topmost item, else the camera.
        let pick = |s: &VideoView, p: Pos2| -> Option<Option<(u32, u32)>> {
            let v = video_at(p);
            if let Some((l, i, _)) =
                s.items_on_picture().into_iter().find(|(_, _, r)| r.x <= v.x && v.x <= r.x + r.w && r.y <= v.y && v.y <= r.y + r.h)
            {
                return Some(Some((l, i)));
            }
            let (x, y, w, h) = s.camera_place()?;
            (x <= v.x && v.x <= x + w && y <= v.y && v.y <= y + h).then_some(None)
        };
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            match pick(self, p) {
                Some(Some(sel)) => {
                    self.item = Some(sel);
                    self.camera_chosen = false;
                }
                Some(None) => {
                    self.item = None;
                    self.camera_chosen = true;
                }
                None => {
                    self.item = None;
                    self.camera_chosen = false;
                }
            }
        }
        if resp.drag_started()
            && let Some(p) = ctx.input(|i| i.pointer.press_origin()).or(resp.interact_pointer_pos())
        {
            // A corner of what's chosen resizes it; inside it, or on another, moves.
            let mut target = match chosen {
                Some(r) if corners(r).iter().any(|c| c.distance(p) <= HANDLE) => Some((true, r)),
                Some(r) if r.contains(p) => Some((false, r)),
                _ => None,
            };
            if target.is_none()
                && let Some(sel) = pick(self, p)
            {
                self.item = sel;
                self.camera_chosen = sel.is_none();
                target = self.chosen_rect(pic).map(|r| (false, r));
            }
            if let Some((resize, r)) = target {
                let centre = video_at(r.center());
                let grab = video_at(p) - centre;
                let dist = grab.length().max(4.0);
                if let Some((layer, item)) = self.item {
                    let place = self.session.timeline.item(layer, item).map(|i| i.place).unwrap_or_default();
                    let w = place.w;
                    // Moved by as far as the pointer goes, from where it is (a broadcast graphic's
                    // place is how far it is from where it was designed, not its middle).
                    let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
                    let grab = video_at(p) - pos2(place.x * vw, place.y * vh);
                    self.session.begin(if resize { "Resize" } else { "Move" });
                    self.canvas_drag = Some(CanvasDrag::Item { layer, item, grab, resize: resize.then_some((w, dist)) });
                } else if self.camera_chosen {
                    self.session.begin(if resize { "Resize Camera" } else { "Move Camera" });
                    let size = self.session.timeline.camera.size;
                    self.canvas_drag = Some(CanvasDrag::Camera { grab, resize: resize.then_some((size, dist)) });
                }
            }
        }
        if resp.dragged()
            && let (Some(p), Some(d)) = (resp.interact_pointer_pos(), self.canvas_drag)
        {
            let v = video_at(p);
            let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
            // Moved near the picture's middle or one of its edges, it's caught there (its middle
            // on the middle, its edge on the edge), and a line shows where. Alt held, nothing
            // catches. How far to go to be caught, across and down, in the video's own pixels.
            let free = ctx.input(|i| i.modifiers.alt);
            let catch = |s: &VideoView| -> (Option<(f32, f32)>, Option<(f32, f32)>) {
                let Some(r) = s.chosen_rect(pic).filter(|_| !free) else { return (None, None) };
                let nearest =
                    |ways: [(f32, f32); 3]| ways.into_iter().filter(|(d, _)| d.abs() <= CATCH).min_by(|a, b| a.0.abs().total_cmp(&b.0.abs()));
                let x =
                    nearest([(pic.center().x - r.center().x, pic.center().x), (pic.min.x - r.min.x, pic.min.x), (pic.max.x - r.max.x, pic.max.x)]);
                let y =
                    nearest([(pic.center().y - r.center().y, pic.center().y), (pic.min.y - r.min.y, pic.min.y), (pic.max.y - r.max.y, pic.max.y)]);
                (x, y)
            };
            let mut lines = (None, None);
            match d {
                CanvasDrag::Item { layer, item, grab, resize } => {
                    let params = match resize {
                        Some((w0, d0)) => {
                            let centre = self.chosen_rect(pic).map(|r| video_at(r.center())).unwrap_or(v);
                            json!({"layer": layer, "item": item, "w": w0 * ((v - centre).length() / d0)})
                        }
                        None => json!({"layer": layer, "item": item, "x": (v.x - grab.x) / vw, "y": (v.y - grab.y) / vh}),
                    };
                    let _ = self.session.drag("video.setItem", &params);
                    if resize.is_none() {
                        let (x, y) = catch(self);
                        if x.is_some() || y.is_some() {
                            let (dx, dy) = (x.map_or(0.0, |c| c.0) / f, y.map_or(0.0, |c| c.0) / f);
                            let params = json!({"layer": layer, "item": item, "x": (v.x - grab.x + dx) / vw, "y": (v.y - grab.y + dy) / vh});
                            let _ = self.session.drag("video.setItem", &params);
                            lines = (x.map(|c| c.1), y.map(|c| c.1));
                        }
                    }
                }
                CanvasDrag::Camera { grab, resize } => {
                    let centre = self.camera_place().map(|(x, y, w, h)| pos2(x + w / 2.0, y + h / 2.0)).unwrap_or(v);
                    let params = match resize {
                        Some((s0, d0)) => json!({"x": centre.x / vw, "y": centre.y / vh, "size": s0 * ((v - centre).length() / d0)}),
                        None => json!({"x": (v.x - grab.x) / vw, "y": (v.y - grab.y) / vh}),
                    };
                    let _ = self.session.drag("video.cameraAt", &params);
                    if resize.is_none() {
                        let (x, y) = catch(self);
                        if x.is_some() || y.is_some() {
                            let (dx, dy) = (x.map_or(0.0, |c| c.0) / f, y.map_or(0.0, |c| c.0) / f);
                            let params = json!({"x": (v.x - grab.x + dx) / vw, "y": (v.y - grab.y + dy) / vh});
                            let _ = self.session.drag("video.cameraAt", &params);
                            lines = (x.map(|c| c.1), y.map(|c| c.1));
                        }
                    }
                }
            }
            let guide = Stroke::new(1.0, Color32::from_rgb(0xFF, 0xD5, 0x4F));
            if let Some(x) = lines.0 {
                ui.painter().line_segment([pos2(x, pic.min.y), pos2(x, pic.max.y)], guide);
            }
            if let Some(y) = lines.1 {
                ui.painter().line_segment([pos2(pic.min.x, y), pos2(pic.max.x, y)], guide);
            }
        }
        if resp.drag_stopped() && self.canvas_drag.take().is_some() {
            self.session.commit();
        }
    }
}

/// An item's colour on the timeline.
pub fn item_colour(item: &Item, media: &[layers::MediaItem]) -> Color32 {
    match &item.what {
        What::Media { id } => match media.iter().find(|m| m.id == *id).map(|m| m.kind) {
            Some(MediaKind::Sound) => Color32::from_rgb(0x2F, 0x80, 0xED),
            Some(MediaKind::Video) => Color32::from_rgb(0x6E, 0x5A, 0xD6),
            _ => Color32::from_rgb(0x1F, 0x9D, 0x8B),
        },
        What::Text { .. } => Color32::from_rgb(0xD9, 0x7B, 0x29),
        What::Transition { .. } => Color32::from_rgb(0x8A, 0x8A, 0x96),
    }
}

#[cfg(test)]
impl VideoView {
    /// The video's picture at `t`, as it's made (for tests).
    pub fn frame_at(&mut self, t: u64) -> Raster {
        self.composer.frame(&self.session.timeline, t)
    }

    /// Where a layer's item is on the timeline, as last drawn (for tests).
    pub fn item_on_timeline(&self, layer: u32, item: u32) -> Option<Rect> {
        let (lane, rows_top) = self.lane_rect?;
        let k = self.session.timeline.layers.iter().rev().position(|l| l.id == layer)?;
        let it = self.session.timeline.item(layer, item)?;
        let x_of = |us: u64| lane.min.x + (us as f64 / 1e6 * f64::from(self.pps)) as f32 - self.scroll;
        let top = rows_top + k as f32 * super::ROW_H;
        Some(Rect::from_min_max(pos2(x_of(it.at), top + 3.0), pos2(x_of(it.end()), top + super::ROW_H - 7.0)))
    }

    /// How far along the timeline is scrolled, in points (for tests).
    pub fn scrolled(&self) -> f32 {
        self.scroll
    }

    /// Asks the host about captions on the next frame (for tests).
    pub fn poll_now(&mut self) {
        self.next_poll = 0.0;
    }

    /// Where what's chosen is on the screen, as last shown (for tests).
    pub fn chosen_on_screen(&self) -> Option<Rect> {
        self.chosen_rect(self.picture_rect?)
    }

    /// Where the camera is on the screen, as last shown (for tests).
    pub fn camera_on_screen(&self) -> Option<Rect> {
        let pic = self.picture_rect?;
        let (x, y, w, h) = self.camera_place()?;
        let f = pic.width() / (self.composer.width as f32).max(1.0);
        Some(Rect::from_min_size(pic.min + vec2(x * f, y * f), vec2(w * f, h * f)))
    }

    /// How many of the library's cards have their picture drawn (for tests).
    pub fn thumbnails_made(&self) -> usize {
        self.thumbs.keys().filter(|k| k.starts_with("template-")).count()
    }

    /// The template whose preview is playing (the pointer on its card).
    pub fn previewing(&self) -> Option<&'static str> {
        self.lib_hover.map(|(k, _)| k).filter(|_| self.preview_tex.is_some())
    }

    /// A template's card picture, as drawn (for tests and the README's pictures).
    pub fn template_card_picture(key: &str) -> Option<Raster> {
        template_thumbnail(&layers::template(key)?)
    }
}
