//! Compares background models on pictures: how long each takes and what it cuts out.
//!
//! `cargo run --release -p screengrabcraft-matte --example compare -- OUT_DIR MODEL.onnx... -- PICTURE...`
//!
//! For every model and picture it writes `OUT_DIR/<picture>-<model>.png` (the cut-out, with its
//! transparency) and `OUT_DIR/<picture>-<model>-on-checks.png` (the picture beside the cut-out
//! on a checkerboard, to look at), and prints the times.

use std::path::{Path, PathBuf};
use std::time::Instant;

use screengrabcraft_matte::{Family, Remover};
use screengrabcraft_model::{Color, Raster};
use screengrabcraft_render::matte::{Matting, apply_clean};

fn main() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let split = args.iter().position(|a| a == "--").ok_or("usage: compare OUT_DIR MODEL... -- PICTURE...")?;
    let out_dir = PathBuf::from(args.first().ok_or("no OUT_DIR")?);
    let models: Vec<PathBuf> = args.get(1..split).unwrap_or_default().iter().map(PathBuf::from).collect();
    let pictures: Vec<PathBuf> = args.get(split + 1..).unwrap_or_default().iter().map(PathBuf::from).collect();
    std::fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
    let decoded: Vec<(String, Raster)> = pictures
        .iter()
        .map(|p| {
            if p.to_string_lossy() == "made-up" {
                return Ok(("window-on-wallpaper".to_string(), window_on_wallpaper()?));
            }
            if p.to_string_lossy() == "made-up-flat" {
                return Ok(("callout-on-plain-colour".to_string(), callout_on_plain_colour()?));
            }
            let bytes = std::fs::read(p).map_err(|e| format!("{}: {e}", p.display()))?;
            let r = screengrabcraft_render::decode(&bytes).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok((stem(p), r))
        })
        .collect::<Result<_, String>>()?;
    println!("| model | file size | ready | picture | size | time |");
    println!("|---|---|---|---|---|---|");
    for m in &models {
        // `solid`: the quick way, no model (the plain colour round the edges).
        if m.to_string_lossy() == "solid" {
            for (name, pic) in &decoded {
                let t = Instant::now();
                let Some(colour) = screengrabcraft_render::matte::edge_colour(pic) else {
                    println!("| solid | | | {name} | | no plain background |");
                    continue;
                };
                let out = screengrabcraft_render::matte::remove_colour(pic, colour, 24, screengrabcraft_render::matte::Region::Edges, true)
                    .map_err(|e| e.to_string())?;
                println!("| solid | — | — | {name} | {}×{} | {:.3} s |", pic.width(), pic.height(), t.elapsed().as_secs_f32());
                write(&out.picture, &out_dir_file(&out_dir, name, "solid"))?;
                write(&side_by_side(pic, &out.picture)?, &out_dir_file(&out_dir, name, "solid-on-checks"))?;
            }
            continue;
        }
        let family = Family::from_name(&m.to_string_lossy()).ok_or_else(|| format!("{}: which family?", m.display()))?;
        let size = std::fs::metadata(m).map(|x| x.len()).unwrap_or(0);
        let bytes = std::fs::read(m).map_err(|e| e.to_string())?;
        let remover = Remover::new(stem(m), family, Box::new(move || Ok(bytes.clone())));
        let t = Instant::now();
        if let Err(e) = remover.model() {
            println!("| {} | {:.1} MB | failed: {e} | | | |", stem(m), size as f64 / 1e6);
            continue;
        }
        let ready = t.elapsed().as_secs_f32();
        for (name, pic) in &decoded {
            let t = Instant::now();
            let matte = match remover.matte(pic, &mut |_| true) {
                Ok(m) => m,
                Err(e) => {
                    println!("| {} | | | {name} | | failed: {e} |", stem(m));
                    continue;
                }
            };
            let took = t.elapsed().as_secs_f32();
            println!("| {} | {:.1} MB | {ready:.1} s | {name} | {}×{} | {took:.2} s |", stem(m), size as f64 / 1e6, pic.width(), pic.height());
            let cut = apply_clean(pic, &matte).map_err(|e| e.to_string())?;
            write(&cut, &out_dir_file(&out_dir, name, &stem(m)))?;
            write(&side_by_side(pic, &cut)?, &out_dir_file(&out_dir, name, &format!("{}-on-checks", stem(m))))?;
        }
    }
    Ok(())
}

fn out_dir_file(dir: &Path, picture: &str, what: &str) -> PathBuf {
    dir.join(format!("{picture}-{what}.png"))
}

