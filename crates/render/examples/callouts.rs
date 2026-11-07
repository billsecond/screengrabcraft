//! Draws every callout form with its tail every way, and the callout quick styles of the first
//! theme, on a plain picture: `cargo run -p screengrabcraft-render --example callouts -- out.png`.

use screengrabcraft_model::quick::{presets, theme};
use screengrabcraft_model::{CalloutForm, Color, Document, Kind, Raster, Rect, Shape, TailSpot};

fn main() -> Result<(), String> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "callouts.png".to_string());
    let (w, h) = (1500u32, 1000u32);
    let mut d = Document::new(Raster::filled(w, h, Color::rgb(0xF4, 0xF4, 0xF2)).map_err(|e| e.to_string())?);
    let spots = [TailSpot::DownLeft, TailSpot::DownRight, TailSpot::Right, TailSpot::Up];
    for (row, form) in CalloutForm::ALL.iter().enumerate() {
        for (col, spot) in spots.iter().enumerate() {
            let rect = Rect::new(80.0 + col as f32 * 300.0, 70.0 + row as f32 * 180.0, 150.0, 60.0);
            let tail = spot.place(*form, rect);
            let shape = Shape::Callout { rect, tail, text: form.label().to_string(), form: *form };
            d.add(shape, Kind::Callout.default_style()).map_err(|e| e.to_string())?;
        }
    }
    // The callout quick styles of the first theme, down the right.
    for (i, p) in presets(Kind::Callout, theme("Bold")).iter().enumerate() {
        let rect = Rect::new(1260.0, 40.0 + i as f32 * 58.0, 110.0, 36.0);
        let form = p.look.options.form.unwrap_or_default();
        let tail = p.look.options.tail.unwrap_or_default().place(form, Rect::new(rect.x, rect.y, rect.w, rect.h * 0.6));
        let shape = Shape::Callout { rect, tail, text: "Aa".into(), form };
        d.add(shape, p.look.style).map_err(|e| e.to_string())?;
    }
    let pic = screengrabcraft_render::render(&d).ok_or("render failed")?;
    let bytes = screengrabcraft_render::encode(&pic, screengrabcraft_model::ImageFormat::Png, 100).map_err(|e| e.to_string())?;
    std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
    println!("wrote {out}");
    Ok(())
}
