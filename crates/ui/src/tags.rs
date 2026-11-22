//! Tags and favourites on captures, and the program each was taken from: All captures filters by
//! them (Favorites, a program, a tag) and searches them; a star on a capture makes it a favourite,
//! its tag button lists its tags to change. The same are in the menu a right click on a capture in
//! the strip opens.
//!
//! Every change is a library command (`library.*`), run on the host's library and on the editor's
//! copy of it.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, vec2};
use serde_json::{Value, json};

use crate::editor::Editor;
use crate::icons::{self, Icon};
use crate::services::RecentItem;
use crate::theme::Tokens;

/// A favourite's star.
pub const GOLD: Color32 = Color32::from_rgb(242, 176, 36);
/// The buttons in a capture's corners in All captures.
const CORNER: f32 = 24.0;
/// The most programs and tags offered as filters (the rest are found by searching).
const MAX_CHIPS: usize = 12;

impl Editor {
    /// Runs a library command on the host's library and then on the editor's copy. False (and
    /// the reason said) when the host couldn't.
    pub fn library_command(&mut self, ctx: &egui::Context, id: &str, params: Value) -> bool {
        match self.services.run_library(id, &params) {
            Ok(_) => {
                if let Err(e) = screengrabcraft_engine::library::run(&mut self.library, id, &params) {
                    log::warn!("{id}: {e}");
                }
                true
            }
            Err(e) => {
                self.error(ctx, e);
                false
            }
        }
    }

    /// Makes a capture a favourite, or not.
    pub fn toggle_favorite(&mut self, ctx: &egui::Context, path: &str) {
        let on = !self.library.get(path).favorite;
        self.library_command(ctx, "library.favorite", json!({"path": path, "on": on}));
    }

    /// Adds the tag being typed to the capture being tagged.
    fn add_typed_tag(&mut self, ctx: &egui::Context, path: &str) {
        let tag = std::mem::take(&mut self.new_tag);
        if screengrabcraft_model::library::clean_tag(&tag).is_some() {
            self.library_command(ctx, "library.tag", json!({"path": path, "tag": tag}));
        }
    }
}

/// What a capture's star button says.
pub fn favorite_label(name: &str, on: bool) -> String {
    if on { format!("Remove {name} from Favorites") } else { format!("Add {name} to Favorites") }
}

/// What a capture's tag button says.
pub fn tags_label(name: &str) -> String {
    format!("Tags of {name}")
}

/// A capture's tooltip: its name, then where it was taken from and its tags, if known.
pub fn tooltip(ed: &Editor, item: &RecentItem) -> String {
    let info = ed.library.get(&item.path);
    let mut lines = vec![item.name.clone()];
    match (info.program.is_empty(), info.title.is_empty()) {
        (false, false) if info.title != info.program => lines.push(format!("From {}: {}", info.program, info.title)),
        (false, _) => lines.push(format!("From {}", info.program)),
        (true, false) => lines.push(format!("From {}", info.title)),
        (true, true) => {}
    }
    if !info.tags.is_empty() {
        lines.push(format!("Tags: {}", info.tags.join(", ")));
    }
    lines.join("\n")
}

