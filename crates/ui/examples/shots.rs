//! Pictures of the interface, drawn headless with the GPU, for looking at the interface without a
//! desktop session: the picture editor with annotations, one of its dialogs, the video editor with
//! a recording, the capture overlay while choosing (crosshair and magnifier) and while adjusting
//! (handles and the capture bar), and the Recorder.
//!
//! `cargo run -p screengrabcraft-ui --example shots -- <out dir> [light|dark]`
//!
//! With `live` after them, the README's pictures instead: the same interface running in real
//! windows on this computer (each opens for a few seconds, then saves a picture of itself, only
//! of itself, and closes), each put on a made-up desktop.
//!
//! `cargo run -p screengrabcraft-ui --example shots -- <out dir> dark live`
//!
//! Everything in them is made up and painted here: the program in the pictures ("Fernhill
//! Market") doesn't exist, nor do its customers, and the person on the camera is three circles.

use egui::{Align2, Color32, FontId, Stroke, StrokeKind, pos2, vec2};
use egui_kittest::Harness;
use screengrabcraft_media::captions::{Captions, Cue, Speaker};
use screengrabcraft_media::recording::{Header, RecordingWriter, StreamInfo, StreamKind, captions_bytes};
use screengrabcraft_model::{ArrowHeads, CalloutForm, Document, EditorSettings, IRect, Pt, Raster, Rect, Shape, Theme};
use screengrabcraft_ui::dialogs::Dialog;
use screengrabcraft_ui::overlay::{Overlay, OverlayMonitor, OverlayWindow, Purpose};
use screengrabcraft_ui::{Editor, NoServices, OpenedFile, Tool};

const INK: Color32 = Color32::from_rgb(0x1E, 0x25, 0x35);
const MUTED: Color32 = Color32::from_rgb(0x6B, 0x74, 0x86);
const HAIR: Color32 = Color32::from_rgb(0xE3, 0xE6, 0xEC);
const BLUE: Color32 = Color32::from_rgb(0x2F, 0x6F, 0xED);
const GREEN: Color32 = Color32::from_rgb(0x1E, 0x9E, 0x6A);
const AMBER: Color32 = Color32::from_rgb(0xB7, 0x79, 0x1F);

/// The made-up program's window is laid out this large, and scaled to where it's painted.
const APP: (f32, f32) = (1100.0, 680.0);

fn said<E: std::fmt::Debug>(e: E) -> String {
    format!("{e:?}")
}

