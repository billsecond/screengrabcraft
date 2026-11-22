//! A PDF of pictures, a page each, every page the size of its picture (72 pixels to the inch is
//! not assumed: the page is sized at 96 pixels to the inch, the way screens are measured), the
//! pictures deflated losslessly and any transparency kept as a soft mask.

use std::io::Write;

use flate2::Compression;
use flate2::write::ZlibEncoder;
use screengrabcraft_model::Raster;

use crate::codec::CodecError;

/// The most pages one PDF has.
pub const MAX_PAGES: usize = 1000;

fn deflate(data: &[u8]) -> Result<Vec<u8>, CodecError> {
    let mut z = ZlibEncoder::new(Vec::new(), Compression::default());
    z.write_all(data).map_err(|e| CodecError::Write(e.to_string()))?;
    z.finish().map_err(|e| CodecError::Write(e.to_string()))
}

/// The picture as a one-page PDF.
pub fn single_page(r: &Raster) -> Result<Vec<u8>, CodecError> {
    pages(std::slice::from_ref(r))
}

/// A picture's page, ready to write: its size in points, and its pixels and mask deflated.
#[derive(Clone)]
struct Page {
    w: u32,
    h: u32,
    image: Vec<u8>,
    mask: Option<Vec<u8>>,
}

fn prepare(r: &Raster) -> Result<Page, CodecError> {
    let (w, h) = (r.width(), r.height());
    let mut rgb = Vec::with_capacity(w as usize * h as usize * 3);
    let mut alpha = Vec::with_capacity(w as usize * h as usize);
    for px in r.pixels().as_chunks::<4>().0 {
        rgb.extend_from_slice(&px[..3]);
        alpha.push(px[3]);
    }
    let has_alpha = alpha.iter().any(|a| *a < 255);
    Ok(Page { w, h, image: deflate(&rgb)?, mask: if has_alpha { Some(deflate(&alpha)?) } else { None } })
}

/// A PDF made a page at a time: each picture is deflated as it's added, so a long run of
/// pictures (Batch Changes' one PDF) isn't held whole at once.
#[derive(Clone, Default)]
pub struct Builder {
    pages: Vec<Page>,
}

impl std::fmt::Debug for Builder {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pdf::Builder({} pages)", self.pages.len())
    }
}

impl Builder {
    pub fn new() -> Builder {
        Builder::default()
    }

    /// Adds a page of `r` after the others; an error once there are [`MAX_PAGES`].
    pub fn add(&mut self, r: &Raster) -> Result<(), CodecError> {
        if self.pages.len() >= MAX_PAGES {
            return Err(too_many());
        }
        self.pages.push(prepare(r)?);
        Ok(())
    }

    pub fn len(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pages.is_empty()
    }

    /// The PDF of the pages added, in order (an error when there are none).
    pub fn finish(&self) -> Result<Vec<u8>, CodecError> {
        write(&self.pages)
    }
}

fn too_many() -> CodecError {
    CodecError::Write(format!("a PDF has 1 to {MAX_PAGES} pages"))
}

/// The pictures as a PDF, a page each, in order.
pub fn pages(pictures: &[Raster]) -> Result<Vec<u8>, CodecError> {
    if pictures.is_empty() || pictures.len() > MAX_PAGES {
        return Err(too_many());
    }
    let prepared = pictures.iter().map(prepare).collect::<Result<Vec<Page>, CodecError>>()?;
    write(&prepared)
}

