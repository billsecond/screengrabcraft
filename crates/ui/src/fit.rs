//! Words that stay inside their box. A window of the capture service has a size of its own (the
//! system's, not egui's), so what it shows can need more room than it was made with: a long
//! message, a file name, a device's name, the interface's own font at another display scale. Its
//! words then ran past its edge, or past the edge of the card or button they were on. Here every
//! such window is sized by what it shows:
//!
//! - [`Fit`]: a window that grows to what it shows. [`Fit::face`] draws it and notes how much more
//!   room it took than it had; [`Fit::size`] is the window's size from then on, never under the
//!   size it was made for. A window just opened is drawn once out of sight ([`Fit::shown`]), so it
//!   appears at its size.
//! - [`ends`]: a row with things at its left and at its right (a bar's words and its buttons) that
//!   takes the width both need, so neither runs into the other and the window grows instead.
//! - [`overflow`]: how much more room something drawn took than it was given (what [`Fit::face`]
//!   measures with).
//! - [`spills`]: the check the tests make of what a frame painted: every line of words inside the
//!   box it's on, inside the window, not cut off by its clip, over no other words and not under
//!   something painted over them. [`crowded`]: no two controls over each other.
//!
//! The Recorder (`crate::recorder`) grows its cards, and its window with them, the same way.

use std::hash::{DefaultHasher, Hash, Hasher};

use egui::epaint::{ClippedShape, Shape};
use egui::{Align, Context, Frame, Id, Layout, Rect, Ui, Vec2, ViewportId, vec2};

/// The most a window grows to, in points: a message as long as a book stays on the screen.
pub const MOST: Vec2 = vec2(2400.0, 1600.0);

/// What [`Fit`] keeps of a window between frames.
#[derive(Clone, Copy, Debug, Default)]
struct Noted {
    /// The size it needed when last drawn, in points (zero: not drawn yet).
    needed: Vec2,
    /// The service's frame it was last asked for in, to know a window opened (again).
    last: Option<u64>,
    /// The service's frame it opened in.
    opened: u64,
    /// What it showed, as a number (see [`Fit::shown_with`]).
    content: u64,
}

/// A window that grows to what it shows (see the module's notes). `name` names the window; `base`
/// is the size it's made with, which it never goes under.
#[derive(Clone, Copy, Debug)]
pub struct Fit {
    id: Id,
    base: Vec2,
}

impl Fit {
    pub fn new(name: &str, base: Vec2) -> Fit {
        Fit { id: Id::new("fit").with(name), base }
    }

    fn noted(&self, ctx: &Context) -> Noted {
        ctx.data(|d| d.get_temp::<Noted>(self.id)).unwrap_or_default()
    }

    /// The window's size: the size it was made with, grown to what it needed when last drawn.
    pub fn size(&self, ctx: &Context) -> Vec2 {
        clamp(self.base.max(self.noted(ctx).needed))
    }

    /// Whether to show the window this frame: not in the frame it opens in, when it's drawn out of
    /// sight to be measured; from the next on, at its size. Asked once a frame by whoever shows
    /// it, before showing it.
    pub fn shown(&self, ctx: &Context) -> bool {
        self.shown_with(ctx, ())
    }

    /// [`Fit::shown`], for a window showing `content`: something else in its place (another
    /// notice) is measured afresh, as if the window had just opened.
    pub fn shown_with(&self, ctx: &Context, content: impl Hash) -> bool {
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let content = hasher.finish();
        let now = ctx.cumulative_frame_nr_for(ViewportId::ROOT);
        let shown = ctx.data_mut(|d| {
            let n = d.get_temp_mut_or_default::<Noted>(self.id);
            // Up last frame (or earlier in this one), showing the same: it stays up. Else it's
            // opening (again), and what it needed then is no guide to what it shows now.
            if !n.last.is_some_and(|f| f.saturating_add(1) >= now) || n.content != content {
                n.needed = Vec2::ZERO;
                n.opened = now;
                n.content = content;
            }
            n.last = Some(now);
            now > n.opened
        });
        if !shown {
            ctx.request_repaint_of(ViewportId::ROOT);
        }
        shown
    }

