//! The standard parts of every window, dialog and popup, so that they all look like the system's
//! own (Windows 11's, which reads as ordinary on Linux and macOS too). Build every new window from
//! these (video recording, updates, …) rather than from egui's plain widgets:
//!
//! - **The frame:** the panel colour, a 1 px [`Tokens::border`] all round and
//!   [`WINDOW_RADIUS`] corners. [`window_frame`] for an `egui::Window` (with egui's own title bar
//!   turned off), [`popup_frame`] for a borderless window of the capture service (square, as the
//!   window itself is).
//! - **The title** at the top left, the close control (an X) at the top right: [`title_bar`]; a
//!   popup without a title has the X alone ([`close_button`]).
//! - **The content**, inset by [`PADDING`]: [`content`].
//! - **The footer**, the buttons on the right: [`dialog_footer`] across the bottom of a dialog (a
//!   band of its own colour under a line), [`footer`] inside a popup. The default button comes
//!   first and Cancel last on Windows and Linux (`[OK] [Cancel]`); macOS has them the other way
//!   round (`[Cancel] [OK]`). A button that is not an answer to the dialog (Quit) is set apart at
//!   the left ([`Role::Aside`]). Enter presses the default button and Esc the Cancel one.
//! - **Buttons** in a footer are at least [`BUTTON_SIZE`], with [`CONTROL_RADIUS`] corners. The
//!   default one is filled with the accent colour ([`primary_button`]), the others are outlined
//!   ([`button`]).
//! - A dialog with nothing to apply has a single Close button, on the right, which Enter and Esc
//!   both press.

use egui::{
    Align2, Color32, Context, CornerRadius, FontId, Frame, Key, Margin, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, Widget, pos2, vec2,
};

use crate::icons::{self, Icon};
use crate::theme::Tokens;
pub use crate::theme::{CONTROL_RADIUS, WINDOW_RADIUS};

/// The smallest a footer's button is, so a row of them lines up.
pub const BUTTON_SIZE: Vec2 = vec2(88.0, 30.0);
/// Around a window's content, and between its edge and its title.
pub const PADDING: i8 = 16;
/// Between the buttons of a footer.
pub const GAP: f32 = 8.0;
/// The height of a window's title bar.
pub const TITLE_HEIGHT: f32 = 44.0;
/// The size of a button's words.
const BUTTON_TEXT: f32 = 13.5;
/// Between a button's words and its edges.
const BUTTON_PAD: f32 = 14.0;

/// The interface's bold type, or the plain one where the fonts aren't installed (egui panics on a
/// family it doesn't know).
pub fn strong_font(ctx: &Context, size: f32) -> FontId {
    let family = egui::FontFamily::Name("sans-bold".into());
    if ctx.fonts(|f| f.definitions().families.contains_key(&family)) { FontId::new(size, family) } else { FontId::proportional(size) }
}

// ---- frames ----

/// The frame of a dialog (an `egui::Window` with `title_bar(false)`): its parts bring their own
/// margins.
pub fn window_frame(style: &egui::Style) -> Frame {
    let t = Tokens::of(&style.visuals);
    Frame::window(style).fill(t.panel).stroke(Stroke::new(1.0, t.border)).corner_radius(WINDOW_RADIUS).inner_margin(0)
}

/// The frame of one of the capture service's borderless windows (a notice, the countdown, the
/// recording control), filling the window: its outline is the window's edge. Its corners are
/// square: outside round ones a borderless window shows black where it can't be see-through
/// (Direct3D 12 on Windows can't).
pub fn popup_frame(ctx: &Context) -> Frame {
    let t = Tokens::get(ctx);
    Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border)).inner_margin(Margin::same(12))
}

