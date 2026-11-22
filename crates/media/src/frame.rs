//! Frames round a picture in the video: the recording's own screen made smaller on a background
//! colour, its corners rounded, with a border and a shadow ([`ScreenFrame`]); and devices drawn
//! round the screen or an item ([`Device`]): a plain laptop, a phone and a browser window. The
//! devices are simple shapes drawn here in code, no maker's likeness or mark.

use screengrabcraft_model::{Color, Raster, Rect};
use screengrabcraft_render::{canvas, draw};
use serde::{Deserialize, Serialize};
use tiny_skia::{FillRule, Paint, PathBuilder, Pixmap, Stroke, Transform};

/// A device drawn round a picture.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Device {
    #[default]
    None,
    /// A laptop: a dark lid round the picture with a camera dot, a light base under it.
    Laptop,
    /// A phone: a dark body with round corners, turned the way the picture is.
    Phone,
    /// A browser window: a bar with an address field above the picture.
    Browser,
}

impl Device {
    pub const ALL: [Device; 4] = [Device::None, Device::Laptop, Device::Phone, Device::Browser];

    pub fn label(self) -> &'static str {
        match self {
            Device::None => "No device",
            Device::Laptop => "Laptop",
            Device::Phone => "Phone",
            Device::Browser => "Browser window",
        }
    }

    /// How far it reaches past a `w × h` picture inside it: left, top, right, bottom.
    pub fn reach(self, w: f32, h: f32) -> [f32; 4] {
        let s = w.min(h);
        match self {
            Device::None => [0.0; 4],
            Device::Laptop => {
                let (side, top, chin, base) = (w * 0.03, w * 0.04, w * 0.04, w * 0.028);
                // The base is wider than the lid.
                let over = (w + 2.0 * side) * 0.07;
                [over, top, over, chin + base]
            }
            Device::Phone => {
                let (thin, thick, button) = (s * 0.045, s * 0.1, s * 0.012);
                if w >= h { [thick, thin + button, thick, thin] } else { [thin + button, thick, thin, thick] }
            }
            Device::Browser => {
                let (bar, edge) = (w * 0.055, w * 0.008);
                [edge, bar, edge, edge]
            }
        }
    }

    /// How round the picture's corners are inside it (a share of its shorter side).
    pub fn screen_radius(self) -> f32 {
        match self {
            Device::Phone => 0.07,
            _ => 0.0,
        }
    }

    /// The outside of the device round `inner`.
    pub fn outer(self, inner: Rect) -> Rect {
        let [l, t, r, b] = self.reach(inner.w, inner.h);
        Rect::new(inner.x - l, inner.y - t, inner.w + l + r, inner.h + t + b)
    }

    /// The largest picture `aspect` times as wide as it's tall that fits in `area` with the device
    /// round it, the whole in the middle of `area`.
    pub fn fit(self, aspect: f32, area: Rect) -> Rect {
        let aspect = if aspect.is_finite() && aspect > 0.01 { aspect.min(100.0) } else { 1.0 };
        // Its reach grows with the picture: work it out for a picture 1 wide, then scale.
        let (uw, uh) = (1.0, 1.0 / aspect);
        let [l, t, r, b] = self.reach(uw, uh);
        let (ow, oh) = (uw + l + r, uh + t + b);
        let k = (area.w / ow).min(area.h / oh).max(0.0);
        let (x, y) = (area.x + (area.w - ow * k) / 2.0, area.y + (area.h - oh * k) / 2.0);
        Rect::new(x + l * k, y + t * k, uw * k, uh * k)
    }
}

fn paint(c: Color, alpha: f32) -> Paint<'static> {
    let mut p = Paint::default();
    p.set_color(draw::skia_color(c, alpha));
    p.anti_alias = true;
    p
}

fn rgb(hex: u32) -> Color {
    Color::rgb((hex >> 16) as u8, (hex >> 8) as u8, hex as u8)
}

fn fill(out: &mut Pixmap, path: Option<tiny_skia::Path>, c: Color, alpha: f32, to: Transform) {
    if let Some(p) = path {
        out.fill_path(&p, &paint(c, alpha), FillRule::Winding, to, None);
    }
}