/// The window of a made-up program, painted to fill `r` (as wide as it is; [`APP`]'s shape): a
/// market's week, with three numbers, a chart, what sold best and the latest orders.
fn paint_app(p: &egui::Painter, ctx: &egui::Context, r: egui::Rect) {
    let u = r.width() / APP.0;
    let at = |x: f32, y: f32| pos2(r.min.x + x * u, r.min.y + y * u);
    let rc = |x: f32, y: f32, w: f32, h: f32| egui::Rect::from_min_size(at(x, y), vec2(w * u, h * u));
    let plain = |size: f32| FontId::proportional(size * u);
    let bold = |size: f32| screengrabcraft_ui::widgets::strong_font(ctx, size * u);
    let hair = Stroke::new(1.0, HAIR);
    let faint = Stroke::new(1.0, Color32::from_rgb(0xEE, 0xF0, 0xF4));
    let card = |x: f32, y: f32, w: f32, h: f32| {
        p.rect_filled(rc(x, y, w, h), 10.0 * u, Color32::WHITE);
        p.rect_stroke(rc(x, y, w, h), 10.0 * u, hair, StrokeKind::Inside);
    };
    p.rect_filled(r, 0.0, Color32::from_rgb(0xFA, 0xFB, 0xFD));

    // Its pages, down the left.
    p.rect_filled(rc(0.0, 0.0, 190.0, APP.1), 0.0, Color32::from_rgb(0xF1, 0xF3, 0xF8));
    p.line_segment([at(190.0, 0.0), at(190.0, APP.1)], hair);
    p.circle_filled(at(30.0, 36.0), 9.0 * u, GREEN);
    p.text(at(48.0, 36.0), Align2::LEFT_CENTER, "Fernhill Market", bold(15.0), INK);
    for (i, name) in ["Overview", "Orders", "Products", "Customers", "Reports", "Settings"].into_iter().enumerate() {
        let y = 92.0 + 38.0 * i as f32;
        let on = i == 0;
        if on {
            p.rect_filled(rc(12.0, y - 16.0, 166.0, 32.0), 8.0 * u, Color32::from_rgb(0xDF, 0xE9, 0xFD));
        }
        p.rect_filled(rc(26.0, y - 5.0, 10.0, 10.0), 3.0 * u, if on { BLUE } else { Color32::from_rgb(0xB4, 0xBB, 0xC9) });
        p.text(at(48.0, y), Align2::LEFT_CENTER, name, if on { bold(13.5) } else { plain(13.5) }, if on { BLUE } else { INK });
    }

    // The page's name, a search box, whose it is.
    p.text(at(218.0, 40.0), Align2::LEFT_CENTER, "Overview", bold(24.0), INK);
    p.text(at(218.0, 66.0), Align2::LEFT_CENTER, "The week of 9 November", plain(13.0), MUTED);
    p.rect_filled(rc(790.0, 24.0, 210.0, 34.0), 8.0 * u, Color32::WHITE);
    p.rect_stroke(rc(790.0, 24.0, 210.0, 34.0), 8.0 * u, hair, StrokeKind::Inside);
    p.text(at(804.0, 41.0), Align2::LEFT_CENTER, "Search orders", plain(12.5), MUTED);
    p.circle_filled(at(1046.0, 41.0), 17.0 * u, BLUE);
    p.text(at(1046.0, 41.0), Align2::CENTER_CENTER, "MC", bold(12.0), Color32::WHITE);

    // Three numbers.
    for (i, (label, number, change)) in
        [("Orders", "1,284", "+12%"), ("Revenue", "$48,210", "+8%"), ("Returning customers", "64%", "+3%")].into_iter().enumerate()
    {
        let x = 218.0 + 290.0 * i as f32;
        card(x, 92.0, 272.0, 92.0);
        p.text(at(x + 18.0, 116.0), Align2::LEFT_CENTER, label, plain(12.5), MUTED);
        p.text(at(x + 18.0, 152.0), Align2::LEFT_CENTER, number, bold(26.0), INK);
        p.text(at(x + 254.0, 154.0), Align2::RIGHT_CENTER, change, bold(13.0), GREEN);
    }

    // The week's orders, a bar a day.
    card(218.0, 202.0, 562.0, 236.0);
    p.text(at(236.0, 228.0), Align2::LEFT_CENTER, "Orders this week", bold(14.5), INK);
    for k in 0..4 {
        let y = 408.0 - 44.0 * k as f32;
        p.line_segment([at(240.0, y), at(758.0, y)], faint);
    }
    for (i, (day, orders)) in
        [("Mon", 46.0), ("Tue", 58.0), ("Wed", 52.0), ("Thu", 64.0), ("Fri", 71.0), ("Sat", 96.0), ("Sun", 80.0)].into_iter().enumerate()
    {
        let x = 262.0 + 70.0 * i as f32;
        let tall = orders * 1.4;
        let colour = if day == "Sat" { BLUE } else { Color32::from_rgb(0xA9, 0xC4, 0xF8) };
        p.rect_filled(rc(x, 408.0 - tall, 44.0, tall), 5.0 * u, colour);
        p.text(at(x + 22.0, 424.0), Align2::CENTER_CENTER, day, plain(11.5), MUTED);
    }

    // What sold best.
    card(798.0, 202.0, 272.0, 236.0);
    p.text(at(816.0, 228.0), Align2::LEFT_CENTER, "Top products", bold(14.5), INK);
    for (i, (name, sold, share)) in
        [("Sourdough loaf", "312", 0.86), ("Apple cider", "231", 0.64), ("Honey, 500 g", "170", 0.47), ("Free-range eggs", "139", 0.38)]
            .into_iter()
            .enumerate()
    {
        let y = 264.0 + 43.0 * i as f32;
        p.text(at(816.0, y), Align2::LEFT_CENTER, name, plain(13.0), INK);
        p.text(at(1052.0, y), Align2::RIGHT_CENTER, sold, plain(12.0), MUTED);
        p.rect_filled(rc(816.0, y + 13.0, 236.0, 6.0), 3.0 * u, Color32::from_rgb(0xEC, 0xEF, 0xF5));
        p.rect_filled(rc(816.0, y + 13.0, 236.0 * share, 6.0), 3.0 * u, GREEN);
    }

    // The latest orders.
    card(218.0, 456.0, 852.0, 206.0);
    p.text(at(236.0, 482.0), Align2::LEFT_CENTER, "Recent orders", bold(14.5), INK);
    for (x, name) in [(236.0, "Order"), (330.0, "Customer"), (520.0, "Email"), (790.0, "Total"), (900.0, "Status")] {
        p.text(at(x, 514.0), Align2::LEFT_CENTER, name, plain(11.5), MUTED);
    }
    let orders = [
        ("#1042", "Maya Chen", "maya.chen@example.com", "$128.40", "Paid"),
        ("#1041", "Tomas Rivera", "t.rivera@example.com", "$64.00", "Pending"),
        ("#1040", "Priya Nair", "priya@example.com", "$212.75", "Paid"),
        ("#1039", "Sam Okafor", "sam.okafor@example.com", "$39.90", "Refunded"),
    ];
    for (i, (order, who, mail, total, status)) in orders.into_iter().enumerate() {
        let y = 548.0 + 32.0 * i as f32;
        p.line_segment([at(236.0, y - 16.0), at(1052.0, y - 16.0)], faint);
        p.text(at(236.0, y), Align2::LEFT_CENTER, order, plain(13.0), MUTED);
        p.text(at(330.0, y), Align2::LEFT_CENTER, who, bold(13.0), INK);
        p.text(at(520.0, y), Align2::LEFT_CENTER, mail, plain(13.0), INK);
        p.text(at(790.0, y), Align2::LEFT_CENTER, total, plain(13.0), INK);
        let (tint, ink) = match status {
            "Paid" => (Color32::from_rgb(0xDF, 0xF4, 0xE9), GREEN),
            "Pending" => (Color32::from_rgb(0xFB, 0xF0, 0xD9), AMBER),
            _ => (Color32::from_rgb(0xEC, 0xEF, 0xF5), MUTED),
        };
        p.rect_filled(rc(900.0, y - 11.0, 84.0, 22.0), 11.0 * u, tint);
        p.text(at(942.0, y), Align2::CENTER_CENTER, status, bold(11.5), ink);
    }
}

