//! The canvas: the picture with its annotations, zoomed and panned, and everything the pointer
//! does on it (drawing with each tool, selecting, moving, resizing, cropping, typing text).

use egui::{Color32, CursorIcon, Pos2, Sense, Stroke, StrokeKind, TextureOptions, Ui, pos2, vec2};
use screengrabcraft_model::object::step_radius;
use screengrabcraft_model::{ArrowHeads, Handle, Kind, ObjectId, Pt, Rect, Shape, ShapeOptions};
use screengrabcraft_render::{Options, text};

use crate::editor::{Editor, Tool};
use crate::theme::Tokens;

/// How the picture sits in the canvas.
#[derive(Clone, Debug)]
pub struct View {
    /// Screen pixels per picture pixel (1.0 is actual size).
    pub zoom: f32,
    /// The picture point at the middle of the canvas.
    pub center: Pt,
    pub fit_pending: bool,
    pub canvas: egui::Rect,
    pub ppp: f32,
    /// The picture as last shown. Another one of another size (cropped, rotated, resized,
    /// undone…) is fitted and centred again, as it is when the window changes size; new pixels
    /// in the same size (a fill, an effect) leave the view where it is.
    shown: Option<screengrabcraft_model::Raster>,
}

impl Default for View {
    fn default() -> View {
        View {
            zoom: 1.0,
            center: Pt::new(400.0, 250.0),
            fit_pending: true,
            canvas: egui::Rect::from_min_size(Pos2::ZERO, vec2(800.0, 500.0)),
            ppp: 1.0,
            shown: None,
        }
    }
}

pub const MIN_ZOOM: f32 = 0.02;
pub const MAX_ZOOM: f32 = 32.0;

impl View {
    /// Screen points per picture pixel.
    pub fn scale(&self) -> f32 {
        self.zoom / self.ppp.max(0.1)
    }

    pub fn to_screen(&self, p: Pt) -> Pos2 {
        let c = self.canvas.center();
        pos2(c.x + (p.x - self.center.x) * self.scale(), c.y + (p.y - self.center.y) * self.scale())
    }

    pub fn to_image(&self, s: Pos2) -> Pt {
        let c = self.canvas.center();
        Pt::new(self.center.x + (s.x - c.x) / self.scale(), self.center.y + (s.y - c.y) / self.scale())
    }

    pub fn rect_to_screen(&self, r: Rect) -> egui::Rect {
        egui::Rect::from_two_pos(self.to_screen(r.min()), self.to_screen(r.max()))
    }

    /// Sets the zoom, keeping the picture point under `anchor` (or the middle) where it is.
    pub fn set_zoom(&mut self, zoom: f32, anchor: Option<Pos2>) {
        let zoom = if zoom.is_finite() { zoom.clamp(MIN_ZOOM, MAX_ZOOM) } else { 1.0 };
        let anchor = anchor.unwrap_or_else(|| self.canvas.center());
        let before = self.to_image(anchor);
        self.zoom = zoom;
        let after = self.to_image(anchor);
        self.center = Pt::new(self.center.x + before.x - after.x, self.center.y + before.y - after.y);
        self.fit_pending = false;
    }

    pub fn zoom_by(&mut self, factor: f32, anchor: Option<Pos2>) {
        self.set_zoom(self.zoom * factor, anchor);
    }

    /// Notes the canvas and the picture of this frame: a new size of canvas (the window
    /// resized) or a new picture fits the picture to the view again, centred. The owner's own
    /// zoom stays until then.
    pub fn follow(&mut self, canvas: egui::Rect, picture: &screengrabcraft_model::Raster) {
        let resized = (canvas.size() - self.canvas.size()).length() > 0.5;
        let replaced = self.shown.as_ref().is_none_or(|s| !s.same_pixels(picture));
        let reshaped = self.shown.as_ref().is_none_or(|s| (s.width(), s.height()) != (picture.width(), picture.height()));
        if resized || reshaped {
            self.fit_pending = true;
        }
        if replaced {
            self.shown = Some(picture.clone());
        }
        self.canvas = canvas;
    }

    /// The whole picture in view, centred: as far out as it takes, but never past actual size.
    pub fn fit(&mut self, w: u32, h: u32) {
        let (cw, ch) = (self.canvas.width().max(50.0) * self.ppp, self.canvas.height().max(50.0) * self.ppp);
        let z = ((cw - 48.0) / w.max(1) as f32).min((ch - 48.0) / h.max(1) as f32).min(1.0);
        self.zoom = z.clamp(MIN_ZOOM, MAX_ZOOM);
        self.center = Pt::new(w as f32 / 2.0, h as f32 / 2.0);
        self.fit_pending = false;
    }
}

/// What a drag on the canvas is doing.
#[derive(Clone, Debug)]
pub enum Drag {
    Create {
        id: ObjectId,
        kind: Kind,
        start: Pt,
    },
    Move {
        ids: Vec<ObjectId>,
        last: Pt,
    },
    Handle {
        id: ObjectId,
        handle: Handle,
    },
    Rubber {
        start: Pt,
        now: Pt,
    },
    Pan {
        last: Pos2,
    },
    CropNew {
        start: Pt,
    },
    CropHandle {
        handle: Handle,
    },
    CropMove {
        last: Pt,
    },
    /// An area of the picture being selected with the Selection tool.
    AreaNew {
        start: Pt,
    },
}

/// The picture's tiles on the GPU (a long scrolling capture is more than one texture).
pub struct Tile {
    pub texture: egui::TextureHandle,
    pub area: Rect,
}

/// How big a handle is drawn, in screen points.
const HANDLE: f32 = 10.0;
/// How near a handle a press takes hold of it: well beyond what's drawn, so a press that's a
/// little off still gets it rather than starting something new.
const GRAB: f32 = 13.0;
/// How near a line or a border a press takes hold of its annotation, in screen points.
const NEAR: f32 = 6.0;
/// How big a tool drawn as the pointer is (the eyedropper, the Fill tool's pail), in screen points.
const POINTER: f32 = 30.0;

fn snap_angle(from: Pt, to: Pt) -> Pt {
    let d = to.sub(from);
    let len = d.length();
    if len < 1.0 {
        return to;
    }
    let a = d.y.atan2(d.x);
    let step = std::f32::consts::FRAC_PI_4;
    let a = (a / step).round() * step;
    Pt::new(from.x + a.cos() * len, from.y + a.sin() * len)
}

fn square_from(start: Pt, now: Pt) -> Pt {
    let (dx, dy) = (now.x - start.x, now.y - start.y);
    let s = dx.abs().max(dy.abs());
    Pt::new(start.x + s * dx.signum(), start.y + s * dy.signum())
}

