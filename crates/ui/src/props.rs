//! The panes at the right, docked: Properties (the Quick Styles for the tool or the selection,
//! the tool properties under them: fill, outline, text colour, the shape and more, then sliders
//! with number boxes) over Effects (`crate::effects`), with handles to resize them.

use egui::{Color32, Sense, Stroke, StrokeKind, Ui, pos2, vec2};
use screengrabcraft_model::color::PALETTE;
use screengrabcraft_model::object::{MAX_SIDES, MIN_SIDES};
use screengrabcraft_model::{
    ArrowHeads, BadgeShape, CalloutForm, Color, FontName, HeadShape, Kind, Object, Shape, ShapeOptions, StampKind, StepLabels, Style, TailSpot,
    TextAlign,
};

use crate::editor::{Editor, Tool};
use crate::icons::{self, Icon};
use crate::quick;
use crate::theme::Tokens;
use crate::{from_color32, to_color32};

/// How wide the panes are, usually.
pub const PANEL_WIDTH: f32 = 272.0;

/// The polygons the Shape tool's picker offers: their names, corners (a star's points) and whether
/// each is a star.
const POLYGONS: [(&str, u32, bool); 6] =
    [("Triangle", 3, false), ("Diamond", 4, false), ("Pentagon", 5, false), ("Hexagon", 6, false), ("Octagon", 8, false), ("Star", 5, true)];
/// How far the arrow picker's curved arrows bow, as a share of their length.
const CURVE: f32 = 0.3;

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
        text_outline_width,
        font
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
        Kind::Arrow | Kind::Line | Kind::Pen => ("Line colour", "Colour"),
        Kind::Highlight => ("Highlight colour", "Colour"),
        Kind::Text | Kind::Callout => ("Border colour", "Border"),
        Kind::Magnify => ("Ring colour", "Ring"),
        _ => ("Outline colour", "Outline"),
    }
}

/// How wide the panes are, as their left edge was dragged (their usual width at first), within
/// what a window `window` points wide leaves room for.
pub fn panes_width(ed: &Editor, window: f32) -> f32 {
    let most = if window.is_finite() { (window * 0.6).clamp(PANEL_MIN, PANEL_MAX) } else { PANEL_MAX };
    ed.settings.side_width.filter(|w| w.is_finite()).unwrap_or(PANEL_WIDTH).clamp(PANEL_MIN, most)
}

/// The panes at the right, docked: Properties (the quick styles and the tool properties of the
/// tool or the selection) over Effects (the picture's effects, each with its settings). Their
/// left edge is dragged to make them wider or narrower, and the bar between them to give one
/// more of the height; a double click on either puts it back.
pub fn panel(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let window = ui.ctx().viewport_rect().width();
    let width = panes_width(ed, window);
    egui::Panel::right("properties").exact_size(width).resizable(false).frame(egui::Frame::NONE.fill(t.panel)).show(ui, |ui| {
        let whole = ui.available_rect_before_wrap();
        // ---- the handle along the left edge: dragged left, the panes grow ----
        let edge = egui::Rect::from_min_max(whole.left_top(), pos2(whole.left() + EDGE, whole.bottom()));
        let grip = ui.interact(edge, ui.id().with("panes-edge"), Sense::click_and_drag());
        grip.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, EDGE_NAME));
        if grip.dragged() {
            let dx = ui.ctx().input(|i| i.pointer.delta().x);
            if dx.is_finite() && dx != 0.0 {
                let most = if window.is_finite() { (window * 0.6).clamp(PANEL_MIN, PANEL_MAX) } else { PANEL_MAX };
                ed.settings.side_width = Some((width - dx).clamp(PANEL_MIN, most));
            }
        }
        if grip.drag_stopped() {
            ed.services.save_settings(&ed.settings);
        }
        if grip.double_clicked() {
            ed.settings.side_width = None;
            ed.services.save_settings(&ed.settings);
        }
        let lit = grip.hovered() || grip.dragged();
        if lit {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeHorizontal);
        }
        handle(ui.painter(), edge, false, lit, &t);
        let _ = grip.on_hover_text("Drag to make the panes wider or narrower (a double click: their usual width)");

        // ---- Properties over Effects, the bar between them ----
        let area = egui::Rect::from_min_max(pos2(edge.right(), whole.top()), whole.right_bottom());
        let room = (area.height() - SPLIT).max(0.0);
        let share = ed.settings.properties_share.filter(|s| s.is_finite()).unwrap_or(PROPERTIES_SHARE);
        let least = PANE_MIN.min(room / 2.0);
        let top_h = (share * room).clamp(least, (room - least).max(least));
        let top = egui::Rect::from_min_size(area.min, vec2(area.width(), top_h));
        let bar = egui::Rect::from_min_size(pos2(area.left(), top.bottom()), vec2(area.width(), SPLIT));
        let bottom = egui::Rect::from_min_max(pos2(area.left(), bar.bottom()), area.max);
        pane(ui, top, Icon::Settings, "Properties", "properties-pane", |ui| contents(ed, ui));
        pane(ui, bottom, Icon::Effects, "Effects", "effects-pane", |ui| crate::effects::pane(ed, ui));
        let split = ui.interact(bar, ui.id().with("panes-split"), Sense::click_and_drag());
        split.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Other, true, SPLIT_NAME));
        if split.dragged() && room > 0.0 {
            let dy = ui.ctx().input(|i| i.pointer.delta().y);
            if dy.is_finite() && dy != 0.0 {
                ed.settings.properties_share = Some(((top_h + dy) / room).clamp(0.0, 1.0));
            }
        }
        if split.drag_stopped() {
            ed.services.save_settings(&ed.settings);
        }
        if split.double_clicked() {
            ed.settings.properties_share = None;
            ed.services.save_settings(&ed.settings);
        }
        let lit = split.hovered() || split.dragged();
        if lit {
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        handle(ui.painter(), bar, true, lit, &t);
        let _ = split.on_hover_text("Drag to give Properties or Effects more room (a double click: as they were)");
    });
}

