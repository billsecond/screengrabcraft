//! ScreenGrabCraft's interface, in egui: the annotation [`editor`] (the same on the desktop and
//! in the browser) and the capture [`overlay`] the capture service shows on every monitor.
//!
//! Layer L6. What only a host can do (the clipboard, file dialogs, the capture service) is asked
//! through [`services::Services`]; this crate itself builds for the browser.
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

pub mod canvas;
pub mod dialogs;
pub mod editor;
pub mod icons;
pub mod overlay;
pub mod playback;
pub mod props;
pub mod services;
pub mod strip;
pub mod theme;

use std::sync::Arc;

pub use editor::{Editor, Tool};
pub use services::{DialogResult, NoServices, OpenedFile, RecentItem, Services};

use screengrabcraft_model::Color;

pub fn to_color32(c: Color) -> egui::Color32 {
    egui::Color32::from_rgba_unmultiplied(c.r, c.g, c.b, c.a)
}

pub fn from_color32(c: egui::Color32) -> Color {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    Color::rgba(r, g, b, a)
}

/// The interface font (Inter) and the annotation fonts as named families, so text being typed
/// looks as it will be drawn.
pub fn install_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    for (name, bytes) in screengrabcraft_render::text::font_files() {
        fonts.font_data.insert(name.to_string(), Arc::new(egui::FontData::from_static(bytes)));
        fonts.families.insert(egui::FontFamily::Name(name.into()), vec![name.to_string()]);
    }
    // Inter for the interface too, egui's own fonts after it for what Inter lacks.
    if let Some(list) = fonts.families.get_mut(&egui::FontFamily::Proportional) {
        list.insert(0, "inter".to_string());
    }
    ctx.set_fonts(fonts);
}

/// A dark or light look for a window that follows the system unless told otherwise.
pub fn setup(ctx: &egui::Context, theme: screengrabcraft_model::Theme) {
    install_fonts(ctx);
    theme::apply(ctx, theme);
}

#[cfg(test)]
mod tests;
