//! A how-to guide made into something to hand round: a PDF (a page a step, the step's number and
//! words over its picture), a word-processor document (`.docx`), slides (`.pptx`, a slide a step),
//! a web page or Markdown (the pictures in a folder beside it). Nothing here reads or writes
//! files; the host does.

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

// ---- a word-processor document and slides (Office Open XML, zipped by hand) ----

/// English Metric Units to the inch, as office documents measure.
const EMU_INCH: u64 = 914_400;

/// Text made safe in XML: the five marked, and characters XML can't hold left out.
fn xml(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            '\t' | '\n' | '\r' => out.push(c),
            c if c.is_control() || matches!(c, '\u{FFFE}' | '\u{FFFF}') => {}
            c => out.push(c),
        }
    }
    out
}

const XML_HEAD: &str = "<?xml version=\"1.0\" encoding=\"UTF-8\" standalone=\"yes\"?>\n";
const REL: &str = "http://schemas.openxmlformats.org/officeDocument/2006/relationships";
const PKG_REL: &str = "http://schemas.openxmlformats.org/package/2006/relationships";
const DRAWING: &str = "http://schemas.openxmlformats.org/drawingml/2006/main";
const PICTURE: &str = "http://schemas.openxmlformats.org/drawingml/2006/picture";

/// A relationships part: each one's id, type (after `.../relationships/`) and target.
fn rels(list: &[(String, &str, String)]) -> String {
    let mut out = format!("{XML_HEAD}<Relationships xmlns=\"{PKG_REL}\">");
    for (id, kind, target) in list {
        out.push_str(&format!("<Relationship Id=\"{id}\" Type=\"{REL}/{kind}\" Target=\"{}\"/>", xml(target)));
    }
    out.push_str("</Relationships>");
    out
}

/// The steps' pictures as PNGs, with their sizes; a step without one has none.
fn pngs(guide: &Guide, pictures: &[Option<Raster>]) -> Vec<Option<(Vec<u8>, u32, u32)>> {
    (0..guide.steps.len())
        .map(|i| {
            let p = pictures.get(i)?.as_ref()?;
            let png = crate::encode(p, screengrabcraft_model::ImageFormat::Png, 90).ok()?;
            Some((png, p.width(), p.height()))
        })
        .collect()
}

/// A picture's size in EMU, fitted inside `max_w × max_h` keeping its shape.
fn fit_emu(w: u32, h: u32, max_w: u64, max_h: u64) -> (u64, u64) {
    let (w, h) = (u64::from(w.max(1)), u64::from(h.max(1)));
    // At 96 pixels to the inch, no larger than that, made smaller to fit.
    let (mut cx, mut cy) = (w * EMU_INCH / 96, h * EMU_INCH / 96);
    if cx > max_w {
        cy = cy * max_w / cx.max(1);
        cx = max_w;
    }
    if cy > max_h {
        cx = cx * max_h / cy.max(1);
        cy = max_h;
    }
    (cx.max(1), cy.max(1))
}

/// How many steps, in words.
fn steps_said(n: usize) -> String {
    if n == 1 { "1 step".to_string() } else { format!("{n} steps") }
}

fn title_of(guide: &Guide) -> String {
    if guide.title.trim().is_empty() { "How-to guide".to_string() } else { guide.title.trim().to_string() }
}

