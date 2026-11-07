//! The Quick Styles at the top of the properties panel: ready-made looks for the tool, or for
//! the selected annotations, in a theme, each drawn by the renderer itself, with a star to keep
//! it under Favorites. The Favorites tool shows the starred looks of every tool together.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use screengrabcraft_model::quick::{self, QuickStyle};
use screengrabcraft_model::{Color, Document, Kind, Object, ObjectId, Pt, Raster, Rect, Shape, ShapeOptions, StampKind, Style, TailSpot, TextAlign};
use serde_json::json;

use crate::editor::{Editor, Tool};
use crate::theme::Tokens;

/// A preview's size in pixels; it is shown at about half that in points, so it stays sharp on
/// a high-density screen.
pub(crate) const PREVIEW: (u32, u32) = (108, 80);
/// The most previews kept at once (they're drawn again when needed).
const MAX_PREVIEWS: usize = 400;
/// Tiles to a row.
const COLUMNS: usize = 4;
/// The room between tiles.
const GAP: f32 = 6.0;
/// The paper the previews are drawn on.
const PAPER: Color = Color::rgb(0xFB, 0xFB, 0xFB);

/// The name a tool goes by in the panel: rectangles and ellipses are the Shape tool's.
pub(crate) fn tool_name(kind: Kind) -> &'static str {
    match kind.family() {
        Kind::Rect => "Shape",
        k => k.label(),
    }
}

/// The look the tool for `kind` draws with now.
pub(crate) fn tool_look(ed: &Editor, kind: Kind) -> QuickStyle {
    let mut options = ed.tool_options(kind);
    if kind.family() == Kind::Rect {
        options.round = Some(kind == Kind::Ellipse);
    }
    QuickStyle { kind: kind.family(), style: ed.tool_style(kind), options }
}

/// An annotation's look.
pub(crate) fn object_look(o: &Object) -> QuickStyle {
    QuickStyle { kind: o.shape.kind().family(), style: o.style, options: ShapeOptions::of(&o.shape) }
}

/// The first selected annotation of `kind`'s tool, if one is selected.
fn selected_of(ed: &Editor, kind: Kind) -> Option<&Object> {
    ed.session.selected().first().and_then(|id| ed.session.doc.get(*id)).filter(|o| o.shape.kind().family() == kind.family())
}

/// The look the panel is about: the selected annotation's, or the tool's.
pub(crate) fn current_look(ed: &Editor, kind: Kind) -> QuickStyle {
    match selected_of(ed, kind) {
        Some(o) => object_look(o),
        None => tool_look(ed, kind),
    }
}

/// Which shape the Shape tool's previews are drawn as: the selection's, or the tool's.
fn shape_kind(ed: &Editor, kind: Kind) -> Kind {
    if kind.family() != Kind::Rect {
        return kind;
    }
    match selected_of(ed, kind) {
        Some(o) => o.shape.kind(),
        None => ed.shape_tool().kind().unwrap_or(Kind::Rect),
    }
}

/// Where a callout's box goes in a preview, so its tail stays in the picture.
fn callout_box(spot: TailSpot) -> Rect {
    match spot {
        TailSpot::DownLeft | TailSpot::DownRight | TailSpot::Down => Rect::new(12.0, 6.0, 84.0, 36.0),
        TailSpot::UpLeft | TailSpot::UpRight | TailSpot::Up => Rect::new(12.0, 38.0, 84.0, 36.0),
        TailSpot::Right => Rect::new(6.0, 22.0, 70.0, 36.0),
        TailSpot::Left => Rect::new(32.0, 22.0, 70.0, 36.0),
    }
}