    /// Notes that the window needs `needed` (points), and has it drawn again at once if that's
    /// more than it has.
    pub fn need(&self, ctx: &Context, needed: Vec2) {
        let needed = vec2(needed.x.ceil(), needed.y.ceil());
        if !needed.is_finite() {
            return;
        }
        let before = self.noted(ctx).needed;
        let after = needed.max(Vec2::ZERO);
        if before != after {
            ctx.data_mut(|d| d.get_temp_mut_or_default::<Noted>(self.id).needed = after);
            // Grown: shown at its new size as soon as can be.
            if after.x > before.x || after.y > before.y {
                ctx.request_repaint();
                ctx.request_repaint_of(ViewportId::ROOT);
            }
        }
    }

    /// The window's face: `frame` filling `ui` (the window), what `add` draws in it, and the size
    /// the window needs for it noted for [`Fit::size`]: as big as it is, and as much bigger as what
    /// was drawn ran past its edges.
    pub fn face<R>(&self, ui: &mut Ui, frame: Frame, add: impl FnOnce(&mut Ui) -> R) -> R {
        let pad = frame.total_margin().sum();
        let (r, needed) = egui::CentralPanel::default()
            .frame(frame)
            .show(ui, |ui| {
                let room = ui.max_rect().size();
                let (r, over) = overflow(ui, add);
                (r, room + over + pad)
            })
            .inner;
        self.need(ui.ctx(), needed);
        r
    }
}

/// A size that's a number, at least 2 by 2 points (the least a window can be, see the service's
/// `SMALLEST_WINDOW`) and at most [`MOST`].
fn clamp(size: Vec2) -> Vec2 {
    let at = |v: f32, most: f32| if v.is_finite() { v.clamp(2.0, most) } else { 2.0 };
    vec2(at(size.x, MOST.x), at(size.y, MOST.y))
}

/// Draws `add` in `ui`, and says how much wider and taller it is than the room `ui` had: what ran
/// past its edges (left and right, top and bottom, added up), zero when it all fitted.
pub fn overflow<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> (R, Vec2) {
    let room = ui.max_rect();
    let r = add(ui);
    let used = ui.min_rect();
    let past = |lo: f32, hi: f32, used_lo: f32, used_hi: f32| (lo - used_lo).max(0.0) + (used_hi - hi).max(0.0);
    let over = vec2(past(room.min.x, room.max.x, used.min.x, used.max.x), past(room.min.y, room.max.y, used.min.y, used.max.y));
    (r, if over.is_finite() { over } else { Vec2::ZERO })
}

/// A row as wide as `ui`: what `left` draws at its left, left to right, and what `right` draws
/// at its right, right to left, centred up and down. When both need more than the row has, the
/// row takes what they need (`ui` grows past its edge, which a [`Fit`] window grows to) rather
/// than one running into the other.
pub fn ends<L, R>(ui: &mut Ui, left: impl FnOnce(&mut Ui) -> L, right: impl FnOnce(&mut Ui) -> R) -> (L, R) {
    ui.horizontal_centered(|ui| {
        let start = ui.max_rect().min.x;
        let l = left(ui);
        let left_end = ui.min_rect().max.x;
        let gap = ui.spacing().item_spacing.x;
        let shown = ui.with_layout(Layout::right_to_left(Align::Center), right);
        let wide = (left_end - start) + gap + shown.response.rect.width();
        if wide.is_finite() && start + wide > ui.max_rect().max.x {
            ui.expand_to_include_x(start + wide);
        }
        (l, shown.inner)
    })
    .inner
}

// ---- the check ----

/// How far (points) words may go past an edge: the soft edge their letters are drawn with.
const GIVE: f32 = 1.0;
/// A box this small (points) painted over words is decoration (grain, a dot), not something
/// hiding them.
const SPECK: f32 = 4.0;

/// One thing a frame painted that the check looks at.
enum Painted {
    /// A box (a panel, a card, a button…), filled (true) or only outlined.
    Box(Rect, bool),
    /// A run of words, where its letters are, and the clip it was drawn in.
    Words(Rect, Rect, String),
}

