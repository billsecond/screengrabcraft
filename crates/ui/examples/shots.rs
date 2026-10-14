//! Pictures of the interface, drawn headless with the GPU: the editor with annotations, the
//! capture overlay while choosing (crosshair and magnifier) and while adjusting (handles and the
//! capture bar). For looking at the interface without a desktop session:
//!
//! `cargo run -p screengrabcraft-ui --example shots -- <out dir> [light|dark]`

use egui::{pos2, vec2};
use egui_kittest::Harness;
use screengrabcraft_model::{ArrowHeads, Color, Document, EditorSettings, IRect, Pt, Raster, Rect, Shape, Theme};
use screengrabcraft_ui::overlay::{Overlay, OverlayMonitor, OverlayWindow, Purpose};
use screengrabcraft_ui::{Editor, NoServices, Tool};

/// A made-up window-like picture.
fn picture(w: u32, h: u32) -> Raster {
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let c = if y < 40 {
                [38, 50, 72, 255]
            } else if (60..300).contains(&x) && (80..400).contains(&y) {
                [232, 240, 254, 255]
            } else if y % 22 < 6 && (330..w.saturating_sub(40)).contains(&x) && y > 70 && (x / 7) % 9 != 0 {
                [120, 128, 140, 255]
            } else {
                [250, 250, 248, 255]
            };
            px.extend_from_slice(&c);
        }
    }
    Raster::new(w, h, px).unwrap_or_else(|_| Raster::single(Color::WHITE))
}

fn save(h: &mut Harness<'_, impl Sized>, path: &std::path::Path) -> Result<(), String> {
    let img = h.render()?;
    img.save(path).map_err(|e| e.to_string())?;
    println!("wrote {}", path.display());
    Ok(())
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    let theme = if args.next().as_deref() == Some("light") { Theme::Light } else { Theme::Dark };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;

    // ---- the editor ----
    let mut settings = EditorSettings { theme, ..EditorSettings::default() };
    settings.show_tray = false;
    let mut ed = Editor::new(Box::new(NoServices), settings);
    let mut doc = Document::new(picture(900, 560));
    let add = |d: &mut Document, s: Shape| {
        let k = s.kind();
        d.add(s, k.default_style()).ok()
    };
    add(&mut doc, Shape::Arrow { from: Pt::new(430.0, 470.0), to: Pt::new(260.0, 330.0), heads: ArrowHeads::End });
    add(&mut doc, Shape::Rect { rect: Rect::new(330.0, 90.0, 260.0, 90.0), radius: 10.0 });
    add(&mut doc, Shape::Step { center: Pt::new(80.0, 100.0), number: 1 });
    add(&mut doc, Shape::Step { center: Pt::new(350.0, 80.0), number: 2 });
    add(&mut doc, Shape::Blur { rect: Rect::new(340.0, 280.0, 200.0, 60.0) });
    let callout = add(
        &mut doc,
        Shape::Callout { rect: Rect::new(560.0, 360.0, 260.0, 90.0), tail: Pt::new(480.0, 300.0), text: "Recent captures live here".into() },
    );
    ed.session.replace(doc);
    if let Some(id) = callout {
        ed.session.select(vec![id]);
    }
    ed.tool = Tool::Select;
    let mut h = Harness::builder().with_size(vec2(1400.0, 860.0)).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    );
    h.run_steps(6);
    save(&mut h, &dir.join("editor.png"))?;

    // ---- the overlay, choosing ----
    let m = OverlayMonitor { rect: IRect::new(0, 0, 1280, 800), picture: picture(1280, 800), name: "Main".into() };
    let win = OverlayWindow { rect: IRect::new(60, 80, 240, 320), title: "Panel".into() };
    let mut o = Overlay::new(vec![m], vec![win], Purpose::Picture);
    o.preview = true;
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(|ui, o: &mut Overlay| o.show_monitor(ui, 0), o);
    h.run_steps(2);
    h.hover_at(pos2(150.0, 200.0));
    h.run_steps(3);
    save(&mut h, &dir.join("overlay-choosing.png"))?;

    // ---- the overlay, adjusting ----
    let m = OverlayMonitor { rect: IRect::new(0, 0, 1280, 800), picture: picture(1280, 800), name: "Main".into() };
    let mut o = Overlay::new(vec![m], Vec::new(), Purpose::Picture);
    o.preselect(0, IRect::new(320, 70, 560, 300));
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(|ui, o: &mut Overlay| o.show_monitor(ui, 0), o);
    h.run_steps(4);
    save(&mut h, &dir.join("overlay-adjusting.png"))?;
    Ok(())
}
