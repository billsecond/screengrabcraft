//! The stamp library in the Stamp tool's panel: every stamp drawn in code (symbols and faces),
//! found by a search, and pictures of your own kept as stamps. Those are kept by the host, in
//! the editor's settings folder ([`crate::services::Services::own_stamps`]): the editor itself
//! keeps no files. A picture of your own is placed as a picture annotation.

use egui::Ui;
use screengrabcraft_model::quick::QuickStyle;
use screengrabcraft_model::{ImageFormat, Kind, Pt, Raster, Rect, Shape, ShapeOptions, StampKind, Style};

use crate::editor::{Editor, Tool};
use crate::quick::{self, Face};
use crate::services::OpenedFile;
use crate::theme::Tokens;

/// What the search box is called.
pub const SEARCH: &str = "Search stamps";
/// The longest side a picture of your own is kept at.
pub const OWN_SIDE: u32 = 512;
/// The most pictures of your own read and kept in memory at once.
const MAX_READ: usize = 64;
/// The most pictures of your own listed.
pub const MAX_OWN: usize = 200;
/// Tiles to a row in the library: smaller than the quick styles', for there are many.
const LIBRARY_COLUMNS: usize = 6;

/// A stamp's look in its own colour, for its tile.
fn look(k: StampKind) -> QuickStyle {
    let base = Kind::Stamp.default_style();
    QuickStyle::new(Kind::Stamp, Style { fill: k.color(), ..base }, ShapeOptions { stamp: Some(k), ..ShapeOptions::default() })
}

/// What a picture of your own is called on its tile.
pub fn own_name(i: usize) -> String {
    format!("Your picture {}", i + 1)
}

/// The library: a search, the stamps it finds (symbols, then faces), then your pictures.
pub(crate) fn library(ed: &mut Editor, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    ui.label(egui::RichText::new("Stamp library").strong());
    ui.add_space(4.0);
    let mut query = ed.stamp_query.clone();
    let resp = ui.add(egui::TextEdit::singleline(&mut query).hint_text("Search: check, arrow, smile…").desired_width(f32::INFINITY));
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, SEARCH));
    if resp.changed() {
        ed.stamp_query = query.chars().take(64).collect();
    }
    ui.add_space(6.0);
    let current = if ed.own_stamp.is_some() { None } else { Some(ed.stamp()) };
    let mut picked = None;
    let mut any = false;
    for (heading, faces) in [("Symbols", false), ("Faces", true)] {
        let found: Vec<StampKind> = StampKind::ALL.into_iter().filter(|k| k.is_face() == faces && k.matches(&ed.stamp_query)).collect();
        if found.is_empty() {
            continue;
        }
        any = true;
        ui.label(egui::RichText::new(heading).size(12.0).color(t.muted));
        quick::grid_of(ui, found.len(), LIBRARY_COLUMNS, |ui, i, size| {
            let Some(k) = found.get(i) else { return };
            let face = Face::Picture(stamp_texture(ed, &ctx, *k));
            if quick::tile(ui, face, k.label(), size, current == Some(*k), None).0 {
                picked = Some(*k);
            }
        });
        ui.add_space(4.0);
    }
    if !any {
        ui.label(egui::RichText::new("No stamp goes by that name.").size(12.0).color(t.muted));
        ui.add_space(4.0);
    }
    if let Some(k) = picked {
        ed.own_stamp = None;
        crate::props::choose_stamp(ed, &ctx, k);
    }
    own(ed, ui);
}