/// What the two handles are called (their tooltips say how they work).
pub const EDGE_NAME: &str = "Resize the panes";
pub const SPLIT_NAME: &str = "Resize Properties and Effects";
/// How wide the handle along the panes' left edge is, and how tall the bar between them.
const EDGE: f32 = 7.0;
const SPLIT: f32 = 9.0;
/// The panes' narrowest and widest.
const PANEL_MIN: f32 = 248.0;
const PANEL_MAX: f32 = 640.0;
/// Properties' usual share of the height, and the least either pane keeps.
const PROPERTIES_SHARE: f32 = 0.64;
const PANE_MIN: f32 = 96.0;
/// How tall a pane's title is.
const PANE_TITLE: f32 = 28.0;

/// A handle to drag: a line along it and a grip of dots in its middle, lit while it's hovered.
fn handle(p: &egui::Painter, r: egui::Rect, across: bool, lit: bool, t: &Tokens) {
    // A bar of its own, edged on both sides, so it reads as something to take hold of.
    p.rect_filled(r, 0.0, if lit { t.hover } else { t.toolbar });
    let line = Stroke::new(1.0, t.border);
    if across {
        p.hline(r.x_range(), r.top() + 0.5, line);
        p.hline(r.x_range(), r.bottom() - 0.5, line);
    } else {
        p.vline(r.left() + 0.5, r.y_range(), line);
        p.vline(r.right() - 0.5, r.y_range(), line);
    }
    let dot = if lit { t.accent } else { t.muted };
    for i in -2..=2 {
        let o = i as f32 * 5.0;
        let c = if across { pos2(r.center().x + o, r.center().y) } else { pos2(r.center().x, r.center().y + o) };
        p.circle_filled(c, 1.5, dot);
    }
}

