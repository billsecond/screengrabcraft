//! The arithmetic of the Linux and BSD backend that needs no display: turning X11 image bytes
//! into RGBA, cutting a large screen into requests the X server takes, reading the X window
//! properties and frame extents, decoding the portal's `file://` address and cutting its
//! picture of the whole desktop into monitors.
//!
//! Everything here reads bytes from another program (the X server, the portal), so nothing may
//! panic on them: lengths are checked, sums are checked or done in `i64`.
//!
//! Compiled everywhere for the tests; used only by the `unix` backend.
#![cfg_attr(not(all(unix, not(target_os = "macos"))), allow(dead_code))]

use screengrabcraft_model::IRect;

/// One colour channel of a pixel value: where its bits are.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Channel {
    shift: u32,
    bits: u32,
}

impl Channel {
    /// The channel of a contiguous, non-empty mask.
    fn of(mask: u32) -> Option<Channel> {
        if mask == 0 {
            return None;
        }
        let shift = mask.trailing_zeros();
        let bits = (mask >> shift).trailing_ones();
        // Contiguous: nothing set above the run of ones.
        (u64::from(mask) >> shift >> bits == 0).then_some(Channel { shift, bits })
    }

    /// The channel's value in a pixel, as 8 bits.
    fn get(self, pixel: u32) -> u8 {
        let max = (1u64 << self.bits) - 1;
        let v = (u64::from(pixel) >> self.shift) & max;
        let v8 = if self.bits >= 8 { v >> (self.bits - 8) } else { (v * 255 + max / 2) / max };
        u8::try_from(v8).unwrap_or(u8::MAX)
    }
}

/// How an X server lays out a TrueColor image in `ZPixmap` format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PixelFormat {
    bytes_per_pixel: usize,
    scanline_pad: u32,
    msb_first: bool,
    red: Channel,
    green: Channel,
    blue: Channel,
}

impl PixelFormat {
    /// The format of `bits_per_pixel` (16, 24 or 32) pixels with these channel masks, rows padded
    /// to `scanline_pad` bits (8, 16 or 32), multi-byte values most significant byte first when
    /// `msb_first`. Anything else is refused with a reason.
    pub fn new(bits_per_pixel: u8, scanline_pad: u8, msb_first: bool, red_mask: u32, green_mask: u32, blue_mask: u32) -> Result<PixelFormat, String> {
        let bytes_per_pixel = match bits_per_pixel {
            16 => 2,
            24 => 3,
            32 => 4,
            other => return Err(format!("{other} bits a pixel")),
        };
        if !matches!(scanline_pad, 8 | 16 | 32) {
            return Err(format!("rows padded to {scanline_pad} bits"));
        }
        let all = u64::from(red_mask) | u64::from(green_mask) | u64::from(blue_mask);
        let overlap = (red_mask & green_mask) | (red_mask & blue_mask) | (green_mask & blue_mask);
        if overlap != 0 || all >> bits_per_pixel != 0 {
            return Err(format!("colour masks {red_mask:#x}/{green_mask:#x}/{blue_mask:#x} that don't fit {bits_per_pixel}-bit pixels"));
        }
        let channel = |mask: u32| Channel::of(mask).ok_or_else(|| format!("colour mask {mask:#x}"));
        Ok(PixelFormat {
            bytes_per_pixel,
            scanline_pad: u32::from(scanline_pad),
            msb_first,
            red: channel(red_mask)?,
            green: channel(green_mask)?,
            blue: channel(blue_mask)?,
        })
    }

    /// The bytes of one row of `width` pixels, padding included.
    pub fn stride(&self, width: u32) -> Option<usize> {
        let bits = u64::from(width).checked_mul(self.bytes_per_pixel as u64 * 8)?;
        let pad = u64::from(self.scanline_pad);
        usize::try_from(bits.div_ceil(pad).checked_mul(pad / 8)?).ok()
    }

    fn value(&self, px: &[u8]) -> u32 {
        let fold_be = |acc: u32, b: &u8| (acc << 8) | u32::from(*b);
        if self.msb_first { px.iter().fold(0, fold_be) } else { px.iter().rev().fold(0, fold_be) }
    }