/// The guide as a word-processor document (`.docx`): its title, then each step's number, its
/// words and its picture, one after another on A4 pages.
pub fn docx(guide: &Guide, pictures: &[Option<Raster>]) -> Result<Vec<u8>, CodecError> {
    if guide.steps.is_empty() {
        return Err(CodecError::Write("the guide has no steps".to_string()));
    }
    let pngs = pngs(guide, pictures);
    let para = |text: &str, size: u32, bold: bool, color: &str| {
        let b = if bold { "<w:b/>" } else { "" };
        format!(
            "<w:p><w:pPr><w:spacing w:after=\"120\"/></w:pPr><w:r><w:rPr>{b}<w:color w:val=\"{color}\"/><w:sz w:val=\"{size}\"/></w:rPr><w:t xml:space=\"preserve\">{}</w:t></w:r></w:p>",
            xml(text)
        )
    };
    let mut body = para(&title_of(guide), 40, true, "141418");
    let mut links = Vec::new();
    // The width between the margins (A4, an inch each side) and most of the height.
    let (max_w, max_h) = (EMU_INCH * 627 / 100, EMU_INCH * 8);
    for (i, step) in guide.steps.iter().enumerate() {
        let n = i + 1;
        body.push_str(&para(&format!("Step {n}"), 30, true, "285AC8"));
        for line in step.text.lines().filter(|l| !l.trim().is_empty()) {
            body.push_str(&para(line, 24, false, "1E1E22"));
        }
        if let Some(Some((_, w, h))) = pngs.get(i) {
            let (cx, cy) = fit_emu(*w, *h, max_w, max_h);
            let id = format!("rIdStep{n}");
            body.push_str(&format!(
                "<w:p><w:pPr><w:spacing w:after=\"360\"/></w:pPr><w:r><w:drawing><wp:inline distT=\"0\" distB=\"0\" distL=\"0\" distR=\"0\"><wp:extent cx=\"{cx}\" cy=\"{cy}\"/><wp:docPr id=\"{n}\" name=\"Step {n}\"/><a:graphic xmlns:a=\"{DRAWING}\"><a:graphicData uri=\"{PICTURE}\"><pic:pic xmlns:pic=\"{PICTURE}\"><pic:nvPicPr><pic:cNvPr id=\"{n}\" name=\"step-{n}.png\"/><pic:cNvPicPr/></pic:nvPicPr><pic:blipFill><a:blip r:embed=\"{id}\"/><a:stretch><a:fillRect/></a:stretch></pic:blipFill><pic:spPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></pic:spPr></pic:pic></a:graphicData></a:graphic></wp:inline></w:drawing></w:r></w:p>"
            ));
            links.push((id, "image", format!("media/step-{n}.png")));
        }
    }
    let document = format!(
        "{XML_HEAD}<w:document xmlns:w=\"http://schemas.openxmlformats.org/wordprocessingml/2006/main\" xmlns:r=\"{REL}\" xmlns:wp=\"http://schemas.openxmlformats.org/drawingml/2006/wordprocessingDrawing\" xmlns:a=\"{DRAWING}\" xmlns:pic=\"{PICTURE}\"><w:body>{body}<w:sectPr><w:pgSz w:w=\"11906\" w:h=\"16838\"/><w:pgMar w:top=\"1440\" w:right=\"1440\" w:bottom=\"1440\" w:left=\"1440\" w:header=\"708\" w:footer=\"708\" w:gutter=\"0\"/></w:sectPr></w:body></w:document>"
    );
    let types = format!(
        "{XML_HEAD}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/word/document.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.wordprocessingml.document.main+xml\"/></Types>"
    );
    let mut z = crate::zip::ZipWriter::new();
    z.add("[Content_Types].xml", types.as_bytes())?;
    z.add("_rels/.rels", rels(&[("rId1".to_string(), "officeDocument", "word/document.xml".to_string())]).as_bytes())?;
    z.add("word/document.xml", document.as_bytes())?;
    z.add("word/_rels/document.xml.rels", rels(&links).as_bytes())?;
    for (i, png) in pngs.iter().enumerate() {
        if let Some((bytes, _, _)) = png {
            z.add(&format!("word/media/step-{}.png", i + 1), bytes)?;
        }
    }
    z.finish()
}

/// A slide's size (16:9), in EMU.
const SLIDE: (u64, u64) = (12_192_000, 6_858_000);

/// The namespaces a slide's part starts with.
fn pml(root: &str, attrs: &str) -> String {
    format!(
        "{XML_HEAD}<p:{root} xmlns:a=\"{DRAWING}\" xmlns:r=\"{REL}\" xmlns:p=\"http://schemas.openxmlformats.org/presentationml/2006/main\"{attrs}>"
    )
}

/// The shapes every slide's tree starts with.
const TREE_START: &str = "<p:nvGrpSpPr><p:cNvPr id=\"1\" name=\"\"/><p:cNvGrpSpPr/><p:nvPr/></p:nvGrpSpPr><p:grpSpPr><a:xfrm><a:off x=\"0\" y=\"0\"/><a:ext cx=\"0\" cy=\"0\"/><a:chOff x=\"0\" y=\"0\"/><a:chExt cx=\"0\" cy=\"0\"/></a:xfrm></p:grpSpPr>";

