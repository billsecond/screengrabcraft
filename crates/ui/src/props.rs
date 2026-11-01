//! The properties panel: how the selected annotations (or the next one the tool draws) look.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, vec2};
use screengrabcraft_model::color::PALETTE;
use screengrabcraft_model::{ArrowHeads, Color, Kind, Shape, StampKind, Style, TextAlign};

use crate::editor::{Editor, Tool};
use crate::theme::Tokens;
use crate::{from_color32, to_color32};

/// The colour a kind is mostly seen in, which the big swatches set.
fn main_colour(kind: Kind, s: &Style) -> Color {
    match kind {
        Kind::Text => s.text_color,
        Kind::Step | Kind::Stamp => s.fill,
        _ => s.stroke,
    }
}

fn set_main_colour(kind: Kind, s: &mut Style, c: Color) {
    match kind {
        Kind::Text => s.text_color = c,
        Kind::Step | Kind::Stamp => s.fill = c,
        Kind::Highlight => s.stroke = c.with_alpha(s.stroke.a.max(60)),
        _ => s.stroke = c,
    }
}

/// Copies onto `target` the fields that differ between `old` and `new`.
fn apply_changes(old: &Style, new: &Style, target: &mut Style) {
    macro_rules! field {
        ($($f:ident),*) => { $( if old.$f != new.$f { target.$f = new.$f; } )* };
    }
    field!(stroke, fill, width, opacity, shadow, dashed, font_size, bold, italic, mono, text_color, align, strength);
}

fn swatch(ui: &mut Ui, c: Color, selected: bool, size: f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    if c.a == 0 {
        ui.painter().rect_filled(r, 4.0, Color32::WHITE);
        ui.painter().line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.5, Color32::from_rgb(0xE5, 0x39, 0x35)));
    } else {
        ui.painter().rect_filled(r, 4.0, to_color32(c));
    }
    let ring = if selected { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, t.border) };
    ui.painter().rect_stroke(r, 4.0, ring, StrokeKind::Outside);
    resp
}

/// A colour row: the palette, "none" when allowed, and a picker for any other colour.
fn colour_row(ui: &mut Ui, label: &str, c: &mut Color, allow_none: bool) -> bool {
    let mut changed = false;
    ui.label(egui::RichText::new(label).size(12.0).color(Tokens::get(ui.ctx()).muted));
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        if allow_none && swatch(ui, Color::TRANSPARENT, c.a == 0, 20.0).on_hover_text("None").clicked() {
            *c = Color::TRANSPARENT;
            changed = true;
        }
        for p in PALETTE {
            if swatch(ui, p, c.with_alpha(255) == p && c.a > 0, 20.0).on_hover_text(p.to_hex()).clicked() {
                *c = p.with_alpha(if c.a == 0 { 255 } else { c.a });
                changed = true;
            }
        }
        let mut c32 = to_color32(*c);
        if egui::color_picker::color_edit_button_srgba(ui, &mut c32, egui::color_picker::Alpha::BlendOrAdditive).changed() {
            *c = from_color32(c32);
            changed = true;
        }
    });
    changed
}

pub fn panel(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("properties")
        .exact_size(264.0)
        .resizable(false)
        .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::same(12)))
        .show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| contents(ed, ui));
        });
}