/// Grey lines of "text" and a coloured block, for the effects and the highlighter to work on.
fn paint_page(base: &mut Raster) {
    let w = base.width() as usize;
    let px = base.pixels_mut();
    let mut set = |x: usize, y: usize, c: [u8; 4]| {
        let i = (y * w + x) * 4;
        if let Some(p) = px.get_mut(i..i + 4) {
            p.copy_from_slice(&c);
        }
    };
    for (top, len) in [(14usize, 80usize), (28, 64), (42, 84), (56, 52)] {
        for y in top..top + 6 {
            for x in 12..12 + len {
                if (x / 9) % 4 != 3 {
                    set(x, y, [0x70, 0x76, 0x80, 0xFF]);
                }
            }
        }
    }
    for y in 24..50 {
        for x in 64..92 {
            set(x, y, [0x2E, 0x7D, 0xD8, 0xFF]);
        }
    }
}

/// A look drawn on a small made-up picture: what its tool would draw with it.
pub(crate) fn draw_preview(look: &QuickStyle, shape: Kind, stamp: StampKind) -> Option<Raster> {
    let (w, h) = PREVIEW;
    let mut base = Raster::filled(w, h, PAPER).ok()?;
    if matches!(look.kind, Kind::Blur | Kind::Pixelate | Kind::Spotlight | Kind::Magnify | Kind::Highlight) {
        paint_page(&mut base);
    }
    let mut doc = Document::new(base);
    let mut style = look.style;
    let o = &look.options;
    let shape = match look.kind.family() {
        Kind::Arrow => Shape::Arrow { from: Pt::new(18.0, 62.0), to: Pt::new(90.0, 18.0), heads: o.heads.unwrap_or_default() },
        Kind::Line => Shape::Line { from: Pt::new(16.0, 62.0), to: Pt::new(92.0, 18.0) },
        Kind::Pen => Shape::Pen {
            points: (0..=24)
                .map(|i| {
                    let t = i as f32 / 24.0;
                    Pt::new(14.0 + t * 80.0, 40.0 + (t * std::f32::consts::TAU).sin() * 16.0)
                })
                .collect(),
        },
        Kind::Highlight => Shape::Highlight { points: vec![Pt::new(12.0, 31.0), Pt::new(96.0, 31.0)] },
        Kind::Rect => {
            let rect = Rect::new(16.0, 14.0, 76.0, 52.0);
            if o.round.unwrap_or(shape == Kind::Ellipse) { Shape::Ellipse { rect } } else { Shape::Rect { rect, radius: o.radius.unwrap_or(0.0) } }
        }
        Kind::Text => {
            style.font_size = 34.0;
            style.align = TextAlign::Center;
            Shape::Text { rect: Rect::new(12.0, 12.0, 84.0, 56.0), text: "Aa".into() }
        }
        Kind::Callout => {
            style.font_size = 20.0;
            style.align = TextAlign::Center;
            let form = o.form.unwrap_or_default();
            let spot = o.tail.unwrap_or_default();
            let rect = callout_box(spot);
            Shape::Callout { rect, tail: spot.place(form, rect), text: "Aa".into(), form }
        }
        Kind::Step => {
            style.font_size = 22.0;
            Shape::Step { center: Pt::new(54.0, 40.0), number: 1 }
        }
        Kind::Stamp => Shape::Stamp { rect: Rect::new(30.0, 16.0, 48.0, 48.0), stamp: o.stamp.unwrap_or(stamp) },
        Kind::Blur => Shape::Blur { rect: Rect::new(10.0, 10.0, 88.0, 60.0) },
        Kind::Pixelate => Shape::Pixelate { rect: Rect::new(10.0, 10.0, 88.0, 60.0) },
        Kind::Spotlight => Shape::Spotlight { rect: Rect::new(32.0, 18.0, 46.0, 40.0), ellipse: o.round.unwrap_or(false) },
        Kind::Magnify => Shape::Magnify { rect: Rect::new(30.0, 12.0, 56.0, 56.0) },
        Kind::Ellipse | Kind::Image => return None,
    };
    doc.add(shape, style.clamped()).ok()?;
    screengrabcraft_render::render(&doc)
}