/// Builds the shape a drag from `start` to `now` makes with a tool whose options are `opts`.
fn shape_for(kind: Kind, start: Pt, now: Pt, shift: bool, opts: ShapeOptions, step: u32, image: (f32, f32)) -> Shape {
    // A square or a snapped line can reach further than the pointer; it stops at the edge.
    let area = Rect::new(0.0, 0.0, image.0, image.1);
    let boxed = |square: bool| {
        let end = if square || shift { area.stop_at_edge(start, square_from(start, now)) } else { now };
        Rect::from_points(start, end)
    };
    let line_end = || if shift { area.stop_at_edge(start, snap_angle(start, now)) } else { now };
    match kind {
        Kind::Arrow => Shape::Arrow { from: start, to: line_end(), heads: ArrowHeads::End },
        Kind::Line => Shape::Line { from: start, to: line_end() },
        Kind::Rect => Shape::Rect { rect: boxed(false), radius: 0.0 },
        Kind::Ellipse => Shape::Ellipse { rect: boxed(false) },
        Kind::Pen => Shape::Pen { points: vec![start, now] },
        Kind::Highlight => Shape::Highlight { points: vec![start, now] },
        Kind::Text => Shape::Text { rect: boxed(false), text: String::new() },
        Kind::Callout => {
            // The tail goes where the tool says, as far as the picture goes.
            let rect = boxed(false);
            let form = opts.form.unwrap_or_default();
            let tail = area.nearest(opts.tail.unwrap_or_default().place(form, rect));
            Shape::Callout { rect, tail, text: String::new(), form }
        }
        Kind::Step => Shape::Step { center: now, number: step },
        Kind::Blur => Shape::Blur { rect: boxed(false) },
        Kind::Pixelate => Shape::Pixelate { rect: boxed(false) },
        Kind::Spotlight => Shape::Spotlight { rect: boxed(false), ellipse: false },
        Kind::Magnify => Shape::Magnify { rect: boxed(true) },
        Kind::Stamp => Shape::Stamp { rect: boxed(true), stamp: opts.stamp.unwrap_or_default() },
        Kind::Image => Shape::Rect { rect: boxed(false), radius: 0.0 },
    }
}

/// Whether a just-drawn shape is big enough to keep.
fn big_enough(shape: &Shape) -> bool {
    match shape {
        Shape::Arrow { from, to, .. } | Shape::Line { from, to } => from.distance(*to) >= 4.0,
        Shape::Pen { points } | Shape::Highlight { points } => points.len() >= 2 && Rect::bounding(points).w.max(Rect::bounding(points).h) >= 2.0,
        Shape::Step { .. } => true,
        other => other.rect().is_some_and(|r| r.w >= 4.0 && r.h >= 4.0),
    }
}

fn cursor_for(h: Handle) -> CursorIcon {
    match h {
        Handle::TopLeft | Handle::BottomRight => CursorIcon::ResizeNwSe,
        Handle::TopRight | Handle::BottomLeft => CursorIcon::ResizeNeSw,
        Handle::Top | Handle::Bottom => CursorIcon::ResizeVertical,
        Handle::Left | Handle::Right => CursorIcon::ResizeHorizontal,
        _ => CursorIcon::Grab,
    }
}

fn crop_handles(r: Rect) -> Vec<(Handle, Pt)> {
    let c = r.center();
    vec![
        (Handle::TopLeft, r.min()),
        (Handle::Top, Pt::new(c.x, r.y)),
        (Handle::TopRight, Pt::new(r.right(), r.y)),
        (Handle::Right, Pt::new(r.right(), c.y)),
        (Handle::BottomRight, r.max()),
        (Handle::Bottom, Pt::new(c.x, r.bottom())),
        (Handle::BottomLeft, Pt::new(r.x, r.bottom())),
        (Handle::Left, Pt::new(r.x, c.y)),
    ]
}

impl Editor {
    /// The shape options the tool keeps (arrow heads, corner radius, a callout's form, the stamp,
    /// a round spotlight). A callout's tail is already where it goes.
    fn with_defaults(&self, shape: Shape) -> Shape {
        let kind = shape.kind();
        let opts = ShapeOptions {
            tail: None,
            round: if kind.family() == Kind::Rect { None } else { self.tool_options(kind).round },
            ..self.tool_options(kind)
        };
        opts.applied(shape)
    }

    /// The annotation under a picture point. Nothing is picked up outside the picture.
    fn object_under(&self, p: Pt) -> Option<ObjectId> {
        if !self.session.doc.bounds().contains(p) {
            return None;
        }
        self.session.doc.object_at(p, NEAR / self.view.scale())
    }

    /// The topmost shape with an inside under an image point, which the Fill tool fills (inside
    /// it as well as on its border, whether it has a fill yet or not).
    pub(crate) fn fillable_under(&self, p: Pt) -> Option<ObjectId> {
        if !self.session.doc.bounds().contains(p) {
            return None;
        }
        let tol = NEAR / self.view.scale();
        let solid = |o: &&screengrabcraft_model::Object| {
            o.shape.hit(&screengrabcraft_model::Style { fill: screengrabcraft_model::Color::WHITE, ..o.style }, p, tol)
        };
        self.session.doc.objects.iter().rev().filter(|o| has_inside(o.shape.kind())).find(solid).map(|o| o.id)
    }

    /// The handle of a selected annotation under a screen point.
    pub(crate) fn handle_at(&self, s: Pos2) -> Option<(ObjectId, Handle)> {
        let sel = self.session.selected();
        if sel.len() != 1 {
            return None;
        }
        let id = *sel.first()?;
        let o = self.session.doc.get(id)?;
        // A pen or highlighter stroke has no box to pull: it's moved, not reshaped.
        if stroke_points(&o.shape).is_some() {
            return None;
        }
        // The nearest one in reach, so two handles close together (a small box) both work.
        nearest(o.shape.handles(&o.style), |p| self.view.to_screen(p).distance(s)).map(|h| (id, h))
    }

    fn crop_handle_at(&self, s: Pos2) -> Option<Handle> {
        let r = self.crop?;
        nearest(crop_handles(r), |p| self.view.to_screen(p).distance(s))
    }

    /// Starts typing into a text annotation.
    pub(crate) fn start_editing(&mut self, id: ObjectId) {
        let Some(o) = self.session.doc.get(id) else { return };
        let Some(t) = o.shape.text() else { return };
        self.edit_buffer = t.to_string();
        self.editing = Some(id);
        self.session.select(vec![id]);
        self.session.begin("Type Text");
    }

    /// Stops typing. A text box left empty goes (there is nothing of it to see); a callout stays,
    /// with or without text, as a shape.
    pub(crate) fn stop_editing(&mut self) {
        let Some(id) = self.editing.take() else { return };
        let empty = self.session.doc.get(id).is_none_or(|o| matches!(&o.shape, Shape::Text { text, .. } if text.trim().is_empty()));
        if empty {
            self.session.doc.remove(&[id]);
            self.session.selection.retain(|s| *s != id);
            self.session.touch();
        }
        self.session.commit();
    }

    /// Stops typing for a while, keeping the annotation however empty: its box is being
    /// reshaped, and typing goes on after (see `resume_editing`).
    fn pause_editing(&mut self) {
        if self.editing.take().is_some() {
            self.session.commit();
        }
    }

    fn upload(&mut self, ctx: &egui::Context) {
        let key = (self.session.revision(), self.editing);
        if key == self.texture_key && !self.tiles.is_empty() {
            return;
        }
        let opts = Options { hide: Vec::new(), no_text: self.editing };
        let Some(pix) = self.renderer.render(&self.session.doc, &opts) else { return };
        let (w, h) = (pix.width(), pix.height());
        let max = ctx.input(|i| i.max_texture_side).clamp(512, 8192) as u32;
        let mut tiles = Vec::new();
        let mut y = 0;
        let mut n = 0;
        while y < h {
            let th = max.min(h - y);
            let mut x = 0;
            while x < w {
                let tw = max.min(w - x);
                let mut data = Vec::with_capacity(tw as usize * th as usize * 4);
                let stride = w as usize * 4;
                for row in y..y + th {
                    let s = row as usize * stride + x as usize * 4;
                    if let Some(part) = pix.data().get(s..s + tw as usize * 4) {
                        data.extend_from_slice(part);
                    }
                }
                let image = egui::ColorImage::from_rgba_premultiplied([tw as usize, th as usize], &data);
                let opts = TextureOptions {
                    magnification: egui::TextureFilter::Nearest,
                    minification: egui::TextureFilter::Linear,
                    ..TextureOptions::default()
                };
                let area = Rect::new(x as f32, y as f32, tw as f32, th as f32);
                // Keep the texture of a tile that is still there; upload anew otherwise.
                let texture = match self.tiles.get_mut(n) {
                    Some(t) if t.area == area => {
                        t.texture.set(image, opts);
                        t.texture.clone()
                    }
                    _ => ctx.load_texture(format!("picture-{n}"), image, opts),
                };
                tiles.push(Tile { texture, area });
                n += 1;
                x += tw;
            }
            y += th;
        }
        self.tiles = tiles;
        self.texture_key = key;
        // Kept for the eyedropper: the picture exactly as it is shown.
        self.shown = Some((w, h, pix.take()));
    }