fn contents(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let sel = ed.session.selected();
    let first = sel.first().and_then(|id| ed.session.doc.get(*id)).cloned();
    let (kind, old) = match (&first, ed.tool) {
        (Some(o), _) => (o.shape.kind(), o.style),
        (None, Tool::Draw(k)) => (k, ed.tool_style(k)),
        (None, Tool::Crop) => {
            ui.heading("Crop");
            ui.add_space(6.0);
            ui.label("Drag the frame's edges or corners, or drag out a new frame. Hold Shift for a square.");
            ui.add_space(8.0);
            ui.label("Enter or Apply Crop keeps what is inside; Esc cancels.");
            return;
        }
        (None, Tool::Select) => {
            ui.heading("Select");
            ui.add_space(6.0);
            ui.label("Click an annotation to change it, drag it to move it, or drag across several to select them all.");
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Pick a tool above to draw. Arrows nudge the selection; Delete removes it.").color(t.muted));
            return;
        }
    };
    let title = if sel.len() > 1 { format!("{} annotations", sel.len()) } else { kind.label().to_string() };
    ui.heading(title);
    if first.is_none() {
        ui.label(egui::RichText::new("For the next one you draw").size(12.0).color(t.muted));
    }
    ui.add_space(8.0);
    let mut s = old;

    // The big swatches: the colour the annotation is mostly seen in.
    if !matches!(kind, Kind::Blur | Kind::Pixelate | Kind::Image) {
        ui.label(egui::RichText::new("Colour").size(12.0).color(t.muted));
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
            let main = main_colour(kind, &s);
            for p in PALETTE {
                if swatch(ui, p, main.with_alpha(255) == p, 28.0).on_hover_text(p.to_hex()).clicked() {
                    set_main_colour(kind, &mut s, p);
                }
            }
        });
        ui.add_space(8.0);
    }

    ui.collapsing("Colours", |ui| {
        let (stroke_label, fill_label) = match kind {
            Kind::Text => ("Border", "Background"),
            Kind::Callout => ("Border", "Fill"),
            Kind::Step | Kind::Stamp => ("Outline", "Fill"),
            Kind::Magnify => ("Ring", ""),
            Kind::Spotlight => ("Outline", ""),
            _ => ("Line", "Fill"),
        };
        if !matches!(kind, Kind::Blur | Kind::Pixelate) {
            colour_row(
                ui,
                stroke_label,
                &mut s.stroke,
                matches!(kind, Kind::Text | Kind::Spotlight | Kind::Step | Kind::Stamp | Kind::Image | Kind::Callout | Kind::Magnify),
            );
        }
        if matches!(kind, Kind::Rect | Kind::Ellipse | Kind::Text | Kind::Callout | Kind::Step | Kind::Stamp) {
            colour_row(ui, fill_label, &mut s.fill, true);
        }
        if kind.has_text() {
            colour_row(ui, "Text", &mut s.text_color, false);
        }
    });
    ui.add_space(4.0);

    if !matches!(kind, Kind::Blur | Kind::Pixelate) {
        let label = match kind {
            Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlight => "Thickness",
            _ => "Border width",
        };
        ui.label(egui::RichText::new(label).size(12.0).color(t.muted));
        let max = if kind == Kind::Highlight { 80.0 } else { 40.0 };
        ui.add(egui::Slider::new(&mut s.width, 0.0..=max).step_by(1.0).suffix(" px"));
    }
    match kind {
        Kind::Blur => {
            ui.label(egui::RichText::new("Strength").size(12.0).color(t.muted));
            ui.add(egui::Slider::new(&mut s.strength, 2.0..=60.0).step_by(1.0));
        }
        Kind::Pixelate => {
            ui.label(egui::RichText::new("Square size").size(12.0).color(t.muted));
            ui.add(egui::Slider::new(&mut s.strength, 4.0..=64.0).step_by(1.0).suffix(" px"));
        }
        Kind::Spotlight => {
            ui.label(egui::RichText::new("Darkness").size(12.0).color(t.muted));
            let mut pct = s.strength * 100.0;
            if ui.add(egui::Slider::new(&mut pct, 10.0..=95.0).step_by(1.0).suffix("%")).changed() {
                s.strength = pct / 100.0;
            }
        }
        Kind::Magnify => {
            ui.label(egui::RichText::new("Magnification").size(12.0).color(t.muted));
            ui.add(egui::Slider::new(&mut s.strength, 1.5..=6.0).step_by(0.25).suffix("×"));
        }
        _ => {}
    }

    if kind.has_text() {
        ui.add_space(6.0);
        ui.label(egui::RichText::new("Text size").size(12.0).color(t.muted));
        ui.add(egui::Slider::new(&mut s.font_size, 8.0..=120.0).step_by(1.0).suffix(" px"));
        if kind != Kind::Step {
            ui.horizontal(|ui| {
                ui.toggle_value(&mut s.bold, egui::RichText::new("B").strong());
                ui.toggle_value(&mut s.italic, egui::RichText::new("I").italics());
                ui.toggle_value(&mut s.mono, egui::RichText::new("Mono").monospace());
                ui.separator();
                for (a, label) in [(TextAlign::Left, "Left"), (TextAlign::Center, "Centre"), (TextAlign::Right, "Right")] {
                    if ui.selectable_label(s.align == a, label).clicked() {
                        s.align = a;
                    }
                }
            });
        }
    }

    ui.add_space(6.0);
    ui.label(egui::RichText::new("Opacity").size(12.0).color(t.muted));
    let mut pct = s.opacity * 100.0;
    if ui.add(egui::Slider::new(&mut pct, 10.0..=100.0).step_by(1.0).suffix("%")).changed() {
        s.opacity = pct / 100.0;
    }
    ui.horizontal(|ui| {
        if !matches!(kind, Kind::Blur | Kind::Pixelate | Kind::Spotlight | Kind::Highlight) {
            ui.checkbox(&mut s.shadow, "Shadow");
        }
        if matches!(kind, Kind::Arrow | Kind::Line | Kind::Rect | Kind::Ellipse | Kind::Pen | Kind::Callout) {
            ui.checkbox(&mut s.dashed, "Dashed");
        }
    });

    // What belongs to the shape rather than its look.
    ui.add_space(8.0);
    shape_options(ed, ui, kind, first.as_ref().map(|o| &o.shape));

    if s != old {
        commit_style(ed, kind, &old, &s, first.is_some());
    }
    // A slider dragged is one undo step, ended when the button is let go.
    if ed.props_editing && ed.editing.is_none() && !ui.ctx().input(|i| i.pointer.any_down()) {
        ed.session.commit();
        ed.props_editing = false;
        ed.services.save_settings(&ed.settings);
    }

    if first.is_some() {
        ui.add_space(14.0);
        ui.separator();
        ui.horizontal_wrapped(|ui| {
            let ctx = ui.ctx().clone();
            if ui.button("Front").on_hover_text("Bring to front").clicked() {
                ed.run(&ctx, "object.order", serde_json::json!({"to": "front"}));
            }
            if ui.button("Back").on_hover_text("Send to back").clicked() {
                ed.run(&ctx, "object.order", serde_json::json!({"to": "back"}));
            }
            if ui.button("Duplicate").clicked() {
                ed.run(&ctx, "edit.duplicate", serde_json::json!({}));
            }
            if ui.button("Delete").clicked() {
                ed.run(&ctx, "edit.delete", serde_json::json!({}));
            }
        });
    }
}

