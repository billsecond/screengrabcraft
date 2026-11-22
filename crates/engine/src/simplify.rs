//! Simplify (`image.simplify`): the parts of a picture of a screen drawn over it as plain shapes
//! ([`screengrabcraft_render::simplify`]): a box of the background's colour under them all, flat
//! boxes for areas of one colour, small boxes for icons, and grey bars for lines of text (from the
//! lines the host's reader read where it gave them). Every shape is an annotation, in a group of
//! their own under the others: Ungroup to change one, Delete to have the picture back.

use screengrabcraft_model::{Color, Kind, Order, Pt, Rect, Shape, Style};
use screengrabcraft_render::simplify::{self, PartKind};
use serde_json::{Value, json};

use crate::details::lines_param;
use crate::registry::{boolean, int};
use crate::session::Session;
use crate::{EngineError, Result};

fn flat(fill: Color) -> Style {
    Style { stroke: Color::TRANSPARENT, fill, width: 0.0, shadow: false, ..Kind::Rect.default_style() }
}

pub(crate) fn simplify(s: &mut Session, p: &Value) -> Result<Value> {
    let tolerance = int(p, "tolerance", 12, 0, 64)? as u8;
    let picture = s.doc.bounds();
    let area = match p.get("area") {
        None | Some(Value::Null) => picture,
        Some(v) => serde_json::from_value::<Rect>(v.clone())
            .ok()
            .map(Rect::finite)
            .and_then(|a| a.intersect(picture))
            .ok_or_else(|| EngineError::Params("`area` is {\"x\", \"y\", \"w\", \"h\"} on the picture".to_string()))?,
    };
    let (x, y, w, h) =
        area.pixel_bounds(s.doc.width(), s.doc.height()).ok_or_else(|| EngineError::Params("the area is outside the picture".to_string()))?;
    let piece = s.doc.base.crop(x, y, w, h).ok_or_else(|| EngineError::Params("the area is outside the picture".to_string()))?;
    let at = Pt::new(x as f32, y as f32);
    // The lines of text: given, or read with the reader lent (none when there is none).
    let given = p.get("lines").is_some_and(|v| !v.is_null());
    let lines = if given {
        lines_param(s, p, None)?
    } else if boolean(p, "read", true) {
        lines_param(s, p, Some(Rect::new(at.x, at.y, w as f32, h as f32))).unwrap_or_default()
    } else {
        Vec::new()
    };
    let boxes: Vec<Rect> = lines.iter().filter(|l| !l.text.trim().is_empty()).map(|l| l.rect.translate(Pt::new(-at.x, -at.y))).collect();
    let found = simplify::simplify(&piece, &boxes, tolerance)?;
    let mut ids = Vec::new();
    let (mut blocks, mut bars, mut marks) = (0, 0, 0);
    if boolean(p, "background", true) {
        let rect = Rect::new(at.x, at.y, w as f32, h as f32);
        ids.push(s.doc.add(Shape::Rect { rect, radius: 0.0 }, flat(found.background))?);
    }
    for part in &found.parts {
        let rect = part.rect.translate(at);
        let radius = match part.kind {
            PartKind::Block => 0.0,
            PartKind::Mark => 2.0_f32.min(rect.w.min(rect.h) / 2.0),
            PartKind::Text => rect.h / 2.0,
        };
        match part.kind {
            PartKind::Block => blocks += 1,
            PartKind::Mark => marks += 1,
            PartKind::Text => bars += 1,
        }
        ids.push(s.doc.add(Shape::Rect { rect, radius }, flat(part.colour))?);
    }
    // Under the annotations there were, as one group.
    s.doc.reorder(&ids, Order::Back);
    if boolean(p, "group", true) && ids.len() >= 2 {
        let g = s.doc.next_group();
        for id in &ids {
            if let Some(o) = s.doc.get_mut(*id) {
                o.group = Some(g);
            }
        }
    }
    s.selection.clear();
    Ok(json!({ "ids": ids, "blocks": blocks, "bars": bars, "marks": marks, "lines": lines.len() }))
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::{Document, Raster, Shape};

    use super::*;

    /// A made-up window: a light background, a dark title bar, a blue button, a line of "text".
    fn session() -> Session {
        let mut r = Raster::filled(300, 200, Color::rgb(240, 240, 240)).unwrap();
        let mut fill = |x0: u32, y0: u32, w: u32, h: u32, c: Color| {
            let width = r.width() as usize;
            for y in y0..y0 + h {
                for x in x0..x0 + w {
                    let i = (y as usize * width + x as usize) * 4;
                    r.pixels_mut()[i..i + 4].copy_from_slice(&[c.r, c.g, c.b, 255]);
                }
            }
        };
        fill(0, 0, 300, 30, Color::rgb(40, 44, 52));
        fill(180, 150, 100, 30, Color::rgb(30, 120, 230));
        for k in 0..20u32 {
            fill(20 + k * 4 + (k / 5) * 4, 80, 3, 12, Color::rgb(20, 20, 20));
        }
        Session::new(Document::new(r))
    }

    #[test]
    fn the_parts_are_drawn_as_plain_shapes_in_a_group_under_the_rest() {
        let mut s = session();
        let arrow = s.execute("object.add", &json!({"object": {"type": "arrow", "from": {"x": 5, "y": 5}, "to": {"x": 90, "y": 90}}})).unwrap();
        let out = s.execute("image.simplify", &json!({})).unwrap();
        assert!(out["blocks"].as_u64().unwrap() >= 2, "{out}");
        assert_eq!(out["bars"], 1, "{out}");
        let ids = out["ids"].as_array().unwrap();
        assert_eq!(ids.len(), s.doc.objects.len() - 1);
        // The background first, the arrow still on top, every new shape in one group.
        let first = s.doc.objects.first().unwrap();
        assert_eq!(first.shape, Shape::Rect { rect: Rect::new(0.0, 0.0, 300.0, 200.0), radius: 0.0 });
        assert_eq!(first.style.fill, Color::rgb(240, 240, 240));
        assert_eq!(s.doc.objects.last().unwrap().id.0, arrow["id"].as_u64().unwrap());
        let group = first.group;
        assert!(group.is_some() && s.doc.objects.iter().filter(|o| o.group == group).count() == ids.len());
        // The button: a flat box in its colour.
        assert!(s.doc.objects.iter().any(|o| o.style.fill == Color::rgb(30, 120, 230) && matches!(o.shape, Shape::Rect { radius: 0.0, .. })));
        // The bar: rounded, grey.
        let bar = s.doc.objects.iter().find(|o| matches!(o.shape, Shape::Rect { radius, .. } if radius > 1.0)).unwrap();
        assert_eq!(bar.style.fill, screengrabcraft_render::simplify::bar_grey(Color::rgb(240, 240, 240)));
        // One undo puts the picture back.
        s.undo();
        assert_eq!(s.doc.objects.len(), 1);
    }

    #[test]
    fn lines_given_are_the_text_and_an_area_keeps_to_itself() {
        let mut s = session();
        let lines = json!([{"text": "Hello", "rect": {"x": 18, "y": 78, "w": 100, "h": 16}}]);
        let out = s.execute("image.simplify", &json!({ "lines": lines, "group": false, "background": false })).unwrap();
        assert_eq!(out["bars"], 1);
        assert!(s.doc.objects.iter().all(|o| o.group.is_none()));
        let near = |r: Rect| (r.x - 18.0).abs() < 0.01 && (r.y - 81.2).abs() < 0.01 && (r.w - 100.0).abs() < 0.01 && (r.h - 9.6).abs() < 0.01;
        assert!(s.doc.objects.iter().any(|o| o.shape.rect().is_some_and(near)), "{:?}", s.doc.objects);
        let mut s = session();
        s.execute("image.simplify", &json!({"area": {"x": 150, "y": 120, "w": 150, "h": 80}})).unwrap();
        assert!(s.doc.objects.iter().all(|o| o.shape.rect().is_some_and(|r| r.x >= 150.0 && r.y >= 120.0)));
        assert!(s.execute("image.simplify", &json!({"area": {"x": 900, "y": 0, "w": 5, "h": 5}})).is_err());
        assert!(s.execute("image.simplify", &json!({"lines": "x"})).is_err());
    }

    #[test]
    fn a_part_moved_or_deleted_can_leave_its_place_filled_from_round_it() {
        let mut s = session();
        let out = s.execute("image.lift", &json!({"x": 170, "y": 140, "w": 120, "h": 50, "fill": true})).unwrap();
        assert!(out["id"].as_u64().is_some());
        // Where the button was is the background again, not see-through.
        assert_eq!(s.doc.base.pixel(230, 165), Some(Color::rgb(240, 240, 240)));
        // The piece is the button.
        let Shape::Image { image, .. } = s.doc.objects.first().unwrap().shape else { panic!() };
        assert_eq!(s.doc.images.get(&image).unwrap().pixel(60, 25), Some(Color::rgb(30, 120, 230)));
        // Delete with fill takes the line of text out the same way.
        let mut s = session();
        s.execute("image.erase", &json!({"x": 15, "y": 75, "w": 120, "h": 22, "fill": true})).unwrap();
        assert_eq!(s.doc.base.pixel(40, 85), Some(Color::rgb(240, 240, 240)));
    }
}
