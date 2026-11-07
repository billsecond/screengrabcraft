//! The properties panel, at the right: the Quick Styles for the tool or the selection, the tool
//! properties under them (fill, outline, text colour, the shape and more, then sliders with
//! number boxes), and the Effects and Properties buttons along its bottom.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use screengrabcraft_model::color::PALETTE;
use screengrabcraft_model::{ArrowHeads, CalloutForm, Color, Kind, Object, Shape, ShapeOptions, StampKind, Style, TailSpot, TextAlign};

use crate::editor::{Editor, Tool};
use crate::icons::{self, Icon};
use crate::quick;
use crate::theme::Tokens;
use crate::{from_color32, to_color32};

/// How wide the panel is.
pub const PANEL_WIDTH: f32 = 272.0;

/// Copies onto `target` the fields that differ between `old` and `new`.
fn apply_changes(old: &Style, new: &Style, target: &mut Style) {
    macro_rules! field {
        ($($f:ident),*) => { $( if old.$f != new.$f { target.$f = new.$f; } )* };
    }
    field!(
        stroke,
        fill,
        width,
        opacity,
        shadow,
        dashed,
        font_size,
        bold,
        italic,
        mono,
        text_color,
        align,
        strength,
        text_outline,
        text_outline_width
    );
}

fn swatch(ui: &mut Ui, c: Color, selected: bool, size: f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let name = if c.a == 0 { "None".to_string() } else { c.to_hex() };
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, &name));
    paint_colour(ui.painter(), r, c);
    let ring = if selected { Stroke::new(2.0, t.accent) } else { Stroke::new(1.0, t.border) };
    ui.painter().rect_stroke(r, 4.0, ring, StrokeKind::Outside);
    resp
}

/// A colour in a box: white with a red stroke through it for none.
fn paint_colour(p: &egui::Painter, r: egui::Rect, c: Color) {
    if c.a == 0 {
        p.rect_filled(r, 4.0, Color32::WHITE);
        p.line_segment([r.left_bottom(), r.right_top()], Stroke::new(1.5, Color32::from_rgb(0xE5, 0x39, 0x35)));
    } else {
        p.rect_filled(r, 4.0, to_color32(c));
    }
}

/// The colours a colour button (called `name`) offers: the palette, none when allowed, the
/// eyedropper, and any other colour in the picker. `keep_alpha` keeps a see-through colour
/// see-through (the highlighter's).
fn colour_choices(ui: &mut Ui, name: &str, c: &mut Color, allow_none: bool, keep_alpha: bool) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        let tip = "Eyedropper: pick a colour from the picture";
        let resp = crate::icons::button_sized(ui, Icon::Eyedropper, tip, false, true, 26.0);
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, tip));
        if resp.clicked() {
            crate::eyedropper::start(ui.ctx(), name);
            ui.close();
        }
        ui.label(egui::RichText::new("Pick from the picture").size(12.0));
    });
    ui.add_space(4.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 5.0);
        if allow_none && swatch(ui, Color::TRANSPARENT, c.a == 0, 24.0).on_hover_text("None").clicked() {
            *c = Color::TRANSPARENT;
            changed = true;
            ui.close();
        }
        for p in PALETTE {
            if swatch(ui, p, c.with_alpha(255) == p && c.a > 0, 24.0).on_hover_text(p.to_hex()).clicked() {
                let a = if keep_alpha {
                    c.a.max(60)
                } else if c.a == 0 {
                    255
                } else {
                    c.a
                };
                *c = p.with_alpha(a);
                changed = true;
                ui.close();
            }
        }
    });
    ui.add_space(6.0);
    let mut c32 = to_color32(*c);
    if egui::color_picker::color_picker_color32(ui, &mut c32, egui::color_picker::Alpha::BlendOrAdditive) {
        *c = from_color32(c32);
        changed = true;
    }
    changed
}

