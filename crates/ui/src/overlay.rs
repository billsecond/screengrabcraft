//! The capture overlay: the frozen screen, on every monitor, while the owner picks what to keep.
//!
//! The capture service takes a picture of each monitor first and shows it full screen, so what
//! is chosen is exactly what was on screen when the shortcut was pressed (menus and tooltips
//! included). Drag out an area, or click a window, and the owner's default runs; or, when the
//! settings ask for the bar, adjust the area with its handles and pick what to do with it from the
//! bar under it, or press Enter for the default.
//!
//! One [`Overlay`] is shared by all the monitors' windows: [`Overlay::show_monitor`] draws one
//! monitor's window and takes its input. Areas are in the pixels of a monitor's picture.
//!
//! Several areas of one screen make one capture: each dragged out with Alt (Option on a Mac)
//! held is kept and another can be dragged out; the last one without it, or Enter, keeps them
//! all, laid out as they were with the rest see-through. A freehand area ([`Purpose::Freehand`])
//! is drawn round with the button held down, and what's outside it is see-through.

use egui::{Align2, Color32, CursorIcon, FontId, Pos2, Sense, Stroke, StrokeKind, TextureOptions, Ui, pos2, vec2};
use screengrabcraft_model::{IRect, Raster, Rect};
pub use screengrabcraft_render::area::AreaShape;
use screengrabcraft_render::area::{MAX_BOXES, MAX_POINTS};

use crate::icons::{self, Icon};
use crate::widgets::{CONTROL_RADIUS, WINDOW_RADIUS, strong_font};

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
    /// A video: Record Video or Video Production.
    Video,
    /// A video of a window: click one (Video Production's Recorder).
    VideoWindow,
    /// A video of a set size, in the monitor's pixels: a frame of that size follows the pointer
    /// and a click places it (Video Production's Recorder). It is moved, not resized.
    VideoFixed(u32, u32),
    /// A picture of a set size, in the monitor's pixels: placed like [`Purpose::VideoFixed`], kept
    /// like a picture.
    PictureFixed(u32, u32),
    /// A picture of an area drawn round freehand: what's outside the outline is see-through.
    Freehand,
}

impl Purpose {
    /// Whether a window is clicked rather than an area dragged out.
    fn clicks_window(self) -> bool {
        matches!(self, Purpose::Window | Purpose::VideoWindow)
    }

    /// Whether the area is for a recording, which waits on its bar to be started.
    fn records(self) -> bool {
        matches!(self, Purpose::Gif | Purpose::Scrolling | Purpose::Video | Purpose::VideoWindow | Purpose::VideoFixed(..))
    }

    /// The fixed size of the frame, if it has one.
    fn fixed(self) -> Option<(u32, u32)> {
        match self {
            Purpose::VideoFixed(w, h) | Purpose::PictureFixed(w, h) => Some((w, h)),
            _ => None,
        }
    }

    /// Whether an area is dragged out (and so can be held to a shape).
    fn drags_area(self) -> bool {
        !self.clicks_window() && self.fixed().is_none() && self != Purpose::Freehand
    }

    /// Whether the frame of a recording, once chosen, has a grip in its middle to move it by (a
    /// window clicked is that window's, so it isn't moved).
    fn moves_by_grip(self) -> bool {
        self.records() && !self.clicks_window()
    }
}

/// The key held while dragging out an area to keep it and drag out another, as the hint says it.
const MORE_KEY: &str = if cfg!(target_os = "macos") { "Option" } else { "Alt" };

/// The shape held while an area is dragged out, by the keys held: Shift a square, Ctrl (Cmd on a
/// Mac) 16:9, both 4:3.
pub fn held_shape(m: egui::Modifiers) -> Option<(u32, u32)> {
    match (m.command, m.shift) {
        (true, true) => Some((4, 3)),
        (true, false) => Some((16, 9)),
        (false, true) => Some((1, 1)),
        (false, false) => None,
    }
}

/// A shape as the area's label says it: `16:9`, `square`.
fn shape_name(w: u32, h: u32) -> String {
    if w == h { "square".to_string() } else { format!("{w}:{h}") }
}

/// The keys that hold a shape, as the hint says them.
const SHAPE_KEYS: &str =
    if cfg!(target_os = "macos") { "Shift: square  ·  Cmd: 16:9  ·  Cmd+Shift: 4:3" } else { "Shift: square  ·  Ctrl: 16:9  ·  Ctrl+Shift: 4:3" };

/// The area dragged from `start` to `p` held to `shape` (`w` wide for every `h` tall): as large as
/// the drag's longer side asks, toward the pointer, made smaller (keeping its shape) to stay on a
/// monitor of `mon` pixels. Whole pixels where the shape allows.
pub fn shaped_area(start: (f32, f32), p: (f32, f32), shape: (u32, u32), mon: (f32, f32)) -> Rect {
    let ratio = shape.0.max(1) as f32 / shape.1.max(1) as f32;
    let (dx, dy) = (p.0 - start.0, p.1 - start.1);
    let (dx, dy) = (if dx.is_finite() { dx } else { 0.0 }, if dy.is_finite() { dy } else { 0.0 });
    let (mut w, mut h) = (dx.abs(), dy.abs());
    if w >= h * ratio {
        h = w / ratio;
    } else {
        w = h * ratio;
    }
    // The room there is toward the pointer.
    let room_x = if dx >= 0.0 { mon.0 - start.0 } else { start.0 };
    let room_y = if dy >= 0.0 { mon.1 - start.1 } else { start.1 };
    let fit = (room_x.max(0.0) / w.max(f32::EPSILON)).min(room_y.max(0.0) / h.max(f32::EPSILON)).min(1.0);
    let w = (w * fit).floor().max(0.0);
    let h = (w / ratio).round().min(room_y.max(0.0).floor());
    let x = if dx >= 0.0 { start.0 } else { start.0 - w };
    let y = if dy >= 0.0 { start.1 } else { start.1 - h };
    Rect::new(x, y, w, h)
}

/// A frame of `size` pixels centred on `p`, kept on a monitor of `mon` pixels: it's made smaller,
/// keeping its shape, when it's larger than the monitor, and its corner is on a whole pixel.
pub fn fixed_frame(size: (u32, u32), p: (f32, f32), mon: (f32, f32)) -> Rect {
    let (mw, mh) = (mon.0.max(1.0), mon.1.max(1.0));
    let (w, h) = (size.0.max(1) as f32, size.1.max(1) as f32);
    let f = (mw / w).min(mh / h).min(1.0);
    let (w, h) = ((w * f).floor().max(1.0), (h * f).floor().max(1.0));
    let x = (p.0 - w / 2.0).round().min(mw - w).max(0.0);
    let y = (p.1 - h / 2.0).round().min(mh - h).max(0.0);
    Rect::new(x, y, w, h)
}

/// A fixed frame in whole pixels, its size exactly as chosen (rounding its corner can't make it
/// a pixel larger).
fn fixed_pixels(r: Rect, w: u32, h: u32) -> IRect {
    let fw = (r.w.round().max(1.0) as u32).min(w.max(1));
    let fh = (r.h.round().max(1.0) as u32).min(h.max(1));
    let x = r.x.round().min(w.saturating_sub(fw) as f32).max(0.0);
    let y = r.y.round().min(h.saturating_sub(fh) as f32).max(0.0);
    IRect::new(x as i32, y as i32, fw, fh)
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
        /// What of `area` is kept, in the monitor's picture's pixels, when it isn't all of it:
        /// several areas (`area` the box round them), or a freehand outline.
        shape: Option<AreaShape>,
    },
}

/// Where the choosing is. `Moving`'s `grip`: moved by the grip in a recording's frame.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    Choosing,
    Dragging { start: (f32, f32) },
    Adjusting,
    Moving { last: (f32, f32), grip: bool },
    Resizing { handle: usize },
}

pub struct Overlay {
    pub monitors: Vec<OverlayMonitor>,
    pub windows: Vec<OverlayWindow>,
    pub purpose: Purpose,
    /// Keep the area adjustable with a bar of actions after it is chosen.
    pub preview: bool,
    /// The bar's button that does what Enter does, or [`Action::Default`] when none of them does
    /// all of it and the default gets a button of its own.
    pub enter: Action,
    pub magnifier: bool,
    pub crosshair: bool,
    /// The shape every area dragged out is held to (a preset's), else the keys held say.
    pub shape: Option<(u32, u32)>,
    /// The shape the area being dragged out (or last dragged out) is held to, for its label.
    held: Option<(u32, u32)>,
    textures: Vec<Option<egui::TextureHandle>>,
    /// The monitor of the area, and the area in its pixels.
    sel: Option<(usize, Rect)>,
    title: Option<String>,
    phase: Phase,
    /// The monitor the pointer is on and where (its pixels).
    pointer: Option<(usize, (f32, f32))>,
    outcome: Outcome,
    focused: Vec<bool>,
    /// The areas kept so far for a capture of several (each dragged out with [`MORE_KEY`] held),
    /// all on monitor `kept_on`.
    kept: Vec<Rect>,
    kept_on: usize,
    /// The outline of a freehand area, being drawn or drawn (its monitor's pixels; the monitor is
    /// the area's).
    path: Vec<(f32, f32)>,
    path_on: usize,
    /// What's outside the drawn outline, dimmed, as a picture over the box round it.
    path_shade: Option<egui::TextureHandle>,
}

const HANDLE: f32 = 9.0;
/// How far the grip in a recording's frame reaches from its middle, in screen points.
const GRIP_RADIUS: f32 = 17.0;
/// How far an arrow key moves the area (or, with Shift, makes it larger), in pixels; with Ctrl
/// (Cmd on a Mac) held, [`NUDGE_FAR`].
const NUDGE: f32 = 1.0;
const NUDGE_FAR: f32 = 10.0;
/// The overlay's colour: the chosen area's edge and the capture bar's main buttons. Bright, to
/// be seen on any picture.
const ACCENT: Color32 = Color32::from_rgb(0x2E, 0xC4, 0xC0);
/// The height of a button of the capture bar.
const BAR_BUTTON: f32 = 56.0;
/// Where a button's words start, after its icon.
const BAR_TEXT: f32 = 46.0;

/// How much a button of the capture bar stands out.
#[derive(Clone, Copy, PartialEq)]
enum Weight {
    Main,
    Plain,
    Quiet,
}

/// A button of the capture bar.
struct BarButton {
    icon: Icon,
    label: &'static str,
    /// The key that does the same, if any.
    keys: &'static str,
    weight: Weight,
    /// What it chooses; none cancels.
    action: Option<Action>,
}