/// A made-up screenshot on a plain colour (the picture `made-up-flat`): a card with a shadow, a
/// callout with text, an arrow and a stamp, all with smooth edges.
fn callout_on_plain_colour() -> Result<Raster, String> {
    use screengrabcraft_model::{ArrowHeads, Document, Kind, Pt, Rect, Shape, StampKind, Style};
    let mut d = Document::new(Raster::filled(800, 480, Color::rgb(52, 101, 164)).map_err(|e| e.to_string())?);
    let e = |r: Result<screengrabcraft_model::ObjectId, screengrabcraft_model::DocError>| r.map(|_| ()).map_err(|e| e.to_string());
    let card = Style { fill: Color::rgb(250, 250, 252), stroke: Color::rgb(200, 204, 214), width: 1.0, shadow: true, ..Kind::Rect.default_style() };
    e(d.add(Shape::Rect { rect: Rect::new(120.0, 80.0, 420.0, 280.0), radius: 14.0 }, card))?;
    e(d.add(Shape::Text { rect: Rect::new(150.0, 110.0, 360.0, 60.0), text: "Settings saved".into() }, Kind::Text.default_style()))?;
    e(d.add(
        Shape::Callout { rect: Rect::new(160.0, 200.0, 260.0, 80.0), tail: Pt::new(560.0, 330.0), text: "Click here to keep it".into() },
        Kind::Callout.default_style(),
    ))?;
    e(d.add(Shape::Arrow { from: Pt::new(720.0, 420.0), to: Pt::new(590.0, 330.0), heads: ArrowHeads::End }, Kind::Arrow.default_style()))?;
    let stamp = Style { fill: screengrabcraft_render::stamp::default_fill(StampKind::Check), ..Kind::Stamp.default_style() };
    e(d.add(Shape::Stamp { rect: Rect::new(470.0, 100.0, 48.0, 48.0), stamp: StampKind::Check }, stamp))?;
    screengrabcraft_render::render(&d).ok_or_else(|| "render failed".to_string())
}

fn stem(p: &Path) -> String {
    p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default()
}

fn write(r: &Raster, p: &Path) -> Result<(), String> {
    let bytes = screengrabcraft_render::encode(r, screengrabcraft_model::ImageFormat::Png, 92).map_err(|e| e.to_string())?;
    std::fs::write(p, bytes).map_err(|e| format!("{}: {e}", p.display()))
}

/// A made-up screenshot (the picture `made-up`): a window with a title bar, lines of "text" and
/// a button, on a wallpaper that fades from blue to purple.
fn window_on_wallpaper() -> Result<Raster, String> {
    use tiny_skia::{Color as C, FillRule, LinearGradient, Paint, PathBuilder, Point, Rect, SpreadMode, Transform};
    let mut pix = tiny_skia::Pixmap::new(900, 600).ok_or("no pixmap")?;
    let shader = LinearGradient::new(
        Point::from_xy(0.0, 0.0),
        Point::from_xy(900.0, 600.0),
        vec![tiny_skia::GradientStop::new(0.0, C::from_rgba8(40, 90, 190, 255)), tiny_skia::GradientStop::new(1.0, C::from_rgba8(120, 50, 150, 255))],
        SpreadMode::Pad,
        Transform::identity(),
    )
    .ok_or("no gradient")?;
    let paint = Paint { anti_alias: true, shader, ..Paint::default() };
    pix.fill_rect(Rect::from_xywh(0.0, 0.0, 900.0, 600.0).ok_or("rect")?, &paint, Transform::identity(), None);
    let mut fill = |x: f32, y: f32, w: f32, h: f32, c: (u8, u8, u8)| -> Result<(), String> {
        let mut p = Paint { anti_alias: true, ..Paint::default() };
        p.set_color_rgba8(c.0, c.1, c.2, 255);
        let mut pb = PathBuilder::new();
        pb.push_rect(Rect::from_xywh(x, y, w, h).ok_or("rect")?);
        let path = pb.finish().ok_or("path")?;
        pix.fill_path(&path, &p, FillRule::Winding, Transform::identity(), None);
        Ok(())
    };
    fill(200.0, 130.0, 500.0, 340.0, (246, 246, 248))?;
    fill(200.0, 130.0, 500.0, 36.0, (225, 228, 235))?;
    for (i, w) in [360.0, 300.0, 410.0, 250.0, 330.0].iter().enumerate() {
        fill(230.0, 200.0 + i as f32 * 30.0, *w, 10.0, (90, 95, 110))?;
    }
    fill(560.0, 410.0, 110.0, 34.0, (30, 120, 220))?;
    screengrabcraft_render::canvas::from_pixmap(&pix).ok_or_else(|| "no picture".to_string())
}

/// The picture, and beside it the cut-out on grey checks.
fn side_by_side(pic: &Raster, cut: &Raster) -> Result<Raster, String> {
    let (w, h) = (pic.width(), pic.height());
    let mut out = Raster::filled(w * 2 + 8, h, Color::WHITE).map_err(|e| e.to_string())?;
    let stride = (w * 2 + 8) as usize * 4;
    let px = out.pixels_mut();
    for y in 0..h {
        for x in 0..w {
            let a = pic.pixel(x, y).unwrap_or(Color::BLACK);
            let i = y as usize * stride + x as usize * 4;
            px[i..i + 4].copy_from_slice(&[a.r, a.g, a.b, 255]);
            let c = cut.pixel(x, y).unwrap_or(Color::TRANSPARENT);
            let check = if ((x / 12) + (y / 12)) % 2 == 0 { 200.0 } else { 150.0 };
            let f = f32::from(c.a) / 255.0;
            let mix = |v: u8| (f32::from(v) * f + check * (1.0 - f)).round() as u8;
            let j = y as usize * stride + (w + 8 + x) as usize * 4;
            px[j..j + 4].copy_from_slice(&[mix(c.r), mix(c.g), mix(c.b), 255]);
        }
    }
    Ok(out)
}
