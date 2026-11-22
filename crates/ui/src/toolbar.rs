//! View › Customize Tool Bar: which tools the tool bar shows after Favorites, and in what order,
//! kept in the editor's settings. A tool left off is still picked by its key.

use egui::{Ui, vec2};

use crate::editor::{Editor, Tool};
use crate::widgets::{self, FooterButton, Role};

/// The window's title.
pub const TITLE: &str = "Customize Tool Bar";

/// The tool bar being chosen: every tool, in the bar's order, each on it or not.
#[derive(Clone, Debug, PartialEq)]
pub struct BarDraft {
    pub tools: Vec<(Tool, bool)>,
}

impl BarDraft {
    /// The bar as `chosen` says (none: the usual one), then the tools left off it.
    pub fn of(chosen: &[String]) -> BarDraft {
        let on: Vec<Tool> = Tool::bar_of(chosen).into_iter().map(|(t, _)| t).collect();
        let mut tools: Vec<(Tool, bool)> = on.iter().map(|t| (*t, true)).collect();
        tools.extend(Tool::BAR.into_iter().filter(|t| !on.contains(t)).map(|t| (t, false)));
        BarDraft { tools }
    }

    /// What is kept in the settings: the tools on the bar, in order; none for the usual bar.
    pub fn chosen(&self) -> Vec<String> {
        let on: Vec<Tool> = self.tools.iter().filter(|(_, on)| *on).map(|(t, _)| *t).collect();
        if on == Tool::BAR.to_vec() { Vec::new() } else { on.iter().map(|t| t.id().to_string()).collect() }
    }

    /// Whether at least one tool is on the bar.
    pub fn any_on(&self) -> bool {
        self.tools.iter().any(|(_, on)| *on)
    }

    /// The tool at `i` one place earlier (`up`) or later.
    pub fn shift(&mut self, i: usize, up: bool) {
        let j = if up { i.checked_sub(1) } else { i.checked_add(1).filter(|j| *j < self.tools.len()) };
        if let Some(j) = j
            && i < self.tools.len()
        {
            self.tools.swap(i, j);
        }
    }
}

/// How a tool is named here (the Shape button for the rectangle, the ellipse and the polygon).
pub fn name(t: Tool) -> &'static str {
    if t.is_shape() {
        return "Shape";
    }
    t.info().map_or("Tool", |(_, n, _)| n)
}

/// The window, while it is open.
pub fn show(ed: &mut Editor, ctx: &egui::Context) {
    let Some(mut d) = ed.customizing_bar.take() else { return };
    let mut close = false;
    let mut save = false;
    let width = 360.0;
    egui::Window::new(TITLE)
        .title_bar(false)
        .collapsible(false)
        .resizable(false)
        .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
        .min_width(width)
        .max_width(width)
        .frame(widgets::window_frame(&ctx.global_style()))
        .show(ctx, |ui| {
            if widgets::title_bar(ui, TITLE) {
                close = true;
            }
            widgets::content(ui, |ui| rows(ui, &mut d));
            let buttons = [FooterButton::primary("Save").enabled(d.any_on()), FooterButton::plain("Usual Tools"), FooterButton::cancel("Cancel")];
            match widgets::dialog_footer(ui, &buttons).and_then(|i| buttons.get(i)).map(|b| (b.role, b.label)) {
                Some((Role::Default, _)) => save = true,
                Some((Role::Plain, _)) => d = BarDraft::of(&[]),
                Some(_) => close = true,
                None => {}
            }
        });
    if save {
        ed.settings.tool_bar = d.chosen();
        ed.services.save_settings(&ed.settings);
        return;
    }
    if !close {
        ed.customizing_bar = Some(d);
    }
}

/// A row for each tool: on the bar or not, and moved up or down.
fn rows(ui: &mut Ui, d: &mut BarDraft) {
    let t = crate::theme::Tokens::get(ui.ctx());
    ui.label(
        egui::RichText::new("Tick the tools to show, and move them up or down. A tool left off is still picked by its key.")
            .size(12.0)
            .color(t.muted),
    );
    ui.add_space(6.0);
    let mut shift = None;
    let n = d.tools.len();
    // All of them in sight when the window has room, else a scroll.
    let room = (ui.ctx().content_rect().height() - 300.0).clamp(160.0, 560.0);
    egui::ScrollArea::vertical().max_height(room).show(ui, |ui| {
        for (i, (tool, on)) in d.tools.iter_mut().enumerate() {
            ui.horizontal(|ui| {
                let label = name(*tool);
                let key = tool.info().map(|(_, _, k)| k.name().to_string()).unwrap_or_default();
                ui.add_sized(vec2(28.0, 20.0), egui::Label::new(egui::RichText::new(key).monospace().color(t.muted)));
                ui.checkbox(on, label);
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let down = ui.add_enabled(i + 1 < n, egui::Button::new("↓").small());
                    down.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, i + 1 < n, format!("Move {label} down")));
                    if down.clicked() {
                        shift = Some((i, false));
                    }
                    let up = ui.add_enabled(i > 0, egui::Button::new("↑").small());
                    up.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, i > 0, format!("Move {label} up")));
                    if up.clicked() {
                        shift = Some((i, true));
                    }
                });
            });
        }
    });
    if let Some((i, up)) = shift {
        d.shift(i, up);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Kind;

    #[test]
    fn the_bar_is_kept_by_name_and_the_usual_one_as_none() {
        let usual = BarDraft::of(&[]);
        assert_eq!(usual.tools.len(), Tool::BAR.len());
        assert!(usual.tools.iter().all(|(_, on)| *on));
        assert!(usual.chosen().is_empty(), "the usual bar is kept as none");
        let mut d = usual.clone();
        d.shift(0, true);
        assert_eq!(d, usual, "the first can't go up");
        d.shift(1, true);
        assert_eq!(d.tools.first().map(|(t, _)| *t), Some(Tool::Hand));
        if let Some((_, on)) = d.tools.iter_mut().find(|(t, _)| *t == Tool::Draw(Kind::Magnify)) {
            *on = false;
        }
        let kept = d.chosen();
        assert_eq!(kept.first().map(String::as_str), Some("hand"));
        assert!(!kept.iter().any(|k| k == "magnify"));
        // Read back: the same order, the one left off at the end, unticked.
        let back = BarDraft::of(&kept);
        assert_eq!(back.tools.last(), Some(&(Tool::Draw(Kind::Magnify), false)));
        assert_eq!(back.chosen(), kept);
        // Names that aren't tools, or twice, are let go.
        let odd = vec!["teapot".to_string(), "pen".to_string(), "pen".to_string(), "shape".to_string()];
        let bar = Tool::bar_of(&odd);
        assert_eq!(bar.iter().map(|(t, _)| *t).collect::<Vec<_>>(), vec![Tool::Draw(Kind::Pen), Tool::Draw(Kind::Rect)]);
        assert!(bar.iter().all(|(_, gap)| *gap), "each from another group: a gap before each");
        for t in Tool::BAR {
            assert_eq!(Tool::from_id(t.id()), Some(t));
        }
    }
}