fn bar_button(ui: &mut Ui, b: &BarButton, width: f32, font: &FontId) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(vec2(width, BAR_BUTTON), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, b.label));
    let hot = resp.hovered();
    let (fill, ink) = match b.weight {
        Weight::Main => (if hot { Color32::from_rgb(0x62, 0xE3, 0xDF) } else { ACCENT }, Color32::from_rgb(0x06, 0x24, 0x24)),
        Weight::Plain => (if hot { Color32::from_rgb(0x4A, 0x52, 0x60) } else { Color32::from_rgb(0x34, 0x3A, 0x46) }, Color32::WHITE),
        Weight::Quiet => (if hot { Color32::from_rgb(0x2C, 0x30, 0x3A) } else { Color32::TRANSPARENT }, Color32::from_rgb(0xD5, 0xD9, 0xE0)),
    };
    let p = ui.painter();
    // Outlined as well, so Cancel reads as a button too.
    let edge = if b.weight == Weight::Quiet { Color32::from_rgb(0x4A, 0x52, 0x60) } else { fill };
    p.rect(rect, CONTROL_RADIUS, fill, Stroke::new(1.0, edge), StrokeKind::Inside);
    let icon = egui::Rect::from_center_size(pos2(rect.min.x + 25.0, rect.center().y), vec2(24.0, 24.0));
    icons::paint(p, icon, b.icon, ink);
    let x = rect.min.x + BAR_TEXT;
    if b.keys.is_empty() {
        p.text(pos2(x, rect.center().y), Align2::LEFT_CENTER, b.label, font.clone(), ink);
    } else {
        p.text(pos2(x, rect.center().y - 9.0), Align2::LEFT_CENTER, b.label, font.clone(), ink);
        p.text(pos2(x, rect.center().y + 12.0), Align2::LEFT_CENTER, b.keys, FontId::proportional(12.0), ink.gamma_multiply(0.75));
    }
    resp.on_hover_cursor(CursorIcon::PointingHand)
}

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

/// Dims all of `screen` but `holes`: in bands between the holes' tops and bottoms, the parts of
/// each band outside every hole. Nothing is dimmed twice.
fn dim_outside(painter: &egui::Painter, screen: egui::Rect, holes: &[egui::Rect], dim: Color32) {
    let holes: Vec<egui::Rect> = holes.iter().map(|h| h.intersect(screen)).filter(|h| h.is_positive()).collect();
    let mut ys: Vec<f32> = holes.iter().flat_map(|h| [h.min.y, h.max.y]).chain([screen.min.y, screen.max.y]).collect();
    ys.sort_by(f32::total_cmp);
    ys.dedup();
    for band in ys.windows(2) {
        let [y0, y1] = band else { continue };
        let mid = (y0 + y1) / 2.0;
        let mut spans: Vec<(f32, f32)> = holes.iter().filter(|h| h.min.y <= mid && h.max.y > mid).map(|h| (h.min.x, h.max.x)).collect();
        spans.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut x = screen.min.x;
        for (a, b) in spans.into_iter().chain([(screen.max.x, screen.max.x)]) {
            if a > x {
                painter.rect_filled(egui::Rect::from_min_max(pos2(x, *y0), pos2(a, *y1)), 0.0, dim);
            }
            x = x.max(b);
        }
    }
}

fn handles(r: Rect) -> [(f32, f32); 8] {
    let (cx, cy) = (r.x + r.w / 2.0, r.y + r.h / 2.0);
    [(r.x, r.y), (cx, r.y), (r.right(), r.y), (r.right(), cy), (r.right(), r.bottom()), (cx, r.bottom()), (r.x, r.bottom()), (r.x, cy)]
}

/// The handle of `r` at `p` (within `tol`), by its place in [`handles`].
fn handle_under(r: Rect, p: (f32, f32), tol: f32) -> Option<usize> {
    handles(r).iter().position(|(x, y)| (x - p.0).abs() <= tol && (y - p.1).abs() <= tol)
}

/// The pointer over a handle: the arrows of the way it pulls.
fn resize_cursor(handle: usize) -> CursorIcon {
    match handle {
        0 | 4 => CursorIcon::ResizeNwSe,
        2 | 6 => CursorIcon::ResizeNeSw,
        1 | 5 => CursorIcon::ResizeVertical,
        _ => CursorIcon::ResizeHorizontal,
    }
}

/// The middle of area `r`'s grip (a monitor of `mon` pixels, shown `scale` screen points to a
/// pixel), where there's room for it clear of the handles and the area can move at all (it isn't
/// the whole monitor).
fn grip_at(r: Rect, mon: (f32, f32), scale: f32) -> Option<(f32, f32)> {
    let room = GRIP_RADIUS * 2.0 + HANDLE * 4.0;
    let scale = if scale.is_finite() { scale.max(0.01) } else { 1.0 };
    let movable = r.w < mon.0 - 0.5 || r.h < mon.1 - 0.5;
    (movable && r.w * scale >= room && r.h * scale >= room).then(|| (r.x + r.w / 2.0, r.y + r.h / 2.0))
}

/// The grip that moves a recording's frame, at `c` (screen points): a round button with arrows
/// four ways, in the overlay's colour while it's under the pointer or held.
fn paint_grip(painter: &egui::Painter, c: Pos2, hot: bool) {
    let (fill, ink) =
        if hot { (ACCENT, Color32::from_rgb(0x06, 0x24, 0x24)) } else { (Color32::from_rgba_unmultiplied(0x14, 0x16, 0x1C, 225), Color32::WHITE) };
    painter.circle_filled(c + vec2(0.0, 2.0), GRIP_RADIUS + 1.0, Color32::from_black_alpha(90));
    painter.circle(c, GRIP_RADIUS, fill, Stroke::new(2.0, ACCENT));
    let arm = GRIP_RADIUS * 0.66;
    let head = 4.6;
    let line = Stroke::new(2.0, ink);
    painter.line_segment([c - vec2(arm - head, 0.0), c + vec2(arm - head, 0.0)], line);
    painter.line_segment([c - vec2(0.0, arm - head), c + vec2(0.0, arm - head)], line);
    for d in [vec2(1.0, 0.0), vec2(-1.0, 0.0), vec2(0.0, 1.0), vec2(0.0, -1.0)] {
        let (tip, base, side) = (c + d * arm, c + d * (arm - head), vec2(-d.y, d.x) * head);
        painter.add(egui::Shape::convex_polygon(vec![tip, base + side, base - side], ink, Stroke::NONE));
    }
}

