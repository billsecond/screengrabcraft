//! Cut Out: a strip taken out of a picture, all the way across it, and the rest joined. Columns
//! (a strip from top to bottom, so the picture gets narrower) or rows (one from side to side, so
//! it gets shorter). The join is straight, or a saw or a wave with a thin see-through gap, so it
//! shows that something was taken out.

use screengrabcraft_model::Raster;
use screengrabcraft_model::raster::{RasterError, checked_len};

/// How the two parts meet (the editor's settings keep it).
pub use screengrabcraft_model::settings::CutEdge as Edge;

/// How far a saw's teeth or a wave reach either side of the join, how far apart they are, and
/// how wide the see-through gap between the parts is (pixels).
const REACH: f32 = 6.0;
const PERIOD: f32 = 20.0;
const GAP: f32 = 3.0;

/// Where the join is moved to, `along` pixels down it (or along it, for rows).
fn offset(edge: Edge, along: f32, reach: f32) -> f32 {
    match edge {
        Edge::Straight => 0.0,
        Edge::Saw => {
            let p = (along / PERIOD).rem_euclid(1.0);
            (if p < 0.5 { p * 4.0 - 1.0 } else { 3.0 - p * 4.0 }) * reach
        }
        Edge::Wave => (along / PERIOD * std::f32::consts::TAU).sin() * reach,
    }
}

/// `r` with `from..to` taken out and the rest joined: columns when `columns`, else rows. An
/// empty strip, or one that leaves nothing, is an error.
pub fn cut_out(r: &Raster, from: u32, to: u32, columns: bool, edge: Edge) -> Result<Raster, RasterError> {
    let (w, h) = (r.width(), r.height());
    let across = if columns { w } else { h };
    let (from, to) = (from.min(to).min(across), from.max(to).min(across));
    let cut = to - from;
    if cut == 0 || cut >= across {
        return Err(RasterError::Empty(if columns { w.saturating_sub(cut) } else { w }, if columns { h } else { h.saturating_sub(cut) }));
    }
    let (ow, oh) = if columns { (w - cut, h) } else { (w, h - cut) };
    let len = checked_len(ow, oh)?;
    let (gap, reach) = match edge {
        Edge::Straight => (0.0, 0.0),
        // The teeth reach no further than the strip taken out: the pixels they show are its.
        _ => (GAP, REACH.min(((cut as f32 - GAP) / 2.0).max(0.0))),
    };
    let src = r.pixels();
    let mut out = vec![0u8; len];
    for y in 0..oh {
        for x in 0..ow {
            let (a, b) = if columns { (x, y) } else { (y, x) };
            let seam = from as f32 + offset(edge, b as f32 + 0.5, reach);
            let mid = a as f32 + 0.5;
            let from_a = if mid < seam - gap / 2.0 {
                a
            } else if mid >= seam + gap / 2.0 {
                a + cut
            } else {
                // The gap: see-through.
                continue;
            };
            let (sx, sy) = if columns { (from_a, y) } else { (x, from_a) };
            let si = (sy as usize * w as usize + sx as usize) * 4;
            let oi = (y as usize * ow as usize + x as usize) * 4;
            if let (Some(s), Some(o)) = (src.get(si..si + 4), out.get_mut(oi..oi + 4)) {
                o.copy_from_slice(s);
            }
        }
    }
    Raster::new(ow, oh, out)
}

/// Where a point `v` across the picture (an x for columns, a y for rows) is once `from..to` is
/// taken out: past the strip it moves back by its width, in it it goes to the join.
pub fn moved(v: f32, from: f32, to: f32) -> f32 {
    let (from, to) = (from.min(to), from.max(to));
    if !v.is_finite() {
        v
    } else if v >= to {
        v - (to - from)
    } else if v > from {
        from
    } else {
        v
    }
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::Color;

    use super::*;

    /// A picture `w × h` whose every column is its own shade of red (the column number).
    fn columns(w: u32, h: u32) -> Raster {
        Raster::new(w, h, (0..h).flat_map(|_| (0..w).flat_map(|x| [x as u8, 0, 0, 255])).collect()).unwrap()
    }

    #[test]
    fn a_straight_cut_takes_the_columns_out_and_joins_the_rest() {
        let r = cut_out(&columns(10, 4), 3, 6, true, Edge::Straight).unwrap();
        assert_eq!((r.width(), r.height()), (7, 4));
        let row: Vec<u8> = (0..7).map(|x| r.pixel(x, 2).unwrap().r).collect();
        assert_eq!(row, vec![0, 1, 2, 6, 7, 8, 9]);
        // Given the other way round, the same.
        assert_eq!(cut_out(&columns(10, 4), 6, 3, true, Edge::Straight).unwrap(), r);
    }

    #[test]
    fn rows_are_taken_out_the_same_way() {
        let rows = Raster::new(2, 8, (0..8).flat_map(|y| (0..2).flat_map(move |_| [0, y as u8, 0, 255])).collect()).unwrap();
        let r = cut_out(&rows, 2, 5, false, Edge::Straight).unwrap();
        assert_eq!((r.width(), r.height()), (2, 5));
        let col: Vec<u8> = (0..5).map(|y| r.pixel(0, y).unwrap().g).collect();
        assert_eq!(col, vec![0, 1, 5, 6, 7]);
    }

    #[test]
    fn a_saw_or_a_wave_leaves_a_see_through_gap_along_a_wandering_join() {
        for edge in [Edge::Saw, Edge::Wave] {
            let r = cut_out(&columns(100, 60), 30, 70, true, edge).unwrap();
            assert_eq!((r.width(), r.height()), (60, 60));
            let clear: Vec<u32> = (0..60).filter_map(|y| (0..60).find(|&x| r.pixel(x, y).unwrap().a == 0)).collect();
            assert_eq!(clear.len(), 60, "{edge:?}: a gap on every row");
            assert!(clear.iter().max() > clear.iter().min(), "{edge:?}: the join wanders");
            // Far from the join, the picture as it was.
            assert_eq!(r.pixel(2, 10).unwrap(), Color::rgb(2, 0, 0));
            assert_eq!(r.pixel(55, 10).unwrap(), Color::rgb(95, 0, 0));
        }
    }

    #[test]
    fn nothing_taken_or_nothing_left_is_an_error_and_junk_is_clamped() {
        let r = columns(10, 4);
        assert!(cut_out(&r, 4, 4, true, Edge::Straight).is_err());
        assert!(cut_out(&r, 0, 10, true, Edge::Saw).is_err());
        assert!(cut_out(&r, 0, u32::MAX, true, Edge::Wave).is_err());
        // A strip running past the edge stops there.
        assert_eq!(cut_out(&r, 8, 400, true, Edge::Straight).unwrap().width(), 8);
        // A narrow strip with a styled edge still works.
        assert_eq!(cut_out(&r, 4, 5, true, Edge::Saw).unwrap().width(), 9);
        assert_eq!(Edge::from_name(" Wave "), Some(Edge::Wave));
        assert_eq!(Edge::from_name("zigzag"), None);
    }

    #[test]
    fn points_past_the_strip_move_back_and_points_in_it_go_to_the_join() {
        assert_eq!(moved(2.0, 3.0, 6.0), 2.0);
        assert_eq!(moved(4.0, 3.0, 6.0), 3.0);
        assert_eq!(moved(9.0, 6.0, 3.0), 6.0);
        assert!(moved(f32::NAN, 3.0, 6.0).is_nan());
    }
}
