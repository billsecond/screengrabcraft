//! A how-to guide made into something to hand round: a PDF (a page a step, the step's number and
//! words over its picture), a web page or Markdown (the pictures in a folder beside it). Nothing
//! here reads or writes files; the host does.

use screengrabcraft_model::guide::Guide;
use screengrabcraft_model::{Color, Document, Kind, Pt, Raster, Rect, Shape, Style};

use crate::codec::CodecError;

/// A PDF page's size in pixels: A4 at 150 pixels to the inch.
pub const PAGE: (u32, u32) = (1240, 1754);
/// The page's margin.
const MARGIN: f32 = 90.0;

/// The step's picture, where the click was marked: a red ring round it, drawn into the picture.
pub fn mark_click(picture: &Raster, x: u32, y: u32) -> Raster {
    let r = (picture.width().min(picture.height()) as f32 * 0.04).clamp(14.0, 40.0);
    let mut doc = Document::new(picture.clone());
    let style = Style { stroke: Color::rgb(230, 40, 40), width: (r * 0.22).max(3.0), ..Kind::Ellipse.default_style() };
    let rect = Rect::new(x as f32 - r, y as f32 - r, 2.0 * r, 2.0 * r);
    if doc.add(Shape::Ellipse { rect }, style).is_err() {
        return picture.clone();
    }
    crate::render(&doc).unwrap_or_else(|| picture.clone())
}

/// Copies `src` onto `dst` with its top left at (`x`, `y`) (what falls outside is left out).
fn paste(dst: &mut Raster, src: &Raster, x: u32, y: u32) {
    let (dw, sw) = (dst.width() as usize, src.width() as usize);
    let w = sw.min(dw.saturating_sub(x as usize));
    for row in 0..src.height() as usize {
        let dy = y as usize + row;
        if dy >= dst.height() as usize {
            break;
        }
        let from = src.pixels().get(row * sw * 4..(row * sw + w) * 4);
        let to_start = (dy * dw + x as usize) * 4;
        if let (Some(from), Some(to)) = (from, dst.pixels_mut().get_mut(to_start..to_start + w * 4)) {
            to.copy_from_slice(from);
        }
    }
}

fn words_style(size: f32, bold: bool, color: Color) -> Style {
    Style { font_size: size, bold, text_color: color, fill: Color::TRANSPARENT, stroke: Color::TRANSPARENT, ..Kind::Text.default_style() }
}

/// One page of the PDF: the guide's title (on the first), the step's number and words, and its
/// picture made to fit the rest.
pub fn page(guide: &Guide, index: usize, picture: Option<&Raster>) -> Option<Raster> {
    let step = guide.steps.get(index)?;
    let mut base = Raster::filled(PAGE.0, PAGE.1, Color::WHITE).ok()?;
    let width = PAGE.0 as f32 - 2.0 * MARGIN;
    let mut y = MARGIN;
    let mut texts: Vec<(Rect, String, Style)> = Vec::new();
    if index == 0 && !guide.title.trim().is_empty() {
        let style = words_style(44.0, true, Color::rgb(20, 20, 24));
        let r = crate::text::fit_box(&guide.title, &style, Pt::new(MARGIN, y), Some(width));
        y += r.h + 30.0;
        texts.push((r, guide.title.clone(), style));
    }
    let style = words_style(30.0, true, Color::rgb(40, 90, 200));
    let heading = format!("Step {}", index + 1);
    let r = crate::text::fit_box(&heading, &style, Pt::new(MARGIN, y), Some(width));
    y += r.h + 10.0;
    texts.push((r, heading, style));
    if !step.text.trim().is_empty() {
        let style = words_style(26.0, false, Color::rgb(30, 30, 34));
        let r = crate::text::fit_box(&step.text, &style, Pt::new(MARGIN, y), Some(width));
        y += r.h + 24.0;
        texts.push((r, step.text.clone(), style));
    }
    if let Some(p) = picture {
        let room = (PAGE.1 as f32 - MARGIN - y).max(100.0);
        let f = (width / p.width().max(1) as f32).min(room / p.height().max(1) as f32).min(1.5);
        let (w, h) = (((p.width() as f32 * f) as u32).max(1), ((p.height() as f32 * f) as u32).max(1));
        if let Ok(small) = crate::effects::resize(p, w, h) {
            let x = (PAGE.0 - w.min(PAGE.0)) / 2;
            paste(&mut base, &small, x, y as u32);
        }
    }
    let mut doc = Document::new(base);
    for (rect, text, style) in texts {
        let _ = doc.add(Shape::Text { rect, text }, style);
    }
    crate::render(&doc)
}

/// The guide as a PDF, a page a step (`pictures` are the steps' pictures, in order; one missing
/// leaves its page without).
pub fn pdf(guide: &Guide, pictures: &[Option<Raster>]) -> Result<Vec<u8>, CodecError> {
    let pages: Vec<Raster> = (0..guide.steps.len()).filter_map(|i| page(guide, i, pictures.get(i).and_then(Option::as_ref))).collect();
    if pages.is_empty() {
        return Err(CodecError::Write("the guide has no steps".to_string()));
    }
    crate::pdf::pages(&pages)
}