/// A look's preview on the GPU, drawn the first time it is asked for.
pub(crate) fn preview(ed: &mut Editor, ctx: &egui::Context, look: &QuickStyle, shape: Kind) -> Option<egui::TextureId> {
    let stamp = ed.stamp();
    let key = format!("{shape:?}|{stamp:?}|{}", serde_json::to_string(look).unwrap_or_default());
    if let Some(t) = ed.previews.get(&key) {
        return Some(t.id());
    }
    if ed.previews.len() >= MAX_PREVIEWS {
        ed.previews.clear();
    }
    let pic = draw_preview(look, shape, stamp)?;
    let image = egui::ColorImage::from_rgba_unmultiplied([pic.width() as usize, pic.height() as usize], pic.pixels());
    let handle = ctx.load_texture(format!("quick-style-{}", ed.previews.len()), image, egui::TextureOptions::LINEAR);
    let id = handle.id();
    ed.previews.insert(key, handle);
    Some(id)
}

/// What a tile shows.
#[derive(Clone, Copy)]
enum Face {
    Picture(Option<egui::TextureId>),
    Colour(Color),
}

/// A five-pointed star, filled or outlined.
pub(crate) fn paint_star(p: &egui::Painter, c: egui::Pos2, r: f32, filled: bool, ink: Color32) {
    let pts: Vec<egui::Pos2> = (0..10)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + i as f32 * std::f32::consts::PI / 5.0;
            let rr = if i % 2 == 0 { r } else { r * 0.45 };
            pos2(c.x + a.cos() * rr, c.y + a.sin() * rr)
        })
        .collect();
    if filled {
        let gold = Color32::from_rgb(0xFF, 0xB3, 0x00);
        // A star isn't convex: filled as triangles from its middle.
        for i in 0..pts.len() {
            if let (Some(a), Some(b)) = (pts.get(i), pts.get((i + 1) % pts.len())) {
                p.add(egui::Shape::convex_polygon(vec![c, *a, *b], gold, Stroke::NONE));
            }
        }
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.0, Color32::from_rgb(0x9A, 0x60, 0x00))));
    } else {
        p.add(egui::Shape::closed_line(pts, Stroke::new(1.3, ink)));
    }
}

/// A tile: its face, a ring when it is the look in use, and a star when it can be starred.
/// Returns whether it was clicked, and whether its star was.
fn tile(ui: &mut Ui, face: Face, name: &str, size: egui::Vec2, active: bool, starred: Option<bool>) -> (bool, bool) {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    let hovered = ui.rect_contains_pointer(rect);
    let radius = 5.0;
    match face {
        Face::Picture(tex) => {
            ui.painter().rect_filled(rect, radius, crate::to_color32(PAPER));
            if let Some(tex) = tex {
                egui::Image::new((tex, rect.size())).corner_radius(radius).paint_at(ui, rect);
            }
        }
        Face::Colour(c) => {
            ui.painter().rect_filled(rect, radius, crate::to_color32(PAPER));
            let inner = rect.shrink(7.0);
            if c.a == 0 {
                ui.painter().rect_stroke(inner, 3.0, Stroke::new(1.0, t.border), StrokeKind::Inside);
                ui.painter().line_segment([inner.left_bottom(), inner.right_top()], Stroke::new(2.0, Color32::from_rgb(0xE5, 0x39, 0x35)));
            } else {
                ui.painter().rect_filled(inner, 3.0, crate::to_color32(c));
                ui.painter().rect_stroke(inner, 3.0, Stroke::new(1.0, Color32::from_black_alpha(40)), StrokeKind::Inside);
            }
        }
    }
    let ring = if active {
        Stroke::new(2.5, t.accent)
    } else if hovered {
        Stroke::new(1.5, t.handle)
    } else {
        Stroke::new(1.0, t.border)
    };
    ui.painter().rect_stroke(rect, radius, ring, StrokeKind::Inside);
    let mut star_clicked = false;
    if let Some(on) = starred {
        let star_rect = egui::Rect::from_min_size(pos2(rect.right() - 19.0, rect.top() + 2.0), vec2(17.0, 17.0));
        let star = ui.interact(star_rect, resp.id.with("star"), Sense::click());
        let label = if on { format!("Remove {name} from Favorites") } else { format!("Add {name} to Favorites") };
        star.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &label));
        if on || hovered {
            if !on {
                ui.painter().circle_filled(star_rect.center(), 8.0, Color32::from_white_alpha(if star.hovered() { 240 } else { 190 }));
            }
            paint_star(ui.painter(), star_rect.center(), 6.5, on, Color32::from_rgb(0x55, 0x55, 0x5E));
        }
        star_clicked = star.clicked();
        star.on_hover_text(label);
    }
    let resp = resp.on_hover_text(name);
    (resp.clicked() && !star_clicked, star_clicked)
}

