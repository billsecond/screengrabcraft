//! Putting a picture of an area together from the pictures of the monitors it lies on.

use screengrabcraft_model::{IRect, Raster};

/// The part of the desktop inside `area`, from monitor pictures placed at their rectangles.
/// A monitor picture whose size differs from its rectangle (a system that reports scaled
/// rectangles) is sampled to fit. What no monitor covers is transparent. None for an area that
/// is empty or too large.
pub fn compose(monitors: &[(IRect, &Raster)], area: IRect) -> Option<Raster> {
    if area.is_empty() {
        return None;
    }
    let len = screengrabcraft_model::raster::checked_len(area.w, area.h).ok()?;
    let mut out = vec![0u8; len];
    let row_bytes = area.w as usize * 4;
    for (rect, pic) in monitors {
        let Some(part) = rect.intersect(area) else { continue };
        let same_size = pic.width() == rect.w && pic.height() == rect.h;
        for dy in 0..part.h {
            let desk_y = i64::from(part.y) + i64::from(dy);
            let src_y = desk_y - i64::from(rect.y);
            let src_y = if same_size { src_y } else { src_y * i64::from(pic.height()) / i64::from(rect.h.max(1)) };
            let out_y = desk_y - i64::from(area.y);
            let (Ok(src_y), Ok(out_y)) = (usize::try_from(src_y), usize::try_from(out_y)) else { continue };
            let out_x = usize::try_from(i64::from(part.x) - i64::from(area.x)).unwrap_or(0);
            let src_x = usize::try_from(i64::from(part.x) - i64::from(rect.x)).unwrap_or(0);
            let dst_start = out_y * row_bytes + out_x * 4;
            if same_size {
                let src_start = (src_y * pic.width() as usize + src_x) * 4;
                let n = part.w as usize * 4;
                if let (Some(src), Some(dst)) = (pic.pixels().get(src_start..src_start + n), out.get_mut(dst_start..dst_start + n)) {
                    dst.copy_from_slice(src);
                }
            } else {
                for dx in 0..part.w as usize {
                    let sx = (src_x + dx) * pic.width() as usize / (rect.w.max(1) as usize);
                    let s = (src_y * pic.width() as usize + sx) * 4;
                    let d = dst_start + dx * 4;
                    if let (Some(src), Some(dst)) = (pic.pixels().get(s..s + 4), out.get_mut(d..d + 4)) {
                        dst.copy_from_slice(src);
                    }
                }
            }
        }
    }
    Raster::new(area.w, area.h, out).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    #[test]
    fn composes_across_monitors_and_leaves_gaps_clear() {
        let a = Raster::filled(4, 4, Color::RED).unwrap();
        let b = Raster::filled(4, 2, Color::WHITE).unwrap();
        let mons = [(IRect::new(0, 0, 4, 4), &a), (IRect::new(4, 0, 4, 2), &b)];
        let out = compose(&mons, IRect::new(2, 0, 4, 4)).unwrap();
        assert_eq!(out.pixel(0, 0), Some(Color::RED));
        assert_eq!(out.pixel(3, 1), Some(Color::WHITE));
        assert_eq!(out.pixel(3, 3), Some(Color::TRANSPARENT), "below the short monitor");
        assert!(compose(&mons, IRect::new(0, 0, 0, 4)).is_none());
    }

    #[test]
    fn scaled_monitor_pictures_are_sampled() {
        // A picture twice the size of its rectangle (a system reporting points, not pixels).
        let mut big = Raster::filled(4, 4, Color::BLACK).unwrap();
        big.pixels_mut()[(2 * 4 + 2) * 4] = 255;
        let mons = [(IRect::new(0, 0, 2, 2), &big)];
        let out = compose(&mons, IRect::new(0, 0, 2, 2)).unwrap();
        assert_eq!(out.pixel(1, 1), Some(Color::rgb(255, 0, 0)));
        assert_eq!(out.pixel(0, 0), Some(Color::BLACK));
    }
}
