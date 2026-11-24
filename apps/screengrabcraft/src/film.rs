//! The countdown before a recording, drawn as the leader of an old film: a big number in two
//! circles, cross lines, and a hand sweeping round once a second, darkening what it has passed,
//! over a grainy sepia picture that flickers and has a few scratches and specks of dust. The
//! picture sits on a strip of film with its sprocket holes; what's coming and Cancel are under it.
//!
//! Everything is drawn here with egui's painter: no picture is loaded. It's light on the computer
//! (a few hundred flat shapes, at film's 24 pictures a second, for the seconds before a
//! recording), and each picture's grain and scratches come from its number, so the same moment
//! always looks the same (and can be tested).

use std::f32::consts::TAU;

use egui::{Align2, Color32, CornerRadius, FontId, Mesh, Painter, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2, pos2, vec2};
use screengrabcraft_ui::widgets;

/// The countdown window's size.
pub const SIZE: Vec2 = vec2(296.0, 266.0);
/// How often the picture changes: film's 24 pictures a second.
pub const FRAME_RATE: f32 = 24.0;
/// How long each picture stays up.
pub const FRAME_TIME: std::time::Duration = std::time::Duration::from_micros(41_667);

/// The film round the picture, and its window.
pub const BLACK: Color32 = Color32::from_rgb(14, 12, 10);
/// The sprocket holes down the strip's edges.
const HOLE: Color32 = Color32::from_rgb(70, 63, 53);
/// The picture: warm grey, cream circles, dark ink.
const SEPIA: Color32 = Color32::from_rgb(158, 146, 124);
const CREAM: Color32 = Color32::from_rgb(238, 230, 212);
const INK: Color32 = Color32::from_rgb(26, 22, 17);
/// What's coming, under the picture.
const CAPTION: Color32 = Color32::from_rgb(206, 196, 176);

/// The strip's edges, with the sprocket holes, either side of the picture.
const EDGE: f32 = 24.0;
/// Between the top of the strip and the picture.
const TOP: f32 = 10.0;
/// The picture's rounded corners, as a projector shows them.
const CORNER: u8 = 10;
/// Specks of grain on each picture.
const GRAIN: u32 = 160;

/// The number to show for `remaining` seconds, and how far round the hand is in that second
/// (0 at the top, 1 all the way round). Nonsense times show 1 with the hand all the way round.
pub fn hand(remaining: f32) -> (u32, f32) {
    let remaining = if remaining.is_finite() { remaining.max(0.0) } else { 0.0 };
    // `as` saturates: a huge time shows the largest number rather than failing.
    let left = (remaining.ceil() as u32).max(1);
    (left, (left as f32 - remaining).clamp(0.0, 1.0))
}

/// Which picture of the film `remaining` seconds is (it changes 24 times a second).
pub fn picture_number(remaining: f32) -> u32 {
    if remaining.is_finite() { (remaining.max(0.0) * FRAME_RATE) as u32 } else { 0 }
}

/// The countdown's face for `remaining` seconds: the film, what's coming, and Cancel. True when
/// Cancel was pressed.
pub fn countdown_ui(ui: &mut Ui, remaining: f32, coming: &str) -> bool {
    let (left, sweep) = hand(remaining);
    let whole = ui.max_rect();
    let width = (whole.width() - 2.0 * EDGE).max(1.0);
    let picture = Rect::from_min_size(pos2(whole.min.x + EDGE, whole.min.y + TOP), vec2(width, (width * 0.75).round()));

    let p = ui.painter().clone();
    p.rect_filled(whole, 0.0, BLACK);
    sprocket_holes(&p, whole);
    // The number is what the countdown says: it's named for the screen reader (and the tests).
    ui.allocate_rect(picture, Sense::hover()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, left.to_string()));
    paint_picture(&p.with_clip_rect(picture), picture, left, sweep, picture_number(remaining));
    // Round the picture's corners, as the projector's gate does.
    p.rect_stroke(picture, CornerRadius::same(CORNER), Stroke::new(6.0, BLACK), StrokeKind::Outside);

    let caption = Rect::from_center_size(pos2(whole.center().x, picture.max.y + 16.0), vec2(width, 18.0));
    ui.allocate_rect(caption, Sense::hover()).widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Label, true, coming));
    p.text(caption.center(), Align2::CENTER_CENTER, coming, FontId::proportional(13.5), CAPTION);

    let button = Rect::from_center_size(pos2(whole.center().x, caption.max.y + 22.0), vec2(widgets::BUTTON_SIZE.x, 28.0));
    film_button(ui, button, "Cancel")
}