fn gather(shape: &Shape, clip: Rect, out: &mut Vec<Painted>, depth: u32) {
    match shape {
        Shape::Vec(shapes) if depth < 8 => {
            for s in shapes {
                gather(s, clip, out, depth + 1);
            }
        }
        // A shadow or a glow is no box to be on.
        Shape::Rect(r) if r.blur_width <= 0.0 && r.angle == 0.0 && (r.fill.a() > 0 || (r.stroke.width > 0.0 && r.stroke.color.a() > 0)) => {
            out.push(Painted::Box(r.rect, r.fill.a() > 0));
        }
        Shape::Text(t) if t.angle == 0.0 && !t.galley.text().trim().is_empty() => {
            let at = t.visual_bounding_rect();
            if at.is_positive() && at.is_finite() {
                out.push(Painted::Words(at, clip, t.galley.text().to_string()));
            }
        }
        _ => {}
    }
}

/// Whether `inner` is inside `outer`, give or take [`GIVE`].
fn inside(outer: Rect, inner: Rect) -> bool {
    outer.expand(GIVE).contains_rect(inner)
}

/// Every place a frame's words don't fit (empty when they all do), said in words: words cut off
/// by their clip, off the window (`screen`), past the edge of the box they're on, or over other
/// words. The box words are on is the smallest one painted before them that holds where they
/// start or their middle. `shapes` is what the frame painted, in the order it was painted
/// (`egui::FullOutput::shapes`).
pub fn spills(shapes: &[ClippedShape], screen: Rect) -> Vec<String> {
    let mut painted = Vec::new();
    for s in shapes {
        gather(&s.shape, s.clip_rect, &mut painted, 0);
    }
    let mut found = Vec::new();
    for (i, p) in painted.iter().enumerate() {
        let Painted::Words(at, clip, text) = p else { continue };
        let (at, clip) = (*at, *clip);
        if !inside(clip, at) {
            found.push(format!("“{text}” is cut off: it's at {at:?}, its clip {clip:?}"));
        }
        if !inside(screen, at) {
            found.push(format!("“{text}” runs off the window: it's at {at:?}, the window {screen:?}"));
        }
        let start = at.left_top() + vec2(2.0, 2.0).min(at.size() / 2.0);
        let on = painted
            .iter()
            .take(i)
            .filter_map(|q| match q {
                Painted::Box(b, _) if b.contains(start) || b.contains(at.center()) => Some(*b),
                _ => None,
            })
            .min_by(|a, b| a.area().total_cmp(&b.area()));
        if let Some(b) = on
            && !inside(b, at)
        {
            found.push(format!("“{text}” runs past the edge of what it's on: it's at {at:?}, that's {b:?}"));
        }
        for q in painted.iter().skip(i + 1) {
            match q {
                Painted::Words(other, _, words) if at.shrink(GIVE).intersects(other.shrink(GIVE)) => {
                    found.push(format!("“{text}” runs into “{words}”: {at:?} and {other:?}"));
                }
                // Something filled painted over part of them (a button over the end of a label);
                // not a speck (a film's grain, a dot).
                Painted::Box(b, true) if b.width().min(b.height()) >= SPECK && at.shrink(GIVE).intersects(*b) && !inside(*b, at) => {
                    found.push(format!("“{text}” is partly under something painted over it: it's at {at:?}, that's {b:?}"));
                }
                _ => {}
            }
        }
    }
    found
}

