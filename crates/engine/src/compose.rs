//! Several pictures made into one: combined side by side, stacked or in a grid
//! (`image.combine`), or laid out in a template with a title and the words of each step
//! (`image.template`). The picture open is the first; the others are pictures the host lent
//! ([`Session::lent`]). Their entries are in [`crate::registry::COMMANDS`].

use screengrabcraft_model::{Color, Document, Kind, Pt, Raster, Rect, Shape, Style, TextAlign};
use screengrabcraft_render::effects;
use serde_json::{Value, json};

use crate::registry::{color, int, num};
use crate::session::Session;
use crate::{EngineError, Result};

/// How pictures are put together.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum Arrangement {
    Across,
    Down,
    Grid,
}

/// Where each of `sizes` goes (its top left) and the whole size: in a row, a column or a grid of
/// `columns`, `gap` apart and from the edges, each centred in its place.
fn arrange(sizes: &[(f32, f32)], how: Arrangement, columns: usize, gap: f32) -> (Vec<Pt>, f32, f32) {
    let n = sizes.len().max(1);
    let cols = match how {
        Arrangement::Across => n,
        Arrangement::Down => 1,
        Arrangement::Grid => columns.clamp(1, n),
    };
    let rows = n.div_ceil(cols);
    // Each column as wide as its widest, each row as tall as its tallest.
    let mut widths = vec![0.0f32; cols];
    let mut heights = vec![0.0f32; rows];
    for (i, (w, h)) in sizes.iter().enumerate() {
        if let Some(cw) = widths.get_mut(i % cols) {
            *cw = cw.max(*w);
        }
        if let Some(rh) = heights.get_mut(i / cols) {
            *rh = rh.max(*h);
        }
    }
    let start = |list: &[f32], k: usize| gap + list.iter().take(k).map(|v| v + gap).sum::<f32>();
    let places = sizes
        .iter()
        .enumerate()
        .map(|(i, (w, h))| {
            let (c, r) = (i % cols, i / cols);
            let (cw, rh) = (widths.get(c).copied().unwrap_or(0.0), heights.get(r).copied().unwrap_or(0.0));
            Pt::new(start(&widths, c) + (cw - w) / 2.0, start(&heights, r) + (rh - h) / 2.0)
        })
        .collect();
    (places, start(&widths, cols), start(&heights, rows))
}

/// The picture open and the pictures lent.
fn pictures(s: &Session) -> Result<Vec<Raster>> {
    if s.lent.is_empty() {
        return Err(EngineError::Params("give the pictures to put with this one (the host lends them)".to_string()));
    }
    let mut all = vec![s.doc.base.clone()];
    all.extend(s.lent.iter().take(crate::fx::MAX_LENT).cloned());
    Ok(all)
}

fn arrangement(p: &Value) -> Result<Arrangement> {
    match p.get("layout") {
        None | Some(Value::Null) => Ok(Arrangement::Across),
        Some(v) => serde_json::from_value(v.clone()).map_err(|_| EngineError::Params("`layout` is \"across\", \"down\" or \"grid\"".to_string())),
    }
}

/// Combine: the picture open and the ones lent, side by side, stacked or in a grid, on one
/// picture; the annotations stay on the picture they were on.
pub(crate) fn combine(s: &mut Session, p: &Value) -> Result<Value> {
    let pics = pictures(s)?;
    let how = arrangement(p)?;
    let gap = int(p, "gap", 16, 0, 400)? as f32;
    let columns = int(p, "columns", (pics.len() as f64).sqrt().ceil() as i64, 1, 64)? as usize;
    let bg = color(p, "background", Color::WHITE)?;
    let sizes: Vec<(f32, f32)> = pics.iter().map(|r| (r.width() as f32, r.height() as f32)).collect();
    let (places, w, h) = arrange(&sizes, how, columns, gap);
    let (w, h) = (w.ceil() as u32, h.ceil() as u32);
    let mut out = Raster::filled(w, h, bg)?;
    for (pic, at) in pics.iter().zip(&places) {
        effects::paste(&mut out, pic, at.x.round() as i64, at.y.round() as i64);
    }
    let first = places.first().copied().unwrap_or(Pt::ZERO);
    s.doc.base = out;
    let d = Pt::new(first.x.round(), first.y.round());
    s.doc.map_objects(&|q| q.add(d), 1.0);
    Ok(json!({ "width": w, "height": h }))
}

/// The templates' layouts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
enum Layout {
    /// The steps in a row, each with its number and words under it.
    StepsAcross,
    /// The steps one under another, each with its number and words beside it.
    StepsDown,
    /// Two pictures side by side, "Before" and "After" over them, an arrow between.
    BeforeAfter,
    /// Two to a row, each with its number and words under it.
    Grid,
}

