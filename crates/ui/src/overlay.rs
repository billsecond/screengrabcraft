//! The capture overlay: the frozen screen, on every monitor, while the owner picks what to keep.
//!
//! The capture service takes a picture of each monitor first and shows it full screen, so what
//! is chosen is exactly what was on screen when the shortcut was pressed (menus and tooltips
//! included). Drag out an area, or click a window; adjust it with its handles; then pick what to
//! do with it from the small bar beside it, or press Enter for the default.
//!
//! One [`Overlay`] is shared by all the monitors' windows: [`Overlay::show_monitor`] draws one
//! monitor's window and takes its input. Areas are in the pixels of a monitor's picture.

use egui::{Align2, Color32, CursorIcon, FontId, Pos2, Sense, Stroke, StrokeKind, TextureOptions, Ui, pos2, vec2};
use screengrabcraft_model::{IRect, Raster, Rect};

use crate::icons::{self, Icon};

/// One monitor as the overlay shows it.
pub struct OverlayMonitor {
    /// Where it is on the desktop (desktop units).
    pub rect: IRect,
    /// What was on it (its own pixels).
    pub picture: Raster,
    pub name: String,
}

/// A window the owner can click.
#[derive(Clone, Debug)]
pub struct OverlayWindow {
    pub rect: IRect,
    pub title: String,
}

/// What the area is being chosen for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    Picture,
    Window,
    Gif,
    Scrolling,
}

/// What to do with the chosen area.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    /// The owner's default (Enter, a double-click, or the release when there is no preview).
    Default,
    Edit,
    Copy,
    Save,
    /// Start recording or scrolling (the GIF and scrolling purposes).
    Start,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    Pending,
    Cancelled,
    Chosen {
        monitor: usize,
        /// In the monitor's picture's pixels.
        area: IRect,
        /// The same on the desktop.
        desktop: IRect,
        action: Action,
        /// The window's title, when a window was clicked.
        title: Option<String>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Choosing,
    Dragging { start: (f32, f32) },
    Adjusting,
    Moving { last: (f32, f32) },
    Resizing { handle: usize },
}

pub struct Overlay {
    pub monitors: Vec<OverlayMonitor>,
    pub windows: Vec<OverlayWindow>,
    pub purpose: Purpose,
    /// Keep the area adjustable with a bar of actions after it is chosen.
    pub preview: bool,
    pub magnifier: bool,
    pub crosshair: bool,
    textures: Vec<Option<egui::TextureHandle>>,
    /// The monitor of the area, and the area in its pixels.
    sel: Option<(usize, Rect)>,
    title: Option<String>,
    phase: Phase,
    /// The monitor the pointer is on and where (its pixels).
    pointer: Option<(usize, (f32, f32))>,
    outcome: Outcome,
    focused: Vec<bool>,
}

const HANDLE: f32 = 9.0;

/// A desktop rectangle in a monitor's picture's pixels.
fn desk_to_px(m: &OverlayMonitor, r: IRect) -> Option<Rect> {
    let part = m.rect.intersect(r)?;
    let sx = m.picture.width() as f32 / m.rect.w.max(1) as f32;
    let sy = m.picture.height() as f32 / m.rect.h.max(1) as f32;
    Some(Rect::new((part.x - m.rect.x) as f32 * sx, (part.y - m.rect.y) as f32 * sy, part.w as f32 * sx, part.h as f32 * sy))
}

fn px_to_desk(m: &OverlayMonitor, r: Rect) -> IRect {
    let sx = m.rect.w.max(1) as f32 / m.picture.width().max(1) as f32;
    let sy = m.rect.h.max(1) as f32 / m.picture.height().max(1) as f32;
    IRect::new(
        m.rect.x + (r.x * sx).round() as i32,
        m.rect.y + (r.y * sy).round() as i32,
        (r.w * sx).round().max(1.0) as u32,
        (r.h * sy).round().max(1.0) as u32,
    )
}