    /// The colour shown at a picture point (annotations included), for the eyedropper.
    pub(crate) fn colour_at(&self, p: Pt) -> Option<screengrabcraft_model::Color> {
        let (w, h, data) = self.shown.as_ref()?;
        if !(p.x.is_finite() && p.y.is_finite()) || p.x < 0.0 || p.y < 0.0 {
            return None;
        }
        let (x, y) = (p.x.floor() as u64, p.y.floor() as u64);
        if x >= u64::from(*w) || y >= u64::from(*h) {
            return None;
        }
        let i = usize::try_from(y.checked_mul(u64::from(*w))?.checked_add(x)?.checked_mul(4)?).ok()?;
        let px = data.get(i..i.checked_add(4)?)?;
        let (r, g, b, a) = (px.first()?, px.get(1)?, px.get(2)?, px.get(3)?);
        // Stored with the colour multiplied by its alpha.
        let un = |v: u8| if *a == 0 { 0 } else { ((u16::from(v) * 255 + u16::from(*a) / 2) / u16::from(*a)).min(255) as u8 };
        Some(screengrabcraft_model::Color { r: un(*r), g: un(*g), b: un(*b), a: *a })
    }

    fn checker(&mut self, ctx: &egui::Context) -> egui::TextureId {
        if let Some(t) = &self.checker {
            return t.id();
        }
        let t = Tokens::get(ctx);
        let (a, b) = (t.checker_a, t.checker_b);
        let img = egui::ColorImage::new([2, 2], vec![a, b, b, a]);
        let opts = TextureOptions {
            magnification: egui::TextureFilter::Nearest,
            minification: egui::TextureFilter::Nearest,
            wrap_mode: egui::TextureWrapMode::Repeat,
            ..TextureOptions::default()
        };
        let handle = ctx.load_texture("checker", img, opts);
        let id = handle.id();
        self.checker = Some(handle);
        id
    }
}

/// Shows the canvas in the space left.
pub fn show(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    egui::CentralPanel::default().frame(egui::Frame::NONE.fill(t.canvas)).show(ui, |ui| {
        let rect = ui.max_rect();
        let resp = ui.interact(rect, ui.id().with("canvas"), Sense::click_and_drag());
        ed.view.follow(rect, &ed.session.doc.base);
        // Moved to a screen of another density: fitted again too.
        if (ctx.pixels_per_point() - ed.view.ppp).abs() > 0.001 {
            ed.view.fit_pending = true;
        }
        ed.view.ppp = ctx.pixels_per_point();
        // Fitted once the canvas has its real size (a browser lays the page out after the first frame).
        if ed.view.fit_pending && rect.width() >= 120.0 && rect.height() >= 120.0 {
            ed.view.fit(ed.session.doc.width(), ed.session.doc.height());
        }
        if ed.theme_changed(&ctx) {
            ed.checker = None;
        }
        ed.upload(&ctx);
        // The right button puts the eyedropper away rather than open the menu.
        let picking = crate::eyedropper::active(&ctx);
        input(ed, ui, &resp);
        if !picking {
            context_menu(ed, &ctx, &resp);
        }
        crate::playback::show(ed, &ctx, rect);
        paint(ed, ui, rect);
        text_editor(ed, ui);
    });
}

