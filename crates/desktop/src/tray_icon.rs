//! The tray and menu-bar icons, drawn in code: the corners of a capture selection round a dot.
//!
//! In colour (the app's teal tile) for Windows and Linux trays; as a black-on-clear template on
//! macOS, which colours menu-bar icons itself for light and dark menu bars; and with a red dot
//! while a GIF or a scrolling capture is recording.

/// How the icon looks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Look {
    Normal,
    /// Black on clear, for the macOS menu bar.
    Template,
    Recording,
}

const TEAL: [f32; 3] = [0x0E as f32 / 255.0, 0x7C as f32 / 255.0, 0x7B as f32 / 255.0];

fn round_rect(x: f32, y: f32, half: f32, r: f32) -> f32 {
    let (dx, dy) = ((x - 0.5).abs() - (half - r), (y - 0.5).abs() - (half - r));
    dx.max(dy).min(0.0) + (dx.max(0.0).powi(2) + dy.max(0.0).powi(2)).sqrt() - r
}

/// Whether a point of the unit square is on one of the four L-shaped corners.
fn corners(x: f32, y: f32, inset: f32, arm: f32, thick: f32) -> bool {
    let (lo, hi) = (inset, 1.0 - inset);
    let near = |v: f32, edge: f32| (v - edge).abs() <= thick / 2.0;
    let along = |v: f32, edge: f32| if edge < 0.5 { v >= edge - thick / 2.0 && v <= edge + arm } else { v <= edge + thick / 2.0 && v >= edge - arm };
    [(lo, lo), (hi, lo), (lo, hi), (hi, hi)].iter().any(|&(cx, cy)| (near(x, cx) && along(y, cy)) || (near(y, cy) && along(x, cx)))
}

/// One sample: colour and coverage.
fn sample(x: f32, y: f32, look: Look) -> [f32; 4] {
    let dot_r = if look == Look::Recording { 0.17 } else { 0.11 };
    let on_dot = ((x - 0.5).powi(2) + (y - 0.5).powi(2)).sqrt() <= dot_r;
    match look {
        Look::Template => {
            if corners(x, y, 0.14, 0.26, 0.11) || on_dot {
                [0.0, 0.0, 0.0, 1.0]
            } else {
                [0.0; 4]
            }
        }
        Look::Normal | Look::Recording => {
            if round_rect(x, y, 0.5, 0.2) > 0.0 {
                return [0.0; 4];
            }
            if on_dot {
                return if look == Look::Recording { [0.92, 0.22, 0.2, 1.0] } else { [1.0, 1.0, 1.0, 1.0] };
            }
            if corners(x, y, 0.2, 0.22, 0.09) {
                return [1.0, 1.0, 1.0, 1.0];
            }
            [TEAL[0], TEAL[1], TEAL[2], 1.0]
        }
    }
}

/// The icon as `size`×`size` RGBA pixels (4×4 samples a pixel).
pub fn pixels(size: u32, look: Look) -> Vec<u8> {
    const SS: u32 = 4;
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    for py in 0..size {
        for px in 0..size {
            let mut acc = [0.0f32; 4];
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32;
                    let s = sample(x, y, look);
                    for c in 0..3 {
                        acc[c] += s[c] * s[3];
                    }
                    acc[3] += s[3];
                }
            }
            let n = (SS * SS) as f32;
            let a = acc[3] / n;
            for c in acc.iter().take(3) {
                out.push(if a > 0.0 { ((c / n / a) * 255.0).round().clamp(0.0, 255.0) as u8 } else { 0 });
            }
            out.push((a * 255.0).round().clamp(0.0, 255.0) as u8);
        }
    }
    out
}

/// The red of a recording, as the tray icon's dot.
const RECORD_RED: [u8; 3] = [0xE5, 0x39, 0x35];

/// The mark of Video Production's menu item: a red record dot with a thin darker rim, on clear,
/// as `size`×`size` RGBA pixels (4×4 samples a pixel, so its edge is smooth).
pub fn record_dot(size: u32) -> Vec<u8> {
    const SS: u32 = 4;
    let size = size.clamp(1, 256);
    let mut out = Vec::with_capacity((size * size * 4) as usize);
    let (r, rim) = (0.40f32, 0.06f32);
    for py in 0..size {
        for px in 0..size {
            let (mut cover, mut inner) = (0u32, 0u32);
            for sy in 0..SS {
                for sx in 0..SS {
                    let x = (px as f32 + (sx as f32 + 0.5) / SS as f32) / size as f32 - 0.5;
                    let y = (py as f32 + (sy as f32 + 0.5) / SS as f32) / size as f32 - 0.5;
                    let d = (x * x + y * y).sqrt();
                    if d <= r {
                        cover += 1;
                        if d <= r - rim {
                            inner += 1;
                        }
                    }
                }
            }
            if cover == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
                continue;
            }
            // The rim is the red darkened, so the dot reads on a light menu and a dark one.
            let f = inner as f32 / cover as f32;
            let shade = 0.72 + 0.28 * f;
            out.extend(RECORD_RED.iter().map(|c| (f32::from(*c) * shade).round() as u8));
            out.push(((cover * 255) / (SS * SS)) as u8);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_record_dot_is_red_round_and_smooth() {
        let d = record_dot(32);
        assert_eq!(d.len(), 32 * 32 * 4);
        let px = |x: usize, y: usize| &d[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4];
        assert_eq!(px(16, 16), &[0xE5, 0x39, 0x35, 255], "red in the middle");
        assert_eq!(px(0, 0)[3], 0, "clear in the corners");
        assert_eq!(px(31, 31)[3], 0);
        // Its edge is partly covered, not stepped.
        assert!((0..32).any(|x| (1..255).contains(&px(x, 16)[3])), "anti-aliased");
        // Round: as wide as it's tall.
        let wide = (0..32).filter(|x| px(*x, 16)[3] > 127).count();
        let tall = (0..32).filter(|y| px(16, *y)[3] > 127).count();
        assert_eq!(wide, tall);
        assert_eq!(record_dot(0).len(), 4, "never empty, never a panic");
    }

    #[test]
    fn icons_have_their_parts() {
        let n = pixels(32, Look::Normal);
        assert_eq!(n.len(), 32 * 32 * 4);
        assert_eq!(n[3], 0, "the corner outside the tile is clear");
        let mid = (16 * 32 + 16) * 4;
        assert_eq!(&n[mid..mid + 3], &[255, 255, 255], "a white dot");
        let rec = pixels(32, Look::Recording);
        assert!(rec[mid] > 200 && rec[mid + 1] < 100, "a red dot");
        let t = pixels(22, Look::Template);
        assert!(t.as_chunks::<4>().0.iter().all(|p| p[0] == 0 && p[1] == 0 && p[2] == 0), "black only");
    }
}
