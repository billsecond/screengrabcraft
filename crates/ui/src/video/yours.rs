//! The Library's things of the owner's own: templates (the layers' items and their look kept under
//! a name, dropped on another recording like any card) and themes (colours and a face for the
//! titles, lower thirds and callouts: a few that come with the program, and the owner's). The host
//! keeps both, a file each, in the settings folder; the chosen theme's name is in the editor's
//! settings.

use egui::{Color32, RichText, Ui};
use screengrabcraft_media::layer_draw;
use screengrabcraft_media::layers::{self, TextFont};
use screengrabcraft_media::yours::{self, OwnTemplate, VideoTheme};
use screengrabcraft_model::Raster;
use serde_json::json;

use super::VideoView;
use super::library::{LibDrop, backdrop, texture};
use crate::editor::Editor;
use crate::icons::Icon;
use crate::services::{Kept, NOT_KEPT};
use crate::theme::Tokens;

/// The Library's section of templates of the owner's.
pub const YOURS: &str = "Yours";
/// The Library's section of themes.
pub const THEMES: &str = "Themes";

/// The owner's templates and themes as the host keeps them, and the theme in use.
#[derive(Default)]
pub struct Yours {
    /// Read from the host yet.
    loaded: bool,
    pub templates: Vec<OwnTemplate>,
    /// The owner's themes (those that come with the program aren't kept).
    pub themes: Vec<VideoTheme>,
    /// The name being typed for a template, and for a theme.
    pub template_name: String,
    pub theme_name: String,
    /// The theme in use, as chosen and perhaps changed since (none: each as it was made).
    pub theme: Option<VideoTheme>,
    /// It's been changed since it was chosen.
    pub changed: bool,
}

/// The moment of a template its card shows: where most of its items are on together.
fn busiest(t: &OwnTemplate) -> u64 {
    let items: Vec<&layers::Item> = t.layers.iter().flat_map(|l| l.items.iter()).collect();
    items.iter().map(|i| i.at + i.len / 2).max_by_key(|m| items.iter().filter(|i| i.at <= *m && *m < i.end()).count()).unwrap_or(0)
}

/// A template of the owner's as its card shows it: its items over the made-up screen (its
/// pictures aren't read for the card), a third the size it's drawn at.
fn own_thumbnail(t: &OwnTemplate) -> Option<Raster> {
    let (w, h) = (768u32, 432u32);
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
    };
    tl.use_template(t, 0).ok()?;
    layer_draw::draw_layers(&mut pic, &tl, &layer_draw::NoMedia, busiest(t), &mut Default::default(), &mut Default::default());
    screengrabcraft_media::compose::scale(&pic, w / 3, h / 3)
}

fn colour_of(hex: &str) -> Color32 {
    let [r, g, b] = layers::color(hex, [0x0b, 0x5f, 0xff]);
    Color32::from_rgb(r, g, b)
}

