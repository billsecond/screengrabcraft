//! The Selection tool's shapes (a box, an ellipse, freehand, the magic wand), turning the
//! selection inside out, and the eraser. What is chosen goes to the engine as the area's
//! parameters (`render::region`), so moving, copying, deleting, cropping and the colour effects
//! all keep to its shape.

use egui::{Color32, Ui};
use screengrabcraft_model::{Color, Pt, Rect};
use screengrabcraft_render::region::{self, AreaShape, Region};
use serde_json::{Value, json};

use crate::editor::Editor;
use crate::theme::Tokens;

/// How the Selection tool chooses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AreaMode {
    #[default]
    Rect,
    Ellipse,
    Freehand,
    /// The magic wand: a click takes the pixels of the colour there.
    Wand,
}

impl AreaMode {
    pub const ALL: [AreaMode; 4] = [AreaMode::Rect, AreaMode::Ellipse, AreaMode::Freehand, AreaMode::Wand];

    pub fn label(self) -> &'static str {
        match self {
            AreaMode::Rect => "Box",
            AreaMode::Ellipse => "Ellipse",
            AreaMode::Freehand => "Freehand",
            AreaMode::Wand => "Magic Wand",
        }
    }

    fn tip(self) -> &'static str {
        match self {
            AreaMode::Rect => "Drag a box",
            AreaMode::Ellipse => "Drag an ellipse (Shift: a circle)",
            AreaMode::Freehand => "Draw round what to select",
            AreaMode::Wand => "Click a colour: the pixels like it are selected",
        }
    }
}

/// The shape of an area selected, when it isn't its box: the area (`Editor::area`) it belongs
/// to, the shape's own box, and whether it is turned inside out.
#[derive(Clone, Debug, PartialEq)]
pub struct Chosen {
    pub bbox: Rect,
    pub rect: Rect,
    pub shape: AreaShape,
    pub invert: bool,
}

impl Chosen {
    /// Moved with the picture.
    pub fn translate(&mut self, d: Pt) {
        self.bbox = self.bbox.translate(d);
        self.rect = self.rect.translate(d);
        self.shape.translate(d);
    }
}

impl Editor {
    /// The shape of the area, if it has one other than its box.
    pub(crate) fn area_shape(&self) -> Option<&Chosen> {
        self.chosen.as_ref().filter(|c| Some(c.bbox) == self.area)
    }

    /// The area as a command's parameters: its box, and its shape when it has one.
    pub(crate) fn area_params(&self) -> Option<Value> {
        let a = self.area?;
        let (x, y, w, h) = a.pixel_bounds(self.session.doc.width(), self.session.doc.height())?;
        let Some(c) = self.area_shape() else { return Some(json!({"x": x, "y": y, "w": w, "h": h})) };
        let mut v = serde_json::to_value(&c.shape).ok()?;
        let r = c.rect;
        v["x"] = json!(r.x);
        v["y"] = json!(r.y);
        v["w"] = json!(r.w);
        v["h"] = json!(r.h);
        v["invert"] = json!(c.invert);
        Some(v)
    }

    /// What of the picture the area takes, shape and all.
    pub(crate) fn area_region(&self) -> Option<Region> {
        let a = self.area?;
        let picture = &self.session.doc.base;
        match self.area_shape() {
            Some(c) => region::region(picture, c.rect, &c.shape, c.invert),
            None => region::region(picture, a, &AreaShape::Rect, false),
        }
    }

    /// Sets the area to a shape; none when it takes nothing of the picture.
    pub(crate) fn choose(&mut self, rect: Rect, shape: AreaShape, invert: bool) {
        if shape == AreaShape::Rect && !invert {
            self.chosen = None;
            self.area = rect.intersect(self.session.doc.bounds());
            return;
        }
        match region::region(&self.session.doc.base, rect, &shape, invert) {
            Some(reg) => {
                let bbox = reg.rect();
                self.area = Some(bbox);
                self.chosen = Some(Chosen { bbox, rect, shape, invert });
            }
            None => {
                self.area = None;
                self.chosen = None;
            }
        }
    }

    /// The magic wand at `p`.
    pub(crate) fn select_by_colour(&mut self, p: Pt) {
        let shape = AreaShape::Wand { seed: p, tolerance: self.wand_tolerance, contiguous: self.wand_contiguous };
        self.session.select(Vec::new());
        self.choose(Rect::default(), shape, false);
    }

    /// Invert Selection: everything but the area (or the area again).
    pub(crate) fn invert_area(&mut self) {
        let Some(a) = self.area else { return };
        let (rect, shape, invert) = match self.area_shape() {
            Some(c) => (c.rect, c.shape.clone(), !c.invert),
            None => (a, AreaShape::Rect, true),
        };
        self.choose(rect, shape, invert);
    }

    /// Erases along `points` with the eraser's width and colour.
    pub(crate) fn erase_along(&mut self, ctx: &egui::Context, points: &[Pt]) {
        let mut params = json!({"points": points, "width": self.eraser_width});
        if let Some(c) = self.eraser_colour {
            params["color"] = json!(c.to_hex());
        }
        self.run(ctx, "image.eraseStroke", params);
    }
}

