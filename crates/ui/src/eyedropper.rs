//! The eyedropper: a colour taken from the picture for one of the colour buttons.
//!
//! A colour button's menu starts it ([`start`], naming the button); the canvas then shows the
//! eyedropper as the pointer and, on a click, hands the colour under it over ([`picked_at`], with
//! the pixel it was on, for a window that wants the place too: [`take_point`]); the
//! button takes it on its next frame ([`take`]) like any colour chosen in its menu, so it goes to
//! the selection or the tool the same way, with its undo step. Esc or the right button puts the
//! eyedropper away ([`cancel`]). The state lives in egui's memory for the window, as the buttons
//! and the canvas are drawn apart.

use egui::{Context, Id};
use screengrabcraft_model::Color;

#[derive(Clone, Default)]
struct State {
    /// The colour button the colour is being picked for.
    for_button: Option<String>,
    /// A colour picked, for that button.
    picked: Option<(String, Color)>,
    /// The pixel of the picture it was picked on, for that button.
    point: Option<(String, (u32, u32))>,
}

fn id() -> Id {
    Id::new("screengrabcraft-eyedropper")
}

fn get(ctx: &Context) -> State {
    ctx.data(|d| d.get_temp::<State>(id())).unwrap_or_default()
}

fn set(ctx: &Context, s: State) {
    ctx.data_mut(|d| d.insert_temp(id(), s));
}

/// Picks a colour from the picture for the colour button called `button`.
pub fn start(ctx: &Context, button: &str) {
    set(ctx, State { for_button: Some(button.to_string()), ..State::default() });
}

/// Whether a colour is being picked from the picture.
pub fn active(ctx: &Context) -> bool {
    get(ctx).for_button.is_some()
}

/// The colour button the colour is being picked for.
pub fn for_button(ctx: &Context) -> Option<String> {
    get(ctx).for_button
}

/// Puts the eyedropper away without a colour.
pub fn cancel(ctx: &Context) {
    set(ctx, State::default());
}

/// The colour clicked on the picture: the eyedropper is put away and the button gets it.
pub fn picked(ctx: &Context, c: Color) {
    picked_at(ctx, c, None);
}

/// [`picked`], on the picture's pixel `at`.
pub fn picked_at(ctx: &Context, c: Color, at: Option<(u32, u32)>) {
    let s = get(ctx);
    if let Some(button) = s.for_button {
        let point = at.map(|p| (button.clone(), p));
        set(ctx, State { for_button: None, picked: Some((button, c)), point });
    }
}

/// The pixel the colour for the colour button called `button` was picked on, once.
pub fn take_point(ctx: &Context, button: &str) -> Option<(u32, u32)> {
    let s = get(ctx);
    match s.point {
        Some((b, p)) if b == button => {
            set(ctx, State { point: None, ..s });
            Some(p)
        }
        _ => None,
    }
}

/// The colour picked for the colour button called `button`, once.
pub fn take(ctx: &Context, button: &str) -> Option<Color> {
    let s = get(ctx);
    match s.picked {
        Some((b, c)) if b == button => {
            set(ctx, State { picked: None, ..s });
            Some(c)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_colour_picked_goes_to_the_button_it_was_picked_for_once() {
        let ctx = Context::default();
        assert!(!active(&ctx));
        picked(&ctx, Color::RED);
        assert_eq!(take(&ctx, "Fill colour"), None, "nothing was being picked");
        start(&ctx, "Fill colour");
        assert!(active(&ctx));
        assert_eq!(for_button(&ctx).as_deref(), Some("Fill colour"));
        picked(&ctx, Color::RED);
        assert!(!active(&ctx), "put away once a colour is picked");
        assert_eq!(take(&ctx, "Text colour"), None, "not for another button");
        assert_eq!(take(&ctx, "Fill colour"), Some(Color::RED));
        assert_eq!(take(&ctx, "Fill colour"), None, "once");
        start(&ctx, "Text colour");
        cancel(&ctx);
        assert!(!active(&ctx));
    }

    #[test]
    fn the_pixel_a_colour_was_picked_on_is_kept_for_its_button_once() {
        let ctx = Context::default();
        start(&ctx, "Colour to replace");
        picked_at(&ctx, Color::RED, Some((12, 7)));
        assert_eq!(take_point(&ctx, "Fill colour"), None, "not for another button");
        assert_eq!(take(&ctx, "Colour to replace"), Some(Color::RED));
        assert_eq!(take_point(&ctx, "Colour to replace"), Some((12, 7)), "kept apart from the colour");
        assert_eq!(take_point(&ctx, "Colour to replace"), None, "once");
        start(&ctx, "Colour to replace");
        picked(&ctx, Color::BLACK);
        assert_eq!(take_point(&ctx, "Colour to replace"), None, "picked nowhere in particular");
    }
}