/// Where the made-up program's window is on the made-up desktop, on a monitor 1280 wide.
const WINDOW: (f32, f32, f32) = (96.0, 36.0, 1088.0);
/// The height of that window's title bar.
const TITLE: f32 = 34.0;

/// A made-up wallpaper filling `r`: from deep blue down to green, two soft lights on it.
fn paint_wallpaper(p: &egui::Painter, r: egui::Rect) {
    let bands = 96;
    for i in 0..bands {
        let t = i as f32 / (bands - 1) as f32;
        let mix = |a: f32, b: f32| (a + (b - a) * t) as u8;
        let y = r.min.y + r.height() * i as f32 / bands as f32;
        let band = egui::Rect::from_min_max(pos2(r.min.x, y), pos2(r.max.x, y + r.height() / bands as f32 + 1.0));
        p.rect_filled(band, 0.0, Color32::from_rgb(mix(31.0, 44.0), mix(58.0, 140.0), mix(110.0, 150.0)));
    }
    for (centre, radius) in [
        (pos2(r.min.x + r.width() * 0.18, r.min.y + r.height() * 0.12), r.width() * 0.34),
        (pos2(r.max.x - r.width() * 0.12, r.max.y), r.width() * 0.3),
    ] {
        for k in 0..10 {
            p.circle_filled(centre, radius * (1.0 - k as f32 * 0.09), Color32::from_white_alpha(3));
        }
    }
}

/// A made-up desktop filling `r`: a wallpaper, the made-up program's window on it, a task bar.
fn paint_desktop(p: &egui::Painter, ctx: &egui::Context, r: egui::Rect) {
    let u = r.width() / 1280.0;
    paint_wallpaper(p, r);
    // The window, with a soft shadow under it.
    let body_tall = WINDOW.2 * APP.1 / APP.0;
    let win = egui::Rect::from_min_size(pos2(r.min.x + WINDOW.0 * u, r.min.y + WINDOW.1 * u), vec2(WINDOW.2 * u, (TITLE + body_tall) * u));
    for k in 1..=6 {
        p.rect_filled(win.expand(k as f32 * 2.0 * u).translate(vec2(0.0, 5.0 * u)), 12.0 * u, Color32::from_black_alpha(12));
    }
    let bar = egui::Rect::from_min_size(win.min, vec2(win.width(), TITLE * u));
    p.rect_filled(bar, 0.0, Color32::from_rgb(0xE6, 0xE9, 0xF0));
    p.text(bar.center(), Align2::CENTER_CENTER, "Fernhill Market", FontId::proportional(12.5 * u), MUTED);
    for k in 0..3 {
        p.circle_filled(pos2(bar.max.x - (22.0 + 22.0 * k as f32) * u, bar.center().y), 5.0 * u, Color32::from_rgb(0xB9, 0xC0, 0xCC));
    }
    let body = egui::Rect::from_min_max(pos2(win.min.x, bar.max.y), win.max);
    paint_app(&p.with_clip_rect(body), ctx, body);
    // The task bar: four programs and the time.
    let task = egui::Rect::from_min_max(pos2(r.min.x, r.max.y - 44.0 * u), r.max);
    p.rect_filled(task, 0.0, Color32::from_rgb(0x14, 0x1B, 0x2B));
    for (k, colour) in [BLUE, GREEN, Color32::from_rgb(0xE0, 0x8A, 0x2B), Color32::from_rgb(0x9B, 0x6B, 0xDF)].into_iter().enumerate() {
        let at = pos2(r.min.x + (34.0 + 44.0 * k as f32) * u, task.center().y);
        p.rect_filled(egui::Rect::from_center_size(at, vec2(24.0 * u, 24.0 * u)), 6.0 * u, colour);
    }
    p.text(
        pos2(r.max.x - 18.0 * u, task.center().y),
        Align2::RIGHT_CENTER,
        "10:24",
        FontId::proportional(12.5 * u),
        Color32::from_rgb(0xDD, 0xE2, 0xEC),
    );
}

/// Somebody on a camera, as plainly made up as can be: a head, hair and shoulders.
fn paint_person(p: &egui::Painter, _ctx: &egui::Context, r: egui::Rect) {
    p.rect_filled(r, 0.0, Color32::from_rgb(0x2B, 0x6F, 0x7B));
    let head = pos2(r.center().x, r.min.y + r.height() * 0.42);
    let size = r.height() * 0.21;
    p.circle_filled(head, r.width() * 0.44, Color32::from_rgb(0x36, 0x82, 0x8B));
    p.circle_filled(pos2(r.center().x, r.max.y + r.height() * 0.30), r.width() * 0.50, Color32::from_rgb(0x1F, 0x2A, 0x44));
    p.circle_filled(pos2(head.x, head.y - size * 0.18), size * 1.08, Color32::from_rgb(0x4A, 0x34, 0x2A));
    p.circle_filled(pos2(head.x, head.y + size * 0.08), size * 0.94, Color32::from_rgb(0xEC, 0xD3, 0xBE));
}