fn input(ed: &mut Editor, ui: &mut Ui, resp: &egui::Response) {
    let ctx = ui.ctx().clone();
    let hover = resp.hover_pos();
    ed.pointer_px =
        hover.map(|p| ed.view.to_image(p)).filter(|p| ed.session.doc.bounds().contains(*p)).map(|p| (p.x.floor() as i32, p.y.floor() as i32));
    // Zoom with Ctrl and the wheel (or a pinch), scroll to pan.
    if resp.hovered() {
        let (zoom, scroll) = ctx.input(|i| (i.zoom_delta(), i.smooth_scroll_delta()));
        if (zoom - 1.0).abs() > 0.001 {
            ed.view.zoom_by(zoom, hover);
        } else if scroll != egui::Vec2::ZERO {
            let s = ed.view.scale();
            ed.view.center = Pt::new(ed.view.center.x - scroll.x / s, ed.view.center.y - scroll.y / s);
        }
    }
    let (shift, space, middle) = ctx.input(|i| (i.modifiers.shift, i.key_down(egui::Key::Space), i.pointer.middle_down()));
    let press = ctx.input(|i| i.pointer.press_origin());

    // Panning: the middle button, or Space with the left one.
    if let Some(Drag::Pan { last }) = ed.drag.clone() {
        if let Some(now) = ctx.input(|i| i.pointer.latest_pos()) {
            let s = ed.view.scale();
            ed.view.center = Pt::new(ed.view.center.x - (now.x - last.x) / s, ed.view.center.y - (now.y - last.y) / s);
            ed.drag = Some(Drag::Pan { last: now });
        }
        if !ctx.input(|i| i.pointer.any_down()) {
            ed.drag = None;
        }
        ctx.set_cursor_icon(CursorIcon::Grabbing);
        return;
    }
    if resp.hovered() && (middle || (space && ctx.input(|i| i.pointer.primary_down()))) && ed.drag.is_none() {
        if let Some(p) = ctx.input(|i| i.pointer.latest_pos()) {
            ed.drag = Some(Drag::Pan { last: p });
        }
        return;
    }
    if space && resp.hovered() {
        ctx.set_cursor_icon(CursorIcon::Grab);
    }

    // ---- the eyedropper: while it's out, a click on the picture takes the colour there and
    // nothing else happens; Esc or the right button puts it away ----
    if crate::eyedropper::active(&ctx) {
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) || resp.secondary_clicked() {
            crate::eyedropper::cancel(&ctx);
            return;
        }
        if let Some(h) = hover
            && resp.hovered()
        {
            let colour = ed.colour_at(ed.view.to_image(h));
            match colour {
                Some(c) => {
                    ctx.set_cursor_icon(CursorIcon::None);
                    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tool-pointer")));
                    // The colour under the tip, beside the eyedropper.
                    let swatch = egui::Rect::from_min_size(h + vec2(14.0, 6.0), vec2(22.0, 22.0));
                    p.rect_filled(swatch, 4.0, crate::to_color32(c));
                    p.rect_stroke(swatch, 4.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Outside);
                    p.rect_stroke(swatch.expand(1.5), 5.0, Stroke::new(1.0, Color32::from_black_alpha(200)), StrokeKind::Outside);
                    crate::icons::paint_pointer(&p, h, crate::icons::Icon::Eyedropper, POINTER);
                    if resp.clicked_by(egui::PointerButton::Primary) || resp.drag_stopped_by(egui::PointerButton::Primary) {
                        crate::eyedropper::picked(&ctx, c);
                    }
                }
                None => ctx.set_cursor_icon(CursorIcon::NotAllowed),
            }
        }
        return;
    }

    let to_img = |p: Pos2, v: &View| v.to_image(p);
    let image_size = (ed.session.doc.width() as f32, ed.session.doc.height() as f32);

    // ---- cursor ----
    if let Some(h) = hover
        && ed.drag.is_none()
    {
        let on_picture = ed.session.doc.bounds().contains(to_img(h, &ed.view));
        let cursor = match ed.tool {
            Tool::Select | Tool::Favorites => {
                if ed.area.is_some_and(|a| a.contains(to_img(h, &ed.view))) {
                    CursorIcon::Move
                } else if let Some((_, handle)) = ed.handle_at(h) {
                    cursor_for(handle)
                } else if ed.object_under(to_img(h, &ed.view)).is_some() {
                    CursorIcon::Move
                } else {
                    CursorIcon::Default
                }
            }
            Tool::Crop => match ed.crop_handle_at(h) {
                Some(handle) => cursor_for(handle),
                None if !on_picture => CursorIcon::Default,
                None if ed.crop.is_some_and(|c| c != ed.session.doc.bounds() && c.contains(to_img(h, &ed.view))) => CursorIcon::Move,
                None => CursorIcon::Crosshair,
            },
            // Inside the selected area (or on the piece taken out of it) a drag moves it.
            Tool::Area if ed.area.is_some_and(|a| a.contains(to_img(h, &ed.view))) => CursorIcon::Move,
            Tool::Area if ed.handle_at(h).is_some() => ed.handle_at(h).map(|(_, handle)| cursor_for(handle)).unwrap_or(CursorIcon::Crosshair),
            Tool::Area if ed.object_under(to_img(h, &ed.view)).is_some_and(|id| ed.session.selected().contains(&id)) => CursorIcon::Move,
            Tool::Area if on_picture => CursorIcon::Crosshair,
            Tool::Area => CursorIcon::Default,
            // The pail is drawn as the pointer, its drop on the spot that's filled.
            Tool::Fill if on_picture => {
                if resp.hovered() {
                    let p = ctx.layer_painter(egui::LayerId::new(egui::Order::Tooltip, egui::Id::new("tool-pointer")));
                    crate::icons::paint_pointer(&p, h, crate::icons::Icon::Fill, POINTER);
                }
                CursorIcon::None
            }
            Tool::Fill => CursorIcon::Default,
            // What a press does: a handle reshapes, an annotation moves, elsewhere draws.
            Tool::Draw(_) if ed.handle_at(h).is_some() => ed.handle_at(h).map(|(_, handle)| cursor_for(handle)).unwrap_or(CursorIcon::Crosshair),
            Tool::Draw(k) if !matches!(k, Kind::Pen | Kind::Highlight) && ed.object_under(to_img(h, &ed.view)).is_some() => CursorIcon::Move,
            Tool::Draw(_) if !on_picture => CursorIcon::Default,
            Tool::Draw(Kind::Text) => CursorIcon::Text,
            Tool::Draw(_) => CursorIcon::Crosshair,
        };
        if resp.hovered() {
            ctx.set_cursor_icon(cursor);
        }
    }
    // While a drag goes on, the cursor stays what it was when it started: the arrows of the
    // handle being pulled, or the hand of what's being moved.
    match &ed.drag {
        Some(Drag::Handle { handle, .. }) => ctx.set_cursor_icon(cursor_for(*handle)),
        Some(Drag::CropHandle { handle }) => ctx.set_cursor_icon(cursor_for(*handle)),
        Some(Drag::Move { .. } | Drag::CropMove { .. }) => ctx.set_cursor_icon(CursorIcon::Grabbing),
        _ => {}
    }

    // ---- double click: type into text ----
    if resp.double_clicked()
        && ed.tool != Tool::Fill
        && let Some(p) = hover
        && let Some(id) = ed.object_under(to_img(p, &ed.view))
        && ed.session.doc.get(id).is_some_and(|o| o.shape.text().is_some())
    {
        ed.stop_editing();
        ed.start_editing(id);
        return;
    }

    // ---- a press and drag ----
    if resp.drag_started_by(egui::PointerButton::Primary) {
        // Pulling a handle of the box being typed into reshapes it, and the typing goes on after
        // (a callout or a text box just drawn has no text yet); any other drag ends the typing.
        let typed = ed.editing.filter(|e| press.and_then(|p| ed.handle_at(p)).is_some_and(|(id, _)| id == *e));
        match typed {
            Some(id) => {
                ed.pause_editing();
                ed.resume_editing = Some(id);
            }
            None => ed.stop_editing(),
        }
        let Some(start_s) = press else { return };
        let start = to_img(start_s, &ed.view);
        // Nothing starts outside the picture, though a handle on its edge is still a handle.
        let on_handle = if ed.tool == Tool::Crop { ed.crop_handle_at(start_s).is_some() } else { ed.handle_at(start_s).is_some() };
        if !ed.session.doc.bounds().contains(start) && !on_handle {
            return;
        }
        match ed.tool {
            Tool::Select | Tool::Favorites => {
                if lift_area(ed, &ctx, start) {
                    // Inside an area of the picture selected: that part of it is moved.
                } else if let Some((id, handle)) = ed.handle_at(start_s) {
                    ed.area = None;
                    ed.session.begin("Resize");
                    ed.drag = Some(Drag::Handle { id, handle });
                } else if let Some(id) = ed.object_under(start) {
                    ed.area = None;
                    let mut sel = ed.session.selected();
                    if !sel.contains(&id) {
                        sel = if shift {
                            let mut s = sel;
                            s.push(id);
                            s
                        } else {
                            vec![id]
                        };
                        ed.session.select(sel.clone());
                    }
                    ed.session.begin("Move");
                    ed.drag = Some(Drag::Move { ids: sel, last: start });
                } else {
                    ed.drag = Some(Drag::Rubber { start, now: start });
                }
            }
            Tool::Crop => {
                // While the frame is still the whole picture, a drag draws a new one.
                let whole = ed.crop.is_none_or(|c| c == ed.session.doc.bounds());
                if let Some(handle) = ed.crop_handle_at(start_s) {
                    ed.drag = Some(Drag::CropHandle { handle });
                } else if !whole && ed.crop.is_some_and(|c| c.contains(start)) {
                    ed.drag = Some(Drag::CropMove { last: start });
                } else {
                    ed.drag = Some(Drag::CropNew { start });
                }
            }
            // A fill happens where the button went down; moving on before letting go is no drag.
            Tool::Fill => fill_at(ed, &ctx, start),
            Tool::Area => {
                if lift_area(ed, &ctx, start) {
                    // Inside the selected area: that part of the picture is moved.
                } else if let Some((id, handle)) = ed.handle_at(start_s) {
                    // A piece taken out is resized by its handles, and moved again by a drag.
                    ed.session.begin("Resize");
                    ed.drag = Some(Drag::Handle { id, handle });
                } else if let Some(id) = ed.object_under(start).filter(|id| ed.session.selected().contains(id)) {
                    ed.session.begin("Move");
                    ed.drag = Some(Drag::Move { ids: vec![id], last: start });
                } else {
                    ed.area = None;
                    ed.session.select(Vec::new());
                    ed.drag = Some(Drag::AreaNew { start });
                }
            }
            Tool::Draw(kind) => {
                // Starting on a handle of the selected annotation reshapes it instead.
                if let Some((id, handle)) = ed.handle_at(start_s) {
                    ed.session.begin("Resize");
                    ed.drag = Some(Drag::Handle { id, handle });
                    return;
                }
                // Starting on an annotation picks it up and moves it, whatever the tool, as a
                // click selects it. Pen and highlighter strokes are drawn over things, so they
                // always draw.
                if !matches!(kind, Kind::Pen | Kind::Highlight)
                    && let Some(id) = ed.object_under(start)
                {
                    ed.session.select(vec![id]);
                    ed.session.begin("Move");
                    ed.drag = Some(Drag::Move { ids: vec![id], last: start });
                    return;
                }
                let step = ed.session.doc.next_step_number();
                let shape = ed.with_defaults(shape_for(kind, start, start, false, ed.tool_options(kind), step, image_size));
                let style = ed.tool_style(kind);
                ed.session.begin(&format!("Draw {}", kind.label()));
                match ed.session.doc.add(shape, style) {
                    Ok(id) => {
                        ed.session.select(vec![id]);
                        ed.drag = Some(Drag::Create { id, kind, start });
                    }
                    Err(e) => {
                        ed.session.cancel();
                        ed.error(&ctx, e.to_string());
                    }
                }
            }
        }
    }

    if let Some(now_s) = ctx.input(|i| i.pointer.latest_pos())
        && ed.drag.is_some()
        && resp.dragged()
    {
        let now = to_img(now_s, &ed.view);
        let bounds = ed.session.doc.bounds();
        let inside = bounds.nearest(now);
        match ed.drag.clone() {
            Some(Drag::Create { id, kind, start }) => {
                let (opts, step) = (
                    ed.tool_options(kind),
                    ed.session.doc.get(id).and_then(|o| if let Shape::Step { number, .. } = o.shape { Some(number) } else { None }).unwrap_or(1),
                );
                let fresh = ed.with_defaults(shape_for(kind, start, inside, shift, opts, step, image_size));
                if let Some(o) = ed.session.doc.get_mut(id) {
                    match (&mut o.shape, kind) {
                        (Shape::Pen { points }, _) | (Shape::Highlight { points }, _) => {
                            if shift && kind == Kind::Highlight {
                                *points = vec![start, Pt::new(inside.x, start.y)];
                            } else if points.last().is_none_or(|l| l.distance(inside) * ed.view.scale() >= 1.5)
                                && points.len() < screengrabcraft_model::object::MAX_POINTS
                            {
                                points.push(inside);
                            }
                        }
                        (shape, _) => *shape = fresh,
                    }
                }
                // A step's badge is round its centre: all of it stays on the picture.
                if kind == Kind::Step {
                    ed.session.doc.bring_inside(id);
                }
                ed.session.touch();
            }
            Some(Drag::Move { ids, last }) => {
                let d = ed.session.doc.limit_move(&ids, now.sub(last));
                for id in &ids {
                    if let Some(o) = ed.session.doc.get_mut(*id) {
                        o.shape.translate(d);
                    }
                }
                // Past the edge the pointer runs ahead, and the annotation waits for it to come back.
                ed.drag = Some(Drag::Move { ids, last: last.add(d) });
                ed.session.touch();
            }
            Some(Drag::Handle { id, handle }) => {
                if let Some(o) = ed.session.doc.get_mut(id) {
                    let style = o.style;
                    let mut reshaped = o.shape.clone();
                    reshaped.drag_handle(&style, handle, inside, shift);
                    // Text boxes keep room for their text.
                    if let Shape::Text { rect, text: body } = &mut reshaped {
                        let need = text::fit_box(body, &style, rect.min(), Some(rect.w));
                        rect.h = rect.h.max(need.h);
                    }
                    // Keeping proportions or room for text can reach past the pointer: it stops at
                    // the edge. An annotation already over it (an old project, a crop) stays free.
                    if bounds.encloses(reshaped.extent()) || !bounds.encloses(o.shape.extent()) {
                        o.shape = reshaped;
                    }
                }
                ed.session.touch();
            }
            Some(Drag::Rubber { start, .. }) => ed.drag = Some(Drag::Rubber { start, now: inside }),
            Some(Drag::CropNew { start }) => {
                let end = if shift { square_from(start, now) } else { now };
                ed.crop = Rect::from_points(start, end).intersect(bounds);
            }
            Some(Drag::AreaNew { start }) => {
                let end = if shift { square_from(start, now) } else { now };
                ed.area = Rect::from_points(start, end).intersect(bounds);
            }
            Some(Drag::CropHandle { handle }) => {
                if let Some(c) = ed.crop {
                    ed.crop = handle.drag_rect(c, now, shift).intersect(bounds);
                }
            }
            Some(Drag::CropMove { last }) => {
                if let Some(c) = ed.crop {
                    let d = now.sub(last);
                    let mut moved = c.translate(d);
                    moved.x = moved.x.clamp(0.0, (bounds.w - moved.w).max(0.0));
                    moved.y = moved.y.clamp(0.0, (bounds.h - moved.h).max(0.0));
                    ed.crop = Some(moved);
                }
                ed.drag = Some(Drag::CropMove { last: now });
            }
            _ => {}
        }
    }

    if resp.drag_stopped() {
        let resume = ed.resume_editing.take();
        match ed.drag.take() {
            Some(Drag::Handle { id, .. }) if resume == Some(id) => {
                ed.session.commit();
                ed.start_editing(id);
            }
            Some(Drag::Create { id, kind, .. }) => {
                let keep = ed.session.doc.get(id).is_some_and(|o| big_enough(&o.shape));
                if keep {
                    // A text box dragged out gets its text typed at once.
                    if matches!(kind, Kind::Text | Kind::Callout) {
                        if let Some(o) = ed.session.doc.get_mut(id)
                            && let Shape::Text { rect, .. } = &mut o.shape
                        {
                            rect.h = rect.h.max(text::fit_box("", &o.style, rect.min(), Some(rect.w)).h);
                        }
                        ed.session.doc.bring_inside(id);
                        ed.session.commit();
                        ed.start_editing(id);
                    } else {
                        ed.session.commit();
                        // A pen or highlighter stroke is left as drawn, as on paper: not
                        // selected, so nothing is drawn over it and the next stroke can follow.
                        if matches!(kind, Kind::Pen | Kind::Highlight) {
                            ed.session.select(Vec::new());
                        }
                    }
                } else {
                    ed.session.cancel();
                    // Too small to be a drag: treat it as a click.
                    click(ed, &ctx, press, shift);
                }
            }
            Some(Drag::Move { .. }) | Some(Drag::Handle { .. }) => ed.session.commit(),
            // Too small to be an area: none.
            Some(Drag::AreaNew { .. }) => {
                if ed.area.is_some_and(|a| a.w < 2.0 || a.h < 2.0) {
                    ed.area = None;
                }
            }
            Some(Drag::Rubber { start, now }) => {
                let area = Rect::from_points(start, now);
                let mut ids = ed.session.doc.objects_in(area);
                // Round no annotation, it selects that part of the picture itself, as the
                // Selection tool does: to move, copy, cut, delete or crop to.
                if ids.is_empty() && !shift {
                    ed.area = area.intersect(ed.session.doc.bounds()).filter(|a| a.w >= 2.0 && a.h >= 2.0);
                }
                if shift {
                    let mut sel = ed.session.selected();
                    sel.extend(ids);
                    ids = sel;
                }
                ed.session.select(ids);
            }
            _ => {}
        }
    } else if resp.clicked_by(egui::PointerButton::Primary) && ed.drag.is_none() && !resp.double_clicked() {
        click(ed, &ctx, press.or(hover), shift);
    }
}