fn round(r: Rect, radius: f32) -> Option<tiny_skia::Path> {
    draw::rounded_rect(r, radius)
}

fn dot(x: f32, y: f32, r: f32) -> Option<tiny_skia::Path> {
    PathBuilder::from_circle(x, y, r.max(0.3))
}

/// Draws `device` round `inner` (where the picture goes, drawn over it afterwards) onto `out`, its
/// space put on the picture by `to`, `alpha` see-through.
pub fn draw_device(out: &mut Pixmap, device: Device, inner: Rect, to: Transform, alpha: f32) {
    let (w, h) = (inner.w, inner.h);
    if !(w > 1.0 && h > 1.0 && w.is_finite() && h.is_finite()) || alpha <= 0.0 {
        return;
    }
    let s = w.min(h);
    let (cx, cy) = (inner.x + w / 2.0, inner.y + h / 2.0);
    let line = |out: &mut Pixmap, path: Option<tiny_skia::Path>, c: Color, width: f32| {
        if let Some(p) = path {
            out.stroke_path(&p, &paint(c, alpha), &Stroke { width: width.max(0.5), ..Stroke::default() }, to, None);
        }
    };
    match device {
        Device::None => {}
        Device::Laptop => {
            let (side, top, chin, base_h) = (w * 0.03, w * 0.04, w * 0.04, w * 0.028);
            let lid = Rect::new(inner.x - side, inner.y - top, w + 2.0 * side, h + top + chin);
            fill(out, round(lid, w * 0.022), rgb(0x20_2227), alpha, to);
            line(out, round(lid, w * 0.022), rgb(0x46_4950), w * 0.002);
            // The camera, in the middle of the lid's top.
            fill(out, dot(cx, inner.y - top / 2.0, w * 0.006), rgb(0x0b_0c0e), alpha, to);
            fill(out, dot(cx, inner.y - top / 2.0, w * 0.0025), rgb(0x2e_3440), alpha, to);
            // The base: light, a darker lip under it, a groove for opening the lid.
            let bw = lid.w * 1.14;
            let base = Rect::new(cx - bw / 2.0, lid.y + lid.h, bw, base_h);
            fill(out, round(base, base_h * 0.5), rgb(0x9d_a2aa), alpha, to);
            fill(out, round(Rect::new(base.x, base.y, base.w, base_h * 0.6), base_h * 0.25), rgb(0xc8_ccd3), alpha, to);
            fill(out, round(Rect::new(cx - w * 0.07, base.y, w * 0.14, base_h * 0.32), base_h * 0.16), rgb(0xa9_aeb6), alpha, to);
        }
        Device::Phone => {
            let (thin, thick, button) = (s * 0.045, s * 0.1, s * 0.012);
            let wide = w >= h;
            let (l, t, r, b) = if wide { (thick, thin, thick, thin) } else { (thin, thick, thin, thick) };
            let body = Rect::new(inner.x - l, inner.y - t, w + l + r, h + t + b);
            // Two buttons along its top (turned on its side) or its left.
            for k in 0..2 {
                let along = 0.22 + k as f32 * 0.16;
                let r = if wide {
                    Rect::new(body.x + body.w * along, body.y - button, body.w * 0.1, button * 2.0)
                } else {
                    Rect::new(body.x - button, body.y + body.h * along, button * 2.0, body.h * 0.1)
                };
                fill(out, round(r, button * 0.8), rgb(0x2a_2c31), alpha, to);
            }
            let radius = body.w.min(body.h) * 0.16;
            fill(out, round(body, radius), rgb(0x15_161a), alpha, to);
            line(out, round(body, radius), rgb(0x3a_3c42), s * 0.006);
            // A speaker slot and a camera dot at its top (at its left, turned on its side).
            let (sx, sy) = if wide { (body.x + l / 2.0, cy) } else { (cx, body.y + t / 2.0) };
            let slot = if wide {
                Rect::new(sx - s * 0.008, sy - s * 0.07, s * 0.016, s * 0.14)
            } else {
                Rect::new(sx - s * 0.07, sy - s * 0.008, s * 0.14, s * 0.016)
            };
            fill(out, round(slot, s * 0.008), rgb(0x30_3238), alpha, to);
            let (dx, dy) = if wide { (sx, sy + s * 0.12) } else { (sx + s * 0.12, sy) };
            fill(out, dot(dx, dy, s * 0.014), rgb(0x0a_0b0d), alpha, to);
        }
        Device::Browser => {
            let (bar, edge) = (w * 0.055, w * 0.008);
            let window = Rect::new(inner.x - edge, inner.y - bar, w + 2.0 * edge, h + bar + edge);
            fill(out, round(window, edge), rgb(0xe9_ebef), alpha, to);
            line(out, round(window, edge), rgb(0xc4_c8cf), w * 0.0015);
            let mid = window.y + bar / 2.0;
            // Three plain grey dots at its left.
            for k in 0..3 {
                fill(out, dot(window.x + bar * 0.5 + k as f32 * bar * 0.42, mid, bar * 0.12), rgb(0xb4_b9c1), alpha, to);
            }
            // The address field, a line of grey words in it.
            let field = Rect::new(window.x + bar * 2.1, mid - bar * 0.27, (window.w - bar * 3.4).max(bar), bar * 0.54);
            fill(out, round(field, field.h / 2.0), rgb(0xff_ffff), alpha, to);
            line(out, round(field, field.h / 2.0), rgb(0xd0_d4da), w * 0.0012);
            fill(out, round(Rect::new(field.x + field.h * 0.6, mid - bar * 0.06, field.w * 0.24, bar * 0.12), bar * 0.06), rgb(0xc2_c6cd), alpha, to);
            // A menu at its right: three small dots, one over another.
            for k in -1..=1 {
                fill(out, dot(window.x + window.w - bar * 0.6, mid + k as f32 * bar * 0.15, bar * 0.045), rgb(0x8f_959e), alpha, to);
            }
        }
    }
}