/// What `paint` draws, as a picture `w` by `h`.
fn drawn(w: u32, h: u32, paint: fn(&egui::Painter, &egui::Context, egui::Rect)) -> Result<Raster, String> {
    let mut harness = Harness::builder().with_size(vec2(w as f32, h as f32)).build_ui(move |ui| {
        let ctx = ui.ctx().clone();
        let p = ctx.layer_painter(egui::LayerId::background());
        paint(&p, &ctx, ctx.viewport_rect());
    });
    screengrabcraft_ui::install_fonts(&harness.ctx);
    harness.run_steps(3);
    let img = harness.render()?;
    Raster::new(img.width(), img.height(), img.into_raw()).map_err(said)
}

/// A made-up recording of the made-up program: 8 s of its window, somebody on the camera, their
/// voice on the microphone, somebody else's from the speakers, and what they said.
fn recording() -> Result<Vec<u8>, String> {
    let screen = drawn(880, 544, paint_app)?;
    let person = drawn(320, 240, paint_person)?;
    let rgb: Vec<u8> = person.pixels().as_chunks::<4>().0.iter().flat_map(|[r, g, b, _]| [*r, *g, *b]).collect();
    let camera = image::RgbImage::from_raw(person.width(), person.height(), rgb).ok_or("the camera's picture")?;
    let mut jpeg = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88).encode_image(&camera).map_err(said)?;
    let streams = vec![
        StreamInfo { id: 0, kind: StreamKind::Screen, name: String::new(), width: screen.width(), height: screen.height(), channels: 0 },
        StreamInfo { id: 1, kind: StreamKind::SystemSound, name: "Speakers".into(), width: 0, height: 0, channels: 2 },
        StreamInfo { id: 2, kind: StreamKind::Microphone, name: "Desk microphone".into(), width: 0, height: 0, channels: 1 },
        StreamInfo { id: 3, kind: StreamKind::Camera, name: "Camera".into(), width: person.width(), height: person.height(), channels: 0 },
    ];
    let mut w = RecordingWriter::new(Vec::new(), &Header { version: 1, created: String::new(), fps: 10, streams }).map_err(said)?;
    // A voice: a tone that swells and fades a few times a second while it speaks, else silence.
    let voice = |sample: u64, pitch: f32, speaks: &[(f32, f32)]| -> i16 {
        use std::f32::consts::TAU;
        let t = sample as f32 / 48_000.0;
        if !speaks.iter().any(|(from, to)| t >= *from && t < *to) {
            return 0;
        }
        let swell = (t * 4.3 * TAU).sin().abs().powf(0.6) * (0.5 + 0.5 * (t * 0.9 * TAU).sin().abs());
        let tone = (t * pitch * TAU).sin() * 0.6 + (t * pitch * 2.31 * TAU).sin() * 0.3 + (t * pitch * 5.7 * TAU).sin() * 0.1;
        (tone * swell * 15_000.0) as i16
    };
    let (you, others) = ([(0.3, 2.6), (5.4, 7.7)], [(3.0, 5.0)]);
    for frame in 0..80u64 {
        w.video_frame(0, frame * 100_000, &screen).map_err(said)?;
        let first = frame * 4800;
        let microphone: Vec<i16> = (0..4800).map(|i| voice(first + i, 170.0, &you)).collect();
        let speakers: Vec<i16> = (0..4800).flat_map(|i| std::iter::repeat_n(voice(first + i, 120.0, &others), 2)).collect();
        w.audio(1, first, &speakers).map_err(said)?;
        w.audio(2, first, &microphone).map_err(said)?;
        w.jpeg_frame(3, frame * 100_000, &jpeg).map_err(said)?;
    }
    let mut bytes = w.finish(8_000_000).map_err(said)?;
    let captions = Captions {
        cues: vec![
            Cue { start: 300_000, end: 2_600_000, speaker: Speaker::You, text: "Here's how the week went at the market.".into() },
            Cue { start: 3_000_000, end: 5_000_000, speaker: Speaker::Others, text: "Saturday really stands out.".into() },
            Cue { start: 5_400_000, end: 7_700_000, speaker: Speaker::You, text: "That was the harvest fair. Orders are next.".into() },
        ],
        model: "made up".into(),
    };
    bytes.extend(captions_bytes(&captions).map_err(said)?);
    Ok(bytes)
}

fn save(h: &mut Harness<'_, impl Sized>, path: &std::path::Path) -> Result<(), String> {
    save_within(h, path, 0)
}