/// Your pictures as stamps, each a tile, and the buttons that add one or take the chosen one
/// out. Greyed out, with the reason, where the host keeps no files (the browser).
fn own(ed: &mut Editor, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    ui.label(egui::RichText::new("Your pictures").size(12.0).color(t.muted));
    let kept = ed.services.keeps_own_stamps();
    if kept {
        let names = own_list(ed);
        let mut picked = None;
        quick::grid_of(ui, names.len(), LIBRARY_COLUMNS, |ui, i, size| {
            let Some(name) = names.get(i) else { return };
            let face = Face::Picture(own_texture(ed, &ctx, name));
            if quick::tile(ui, face, &own_name(i), size, ed.own_stamp.as_deref() == Some(name.as_str()), None).0 {
                picked = Some(name.clone());
            }
        });
        if let Some(name) = picked {
            ed.own_stamp = Some(name);
            if ed.tool != Tool::Draw(Kind::Stamp) {
                ed.set_tool(Tool::Draw(Kind::Stamp));
            }
        }
        if names.is_empty() {
            ui.label(egui::RichText::new("Add a picture (a logo, a signature) to place it as a stamp.").size(12.0).color(t.muted));
        }
    } else {
        ui.label(egui::RichText::new("Pictures of your own are kept as stamps on the computer: not in the browser.").size(12.0).color(t.muted));
    }
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        if ui.add_enabled(kept, egui::Button::new("Add a Picture…")).on_hover_text("Pick a picture file to keep as a stamp").clicked() {
            ed.adding_stamp = true;
            ed.services.request_open();
        }
        if ui.add_enabled(kept, egui::Button::new("Add the Copied Picture")).on_hover_text("Keep the picture on the clipboard as a stamp").clicked() {
            match ed.services.paste_image() {
                Some(pic) => add(ed, &ctx, &pic),
                None => ed.error(&ctx, "There is no picture on the clipboard"),
            }
        }
        let chosen = ed.own_stamp.clone();
        if ui.add_enabled(chosen.is_some(), egui::Button::new("Remove")).on_hover_text("Take the chosen picture out of your stamps").clicked()
            && let Some(name) = chosen
        {
            remove(ed, &ctx, &name);
        }
    });
}

/// The names of your pictures, asked of the host once and kept.
fn own_list(ed: &mut Editor) -> Vec<String> {
    if let Some(names) = &ed.own_stamps {
        return names.clone();
    }
    let mut names = ed.services.own_stamps();
    names.truncate(MAX_OWN);
    ed.own_stamps = Some(names.clone());
    names
}

/// A picture of your own, read from the host once.
pub(crate) fn own_picture(ed: &mut Editor, name: &str) -> Option<Raster> {
    if let Some(p) = ed.own_stamp_pictures.get(name) {
        return Some(p.clone());
    }
    let pic = ed.services.own_stamp(name)?;
    if ed.own_stamp_pictures.len() >= MAX_READ {
        ed.own_stamp_pictures.clear();
    }
    ed.own_stamp_pictures.insert(name.to_string(), pic.clone());
    Some(pic)
}

/// A stamp's tile on the GPU: the stamp large in it, in its own colour.
fn stamp_texture(ed: &mut Editor, ctx: &egui::Context, k: StampKind) -> Option<egui::TextureId> {
    let key = format!("library-stamp|{k:?}");
    if let Some(t) = ed.previews.get(&key) {
        return Some(t.id());
    }
    let (w, h) = quick::PREVIEW;
    let mut doc = screengrabcraft_model::Document::new(Raster::filled(w, h, screengrabcraft_model::Color::rgb(0xFB, 0xFB, 0xFB)).ok()?);
    let side = h as f32 - 8.0;
    let rect = Rect::new((w as f32 - side) / 2.0, 4.0, side, side);
    doc.add(Shape::Stamp { rect, stamp: k }, Style { shadow: false, ..look(k).style }).ok()?;
    let tile = screengrabcraft_render::render(&doc)?;
    let image = egui::ColorImage::from_rgba_unmultiplied([tile.width() as usize, tile.height() as usize], tile.pixels());
    let handle = ctx.load_texture(format!("library-stamp-{}", ed.previews.len()), image, egui::TextureOptions::LINEAR);
    let id = handle.id();
    ed.previews.insert(key, handle);
    Some(id)
}

/// Its tile's picture on the GPU.
fn own_texture(ed: &mut Editor, ctx: &egui::Context, name: &str) -> Option<egui::TextureId> {
    let key = format!("own-stamp|{name}");
    if let Some(t) = ed.previews.get(&key) {
        return Some(t.id());
    }
    let pic = own_picture(ed, name)?;
    // Small, on the paper the other tiles have, kept in proportion.
    let (w, h) = quick::PREVIEW;
    let f = ((w as f32 - 16.0) / pic.width().max(1) as f32).min((h as f32 - 16.0) / pic.height().max(1) as f32).min(1.0);
    let (tw, th) = (((pic.width() as f32 * f).round() as u32).max(1), ((pic.height() as f32 * f).round() as u32).max(1));
    let small = screengrabcraft_render::effects::resize(&pic, tw, th).ok()?;
    let mut doc = screengrabcraft_model::Document::new(Raster::filled(w, h, screengrabcraft_model::Color::rgb(0xFB, 0xFB, 0xFB)).ok()?);
    let key_img = doc.add_image(small).ok()?;
    let rect = Rect::new((w - tw) as f32 / 2.0, (h - th) as f32 / 2.0, tw as f32, th as f32);
    doc.add(Shape::Image { rect, image: key_img }, Style { shadow: false, ..Kind::Image.default_style() }).ok()?;
    let tile = screengrabcraft_render::render(&doc)?;
    let image = egui::ColorImage::from_rgba_unmultiplied([tile.width() as usize, tile.height() as usize], tile.pixels());
    let handle = ctx.load_texture(format!("own-stamp-{}", ed.previews.len()), image, egui::TextureOptions::LINEAR);
    let id = handle.id();
    ed.previews.insert(key, handle);
    Some(id)
}