/// A button of the tool properties: a face in a box with a small arrow (it opens a menu), and
/// a short caption under it. `name` is what it is called (its tooltip).
fn prop_button(ui: &mut Ui, name: &str, caption: &str, face: &dyn Fn(&egui::Painter, egui::Rect)) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    // One block, the caption under the button, so a row of them wraps like any other widgets.
    let (block, resp) = ui.allocate_exact_size(vec2(44.0, 34.0 + 16.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    let rect = egui::Rect::from_min_size(block.min, vec2(44.0, 34.0));
    let p = ui.painter();
    p.rect_filled(rect, 5.0, if resp.hovered() { t.hover } else { t.control });
    p.rect_stroke(rect, 5.0, Stroke::new(1.0, t.control_border), StrokeKind::Inside);
    face(p, egui::Rect::from_min_max(rect.min + vec2(6.0, 6.0), rect.max - vec2(15.0, 6.0)));
    let (x, y) = (rect.right() - 9.0, rect.center().y);
    p.add(egui::Shape::convex_polygon(vec![pos2(x - 3.0, y - 2.0), pos2(x + 3.0, y - 2.0), pos2(x, y + 2.0)], t.muted, Stroke::NONE));
    p.text(pos2(rect.left(), rect.bottom() + 2.0), egui::Align2::LEFT_TOP, caption, egui::FontId::proportional(11.0), t.muted);
    resp.on_hover_text(name)
}

/// A colour button and its menu.
fn colour_button(ui: &mut Ui, name: &str, caption: &str, c: &mut Color, allow_none: bool, keep_alpha: bool) -> bool {
    let mut changed = false;
    // A colour taken from the picture with this button's eyedropper.
    if let Some(p) = crate::eyedropper::take(ui.ctx(), name) {
        *c = if keep_alpha { p.with_alpha(c.a.max(60)) } else { p };
        changed = true;
    }
    let shown = *c;
    let picking = crate::eyedropper::for_button(ui.ctx()).is_some_and(|b| b == name);
    let resp = prop_button(ui, name, caption, &|p, r| {
        paint_colour(p, r, shown);
        p.rect_stroke(r, 4.0, Stroke::new(1.0, Color32::from_black_alpha(50)), StrokeKind::Inside);
    });
    if picking {
        // The eyedropper is out for this one: it says so.
        let t = Tokens::get(ui.ctx());
        ui.painter().rect_stroke(resp.rect.shrink2(vec2(0.0, 8.0)).translate(vec2(0.0, -8.0)), 5.0, Stroke::new(2.0, t.accent), StrokeKind::Outside);
    }
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_width(204.0);
        ui.label(egui::RichText::new(name).strong());
        ui.add_space(4.0);
        changed |= colour_choices(ui, name, c, allow_none, keep_alpha);
    });
    changed
}

/// What the colour of lines and borders is called for a kind.
fn outline_name(kind: Kind) -> (&'static str, &'static str) {
    match kind {
        Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlight => ("Line colour", "Colour"),
        Kind::Text | Kind::Callout => ("Border colour", "Border"),
        Kind::Magnify => ("Ring colour", "Ring"),
        _ => ("Outline colour", "Outline"),
    }
}

pub fn panel(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    egui::Panel::right("properties").exact_size(PANEL_WIDTH).resizable(false).frame(egui::Frame::NONE.fill(t.panel)).show(ui, |ui| {
        egui::Panel::bottom("properties_footer")
            .frame(egui::Frame::NONE.fill(t.panel).inner_margin(egui::Margin::symmetric(10, 6)))
            .show_separator_line(true)
            .show(ui, |ui| footer(ed, ui));
        egui::CentralPanel::default().frame(egui::Frame::NONE.inner_margin(egui::Margin::same(12))).show(ui, |ui| {
            egui::ScrollArea::vertical().auto_shrink([false, false]).show(ui, |ui| contents(ed, ui));
        });
    });
}