/// One of the panes: its title along the top, and what's in it under that, scrolling.
fn pane(ui: &mut Ui, r: egui::Rect, icon: Icon, title: &str, id: &str, add: impl FnOnce(&mut Ui)) {
    let t = Tokens::get(ui.ctx());
    let head = egui::Rect::from_min_size(r.min, vec2(r.width(), PANE_TITLE.min(r.height())));
    ui.painter().rect_filled(head, 0.0, t.toolbar);
    ui.painter().hline(head.x_range(), head.bottom() - 0.5, Stroke::new(1.0, t.border));
    let icon_rect = egui::Rect::from_center_size(pos2(head.left() + 18.0, head.center().y), vec2(15.0, 15.0));
    icons::paint(ui.painter(), icon_rect, icon, t.muted);
    ui.painter().text(
        pos2(icon_rect.right() + 7.0, head.center().y),
        egui::Align2::LEFT_CENTER,
        title,
        crate::widgets::strong_font(ui.ctx(), 13.0),
        t.text,
    );
    let body = egui::Rect::from_min_max(pos2(r.left(), head.bottom()), r.max);
    if body.height() < 1.0 {
        return;
    }
    ui.scope_builder(egui::UiBuilder::new().max_rect(body).id_salt(id), |ui| {
        ui.set_clip_rect(body.intersect(ui.clip_rect()));
        egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).max_height(body.height()).show(ui, |ui| {
            egui::Frame::NONE.inner_margin(egui::Margin::same(12)).show(ui, add);
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
        (None, Tool::Area) => {
            area_panel(ed, ui);
            return;
        }
        (None, _) if ed.area.is_some() => {
            area_panel(ed, ui);
            return;
        }
        (None, Tool::CutOut) => {
            cut_out_panel(ed, ui);
            return;
        }
        (None, Tool::Eraser) => {
            crate::selection::eraser_panel(ed, ui);
            return;
        }
        (None, Tool::Fill) => {
            ui.heading("Fill");
            ui.add_space(6.0);
            quick::fill_section(ed, ui);
            ui.add_space(10.0);
            ui.separator();
            fill_settings(ed, ui);
            return;
        }
        (None, Tool::Select) => {
            ui.heading("Move");
            ui.label(egui::RichText::new("Click an annotation to change it, or pick a tool above to draw.").size(12.0).color(t.muted));
            ui.add_space(10.0);
            crate::widgets::tips(
                ui,
                &[
                    ("Click", "an annotation to select it. Shift adds another, or lets one go."),
                    ("Drag", "it to move it, a handle to resize it, or across several to select them all."),
                    ("Double-click", "its words to change them."),
                    ("Arrows", "nudge what's selected; Delete removes it."),
                    ("Right-click", "the picture for more."),
                ],
            );
            return;
        }
        (None, Tool::Hand) => {
            ui.heading("Hand");
            ui.label(egui::RichText::new("Drag the picture to see another part of it.").size(12.0).color(t.muted));
            ui.add_space(10.0);
            crate::widgets::tips(
                ui,
                &[
                    ("Drag", "the picture to move it round in the window."),
                    ("Ctrl and the wheel", "zoom in or out; Ctrl+0 fits it all in."),
                    ("Space", "held down, with any tool, moves the picture too, as does the middle button."),
                ],
            );
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
        let how = if ed.tool == Tool::Draw(Kind::Polygon) && ed.polygon_points {
            "Click each corner on the picture. A double click, a click on the first corner or Enter closes it; Backspace takes the last back."
        } else {
            how_to(kind)
        };
        ui.label(egui::RichText::new(how).size(12.0).color(t.muted));
    }
    ui.add_space(8.0);
    if kind != Kind::Image {
        quick::section(ed, ui, kind);
        ui.add_space(8.0);
    }
    if kind == Kind::Stamp {
        crate::stamps::library(ed, ui);
        ui.add_space(8.0);
    }
    // A quick style may just have changed the selection.
    let first = first(ed);
    ui.separator();
    ui.add_space(4.0);
    ui.label(egui::RichText::new("Tool properties").strong());
    ui.add_space(4.0);
    properties(ed, ui, kind, first.as_ref());
    // A slider dragged is one undo step, ended when the button is let go.
    if ed.props_editing && ed.editing.is_none() && !ui.ctx().input(|i| i.pointer.any_down()) {
        ed.session.commit();
        ed.props_editing = false;
        ed.services.save_settings(&ed.settings);
    }
    if first.is_some() {
        ui.add_space(14.0);
        ui.separator();
        ui.add_space(6.0);
        let ctx = ui.ctx().clone();
        // What's done with the selection, in two rows of even buttons (kept low: the panel of a
        // callout is already as tall as a small window).
        let wide = ui.available_width();
        let even = |ui: &mut Ui, n: f32, label: &str, tip: &str| {
            let w = ((wide - (n - 1.0) * 6.0) / n).floor();
            ui.add(egui::Button::new(label).min_size(vec2(w, 26.0))).on_hover_text(tip).clicked()
        };
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if even(ui, 2.0, "Bring to Front", "In front of every other annotation") {
                ed.run(&ctx, "object.order", serde_json::json!({"to": "front"}));
            }
            if even(ui, 2.0, "Send to Back", "Behind every other annotation") {
                ed.run(&ctx, "object.order", serde_json::json!({"to": "back"}));
            }
        });
        ui.add_space(2.0);
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 6.0;
            if even(ui, 3.0, "Duplicate", "Another one like it, beside it (Ctrl+D)") {
                ed.run(&ctx, "edit.duplicate", serde_json::json!({}));
            }
            if even(ui, 3.0, "Delete", "Take it off the picture (Del)") {
                ed.run(&ctx, "edit.delete", serde_json::json!({}));
            }
            if even(ui, 3.0, "Flatten", "Draw the selected annotations into the picture for good (Ctrl+Shift+F)") {
                ed.flatten(&ctx, false);
            }
        });
    }
}