/// Keeps `pic` as a stamp (no bigger than [`OWN_SIDE`]), and picks it.
pub(crate) fn add(ed: &mut Editor, ctx: &egui::Context, pic: &Raster) {
    let f = (OWN_SIDE as f32 / pic.width().max(1) as f32).min(OWN_SIDE as f32 / pic.height().max(1) as f32).min(1.0);
    let (w, h) = (((pic.width() as f32 * f).round() as u32).max(1), ((pic.height() as f32 * f).round() as u32).max(1));
    let small = if f < 1.0 { screengrabcraft_render::effects::resize(pic, w, h) } else { Ok(pic.clone()) };
    let png =
        small.map_err(|e| e.to_string()).and_then(|p| screengrabcraft_render::codec::encode(&p, ImageFormat::Png, 100).map_err(|e| e.to_string()));
    match png.and_then(|bytes| ed.services.add_own_stamp(&bytes)) {
        Ok(name) => {
            ed.own_stamps = None;
            ed.own_stamp = Some(name);
            ed.set_tool(Tool::Draw(Kind::Stamp));
            ed.toast(ctx, "Kept as a stamp: click the picture to place it");
        }
        Err(e) => ed.error(ctx, format!("The picture couldn't be kept as a stamp: {e}")),
    }
}

/// A picture file picked to keep as a stamp.
pub(crate) fn add_file(ed: &mut Editor, ctx: &egui::Context, f: &OpenedFile) {
    match screengrabcraft_render::codec::decode(&f.bytes) {
        Ok(pic) => add(ed, ctx, &pic),
        Err(_) => ed.error(ctx, format!("{} isn't a picture that can be kept as a stamp", f.name)),
    }
}

/// Takes a picture out of your stamps.
fn remove(ed: &mut Editor, ctx: &egui::Context, name: &str) {
    match ed.services.remove_own_stamp(name) {
        Ok(()) => {
            ed.own_stamps = None;
            ed.own_stamp = None;
            ed.own_stamp_pictures.remove(name);
            ed.previews.remove(&format!("own-stamp|{name}"));
        }
        Err(e) => ed.error(ctx, format!("The picture couldn't be taken out of your stamps: {e}")),
    }
}

/// Places the chosen picture of your own at `p` (its middle), a size that suits the picture,
/// in proportion; selected, and moved by the drag that placed it. False when there is none.
pub(crate) fn place(ed: &mut Editor, ctx: &egui::Context, p: Pt) -> Option<screengrabcraft_model::ObjectId> {
    let name = ed.own_stamp.clone()?;
    let Some(pic) = own_picture(ed, &name) else {
        ed.own_stamp = None;
        ed.own_stamps = None;
        ed.error(ctx, "That picture is no longer among your stamps");
        return None;
    };
    let (dw, dh) = (ed.session.doc.width() as f32, ed.session.doc.height() as f32);
    let side = (dw.min(dh) * 0.2).clamp(48.0, 240.0);
    let (pw, ph) = (pic.width().max(1) as f32, pic.height().max(1) as f32);
    let f = (side / pw.max(ph)).min(1.0).min(dw / pw).min(dh / ph);
    let (w, h) = (pw * f, ph * f);
    let rect = Rect::new(p.x - w / 2.0, p.y - h / 2.0, w, h);
    ed.session.begin("Add Stamp");
    let key = match ed.session.doc.add_image(pic) {
        Ok(k) => k,
        Err(e) => {
            ed.session.cancel();
            ed.error(ctx, e.to_string());
            return None;
        }
    };
    match ed.session.doc.add(Shape::Image { rect, image: key }, Style { shadow: false, ..Kind::Image.default_style() }) {
        Ok(id) => {
            ed.session.doc.bring_inside(id);
            ed.session.select(vec![id]);
            Some(id)
        }
        Err(e) => {
            ed.session.cancel();
            ed.error(ctx, e.to_string());
            None
        }
    }
}
