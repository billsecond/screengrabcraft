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

/// The picture under the library's thumbnails: a calm gradient.
fn backdrop(w: u32, h: u32) -> Option<Raster> {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32);
            px.extend_from_slice(&[(40.0 + 50.0 * fx) as u8, (52.0 + 30.0 * fy) as u8, (90.0 + 60.0 * (1.0 - fx)) as u8, 255]);
        }
    }
    Raster::new(w, h, px).ok()
}

/// A template drawn as its card shows it.
fn template_thumbnail(t: &layers::Template) -> Option<Raster> {
    let (w, h) = (256u32, 144u32);
    let mut pic = backdrop(w, h)?;
    let mut tl = screengrabcraft_media::timeline::Timeline {
        tracks: Vec::new(),
        camera: Default::default(),
        captions: Default::default(),
        layers: Vec::new(),
        media: Vec::new(),
        pointer: Default::default(),
    };
    // A broadcast graphic: shown close up, as it is once it has come on (a transition, part-way).
    if let What::Text { style, title, subtitle, color } = &t.item.what
        && screengrabcraft_media::stock::is_stock(*style)
    {
        return screengrabcraft_media::stock::card(
            *style,
            title,
            subtitle,
            layers::color(color, [0x0b, 0x5f, 0xff]),
            t.item.len,
            t.item.place.w,
            &pic,
        );
    }
    // Words drawn larger than on a video, so they can be read on a card.
    let mut item = t.item.clone();
    if matches!(item.what, What::Text { .. }) {
        item.place.w = (item.place.w * 2.4).min(3.0);
    }
    let (_, _) = tl.place(item, None, 0).ok()?;
    // Half-way into a transition shows it at its strongest; anything else, once it has come in.
    let at = if matches!(t.item.what, What::Transition { .. }) { t.item.len * 2 / 5 } else { t.item.len / 2 };
    layer_draw::draw_layers(&mut pic, &tl, &layer_draw::NoMedia, at, &mut Default::default(), &mut Default::default());
    Some(pic)
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
        // A section for each kind, opened and closed by its heading (the first is open).
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
                                let key = format!("template-{}", tpl.key);
                                // A card's picture is drawn the first time its section is open.
                                if !self.thumbs.contains_key(&key)
                                    && let Some(pic) = template_thumbnail(tpl)
                                {
                                    self.thumbs.insert(key.clone(), texture(ctx, &key, &pic));
                                }
                                let tex = self.thumbs.get(&key).cloned();
                                let added = self.card(ui, LibDrop::Template(tpl.key), tex.as_ref(), None, tpl.name, "", None);
                                if added {
                                    let at = self.playhead;
                                    self.drop_on_timeline(ed, ctx, &LibDrop::Template(tpl.key), None, at);
                                }
                            }
                        });
                    }
                    ui.add_space(6.0);
                });
            ui.add_space(2.0);
        }
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
                    let added = self.card(ui, LibDrop::Media(m.id), tex.as_ref(), icon, &m.name, &detail, Some(&mut remove));
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
    /// or + adds it at the playhead (true). A file of the bin can be taken out of it (its id goes
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
    ) -> bool {
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
        added
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
            .exact_size(260.0)
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
                    if let What::Text { style, title, subtitle, color } = &item.what {
                        if style.has_words() {
                            ui.label(RichText::new("Words").color(t.muted));
                            let mut w = title.clone();
                            let r = ui.add(egui::TextEdit::multiline(&mut w).desired_rows(2).desired_width(f32::INFINITY));
                            self.text_change(r, || json!({"title": w.clone()}), &set);
                            if style.has_subtitle() {
                                let mut s2 = subtitle.clone();
                                let r = ui.add(egui::TextEdit::singleline(&mut s2).desired_width(f32::INFINITY).hint_text("Second line"));
                                self.text_change(r, || json!({"subtitle": s2.clone()}), &set);
                            }
                            ui.add_space(8.0);
                        }
                        if !matches!(style, layers::TextStyle::Spotlight) {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Colour").color(t.muted));
                                let [r, g, b] = layers::color(color, [0x6e, 0x5a, 0xd6]);
                                let mut c = Color32::from_rgb(r, g, b);
                                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                                    set(self, json!({"color": format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())}), false);
                                }
                            });
                            ui.add_space(8.0);
                        }
                    }
                    if !matches!(item.what, What::Transition { .. }) && item.layer_kind(&media) == layers::LayerKind::Picture {
                        let size_label = if matches!(item.what, What::Text { .. }) { "Size" } else { "Width (share of the video's)" };
                        let (lo, hi) = if matches!(item.what, What::Text { .. }) { (0.3, 2.5) } else { (0.05, 1.0) };
                        self.slider(ui, size_label, item.place.w, lo, hi, "w", &set);
                        self.slider(ui, "Across", item.place.x, -0.5, 1.5, "x", &set);
                        self.slider(ui, "Down", item.place.y, -0.5, 1.5, "y", &set);
                        self.slider(ui, "Seen through it", 1.0 - item.place.opacity, 0.0, 0.9, "see", &set);
                        ui.label(
                            RichText::new("Drag it on the picture to move it (off its edges too); drag a corner to resize it.")
                                .size(11.5)
                                .color(t.muted),
                        );
                        ui.add_space(10.0);
                        for (label, key, anim) in [("Comes in", "intro", item.intro), ("Goes out", "outro", item.outro)] {
                            ui.horizontal(|ui| {
                                ui.label(RichText::new(label).color(t.muted));
                                egui::ComboBox::from_id_salt(("anim", key)).width(150.0).selected_text(anim.kind.label()).show_ui(ui, |ui| {
                                    for k in AnimKind::ALL {
                                        if ui.selectable_label(anim.kind == k, k.label()).clicked() {
                                            set(self, json!({ key: k }), false);
                                        }
                                    }
                                });
                            });
                        }
                    }
                    ui.add_space(14.0);
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
        let t = Tokens::get(ui.ctx());
        ui.label(RichText::new(label).color(t.muted));
        let mut v = value * 100.0;
        let r = ui.add(egui::Slider::new(&mut v, lo * 100.0..=hi * 100.0).suffix("%"));
        let p = |v: f32| if key == "see" { json!({"opacity": 1.0 - v / 100.0}) } else { json!({ key: v / 100.0 }) };
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
                }
                CanvasDrag::Camera { grab, resize } => {
                    let centre = self.camera_place().map(|(x, y, w, h)| pos2(x + w / 2.0, y + h / 2.0)).unwrap_or(v);
                    let params = match resize {
                        Some((s0, d0)) => json!({"x": centre.x / vw, "y": centre.y / vh, "size": s0 * ((v - centre).length() / d0)}),
                        None => json!({"x": (v.x - grab.x) / vw, "y": (v.y - grab.y) / vh}),
                    };
                    let _ = self.session.drag("video.cameraAt", &params);
                }
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
}
