//! The check the tests make of every small window of the service: drawn as the service draws it
//! (the interface's own font, at display scales 1, 1.5 and 2), its window grown as the service
//! grows it (`screengrabcraft_ui::fit`), no words run past the edge of what they're on, off the
//! window or into other words, and no two of its controls are over each other.

use egui::accesskit::Role;
use egui::{Context, Frame, Rect, Ui, Vec2, vec2};
use egui_kittest::Harness;
use egui_kittest::kittest::{By, NodeT as _, Queryable as _};
use screengrabcraft_model::Theme;
use screengrabcraft_ui::fit::{self, Fit};

/// Where the harness puts its contents: the window's face fills the rest.
pub const ROOT: f32 = 8.0;
/// The display scales every window is checked at.
pub const SCALES: [f32; 3] = [1.0, 1.5, 2.0];

/// A window's face drawn by `face` in `frame`, as the service draws it in a window sized by `fit`,
/// at `scale`, grown until it's the size it needs. The harness (to look at and press), the
/// window's size, and what doesn't fit (empty when everything does).
pub fn fitted(
    fit: Fit,
    frame: fn(&Context) -> Frame,
    scale: f32,
    theme: Theme,
    mut face: impl FnMut(&mut Ui) + 'static,
) -> (Harness<'static>, Vec2, Vec<String>) {
    let edge = vec2(2.0 * ROOT, 2.0 * ROOT);
    let mut size = fit.size(&Context::default());
    let mut h = Harness::builder().with_size(size + edge).with_pixels_per_point(scale).build_ui(move |ui| {
        screengrabcraft_ui::theme::apply(ui.ctx(), theme);
        fit.face(ui, frame(ui.ctx()), |ui| face(ui));
    });
    screengrabcraft_ui::install_fonts(&h.ctx);
    h.run_steps(2);
    // As the service does: the window takes the size its face asked for, until it asks no more.
    for _ in 0..6 {
        let wanted = fit.size(&h.ctx);
        if wanted == size {
            break;
        }
        size = wanted;
        h.set_size(size + edge);
        h.run_steps(2);
    }
    let mut found = check(&h);
    if fit.size(&h.ctx) != size {
        found.push(format!("the window keeps growing: {size:?}, then {:?}", fit.size(&h.ctx)));
    }
    (h, size, found)
}

/// What doesn't fit in what the harness last drew: words past their edges, and controls over
/// each other.
pub fn check<S>(h: &Harness<'_, S>) -> Vec<String> {
    let mut found = fit::spills(&h.output().shapes, h.ctx.content_rect());
    found.extend(fit::crowded(&controls(h)));
    found
}

/// Every control and label the harness shows, named, and where it is.
pub fn controls<S>(h: &Harness<'_, S>) -> Vec<(String, Rect)> {
    const SHOWN: [Role; 13] = [
        Role::Button,
        Role::CheckBox,
        Role::Label,
        Role::ProgressIndicator,
        Role::ComboBox,
        Role::Image,
        Role::Link,
        Role::SpinButton,
        Role::TextInput,
        Role::Slider,
        Role::RadioButton,
        Role::Switch,
        Role::ColorWell,
    ];
    h.query_all(By::new().predicate(|n| SHOWN.contains(&n.role())))
        .map(|n| {
            let a = n.accesskit_node();
            (a.label().or_else(|| a.value()).unwrap_or_default(), n.rect())
        })
        .filter(|(_, r)| r.is_positive())
        .collect()
}

/// Saves what the harness shows as `<name>@<scale>.png` in `SGC_SHOTS`, when that's set.
pub fn shot<S>(h: &mut Harness<'_, S>, name: &str, scale: f32) {
    if let Ok(dir) = std::env::var("SGC_SHOTS")
        && let Ok(img) = h.render()
    {
        let _ = img.save(format!("{dir}/{name}@{scale}.png"));
    }
}
