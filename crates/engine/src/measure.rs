//! Measuring: the distance between two points of the picture, kept on it as a dimension line (a
//! line with a bar at each end) and its length in pixels, grouped. Its entry is in
//! [`crate::registry::COMMANDS`].

use screengrabcraft_model::{ArrowHeads, Color, HeadShape, Pt, Rect, Shape, Style, TextAlign};
use serde_json::{Value, json};

use crate::registry::{color, num};
use crate::session::Session;
use crate::{EngineError, Result};

/// How a measured length is written: whole pixels, or to a tenth when under ten.
pub fn length_text(from: Pt, to: Pt) -> String {
    let d = from.distance(to);
    if !d.is_finite() {
        return "0 px".to_string();
    }
    if d < 10.0 && d.fract() > 0.05 { format!("{d:.1} px") } else { format!("{:.0} px", d.round()) }
}

/// The angle of the line from `from` to `to`, in degrees, as a protractor reads it: 0 to the
/// right, 90 up.
pub fn angle(from: Pt, to: Pt) -> f32 {
    let a = (from.y - to.y).atan2(to.x - from.x).to_degrees();
    if a.is_finite() { a } else { 0.0 }
}

fn point(p: &Value, key: &str) -> Result<Pt> {
    let v = p.get(key).ok_or_else(|| EngineError::Params(format!("`{key}` is a point: {{\"x\", \"y\"}}")))?;
    match (num(v, "x"), num(v, "y")) {
        (Some(x), Some(y)) => Ok(Pt::new(x, y)),
        _ => Err(EngineError::Params(format!("`{key}` is a point: {{\"x\", \"y\"}}"))),
    }
}

pub(crate) fn measure(s: &mut Session, p: &Value) -> Result<Value> {
    let (from, to) = (point(p, "from")?, point(p, "to")?);
    let picture = s.doc.bounds();
    let reach = picture.inflate(screengrabcraft_model::document::MAX_GROW);
    if !reach.contains(from) || !reach.contains(to) {
        return Err(EngineError::Params("measure between points on the picture".to_string()));
    }
    if from.distance(to) < 1.0 {
        return Err(EngineError::Params("the two points are the same".to_string()));
    }
    let ink = color(p, "color", Color::rgb(0xE5, 0x39, 0x35))?;
    let width = num(p, "width").unwrap_or(2.0).clamp(1.0, 20.0);
    let size = num(p, "fontSize").unwrap_or(16.0).clamp(8.0, 120.0);
    let line = Style { stroke: ink, width, shadow: false, ..Style::default() };
    let line_id = s.doc.add(Shape::Arrow { from, to, heads: ArrowHeads::Both, bend: 0.0, head: HeadShape::Bar }, line)?;
    // The length on a white label beside the middle of the line, on the side away from it.
    let text = length_text(from, to);
    let label = Style {
        stroke: Color::TRANSPARENT,
        fill: Color::rgba(255, 255, 255, 230),
        width: 0.0,
        text_color: ink,
        font_size: size,
        bold: true,
        align: TextAlign::Center,
        shadow: false,
        ..Style::default()
    };
    let lay = screengrabcraft_render::text::layout(&text, &label, None);
    let pad = screengrabcraft_render::text::padding(&label);
    let (w, h) = (lay.width + 2.0 * pad + 2.0, lay.height + 2.0 * pad);
    let mid = from.lerp(to, 0.5);
    let d = to.sub(from);
    let len = d.length().max(1.0);
    // The side of the line above it (or to its left when it goes straight up and down).
    let mut normal = Pt::new(d.y / len, -d.x / len);
    if normal.y > 0.0 || (normal.y == 0.0 && normal.x > 0.0) {
        normal = normal.scale(-1.0);
    }
    let away = (w * normal.x.abs() + h * normal.y.abs()) / 2.0 + width + 4.0;
    let place = |n: Pt| {
        let c = mid.add(n.scale(away));
        Rect::new(c.x - w / 2.0, c.y - h / 2.0, w, h)
    };
    // On the other side when that one would be off the picture and this one isn't.
    let (one, other) = (place(normal), place(normal.scale(-1.0)));
    let rect = if !picture.encloses(one) && picture.encloses(other) { other } else { one };
    let label_id = s.doc.add(Shape::Text { rect, text: text.clone() }, label)?;
    let g = s.doc.next_group();
    for id in [line_id, label_id] {
        if let Some(o) = s.doc.get_mut(id) {
            o.group = Some(g);
        }
    }
    if let Some(area) = s.doc.bounds_of(&[line_id, label_id]) {
        s.doc.grow_to(area);
    }
    s.selection = vec![line_id, label_id];
    Ok(json!({ "ids": [line_id, label_id], "length": from.distance(to), "text": text }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lengths_and_angles_read_plainly() {
        assert_eq!(length_text(Pt::new(0.0, 0.0), Pt::new(3.0, 4.0)), "5 px");
        assert_eq!(length_text(Pt::new(0.0, 0.0), Pt::new(2.5, 0.0)), "2.5 px");
        assert_eq!(length_text(Pt::new(0.0, 0.0), Pt::new(120.4, 0.0)), "120 px");
        assert_eq!(length_text(Pt::new(f32::NAN, 0.0), Pt::new(1.0, 0.0)), "0 px");
        assert_eq!(angle(Pt::new(0.0, 0.0), Pt::new(10.0, 0.0)), 0.0);
        assert_eq!(angle(Pt::new(0.0, 0.0), Pt::new(0.0, -10.0)), 90.0, "up the screen is up");
        assert_eq!(angle(Pt::new(0.0, 0.0), Pt::new(-10.0, 0.0)), 180.0);
    }
}