/// The panel while an area of the picture is selected (or the Selection tool is picked): what
/// can be done with that part of the picture.
fn area_panel(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.heading("Selection");
    ui.add_space(6.0);
    crate::selection::mode_panel(ed, ui);
    ui.add_space(6.0);
    ui.label("Select an area of the picture, then drag inside it to move that part of the picture.");
    ui.add_space(8.0);
    let ctx = ui.ctx().clone();
    let any = ed.area.is_some();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = egui::vec2(6.0, 6.0);
        if ui.add_enabled(any, egui::Button::new("Copy")).on_hover_text("Copy the area (Ctrl+C)").clicked() {
            ed.copy(&ctx);
        }
        if ui.add_enabled(any, egui::Button::new("Cut")).on_hover_text("Copy the area, and leave it see-through (Ctrl+X)").clicked() {
            ed.cut_area(&ctx);
        }
        if ui.add_enabled(any, egui::Button::new("Delete")).on_hover_text("Leave the area see-through (Delete)").clicked() {
            ed.erase_area(&ctx);
        }
        if ui.add_enabled(any, egui::Button::new("Crop to Selection")).on_hover_text("Keep only the area").clicked() {
            ed.crop_to_area(&ctx);
        }
        if ui.button("Select All").on_hover_text("The whole picture (Ctrl+A)").clicked() {
            ed.select_whole_area();
        }
        if ui.add_enabled(any, egui::Button::new("Invert")).on_hover_text("Everything but the area (Ctrl+Shift+I)").clicked() {
            ed.invert_area();
        }
        if ui
            .add_enabled(any, egui::Button::new("Fill from Round It"))
            .on_hover_text("Take what is in the area out of the picture: the colours round it close over it")
            .clicked()
        {
            ed.fill_area_from_round(&ctx);
        }
    });
    ui.add_space(8.0);
    if ui
        .checkbox(&mut ed.settings.fill_lifted, "Fill where a moved part was")
        .on_hover_text("A part of the picture dragged away leaves its place filled from the colours round it, not see-through")
        .changed()
    {
        ed.services.save_settings(&ed.settings);
    }
    ui.add_space(8.0);
    ui.label(
        egui::RichText::new(
            "Shift keeps it square; Esc lets go of it; right-click for more. A part moved is a piece of its own: Flatten draws it into the picture.",
        )
        .color(t.muted),
    );
}

/// How the tool for `kind` is used, in a line under its name while nothing is selected.
pub(crate) fn how_to(kind: Kind) -> &'static str {
    match kind {
        Kind::Arrow => "Drag on the picture, from where it starts to what it points at. Shift keeps it straight.",
        Kind::Line => "Drag on the picture to draw one. Shift keeps it straight.",
        Kind::Rect | Kind::Ellipse | Kind::Polygon => {
            "Drag on the picture to draw one. Shift makes it a square or a circle. Shape picks triangles, polygons and stars."
        }
        Kind::Text => "Click the picture where the words go, or drag out a box, then type.",
        Kind::Callout => "Click what it's about, or drag out a box, then type. Its tail can be dragged.",
        Kind::Step => "Click the picture: each click puts the next number there. Shape sets letters, Roman numerals or another badge.",
        Kind::Stamp => "Click the picture to put one there, or drag for its size.",
        Kind::Pen => "Draw on the picture with the button held down.",
        Kind::Highlight => "Drag a box over what should stand out. Its handles resize it after.",
        Kind::Blur | Kind::Pixelate => "Drag a box over what shouldn't be read.",
        Kind::Spotlight => "Drag a box round what matters: the rest is dimmed.",
        Kind::Magnify => "Click what to enlarge, or drag a box round it.",
        _ => "For the next one you draw",
    }
}

/// A slider with its name above it (the slider has its number box).
fn slider(ui: &mut Ui, name: &str, value: &mut f32, range: std::ops::RangeInclusive<f32>, step: f64, suffix: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new(name).size(12.0).color(t.muted));
    ui.scope(|ui| {
        // As wide as the panel leaves beside the number box.
        ui.spacing_mut().slider_width = (ui.available_width() - 76.0).max(80.0);
        // As many decimals as its steps have: "22 px", not "22.0 px".
        let decimals = if step >= 1.0 {
            0
        } else if (step * 10.0).fract() == 0.0 {
            1
        } else {
            2
        };
        ui.add(egui::Slider::new(value, range).step_by(step).fixed_decimals(decimals).suffix(suffix)).changed()
    })
    .inner
}