/// The recording's screen framed: smaller in the video on a background colour, its corners
/// rounded, a border round it, a soft shadow under it, a device round it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ScreenFrame {
    /// How much of the video it takes, across and down (0.5 to 1).
    pub size: f32,
    /// The colour round it (`#rrggbb`).
    pub background: String,
    /// Its corners rounded: the radius as a share of its shorter side (0 to 0.2).
    pub radius: f32,
    /// A border round it as wide as this share of its width (0 none, to 0.05)...
    pub border: f32,
    /// ...of this colour.
    pub border_colour: String,
    pub shadow: bool,
    pub device: Device,
}

impl Default for ScreenFrame {
    fn default() -> ScreenFrame {
        ScreenFrame {
            size: 1.0,
            background: "#1e2230".to_string(),
            radius: 0.0,
            border: 0.0,
            border_colour: "#ffffff".to_string(),
            shadow: false,
            device: Device::None,
        }
    }
}

/// The smallest the screen is made (a share of the video).
pub const MIN_SIZE: f32 = 0.5;

impl ScreenFrame {
    /// Every value in its range, the colours `#rrggbb`.
    pub fn checked(self) -> ScreenFrame {
        let f = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let hex = |s: &str, or: [u8; 3]| {
            let [r, g, b] = crate::layers::color(s, or);
            format!("#{r:02x}{g:02x}{b:02x}")
        };
        ScreenFrame {
            size: f(self.size, MIN_SIZE, 1.0, 1.0),
            background: hex(&self.background, [0x1e, 0x22, 0x30]),
            radius: f(self.radius, 0.0, 0.2, 0.0),
            border: f(self.border, 0.0, 0.05, 0.0),
            border_colour: hex(&self.border_colour, [0xff, 0xff, 0xff]),
            shadow: self.shadow,
            device: self.device,
        }
    }

    /// The screen fills the video as recorded.
    pub fn is_plain(&self) -> bool {
        let c = self.clone().checked();
        c.size >= 0.999 && c.radius <= 0.0 && c.border <= 0.0 && !c.shadow && c.device == Device::None
    }