/// Whether a kind is a shape with an inside to fill.
fn has_inside(kind: Kind) -> bool {
    matches!(kind, Kind::Rect | Kind::Ellipse | Kind::Callout | Kind::Text | Kind::Step | Kind::Stamp)
}

/// The Fill tool at an image point: a shape with an inside there takes the colour as its fill;
/// anywhere else, the area of the picture round the pixel does.
fn fill_at(ed: &mut Editor, ctx: &egui::Context, p: Pt) {
    let fill = ed.settings.fill;
    let colour = fill.color.to_hex();
    if let Some(id) = ed.fillable_under(p) {
        ed.run(ctx, "object.update", serde_json::json!({"id": id.0, "style": {"fill": colour}}));
    } else if ed.session.doc.bounds().contains(p) {
        ed.run(ctx, "image.fill", serde_json::json!({"x": p.x.floor(), "y": p.y.floor(), "color": colour, "tolerance": fill.tolerance}));
    }
}

/// A click (no drag) on the canvas.
/// A drag starting inside the area of the picture selected: that part of the picture comes out
/// as a piece of its own (the command `image.lift`), and the drag moves it. False when the drag
/// starts elsewhere.
fn lift_area(ed: &mut Editor, ctx: &egui::Context, start: Pt) -> bool {
    let Some(params) = ed.area.filter(|a| a.contains(start)).and_then(|_| ed.area_params()) else { return false };
    ed.area = None;
    if let Some(id) = ed.run(ctx, "image.lift", params).and_then(|out| out.get("id").and_then(serde_json::Value::as_u64)) {
        let id = ObjectId(id);
        ed.session.select(vec![id]);
        ed.session.begin("Move");
        ed.drag = Some(Drag::Move { ids: vec![id], last: start });
    }
    true
}