/// A box of words on a slide: each line its own paragraph, the first in `first` (size in
/// hundredths of a point, bold), the rest in `rest`.
fn text_box(id: u32, (x, y, cx, cy): (u64, u64, u64, u64), lines: &[(String, u32, bool, &str)]) -> String {
    let mut paras = String::new();
    for (text, size, bold, color) in lines {
        let b = if *bold { " b=\"1\"" } else { "" };
        paras.push_str(&format!(
            "<a:p><a:r><a:rPr lang=\"en-US\" sz=\"{size}\"{b} dirty=\"0\"><a:solidFill><a:srgbClr val=\"{color}\"/></a:solidFill></a:rPr><a:t>{}</a:t></a:r></a:p>",
            xml(text)
        ));
    }
    format!(
        "<p:sp><p:nvSpPr><p:cNvPr id=\"{id}\" name=\"Words {id}\"/><p:cNvSpPr txBox=\"1\"/><p:nvPr/></p:nvSpPr><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom><a:noFill/></p:spPr><p:txBody><a:bodyPr wrap=\"square\" rtlCol=\"0\"><a:normAutofit/></a:bodyPr><a:lstStyle/>{paras}</p:txBody></p:sp>"
    )
}

/// A plain theme: the colours, fonts and line and fill styles a presentation must have.
fn theme() -> String {
    let colours: String = [
        ("dk1", "000000"),
        ("lt1", "FFFFFF"),
        ("dk2", "1E1E22"),
        ("lt2", "F2F3F5"),
        ("accent1", "285AC8"),
        ("accent2", "E62828"),
        ("accent3", "2E9E5B"),
        ("accent4", "F5A00B"),
        ("accent5", "7C4DCC"),
        ("accent6", "1AA0B4"),
        ("hlink", "285AC8"),
        ("folHlink", "7C4DCC"),
    ]
    .iter()
    .map(|(k, v)| format!("<a:{k}><a:srgbClr val=\"{v}\"/></a:{k}>"))
    .collect();
    let fill = "<a:solidFill><a:schemeClr val=\"phClr\"/></a:solidFill>";
    let font = "<a:latin typeface=\"Arial\"/><a:ea typeface=\"\"/><a:cs typeface=\"\"/>";
    format!(
        "{XML_HEAD}<a:theme xmlns:a=\"{DRAWING}\" name=\"Plain\"><a:themeElements><a:clrScheme name=\"Plain\">{colours}</a:clrScheme><a:fontScheme name=\"Plain\"><a:majorFont>{font}</a:majorFont><a:minorFont>{font}</a:minorFont></a:fontScheme><a:fmtScheme name=\"Plain\"><a:fillStyleLst>{fill}{fill}{fill}</a:fillStyleLst><a:lnStyleLst><a:ln w=\"6350\">{fill}</a:ln><a:ln w=\"12700\">{fill}</a:ln><a:ln w=\"19050\">{fill}</a:ln></a:lnStyleLst><a:effectStyleLst><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle><a:effectStyle><a:effectLst/></a:effectStyle></a:effectStyleLst><a:bgFillStyleLst>{fill}{fill}{fill}</a:bgFillStyleLst></a:fmtScheme></a:themeElements><a:objectDefaults/><a:extraClrSchemeLst/></a:theme>"
    )
}