    /// Appends `rows` rows of `width` pixels from `data` (as the X server sent them) to `out` as
    /// opaque RGBA. Fails when `data` is too short.
    pub fn convert(&self, data: &[u8], width: u32, rows: u32, out: &mut Vec<u8>) -> Result<(), String> {
        let stride = self.stride(width).ok_or("the image is too wide")?;
        let used = (width as usize).checked_mul(self.bytes_per_pixel).ok_or("the image is too wide")?;
        let need = stride.checked_mul(rows as usize).ok_or("the image is too large")?;
        if data.len() < need {
            return Err(format!("the X server sent {} bytes for {width}×{rows} pixels (it needs {need})", data.len()));
        }
        // The usual layout (little-endian 0x00RRGGBB, bytes B G R x) goes the short way.
        let bgrx = self.bytes_per_pixel == 4
            && !self.msb_first
            && (self.red, self.green, self.blue) == (Channel { shift: 16, bits: 8 }, Channel { shift: 8, bits: 8 }, Channel { shift: 0, bits: 8 });
        for row in data.chunks(stride.max(1)).take(rows as usize) {
            let line = row.get(..used).ok_or("a short row")?;
            if bgrx {
                for [b, g, r, _] in line.as_chunks::<4>().0 {
                    out.extend_from_slice(&[*r, *g, *b, 255]);
                }
            } else {
                for px in line.chunks_exact(self.bytes_per_pixel) {
                    let v = self.value(px);
                    out.extend_from_slice(&[self.red.get(v), self.green.get(v), self.blue.get(v), 255]);
                }
            }
        }
        Ok(())
    }
}

/// Cuts `height` rows of `row_bytes` each into strips of at most `max_bytes` (at least one row
/// a strip): `(first row, rows)`.
pub fn strips(height: u32, row_bytes: usize, max_bytes: usize) -> Vec<(u32, u32)> {
    let per = u32::try_from(max_bytes / row_bytes.max(1)).unwrap_or(u32::MAX).clamp(1, u32::from(u16::MAX));
    let mut out = Vec::new();
    let mut y = 0u32;
    while y < height {
        let rows = per.min(height - y);
        out.push((y, rows));
        y += rows;
    }
    out
}

/// The visible frame of a client window: `(x, y, w, h)` its inside in root coordinates,
/// `border` its X border, `wm` the frame the window manager draws around it
/// (`_NET_FRAME_EXTENTS`) and `csd` the invisible margin a client-side-decorated window keeps
/// around itself for shadows (`_GTK_FRAME_EXTENTS`), both left, right, top, bottom. None when
/// nothing is left or it is off any desktop.
pub fn frame_rect(x: i32, y: i32, w: u32, h: u32, border: u32, wm: [u32; 4], csd: [u32; 4]) -> Option<IRect> {
    let [wl, wr, wt, wb] = wm.map(i64::from);
    let [cl, cr, ct, cb] = csd.map(i64::from);
    let bw = i64::from(border);
    let x0 = i64::from(x) - bw - wl + cl;
    let y0 = i64::from(y) - bw - wt + ct;
    let x1 = i64::from(x) + i64::from(w) + bw + wr - cr;
    let y1 = i64::from(y) + i64::from(h) + bw + wb - cb;
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    Some(IRect::new(i32::try_from(x0).ok()?, i32::try_from(y0).ok()?, u32::try_from(x1 - x0).ok()?, u32::try_from(y1 - y0).ok()?))
}

/// Four CARDINALs of a property (frame extents), zero when it holds fewer.
pub fn extents(values: &[u32]) -> [u32; 4] {
    match values {
        [l, r, t, b, ..] => [*l, *r, *t, *b],
        _ => [0; 4],
    }
}

/// An X text property's bytes as text: UTF-8 (`_NET_WM_NAME`, a `UTF8_STRING`), or Latin-1 (a
/// `STRING`, what `WM_NAME` usually is). Stops at the first NUL.
pub fn text(bytes: &[u8], latin1: bool) -> String {
    let bytes = bytes.split(|b| *b == 0).next().unwrap_or_default();
    if latin1 { bytes.iter().map(|b| char::from(*b)).collect() } else { String::from_utf8_lossy(bytes).into_owned() }
}

