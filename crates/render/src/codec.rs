//! Reading pictures (PNG, JPEG, GIF, BMP, WebP, ICO… whatever the `image` crate knows, by
//! their bytes rather than their names) and writing them in the formats a capture is saved in.
//!
//! A file opened from disk or pasted from another program is hostile: the decoder is given
//! limits (size, memory) before it reads anything, so a picture that claims to be a million
//! pixels wide is refused instead of exhausting memory.

use std::io::Cursor;

use image::{ImageDecoder, ImageEncoder, ImageReader, Limits};
use screengrabcraft_model::{Color, ImageFormat, MAX_PIXELS, MAX_SIDE, Raster};

#[derive(Debug, thiserror::Error)]
pub enum CodecError {
    #[error("this is not a picture ScreenGrabCraft can read: {0}")]
    Unreadable(String),
    #[error("the picture could not be written: {0}")]
    Write(String),
    #[error(transparent)]
    Size(#[from] screengrabcraft_model::RasterError),
}

/// The most bytes a picture file may have before it is read.
pub const MAX_FILE: usize = 512 * 1024 * 1024;

/// Decodes a picture from its bytes.
pub fn decode(bytes: &[u8]) -> Result<Raster, CodecError> {
    if bytes.len() > MAX_FILE {
        return Err(CodecError::Unreadable(format!("the file is larger than {} MB", MAX_FILE / 1024 / 1024)));
    }
    let reader = ImageReader::new(Cursor::new(bytes)).with_guessed_format().map_err(|e| CodecError::Unreadable(e.to_string()))?;
    if reader.format().is_none() {
        return Err(CodecError::Unreadable("unknown format".to_string()));
    }
    let mut decoder = reader.into_decoder().map_err(|e| CodecError::Unreadable(e.to_string()))?;
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_SIDE);
    limits.max_image_height = Some(MAX_SIDE);
    limits.max_alloc = Some(MAX_PIXELS * 4 + 64 * 1024 * 1024);
    decoder.set_limits(limits).map_err(|e| CodecError::Unreadable(e.to_string()))?;
    let (w, h) = decoder.dimensions();
    screengrabcraft_model::raster::checked_len(w, h)?;
    let img = image::DynamicImage::from_decoder(decoder).map_err(|e| CodecError::Unreadable(e.to_string()))?;
    let rgba = img.into_rgba8();
    let (w, h) = rgba.dimensions();
    Ok(Raster::new(w, h, rgba.into_raw())?)
}

/// The raster flattened onto `background` (for formats without transparency).
pub fn flatten(r: &Raster, background: Color) -> Vec<u8> {
    let mut out = Vec::with_capacity(r.width() as usize * r.height() as usize * 3);
    for px in r.pixels().as_chunks::<4>().0 {
        let a = u32::from(px[3]);
        let mix = |c: u8, b: u8| ((u32::from(c) * a + u32::from(b) * (255 - a) + 127) / 255) as u8;
        out.extend_from_slice(&[mix(px[0], background.r), mix(px[1], background.g), mix(px[2], background.b)]);
    }
    out
}

fn write_err(e: impl std::fmt::Display) -> CodecError {
    CodecError::Write(e.to_string())
}

/// Encodes a picture. `quality` (10–100) is used by JPEG.
pub fn encode(r: &Raster, format: ImageFormat, quality: u8) -> Result<Vec<u8>, CodecError> {
    let (w, h) = (r.width(), r.height());
    let mut out = Vec::new();
    match format {
        ImageFormat::Png => {
            let enc = image::codecs::png::PngEncoder::new_with_quality(
                &mut out,
                image::codecs::png::CompressionType::Default,
                image::codecs::png::FilterType::Adaptive,
            );
            enc.write_image(r.pixels(), w, h, image::ExtendedColorType::Rgba8).map_err(write_err)?;
        }
        ImageFormat::Jpeg => {
            let rgb = flatten(r, Color::WHITE);
            let enc = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, quality.clamp(10, 100));
            enc.write_image(&rgb, w, h, image::ExtendedColorType::Rgb8).map_err(write_err)?;
        }
        ImageFormat::Webp => {
            let enc = image::codecs::webp::WebPEncoder::new_lossless(&mut out);
            enc.write_image(r.pixels(), w, h, image::ExtendedColorType::Rgba8).map_err(write_err)?;
        }
        ImageFormat::Bmp => {
            let mut enc = image::codecs::bmp::BmpEncoder::new(&mut out);
            enc.encode(r.pixels(), w, h, image::ExtendedColorType::Rgba8).map_err(write_err)?;
        }
        ImageFormat::Gif => {
            let (gw, gh) = (u16::try_from(w).map_err(write_err)?, u16::try_from(h).map_err(write_err)?);
            let mut rgba = r.pixels().to_vec();
            let frame = gif::Frame::from_rgba_speed(gw, gh, &mut rgba, 10);
            let mut enc = gif::Encoder::new(&mut out, gw, gh, &[]).map_err(write_err)?;
            enc.write_frame(&frame).map_err(write_err)?;
        }
        ImageFormat::Pdf => out = crate::pdf::single_page(r)?,
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Raster {
        let mut r = Raster::filled(5, 3, Color::RED).unwrap();
        r.pixels_mut()[4..8].copy_from_slice(&[0, 0, 255, 128]);
        r
    }

    #[test]
    fn every_format_writes_and_the_readable_ones_read_back() {
        let r = sample();
        for f in ImageFormat::ALL {
            let bytes = encode(&r, f, 90).unwrap();
            assert!(!bytes.is_empty(), "{f:?}");
            if f == ImageFormat::Pdf {
                assert!(bytes.starts_with(b"%PDF-"));
                continue;
            }
            let back = decode(&bytes).unwrap();
            assert_eq!((back.width(), back.height()), (5, 3), "{f:?}");
            if matches!(f, ImageFormat::Png | ImageFormat::Webp) {
                assert_eq!(back, r, "{f:?} is lossless");
            }
        }
    }

    #[test]
    fn garbage_is_refused() {
        assert!(decode(b"").is_err());
        assert!(decode(b"definitely not a picture").is_err());
        // A PNG header claiming an enormous size.
        let mut png = Vec::new();
        png.extend_from_slice(b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR");
        png.extend_from_slice(&100_000u32.to_be_bytes());
        png.extend_from_slice(&100_000u32.to_be_bytes());
        png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
        assert!(decode(&png).is_err());
        let truncated = &encode(&sample(), ImageFormat::Png, 90).unwrap()[..40];
        assert!(decode(truncated).is_err());
    }

    #[test]
    fn flattening_mixes_with_the_background() {
        let r = Raster::filled(1, 1, Color::rgba(0, 0, 0, 0)).unwrap();
        assert_eq!(flatten(&r, Color::WHITE), vec![255, 255, 255]);
        let r = Raster::filled(1, 1, Color::rgba(0, 0, 0, 255)).unwrap();
        assert_eq!(flatten(&r, Color::WHITE), vec![0, 0, 0]);
    }
}
