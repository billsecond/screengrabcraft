//! Drawing on the screen while recording: a pen, arrows, boxes and step numbers drawn over the
//! area being recorded and into the recording, each fading away a few seconds after it's drawn.
//!
//! While the owner draws, a window over the area shows what's being recorded (the latest frame,
//! with the marks on it) and takes the pointer. Windows keeps that window out of the recording,
//! so the video has the screen as it is and the marks, which the recording draws into each frame
//! ([`Ink::mark`]). Elsewhere a window can't be kept out of a recording, so drawing is Windows
//! only, and a recording of every screen at once (one picture made of several) has none.

use std::time::{Duration, Instant};

use egui::{Sense, Ui, pos2, vec2};
use screengrabcraft_model::{ArrowHeads, BadgeShape, Color, Document, HeadShape, Object, ObjectId, Pt, Raster, Rect, Shape, StepLabels, Style};
use screengrabcraft_ui::icons::{self, Icon};

/// How long a mark stays once it's drawn, and how long it then takes to fade away.
pub const HOLD: Duration = Duration::from_secs(3);
pub const FADE: Duration = Duration::from_secs(1);
/// The most marks kept at once (older ones go first).
const MAX_MARKS: usize = 200;
/// The most points one pen line keeps.
const MAX_POINTS: usize = 4000;
/// The marks' colour: bright, to be seen on anything.
const INK: Color = Color::rgb(0xFF, 0x3B, 0x30);

/// What a drag or a click draws.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Tool {
    #[default]
    Pen,
    Arrow,
    Box,
    Step,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::Pen, Tool::Arrow, Tool::Box, Tool::Step];

    fn icon(self) -> Icon {
        match self {
            Tool::Pen => Icon::Pen,
            Tool::Arrow => Icon::Arrow,
            Tool::Box => Icon::Rect,
            Tool::Step => Icon::Step,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Tool::Pen => "Pen",
            Tool::Arrow => "Arrow",
            Tool::Box => "Box",
            Tool::Step => "Step number",
        }
    }
}

/// A mark drawn, and when it was finished.
struct Mark {
    shape: Shape,
    done: Instant,
}

/// The marks of a recording, shared by the window they're drawn in and the recording that draws
/// them into its frames. Places are in the recorded frame's pixels.
pub struct Ink {
    marks: Vec<Mark>,
    /// The mark being drawn: it's in the recording as it's drawn.
    current: Option<Shape>,
    /// The next step's number.
    next_step: u32,
    pub tool: Tool,
    /// Lines' thickness and steps' size grow with the frame's pixels a point.
    pub scale: f32,
    /// The owner is drawing: the recording keeps its latest frame for the window.
    pub on: bool,
    /// The latest frame recorded with the marks on it (while drawing), and its number.
    pub latest: Option<Raster>,
    pub serial: u64,
}

impl Default for Ink {
    fn default() -> Ink {
        Ink { marks: Vec::new(), current: None, next_step: 1, tool: Tool::Pen, scale: 1.0, on: false, latest: None, serial: 0 }
    }
}

/// How visible a mark finished at `done` is at `now`: whole for [`HOLD`], then less and less for
/// [`FADE`], then not at all.
pub fn opacity(done: Instant, now: Instant) -> f32 {
    let age = now.saturating_duration_since(done);
    if age <= HOLD {
        return 1.0;
    }
    let fading = age.saturating_sub(HOLD).as_secs_f32() / FADE.as_secs_f32().max(0.001);
    (1.0 - fading).clamp(0.0, 1.0)
}

impl Ink {
    fn style(&self, opacity: f32) -> Style {
        let s = self.scale.clamp(0.25, 8.0);
        Style {
            stroke: INK,
            fill: Color::TRANSPARENT,
            width: 6.0 * s,
            opacity,
            shadow: false,
            font_size: 22.0 * s,
            bold: true,
            text_color: Color::WHITE,
            ..Style::default()
        }
    }