fn hex_of(c: Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

impl VideoView {
    /// The owner's templates and themes read from the host (once), and the theme the settings
    /// name chosen.
    pub(super) fn load_yours(&mut self, ed: &Editor) {
        if self.yours.loaded {
            return;
        }
        self.yours.loaded = true;
        self.read_kept(ed);
        let name = ed.settings.video_theme.clone();
        self.yours.theme = self.all_themes().into_iter().find(|t| !name.is_empty() && t.name == name);
    }

    /// The templates and themes the host keeps, read again.
    fn read_kept(&mut self, ed: &Editor) {
        let text = |b: &[u8]| String::from_utf8_lossy(b).to_string();
        let mut templates: Vec<OwnTemplate> =
            ed.services.kept(Kept::VideoTemplates).iter().take(yours::MAX_KEPT).filter_map(|(_, b)| OwnTemplate::from_json(&text(b))).collect();
        templates.dedup_by(|a, b| a.name == b.name);
        let mut themes: Vec<VideoTheme> =
            ed.services.kept(Kept::VideoThemes).iter().take(yours::MAX_KEPT).filter_map(|(_, b)| VideoTheme::from_json(&text(b))).collect();
        themes.dedup_by(|a, b| a.name == b.name);
        self.yours.templates = templates;
        self.yours.themes = themes;
        self.thumbs.retain(|k, _| !k.starts_with("yours-"));
    }

    /// The themes there are: those that come with the program, then the owner's.
    fn all_themes(&self) -> Vec<VideoTheme> {
        let mut all = yours::built_in();
        all.extend(self.yours.themes.iter().cloned());
        all
    }

    /// The theme in use, as `video.place` and `video.applyTheme` take it (none: as each was made).
    pub(super) fn theme_param(&self) -> Option<serde_json::Value> {
        self.yours.theme.as_ref().and_then(|t| serde_json::to_value(t).ok())
    }

    /// A library item as it will be placed: in the theme in use.
    pub(super) fn themed(&self, tpl: &layers::Template) -> layers::Template {
        let mut t = tpl.clone();
        if let Some(th) = &self.yours.theme {
            th.restyle(&mut t.item);
        }
        t
    }

    /// Another theme in use (or none): the library's cards are drawn in it again, and the settings
    /// keep its name when it was chosen as it is.
    fn set_theme(&mut self, ed: &mut Editor, theme: Option<VideoTheme>, chosen: bool) {
        self.yours.theme = theme;
        self.thumbs.retain(|k, _| !k.starts_with("template-"));
        if chosen {
            self.yours.changed = false;
            ed.settings.video_theme = self.yours.theme.as_ref().map(|t| t.name.clone()).unwrap_or_default();
            ed.services.save_settings(&ed.settings);
        } else {
            self.yours.changed = true;
        }
    }

    /// The layers' items (or just the one chosen) kept as a template under the name typed.
    pub fn save_template(&mut self, ed: &mut Editor, ctx: &egui::Context, just_chosen: bool) {
        let name = yours::clean_name(&self.yours.template_name, "");
        if name.is_empty() {
            return ed.error(ctx, "Type a name for the template first.".to_string());
        }
        let mut p = json!({ "name": name });
        if just_chosen && let (Some((layer, item)), Some(o)) = (self.item, p.as_object_mut()) {
            o.insert("items".into(), json!([[layer, item]]));
        }
        let made = match self.session.execute("video.makeTemplate", &p) {
            Ok(v) => v,
            Err(e) => return ed.error(ctx, e.to_string()),
        };
        let Some(t) = made.get("template").cloned().and_then(|v| serde_json::from_value::<OwnTemplate>(v).ok()) else { return };
        match ed.services.keep(Kept::VideoTemplates, &t.name, t.to_json().as_bytes()) {
            Ok(()) => {
                let n = t.item_count();
                ed.toast(ctx, format!("Kept \u{201c}{}\u{201d}: {n} {}", t.name, if n == 1 { "item" } else { "items" }));
                self.yours.template_name.clear();
                self.read_kept(ed);
            }
            Err(e) => ed.error(ctx, format!("The template couldn't be kept: {e}")),
        }
    }

    /// A template of the owner's put on layers of its own from `at`.
    pub fn use_template(&mut self, ed: &mut Editor, ctx: &egui::Context, name: &str, at: u64) {
        let Some(t) = self.yours.templates.iter().find(|t| t.name == name).cloned() else { return };
        let Ok(template) = serde_json::to_value(&t) else { return };
        match self.session.execute("video.useTemplate", &json!({"template": template, "at": at})) {
            Ok(_) => {
                self.item = None;
                self.selected = None;
                self.camera_chosen = false;
            }
            Err(e) => ed.error(ctx, e.to_string()),
        }
    }

    fn delete_template(&mut self, ed: &mut Editor, ctx: &egui::Context, name: &str) {
        match ed.services.forget(Kept::VideoTemplates, name) {
            Ok(()) => self.read_kept(ed),
            Err(e) => ed.error(ctx, e),
        }
    }

    /// Every title, lower third, callout and graphic on the video in the theme in use.
    pub fn apply_theme(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let Some(theme) = self.theme_param() else { return };
        match self.session.execute("video.applyTheme", &json!({ "theme": theme })) {
            Ok(v) => {
                let n = v.get("items").and_then(serde_json::Value::as_u64).unwrap_or(0);
                ed.toast(ctx, if n == 0 { "Everything is in the theme already".to_string() } else { format!("{n} put in the theme") });
            }
            Err(e) => ed.error(ctx, e.to_string()),
        }
    }

    /// The theme in use kept as one of the owner's under the name typed, and chosen.
    pub fn save_theme(&mut self, ed: &mut Editor, ctx: &egui::Context) {
        let name = yours::clean_name(&self.yours.theme_name, "");
        let Some(mut theme) = self.yours.theme.clone() else { return };
        if name.is_empty() {
            return ed.error(ctx, "Type a name for the theme first.".to_string());
        }
        if yours::built_in().iter().any(|t| t.name.eq_ignore_ascii_case(&name)) {
            return ed.error(ctx, format!("\u{201c}{name}\u{201d} is the name of a theme that comes with the program: choose another."));
        }
        theme.name = name;
        match ed.services.keep(Kept::VideoThemes, &theme.name, theme.to_json().as_bytes()) {
            Ok(()) => {
                ed.toast(ctx, format!("Kept the theme \u{201c}{}\u{201d}", theme.name));
                self.yours.theme_name.clear();
                self.read_kept(ed);
                self.set_theme(ed, Some(theme), true);
            }
            Err(e) => ed.error(ctx, format!("The theme couldn't be kept: {e}")),
        }
    }

    fn delete_theme(&mut self, ed: &mut Editor, ctx: &egui::Context, name: &str) {
        match ed.services.forget(Kept::VideoThemes, name) {
            Ok(()) => {
                self.read_kept(ed);
                self.set_theme(ed, None, true);
            }
            Err(e) => ed.error(ctx, e),
        }
    }

    // ---- the Library's sections ----

    /// Yours: a name for a template and the buttons that keep one, then a card for each kept.
    pub(super) fn yours_section(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context, wanted: &str) {
        let t = Tokens::get(ctx);
        let can = ed.services.can_keep();
        if wanted.is_empty() {
            ui.label(RichText::new("Keep what's on the layers as a template, to drop on another recording.").size(12.0).color(t.muted));
            ui.add_space(4.0);
            ui.add_enabled(can, egui::TextEdit::singleline(&mut self.yours.template_name).hint_text("Template name").desired_width(f32::INFINITY))
                .on_disabled_hover_text(NOT_KEPT);
            ui.add_space(4.0);
            let any = self.session.timeline.layers.iter().any(|l| !l.items.is_empty());
            let named = !self.yours.template_name.trim().is_empty();
            ui.horizontal(|ui| {
                if ui
                    .add_enabled(can && any && named, egui::Button::new("Save the Layers"))
                    .on_hover_text("Every item on the layers, timed from the first, with their look")
                    .on_disabled_hover_text(if !can {
                        NOT_KEPT
                    } else if !any {
                        "There's nothing on the layers yet"
                    } else {
                        "Type a name first"
                    })
                    .clicked()
                {
                    self.save_template(ed, ctx, false);
                }
                if ui
                    .add_enabled(can && self.item.is_some() && named, egui::Button::new("Save the Chosen Item"))
                    .on_hover_text("Just the item chosen on the timeline, with its look")
                    .on_disabled_hover_text(if can { "Choose an item on the timeline and type a name" } else { NOT_KEPT })
                    .clicked()
                {
                    self.save_template(ed, ctx, true);
                }
            });
            ui.add_space(6.0);
            if self.yours.templates.is_empty() {
                ui.label(RichText::new("No templates of your own yet.").size(12.0).color(t.muted));
            }
        }
        let shown: Vec<OwnTemplate> =
            self.yours.templates.iter().filter(|x| wanted.is_empty() || x.name.to_lowercase().contains(wanted)).cloned().collect();
        let mut forget = None;
        for row in shown.chunks(2) {
            ui.horizontal(|ui| {
                for tpl in row {
                    let key = format!("yours-{}", tpl.name);
                    if !self.thumbs.contains_key(&key)
                        && let Some(pic) = own_thumbnail(tpl)
                    {
                        self.thumbs.insert(key.clone(), texture(ctx, &key, &pic));
                    }
                    let tex = self.thumbs.get(&key).cloned();
                    let n = tpl.item_count();
                    let detail = format!("{n} {}, {:.1} s", if n == 1 { "item" } else { "items" }, tpl.length() as f64 / 1e6);
                    let icon = tex.is_none().then_some(Icon::Capture);
                    let (added, resp) = self.card(ui, LibDrop::Yours(tpl.name.clone()), tex.as_ref(), icon, &tpl.name, &detail, None);
                    if added {
                        let at = self.playhead;
                        self.use_template(ed, ctx, &tpl.name, at);
                    }
                    resp.context_menu(|ui| {
                        if ui.button("Delete the Template").clicked() {
                            forget = Some(tpl.name.clone());
                            ui.close();
                        }
                    });
                }
            });
        }
        if let Some(name) = forget {
            self.delete_template(ed, ctx, &name);
        }
    }

    /// Themes: the one in use (or none), its colours and face, and keeping it as the owner's.
    pub(super) fn themes_section(&mut self, ed: &mut Editor, ui: &mut Ui, ctx: &egui::Context) {
        let t = Tokens::get(ctx);
        ui.label(RichText::new("Titles, lower thirds and callouts added from the Library take its colours and face.").size(12.0).color(t.muted));
        ui.add_space(4.0);
        let all = self.all_themes();
        let current = match (&self.yours.theme, self.yours.changed) {
            (None, _) => "As each was made".to_string(),
            (Some(th), false) => th.name.clone(),
            (Some(th), true) => format!("{} (changed)", th.name),
        };
        let mut pick: Option<Option<VideoTheme>> = None;
        ui.horizontal(|ui| {
            ui.label(RichText::new("Theme").size(12.0).color(t.muted));
            egui::ComboBox::from_id_salt("video-theme").width(190.0).selected_text(current).show_ui(ui, |ui| {
                if ui.selectable_label(self.yours.theme.is_none(), "As each was made").clicked() {
                    pick = Some(None);
                }
                for th in &all {
                    let on = self.yours.theme.as_ref().is_some_and(|c| c.name == th.name) && !self.yours.changed;
                    if ui.selectable_label(on, &th.name).clicked() {
                        pick = Some(Some(th.clone()));
                    }
                }
            });
        });
        if let Some(p) = pick {
            self.set_theme(ed, p, true);
        }
        let Some(theme) = self.yours.theme.clone() else { return };
        ui.add_space(4.0);
        let mut changed = theme.clone();
        egui::Grid::new("video-theme-look").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
            for (label, value) in [("Bars and cards", &mut changed.main), ("Marks", &mut changed.mark), ("A title's words", &mut changed.words)] {
                ui.label(RichText::new(label).size(12.0).color(t.muted));
                let mut c = colour_of(value);
                if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).changed() {
                    *value = hex_of(c);
                }
                ui.end_row();
            }
            ui.label(RichText::new("Face").size(12.0).color(t.muted));
            egui::ComboBox::from_id_salt("video-theme-font").width(150.0).selected_text(changed.font.label()).show_ui(ui, |ui| {
                for f in TextFont::ALL {
                    if ui.selectable_label(changed.font == f, f.label()).clicked() {
                        changed.font = f;
                    }
                }
            });
            ui.end_row();
        });
        if changed != theme {
            self.set_theme(ed, Some(changed), false);
        }
        ui.add_space(6.0);
        let anything = self.session.timeline.layers.iter().flat_map(|l| l.items.iter()).any(|i| matches!(i.what, layers::What::Text { .. }));
        if ui
            .add_enabled(anything, egui::Button::new("Apply to This Video"))
            .on_hover_text("Every title, lower third, callout and graphic on the video in its colours and face (one undo step)")
            .on_disabled_hover_text("There are no titles or callouts on this video yet")
            .clicked()
        {
            self.apply_theme(ed, ctx);
        }
        ui.add_space(6.0);
        let can = ed.services.can_keep();
        ui.horizontal(|ui| {
            ui.add_enabled(can, egui::TextEdit::singleline(&mut self.yours.theme_name).hint_text("Theme name").desired_width(140.0))
                .on_disabled_hover_text(NOT_KEPT);
            let named = !self.yours.theme_name.trim().is_empty();
            if ui
                .add_enabled(can && named, egui::Button::new("Save as Yours"))
                .on_hover_text("Keep these colours and this face as a theme of your own")
                .on_disabled_hover_text(if can { "Type a name first" } else { NOT_KEPT })
                .clicked()
            {
                self.save_theme(ed, ctx);
            }
        });
        let own = self.yours.themes.iter().any(|x| x.name == theme.name) && !self.yours.changed;
        if own && ui.button("Delete This Theme").on_hover_text("Let go of it; what's on videos stays as it is").clicked() {
            self.delete_theme(ed, ctx, &theme.name);
        }
    }
}