/// The pages written out as a PDF.
fn write(prepared: &[Page]) -> Result<Vec<u8>, CodecError> {
    if prepared.is_empty() || prepared.len() > MAX_PAGES {
        return Err(too_many());
    }
    // Each page's objects: the page, its contents, its picture and the picture's mask, numbered
    // after the catalog (1) and the page tree (2).
    let mut next = 3usize;
    let mut ids = Vec::with_capacity(prepared.len());
    for p in prepared {
        let page = next;
        next += if p.mask.is_some() { 4 } else { 3 };
        ids.push(page);
    }
    let kids: Vec<String> = ids.iter().map(|id| format!("{id} 0 R")).collect();

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
    object(&mut out, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), prepared.len()).as_bytes());
    for (p, page) in prepared.iter().zip(ids) {
        let (w, h) = (p.w, p.h);
        // Page size in points: 96 pixels to the inch.
        let (pw, ph) = (f64::from(w) * 0.75, f64::from(h) * 0.75);
        let (content_id, image_id, mask_id) = (page + 1, page + 2, page + 3);
        object(
            &mut out,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {pw:.2} {ph:.2}] /Resources << /XObject << /Im1 {image_id} 0 R >> >> /Contents {content_id} 0 R >>"
            )
            .as_bytes(),
        );
        let content = format!("q {pw:.2} 0 0 {ph:.2} 0 0 cm /Im1 Do Q\n");
        let mut stream = format!("<< /Length {} >>\nstream\n", content.len()).into_bytes();
        stream.extend_from_slice(content.as_bytes());
        stream.extend_from_slice(b"endstream");
        object(&mut out, &stream);
        let smask = if p.mask.is_some() { format!(" /SMask {mask_id} 0 R") } else { String::new() };
        let mut img = format!(
            "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /FlateDecode{smask} /Length {} >>\nstream\n",
            p.image.len()
        )
        .into_bytes();
        img.extend_from_slice(&p.image);
        img.extend_from_slice(b"\nendstream");
        object(&mut out, &img);
        if let Some(mask) = &p.mask {
            let mut m = format!(
                "<< /Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceGray /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                mask.len()
            )
            .into_bytes();
            m.extend_from_slice(mask);
            m.extend_from_slice(b"\nendstream");
            object(&mut out, &m);
        }
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

    #[test]
    fn several_pictures_are_a_page_each_in_order() {
        let pics = [
            Raster::filled(40, 20, Color::RED).unwrap(),
            Raster::filled(4, 4, Color::rgba(0, 0, 0, 10)).unwrap(),
            Raster::filled(80, 60, Color::WHITE).unwrap(),
        ];
        let pdf = pages(&pics).unwrap();
        let text = String::from_utf8_lossy(&pdf);
        assert!(text.contains("/Kids [3 0 R 6 0 R 10 0 R] /Count 3"), "the second has a mask: four objects");
        assert!(text.contains("/MediaBox [0 0 60.00 45.00]"));
        assert!(text.contains("/SMask 9 0 R"));
        let xref_at: usize = text
            .rsplit(
                "startxref
",
            )
            .next()
            .and_then(|s| s.lines().next())
            .and_then(|s| s.parse().ok())
            .unwrap();
        let table = String::from_utf8_lossy(&pdf[xref_at..]).to_string();
        let objects = 2 + 3 + 4 + 3;
        for (i, line) in table.lines().skip(3).take(objects).enumerate() {
            let off: usize = line[..10].parse().unwrap();
            assert!(pdf[off..].starts_with(format!("{} 0 obj", i + 1).as_bytes()), "object {}", i + 1);
        }
        assert!(pages(&[]).is_err());
    }

    #[test]
    fn a_pdf_made_a_page_at_a_time_is_the_same_as_one_made_at_once() {
        let pics = [Raster::filled(40, 20, Color::RED).unwrap(), Raster::filled(4, 4, Color::rgba(0, 0, 0, 10)).unwrap()];
        let mut b = Builder::new();
        assert!(b.is_empty() && b.finish().is_err(), "no pages, no PDF");
        for p in &pics {
            b.add(p).unwrap();
        }
        assert_eq!(b.len(), 2);
        assert_eq!(b.finish().unwrap(), pages(&pics).unwrap());
        // No more than a PDF holds.
        let dot = Raster::filled(1, 1, Color::WHITE).unwrap();
        let mut full = Builder::new();
        for _ in 0..MAX_PAGES {
            full.add(&dot).unwrap();
        }
        assert!(full.add(&dot).is_err());
        assert_eq!(full.len(), MAX_PAGES);
        assert!(full.finish().is_ok());
    }
}
