//! Pictures as video encoders take them: 4:2:0 YUV, BT.709, limited range.
//!
//! A video's sides must be even (each chroma sample covers two by two pixels), so an odd side
//! is padded by repeating the last row or column.

/// One picture in three planes: full-size luma, half-size chroma.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct I420 {
    pub width: u32,
    pub height: u32,
    pub y: Vec<u8>,
    pub u: Vec<u8>,
    pub v: Vec<u8>,
}

/// The side rounded up to even.
pub fn even(n: u32) -> u32 {
    n.saturating_add(n & 1)
}

fn luma(r: i32, g: i32, b: i32) -> u8 {
    (((47 * r + 157 * g + 16 * b + 128) >> 8) + 16).clamp(16, 235) as u8
}

fn chroma(r: i32, g: i32, b: i32) -> (u8, u8) {
    let u = ((-26 * r - 87 * g + 112 * b + 128) >> 8) + 128;
    let v = ((112 * r - 102 * g - 10 * b + 128) >> 8) + 128;
    (u.clamp(16, 240) as u8, v.clamp(16, 240) as u8)
}

impl I420 {
    /// RGBA pixels (`width × height × 4`, rows top first) as YUV. None if the sizes don't agree.
    pub fn from_rgba(pixels: &[u8], width: u32, height: u32) -> Option<I420> {
        let (w, h) = (width as usize, height as usize);
        if w == 0 || h == 0 || pixels.len() != w.checked_mul(h)?.checked_mul(4)? {
            return None;
        }
        let (ew, eh) = (even(width) as usize, even(height) as usize);
        let rows: Vec<&[u8]> = pixels.chunks_exact(w * 4).collect();
        let row = |y: usize| rows.get(y.min(h - 1)).copied().unwrap_or_default();
        let px = |r: &[u8], x: usize| {
            let i = x.min(w - 1) * 4;
            match r.get(i..i + 3) {
                Some(&[r, g, b]) => (i32::from(r), i32::from(g), i32::from(b)),
                _ => (0, 0, 0),
            }
        };
        let mut y_plane = Vec::with_capacity(ew * eh);
        for y in 0..eh {
            let r = row(y);
            y_plane.extend((0..ew).map(|x| {
                let (cr, cg, cb) = px(r, x);
                luma(cr, cg, cb)
            }));
        }
        let (cw, ch) = (ew / 2, eh / 2);
        let mut u_plane = Vec::with_capacity(cw * ch);
        let mut v_plane = Vec::with_capacity(cw * ch);
        for cy in 0..ch {
            let (r0, r1) = (row(cy * 2), row(cy * 2 + 1));
            for cx in 0..cw {
                let (a, b, c, d) = (px(r0, cx * 2), px(r0, cx * 2 + 1), px(r1, cx * 2), px(r1, cx * 2 + 1));
                let (u, v) = chroma((a.0 + b.0 + c.0 + d.0 + 2) / 4, (a.1 + b.1 + c.1 + d.1 + 2) / 4, (a.2 + b.2 + c.2 + d.2 + 2) / 4);
                u_plane.push(u);
                v_plane.push(v);
            }
        }
        Some(I420 { width: ew as u32, height: eh as u32, y: y_plane, u: u_plane, v: v_plane })
    }

    /// An all-black picture.
    pub fn black(width: u32, height: u32) -> I420 {
        let (ew, eh) = (even(width.max(2)), even(height.max(2)));
        let (w, h) = (ew as usize, eh as usize);
        I420 { width: ew, height: eh, y: vec![16; w * h], u: vec![128; w * h / 4], v: vec![128; w * h / 4] }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn white_black_and_grey_land_on_the_limited_range() {
        let px = [255, 255, 255, 255, 0, 0, 0, 255, 128, 128, 128, 255, 128, 128, 128, 255];
        let f = I420::from_rgba(&px, 2, 2).unwrap();
        assert_eq!(f.y, vec![235, 16, 126, 126]);
        assert_eq!((f.u[0], f.v[0]), (128, 128));
    }

    #[test]
    fn odd_sides_are_padded_to_even() {
        let px = vec![200u8; 3 * 5 * 4];
        let f = I420::from_rgba(&px, 3, 5).unwrap();
        assert_eq!((f.width, f.height), (4, 6));
        assert_eq!(f.y.len(), 24);
        assert_eq!(f.u.len(), 6);
    }

    #[test]
    fn sizes_that_dont_agree_are_refused() {
        assert!(I420::from_rgba(&[0; 15], 2, 2).is_none());
        assert!(I420::from_rgba(&[], 0, 0).is_none());
    }

    #[test]
    fn red_has_its_chroma_where_bt709_puts_it() {
        let px = [255, 0, 0, 255].repeat(4);
        let f = I420::from_rgba(&px, 2, 2).unwrap();
        assert_eq!(f.y[0], 63);
        assert!(f.u[0] < 128 && f.v[0] > 200);
    }
}
