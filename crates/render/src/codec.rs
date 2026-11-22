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
        ImageFormat::Tiff => out = tiff(r)?,
    }
    Ok(out)
}

/// A TIFF of the picture: red, green, blue and (unassociated) alpha, 8 bits each, LZW-compressed
/// with the horizontal predictor, which every TIFF reader knows.
fn tiff(r: &Raster) -> Result<Vec<u8>, CodecError> {
    use tiff::encoder::{Compression, Predictor, TiffEncoder, colortype};
    use tiff::tags::{ExtraSamples, Tag};
    let mut file = Cursor::new(Vec::new());
    {
        let mut enc = TiffEncoder::new(&mut file).map_err(write_err)?.with_compression(Compression::Lzw).with_predictor(Predictor::Horizontal);
        let mut image = enc.new_image::<colortype::RGBA8>(r.width(), r.height()).map_err(write_err)?;
        // The fourth sample is said to be alpha, not colour premultiplied by it.
        image.encoder().write_tag(Tag::ExtraSamples, &[ExtraSamples::UnassociatedAlpha.to_u16()][..]).map_err(write_err)?;
        image.write_data(r.pixels()).map_err(write_err)?;
    }
    Ok(file.into_inner())
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
            if matches!(f, ImageFormat::Png | ImageFormat::Webp | ImageFormat::Tiff) {
                assert_eq!(back, r, "{f:?} is lossless");
            }
        }
    }

    #[test]
    fn a_tiff_is_compressed_says_its_alpha_and_reads_back_from_other_writers() {
        // A picture of a few flat colours, as a screenshot is: LZW makes it far smaller.
        let mut r = Raster::filled(300, 200, Color::WHITE).unwrap();
        for (i, px) in r.pixels_mut().as_chunks_mut::<4>().0.iter_mut().enumerate() {
            if (i % 300) < 120 {
                *px = [30, 90, 200, 255];
            }
        }
        let ours = encode(&r, ImageFormat::Tiff, 90).unwrap();
        assert!(ours.starts_with(b"II*\0") || ours.starts_with(b"MM\0*"));
        assert!(ours.len() < 300 * 200 * 4 / 10, "compressed: {} bytes", ours.len());
        assert_eq!(decode(&ours).unwrap(), r);
        // The fourth sample is marked as alpha (tag 338, ExtraSamples, = 2).
        let mut d = tiff::decoder::Decoder::new(Cursor::new(&ours)).unwrap();
        assert_eq!(d.get_tag_u32_vec(tiff::tags::Tag::ExtraSamples).unwrap(), vec![2]);
        // One written without compression by another writer, and an opaque one, read too.
        let mut plain = Cursor::new(Vec::new());
        image::codecs::tiff::TiffEncoder::new(&mut plain).write_image(r.pixels(), 300, 200, image::ExtendedColorType::Rgba8).unwrap();
        assert_eq!(decode(plain.get_ref()).unwrap(), r);
        let rgb = flatten(&r, Color::WHITE);
        let mut opaque = Cursor::new(Vec::new());
        image::codecs::tiff::TiffEncoder::new(&mut opaque).write_image(&rgb, 300, 200, image::ExtendedColorType::Rgb8).unwrap();
        assert_eq!(decode(opaque.get_ref()).unwrap(), r);
    }

    #[test]
    fn a_broken_or_enormous_tiff_is_refused() {
        let good = encode(&sample(), ImageFormat::Tiff, 90).unwrap();
        for cut in [4, 8, 16, good.len() / 2, good.len() - 1] {
            assert!(decode(&good[..cut]).is_err(), "cut at {cut}");
        }
        // A header and one directory claiming 100 000 by 100 000 pixels.
        let mut big = Cursor::new(Vec::new());
        {
            let mut enc = tiff::encoder::TiffEncoder::new(&mut big).unwrap();
            let mut dir = enc.image_directory().unwrap();
            dir.write_tag(tiff::tags::Tag::ImageWidth, 100_000u32).unwrap();
            dir.write_tag(tiff::tags::Tag::ImageLength, 100_000u32).unwrap();
            dir.write_tag(tiff::tags::Tag::BitsPerSample, &[8u16, 8, 8][..]).unwrap();
            dir.write_tag(tiff::tags::Tag::SamplesPerPixel, 3u16).unwrap();
            dir.write_tag(tiff::tags::Tag::PhotometricInterpretation, 2u16).unwrap();
            dir.write_tag(tiff::tags::Tag::StripOffsets, 8u32).unwrap();
            dir.write_tag(tiff::tags::Tag::StripByteCounts, 4u32).unwrap();
            dir.write_tag(tiff::tags::Tag::RowsPerStrip, 100_000u32).unwrap();
            dir.finish().unwrap();
        }
        assert!(decode(big.get_ref()).is_err());
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