/// A button of the panel's footer: an icon and a word. `name` is what it is called.
fn footer_button(ui: &mut Ui, icon: Icon, label: &str, name: &str, on: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(13.0), t.text);
    let (rect, resp) = ui.allocate_exact_size(vec2(18.0 + 8.0 + galley.size().x + 20.0, 30.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, name));
    let bg = if on {
        t.selected
    } else if resp.hovered() {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 5.0, bg);
    let icon_rect = egui::Rect::from_center_size(pos2(rect.left() + 8.0 + 9.0, rect.center().y), vec2(18.0, 18.0));
    icons::paint(ui.painter(), icon_rect, icon, if on { t.accent } else { t.text });
    ui.painter().galley(pos2(icon_rect.right() + 8.0, rect.center().y - galley.size().y / 2.0), galley, t.text);
    resp
}

/// Effects (the picture's effects, in a menu) and Properties (shows or folds away the tool
/// properties).
fn footer(ed: &mut Editor, ui: &mut Ui) {
    ui.horizontal(|ui| {
        let effects = footer_button(ui, Icon::Effects, "Effects", "Picture effects", false)
            .on_hover_text("Effects for the whole picture: border, shadow, torn edge…");
        egui::Popup::menu(&effects).show(|ui| {
            ui.set_min_width(200.0);
            ed.effects_menu(ui);
        });
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let shown = !ed.settings.hide_properties;
            let tip = if shown { "Fold the tool properties away" } else { "Show the tool properties" };
            if footer_button(ui, Icon::Settings, "Properties", "Properties", shown).on_hover_text(tip).clicked() {
                ed.settings.hide_properties = shown;
                ed.services.save_settings(&ed.settings);
            }
        });
    });
}