/// A button drawn in the film's colours (cream on black), whatever the light or dark look.
fn film_button(ui: &mut Ui, rect: Rect, label: &str) -> bool {
    let resp = ui.interact(rect, ui.id().with(("film-button", label)), Sense::click());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::WidgetType::Button, true, label));
    let fill = if resp.is_pointer_button_down_on() {
        Color32::from_rgb(10, 9, 7)
    } else if resp.hovered() {
        Color32::from_rgb(58, 52, 44)
    } else {
        Color32::from_rgb(34, 30, 25)
    };
    let p = ui.painter();
    p.rect(rect, CornerRadius::same(4), fill, Stroke::new(1.0, Color32::from_rgb(120, 112, 98)), StrokeKind::Inside);
    p.text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(13.5), CREAM);
    resp.clicked()
}

/// The holes down both edges of the strip.
fn sprocket_holes(p: &Painter, whole: Rect) {
    const PITCH: f32 = 22.0;
    let count = ((whole.height() / PITCH).floor() as u32).clamp(1, 64);
    let first = whole.min.y + (whole.height() - (count - 1) as f32 * PITCH) / 2.0;
    for n in 0..count {
        let y = first + n as f32 * PITCH;
        for x in [whole.min.x + EDGE / 2.0, whole.max.x - EDGE / 2.0] {
            p.rect_filled(Rect::from_center_size(pos2(x, y), vec2(10.0, 13.0)), 2.5, HOLE);
        }
    }
}

/// One picture of the leader in `r`: the number `left`, the hand `sweep` of the way round, and
/// picture `frame`'s grain, dust, scratches and flicker.
fn paint_picture(p: &Painter, r: Rect, left: u32, sweep: f32, frame: u32) {
    let c = r.center();
    // Far enough to reach past the corners.
    let reach = r.size().length();
    let angle = sweep * TAU;
    p.rect_filled(r, 0.0, SEPIA);
    // What the hand has passed is darker.
    p.add(Shape::mesh(wedge(c, reach, angle, Color32::from_black_alpha(84))));
    p.add(Shape::mesh(vignette(r)));
    let ink = Stroke::new(1.5, INK);
    p.line_segment([pos2(r.min.x, c.y), pos2(r.max.x, c.y)], ink);
    p.line_segment([pos2(c.x, r.min.y), pos2(c.x, r.max.y)], ink);
    let radius = r.height() * 0.4;
    p.circle_stroke(c, radius, Stroke::new(3.0, CREAM));
    p.circle_stroke(c, radius - 11.0, Stroke::new(1.5, CREAM));
    p.line_segment([c, c + vec2(angle.sin(), -angle.cos()) * reach], Stroke::new(2.0, INK));
    // A figure's middle sits a little above its line's: nudged down to sit in the circles.
    let size = (r.height() * 0.52).round();
    p.text(c + vec2(0.0, size * 0.04), Align2::CENTER_CENTER, left.to_string(), widgets::strong_font(p.ctx(), size), INK);
    grain(p, r, frame);
    scratches(p, r, frame);
    // Flicker: each picture a touch lighter or darker than the last.
    let f = unit(frame, 1);
    let flicker = if f < 0.5 { Color32::from_black_alpha((f * 2.0 * 26.0) as u8) } else { Color32::from_white_alpha(((f - 0.5) * 2.0 * 16.0) as u8) };
    p.rect_filled(r, 0.0, flicker);
}

/// The part of the picture the hand has swept, from the top round to `angle` (clockwise).
fn wedge(c: Pos2, reach: f32, angle: f32, color: Color32) -> Mesh {
    let mut m = Mesh::default();
    if angle.is_nan() || angle <= 0.0 {
        return m;
    }
    let steps = ((angle / TAU) * 64.0).ceil().clamp(1.0, 64.0) as u32;
    m.colored_vertex(c, color);
    for i in 0..=steps {
        let a = angle.min(TAU) * i as f32 / steps as f32;
        m.colored_vertex(c + vec2(a.sin(), -a.cos()) * reach, color);
        if i > 0 {
            m.add_triangle(0, i, i + 1);
        }
    }
    m
}

