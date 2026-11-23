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
    /// A transition between two clips, dropped where they meet.
    Blend(screengrabcraft_media::timeline::Blend),
    /// A template of the owner's, by its name.
    Yours(String),
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
    Item {
        layer: u32,
        item: u32,
        grab: Vec2,
        resize: Option<(f32, f32)>,
    },
    Camera {
        /// Its track.
        track: usize,
        grab: Vec2,
        resize: Option<(f32, f32)>,
    },
    /// A pinned picture's corner (0 top left, then round).
    Pin {
        layer: u32,
        item: u32,
        corner: usize,
    },
    /// A point of the pointer's redrawn path.
    PointerPoint {
        path: usize,
        point: usize,
    },
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
pub(super) fn backdrop(w: u32, h: u32) -> Option<Raster> {
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
        corrected: None,
        markers: Vec::new(),
        keys: Default::default(),
        ducking: Default::default(),
        watermark: Default::default(),
        frame: Default::default(),
        motion_blur: Default::default(),
        pointer_hidden: Vec::new(),
        pointer_paths: Vec::new(),
        camera_full: Vec::new(),
        cameras: Vec::new(),
    };
    tl.place(t.item.clone(), None, 0).ok()?;
    layer_draw::draw_layers(&mut pic, &tl, &layer_draw::NoMedia, at.min(t.item.len.saturating_sub(1)), texts, &mut Default::default());
    // A zoom shows the made-up screen as it zooms it.
    if matches!(t.item.what, What::Zoom) {
        let view = screengrabcraft_media::zoom::view_at(&tl, at.min(t.item.len.saturating_sub(1)));
        pic = screengrabcraft_media::compose::zoomed(&pic, view, w, h).unwrap_or(pic);
    }
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