/// The guide as slides (`.pptx`): its title on the first, then a slide a step, its number and
/// words at the top and its picture under them.
pub fn pptx(guide: &Guide, pictures: &[Option<Raster>]) -> Result<Vec<u8>, CodecError> {
    if guide.steps.is_empty() {
        return Err(CodecError::Write("the guide has no steps".to_string()));
    }
    let pngs = pngs(guide, pictures);
    let (sw, sh) = SLIDE;
    let margin = EMU_INCH / 2;
    let end = "</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sld>";
    // Each slide: its XML and its picture's part, if any.
    let mut slides: Vec<(String, Option<String>)> = Vec::new();
    let words_box = (margin, margin / 2, sw - 2 * margin, EMU_INCH * 3 / 2);
    let title_box = (margin, sh / 3, sw - 2 * margin, sh / 3);
    let title = text_box(2, title_box, &[(title_of(guide), 4400, true, "141418"), (steps_said(guide.steps.len()), 2000, false, "5A5A64")]);
    slides.push((format!("{}<p:cSld><p:spTree>{TREE_START}{title}{end}", pml("sld", "")), None));
    for (i, step) in guide.steps.iter().enumerate() {
        let n = i + 1;
        let mut lines = vec![(format!("Step {n}"), 2800, true, "285AC8")];
        lines.extend(step.text.lines().filter(|l| !l.trim().is_empty()).take(6).map(|l| (l.to_string(), 1800, false, "1E1E22")));
        let mut tree = text_box(2, words_box, &lines);
        let mut media = None;
        if let Some(Some((_, w, h))) = pngs.get(i) {
            let top = words_box.1 + words_box.3 + EMU_INCH / 8;
            let (cx, cy) = fit_emu(*w, *h, sw - 2 * margin, sh.saturating_sub(top + margin / 2));
            let (x, y) = ((sw - cx) / 2, top);
            tree.push_str(&format!(
                "<p:pic><p:nvPicPr><p:cNvPr id=\"3\" name=\"Step {n}\"/><p:cNvPicPr><a:picLocks noChangeAspect=\"1\"/></p:cNvPicPr><p:nvPr/></p:nvPicPr><p:blipFill><a:blip r:embed=\"rId2\"/><a:stretch><a:fillRect/></a:stretch></p:blipFill><p:spPr><a:xfrm><a:off x=\"{x}\" y=\"{y}\"/><a:ext cx=\"{cx}\" cy=\"{cy}\"/></a:xfrm><a:prstGeom prst=\"rect\"><a:avLst/></a:prstGeom></p:spPr></p:pic>"
            ));
            media = Some(format!("step-{n}.png"));
        }
        slides.push((format!("{}<p:cSld><p:spTree>{TREE_START}{tree}{end}", pml("sld", "")), media));
    }
    let mut types = format!(
        "{XML_HEAD}<Types xmlns=\"http://schemas.openxmlformats.org/package/2006/content-types\"><Default Extension=\"rels\" ContentType=\"application/vnd.openxmlformats-package.relationships+xml\"/><Default Extension=\"xml\" ContentType=\"application/xml\"/><Default Extension=\"png\" ContentType=\"image/png\"/><Override PartName=\"/ppt/presentation.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.presentation.main+xml\"/><Override PartName=\"/ppt/slideMasters/slideMaster1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideMaster+xml\"/><Override PartName=\"/ppt/slideLayouts/slideLayout1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slideLayout+xml\"/><Override PartName=\"/ppt/theme/theme1.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.theme+xml\"/>"
    );
    let mut ids = String::new();
    let mut links = vec![
        ("rId1".to_string(), "slideMaster", "slideMasters/slideMaster1.xml".to_string()),
        ("rId2".to_string(), "theme", "theme/theme1.xml".to_string()),
    ];
    for k in 1..=slides.len() {
        types.push_str(&format!(
            "<Override PartName=\"/ppt/slides/slide{k}.xml\" ContentType=\"application/vnd.openxmlformats-officedocument.presentationml.slide+xml\"/>"
        ));
        ids.push_str(&format!("<p:sldId id=\"{}\" r:id=\"rIdSlide{k}\"/>", 255 + k));
        links.push((format!("rIdSlide{k}"), "slide", format!("slides/slide{k}.xml")));
    }
    types.push_str("</Types>");
    let presentation = format!(
        "{}<p:sldMasterIdLst><p:sldMasterId id=\"2147483648\" r:id=\"rId1\"/></p:sldMasterIdLst><p:sldIdLst>{ids}</p:sldIdLst><p:sldSz cx=\"{sw}\" cy=\"{sh}\"/><p:notesSz cx=\"6858000\" cy=\"9144000\"/></p:presentation>",
        pml("presentation", "")
    );
    let master = format!(
        "{}<p:cSld><p:bg><p:bgPr><a:solidFill><a:srgbClr val=\"FFFFFF\"/></a:solidFill><a:effectLst/></p:bgPr></p:bg><p:spTree>{TREE_START}</p:spTree></p:cSld><p:clrMap bg1=\"lt1\" tx1=\"dk1\" bg2=\"lt2\" tx2=\"dk2\" accent1=\"accent1\" accent2=\"accent2\" accent3=\"accent3\" accent4=\"accent4\" accent5=\"accent5\" accent6=\"accent6\" hlink=\"hlink\" folHlink=\"folHlink\"/><p:sldLayoutIdLst><p:sldLayoutId id=\"2147483649\" r:id=\"rId1\"/></p:sldLayoutIdLst><p:txStyles><p:titleStyle><a:lvl1pPr><a:defRPr sz=\"4400\"/></a:lvl1pPr></p:titleStyle><p:bodyStyle><a:lvl1pPr><a:defRPr sz=\"2800\"/></a:lvl1pPr></p:bodyStyle><p:otherStyle><a:lvl1pPr><a:defRPr sz=\"1800\"/></a:lvl1pPr></p:otherStyle></p:txStyles></p:sldMaster>",
        pml("sldMaster", "")
    );
    let layout = format!(
        "{}<p:cSld name=\"Blank\"><p:spTree>{TREE_START}</p:spTree></p:cSld><p:clrMapOvr><a:masterClrMapping/></p:clrMapOvr></p:sldLayout>",
        pml("sldLayout", " type=\"blank\" preserve=\"1\"")
    );
    let mut z = crate::zip::ZipWriter::new();
    z.add("[Content_Types].xml", types.as_bytes())?;
    z.add("_rels/.rels", rels(&[("rId1".to_string(), "officeDocument", "ppt/presentation.xml".to_string())]).as_bytes())?;
    z.add("ppt/presentation.xml", presentation.as_bytes())?;
    z.add("ppt/_rels/presentation.xml.rels", rels(&links).as_bytes())?;
    z.add("ppt/slideMasters/slideMaster1.xml", master.as_bytes())?;
    z.add(
        "ppt/slideMasters/_rels/slideMaster1.xml.rels",
        rels(&[
            ("rId1".to_string(), "slideLayout", "../slideLayouts/slideLayout1.xml".to_string()),
            ("rId2".to_string(), "theme", "../theme/theme1.xml".to_string()),
        ])
        .as_bytes(),
    )?;
    z.add("ppt/slideLayouts/slideLayout1.xml", layout.as_bytes())?;
    z.add(
        "ppt/slideLayouts/_rels/slideLayout1.xml.rels",
        rels(&[("rId1".to_string(), "slideMaster", "../slideMasters/slideMaster1.xml".to_string())]).as_bytes(),
    )?;
    z.add("ppt/theme/theme1.xml", theme().as_bytes())?;
    for (k, (slide, media)) in slides.iter().enumerate() {
        let k = k + 1;
        z.add(&format!("ppt/slides/slide{k}.xml"), slide.as_bytes())?;
        let mut list = vec![("rId1".to_string(), "slideLayout", "../slideLayouts/slideLayout1.xml".to_string())];
        if let Some(m) = media {
            list.push(("rId2".to_string(), "image", format!("../media/{m}")));
        }
        z.add(&format!("ppt/slides/_rels/slide{k}.xml.rels"), rels(&list).as_bytes())?;
    }
    for (i, png) in pngs.iter().enumerate() {
        if let Some((bytes, _, _)) = png {
            z.add(&format!("ppt/media/step-{}.png", i + 1), bytes)?;
        }
    }
    z.finish()
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

    /// Whether XML is well formed as far as its tags go: each one closed, in order.
    fn balanced(text: &str) -> Result<(), String> {
        let mut open: Vec<String> = Vec::new();
        let mut rest = text;
        while let Some(at) = rest.find('<') {
            let after = rest.get(at + 1..).unwrap_or_default();
            let end = after.find('>').ok_or("a tag never ends")?;
            let tag = after.get(..end).unwrap_or_default();
            rest = after.get(end + 1..).unwrap_or_default();
            if tag.starts_with('?') {
                continue;
            }
            let name = |t: &str| t.split_whitespace().next().unwrap_or_default().trim_end_matches('/').to_string();
            if let Some(closing) = tag.strip_prefix('/') {
                let top = open.pop().ok_or_else(|| format!("</{closing}> closes nothing"))?;
                if top != name(closing) {
                    return Err(format!("<{top}> closed by </{closing}>"));
                }
            } else if !tag.ends_with('/') {
                open.push(name(tag));
            }
        }
        if open.is_empty() { Ok(()) } else { Err(format!("never closed: {open:?}")) }
    }

    #[test]
    fn the_document_and_the_slides_are_zips_of_the_steps_words_and_pictures() {
        let pic = Raster::filled(800, 500, Color::rgb(0, 120, 255)).unwrap();
        let g = guide();
        for (name, bytes) in [("docx", docx(&g, &[Some(pic.clone()), None]).unwrap()), ("pptx", pptx(&g, &[Some(pic.clone()), None]).unwrap())] {
            let parts = crate::zip::entries(&bytes).unwrap_or_else(|| panic!("{name} isn't a zip"));
            let names: Vec<&str> = parts.iter().map(|(n, _)| n.as_str()).collect();
            assert_eq!(names.first(), Some(&"[Content_Types].xml"), "{name}");
            assert!(names.contains(&"_rels/.rels"), "{name}");
            // Every XML part well formed; the words marked, never taken as XML.
            for (n, data) in &parts {
                if n.ends_with(".xml") || n.ends_with(".rels") {
                    let text = std::str::from_utf8(data).unwrap();
                    balanced(text).unwrap_or_else(|e| panic!("{name} {n}: {e}"));
                    assert!(!text.contains("<b>menu"), "{name} {n}");
                }
            }
            // One picture: the first step's, a PNG; the second step has none.
            let pngs: Vec<&(String, Vec<u8>)> = parts.iter().filter(|(n, _)| n.ends_with(".png")).collect();
            assert_eq!(pngs.len(), 1, "{name}");
            assert!(pngs[0].0.ends_with("step-1.png") && crate::decode(&pngs[0].1).is_ok_and(|p| p.width() == 800), "{name}");
            let all: String = parts.iter().filter_map(|(_, d)| std::str::from_utf8(d).ok()).collect();
            for words in ["Log in &lt;safely&gt;", "Step 1", "Step 2", "Open the &lt;b&gt;menu&lt;/b&gt;", "then wait"] {
                assert!(all.contains(words), "{name}: {words}");
            }
        }
        // The document: its parts and its picture linked from the words.
        let parts = crate::zip::entries(&docx(&g, &[Some(pic.clone())]).unwrap()).unwrap();
        let part = |n: &str| parts.iter().find(|(p, _)| p == n).map(|(_, d)| String::from_utf8_lossy(d).to_string()).unwrap_or_default();
        assert!(part("word/document.xml").contains("r:embed=\"rIdStep1\""));
        assert!(
            part("word/_rels/document.xml.rels").contains("Id=\"rIdStep1\"") && part("word/_rels/document.xml.rels").contains("media/step-1.png")
        );
        assert!(part("[Content_Types].xml").contains("/word/document.xml"));
        // The slides: a title slide, then a slide a step.
        let parts = crate::zip::entries(&pptx(&g, &[Some(pic)]).unwrap()).unwrap();
        let slides = parts.iter().filter(|(n, _)| n.starts_with("ppt/slides/slide") && n.ends_with(".xml")).count();
        assert_eq!(slides, 3);
        let part = |n: &str| parts.iter().find(|(p, _)| p == n).map(|(_, d)| String::from_utf8_lossy(d).to_string()).unwrap_or_default();
        assert!(part("ppt/slides/_rels/slide2.xml.rels").contains("../media/step-1.png"));
        assert!(!part("ppt/slides/_rels/slide3.xml.rels").contains("image"), "the second step has no picture");
        assert_eq!(part("ppt/presentation.xml").matches("<p:sldId ").count(), 3);
        for n in ["ppt/slideMasters/slideMaster1.xml", "ppt/slideLayouts/slideLayout1.xml", "ppt/theme/theme1.xml"] {
            assert!(part("[Content_Types].xml").contains(&format!("/{n}")), "{n}");
        }
        // No steps: nothing to make. Words XML can't hold are left out.
        assert!(docx(&Guide::default(), &[]).is_err() && pptx(&Guide::default(), &[]).is_err());
        assert_eq!(xml("a\u{1}b\u{FFFF}c&"), "abc&amp;");
        // Saved for the eye (SGC_SHOTS=<folder>), to open in an office program.
        if let Ok(dir) = std::env::var("SGC_SHOTS") {
            let pic = Raster::filled(1200, 700, Color::rgb(230, 236, 245)).unwrap();
            let _ = std::fs::write(format!("{dir}/guide.docx"), docx(&g, &[Some(pic.clone()), Some(pic.clone())]).unwrap());
            let _ = std::fs::write(format!("{dir}/guide.pptx"), pptx(&g, &[Some(pic.clone()), Some(pic)]).unwrap());
        }
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