/// Darker towards the edges and darkest in the corners, as an old projector's lamp lights it.
fn vignette(r: Rect) -> Mesh {
    const STEPS: u32 = 48;
    let (c, half) = (r.center(), r.size() / 2.0);
    let mut m = Mesh::default();
    for i in 0..STEPS {
        let a = TAU * i as f32 / STEPS as f32;
        let d = vec2(a.cos() * half.x, a.sin() * half.y);
        m.colored_vertex(c + d * 0.55, Color32::TRANSPARENT);
        m.colored_vertex(c + d * 1.5, Color32::from_black_alpha(210));
    }
    for i in 0..STEPS {
        let (a, b) = (2 * i, 2 * ((i + 1) % STEPS));
        m.add_triangle(a, a + 1, b);
        m.add_triangle(b, a + 1, b + 1);
    }
    m
}

/// The film's grain, and now and then a speck of dust or a hair.
fn grain(p: &Painter, r: Rect, frame: u32) {
    let at = |salt: u32| pos2(r.min.x + unit(frame, salt) * r.width(), r.min.y + unit(frame, salt + 1) * r.height());
    for i in 0..GRAIN {
        let s = 16 + i * 4;
        let side = 1.0 + unit(frame, s + 2) * 1.2;
        let alpha = (24.0 + unit(frame, s + 3) * 72.0) as u8;
        let color =
            if i % 2 == 0 { Color32::from_rgba_unmultiplied(CREAM.r(), CREAM.g(), CREAM.b(), alpha) } else { Color32::from_black_alpha(alpha) };
        p.rect_filled(Rect::from_min_size(at(s), vec2(side, side)), 0.0, color);
    }
    for k in 0..3 {
        if unit(frame, 1000 + k) < 0.3 {
            p.circle_filled(at(1010 + 2 * k), 1.2 + unit(frame, 1020 + k) * 2.0, Color32::from_black_alpha(150));
        }
    }
    if unit(frame, 1100) < 0.1 {
        let start = at(1101);
        let bend = |n: u32| vec2((unit(frame, 1110 + n) - 0.5) * 16.0, (unit(frame, 1120 + n) - 0.5) * 16.0);
        let points = vec![start, start + bend(0), start + bend(0) + bend(1), start + bend(0) + bend(1) + bend(2)];
        p.add(Shape::line(points, Stroke::new(1.0, Color32::from_black_alpha(140))));
    }
}

/// A light scratch and a dark one down the picture, each staying a few pictures in about the same
/// place before it moves on.
fn scratches(p: &Painter, r: Rect, frame: u32) {
    let run = frame / 8;
    for k in 0..2 {
        if unit(run, 2000 + k) < 0.4 {
            continue;
        }
        let x = r.min.x + r.width() * (0.08 + 0.84 * unit(run, 2010 + k)) + (unit(frame, 2020 + k) - 0.5) * 2.0;
        let top = r.min.y + unit(run, 2030 + k) * r.height() * 0.35;
        let bottom = r.max.y - unit(run, 2040 + k) * r.height() * 0.35;
        let color = if k == 0 { Color32::from_rgba_unmultiplied(CREAM.r(), CREAM.g(), CREAM.b(), 120) } else { Color32::from_black_alpha(110) };
        p.line_segment([pos2(x, top), pos2(x, bottom)], Stroke::new(1.0, color));
    }
}

/// A number from 0 up to 1 that looks random, the same every time for the same picture and use.
fn unit(frame: u32, salt: u32) -> f32 {
    let mut x = frame.wrapping_mul(0x9E37_79B9) ^ salt.wrapping_mul(0x85EB_CA6B);
    x ^= x >> 16;
    x = x.wrapping_mul(0x7FEB_352D);
    x ^= x >> 15;
    x = x.wrapping_mul(0x846C_A68B);
    x ^= x >> 16;
    (x >> 8) as f32 / 16_777_216.0
}