/// Saves the picture without `margin` pixels all round.
fn save_within(h: &mut Harness<'_, impl Sized>, path: &std::path::Path, margin: u32) -> Result<(), String> {
    let img = h.render()?;
    let (w, t) = (img.width().saturating_sub(2 * margin), img.height().saturating_sub(2 * margin));
    image::imageops::crop_imm(&img, margin, margin, w, t).to_image().save(path).map_err(|e| e.to_string())?;
    println!("wrote {}", path.display());
    Ok(())
}

/// The margin a headless window puts round what it shows (a real window has none).
const MARGIN: f32 = 8.0;

/// The editor's window in these pictures.
const EDITOR: egui::Vec2 = vec2(1480.0, 920.0);

fn editor_harness(ed: Editor) -> Harness<'static, Editor> {
    Harness::builder().with_size(EDITOR).build_ui_state(
        |ui, ed: &mut Editor| {
            let ctx = ui.ctx().clone();
            ed.logic(&ctx);
            ed.ui(ui);
        },
        ed,
    )
}

/// The picture editor with the made-up program's window in it, marked up: the addresses blurred,
/// the number that matters boxed, the total asked about marked, the order that waits pointed at,
/// three steps, and a callout chosen.
fn picture_editor(settings: &EditorSettings) -> Result<Editor, String> {
    let mut ed = Editor::new(Box::new(NoServices), settings.clone());
    let mut doc = Document::new(drawn(APP.0 as u32, APP.1 as u32, paint_app)?);
    let add = |d: &mut Document, s: Shape| {
        let k = s.kind();
        d.add(s, k.default_style()).ok()
    };
    // The addresses aren't for showing; the number that matters is boxed; the total that was
    // asked about is marked; the order that waits is pointed at.
    add(&mut doc, Shape::Blur { rect: Rect::new(512.0, 534.0, 236.0, 124.0) });
    add(&mut doc, Shape::highlight(Rect::new(777.0, 537.0, 84.0, 22.0)));
    add(&mut doc, Shape::Rect { rect: Rect::new(502.0, 84.0, 284.0, 108.0), radius: 12.0 });
    add(
        &mut doc,
        Shape::Arrow { from: Pt::new(1052.0, 624.0), to: Pt::new(992.0, 586.0), heads: ArrowHeads::End, bend: 0.0, head: Default::default() },
    );
    add(&mut doc, Shape::step(Pt::new(164.0, 130.0), 1));
    add(&mut doc, Shape::step(Pt::new(504.0, 86.0), 2));
    add(&mut doc, Shape::step(Pt::new(222.0, 460.0), 3));
    let callout = add(
        &mut doc,
        Shape::Callout {
            rect: Rect::new(300.0, 248.0, 250.0, 58.0),
            tail: Pt::new(618.0, 284.0),
            text: "Saturday was the busiest day".into(),
            form: CalloutForm::Rounded,
            tails: Vec::new(),
        },
    );
    ed.session.replace(doc);
    if let Some(id) = callout {
        ed.session.select(vec![id]);
    }
    ed.tool = Tool::Select;
    Ok(ed)
}

/// The video editor with the made-up recording in it, a name put on it, and that edit in the file
/// with the recording, as the editor writes it (opened again, it is as it was left, "Edits
/// saved"). `chosen`: a stretch chosen, the playhead in it; else the name chosen.
fn video_editor(settings: &EditorSettings, chosen: bool) -> Result<Editor, String> {
    let mut bytes = recording()?;
    let mut ed = Editor::new(Box::new(NoServices), settings.clone());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Market week.sgvideo".into(), path: None, bytes: bytes.clone() });
    let v = ed.video.as_mut().ok_or("the recording didn't open")?;
    let name = serde_json::json!({"template": "lower-bar", "at": 400_000, "title": "Maya Chen", "subtitle": "Fernhill Market"});
    v.session.execute("video.place", &name).map_err(|e| e.to_string())?;
    bytes.extend(screengrabcraft_media::recording::edit_bytes(&v.session.timeline).map_err(said)?);
    let mut ed = Editor::new(Box::new(NoServices), settings.clone());
    ed.open_file(&egui::Context::default(), OpenedFile { name: "Market week.sgvideo".into(), path: None, bytes });
    let v = ed.video.as_mut().ok_or("the recording didn't open again")?;
    if chosen {
        v.range = Some((2_400_000, 5_200_000));
        v.playhead = 3_000_000;
    } else {
        v.item = v.session.timeline.layers.first().and_then(|layer| layer.items.first().map(|item| (layer.id, item.id)));
        v.playhead = 1_400_000;
    }
    Ok(ed)
}

/// The made-up desktop, and the capture overlay over it with the chart chosen and the capture
/// bar under it.
fn overlay_on_desktop() -> Result<Overlay, String> {
    let desktop = drawn(1280, 800, paint_desktop)?;
    let mut o =
        Overlay::new(vec![OverlayMonitor { rect: IRect::new(0, 0, 1280, 800), picture: desktop, name: "Main".into() }], Vec::new(), Purpose::Picture);
    o.preselect(0, IRect::new(302, 260, 574, 252));
    Ok(o)
}