/// The tint over a shaped area (what is in it, in the accent colour), as a texture over its box.
pub(crate) fn paint_tint(ed: &mut Editor, ctx: &egui::Context, painter: &egui::Painter) {
    let Some(c) = ed.area_shape().cloned() else { return };
    let key = format!("{:?}|{}", c, ed.session.revision());
    if ed.area_tint.as_ref().is_none_or(|(k, _)| *k != key) {
        let Some(reg) = region::region(&ed.session.doc.base, c.rect, &c.shape, c.invert) else { return };
        // A texture as large as a graphics card takes.
        if reg.w > 8192 || reg.h > 8192 {
            return;
        }
        let accent = Tokens::get(ctx).handle;
        let pixels: Vec<Color32> = reg.mask.iter().map(|m| Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), m / 3)).collect();
        let image = egui::ColorImage::new([reg.w as usize, reg.h as usize], pixels);
        let tex = ctx.load_texture("area-tint", image, egui::TextureOptions::NEAREST);
        ed.area_tint = Some((key, tex));
    }
    if let Some((_, tex)) = &ed.area_tint {
        let r = ed.view.rect_to_screen(c.bbox);
        painter.image(tex.id(), r, egui::Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
    }
    // An ellipse or an outline drawn round as well, in dashes that show on anything.
    let outline: Vec<egui::Pos2> = match &c.shape {
        AreaShape::Ellipse => (0..=64)
            .map(|i| {
                let t = i as f32 / 64.0 * std::f32::consts::TAU;
                let r = c.rect;
                ed.view.to_screen(Pt::new(r.center().x + r.w / 2.0 * t.cos(), r.center().y + r.h / 2.0 * t.sin()))
            })
            .collect(),
        AreaShape::Freehand { points } => points.iter().chain(points.first()).map(|p| ed.view.to_screen(*p)).collect(),
        _ => Vec::new(),
    };
    if outline.len() > 1 {
        painter.add(egui::Shape::line(outline.clone(), egui::Stroke::new(1.0, Color32::BLACK)));
        painter.extend(egui::Shape::dashed_line(&outline, egui::Stroke::new(1.0, Color32::WHITE), 5.0, 3.0));
    }
}

/// The Selection tool's shapes and the wand's settings, at the top of its panel.
pub(crate) fn mode_panel(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        for m in AreaMode::ALL {
            if ui.selectable_label(ed.area_mode == m, m.label()).on_hover_text(m.tip()).clicked() {
                ed.area_mode = m;
            }
        }
    });
    if ed.area_mode == AreaMode::Wand {
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.label("Tolerance");
            ui.add(egui::Slider::new(&mut ed.wand_tolerance, 0..=255));
        });
        ui.checkbox(&mut ed.wand_contiguous, "Only what joins the pixel clicked")
            .on_hover_text("Off: every pixel of that colour, anywhere on the picture");
    }
    ui.add_space(4.0);
    ui.label(egui::RichText::new(ed.area_mode.tip()).size(12.0).color(t.muted));
}

/// The eraser's panel: how wide, and see-through or a colour.
pub(crate) fn eraser_panel(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.heading("Eraser");
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new("Drag over the picture to rub it out. Annotations aren't touched: they are moved or deleted.").size(12.0).color(t.muted),
    );
    ui.add_space(10.0);
    ui.label("Width");
    ui.add(egui::Slider::new(&mut ed.eraser_width, 2.0..=200.0).suffix(" px"));
    ui.add_space(8.0);
    ui.label("Leaves");
    let mut see_through = ed.eraser_colour.is_none();
    ui.horizontal(|ui| {
        if ui.radio(see_through, "See-through").clicked() {
            see_through = true;
        }
        if ui.radio(!see_through, "A colour").clicked() {
            see_through = false;
        }
    });
    if see_through {
        ed.eraser_colour = None;
    } else {
        let mut c = ed.eraser_colour.map(crate::to_color32).unwrap_or(Color32::WHITE);
        ui.horizontal(|ui| {
            egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque);
            ui.label(egui::RichText::new("the canvas colour").size(12.0).color(t.muted));
        });
        ed.eraser_colour = Some(crate::from_color32(c));
    }
}

/// The eraser's stroke while it is drawn: its width, see-through red over what it will take.
pub(crate) fn paint_stroke(ed: &Editor, painter: &egui::Painter, points: &[Pt]) {
    let width = (ed.eraser_width * ed.view.scale()).max(1.0);
    let pts: Vec<egui::Pos2> = points.iter().map(|p| ed.view.to_screen(*p)).collect();
    let colour =
        ed.eraser_colour.map_or(Color32::from_rgba_unmultiplied(220, 60, 60, 110), |c: Color| Color32::from_rgba_unmultiplied(c.r, c.g, c.b, 200));
    if let [one] = pts.as_slice() {
        painter.circle_filled(*one, width / 2.0, colour);
    } else {
        painter.add(egui::Shape::line(pts, egui::Stroke::new(width, colour)));
    }
}