/// The menu a right-click on the picture opens: what can be done with the area of the picture
/// selected, or with the annotations selected (Flatten among it), and Flatten All.
fn context_menu(ed: &mut Editor, ctx: &egui::Context, resp: &egui::Response) {
    // A right-click on an annotation picks it first, unless it's inside the area selected.
    if resp.secondary_clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let at = ed.view.to_image(p);
        if !ed.area.is_some_and(|a| a.contains(at)) {
            ed.area = None;
            ed.stop_editing();
            match ed.object_under(at) {
                Some(id) if !ed.session.selected().contains(&id) => ed.session.select(vec![id]),
                Some(_) => {}
                None => ed.session.select(Vec::new()),
            }
        }
    }
    resp.context_menu(|ui| {
        ui.set_min_width(240.0);
        let item =
            |ui: &mut Ui, enabled: bool, label: &str, keys: &str| ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(keys)).clicked();
        let undo = ed.session.undo_label().map(|l| format!("Undo {l}")).unwrap_or_else(|| "Undo".to_string());
        if item(ui, ed.session.can_undo(), &undo, "Ctrl+Z") {
            ed.run(ctx, "edit.undo", serde_json::json!({}));
            ui.close();
        }
        let redo = ed.session.redo_label().map(|l| format!("Redo {l}")).unwrap_or_else(|| "Redo".to_string());
        if item(ui, ed.session.can_redo(), &redo, "Ctrl+Y") {
            ed.run(ctx, "edit.redo", serde_json::json!({}));
            ui.close();
        }
        ui.separator();
        if ed.area.is_some() {
            if item(ui, true, "Copy Area", "Ctrl+C") {
                ed.copy(ctx);
                ui.close();
            }
            if item(ui, true, "Cut Area", "Ctrl+X") {
                ed.cut_area(ctx);
                ui.close();
            }
            if item(ui, true, "Delete Area", "Del") {
                ed.erase_area(ctx);
                ui.close();
            }
            if item(ui, true, "Crop to Selection", "") {
                ed.crop_to_area(ctx);
                ui.close();
            }
            ui.separator();
        }
        let sel = ed.session.selected();
        let any = !sel.is_empty();
        if any {
            if item(ui, true, "Copy", "Ctrl+C") {
                ed.copy(ctx);
                ui.close();
            }
            if item(ui, true, "Cut", "Ctrl+X") {
                ed.cut_selection(ctx);
                ui.close();
            }
            if item(ui, true, "Flatten", "Ctrl+Shift+F") {
                ed.flatten(ctx, false);
                ui.close();
            }
            // One with text: type into it.
            let text = match sel.as_slice() {
                [one] if ed.session.doc.get(*one).is_some_and(|o| o.shape.text().is_some()) => Some(*one),
                _ => None,
            };
            if let Some(id) = text
                && item(ui, true, "Edit Text", "Double-click")
            {
                ed.start_editing(id);
                ui.close();
            }
            if item(ui, true, "Duplicate", "Ctrl+D") {
                ed.run(ctx, "edit.duplicate", serde_json::json!({}));
                ui.close();
            }
            if item(ui, true, "Delete", "Del") {
                ed.run(ctx, "edit.delete", serde_json::json!({}));
                ui.close();
            }
            if item(ui, true, "Bring to Front", "") {
                ed.run(ctx, "object.order", serde_json::json!({"to": "front"}));
                ui.close();
            }
            if item(ui, true, "Send to Back", "") {
                ed.run(ctx, "object.order", serde_json::json!({"to": "back"}));
                ui.close();
            }
            ui.separator();
        }
        if item(ui, true, "Paste", "Ctrl+V") {
            ed.paste(ctx, false);
            ui.close();
        }
        let plain_copy = ed.area.is_none() && ed.session.selected().is_empty();
        if item(ui, true, "Copy Picture", if plain_copy { "Ctrl+C" } else { "" }) {
            ed.copy_whole(ctx);
            ui.close();
        }
        if item(ui, !ed.session.doc.objects.is_empty(), "Flatten All", "") {
            ed.flatten(ctx, true);
            ui.close();
        }
        ui.separator();
        if item(ui, !ed.session.doc.objects.is_empty(), "Select All Annotations", "Ctrl+A") {
            ed.set_tool(Tool::Select);
            ed.run(ctx, "edit.selectAll", serde_json::json!({}));
            ui.close();
        }
        if item(ui, true, "Select an Area of the Picture", "Q") {
            ed.set_tool(Tool::Area);
            ui.close();
        }
        if item(ui, true, "Zoom to Fit", "Ctrl+0") {
            ed.view.fit_pending = true;
            ui.close();
        }
    });
}