/// The tile size for the width there is.
fn tile_size(ui: &Ui) -> egui::Vec2 {
    let w = ((ui.available_width() - GAP * (COLUMNS - 1) as f32) / COLUMNS as f32).floor().max(30.0);
    vec2(w, (w * 0.74).round())
}

/// Lays `n` tiles out in rows of four.
fn grid(ui: &mut Ui, n: usize, mut each: impl FnMut(&mut Ui, usize, egui::Vec2)) {
    let size = tile_size(ui);
    let mut i = 0;
    while i < n {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing = vec2(GAP, GAP);
            for j in i..(i + COLUMNS).min(n) {
                each(ui, j, size);
            }
        });
        ui.add_space(GAP - ui.spacing().item_spacing.y.min(GAP));
        i += COLUMNS;
    }
}

/// The heading of a quick styles section, with the theme menu.
fn heading(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Quick Styles").strong().color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let current = quick::theme(&ed.settings.quick_theme).name;
            egui::ComboBox::from_id_salt("quick-theme")
                .selected_text(current)
                .width(86.0)
                .show_ui(ui, |ui| {
                    for th in &quick::THEMES {
                        if ui.selectable_label(th.name == current, th.name).clicked() {
                            ed.settings.quick_theme = th.name.to_string();
                            ed.services.save_settings(&ed.settings);
                        }
                    }
                })
                .response
                .on_hover_text("The colours the quick styles come in");
        });
    });
    ui.add_space(4.0);
}

/// The quick styles of a drawing tool, or of the selected annotations.
pub(crate) fn section(ed: &mut Editor, ui: &mut Ui, kind: Kind) {
    let ctx = ui.ctx().clone();
    heading(ed, ui);
    let presets = quick::presets(kind, quick::theme(&ed.settings.quick_theme));
    let current = current_look(ed, kind);
    let shape = shape_kind(ed, kind);
    let mut picked = None;
    let mut starred = None;
    grid(ui, presets.len(), |ui, i, size| {
        let Some(p) = presets.get(i) else { return };
        let face = Face::Picture(preview(ed, &ctx, &p.look, shape));
        let (clicked, star) = tile(ui, face, &p.name, size, p.look.matches(&current), Some(ed.settings.is_favorite(&p.look)));
        if clicked {
            picked = Some(p.look);
        }
        if star {
            starred = Some(p.look);
        }
    });
    if let Some(look) = picked {
        apply(ed, &ctx, &look);
    }
    if let Some(look) = starred {
        ed.settings.toggle_favorite(&look);
        ed.services.save_settings(&ed.settings);
    }
    // The look in use, as it is (one made by hand too), kept under Favorites.
    let current = current_look(ed, kind);
    let kept = ed.settings.is_favorite(&current);
    ui.add_space(2.0);
    let label = if kept { "This look is in Favorites" } else { "Add this look to Favorites" };
    if ui.add_enabled(!kept, egui::Button::new(label).small()).on_hover_text("Keep the look in use, as it is now, under Favorites").clicked() {
        ed.settings.toggle_favorite(&current);
        ed.services.save_settings(&ed.settings);
    }
}