#[cfg(test)]
mod tests {
    use egui_kittest::Harness;
    use egui_kittest::kittest::Queryable as _;
    use screengrabcraft_model::{Raster, Theme};

    use super::*;

    /// Where the harness puts its contents.
    const ROOT: f32 = 8.0;

    /// The countdown drawn headless the way the service draws it, at `remaining` seconds (which
    /// the test can change), and whether Cancel was pressed.
    fn film_harness(theme: Theme, remaining: f32) -> Harness<'static, (f32, bool)> {
        let h = Harness::builder().with_size(SIZE + vec2(2.0 * ROOT, 2.0 * ROOT)).build_ui_state(
            move |ui, (remaining, cancelled): &mut (f32, bool)| {
                screengrabcraft_ui::theme::apply(ui.ctx(), theme);
                egui::CentralPanel::default().frame(egui::Frame::NONE.fill(BLACK)).show(ui, |ui| {
                    if countdown_ui(ui, *remaining, "Recording starts…") {
                        *cancelled = true;
                    }
                });
            },
            (remaining, false),
        );
        // The service's fonts, as it has them (the number is in the bold one).
        screengrabcraft_ui::install_fonts(&h.ctx);
        h
    }

    #[test]
    fn the_hand_goes_round_once_a_second() {
        assert_eq!(hand(3.0), (3, 0.0));
        assert_eq!(hand(2.75), (3, 0.25));
        assert_eq!(hand(2.5), (3, 0.5));
        assert_eq!(hand(2.0), (2, 0.0));
        assert_eq!(hand(0.25), (1, 0.75));
        // Nonsense never fails: 1, all the way round.
        for t in [0.0, -4.0, f32::NAN, f32::NEG_INFINITY, f32::INFINITY] {
            assert_eq!(hand(t), (1, 1.0), "{t}");
        }
        assert_eq!(hand(1e12).0, u32::MAX);
        // 24 pictures a second.
        assert_eq!(picture_number(1.0), 24);
        assert_ne!(picture_number(1.0), picture_number(1.0 - 1.0 / FRAME_RATE));
        assert_eq!(picture_number(f32::NAN), 0);
        assert_eq!((1.0 / FRAME_TIME.as_secs_f32()).round(), FRAME_RATE);
    }

    #[test]
    fn each_picture_has_its_own_grain_and_the_same_picture_the_same() {
        for salt in 0..500 {
            for frame in [0, 1, 71, u32::MAX] {
                let u = unit(frame, salt);
                assert!((0.0..1.0).contains(&u), "{frame} {salt}: {u}");
                assert_eq!(u.to_bits(), unit(frame, salt).to_bits());
            }
        }
        let differ = (0..GRAIN).filter(|&s| unit(5, s) != unit(6, s)).count();
        assert!(differ as u32 > GRAIN * 9 / 10, "{differ}");
    }

    #[test]
    fn the_swept_part_is_a_closed_fan_and_nothing_is_drawn_before_the_hand_moves() {
        assert!(wedge(pos2(0.0, 0.0), 10.0, 0.0, Color32::BLACK).is_empty());
        assert!(wedge(pos2(0.0, 0.0), 10.0, f32::NAN, Color32::BLACK).is_empty());
        let m = wedge(pos2(0.0, 0.0), 10.0, TAU, Color32::BLACK);
        assert!(m.is_valid());
        assert_eq!(m.indices.len() / 3, 64);
        assert!(vignette(Rect::from_min_size(pos2(0.0, 0.0), vec2(100.0, 75.0))).is_valid());
    }

    /// The number, what's coming and Cancel are there and work; Cancel is inside the window.
    #[test]
    fn the_film_countdown_says_the_seconds_and_can_be_cancelled() {
        for theme in [Theme::Light, Theme::Dark] {
            let mut h = film_harness(theme, 2.4);
            h.run_steps(2);
            h.get_by_label("3");
            h.get_by_label("Recording starts…");
            let r = h.get_by_label("Cancel").rect();
            assert!(r.max.y <= SIZE.y + ROOT && r.min.x >= ROOT && r.max.x <= SIZE.x + ROOT, "{r:?}");
            // The seconds go down.
            h.state_mut().0 = 1.6;
            h.run_steps(1);
            h.get_by_label("2");
            h.get_by_label("Cancel").click();
            h.run_steps(1);
            assert!(h.state().1);
        }
    }

    /// Odd times are drawn without failing.
    #[test]
    fn odd_times_never_fail() {
        for t in [0.0, -1.0, 1e-9, 1e12, f32::NAN, f32::INFINITY, f32::MIN] {
            let mut h = film_harness(Theme::Dark, t);
            h.run_steps(1);
            assert!(!h.state().1);
        }
    }

    /// Halfway through a second the hand has swept the right half (from the top, clockwise),
    /// which is darker than the left half; and the picture is sepia, warmer than grey. When the
    /// harness can't draw (no graphics), there's nothing to look at.
    #[test]
    fn the_hand_sweeps_clockwise_over_a_sepia_picture() {
        let mut h = film_harness(Theme::Light, 2.5);
        h.run_steps(2);
        let Ok(img) = h.render() else { return };
        let ppp = img.width() as f32 / (SIZE.x + 2.0 * ROOT);
        let width = SIZE.x - 2.0 * EDGE;
        let picture = Rect::from_min_size(pos2(ROOT + EDGE, ROOT + TOP), vec2(width, (width * 0.75).round()));
        let c = picture.center();
        // A patch of the picture, outside the circles, away from the cross lines, `dx` across.
        let mean = |dx: f32| {
            let (mut sum, mut n) = ([0.0f32; 3], 0.0f32);
            for y in (c.y - 40.0) as u32..(c.y - 10.0) as u32 {
                for x in (c.x + dx - 8.0) as u32..(c.x + dx + 8.0) as u32 {
                    if let Some(px) = img.get_pixel_checked((x as f32 * ppp) as u32, (y as f32 * ppp) as u32) {
                        for (s, v) in sum.iter_mut().zip(px.0) {
                            *s += f32::from(v);
                        }
                        n += 1.0;
                    }
                }
            }
            sum.map(|s| s / n.max(1.0))
        };
        let (left, right) = (mean(-96.0), mean(96.0));
        assert!(right[0] + 8.0 < left[0], "the swept right half {right:?} is darker than the left {left:?}");
        assert!(left[0] > left[2] + 15.0, "sepia: more red than blue {left:?}");
        if let Ok(dir) = std::env::var("SGC_SHOTS")
            && let Some(shot) = window_of(img.dimensions(), img.into_raw())
            && let Ok(png) = screengrabcraft_render::codec::encode(&shot, screengrabcraft_model::ImageFormat::Png, 100)
        {
            let _ = std::fs::write(format!("{dir}/record-countdown.png"), png);
        }
    }

    /// The countdown window's own part of a rendered harness (`size`, its RGBA `pixels`).
    fn window_of((w, h): (u32, u32), pixels: Vec<u8>) -> Option<Raster> {
        let ppp = w as f32 / (SIZE.x + 2.0 * ROOT);
        let at = (ROOT * ppp).round() as u32;
        Raster::new(w, h, pixels).ok()?.crop(at, at, (SIZE.x * ppp).round() as u32, (SIZE.y * ppp).round() as u32)
    }

    /// With `SGC_SHOTS` set: three seconds of the countdown at film's rate, as a GIF to look at.
    #[test]
    fn the_film_countdown_as_a_moving_picture() {
        let Ok(dir) = std::env::var("SGC_SHOTS") else { return };
        let mut h = film_harness(Theme::Dark, 3.0);
        h.run_steps(2);
        let Some(first) = h.render().ok().and_then(|img| window_of(img.dimensions(), img.into_raw())) else { return };
        let Ok(file) = std::fs::File::create(format!("{dir}/record-countdown.gif")) else { return };
        let Ok(mut gif) = screengrabcraft_render::anim::GifWriter::new(file, first.width(), first.height()) else { return };
        let delay = (1000.0 / FRAME_RATE).round() as u32;
        for n in 0..(3.0 * FRAME_RATE) as u32 {
            h.state_mut().0 = 3.0 - n as f32 / FRAME_RATE;
            h.run_steps(1);
            let Some(shot) = h.render().ok().and_then(|img| window_of(img.dimensions(), img.into_raw())) else { return };
            assert!(gif.push(shot, delay).is_ok());
        }
        assert!(gif.finish(delay).is_ok());
    }
}