/// A button with a letter on it that is on or off (bold, italic, the typewriter's letters), drawn
/// as the icon buttons beside it are. `name` is what it is called.
fn letter_button(ui: &mut Ui, on: &mut bool, face: egui::RichText, name: &str) {
    let t = Tokens::get(ui.ctx());
    let is_on = *on;
    let colour = if is_on { t.accent } else { t.text };
    let galley = egui::WidgetText::from(face.color(colour)).into_galley(ui, Some(egui::TextWrapMode::Extend), f32::INFINITY, egui::TextStyle::Button);
    let (rect, resp) = ui.allocate_exact_size(vec2((galley.size().x + 14.0).max(STYLE_BUTTON), STYLE_BUTTON), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::selected(egui::WidgetType::Button, true, is_on, name));
    if is_on {
        ui.painter().rect_filled(rect, 5.0, t.selected);
        ui.painter().rect_stroke(rect, 5.0, egui::Stroke::new(1.0, t.accent), egui::StrokeKind::Inside);
    } else if resp.hovered() {
        ui.painter().rect_filled(rect, 5.0, t.hover);
    }
    ui.painter().galley(rect.center() - galley.size() / 2.0, galley, colour);
    if resp.on_hover_text(name).clicked() {
        *on = !is_on;
    }
}

/// The side of the buttons for how words look (bold, italic, which side they're against).
const STYLE_BUTTON: f32 = 28.0;

/// The tool properties of a kind: for the selected annotations when there are some, else for
/// the next one the tool draws.
fn properties(ed: &mut Editor, ui: &mut Ui, kind: Kind, first: Option<&Object>) {
    let t = Tokens::get(ui.ctx());
    let old = match first {
        Some(o) => o.style,
        None => ed.tool_style(kind),
    };
    let mut s = old;
    let fill = matches!(kind, Kind::Rect | Kind::Ellipse | Kind::Polygon | Kind::Callout | Kind::Text | Kind::Step | Kind::Stamp);
    let outline = !matches!(kind, Kind::Blur | Kind::Pixelate);
    let picker = matches!(
        kind,
        Kind::Arrow | Kind::Line | Kind::Rect | Kind::Ellipse | Kind::Polygon | Kind::Callout | Kind::Stamp | Kind::Spotlight | Kind::Step
    );
    let shadow = !matches!(kind, Kind::Blur | Kind::Pixelate | Kind::Spotlight | Kind::Highlight);
    let dashed = matches!(kind, Kind::Arrow | Kind::Line | Kind::Rect | Kind::Ellipse | Kind::Polygon | Kind::Pen | Kind::Callout);
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
            let none = !matches!(kind, Kind::Arrow | Kind::Line | Kind::Pen | Kind::Highlight | Kind::Rect | Kind::Ellipse | Kind::Polygon);
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
        if letters {
            font_menu(ed, ui, &mut s.font);
            ui.add_space(2.0);
        }
        if kind != Kind::Step {
            ui.add_space(2.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                letter_button(ui, &mut s.bold, egui::RichText::new("B").size(14.0).strong(), "Bold");
                letter_button(ui, &mut s.italic, egui::RichText::new("I").size(14.0).italics(), "Italic");
                letter_button(ui, &mut s.mono, egui::RichText::new("Mono").size(12.5).monospace(), "Typewriter letters, each as wide as the next");
                ui.add_space(3.0);
                ui.separator();
                ui.add_space(3.0);
                for (a, icon, name) in [
                    (TextAlign::Left, Icon::AlignLeft, "Against the left"),
                    (TextAlign::Center, Icon::AlignCentre, "In the middle"),
                    (TextAlign::Right, Icon::AlignRight, "Against the right"),
                ] {
                    if icons::button_sized(ui, icon, name, s.align == a, true, STYLE_BUTTON).clicked() {
                        s.align = a;
                    }
                }
            });
            ui.add_space(2.0);
        }
        if letters && s.text_outline.a > 0 {
            slider(ui, "Text outline", &mut s.text_outline_width, 0.0..=16.0, 0.5, " px");
        }
        ui.add_space(4.0);
    }
    // A highlight is a box of colour, with no line to be thick.
    if outline && kind != Kind::Highlight {
        let name = match kind {
            Kind::Arrow | Kind::Line | Kind::Pen => "Thickness",
            Kind::Text | Kind::Callout => "Border width",
            Kind::Magnify => "Ring width",
            _ => "Outline width",
        };
        slider(ui, name, &mut s.width, 0.0..=40.0, 1.0, " px");
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
        if let Some(Shape::Step { number, labels, .. }) = first.map(|o| &o.shape) {
            let (mut n, labels) = (*number, *labels);
            ui.label(egui::RichText::new("Number").size(12.0).color(t.muted));
            ui.horizontal(|ui| {
                if ui.add(egui::DragValue::new(&mut n).range(0..=9999)).changed() {
                    set_on_selection(ed, &|s| {
                        if let Shape::Step { number, .. } = s {
                            *number = n;
                        }
                    });
                }
                // Its place in the count, as the badge writes it.
                if labels != StepLabels::Numbers {
                    ui.label(egui::RichText::new(format!("shows {}", labels.text(n))).color(t.muted));
                }
            });
        } else {
            let labels = ed.tool_options(Kind::Step).labels.unwrap_or_default();
            let next = labels.text(ed.session.doc.next_step_number(labels));
            ui.label(egui::RichText::new(format!("The next step is {next}.")).color(t.muted));
        }
    }

    if s != old {
        commit_style(ed, kind, &old, &s, first.is_some());
    }
}

