//! A PDF of one page holding a picture at its own size (72 pixels to the inch is not assumed:
//! the page is sized at 96 pixels to the inch, the way screens are measured), the picture
//! deflated losslessly and any transparency kept as a soft mask.

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use screengrabcraft_model::Raster;

use crate::codec::CodecError;

fn deflate(data: &[u8]) -> Result<Vec<u8>, CodecError> {
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(data).map_err(|e| CodecError::Write(e.to_string()))?;
    z.finish().map_err(|e| CodecError::Write(e.to_string()))
}

/// The picture as a one-page PDF.
pub fn single_page(r: &Raster) -> Result<Vec<u8>, CodecError> {
    let (w, h) = (r.width(), r.height());
    let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
    let mut alpha = Vec::with_capacity(w as usize * h as usize);
    for px in r.pixels().as_chunks::<4>().0 {
        rgb.extend_from_slice(&px[..3]);
        alpha.push(px[3]);
    }
    let has_alpha = alpha.iter().any(|a| *a < 255);
    let image = deflate(&rgb)?;
    let mask = if has_alpha { Some(deflate(&alpha)?) } else { None };
    // Page size in points: 96 pixels to the inch.
    let (pw, ph) = (f64::from(w) * 0.75, f64::from(h) * 0.75);
    let content = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im1 Do Q\n");

    let mut out: Vec<u8> = Vec::new();
    let mut offsets: Vec<usize> = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut object = |out: &mut Vec<u8>, body: &[u8]| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    object(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    object(&mut out, b"<< /Type /Pages /Kids [3 0 R] /Count 1 >>");
    object(
        &mut out,
        format!("<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im1 5 0 R >> >> /Contents 4 0 R >>")
            .as_bytes(),
    );
    let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
    stream.extend_from_slice(content.as_bytes());
    stream.extend_from_slice(b"endstream");
    object(&mut out, &stream);
    let smask = if mask.is_some() { " /SMask 6 0 R" } else { "" };
    let mut img = format!(
        "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode{smask} /Length {} >>\nstream\n",
        image.len()
    )
    .into_bytes();
    img.extend_from_slice(&image);
    img.extend_from_slice(b"\nendstream");
    object(&mut out, &img);
    if let Some(mask) = &mask {
        let mut m = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
            mask.len()
        )
        .into_bytes();
        m.extend_from_slice(mask);
        m.extend_from_slice(b"\nendstream");
        object(&mut out, &m);
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for off in &offsets {
        out.extend_from_slice(format!("{off:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use screengrabcraft_model::Color;

    #[test]
    fn writes_a_well_formed_pdf() {
        let pdf = single_page(&Raster::filled(40, 20, Color::RED).unwrap()).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/MediaBox [0 0 30.00 15.00]"));
        assert!(!text.contains("/SMask"), "opaque pictures need no mask");
        assert!(text.trim_end().ends_with("%%EOF"));
        // Every offset in the cross-reference table points at its object.
        let xref_at: usize = text.rsplit("startxref\n").next().and_then(|s| s.lines().next()).and_then(|s| s.parse().ok()).unwrap();
        assert!(pdf[xref_at..].starts_with(b"xref"));
        let table = String::from_utf8_lossy(&pdf[xref_at..]).to_string();
        for (i, line) in table.lines().skip(3).take(5).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "object {}", i + 1);
        }
        let see_through = single_page(&Raster::filled(4, 4, Color::rgba(0, 0, 0, 10)).unwrap()).unwrap();
        assert!(String::from_utf8_lossy(&see_through).contains("/SMask 6 0 R"));
    }
}
