//! Draws one of every annotation on a made-up picture and writes it as a PNG, for looking at
//! the drawing code's results: `cargo run -p screengrabcraft-render --example sample -- out.png`.

use screengrabcraft_model::{ArrowHeads, Color, Document, Kind, Pt, Raster, Rect, Shape, StampKind, Style};

fn main() -> Result<(), String> {
    let out = std::env::args().nth(1).unwrap_or_else(|| "sample.png".to_string());
    let (w, h) = (900u32, 560u32);
    // A made-up "window": a title bar, some text-like lines, a coloured panel.
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = if y < 36 {
                [38, 50, 72, 255]
            } else if (60..300).contains(&x) && (70..400).contains(&y) {
                [232, 240, 254, 255]
            } else if y % 22 < 6 && (330..860).contains(&x) && y > 60 && (x / 7) % 9 != 0 {
                [120, 128, 140, 255]
            } else {
                [250, 250, 248, 255]
            };
            px.extend_from_slice(&c);
        }
    }
    let base = Raster::new(w, h, px).map_err(|e| e.to_string())?;
    let mut d = Document::new(base);
    let add = |d: &mut Document, s: Shape| {
        let k = s.kind();
        d.add(s, k.default_style()).map(|_| ())
    };
    let e = |r: Result<(), screengrabcraft_model::DocError>| r.map_err(|e| e.to_string());
    e(add(&mut d, Shape::Arrow { from: Pt::new(420.0, 470.0), to: Pt::new(250.0, 330.0), heads: ArrowHeads::End }))?;
    e(add(&mut d, Shape::Rect { rect: Rect::new(330.0, 90.0, 260.0, 90.0), radius: 10.0 }))?;
    e(add(&mut d, Shape::Ellipse { rect: Rect::new(620.0, 90.0, 200.0, 110.0) }))?;
    e(add(&mut d, Shape::Highlight { points: vec![Pt::new(335.0, 240.0), Pt::new(700.0, 240.0)] }))?;
    e(add(&mut d, Shape::Pen { points: (0..40).map(|i| Pt::new(640.0 + i as f32 * 5.0, 300.0 + (i as f32 * 0.5).sin() * 18.0)).collect() }))?;
    e(add(&mut d, Shape::Text { rect: Rect::new(600.0, 360.0, 260.0, 60.0), text: "Click Save to keep it".into() }))?;
    e(add(
        &mut d,
        Shape::Callout {
            rect: Rect::new(90.0, 430.0, 230.0, 90.0),
            tail: Pt::new(150.0, 380.0),
            text: "This panel shows your recent captures".into(),
        },
    ))?;
    for (i, c) in [(1u32, Pt::new(80.0, 90.0)), (2, Pt::new(350.0, 70.0)), (3, Pt::new(640.0, 70.0))] {
        e(add(&mut d, Shape::Step { center: c, number: i }))?;
    }
    e(add(&mut d, Shape::Blur { rect: Rect::new(340.0, 280.0, 200.0, 60.0) }))?;
    e(add(&mut d, Shape::Pixelate { rect: Rect::new(560.0, 270.0, 60.0, 80.0) }))?;
    e(add(&mut d, Shape::Magnify { rect: Rect::new(470.0, 380.0, 110.0, 110.0) }))?;
    for (i, k) in StampKind::ALL.iter().enumerate() {
        let style = Style { fill: screengrabcraft_render::stamp::default_fill(*k), ..Kind::Stamp.default_style() };
        d.add(Shape::Stamp { rect: Rect::new(610.0 + i as f32 * 40.0, 460.0, 34.0, 34.0), stamp: *k }, style).map_err(|e| e.to_string())?;
    }
    d.add(
        Shape::Spotlight { rect: Rect::new(20.0, 50.0, 880.0, 500.0), ellipse: false },
        Style { strength: 0.25, ..Kind::Spotlight.default_style() },
    )
    .map_err(|e| e.to_string())?;
    let _ = Color::WHITE;
    let pic = screengrabcraft_render::render(&d).ok_or("render failed")?;
    let bytes = screengrabcraft_render::encode(&pic, screengrabcraft_model::ImageFormat::Png, 100).map_err(|e| e.to_string())?;
    std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
    println!("wrote {out}");
    Ok(())
}