    /// The pointer went down at `at`: a mark begins (a step is placed at once).
    pub fn begin(&mut self, at: Pt, now: Instant) {
        let at = at.finite();
        match self.tool {
            Tool::Pen => self.current = Some(Shape::Pen { points: vec![at] }),
            Tool::Arrow => self.current = Some(Shape::Arrow { from: at, to: at, heads: ArrowHeads::End, bend: 0.0, head: HeadShape::default() }),
            Tool::Box => self.current = Some(Shape::Rect { rect: Rect::new(at.x, at.y, 0.0, 0.0), radius: 0.0 }),
            Tool::Step => {
                self.current = None;
                let number = self.next_step;
                self.next_step = self.next_step.saturating_add(1);
                self.keep(Shape::Step { center: at, number, labels: StepLabels::default(), badge: BadgeShape::default() }, now);
            }
        }
    }

    /// The pointer, still down, moved to `to`.
    pub fn drag(&mut self, to: Pt) {
        let to = to.finite();
        match &mut self.current {
            Some(Shape::Pen { points }) if points.len() < MAX_POINTS => points.push(to),
            Some(Shape::Arrow { to: end, .. }) => *end = to,
            // The corner it began at stays where it is; the size may be less than nothing until
            // it's finished.
            Some(Shape::Rect { rect, .. }) => *rect = Rect::new(rect.x, rect.y, to.x - rect.x, to.y - rect.y),
            _ => {}
        }
    }

    /// The pointer came up: the mark is finished and starts its time.
    pub fn end(&mut self, now: Instant) {
        let Some(shape) = self.current.take() else { return };
        let shape = match shape {
            // A box is kept the right way round.
            Shape::Rect { rect, radius } => {
                Shape::Rect { rect: Rect::from_points(Pt::new(rect.x, rect.y), Pt::new(rect.x + rect.w, rect.y + rect.h)), radius }
            }
            other => other,
        };
        self.keep(shape, now);
    }

    fn keep(&mut self, shape: Shape, now: Instant) {
        if self.marks.len() >= MAX_MARKS {
            self.marks.remove(0);
        }
        self.marks.push(Mark { shape, done: now });
    }

    /// Takes every mark away at once, and the steps count from 1 again.
    pub fn clear(&mut self) {
        self.marks.clear();
        self.current = None;
        self.next_step = 1;
    }

    /// Lets go of the marks that have faded away.
    pub fn prune(&mut self, now: Instant) {
        self.marks.retain(|m| opacity(m.done, now) > 0.0);
    }

    /// The marks to see at `now`, the one being drawn last.
    pub fn objects(&self, now: Instant) -> Vec<Object> {
        let mut out: Vec<Object> = self
            .marks
            .iter()
            .filter_map(|m| {
                let o = opacity(m.done, now);
                (o > 0.0).then(|| Object { id: ObjectId(0), shape: m.shape.clone(), style: self.style(o), rotation: 0.0, group: None })
            })
            .collect();
        if let Some(c) = &self.current {
            let shape = match c {
                Shape::Rect { rect, radius } => {
                    Shape::Rect { rect: Rect::from_points(Pt::new(rect.x, rect.y), Pt::new(rect.x + rect.w, rect.y + rect.h)), radius: *radius }
                }
                other => other.clone(),
            };
            out.push(Object { id: ObjectId(0), shape, style: self.style(1.0), rotation: 0.0, group: None });
        }
        out
    }

    /// A recorded frame with the marks of `now` drawn on it; kept as the latest while the owner
    /// draws (the window over the area shows it).
    pub fn mark(&mut self, frame: Raster, now: Instant) -> Raster {
        self.prune(now);
        let objects = self.objects(now);
        let out = if objects.is_empty() { frame } else { draw_marks(frame, &objects) };
        if self.on {
            self.serial = self.serial.wrapping_add(1);
            self.latest = Some(out.clone());
        }
        out
    }
}

/// `frame` with `objects` drawn on it; as it was if they can't be.
pub fn draw_marks(frame: Raster, objects: &[Object]) -> Raster {
    let Some(mut canvas) = screengrabcraft_render::canvas::to_pixmap(&frame) else { return frame };
    let doc = Document::new(Raster::filled(1, 1, Color::TRANSPARENT).unwrap_or_else(|_| frame.clone()));
    for o in objects {
        screengrabcraft_render::draw::draw_object(&mut canvas, &doc, o);
    }
    screengrabcraft_render::canvas::from_pixmap(&canvas).unwrap_or(frame)
}

/// A recording's marks, shared by its frames' thread and the window they're drawn in.
pub type SharedInk = std::sync::Arc<std::sync::Mutex<Ink>>;