fn whole(m: &OverlayMonitor) -> Rect {
    Rect::new(0.0, 0.0, m.picture.width() as f32, m.picture.height() as f32)
}

/// Whole pixels: the area rounded and clipped to the picture.
fn pixels(r: Rect, w: u32, h: u32) -> IRect {
    match r.pixel_bounds(w, h) {
        Some((x, y, cw, ch)) => IRect::new(x as i32, y as i32, cw, ch),
        None => IRect::new(0, 0, w.min(1), h.min(1)),
    }
}

fn handles(r: Rect) -> [(f32, f32); 8] {
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    [(r.x, r.y), (cx, r.y), (r.right(), r.y), (r.right(), cy), (r.right(), r.bottom()), (cx, r.bottom()), (r.x, r.bottom()), (r.x, cy)]
}

impl Overlay {
    pub fn new(monitors: Vec<OverlayMonitor>, windows: Vec<OverlayWindow>, purpose: Purpose) -> Overlay {
        let n = monitors.len();
        Overlay {
            monitors,
            windows,
            purpose,
            preview: true,
            magnifier: true,
            crosshair: true,
            textures: (0..n).map(|_| None).collect(),
            sel: None,
            title: None,
            phase: Phase::Choosing,
            pointer: None,
            outcome: Outcome::Pending,
            focused: vec![false; n],
        }
    }

    pub fn outcome(&self) -> &Outcome {
        &self.outcome
    }

    /// Starts with an area already chosen (Repeat Last Capture with preview).
    pub fn preselect(&mut self, monitor: usize, area: IRect) {
        if self.monitors.get(monitor).is_some() {
            self.sel = Some((monitor, Rect::new(area.x as f32, area.y as f32, area.w as f32, area.h as f32)));
            self.phase = Phase::Adjusting;
        }
    }

    fn choose(&mut self, action: Action) {
        let Some((mi, r)) = self.sel else { return };
        let Some(m) = self.monitors.get(mi) else { return };
        let area = pixels(r, m.picture.width(), m.picture.height());
        let desktop = px_to_desk(m, Rect::new(area.x as f32, area.y as f32, area.w as f32, area.h as f32));
        self.outcome = Outcome::Chosen { monitor: mi, area, desktop, action, title: self.title.clone() };
    }

    /// The frontmost window under a point of a monitor, in that monitor's pixels.
    fn window_at(&self, mi: usize, p: (f32, f32)) -> Option<(Rect, String)> {
        let m = self.monitors.get(mi)?;
        self.windows.iter().find_map(|w| {
            let r = desk_to_px(m, w.rect)?;
            r.contains(screengrabcraft_model::Pt::new(p.0, p.1)).then(|| (r, w.title.clone()))
        })
    }