pub(crate) fn commit_style(ed: &mut Editor, kind: Kind, old: &Style, new: &Style, on_selection: bool) {
    // The tool keeps the look for the next one, too.
    let mut remembered = ed.tool_style(kind);
    apply_changes(old, new, &mut remembered);
    ed.settings.styles.insert(kind, remembered.clamped());
    if on_selection {
        if !ed.props_editing {
            ed.session.begin("Change Style");
            ed.props_editing = true;
        }
        let picture = ed.session.doc.bounds();
        for id in ed.session.selected() {
            let Some(o) = ed.session.doc.get_mut(id) else { continue };
            let was_inside = picture.encloses(o.shape.bounds(&o.style));
            apply_changes(old, new, &mut o.style);
            o.style = o.style.clamped();
            // A thicker line or a bigger badge doesn't push it over the edge.
            if was_inside {
                ed.session.doc.bring_inside(id);
            }
        }
        ed.session.touch();
    } else if !ed.props_editing {
        ed.props_editing = true;
    }
}

fn shape_options(ed: &mut Editor, ui: &mut Ui, kind: Kind, shape: Option<&Shape>) {
    let t = Tokens::get(ui.ctx());
    match kind {
        Kind::Arrow => {
            let current = match shape {
                Some(Shape::Arrow { heads, .. }) => *heads,
                _ => ed.arrow_heads,
            };
            ui.label(egui::RichText::new("Heads").size(12.0).color(t.muted));
            ui.horizontal(|ui| {
                for (h, label) in [(ArrowHeads::End, "→"), (ArrowHeads::Start, "←"), (ArrowHeads::Both, "↔"), (ArrowHeads::None, "—")] {
                    if ui.selectable_label(current == h, egui::RichText::new(label).size(16.0)).clicked() {
                        ed.arrow_heads = h;
                        set_on_selection(ed, &|s| {
                            if let Shape::Arrow { heads, .. } = s {
                                *heads = h;
                            }
                        });
                    }
                }
            });
        }
        Kind::Rect => {
            let mut r = match shape {
                Some(Shape::Rect { radius, .. }) => *radius,
                _ => ed.rect_radius,
            };
            ui.label(egui::RichText::new("Corner radius").size(12.0).color(t.muted));
            if ui.add(egui::Slider::new(&mut r, 0.0..=60.0).step_by(1.0).suffix(" px")).changed() {
                ed.rect_radius = r;
                set_on_selection(ed, &|s| {
                    if let Shape::Rect { radius, .. } = s {
                        *radius = r;
                    }
                });
            }
        }
        Kind::Spotlight => {
            let mut e = match shape {
                Some(Shape::Spotlight { ellipse, .. }) => *ellipse,
                _ => ed.spot_ellipse,
            };
            if ui.checkbox(&mut e, "Round").changed() {
                ed.spot_ellipse = e;
                set_on_selection(ed, &|s| {
                    if let Shape::Spotlight { ellipse, .. } = s {
                        *ellipse = e;
                    }
                });
            }
        }
        Kind::Stamp => {
            let current = match shape {
                Some(Shape::Stamp { stamp, .. }) => *stamp,
                _ => ed.stamp,
            };
            ui.label(egui::RichText::new("Stamp").size(12.0).color(t.muted));
            ui.horizontal_wrapped(|ui| {
                for k in StampKind::ALL {
                    if ui.selectable_label(current == k, k.label()).clicked() {
                        ed.stamp = k;
                        // Each stamp starts in its own colour.
                        let mut style = ed.tool_style(Kind::Stamp);
                        style.fill = screengrabcraft_render::stamp::default_fill(k);
                        ed.settings.styles.insert(Kind::Stamp, style);
                        set_on_selection(ed, &|s| {
                            if let Shape::Stamp { stamp, .. } = s {
                                *stamp = k;
                            }
                        });
                    }
                }
            });
        }
        Kind::Step => {
            if let Some(Shape::Step { number, .. }) = shape {
                let mut n = *number;
                ui.label(egui::RichText::new("Number").size(12.0).color(t.muted));
                if ui.add(egui::DragValue::new(&mut n).range(0..=9999)).changed() {
                    set_on_selection(ed, &|s| {
                        if let Shape::Step { number, .. } = s {
                            *number = n;
                        }
                    });
                }
            } else {
                ui.label(egui::RichText::new(format!("The next step is {}.", ed.session.doc.next_step_number())).color(t.muted));
            }
        }
        _ => {}
    }
}

fn set_on_selection(ed: &mut Editor, f: &dyn Fn(&mut Shape)) {
    let ids = ed.session.selected();
    if ids.is_empty() {
        return;
    }
    ed.session.begin("Change Shape");
    for id in ids {
        if let Some(o) = ed.session.doc.get_mut(id) {
            f(&mut o.shape);
        }
    }
    ed.session.touch();
    ed.props_editing = true;
}