/// A window's title at the top left, and its close control at the top right. True when the close
/// control was pressed.
pub fn title_bar(ui: &mut Ui, title: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), TITLE_HEIGHT), Sense::hover());
    let font = strong_font(ui.ctx(), 14.0);
    ui.painter().text(pos2(rect.min.x + f32::from(PADDING), rect.center().y), Align2::LEFT_CENTER, title, font, t.text);
    let x = Rect::from_center_size(pos2(rect.max.x - 8.0 - 16.0, rect.center().y), vec2(32.0, 28.0));
    close_button(ui, x).clicked()
}

/// The X that closes a window or a popup, in `rect`.
pub fn close_button(ui: &mut Ui, rect: Rect) -> Response {
    let t = Tokens::get(ui.ctx());
    let resp = ui.interact(rect, ui.auto_id_with("close-x"), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, "Close window"));
    let p = ui.painter();
    if resp.is_pointer_button_down_on() {
        p.rect_filled(rect, CONTROL_RADIUS, t.control_pressed);
    } else if resp.hovered() {
        p.rect_filled(rect, CONTROL_RADIUS, t.control_hover);
    }
    if resp.has_focus() {
        p.rect_stroke(rect, CONTROL_RADIUS, Stroke::new(1.5, t.text), StrokeKind::Inside);
    }
    let ink = if resp.hovered() { t.text } else { t.muted };
    icons::paint(p, Rect::from_center_size(rect.center(), vec2(14.0, 14.0)), Icon::Close, ink);
    resp.on_hover_text("Close")
}

/// A window's content, inset like its title.
pub fn content<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    let margin = Margin { left: PADDING, right: PADDING, top: 2, bottom: PADDING + 4 };
    Frame::NONE.inner_margin(margin).show(ui, add).inner
}

// ---- buttons ----

/// A button as the system draws one: at least [`BUTTON_SIZE`], outlined, or filled with the
/// accent colour when it is the default one.
pub struct StandardButton<'a> {
    label: &'a str,
    primary: bool,
    enabled: bool,
}

impl<'a> StandardButton<'a> {
    pub fn new(label: &'a str) -> StandardButton<'a> {
        StandardButton { label, primary: false, enabled: true }
    }

    /// The default button of its window.
    pub fn primary(mut self, primary: bool) -> StandardButton<'a> {
        self.primary = primary;
        self
    }

    pub fn enabled(mut self, enabled: bool) -> StandardButton<'a> {
        self.enabled = enabled;
        self
    }

    /// Its size: its words with room around them, and no smaller than [`BUTTON_SIZE`].
    pub fn size(&self, ui: &Ui) -> Vec2 {
        let words = ui.painter().layout_no_wrap(self.label.to_string(), FontId::proportional(BUTTON_TEXT), Color32::WHITE).size().x;
        vec2((words + BUTTON_PAD * 2.0).ceil().max(BUTTON_SIZE.x), BUTTON_SIZE.y)
    }

    fn paint(&self, ui: &Ui, rect: Rect, resp: &Response) {
        let t = Tokens::get(ui.ctx());
        let pressed = resp.is_pointer_button_down_on();
        let hot = resp.hovered();
        let (fill, edge, ink) = if !self.enabled {
            (t.control, t.border, t.muted)
        } else if self.primary {
            let fill = if pressed {
                t.accent_pressed
            } else if hot {
                t.accent_hover
            } else {
                t.accent
            };
            (fill, fill, t.accent_text)
        } else {
            let fill = if pressed {
                t.control_pressed
            } else if hot {
                t.control_hover
            } else {
                t.control
            };
            (fill, t.control_border, if pressed { t.muted } else { t.text })
        };
        let p = ui.painter();
        p.rect(rect, CONTROL_RADIUS, fill, Stroke::new(1.0, edge), StrokeKind::Inside);
        p.text(rect.center(), Align2::CENTER_CENTER, self.label, FontId::proportional(BUTTON_TEXT), ink);
        if resp.has_focus() {
            // Where the keyboard is, as the system shows it.
            p.rect_stroke(rect.expand(2.0), CornerRadius::same(CONTROL_RADIUS + 2), Stroke::new(1.5, t.text), StrokeKind::Inside);
        }
    }

    fn sense(&self) -> Sense {
        if self.enabled { Sense::click() } else { Sense::hover() }
    }

    fn finish(self, ui: &Ui, rect: Rect, resp: Response) -> Response {
        let (enabled, label) = (self.enabled, self.label);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, enabled, label));
        if ui.is_rect_visible(rect) {
            self.paint(ui, rect, &resp);
        }
        resp
    }

    /// Draws it in `rect`, a part of `ui` already taken (by a layout of one's own, like
    /// [`footer`]'s), as the widget `id`.
    pub fn put(self, ui: &mut Ui, rect: Rect, id: egui::Id) -> Response {
        let resp = ui.interact(rect, id, self.sense());
        self.finish(ui, rect, resp)
    }
}