/// The filters over All captures: Favorites, then the programs and the tags of the captures
/// listed, each a button that shows only those (and again, all of them).
pub(crate) fn filters(ed: &mut Editor, ui: &mut Ui, items: &[RecentItem]) {
    let t = Tokens::get(ui.ctx());
    let paths = || items.iter().map(|i| i.path.as_str());
    let programs = ed.library.programs(paths());
    let tags = ed.library.tags(paths());
    // A filter on what no capture has any more shows nothing: it goes.
    let f = &mut ed.capture_filter;
    if f.program.as_ref().is_some_and(|p| !programs.iter().any(|(n, _)| n.eq_ignore_ascii_case(p))) {
        f.program = None;
    }
    if f.tag.as_ref().is_some_and(|tag| !tags.iter().any(|(n, _)| n.to_lowercase() == tag.to_lowercase())) {
        f.tag = None;
    }
    let favorites = items.iter().filter(|i| ed.library.get(&i.path).favorite).count();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
        let f = &mut ed.capture_filter;
        if ui
            .selectable_label(f.favorites, format!("\u{2605} Favorites ({favorites})"))
            .on_hover_text("Only the captures marked with a star")
            .clicked()
        {
            f.favorites = !f.favorites;
        }
        if !programs.is_empty() {
            ui.label(egui::RichText::new("From").color(t.muted));
            for (name, n) in programs.iter().take(MAX_CHIPS) {
                let on = f.program.as_ref().is_some_and(|p| p.eq_ignore_ascii_case(name));
                if ui.selectable_label(on, format!("{name} ({n})")).on_hover_text(format!("Only the captures taken from {name}")).clicked() {
                    f.program = if on { None } else { Some(name.clone()) };
                }
            }
        }
        if !tags.is_empty() {
            ui.label(egui::RichText::new("Tags").color(t.muted));
            for (name, n) in tags.iter().take(MAX_CHIPS) {
                let on = f.tag.as_ref().is_some_and(|tag| tag.to_lowercase() == name.to_lowercase());
                if ui.selectable_label(on, format!("#{name} ({n})")).on_hover_text(format!("Only the captures tagged {name}")).clicked() {
                    f.tag = if on { None } else { Some(name.clone()) };
                }
            }
        }
    });
}

/// The tags of the capture being tagged, over the captures in All captures: each with a cross to
/// take it off, a field to add one (Enter), the tags of other captures to add with a click, and
/// Done.
pub(crate) fn tag_row(ed: &mut Editor, ui: &mut Ui, items: &[RecentItem]) {
    let Some(path) = ed.tagging.clone() else { return };
    let Some(item) = items.iter().find(|i| i.path == path) else {
        ed.tagging = None;
        return;
    };
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let info = ed.library.get(&path);
    egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border)).corner_radius(6).inner_margin(8).show(ui, |ui| {
        ui.set_width(ui.available_width());
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
            ui.label(egui::RichText::new(tags_label(&item.name)).strong());
            if info.tags.is_empty() {
                ui.label(egui::RichText::new("none yet").color(t.muted));
            }
            for tag in &info.tags {
                if ui.button(format!("{tag}  \u{00d7}")).on_hover_text(format!("Take {tag} off")).clicked() {
                    ed.library_command(&ctx, "library.untag", json!({"path": path, "tag": tag}));
                }
            }
        });
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let field = ui.add(egui::TextEdit::singleline(&mut ed.new_tag).hint_text("Add a tag").desired_width(180.0));
            let entered = field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if entered || ui.button("Add").clicked() {
                ed.add_typed_tag(&ctx, &path);
                field.request_focus();
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                if ui.button("Done").clicked() {
                    ed.tagging = None;
                    ed.new_tag.clear();
                }
            });
        });
        // Tags other captures have, to add with a click.
        let others: Vec<String> =
            ed.library.tags(items.iter().map(|i| i.path.as_str())).into_iter().map(|(n, _)| n).filter(|n| !info.has_tag(n)).take(MAX_CHIPS).collect();
        if !others.is_empty() {
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing = vec2(6.0, 6.0);
                ui.label(egui::RichText::new("Used before").color(t.muted));
                for tag in others {
                    if ui.small_button(format!("+ {tag}")).on_hover_text(format!("Tag it {tag}")).clicked() {
                        ed.library_command(&ctx, "library.tag", json!({"path": path, "tag": tag}));
                    }
                }
            });
        }
    });
    ui.add_space(6.0);
}