/// The Fill tool's colours, as tiles.
pub(crate) fn fill_section(ed: &mut Editor, ui: &mut Ui) {
    heading(ed, ui);
    let colours = quick::fill_colors(quick::theme(&ed.settings.quick_theme));
    let mut picked = None;
    let now = ed.settings.fill.color;
    grid(ui, colours.len(), |ui, i, size| {
        let Some((name, c)) = colours.get(i) else { return };
        if tile(ui, Face::Colour(*c), &format!("{name} fill"), size, now == *c, None).0 {
            picked = Some(*c);
        }
    });
    if let Some(c) = picked {
        ed.settings.fill.color = c;
        ed.services.save_settings(&ed.settings);
    }
}

/// The Favorites tool's panel: every starred look, of every tool.
pub(crate) fn favorites(ed: &mut Editor, ui: &mut Ui) {
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    ui.heading("Favorites");
    ui.add_space(6.0);
    if ed.settings.favorites.is_empty() {
        ui.label("Star a quick style (the star at its corner) to keep it here.");
        ui.add_space(4.0);
        ui.label(egui::RichText::new("The favourites of every tool are kept together; picking one picks its tool too.").color(t.muted));
        return;
    }
    let favs = ed.settings.favorites.clone();
    let mut picked = None;
    let mut unstar = None;
    grid(ui, favs.len(), |ui, i, size| {
        let Some(f) = favs.get(i) else { return };
        let shape = if f.options.round == Some(true) { Kind::Ellipse } else { f.kind };
        let face = Face::Picture(preview(ed, &ctx, f, shape));
        let name = favorite_name(f, i);
        let (clicked, star) = tile(ui, face, &name, size, false, Some(true));
        if clicked {
            picked = Some(*f);
        }
        if star {
            unstar = Some(*f);
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Click one to draw with it, or to give it to what is selected. Its star takes it away.").size(12.0).color(t.muted));
    if let Some(f) = picked {
        apply(ed, &ctx, &f);
    }
    if let Some(f) = unstar {
        ed.settings.toggle_favorite(&f);
        ed.services.save_settings(&ed.settings);
    }
}

/// How a favourite is named: its tool, and its place among the favourites.
pub(crate) fn favorite_name(f: &QuickStyle, i: usize) -> String {
    format!("{} favourite {}", tool_name(f.kind), i + 1)
}

/// Puts a look on its tool (for what it draws next) and on the selected annotations of that
/// tool (one undo step). From Favorites, or with a shape made round or square, it picks the tool
/// that draws it.
pub(crate) fn apply(ed: &mut Editor, ctx: &egui::Context, look: &QuickStyle) {
    let look = look.clamped();
    let family = look.kind;
    let draw = match (family, look.options.round) {
        (Kind::Rect, Some(true)) => Kind::Ellipse,
        (Kind::Rect, Some(false)) => Kind::Rect,
        (Kind::Rect, None) => ed.shape_tool().kind().unwrap_or(Kind::Rect),
        (k, _) => k,
    };
    // The tool keeps the look; its text size stays what it was.
    let kinds: &[Kind] = if family == Kind::Rect { &[Kind::Rect, Kind::Ellipse] } else { std::slice::from_ref(&family) };
    for k in kinds {
        let size = ed.tool_style(*k).font_size;
        ed.settings.styles.insert(*k, Style { font_size: size, ..look.style }.clamped());
    }
    ed.set_tool_options(family, look.options);
    let ids: Vec<ObjectId> =
        ed.session.selected().into_iter().filter(|id| ed.session.doc.get(*id).is_some_and(|o| o.shape.kind().family() == family)).collect();
    if !ids.is_empty() {
        let style = serde_json::to_value(look.style).unwrap_or(serde_json::Value::Null);
        ed.run(ctx, "object.look", json!({ "ids": ids, "style": style, "options": look.options }));
    }
    if ed.tool == Tool::Favorites || (ed.tool.is_shape() && ed.tool != Tool::Draw(draw)) {
        let keep = ed.session.selected();
        ed.set_tool(Tool::Draw(draw));
        // Picking a favourite for the selection keeps it selected.
        ed.session.select(keep);
    }
    ed.services.save_settings(&ed.settings);
}