impl Overlay {
    pub fn new(monitors: Vec<OverlayMonitor>, windows: Vec<OverlayWindow>, purpose: Purpose) -> Overlay {
        let n = monitors.len();
        Overlay {
            monitors,
            windows,
            purpose,
            preview: true,
            enter: Action::Default,
            magnifier: true,
            crosshair: true,
            shape: None,
            held: None,
            textures: (0..n).map(|_| None).collect(),
            sel: None,
            title: None,
            phase: Phase::Choosing,
            pointer: None,
            outcome: Outcome::Pending,
            focused: vec![false; n],
            kept: Vec::new(),
            kept_on: 0,
            path: Vec::new(),
            path_on: 0,
            path_shade: None,
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

    /// Starts with an area of the desktop already chosen, on the monitor its middle is on, ready
    /// to adjust (Capture Region, Adjust First starts from the last area). An area on no monitor
    /// now is left out.
    pub fn preselect_desktop(&mut self, area: IRect) {
        let (cx, cy) = (i64::from(area.x) + i64::from(area.w / 2), i64::from(area.y) + i64::from(area.h / 2));
        let Some(mi) = self.monitors.iter().position(|m| m.rect.contains(cx, cy)) else { return };
        let Some(r) = self.monitors.get(mi).and_then(|m| desk_to_px(m, area)) else { return };
        if r.w >= 1.0 && r.h >= 1.0 {
            self.sel = Some((mi, r));
            self.phase = Phase::Adjusting;
        }
    }

    fn choose(&mut self, action: Action) {
        // The area being adjusted, else (Enter after some were kept) the last one kept.
        let Some((mi, r)) = self.sel.or_else(|| self.kept.last().map(|r| (self.kept_on, *r))) else { return };
        let Some(m) = self.monitors.get(mi) else { return };
        let (pw, ph) = (m.picture.width(), m.picture.height());
        let (area, shape) = if self.purpose == Purpose::Freehand {
            (pixels(r, pw, ph), Some(AreaShape::Freehand(self.path.clone())))
        } else if self.purpose.fixed().is_some() {
            (fixed_pixels(r, pw, ph), None)
        } else {
            // With the ones kept before it, on its monitor.
            let mut boxes: Vec<IRect> = if self.kept_on == mi { self.kept.iter().map(|k| pixels(*k, pw, ph)).collect() } else { Vec::new() };
            if self.sel.is_some() {
                boxes.push(pixels(r, pw, ph));
            }
            match boxes.as_slice() {
                [one] => (*one, None),
                _ => {
                    let shape = AreaShape::Boxes(boxes);
                    (shape.bounds().unwrap_or_else(|| pixels(r, pw, ph)), Some(shape))
                }
            }
        };
        let desktop = px_to_desk(m, Rect::new(area.x as f32, area.y as f32, area.w as f32, area.h as f32));
        let title = if shape.is_some() { None } else { self.title.clone() };
        self.outcome = Outcome::Chosen { monitor: mi, area, desktop, action, title, shape };
    }

    /// Keeps area `r` of monitor `mi` for a capture of several, and gets ready for the next. The
    /// ones kept on another monitor are let go: a capture of several is of one monitor.
    fn keep_area(&mut self, mi: usize, r: Rect) {
        if self.kept_on != mi {
            self.kept.clear();
        }
        self.kept_on = mi;
        if self.kept.len() < MAX_BOXES.saturating_sub(1) {
            self.kept.push(r);
        }
        self.sel = None;
        self.title = None;
        self.phase = Phase::Choosing;
    }

    /// The areas kept so far on monitor `mi`.
    fn kept_on(&self, mi: usize) -> &[Rect] {
        if self.kept_on == mi { &self.kept } else { &[] }
    }

    /// Where the grip of a recording's frame on monitor `mi` (`size` pixels, shown `scale` points
    /// to a pixel) is, while the frame is there to be adjusted and has room for it.
    fn grip(&self, mi: usize, size: (f32, f32), scale: f32) -> Option<(f32, f32)> {
        let (m, r) = self.sel?;
        let shown = matches!(self.phase, Phase::Adjusting | Phase::Moving { .. } | Phase::Resizing { .. });
        if m != mi || !shown || !self.purpose.moves_by_grip() {
            return None;
        }
        grip_at(r, size, scale)
    }

    /// Whether `p` (monitor `mi`'s pixels) is on the grip of the frame being adjusted.
    fn over_grip(&self, mi: usize, p: (f32, f32), size: (f32, f32), scale: f32) -> bool {
        let scale = if scale.is_finite() { scale.max(0.01) } else { 1.0 };
        self.grip(mi, size, scale).is_some_and(|(cx, cy)| {
            let (dx, dy) = ((p.0 - cx) * scale, (p.1 - cy) * scale);
            dx * dx + dy * dy <= GRIP_RADIUS * GRIP_RADIUS
        })
    }

    /// The box round the freehand outline drawn on a monitor of `size` pixels, if it's an area
    /// (three points or more, three pixels across or more).
    fn path_bounds(&self, size: (f32, f32)) -> Option<Rect> {
        if self.path.len() < 3 {
            return None;
        }
        let b = AreaShape::Freehand(self.path.clone()).bounds()?;
        let r = Rect::new(b.x as f32, b.y as f32, b.w as f32, b.h as f32).intersect(Rect::new(0.0, 0.0, size.0, size.1))?;
        (r.w >= 3.0 && r.h >= 3.0).then_some(r)
    }

    /// The dimmed outside of the freehand outline over the box `bounds` round it, as a picture no
    /// more than 512 pixels a side.
    fn shade_texture(&mut self, ctx: &egui::Context, bounds: Rect) -> Option<egui::TextureId> {
        if let Some(t) = &self.path_shade {
            return Some(t.id());
        }
        let f = (512.0 / bounds.w.max(bounds.h).max(1.0)).min(1.0);
        let (w, h) = (((bounds.w * f).ceil() as u32).max(1), ((bounds.h * f).ceil() as u32).max(1));
        let (bx, by) = (bounds.x.floor() as i32, bounds.y.floor() as i32);
        let shape = AreaShape::Freehand(self.path.clone()).moved(-bx, -by).scaled(f);
        let cover = screengrabcraft_render::area::coverage(&shape, w, h)?;
        let px: Vec<u8> = cover.iter().flat_map(|c| [0, 0, 0, ((255 - u16::from(*c)) * 110 / 255) as u8]).collect();
        let image = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &px);
        let tex = ctx.load_texture("overlay-freehand", image, TextureOptions::LINEAR);
        let id = tex.id();
        self.path_shade = Some(tex);
        Some(id)
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
        if let Some(h) = hover
            && let Some(cursor) = self.adjusting_cursor(mi, to_px(h), (pic_w, pic_h), sx)
        {
            ctx.set_cursor_icon(cursor);
        }

        // ---- what to draw ----
        let dim = Color32::from_black_alpha(110);
        let lit: Option<Rect> = if let Some((m, r)) = self.sel {
            (m == mi).then_some(r)
        } else if let (Some(size), Phase::Choosing) = (self.purpose.fixed(), self.phase) {
            // The frame follows the pointer until a click places it.
            self.pointer.filter(|(m, _)| *m == mi).map(|(_, p)| fixed_frame(size, p, (pic_w, pic_h)))
        } else if self.phase == Phase::Choosing && (self.purpose == Purpose::Picture || self.purpose.clicks_window()) {
            self.pointer.filter(|(m, _)| *m == mi).and_then(|(_, p)| self.window_at(mi, p)).map(|(r, _)| r)
        } else {
            None
        };
        // The areas kept for a capture of several, and the one lit now, are shown as they are;
        // the rest is dimmed.
        let kept: Vec<egui::Rect> = self.kept_on(mi).iter().map(|r| rect_screen(*r)).collect();
        let holes: Vec<egui::Rect> = kept.iter().copied().chain(lit.map(rect_screen)).collect();
        dim_outside(&painter, screen, &holes, dim);
        for (i, rs) in kept.iter().enumerate() {
            painter.rect_stroke(*rs, 0.0, Stroke::new(1.5, ACCENT), StrokeKind::Outside);
            let badge = pos2(rs.min.x + 13.0, rs.min.y + 13.0);
            painter.circle_filled(badge, 10.0, ACCENT);
            painter.text(badge, Align2::CENTER_CENTER, (i + 1).to_string(), strong_font(&ctx, 12.0), Color32::from_rgb(0x06, 0x24, 0x24));
        }
        // A freehand outline: while it's drawn, and once it is, its outside dimmed in its box.
        let freehand = self.purpose == Purpose::Freehand;
        if freehand && self.path.len() >= 2 {
            if let Some((m, r)) = self.sel
                && self.phase == Phase::Adjusting
                && m == mi
                && let Some(shade) = self.shade_texture(&ctx, r)
            {
                painter.image(shade, rect_screen(r), egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
            }
            if self.path_on == mi {
                let pts: Vec<Pos2> = self.path.iter().map(|(x, y)| to_screen(*x, *y)).collect();
                let line = Stroke::new(2.0, ACCENT);
                if self.sel.is_some() {
                    painter.add(egui::Shape::closed_line(pts, line));
                } else {
                    // The way back to where it started, as it will be closed.
                    if let (Some(a), Some(b)) = (pts.first(), pts.last()) {
                        painter.extend(egui::Shape::dashed_line(&[*b, *a], Stroke::new(1.5, ACCENT.gamma_multiply(0.7)), 6.0, 5.0));
                    }
                    painter.add(egui::Shape::line(pts, line));
                }
            }
        }
        if let Some(r) = lit {
            let rs = rect_screen(r);
            let chosen = self.sel.is_some();
            if !freehand {
                painter.rect_stroke(rs, 0.0, Stroke::new(if chosen { 1.5 } else { 3.0 }, ACCENT), StrokeKind::Outside);
            }
            // A fixed size has no handles: it's moved, not resized; nor has a freehand outline.
            if chosen
                && self.purpose.fixed().is_none()
                && !freehand
                && matches!(self.phase, Phase::Adjusting | Phase::Moving { .. } | Phase::Resizing { .. })
            {
                for (x, y) in handles(r) {
                    let hr = egui::Rect::from_center_size(to_screen(x, y), vec2(HANDLE, HANDLE));
                    painter.rect_filled(hr, 2.0, Color32::WHITE);
                    painter.rect_stroke(hr, 2.0, Stroke::new(1.5, ACCENT), StrokeKind::Middle);
                }
            }
            // A recording's frame: the grip in its middle that moves it.
            if let Some((gx, gy)) = self.grip(mi, (pic_w, pic_h), sx) {
                let hot = match self.phase {
                    Phase::Moving { grip, .. } => grip,
                    Phase::Adjusting => self.pointer.is_some_and(|(m, p)| m == mi && self.over_grip(mi, p, (pic_w, pic_h), sx)),
                    _ => false,
                };
                paint_grip(&painter, to_screen(gx, gy), hot);
            }
            // The size, outside the top left corner (inside if there's no room).
            let mut label = format!("{} × {}", r.w.round() as i64, r.h.round() as i64);
            if let Some((w, h)) = self.held.filter(|_| self.title.is_none()) {
                label = format!("{label}   {}", shape_name(w, h));
            }
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
            if self.magnifier && self.purpose.fixed().is_none() {
                self.loupe(&painter, tex, screen, s, p, (pic_w, pic_h), mi);
            }
            ctx.set_cursor_icon(CursorIcon::Crosshair);
        }
        if self.sel.is_none() && self.phase == Phase::Choosing {
            self.hint(&painter, screen);
        } else if let Some((m, r)) = self.sel
            && m == mi
            && self.purpose.moves_by_grip()
            && matches!(self.phase, Phase::Adjusting | Phase::Moving { .. } | Phase::Resizing { .. })
        {
            self.move_hint(&painter, screen, rect_screen(r), self.grip(mi, (pic_w, pic_h), sx).is_some());
        }
    }

    /// While the area is being adjusted, what the pointer at `p` (this monitor's pixels, `size`
    /// of them, shown `scale` points to a pixel) shows a press would do: arrows four ways over a
    /// recording's grip (and while it's moved by it), arrows over a handle, a hand inside the area
    /// (a closed one while it's moved), a crosshair outside it, where a drag starts a new area.
    fn adjusting_cursor(&self, mi: usize, p: (f32, f32), size: (f32, f32), scale: f32) -> Option<CursorIcon> {
        let tol = HANDLE / scale.max(0.01);
        match self.phase {
            Phase::Resizing { handle } => Some(resize_cursor(handle)),
            Phase::Moving { grip: true, .. } => Some(CursorIcon::Move),
            Phase::Moving { .. } => Some(CursorIcon::Grabbing),
            Phase::Adjusting => {
                let (m, r) = self.sel?;
                // A freehand area isn't moved or pulled: a drag draws another.
                if m != mi || self.purpose.clicks_window() || self.purpose == Purpose::Freehand {
                    return Some(CursorIcon::Crosshair);
                }
                if self.over_grip(mi, p, size, scale) {
                    return Some(CursorIcon::Move);
                }
                // A fixed size has no handles: it's moved, not resized.
                if self.purpose.fixed().is_none()
                    && let Some(h) = handle_under(r, p, tol)
                {
                    return Some(resize_cursor(h));
                }
                Some(if r.contains(screengrabcraft_model::Pt::new(p.0, p.1)) { CursorIcon::Grab } else { CursorIcon::Crosshair })
            }
            Phase::Choosing | Phase::Dragging { .. } => None,
        }
    }

    fn hint(&self, painter: &egui::Painter, screen: egui::Rect) {
        let several = !self.kept.is_empty() && self.purpose == Purpose::Picture;
        let text = match self.purpose {
            Purpose::Picture if several => "Drag out the last area  ·  Enter: keep these  ·  Esc: cancel",
            Purpose::Picture => "Drag to capture an area  ·  Click a window  ·  Enter: the whole screen  ·  Esc: cancel",
            Purpose::Window => "Click the window to capture  ·  Esc: cancel",
            Purpose::Gif => "Drag out the area to record as a GIF  ·  Esc: cancel",
            Purpose::Scrolling => "Drag out the area that scrolls  ·  Esc: cancel",
            Purpose::Video => "Drag out the area to record as a video  ·  Esc: cancel",
            Purpose::VideoWindow => "Click the window to record  ·  Esc: cancel",
            Purpose::VideoFixed(..) => "Click to place the frame to record  ·  Esc: cancel",
            Purpose::PictureFixed(..) => "Click to place the area to capture  ·  Esc: cancel",
            Purpose::Freehand => "Hold the button down and draw round what to capture  ·  Esc: cancel",
        };
        // Under it, the shape an area is held to, or the keys that hold one; and for a picture,
        // the key that keeps an area and lets another be dragged out.
        let shapes = match self.shape {
            _ if several => {
                let n = self.kept.len();
                Some(format!("{n} area{} kept  ·  {MORE_KEY}: one more  ·  Backspace: take the last one out", if n == 1 { "" } else { "s" }))
            }
            _ if !self.purpose.drags_area() => None,
            Some((w, h)) => Some(format!("Held to {}", shape_name(w, h))),
            None => Some(SHAPE_KEYS.to_string()),
        };
        let shapes = match shapes {
            Some(s) if self.purpose == Purpose::Picture && !several => Some(format!("{s}  ·  {MORE_KEY}: several areas")),
            other => other,
        };
        // Wrapped to the monitor: on a narrow one (or a small one at a large scale) it never runs
        // off its edges.
        let room = (screen.width() - 60.0).max(80.0);
        let galley = painter.layout(text.to_string(), FontId::proportional(15.0), Color32::WHITE, room);
        let small = shapes.map(|s| painter.layout(s, FontId::proportional(12.5), Color32::from_gray(210), room));
        let extra = small.as_ref().map_or(0.0, |g| g.size().y + 4.0);
        let wide = small.as_ref().map_or(galley.size().x, |g| g.size().x.max(galley.size().x));
        let at = pos2(screen.center().x - wide / 2.0, screen.min.y + 28.0);
        let bg = egui::Rect::from_min_size(at - vec2(14.0, 8.0), vec2(wide, galley.size().y + extra) + vec2(28.0, 16.0));
        painter.rect_filled(bg, 10.0, Color32::from_black_alpha(190));
        let below = at.y + galley.size().y + 4.0;
        painter.galley(pos2(screen.center().x - galley.size().x / 2.0, at.y), galley, Color32::WHITE);
        if let Some(g) = small {
            painter.galley(pos2(screen.center().x - g.size().x / 2.0, below), g, Color32::from_gray(210));
        }
    }

    /// While a recording's frame (`frame`, on screen) is adjusted, how to move it: at the top of
    /// the screen, else at the bottom where the frame is in the way, else not at all. `grip`:
    /// whether the frame has room for its grip.
    fn move_hint(&self, painter: &egui::Painter, screen: egui::Rect, frame: egui::Rect, grip: bool) {
        const NUDGE_KEYS: &str = if cfg!(target_os = "macos") { "Cmd" } else { "Ctrl" };
        let drag = if grip { "Drag the middle to move the frame" } else { "Drag the frame to move it" };
        let resize = if self.purpose.fixed().is_none() { "  ·  Shift+arrows: resize" } else { "" };
        let text = format!("{drag}  ·  Arrow keys nudge it ({NUDGE_KEYS}: {} pixels){resize}", NUDGE_FAR as u32);
        let galley = painter.layout(text, FontId::proportional(13.0), Color32::WHITE, (screen.width() - 48.0).max(80.0));
        let size = galley.size() + vec2(24.0, 12.0);
        let left = screen.center().x - size.x / 2.0;
        let top = egui::Rect::from_min_size(pos2(left, screen.min.y + 28.0), size);
        let bottom = egui::Rect::from_min_size(pos2(left, screen.max.y - 28.0 - size.y), size);
        let Some(bg) = [top, bottom].into_iter().find(|r| !r.intersects(frame.expand(6.0))) else { return };
        painter.rect_filled(bg, 8.0, Color32::from_black_alpha(190));
        painter.galley(bg.min + vec2(12.0, 6.0), galley, Color32::WHITE);
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

    /// The bar beside the chosen area. Choosing from it is the capture's last step, so it must
    /// not be missed on any picture: words on big buttons in a solid, outlined panel, with what
    /// people come for in the overlay's bright colour.
    fn action_bar(&mut self, ui: &mut Ui, mi: usize, sel: egui::Rect, screen: egui::Rect) {
        const GAP: f32 = 8.0;
        const PAD: i8 = 10;
        /// Clear of the area's handles.
        const OFFSET: f32 = 16.0;
        const COPY_KEYS: &str = if cfg!(target_os = "macos") { "Cmd+C" } else { "Ctrl+C" };
        const SAVE_KEYS: &str = if cfg!(target_os = "macos") { "Cmd+S" } else { "Ctrl+S" };
        let button = |icon, label, keys, weight, action| BarButton { icon, label, keys, weight, action };
        let cancel = button(Icon::Close, "Cancel", "Esc", Weight::Quiet, None);
        let enter = self.enter;
        let keys = |action, own: &'static str| if enter == action { "Enter" } else { own };
        let mut buttons = match self.purpose {
            Purpose::Gif => vec![button(Icon::Gif, "Record GIF", "Enter", Weight::Main, Some(Action::Start)), cancel],
            Purpose::Scrolling => vec![button(Icon::Scrolling, "Start scrolling capture", "Enter", Weight::Main, Some(Action::Start)), cancel],
            Purpose::Video | Purpose::VideoWindow | Purpose::VideoFixed(..) => {
                vec![button(Icon::Gif, "Start recording", "Enter", Weight::Main, Some(Action::Start)), cancel]
            }
            Purpose::Picture | Purpose::Window | Purpose::PictureFixed(..) | Purpose::Freehand => {
                let mut bar = Vec::with_capacity(5);
                if enter == Action::Default {
                    bar.push(button(Icon::Edit, "Copy & Edit", "Enter", Weight::Main, Some(Action::Default)));
                }
                bar.extend([
                    button(Icon::Edit, "Edit", keys(Action::Edit, ""), Weight::Main, Some(Action::Edit)),
                    button(Icon::Copy, "Copy", keys(Action::Copy, COPY_KEYS), Weight::Main, Some(Action::Copy)),
                    button(Icon::Save, "Save", keys(Action::Save, SAVE_KEYS), Weight::Plain, Some(Action::Save)),
                    cancel,
                ]);
                bar
            }
        };
        // The platform's order: the default first and Cancel last, or the other way round on a Mac.
        if cfg!(target_os = "macos") {
            buttons.reverse();
        }
        let font = strong_font(ui.ctx(), 17.0);
        let widths: Vec<f32> = buttons
            .iter()
            .map(|b| {
                let words = ui.painter().layout_no_wrap(b.label.to_string(), font.clone(), Color32::WHITE).size().x;
                (BAR_TEXT + words + 18.0).max(112.0)
            })
            .collect();
        let pad = f32::from(PAD);
        let w = widths.iter().sum::<f32>() + GAP * buttons.len().saturating_sub(1) as f32 + pad * 2.0;
        let h = BAR_BUTTON + pad * 2.0;
        // Centred under the area; above it, or inside it, where there's no room.
        let mut at = pos2(sel.center().x - w / 2.0, sel.max.y + OFFSET);
        if at.y + h > screen.max.y - 4.0 {
            at.y = sel.min.y - h - OFFSET;
            if at.y < screen.min.y + 4.0 {
                at.y = sel.max.y - h - OFFSET;
            }
        }
        // Not `clamp`: it panics on a screen smaller than the bar.
        at.x = at.x.min(screen.max.x - w - 4.0).max(screen.min.x + 4.0);
        at.y = at.y.min(screen.max.y - h - 4.0).max(screen.min.y + 4.0);
        let mut picked: Option<Option<Action>> = None;
        egui::Area::new(egui::Id::new(("overlay-bar", mi))).fixed_pos(at).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
            egui::Frame::NONE
                .fill(Color32::from_rgb(0x14, 0x16, 0x1C))
                .stroke(Stroke::new(2.0, ACCENT))
                .corner_radius(WINDOW_RADIUS)
                .inner_margin(egui::Margin::same(PAD))
                .shadow(egui::Shadow { offset: [0, 6], blur: 24, spread: 2, color: Color32::from_black_alpha(170) })
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = GAP;
                        for (b, width) in buttons.iter().zip(&widths) {
                            if bar_button(ui, b, *width, &font).clicked() {
                                picked = Some(b.action);
                            }
                        }
                    });
                });
        });
        match picked {
            Some(Some(a)) => self.choose(a),
            Some(None) => self.outcome = Outcome::Cancelled,
            None => {}
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
            // With areas kept, Enter keeps them (and the one being adjusted, if there is one).
            if self.sel.is_none() && self.purpose == Purpose::Picture && self.kept.is_empty() {
                let m = self.pointer.map(|(m, _)| m).unwrap_or(mi);
                if let Some(mon) = self.monitors.get(m) {
                    self.sel = Some((m, whole(mon)));
                    self.title = Some(mon.name.clone());
                }
            }
            if self.sel.is_none()
                && let Some(size) = self.purpose.fixed()
            {
                // Where the frame is shown: at the pointer, else in the middle.
                let (m, p) = self.pointer.unwrap_or((mi, (pw / 2.0, ph / 2.0)));
                if let Some(mon) = self.monitors.get(m) {
                    let full = (mon.picture.width() as f32, mon.picture.height() as f32);
                    self.sel = Some((m, fixed_frame(size, p, full)));
                }
            }
            self.choose(Action::Default);
            return;
        }
        // Backspace takes the last area kept out again.
        if self.sel.is_none() && self.phase == Phase::Choosing && ctx.input(|i| i.key_pressed(egui::Key::Backspace)) {
            self.kept.pop();
        }
        if self.sel.is_some() && self.phase == Phase::Adjusting {
            // egui turns Ctrl+C into a Copy event rather than a key press.
            let copy = ctx.input(|i| i.events.iter().any(|e| matches!(e, egui::Event::Copy)));
            let save = ctx.input_mut(|i| i.consume_shortcut(&egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S)));
            if copy {
                self.choose(Action::Copy);
                return;
            }
            if save {
                self.choose(Action::Save);
                return;
            }
            // Arrows move the area a pixel (ten with Ctrl); with Shift they make it larger. It
            // stays on its monitor. (A freehand outline stays where it was drawn.)
            if let Some((m, r)) = self.sel.as_mut()
                && *m == mi
                && self.purpose != Purpose::Freehand
            {
                // Each press (and each repeat of a key held down), with the keys held with it.
                let presses: Vec<(egui::Key, egui::Modifiers)> = ctx.input(|i| {
                    i.events
                        .iter()
                        .filter_map(|e| match e {
                            egui::Event::Key { key, pressed: true, modifiers, .. } => Some((*key, *modifiers)),
                            _ => None,
                        })
                        .collect()
                });
                for (key, mods) in presses {
                    let step = if mods.command { NUDGE_FAR } else { NUDGE };
                    let (dx, dy) = match key {
                        egui::Key::ArrowLeft => (-step, 0.0),
                        egui::Key::ArrowRight => (step, 0.0),
                        egui::Key::ArrowUp => (0.0, -step),
                        egui::Key::ArrowDown => (0.0, step),
                        _ => continue,
                    };
                    // A fixed size keeps its size.
                    if mods.shift && self.purpose.fixed().is_none() {
                        r.w = (r.w + dx).min(pw - r.x).max(1.0);
                        r.h = (r.h + dy).min(ph - r.y).max(1.0);
                    } else {
                        r.x = (r.x + dx).clamp(0.0, (pw - r.w).max(0.0));
                        r.y = (r.y + dy).clamp(0.0, (ph - r.h).max(0.0));
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
            let more = ctx.input(|i| i.modifiers.alt) && self.purpose == Purpose::Picture;
            let grip = self.phase == Phase::Adjusting && self.over_grip(mi, p, size, scale);
            self.phase = match (self.sel, self.purpose.fixed()) {
                _ if self.purpose.clicks_window() => Phase::Choosing,
                // A freehand outline starts again wherever the drag starts.
                _ if self.purpose == Purpose::Freehand => {
                    self.sel = None;
                    self.path = vec![p];
                    self.path_on = mi;
                    self.path_shade = None;
                    Phase::Dragging { start: p }
                }
                // A fixed frame is moved from wherever it's grabbed; grabbed outside, it comes to the pointer first.
                (sel, Some((w, h))) => {
                    let inside = sel.is_some_and(|(m, r)| m == mi && r.contains(screengrabcraft_model::Pt::new(p.0, p.1)));
                    if !inside {
                        self.sel = Some((mi, fixed_frame((w, h), p, (pw, ph))));
                    }
                    Phase::Moving { last: p, grip }
                }
                // A recording's grip moves the frame, whatever is near it.
                (Some((m, _)), _) if m == mi && grip => Phase::Moving { last: p, grip: true },
                (Some((m, r)), _) if m == mi && self.phase == Phase::Adjusting => {
                    if let Some(h) = handle_under(r, p, tol) {
                        Phase::Resizing { handle: h }
                    } else if r.contains(screengrabcraft_model::Pt::new(p.0, p.1)) {
                        Phase::Moving { last: p, grip: false }
                    } else {
                        // A new area: with the key held, the one adjusted is kept with it.
                        if more {
                            self.keep_area(m, r);
                        }
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
            if matches!(self.phase, Phase::Dragging { .. }) {
                self.held = self.shape;
            }
        }
        if resp.dragged()
            && let Some(now) = latest
        {
            let p = to_px(now);
            match self.phase {
                Phase::Dragging { .. } if self.purpose == Purpose::Freehand => {
                    // A point wherever the pointer has gone a pixel or more.
                    let far = self.path.last().is_none_or(|(x, y)| (x - p.0).abs() >= 1.0 || (y - p.1).abs() >= 1.0);
                    if far && self.path.len() < MAX_POINTS {
                        self.path.push(p);
                    }
                }
                Phase::Dragging { start } => {
                    // Held to a shape: the preset's, else the one the keys held say.
                    self.held = self.shape.or_else(|| ctx.input(|i| held_shape(i.modifiers))).filter(|_| self.purpose.drags_area());
                    let r = match self.held {
                        Some(shape) => shaped_area(start, p, shape, (pw, ph)),
                        None => Rect::from_points(screengrabcraft_model::Pt::new(start.0, start.1), screengrabcraft_model::Pt::new(p.0, p.1)),
                    };
                    self.sel = r.intersect(monitor_rect).map(|r| (mi, r));
                }
                Phase::Moving { last, grip } => {
                    // The whole frame, its size kept, on its monitor.
                    if let Some((_, r)) = self.sel.as_mut() {
                        r.x = (r.x + p.0 - last.0).clamp(0.0, (pw - r.w).max(0.0));
                        r.y = (r.y + p.1 - last.1).clamp(0.0, (ph - r.h).max(0.0));
                    }
                    self.phase = Phase::Moving { last: p, grip };
                }
                Phase::Resizing { handle } => {
                    if let Some((_, r)) = self.sel.as_mut() {
                        let h = screengrabcraft_model::Handle::BOX.get(handle).copied().unwrap_or(screengrabcraft_model::Handle::BottomRight);
                        // An area held to a shape keeps it when pulled by a corner.
                        if let Some(nr) = h.drag_rect(*r, screengrabcraft_model::Pt::new(p.0, p.1), self.held.is_some()).intersect(monitor_rect) {
                            *r = nr;
                        }
                    }
                }
                _ => {}
            }
        }
        let released = resp.drag_stopped() || (resp.clicked() && !resp.double_clicked());
        if released && let Some(size) = self.purpose.fixed() {
            // A click places the frame where it was shown; a drag has moved it there.
            let at = press.or(latest).map(to_px);
            let outside =
                |sel: Option<(usize, Rect)>, p: (f32, f32)| sel.is_none_or(|(m, r)| m != mi || !r.contains(screengrabcraft_model::Pt::new(p.0, p.1)));
            match (self.phase, at) {
                (Phase::Choosing | Phase::Dragging { .. }, Some(p)) => self.sel = Some((mi, fixed_frame(size, p, (pw, ph)))),
                (Phase::Adjusting, Some(p)) if outside(self.sel, p) => self.sel = Some((mi, fixed_frame(size, p, (pw, ph)))),
                _ => {}
            }
            if self.sel.is_some() {
                self.phase = Phase::Adjusting;
                // A picture of a set size is kept where it's placed, unless the bar is asked for.
                if matches!(self.purpose, Purpose::PictureFixed(..)) && !self.preview {
                    self.choose(Action::Default);
                }
            }
            return;
        }
        if released && self.purpose == Purpose::Freehand {
            if matches!(self.phase, Phase::Dragging { .. } | Phase::Choosing) {
                // Drawn: the box round it is the area; a click or a scribble is nothing.
                if let Some(end) = latest.map(to_px)
                    && self.path.last() != Some(&end)
                    && self.path.len() < MAX_POINTS
                {
                    self.path.push(end);
                }
                self.sel = self.path_bounds(size).map(|r| (mi, r));
                self.path_shade = None;
                if self.sel.is_none() {
                    self.path.clear();
                    self.phase = Phase::Choosing;
                } else if self.preview {
                    self.phase = Phase::Adjusting;
                } else {
                    self.choose(Action::Default);
                }
            }
            return;
        }
        if released {
            let more = ctx.input(|i| i.modifiers.alt) && self.purpose == Purpose::Picture;
            match self.phase {
                Phase::Dragging { .. } | Phase::Choosing => {
                    let small = self.sel.is_none_or(|(_, r)| r.w < 3.0 || r.h < 3.0);
                    // With areas kept (or one more to come), a click on no window is nothing.
                    let several = more || !self.kept.is_empty();
                    if small && several && press.or(latest).map(to_px).and_then(|p| self.window_at(mi, p)).is_none() {
                        self.sel = None;
                        self.phase = Phase::Choosing;
                        return;
                    }
                    if small {
                        // A click: the window under it, else the whole screen.
                        let at = press.or(latest).map(to_px);
                        let window = at.and_then(|p| self.window_at(mi, p));
                        match (window, self.purpose) {
                            (Some((r, title)), _) => {
                                self.sel = r.intersect(monitor_rect).map(|r| (mi, r));
                                self.title = Some(title);
                            }
                            (None, purpose) if purpose.clicks_window() => {
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
                    // Kept, with another to come.
                    if more && let Some((m, r)) = self.sel {
                        self.keep_area(m, r);
                        return;
                    }
                    if self.preview || self.purpose.records() {
                        self.phase = Phase::Adjusting;
                    } else {
                        self.choose(Action::Default);
                    }
                }
                Phase::Moving { .. } => {
                    // Put down on whole pixels, so it's kept exactly its size.
                    if let Some((_, r)) = self.sel.as_mut() {
                        r.x = r.x.round().min((pw - r.w).max(0.0)).max(0.0);
                        r.y = r.y.round().min((ph - r.h).max(0.0)).max(0.0);
                    }
                    self.phase = Phase::Adjusting;
                }
                Phase::Resizing { .. } => self.phase = Phase::Adjusting,
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
    fn the_last_area_is_ready_to_adjust_on_its_monitor_and_one_off_every_monitor_is_left_out() {
        let mut o = overlay();
        o.preselect_desktop(IRect::new(-90, 10, 40, 20));
        // In the monitor's picture's pixels (twice the desktop's here).
        assert_eq!(o.sel, Some((0, Rect::new(20.0, 20.0, 80.0, 40.0))));
        assert_eq!(o.phase, Phase::Adjusting);
        // Enter keeps it as it is.
        o.choose(Action::Default);
        let Outcome::Chosen { desktop, .. } = o.outcome() else { panic!("not chosen") };
        assert_eq!(*desktop, IRect::new(-90, 10, 40, 20));
        let mut o = overlay();
        o.preselect_desktop(IRect::new(500, 500, 10, 10));
        assert_eq!((o.sel, o.phase), (None, Phase::Choosing));
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
                title: None,
                shape: None
            }
        );
        assert_eq!(pixels(Rect::new(-5.0, 2.4, 1000.0, 10.0), 200, 100), IRect::new(0, 2, 200, 11));
    }

    #[test]
    fn a_fixed_frame_follows_the_pointer_and_stays_on_the_monitor() {
        let mon = (1920.0, 1080.0);
        // Centred on the pointer, on whole pixels.
        assert_eq!(fixed_frame((640, 360), (960.4, 540.0), mon), Rect::new(640.0, 360.0, 640.0, 360.0));
        // Against an edge or a corner: pushed back on.
        assert_eq!(fixed_frame((640, 360), (10.0, 10.0), mon), Rect::new(0.0, 0.0, 640.0, 360.0));
        assert_eq!(fixed_frame((640, 360), (1919.0, 1079.0), mon), Rect::new(1280.0, 720.0, 640.0, 360.0));
        // As large as the monitor: the whole of it.
        assert_eq!(fixed_frame((1920, 1080), (5.0, 1000.0), mon), Rect::new(0.0, 0.0, 1920.0, 1080.0));
        // Larger (a vertical video on a landscape monitor): made smaller, keeping its shape.
        let tall = fixed_frame((1080, 1920), (100.0, 500.0), mon);
        assert_eq!((tall.w, tall.h, tall.x, tall.y), (607.0, 1080.0, 0.0, 0.0));
        // Nonsense in, a frame on the monitor out.
        for (size, p, m) in [((0, 0), (f32::MAX, -5.0), mon), ((5000, 5000), (0.0, 0.0), (0.0, 0.0)), ((u32::MAX, 1), (3.0, 3.0), (10.0, 10.0))] {
            let r = fixed_frame(size, p, m);
            assert!(r.w >= 1.0 && r.h >= 1.0 && r.x >= 0.0 && r.y >= 0.0, "{r:?}");
            assert!(r.right() <= m.0.max(1.0) && r.bottom() <= m.1.max(1.0), "{r:?}");
        }
        // Its pixels are exactly its size, wherever it was moved to.
        assert_eq!(fixed_pixels(Rect::new(10.4, 20.6, 300.0, 200.0), 800, 600), IRect::new(10, 21, 300, 200));
        assert_eq!(fixed_pixels(Rect::new(700.0, 500.0, 300.0, 200.0), 800, 600), IRect::new(500, 400, 300, 200));
    }

    fn harness(o: Overlay) -> egui_kittest::Harness<'static, Overlay> {
        egui_kittest::Harness::builder().with_size(egui::vec2(800.0, 600.0)).build_ui_state(
            |ui, o: &mut Overlay| {
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| o.show_monitor(ui, 0));
            },
            o,
        )
    }

    fn screen_800() -> OverlayMonitor {
        OverlayMonitor { rect: IRect::new(0, 0, 800, 600), picture: Raster::filled(800, 600, Color::WHITE).unwrap(), name: "Main".into() }
    }

    fn click(h: &mut egui_kittest::Harness<'_, Overlay>, at: egui::Pos2) {
        h.hover_at(at);
        h.run_steps(1);
        h.drag_at(at);
        h.run_steps(1);
        h.drop_at(at);
        h.run_steps(2);
    }

    #[test]
    fn a_fixed_size_is_placed_with_a_click_and_clamped_to_the_monitor() {
        use egui_kittest::kittest::Queryable as _;
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::VideoFixed(300, 200));
        o.preview = false;
        let mut h = harness(o);
        h.run_steps(2);
        // Near the bottom right corner: the frame stays on the monitor.
        click(&mut h, egui::pos2(790.0, 590.0));
        assert_eq!(*h.state().outcome(), Outcome::Pending, "a recording waits to be started");
        h.get_by_label("Start recording").click();
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, desktop, .. } => {
                assert_eq!(*area, IRect::new(500, 400, 300, 200));
                assert_eq!(*desktop, IRect::new(500, 400, 300, 200));
            }
            other => panic!("{other:?}"),
        }
        // Larger than the monitor: as large as fits, keeping its shape; Enter places it at once.
        let mut h = harness(Overlay::new(vec![screen_800()], Vec::new(), Purpose::VideoFixed(1080, 1920)));
        h.run_steps(2);
        h.hover_at(egui::pos2(400.0, 300.0));
        h.run_steps(1);
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, .. } => assert_eq!(*area, IRect::new(232, 0, 337, 600)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn the_pointer_shows_what_a_press_does_while_the_area_is_adjusted() {
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Picture);
        o.preview = true;
        o.preselect(0, IRect::new(100, 100, 200, 100));
        let mut h = harness(o);
        h.run_steps(2);
        let cursor_at = |h: &mut egui_kittest::Harness<'_, Overlay>, x: f32, y: f32| {
            h.hover_at(egui::pos2(x, y));
            h.run_steps(1);
            h.output().platform_output.cursor_icon
        };
        assert_eq!(cursor_at(&mut h, 300.0, 200.0), CursorIcon::ResizeNwSe, "the bottom right corner");
        assert_eq!(cursor_at(&mut h, 102.0, 199.0), CursorIcon::ResizeNeSw, "the bottom left corner, a little off");
        assert_eq!(cursor_at(&mut h, 200.0, 100.0), CursorIcon::ResizeVertical, "the top edge");
        assert_eq!(cursor_at(&mut h, 300.0, 150.0), CursorIcon::ResizeHorizontal, "the right edge");
        assert_eq!(cursor_at(&mut h, 200.0, 150.0), CursorIcon::Grab, "inside: it moves");
        assert_eq!(cursor_at(&mut h, 600.0, 400.0), CursorIcon::Crosshair, "outside: a new area");
        // While it's being pulled, the arrows stay.
        h.drag_at(egui::pos2(300.0, 200.0));
        h.run_steps(1);
        h.hover_at(egui::pos2(340.0, 230.0));
        h.run_steps(1);
        assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::ResizeNwSe);
        h.drop_at(egui::pos2(340.0, 230.0));
        h.run_steps(2);
    }

    #[test]
    fn a_fixed_frame_is_moved_not_resized() {
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::VideoFixed(200, 100));
        o.preselect(0, IRect::new(100, 100, 200, 100));
        let mut h = harness(o);
        h.run_steps(2);
        // Grabbed by its bottom right corner, where a handle would be.
        h.hover_at(egui::pos2(300.0, 200.0));
        h.run_steps(1);
        h.drag_at(egui::pos2(299.0, 199.0));
        h.run_steps(1);
        for i in 1..=5 {
            h.hover_at(egui::pos2(299.0 + i as f32 * 20.0, 199.0 + i as f32 * 10.0));
            h.run_steps(1);
        }
        h.drop_at(egui::pos2(399.0, 249.0));
        h.run_steps(2);
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, .. } => assert_eq!((area.w, area.h), (200, 100), "{area:?}"),
            other => panic!("{other:?}"),
        }
    }

    fn cursor_at(h: &mut egui_kittest::Harness<'_, Overlay>, x: f32, y: f32) -> CursorIcon {
        h.hover_at(egui::pos2(x, y));
        h.run_steps(1);
        h.output().platform_output.cursor_icon
    }

    /// Presses the grip of the frame at `from` and drags it to `to`, a few steps at a time; the
    /// pointer shows arrows four ways all the while.
    fn drag_grip(h: &mut egui_kittest::Harness<'_, Overlay>, from: egui::Pos2, to: egui::Pos2) {
        assert_eq!(cursor_at(h, from.x, from.y), CursorIcon::Move, "over the grip");
        h.drag_at(from);
        h.run_steps(1);
        for i in 1..=5 {
            h.hover_at(from + (to - from) * (i as f32 / 5.0));
            h.run_steps(1);
            assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Move, "while it's moved by the grip");
        }
        h.drop_at(to);
        h.run_steps(2);
    }

    #[test]
    fn a_recordings_frame_is_moved_by_the_grip_in_its_middle_and_stays_on_its_monitor() {
        use egui_kittest::kittest::Queryable as _;
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Video);
        o.preselect(0, IRect::new(100, 100, 300, 200));
        let mut h = harness(o);
        h.run_steps(2);
        // The grip in the middle moves it; the rest is as it was.
        assert_eq!(cursor_at(&mut h, 250.0, 200.0), CursorIcon::Move, "the grip");
        assert_eq!(cursor_at(&mut h, 150.0, 150.0), CursorIcon::Grab, "inside, off the grip: it moves too");
        assert_eq!(cursor_at(&mut h, 400.0, 300.0), CursorIcon::ResizeNwSe, "the bottom right corner");
        assert_eq!(cursor_at(&mut h, 700.0, 500.0), CursorIcon::Crosshair, "outside");
        h.hover_at(egui::pos2(250.0, 200.0));
        h.run_steps(1);
        shot(&mut h, "overlay-move-grip");
        // Dragged by the grip: the whole frame goes, the same size, and is shown there.
        drag_grip(&mut h, egui::pos2(250.0, 200.0), egui::pos2(430.0, 290.0));
        assert_eq!(h.state().sel, Some((0, Rect::new(280.0, 190.0, 300.0, 200.0))));
        assert_eq!(h.state().grip(0, (800.0, 600.0), 1.0), Some((430.0, 290.0)), "the grip goes with it");
        h.hover_at(egui::pos2(100.0, 500.0));
        h.run_steps(2);
        shot(&mut h, "overlay-moved-frame");
        // Dragged off the screen: it stops at the monitor's edge, still the same size.
        drag_grip(&mut h, egui::pos2(430.0, 290.0), egui::pos2(799.0, 599.0));
        assert_eq!(h.state().sel, Some((0, Rect::new(500.0, 400.0, 300.0, 200.0))));
        drag_grip(&mut h, egui::pos2(650.0, 500.0), egui::pos2(0.0, 0.0));
        assert_eq!(h.state().sel, Some((0, Rect::new(0.0, 0.0, 300.0, 200.0))));
        // Moved to the middle and started: recorded where it was moved to.
        drag_grip(&mut h, egui::pos2(150.0, 100.0), egui::pos2(370.0, 260.0));
        h.get_by_label("Start recording").click();
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, desktop, action, .. } => {
                assert_eq!((*area, *desktop, *action), (IRect::new(220, 160, 300, 200), IRect::new(220, 160, 300, 200), Action::Start));
            }
            other => panic!("{other:?}"),
        }
    }

    /// On a narrow monitor (one turned on its end, or a small one at a large scale) the hints at
    /// the top wrap to it rather than run off its edges, at every display scale.
    #[test]
    fn the_hints_wrap_on_a_narrow_monitor() {
        let narrow =
            || OverlayMonitor { rect: IRect::new(0, 0, 360, 640), picture: Raster::filled(360, 640, Color::WHITE).unwrap(), name: "Narrow".into() };
        for scale in [1.0, 1.5, 2.0] {
            let choosing = Overlay::new(vec![narrow()], Vec::new(), Purpose::Picture);
            let mut moving = Overlay::new(vec![narrow()], Vec::new(), Purpose::Video);
            moving.preselect(0, IRect::new(60, 300, 240, 120));
            for (name, o) in [("overlay-narrow-hint", choosing), ("overlay-narrow-move-hint", moving)] {
                let mut h = egui_kittest::Harness::builder().with_size(egui::vec2(376.0, 656.0)).with_pixels_per_point(scale).build_ui_state(
                    |ui, o: &mut Overlay| {
                        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| o.show_monitor(ui, 0));
                    },
                    o,
                );
                crate::install_fonts(&h.ctx);
                h.run_steps(3);
                let found = crate::fit::spills(&h.output().shapes, h.ctx.content_rect());
                assert!(found.is_empty(), "{name} at {scale}: {}", found.join("\n"));
                if let Ok(dir) = std::env::var("SGC_SHOTS")
                    && let Ok(img) = h.render()
                {
                    let _ = img.save(format!("{dir}/{name}@{scale}.png"));
                }
            }
        }
    }

    #[test]
    fn a_frame_moved_part_way_across_a_pixel_is_put_down_on_whole_pixels() {
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Gif);
        o.preselect(0, IRect::new(100, 100, 300, 200));
        let mut h = harness(o);
        h.run_steps(2);
        drag_grip(&mut h, egui::pos2(250.0, 200.0), egui::pos2(310.6, 240.3));
        assert_eq!(h.state().sel, Some((0, Rect::new(161.0, 140.0, 300.0, 200.0))));
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, .. } => assert_eq!(*area, IRect::new(161, 140, 300, 200)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn arrow_keys_nudge_a_frame_and_keep_it_on_its_monitor() {
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Video);
        o.preselect(0, IRect::new(100, 100, 300, 200));
        let mut h = harness(o);
        h.run_steps(2);
        let press = |h: &mut egui_kittest::Harness<'_, Overlay>, mods: egui::Modifiers, key: egui::Key| {
            h.key_press_modifiers(mods, key);
            h.run_steps(1);
            h.state().sel.map(|(_, r)| (r.x, r.y, r.w, r.h))
        };
        assert_eq!(press(&mut h, egui::Modifiers::NONE, egui::Key::ArrowRight), Some((101.0, 100.0, 300.0, 200.0)), "a pixel");
        assert_eq!(press(&mut h, egui::Modifiers::NONE, egui::Key::ArrowUp), Some((101.0, 99.0, 300.0, 200.0)));
        assert_eq!(press(&mut h, egui::Modifiers::COMMAND, egui::Key::ArrowDown), Some((101.0, 109.0, 300.0, 200.0)), "ten with Ctrl");
        assert_eq!(press(&mut h, egui::Modifiers::COMMAND, egui::Key::ArrowLeft), Some((91.0, 109.0, 300.0, 200.0)));
        assert_eq!(press(&mut h, egui::Modifiers::SHIFT, egui::Key::ArrowRight), Some((91.0, 109.0, 301.0, 200.0)), "larger with Shift");
        // Not past the monitor's edge, either way.
        for _ in 0..12 {
            press(&mut h, egui::Modifiers::COMMAND, egui::Key::ArrowLeft);
        }
        assert_eq!(h.state().sel.map(|(_, r)| r.x), Some(0.0));
        for _ in 0..60 {
            press(&mut h, egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::ArrowRight);
        }
        assert_eq!(h.state().sel.map(|(_, r)| (r.x, r.w)), Some((0.0, 800.0)), "as wide as the monitor, no wider");
        // A fixed size is only moved.
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::VideoFixed(200, 100));
        o.preselect(0, IRect::new(100, 100, 200, 100));
        let mut h = harness(o);
        h.run_steps(2);
        assert_eq!(press(&mut h, egui::Modifiers::SHIFT | egui::Modifiers::COMMAND, egui::Key::ArrowRight), Some((110.0, 100.0, 200.0, 100.0)));
    }

    #[test]
    fn a_fixed_frame_has_the_grip_and_a_window_or_a_small_or_whole_frame_has_none() {
        // A frame of a set size is moved by its grip too.
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::VideoFixed(300, 200));
        o.preselect(0, IRect::new(100, 100, 300, 200));
        let mut h = harness(o);
        h.run_steps(2);
        drag_grip(&mut h, egui::pos2(250.0, 200.0), egui::pos2(350.0, 250.0));
        assert_eq!(h.state().sel.map(|(_, r)| (r.w, r.h)), Some((300.0, 200.0)));
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, .. } => assert_eq!(*area, IRect::new(200, 150, 300, 200)),
            other => panic!("{other:?}"),
        }
        // A window clicked to record is that window: nothing to move it by.
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::VideoWindow);
        o.preselect(0, IRect::new(100, 100, 300, 200));
        assert_eq!(o.grip(0, (800.0, 600.0), 1.0), None);
        // Too small for the grip beside its handles, it's moved from inside it; the whole screen
        // can't be moved at all.
        assert_eq!(grip_at(Rect::new(10.0, 10.0, 60.0, 300.0), (800.0, 600.0), 1.0), None);
        assert_eq!(grip_at(Rect::new(10.0, 10.0, 80.0, 80.0), (800.0, 600.0), 1.0), Some((50.0, 50.0)));
        assert_eq!(grip_at(Rect::new(10.0, 10.0, 80.0, 80.0), (800.0, 600.0), 0.5), None, "shown at half size, it's small");
        assert_eq!(grip_at(Rect::new(0.0, 0.0, 800.0, 600.0), (800.0, 600.0), 1.0), None);
        assert_eq!(grip_at(Rect::new(0.0, 0.0, 800.0, 590.0), (800.0, 600.0), 1.0), Some((400.0, 295.0)));
        for scale in [0.0, -1.0, f32::NAN, f32::INFINITY] {
            let _ = grip_at(Rect::new(0.0, 0.0, 100.0, 100.0), (800.0, 600.0), scale);
        }
    }

    #[test]
    fn a_pictures_area_has_no_grip_and_is_chosen_as_before() {
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Picture);
        o.preview = true;
        let mut h = harness(o);
        h.run_steps(2);
        drag_with(&mut h, egui::pos2(100.0, 100.0), egui::pos2(400.0, 300.0), egui::Modifiers::NONE);
        assert_eq!(h.state().sel, Some((0, Rect::new(100.0, 100.0, 300.0, 200.0))));
        assert_eq!(h.state().grip(0, (800.0, 600.0), 1.0), None, "no grip");
        // Its middle is a hand, as before: dragged from there it moves.
        assert_eq!(cursor_at(&mut h, 250.0, 200.0), CursorIcon::Grab);
        h.drag_at(egui::pos2(250.0, 200.0));
        h.run_steps(1);
        h.hover_at(egui::pos2(280.0, 220.0));
        h.run_steps(1);
        assert_eq!(h.output().platform_output.cursor_icon, CursorIcon::Grabbing);
        h.hover_at(egui::pos2(300.0, 230.0));
        h.run_steps(1);
        h.drop_at(egui::pos2(300.0, 230.0));
        h.run_steps(2);
        assert_eq!(h.state().sel, Some((0, Rect::new(150.0, 130.0, 300.0, 200.0))));
        shot(&mut h, "overlay-picture-no-grip");
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, action, .. } => assert_eq!((*area, *action), (IRect::new(150, 130, 300, 200), Action::Default)),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn an_area_held_to_a_shape_follows_the_pointer_and_stays_on_the_monitor() {
        let mon = (800.0, 600.0);
        // Down and right: the longer side of the drag decides.
        assert_eq!(shaped_area((100.0, 100.0), (420.0, 130.0), (16, 9), mon), Rect::new(100.0, 100.0, 320.0, 180.0));
        assert_eq!(shaped_area((100.0, 100.0), (130.0, 300.0), (1, 1), mon), Rect::new(100.0, 100.0, 200.0, 200.0));
        // Up and left, toward the pointer.
        assert_eq!(shaped_area((500.0, 400.0), (100.0, 390.0), (4, 3), mon), Rect::new(100.0, 100.0, 400.0, 300.0));
        // Past the monitor's edge: smaller, keeping its shape.
        let r = shaped_area((600.0, 100.0), (900.0, 110.0), (4, 3), mon);
        assert_eq!((r.x, r.y, r.w, r.h), (600.0, 100.0, 200.0, 150.0));
        // Nonsense in, an area on the monitor out.
        for (s, p, shape) in
            [((0.0, 0.0), (f32::NAN, f32::INFINITY), (0, 0)), ((800.0, 600.0), (-1e9, -1e9), (u32::MAX, 1)), ((5.0, 5.0), (5.0, 5.0), (9, 16))]
        {
            let r = shaped_area(s, p, shape, mon);
            assert!(r.w >= 0.0 && r.h >= 0.0 && r.x >= 0.0 && r.y >= 0.0 && r.right() <= 800.0 && r.bottom() <= 600.0, "{r:?}");
        }
        assert_eq!(held_shape(egui::Modifiers::SHIFT), Some((1, 1)));
        assert_eq!(held_shape(egui::Modifiers::COMMAND), Some((16, 9)));
        assert_eq!(held_shape(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT), Some((4, 3)));
        assert_eq!(held_shape(egui::Modifiers::ALT), None);
    }

    /// Drags from `from` to `to` with `mods` held, a few steps at a time.
    fn drag_with(h: &mut egui_kittest::Harness<'_, Overlay>, from: egui::Pos2, to: egui::Pos2, mods: egui::Modifiers) {
        h.hover_at(from);
        h.run_steps(1);
        h.drag_at(from);
        h.run_steps(1);
        for i in 1..=4 {
            h.event(egui::Event::ModifiersChanged(mods));
            h.hover_at(from + (to - from) * (i as f32 / 4.0));
            h.run_steps(1);
        }
        h.drop_at(to);
        h.run_steps(2);
        h.event(egui::Event::ModifiersChanged(egui::Modifiers::NONE));
        h.run_steps(1);
    }

    #[test]
    fn keys_held_while_dragging_hold_the_area_to_a_shape() {
        for (mods, want) in [
            (egui::Modifiers::NONE, (300, 60)),
            (egui::Modifiers::SHIFT, (300, 300)),
            (egui::Modifiers::COMMAND, (300, 169)),
            (egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, (300, 225)),
        ] {
            let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Picture);
            o.preview = false;
            let mut h = harness(o);
            h.run_steps(2);
            drag_with(&mut h, egui::pos2(100.0, 100.0), egui::pos2(400.0, 160.0), mods);
            match h.state().outcome() {
                Outcome::Chosen { area, action, .. } => {
                    assert_eq!(((area.w, area.h), *action), (want, Action::Default), "{mods:?}");
                    assert_eq!((area.x, area.y), (100, 100));
                }
                other => panic!("{mods:?}: {other:?}"),
            }
        }
    }

    #[test]
    fn a_presets_shape_holds_without_keys_and_the_hint_says_so() {
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Picture);
        o.shape = Some((4, 3));
        o.preview = true;
        let mut h = harness(o);
        h.run_steps(2);
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/overlay-held-hint.png"));
        }
        drag_with(&mut h, egui::pos2(400.0, 400.0), egui::pos2(300.0, 150.0), egui::Modifiers::NONE);
        assert_eq!(h.state().sel.map(|(_, r)| (r.x, r.y, r.w, r.h)), Some((67.0, 150.0, 333.0, 250.0)), "toward the pointer, 4:3 of the longer side");
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/overlay-held-area.png"));
        }
        // Pulled by a corner it keeps its shape.
        h.hover_at(egui::pos2(400.0, 400.0));
        h.run_steps(1);
        h.drag_at(egui::pos2(400.0, 400.0));
        h.run_steps(1);
        h.hover_at(egui::pos2(500.0, 410.0));
        h.run_steps(1);
        h.drop_at(egui::pos2(500.0, 410.0));
        h.run_steps(2);
        let (_, r) = h.state().sel.unwrap();
        assert!((r.w / r.h - 4.0 / 3.0).abs() < 0.02, "{r:?}");
    }

    #[test]
    fn a_picture_of_a_set_size_is_kept_where_its_clicked() {
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::PictureFixed(320, 240));
        o.preview = false;
        let mut h = harness(o);
        h.run_steps(2);
        h.hover_at(egui::pos2(400.0, 300.0));
        h.run_steps(2);
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/overlay-picture-fixed.png"));
        }
        click(&mut h, egui::pos2(790.0, 20.0));
        match h.state().outcome() {
            Outcome::Chosen { area, action, .. } => assert_eq!((*area, *action), (IRect::new(480, 0, 320, 240), Action::Default)),
            other => panic!("{other:?}"),
        }
        // With the bar it waits there, with the picture's buttons.
        use egui_kittest::kittest::Queryable as _;
        let mut h = harness(Overlay::new(vec![screen_800()], Vec::new(), Purpose::PictureFixed(320, 240)));
        h.run_steps(2);
        click(&mut h, egui::pos2(300.0, 300.0));
        assert_eq!(*h.state().outcome(), Outcome::Pending);
        h.get_by_label("Copy").click();
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, action, .. } => assert_eq!((*area, *action), (IRect::new(140, 180, 320, 240), Action::Copy)),
            other => panic!("{other:?}"),
        }
    }

    fn shot(h: &mut egui_kittest::Harness<'_, Overlay>, name: &str) {
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/{name}.png"));
        }
    }

    #[test]
    fn areas_dragged_out_with_alt_held_are_kept_and_joined_with_the_last_one() {
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Picture);
        o.preview = false;
        let mut h = harness(o);
        h.run_steps(2);
        drag_with(&mut h, egui::pos2(100.0, 100.0), egui::pos2(200.0, 150.0), egui::Modifiers::ALT);
        assert_eq!(*h.state().outcome(), Outcome::Pending, "kept, with another to come");
        drag_with(&mut h, egui::pos2(300.0, 300.0), egui::pos2(400.0, 420.0), egui::Modifiers::ALT);
        assert_eq!(h.state().kept.len(), 2);
        shot(&mut h, "overlay-several-areas");
        // Backspace takes the last one out; a click on nothing adds nothing.
        h.key_press(egui::Key::Backspace);
        h.run_steps(2);
        assert_eq!(h.state().kept.len(), 1);
        click(&mut h, egui::pos2(700.0, 500.0));
        assert_eq!((h.state().kept.len(), h.state().outcome().clone()), (1, Outcome::Pending));
        drag_with(&mut h, egui::pos2(300.0, 300.0), egui::pos2(400.0, 420.0), egui::Modifiers::ALT);
        // The last one without the key: all three, in the box round them, as they were.
        drag_with(&mut h, egui::pos2(500.0, 100.0), egui::pos2(600.0, 200.0), egui::Modifiers::NONE);
        match h.state().outcome() {
            Outcome::Chosen { area, desktop, action, title, shape, .. } => {
                assert_eq!(
                    (*area, *desktop, *action, title.clone()),
                    (IRect::new(100, 100, 500, 320), IRect::new(100, 100, 500, 320), Action::Default, None)
                );
                assert_eq!(
                    shape,
                    &Some(AreaShape::Boxes(vec![IRect::new(100, 100, 100, 50), IRect::new(300, 300, 100, 120), IRect::new(500, 100, 100, 100)]))
                );
            }
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn with_the_bar_the_last_area_is_adjusted_and_enter_keeps_the_ones_kept() {
        use egui_kittest::kittest::Queryable as _;
        let mut h = harness(Overlay::new(vec![screen_800()], Vec::new(), Purpose::Picture));
        h.run_steps(2);
        drag_with(&mut h, egui::pos2(60.0, 80.0), egui::pos2(260.0, 200.0), egui::Modifiers::ALT);
        drag_with(&mut h, egui::pos2(400.0, 260.0), egui::pos2(700.0, 400.0), egui::Modifiers::NONE);
        // The last one waits on its bar, the first shown with it.
        shot(&mut h, "overlay-several-areas-bar");
        h.get_by_label("Copy").click();
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, action, shape: Some(AreaShape::Boxes(b)), .. } => {
                assert_eq!((*area, *action, b.len()), (IRect::new(60, 80, 640, 320), Action::Copy, 2));
            }
            other => panic!("{other:?}"),
        }
        // Enter with only areas kept keeps them, not the whole screen.
        let mut h = harness(Overlay::new(vec![screen_800()], Vec::new(), Purpose::Picture));
        h.run_steps(2);
        drag_with(&mut h, egui::pos2(60.0, 80.0), egui::pos2(260.0, 200.0), egui::Modifiers::ALT);
        drag_with(&mut h, egui::pos2(400.0, 260.0), egui::pos2(700.0, 400.0), egui::Modifiers::ALT);
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, shape: Some(AreaShape::Boxes(b)), .. } => assert_eq!((*area, b.len()), (IRect::new(60, 80, 640, 320), 2)),
            other => panic!("{other:?}"),
        }
        // One area alone is a plain one.
        let mut h = harness(Overlay::new(vec![screen_800()], Vec::new(), Purpose::Picture));
        h.run_steps(2);
        drag_with(&mut h, egui::pos2(60.0, 80.0), egui::pos2(260.0, 200.0), egui::Modifiers::ALT);
        h.key_press(egui::Key::Enter);
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, shape: None, .. } => assert_eq!(*area, IRect::new(60, 80, 200, 120)),
            other => panic!("{other:?}"),
        }
    }

    /// Draws round a circle about `c`, `r` across, with the button held down.
    fn draw_round(h: &mut egui_kittest::Harness<'_, Overlay>, c: egui::Pos2, r: f32) {
        let at = |i: usize| c + egui::vec2((i as f32 * 0.25).cos() * r, (i as f32 * 0.25).sin() * r * 0.7);
        h.hover_at(at(0));
        h.run_steps(1);
        h.drag_at(at(0));
        h.run_steps(1);
        for i in 1..=24 {
            h.hover_at(at(i));
            h.run_steps(1);
        }
        h.drop_at(at(24));
        h.run_steps(2);
    }

    #[test]
    fn a_freehand_area_is_drawn_round_and_kept_with_its_outline() {
        use egui_kittest::kittest::Queryable as _;
        let mut o = Overlay::new(vec![screen_800()], Vec::new(), Purpose::Freehand);
        o.preview = false;
        let mut h = harness(o);
        h.run_steps(2);
        shot(&mut h, "overlay-freehand-hint");
        // A click is nothing.
        click(&mut h, egui::pos2(300.0, 300.0));
        assert_eq!(*h.state().outcome(), Outcome::Pending);
        draw_round(&mut h, egui::pos2(400.0, 300.0), 150.0);
        match h.state().outcome() {
            Outcome::Chosen { area, action, shape: Some(AreaShape::Freehand(pts)), .. } => {
                assert_eq!(*action, Action::Default);
                assert!(pts.len() >= 20, "{}", pts.len());
                // The box round the outline.
                assert!((area.x - 250).abs() <= 2 && (area.w as i32 - 300).abs() <= 3, "{area:?}");
                assert!((area.y - 195).abs() <= 2 && (area.h as i32 - 210).abs() <= 3, "{area:?}");
            }
            other => panic!("{other:?}"),
        }
        // With the bar it waits there, its outside dimmed, and is drawn again with a drag.
        let mut h = harness(Overlay::new(vec![screen_800()], Vec::new(), Purpose::Freehand));
        h.run_steps(2);
        draw_round(&mut h, egui::pos2(300.0, 250.0), 120.0);
        draw_round(&mut h, egui::pos2(400.0, 250.0), 150.0);
        assert_eq!(*h.state().outcome(), Outcome::Pending);
        shot(&mut h, "overlay-freehand");
        h.get_by_label("Save").click();
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, action, shape: Some(AreaShape::Freehand(_)), .. } => {
                assert_eq!(*action, Action::Save);
                assert!((area.x - 250).abs() <= 2, "the second outline: {area:?}");
            }
            other => panic!("{other:?}"),
        }
    }

    /// An open menu is listed in front of the windows (the capture service finds it): a click on
    /// it keeps just the menu, a click beside it the window under it.
    #[test]
    fn an_open_menu_in_front_of_its_window_is_clicked_alone() {
        use egui_kittest::kittest::Queryable as _;
        let menu = OverlayWindow { rect: IRect::new(150, 100, 120, 160), title: "Menu".into() };
        let notes = OverlayWindow { rect: IRect::new(100, 50, 400, 300), title: "Notes".into() };
        for (at, rect, title) in [((200.0, 200.0), menu.rect, "Menu"), ((400.0, 300.0), notes.rect, "Notes")] {
            let mut h = harness(Overlay::new(vec![screen_800()], vec![menu.clone(), notes.clone()], Purpose::VideoWindow));
            h.run_steps(2);
            click(&mut h, egui::pos2(at.0, at.1));
            h.get_by_label("Start recording").click();
            h.run_steps(2);
            match h.state().outcome() {
                Outcome::Chosen { area, title: t, .. } => {
                    assert_eq!(*area, rect);
                    assert_eq!(t.as_deref(), Some(title));
                }
                other => panic!("{other:?}"),
            }
        }
    }

    #[test]
    fn a_window_to_record_is_clicked() {
        use egui_kittest::kittest::Queryable as _;
        let w = OverlayWindow { rect: IRect::new(100, 50, 400, 300), title: "Notes".into() };
        let mut h = harness(Overlay::new(vec![screen_800()], vec![w], Purpose::VideoWindow));
        h.run_steps(2);
        // Nothing there: still choosing.
        click(&mut h, egui::pos2(700.0, 500.0));
        assert!(h.query_by_label("Start recording").is_none());
        click(&mut h, egui::pos2(200.0, 200.0));
        h.get_by_label("Start recording").click();
        h.run_steps(2);
        match h.state().outcome() {
            Outcome::Chosen { area, title, .. } => {
                assert_eq!(*area, IRect::new(100, 50, 400, 300));
                assert_eq!(title.as_deref(), Some("Notes"));
            }
            other => panic!("{other:?}"),
        }
    }
}