/// `frame` marked as `ink` says now (as it was if the marks can't be had).
pub fn mark_frame(ink: &SharedInk, frame: Raster) -> Raster {
    match ink.lock() {
        Ok(mut ink) => ink.mark(frame, Instant::now()),
        Err(_) => frame,
    }
}

/// What the drawing window's bar asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DrawChoice {
    /// Stop drawing (the marks fade away as they would).
    Done,
}

/// The drawing window's face: the latest recorded frame (`picture`, the frame's size) filling it,
/// the pointer drawing on it with the tool chosen, and a bar of tools at the top left with Clear
/// and Done. `done_key` is the shortcut that stops drawing, as the tip says it.
pub fn draw_ui(
    ui: &mut Ui,
    ink: &mut Ink,
    picture: Option<(egui::TextureId, (u32, u32))>,
    done_key: Option<&str>,
    now: Instant,
) -> Option<DrawChoice> {
    // The whole window, edge to edge, whatever the Ui it was given.
    let rect = ui.ctx().viewport_rect();
    let mut choice = None;
    let size = picture.map_or((rect.width().max(1.0) as u32, rect.height().max(1.0) as u32), |(_, s)| s);
    let (sx, sy) = (size.0.max(1) as f32 / rect.width().max(1.0), size.1.max(1) as f32 / rect.height().max(1.0));
    ink.scale = sx.max(sy);
    let to_px = |p: egui::Pos2| Pt::new((p.x - rect.min.x) * sx, (p.y - rect.min.y) * sy);
    match picture {
        Some((tex, _)) => {
            ui.painter().image(tex, rect, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), egui::Color32::WHITE);
        }
        None => {
            ui.painter().rect_filled(rect, 0.0, egui::Color32::from_gray(30));
        }
    }
    // A thin edge round it: drawing is on.
    ui.painter().rect_stroke(rect.shrink(1.0), 0.0, egui::Stroke::new(2.0, egui::Color32::from_rgb(0xFF, 0x3B, 0x30)), egui::StrokeKind::Inside);
    let resp = ui.interact(rect, ui.id().with("draw on the screen"), Sense::click_and_drag());
    let press = ui.ctx().input(|i| i.pointer.press_origin());
    let latest = ui.ctx().input(|i| i.pointer.latest_pos());
    if resp.drag_started_by(egui::PointerButton::Primary)
        && let Some(p) = press
    {
        ink.begin(to_px(p), now);
    }
    if resp.dragged()
        && let Some(p) = latest
    {
        ink.drag(to_px(p));
    }
    if resp.drag_stopped() {
        ink.end(now);
    }
    if resp.clicked()
        && let Some(p) = press.or(latest)
    {
        // A click: a step is placed; other tools make nothing of a click.
        if ink.tool == Tool::Step {
            ink.begin(to_px(p), now);
        }
    }
    ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    if ui.ctx().input(|i| i.key_pressed(egui::Key::Escape)) {
        choice = Some(DrawChoice::Done);
    }
    // The bar of tools.
    egui::Area::new(ui.id().with("draw bar")).fixed_pos(rect.min + vec2(12.0, 12.0)).order(egui::Order::Foreground).show(ui.ctx(), |ui| {
        screengrabcraft_ui::widgets::popup_frame(ui.ctx()).inner_margin(egui::Margin::same(6)).show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for tool in Tool::ALL {
                    if icons::button(ui, tool.icon(), tool.label(), ink.tool == tool, true).clicked() {
                        ink.tool = tool;
                    }
                }
                ui.separator();
                if icons::button(ui, Icon::Delete, "Clear: take every mark away", false, true).clicked() {
                    ink.clear();
                }
                let tip = match done_key {
                    Some(k) => format!("Stop drawing ({k}, or Esc); the marks fade away"),
                    None => "Stop drawing (Esc); the marks fade away".to_string(),
                };
                if screengrabcraft_ui::widgets::primary_button(ui, "Done").on_hover_text(tip).clicked() {
                    choice = Some(DrawChoice::Done);
                }
            });
        });
    });
    choice
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(ms: u64, from: Instant) -> Instant {
        from + Duration::from_millis(ms)
    }

    #[test]
    fn a_mark_stays_then_fades_away() {
        let t0 = Instant::now();
        assert_eq!(opacity(t0, t0), 1.0);
        assert_eq!(opacity(t0, at(2999, t0)), 1.0);
        assert!((opacity(t0, at(3500, t0)) - 0.5).abs() < 0.01);
        assert_eq!(opacity(t0, at(4000, t0)), 0.0);
        assert_eq!(opacity(at(5000, t0), t0), 1.0, "a clock that went back is no fade");
    }

    #[test]
    fn each_tool_draws_its_mark_and_steps_count_up() {
        let t0 = Instant::now();
        let mut ink = Ink::default();
        ink.begin(Pt::new(10.0, 10.0), t0);
        ink.drag(Pt::new(20.0, 15.0));
        ink.drag(Pt::new(30.0, 30.0));
        assert_eq!(ink.objects(t0).len(), 1, "the line being drawn is seen");
        ink.end(t0);
        ink.tool = Tool::Arrow;
        ink.begin(Pt::new(100.0, 100.0), t0);
        ink.drag(Pt::new(150.0, 120.0));
        ink.end(t0);
        ink.tool = Tool::Box;
        ink.begin(Pt::new(80.0, 90.0), t0);
        ink.drag(Pt::new(40.0, 30.0));
        ink.end(t0);
        ink.tool = Tool::Step;
        ink.begin(Pt::new(5.0, 5.0), t0);
        ink.begin(Pt::new(50.0, 5.0), t0);
        let shapes: Vec<Shape> = ink.objects(t0).into_iter().map(|o| o.shape).collect();
        assert!(matches!(&shapes[0], Shape::Pen { points } if points.len() == 3));
        assert_eq!(shapes[1], Shape::Arrow { from: Pt::new(100.0, 100.0), to: Pt::new(150.0, 120.0), heads: ArrowHeads::End, bend: 0.0, head: HeadShape::default() });
        assert_eq!(shapes[2], Shape::Rect { rect: Rect::new(40.0, 30.0, 40.0, 60.0), radius: 0.0 }, "dragged up and left, the right way round");
        assert_eq!(shapes[3], Shape::Step { center: Pt::new(5.0, 5.0), number: 1, labels: StepLabels::default(), badge: BadgeShape::default() });
        assert_eq!(shapes[4], Shape::Step { center: Pt::new(50.0, 5.0), number: 2, labels: StepLabels::default(), badge: BadgeShape::default() });
        // They fade together, and are let go of once faded.
        let half = ink.objects(at(3500, t0));
        assert!(half.iter().all(|o| (o.style.opacity - 0.5).abs() < 0.01));
        ink.prune(at(4100, t0));
        assert!(ink.objects(at(4100, t0)).is_empty());
        // Clear starts the steps again.
        ink.begin(Pt::new(1.0, 1.0), t0);
        ink.clear();
        ink.begin(Pt::new(1.0, 1.0), t0);
        assert_eq!(ink.objects(t0).first().map(|o| o.shape.clone()), Some(Shape::Step { center: Pt::new(1.0, 1.0), number: 1, labels: StepLabels::default(), badge: BadgeShape::default() }));
    }

    #[test]
    fn the_marks_are_drawn_into_the_frame_and_kept_for_the_window_while_drawing() {
        let t0 = Instant::now();
        let frame = Raster::filled(200, 100, Color::WHITE).unwrap();
        let mut ink = Ink::default();
        // No marks: the frame as it was, and nothing kept while not drawing.
        let same = ink.mark(frame.clone(), t0);
        assert_eq!(same, frame);
        assert!(ink.latest.is_none());
        ink.on = true;
        ink.tool = Tool::Box;
        ink.begin(Pt::new(20.0, 20.0), t0);
        ink.drag(Pt::new(180.0, 80.0));
        ink.end(t0);
        let marked = ink.mark(frame.clone(), t0);
        let edge = marked.pixel(100, 20).unwrap();
        assert!(edge.r > 200 && edge.g < 120, "the box's edge is red: {edge:?}");
        assert_eq!(marked.pixel(100, 50), frame.pixel(100, 50), "inside it is the frame");
        assert_eq!(ink.latest.as_ref(), Some(&marked));
        assert_eq!(ink.serial, 1);
        // Faded: the frame as it was again.
        assert_eq!(ink.mark(frame.clone(), at(4500, t0)), frame);
    }

    /// The drawing window: a drag draws with the pen, a click with Step places a number, Clear
    /// takes them away, and Done (or Esc) stops drawing. Places are in the frame's pixels, twice
    /// the window's points here.
    #[test]
    fn the_drawing_window_draws_steps_clears_and_is_done() {
        use egui_kittest::Harness;
        use egui_kittest::kittest::Queryable as _;
        let mut h = Harness::builder().with_size(vec2(400.0, 300.0)).build_ui_state(
            |ui, (ink, done, kept): &mut (Ink, Option<DrawChoice>, Option<egui::TextureHandle>)| {
                screengrabcraft_ui::theme::apply(ui.ctx(), screengrabcraft_model::Theme::Dark);
                // As the recording does: each frame marked, the latest shown.
                ink.on = true;
                let frame = Raster::filled(800, 600, Color::rgb(235, 238, 242)).unwrap();
                let shown = ink.mark(frame, Instant::now());
                let img = egui::ColorImage::from_rgba_unmultiplied([800, 600], shown.pixels());
                let tex = ui.ctx().load_texture("frame", img, egui::TextureOptions::LINEAR);
                let id = tex.id();
                // Kept until it's drawn, as the service keeps it.
                *kept = Some(tex);
                egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
                    if let Some(c) = draw_ui(ui, ink, Some((id, (800, 600))), Some("F8"), Instant::now()) {
                        *done = Some(c);
                    }
                });
            },
            (Ink::default(), None, None),
        );
        h.run_steps(2);
        assert_eq!(h.state().0.scale, 2.0);
        h.hover_at(egui::pos2(100.0, 150.0));
        h.run_steps(1);
        h.drag_at(egui::pos2(100.0, 150.0));
        h.run_steps(1);
        for x in [140.0, 180.0, 220.0] {
            h.hover_at(egui::pos2(x, 170.0));
            h.run_steps(1);
        }
        h.drop_at(egui::pos2(220.0, 170.0));
        h.run_steps(2);
        let shapes: Vec<Shape> = h.state().0.objects(Instant::now()).into_iter().map(|o| o.shape).collect();
        assert!(
            matches!(shapes.first(), Some(Shape::Pen { points }) if points.first() == Some(&Pt::new(200.0, 300.0)) && points.len() >= 3),
            "{shapes:?}"
        );
        h.get_by_label("Step number").click();
        h.run_steps(1);
        for x in [300.0, 340.0] {
            h.hover_at(egui::pos2(x, 250.0));
            h.run_steps(1);
            h.drag_at(egui::pos2(x, 250.0));
            h.run_steps(1);
            h.drop_at(egui::pos2(x, 250.0));
            h.run_steps(2);
        }
        let steps: Vec<u32> = h
            .state()
            .0
            .objects(Instant::now())
            .into_iter()
            .filter_map(|o| if let Shape::Step { number, .. } = o.shape { Some(number) } else { None })
            .collect();
        assert_eq!(steps, vec![1, 2]);
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Ok(img) = h.render()
        {
            let _ = img.save(format!("{dir}/drawing-window.png"));
        }
        h.get_by_label("Clear: take every mark away").click();
        h.run_steps(1);
        assert!(h.state().0.objects(Instant::now()).is_empty());
        h.get_by_label("Done").click();
        h.run_steps(1);
        assert_eq!(h.state().1, Some(DrawChoice::Done));
    }

    #[test]
    fn junk_places_and_many_marks_are_taken_without_fuss() {
        let t0 = Instant::now();
        let mut ink = Ink { tool: Tool::Pen, ..Ink::default() };
        for i in 0..(MAX_MARKS + 20) {
            ink.tool = Tool::ALL[i % 4];
            ink.begin(Pt::new(f32::NAN, f32::INFINITY), t0);
            for _ in 0..3 {
                ink.drag(Pt::new(f32::NEG_INFINITY, 1e30));
            }
            ink.end(t0);
        }
        assert!(ink.marks.len() <= MAX_MARKS);
        let frame = Raster::filled(64, 32, Color::WHITE).unwrap();
        let out = ink.mark(frame, t0);
        assert_eq!((out.width(), out.height()), (64, 32));
    }
}