fn click(ed: &mut Editor, ctx: &egui::Context, at: Option<Pos2>, shift: bool) {
    let Some(s) = at else { return };
    let p = ed.view.to_image(s);
    let hit = ed.object_under(p);
    let (w, h) = (ed.session.doc.width() as f32, ed.session.doc.height() as f32);
    let editing_before = ed.editing.is_some();
    ed.stop_editing();
    match ed.tool {
        Tool::Select | Tool::Favorites | Tool::Crop => {
            if ed.tool == Tool::Crop {
                return;
            }
            // A click lets go of an area of the picture selected.
            ed.area = None;
            match hit {
                Some(id) if shift => {
                    let mut sel = ed.session.selected();
                    if let Some(i) = sel.iter().position(|s| *s == id) {
                        sel.remove(i);
                    } else {
                        sel.push(id);
                    }
                    ed.session.select(sel);
                }
                Some(id) => ed.session.select(vec![id]),
                None => ed.session.select(Vec::new()),
            }
        }
        Tool::Fill => {
            if !editing_before {
                fill_at(ed, ctx, p);
            }
        }
        // A click lets go of the area; on a piece taken out of the picture it selects that.
        Tool::Area => {
            ed.area = None;
            ed.session.select(hit.into_iter().collect());
        }
        Tool::Draw(kind) => {
            // A click on an annotation selects it (and types into text with the text tools).
            if let Some(id) = hit {
                ed.session.select(vec![id]);
                if matches!(kind, Kind::Text | Kind::Callout) && ed.session.doc.get(id).is_some_and(|o| o.shape.text().is_some()) {
                    ed.start_editing(id);
                }
                return;
            }
            if editing_before {
                return;
            }
            let style = ed.tool_style(kind);
            let on_picture = ed.session.doc.bounds().contains(p);
            let shape = match kind {
                _ if !on_picture => None,
                Kind::Text => {
                    let width = (w * 0.3).clamp(160.0, 420.0).min(w);
                    Some(Shape::Text { rect: text::fit_box("", &style, p, Some(width)), text: String::new() })
                }
                Kind::Callout => {
                    // The tail at the click, the box placed for the tail to point the tool's way.
                    let opts = ed.tool_options(Kind::Callout);
                    let form = opts.form.unwrap_or_default();
                    let (bw, bh) = ((w * 0.25).clamp(160.0, 300.0).min(w), 80.0f32.min(h * 0.4));
                    let off = opts.tail.unwrap_or_default().offset(form, bw, bh);
                    let x = (p.x - off.x).min(w - bw).max(0.0);
                    let y = (p.y - off.y).min(h - bh).max(0.0);
                    Some(Shape::Callout { rect: Rect::new(x, y, bw, bh), tail: p, text: String::new(), form })
                }
                Kind::Step => Some(Shape::Step { center: p, number: ed.session.doc.next_step_number() }),
                Kind::Stamp => {
                    let size = (w.min(h) * 0.08).clamp(32.0, 128.0).min(w).min(h);
                    Some(Shape::Stamp { rect: Rect::new(p.x - size / 2.0, p.y - size / 2.0, size, size), stamp: ed.stamp() })
                }
                Kind::Magnify => {
                    let size = (w.min(h) * 0.25).clamp(80.0, 260.0).min(w).min(h);
                    Some(Shape::Magnify { rect: Rect::new(p.x - size / 2.0, p.y - size / 2.0, size, size) })
                }
                _ => None,
            };
            let Some(shape) = shape else {
                ed.session.select(Vec::new());
                return;
            };
            ed.session.begin(&format!("Add {}", kind.label()));
            match ed.session.doc.add(shape, style) {
                Ok(id) => {
                    ed.session.doc.bring_inside(id);
                    ed.session.select(vec![id]);
                    ed.session.commit();
                    if matches!(kind, Kind::Text | Kind::Callout) {
                        ed.start_editing(id);
                    }
                }
                Err(e) => {
                    ed.session.cancel();
                    ed.error(ctx, e.to_string());
                }
            }
        }
    }
}

fn paint(ed: &mut Editor, ui: &mut Ui, clip: egui::Rect) {
    let t = Tokens::get(ui.ctx());
    let ctx = ui.ctx().clone();
    let painter = ui.painter_at(clip);
    let img = ed.view.rect_to_screen(ed.session.doc.bounds());
    // A soft frame round the picture, then the checks for transparency, then the picture.
    painter.rect_filled(img.translate(vec2(0.0, 3.0)).expand(2.0), 2.0, Color32::from_black_alpha(if t.dark { 120 } else { 40 }));
    let checker = ed.checker(&ctx);
    let uv = egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(img.width() / 16.0, img.height() / 16.0));
    painter.image(checker, img, uv, Color32::WHITE);
    for tile in &ed.tiles {
        let r = ed.view.rect_to_screen(tile.area);
        painter.image(tile.texture.id(), r, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    crate::playback::paint(ed, &painter, img);
    let scale = ed.view.scale();

    // Hover outline in the select tool.
    if ed.tool.selects()
        && ed.drag.is_none()
        && let Some(h) = ctx.input(|i| i.pointer.hover_pos())
        && clip.contains(h)
        && let Some(id) = ed.object_under(ed.view.to_image(h))
        && !ed.session.selected().contains(&id)
        && let Some(o) = ed.session.doc.get(id)
    {
        if let Some(points) = stroke_points(&o.shape) {
            stroke_mark(&painter, ed, points, t.handle.gamma_multiply(0.7), false);
        } else {
            let b = ed.view.rect_to_screen(o.shape.bounds(&o.style));
            painter.rect_stroke(b.expand(2.0), 2.0, Stroke::new(1.0, t.handle.gamma_multiply(0.6)), StrokeKind::Outside);
        }
    }

    // The selection.
    let selected = ed.session.selected();
    for id in &selected {
        let Some(o) = ed.session.doc.get(*id) else { continue };
        // A pen or highlighter stroke is shown selected along itself, with no box round it, and
        // not at all while it's being drawn: it looks as it will.
        if let Some(points) = stroke_points(&o.shape) {
            if !matches!(&ed.drag, Some(Drag::Create { id: drawing, .. }) if drawing == id) {
                stroke_mark(&painter, ed, points, t.handle, true);
            }
            continue;
        }
        let line_like = matches!(o.shape, Shape::Arrow { .. } | Shape::Line { .. });
        if !line_like {
            let b = ed.view.rect_to_screen(o.shape.rect().unwrap_or_else(|| o.shape.bounds(&o.style)));
            dashed_rect(&painter, b.expand(1.0), t.handle);
        }
        if selected.len() == 1 {
            for (h, p) in o.shape.handles(&o.style) {
                let s = ed.view.to_screen(p);
                if h == Handle::Tail || line_like {
                    painter.circle_filled(s, HANDLE * 0.6, Color32::WHITE);
                    painter.circle_stroke(s, HANDLE * 0.6, Stroke::new(1.5, t.handle));
                } else {
                    let r = egui::Rect::from_center_size(s, vec2(HANDLE, HANDLE));
                    painter.rect_filled(r, 1.5, Color32::WHITE);
                    painter.rect_stroke(r, 1.5, Stroke::new(1.5, t.handle), StrokeKind::Middle);
                }
            }
            if let Shape::Step { center, .. } = o.shape {
                let r = step_radius(o.style.font_size) * scale;
                painter.circle_stroke(ed.view.to_screen(center), r + 3.0, Stroke::new(1.5, t.handle));
            }
        }
    }

    // A rubber band.
    if let Some(Drag::Rubber { start, now }) = &ed.drag {
        let r = egui::Rect::from_two_pos(ed.view.to_screen(*start), ed.view.to_screen(*now));
        painter.rect_filled(r, 0.0, t.handle.gamma_multiply(0.12));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, t.handle), StrokeKind::Middle);
    }

    // The area selected with the Selection tool: a frame of dashes in black and white (it shows
    // on any picture), and its size.
    if let Some(a) = ed.area {
        let r = ed.view.rect_to_screen(a);
        painter.rect_filled(r, 0.0, t.handle.gamma_multiply(0.08));
        painter.rect_stroke(r, 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);
        dashed_rect(&painter, r, Color32::WHITE);
        let label = format!("{:.0} × {:.0}", a.w, a.h);
        let at = pos2(r.center().x, (r.max.y + 14.0).min(clip.max.y - 12.0));
        let galley = painter.layout_no_wrap(label, egui::FontId::proportional(12.0), Color32::WHITE);
        let bg = egui::Rect::from_center_size(at, galley.size() + vec2(12.0, 6.0));
        painter.rect_filled(bg, 4.0, Color32::from_black_alpha(200));
        painter.galley(bg.min + vec2(6.0, 3.0), galley, Color32::WHITE);
    }

    // The crop: the rest darkened, the frame with its handles and size.
    if let Some(c) = ed.crop {
        let r = ed.view.rect_to_screen(c);
        let dim = Color32::from_black_alpha(140);
        for part in [
            egui::Rect::from_min_max(img.min, pos2(img.max.x, r.min.y)),
            egui::Rect::from_min_max(pos2(img.min.x, r.max.y), img.max),
            egui::Rect::from_min_max(pos2(img.min.x, r.min.y), pos2(r.min.x, r.max.y)),
            egui::Rect::from_min_max(pos2(r.max.x, r.min.y), pos2(img.max.x, r.max.y)),
        ] {
            if part.is_positive() {
                painter.rect_filled(part, 0.0, dim);
            }
        }
        painter.rect_stroke(r, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Middle);
        for i in 1..3 {
            let f = i as f32 / 3.0;
            let x = r.min.x + r.width() * f;
            let y = r.min.y + r.height() * f;
            painter.line_segment([pos2(x, r.min.y), pos2(x, r.max.y)], Stroke::new(0.5, Color32::from_white_alpha(120)));
            painter.line_segment([pos2(r.min.x, y), pos2(r.max.x, y)], Stroke::new(0.5, Color32::from_white_alpha(120)));
        }
        for (_, p) in crop_handles(c) {
            let s = ed.view.to_screen(p);
            let hr = egui::Rect::from_center_size(s, vec2(HANDLE + 2.0, HANDLE + 2.0));
            painter.rect_filled(hr, 1.0, Color32::WHITE);
            painter.rect_stroke(hr, 1.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);
        }
        let label = format!("{:.0} × {:.0}", c.w, c.h);
        let at = pos2(r.center().x, (r.max.y + 14.0).min(clip.max.y - 12.0));
        let galley = painter.layout_no_wrap(label, egui::FontId::proportional(12.0), Color32::WHITE);
        let bg = egui::Rect::from_center_size(at, galley.size() + vec2(12.0, 6.0));
        painter.rect_filled(bg, 4.0, Color32::from_black_alpha(200));
        painter.galley(bg.min + vec2(6.0, 3.0), galley, Color32::WHITE);
    }
}