/// Every two of `widgets` (each named, and where it is) that are over each other, said in words
/// (empty when none are): a label under a button, a meter under an icon. One wholly inside
/// another (a label in its group) isn't over it.
pub fn crowded(widgets: &[(String, Rect)]) -> Vec<String> {
    let mut found = Vec::new();
    for (i, (a, ra)) in widgets.iter().enumerate() {
        for (b, rb) in widgets.iter().skip(i + 1) {
            let nested = ra.expand(GIVE).contains_rect(*rb) || rb.expand(GIVE).contains_rect(*ra);
            if !nested && ra.shrink(GIVE).intersects(rb.shrink(GIVE)) {
                found.push(format!("“{a}” and “{b}” are over each other: {ra:?} and {rb:?}"));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;

    use super::*;

    /// What a frame of `face` painted, checked, in a window `size` points big at `scale`.
    fn check(size: Vec2, scale: f32, face: impl FnMut(&mut Ui) + 'static) -> Vec<String> {
        let mut h = Harness::builder().with_size(size).with_pixels_per_point(scale).build_ui(face);
        h.run_steps(2);
        spills(&h.output().shapes, h.ctx.content_rect())
    }

    /// A label in a box that's big enough, and one that isn't.
    fn boxed(width: f32, words: &'static str) -> impl FnMut(&mut Ui) + 'static {
        move |ui: &mut Ui| {
            let (rect, _) = ui.allocate_exact_size(vec2(width, 40.0), egui::Sense::hover());
            ui.painter().rect_filled(rect, 4.0, egui::Color32::GRAY);
            ui.painter().text(
                rect.left_center() + vec2(6.0, 0.0),
                egui::Align2::LEFT_CENTER,
                words,
                egui::FontId::proportional(14.0),
                egui::Color32::BLACK,
            );
        }
    }

    #[test]
    fn words_in_their_box_pass_and_words_past_its_edge_are_found() {
        for scale in [1.0, 1.5, 2.0] {
            assert!(check(vec2(400.0, 100.0), scale, boxed(200.0, "Short")).is_empty(), "{scale}");
            let found = check(vec2(400.0, 100.0), scale, boxed(60.0, "Far too many words for this"));
            assert!(found.iter().any(|f| f.contains("runs past the edge")), "{scale}: {found:?}");
        }
    }

    #[test]
    fn words_off_the_window_cut_off_or_over_each_other_are_found() {
        let off = check(vec2(200.0, 80.0), 1.0, |ui| {
            ui.add(egui::Label::new("A label far wider than the window it is shown in").extend());
        });
        assert!(off.iter().any(|f| f.contains("runs off the window")), "{off:?}");
        let cut = check(vec2(400.0, 80.0), 1.0, |ui| {
            let clip = Rect::from_min_size(ui.max_rect().min, vec2(40.0, 30.0));
            ui.painter().with_clip_rect(clip).text(
                clip.min,
                egui::Align2::LEFT_TOP,
                "Clipped words",
                egui::FontId::proportional(14.0),
                egui::Color32::WHITE,
            );
        });
        assert!(cut.iter().any(|f| f.contains("cut off")), "{cut:?}");
        let over = check(vec2(400.0, 80.0), 1.0, |ui| {
            let at = ui.max_rect().min;
            ui.painter().text(at, egui::Align2::LEFT_TOP, "First words", egui::FontId::proportional(14.0), egui::Color32::WHITE);
            ui.painter().text(at + vec2(30.0, 0.0), egui::Align2::LEFT_TOP, "Second", egui::FontId::proportional(14.0), egui::Color32::WHITE);
        });
        assert!(over.iter().any(|f| f.contains("runs into")), "{over:?}");
    }

    /// The harness's margin round what it shows (a window's face fills the rest).
    const ROOT: Vec2 = vec2(16.0, 16.0);

    /// A window too small for its words grows to them, and never under the size it was made with.
    #[test]
    fn a_window_grows_to_what_it_shows() {
        let fit = Fit::new("test", vec2(200.0, 60.0));
        let words = "Words enough to wrap onto several lines in a window only two hundred points wide, and more";
        let mut h = Harness::builder().with_size(vec2(200.0, 60.0) + ROOT).build_ui(move |ui| {
            fit.face(ui, Frame::NONE.inner_margin(10), |ui| {
                ui.label(words);
                ui.add(egui::Button::new("A button"));
            });
        });
        h.run_steps(2);
        let grown = fit.size(&h.ctx);
        assert!(grown.y > 60.0 && grown.x >= 200.0, "{grown:?}");
        h.set_size(grown + ROOT);
        h.run_steps(2);
        assert_eq!(fit.size(&h.ctx), grown, "it stays the size it needs");
        assert!(spills(&h.output().shapes, h.ctx.content_rect()).is_empty());
        // Short words: the size it was made with.
        let small = Fit::new("small", vec2(200.0, 60.0));
        let mut h = Harness::builder().with_size(vec2(200.0, 60.0) + ROOT).build_ui(move |ui| {
            small.face(ui, Frame::NONE.inner_margin(10), |ui| ui.label("Short"));
        });
        h.run_steps(2);
        assert_eq!(small.size(&h.ctx), vec2(200.0, 60.0));
    }

    /// A row's ends that would run into each other take the width they need instead.
    #[test]
    fn the_ends_of_a_row_never_run_into_each_other() {
        let fit = Fit::new("row", vec2(240.0, 50.0));
        let mut h = Harness::builder().with_size(vec2(240.0, 50.0) + ROOT).build_ui(move |ui| {
            fit.face(ui, Frame::NONE.inner_margin(8), |ui| {
                ends(ui, |ui| ui.label("Words at the left of a bar far too narrow for them"), |ui| ui.add(egui::Button::new("Stop")));
            });
        });
        h.run_steps(2);
        let before = spills(&h.output().shapes, h.ctx.content_rect());
        assert!(!before.is_empty(), "too narrow at first: {:?}", fit.size(&h.ctx));
        let grown = fit.size(&h.ctx);
        assert!(grown.x > 240.0, "{grown:?}");
        h.set_size(grown + ROOT);
        h.run_steps(2);
        let after = spills(&h.output().shapes, h.ctx.content_rect());
        assert!(after.is_empty(), "{after:?}");
    }

    #[test]
    fn a_window_opened_again_is_measured_out_of_sight_first() {
        let ctx = Context::default();
        let frame = |ctx: &Context| {
            let mut out = ctx.run_ui(egui::RawInput::default(), |_| {});
            out.textures_delta.clear();
        };
        frame(&ctx);
        let fit = Fit::new("again", vec2(100.0, 40.0));
        assert!(!fit.shown(&ctx), "opening: drawn out of sight");
        assert!(!fit.shown(&ctx), "nor later in the same frame");
        fit.need(&ctx, vec2(120.0, 40.0));
        frame(&ctx);
        assert!(fit.shown(&ctx), "the next frame: shown");
        assert_eq!(fit.size(&ctx), vec2(120.0, 40.0));
        frame(&ctx);
        assert!(fit.shown(&ctx), "and on");
        // Not asked for for a frame: it was closed. Opened again, it's measured again.
        frame(&ctx);
        frame(&ctx);
        assert!(!fit.shown(&ctx));
        assert_eq!(fit.size(&ctx), vec2(100.0, 40.0), "what it needed before is forgotten");
        // Hostile sizes stay sizes.
        fit.need(&ctx, vec2(f32::NAN, 1e9));
        fit.need(&ctx, vec2(1e9, f32::INFINITY));
        fit.need(&ctx, vec2(5e6, 7e6));
        let s = fit.size(&ctx);
        assert!(s.is_finite() && s.x <= MOST.x && s.y <= MOST.y, "{s:?}");
    }

    #[test]
    fn widgets_over_each_other_are_found_and_one_inside_another_is_not() {
        let r = |x: f32, w: f32| Rect::from_min_size(egui::pos2(x, 0.0), vec2(w, 20.0));
        let apart = [("Pause".to_string(), r(0.0, 30.0)), ("Stop".to_string(), r(40.0, 30.0))];
        assert!(crowded(&apart).is_empty());
        let over = [("01:15 Paused".to_string(), r(0.0, 80.0)), ("Pause".to_string(), r(70.0, 30.0))];
        assert_eq!(crowded(&over).len(), 1);
        let inside = [("Group".to_string(), r(0.0, 100.0)), ("Label".to_string(), r(10.0, 30.0))];
        assert!(crowded(&inside).is_empty());
    }
}