fn main() -> Result<(), String> {
    let mut args = std::env::args().skip(1);
    let dir = std::path::PathBuf::from(args.next().unwrap_or_else(|| ".".to_string()));
    let theme = if args.next().as_deref() == Some("light") { Theme::Light } else { Theme::Dark };
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    match args.next().as_deref() {
        Some("live") => return live::all(&dir, theme),
        Some("live-one") => return live::one(&dir, theme, &args.next().unwrap_or_default()),
        _ => {}
    }

    // ---- the picture editor: the made-up program's window, marked up ----
    let mut settings = EditorSettings { theme, ..EditorSettings::default() };
    settings.show_tray = false;
    let mut h = editor_harness(picture_editor(&settings)?);
    h.run_steps(6);
    save(&mut h, &dir.join("editor.png"))?;

    // ---- a dialog (`widgets`: title and X, settings, [Apply] [Cancel] at the bottom right) ----
    let doc = h.state().session.doc.clone();
    screengrabcraft_ui::dialogs::open(h.state_mut(), Dialog::resize(&doc));
    h.run_steps(4);
    save(&mut h, &dir.join("dialog.png"))?;

    // ---- the video editor: a made-up recording, a name put on it ----
    let mut h = editor_harness(video_editor(&settings, false)?);
    h.run_steps(12);
    save(&mut h, &dir.join("video-editor.png"))?;

    // ---- the overlay, choosing: the window under the pointer is offered ----
    let desktop = drawn(1280, 800, paint_desktop)?;
    let monitor = || OverlayMonitor { rect: IRect::new(0, 0, 1280, 800), picture: desktop.clone(), name: "Main".into() };
    let tall = (TITLE + WINDOW.2 * APP.1 / APP.0) as u32;
    let win = OverlayWindow { rect: IRect::new(WINDOW.0 as i32, WINDOW.1 as i32, WINDOW.2 as u32, tall), title: "Fernhill Market".into() };
    let mut o = Overlay::new(vec![monitor()], vec![win], Purpose::Picture);
    o.preview = true;
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(|ui, o: &mut Overlay| o.show_monitor(ui, 0), o);
    h.run_steps(2);
    h.hover_at(pos2(520.0, 300.0));
    h.run_steps(3);
    save(&mut h, &dir.join("overlay-choosing.png"))?;

    // ---- the overlay, adjusting: the chart chosen, the capture bar under it ----
    let o = overlay_on_desktop()?;
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(|ui, o: &mut Overlay| o.show_monitor(ui, 0), o);
    screengrabcraft_ui::install_fonts(&h.ctx);
    h.run_steps(4);
    save(&mut h, &dir.join("overlay-adjusting.png"))?;

    // ---- the overlay, placing a fixed size ----
    let o = Overlay::new(vec![monitor()], Vec::new(), Purpose::VideoFixed(640, 360));
    let mut h = Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(|ui, o: &mut Overlay| o.show_monitor(ui, 0), o);
    screengrabcraft_ui::install_fonts(&h.ctx);
    h.run_steps(2);
    h.hover_at(pos2(1200.0, 700.0));
    h.run_steps(3);
    save(&mut h, &dir.join("overlay-fixed.png"))?;

    // ---- the Recorder (Video Production's window) ----
    recorder_shots(&dir, theme)?;
    Ok(())
}

/// The Recorder as the capture service shows it: an area dragged out with every input on and the
/// microphone's meter; a fixed size of one's own; the camera alone.
fn recorder_shots(dir: &std::path::Path, theme: Theme) -> Result<(), String> {
    use screengrabcraft_model::{RecordArea, Settings};
    use screengrabcraft_ui::recorder::{self, MicLevel, RecorderView};
    let devices = made_up_devices();
    let mut fixed = Settings::default();
    fixed.recorder.area = RecordArea::Fixed;
    (fixed.recorder.width, fixed.recorder.height) = (1000, 640);
    fixed.production.camera = false;
    let mut camera_only = Settings::default();
    camera_only.recorder.screen = false;
    camera_only.production.system_sound = false;
    let mut all = Settings::default();
    all.recorder.area = RecordArea::AllScreens;
    let mut listening = RecorderView::default();
    listening.listen = true;
    let person = drawn(320, 240, paint_person)?;
    for (name, settings, view, mic, camera_on) in [
        ("recorder.png", Settings::default(), listening.clone(), MicLevel::Level(0.6), true),
        ("recorder-fixed.png", fixed, RecorderView::default(), MicLevel::Off, false),
        ("recorder-camera-only.png", camera_only, RecorderView::default(), MicLevel::Off, true),
        ("recorder-all-screens.png", all, listening, MicLevel::Failed("there is no microphone".into()), false),
    ] {
        let state = (settings, view, mic, recorder::CameraPreview::Off);
        let d = devices.clone();
        // As large inside as the real window is.
        let mut h = Harness::builder().with_size(recorder::SIZE + vec2(2.0 * MARGIN, 2.0 * MARGIN)).build_ui_state(
            move |ui, (s, v, m, camera): &mut (Settings, RecorderView, MicLevel, recorder::CameraPreview)| {
                screengrabcraft_ui::theme::apply(ui.ctx(), theme);
                let _ = recorder::recorder_ui(ui, v, &mut s.recorder, &mut s.production, &d, m, &recorder::MicLevel::Level(0.25), camera);
            },
            state,
        );
        screengrabcraft_ui::install_fonts(&h.ctx);
        if camera_on {
            let size = [person.width() as usize, person.height() as usize];
            let picture = egui::ColorImage::from_rgba_unmultiplied(size, person.pixels());
            let texture = h.ctx.load_texture("camera", picture, egui::TextureOptions::LINEAR);
            h.state_mut().3 = recorder::CameraPreview::Live(texture);
        }
        h.run_steps(4);
        save_within(&mut h, &dir.join(name), MARGIN as u32)?;
    }
    Ok(())
}