/// What the font menu, its search and the bundled font in it are called.
pub const FONT_MENU: &str = "Choose a font";
pub const FONT_SEARCH: &str = "Search fonts";
pub const BUNDLED_FONT: &str = "Built-in font";
/// The most fonts the menu lists at once (the search finds the rest).
const MAX_LISTED: usize = 400;

/// The font menu of text and callouts: the built-in font, then the fonts on the computer, with a
/// search. Greyed out, with the reason, where there is no way to them (the browser).
fn font_menu(ed: &mut Editor, ui: &mut Ui, font: &mut FontName) {
    let t = Tokens::get(ui.ctx());
    ui.label(egui::RichText::new("Font").size(12.0).color(t.muted));
    let shown = if font.is_bundled() { BUNDLED_FONT.to_string() } else { font.as_str().to_string() };
    if !screengrabcraft_render::text::has_font_source() {
        ui.add_enabled_ui(false, |ui| {
            egui::ComboBox::from_id_salt("font-menu").width(ui.available_width()).selected_text(shown).show_ui(ui, |_| {});
        })
        .response
        .on_disabled_hover_text("The browser can't use the computer's fonts: text is in the built-in font");
        return;
    }
    let resp = egui::ComboBox::from_id_salt("font-menu")
        .width(ui.available_width())
        .height(320.0)
        .selected_text(shown)
        .close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside)
        .show_ui(ui, |ui| {
            let search = ui.add(egui::TextEdit::singleline(&mut ed.font_query).hint_text("Search").desired_width(f32::INFINITY));
            search.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::TextEdit, true, FONT_SEARCH));
            if ui.selectable_label(font.is_bundled(), BUNDLED_FONT).on_hover_text("PatroneOS Sans, or PatroneOS Mono for Mono").clicked() {
                *font = FontName::BUNDLED;
            }
            let list = ed.font_list.get_or_insert_with(|| screengrabcraft_render::text::font_families().unwrap_or_default()).clone();
            let query = ed.font_query.trim().to_lowercase();
            let found: Vec<&String> = list.iter().filter(|n| query.is_empty() || n.to_lowercase().contains(&query)).take(MAX_LISTED).collect();
            for name in &found {
                if ui.selectable_label(font.as_str() == name.as_str(), name.as_str()).clicked() {
                    *font = FontName::new(name);
                }
            }
            if found.is_empty() {
                ui.label(egui::RichText::new("No font on this computer goes by that name.").size(12.0).color(t.muted));
            }
        });
    resp.response.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::ComboBox, true, FONT_MENU));
    let _ =
        resp.response.on_hover_text("A font on this computer for the text (projects keep its name; a computer without it shows the built-in font)");
}