/// The star (top left) and the tag button (top right) of a capture in All captures, over its
/// thumbnail `pic`: a favourite's star is always there, the rest under the pointer. True when one
/// of them took the click.
pub(crate) fn tile_buttons(ed: &mut Editor, ui: &mut Ui, pic: egui::Rect, item: &RecentItem, pointed: bool) -> bool {
    let ctx = ui.ctx().clone();
    let favorite = ed.library.get(&item.path).favorite;
    let star = egui::Rect::from_min_size(pic.min + vec2(4.0, 4.0), vec2(CORNER, CORNER));
    let label = favorite_label(&item.name, favorite);
    let resp = ui.interact(star, ui.id().with(("favorite", &item.path)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    if favorite || pointed {
        corner(
            ui,
            star,
            if favorite { Icon::Starred } else { Icon::Favorites },
            if favorite { GOLD } else { Tokens::get(ui.ctx()).text },
            resp.hovered(),
        );
    }
    let starred = resp.on_hover_text(if favorite { "Remove from Favorites" } else { "Add to Favorites" }).clicked();
    if starred {
        ed.toggle_favorite(&ctx, &item.path);
    }
    let tag = egui::Rect::from_min_size(egui::pos2(pic.max.x - CORNER - 4.0, pic.min.y + 4.0), vec2(CORNER, CORNER));
    let label = tags_label(&item.name);
    let resp = ui.interact(tag, ui.id().with(("tags", &item.path)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
    let tagging = ed.tagging.as_deref() == Some(item.path.as_str());
    if pointed || tagging {
        corner(ui, tag, Icon::Tag, Tokens::get(ui.ctx()).text, resp.hovered() || tagging);
    }
    let tagged = resp.on_hover_text("Tags").clicked();
    if tagged {
        ed.tagging = if tagging { None } else { Some(item.path.clone()) };
        ed.new_tag.clear();
    }
    starred || tagged
}

/// A small button drawn in a thumbnail's corner.
fn corner(ui: &Ui, rect: egui::Rect, icon: Icon, ink: Color32, hot: bool) {
    let t = Tokens::get(ui.ctx());
    ui.painter().rect_filled(rect, 5.0, if hot { t.hover } else { t.panel.gamma_multiply(0.92) });
    ui.painter().rect_stroke(rect, 5.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
    icons::paint(ui.painter(), rect.shrink(4.0), icon, ink);
}

/// A favourite's star in a corner of its thumbnail in the strip.
pub(crate) fn strip_star(ed: &Editor, ui: &Ui, thumb: egui::Rect, item: &RecentItem) {
    if ed.library.get(&item.path).favorite {
        let r = egui::Rect::from_min_size(egui::pos2(thumb.max.x - 22.0, thumb.max.y - 22.0), vec2(18.0, 18.0));
        icons::paint(ui.painter(), r.expand(1.5), Icon::Starred, Color32::from_black_alpha(140));
        icons::paint(ui.painter(), r, Icon::Starred, GOLD);
    }
}

/// The menu a right click on a capture in the strip opens: Favorites, and its tags (those of the
/// other captures to tick, and a field for a new one).
pub(crate) fn menu(ed: &mut Editor, ui: &mut Ui, item: &RecentItem, items: &[RecentItem]) {
    let ctx = ui.ctx().clone();
    let info = ed.library.get(&item.path);
    if ui.button(if info.favorite { "Remove from Favorites" } else { "Add to Favorites" }).clicked() {
        ed.toggle_favorite(&ctx, &item.path);
        ui.close();
    }
    ui.separator();
    ui.label(egui::RichText::new("Tags").strong());
    let known: Vec<String> = ed.library.tags(items.iter().map(|i| i.path.as_str())).into_iter().map(|(n, _)| n).take(MAX_CHIPS * 2).collect();
    for tag in known {
        let mut on = info.has_tag(&tag);
        if ui.checkbox(&mut on, &tag).changed() {
            let id = if on { "library.tag" } else { "library.untag" };
            ed.library_command(&ctx, id, json!({"path": item.path, "tag": tag}));
        }
    }
    let field = ui.add(egui::TextEdit::singleline(&mut ed.new_tag).hint_text("New tag").desired_width(160.0));
    if field.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
        ed.add_typed_tag(&ctx, &item.path);
    }
}
