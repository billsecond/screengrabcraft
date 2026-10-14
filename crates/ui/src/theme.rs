//! Colours and spacing of ScreenGrabCraft's windows, light and dark.

use egui::{Color32, Context, CornerRadius, Stroke, Visuals};
use screengrabcraft_model::Theme;

/// The colours the interface is drawn with.
#[derive(Clone, Copy, Debug)]
pub struct Tokens {
    pub dark: bool,
    /// The app's colour (the editor's indigo, as in its icon).
    pub accent: Color32,
    pub accent_text: Color32,
    pub window: Color32,
    pub panel: Color32,
    pub toolbar: Color32,
    /// Around the picture on the canvas.
    pub canvas: Color32,
    pub checker_a: Color32,
    pub checker_b: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub border: Color32,
    pub hover: Color32,
    pub selected: Color32,
    /// Selection outlines and handles on the canvas.
    pub handle: Color32,
    pub danger: Color32,
}

impl Tokens {
    pub fn light() -> Tokens {
        Tokens {
            dark: false,
            accent: Color32::from_rgb(0x4E, 0x3C, 0xA8),
            accent_text: Color32::WHITE,
            window: Color32::from_rgb(0xF7, 0xF7, 0xF9),
            panel: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            toolbar: Color32::from_rgb(0xF0, 0xF0, 0xF4),
            canvas: Color32::from_rgb(0xDE, 0xDE, 0xE4),
            checker_a: Color32::from_rgb(0xFF, 0xFF, 0xFF),
            checker_b: Color32::from_rgb(0xE6, 0xE6, 0xEA),
            text: Color32::from_rgb(0x1D, 0x1D, 0x22),
            muted: Color32::from_rgb(0x6A, 0x6A, 0x75),
            border: Color32::from_rgb(0xD4, 0xD4, 0xDC),
            hover: Color32::from_rgb(0xE6, 0xE3, 0xF6),
            selected: Color32::from_rgb(0xD9, 0xD3, 0xF7),
            handle: Color32::from_rgb(0x2F, 0x6F, 0xED),
            danger: Color32::from_rgb(0xC6, 0x28, 0x28),
        }
    }

    pub fn dark() -> Tokens {
        Tokens {
            dark: true,
            accent: Color32::from_rgb(0x8C, 0x7B, 0xF0),
            accent_text: Color32::from_rgb(0x12, 0x10, 0x20),
            window: Color32::from_rgb(0x1E, 0x1E, 0x22),
            panel: Color32::from_rgb(0x26, 0x26, 0x2B),
            toolbar: Color32::from_rgb(0x2B, 0x2B, 0x31),
            canvas: Color32::from_rgb(0x17, 0x17, 0x1A),
            checker_a: Color32::from_rgb(0x3A, 0x3A, 0x40),
            checker_b: Color32::from_rgb(0x30, 0x30, 0x35),
            text: Color32::from_rgb(0xEC, 0xEC, 0xF1),
            muted: Color32::from_rgb(0xA0, 0xA0, 0xAC),
            border: Color32::from_rgb(0x3C, 0x3C, 0x44),
            hover: Color32::from_rgb(0x38, 0x34, 0x52),
            selected: Color32::from_rgb(0x44, 0x3C, 0x70),
            handle: Color32::from_rgb(0x5B, 0x9B, 0xFF),
            danger: Color32::from_rgb(0xFF, 0x6B, 0x6B),
        }
    }

    /// The tokens for the context's current look.
    pub fn get(ctx: &Context) -> Tokens {
        if ctx.global_style().visuals.dark_mode { Tokens::dark() } else { Tokens::light() }
    }
}

/// egui's look for one of the two themes, from the tokens.
fn visuals(dark: bool) -> Visuals {
    let t = if dark { Tokens::dark() } else { Tokens::light() };
    let mut v = if dark { Visuals::dark() } else { Visuals::light() };
    v.window_fill = t.panel;
    v.panel_fill = t.window;
    v.extreme_bg_color = if dark { Color32::from_rgb(0x18, 0x18, 0x1C) } else { Color32::WHITE };
    v.selection.bg_fill = t.accent;
    v.selection.stroke = Stroke::new(1.0, t.accent_text);
    v.hyperlink_color = t.accent;
    v.widgets.hovered.weak_bg_fill = t.hover;
    v.widgets.hovered.bg_fill = t.hover;
    v.widgets.active.weak_bg_fill = t.selected;
    v.widgets.inactive.corner_radius = CornerRadius::same(5);
    v.widgets.hovered.corner_radius = CornerRadius::same(5);
    v.widgets.active.corner_radius = CornerRadius::same(5);
    v.window_corner_radius = CornerRadius::same(8);
    v.window_stroke = Stroke::new(1.0, t.border);
    v
}

/// Sets both looks and which one is used: the owner's choice, or the system's (followed as it
/// changes).
pub fn apply(ctx: &Context, theme: Theme) {
    ctx.set_visuals_of(egui::Theme::Light, visuals(false));
    ctx.set_visuals_of(egui::Theme::Dark, visuals(true));
    ctx.set_theme(match theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    });
}