pub(super) fn texture(ctx: &egui::Context, name: &str, pic: &Raster) -> egui::TextureHandle {
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
        // A new video's pictures, once they're all read in: put in turn.
        self.poll_pictures(ed, ctx);
        if !self.importing.is_empty() || !self.rereading.is_empty() {
            ctx.request_repaint_after(std::time::Duration::from_millis(150));
        }
    }

    /// Puts a template or a file of the bin on the timeline: at `at` on `layer` (or a new one),
    /// the nearest place it fits; it's then chosen.
    pub fn drop_on_timeline(&mut self, ed: &mut Editor, ctx: &egui::Context, what: &LibDrop, layer: Option<u32>, at: u64) {
        let mut p = match what {
            LibDrop::Template(key) => json!({"template": key, "theme": self.theme_param()}),
            LibDrop::Media(id) => json!({"media": id}),
            LibDrop::Blend(kind) => return self.drop_blend(ed, ctx, *kind, None, at),
            // Its own layers, whatever it was dropped on.
            LibDrop::Yours(name) => return self.use_template(ed, ctx, name, at),
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
        self.load_yours(ed);
        let wanted = self.lib_search.trim().to_lowercase();
        if !wanted.is_empty() {
            let found: Vec<&layers::Template> =
                lib.iter().filter(|x| x.name.to_lowercase().contains(&wanted) || x.group.label().to_lowercase().contains(&wanted)).collect();
            let blends = super::transitions::matching(&wanted);
            let own = self.yours.templates.iter().any(|x| x.name.to_lowercase().contains(&wanted));
            if found.is_empty() && blends.is_empty() && !own {
                ui.add_space(8.0);
                ui.label(RichText::new(format!("Nothing in the library is called \u{201c}{}\u{201d}.", self.lib_search.trim())).color(t.muted));
            }
            self.yours_section(ed, ui, ctx, &wanted);
            let mut budget = THUMBS_PER_FRAME;
            let mut hovered = false;
            for row in found.chunks(2) {
                ui.horizontal(|ui| {
                    for tpl in row {
                        self.template_card(ed, ui, ctx, tpl, tpl.group.label(), &mut budget, &mut hovered);
                    }
                });
            }
            self.blend_cards(ed, ui, ctx, &blends, super::transitions::GROUP, &mut budget);
            if !hovered {
                self.lib_hover = None;
            }
            return;
        }
        // The owner's own templates, and the themes, first.
        for (name, themes) in [(super::yours::YOURS, false), (super::yours::THEMES, true)] {
            egui::CollapsingHeader::new(RichText::new(name).size(14.0).strong().color(t.text))
                .id_salt(("library-section", name))
                .default_open(false)
                .show_unindented(ui, |ui| {
                    ui.add_space(4.0);
                    if themes {
                        self.themes_section(ed, ui, ctx);
                    } else {
                        self.yours_section(ed, ui, ctx, "");
                    }
                    ui.add_space(6.0);
                });
            ui.add_space(2.0);
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
                    if g == Group::Zoom {
                        self.zoom_on_clicks_button(ed, ui, ctx);
                    }
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
            // Transitions between clips, after the zooms.
            if g == Group::Zoom {
                egui::CollapsingHeader::new(RichText::new(super::transitions::GROUP).size(14.0).strong().color(t.text))
                    .id_salt(("library-section", super::transitions::GROUP))
                    .default_open(false)
                    .show_unindented(ui, |ui| {
                        ui.add_space(4.0);
                        ui.label(RichText::new("Drop one where two clips of a track meet.").size(12.0).color(t.muted));
                        ui.add_space(4.0);
                        self.blend_cards(ed, ui, ctx, &screengrabcraft_media::timeline::Blend::ALL, "", &mut budget);
                        ui.add_space(6.0);
                    });
                ui.add_space(2.0);
            }
        }
        if !hovered {
            self.lib_hover = None;
        }
    }

    /// Zoom on the Clicks: zooms that follow the recording's clicks (greyed out without any).
    pub(super) fn zoom_on_clicks_button(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        let clicks = self.session.has_clicks();
        if ui
            .add_enabled(clicks, egui::Button::new("Zoom on the Clicks"))
            .on_hover_text("A zoom for each run of clicks, in a little before and out a while after: ordinary zooms, to change or delete")
            .on_disabled_hover_text("This recording kept no clicks")
            .clicked()
        {
            self.zoom_on_clicks(ed, ctx);
        }
        ui.label(RichText::new("Or drag a zoom onto the timeline.").size(12.0).color(t.muted));
        ui.add_space(6.0);
    }

    /// Zooms made on the clicks; the layer they're on is shown.
    pub(super) fn zoom_on_clicks(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        match self.session.execute("video.zoomOnClicks", &json!({})) {
            Ok(_) => self.item = None,
            Err(e) => ed.error(ctx, e.to_string()),
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
        // As it will be placed: in the theme in use.
        let themed = self.themed(tpl);
        let tpl = &themed;
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
        // A video made of the pictures: one after another from the playhead, a few seconds each.
        let pictures = media.iter().filter(|m| m.kind == MediaKind::Picture).count();
        if pictures > 0 {
            ui.horizontal(|ui| {
                let put = ui
                    .button("Put the Pictures in Turn")
                    .on_hover_text("Every picture here, one after another from the playhead on a layer of its own, each as large as fits the video")
                    .clicked();
                ui.add(egui::DragValue::new(&mut self.picture_secs).range(1.0..=30.0).speed(0.1).suffix(" s each"));
                if put {
                    let each = (self.picture_secs.clamp(1.0, 30.0) * 1e6) as u64;
                    self.run(ed, ctx, "video.picturesInTurn", json!({"each": each, "at": self.playhead}));
                }
            });
            ui.label(
                RichText::new("For a narrated video, talk over them with Record Narration (the side panel, with nothing chosen).")
                    .size(12.0)
                    .color(t.muted),
            );
            ui.add_space(8.0);
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
    pub(super) fn card(
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
                    // With keyframes, its place, size and opacity are those of the keyframe at the
                    // playhead (made there if there's none).
                    let keyed = !item.keys.is_empty();
                    let (start, end) = (item.at, item.end());
                    let set = |s: &mut VideoView, p: serde_json::Value, dragging: bool| {
                        let mut p = p;
                        let placing = keyed && ["x", "y", "w", "opacity"].iter().any(|k| p.get(*k).is_some());
                        if let Some(o) = p.as_object_mut() {
                            o.insert("layer".into(), json!(layer));
                            o.insert("item".into(), json!(id));
                            if placing {
                                o.insert("at".into(), json!(s.playhead.clamp(start, end)));
                            }
                        }
                        let cmd = if placing { "video.keyframe" } else { "video.setItem" };
                        if dragging {
                            let _ = s.session.drag(cmd, &p);
                        } else {
                            let _ = s.session.execute(cmd, &p);
                        }
                    };
                    let place = item.place_at(self.playhead);
                    let mut first = true;
                    if matches!(item.what, What::Freeze { .. }) {
                        self.freeze_panel(ui, &item, &set);
                        ui.add_space(16.0);
                        if widgets::button(ui, "Delete").on_hover_text("Take it off the timeline (Del)").clicked() {
                            self.run(ed, &ctx, "video.deleteItem", json!({"layer": layer, "item": id}));
                            self.item = None;
                        }
                        return;
                    }
                    if matches!(item.what, What::Zoom) {
                        self.zoom_panel(ui, &item, place, &set);
                        super::section(ui, "Motion", &mut false);
                        self.motion_section(ed, ui, &ctx, layer, &item);
                        ui.add_space(16.0);
                        if widgets::button(ui, "Delete").on_hover_text("Take it off the timeline (Del)").clicked() {
                            self.run(ed, &ctx, "video.deleteItem", json!({"layer": layer, "item": id}));
                            self.item = None;
                        }
                        return;
                    }
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
                    if let What::Text { style, font, .. } = &item.what
                        && style.has_words()
                    {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new("Face").size(12.0).color(t.muted));
                            egui::ComboBox::from_id_salt("item-font").width(150.0).selected_text(font.label()).show_ui(ui, |ui| {
                                for f in layers::TextFont::ALL {
                                    if ui.selectable_label(*font == f, f.label()).clicked() {
                                        set(self, json!({ "font": f }), false);
                                    }
                                }
                            });
                        });
                        ui.add_space(4.0);
                    }
                    if on_picture {
                        let size_label = if matches!(item.what, What::Text { .. }) { "Size" } else { "Width, as a share of the video's" };
                        let (lo, hi) = if matches!(item.what, What::Text { .. }) { (0.3, 2.5) } else { (0.05, 1.0) };
                        self.slider(ui, size_label, place.w, lo, hi, "w", &set);
                        self.slider(ui, "Opacity", place.opacity, 0.1, 1.0, "opacity", &set);
                        self.frame_section(ui, &item, &media, &mut first, &set);
                        if !matches!(item.what, What::Text { style: layers::TextStyle::Blur, .. }) {
                            self.mask_section(ui, &item, &mut first, &set);
                        }
                        super::section(ui, "Position", &mut first);
                        self.slider(ui, "Across", place.x, -0.5, 1.5, "x", &set);
                        self.slider(ui, "Down", place.y, -0.5, 1.5, "y", &set);
                        ui.add_space(2.0);
                        ui.label(
                            RichText::new("Or drag it on the picture: past the edges too. Drag a corner to resize it.").size(12.0).color(t.muted),
                        );
                        super::section(ui, "Motion", &mut first);
                        self.motion_section(ed, ui, &ctx, layer, &item);
                        super::section(ui, "Animation", &mut first);
                        egui::Grid::new("item-animation").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                            for (label, key, anim) in [("Entrance", "intro", item.intro), ("Exit", "outro", item.outro)] {
                                ui.label(RichText::new(label).size(12.0).color(t.muted));
                                egui::ComboBox::from_id_salt(("anim", key)).width(150.0).selected_text(anim.kind.label()).show_ui(ui, |ui| {
                                    for k in AnimKind::ALL {
                                        if ui.selectable_label(anim.kind == k, k.label()).clicked() {
                                            // Writing takes a while longer than the others: a second
                                            // or more (of no more than half of it).
                                            let len_key = if key == "intro" { "introLen" } else { "outroLen" };
                                            let len = if k == AnimKind::Write { anim.len.max(1_200_000) } else { anim.len };
                                            set(self, json!({ key: k, len_key: len }), false);
                                        }
                                    }
                                });
                                ui.end_row();
                            }
                        });
                    }
                    self.item_sound_section(ed, ui, &ctx, layer, &item, &mut first);
                    // Its media swapped, its effects copied or pasted.
                    super::section(ui, "Effects", &mut first);
                    ui.horizontal_wrapped(|ui| {
                        if matches!(item.what, What::Media { .. }) {
                            self.replace_menu(ed, ui, &ctx, layer, id);
                        }
                        self.effects_items(ed, ui, &ctx, json!({"layer": layer, "item": id}));
                    });
                    ui.add_space(16.0);
                    if widgets::button(ui, "Delete").on_hover_text("Take it off the timeline (Del)").clicked() {
                        self.run(ed, &ctx, "video.deleteItem", json!({"layer": layer, "item": id}));
                        self.item = None;
                    }
                });
            });
        true
    }

    /// Whether an item is a video whose sound can be taken apart from its picture.
    pub(super) fn separable(&self, item: &Item) -> bool {
        let media = &self.session.timeline.media;
        !item.sound_only
            && !item.silent
            && matches!(&item.what, What::Media { id } if media.iter().any(|m| m.id == *id && m.kind == MediaKind::Video))
    }

    /// A video's sound put apart from its picture, on a sound layer; the sound then chosen.
    pub(super) fn separate_sound(&mut self, ed: &mut Editor, ctx: &egui::Context, layer: u32, item: u32) {
        match self.session.execute("video.separateSound", &json!({"layer": layer, "item": item})) {
            Ok(v) => {
                let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).and_then(|n| u32::try_from(n).ok());
                if let (Some(l), Some(i)) = (n("layer"), n("item")) {
                    self.item = Some((l, i));
                }
            }
            Err(e) => ed.error(ctx, e.to_string()),
        }
    }

    /// An item's sound: a video's taken apart from its picture or kept silent; a sound layer mixed
    /// to mono.
    fn item_sound_section(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context, layer: u32, item: &Item, first: &mut bool) {
        let t = Tokens::get(ctx);
        let media = self.session.timeline.media.clone();
        let video = matches!(&item.what, What::Media { id } if media.iter().any(|m| m.id == *id && m.kind == MediaKind::Video));
        let sound = item.layer_kind(&media) == layers::LayerKind::Sound;
        if !video && !sound {
            return;
        }
        super::section(ui, "Sound", first);
        if video && !item.sound_only {
            if ui
                .add_enabled(self.separable(item), egui::Button::new("Separate the Sound").min_size(vec2(ui.available_width(), 28.0)))
                .on_hover_text(super::SEPARATE_TIP)
                .on_disabled_hover_text("Its sound is apart already, or silent")
                .clicked()
            {
                self.separate_sound(ed, ctx, layer, item.id);
            }
            let mut heard = !item.silent;
            if ui.checkbox(&mut heard, "Its sound heard with it").changed() {
                self.run(ed, ctx, "video.setItem", json!({"layer": layer, "item": item.id, "silent": !heard}));
            }
        }
        if item.sound_only {
            ui.label(RichText::new("The video's sound, apart from its picture: it moves, trims and goes on its own.").size(12.0).color(t.muted));
            ui.add_space(4.0);
        }
        if sound {
            let mut mono = self.session.timeline.layer(layer).is_some_and(|l| l.mono);
            if ui
                .checkbox(&mut mono, "Mix the layer to mono")
                .on_hover_text("Its two sides mixed into one, heard the same from both speakers")
                .changed()
            {
                self.run(ed, ctx, "video.mono", json!({"layer": layer, "mono": mono}));
            }
        }
        if item.plays_sound(&media) {
            ui.add_space(4.0);
            self.line_up_menu(ed, ui, ctx, json!({"layer": layer, "item": item.id}));
        }
    }

    /// A zoom's settings: how close, where, and how it goes in and out.
    fn zoom_panel(&mut self, ui: &mut Ui, item: &Item, place: layers::Placement, set: &dyn Fn(&mut VideoView, serde_json::Value, bool)) {
        let t = Tokens::get(ui.ctx());
        let mut first = true;
        super::section(ui, "Zoom", &mut first);
        ui.label(RichText::new("While it's chosen the picture shows all of the screen and the zoom's frame: drag the frame to where it zooms, and its corners to zoom closer or less.").size(12.0).color(t.muted));
        ui.add_space(6.0);
        self.slider(ui, "Shows of the screen", screengrabcraft_media::zoom::share(place.w), screengrabcraft_media::zoom::MIN_SHARE, 1.0, "w", set);
        super::section(ui, "Position", &mut first);
        self.slider(ui, "Across", place.x, 0.0, 1.0, "x", set);
        self.slider(ui, "Down", place.y, 0.0, 1.0, "y", set);
        super::section(ui, "Going in and out", &mut first);
        egui::Grid::new("zoom-ease").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            for (label, key, anim) in [("In", "intro", item.intro), ("Out", "outro", item.outro)] {
                ui.label(RichText::new(label).size(12.0).color(t.muted));
                let smooth = anim.kind != AnimKind::None;
                egui::ComboBox::from_id_salt(("zoom-ease", key)).width(150.0).selected_text(if smooth { "Smoothly" } else { "At once" }).show_ui(
                    ui,
                    |ui| {
                        if ui.selectable_label(smooth, "Smoothly").clicked() {
                            set(self, json!({ key: AnimKind::Fade }), false);
                        }
                        if ui.selectable_label(!smooth, "At once").clicked() {
                            set(self, json!({ key: AnimKind::None }), false);
                        }
                    },
                );
                ui.end_row();
            }
        });
        ui.add_space(4.0);
        ui.label(RichText::new("A zoom that starts where another ends pans straight from one to the other.").size(12.0).color(t.muted));
        ui.add_space(4.0);
        let mut blur = self.session.timeline.motion_blur.on;
        if ui
            .checkbox(&mut blur, "Motion blur")
            .on_hover_text("Going in, out or along, the screen blurred along its way (for every zoom and moving item: the side panel's Motion Blur)")
            .changed()
        {
            let _ = self.session.execute("video.motionBlur", &json!({"on": blur}));
        }
    }

    /// An item's keyframes: one added at the playhead, each gone to or taken away.
    fn motion_section(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context, layer: u32, item: &Item) {
        let t = Tokens::get(ctx);
        let on_it = (item.at..=item.end()).contains(&self.playhead);
        let what = if matches!(item.what, What::Zoom) { "where it zooms and how close" } else { "where it is, how large and how see-through" };
        if item.keys.is_empty() {
            ui.label(
                RichText::new(format!(
                    "Keyframes move it: add one where it starts, move the playhead on, then change {what}; it goes there smoothly."
                ))
                .size(12.0)
                .color(t.muted),
            );
            ui.add_space(4.0);
        }
        let add = ui
            .add_enabled(on_it, egui::Button::new("Add Keyframe at the Playhead").min_size(vec2(ui.available_width(), 28.0)))
            .on_hover_text(format!("Keeps {what} at this moment"))
            .on_disabled_hover_text("Move the playhead onto the item first")
            .clicked();
        if add {
            self.run(ed, ctx, "video.keyframe", json!({"layer": layer, "item": item.id, "at": self.playhead}));
        }
        let (mut go, mut remove) = (None, None);
        for (k, key) in item.keys.iter().enumerate() {
            let at = item.at.saturating_add(key.at);
            let here = at.abs_diff(self.playhead) < screengrabcraft_media::timeline::MIN_CLIP_US;
            ui.horizontal(|ui| {
                // A diamond, as on the timeline, and when.
                let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                let (c, s) = (r.center(), 4.5);
                let colour = if here { t.accent } else { t.text };
                ui.painter().add(egui::Shape::convex_polygon(
                    vec![pos2(c.x, c.y - s), pos2(c.x + s, c.y), pos2(c.x, c.y + s), pos2(c.x - s, c.y)],
                    colour,
                    Stroke::NONE,
                ));
                let words = RichText::new(super::clock(at)).size(12.5);
                ui.label(if here { words.color(t.accent).strong() } else { words.color(t.text) });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if ui.small_button("Remove").on_hover_text("Take this keyframe away").clicked() {
                        remove = Some(k);
                    }
                    if ui.small_button("Go To").on_hover_text("Move the playhead to it").clicked() {
                        go = Some(at);
                    }
                });
            });
        }
        if !item.keys.is_empty() {
            ui.add_space(4.0);
            ui.label(
                RichText::new("With keyframes, changing it changes the keyframe at the playhead (or adds one there).").size(12.0).color(t.muted),
            );
            let mut path = item.path;
            if ui
                .add_enabled(item.keys.len() >= 2 && !matches!(item.what, What::Zoom), egui::Checkbox::new(&mut path, "Along a curved path"))
                .on_hover_text(
                    "Through its keyframes' places on a smooth curve, without stopping at them; the path shows on the picture while it's chosen",
                )
                .on_disabled_hover_text("Two keyframes or more make a path")
                .changed()
            {
                self.run(ed, ctx, "video.setItem", json!({"layer": layer, "item": item.id, "path": path}));
            }
            if ui.button("Remove Every Keyframe").on_hover_text("It stays where the last one had it").clicked() {
                self.run(ed, ctx, "video.removeKeyframe", json!({"layer": layer, "item": item.id, "all": true}));
            }
        }
        if let Some(at) = go {
            self.pause();
            self.playhead = at;
        }
        if let Some(k) = remove {
            self.run(ed, ctx, "video.removeKeyframe", json!({"layer": layer, "item": item.id, "key": k}));
        }
    }

    /// Whether the item chosen is a zoom or a frozen region (the picture then shows all of the
    /// screen, to place it).
    pub(super) fn zoom_chosen(&self) -> bool {
        self.item.and_then(|(l, i)| self.session.timeline.item(l, i)).is_some_and(|it| matches!(it.what, What::Zoom | What::Freeze { .. }))
    }

    /// A frozen region's panel: where it is and how large, and what it does.
    fn freeze_panel(&mut self, ui: &mut Ui, item: &Item, set: &dyn Fn(&mut VideoView, serde_json::Value, bool)) {
        let t = Tokens::get(ui.ctx());
        let mut first = true;
        let h = match item.what {
            What::Freeze { h } => h,
            _ => return,
        };
        let place = item.place;
        super::section(ui, "Frozen Region", &mut first);
        ui.label(
            RichText::new("This part of the screen stays as it was when the item starts, while the rest plays on. Drag it on the picture, and its corners to size it; drag its ends on the timeline for how long.")
                .size(12.0)
                .color(t.muted),
        );
        ui.add_space(6.0);
        self.slider(ui, "Width", place.w, 0.02, 1.0, "w", set);
        self.slider(ui, "Height", h, 0.02, 1.0, "h", set);
        super::section(ui, "Position", &mut first);
        self.slider(ui, "Across", place.x, 0.0, 1.0, "x", set);
        self.slider(ui, "Down", place.y, 0.0, 1.0, "y", set);
        ui.add_space(4.0);
        ui.label(RichText::new("The mouse pointer still shows over it.").size(12.0).color(t.muted));
    }

    /// A rectangle of the screen frozen over the chosen stretch, or for three seconds from the
    /// playhead: chosen, to place and size it.
    pub(super) fn freeze_region(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let (from, to) = self.range.unwrap_or((self.playhead, self.playhead.saturating_add(3_000_000).min(self.length().max(self.playhead + 1))));
        match self.session.execute("video.freezeRegion", &json!({"from": from, "to": to})) {
            Ok(v) => {
                let n = |k: &str| v.get(k).and_then(serde_json::Value::as_u64).and_then(|n| u32::try_from(n).ok());
                if let (Some(l), Some(i)) = (n("layer"), n("item")) {
                    self.item = Some((l, i));
                    self.camera_chosen = false;
                    self.pause();
                    self.playhead = from;
                }
            }
            Err(e) => ed.error(ctx, e.to_string()),
        }
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

    /// How an item is framed: turned (anything on the picture), and a picture or a video cut down
    /// at its sides, with a border, rounded corners and a shadow.
    fn frame_section(
        &mut self,
        ui: &mut Ui,
        item: &Item,
        media: &[layers::MediaItem],
        first: &mut bool,
        set: &dyn Fn(&mut VideoView, serde_json::Value, bool),
    ) {
        let t = Tokens::get(ui.ctx());
        let look = item.look.clone().checked();
        let mut deg = look.rotate;
        let r = super::named_slider(ui, "Turn", &mut deg, -180.0..=180.0, "°");
        self.look_change(&r, (deg - look.rotate).abs() > 0.4, json!({"rotate": deg.round()}), set);
        let picture = matches!(&item.what, What::Media { id } if media.iter().any(|m| m.id == *id && m.kind != MediaKind::Sound));
        if !picture {
            return;
        }
        super::section(ui, "Frame", first);
        for (k, name) in ["Crop the left", "Crop the top", "Crop the right", "Crop the bottom"].into_iter().enumerate() {
            let was = look.crop.get(k).copied().unwrap_or(0.0) * 100.0;
            let mut v = was;
            let r = super::named_slider(ui, name, &mut v, 0.0..=45.0, "%");
            let mut crop = look.crop;
            if let Some(c) = crop.get_mut(k) {
                *c = v / 100.0;
            }
            self.look_change(&r, (v - was).abs() > 0.4, json!({"crop": crop}), set);
        }
        let mut border = look.border * 100.0;
        let r = super::named_slider(ui, "Border, of its width", &mut border, 0.0..=10.0, "%");
        self.look_change(&r, (border - look.border * 100.0).abs() > 0.05, json!({"border": border / 100.0}), set);
        if look.border > 0.0 {
            ui.horizontal(|ui| {
                ui.label(RichText::new("Border colour").size(12.0).color(t.muted));
                let [r, g, b] = layers::color(&look.border_colour, [0xff, 0xff, 0xff]);
                let mut c = Color32::from_rgb(r, g, b);
                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                    set(self, json!({"borderColour": format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())}), false);
                }
            });
        }
        let mut radius = look.radius * 100.0;
        let r = super::named_slider(ui, "Rounded corners", &mut radius, 0.0..=50.0, "%");
        self.look_change(&r, (radius - look.radius * 100.0).abs() > 0.4, json!({"radius": radius / 100.0}), set);
        let mut shadow = look.shadow;
        if ui.checkbox(&mut shadow, "Shadow").on_hover_text("A soft shadow under it").changed() {
            set(self, json!({"shadow": shadow}), false);
        }
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            ui.label(RichText::new("Device round it").size(12.0).color(t.muted));
            egui::ComboBox::from_id_salt("item-device").width(140.0).selected_text(look.device.label()).show_ui(ui, |ui| {
                for d in screengrabcraft_media::frame::Device::ALL {
                    if ui.selectable_label(look.device == d, d.label()).clicked() {
                        set(self, json!({ "device": d }), false);
                    }
                }
            });
        });
        // Pinned onto a slanted surface: its four corners dragged on the picture.
        super::section(ui, "Corner Pin", first);
        let mut pinned = look.pinned;
        if ui
            .checkbox(&mut pinned, "Pin its corners")
            .on_hover_text(
                "Drag its four corners on the picture: it's drawn in perspective between them (a screen seen at an angle, a poster on a wall)",
            )
            .changed()
        {
            set(self, json!({"pinned": pinned}), false);
        }
        if look.pinned {
            ui.label(
                RichText::new("Drag the round handles at its corners on the picture. While pinned it isn't turned, rounded or bordered.")
                    .size(12.0)
                    .color(t.muted),
            );
            if ui.button("Put the Corners Back").on_hover_text("Its corners where its box has them").clicked() {
                set(self, json!({"pin": [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0], [0.0, 0.0]]}), false);
            }
        }
    }

    /// An item's mask (the shape it's shown in, or that's cut out of it: where in it and how
    /// large) and how it's blended with what's under it.
    fn mask_section(&mut self, ui: &mut Ui, item: &Item, first: &mut bool, set: &dyn Fn(&mut VideoView, serde_json::Value, bool)) {
        let t = Tokens::get(ui.ctx());
        let look = item.look.clone().checked();
        let mask = look.mask;
        super::section(ui, "Mask and Blend", first);
        egui::Grid::new("item-mask").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            ui.label(RichText::new("Shown in").size(12.0).color(t.muted));
            egui::ComboBox::from_id_salt("mask-shape").width(150.0).selected_text(mask.shape.label()).show_ui(ui, |ui| {
                for k in layers::MaskShape::ALL {
                    if ui.selectable_label(mask.shape == k, k.label()).clicked() {
                        set(self, json!({ "mask": k }), false);
                    }
                }
            });
            ui.end_row();
            ui.label(RichText::new("Blend").size(12.0).color(t.muted));
            egui::ComboBox::from_id_salt("blend-mode").width(150.0).selected_text(look.blend.label()).show_ui(ui, |ui| {
                for k in layers::BlendMode::ALL {
                    if ui.selectable_label(look.blend == k, k.label()).clicked() {
                        set(self, json!({ "blend": k }), false);
                    }
                }
            });
            ui.end_row();
        });
        if mask.is_none() {
            return;
        }
        ui.add_space(4.0);
        let mut cut = mask.cut;
        if ui.checkbox(&mut cut, "Cut the shape out").on_hover_text("Everything but the shape shown, instead of only the shape").changed() {
            // Cut out at its whole size it would hide everything: start at half its size.
            let whole = mask.w >= 0.99 && mask.h >= 0.99;
            let p = if cut && whole { json!({"maskCut": true, "maskArea": [mask.x, mask.y, 0.5, 0.5]}) } else { json!({ "maskCut": cut }) };
            set(self, p, false);
        }
        let area = [mask.x, mask.y, mask.w, mask.h];
        for (k, (name, lo, hi)) in
            [("Shape across", -50.0, 150.0), ("Shape down", -50.0, 150.0), ("Shape width", 5.0, 200.0), ("Shape height", 5.0, 200.0)]
                .into_iter()
                .enumerate()
        {
            let was = area.get(k).copied().unwrap_or(0.0) * 100.0;
            let mut v = was;
            let r = super::named_slider(ui, name, &mut v, lo..=hi, "%");
            let mut new = area;
            if let Some(a) = new.get_mut(k) {
                *a = v / 100.0;
            }
            self.look_change(&r, (v - was).abs() > 0.4, json!({ "maskArea": new }), set);
        }
    }

    /// A change of an item's look from a slider: a drag is one undo step. (A slider rounds what
    /// it's given: only a move is a change.)
    fn look_change(&mut self, r: &egui::Response, moved: bool, p: serde_json::Value, set: &dyn Fn(&mut VideoView, serde_json::Value, bool)) {
        if r.drag_started() {
            self.session.begin("Change");
        }
        if r.changed() && moved {
            set(self, p, r.dragged());
        }
        if r.drag_stopped() {
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

    /// The way the chosen item goes on the picture (screen points), when it has two keyframes or
    /// more: the line (a curve, along a path) through its keyframes' places, and those places.
    fn chosen_path(&self, pic: Rect) -> Option<(Vec<Pos2>, Vec<Pos2>)> {
        let (layer, id) = self.item?;
        let item = self.session.timeline.item(layer, id)?;
        if item.keys.len() < 2 {
            return None;
        }
        let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
        let f = pic.width() / vw.max(1.0);
        let on = |p: layers::Placement| pic.min + vec2(p.x * vw * f, p.y * vh * f);
        let (a, b) = (item.keys.first()?.at, item.keys.last()?.at);
        let steps = 64u64;
        let curve = (0..=steps).map(|k| on(item.place_at(item.at.saturating_add(a + (b - a) * k / steps)))).collect();
        let points = item.keys.iter().map(|k| on(k.place)).collect();
        Some((curve, points))
    }

    /// The pointer's redrawn path the playhead is in, on the picture shown in `pic` (screen points):
    /// which it is, its way and its points; and how a point of the picture goes back to the
    /// screen's own pixels (an offset and a scale).
    #[allow(clippy::type_complexity)]
    pub(super) fn pointer_path_on_picture(&self, pic: Rect) -> Option<(usize, Vec<Pos2>, Vec<Pos2>, (Pos2, Vec2))> {
        let tl = &self.session.timeline;
        let i = tl.find(StreamKind::Screen)?;
        let src = tl.source_time(i, self.playhead)?;
        let n = tl.pointer_paths.iter().position(|p| p.from <= src && src <= p.to)?;
        let path = tl.pointer_paths.get(n)?;
        let s = self.rec.stream(tl.tracks.get(i)?.stream)?;
        let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
        let f = pic.width() / vw.max(1.0);
        let on = |x: f32, y: f32| {
            let (px, py) = screengrabcraft_media::compose::screen_point(tl, (s.width, s.height), self.playhead, (x, y), (vw, vh), self.composer.zoom);
            pic.min + vec2(px * f, py * f)
        };
        let steps = 64u64;
        let way = (0..=steps)
            .filter_map(|k| screengrabcraft_media::pointer::path_at(&tl.pointer_paths, path.from + (path.to - path.from) * k / steps))
            .map(|(x, y)| on(x, y))
            .collect();
        let points = path.points.iter().map(|q| on(q.x, q.y)).collect();
        // The mapping is a scale and a shift: two points of the screen say both.
        let (a, b) = (on(0.0, 0.0), on(1000.0, 1000.0));
        let scale = vec2(((b.x - a.x) / 1000.0).max(1.0e-4), ((b.y - a.y) / 1000.0).max(1.0e-4));
        Some((n, way, points, (a, scale)))
    }

    /// Where the chosen item's pinned corners are on the picture (screen points), and where they'd
    /// be unpinned (its box's corners), when it's pinned: top left, top right, bottom right, bottom
    /// left.
    fn pin_corners(&self, pic: Rect) -> Option<([Pos2; 4], [Pos2; 4])> {
        let (layer, id) = self.item?;
        let item = self.session.timeline.item(layer, id)?.placed_at(self.playhead).into_owned();
        if !item.look.pinned {
            return None;
        }
        let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
        let r = layer_draw::item_rect(&item, &self.session.timeline, self.library.as_ref(), vw, vh)?;
        let f = pic.width() / vw.max(1.0);
        let box_ = [pos2(r.x, r.y), pos2(r.x + r.w, r.y), pos2(r.x + r.w, r.y + r.h), pos2(r.x, r.y + r.h)];
        let base = box_.map(|p| pic.min + vec2(p.x * f, p.y * f));
        let pin = item.look.clone().checked().pin;
        let mut moved = base;
        for (k, m) in moved.iter_mut().enumerate() {
            let [dx, dy] = pin.get(k).copied().unwrap_or([0.0; 2]);
            *m += vec2(dx * vw * f, dy * vh * f);
        }
        Some((moved, base))
    }

    /// Where something chosen is on the picture shown in `pic` (screen points): the item chosen,
    /// or the camera.
    fn chosen_rect(&self, pic: Rect) -> Option<Rect> {
        let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
        let to_screen = |r: screengrabcraft_model::Rect| {
            let f = pic.width() / vw.max(1.0);
            Rect::from_min_size(pic.min + vec2(r.x * f, r.y * f), vec2(r.w * f, r.h * f))
        };
        if let Some((layer, id)) = self.item {
            let item = self.session.timeline.item(layer, id)?.placed_at(self.playhead);
            let r = layer_draw::item_rect(&item, &self.session.timeline, self.library.as_ref(), vw, vh)?;
            return Some(to_screen(r));
        }
        if self.camera_chosen {
            let (x, y, w, h) = self.camera_place()?;
            return Some(to_screen(screengrabcraft_model::Rect::new(x, y, w, h)));
        }
        None
    }

    /// The chosen camera's track (the first, unless another was chosen).
    pub(super) fn chosen_camera(&self) -> Option<usize> {
        let tl = &self.session.timeline;
        self.camera_track.filter(|&i| tl.tracks.get(i).is_some_and(|t| t.kind == StreamKind::Camera)).or_else(|| tl.main_camera())
    }

    /// The chosen camera's place on the video (pixels), if it's on the picture.
    fn camera_place(&self) -> Option<(f32, f32, f32, f32)> {
        self.camera_place_of(self.chosen_camera()?)
    }

    /// Camera track `i`'s place on the video (pixels), if it's on the picture: shown, and over
    /// something (without the screen the first camera fills the picture).
    fn camera_place_of(&self, i: usize) -> Option<(f32, f32, f32, f32)> {
        let tl = &self.session.timeline;
        let track = tl.tracks.get(i)?;
        let on_something = tl.find(StreamKind::Screen).is_some() || tl.main_camera() != Some(i);
        if track.muted || !on_something {
            return None;
        }
        let s = self.rec.stream(track.stream)?;
        let aspect = s.width as f32 / s.height.max(1) as f32;
        let layout = tl.camera_layout(i)?;
        Some(screengrabcraft_media::compose::camera_rect(&layout, aspect, self.composer.width as f32, self.composer.height as f32))
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
                Some((l.id, i.id, layer_draw::item_rect(&i.placed_at(t), tl, self.library.as_ref(), vw, vh)?))
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
        // The way it goes, through its keyframes' places.
        if let Some((line, points)) = self.chosen_path(pic) {
            let way = Stroke::new(2.0, Color32::from_rgb(0xFF, 0xD5, 0x4F));
            ui.painter().add(egui::Shape::dashed_line(&line, way, 6.0, 4.0));
            for p in points {
                ui.painter().circle(p, 4.5, Color32::from_rgb(0xFF, 0xD5, 0x4F), Stroke::new(1.0, Color32::BLACK));
            }
        }
        // The pointer's redrawn path the playhead is in: its way, and its points to drag.
        let pointer_path = self.pointer_path_on_picture(pic);
        if let Some((_, way, points, _)) = &pointer_path {
            let colour = Color32::from_rgb(0x4F, 0xC3, 0xF7);
            ui.painter().add(egui::Shape::dashed_line(way, Stroke::new(2.0, colour), 6.0, 4.0));
            for p in points {
                ui.painter().circle(*p, 5.5, colour, Stroke::new(1.5, Color32::WHITE));
            }
        }
        let pins = self.pin_corners(pic);
        if let Some(r) = chosen.filter(|_| pins.is_none()) {
            ui.painter().rect_stroke(r, 0.0, Stroke::new(1.5, t.accent), StrokeKind::Outside);
            for c in corners(r) {
                ui.painter().rect_filled(Rect::from_center_size(c, vec2(9.0, 9.0)), 2.0, Color32::WHITE);
                ui.painter().rect_stroke(Rect::from_center_size(c, vec2(9.0, 9.0)), 2.0, Stroke::new(1.0, t.accent), StrokeKind::Outside);
            }
        }
        // A pinned picture: its four corners, round, to drag anywhere.
        if let Some((moved, _)) = pins {
            ui.painter().add(egui::Shape::closed_line(moved.to_vec(), Stroke::new(1.0, t.accent)));
            for c in moved {
                ui.painter().circle(c, 6.0, Color32::WHITE, Stroke::new(1.5, t.accent));
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
        // What's under the pointer: the topmost item (Ok), else the topmost camera (Err, its track).
        let pick = |s: &VideoView, p: Pos2| -> Option<Result<(u32, u32), usize>> {
            let v = video_at(p);
            if let Some((l, i, _)) =
                s.items_on_picture().into_iter().find(|(_, _, r)| r.x <= v.x && v.x <= r.x + r.w && r.y <= v.y && v.y <= r.y + r.h)
            {
                return Some(Ok((l, i)));
            }
            s.session.timeline.camera_tracks().into_iter().rev().find_map(|k| {
                let (x, y, w, h) = s.camera_place_of(k)?;
                (x <= v.x && v.x <= x + w && y <= v.y && v.y <= y + h).then_some(Err(k))
            })
        };
        if resp.clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            match pick(self, p) {
                Some(Ok(sel)) => {
                    self.item = Some(sel);
                    self.camera_chosen = false;
                }
                Some(Err(k)) => {
                    self.item = None;
                    self.camera_chosen = true;
                    self.camera_track = Some(k);
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
            // A point of the pointer's redrawn path moves that point.
            let way_point = pointer_path.as_ref().and_then(|(n, _, points, _)| Some((*n, points.iter().position(|c| c.distance(p) <= HANDLE)?)));
            if let Some((path, point)) = way_point {
                self.session.begin("Move the Pointer");
                self.canvas_drag = Some(CanvasDrag::PointerPoint { path, point });
            }
            // A pinned picture's corner moves that corner.
            let pin_corner = pins.and_then(|(moved, _)| moved.iter().position(|c| c.distance(p) <= HANDLE)).filter(|_| way_point.is_none());
            if let (Some(corner), Some((layer, item))) = (pin_corner, self.item) {
                self.session.begin("Pin Corner");
                self.canvas_drag = Some(CanvasDrag::Pin { layer, item, corner });
            }
            // A corner of what's chosen resizes it; inside it, or on another, moves.
            let mut target = match chosen {
                _ if pin_corner.is_some() || way_point.is_some() => None,
                Some(r) if pins.is_none() && corners(r).iter().any(|c| c.distance(p) <= HANDLE) => Some((true, r)),
                Some(r) if r.contains(p) => Some((false, r)),
                _ => None,
            };
            if target.is_none()
                && pin_corner.is_none()
                && way_point.is_none()
                && let Some(sel) = pick(self, p)
            {
                self.item = sel.ok();
                self.camera_chosen = sel.is_err();
                self.camera_track = sel.err().or(self.camera_track);
                target = self.chosen_rect(pic).map(|r| (false, r));
            }
            if let Some((resize, r)) = target {
                let centre = video_at(r.center());
                let grab = video_at(p) - centre;
                let dist = grab.length().max(4.0);
                if let Some((layer, item)) = self.item {
                    let place = self.session.timeline.item(layer, item).map(|i| i.place_at(self.playhead)).unwrap_or_default();
                    let w = place.w;
                    // Moved by as far as the pointer goes, from where it is (a broadcast graphic's
                    // place is how far it is from where it was designed, not its middle).
                    let (vw, vh) = (self.composer.width as f32, self.composer.height as f32);
                    let grab = video_at(p) - pos2(place.x * vw, place.y * vh);
                    self.session.begin(if resize { "Resize" } else { "Move" });
                    self.canvas_drag = Some(CanvasDrag::Item { layer, item, grab, resize: resize.then_some((w, dist)) });
                } else if self.camera_chosen
                    && let Some(track) = self.chosen_camera()
                {
                    self.session.begin(if resize { "Resize Camera" } else { "Move Camera" });
                    let size = self.session.timeline.camera_layout(track).map_or(0.22, |c| c.size);
                    self.canvas_drag = Some(CanvasDrag::Camera { track, grab, resize: resize.then_some((size, dist)) });
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
                CanvasDrag::PointerPoint { path, point } => {
                    // The point under the pointer, in the screen's own pixels.
                    if let Some((_, _, _, (origin, scale))) = self.pointer_path_on_picture(pic) {
                        let (x, y) = ((p.x - origin.x) / scale.x, (p.y - origin.y) / scale.y);
                        let _ = self.session.drag("video.pointerPoint", &json!({"path": path, "point": point, "x": x, "y": y}));
                    }
                }
                CanvasDrag::Pin { layer, item, corner } => {
                    // That corner moved from where its box has it to the pointer.
                    if let Some((_, base)) = self.pin_corners(pic)
                        && let Some(b) = base.get(corner)
                        && let Some(it) = self.session.timeline.item(layer, item)
                    {
                        let mut pin = it.look.pin;
                        if let Some(c) = pin.get_mut(corner) {
                            *c = [(p.x - b.x) / f / vw, (p.y - b.y) / f / vh];
                        }
                        let _ = self.session.drag("video.setItem", &json!({"layer": layer, "item": item, "pin": pin}));
                    }
                }
                CanvasDrag::Item { layer, item, grab, resize } => {
                    // With keyframes, it's the keyframe at the playhead that's moved (made there
                    // if there's none).
                    let key_at = self.session.timeline.item(layer, item).filter(|i| !i.keys.is_empty()).map(|i| self.playhead.clamp(i.at, i.end()));
                    let place = |s: &mut VideoView, mut params: serde_json::Value| {
                        let cmd = match (key_at, params.as_object_mut()) {
                            (Some(at), Some(o)) => {
                                o.insert("at".into(), json!(at));
                                "video.keyframe"
                            }
                            _ => "video.setItem",
                        };
                        let _ = s.session.drag(cmd, &params);
                    };
                    let frozen = self.session.timeline.item(layer, item).is_some_and(|i| matches!(i.what, What::Freeze { .. }));
                    let params = match resize {
                        // A frozen region: the corner where the pointer is, its middle staying put.
                        Some(_) if frozen => {
                            let centre = self.chosen_rect(pic).map(|r| video_at(r.center())).unwrap_or(v);
                            json!({"layer": layer, "item": item, "w": 2.0 * (v.x - centre.x).abs() / vw, "h": 2.0 * (v.y - centre.y).abs() / vh})
                        }
                        Some((w0, d0)) => {
                            let centre = self.chosen_rect(pic).map(|r| video_at(r.center())).unwrap_or(v);
                            json!({"layer": layer, "item": item, "w": w0 * ((v - centre).length() / d0)})
                        }
                        None => json!({"layer": layer, "item": item, "x": (v.x - grab.x) / vw, "y": (v.y - grab.y) / vh}),
                    };
                    place(self, params);
                    if resize.is_none() {
                        let (x, y) = catch(self);
                        if x.is_some() || y.is_some() {
                            let (dx, dy) = (x.map_or(0.0, |c| c.0) / f, y.map_or(0.0, |c| c.0) / f);
                            let params = json!({"layer": layer, "item": item, "x": (v.x - grab.x + dx) / vw, "y": (v.y - grab.y + dy) / vh});
                            place(self, params);
                            lines = (x.map(|c| c.1), y.map(|c| c.1));
                        }
                    }
                }
                CanvasDrag::Camera { track, grab, resize } => {
                    let centre = self.camera_place_of(track).map(|(x, y, w, h)| pos2(x + w / 2.0, y + h / 2.0)).unwrap_or(v);
                    let params = match resize {
                        Some((s0, d0)) => json!({"x": centre.x / vw, "y": centre.y / vh, "size": s0 * ((v - centre).length() / d0), "track": track}),
                        None => json!({"x": (v.x - grab.x) / vw, "y": (v.y - grab.y) / vh, "track": track}),
                    };
                    let _ = self.session.drag("video.cameraAt", &params);
                    if resize.is_none() {
                        let (x, y) = catch(self);
                        if x.is_some() || y.is_some() {
                            let (dx, dy) = (x.map_or(0.0, |c| c.0) / f, y.map_or(0.0, |c| c.0) / f);
                            let params = json!({"x": (v.x - grab.x + dx) / vw, "y": (v.y - grab.y + dy) / vh, "track": track});
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
        What::Media { .. } if item.sound_only => Color32::from_rgb(0x2F, 0x80, 0xED),
        What::Media { id } => match media.iter().find(|m| m.id == *id).map(|m| m.kind) {
            Some(MediaKind::Sound) => Color32::from_rgb(0x2F, 0x80, 0xED),
            Some(MediaKind::Video) => Color32::from_rgb(0x6E, 0x5A, 0xD6),
            _ => Color32::from_rgb(0x1F, 0x9D, 0x8B),
        },
        What::Text { .. } => Color32::from_rgb(0xD9, 0x7B, 0x29),
        What::Transition { .. } => Color32::from_rgb(0x8A, 0x8A, 0x96),
        What::Zoom => Color32::from_rgb(0x3B, 0x8E, 0xD0),
        What::Freeze { .. } => Color32::from_rgb(0x5B, 0xA8, 0xC8),
    }
}

#[cfg(test)]
impl VideoView {
    /// The video's picture at `t`, as it's made (for tests).
    pub fn frame_at(&mut self, t: u64) -> Raster {
        self.composer.frame(&self.session.timeline, t)
    }

    /// Whether the picture shown has the zooms in it (not while one is being placed).
    pub fn zooms_shown(&self) -> bool {
        self.composer.zoom
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

    /// A transition's card picture (for tests: each a different picture).
    pub fn blend_card_picture(kind: screengrabcraft_media::timeline::Blend) -> Option<Raster> {
        super::transitions::card_picture(kind)
    }
}