/// The points of a pen or highlighter stroke (shown selected along itself, not by a box).
fn stroke_points(shape: &Shape) -> Option<&[Pt]> {
    match shape {
        Shape::Pen { points } | Shape::Highlight { points } => Some(points),
        _ => None,
    }
}

/// A pen or highlighter stroke marked as hovered or selected along its own middle: a thin line
/// (dashes, white under the colour, when selected) and a dot at each end. Nothing covers the
/// stroke, so it keeps its colour, and a thin line can't fold over itself where the stroke turns.
fn stroke_mark(p: &egui::Painter, ed: &Editor, points: &[Pt], color: Color32, selected: bool) {
    // Every few points on the screen, not every point the pointer gave: no jitter.
    let mut pts: Vec<Pos2> = Vec::new();
    for q in points {
        let s = ed.view.to_screen(*q);
        if pts.last().is_none_or(|l| l.distance(s) >= 3.0) {
            pts.push(s);
        }
    }
    if let (Some(last), Some(end)) = (pts.last().copied(), points.last().map(|q| ed.view.to_screen(*q)))
        && last != end
    {
        pts.push(end);
    }
    if pts.len() >= 2 {
        if selected {
            p.add(egui::Shape::line(pts.clone(), Stroke::new(3.0, Color32::from_white_alpha(220))));
            p.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.5, color), 6.0, 4.0));
        } else {
            p.add(egui::Shape::line(pts.clone(), Stroke::new(1.5, color)));
        }
    }
    if selected {
        for end in [pts.first(), pts.last()].into_iter().flatten() {
            p.circle_filled(*end, HANDLE * 0.45, Color32::WHITE);
            p.circle_stroke(*end, HANDLE * 0.45, Stroke::new(1.5, color));
        }
    }
}

fn dashed_rect(p: &egui::Painter, r: egui::Rect, color: Color32) {
    let pts = [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
    p.extend(egui::Shape::dashed_line(&pts, Stroke::new(1.0, color), 5.0, 3.0));
}

/// The text box being typed into, over the annotation, in its own font and size.
fn text_editor(ed: &mut Editor, ui: &mut Ui) {
    let Some(id) = ed.editing else { return };
    let Some(o) = ed.session.doc.get(id).cloned() else {
        ed.editing = None;
        return;
    };
    let Some(rect) = o.shape.rect() else { return };
    // An oval's or a cloud's text is in its middle.
    let area = match &o.shape {
        Shape::Callout { form, .. } => form.text_area(rect),
        _ => rect,
    };
    let scale = ed.view.scale();
    let pad = text::padding(&o.style);
    let family = if o.style.mono {
        egui::FontFamily::Name("mono".into())
    } else if o.style.bold {
        egui::FontFamily::Name("sans-bold".into())
    } else {
        egui::FontFamily::Name("sans".into())
    };
    let font = egui::FontId::new((o.style.font_size * scale).max(4.0), family);
    let screen = ed.view.rect_to_screen(area);
    let inner_w = ((area.w - 2.0 * pad) * scale).max(20.0);
    let color = crate::to_color32(o.style.text_color);
    let align = match o.style.align {
        screengrabcraft_model::TextAlign::Left => egui::Align::Min,
        screengrabcraft_model::TextAlign::Center => egui::Align::Center,
        screengrabcraft_model::TextAlign::Right => egui::Align::Max,
    };
    // Callouts have their text in the middle; text boxes at the top.
    let top = if matches!(o.shape, Shape::Callout { .. }) {
        let lay = text::layout(&ed.edit_buffer, &o.style, Some(area.w - 2.0 * pad));
        screen.min.y + ((area.h - lay.height) / 2.0).max(pad) * scale
    } else {
        screen.min.y + pad * scale
    };
    let mut buffer = std::mem::take(&mut ed.edit_buffer);
    let out = egui::Area::new(egui::Id::new(("text-edit", id.0)))
        .fixed_pos(pos2(screen.min.x + pad * scale, top))
        .order(egui::Order::Foreground)
        .show(ui.ctx(), |ui| {
            ui.set_max_width(inner_w);
            let edit = egui::TextEdit::multiline(&mut buffer)
                .font(font)
                .text_color(color)
                .desired_width(inner_w)
                .desired_rows(1)
                .frame(egui::Frame::NONE)
                .margin(egui::Margin::ZERO)
                .horizontal_align(align)
                .background_color(Color32::TRANSPARENT);
            ui.add(edit)
        });
    let resp = out.inner;
    if !resp.has_focus() && !resp.lost_focus() {
        resp.request_focus();
    }
    let changed = resp.changed();
    let done = resp.lost_focus() || ui.ctx().input(|i| i.key_pressed(egui::Key::Escape));
    if changed {
        if let Some(obj) = ed.session.doc.get_mut(id) {
            if let Some(t) = obj.shape.text_mut() {
                *t = buffer.clone();
            }
            let style = obj.style;
            match &mut obj.shape {
                Shape::Text { rect, text: body } => {
                    let need = text::fit_box(body, &style, rect.min(), Some(rect.w));
                    rect.h = need.h.max(rect.h.min(need.h + style.font_size));
                }
                Shape::Callout { rect, text: body, form, .. } => {
                    let lay = text::layout(body, &style, Some(form.text_area(*rect).w - 2.0 * pad));
                    rect.h = rect.h.max(form.box_height(lay.height + 2.0 * pad));
                }
                _ => {}
            }
        }
        // A box that grows past the bottom of the picture moves up.
        ed.session.doc.bring_inside(id);
        ed.session.touch();
    }
    ed.edit_buffer = buffer;
    // The box being typed in, outlined.
    dashed_rect(
        &ui.painter_at(ed.view.canvas),
        ed.view.rect_to_screen(ed.session.doc.get(id).and_then(|o| o.shape.rect()).unwrap_or(rect)),
        Tokens::get(ui.ctx()).handle,
    );
    if done {
        ed.stop_editing();
    }
}

/// The handle nearest the pointer (`distance` in screen points), if it's within [`GRAB`].
fn nearest(handles: Vec<(Handle, Pt)>, distance: impl Fn(Pt) -> f32) -> Option<Handle> {
    handles.into_iter().map(|(h, p)| (h, distance(p))).filter(|(_, d)| *d <= GRAB).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(h, _)| h)
}