/// The program from a `WM_CLASS` property (`instance\0Class\0`): the class, else the instance.
pub fn wm_class(bytes: &[u8]) -> String {
    let mut parts = bytes.split(|b| *b == 0);
    let instance = parts.next().unwrap_or_default();
    let class = parts.next().unwrap_or_default();
    text(if class.is_empty() { instance } else { class }, true)
}

/// The path of a local `file://` URI as bytes, percent escapes decoded: `file:///a%20b` and
/// `file://localhost/a%20b` are `/a b`. None for any other kind of address, a remote host, a
/// broken escape or a NUL.
pub fn file_uri_path(uri: &str) -> Option<Vec<u8>> {
    let scheme = uri.get(..7)?;
    if !scheme.eq_ignore_ascii_case("file://") {
        return None;
    }
    let rest = uri.get(7..)?;
    let rest = rest.split(['?', '#']).next().unwrap_or_default();
    let path = if rest.starts_with('/') {
        rest
    } else {
        let slash = rest.find('/')?;
        if !rest.get(..slash)?.eq_ignore_ascii_case("localhost") {
            return None;
        }
        rest.get(slash..)?
    };
    let hex = |b: u8| char::from(b).to_digit(16).and_then(|d| u8::try_from(d).ok());
    let mut out = Vec::with_capacity(path.len());
    let mut bytes = path.bytes();
    while let Some(b) = bytes.next() {
        let b = if b == b'%' { (hex(bytes.next()?)? << 4) | hex(bytes.next()?)? } else { b };
        if b == 0 {
            return None;
        }
        out.push(b);
    }
    Some(out)
}

/// The part of a portal request's object path that names the caller: its bus name without the
/// leading `:` and with `.` as `_` (`:1.42` is `1_42`).
pub fn portal_sender(unique_name: &str) -> String {
    unique_name.trim_start_matches(':').replace('.', "_")
}

