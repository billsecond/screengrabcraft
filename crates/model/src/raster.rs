//! A picture in memory: 8-bit RGBA, straight alpha, rows top to bottom.
//!
//! The pixels are shared (`Arc`), so that keeping a document for undo, or handing a capture
//! from one part of the program to another, copies nothing. A change makes its own copy.

use std::sync::Arc;

use crate::color::Color;

/// The widest or tallest picture taken in. Scrolling captures are long, so this is generous.
pub const MAX_SIDE: u32 = 65_535;
/// The most pixels a picture may have: 120 million (480 MB in memory) — a 1920-pixel-wide
/// scrolling capture 62,000 pixels long.
pub const MAX_PIXELS: u64 = 120_000_000;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RasterError {
    #[error("a picture of {0}×{1} pixels is empty")]
    Empty(u32, u32),
    #[error("a picture of {0}×{1} pixels is larger than ScreenGrabCraft takes ({MAX_SIDE} pixels a side, {MAX_PIXELS} pixels in all)")]
    TooLarge(u32, u32),
    #[error("{got} bytes of pixels for a picture of {width}×{height} (it needs {want})")]
    WrongLength { width: u32, height: u32, got: usize, want: usize },
}

/// Checks that a `width`×`height` picture may be made, and returns its byte length.
pub fn checked_len(width: u32, height: u32) -> Result<usize, RasterError> {
    if width == 0 || height == 0 {
        return Err(RasterError::Empty(width, height));
    }
    if width > MAX_SIDE || height > MAX_SIDE || u64::from(width) * u64::from(height) > MAX_PIXELS {
        return Err(RasterError::TooLarge(width, height));
    }
    usize::try_from(u64::from(width) * u64::from(height) * 4).map_err(|_| RasterError::TooLarge(width, height))
}

#[derive(Clone, PartialEq, Eq)]
pub struct Raster {
    width: u32,
    height: u32,
    pixels: Arc<Vec<u8>>,
}

impl std::fmt::Debug for Raster {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Raster({}×{})", self.width, self.height)
    }
}

impl Raster {
    /// A picture from its RGBA bytes.
    pub fn new(width: u32, height: u32, pixels: Vec<u8>) -> Result<Raster, RasterError> {
        let want = checked_len(width, height)?;
        if pixels.len() != want {
            return Err(RasterError::WrongLength { width, height, got: pixels.len(), want });
        }
        Ok(Raster { width, height, pixels: Arc::new(pixels) })
    }

    /// One pixel of one colour (never fails).
    pub fn single(color: Color) -> Raster {
        Raster { width: 1, height: 1, pixels: Arc::new(vec![color.r, color.g, color.b, color.a]) }
    }

    /// A picture filled with one colour.
    pub fn filled(width: u32, height: u32, color: Color) -> Result<Raster, RasterError> {
        let len = checked_len(width, height)?;
        let mut pixels = Vec::with_capacity(len);
        for _ in 0..len / 4 {
            pixels.extend_from_slice(&[color.r, color.g, color.b, color.a]);
        }
        Ok(Raster { width, height, pixels: Arc::new(pixels) })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    /// The bytes, for changing (copied first if anything else holds them).
    pub fn pixels_mut(&mut self) -> &mut [u8] {
        Arc::make_mut(&mut self.pixels).as_mut_slice()
    }

    /// The bytes, taken out (copied if anything else holds them).
    pub fn into_pixels(self) -> Vec<u8> {
        Arc::try_unwrap(self.pixels).unwrap_or_else(|shared| (*shared).clone())
    }

    /// Whether two rasters share the same bytes (no copy was made between them).
    pub fn same_pixels(&self, other: &Raster) -> bool {
        Arc::ptr_eq(&self.pixels, &other.pixels)
    }

    /// The colour of one pixel, or none outside the picture.
    pub fn pixel(&self, x: u32, y: u32) -> Option<Color> {
        if x >= self.width || y >= self.height {
            return None;
        }
        let i = (y as usize * self.width as usize + x as usize) * 4;
        match self.pixels.get(i..i + 4) {
            Some([r, g, b, a]) => Some(Color::rgba(*r, *g, *b, *a)),
            _ => None,
        }
    }

    /// A copy of the part inside `(x, y, w, h)`, clipped to the picture; none when nothing is left.
    pub fn crop(&self, x: u32, y: u32, w: u32, h: u32) -> Option<Raster> {
        let x1 = x.saturating_add(w).min(self.width);
        let y1 = y.saturating_add(h).min(self.height);
        if x >= x1 || y >= y1 {
            return None;
        }
        let (cw, ch) = (x1 - x, y1 - y);
        let mut out = Vec::with_capacity(cw as usize * ch as usize * 4);
        let stride = self.width as usize * 4;
        for row in y..y1 {
            let start = row as usize * stride + x as usize * 4;
            out.extend_from_slice(self.pixels.get(start..start + cw as usize * 4)?);
        }
        Raster::new(cw, ch, out).ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sizes_are_checked() {
        assert!(matches!(Raster::new(0, 10, vec![]), Err(RasterError::Empty(0, 10))));
        assert!(matches!(Raster::new(70_000, 1, vec![]), Err(RasterError::TooLarge(..))));
        assert!(matches!(Raster::new(20_000, 20_000, vec![]), Err(RasterError::TooLarge(..))));
        assert!(matches!(Raster::new(2, 2, vec![0; 15]), Err(RasterError::WrongLength { want: 16, .. })));
        assert!(Raster::new(2, 2, vec![0; 16]).is_ok());
    }

    #[test]
    fn crop_and_pixels() {
        let mut r = Raster::filled(4, 3, Color::WHITE).unwrap();
        let shared = r.clone();
        assert!(r.same_pixels(&shared));
        r.pixels_mut()[(4 + 2) * 4] = 0;
        assert!(!r.same_pixels(&shared), "changing makes its own copy");
        assert_eq!(r.pixel(2, 1), Some(Color::rgb(0, 255, 255)));
        assert_eq!(r.pixel(4, 0), None);
        let c = r.crop(2, 1, 10, 10).unwrap();
        assert_eq!((c.width(), c.height()), (2, 2));
        assert_eq!(c.pixel(0, 0), Some(Color::rgb(0, 255, 255)));
        assert!(r.crop(4, 0, 1, 1).is_none());
        assert!(r.crop(0, 0, 0, 1).is_none());
    }
}