/// The words of each step, as given.
fn steps(p: &Value) -> Result<Vec<String>> {
    match p.get("steps") {
        None | Some(Value::Null) => Ok(Vec::new()),
        Some(Value::Array(a)) => Ok(a.iter().take(64).map(|v| v.as_str().unwrap_or("").chars().take(400).collect()).collect()),
        Some(_) => Err(EngineError::Params("`steps` is a list of words, one for each picture".to_string())),
    }
}

/// `r` scaled to fit `w`×`h` (never larger than it is).
fn fit(r: &Raster, w: f32, h: f32) -> Result<Raster> {
    let f = (w / r.width() as f32).min(h / r.height() as f32).min(1.0);
    let (nw, nh) = ((r.width() as f32 * f).round().max(1.0) as u32, (r.height() as f32 * f).round().max(1.0) as u32);
    Ok(effects::resize(r, nw, nh)?)
}

/// A template: the pictures laid out on a frame with a title and the words of each step, all of
/// it annotations to change, move or take away. The document becomes it (Undo puts it back).
pub(crate) fn template(s: &mut Session, p: &Value) -> Result<Value> {
    let layout: Layout = match p.get("layout") {
        None | Some(Value::Null) => Layout::StepsAcross,
        Some(v) => serde_json::from_value(v.clone())
            .map_err(|_| EngineError::Params("`layout` is \"stepsAcross\", \"stepsDown\", \"beforeAfter\" or \"grid\"".to_string()))?,
    };
    // The picture open as it is shown, annotations and all.
    let shown = screengrabcraft_render::render(&s.doc).ok_or_else(|| EngineError::Failed("the picture could not be drawn".to_string()))?;
    let mut pics = vec![shown];
    pics.extend(s.lent.iter().take(crate::fx::MAX_LENT).cloned());
    if layout == Layout::BeforeAfter && pics.len() < 2 {
        return Err(EngineError::Params("before and after needs two pictures".to_string()));
    }
    let title: String = p.get("title").and_then(Value::as_str).unwrap_or("").chars().take(200).collect();
    let mut words = steps(p)?;
    if layout == Layout::BeforeAfter && words.is_empty() {
        words = vec!["Before".to_string(), "After".to_string()];
    }
    let bg = color(p, "background", Color::rgb(0xF4, 0xF6, 0xFA))?;
    let accent = color(p, "accent", Color::rgb(0x1E, 0x88, 0xE5))?;
    let ink = if bg.luminance() > 0.5 { Color::rgb(0x1F, 0x23, 0x2B) } else { Color::WHITE };
    let size = num(p, "size").unwrap_or(480.0).clamp(120.0, 2000.0);
    let pics = match layout {
        Layout::BeforeAfter => pics.iter().take(2).map(|r| fit(r, size * 1.3, size)).collect::<Result<Vec<_>>>()?,
        Layout::StepsDown => pics.iter().map(|r| fit(r, size * 1.5, size)).collect::<Result<Vec<_>>>()?,
        _ => pics.iter().map(|r| fit(r, size, size * 0.75)).collect::<Result<Vec<_>>>()?,
    };
    let (margin, gap) = (48.0f32, 36.0f32);
    let title_h = if title.trim().is_empty() { 0.0 } else { 84.0 };
    let caption_h = if words.iter().all(|w| w.trim().is_empty()) && layout == Layout::BeforeAfter { 0.0 } else { 72.0 };
    let side = 300.0f32;
    // Where each picture goes, and the frame's size.
    let sizes: Vec<(f32, f32)> = pics.iter().map(|r| (r.width() as f32, r.height() as f32)).collect();
    let (places, w, h) = match layout {
        Layout::StepsAcross | Layout::BeforeAfter => {
            let space = if layout == Layout::BeforeAfter { gap + 60.0 } else { gap };
            let (pl, w, h) = arrange(&sizes, Arrangement::Across, 1, space);
            (pl, w, h)
        }
        Layout::StepsDown => {
            let (pl, w, h) = arrange(&sizes, Arrangement::Down, 1, gap);
            (pl.into_iter().map(|q| Pt::new(q.x + side, q.y)).collect(), w + side, h)
        }
        Layout::Grid => {
            let sized: Vec<(f32, f32)> = sizes.iter().map(|(w, h)| (*w, *h + caption_h)).collect();
            arrange(&sized, Arrangement::Grid, 2, gap)
        }
    };
    let extra_h = match layout {
        Layout::StepsAcross | Layout::BeforeAfter => caption_h,
        _ => 0.0,
    };
    let (fw, fh) = ((w + 2.0 * (margin - gap).max(0.0)).ceil(), (h + title_h + extra_h + 2.0 * (margin - gap).max(0.0)).ceil());
    let origin = Pt::new((margin - gap).max(0.0), title_h + (margin - gap).max(0.0));
    let mut doc = Document::new(Raster::filled(fw as u32, fh as u32, bg)?);
    doc.meta = s.doc.meta.clone();
    if title_h > 0.0 {
        let style = Style {
            font_size: 40.0,
            bold: true,
            text_color: ink,
            stroke: Color::TRANSPARENT,
            align: TextAlign::Center,
            ..Kind::Text.default_style()
        };
        doc.add(Shape::Text { rect: Rect::new(margin, margin * 0.6, fw - 2.0 * margin, title_h), text: title.clone() }, style)?;
    }
    let caption = Style { font_size: 20.0, bold: false, text_color: ink, stroke: Color::TRANSPARENT, ..Kind::Text.default_style() };
    let badge = Style { fill: accent, text_color: accent.contrasting(), stroke: Color::WHITE, font_size: 18.0, ..Kind::Step.default_style() };
    let picture_style = Style { stroke: Color::rgba(0, 0, 0, 40), width: 1.0, shadow: true, ..Kind::Image.default_style() };
    for (i, (pic, at)) in pics.into_iter().zip(places).enumerate() {
        let at = at.add(origin);
        let rect = Rect::new(at.x, at.y, pic.width() as f32, pic.height() as f32);
        let key = doc.add_image(pic)?;
        doc.add(Shape::Image { rect, image: key }, picture_style)?;
        let said = words.get(i).map(String::as_str).unwrap_or("").trim().to_string();
        let number = u32::try_from(i + 1).unwrap_or(u32::MAX);
        match layout {
            Layout::StepsAcross | Layout::Grid => {
                let y = rect.bottom() + 16.0;
                doc.add(Shape::step(Pt::new(rect.x + 18.0, y + 16.0), number), badge)?;
                if !said.is_empty() {
                    doc.add(Shape::Text { rect: Rect::new(rect.x + 42.0, y, (rect.w - 42.0).max(80.0), 48.0), text: said }, caption)?;
                }
            }
            Layout::StepsDown => {
                let x = origin.x + gap;
                doc.add(Shape::step(Pt::new(x + 18.0, rect.y + 20.0), number), badge)?;
                if !said.is_empty() {
                    doc.add(Shape::Text { rect: Rect::new(x + 44.0, rect.y + 4.0, side - 70.0, 120.0), text: said }, caption)?;
                }
            }
            Layout::BeforeAfter => {
                if !said.is_empty() {
                    let style = Style { font_size: 26.0, bold: true, align: TextAlign::Center, ..caption };
                    doc.add(Shape::Text { rect: Rect::new(rect.x, rect.bottom() + 14.0, rect.w, 48.0), text: said }, style)?;
                }
                if i == 0 {
                    let mid = rect.center().y;
                    let arrow = Style { stroke: accent, width: 8.0, shadow: false, ..Kind::Arrow.default_style() };
                    let shape = Shape::Arrow {
                        from: Pt::new(rect.right() + 22.0, mid),
                        to: Pt::new(rect.right() + gap + 60.0 - 22.0, mid),
                        heads: Default::default(),
                        bend: 0.0,
                        head: Default::default(),
                    };
                    doc.add(shape, arrow)?;
                }
            }
        }
    }
    s.doc = doc;
    s.selection.clear();
    Ok(json!({ "width": fw, "height": fh }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pictures_are_arranged_with_gaps_and_centred() {
        let sizes = [(100.0, 50.0), (60.0, 80.0), (40.0, 40.0)];
        let (at, w, h) = arrange(&sizes, Arrangement::Across, 1, 10.0);
        assert_eq!((w, h), (10.0 + 110.0 + 70.0 + 50.0, 100.0));
        assert_eq!(at, vec![Pt::new(10.0, 25.0), Pt::new(120.0, 10.0), Pt::new(190.0, 30.0)]);
        let (at, w, h) = arrange(&sizes, Arrangement::Down, 1, 0.0);
        assert_eq!((w, h, at[2]), (100.0, 170.0, Pt::new(30.0, 130.0)));
        let (at, w, h) = arrange(&sizes, Arrangement::Grid, 2, 0.0);
        assert_eq!((w, h, at[2]), (160.0, 120.0, Pt::new(30.0, 80.0)));
        assert_eq!(arrange(&[], Arrangement::Grid, 0, 5.0).0.len(), 0);
    }
}