/// Where each output is in the screenshot portal's picture of the whole desktop (`pic_w`×`pic_h`
/// pixels), from the outputs' logical rectangles. The picture covers their bounding box at one
/// scale (twice the logical size when an output is at scale 2), so each output's part is its
/// rectangle scaled by the picture's size over the box's. None when the picture's sides don't
/// share one scale (it isn't of these outputs); an output that falls outside the picture is
/// none in the list.
pub fn split_desktop(pic_w: u32, pic_h: u32, outputs: &[IRect]) -> Option<Vec<Option<IRect>>> {
    let bbox = outputs.iter().fold(IRect::default(), |acc, r| acc.union(*r));
    if bbox.is_empty() || pic_w == 0 || pic_h == 0 {
        return None;
    }
    let sx = f64::from(pic_w) / f64::from(bbox.w);
    let sy = f64::from(pic_h) / f64::from(bbox.h);
    // A pixel of rounding either way, and a percent for compositors that round their scale.
    let slack = 0.01 * sx.max(sy) + 2.0 * sx.max(sy) / f64::from(bbox.w.min(bbox.h));
    if (sx - sy).abs() > slack {
        return None;
    }
    let edge = |v: i64, origin: i32, scale: f64, limit: u32| ((v - i64::from(origin)) as f64 * scale).round().clamp(0.0, f64::from(limit));
    Some(
        outputs
            .iter()
            .map(|r| {
                let x0 = edge(i64::from(r.x), bbox.x, sx, pic_w);
                let y0 = edge(i64::from(r.y), bbox.y, sy, pic_h);
                let x1 = edge(r.right(), bbox.x, sx, pic_w);
                let y1 = edge(r.bottom(), bbox.y, sy, pic_h);
                (x1 > x0 && y1 > y0).then(|| IRect::new(x0 as i32, y0 as i32, (x1 - x0) as u32, (y1 - y0) as u32))
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgba(fmt: &PixelFormat, data: &[u8], width: u32, rows: u32) -> Vec<u8> {
        let mut out = Vec::new();
        fmt.convert(data, width, rows, &mut out).unwrap();
        out
    }

    #[test]
    fn the_usual_32_bit_layout() {
        let fmt = PixelFormat::new(32, 32, false, 0xff_0000, 0xff00, 0xff).unwrap();
        // Two pixels, B G R x each: red, then a grey-blue.
        let data = [0, 0, 255, 0, 200, 100, 50, 7];
        assert_eq!(rgba(&fmt, &data, 2, 1), vec![255, 0, 0, 255, 50, 100, 200, 255]);
    }

    #[test]
    fn masks_and_byte_orders() {
        // The same pixel 0x00112233 (R 11, G 22, B 33) in each byte order.
        let lsb = PixelFormat::new(32, 32, false, 0xff_0000, 0xff00, 0xff).unwrap();
        let msb = PixelFormat::new(32, 32, true, 0xff_0000, 0xff00, 0xff).unwrap();
        assert_eq!(rgba(&lsb, &[0x33, 0x22, 0x11, 0], 1, 1), vec![0x11, 0x22, 0x33, 255]);
        assert_eq!(rgba(&msb, &[0, 0x11, 0x22, 0x33], 1, 1), vec![0x11, 0x22, 0x33, 255]);
        // Blue in the high bits (0x00BBGGRR), as some servers have it.
        let bgr = PixelFormat::new(32, 32, false, 0xff, 0xff00, 0xff_0000).unwrap();
        assert_eq!(rgba(&bgr, &[0x11, 0x22, 0x33, 0], 1, 1), vec![0x11, 0x22, 0x33, 255]);
        // RGBx in the top three bytes, big-endian.
        let rgbx = PixelFormat::new(32, 32, true, 0xff00_0000, 0xff_0000, 0xff00).unwrap();
        assert_eq!(rgba(&rgbx, &[0x11, 0x22, 0x33, 0x99], 1, 1), vec![0x11, 0x22, 0x33, 255]);
        // 24 bits a pixel, three bytes each, both orders.
        let p24 = PixelFormat::new(24, 32, false, 0xff_0000, 0xff00, 0xff).unwrap();
        assert_eq!(rgba(&p24, &[0x33, 0x22, 0x11, 0x66, 0x55, 0x44, 0, 0], 2, 1), vec![0x11, 0x22, 0x33, 255, 0x44, 0x55, 0x66, 255]);
        let p24m = PixelFormat::new(24, 8, true, 0xff_0000, 0xff00, 0xff).unwrap();
        assert_eq!(rgba(&p24m, &[0x11, 0x22, 0x33], 1, 1), vec![0x11, 0x22, 0x33, 255]);
    }

    #[test]
    fn deep_and_shallow_channels_are_scaled_to_8_bits() {
        // 10 bits a channel (depth 30): 0x3ff is full, 0x200 is half.
        let deep = PixelFormat::new(32, 32, false, 0x3ff0_0000, 0x000f_fc00, 0x3ff).unwrap();
        let v: u32 = (0x3ff << 20) | (0x200 << 10);
        assert_eq!(rgba(&deep, &v.to_le_bytes(), 1, 1), vec![255, 128, 0, 255]);
        // RGB565: full red, full green, a third of blue (10 of 31).
        let p565 = PixelFormat::new(16, 16, false, 0xf800, 0x07e0, 0x001f).unwrap();
        let v: u16 = 0xf800 | 0x07e0 | 10;
        assert_eq!(rgba(&p565, &v.to_le_bytes(), 1, 1), vec![255, 255, 82, 255]);
        assert_eq!(rgba(&p565, &0u16.to_be_bytes(), 1, 1), vec![0, 0, 0, 255]);
    }

    #[test]
    fn rows_are_padded() {
        // 3 pixels of 24 bits are 9 bytes, padded to 12.
        let p24 = PixelFormat::new(24, 32, false, 0xff_0000, 0xff00, 0xff).unwrap();
        assert_eq!(p24.stride(3), Some(12));
        let mut data = vec![0u8; 24];
        data[12..15].copy_from_slice(&[3, 2, 1]);
        let out = rgba(&p24, &data, 3, 2);
        assert_eq!(out.len(), 3 * 2 * 4);
        assert_eq!(&out[12..16], &[1, 2, 3, 255], "the second row starts after the padding");
        assert_eq!(PixelFormat::new(16, 32, false, 0xf800, 0x07e0, 0x1f).unwrap().stride(3), Some(8));
    }

    #[test]
    fn bad_formats_and_short_data_are_refused() {
        assert!(PixelFormat::new(8, 32, false, 0xe0, 0x1c, 0x3).is_err(), "8-bit pixels");
        assert!(PixelFormat::new(32, 32, false, 0, 0xff00, 0xff).is_err(), "no red");
        assert!(PixelFormat::new(32, 32, false, 0xf0f0_0000, 0xff00, 0xff).is_err(), "a split mask");
        assert!(PixelFormat::new(32, 32, false, 0xffff, 0xff00, 0xff).is_err(), "overlapping masks");
        assert!(PixelFormat::new(24, 32, false, 0xff00_0000, 0xff00, 0xff).is_err(), "a mask past 24 bits");
        assert!(PixelFormat::new(32, 12, false, 0xff_0000, 0xff00, 0xff).is_err(), "odd padding");
        let fmt = PixelFormat::new(32, 32, false, 0xff_0000, 0xff00, 0xff).unwrap();
        let mut out = Vec::new();
        assert!(fmt.convert(&[0; 7], 2, 1, &mut out).is_err());
        assert!(fmt.convert(&[0; 8], 2, 2, &mut out).is_err());
        assert!(out.is_empty());
        // A whole 32-bit mask is one channel, not an overflow.
        assert!(PixelFormat::new(32, 32, false, 0xffff_ffff, 0, 0).is_err());
        assert_eq!(Channel::of(u32::MAX), Some(Channel { shift: 0, bits: 32 }));
        assert_eq!(Channel { shift: 0, bits: 32 }.get(u32::MAX), 255);
    }

    #[test]
    fn strips_cover_every_row_within_the_limit() {
        assert_eq!(strips(10, 100, 1000), vec![(0, 10)]);
        assert_eq!(strips(10, 100, 350), vec![(0, 3), (3, 3), (6, 3), (9, 1)]);
        assert_eq!(strips(3, 100, 10), vec![(0, 1), (1, 1), (2, 1)], "at least a row a strip");
        assert_eq!(strips(0, 100, 1000), vec![]);
        assert_eq!(strips(5, 0, 1000), vec![(0, 5)]);
        // A 4K screen of 32-bit pixels against the plain 256 KiB request limit.
        let s = strips(2160, 3840 * 4, 262_144);
        assert_eq!(s.first(), Some(&(0, 17)));
        assert_eq!(s.iter().map(|(_, h)| h).sum::<u32>(), 2160);
        assert!(s.windows(2).all(|w| w[0].0 + w[0].1 == w[1].0));
    }

    #[test]
    fn frames_add_the_window_manager_and_drop_the_shadow() {
        let none = [0; 4];
        assert_eq!(frame_rect(100, 50, 640, 480, 0, none, none), Some(IRect::new(100, 50, 640, 480)));
        // A title bar of 30 and borders of 2 (left, right, top, bottom).
        assert_eq!(frame_rect(102, 80, 640, 480, 0, [2, 2, 30, 2], none), Some(IRect::new(100, 50, 644, 512)));
        // A GTK window drawing 26 pixels of shadow round itself, 23 at the top.
        assert_eq!(frame_rect(74, 27, 692, 532, 0, none, [26, 26, 23, 29]), Some(IRect::new(100, 50, 640, 480)));
        // The X border counts.
        assert_eq!(frame_rect(10, 10, 100, 100, 1, none, none), Some(IRect::new(9, 9, 102, 102)));
        // A window at the left of a monitor left of the main one.
        assert_eq!(frame_rect(-1918, 30, 100, 100, 0, [2, 2, 30, 2], none), Some(IRect::new(-1920, 0, 104, 132)));
        // Nonsense from the server: all shadow, or huge extents.
        assert_eq!(frame_rect(0, 0, 10, 10, 0, none, [5, 5, 0, 0]), None);
        assert_eq!(frame_rect(0, 0, 10, 10, 0, [u32::MAX, 0, 0, 0], none), None);
        assert_eq!(extents(&[1, 2, 3]), [0; 4]);
        assert_eq!(extents(&[1, 2, 3, 4, 5]), [1, 2, 3, 4]);
    }

    #[test]
    fn window_text() {
        assert_eq!(text(b"caf\xc3\xa9", false), "café");
        assert_eq!(text(b"caf\xe9\0junk", true), "café");
        assert_eq!(text(b"bad \xff", false), "bad \u{fffd}");
        assert_eq!(wm_class(b"xterm\0XTerm\0"), "XTerm");
        assert_eq!(wm_class(b"navigator\0"), "navigator");
        assert_eq!(wm_class(b""), "");
    }

    #[test]
    fn file_uris() {
        assert_eq!(file_uri_path("file:///tmp/Screenshot.png").as_deref(), Some(&b"/tmp/Screenshot.png"[..]));
        assert_eq!(
            file_uri_path("file:///home/me/Pictures/Screenshot%20from%202026-10-14%2010-00-00.png").as_deref(),
            Some(&b"/home/me/Pictures/Screenshot from 2026-10-14 10-00-00.png"[..])
        );
        assert_eq!(file_uri_path("FILE://localhost/a%2fb%C3%A9").as_deref(), Some(&b"/a/b\xc3\xa9"[..]));
        assert_eq!(file_uri_path("file:///odd%ff").as_deref(), Some(&b"/odd\xff"[..]), "not UTF-8 is still a path");
        assert_eq!(file_uri_path("file:///a.png?x=1#frag").as_deref(), Some(&b"/a.png"[..]));
        assert_eq!(file_uri_path("file://elsewhere/a.png"), None);
        assert_eq!(file_uri_path("https://example.com/a.png"), None);
        assert_eq!(file_uri_path("file:///bad%2"), None);
        assert_eq!(file_uri_path("file:///bad%zz"), None);
        assert_eq!(file_uri_path("file:///nul%00"), None);
        assert_eq!(file_uri_path("file:/"), None);
        assert_eq!(file_uri_path("fïle:///x"), None);
    }

    #[test]
    fn portal_request_paths() {
        assert_eq!(portal_sender(":1.42"), "1_42");
        assert_eq!(portal_sender(":1.2.3"), "1_2_3");
    }

    #[test]
    fn the_desktop_picture_is_cut_into_monitors() {
        let one = [IRect::new(0, 0, 1920, 1080)];
        assert_eq!(split_desktop(1920, 1080, &one), Some(vec![Some(IRect::new(0, 0, 1920, 1080))]));
        // Two outputs at scale 2 side by side: the picture is twice the logical size.
        let two = [IRect::new(0, 0, 1440, 900), IRect::new(1440, 0, 1920, 1080)];
        assert_eq!(split_desktop(6720, 2160, &two), Some(vec![Some(IRect::new(0, 0, 2880, 1800)), Some(IRect::new(2880, 0, 3840, 2160))]));
        // A monitor left of the main one (negative logical x) at fractional scale 1.25.
        let left = [IRect::new(-1536, 0, 1536, 864), IRect::new(0, 0, 1536, 864)];
        assert_eq!(split_desktop(3840, 1080, &left), Some(vec![Some(IRect::new(0, 0, 1920, 1080)), Some(IRect::new(1920, 0, 1920, 1080))]));
        // A pixel off from rounding is still the same picture.
        assert!(split_desktop(1921, 1080, &one).is_some());
        // A picture of only one of two outputs is not the desktop.
        assert_eq!(split_desktop(1440, 900, &two), None);
        assert_eq!(split_desktop(0, 900, &two), None);
        assert_eq!(split_desktop(100, 100, &[]), None);
    }
}