impl Widget for StandardButton<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let (rect, resp) = ui.allocate_exact_size(self.size(ui), self.sense());
        self.finish(ui, rect, resp)
    }
}

/// An outlined button, at least [`BUTTON_SIZE`].
pub fn button(ui: &mut Ui, label: &str) -> Response {
    ui.add(StandardButton::new(label))
}

/// The default button of a window, filled with the accent colour, at least [`BUTTON_SIZE`].
pub fn primary_button(ui: &mut Ui, label: &str) -> Response {
    ui.add(StandardButton::new(label).primary(true))
}

// ---- footers ----

/// What a footer's button is to its window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    /// The window's main action (OK, Apply, Save, Close when there is nothing else): filled with
    /// the accent colour, pressed by Enter.
    Default,
    /// Another action, beside the default one.
    Plain,
    /// Closes the window without doing anything: pressed by Esc.
    Cancel,
    /// Not an answer to the window (Quit): set apart at the footer's left.
    Aside,
}

/// A button of a footer.
#[derive(Clone, Copy, Debug)]
pub struct FooterButton<'a> {
    pub label: &'a str,
    pub role: Role,
    pub enabled: bool,
    /// Said when the pointer rests on it.
    pub tip: Option<&'a str>,
}

impl<'a> FooterButton<'a> {
    pub fn new(label: &'a str, role: Role) -> FooterButton<'a> {
        FooterButton { label, role, enabled: true, tip: None }
    }

    /// The default button ([`Role::Default`]).
    pub fn primary(label: &'a str) -> FooterButton<'a> {
        FooterButton::new(label, Role::Default)
    }

    pub fn plain(label: &'a str) -> FooterButton<'a> {
        FooterButton::new(label, Role::Plain)
    }

    pub fn cancel(label: &'a str) -> FooterButton<'a> {
        FooterButton::new(label, Role::Cancel)
    }

    pub fn aside(label: &'a str) -> FooterButton<'a> {
        FooterButton::new(label, Role::Aside)
    }

    pub fn enabled(mut self, enabled: bool) -> FooterButton<'a> {
        self.enabled = enabled;
        self
    }

    pub fn tip(mut self, tip: &'a str) -> FooterButton<'a> {
        self.tip = Some(tip);
        self
    }
}

/// Where a footer's buttons go, each side left to right, as indices into `buttons`: those set
/// apart at the left; the others at the right, the default first and Cancel last, or (`mac`) the
/// other way round with the default last. Buttons of the same role keep their order.
pub fn arrange(buttons: &[FooterButton<'_>], mac: bool) -> (Vec<usize>, Vec<usize>) {
    let of = |role: Role| buttons.iter().enumerate().filter(move |(_, b)| b.role == role).map(|(i, _)| i);
    let left: Vec<usize> = of(Role::Aside).collect();
    let right: Vec<usize> = if mac {
        of(Role::Plain).chain(of(Role::Cancel)).chain(of(Role::Default)).collect()
    } else {
        of(Role::Default).chain(of(Role::Plain)).chain(of(Role::Cancel)).collect()
    };
    (left, right)
}

/// A row of buttons as wide as `ui`, laid out by [`arrange`] for this platform. Returns the index
/// (in `buttons`) of the one clicked. Keys are [`dialog_footer`]'s (or [`pressed_key`]'s).
pub fn footer(ui: &mut Ui, buttons: &[FooterButton<'_>]) -> Option<usize> {
    let (left, right) = arrange(buttons, cfg!(target_os = "macos"));
    let size = |ui: &Ui, i: usize| buttons.get(i).map_or(BUTTON_SIZE, |b| StandardButton::new(b.label).size(ui));
    let needed: f32 = buttons.iter().enumerate().map(|(i, _)| size(ui, i).x + GAP).sum::<f32>() - GAP;
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width().max(needed.max(0.0)), BUTTON_SIZE.y), Sense::hover());
    // Every button's place, worked out first so they are made (and tabbed through) left to right.
    let mut places: Vec<(usize, Rect)> = Vec::with_capacity(buttons.len());
    let mut x = row.min.x;
    for &i in &left {
        let s = size(ui, i);
        places.push((i, Rect::from_min_size(pos2(x, row.min.y), s)));
        x += s.x + GAP;
    }
    let width: f32 = right.iter().map(|&i| size(ui, i).x + GAP).sum::<f32>() - GAP;
    let mut x = row.max.x - width.max(0.0);
    for &i in &right {
        let s = size(ui, i);
        places.push((i, Rect::from_min_size(pos2(x, row.min.y), s)));
        x += s.x + GAP;
    }
    let mut picked = None;
    for (i, rect) in places {
        let Some(b) = buttons.get(i) else { continue };
        let id = ui.auto_id_with(("footer-button", i));
        let mut resp = StandardButton::new(b.label).primary(b.role == Role::Default).enabled(b.enabled).put(ui, rect, id);
        if let Some(tip) = b.tip {
            resp = resp.on_hover_text(tip);
        }
        if resp.clicked() {
            picked = Some(i);
        }
    }
    picked
}

/// The button Enter or Esc pressed, if either was: Enter the default button, Esc the Cancel one,
/// or the only button when there is nothing else (a Close).
pub fn pressed_key(ui: &Ui, buttons: &[FooterButton<'_>]) -> Option<usize> {
    let first = |role: Role| buttons.iter().position(|b| b.role == role && b.enabled);
    if ui.input(|i| i.key_pressed(Key::Enter))
        && let Some(i) = first(Role::Default)
    {
        return Some(i);
    }
    if ui.input(|i| i.key_pressed(Key::Escape)) {
        if let Some(i) = first(Role::Cancel) {
            return Some(i);
        }
        let answers: Vec<usize> = buttons.iter().enumerate().filter(|(_, b)| b.role != Role::Aside && b.enabled).map(|(i, _)| i).collect();
        if let [only] = answers.as_slice() {
            return Some(*only);
        }
    }
    None
}

/// The footer across the bottom of a dialog: a band of its own colour under a line, the buttons
/// on the right (see [`footer`]). Enter and Esc press theirs ([`pressed_key`]). Returns the index
/// (in `buttons`) of the one pressed.
pub fn dialog_footer(ui: &mut Ui, buttons: &[FooterButton<'_>]) -> Option<usize> {
    let t = Tokens::get(ui.ctx());
    // Inside the window's own outline and round bottom corners.
    let r = WINDOW_RADIUS.saturating_sub(1);
    let band = Frame::NONE.fill(t.footer).inner_margin(Margin::symmetric(PADDING, 14)).corner_radius(CornerRadius { nw: 0, ne: 0, sw: r, se: r });
    let shown = band.show(ui, |ui| {
        ui.set_min_width(ui.available_width());
        footer(ui, buttons)
    });
    let top = shown.response.rect;
    ui.painter().hline(top.x_range(), top.top(), Stroke::new(1.0, t.border));
    shown.inner.or_else(|| pressed_key(ui, buttons))
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;

    use super::*;

    fn buttons() -> Vec<FooterButton<'static>> {
        vec![FooterButton::cancel("Cancel"), FooterButton::aside("Quit"), FooterButton::primary("OK"), FooterButton::plain("Later")]
    }

    #[test]
    fn the_default_comes_first_on_windows_and_last_on_a_mac() {
        let b = buttons();
        assert_eq!(arrange(&b, false), (vec![1], vec![2, 3, 0]), "[Quit] … [OK] [Later] [Cancel]");
        assert_eq!(arrange(&b, true), (vec![1], vec![3, 0, 2]), "[Quit] … [Later] [Cancel] [OK]");
        assert_eq!(arrange(&[], false), (vec![], vec![]));
    }

    /// A 480-point-wide window with the footer at its bottom, and what was pressed.
    fn dialog() -> Harness<'static, Option<usize>> {
        Harness::builder().with_size(egui::vec2(480.0, 200.0)).build_ui_state(
            |ui, pressed: &mut Option<usize>| {
                crate::theme::apply(ui.ctx(), screengrabcraft_model::Theme::Light);
                egui::CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
                    ui.add_space(120.0);
                    if let Some(i) = dialog_footer(ui, &buttons()) {
                        *pressed = Some(i);
                    }
                });
            },
            None,
        )
    }

    #[test]
    fn a_footer_has_its_buttons_on_the_right_in_the_platforms_order() {
        let mut h = dialog();
        h.run_steps(2);
        let rect = |label: &str| h.get_by_label(label).rect();
        let (ok, later, cancel, quit) = (rect("OK"), rect("Later"), rect("Cancel"), rect("Quit"));
        for r in [ok, later, cancel, quit] {
            assert!(r.width() >= BUTTON_SIZE.x && (r.height() - BUTTON_SIZE.y).abs() < 0.5, "{r:?}");
            assert!((r.center().y - ok.center().y).abs() < 0.5, "one row: {r:?}");
        }
        // Quit at the left edge, the rest at the right one, as far in.
        let inset = quit.min.x;
        assert!(inset >= f32::from(PADDING) && inset < 40.0, "{quit:?}");
        let (first, last) = if cfg!(target_os = "macos") { (later, ok) } else { (ok, cancel) };
        assert!((last.max.x - (480.0 - inset)).abs() < 1.0, "{last:?}");
        assert!(quit.max.x < first.min.x);
        if cfg!(target_os = "macos") {
            assert!(later.max.x < cancel.min.x && cancel.max.x < ok.min.x);
        } else {
            assert!(ok.max.x < later.min.x && later.max.x < cancel.min.x);
        }
        // The same gap between neighbours.
        assert!((later.min.x - ok.max.x - GAP).abs() < 1.0 || (cancel.min.x - later.max.x - GAP).abs() < 1.0);
    }

    #[test]
    fn enter_presses_the_default_button_and_esc_the_cancel_one() {
        for (key, wanted) in [(Key::Enter, 2), (Key::Escape, 0)] {
            let mut h = dialog();
            h.run_steps(2);
            h.key_press(key);
            h.run_steps(1);
            assert_eq!(*h.state(), Some(wanted), "{key:?}");
        }
        let mut h = dialog();
        h.run_steps(2);
        h.get_by_label("Later").click();
        h.run_steps(1);
        assert_eq!(*h.state(), Some(3));
    }

    #[test]
    fn esc_closes_a_dialog_that_only_has_a_close_button() {
        let only = [FooterButton::primary("Close")];
        assert_eq!(arrange(&only, false), (vec![], vec![0]));
        let mut h = Harness::builder().with_size(egui::vec2(300.0, 100.0)).build_ui_state(
            |ui, pressed: &mut Option<usize>| {
                if let Some(i) = dialog_footer(ui, &only) {
                    *pressed = Some(i);
                }
            },
            None,
        );
        h.run_steps(1);
        h.key_press(Key::Escape);
        h.run_steps(1);
        assert_eq!(*h.state(), Some(0));
    }
}