/// The made-up computer the Recorder is shown on: two monitors, two microphones, a camera.
fn made_up_devices() -> screengrabcraft_ui::recorder::RecorderDevices {
    use screengrabcraft_ui::recorder::{RecorderDevices, RecorderMonitor};
    RecorderDevices {
        monitors: vec![
            RecorderMonitor { name: "DELL U2720Q".into(), x: 0, y: 0, width: 3840, height: 2160, primary: true },
            RecorderMonitor { name: "Built-in display".into(), x: -1920, y: 540, width: 1920, height: 1080, primary: false },
        ],
        all_screens: Some((3840, 1620)),
        microphones: vec!["Headset microphone".into(), "Webcam microphone".into()],
        default_microphone: Some("Headset microphone".into()),
        cameras: Some(vec![(0, "Integrated Webcam".into())]),
        camera_supported: true,
        system_sound: true,
        system_sound_platform: true,
        speakers: vec!["Speakers".to_string()],
        default_speaker: Some("Speakers".to_string()),
    }
}

/// The README's pictures, from the interface running in real windows on this computer.
mod live {
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex};

    use egui::{Color32, Stroke, StrokeKind, pos2, vec2};
    use egui_kittest::Harness;
    use screengrabcraft_model::{EditorSettings, Raster, Settings, Theme};
    use screengrabcraft_ui::Editor;
    use screengrabcraft_ui::overlay::Overlay;
    use screengrabcraft_ui::recorder::{self, CameraPreview, MicLevel, RecorderDevices, RecorderView};

    use super::{drawn, paint_desktop, paint_person, paint_wallpaper, said};

    /// How many frames a window is drawn before its picture is taken: its fonts, textures and the
    /// pictures made over several frames (the library's cards) all in.
    const SETTLE: u32 = 45;

    type RecorderScene = (Settings, RecorderView, RecorderDevices, Raster, Option<CameraPreview>, Theme);

    /// What a window shows.
    enum Scene {
        Editor(Box<Editor>),
        Overlay(Box<Overlay>),
        Recorder(Box<RecorderScene>),
    }

    /// A real window showing a scene: once it has been drawn a while, it saves a picture of
    /// itself (what it shows, nothing else of the screen) and closes.
    struct Live {
        scene: Scene,
        frames: u32,
        out: PathBuf,
        done: Arc<Mutex<Option<Result<(), String>>>>,
    }

    impl eframe::App for Live {
        fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
            if let Scene::Editor(ed) = &mut self.scene {
                ed.logic(ctx);
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(image) = shot {
                let rgba: Vec<u8> = image.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect();
                let saved = image::RgbaImage::from_raw(image.size[0] as u32, image.size[1] as u32, rgba)
                    .ok_or_else(|| "the picture of the window".to_string())
                    .and_then(|img| img.save(&self.out).map_err(said));
                if let Ok(mut done) = self.done.lock() {
                    *done = Some(saved);
                }
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
            self.frames += 1;
            if self.frames == SETTLE {
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            ctx.request_repaint();
        }

        fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
            match &mut self.scene {
                Scene::Editor(ed) => ed.ui(ui),
                Scene::Overlay(o) => o.show_monitor(ui, 0),
                Scene::Recorder(r) => {
                    let (settings, view, devices, person, camera, theme) = &mut **r;
                    screengrabcraft_ui::theme::apply(ui.ctx(), *theme);
                    let preview = camera.get_or_insert_with(|| {
                        let size = [person.width() as usize, person.height() as usize];
                        let picture = egui::ColorImage::from_rgba_unmultiplied(size, person.pixels());
                        CameraPreview::Live(ui.ctx().load_texture("camera", picture, egui::TextureOptions::LINEAR))
                    });
                    egui::CentralPanel::default().show(ui, |ui| {
                        let _ = recorder::recorder_ui(
                            ui,
                            view,
                            &mut settings.recorder,
                            &mut settings.production,
                            devices,
                            &MicLevel::Level(0.6),
                            &MicLevel::Level(0.25),
                            preview,
                        );
                    });
                }
            }
        }
    }

    /// One scene in a real window of its own, its picture saved as `window-<scene>.png`.
    pub fn one(dir: &Path, theme: Theme, scene: &str) -> Result<(), String> {
        let mut settings = EditorSettings { theme, ..EditorSettings::default() };
        settings.show_tray = false;
        let (shown, size, title) = match scene {
            "editor" => (Scene::Editor(Box::new(super::picture_editor(&settings)?)), super::EDITOR, screengrabcraft_model::EDITOR_NAME),
            "video" => (Scene::Editor(Box::new(super::video_editor(&settings, true)?)), super::EDITOR, screengrabcraft_model::VIDEO_EDITOR_NAME),
            "overlay" => (Scene::Overlay(Box::new(super::overlay_on_desktop()?)), vec2(1280.0, 800.0), "ScreenGrabCraft"),
            "recorder" => {
                let mut view = RecorderView::default();
                view.listen = true;
                let person = drawn(320, 240, paint_person)?;
                (Scene::Recorder(Box::new((Settings::default(), view, super::made_up_devices(), person, None, theme))), recorder::SIZE, "Recorder")
            }
            other => return Err(format!("no scene called {other}")),
        };
        let out = dir.join(format!("window-{scene}.png"));
        let done = Arc::new(Mutex::new(None));
        let options = eframe::NativeOptions {
            viewport: egui::ViewportBuilder::default().with_title(title).with_inner_size(size).with_resizable(false),
            ..Default::default()
        };
        let finished = done.clone();
        eframe::run_native(
            title,
            options,
            Box::new(move |cc| {
                screengrabcraft_ui::install_fonts(&cc.egui_ctx);
                Ok(Box::new(Live { scene: shown, frames: 0, out, done: finished }))
            }),
        )
        .map_err(said)?;
        let result = done.lock().map_err(said)?.take();
        result.unwrap_or_else(|| Err("the window closed before its picture was taken".to_string()))
    }

    /// A window's picture on a made-up desktop: the wallpaper (or the whole made-up desktop), a
    /// soft shadow, and the window with rounded corners, its top left at `at`.
    fn on_desktop(window: &image::RgbaImage, size: (u32, u32), at: (f32, f32), whole_desktop: bool) -> Result<image::RgbaImage, String> {
        let (w, h) = (window.width() as usize, window.height() as usize);
        let picture = egui::ColorImage::from_rgba_unmultiplied([w, h], window.as_raw());
        let mut texture: Option<egui::TextureHandle> = None;
        let mut harness = Harness::builder().with_size(vec2(size.0 as f32, size.1 as f32)).build_ui(move |ui| {
            let ctx = ui.ctx().clone();
            let p = ctx.layer_painter(egui::LayerId::background());
            let r = ctx.viewport_rect();
            if whole_desktop {
                paint_desktop(&p, &ctx, r);
            } else {
                paint_wallpaper(&p, r);
            }
            let tex = texture.get_or_insert_with(|| ctx.load_texture("window", picture.clone(), egui::TextureOptions::NEAREST));
            let win = egui::Rect::from_min_size(pos2(at.0, at.1), vec2(w as f32, h as f32));
            p.add(egui::epaint::Shadow { offset: [0, 16], blur: 48, spread: 2, color: Color32::from_black_alpha(120) }.as_shape(win, 10.0));
            p.add(
                egui::epaint::RectShape::filled(win, 10.0, Color32::WHITE)
                    .with_texture(tex.id(), egui::Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0))),
            );
            p.rect_stroke(win, 10.0, Stroke::new(1.0, Color32::from_white_alpha(46)), StrokeKind::Outside);
        });
        screengrabcraft_ui::install_fonts(&harness.ctx);
        harness.run_steps(3);
        harness.render()
    }

    /// Every scene, each in a process of its own (a window system is opened once a process), then
    /// each window put on the made-up desktop as the README shows it.
    pub fn all(dir: &Path, theme: Theme) -> Result<(), String> {
        let me = std::env::current_exe().map_err(said)?;
        let word = if theme == Theme::Light { "light" } else { "dark" };
        for scene in ["editor", "video", "overlay", "recorder"] {
            let status = std::process::Command::new(&me).arg(dir).arg(word).arg("live-one").arg(scene).status().map_err(said)?;
            if !status.success() {
                return Err(format!("the {scene} window: {status}"));
            }
        }
        let window = |scene: &str| image::open(dir.join(format!("window-{scene}.png"))).map(|i| i.to_rgba8()).map_err(said);
        let margin = 56u32;
        for (scene, name) in [("editor", "editor.png"), ("video", "video-editor.png")] {
            let w = window(scene)?;
            let size = (w.width() + 2 * margin, w.height() + 2 * margin);
            on_desktop(&w, size, (margin as f32, margin as f32 - 16.0), false)?.save(dir.join(name)).map_err(said)?;
        }
        // The overlay is over the whole screen: its window is the picture.
        window("overlay")?.save(dir.join("overlay.png")).map_err(said)?;
        // The Recorder sits over the desktop it is about to record.
        let r = window("recorder")?;
        let at = (((1280 - r.width().min(1280)) / 2) as f32, (800u32.saturating_sub(44 + r.height() + 36)) as f32);
        on_desktop(&r, (1280, 800), at, true)?.save(dir.join("recorder.png")).map_err(said)?;
        for scene in ["editor", "video", "overlay", "recorder"] {
            let _ = std::fs::remove_file(dir.join(format!("window-{scene}.png")));
        }
        println!("wrote editor.png, video-editor.png, overlay.png and recorder.png in {}", dir.display());
        Ok(())
    }
}