    /// Where the screen goes in a `w × h` video.
    pub fn screen_rect(&self, w: f32, h: f32) -> Rect {
        let c = self.clone().checked();
        let (aw, ah) = (w * c.size, h * c.size);
        c.device.fit(w / h.max(1.0), Rect::new((w - aw) / 2.0, (h - ah) / 2.0, aw, ah))
    }
}

/// A soft shadow under `r` (rounded `radius`): rounded boxes below and to the right, each larger
/// and fainter.
pub(crate) fn shadow(out: &mut Pixmap, r: Rect, radius: f32, to: Transform, alpha: f32) {
    let off = (r.w.min(r.h) * 0.04).max(2.0);
    for step in 0..4 {
        let grow = off * step as f32 * 0.5;
        let b = Rect::new(r.x - grow + off, r.y - grow + off, r.w + 2.0 * grow, r.h + 2.0 * grow);
        fill(out, round(b, radius + grow), Color::BLACK, 0.12 * alpha, to);
    }
}

/// The screen's picture `pic` (the video's size) framed as `frame` says: a new picture of the same
/// size, the background round it. As it was if the frame is plain, or it can't be drawn.
pub fn frame_screen(pic: Raster, frame: &ScreenFrame) -> Raster {
    if frame.is_plain() {
        return pic;
    }
    let f = frame.clone().checked();
    let (w, h) = (pic.width() as f32, pic.height() as f32);
    let (Some(src), Some(mut out)) = (canvas::to_pixmap(&pic), Pixmap::new(pic.width(), pic.height())) else { return pic };
    let [br, bg, bb] = crate::layers::color(&f.background, [0x1e, 0x22, 0x30]);
    out.fill(tiny_skia::Color::from_rgba8(br, bg, bb, 255));
    let inner = f.screen_rect(w, h);
    if inner.w < 2.0 || inner.h < 2.0 {
        return pic;
    }
    let to = Transform::identity();
    let radius = (f.radius.max(f.device.screen_radius())) * inner.w.min(inner.h);
    if f.shadow {
        let body = f.device.outer(inner);
        let r = if f.device == Device::None { radius } else { body.w.min(body.h) * 0.04 };
        shadow(&mut out, body, r, to, 1.0);
    }
    draw_device(&mut out, f.device, inner, to, 1.0);
    let Some(shape) = round(inner, radius) else { return pic };
    let place = Transform::from_scale(inner.w / w, inner.h / h).post_translate(inner.x, inner.y);
    let fill_pic = Paint {
        shader: tiny_skia::Pattern::new(src.as_ref(), tiny_skia::SpreadMode::Pad, tiny_skia::FilterQuality::Bilinear, 1.0, place),
        anti_alias: true,
        ..Paint::default()
    };
    out.fill_path(&shape, &fill_pic, FillRule::Winding, to, None);
    if f.border > 0.0 {
        let [r, g, b] = crate::layers::color(&f.border_colour, [0xff, 0xff, 0xff]);
        let stroke = Stroke { width: (f.border * inner.w).max(1.0), ..Stroke::default() };
        out.stroke_path(&shape, &paint(Color::rgb(r, g, b), 1.0), &stroke, to, None);
    }
    canvas::from_pixmap(&out).unwrap_or(pic)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checks(w: u32, h: u32) -> Raster {
        Raster::filled(w, h, Color::rgb(250, 250, 250)).unwrap()
    }

    #[test]
    fn a_device_fits_round_a_picture_in_the_room_given() {
        let area = Rect::new(0.0, 0.0, 1600.0, 900.0);
        for d in Device::ALL {
            let inner = d.fit(16.0 / 9.0, area);
            let outer = d.outer(inner);
            assert!((inner.w / inner.h - 16.0 / 9.0).abs() < 1e-3, "{d:?} keeps its shape");
            assert!(outer.x >= -0.5 && outer.y >= -0.5 && outer.x + outer.w <= 1600.5 && outer.y + outer.h <= 900.5, "{d:?}: {outer:?}");
            assert!(outer.w > 1599.0 || outer.h > 899.0, "{d:?} as large as fits: {outer:?}");
        }
        assert_eq!(Device::None.fit(16.0 / 9.0, area), area);
        // A phone stands up for a tall picture: its thick ends at the top and bottom.
        let [l, t, ..] = Device::Phone.reach(90.0, 160.0);
        assert!(t > l);
        // Junk shapes and room stay sensible.
        for (a, r) in [(f32::NAN, area), (0.0, area), (f32::INFINITY, Rect::new(0.0, 0.0, 0.0, 0.0))] {
            let i = Device::Laptop.fit(a, r);
            assert!(i.w.is_finite() && i.h.is_finite() && i.w >= 0.0, "{i:?}");
        }
    }

    #[test]
    fn every_device_draws_round_the_picture() {
        for d in Device::ALL {
            let mut out = Pixmap::new(200, 150).unwrap();
            let inner = Rect::new(50.0, 40.0, 100.0, 60.0);
            draw_device(&mut out, d, inner, Transform::identity(), 1.0);
            let alpha = |x: u32, y: u32| out.pixel(x, y).map(|p| p.alpha()).unwrap_or(0);
            let outer = d.outer(inner);
            if d == Device::None {
                assert!(out.data().iter().all(|v| *v == 0));
            } else {
                // Something drawn just outside the picture, above and below it.
                assert!(alpha(100, (outer.y + 1.5) as u32) > 0 || alpha(100, 38) > 0, "{d:?} above");
                assert!(alpha(100, 101) > 0, "{d:?} below");
            }
        }
    }

    #[test]
    fn the_screen_is_made_smaller_on_its_background_rounded_and_bordered() {
        let pic = checks(160, 90);
        assert_eq!(frame_screen(pic.clone(), &ScreenFrame::default()), pic, "plain: as it was");
        let f = ScreenFrame {
            size: 0.8,
            background: "#ff0000".into(),
            radius: 0.2,
            border: 0.02,
            border_colour: "#0000ff".into(),
            ..ScreenFrame::default()
        };
        let out = frame_screen(pic.clone(), &f);
        let px = |x, y| out.pixel(x, y).unwrap();
        assert_eq!(px(2, 2), Color::rgb(255, 0, 0), "the background round it");
        assert_eq!(px(80, 45), Color::rgb(250, 250, 250), "the screen in the middle");
        let r = f.screen_rect(160.0, 90.0);
        assert!((r.w - 128.0).abs() < 0.01 && (r.x - 16.0).abs() < 0.01, "{r:?}");
        assert!(px(17, 10) == Color::rgb(255, 0, 0), "a round corner leaves the background: {:?}", px(17, 10));
        assert!(px(80, 9).b > 200 && px(80, 9).r < 60, "the border along its top: {:?}", px(80, 9));
        // A laptop round it: the lid dark round the screen, the background beyond.
        let f = ScreenFrame { device: Device::Laptop, shadow: true, ..ScreenFrame::default() };
        let out = frame_screen(pic.clone(), &f);
        let r = f.screen_rect(160.0, 90.0);
        assert!(r.w < 160.0 * 0.9);
        let lid = out.pixel((r.x - 1.5) as u32, 45).unwrap();
        assert!(lid.r < 60 && lid.g < 60, "the lid: {lid:?}");
        assert_eq!(out.pixel(80, 45).unwrap(), Color::rgb(250, 250, 250));
        // Junk is made sensible.
        let wild = ScreenFrame {
            size: f32::NAN,
            background: "nope".into(),
            radius: 9.0,
            border: -1.0,
            border_colour: String::new(),
            shadow: true,
            device: Device::Phone,
        }
        .checked();
        assert_eq!((wild.size, wild.background.as_str(), wild.radius, wild.border), (1.0, "#1e2230", 0.2, 0.0));
        let _ = frame_screen(pic, &wild);
        let _ = frame_screen(Raster::single(Color::BLACK), &wild);
    }
}