/// The Cut Out tool's panel: what it takes out (columns or rows) and how the parts meet.
fn cut_out_panel(ed: &mut Editor, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    ui.heading("Cut Out");
    ui.add_space(6.0);
    ui.label("Drag across the picture over what to take out. The strip goes all the way across, and the rest is joined.");
    ui.add_space(10.0);
    let mut c = ed.settings.cut_out;
    ui.label(egui::RichText::new("Take out").size(12.0).color(t.muted));
    ui.horizontal(|ui| {
        if ui.selectable_label(!c.rows, "Columns").on_hover_text("A strip from top to bottom: the picture gets narrower").clicked() {
            c.rows = false;
        }
        if ui.selectable_label(c.rows, "Rows").on_hover_text("A strip from side to side: the picture gets shorter").clicked() {
            c.rows = true;
        }
    });
    ui.add_space(8.0);
    ui.label(egui::RichText::new("Where the parts meet").size(12.0).color(t.muted));
    ui.horizontal(|ui| {
        for e in screengrabcraft_model::settings::CutEdge::ALL {
            if ui.selectable_label(c.edge == e, e.label()).clicked() {
                c.edge = e;
            }
        }
    });
    ui.add_space(4.0);
    ui.label(egui::RichText::new("A saw or a wave leaves a thin see-through gap, so it shows something was taken out.").size(12.0).color(t.muted));
    if c != ed.settings.cut_out {
        ed.settings.cut_out = c;
        ed.services.save_settings(&ed.settings);
    }
    ui.add_space(12.0);
    crate::widgets::tips(ui, &[("Drag", "across the part to take out."), ("Undo", "puts it back (Ctrl+Z).")]);
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
    let kinds: &[Kind] = if kind.family() == Kind::Rect { &[Kind::Rect, Kind::Ellipse, Kind::Polygon] } else { std::slice::from_ref(&kind) };
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
    // A polygon of clicked points picked (Some(true)), or another shape (Some(false)).
    let mut by_points: Option<bool> = None;
    let (mut add_tail, mut remove_tail) = (false, false);
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
                ui.add_space(6.0);
                ui.label(small("Head shape"));
                ui.horizontal_wrapped(|ui| {
                    for s in HeadShape::ALL {
                        if ui.selectable_label(opts.head.unwrap_or_default() == s, s.label()).clicked() {
                            chosen = Some(ShapeOptions { head: Some(s), ..none });
                        }
                    }
                });
                ui.add_space(6.0);
                ui.label(small("Line"));
                ui.horizontal_wrapped(|ui| {
                    let bend = opts.bend.unwrap_or(0.0);
                    for (b, label, name) in [
                        (0.0, "Straight", "A straight arrow"),
                        (CURVE, "Curved", "Bowed to the left of the way it points"),
                        (-CURVE, "Curved the other way", "Bowed to the right of the way it points"),
                    ] {
                        let on = if b == 0.0 { bend == 0.0 } else { bend * b > 0.0 };
                        if ui.selectable_label(on, label).on_hover_text(name).clicked() {
                            chosen = Some(ShapeOptions { bend: Some(b), ..none });
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(small("Drag the diamond in a selected arrow's middle to bend it as you like."));
            }
            Kind::Line => {
                ui.label(small("Line"));
                ui.horizontal_wrapped(|ui| {
                    let bend = opts.bend.unwrap_or(0.0);
                    for (b, label, name) in [
                        (0.0, "Straight", "A straight line"),
                        (CURVE, "Curved", "Bowed to the left of the way it was drawn"),
                        (-CURVE, "Curved the other way", "Bowed to the right of the way it was drawn"),
                    ] {
                        let on = if b == 0.0 { bend == 0.0 } else { bend * b > 0.0 };
                        if ui.selectable_label(on, label).on_hover_text(name).clicked() {
                            chosen = Some(ShapeOptions { bend: Some(b), ..none });
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(small("Drag the diamond in a selected line's middle to bend it as you like."));
            }
            Kind::Rect | Kind::Ellipse | Kind::Polygon => {
                let radius = opts.radius.unwrap_or(0.0);
                for (label, round, rounded) in [("Rectangle", false, false), ("Rounded rectangle", false, true), ("Ellipse", true, false)] {
                    let on = match shown_kind {
                        Kind::Ellipse => round,
                        Kind::Rect => !round && (radius > 0.0) == rounded,
                        _ => false,
                    };
                    let on = on && !ed.polygon_points;
                    if ui.selectable_label(on, label).clicked() {
                        by_points = Some(false);
                        let r = if rounded { if radius > 0.0 { radius } else { 14.0 } } else { 0.0 };
                        chosen = Some(ShapeOptions { round: Some(round), radius: (!round).then_some(r), ..none });
                        new_tool = Some(Tool::Draw(if round { Kind::Ellipse } else { Kind::Rect }));
                    }
                }
                ui.add_space(4.0);
                ui.horizontal_wrapped(|ui| {
                    for (label, sides, star) in POLYGONS {
                        let on = shown_kind == Kind::Polygon && opts.sides == Some(sides) && opts.star.unwrap_or(false) == star && !ed.polygon_points;
                        if ui.selectable_label(on, label).clicked() {
                            by_points = Some(false);
                            chosen = Some(ShapeOptions { sides: Some(sides), star: Some(star), ..none });
                            new_tool = Some(Tool::Draw(Kind::Polygon));
                        }
                    }
                });
                let free = shape.is_some_and(|s| matches!(s, Shape::Polygon { corners, .. } if !corners.is_empty()));
                if ui
                    .selectable_label(free || (shape.is_none() && ed.polygon_points), "Points clicked")
                    .on_hover_text("A polygon of points clicked one by one: a double click or Enter closes it")
                    .clicked()
                {
                    by_points = Some(true);
                }
                if shown_kind == Kind::Polygon && !free && !ed.polygon_points {
                    let star = opts.star.unwrap_or(false);
                    ui.add_space(6.0);
                    ui.label(small(if star { "Points" } else { "Corners" }));
                    ui.horizontal_wrapped(|ui| {
                        for n in MIN_SIDES..=MAX_SIDES {
                            let name = if star { format!("{n} points") } else { format!("{n} corners") };
                            if ui.selectable_label(opts.sides == Some(n), n.to_string()).on_hover_text(name).clicked() {
                                chosen = Some(ShapeOptions { sides: Some(n), star: Some(star), ..none });
                            }
                        }
                    });
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
                if let Some(Shape::Callout { tails, .. }) = shape {
                    ui.add_space(6.0);
                    ui.label(small("Tails"));
                    ui.horizontal(|ui| {
                        let room = tails.len() + 1 < screengrabcraft_model::object::MAX_TAILS;
                        if ui
                            .add_enabled(room, egui::Button::new("Add a Tail"))
                            .on_hover_text("Another tail, to point at something else too")
                            .clicked()
                        {
                            add_tail = true;
                        }
                        if ui
                            .add_enabled(!tails.is_empty(), egui::Button::new("Remove a Tail"))
                            .on_hover_text("Takes away the tail added last")
                            .clicked()
                        {
                            remove_tail = true;
                        }
                    });
                }
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
            Kind::Step => {
                ui.label(small("Count in"));
                ui.horizontal_wrapped(|ui| {
                    for l in StepLabels::ALL {
                        if ui.selectable_label(opts.labels == Some(l), l.label()).on_hover_text(l.name()).clicked() {
                            chosen = Some(ShapeOptions { labels: Some(l), ..none });
                        }
                    }
                });
                ui.add_space(6.0);
                ui.label(small("Badge"));
                ui.horizontal_wrapped(|ui| {
                    for b in BadgeShape::ALL {
                        if ui.selectable_label(opts.badge == Some(b), b.label()).clicked() {
                            chosen = Some(ShapeOptions { badge: Some(b), ..none });
                        }
                    }
                });
                ui.add_space(4.0);
                ui.label(small("Each way of counting counts on its own: 1, 2, 3 can have a, b, c beside them."));
            }
            _ => {}
        }
    });
    if add_tail || remove_tail {
        ed.run(&ctx, if add_tail { "object.addTail" } else { "object.removeTail" }, serde_json::json!({}));
    }
    match by_points {
        Some(true) => {
            ed.polygon_points = true;
            ed.polygon_draft.clear();
            if ed.session.selected().is_empty() {
                ed.set_tool(Tool::Draw(Kind::Polygon));
                ed.polygon_points = true;
            }
        }
        Some(false) => ed.polygon_points = false,
        None => {}
    }
    if let Some(o) = chosen {
        // A rectangle or an ellipse picked: the Shape tool draws no polygon until one is picked.
        if o.round.is_some() && kind.family() == Kind::Rect {
            ed.settings.options.entry(Kind::Rect).or_default().sides = None;
        }
        if let Some(k) = o.stamp {
            ed.own_stamp = None;
            choose_stamp(ed, &ctx, k);
            return;
        }
        set_options(ed, &ctx, kind, o);
        if let Some(tool) = new_tool
            && ed.session.selected().is_empty()
        {
            ed.set_tool(tool);
        }
    }
}

/// Picks a stamp for the Stamp tool, in its own colour, and puts it (and its colour) on the
/// selected stamps, one undo step.
pub(crate) fn choose_stamp(ed: &mut Editor, ctx: &egui::Context, k: StampKind) {
    let mut style = ed.tool_style(Kind::Stamp);
    style.fill = k.color();
    ed.settings.styles.insert(Kind::Stamp, style);
    ed.set_tool_options(Kind::Stamp, ShapeOptions { stamp: Some(k), ..ShapeOptions::default() });
    let ids: Vec<u64> = ed
        .session
        .selected()
        .into_iter()
        .filter(|id| ed.session.doc.get(*id).is_some_and(|o| o.shape.kind() == Kind::Stamp))
        .map(|id| id.0)
        .collect();
    if !ids.is_empty() {
        let fill = serde_json::to_value(k.color()).unwrap_or(serde_json::Value::Null);
        ed.run(ctx, "object.look", serde_json::json!({ "ids": ids, "options": { "stamp": k }, "style": { "fill": fill } }));
    }
    ed.services.save_settings(&ed.settings);
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