    fn texture(&mut self, ctx: &egui::Context, mi: usize) -> Option<egui::TextureId> {
        if let Some(Some(t)) = self.textures.get(mi) {
            return Some(t.id());
        }
        let m = self.monitors.get(mi)?;
        let max = ctx.input(|i| i.max_texture_side) as u32;
        let pic = &m.picture;
        let image = if pic.width() <= max && pic.height() <= max {
            egui::ColorImage::from_rgba_unmultiplied([pic.width() as usize, pic.height() as usize], pic.pixels())
        } else {
            // Larger than the graphics card takes: shown reduced (the capture keeps every pixel).
            let f = (max as f32 / pic.width() as f32).min(max as f32 / pic.height() as f32);
            let (w, h) = (((pic.width() as f32 * f) as usize).max(1), ((pic.height() as f32 * f) as usize).max(1));
            let mut px = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let c = pic.pixel((x as f32 / f) as u32, (y as f32 / f) as u32).unwrap_or_default();
                    px.extend_from_slice(&[c.r, c.g, c.b, 255]);
                }
            }
            egui::ColorImage::from_rgba_unmultiplied([w, h], &px)
        };
        let opts =
            TextureOptions { magnification: egui::TextureFilter::Nearest, minification: egui::TextureFilter::Linear, ..TextureOptions::default() };
        let tex = ctx.load_texture(format!("overlay-{mi}"), image, opts);
        let id = tex.id();
        if let Some(slot) = self.textures.get_mut(mi) {
            *slot = Some(tex);
        }
        Some(id)
    }

    /// Draws monitor `mi`'s window and takes its input. Call it from inside that monitor's
    /// viewport.
    pub fn show_monitor(&mut self, ui: &mut Ui, mi: usize) {
        let ctx = ui.ctx().clone();
        let Some(tex) = self.texture(&ctx, mi) else { return };
        let Some((pic_w, pic_h)) = self.monitors.get(mi).map(|m| (m.picture.width() as f32, m.picture.height() as f32)) else { return };
        // The whole window: the overlay is drawn edge to edge, whatever the Ui it was given.
        let screen = ctx.viewport_rect();
        if !self.focused.get(mi).copied().unwrap_or(true) && ctx.input(|i| i.pointer.hover_pos()).is_some() {
            ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
            if let Some(f) = self.focused.get_mut(mi) {
                *f = true;
            }
        }
        let sx = screen.width() / pic_w.max(1.0);
        let sy = screen.height() / pic_h.max(1.0);
        let to_px = |p: Pos2| (((p.x - screen.min.x) / sx).clamp(0.0, pic_w), ((p.y - screen.min.y) / sy).clamp(0.0, pic_h));
        let to_screen = |x: f32, y: f32| pos2(screen.min.x + x * sx, screen.min.y + y * sy);
        let rect_screen = |r: Rect| egui::Rect::from_min_max(to_screen(r.x, r.y), to_screen(r.right(), r.bottom()));

        let painter = ui.painter().clone();
        painter.image(tex, screen, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);

        let resp = ui.interact(screen, ui.id().with(("overlay", mi)), Sense::click_and_drag());
        let hover = resp.hover_pos();
        if let Some(h) = hover {
            self.pointer = Some((mi, to_px(h)));
        } else if self.pointer.is_some_and(|(m, _)| m == mi) && !ctx.input(|i| i.pointer.any_down()) {
            self.pointer = None;
        }
        self.input(&ctx, &resp, mi, &to_px, (pic_w, pic_h), sx);

        // ---- what to draw ----
        let dim = Color32::from_black_alpha(110);
        let lit: Option<Rect> = if let Some((m, r)) = self.sel {
            (m == mi).then_some(r)
        } else if self.phase == Phase::Choosing && matches!(self.purpose, Purpose::Picture | Purpose::Window) {
            self.pointer.filter(|(m, _)| *m == mi).and_then(|(_, p)| self.window_at(mi, p)).map(|(r, _)| r)
        } else {
            None
        };
        match lit {
            Some(r) => {
                let rs = rect_screen(r);
                for part in [
                    egui::Rect::from_min_max(screen.min, pos2(screen.max.x, rs.min.y)),
                    egui::Rect::from_min_max(pos2(screen.min.x, rs.max.y), screen.max),
                    egui::Rect::from_min_max(pos2(screen.min.x, rs.min.y), pos2(rs.min.x, rs.max.y)),
                    egui::Rect::from_min_max(pos2(rs.max.x, rs.min.y), pos2(screen.max.x, rs.max.y)),
                ] {
                    if part.is_positive() {
                        painter.rect_filled(part, 0.0, dim);
                    }
                }
                let accent = Color32::from_rgb(0x2E, 0xC4, 0xC0);
                let chosen = self.sel.is_some();
                painter.rect_stroke(rs, 0.0, Stroke::new(if chosen { 1.5 } else { 3.0 }, accent), StrokeKind::Outside);
                if chosen && matches!(self.phase, Phase::Adjusting | Phase::Moving { .. } | Phase::Resizing { .. }) {
                    for (x, y) in handles(r) {
                        let hr = egui::Rect::from_center_size(to_screen(x, y), vec2(HANDLE, HANDLE));
                        painter.rect_filled(hr, 2.0, Color32::WHITE);
                        painter.rect_stroke(hr, 2.0, Stroke::new(1.5, accent), StrokeKind::Middle);
                    }
                }
                // The size, outside the top left corner (inside if there's no room).
                let label = format!("{} × {}", r.w.round() as i64, r.h.round() as i64);
                let galley = painter.layout_no_wrap(label, FontId::proportional(13.0), Color32::WHITE);
                let mut at = rs.min + vec2(0.0, -galley.size().y - 10.0);
                if at.y < screen.min.y + 4.0 {
                    at = rs.min + vec2(6.0, 6.0);
                }
                let bg = egui::Rect::from_min_size(at, galley.size() + vec2(12.0, 6.0));
                painter.rect_filled(bg, 4.0, Color32::from_black_alpha(200));
                painter.galley(bg.min + vec2(6.0, 3.0), galley, Color32::WHITE);
                if chosen && self.phase == Phase::Adjusting {
                    self.action_bar(ui, mi, rs, screen);
                }
            }
            None => {
                painter.rect_filled(screen, 0.0, dim);
            }
        }

        // Crosshair and loupe while choosing.
        if let Some((m, p)) = self.pointer
            && m == mi
            && matches!(self.phase, Phase::Choosing | Phase::Dragging { .. })
        {
            let s = to_screen(p.0, p.1);
            if self.crosshair {
                let line = Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 150));
                painter.line_segment([pos2(screen.min.x, s.y), pos2(screen.max.x, s.y)], line);
                painter.line_segment([pos2(s.x, screen.min.y), pos2(s.x, screen.max.y)], line);
            }
            if self.magnifier {
                self.loupe(&painter, tex, screen, s, p, (pic_w, pic_h), mi);
            }
            ctx.set_cursor_icon(CursorIcon::Crosshair);
        }
        if self.sel.is_none() && self.phase == Phase::Choosing {
            self.hint(&painter, screen);
        }
    }

    fn hint(&self, painter: &egui::Painter, screen: egui::Rect) {
        let text = match self.purpose {
            Purpose::Picture => "Drag to capture an area  ·  Click a window  ·  Enter: the whole screen  ·  Esc: cancel",
            Purpose::Window => "Click the window to capture  ·  Esc: cancel",
            Purpose::Gif => "Drag out the area to record as a GIF  ·  Esc: cancel",
            Purpose::Scrolling => "Drag out the area that scrolls  ·  Esc: cancel",
        };
        let galley = painter.layout_no_wrap(text.to_string(), FontId::proportional(15.0), Color32::WHITE);
        let at = pos2(screen.center().x - galley.size().x / 2.0, screen.min.y + 28.0);
        let bg = egui::Rect::from_min_size(at - vec2(14.0, 8.0), galley.size() + vec2(28.0, 16.0));
        painter.rect_filled(bg, 10.0, Color32::from_black_alpha(190));
        painter.galley(at, galley, Color32::WHITE);
    }

    fn loupe(&self, painter: &egui::Painter, tex: egui::TextureId, screen: egui::Rect, s: Pos2, p: (f32, f32), size: (f32, f32), mi: usize) {
        const CELLS: f32 = 15.0;
        const BOX: f32 = 135.0;
        let (pw, ph) = size;
        let (cx, cy) = (p.0.floor(), p.1.floor());
        let half = (CELLS / 2.0).floor();
        let uv = egui::Rect::from_min_max(pos2((cx - half) / pw, (cy - half) / ph), pos2((cx + half + 1.0) / pw, (cy + half + 1.0) / ph));
        // Beside the pointer, on the side with room.
        let mut at = s + vec2(24.0, 24.0);
        if at.x + BOX > screen.max.x {
            at.x = s.x - 24.0 - BOX;
        }
        if at.y + BOX + 40.0 > screen.max.y {
            at.y = s.y - 24.0 - BOX - 40.0;
        }
        let r = egui::Rect::from_min_size(at, vec2(BOX, BOX));
        painter.rect_filled(r.expand(3.0), 6.0, Color32::from_black_alpha(220));
        painter.image(tex, r, uv, Color32::WHITE);
        let cell = BOX / CELLS;
        for i in 1..CELLS as i32 {
            let o = i as f32 * cell;
            let grid = Stroke::new(0.5, Color32::from_black_alpha(50));
            painter.line_segment([pos2(r.min.x + o, r.min.y), pos2(r.min.x + o, r.max.y)], grid);
            painter.line_segment([pos2(r.min.x, r.min.y + o), pos2(r.max.x, r.min.y + o)], grid);
        }
        let mid = egui::Rect::from_min_size(r.min + vec2(half * cell, half * cell), vec2(cell, cell));
        painter.rect_stroke(mid, 0.0, Stroke::new(1.5, Color32::WHITE), StrokeKind::Middle);
        painter.rect_stroke(mid.expand(1.5), 0.0, Stroke::new(1.0, Color32::BLACK), StrokeKind::Middle);
        let colour = self.monitors.get(mi).and_then(|m| m.picture.pixel(cx as u32, cy as u32)).map(|c| c.to_hex()).unwrap_or_default();
        let info = format!("{} , {}   {colour}", cx as i64, cy as i64);
        let label = egui::Rect::from_min_size(pos2(r.min.x - 3.0, r.max.y + 6.0), vec2(BOX + 6.0, 24.0));
        painter.rect_filled(label, 5.0, Color32::from_black_alpha(220));
        painter.text(label.center(), Align2::CENTER_CENTER, info, FontId::monospace(12.0), Color32::WHITE);
    }

    fn action_bar(&mut self, ui: &mut Ui, mi: usize, sel: egui::Rect, screen: egui::Rect) {
        let actions: Vec<(Icon, &str, Action)> = match self.purpose {
            Purpose::Gif => vec![(Icon::Gif, "Record GIF (Enter)", Action::Start)],
            Purpose::Scrolling => vec![(Icon::Scrolling, "Start scrolling capture (Enter)", Action::Start)],
            _ => vec![
                (Icon::Edit, "Edit (Enter)", Action::Edit),
                (Icon::Copy, "Copy (Ctrl+C)", Action::Copy),
                (Icon::Save, "Save (Ctrl+S)", Action::Save),
            ],
        };
        let w = 40.0 * (actions.len() as f32 + 1.0) + 12.0;
        let h = 44.0;
        let mut at = pos2(sel.max.x - w, sel.max.y + 10.0);
        if at.y + h > screen.max.y - 4.0 {
            at.y = (sel.min.y - h - 10.0).max(screen.min.y + 4.0);
            if at.y + h > sel.min.y {
                at.y = sel.max.y - h - 10.0;
            }
        }
        at.x = at.x.clamp(screen.min.x + 4.0, screen.max.x - w - 4.0);
        let bar = egui::Rect::from_min_size(at, vec2(w, h));
        let mut picked: Option<Action> = None;
        let mut cancel = false;
        egui::Area::new(egui::Id::new(("overlay-bar", mi))).fixed_pos(bar.min).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::NONE.fill(Color32::from_rgb(0x20, 0x20, 0x26)).corner_radius(10).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.style_mut().visuals = egui::Visuals::dark();
                    for (icon, tip, action) in &actions {
                        if icons::button(ui, *icon, tip, false, true).clicked() {
                            picked = Some(*action);
                        }
                        ui.add_space(4.0);
                    }
                    if icons::button(ui, Icon::Close, "Cancel (Esc)", false, true).clicked() {
                        cancel = true;
                    }
                });
            });
        });
        if let Some(a) = picked {
            self.choose(a);
        }
        if cancel {
            self.outcome = Outcome::Cancelled;
        }
    }

    fn input(&mut self, ctx: &egui::Context, resp: &egui::Response, mi: usize, to_px: &dyn Fn(Pos2) -> (f32, f32), size: (f32, f32), scale: f32) {
        if self.outcome != Outcome::Pending {
            return;
        }
        let (pw, ph) = size;
        let monitor_rect = Rect::new(0.0, 0.0, pw, ph);
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || resp.secondary_clicked() {
            self.outcome = Outcome::Cancelled;
            return;
        }
        let enter = ctx.input(|i| i.key_pressed(egui::Key::Enter));
        if enter {
            if self.sel.is_none() && self.purpose == Purpose::Picture {
                let m = self.pointer.map(|(m, _)| m).unwrap_or(mi);
                if let Some(mon) = self.monitors.get(m) {
                    self.sel = Some((m, whole(mon)));
                    self.title = Some(mon.name.clone());
                }
            }
            self.choose(Action::Default);
            return;
        }
        if self.sel.is_some() && self.phase == Phase::Adjusting {
            let copy = ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::C)));
            let save = ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S)));
            if copy {
                self.choose(Action::Copy);
                return;
            }
            if save {
                self.choose(Action::Save);
                return;
            }
            // Arrows move the area a pixel; with Shift they make it larger.
            if let Some((m, r)) = self.sel.as_mut()
                && *m == mi
            {
                let shift = ctx.input(|i| i.modifiers.shift);
                for (key, dx, dy) in [
                    (egui::Key::ArrowLeft, -1.0, 0.0),
                    (egui::Key::ArrowRight, 1.0, 0.0),
                    (egui::Key::ArrowUp, 0.0, -1.0),
                    (egui::Key::ArrowDown, 0.0, 1.0),
                ] {
                    if ctx.input(|i| i.key_pressed(key)) {
                        if shift {
                            r.w = (r.w + dx).max(1.0);
                            r.h = (r.h + dy).max(1.0);
                        } else {
                            r.x = (r.x + dx).clamp(0.0, (pw - r.w).max(0.0));
                            r.y = (r.y + dy).clamp(0.0, (ph - r.h).max(0.0));
                        }
                    }
                }
            }
        }
        let press = ctx.input(|i| i.pointer.press_origin());
        let latest = ctx.input(|i| i.pointer.latest_pos());
        if resp.double_clicked()
            && let (Some((m, r)), Some(p)) = (self.sel, latest)
            && m == mi
            && r.contains(screengrabcraft_model::Pt::new(to_px(p).0, to_px(p).1))
        {
            self.choose(Action::Default);
            return;
        }
        if resp.drag_started_by(egui::PointerButton::Primary)
            && let Some(start) = press
        {
            let p = to_px(start);
            let tol = HANDLE / scale.max(0.01);
            self.phase = match (self.sel, self.purpose) {
                (_, Purpose::Window) => Phase::Choosing,
                (Some((m, r)), _) if m == mi && self.phase == Phase::Adjusting => {
                    if let Some(h) = handles(r).iter().position(|(x, y)| (x - p.0).abs() <= tol && (y - p.1).abs() <= tol) {
                        Phase::Resizing { handle: h }
                    } else if r.contains(screengrabcraft_model::Pt::new(p.0, p.1)) {
                        Phase::Moving { last: p }
                    } else {
                        self.sel = None;
                        Phase::Dragging { start: p }
                    }
                }
                _ => {
                    self.sel = None;
                    Phase::Dragging { start: p }
                }
            };
            self.title = None;
        }
        if resp.dragged()
            && let Some(now) = latest
        {
            let p = to_px(now);
            match self.phase {
                Phase::Dragging { start } => {
                    let r = Rect::from_points(screengrabcraft_model::Pt::new(start.0, start.1), screengrabcraft_model::Pt::new(p.0, p.1));
                    self.sel = r.intersect(monitor_rect).map(|r| (mi, r));
                }
                Phase::Moving { last } => {
                    if let Some((_, r)) = self.sel.as_mut() {
                        r.x = (r.x + p.0 - last.0).clamp(0.0, (pw - r.w).max(0.0));
                        r.y = (r.y + p.1 - last.1).clamp(0.0, (ph - r.h).max(0.0));
                    }
                    self.phase = Phase::Moving { last: p };
                }
                Phase::Resizing { handle } => {
                    if let Some((_, r)) = self.sel.as_mut() {
                        let h = screengrabcraft_model::Handle::BOX.get(handle).copied().unwrap_or(screengrabcraft_model::Handle::BottomRight);
                        if let Some(nr) = h.drag_rect(*r, screengrabcraft_model::Pt::new(p.0, p.1), false).intersect(monitor_rect) {
                            *r = nr;
                        }
                    }
                }
                _ => {}
            }
        }
        let released = resp.drag_stopped() || (resp.clicked() && !resp.double_clicked());
        if released {
            match self.phase {
                Phase::Dragging { .. } | Phase::Choosing => {
                    let small = self.sel.is_none_or(|(_, r)| r.w < 3.0 || r.h < 3.0);
                    if small {
                        // A click: the window under it, else the whole screen.
                        let at = press.or(latest).map(to_px);
                        let window = at.and_then(|p| self.window_at(mi, p));
                        match (window, self.purpose) {
                            (Some((r, title)), _) => {
                                self.sel = r.intersect(monitor_rect).map(|r| (mi, r));
                                self.title = Some(title);
                            }
                            (None, Purpose::Window) => {
                                self.phase = Phase::Choosing;
                                return;
                            }
                            (None, _) => {
                                if let Some(m) = self.monitors.get(mi) {
                                    self.sel = Some((mi, whole(m)));
                                    self.title = Some(m.name.clone());
                                }
                            }
                        }
                    }
                    if self.sel.is_none() {
                        self.phase = Phase::Choosing;
                        return;
                    }
                    if self.preview || matches!(self.purpose, Purpose::Gif | Purpose::Scrolling) {
                        self.phase = Phase::Adjusting;
                    } else {
                        self.choose(Action::Default);
                    }
                }
                Phase::Moving { .. } | Phase::Resizing { .. } => self.phase = Phase::Adjusting,
                Phase::Adjusting => {}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    fn overlay() -> Overlay {
        let m = OverlayMonitor { rect: IRect::new(-100, 0, 100, 50), picture: Raster::filled(200, 100, Color::WHITE).unwrap(), name: "left".into() };
        let w = OverlayWindow { rect: IRect::new(-90, 10, 40, 20), title: "Notes".into() };
        Overlay::new(vec![m], vec![w], Purpose::Picture)
    }

    #[test]
    fn coordinates_map_between_desktop_and_picture() {
        let o = overlay();
        let m = &o.monitors[0];
        assert_eq!(desk_to_px(m, IRect::new(-90, 10, 40, 20)), Some(Rect::new(20.0, 20.0, 80.0, 40.0)));
        assert_eq!(px_to_desk(m, Rect::new(20.0, 20.0, 80.0, 40.0)), IRect::new(-90, 10, 40, 20));
        assert_eq!(o.window_at(0, (30.0, 30.0)).map(|w| w.1), Some("Notes".to_string()));
        assert!(o.window_at(0, (150.0, 90.0)).is_none());
    }

    #[test]
    fn choosing_reports_pixels_and_desktop() {
        let mut o = overlay();
        o.preselect(0, IRect::new(20, 20, 80, 40));
        o.choose(Action::Copy);
        assert_eq!(
            o.outcome(),
            &Outcome::Chosen {
                monitor: 0,
                area: IRect::new(20, 20, 80, 40),
                desktop: IRect::new(-90, 10, 40, 20),
                action: Action::Copy,
                title: None
            }
        );
        assert_eq!(pixels(Rect::new(-5.0, 2.4, 1000.0, 10.0), 200, 100), IRect::new(0, 2, 200, 11));
    }
}