/// Text made safe for a web page.
fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// A file name made safe in a web address (spaces and the like written as `%20`…).
fn url_path(path: &str) -> String {
    let mut out = String::new();
    for b in path.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~' | b'/') {
            out.push(char::from(b));
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The guide as a web page, its pictures in the folder `pictures` beside it.
pub fn html(guide: &Guide, pictures: &str) -> String {
    let title = if guide.title.trim().is_empty() { "How-to guide".to_string() } else { guide.title.clone() };
    let mut out = String::new();
    out.push_str(
        "<!doctype html>\n<html>\n<head>\n<meta charset=\"utf-8\">\n<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n",
    );
    out.push_str(&format!("<title>{}</title>\n", escape(&title)));
    out.push_str("<style>body{font-family:system-ui,sans-serif;max-width:960px;margin:2em auto;padding:0 1em;color:#1c1c20;line-height:1.5}");
    out.push_str(
        "li{margin:0 0 2.5em}img{max-width:100%;border:1px solid #ccc;border-radius:6px;margin-top:.5em}h2{font-size:1.1em;margin:0}</style>\n",
    );
    out.push_str(&format!("</head>\n<body>\n<h1>{}</h1>\n<ol>\n", escape(&title)));
    for (i, s) in guide.steps.iter().enumerate() {
        out.push_str(&format!("<li>\n<h2>Step {}</h2>\n", i + 1));
        for line in s.text.lines().filter(|l| !l.trim().is_empty()) {
            out.push_str(&format!("<p>{}</p>\n", escape(line)));
        }
        out.push_str(&format!("<img src=\"{}\" alt=\"Step {}\">\n</li>\n", escape(&url_path(&format!("{pictures}/{}", s.picture))), i + 1));
    }
    out.push_str("</ol>\n</body>\n</html>\n");
    out
}

/// The guide as Markdown, its pictures in the folder `pictures` beside it.
pub fn markdown(guide: &Guide, pictures: &str) -> String {
    let title = if guide.title.trim().is_empty() { "How-to guide".to_string() } else { guide.title.clone() };
    let mut out = format!("# {}\n\n", title.replace('\n', " "));
    for (i, s) in guide.steps.iter().enumerate() {
        out.push_str(&format!("## Step {}\n\n", i + 1));
        let text = s.text.trim();
        if !text.is_empty() {
            out.push_str(text);
            out.push_str("\n\n");
        }
        out.push_str(&format!("![Step {}]({})\n\n", i + 1, url_path(&format!("{pictures}/{}", s.picture))));
    }
    out
}

#[cfg(test)]
mod tests {
    use screengrabcraft_model::guide::GuideStep;

    use super::*;

    fn guide() -> Guide {
        Guide {
            title: "Log in <safely>".into(),
            steps: vec![
                GuideStep { picture: "step 01.png".into(), text: "Open the <b>menu</b>\nthen wait".into(), click: Some((5, 5)) },
                GuideStep { picture: "step 02.png".into(), text: String::new(), click: None },
            ],
        }
    }

    #[test]
    fn the_web_page_and_markdown_number_the_steps_and_show_their_pictures() {
        let h = html(&guide(), "Log in files");
        assert!(h.contains("<title>Log in &lt;safely&gt;</title>"));
        assert!(h.contains("<p>Open the &lt;b&gt;menu&lt;/b&gt;</p>\n<p>then wait</p>"), "{h}");
        assert!(h.contains("<img src=\"Log%20in%20files/step%2001.png\" alt=\"Step 1\">"));
        assert!(h.contains("<h2>Step 2</h2>"));
        assert!(!h.contains("<b>"), "nothing of the words is taken as HTML");
        let m = markdown(&guide(), "Log in files");
        assert!(
            m.starts_with("# Log in <safely>\n\n## Step 1\n\nOpen the <b>menu</b>\nthen wait\n\n![Step 1](Log%20in%20files/step%2001.png)"),
            "{m}"
        );
        assert!(m.contains("## Step 2\n\n![Step 2](Log%20in%20files/step%2002.png)"));
    }

    #[test]
    fn the_pdf_has_a_page_a_step_with_its_picture() {
        let pic = Raster::filled(800, 500, Color::rgb(0, 120, 255)).unwrap();
        let p = page(&guide(), 0, Some(&pic)).unwrap();
        assert_eq!((p.width(), p.height()), PAGE);
        // The picture is on the page, under the words.
        let blue = (0..PAGE.1).filter(|y| p.pixel(PAGE.0 / 2, *y).is_some_and(|c| c.b > 200 && c.r < 50)).count();
        assert!(blue > 400, "the picture fills the width: {blue} rows");
        if let Ok(dir) = std::env::var("SGC_SHOTS") {
            let _ = std::fs::write(format!("{dir}/guide-page.png"), crate::encode(&p, screengrabcraft_model::ImageFormat::Png, 90).unwrap());
        }
        let bytes = pdf(&guide(), &[Some(pic), None]).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.starts_with("%PDF-1.4"));
        assert!(text.contains("/Count 2"), "two pages");
        assert!(pdf(&Guide::default(), &[]).is_err(), "no steps, no PDF");
    }

    #[test]
    fn a_click_is_marked_with_a_ring() {
        let pic = Raster::filled(400, 300, Color::WHITE).unwrap();
        let marked = mark_click(&pic, 200, 150);
        let red = |x: u32, y: u32| marked.pixel(x, y).is_some_and(|c| c.r > 200 && c.g < 100);
        let r = (300.0_f32 * 0.04).clamp(14.0, 40.0) as u32;
        assert!(red(200 + r, 150) || red(200 + r - 1, 150), "the ring");
        assert!(!red(200, 150), "the middle stays clear");
        assert_eq!(mark_click(&pic, 10_000, 10_000).width(), 400, "a click off the picture changes nothing that matters");
    }
}