fn contents(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let first = |ed: &Editor| ed.session.selected().first().and_then(|id| ed.session.doc.get(*id)).cloned();
    // Favorites: the starred looks, then what is selected.
    if ed.tool == Tool::Favorites {
        quick::favorites(ed, ui);
        if first(ed).is_none() {
            return;
        }
        ui.add_space(10.0);
        ui.separator();
        ui.add_space(6.0);
    }
    let kind = match (first(ed), ed.tool) {
        (Some(o), _) => o.shape.kind(),
        (None, Tool::Draw(k)) => k,
        (None, Tool::Crop) => {
            ui.heading("Crop");
            ui.add_space(6.0);
            ui.label("Drag the frame's edges or corners, or drag out a new frame. Hold Shift for a square.");
            ui.add_space(8.0);
            ui.label("Enter or Apply Crop keeps what is inside; Esc cancels.");
            ui.add_space(12.0);
            // Here rather than on the tool bar, which has no room for them beside its bigger buttons.
            let ctx = ui.ctx().clone();
            ui.horizontal(|ui| {
                let tall = egui::vec2(0.0, 32.0);
                let label = egui::RichText::new("Apply Crop").color(t.accent_text);
                if ui.add(egui::Button::new(label).fill(t.accent).min_size(tall)).clicked() {
                    ed.apply_crop(&ctx);
                }
                if ui.add(egui::Button::new("Cancel").min_size(tall)).clicked() {
                    ed.crop = None;
                    ed.tool = Tool::Select;
                }
            });
            return;
        }
        (None, Tool::Fill) => {
            ui.heading("Fill");
            ui.add_space(6.0);
            quick::fill_section(ed, ui);
            if !ed.settings.hide_properties {
                ui.add_space(10.0);
                ui.separator();
                fill_settings(ed, ui);
            }
            return;
        }
        (None, Tool::Select) => {
            ui.heading("Move");
            ui.add_space(6.0);
            ui.label("Click an annotation to change it, drag it to move it, or drag across several to select them all.");
            ui.add_space(8.0);
            ui.label(egui::RichText::new("Pick a tool above to draw. Arrows nudge the selection; Delete removes it.").color(t.muted));
            return;
        }
        (None, Tool::Favorites) => return,
    };
    let sel = ed.session.selected();
    let title = if sel.len() > 1 {
        format!("{} annotations", sel.len())
    } else if sel.is_empty() {
        quick::tool_name(kind).to_string()
    } else {
        kind.label().to_string()
    };
    ui.heading(title);
    if sel.is_empty() {
        ui.label(egui::RichText::new("For the next one you draw").size(12.0).color(t.muted));
    }
    ui.add_space(8.0);
    if kind != Kind::Image {
        quick::section(ed, ui, kind);
        ui.add_space(8.0);
    }
    // A quick style may just have changed the selection.
    let first = first(ed);
    if !ed.settings.hide_properties {
        ui.separator();
        ui.add_space(4.0);
        ui.label(egui::RichText::new("Tool properties").strong());
        ui.add_space(4.0);
        properties(ed, ui, kind, first.as_ref());
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

/// A slider with its name above it (the slider has its number box).
fn slider(ui: &mut Ui, name: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, step: f64, suffix: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(name).size(12.0).color(t.muted));
    ui.scope(|ui| {
        // As wide as the panel leaves beside the number box.
        ui.spacing_mut().slider_width = (ui.available_width() - 76.0).max(80.0);
        ui.add(egui::Slider::new(value, range).step_by(step).suffix(suffix)).changed()
    })
    .inner
}

/// The tool properties of a kind: for the selected annotations when there are some, else for
/// the next one the tool draws.
fn properties(ed: &mut Editor, ui: &mut Ui, kind: Kind, first: Option<&Object>) {
    let t = Tokens::get(ui.ctx());
    let old = match first {
        Some(o) => o.style,
        None => ed.tool_style(kind),
    };
    let mut s = old;
    let fill = matches!(kind, Kind::Rect | Kind::Ellipse | Kind::Callout | Kind::Text | Kind::Step | Kind::Stamp);
    let outline = !matches!(kind, Kind::Blur | Kind::Pixelate);
    let picker = matches!(kind, Kind::Arrow | Kind::Rect | Kind::Ellipse | Kind::Callout | Kind::Stamp | Kind::Spotlight);
    let shadow = !matches!(kind, Kind::Blur | Kind::Pixelate | Kind::Spotlight | Kind::Highlight);
    let dashed = matches!(kind, Kind::Arrow | Kind::Line | Kind::Rect | Kind::Ellipse | Kind::Pen | Kind::Callout);
    // Text and callouts can have an outline round their letters too.
    let letters = matches!(kind, Kind::Text | Kind::Callout);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(5.0, 6.0);
        if fill {
            let (name, caption) = if kind == Kind::Text { ("Background colour", "Fill") } else { ("Fill colour", "Fill") };
            colour_button(ui, name, caption, &mut s.fill, true, false);
        }
        if outline {
            let (name, caption) = outline_name(kind);
            let none = !matches!(kind, Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlight | Kind::Rect | Kind::Ellipse);
            colour_button(ui, name, caption, &mut s.stroke, none, kind == Kind::Highlight);
        }
        if kind.has_text() {
            colour_button(ui, "Text colour", "Text", &mut s.text_color, false, false);
        }
        if letters {
            colour_button(ui, "Text outline colour", "Outline", &mut s.text_outline, true, false);
        }
        if picker {
            shape_button(ed, ui, kind, first.map(|o| &o.shape));
        }
        if shadow || dashed {
            let resp = prop_button(ui, "More options", "More", &|p, r| {
                // A small grid of dots: more to choose.
                for iy in 0..3 {
                    for ix in 0..3 {
                        let c = pos2(r.left() + r.width() * (0.2 + 0.3 * ix as f32), r.top() + r.height() * (0.2 + 0.3 * iy as f32));
                        p.circle_filled(c, 1.8, t.text);
                    }
                }
            });
            egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
                ui.set_min_width(160.0);
                if shadow {
                    ui.checkbox(&mut s.shadow, "Shadow");
                }
                if dashed {
                    ui.checkbox(&mut s.dashed, "Dashed");
                }
            });
        }
    });
    ui.add_space(8.0);

    if kind.has_text() {
        slider(ui, "Text size", &mut s.font_size, 8.0..=120.0, 1.0, " px");
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
        if letters && s.text_outline.a > 0 {
            slider(ui, "Text outline", &mut s.text_outline_width, 0.0..=16.0, 0.5, " px");
        }
        ui.add_space(4.0);
    }
    if outline {
        let name = match kind {
            Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlight => "Thickness",
            Kind::Text | Kind::Callout => "Border width",
            Kind::Magnify => "Ring width",
            _ => "Outline width",
        };
        let max = if kind == Kind::Highlight { 80.0 } else { 40.0 };
        slider(ui, name, &mut s.width, 0.0..=max, 1.0, " px");
    }
    match kind {
        Kind::Blur => {
            slider(ui, "Strength", &mut s.strength, 2.0..=60.0, 1.0, "");
        }
        Kind::Pixelate => {
            slider(ui, "Square size", &mut s.strength, 4.0..=64.0, 1.0, " px");
        }
        Kind::Spotlight => {
            let mut pct = s.strength * 100.0;
            if slider(ui, "Darkness", &mut pct, 10.0..=95.0, 1.0, "%") {
                s.strength = pct / 100.0;
            }
        }
        Kind::Magnify => {
            slider(ui, "Magnification", &mut s.strength, 1.5..=6.0, 0.25, "×");
        }
        Kind::Rect => {
            let mut r = match first.map(|o| &o.shape) {
                Some(Shape::Rect { radius, .. }) => *radius,
                _ => ed.tool_options(Kind::Rect).radius.unwrap_or(0.0),
            };
            if slider(ui, "Corner radius", &mut r, 0.0..=60.0, 1.0, " px") {
                ed.set_tool_options(Kind::Rect, ShapeOptions { radius: Some(r), ..ShapeOptions::default() });
                ed.props_editing = true;
                set_on_selection(ed, &|s| {
                    if let Shape::Rect { radius, .. } = s {
                        *radius = r;
                    }
                });
            }
        }
        _ => {}
    }
    let mut pct = s.opacity * 100.0;
    if slider(ui, "Opacity", &mut pct, 10.0..=100.0, 1.0, "%") {
        s.opacity = pct / 100.0;
    }

    if kind == Kind::Step {
        ui.add_space(6.0);
        if let Some(Shape::Step { number, .. }) = first.map(|o| &o.shape) {
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

    if s != old {
        commit_style(ed, kind, &old, &s, first.is_some());
    }
}

/// The Fill tool's own settings, under its colours: any other colour, and the tolerance.
fn fill_settings(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Tool properties").strong());
    ui.add_space(4.0);
    ui.label("Click the picture to fill the area round that spot that is all one colour.");
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Click a box, ellipse, callout or text box to fill it instead.").color(t.muted));
    ui.add_space(8.0);
    let mut f = ed.settings.fill;
    ui.horizontal(|ui| {
        colour_button(ui, "Fill colour", "Colour", &mut f.color, true, false);
    });
    ui.label(egui::RichText::new("None makes the area see-through.").size(12.0).color(t.muted));
    ui.add_space(8.0);
    let mut pct = (f32::from(f.tolerance) / 2.55).round();
    ui.label(egui::RichText::new("Tolerance").size(12.0).color(t.muted));
    if ui
        .add(egui::Slider::new(&mut pct, 0.0..=100.0).step_by(1.0).suffix("%"))
        .on_hover_text("How close a colour must be to the one clicked to be filled too")
        .changed()
    {
        f.tolerance = (pct * 2.55).round().clamp(0.0, 255.0) as u8;
    }
    if f != ed.settings.fill {
        ed.settings.fill = f;
        ed.props_editing = true;
    }
    // Saved once a slider is let go.
    if ed.props_editing && !ui.ctx().input(|i| i.pointer.any_down()) {
        ed.props_editing = false;
        ed.services.save_settings(&ed.settings);
    }
}

pub(crate) fn commit_style(ed: &mut Editor, kind: Kind, old: &Style, new: &Style, on_selection: bool) {
    // The tool keeps the look for the next one, too (the Shape tool's two kinds alike).
    let kinds: &[Kind] = if kind.family() == Kind::Rect { &[Kind::Rect, Kind::Ellipse] } else { std::slice::from_ref(&kind) };
    for k in kinds {
        let mut remembered = ed.tool_style(*k);
        apply_changes(old, new, &mut remembered);
        ed.settings.styles.insert(*k, remembered.clamped());
    }
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

/// Sets shape options on the tool, and on the selection in one undo step.
fn set_options(ed: &mut Editor, ctx: &egui::Context, kind: Kind, o: ShapeOptions) {
    ed.set_tool_options(kind, o);
    if !ed.session.selected().is_empty() {
        let options = serde_json::to_value(o).unwrap_or(serde_json::Value::Null);
        ed.run(ctx, "object.look", serde_json::json!({ "options": options }));
    }
    ed.services.save_settings(&ed.settings);
}

/// The shape picker: the button shows the shape in use, and its menu the others (a callout's
/// form and tail, the Shape tool's rectangle, rounded rectangle or ellipse, an arrow's heads, the
/// stamp, a round or square spotlight).
fn shape_button(ed: &mut Editor, ui: &mut Ui, kind: Kind, shape: Option<&Shape>) {
    let ctx = ui.ctx().clone();
    let t = Tokens::get(&ctx);
    let look = quick::current_look(ed, kind);
    let shown_kind = match shape {
        Some(s) => s.kind(),
        None => kind,
    };
    let tex = quick::preview(ed, &ctx, &look, shown_kind);
    let resp = prop_button(ui, "Shape options", "Shape", &|p, r| {
        if let Some(tex) = tex {
            let (w, h) = quick::PREVIEW;
            let aspect = w as f32 / h as f32;
            let fit = egui::Rect::from_center_size(r.center(), vec2(r.height() * aspect, r.height()));
            p.image(tex, fit, egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
    });
    let opts = options_of(shape, ed.tool_options(kind));
    let mut chosen: Option<ShapeOptions> = None;
    let mut new_tool: Option<Tool> = None;
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.set_min_width(220.0);
        let none = ShapeOptions::default();
        let small = |s: &str| egui::RichText::new(s.to_string()).size(12.0).color(t.muted);
        match kind {
            Kind::Arrow => {
                ui.label(small("Heads"));
                ui.horizontal(|ui| {
                    for (h, label, name) in [
                        (ArrowHeads::End, "→", "Head at the end"),
                        (ArrowHeads::Start, "←", "Head at the start"),
                        (ArrowHeads::Both, "↔", "Heads at both ends"),
                        (ArrowHeads::None, "—", "No heads"),
                    ] {
                        if ui.selectable_label(opts.heads == Some(h), egui::RichText::new(label).size(18.0)).on_hover_text(name).clicked() {
                            chosen = Some(ShapeOptions { heads: Some(h), ..none });
                        }
                    }
                });
            }
            Kind::Rect | Kind::Ellipse => {
                let ellipse = shown_kind == Kind::Ellipse;
                let radius = opts.radius.unwrap_or(0.0);
                for (label, round, rounded) in [("Rectangle", false, false), ("Rounded rectangle", false, true), ("Ellipse", true, false)] {
                    let on = ellipse == round && (round || (radius > 0.0) == rounded);
                    if ui.selectable_label(on, label).clicked() {
                        let r = if rounded { if radius > 0.0 { radius } else { 14.0 } } else { 0.0 };
                        chosen = Some(ShapeOptions { round: Some(round), radius: (!round).then_some(r), ..none });
                        new_tool = Some(Tool::Draw(if round { Kind::Ellipse } else { Kind::Rect }));
                    }
                }
            }
            Kind::Callout => {
                ui.label(small("Form"));
                ui.horizontal_wrapped(|ui| {
                    for f in CalloutForm::ALL {
                        if ui.selectable_label(opts.form == Some(f), f.label()).clicked() {
                            chosen = Some(ShapeOptions { form: Some(f), ..none });
                        }
                    }
                });
                ui.add_space(6.0);
                ui.label(small("Tail"));
                ui.horizontal_wrapped(|ui| {
                    for spot in TailSpot::ALL {
                        if direction_button(ui, spot, opts.tail == Some(spot)).clicked() {
                            chosen = Some(ShapeOptions { tail: Some(spot), ..none });
                        }
                    }
                });
            }
            Kind::Stamp => {
                ui.label(small("Stamp"));
                ui.horizontal_wrapped(|ui| {
                    for k in StampKind::ALL {
                        if ui.selectable_label(opts.stamp == Some(k), k.label()).clicked() {
                            chosen = Some(ShapeOptions { stamp: Some(k), ..none });
                        }
                    }
                });
            }
            Kind::Spotlight => {
                ui.horizontal(|ui| {
                    for (label, round) in [("Square", false), ("Round", true)] {
                        if ui.selectable_label(opts.round == Some(round), label).clicked() {
                            chosen = Some(ShapeOptions { round: Some(round), ..none });
                        }
                    }
                });
            }
            _ => {}
        }
    });
    if let Some(o) = chosen {
        // Each stamp starts in its own colour.
        if let Some(k) = o.stamp {
            let mut style = ed.tool_style(Kind::Stamp);
            style.fill = k.color();
            ed.settings.styles.insert(Kind::Stamp, style);
        }
        set_options(ed, &ctx, kind, o);
        if let Some(tool) = new_tool
            && ed.session.selected().is_empty()
        {
            ed.set_tool(tool);
        }
    }
}

/// A button with an arrow pointing where a callout's tail goes.
fn direction_button(ui: &mut Ui, spot: TailSpot, on: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, format!("Tail {}", spot.label().to_lowercase())));
    let bg = if on {
        t.selected
    } else if resp.hovered() {
        t.hover
    } else {
        Color32::TRANSPARENT
    };
    ui.painter().rect_filled(rect, 4.0, bg);
    let (dx, dy): (f32, f32) = match spot {
        TailSpot::DownLeft => (-1.0, 1.0),
        TailSpot::DownRight => (1.0, 1.0),
        TailSpot::UpLeft => (-1.0, -1.0),
        TailSpot::UpRight => (1.0, -1.0),
        TailSpot::Left => (-1.0, 0.0),
        TailSpot::Right => (1.0, 0.0),
        TailSpot::Up => (0.0, -1.0),
        TailSpot::Down => (0.0, 1.0),
    };
    let len = (dx * dx + dy * dy).sqrt().max(0.001);
    let (ux, uy) = (dx / len, dy / len);
    let c = rect.center();
    let ink = if on { t.accent } else { t.text };
    let tip = pos2(c.x + ux * 8.0, c.y + uy * 8.0);
    ui.painter().line_segment([pos2(c.x - ux * 7.0, c.y - uy * 7.0), tip], Stroke::new(1.8, ink));
    let (nx, ny) = (-uy, ux);
    ui.painter().add(egui::Shape::convex_polygon(
        vec![tip, pos2(tip.x - ux * 5.0 + nx * 3.5, tip.y - uy * 5.0 + ny * 3.5), pos2(tip.x - ux * 5.0 - nx * 3.5, tip.y - uy * 5.0 - ny * 3.5)],
        ink,
        Stroke::NONE,
    ));
    resp.on_hover_text(format!("Tail {}", spot.label().to_lowercase()))
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

/// What the shape picker shows as chosen: the selected shape's own options, or the tool's.
fn options_of(shape: Option<&Shape>, tool: ShapeOptions) -> ShapeOptions {
    match shape {
        Some(s) => ShapeOptions { tail: None, ..ShapeOptions::of(s) },
        None => tool,
    }
}
